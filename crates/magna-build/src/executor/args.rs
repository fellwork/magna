//! ConnectionArgs, FilterInput, ConditionInput — parsed from async_graphql resolver contexts.

use async_graphql::Value;
use magna_types::PgValue;

// ── QueryError ────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Invalid cursor: {0}")]
    InvalidCursor(String),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Not found")]
    NotFound,
    #[error("RLS context error: {0}")]
    RlsError(String),
}

impl QueryError {
    /// Convert to an async_graphql::Error, sanitizing database errors so SQL
    /// details are never exposed to the client.
    pub fn into_gql(self) -> async_graphql::Error {
        match &self {
            QueryError::Database(_) => {
                tracing::error!(error = %self, "database error in resolver");
                async_graphql::Error::new("Internal database error")
            }
            QueryError::RlsError(_) => {
                tracing::error!(error = %self, "RLS error in resolver");
                async_graphql::Error::new("Internal error")
            }
            _ => async_graphql::Error::new(self.to_string()),
        }
    }
}

// ── FilterOp ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterOp {
    Eq, Ne, Lt, Lte, Gt, Gte, In, Like, Ilike, StartsWith, IsNull,
}

// ── ColumnFilter ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ColumnFilter {
    pub column: String,
    pub op: FilterOp,
    pub value: PgValue,
}

// ── FilterInput ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct FilterInput {
    pub column_ops: Vec<ColumnFilter>,
    pub and: Vec<FilterInput>,
    pub or: Vec<FilterInput>,
    pub not: Option<Box<FilterInput>>,
}

// ── ConditionInput ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ConditionInput {
    pub column_values: Vec<(String, PgValue)>,
}

// ── OrderDirection + OrderByInput ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection { Asc, Desc }

#[derive(Debug, Clone)]
pub struct OrderByInput {
    pub column: String,
    pub direction: OrderDirection,
}

// ── ConnectionArgs ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ConnectionArgs {
    pub first: Option<i64>,
    pub last: Option<i64>,
    pub after: Option<String>,
    pub before: Option<String>,
    pub filter: Option<FilterInput>,
    pub order_by: Vec<OrderByInput>,
    pub condition: Option<ConditionInput>,
    /// Set by resolver if totalCount field was requested via look_ahead.
    pub need_total_count: bool,
}

impl ConnectionArgs {
    /// Parse ConnectionArgs from a resolver context.
    /// Does not fail — unknown or missing args are silently skipped.
    pub fn from_ctx(ctx: &async_graphql::dynamic::ResolverContext<'_>) -> Self {
        let mut args = ConnectionArgs::default();

        if let Some(Value::Number(n)) = ctx.args.get("first").map(|a| a.as_value().clone()) {
            args.first = n.as_i64();
        }
        if let Some(Value::Number(n)) = ctx.args.get("last").map(|a| a.as_value().clone()) {
            args.last = n.as_i64();
        }
        if let Some(Value::String(s)) = ctx.args.get("after").map(|a| a.as_value().clone()) {
            args.after = Some(s.clone());
        }
        if let Some(Value::String(s)) = ctx.args.get("before").map(|a| a.as_value().clone()) {
            args.before = Some(s.clone());
        }
        // Check if totalCount was requested in the selection set.
        args.need_total_count = ctx.look_ahead().field("totalCount").exists();

        args
    }
}

// ── PgValue helper: gql Value → PgValue ──────────────────────────────────────

/// Convert an async_graphql `Value` to a `PgValue` for use in SQL parameters.
/// Returns `PgValue::Null` for unrecognized or Null values.
pub fn gql_value_to_pg_value(val: &Value) -> PgValue {
    match val {
        Value::Null => PgValue::Null,
        Value::Boolean(b) => PgValue::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                PgValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                PgValue::Float(f)
            } else {
                PgValue::Null
            }
        }
        Value::String(s) => {
            // Try UUID parse first, then fall back to text.
            if let Ok(u) = s.parse::<uuid::Uuid>() {
                PgValue::Uuid(u)
            } else {
                PgValue::Text(s.clone())
            }
        }
        Value::List(items) => {
            PgValue::Array(items.iter().map(gql_value_to_pg_value).collect())
        }
        _ => PgValue::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::Value;

    #[test]
    fn test_gql_value_null() {
        assert!(matches!(gql_value_to_pg_value(&Value::Null), PgValue::Null));
    }

    #[test]
    fn test_gql_value_bool() {
        assert!(matches!(gql_value_to_pg_value(&Value::Boolean(true)), PgValue::Bool(true)));
    }

    #[test]
    fn test_gql_value_int() {
        let v = Value::Number(async_graphql::Number::from(42i64));
        assert!(matches!(gql_value_to_pg_value(&v), PgValue::Int(42)));
    }

    #[test]
    fn test_gql_value_string_text() {
        let v = Value::String("hello".to_string());
        assert!(matches!(gql_value_to_pg_value(&v), PgValue::Text(_)));
    }

    #[test]
    fn test_gql_value_string_uuid() {
        let v = Value::String("550e8400-e29b-41d4-a716-446655440000".to_string());
        assert!(matches!(gql_value_to_pg_value(&v), PgValue::Uuid(_)));
    }

    #[test]
    fn test_gql_value_list() {
        let v = Value::List(vec![
            Value::Number(async_graphql::Number::from(1i64)),
            Value::Number(async_graphql::Number::from(2i64)),
        ]);
        let result = gql_value_to_pg_value(&v);
        assert!(matches!(result, PgValue::Array(_)));
        if let PgValue::Array(arr) = result {
            assert_eq!(arr.len(), 2);
        }
    }

    #[test]
    fn test_query_error_database_hides_details() {
        // Database errors must not leak SQL to client.
        let db_err = sqlx::Error::RowNotFound;
        let qe = QueryError::Database(db_err);
        let gql_err = qe.into_gql();
        assert_eq!(gql_err.message, "Internal database error");
    }
}
