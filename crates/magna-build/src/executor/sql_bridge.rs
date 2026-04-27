//! Bridge between magna-sql SqlFragment output and sqlx PgConnection execution.

use magna_types::{PgRow, PgValue};
use sqlx::{PgConnection, postgres::PgArguments, query::Query, Postgres};
use magna_dataplan::decode_row;

use crate::executor::args::QueryError;

// OID constants (same values as Postgres catalog)
pub mod oids {
    pub const UUID: u32 = 2950;
    pub const INT4: u32 = 23;
    pub const INT8: u32 = 20;
    pub const INT2: u32 = 21;
    pub const TEXT: u32 = 25;
}

/// Parse a string key (from DataLoader HashMap key) back to a typed PgValue
/// using the column's OID so it can be bound correctly to sqlx.
pub fn parse_pk_string(s: &str, type_oid: u32) -> PgValue {
    match type_oid {
        oids::UUID => match uuid::Uuid::parse_str(s) {
            Ok(u) => PgValue::Uuid(u),
            Err(_) => PgValue::Text(s.to_string()),
        },
        oids::INT4 | oids::INT8 | oids::INT2 => {
            PgValue::Int(s.parse::<i64>().unwrap_or(0))
        }
        _ => PgValue::Text(s.to_string()),
    }
}

/// Execute a SQL string + PgValue params on an existing &mut PgConnection.
/// Returns decoded PgRows. Uses the connection as-is (RLS already applied).
pub async fn execute_on_conn(
    conn: &mut PgConnection,
    sql: &str,
    params: &[PgValue],
) -> Result<Vec<PgRow>, QueryError> {
    let mut query = sqlx::query(sql);
    for p in params {
        query = bind_value(query, p);
    }
    let rows = query.fetch_all(conn).await.map_err(QueryError::Database)?;
    Ok(rows.iter().map(decode_row).collect())
}

/// Bind a single PgValue to a sqlx query.
pub fn bind_value<'q>(
    query: Query<'q, Postgres, PgArguments>,
    value: &'q PgValue,
) -> Query<'q, Postgres, PgArguments> {
    match value {
        PgValue::Null          => query.bind(None::<String>),
        PgValue::Bool(b)       => query.bind(b),
        PgValue::Int(n)        => query.bind(n),
        PgValue::Float(f)      => query.bind(f),
        PgValue::Text(s)       => query.bind(s.as_str()),
        PgValue::Uuid(u)       => query.bind(u),
        PgValue::Timestamp(dt) => query.bind(dt),
        PgValue::Json(j)       => query.bind(j),
        PgValue::Array(arr) => {
            if arr.is_empty() {
                return query.bind(Vec::<String>::new());
            }
            match &arr[0] {
                PgValue::Int(_) => {
                    let v: Vec<i64> = arr.iter().filter_map(|x| x.as_i64()).collect();
                    query.bind(v)
                }
                PgValue::Text(_) => {
                    let v: Vec<String> = arr.iter().filter_map(|x| x.as_text().map(|s| s.to_string())).collect();
                    query.bind(v)
                }
                PgValue::Uuid(_) => {
                    let v: Vec<uuid::Uuid> = arr.iter().filter_map(|x| match x { PgValue::Uuid(u) => Some(*u), _ => None }).collect();
                    query.bind(v)
                }
                PgValue::Bool(_) => {
                    let v: Vec<bool> = arr.iter().filter_map(|x| x.as_bool()).collect();
                    query.bind(v)
                }
                _ => {
                    let v: Vec<String> = arr.iter().map(|x| format!("{:?}", x)).collect();
                    query.bind(v)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pk_uuid() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let result = parse_pk_string(uuid_str, oids::UUID);
        assert!(matches!(result, PgValue::Uuid(_)));
    }

    #[test]
    fn test_parse_pk_int4() {
        let result = parse_pk_string("42", oids::INT4);
        assert!(matches!(result, PgValue::Int(42)));
    }

    #[test]
    fn test_parse_pk_int8() {
        let result = parse_pk_string("99", oids::INT8);
        assert!(matches!(result, PgValue::Int(99)));
    }

    #[test]
    fn test_parse_pk_text_fallback() {
        let result = parse_pk_string("my-slug", oids::TEXT);
        assert!(matches!(result, PgValue::Text(_)));
    }

    #[test]
    fn test_parse_pk_uuid_invalid_falls_back_to_text() {
        let result = parse_pk_string("not-a-uuid!!!", oids::UUID);
        assert!(matches!(result, PgValue::Text(_)));
    }

    #[test]
    fn test_parse_pk_int_invalid_returns_zero() {
        let result = parse_pk_string("not_a_number", oids::INT4);
        assert!(matches!(result, PgValue::Int(0)));
    }
}
