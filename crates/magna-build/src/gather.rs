//! Gather phase — converts introspection results + config into a [`GatherOutput`] IR.
//!
//! This is the first major build phase. It reads raw Postgres catalog data
//! (namespaces, tables, columns, constraints, enums, descriptions) and resolves
//! it into typed IR structs that downstream phases (schema generation) consume.

use std::collections::HashMap;

use magna_config::{GatherContext, Preset};
use magna_dataplan::PgResourceRegistry;
use magna_introspect::{
    IntrospectionResult, PgClassKind, PgConstraintKind, PgDescription,
};

use crate::{
    error::BuildError,
    ir::{
        BehaviorSet, GatherOutput, ResolvedColumn, ResolvedEnum, ResolvedRelation,
        ResolvedResource, ResourceKind,
    },
    naming::to_type_name,
    naming::to_field_name,
    smart_tags::parse_smart_tags,
    type_map::pg_oid_to_gql_type,
};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the gather phase and produce a [`GatherOutput`] IR.
pub fn gather(
    introspection: &IntrospectionResult,
    _registry: &PgResourceRegistry,
    preset: &Preset,
) -> Result<GatherOutput, BuildError> {
    // 1. Resolve which namespaces match the configured schemas.
    let schema_oids = resolve_schema_oids(introspection, &preset.pg_schemas);

    // 2. Build (obj_oid, obj_sub_id) → description text lookup.
    let desc_map = build_description_map(&introspection.descriptions);

    // 3. Build ResolvedResources from tables and views in the matched schemas.
    let mut resources: Vec<ResolvedResource> = Vec::new();
    for class in &introspection.classes {
        // Only Table and View are exposed. Skip MatView, CompositeType, ForeignTable.
        match class.kind {
            PgClassKind::Table | PgClassKind::View => {}
            _ => continue,
        }

        // Only expose classes in the configured schemas.
        if !schema_oids.contains(&class.schema_oid) {
            continue;
        }

        let resource = build_resource(class, introspection, &desc_map)?;
        resources.push(resource);
    }

    // 4. Build ResolvedRelations from FK constraints in the matched schemas.
    let relations = build_relations(introspection, &schema_oids, &resources);

    // 5. Build ResolvedEnums from PG enum types in the matched schemas.
    let enums = build_enums(introspection, &schema_oids);

    // 6. Build the behaviors map (resource name → BehaviorSet) applying smart tags.
    let mut behaviors: HashMap<String, BehaviorSet> = HashMap::new();
    for resource in &resources {
        let mut bs = match resource.kind {
            ResourceKind::Table => BehaviorSet::table_defaults(),
            ResourceKind::View | ResourceKind::Function => BehaviorSet::view_defaults(),
        };

        // Apply smart tag omit overrides from the class-level description (sub_id = 0).
        if let Some(comment) = desc_map.get(&(resource.class_oid, 0)) {
            let tags = parse_smart_tags(comment);
            for omit in &tags.omit {
                if let Some(flag) = BehaviorSet::flag_from_name(omit) {
                    bs.remove(flag);
                }
            }
            for add in &tags.behavior_add {
                if let Some(flag) = BehaviorSet::flag_from_name(add) {
                    bs.add(flag);
                }
            }
            for remove in &tags.behavior_remove {
                if let Some(flag) = BehaviorSet::flag_from_name(remove) {
                    bs.remove(flag);
                }
            }
        }

        behaviors.insert(resource.name.clone(), bs);
    }

    // 7. Run plugin gather hooks.
    let mut gather_ctx = GatherContext::default();
    for plugin in &preset.plugins {
        plugin.gather_hook(&mut gather_ctx);
    }

    Ok(GatherOutput {
        resources,
        relations,
        behaviors,
        enums,
        smart_tags: HashMap::new(),
        plugin_metadata: gather_ctx.metadata,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve the OIDs of namespaces whose names appear in `pg_schemas`.
fn resolve_schema_oids(
    introspection: &IntrospectionResult,
    pg_schemas: &[String],
) -> Vec<u32> {
    introspection
        .namespaces
        .iter()
        .filter(|ns| pg_schemas.iter().any(|s| s == &ns.name))
        .map(|ns| ns.oid)
        .collect()
}

/// Build a `(obj_oid, obj_sub_id) → &str` description lookup from all
/// [`PgDescription`] rows. `obj_sub_id == 0` is the table-level comment;
/// positive values refer to the column at that attnum.
fn build_description_map<'a>(
    descriptions: &'a [PgDescription],
) -> HashMap<(u32, i32), &'a str> {
    descriptions
        .iter()
        .map(|d| ((d.obj_oid, d.obj_sub_id), d.description.as_str()))
        .collect()
}

/// Build a single [`ResolvedResource`] from a `PgClass` + supporting data.
fn build_resource(
    class: &magna_introspect::PgClass,
    introspection: &IntrospectionResult,
    desc_map: &HashMap<(u32, i32), &str>,
) -> Result<ResolvedResource, BuildError> {
    // Resolve schema name.
    let schema = introspection
        .namespaces
        .iter()
        .find(|ns| ns.oid == class.schema_oid)
        .map(|ns| ns.name.clone())
        .unwrap_or_else(|| "public".to_string());

    // Collect attributes for this class, skipping system columns (num <= 0).
    let mut attrs: Vec<&magna_introspect::PgAttribute> = introspection
        .attributes
        .iter()
        .filter(|a| a.class_oid == class.oid && a.num > 0)
        .collect();

    // Sort by pg_name for deterministic output.
    attrs.sort_by(|a, b| a.name.cmp(&b.name));

    // Map each attribute to a ResolvedColumn.
    let mut columns: Vec<ResolvedColumn> = Vec::with_capacity(attrs.len());
    for attr in &attrs {
        let gql_type_base = pg_oid_to_gql_type(attr.type_oid).unwrap_or("String");
        let gql_type = if attr.is_not_null {
            format!("{}!", gql_type_base)
        } else {
            gql_type_base.to_string()
        };

        columns.push(ResolvedColumn {
            pg_name: attr.name.clone(),
            gql_name: to_field_name(&attr.name),
            type_oid: attr.type_oid,
            gql_type,
            is_not_null: attr.is_not_null,
            has_default: attr.has_default,
        });
    }

    // Gather constraints for this class.
    let class_constraints: Vec<&magna_introspect::PgConstraint> = introspection
        .constraints
        .iter()
        .filter(|c| c.class_oid == class.oid)
        .collect();

    // Extract primary key column names.
    let primary_key = {
        let pk = class_constraints
            .iter()
            .find(|c| c.kind == PgConstraintKind::PrimaryKey);
        match pk {
            None => Vec::new(),
            Some(con) => con
                .key_attrs
                .iter()
                .filter_map(|&num| {
                    introspection
                        .attributes
                        .iter()
                        .find(|a| a.class_oid == class.oid && a.num == num)
                        .map(|a| a.name.clone())
                })
                .collect(),
        }
    };

    // Extract unique constraint column sets (excluding PK).
    let unique_constraints: Vec<Vec<String>> = class_constraints
        .iter()
        .filter(|c| c.kind == PgConstraintKind::Unique)
        .map(|con| {
            con.key_attrs
                .iter()
                .filter_map(|&num| {
                    introspection
                        .attributes
                        .iter()
                        .find(|a| a.class_oid == class.oid && a.num == num)
                        .map(|a| a.name.clone())
                })
                .collect()
        })
        .collect();

    // Resolve the type name, optionally overridden by a @name smart tag.
    let mut name = to_type_name(&class.name);
    if let Some(comment) = desc_map.get(&(class.oid, 0)) {
        let tags = parse_smart_tags(comment);
        if let Some(override_name) = tags.name_override {
            name = override_name;
        }
    }

    let kind = match class.kind {
        PgClassKind::Table => ResourceKind::Table,
        _ => ResourceKind::View,
    };

    Ok(ResolvedResource {
        name,
        schema,
        table: class.name.clone(),
        kind,
        columns,
        primary_key,
        unique_constraints,
        class_oid: class.oid,
    })
}

/// Build [`ResolvedRelation`] entries from FK constraints in the matched schemas.
fn build_relations(
    introspection: &IntrospectionResult,
    schema_oids: &[u32],
    resources: &[ResolvedResource],
) -> Vec<ResolvedRelation> {
    // Index resources by class_oid for fast lookup.
    let resource_by_oid: HashMap<u32, &ResolvedResource> =
        resources.iter().map(|r| (r.class_oid, r)).collect();

    // Index classes by OID to check schema membership.
    let class_schema_oid: HashMap<u32, u32> = introspection
        .classes
        .iter()
        .map(|c| (c.oid, c.schema_oid))
        .collect();

    let mut relations = Vec::new();

    for con in &introspection.constraints {
        if con.kind != PgConstraintKind::ForeignKey {
            continue;
        }
        let Some(foreign_class_oid) = con.foreign_class_oid else {
            continue;
        };
        let Some(ref foreign_key_attrs) = con.foreign_key_attrs else {
            continue;
        };

        // Both source and target must be in the configured schemas.
        let src_schema = class_schema_oid.get(&con.class_oid).copied().unwrap_or(0);
        let tgt_schema = class_schema_oid.get(&foreign_class_oid).copied().unwrap_or(0);
        if !schema_oids.contains(&src_schema) || !schema_oids.contains(&tgt_schema) {
            continue;
        }

        // Source and target must be resolved as resources.
        let Some(src_resource) = resource_by_oid.get(&con.class_oid) else {
            continue;
        };
        let Some(tgt_resource) = resource_by_oid.get(&foreign_class_oid) else {
            continue;
        };

        // Resolve source column names.
        let source_columns: Vec<String> = con
            .key_attrs
            .iter()
            .filter_map(|&num| {
                introspection
                    .attributes
                    .iter()
                    .find(|a| a.class_oid == con.class_oid && a.num == num)
                    .map(|a| a.name.clone())
            })
            .collect();

        // Resolve target column names.
        let target_columns: Vec<String> = foreign_key_attrs
            .iter()
            .filter_map(|&num| {
                introspection
                    .attributes
                    .iter()
                    .find(|a| a.class_oid == foreign_class_oid && a.num == num)
                    .map(|a| a.name.clone())
            })
            .collect();

        // A FK is unique if its source columns also form a PK or unique constraint
        // on the source table.
        let is_unique = {
            let src_col_set: std::collections::HashSet<&str> =
                source_columns.iter().map(|s| s.as_str()).collect();

            // Check against PK columns.
            let pk_match = {
                let pk: std::collections::HashSet<&str> =
                    src_resource.primary_key.iter().map(|s| s.as_str()).collect();
                !pk.is_empty() && pk == src_col_set
            };

            // Check against unique constraints.
            let unique_match = src_resource.unique_constraints.iter().any(|uc| {
                let uc_set: std::collections::HashSet<&str> =
                    uc.iter().map(|s| s.as_str()).collect();
                uc_set == src_col_set
            });

            pk_match || unique_match
        };

        relations.push(ResolvedRelation {
            name: con.name.clone(),
            source_resource: src_resource.name.clone(),
            source_columns,
            target_resource: tgt_resource.name.clone(),
            target_columns,
            is_unique,
        });
    }

    relations
}

/// Build [`ResolvedEnum`] entries for all PG enum types in the matched schemas.
fn build_enums(
    introspection: &IntrospectionResult,
    schema_oids: &[u32],
) -> Vec<ResolvedEnum> {
    let mut enums = Vec::new();

    for pg_type in &introspection.types {
        if !pg_type.is_enum {
            continue;
        }
        if !schema_oids.contains(&pg_type.schema_oid) {
            continue;
        }

        // Collect enum labels for this type, sorted by sort_order.
        let mut labels: Vec<&magna_introspect::PgEnum> = introspection
            .enums
            .iter()
            .filter(|e| e.type_oid == pg_type.oid)
            .collect();
        labels.sort_by(|a, b| a.sort_order.partial_cmp(&b.sort_order).unwrap_or(std::cmp::Ordering::Equal));

        let values: Vec<String> = labels.iter().map(|e| e.label.clone()).collect();

        enums.push(ResolvedEnum {
            name: to_type_name(&pg_type.name),
            values,
            pg_type_oid: pg_type.oid,
        });
    }

    enums
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use magna_introspect::{
        ForeignKeyAction, IntrospectionResult, PgAttribute, PgClass, PgClassKind,
        PgConstraint, PgConstraintKind, PgDescription, PgEnum, PgNamespace,
        PgType,
    };

    /// Minimal test introspection: "public" schema with users + posts tables
    /// and a posts.author_id → users.id FK.
    fn make_introspection() -> IntrospectionResult {
        IntrospectionResult {
            namespaces: vec![PgNamespace {
                oid: 1,
                name: "public".to_string(),
            }],
            classes: vec![
                PgClass {
                    oid: 100,
                    name: "users".to_string(),
                    schema_oid: 1,
                    kind: PgClassKind::Table,
                    is_rls_enabled: false,
                },
                PgClass {
                    oid: 200,
                    name: "posts".to_string(),
                    schema_oid: 1,
                    kind: PgClassKind::Table,
                    is_rls_enabled: false,
                },
            ],
            attributes: vec![
                // users columns
                PgAttribute {
                    class_oid: 100,
                    name: "id".to_string(),
                    type_oid: 2950, // uuid
                    num: 1,
                    is_not_null: true,
                    has_default: false,
                    is_identity: false,
                },
                PgAttribute {
                    class_oid: 100,
                    name: "email".to_string(),
                    type_oid: 25, // text
                    num: 2,
                    is_not_null: true,
                    has_default: false,
                    is_identity: false,
                },
                PgAttribute {
                    class_oid: 100,
                    name: "created_at".to_string(),
                    type_oid: 1184, // timestamptz
                    num: 3,
                    is_not_null: true,
                    has_default: true,
                    is_identity: false,
                },
                // posts columns
                PgAttribute {
                    class_oid: 200,
                    name: "id".to_string(),
                    type_oid: 2950,
                    num: 1,
                    is_not_null: true,
                    has_default: false,
                    is_identity: false,
                },
                PgAttribute {
                    class_oid: 200,
                    name: "title".to_string(),
                    type_oid: 25,
                    num: 2,
                    is_not_null: true,
                    has_default: false,
                    is_identity: false,
                },
                PgAttribute {
                    class_oid: 200,
                    name: "author_id".to_string(),
                    type_oid: 2950,
                    num: 3,
                    is_not_null: true,
                    has_default: false,
                    is_identity: false,
                },
            ],
            constraints: vec![
                // users PK
                PgConstraint {
                    oid: 1000,
                    name: "users_pkey".to_string(),
                    class_oid: 100,
                    kind: PgConstraintKind::PrimaryKey,
                    key_attrs: vec![1],
                    foreign_class_oid: None,
                    foreign_key_attrs: None,
                    on_delete: None,
                    on_update: None,
                },
                // posts PK
                PgConstraint {
                    oid: 2000,
                    name: "posts_pkey".to_string(),
                    class_oid: 200,
                    kind: PgConstraintKind::PrimaryKey,
                    key_attrs: vec![1],
                    foreign_class_oid: None,
                    foreign_key_attrs: None,
                    on_delete: None,
                    on_update: None,
                },
                // posts.author_id → users.id
                PgConstraint {
                    oid: 3000,
                    name: "posts_author_id_fkey".to_string(),
                    class_oid: 200,
                    kind: PgConstraintKind::ForeignKey,
                    key_attrs: vec![3],
                    foreign_class_oid: Some(100),
                    foreign_key_attrs: Some(vec![1]),
                    on_delete: Some(ForeignKeyAction::Cascade),
                    on_update: Some(ForeignKeyAction::NoAction),
                },
            ],
            procs: vec![],
            types: vec![],
            enums: vec![],
            indexes: vec![],
            descriptions: vec![],
        }
    }

    fn make_preset() -> Preset {
        Preset::default()
    }

    fn make_registry(introspection: &IntrospectionResult) -> PgResourceRegistry {
        PgResourceRegistry::from_introspection(introspection)
    }

    // ------------------------------------------------------------------
    // test_gather_builds_resources
    // ------------------------------------------------------------------

    #[test]
    fn test_gather_builds_resources() {
        let intro = make_introspection();
        let preset = make_preset();
        let registry = make_registry(&intro);

        let output = gather(&intro, &registry, &preset).expect("gather failed");

        assert_eq!(output.resources.len(), 2, "expected 2 resources");

        let names: Vec<&str> = output.resources.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"User"), "expected User resource");
        assert!(names.contains(&"Post"), "expected Post resource");

        let user = output.resources.iter().find(|r| r.name == "User").unwrap();
        assert_eq!(user.schema, "public");
        assert_eq!(user.table, "users");
        assert_eq!(user.columns.len(), 3);
        assert_eq!(user.primary_key, vec!["id"]);

        let post = output.resources.iter().find(|r| r.name == "Post").unwrap();
        assert_eq!(post.schema, "public");
        assert_eq!(post.table, "posts");
        assert_eq!(post.columns.len(), 3);
        assert_eq!(post.primary_key, vec!["id"]);
    }

    // ------------------------------------------------------------------
    // test_gather_builds_columns_with_gql_types
    // ------------------------------------------------------------------

    #[test]
    fn test_gather_builds_columns_with_gql_types() {
        let intro = make_introspection();
        let preset = make_preset();
        let registry = make_registry(&intro);

        let output = gather(&intro, &registry, &preset).expect("gather failed");

        let user = output.resources.iter().find(|r| r.name == "User").unwrap();

        // Columns are sorted by pg_name, so: created_at, email, id
        let email_col = user.columns.iter().find(|c| c.pg_name == "email").unwrap();
        assert_eq!(email_col.gql_name, "email");
        assert_eq!(email_col.gql_type, "String!", "email should be String! (not null)");

        let created_col = user.columns.iter().find(|c| c.pg_name == "created_at").unwrap();
        assert_eq!(created_col.gql_name, "createdAt", "created_at → camelCase createdAt");
        assert_eq!(created_col.gql_type, "DateTime!", "timestamptz → DateTime!");
        assert!(created_col.has_default);

        let id_col = user.columns.iter().find(|c| c.pg_name == "id").unwrap();
        assert_eq!(id_col.gql_type, "UUID!", "uuid → UUID!");
    }

    // ------------------------------------------------------------------
    // test_gather_builds_relations
    // ------------------------------------------------------------------

    #[test]
    fn test_gather_builds_relations() {
        let intro = make_introspection();
        let preset = make_preset();
        let registry = make_registry(&intro);

        let output = gather(&intro, &registry, &preset).expect("gather failed");

        assert_eq!(output.relations.len(), 1, "expected 1 relation");

        let rel = &output.relations[0];
        assert_eq!(rel.name, "posts_author_id_fkey");
        assert_eq!(rel.source_resource, "Post");
        assert_eq!(rel.source_columns, vec!["author_id"]);
        assert_eq!(rel.target_resource, "User");
        assert_eq!(rel.target_columns, vec!["id"]);
        // author_id is not PK or unique on posts, so is_unique = false
        assert!(!rel.is_unique, "FK on non-unique column should not be unique");
    }

    // ------------------------------------------------------------------
    // test_gather_default_behaviors_for_table
    // ------------------------------------------------------------------

    #[test]
    fn test_gather_default_behaviors_for_table() {
        let intro = make_introspection();
        let preset = make_preset();
        let registry = make_registry(&intro);

        let output = gather(&intro, &registry, &preset).expect("gather failed");

        let bs = output.behaviors.get("User").expect("User behavior missing");
        assert!(bs.has(BehaviorSet::CONNECTION), "table should have CONNECTION");
        assert!(bs.has(BehaviorSet::SELECT_ONE), "table should have SELECT_ONE");
        assert!(bs.has(BehaviorSet::INSERT), "table should have INSERT");
        assert!(bs.has(BehaviorSet::UPDATE), "table should have UPDATE");
        assert!(bs.has(BehaviorSet::DELETE), "table should have DELETE");
        assert!(bs.has(BehaviorSet::FILTER), "table should have FILTER");
        assert!(bs.has(BehaviorSet::ORDER_BY), "table should have ORDER_BY");
    }

    // ------------------------------------------------------------------
    // test_gather_smart_tag_omit
    // ------------------------------------------------------------------

    #[test]
    fn test_gather_smart_tag_omit() {
        let mut intro = make_introspection();

        // Add a description for "users" (obj_oid=100, obj_sub_id=0) with @omit create,delete
        intro.descriptions.push(PgDescription {
            obj_oid: 100,
            class_oid: 1259, // pg_class catalog oid (arbitrary in test)
            obj_sub_id: 0,
            description: "@omit create,delete".to_string(),
        });

        let preset = make_preset();
        let registry = make_registry(&intro);

        let output = gather(&intro, &registry, &preset).expect("gather failed");

        let bs = output.behaviors.get("User").expect("User behavior missing");
        assert!(!bs.has(BehaviorSet::INSERT), "create should be omitted → INSERT removed");
        assert!(!bs.has(BehaviorSet::DELETE), "delete should be omitted → DELETE removed");
        // Other behaviors should remain.
        assert!(bs.has(BehaviorSet::CONNECTION));
        assert!(bs.has(BehaviorSet::SELECT_ONE));
        assert!(bs.has(BehaviorSet::UPDATE));
    }

    // ------------------------------------------------------------------
    // test_gather_enums
    // ------------------------------------------------------------------

    #[test]
    fn test_gather_enums() {
        let mut intro = make_introspection();

        // Add a PG enum type "status" with 2 values.
        intro.types.push(PgType {
            oid: 500,
            name: "statuses".to_string(),
            schema_oid: 1,
            category: 'E',
            array_element_type_oid: 0,
            base_type_oid: 0,
            class_oid: 0,
            is_enum: true,
        });
        intro.enums.push(PgEnum {
            oid: 501,
            type_oid: 500,
            sort_order: 1.0,
            label: "active".to_string(),
        });
        intro.enums.push(PgEnum {
            oid: 502,
            type_oid: 500,
            sort_order: 2.0,
            label: "inactive".to_string(),
        });

        let preset = make_preset();
        let registry = make_registry(&intro);

        let output = gather(&intro, &registry, &preset).expect("gather failed");

        assert_eq!(output.enums.len(), 1, "expected 1 resolved enum");

        let resolved = &output.enums[0];
        assert_eq!(resolved.name, "Status", "statuses → Status (singularized + PascalCase)");
        assert_eq!(resolved.pg_type_oid, 500);
        assert_eq!(resolved.values, vec!["active", "inactive"], "values sorted by sort_order");
    }
}
