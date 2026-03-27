//! Postgres value types — normalized representation for cross-layer use.
//!
//! fw-graph-dataplan produces these. fw-graph-build consumes them.
//! Neither layer imports the other's types.

/// A Postgres value as received from sqlx, normalized for cross-layer use.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum PgValue {
  Null,
  Bool(bool),
  Int(i64),
  Float(f64),
  Text(String),
  Uuid(uuid::Uuid),
  Timestamp(chrono::DateTime<chrono::Utc>),
  Json(serde_json::Value),
  Array(Vec<PgValue>),
}

impl PgValue {
  pub fn is_null(&self) -> bool {
    matches!(self, PgValue::Null)
  }

  pub fn as_text(&self) -> Option<&str> {
    match self {
      PgValue::Text(s) => Some(s),
      _ => None,
    }
  }

  pub fn as_i64(&self) -> Option<i64> {
    match self {
      PgValue::Int(n) => Some(*n),
      _ => None,
    }
  }

  pub fn as_bool(&self) -> Option<bool> {
    match self {
      PgValue::Bool(b) => Some(*b),
      _ => None,
    }
  }
}

/// A single row from Postgres, represented as an ordered map.
/// Uses IndexMap to preserve column order from the SELECT clause.
pub type PgRow = indexmap::IndexMap<String, PgValue>;

/// Postgres OID for type identification during introspection.
pub type PgTypeOid = u32;

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn pg_value_accessors() {
    let text = PgValue::Text("hello".to_string());
    assert_eq!(text.as_text(), Some("hello"));
    assert_eq!(text.as_i64(), None);

    let num = PgValue::Int(42);
    assert_eq!(num.as_i64(), Some(42));
    assert_eq!(num.as_text(), None);

    assert!(PgValue::Null.is_null());
    assert!(!PgValue::Bool(true).is_null());
  }

  #[test]
  fn pg_value_serde_roundtrip() {
    let val = PgValue::Text("test".to_string());
    let json = serde_json::to_string(&val).unwrap();
    let back: PgValue = serde_json::from_str(&json).unwrap();
    assert_eq!(back.as_text(), Some("test"));
  }
}
