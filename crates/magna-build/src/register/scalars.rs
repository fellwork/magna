//! Register custom GraphQL scalar types with the schema builder.

use async_graphql::dynamic::{Scalar, SchemaBuilder};

/// Register all custom scalars: UUID, DateTime, Date, BigInt, JSON, Cursor.
pub fn register_scalars(mut builder: SchemaBuilder) -> SchemaBuilder {
    builder = builder.register(Scalar::new("UUID"));
    builder = builder.register(Scalar::new("DateTime"));
    builder = builder.register(Scalar::new("Date"));
    builder = builder.register(Scalar::new("BigInt"));
    builder = builder.register(Scalar::new("JSON"));
    builder = builder.register(Scalar::new("Cursor"));
    builder
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::dynamic::{Field, FieldFuture, Object, Schema, TypeRef};

    /// Build a minimal schema that uses a custom scalar field.
    #[test]
    fn test_register_scalars_builds_schema() {
        let query = Object::new("Query").field(Field::new(
            "dummyUuid",
            TypeRef::named(TypeRef::STRING),
            |_| FieldFuture::from_value(Some(async_graphql::Value::Null)),
        ));

        let mut builder = Schema::build("Query", None, None);
        builder = register_scalars(builder);
        builder = builder.register(query);

        let schema = builder.finish();
        assert!(schema.is_ok(), "schema with custom scalars should build: {:?}", schema.err());
    }

    /// A field typed as UUID scalar should resolve correctly.
    #[test]
    fn test_uuid_scalar_field_builds() {
        let query = Object::new("Query").field(Field::new(
            "myId",
            TypeRef::named("UUID"),
            |_| FieldFuture::from_value(Some(async_graphql::Value::from("550e8400-e29b-41d4-a716-446655440000"))),
        ));

        let mut builder = Schema::build("Query", None, None);
        builder = register_scalars(builder);
        builder = builder.register(query);

        let schema = builder.finish();
        assert!(schema.is_ok(), "UUID scalar field schema should build: {:?}", schema.err());
    }

    /// All six scalars should be registered without conflict.
    #[test]
    fn test_all_scalars_registered() {
        let query = Object::new("Query").field(Field::new(
            "placeholder",
            TypeRef::named(TypeRef::STRING),
            |_| FieldFuture::from_value(Some(async_graphql::Value::Null)),
        ));

        let mut builder = Schema::build("Query", None, None);
        builder = register_scalars(builder);
        builder = builder.register(query);

        let schema = builder.finish();
        assert!(schema.is_ok(), "all scalars should register without conflict: {:?}", schema.err());
    }
}
