//! Mutation publisher — converts primary-key values to NOTIFY payloads
//! and fires `pg_notify` after a mutation completes.

use sqlx::postgres::PgConnection;
use sqlx::Executor;
use magna_types::PgValue;
use crate::error::SubscriptionError;

const MAX_PAYLOAD_BYTES: usize = 7900;

/// A NOTIFY payload ready to be dispatched to subscribers.
#[derive(Debug, Clone)]
pub struct NotifyPayload {
    pub channel: String,
    pub pk_text: String,
}

/// Convert a single [`PgValue`] primary key to its text representation.
///
/// - [`PgValue::Uuid`]  → UUID string
/// - [`PgValue::Int`]   → decimal integer string
/// - [`PgValue::Text`]  → cloned as-is
/// - [`PgValue::Null`]  → `SubscriptionError::Serialization`
///
/// Returns [`SubscriptionError::PayloadTooLarge`] if the result exceeds 7900 bytes.
pub fn pk_to_text(pk: &PgValue) -> Result<String, SubscriptionError> {
    let text = match pk {
        PgValue::Uuid(u) => u.to_string(),
        PgValue::Int(n) => n.to_string(),
        PgValue::Text(s) => s.clone(),
        PgValue::Null => {
            return Err(SubscriptionError::Serialization(
                "NULL is not a valid primary key".to_string(),
            ))
        }
        other => {
            return Err(SubscriptionError::Serialization(format!(
                "Unsupported PgValue variant for primary key: {:?}",
                other
            )))
        }
    };

    let size = text.len();
    if size >= MAX_PAYLOAD_BYTES {
        return Err(SubscriptionError::PayloadTooLarge { size });
    }

    Ok(text)
}

/// Convert a composite primary key (multiple columns) to a JSON text payload.
///
/// Builds a JSON object: `{"col1":"val1","col2":"val2"}`.
/// Returns [`SubscriptionError::PayloadTooLarge`] if the result exceeds 7900 bytes.
pub fn composite_pk_to_text(columns: &[(&str, &PgValue)]) -> Result<String, SubscriptionError> {
    let mut map = serde_json::Map::new();

    for (col, val) in columns {
        let text_val = pk_to_text(val)?;
        map.insert(col.to_string(), serde_json::Value::String(text_val));
    }

    let json = serde_json::to_string(&map)
        .map_err(|e| SubscriptionError::Serialization(e.to_string()))?;

    let size = json.len();
    if size >= MAX_PAYLOAD_BYTES {
        return Err(SubscriptionError::PayloadTooLarge { size });
    }

    Ok(json)
}

/// Fire `pg_notify(channel, pk_text)` over an existing connection.
///
/// Validates that `pk_text` is within the 7900-byte limit before sending.
pub async fn notify_mutation(
    conn: &mut PgConnection,
    channel: &str,
    pk_text: &str,
) -> Result<(), SubscriptionError> {
    let size = pk_text.len();
    if size >= MAX_PAYLOAD_BYTES {
        return Err(SubscriptionError::PayloadTooLarge { size });
    }

    conn.execute(sqlx::query("SELECT pg_notify($1, $2)")
        .bind(channel)
        .bind(pk_text))
        .await?;

    Ok(())
}

/// Build the conventional LISTEN channel name for a table's mutation events.
///
/// Format: `"{schema}_{table}_mutation"`
pub fn mutation_channel(schema: &str, table: &str) -> String {
    format!("{schema}_{table}_mutation")
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // Test 1: pk_to_text with UUID
    #[test]
    fn pk_to_text_uuid() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let result = pk_to_text(&PgValue::Uuid(id)).unwrap();
        assert_eq!(result, "550e8400-e29b-41d4-a716-446655440000");
    }

    // Test 2: pk_to_text with Int
    #[test]
    fn pk_to_text_int() {
        let result = pk_to_text(&PgValue::Int(42)).unwrap();
        assert_eq!(result, "42");
    }

    // Test 3: pk_to_text with String
    #[test]
    fn pk_to_text_string() {
        let result = pk_to_text(&PgValue::Text("hello".to_string())).unwrap();
        assert_eq!(result, "hello");
    }

    // Test 4: pk_to_text with Null → error
    #[test]
    fn pk_to_text_null_is_error() {
        let result = pk_to_text(&PgValue::Null);
        assert!(result.is_err());
        match result.unwrap_err() {
            SubscriptionError::Serialization(msg) => {
                assert!(msg.contains("NULL"));
            }
            other => panic!("Expected Serialization error, got {:?}", other),
        }
    }

    // Test 5: pk_to_text too large → PayloadTooLarge
    #[test]
    fn pk_to_text_too_large() {
        let huge = "x".repeat(8000);
        let result = pk_to_text(&PgValue::Text(huge));
        assert!(result.is_err());
        match result.unwrap_err() {
            SubscriptionError::PayloadTooLarge { size } => {
                assert!(size >= MAX_PAYLOAD_BYTES);
            }
            other => panic!("Expected PayloadTooLarge, got {:?}", other),
        }
    }

    // Test 6: composite_pk_to_text with two columns → valid JSON
    #[test]
    fn composite_pk_two_columns() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let cols: &[(&str, &PgValue)] = &[
            ("user_id", &PgValue::Uuid(id)),
            ("concept_id", &PgValue::Int(7)),
        ];
        let result = composite_pk_to_text(cols).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["user_id"], "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(parsed["concept_id"], "7");
    }

    // Test 7: mutation_channel formatting
    #[test]
    fn mutation_channel_format() {
        let ch = mutation_channel("public", "concepts");
        assert_eq!(ch, "public_concepts_mutation");
    }
}
