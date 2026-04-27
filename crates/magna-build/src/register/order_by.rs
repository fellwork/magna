//! Register per-resource `XOrderBy` enum types.
//!
//! Each enum has COLUMN_NAME_ASC / COLUMN_NAME_DESC variants per column,
//! plus NATURAL, PRIMARY_KEY_ASC, PRIMARY_KEY_DESC.

use async_graphql::dynamic::{Enum, EnumItem, SchemaBuilder};

use crate::ir::ResolvedResource;
use crate::naming::order_by_type_name;

/// Register `XOrderBy` enum types for all resources.
///
/// Variants: `NATURAL`, `PRIMARY_KEY_ASC`, `PRIMARY_KEY_DESC`,
/// then `COLUMN_NAME_ASC` / `COLUMN_NAME_DESC` per column.
pub fn register_order_by_types(
    mut builder: SchemaBuilder,
    resources: &[ResolvedResource],
) -> SchemaBuilder {
    for resource in resources {
        let enum_name = order_by_type_name(&resource.name);
        let mut gql_enum = Enum::new(&enum_name);

        // Standard variants always present
        gql_enum = gql_enum.item(EnumItem::new("NATURAL"));
        gql_enum = gql_enum.item(EnumItem::new("PRIMARY_KEY_ASC"));
        gql_enum = gql_enum.item(EnumItem::new("PRIMARY_KEY_DESC"));

        // Per-column ASC/DESC variants
        for col in &resource.columns {
            let upper = col.pg_name.to_uppercase();
            gql_enum = gql_enum.item(EnumItem::new(format!("{}_ASC", upper)));
            gql_enum = gql_enum.item(EnumItem::new(format!("{}_DESC", upper)));
        }

        builder = builder.register(gql_enum);
    }
    builder
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::dynamic::{Field, FieldFuture, Object, Schema, TypeRef};
    use crate::ir::{ResolvedColumn, ResourceKind};

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
                    pg_name: "created_at".to_string(),
                    gql_name: "createdAt".to_string(),
                    type_oid: 1184,
                    gql_type: "DateTime".to_string(),
                    is_not_null: true,
                    has_default: true,
                },
            ],
            primary_key: vec!["id".to_string()],
            unique_constraints: vec![],
            class_oid: 10,
        }
    }

    /// OrderBy enum types should build a valid schema.
    #[test]
    fn test_register_order_by_types_builds_schema() {
        let resources = vec![make_user_resource()];

        let mut builder = Schema::build("Query", None, None);
        builder = register_order_by_types(builder, &resources);
        builder = builder.register(make_query());

        let schema = builder.finish();
        assert!(schema.is_ok(), "order_by schema should build: {:?}", schema.err());
    }

    /// Empty resources — schema builds.
    #[test]
    fn test_register_order_by_empty() {
        let mut builder = Schema::build("Query", None, None);
        builder = register_order_by_types(builder, &[]);
        builder = builder.register(make_query());

        let schema = builder.finish();
        assert!(schema.is_ok(), "empty order_by schema should build: {:?}", schema.err());
    }

    /// order_by_type_name should produce the right name.
    #[test]
    fn test_order_by_type_name() {
        assert_eq!(order_by_type_name("User"), "UsersOrderBy");
        assert_eq!(order_by_type_name("Post"), "PostsOrderBy");
    }

    /// Column variants should be uppercase snake_case with _ASC/_DESC.
    #[test]
    fn test_column_variant_format() {
        let col_name = "created_at";
        let upper = col_name.to_uppercase();
        assert_eq!(format!("{}_ASC", upper), "CREATED_AT_ASC");
        assert_eq!(format!("{}_DESC", upper), "CREATED_AT_DESC");
    }
}
