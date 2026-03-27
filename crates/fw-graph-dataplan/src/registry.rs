//! PgResourceRegistry — maps introspected tables to PgSelectStep
//! configurations.
//!
//! Given an [`IntrospectionResult`], the registry creates a [`PgResource`]
//! for each table, capturing its schema, name, columns, primary key, and
//! foreign key relationships. Downstream code (fw-graph-build) uses these
//! resources to wire up the execution plan.

use fw_graph_introspect::{
    IntrospectionResult, PgAttribute, PgClassKind, PgConstraint, PgConstraintKind,
    PgNamespace,
};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// PgResource
// ---------------------------------------------------------------------------

/// A registered Postgres table resource, derived from introspection.
///
/// Contains all the metadata needed to construct PgSelectStep (and mutation
/// steps) for this table at plan-time.
#[derive(Debug, Clone)]
pub struct PgResource {
    /// Postgres OID of the table (pg_class.oid).
    pub class_oid: u32,

    /// Schema name (e.g. "public").
    pub schema: String,

    /// Table name.
    pub table: String,

    /// All columns on this table.
    pub columns: Vec<PgResourceColumn>,

    /// Primary key column name(s). Most tables have a single PK column;
    /// composite PKs produce multiple entries.
    pub primary_key: Vec<String>,

    /// Foreign key relationships originating from this table.
    pub foreign_keys: Vec<PgForeignKey>,

    /// Unique constraints on this table (excluding PK).
    pub unique_constraints: Vec<Vec<String>>,
}

/// A column within a PgResource.
#[derive(Debug, Clone)]
pub struct PgResourceColumn {
    pub name: String,
    pub type_oid: u32,
    pub is_not_null: bool,
    pub has_default: bool,
    pub ordinal: i16,
}

/// A foreign key relationship from this table to another.
#[derive(Debug, Clone)]
pub struct PgForeignKey {
    /// Constraint name.
    pub name: String,
    /// Column name(s) on this (source) table.
    pub columns: Vec<String>,
    /// OID of the referenced (target) table.
    pub foreign_class_oid: u32,
    /// Column name(s) on the referenced table.
    pub foreign_columns: Vec<String>,
}

// ---------------------------------------------------------------------------
// PgResourceRegistry
// ---------------------------------------------------------------------------

/// Registry of all Postgres table resources discovered by introspection.
///
/// Built once from an `IntrospectionResult` and then consulted during plan
/// construction to look up table metadata, columns, and relationships.
#[derive(Debug, Clone)]
pub struct PgResourceRegistry {
    /// Resources keyed by table OID.
    resources_by_oid: HashMap<u32, PgResource>,

    /// Resources keyed by `"schema"."table"` qualified name.
    resources_by_name: HashMap<String, u32>,
}

impl PgResourceRegistry {
    /// Build the registry from introspection results.
    ///
    /// Only tables (PgClassKind::Table) are registered. Views and
    /// materialized views are excluded for now (they can be added later).
    pub fn from_introspection(result: &IntrospectionResult) -> Self {
        let namespace_map: HashMap<u32, &PgNamespace> =
            result.namespaces.iter().map(|ns| (ns.oid, ns)).collect();

        let attr_map: HashMap<u32, Vec<&PgAttribute>> = {
            let mut m: HashMap<u32, Vec<&PgAttribute>> = HashMap::new();
            for attr in &result.attributes {
                m.entry(attr.class_oid).or_default().push(attr);
            }
            m
        };

        let constraint_map: HashMap<u32, Vec<&PgConstraint>> = {
            let mut m: HashMap<u32, Vec<&PgConstraint>> = HashMap::new();
            for con in &result.constraints {
                m.entry(con.class_oid).or_default().push(con);
            }
            m
        };

        let mut resources_by_oid = HashMap::new();
        let mut resources_by_name = HashMap::new();

        for class in &result.classes {
            if class.kind != PgClassKind::Table {
                continue;
            }

            let schema_name = namespace_map
                .get(&class.schema_oid)
                .map(|ns| ns.name.clone())
                .unwrap_or_else(|| "public".to_string());

            let attrs = attr_map.get(&class.oid).cloned().unwrap_or_default();
            let constraints = constraint_map.get(&class.oid).cloned().unwrap_or_default();

            // Build column list
            let mut columns: Vec<PgResourceColumn> = attrs
                .iter()
                .map(|a| PgResourceColumn {
                    name: a.name.clone(),
                    type_oid: a.type_oid,
                    is_not_null: a.is_not_null,
                    has_default: a.has_default,
                    ordinal: a.num,
                })
                .collect();
            columns.sort_by_key(|c| c.ordinal);

            // Extract primary key
            let primary_key = extract_primary_key(&constraints, &attrs);

            // Extract foreign keys
            let foreign_keys =
                extract_foreign_keys(&constraints, &attrs, &result.attributes);

            // Extract unique constraints (excluding PK)
            let unique_constraints = extract_unique_constraints(&constraints, &attrs);

            let qualified = format!("{}.{}", schema_name, class.name);

            let resource = PgResource {
                class_oid: class.oid,
                schema: schema_name,
                table: class.name.clone(),
                columns,
                primary_key,
                foreign_keys,
                unique_constraints,
            };

            resources_by_name.insert(qualified, class.oid);
            resources_by_oid.insert(class.oid, resource);
        }

        Self {
            resources_by_oid,
            resources_by_name,
        }
    }

    /// Look up a resource by table OID.
    pub fn get_by_oid(&self, oid: u32) -> Option<&PgResource> {
        self.resources_by_oid.get(&oid)
    }

    /// Look up a resource by qualified name (`"schema.table"`).
    pub fn get_by_name(&self, schema: &str, table: &str) -> Option<&PgResource> {
        let key = format!("{}.{}", schema, table);
        self.resources_by_name
            .get(&key)
            .and_then(|oid| self.resources_by_oid.get(oid))
    }

    /// Iterator over all registered resources.
    pub fn iter(&self) -> impl Iterator<Item = &PgResource> {
        self.resources_by_oid.values()
    }

    /// Number of registered resources.
    pub fn len(&self) -> usize {
        self.resources_by_oid.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.resources_by_oid.is_empty()
    }

    /// Get all column names for a table.
    pub fn column_names(&self, schema: &str, table: &str) -> Vec<String> {
        self.get_by_name(schema, table)
            .map(|r| r.columns.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the PK column names from constraints.
fn extract_primary_key(
    constraints: &[&PgConstraint],
    attrs: &[&PgAttribute],
) -> Vec<String> {
    for con in constraints {
        if con.kind == PgConstraintKind::PrimaryKey {
            return con
                .key_attrs
                .iter()
                .filter_map(|&num| {
                    attrs
                        .iter()
                        .find(|a| a.num == num)
                        .map(|a| a.name.clone())
                })
                .collect();
        }
    }
    Vec::new()
}

/// Extract foreign key relationships.
fn extract_foreign_keys(
    constraints: &[&PgConstraint],
    local_attrs: &[&PgAttribute],
    all_attrs: &[PgAttribute],
) -> Vec<PgForeignKey> {
    let mut fks = Vec::new();
    for con in constraints {
        if con.kind != PgConstraintKind::ForeignKey {
            continue;
        }
        let Some(foreign_class_oid) = con.foreign_class_oid else {
            continue;
        };
        let Some(ref foreign_key_attrs) = con.foreign_key_attrs else {
            continue;
        };

        let local_cols: Vec<String> = con
            .key_attrs
            .iter()
            .filter_map(|&num| {
                local_attrs
                    .iter()
                    .find(|a| a.num == num)
                    .map(|a| a.name.clone())
            })
            .collect();

        let foreign_cols: Vec<String> = foreign_key_attrs
            .iter()
            .filter_map(|&num| {
                all_attrs
                    .iter()
                    .find(|a| a.class_oid == foreign_class_oid && a.num == num)
                    .map(|a| a.name.clone())
            })
            .collect();

        fks.push(PgForeignKey {
            name: con.name.clone(),
            columns: local_cols,
            foreign_class_oid,
            foreign_columns: foreign_cols,
        });
    }
    fks
}

/// Extract unique constraints (excluding PK).
fn extract_unique_constraints(
    constraints: &[&PgConstraint],
    attrs: &[&PgAttribute],
) -> Vec<Vec<String>> {
    constraints
        .iter()
        .filter(|c| c.kind == PgConstraintKind::Unique)
        .map(|con| {
            con.key_attrs
                .iter()
                .filter_map(|&num| {
                    attrs
                        .iter()
                        .find(|a| a.num == num)
                        .map(|a| a.name.clone())
                })
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use fw_graph_introspect::*;

    /// Build a minimal IntrospectionResult with two tables: users and posts.
    /// posts.author_id references users.id.
    fn mock_introspection() -> IntrospectionResult {
        IntrospectionResult {
            namespaces: vec![PgNamespace {
                oid: 2200,
                name: "public".to_string(),
            }],
            classes: vec![
                PgClass {
                    oid: 100,
                    name: "users".to_string(),
                    schema_oid: 2200,
                    kind: PgClassKind::Table,
                    is_rls_enabled: true,
                },
                PgClass {
                    oid: 200,
                    name: "posts".to_string(),
                    schema_oid: 2200,
                    kind: PgClassKind::Table,
                    is_rls_enabled: false,
                },
                // A view — should NOT be registered
                PgClass {
                    oid: 300,
                    name: "active_users".to_string(),
                    schema_oid: 2200,
                    kind: PgClassKind::View,
                    is_rls_enabled: false,
                },
            ],
            attributes: vec![
                // users columns
                PgAttribute {
                    class_oid: 100,
                    name: "id".to_string(),
                    type_oid: 2950,
                    num: 1,
                    is_not_null: true,
                    has_default: true,
                    is_identity: false,
                },
                PgAttribute {
                    class_oid: 100,
                    name: "name".to_string(),
                    type_oid: 25,
                    num: 2,
                    is_not_null: true,
                    has_default: false,
                    is_identity: false,
                },
                PgAttribute {
                    class_oid: 100,
                    name: "email".to_string(),
                    type_oid: 25,
                    num: 3,
                    is_not_null: true,
                    has_default: false,
                    is_identity: false,
                },
                // posts columns
                PgAttribute {
                    class_oid: 200,
                    name: "id".to_string(),
                    type_oid: 2950,
                    num: 1,
                    is_not_null: true,
                    has_default: true,
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
                // posts.author_id -> users.id FK
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
                // users.email unique
                PgConstraint {
                    oid: 4000,
                    name: "users_email_key".to_string(),
                    class_oid: 100,
                    kind: PgConstraintKind::Unique,
                    key_attrs: vec![3],
                    foreign_class_oid: None,
                    foreign_key_attrs: None,
                    on_delete: None,
                    on_update: None,
                },
            ],
            procs: vec![],
            types: vec![
                PgType {
                    oid: 2950,
                    name: "uuid".to_string(),
                    schema_oid: 11,
                    category: 'U',
                    array_element_type_oid: 0,
                    base_type_oid: 0,
                    class_oid: 0,
                    is_enum: false,
                },
                PgType {
                    oid: 25,
                    name: "text".to_string(),
                    schema_oid: 11,
                    category: 'S',
                    array_element_type_oid: 0,
                    base_type_oid: 0,
                    class_oid: 0,
                    is_enum: false,
                },
            ],
            enums: vec![],
            indexes: vec![],
            descriptions: vec![],
        }
    }

    #[test]
    fn registry_from_introspection_registers_tables() {
        let result = mock_introspection();
        let registry = PgResourceRegistry::from_introspection(&result);

        // Should register 2 tables (users, posts), not the view
        assert_eq!(registry.len(), 2);
        assert!(!registry.is_empty());
    }

    #[test]
    fn registry_lookup_by_name() {
        let result = mock_introspection();
        let registry = PgResourceRegistry::from_introspection(&result);

        let users = registry.get_by_name("public", "users");
        assert!(users.is_some());
        let users = users.unwrap();
        assert_eq!(users.table, "users");
        assert_eq!(users.schema, "public");

        let missing = registry.get_by_name("public", "nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn registry_lookup_by_oid() {
        let result = mock_introspection();
        let registry = PgResourceRegistry::from_introspection(&result);

        let users = registry.get_by_oid(100);
        assert!(users.is_some());
        assert_eq!(users.unwrap().table, "users");

        let posts = registry.get_by_oid(200);
        assert!(posts.is_some());
        assert_eq!(posts.unwrap().table, "posts");
    }

    #[test]
    fn registry_columns_are_ordered() {
        let result = mock_introspection();
        let registry = PgResourceRegistry::from_introspection(&result);

        let users = registry.get_by_name("public", "users").unwrap();
        assert_eq!(users.columns.len(), 3);
        assert_eq!(users.columns[0].name, "id");
        assert_eq!(users.columns[1].name, "name");
        assert_eq!(users.columns[2].name, "email");
    }

    #[test]
    fn registry_primary_key_extraction() {
        let result = mock_introspection();
        let registry = PgResourceRegistry::from_introspection(&result);

        let users = registry.get_by_name("public", "users").unwrap();
        assert_eq!(users.primary_key, vec!["id"]);

        let posts = registry.get_by_name("public", "posts").unwrap();
        assert_eq!(posts.primary_key, vec!["id"]);
    }

    #[test]
    fn registry_foreign_key_extraction() {
        let result = mock_introspection();
        let registry = PgResourceRegistry::from_introspection(&result);

        let posts = registry.get_by_name("public", "posts").unwrap();
        assert_eq!(posts.foreign_keys.len(), 1);

        let fk = &posts.foreign_keys[0];
        assert_eq!(fk.name, "posts_author_id_fkey");
        assert_eq!(fk.columns, vec!["author_id"]);
        assert_eq!(fk.foreign_class_oid, 100);
        assert_eq!(fk.foreign_columns, vec!["id"]);
    }

    #[test]
    fn registry_unique_constraints() {
        let result = mock_introspection();
        let registry = PgResourceRegistry::from_introspection(&result);

        let users = registry.get_by_name("public", "users").unwrap();
        assert_eq!(users.unique_constraints.len(), 1);
        assert_eq!(users.unique_constraints[0], vec!["email"]);
    }

    #[test]
    fn registry_views_are_excluded() {
        let result = mock_introspection();
        let registry = PgResourceRegistry::from_introspection(&result);

        // The view "active_users" should not be registered
        let view = registry.get_by_name("public", "active_users");
        assert!(view.is_none());
    }

    #[test]
    fn registry_column_names_helper() {
        let result = mock_introspection();
        let registry = PgResourceRegistry::from_introspection(&result);

        let cols = registry.column_names("public", "users");
        assert_eq!(cols, vec!["id", "name", "email"]);

        let empty = registry.column_names("public", "nonexistent");
        assert!(empty.is_empty());
    }

    #[test]
    fn registry_iter() {
        let result = mock_introspection();
        let registry = PgResourceRegistry::from_introspection(&result);

        let names: Vec<String> = registry.iter().map(|r| r.table.clone()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"users".to_string()));
        assert!(names.contains(&"posts".to_string()));
    }

    #[test]
    fn registry_column_metadata() {
        let result = mock_introspection();
        let registry = PgResourceRegistry::from_introspection(&result);

        let users = registry.get_by_name("public", "users").unwrap();
        let id_col = users.columns.iter().find(|c| c.name == "id").unwrap();
        assert!(id_col.is_not_null);
        assert!(id_col.has_default);
        assert_eq!(id_col.type_oid, 2950); // uuid

        let name_col = users.columns.iter().find(|c| c.name == "name").unwrap();
        assert!(name_col.is_not_null);
        assert!(!name_col.has_default);
        assert_eq!(name_col.type_oid, 25); // text
    }
}
