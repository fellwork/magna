//! Relay cursor encoding/decoding for keyset pagination.
//!
//! Cursor format: base64(json({"pk":{"col":"val",...},"sort":{"col":"val",...}}))

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde_json::{Map, Value as JsVal};

use crate::executor::args::QueryError;

/// Encode a cursor from primary-key column→string-value pairs and optional sort pairs.
pub fn encode_cursor(pk: &[(&str, &str)], sort: &[(&str, &str)]) -> String {
    let mut obj = Map::new();
    let pk_map: Map<String, JsVal> = pk
        .iter()
        .map(|(k, v)| (k.to_string(), JsVal::String(v.to_string())))
        .collect();
    obj.insert("pk".to_string(), JsVal::Object(pk_map));
    if !sort.is_empty() {
        let sort_map: Map<String, JsVal> = sort
            .iter()
            .map(|(k, v)| (k.to_string(), JsVal::String(v.to_string())))
            .collect();
        obj.insert("sort".to_string(), JsVal::Object(sort_map));
    }
    BASE64.encode(serde_json::to_string(&JsVal::Object(obj)).unwrap_or_default())
}

/// Decode a cursor into its pk and sort maps.
pub fn decode_cursor(cursor: &str) -> Result<(Map<String, JsVal>, Map<String, JsVal>), QueryError> {
    let bytes = BASE64
        .decode(cursor)
        .map_err(|_| QueryError::InvalidCursor("invalid base64 in cursor".to_string()))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| QueryError::InvalidCursor("cursor is not valid UTF-8".to_string()))?;
    let val: JsVal = serde_json::from_str(&text)
        .map_err(|_| QueryError::InvalidCursor("cursor is not valid JSON".to_string()))?;
    let obj = val
        .as_object()
        .ok_or_else(|| QueryError::InvalidCursor("cursor JSON must be an object".to_string()))?;

    let pk = obj
        .get("pk")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let sort = obj
        .get("sort")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    Ok((pk, sort))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_roundtrip_simple_pk() {
        let cursor = encode_cursor(&[("id", "550e8400-e29b-41d4-a716-446655440000")], &[]);
        let (pk, sort) = decode_cursor(&cursor).expect("decode should succeed");
        assert_eq!(pk.get("id").and_then(|v| v.as_str()), Some("550e8400-e29b-41d4-a716-446655440000"));
        assert!(sort.is_empty());
    }

    #[test]
    fn test_cursor_roundtrip_with_sort() {
        let cursor = encode_cursor(&[("id", "42")], &[("created_at", "2026-01-01T00:00:00Z")]);
        let (pk, sort) = decode_cursor(&cursor).expect("decode should succeed");
        assert_eq!(pk.get("id").and_then(|v| v.as_str()), Some("42"));
        assert_eq!(sort.get("created_at").and_then(|v| v.as_str()), Some("2026-01-01T00:00:00Z"));
    }

    #[test]
    fn test_cursor_roundtrip_composite_pk() {
        let cursor = encode_cursor(&[("order_id", "10"), ("item_id", "20")], &[]);
        let (pk, _) = decode_cursor(&cursor).expect("decode should succeed");
        assert_eq!(pk.get("order_id").and_then(|v| v.as_str()), Some("10"));
        assert_eq!(pk.get("item_id").and_then(|v| v.as_str()), Some("20"));
    }

    #[test]
    fn test_cursor_invalid_base64() {
        let result = decode_cursor("!!!not_base64!!!");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), QueryError::InvalidCursor(_)));
    }

    #[test]
    fn test_cursor_invalid_json() {
        let not_json = BASE64.encode(b"not json at all");
        let result = decode_cursor(&not_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_cursor_missing_pk_returns_empty() {
        // A cursor with no "pk" key should return empty pk map (not error).
        let no_pk = BASE64.encode(br#"{"sort":{"col":"val"}}"#);
        let (pk, sort) = decode_cursor(&no_pk).expect("should decode");
        assert!(pk.is_empty());
        assert!(!sort.is_empty());
    }
}
