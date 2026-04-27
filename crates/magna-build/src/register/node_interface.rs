//! Register the Relay-style `Node` interface and helpers for nodeId encoding.

use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, Interface, InterfaceField, Object, SchemaBuilder, TypeRef,
};
use async_graphql::Value;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use magna_types::{PgRow, PgValue};

use crate::ir::ResolvedResource;

// ── NodeId encoding / decoding ────────────────────────────────────────────────

/// Encode a node ID from a type name and primary-key column→value pairs.
///
/// Simple PK (single column):   base64("TypeName:pk_value")
/// Composite PK (multi-column): base64("TypeName:col1=val1,col2=val2")
pub fn encode_node_id(type_name: &str, pk_values: &[(&str, &str)]) -> String {
    let payload = match pk_values {
        [] => format!("{}:", type_name),
        [(_, value)] => format!("{}:{}", type_name, value),
        pairs => {
            let kv: Vec<String> = pairs
                .iter()
                .map(|(col, val)| format!("{}={}", col, val))
                .collect();
            format!("{}:{}", type_name, kv.join(","))
        }
    };
    BASE64.encode(payload.as_bytes())
}

/// Decode a node ID into `(type_name, pk_string)`.
///
/// Returns `Err` for invalid base64 or a missing `:` separator.
pub fn decode_node_id(node_id: &str) -> Result<(String, String), String> {
    let bytes = BASE64
        .decode(node_id)
        .map_err(|e| format!("invalid base64: {}", e))?;
    let text = String::from_utf8(bytes).map_err(|e| format!("invalid UTF-8: {}", e))?;
    let sep = text
        .find(':')
        .ok_or_else(|| "node ID missing ':' separator".to_string())?;
    let type_name = text[..sep].to_string();
    let pk_str = text[sep + 1..].to_string();
    Ok((type_name, pk_str))
}

// ── Interface registration ────────────────────────────────────────────────────

/// Register the `Node` interface and wire up a `node(id: ID!)` root query field.
///
/// Also adds a stub `node` field on `query` that accepts an ID and returns null
/// (the real implementation is in a later build phase).
pub fn register_node_interface(
    mut builder: SchemaBuilder,
    query: &mut Object,
    _resources: &[ResolvedResource],
) -> SchemaBuilder {
    let node_interface = Interface::new("Node")
        .field(InterfaceField::new("nodeId", TypeRef::named_nn(TypeRef::ID)));

    builder = builder.register(node_interface);

    // Temporarily take ownership of `*query` so we can chain `.field()` on it.
    // We swap in a placeholder and then put the modified object back.
    let placeholder = Object::new("Query");
    let current = std::mem::replace(query, placeholder);

    let updated = current.field(Field::new(
        "node",
        TypeRef::named("Node"),
        |_ctx| FieldFuture::from_value(Some(Value::Null)),
    ));

    *query = updated;

    builder
}

/// Add a `nodeId` field to an object type and declare it implements `Node`.
///
/// Reads the primary-key columns from the parent `PgRow` and encodes them
/// as a base64 node ID.
pub fn add_node_id_field(mut obj: Object, resource: &ResolvedResource) -> Object {
    let type_name = resource.name.clone();
    let pk_cols: Vec<String> = resource.primary_key.clone();

    obj = obj.implement("Node");

    obj = obj.field(Field::new(
        "nodeId",
        TypeRef::named_nn(TypeRef::ID),
        move |ctx| {
            let type_name = type_name.clone();
            let pk_cols = pk_cols.clone();
            FieldFuture::new(async move {
                let row = match ctx.parent_value.try_downcast_ref::<PgRow>() {
                    Ok(r) => r,
                    Err(_) => {
                        return Ok(Some(FieldValue::value(Value::Null)));
                    }
                };

                let pairs: Vec<(String, String)> = pk_cols
                    .iter()
                    .map(|col| {
                        let val = row.get(col.as_str()).map(pg_value_as_str).unwrap_or_default();
                        (col.clone(), val)
                    })
                    .collect();

                let borrowed: Vec<(&str, &str)> = pairs
                    .iter()
                    .map(|(c, v): &(String, String)| (c.as_str(), v.as_str()))
                    .collect();

                let encoded = encode_node_id(&type_name, &borrowed);
                Ok(Some(FieldValue::value(Value::from(encoded))))
            })
        },
    ));

    obj
}

/// Extract a string representation from a `PgValue` for use in node IDs.
fn pg_value_as_str(val: &PgValue) -> String {
    match val {
        PgValue::Null => String::new(),
        PgValue::Bool(b) => b.to_string(),
        PgValue::Int(i) => i.to_string(),
        PgValue::Float(f) => f.to_string(),
        PgValue::Text(s) => s.clone(),
        PgValue::Uuid(u) => u.to_string(),
        PgValue::Timestamp(t) => t.to_rfc3339(),
        PgValue::Json(j) => j.to_string(),
        PgValue::Array(_) => String::new(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── encode/decode roundtrip ───────────────────────────────────────────────

    #[test]
    fn test_encode_decode_simple_pk() {
        let encoded = encode_node_id("User", &[("id", "42")]);
        let (type_name, pk) = decode_node_id(&encoded).expect("decode should succeed");
        assert_eq!(type_name, "User");
        assert_eq!(pk, "42");
    }

    #[test]
    fn test_encode_decode_composite_pk() {
        let encoded = encode_node_id("OrderItem", &[("order_id", "7"), ("item_id", "3")]);
        let (type_name, pk) = decode_node_id(&encoded).expect("decode should succeed");
        assert_eq!(type_name, "OrderItem");
        assert_eq!(pk, "order_id=7,item_id=3");
    }

    #[test]
    fn test_encode_decode_uuid_pk() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let encoded = encode_node_id("Post", &[("id", uuid_str)]);
        let (type_name, pk) = decode_node_id(&encoded).expect("decode should succeed");
        assert_eq!(type_name, "Post");
        assert_eq!(pk, uuid_str);
    }

    #[test]
    fn test_encode_decode_string_with_special_chars() {
        let encoded = encode_node_id("Item", &[("slug", "hello-world")]);
        let (type_name, pk) = decode_node_id(&encoded).expect("decode should succeed");
        assert_eq!(type_name, "Item");
        assert_eq!(pk, "hello-world");
    }

    /// Empty pk_values slice should encode/decode without panic.
    #[test]
    fn test_encode_decode_empty_pk() {
        let encoded = encode_node_id("Misc", &[]);
        let (type_name, pk) = decode_node_id(&encoded).expect("decode should succeed");
        assert_eq!(type_name, "Misc");
        assert_eq!(pk, "");
    }

    // ── error cases ───────────────────────────────────────────────────────────

    #[test]
    fn test_decode_invalid_base64() {
        let result = decode_node_id("!!!not_base64!!!");
        assert!(result.is_err(), "invalid base64 should return Err");
        let msg = result.unwrap_err();
        assert!(msg.contains("invalid base64"), "error should mention invalid base64: {}", msg);
    }

    #[test]
    fn test_decode_missing_colon() {
        // Valid base64 but no colon in the decoded string.
        let no_colon = BASE64.encode(b"TypeNameWithoutColon");
        let result = decode_node_id(&no_colon);
        assert!(result.is_err(), "missing colon should return Err");
        let msg = result.unwrap_err();
        assert!(msg.contains("':'"), "error should mention missing separator: {}", msg);
    }

    // ── schema registration ───────────────────────────────────────────────────

    #[test]
    fn test_register_node_interface_builds_schema() {
        use async_graphql::dynamic::Schema;

        let mut query = Object::new("Query").field(Field::new(
            "placeholder",
            TypeRef::named(TypeRef::STRING),
            |_| FieldFuture::from_value(Some(Value::Null)),
        ));

        let mut builder = Schema::build("Query", None, None);
        builder = register_node_interface(builder, &mut query, &[]);
        builder = builder.register(query);

        let schema = builder.finish();
        assert!(schema.is_ok(), "Node interface schema should build: {:?}", schema.err());
    }
}
