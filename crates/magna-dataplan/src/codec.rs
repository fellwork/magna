//! PgCodec — converts between sqlx `Row` types and `PgValue`/`PgRow`
//! from magna-types.
//!
//! Handles all common Postgres types: text, int2/int4/int8, float4/float8,
//! bool, uuid, timestamptz/timestamp, jsonb/json, arrays, and NULL.

use magna_types::{PgRow, PgTypeOid, PgValue};
use sqlx::postgres::PgRow as SqlxPgRow;
use sqlx::{Column, PgPool, Row, TypeInfo};
use tracing::warn;

// ---------------------------------------------------------------------------
// PgCodec — OID-based encode/decode
// ---------------------------------------------------------------------------

/// Maps a Postgres OID to encode/decode logic.
/// magna-dataplan registers built-in codecs.
/// magna-config plugins can register custom codecs.
pub struct PgCodec {
    pub oid: PgTypeOid,
    pub name: &'static str,
    pub decode: fn(raw: &str) -> PgValue,
    pub encode: fn(val: &PgValue) -> Option<String>,
    /// The GraphQL scalar name this maps to (e.g. "String", "Int", "UUID").
    pub gql_scalar: &'static str,
}

/// Built-in codec registry covering the most common Postgres types.
pub fn default_codecs() -> Vec<PgCodec> {
    vec![
        PgCodec {
            oid: 16,
            name: "bool",
            gql_scalar: "Boolean",
            decode: |s| PgValue::Bool(s == "t" || s == "true" || s == "1"),
            encode: |v| match v {
                PgValue::Bool(b) => Some(if *b { "t" } else { "f" }.to_string()),
                _ => None,
            },
        },
        PgCodec {
            oid: 21,
            name: "int2",
            gql_scalar: "Int",
            decode: |s| PgValue::Int(s.parse::<i64>().unwrap_or(0)),
            encode: |v| match v {
                PgValue::Int(n) => Some(n.to_string()),
                _ => None,
            },
        },
        PgCodec {
            oid: 23,
            name: "int4",
            gql_scalar: "Int",
            decode: |s| PgValue::Int(s.parse::<i64>().unwrap_or(0)),
            encode: |v| match v {
                PgValue::Int(n) => Some(n.to_string()),
                _ => None,
            },
        },
        PgCodec {
            oid: 20,
            name: "int8",
            gql_scalar: "BigInt",
            decode: |s| PgValue::Int(s.parse::<i64>().unwrap_or(0)),
            encode: |v| match v {
                PgValue::Int(n) => Some(n.to_string()),
                _ => None,
            },
        },
        PgCodec {
            oid: 700,
            name: "float4",
            gql_scalar: "Float",
            decode: |s| PgValue::Float(s.parse::<f64>().unwrap_or(0.0)),
            encode: |v| match v {
                PgValue::Float(f) => Some(f.to_string()),
                _ => None,
            },
        },
        PgCodec {
            oid: 701,
            name: "float8",
            gql_scalar: "Float",
            decode: |s| PgValue::Float(s.parse::<f64>().unwrap_or(0.0)),
            encode: |v| match v {
                PgValue::Float(f) => Some(f.to_string()),
                _ => None,
            },
        },
        PgCodec {
            oid: 25,
            name: "text",
            gql_scalar: "String",
            decode: |s| PgValue::Text(s.to_string()),
            encode: |v| match v {
                PgValue::Text(s) => Some(s.clone()),
                _ => None,
            },
        },
        PgCodec {
            oid: 1043,
            name: "varchar",
            gql_scalar: "String",
            decode: |s| PgValue::Text(s.to_string()),
            encode: |v| match v {
                PgValue::Text(s) => Some(s.clone()),
                _ => None,
            },
        },
        PgCodec {
            oid: 1114,
            name: "timestamp",
            gql_scalar: "Datetime",
            decode: |s| {
                // Try parsing as timestamp, fall back to text
                if let Ok(dt) = s.parse::<chrono::DateTime<chrono::Utc>>() {
                    PgValue::Timestamp(dt)
                } else {
                    PgValue::Text(s.to_string())
                }
            },
            encode: |v| match v {
                PgValue::Timestamp(dt) => Some(dt.to_rfc3339()),
                _ => None,
            },
        },
        PgCodec {
            oid: 1184,
            name: "timestamptz",
            gql_scalar: "Datetime",
            decode: |s| {
                if let Ok(dt) = s.parse::<chrono::DateTime<chrono::Utc>>() {
                    PgValue::Timestamp(dt)
                } else {
                    PgValue::Text(s.to_string())
                }
            },
            encode: |v| match v {
                PgValue::Timestamp(dt) => Some(dt.to_rfc3339()),
                _ => None,
            },
        },
        PgCodec {
            oid: 2950,
            name: "uuid",
            gql_scalar: "UUID",
            decode: |s| match uuid::Uuid::parse_str(s) {
                Ok(u) => PgValue::Uuid(u),
                Err(_) => PgValue::Text(s.to_string()),
            },
            encode: |v| match v {
                PgValue::Uuid(u) => Some(u.to_string()),
                _ => None,
            },
        },
        PgCodec {
            oid: 114,
            name: "json",
            gql_scalar: "JSON",
            decode: |s| match serde_json::from_str(s) {
                Ok(v) => PgValue::Json(v),
                Err(_) => PgValue::Text(s.to_string()),
            },
            encode: |v| match v {
                PgValue::Json(j) => Some(j.to_string()),
                _ => None,
            },
        },
        PgCodec {
            oid: 3802,
            name: "jsonb",
            gql_scalar: "JSON",
            decode: |s| match serde_json::from_str(s) {
                Ok(v) => PgValue::Json(v),
                Err(_) => PgValue::Text(s.to_string()),
            },
            encode: |v| match v {
                PgValue::Json(j) => Some(j.to_string()),
                _ => None,
            },
        },
    ]
}

// ---------------------------------------------------------------------------
// sqlx Row -> PgRow conversion
// ---------------------------------------------------------------------------

/// Convert a sqlx `PgRow` to our `PgRow` (IndexMap<String, PgValue>).
///
/// Uses column type names to determine the appropriate conversion.
/// NULL values are correctly represented as `PgValue::Null`.
pub fn decode_row(row: &SqlxPgRow) -> PgRow {
    let mut result = PgRow::new();
    for col in row.columns() {
        let name = col.name().to_string();
        let type_name = col.type_info().name();
        let value = decode_column(row, col.ordinal(), type_name);
        result.insert(name, value);
    }
    result
}

/// Decode a single column value from a sqlx row.
fn decode_column(row: &SqlxPgRow, ordinal: usize, type_name: &str) -> PgValue {
    // sqlx returns Option<T> for nullable columns. We try the expected type
    // and fall back to text representation.
    match type_name {
        "BOOL" => match row.try_get::<Option<bool>, _>(ordinal) {
            Ok(Some(v)) => PgValue::Bool(v),
            Ok(None) => PgValue::Null,
            Err(_) => PgValue::Null,
        },
        "INT2" => match row.try_get::<Option<i16>, _>(ordinal) {
            Ok(Some(v)) => PgValue::Int(v as i64),
            Ok(None) => PgValue::Null,
            Err(_) => PgValue::Null,
        },
        "INT4" => match row.try_get::<Option<i32>, _>(ordinal) {
            Ok(Some(v)) => PgValue::Int(v as i64),
            Ok(None) => PgValue::Null,
            Err(_) => PgValue::Null,
        },
        "INT8" => match row.try_get::<Option<i64>, _>(ordinal) {
            Ok(Some(v)) => PgValue::Int(v),
            Ok(None) => PgValue::Null,
            Err(_) => PgValue::Null,
        },
        "FLOAT4" => match row.try_get::<Option<f32>, _>(ordinal) {
            Ok(Some(v)) => PgValue::Float(v as f64),
            Ok(None) => PgValue::Null,
            Err(_) => PgValue::Null,
        },
        "FLOAT8" => match row.try_get::<Option<f64>, _>(ordinal) {
            Ok(Some(v)) => PgValue::Float(v),
            Ok(None) => PgValue::Null,
            Err(_) => PgValue::Null,
        },
        "UUID" => match row.try_get::<Option<uuid::Uuid>, _>(ordinal) {
            Ok(Some(v)) => PgValue::Uuid(v),
            Ok(None) => PgValue::Null,
            Err(_) => PgValue::Null,
        },
        "TIMESTAMPTZ" => {
            match row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(ordinal) {
                Ok(Some(v)) => PgValue::Timestamp(v),
                Ok(None) => PgValue::Null,
                Err(_) => PgValue::Null,
            }
        }
        "TIMESTAMP" => {
            match row.try_get::<Option<chrono::NaiveDateTime>, _>(ordinal) {
                Ok(Some(v)) => PgValue::Timestamp(v.and_utc()),
                Ok(None) => PgValue::Null,
                Err(_) => PgValue::Null,
            }
        }
        "JSON" | "JSONB" => match row.try_get::<Option<serde_json::Value>, _>(ordinal) {
            Ok(Some(v)) => PgValue::Json(v),
            Ok(None) => PgValue::Null,
            Err(_) => PgValue::Null,
        },
        "BOOL[]" => match row.try_get::<Option<Vec<bool>>, _>(ordinal) {
            Ok(Some(v)) => PgValue::Array(v.into_iter().map(PgValue::Bool).collect()),
            Ok(None) => PgValue::Null,
            Err(_) => PgValue::Null,
        },
        "INT4[]" => match row.try_get::<Option<Vec<i32>>, _>(ordinal) {
            Ok(Some(v)) => PgValue::Array(v.into_iter().map(|i| PgValue::Int(i as i64)).collect()),
            Ok(None) => PgValue::Null,
            Err(_) => PgValue::Null,
        },
        "INT8[]" => match row.try_get::<Option<Vec<i64>>, _>(ordinal) {
            Ok(Some(v)) => PgValue::Array(v.into_iter().map(PgValue::Int).collect()),
            Ok(None) => PgValue::Null,
            Err(_) => PgValue::Null,
        },
        "TEXT[]" | "VARCHAR[]" => match row.try_get::<Option<Vec<String>>, _>(ordinal) {
            Ok(Some(v)) => PgValue::Array(v.into_iter().map(PgValue::Text).collect()),
            Ok(None) => PgValue::Null,
            Err(_) => PgValue::Null,
        },
        "UUID[]" => match row.try_get::<Option<Vec<uuid::Uuid>>, _>(ordinal) {
            Ok(Some(v)) => PgValue::Array(v.into_iter().map(PgValue::Uuid).collect()),
            Ok(None) => PgValue::Null,
            Err(_) => PgValue::Null,
        },
        // Default: try to get as text
        _ => {
            warn!(type_name, ordinal, "unknown column type, falling back to text");
            match row.try_get::<Option<String>, _>(ordinal) {
                Ok(Some(v)) => PgValue::Text(v),
                Ok(None) => PgValue::Null,
                Err(_) => PgValue::Null,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Query execution helper
// ---------------------------------------------------------------------------

/// Execute a SQL query with PgValue parameters and return decoded rows.
///
/// This is the bridge between magna-sql's `(String, Vec<PgValue>)` output
/// and sqlx's typed query interface. Parameters are bound dynamically based
/// on their PgValue variant.
pub async fn execute_select(
    pool: &PgPool,
    sql: &str,
    params: &[PgValue],
) -> Result<Vec<PgRow>, sqlx::Error> {
    let mut query = sqlx::query(sql);

    for p in params {
        query = bind_pg_value(query, p);
    }

    let rows = query.fetch_all(pool).await?;
    Ok(rows.iter().map(decode_row).collect())
}

/// Bind a PgValue to a sqlx query.
fn bind_pg_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    value: &'q PgValue,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    match value {
        PgValue::Null => query.bind(None::<String>),
        PgValue::Bool(b) => query.bind(b),
        PgValue::Int(n) => query.bind(n),
        PgValue::Float(f) => query.bind(f),
        PgValue::Text(s) => query.bind(s.as_str()),
        PgValue::Uuid(u) => query.bind(u),
        PgValue::Timestamp(dt) => query.bind(dt),
        PgValue::Json(j) => query.bind(j),
        PgValue::Array(arr) => {
            // For arrays, we need to determine the element type.
            // Try common cases: if all elements are the same type, bind as typed array.
            if arr.is_empty() {
                // Empty array — bind as text array (Postgres will coerce)
                query.bind(Vec::<String>::new())
            } else {
                match &arr[0] {
                    PgValue::Int(_) => {
                        let ints: Vec<i64> = arr
                            .iter()
                            .filter_map(|v| v.as_i64())
                            .collect();
                        query.bind(ints)
                    }
                    PgValue::Text(_) => {
                        let texts: Vec<String> = arr
                            .iter()
                            .filter_map(|v| v.as_text().map(|s| s.to_string()))
                            .collect();
                        query.bind(texts)
                    }
                    PgValue::Uuid(_) => {
                        let uuids: Vec<uuid::Uuid> = arr
                            .iter()
                            .filter_map(|v| match v {
                                PgValue::Uuid(u) => Some(*u),
                                _ => None,
                            })
                            .collect();
                        query.bind(uuids)
                    }
                    PgValue::Bool(_) => {
                        let bools: Vec<bool> = arr
                            .iter()
                            .filter_map(|v| v.as_bool())
                            .collect();
                        query.bind(bools)
                    }
                    _ => {
                        // Fallback: convert everything to strings
                        let texts: Vec<String> = arr
                            .iter()
                            .map(|v| match v {
                                PgValue::Text(s) => s.clone(),
                                other => format!("{:?}", other),
                            })
                            .collect();
                        query.bind(texts)
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_codecs_cover_common_types() {
        let codecs = default_codecs();

        let names: Vec<&str> = codecs.iter().map(|c| c.name).collect();
        assert!(names.contains(&"bool"));
        assert!(names.contains(&"int4"));
        assert!(names.contains(&"int8"));
        assert!(names.contains(&"float4"));
        assert!(names.contains(&"float8"));
        assert!(names.contains(&"text"));
        assert!(names.contains(&"uuid"));
        assert!(names.contains(&"timestamp"));
        assert!(names.contains(&"timestamptz"));
        assert!(names.contains(&"jsonb"));
        assert!(names.contains(&"json"));
        assert!(names.contains(&"varchar"));
        assert!(names.contains(&"int2"));

        // Each codec has a unique OID
        let oids: Vec<PgTypeOid> = codecs.iter().map(|c| c.oid).collect();
        let unique_oids: std::collections::HashSet<_> = oids.iter().collect();
        assert_eq!(oids.len(), unique_oids.len(), "OIDs must be unique");
    }

    #[test]
    fn bool_codec_roundtrip() {
        let codecs = default_codecs();
        let bool_codec = codecs.iter().find(|c| c.name == "bool").unwrap();

        let decoded = (bool_codec.decode)("t");
        assert_eq!(decoded.as_bool(), Some(true));

        let decoded = (bool_codec.decode)("f");
        assert_eq!(decoded.as_bool(), Some(false));

        let encoded = (bool_codec.encode)(&PgValue::Bool(true));
        assert_eq!(encoded, Some("t".to_string()));

        let encoded = (bool_codec.encode)(&PgValue::Bool(false));
        assert_eq!(encoded, Some("f".to_string()));
    }

    #[test]
    fn int_codec_roundtrip() {
        let codecs = default_codecs();
        let int_codec = codecs.iter().find(|c| c.name == "int4").unwrap();

        let decoded = (int_codec.decode)("42");
        assert_eq!(decoded.as_i64(), Some(42));

        let decoded = (int_codec.decode)("-7");
        assert_eq!(decoded.as_i64(), Some(-7));

        let encoded = (int_codec.encode)(&PgValue::Int(42));
        assert_eq!(encoded, Some("42".to_string()));

        // Wrong type returns None
        let encoded = (int_codec.encode)(&PgValue::Text("not a number".into()));
        assert_eq!(encoded, None);
    }

    #[test]
    fn text_codec_roundtrip() {
        let codecs = default_codecs();
        let text_codec = codecs.iter().find(|c| c.name == "text").unwrap();

        let decoded = (text_codec.decode)("hello world");
        assert_eq!(decoded.as_text(), Some("hello world"));

        let encoded = (text_codec.encode)(&PgValue::Text("hello".into()));
        assert_eq!(encoded, Some("hello".to_string()));
    }

    #[test]
    fn uuid_codec_roundtrip() {
        let codecs = default_codecs();
        let uuid_codec = codecs.iter().find(|c| c.name == "uuid").unwrap();

        let u = uuid::Uuid::new_v4();
        let decoded = (uuid_codec.decode)(&u.to_string());
        match decoded {
            PgValue::Uuid(parsed) => assert_eq!(parsed, u),
            other => panic!("expected Uuid, got {:?}", other),
        }

        let encoded = (uuid_codec.encode)(&PgValue::Uuid(u));
        assert_eq!(encoded, Some(u.to_string()));
    }

    #[test]
    fn json_codec_roundtrip() {
        let codecs = default_codecs();
        let json_codec = codecs.iter().find(|c| c.name == "jsonb").unwrap();

        let decoded = (json_codec.decode)(r#"{"key": "value"}"#);
        match &decoded {
            PgValue::Json(v) => assert_eq!(v["key"], "value"),
            other => panic!("expected Json, got {:?}", other),
        }

        let encoded = (json_codec.encode)(&decoded);
        assert!(encoded.is_some());
        let re_decoded: serde_json::Value =
            serde_json::from_str(&encoded.unwrap()).unwrap();
        assert_eq!(re_decoded["key"], "value");
    }

    #[test]
    fn float_codec_roundtrip() {
        let codecs = default_codecs();
        let float_codec = codecs.iter().find(|c| c.name == "float8").unwrap();

        let decoded = (float_codec.decode)("3.14");
        match decoded {
            PgValue::Float(f) => assert!((f - 3.14).abs() < f64::EPSILON),
            other => panic!("expected Float, got {:?}", other),
        }

        let encoded = (float_codec.encode)(&PgValue::Float(3.14));
        assert!(encoded.is_some());
    }

    #[test]
    fn codec_gql_scalars_are_set() {
        let codecs = default_codecs();
        for codec in &codecs {
            assert!(
                !codec.gql_scalar.is_empty(),
                "codec {} should have a gql_scalar",
                codec.name
            );
        }
    }

    #[test]
    fn encode_wrong_type_returns_none() {
        let codecs = default_codecs();
        let bool_codec = codecs.iter().find(|c| c.name == "bool").unwrap();

        // Encoding a non-bool value with the bool codec should return None
        assert_eq!((bool_codec.encode)(&PgValue::Int(42)), None);
        assert_eq!((bool_codec.encode)(&PgValue::Text("true".into())), None);
        assert_eq!((bool_codec.encode)(&PgValue::Null), None);
    }

    #[test]
    fn decode_invalid_int_returns_zero() {
        let codecs = default_codecs();
        let int_codec = codecs.iter().find(|c| c.name == "int4").unwrap();
        let decoded = (int_codec.decode)("not_a_number");
        assert_eq!(decoded.as_i64(), Some(0));
    }
}
