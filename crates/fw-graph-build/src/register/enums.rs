//! Register PostgreSQL enum types as GraphQL enums.

use async_graphql::dynamic::{Enum, EnumItem, SchemaBuilder};

use crate::ir::ResolvedEnum;

/// Register all resolved enums with the schema builder.
///
/// Each `ResolvedEnum` becomes a GraphQL `Enum` type with one `EnumItem`
/// per variant.
pub fn register_enums(mut builder: SchemaBuilder, enums: &[ResolvedEnum]) -> SchemaBuilder {
    for resolved in enums {
        let mut gql_enum = Enum::new(&resolved.name);
        for value in &resolved.values {
            gql_enum = gql_enum.item(EnumItem::new(value));
        }
        builder = builder.register(gql_enum);
    }
    builder
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::dynamic::{Field, FieldFuture, Object, Schema, TypeRef};

    fn make_query() -> Object {
        Object::new("Query").field(Field::new(
            "placeholder",
            TypeRef::named(TypeRef::STRING),
            |_| FieldFuture::from_value(Some(async_graphql::Value::Null)),
        ))
    }

    /// Empty enum list — schema should still build.
    #[test]
    fn test_register_enums_empty() {
        let mut builder = Schema::build("Query", None, None);
        builder = register_enums(builder, &[]);
        builder = builder.register(make_query());

        let schema = builder.finish();
        assert!(schema.is_ok(), "empty enums list should produce a valid schema: {:?}", schema.err());
    }

    /// A single enum with three variants should register and build successfully.
    #[test]
    fn test_register_single_enum() {
        let resolved = ResolvedEnum {
            name: "UserRole".to_string(),
            values: vec!["ADMIN".to_string(), "EDITOR".to_string(), "VIEWER".to_string()],
            pg_type_oid: 1000,
        };

        let mut builder = Schema::build("Query", None, None);
        builder = register_enums(builder, &[resolved]);
        builder = builder.register(make_query());

        let schema = builder.finish();
        assert!(schema.is_ok(), "single enum schema should build: {:?}", schema.err());
    }

    /// A field typed as a registered enum should resolve correctly.
    #[test]
    fn test_enum_typed_field_builds() {
        let resolved = ResolvedEnum {
            name: "Status".to_string(),
            values: vec!["ACTIVE".to_string(), "INACTIVE".to_string()],
            pg_type_oid: 1001,
        };

        let query = Object::new("Query").field(Field::new(
            "myStatus",
            TypeRef::named("Status"),
            |_| FieldFuture::from_value(Some(async_graphql::Value::Null)),
        ));

        let mut builder = Schema::build("Query", None, None);
        builder = register_enums(builder, &[resolved]);
        builder = builder.register(query);

        let schema = builder.finish();
        assert!(schema.is_ok(), "enum-typed field schema should build: {:?}", schema.err());
    }

    /// Multiple enums should all register without conflict.
    #[test]
    fn test_register_multiple_enums() {
        let enums = vec![
            ResolvedEnum {
                name: "Color".to_string(),
                values: vec!["RED".to_string(), "GREEN".to_string(), "BLUE".to_string()],
                pg_type_oid: 2000,
            },
            ResolvedEnum {
                name: "Direction".to_string(),
                values: vec!["NORTH".to_string(), "SOUTH".to_string(), "EAST".to_string(), "WEST".to_string()],
                pg_type_oid: 2001,
            },
        ];

        let mut builder = Schema::build("Query", None, None);
        builder = register_enums(builder, &enums);
        builder = builder.register(make_query());

        let schema = builder.finish();
        assert!(schema.is_ok(), "multiple enums should register without conflict: {:?}", schema.err());
    }

    /// An enum with a single variant should build.
    #[test]
    fn test_register_single_variant_enum() {
        let resolved = ResolvedEnum {
            name: "Singleton".to_string(),
            values: vec!["ONLY".to_string()],
            pg_type_oid: 3000,
        };

        let mut builder = Schema::build("Query", None, None);
        builder = register_enums(builder, &[resolved]);
        builder = builder.register(make_query());

        let schema = builder.finish();
        assert!(schema.is_ok(), "single-variant enum should build: {:?}", schema.err());
    }
}
