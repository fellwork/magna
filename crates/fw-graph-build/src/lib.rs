pub mod error;
pub mod executor;
pub mod gather;
pub mod ir;
pub mod naming;
pub mod plan_resolver;
pub mod register;
pub mod resolve;
pub mod smart_tags;
pub mod type_map;
pub mod union_step;

pub use error::BuildError;
pub use gather::gather;
pub use ir::{
    BehaviorSet, GatherOutput, ResolvedColumn, ResolvedEnum, ResolvedRelation, ResolvedResource,
    ResourceKind,
};
pub use plan_resolver::PlanContext;
pub use union_step::{PgUnionStep, TaggedRow};

use std::collections::HashMap;
use std::sync::Arc;

use async_graphql::dynamic::{Field, FieldFuture, Object, Schema};
use async_graphql::Value;
use sqlx::PgPool;

use executor::QueryExecutor;
use executor::dataloader::DataLoaderRegistry;
use resolve::graph::{
    build_concept_thread_resolver, build_related_verses_resolver, build_verse_context_resolver,
    register_graph_types,
};
use resolve::mutation::{build_create_resolver, build_delete_resolver, build_update_resolver};
use resolve::query::{build_allx_resolver, build_by_pk_resolver};
use resolve::relation::{build_belongs_to_resolver, build_has_many_resolver};

use register::conditions::register_condition_types;
use register::connections::{register_connection_types, register_page_info};
use register::enums::register_enums;
use register::filters::register_filter_types;
use register::functions::build_function_fields;
use register::mutations::register_mutation_types;
use register::node_interface::{add_node_id_field, register_node_interface};
use register::object_types::build_object_type;
use register::order_by::register_order_by_types;
use register::scalars::register_scalars;

/// Build a complete GraphQL schema from gathered introspection output.
///
/// Orchestrates all registration passes in the correct order and returns
/// a finished `async_graphql::dynamic::Schema`.
pub fn build_schema(
    output: &GatherOutput,
    behaviors: &HashMap<String, BehaviorSet>,
    pool: PgPool,
) -> Result<Schema, BuildError> {
    // 1. Determine if we need a Mutation root.
    let has_mutations = behaviors.values().any(|bs| {
        bs.has(BehaviorSet::INSERT) || bs.has(BehaviorSet::UPDATE) || bs.has(BehaviorSet::DELETE)
    });

    // Create executor and DataLoader registry (shared across all resolvers).
    let executor = Arc::new(QueryExecutor::new(pool.clone()));
    let mut registry = DataLoaderRegistry::new();

    let mutation_name: Option<&str> = if has_mutations { Some("Mutation") } else { None };

    // 2. Start SchemaBuilder
    let mut builder = Schema::build("Query", mutation_name, None);

    // 3. Register scalars
    builder = register_scalars(builder);

    // 4. Register enums
    builder = register_enums(builder, &output.enums);

    // 5. Build object types, add relation fields + nodeId, then register
    //    We must build all objects, add relation fields and nodeId BEFORE registering.
    //    Use real DataLoader-backed resolver factories instead of stub closures.
    let mut resolved_relation_fields: Vec<(String, Field)> = Vec::new();
    for rel in &output.relations {
        let source_res = output.resources.iter().find(|r| r.name == rel.source_resource);
        let target_res = output.resources.iter().find(|r| r.name == rel.target_resource);
        if let (Some(src), Some(tgt)) = (source_res, target_res) {
            // BelongsTo field on source type.
            let (type_name, field) = build_belongs_to_resolver(rel, src, tgt);
            resolved_relation_fields.push((type_name, field));

            if !rel.is_unique {
                // HasMany field on target type.
                let fk_col = rel.source_columns.first().cloned().unwrap_or_default();
                let fk_oid = src
                    .columns
                    .iter()
                    .find(|c| c.pg_name == fk_col)
                    .map(|c| c.type_oid)
                    .unwrap_or(25);
                let has_many_key = format!("{}:{}", src.name, fk_col);
                registry.register_has_many(
                    &has_many_key,
                    executor.clone(),
                    src.clone(),
                    fk_col,
                    fk_oid,
                    None,
                );
                let (type_name, field) = build_has_many_resolver(rel, src, tgt);
                resolved_relation_fields.push((type_name, field));
            }

            // BelongsTo DataLoader.
            let pk_col = tgt.primary_key.first().cloned().unwrap_or_default();
            let pk_oid = tgt
                .columns
                .iter()
                .find(|c| c.pg_name == pk_col)
                .map(|c| c.type_oid)
                .unwrap_or(25);
            let belongs_to_key = format!("{}:{}", tgt.name, pk_col);
            registry.register_belongs_to(
                &belongs_to_key,
                executor.clone(),
                tgt.clone(),
                pk_col,
                pk_oid,
                None,
            );
        }
    }
    let relation_fields = resolved_relation_fields;

    // Group relation fields by target type name
    let mut relation_fields_by_type: HashMap<String, Vec<Field>> = HashMap::new();
    for (type_name, field) in relation_fields {
        relation_fields_by_type
            .entry(type_name)
            .or_default()
            .push(field);
    }

    // Build and register each object type
    for resource in &output.resources {
        let mut obj = build_object_type(resource);

        // Add relation fields for this type
        if let Some(fields) = relation_fields_by_type.remove(&resource.name) {
            for field in fields {
                obj = obj.field(field);
            }
        }

        // Add nodeId field if the resource has a PK and no column already maps to "nodeId"
        let has_node_id_column = resource.columns.iter().any(|c| c.gql_name == "nodeId");
        if !resource.primary_key.is_empty() && !has_node_id_column {
            obj = add_node_id_field(obj, resource);
        }

        builder = builder.register(obj);
    }

    // 6. Register the Node interface (adds `node(id: ID!)` to Query later)
    let mut query = Object::new("Query").field(Field::new(
        "_placeholder",
        async_graphql::dynamic::TypeRef::named(async_graphql::dynamic::TypeRef::STRING),
        |_| FieldFuture::from_value(Some(Value::Null)),
    ));
    builder = register_node_interface(builder, &mut query, &output.resources);

    // 7. Register PageInfo
    builder = register_page_info(builder);

    // 8. Register connection types for resources with CONNECTION behavior
    for resource in &output.resources {
        let bs = behaviors.get(&resource.name).copied().unwrap_or_else(BehaviorSet::none);
        if bs.has(BehaviorSet::CONNECTION) {
            builder = register_connection_types(builder, resource);
        }
    }

    // 9. Register filter types for resources with FILTER behavior
    let filter_resources: Vec<&ResolvedResource> = output
        .resources
        .iter()
        .filter(|r| {
            behaviors
                .get(&r.name)
                .map(|bs| bs.has(BehaviorSet::FILTER))
                .unwrap_or(false)
        })
        .collect();
    if !filter_resources.is_empty() {
        let owned: Vec<ResolvedResource> = filter_resources.iter().map(|r| (*r).clone()).collect();
        builder = register_filter_types(builder, &owned);
    }

    // 10. Register order_by types for resources with ORDER_BY behavior
    let order_by_resources: Vec<ResolvedResource> = output
        .resources
        .iter()
        .filter(|r| {
            behaviors
                .get(&r.name)
                .map(|bs| bs.has(BehaviorSet::ORDER_BY))
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    if !order_by_resources.is_empty() {
        builder = register_order_by_types(builder, &order_by_resources);
    }

    // 11. Register condition types for all resources
    builder = register_condition_types(builder, &output.resources);

    // 11b. Register custom graph output types (ConceptEdge, VerseXref, VerseContext)
    builder = register_graph_types(builder);

    // 12. Build Query root fields using real resolver factories.
    for resource in &output.resources {
        let bs = behaviors.get(&resource.name).copied().unwrap_or_else(BehaviorSet::none);
        if bs.has(BehaviorSet::CONNECTION) {
            query = query.field(build_allx_resolver(resource, executor.clone()));
        }
        if bs.has(BehaviorSet::SELECT_ONE) {
            query = query.field(build_by_pk_resolver(resource, executor.clone()));
        }
    }

    // 12b. Register concept-graph traversal fields
    query = query.field(build_concept_thread_resolver(executor.clone()));
    query = query.field(build_related_verses_resolver(executor.clone()));
    query = query.field(build_verse_context_resolver(executor.clone()));

    // 13. Build Mutation root using real resolver factories.
    if has_mutations {
        let mut mutation = Object::new("Mutation");

        for resource in &output.resources {
            let bs = behaviors.get(&resource.name).copied().unwrap_or_else(BehaviorSet::none);

            // Register input/payload types (still needed for schema type registration).
            builder = register_mutation_types(builder, resource, &bs);

            // Add real mutation resolver fields.
            if bs.has(BehaviorSet::INSERT) {
                mutation = mutation.field(build_create_resolver(resource, executor.clone()));
            }
            if bs.has(BehaviorSet::UPDATE) {
                mutation = mutation.field(build_update_resolver(resource, executor.clone()));
            }
            if bs.has(BehaviorSet::DELETE) {
                mutation = mutation.field(build_delete_resolver(resource, executor.clone()));
            }
        }

        // 14. Register function fields (mutation portion)
        let (_func_query_fields, func_mutation_fields) = build_function_fields(&[]);
        for field in func_mutation_fields {
            mutation = mutation.field(field);
        }

        builder = builder.register(mutation);
    }

    // 14. Register function fields (query portion)
    let (func_query_fields, _func_mutation_fields) = build_function_fields(&[]);
    for field in func_query_fields {
        query = query.field(field);
    }

    // 15. Register Query object
    builder = builder.register(query);

    // 16. Store executor and DataLoaderRegistry as schema data for resolvers.
    let registry = Arc::new(registry);
    builder = builder.data(registry);
    builder = builder.data(executor);

    // 17. Apply limits and finish
    builder = builder.limit_complexity(200).limit_depth(10);

    builder
        .finish()
        .map_err(|e| BuildError::SchemaFinish(e.to_string()))
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use fw_graph_config::Preset;
    use fw_graph_dataplan::PgResourceRegistry;
    use fw_graph_introspect::{
        ForeignKeyAction, IntrospectionResult, PgAttribute, PgClass, PgClassKind,
        PgConstraint, PgConstraintKind, PgDescription, PgNamespace,
    };

    fn test_pool() -> sqlx::PgPool {
        sqlx::PgPool::connect_lazy(
            "postgresql://postgres:postgres@127.0.0.1:54322/postgres",
        )
        .unwrap()
    }

    /// Minimal test introspection: "public" schema with users + posts tables
    /// and a posts.author_id -> users.id FK.
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
                // posts.author_id -> users.id
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

    #[tokio::test]
    async fn test_full_schema_builds_from_introspection() {
        let intro = make_introspection();
        let preset = make_preset();
        let registry = make_registry(&intro);

        let output = gather(&intro, &registry, &preset).expect("gather failed");
        let result = build_schema(&output, &output.behaviors, test_pool());

        assert!(result.is_ok(), "build_schema should succeed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_schema_sdl_contains_expected_types() {
        let intro = make_introspection();
        let preset = make_preset();
        let registry = make_registry(&intro);

        let output = gather(&intro, &registry, &preset).expect("gather failed");
        let schema = build_schema(&output, &output.behaviors, test_pool()).expect("build_schema failed");

        let sdl = schema.sdl();

        // Object types
        assert!(sdl.contains("type User"), "SDL should contain 'type User', got:\n{}", sdl);
        assert!(sdl.contains("type Post"), "SDL should contain 'type Post', got:\n{}", sdl);

        // Connection types
        assert!(sdl.contains("type UsersConnection"), "SDL should contain 'type UsersConnection'");
        assert!(sdl.contains("type PostsConnection"), "SDL should contain 'type PostsConnection'");

        // Query fields
        assert!(sdl.contains("allUsers"), "SDL should contain 'allUsers'");
        assert!(sdl.contains("allPosts"), "SDL should contain 'allPosts'");

        // Mutation fields
        assert!(sdl.contains("createUser"), "SDL should contain 'createUser'");
        assert!(sdl.contains("updateUser"), "SDL should contain 'updateUser'");
        assert!(sdl.contains("deleteUser"), "SDL should contain 'deleteUser'");

        // PageInfo
        assert!(sdl.contains("type PageInfo"), "SDL should contain 'type PageInfo'");

        // Node interface
        assert!(sdl.contains("interface Node"), "SDL should contain 'interface Node'");
    }
}
