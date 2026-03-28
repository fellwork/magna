//! Register per-resource `XFilter` InputObject types.
//!
//! Each filter has per-column operator fields (e.g. `email_eq`, `age_gt`)
//! plus `and`, `or`, `not` combinators.

use async_graphql::dynamic::{InputObject, InputValue, SchemaBuilder, TypeRef};

use crate::ir::ResolvedResource;
use crate::naming::filter_type_name;

// ── Column type classification ────────────────────────────────────────────────

/// What category of filter operators applies to a column's GraphQL type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    String,
    Numeric,
    Boolean,
    Other,
}

/// Classify a GraphQL type name into a FilterKind.
pub fn classify_filter_kind(gql_type: &str) -> FilterKind {
    match gql_type {
        "String" | "UUID" | "Cursor" => FilterKind::String,
        "Int" | "Float" | "BigInt" => FilterKind::Numeric,
        "DateTime" | "Date" => FilterKind::Numeric, // same range operators
        "Boolean" => FilterKind::Boolean,
        _ => FilterKind::Other,
    }
}

/// Return the list of operator suffixes for a given FilterKind.
pub fn operators_for_kind(kind: FilterKind) -> Vec<&'static str> {
    match kind {
        FilterKind::String => vec!["eq", "ne", "in", "like", "ilike", "startsWith", "isNull"],
        FilterKind::Numeric => vec!["eq", "ne", "lt", "lte", "gt", "gte", "in", "isNull"],
        FilterKind::Boolean => vec!["eq", "ne", "isNull"],
        FilterKind::Other => vec!["eq", "ne", "in", "isNull"],
    }
}

// ── Registration ──────────────────────────────────────────────────────────────

/// Register `XFilter` InputObject types for all resources.
///
/// Each filter includes per-column operator fields and `and`/`or`/`not`
/// combinators.
pub fn register_filter_types(
    mut builder: SchemaBuilder,
    resources: &[ResolvedResource],
) -> SchemaBuilder {
    for resource in resources {
        let filter_name = filter_type_name(&resource.name);
        let mut input = InputObject::new(&filter_name);

        // Per-column operator fields
        for col in &resource.columns {
            let base_type = col.gql_type.trim_end_matches('!');
            let kind = classify_filter_kind(base_type);
            let ops = operators_for_kind(kind);

            for op in ops {
                let field_name = format!("{}_{}", col.gql_name, op);

                let field = if op == "isNull" {
                    // isNull takes a Boolean
                    InputValue::new(field_name, TypeRef::named(TypeRef::BOOLEAN))
                } else if op == "in" {
                    // in takes a list of the column's type
                    InputValue::new(field_name, TypeRef::named_list(base_type))
                } else {
                    // eq, ne, lt, etc. take the column's scalar type (nullable)
                    InputValue::new(field_name, TypeRef::named(base_type))
                };

                input = input.field(field);
            }
        }

        // Combinators: and / or take a list of XFilter, not takes a single XFilter
        let filter_name_clone = filter_name.clone();
        input = input.field(InputValue::new(
            "and",
            TypeRef::named_list(&filter_name_clone),
        ));
        input = input.field(InputValue::new(
            "or",
            TypeRef::named_list(&filter_name_clone),
        ));
        input = input.field(InputValue::new("not", TypeRef::named(&filter_name_clone)));

        builder = builder.register(input);
    }
    builder
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::dynamic::{Field, FieldFuture, Object, Schema, TypeRef as TR};
    use crate::ir::{ResolvedColumn, ResourceKind};
    use crate::register::scalars::register_scalars;

    fn make_query() -> Object {
        Object::new("Query").field(Field::new(
            "placeholder",
            TR::named(TR::STRING),
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
                    gql_type: TR::INT.to_string(),
                    is_not_null: true,
                    has_default: true,
                },
                ResolvedColumn {
                    pg_name: "email".to_string(),
                    gql_name: "email".to_string(),
                    type_oid: 25,
                    gql_type: TR::STRING.to_string(),
                    is_not_null: false,
                    has_default: false,
                },
                ResolvedColumn {
                    pg_name: "active".to_string(),
                    gql_name: "active".to_string(),
                    type_oid: 16,
                    gql_type: TR::BOOLEAN.to_string(),
                    is_not_null: false,
                    has_default: false,
                },
            ],
            primary_key: vec!["id".to_string()],
            unique_constraints: vec![],
            class_oid: 10,
        }
    }

    /// Filter types should build a valid schema.
    #[test]
    fn test_register_filter_types_builds_schema() {
        let resources = vec![make_user_resource()];

        let mut builder = Schema::build("Query", None, None);
        builder = register_scalars(builder);
        builder = register_filter_types(builder, &resources);
        builder = builder.register(make_query());

        let schema = builder.finish();
        assert!(schema.is_ok(), "filter types schema should build: {:?}", schema.err());
    }

    /// Empty resource list — schema should build.
    #[test]
    fn test_register_filter_types_empty() {
        let mut builder = Schema::build("Query", None, None);
        builder = register_filter_types(builder, &[]);
        builder = builder.register(make_query());

        let schema = builder.finish();
        assert!(schema.is_ok(), "empty filter types should build: {:?}", schema.err());
    }

    /// String columns should get string-specific operators.
    #[test]
    fn test_string_operators() {
        let ops = operators_for_kind(FilterKind::String);
        assert!(ops.contains(&"eq"));
        assert!(ops.contains(&"like"));
        assert!(ops.contains(&"ilike"));
        assert!(ops.contains(&"startsWith"));
        assert!(ops.contains(&"isNull"));
        // Should NOT have numeric operators
        assert!(!ops.contains(&"lt"));
        assert!(!ops.contains(&"gt"));
    }

    /// Numeric columns should get range operators.
    #[test]
    fn test_numeric_operators() {
        let ops = operators_for_kind(FilterKind::Numeric);
        assert!(ops.contains(&"eq"));
        assert!(ops.contains(&"lt"));
        assert!(ops.contains(&"lte"));
        assert!(ops.contains(&"gt"));
        assert!(ops.contains(&"gte"));
        assert!(ops.contains(&"isNull"));
        // Should NOT have like
        assert!(!ops.contains(&"like"));
    }

    /// Boolean columns get only eq/ne/isNull.
    #[test]
    fn test_boolean_operators() {
        let ops = operators_for_kind(FilterKind::Boolean);
        assert!(ops.contains(&"eq"));
        assert!(ops.contains(&"ne"));
        assert!(ops.contains(&"isNull"));
        assert!(!ops.contains(&"like"));
        assert!(!ops.contains(&"lt"));
    }

    /// classify_filter_kind maps types correctly.
    #[test]
    fn test_classify_filter_kind() {
        assert_eq!(classify_filter_kind("String"), FilterKind::String);
        assert_eq!(classify_filter_kind("UUID"), FilterKind::String);
        assert_eq!(classify_filter_kind("Int"), FilterKind::Numeric);
        assert_eq!(classify_filter_kind("Float"), FilterKind::Numeric);
        assert_eq!(classify_filter_kind("BigInt"), FilterKind::Numeric);
        assert_eq!(classify_filter_kind("DateTime"), FilterKind::Numeric);
        assert_eq!(classify_filter_kind("Boolean"), FilterKind::Boolean);
        assert_eq!(classify_filter_kind("SomeEnum"), FilterKind::Other);
    }
}
