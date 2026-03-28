//! Register per-resource `XCondition` InputObject types.
//!
//! A condition is a simple exact-match filter: one nullable field per column.
//! Used for exact-equality checks (as opposed to the richer XFilter types).

use async_graphql::dynamic::{InputObject, InputValue, SchemaBuilder, TypeRef};

use crate::ir::ResolvedResource;
use crate::naming::condition_type_name;

/// Register `XCondition` InputObject types for all resources.
///
/// Each condition has one nullable field per column (exact match).
pub fn register_condition_types(
    mut builder: SchemaBuilder,
    resources: &[ResolvedResource],
) -> SchemaBuilder {
    for resource in resources {
        let cond_name = condition_type_name(&resource.name);
        let mut input = InputObject::new(&cond_name);

        for col in &resource.columns {
            // All fields are nullable — conditions are optional per-column
            let field = InputValue::new(&col.gql_name, TypeRef::named(&col.gql_type));
            input = input.field(field);
        }

        builder = builder.register(input);
    }
    builder
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::dynamic::{Field, FieldFuture, Object, Schema, TypeRef};
    use crate::ir::{ResolvedColumn, ResourceKind};
    use crate::register::scalars::register_scalars;

    fn make_query() -> Object {
        Object::new("Query").field(Field::new(
            "placeholder",
            TypeRef::named(TypeRef::STRING),
            |_| FieldFuture::from_value(Some(async_graphql::Value::Null)),
        ))
    }

    fn make_user_resource() -> ResolvedResource {
        ResolvedResource {
            name: "User".to_string(),
            schema: "public".to_string(),
            table: "users".to_string(),
            kind: ResourceKind::Table,
            columns: vec![
                ResolvedColumn {
                    pg_name: "id".to_string(),
                    gql_name: "id".to_string(),
                    type_oid: 20,
                    gql_type: TypeRef::INT.to_string(),
                    is_not_null: true,
                    has_default: true,
                },
                ResolvedColumn {
                    pg_name: "email".to_string(),
                    gql_name: "email".to_string(),
                    type_oid: 25,
                    gql_type: TypeRef::STRING.to_string(),
                    is_not_null: false,
                    has_default: false,
                },
            ],
            primary_key: vec!["id".to_string()],
            unique_constraints: vec![],
            class_oid: 10,
        }
    }

    /// Condition types should build a valid schema.
    #[test]
    fn test_register_condition_types_builds_schema() {
        let resources = vec![make_user_resource()];

        let mut builder = Schema::build("Query", None, None);
        builder = register_scalars(builder);
        builder = register_condition_types(builder, &resources);
        builder = builder.register(make_query());

        let schema = builder.finish();
        assert!(schema.is_ok(), "condition types schema should build: {:?}", schema.err());
    }

    /// Empty resource list — schema builds.
    #[test]
    fn test_register_condition_types_empty() {
        let mut builder = Schema::build("Query", None, None);
        builder = register_condition_types(builder, &[]);
        builder = builder.register(make_query());

        let schema = builder.finish();
        assert!(schema.is_ok(), "empty condition types should build: {:?}", schema.err());
    }

    /// condition_type_name should produce the right name.
    #[test]
    fn test_condition_type_name() {
        assert_eq!(condition_type_name("User"), "UserCondition");
        assert_eq!(condition_type_name("Post"), "PostCondition");
    }

    /// Multiple resources should register without conflict.
    #[test]
    fn test_register_multiple_condition_types() {
        let post_resource = ResolvedResource {
            name: "Post".to_string(),
            schema: "public".to_string(),
            table: "posts".to_string(),
            kind: ResourceKind::Table,
            columns: vec![ResolvedColumn {
                pg_name: "title".to_string(),
                gql_name: "title".to_string(),
                type_oid: 25,
                gql_type: TypeRef::STRING.to_string(),
                is_not_null: true,
                has_default: false,
            }],
            primary_key: vec![],
            unique_constraints: vec![],
            class_oid: 20,
        };

        let resources = vec![make_user_resource(), post_resource];

        let mut builder = Schema::build("Query", None, None);
        builder = register_condition_types(builder, &resources);
        builder = builder.register(make_query());

        let schema = builder.finish();
        assert!(schema.is_ok(), "multiple condition types should build: {:?}", schema.err());
    }
}
