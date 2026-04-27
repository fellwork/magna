//! PgInsertStep — builds INSERT ... RETURNING SQL, executes, and returns
//! the inserted rows.

use magna_core::{ExecutableStep, ExecutionContext, StepFingerprint, StepInputs, StepOutput};
use magna_sql::SqlFragment;
use magna_types::{FwGraphError, PgValue, StepId};
use sqlx::PgPool;
use std::any::TypeId;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tracing::debug;

use crate::codec;

/// Executes an INSERT query against a Postgres table with RETURNING support.
///
/// At plan-time, the caller specifies the target table, columns, and which
/// columns to return. At execute-time, the step receives the values to insert
/// from its dependency inputs.
pub struct PgInsertStep {
    id: StepId,
    pool: PgPool,
    schema: String,
    table: String,

    /// Column names to insert into.
    columns: Vec<String>,

    /// Columns to return via RETURNING clause.
    /// If empty, RETURNING * is used.
    returning: Vec<String>,

    /// Static values to insert (for root-level inserts not driven by deps).
    /// Each inner Vec is one row of values, positionally matching `columns`.
    static_rows: Vec<Vec<PgValue>>,

    deps: Vec<StepId>,
}

impl PgInsertStep {
    /// Create a new PgInsertStep.
    pub fn new(
        id: StepId,
        pool: PgPool,
        schema: impl Into<String>,
        table: impl Into<String>,
        columns: Vec<String>,
    ) -> Self {
        Self {
            id,
            pool,
            schema: schema.into(),
            table: table.into(),
            columns,
            returning: Vec::new(),
            static_rows: Vec::new(),
            deps: Vec::new(),
        }
    }

    /// Set the RETURNING columns. If not called, RETURNING * is used.
    pub fn returning(mut self, cols: Vec<String>) -> Self {
        self.returning = cols;
        self
    }

    /// Add a static row of values to insert.
    pub fn add_row(mut self, values: Vec<PgValue>) -> Self {
        debug_assert_eq!(
            values.len(),
            self.columns.len(),
            "row values must match column count"
        );
        self.static_rows.push(values);
        self
    }

    /// Add an explicit dependency.
    pub fn add_dep(&mut self, dep: StepId) {
        if !self.deps.contains(&dep) {
            self.deps.push(dep);
        }
    }

    fn compute_config_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.schema.hash(&mut h);
        self.table.hash(&mut h);
        self.columns.hash(&mut h);
        self.returning.hash(&mut h);
        h.finish()
    }

    /// Build the INSERT SQL and parameter list.
    fn build_sql(&self, rows: &[Vec<PgValue>]) -> (String, Vec<PgValue>) {
        let mut frag = SqlFragment::raw("INSERT INTO ")
            .append(SqlFragment::qualified_ident(&self.schema, &self.table))
            .push_raw(" (");

        // Column list
        for (i, col) in self.columns.iter().enumerate() {
            if i > 0 {
                frag = frag.push_raw(", ");
            }
            frag = frag.append(SqlFragment::ident(col));
        }
        frag = frag.push_raw(") VALUES ");

        // Value rows
        for (row_idx, row) in rows.iter().enumerate() {
            if row_idx > 0 {
                frag = frag.push_raw(", ");
            }
            frag = frag.push_raw("(");
            for (col_idx, val) in row.iter().enumerate() {
                if col_idx > 0 {
                    frag = frag.push_raw(", ");
                }
                frag = frag.append(SqlFragment::param(val.clone()));
            }
            frag = frag.push_raw(")");
        }

        // RETURNING
        frag = frag.push_raw(" RETURNING ");
        if self.returning.is_empty() {
            frag = frag.push_raw("*");
        } else {
            for (i, col) in self.returning.iter().enumerate() {
                if i > 0 {
                    frag = frag.push_raw(", ");
                }
                frag = frag.append(SqlFragment::ident(col));
            }
        }

        frag.build()
    }
}

#[async_trait::async_trait]
impl ExecutableStep for PgInsertStep {
    fn id(&self) -> StepId {
        self.id
    }

    fn dependencies(&self) -> &[StepId] {
        &self.deps
    }

    fn fingerprint(&self) -> StepFingerprint {
        StepFingerprint::new(
            TypeId::of::<PgInsertStep>(),
            self.deps.clone(),
            self.compute_config_hash(),
        )
    }

    async fn execute(
        &self,
        _ctx: &ExecutionContext,
        _inputs: StepInputs,
    ) -> Result<StepOutput, FwGraphError> {
        if self.static_rows.is_empty() {
            return Err(FwGraphError::ExecutionError(
                "PgInsertStep: no rows to insert".into(),
            ));
        }

        let (sql, sql_params) = self.build_sql(&self.static_rows);

        debug!(
            step_id = self.id,
            sql = %sql,
            param_count = sql_params.len(),
            "PgInsertStep executing query"
        );

        let rows = codec::execute_select(&self.pool, &sql, &sql_params)
            .await
            .map_err(|e| FwGraphError::ExecutionError(format!("PgInsertStep: {}", e)))?;

        // One output per inserted row
        let values: Vec<Arc<dyn std::any::Any + Send + Sync>> = rows
            .into_iter()
            .map(|row| Arc::new(row) as Arc<dyn std::any::Any + Send + Sync>)
            .collect();

        Ok(StepOutput::from_values(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_pool() -> PgPool {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost/test_does_not_exist")
            .expect("connect_lazy should not fail")
    }

    #[tokio::test]
    async fn insert_sql_generation_single_row() {
        let step = PgInsertStep::new(
            1,
            mock_pool(),
            "public",
            "users",
            vec!["name".into(), "email".into()],
        )
        .returning(vec!["id".into(), "name".into(), "email".into()])
        .add_row(vec![
            PgValue::Text("Alice".into()),
            PgValue::Text("alice@example.com".into()),
        ]);

        let (sql, params) = step.build_sql(&step.static_rows);
        assert_eq!(
            sql,
            "INSERT INTO \"public\".\"users\" (\"name\", \"email\") \
             VALUES ($1, $2) \
             RETURNING \"id\", \"name\", \"email\""
        );
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].as_text(), Some("Alice"));
        assert_eq!(params[1].as_text(), Some("alice@example.com"));
    }

    #[tokio::test]
    async fn insert_sql_generation_multiple_rows() {
        let step = PgInsertStep::new(
            1,
            mock_pool(),
            "public",
            "tags",
            vec!["name".into()],
        )
        .add_row(vec![PgValue::Text("rust".into())])
        .add_row(vec![PgValue::Text("postgres".into())]);

        let (sql, params) = step.build_sql(&step.static_rows);
        assert!(sql.contains("VALUES ($1), ($2)"));
        assert!(sql.contains("RETURNING *"));
        assert_eq!(params.len(), 2);
    }

    #[tokio::test]
    async fn insert_returning_star_when_empty() {
        let step = PgInsertStep::new(
            1,
            mock_pool(),
            "public",
            "users",
            vec!["name".into()],
        )
        .add_row(vec![PgValue::Text("Bob".into())]);

        let (sql, _) = step.build_sql(&step.static_rows);
        assert!(sql.ends_with("RETURNING *"));
    }

    #[tokio::test]
    async fn insert_fingerprint_same_config() {
        let pool = mock_pool();
        let s1 = PgInsertStep::new(
            1,
            pool.clone(),
            "public",
            "users",
            vec!["name".into()],
        )
        .returning(vec!["id".into()]);
        let s2 = PgInsertStep::new(
            2,
            pool,
            "public",
            "users",
            vec!["name".into()],
        )
        .returning(vec!["id".into()]);
        assert_eq!(s1.fingerprint(), s2.fingerprint());
    }
}
