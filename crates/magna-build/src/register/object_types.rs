//! Register PostgreSQL table/view resources as GraphQL object types.

use async_graphql::dynamic::{Field, FieldFuture, FieldValue, Object, SchemaBuilder, TypeRef};
use async_graphql::Value;
use magna_types::{PgRow, PgValue};

use crate::ir::ResolvedResource;

// ── JSON → async_graphql::Value conversion ────────────────────────────────────

/// Convert a `serde_json::Value` into an `async_graphql::Value`.
pub fn json_to_gql_value(val: &serde_json::Value) -> Value {
    match val {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::from(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(f) = n.as_f64() {
                Value::from(f)
            } else {
                Value::from(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::from(s.as_str()),
        serde_json::Value::Array(arr) => {
            Value::List(arr.iter().map(json_to_gql_value).collect())
        }
        serde_json::Value::Object(map) => {
            let obj: async_graphql::indexmap::IndexMap<async_graphql::Name, Value> = map
                .iter()
                .map(|(k, v)| (async_graphql::Name::new(k), json_to_gql_value(v)))
                .collect();
            Value::Object(obj)
        }
    }
}

// ── PgValue → FieldValue conversion ──────────────────────────────────────────

/// Convert a `PgValue` into a `FieldValue` suitable for async_graphql resolvers.
pub fn pg_value_to_field_value(val: &PgValue) -> FieldValue<'static> {
    match val {
        PgValue::Null => FieldValue::value(Value::Null),
        PgValue::Bool(b) => FieldValue::value(Value::from(*b)),
        PgValue::Int(i) => FieldValue::value(Value::from(*i)),
        PgValue::Float(f) => FieldValue::value(Value::from(*f)),
        PgValue::Text(s) => FieldValue::value(Value::from(s.clone())),
        PgValue::Uuid(u) => FieldValue::value(Value::from(u.to_string())),
        PgValue::Timestamp(t) => FieldValue::value(Value::from(t.to_rfc3339())),
        PgValue::Json(j) => FieldValue::value(json_to_gql_value(j)),
        PgValue::Array(arr) => FieldValue::list(arr.iter().map(pg_value_to_field_value)),
    }
}

// ── Object type registration ──────────────────────────────────────────────────

/// Register all resolved resources as GraphQL object types.
///
/// Each `ResolvedResource` becomes a GraphQL `Object` with one field per
/// `ResolvedColumn`. Field resolvers read the column by `pg_name` from a
/// `PgRow` stored as the parent `FieldValue`.
pub fn register_object_types(
    mut builder: SchemaBuilder,
    resources: &[ResolvedResource],
) -> SchemaBuilder {
    for resource in resources {
        let obj = build_object_type(resource);
        builder = builder.register(obj);
    }
    builder
}

/// Convert a gql_type string (possibly with array brackets and `!` suffix) to a TypeRef.
/// Handles: "String", "String!", "[String]", "[String]!", "[UUID]", etc.
pub fn gql_type_to_type_ref(gql_type: &str, is_not_null: bool) -> TypeRef {
    let base = gql_type.trim_end_matches('!');

    if base.starts_with('[') && base.ends_with(']') {
        // Array type: "[ElementType]"
        let element = &base[1..base.len() - 1];
        if is_not_null {
            TypeRef::named_nn_list_nn(element)
        } else {
            TypeRef::named_list(element)
        }
    } else if is_not_null {
        TypeRef::named_nn(base)
    } else {
        TypeRef::named(base)
    }
}

/// Build a single GraphQL `Object` for a `ResolvedResource`.
pub fn build_object_type(resource: &ResolvedResource) -> Object {
    let mut obj = Object::new(&resource.name);

    for column in &resource.columns {
        let pg_name = column.pg_name.clone();
        let gql_name = column.gql_name.clone();
        let is_not_null = column.is_not_null;

        let type_ref = gql_type_to_type_ref(&column.gql_type, is_not_null);

        let field = Field::new(gql_name, type_ref, move |ctx| {
            let pg_name = pg_name.clone();
            FieldFuture::new(async move {
                let row = match ctx.parent_value.try_downcast_ref::<PgRow>() {
                    Ok(r) => r,
                    Err(_) => return Ok(Some(FieldValue::value(Value::Null))),
                };

                match row.get(pg_name.as_str()) {
                    Some(val) => Ok(Some(pg_value_to_field_value(val))),
                    None => Ok(Some(FieldValue::value(Value::Null))),
                }
            })
        });

        obj = obj.field(field);
    }

    obj
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::dynamic::{Schema};
    use crate::ir::ResolvedColumn;

    fn make_query_placeholder() -> Object {
        Object::new("Query").field(Field::new(
            "placeholder",
            TypeRef::named(TypeRef::STRING),
            |_| FieldFuture::from_value(Some(Value::Null)),
        ))
    }

    fn make_user_resource() -> ResolvedResource {
        ResolvedResource {
            name: "User".to_string(),
            schema: "public".to_string(),
            table: "users".to_string(),
            kind: crate::ir::ResourceKind::Table,
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
                    pg_name: "name".to_string(),
                    gql_name: "name".to_string(),
                    type_oid: 25,
                    gql_type: TypeRef::STRING.to_string(),
                    is_not_null: false,
                    has_default: false,
                },
            ],
            primary_key: vec!["id".to_string()],
            unique_constraints: vec![],
            class_oid: 12345,
        }
    }

    /// A schema with a User object type should build successfully.
    #[test]
    fn test_register_object_types_builds_schema() {
        let resources = vec![make_user_resource()];

        let query = Object::new("Query").field(Field::new(
            "user",
            TypeRef::named("User"),
            |_| FieldFuture::from_value(Some(Value::Null)),
        ));

        let mut builder = Schema::build("Query", None, None);
        builder = register_object_types(builder, &resources);
        builder = builder.register(query);

        let schema = builder.finish();
        assert!(schema.is_ok(), "User object type schema should build: {:?}", schema.err());
    }

    /// Empty resources list — schema should build.
    #[test]
    fn test_register_object_types_empty() {
        let mut builder = Schema::build("Query", None, None);
        builder = register_object_types(builder, &[]);
        builder = builder.register(make_query_placeholder());

        let schema = builder.finish();
        assert!(schema.is_ok(), "empty resources schema should build: {:?}", schema.err());
    }

    // ── PgValue conversion tests ──────────────────────────────────────────────

    #[test]
    fn test_pg_value_null() {
        let fv = pg_value_to_field_value(&PgValue::Null);
        // FieldValue::value(Value::Null) — verify it's not a list/object etc.
        // We can't deeply inspect FieldValue, but construction must not panic.
        let _ = fv;
    }

    #[test]
    fn test_pg_value_bool() {
        let _ = pg_value_to_field_value(&PgValue::Bool(true));
        let _ = pg_value_to_field_value(&PgValue::Bool(false));
    }

    #[test]
    fn test_pg_value_int() {
        let _ = pg_value_to_field_value(&PgValue::Int(42));
        let _ = pg_value_to_field_value(&PgValue::Int(-1));
    }

    #[test]
    fn test_pg_value_float() {
        let _ = pg_value_to_field_value(&PgValue::Float(3.14));
    }

    #[test]
    fn test_pg_value_text() {
        let _ = pg_value_to_field_value(&PgValue::Text("hello".to_string()));
    }

    #[test]
    fn test_pg_value_uuid() {
        let u = uuid::Uuid::new_v4();
        let _ = pg_value_to_field_value(&PgValue::Uuid(u));
    }

    #[test]
    fn test_pg_value_timestamp() {
        let ts = chrono::DateTime::from_timestamp(0, 0).unwrap();
        let _ = pg_value_to_field_value(&PgValue::Timestamp(ts));
    }

    #[test]
    fn test_pg_value_json() {
        let j = serde_json::json!({ "key": "value", "num": 42 });
        let _ = pg_value_to_field_value(&PgValue::Json(j));
    }

    #[test]
    fn test_pg_value_array() {
        let arr = PgValue::Array(vec![
            PgValue::Int(1),
            PgValue::Int(2),
            PgValue::Int(3),
        ]);
        let _ = pg_value_to_field_value(&arr);
    }

    #[test]
    fn test_pg_value_array_nested() {
        let arr = PgValue::Array(vec![
            PgValue::Text("a".to_string()),
            PgValue::Null,
            PgValue::Bool(true),
        ]);
        let _ = pg_value_to_field_value(&arr);
    }

    // ── json_to_gql_value tests ───────────────────────────────────────────────

    #[test]
    fn test_json_null() {
        let v = json_to_gql_value(&serde_json::Value::Null);
        assert!(matches!(v, Value::Null));
    }

    #[test]
    fn test_json_bool() {
        let v = json_to_gql_value(&serde_json::json!(true));
        assert!(matches!(v, Value::Boolean(true)));
    }

    #[test]
    fn test_json_number_int() {
        let v = json_to_gql_value(&serde_json::json!(7));
        assert!(matches!(v, Value::Number(_)));
    }

    #[test]
    fn test_json_number_float() {
        let v = json_to_gql_value(&serde_json::json!(1.5));
        assert!(matches!(v, Value::Number(_)));
    }

    #[test]
    fn test_json_string() {
        let v = json_to_gql_value(&serde_json::json!("hello"));
        assert!(matches!(v, Value::String(ref s) if s == "hello"));
    }

    #[test]
    fn test_json_array() {
        let v = json_to_gql_value(&serde_json::json!([1, 2, 3]));
        assert!(matches!(v, Value::List(_)));
    }

    #[test]
    fn test_json_object() {
        let v = json_to_gql_value(&serde_json::json!({ "a": 1 }));
        assert!(matches!(v, Value::Object(_)));
    }
}
