//! PgUpdateStep — builds UPDATE ... SET ... WHERE ... RETURNING SQL,
//! executes, and returns the updated rows.

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

/// A column-value pair for SET clauses.
#[derive(Debug, Clone)]
pub struct SetClause {
    pub column: String,
    pub value: PgValue,
}

/// A condition for WHERE clauses.
#[derive(Debug, Clone)]
pub struct WhereCondition {
    pub column: String,
    pub value: PgValue,
}

/// Executes an UPDATE query against a Postgres table with RETURNING support.
pub struct PgUpdateStep {
    id: StepId,
    pool: PgPool,
    schema: String,
    table: String,

    /// SET clauses: column = value pairs.
    set_clauses: Vec<SetClause>,

    /// WHERE conditions: column = value pairs (combined with AND).
    conditions: Vec<WhereCondition>,

    /// Columns to return via RETURNING clause.
    /// If empty, RETURNING * is used.
    returning: Vec<String>,

    deps: Vec<StepId>,
}

impl PgUpdateStep {
    /// Create a new PgUpdateStep.
    pub fn new(
        id: StepId,
        pool: PgPool,
        schema: impl Into<String>,
        table: impl Into<String>,
    ) -> Self {
        Self {
            id,
            pool,
            schema: schema.into(),
            table: table.into(),
            set_clauses: Vec::new(),
            conditions: Vec::new(),
            returning: Vec::new(),
            deps: Vec::new(),
        }
    }

    /// Add a SET clause.
    pub fn set(mut self, column: impl Into<String>, value: PgValue) -> Self {
        self.set_clauses.push(SetClause {
            column: column.into(),
            value,
        });
        self
    }

    /// Add a WHERE condition (equality check).
    pub fn where_eq(mut self, column: impl Into<String>, value: PgValue) -> Self {
        self.conditions.push(WhereCondition {
            column: column.into(),
            value,
        });
        self
    }

    /// Set the RETURNING columns. If not called, RETURNING * is used.
    pub fn returning(mut self, cols: Vec<String>) -> Self {
        self.returning = cols;
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
        // Hash SET column names (not values — those are runtime)
        for sc in &self.set_clauses {
            sc.column.hash(&mut h);
        }
        // Hash WHERE column names
        for wc in &self.conditions {
            wc.column.hash(&mut h);
        }
        self.returning.hash(&mut h);
        h.finish()
    }

    /// Build the UPDATE SQL and parameter list.
    fn build_sql(&self) -> (String, Vec<PgValue>) {
        let mut frag = SqlFragment::raw("UPDATE ")
            .append(SqlFragment::qualified_ident(&self.schema, &self.table))
            .push_raw(" SET ");

        // SET clauses
        for (i, sc) in self.set_clauses.iter().enumerate() {
            if i > 0 {
                frag = frag.push_raw(", ");
            }
            frag = frag
                .append(SqlFragment::ident(&sc.column))
                .push_raw(" = ")
                .append(SqlFragment::param(sc.value.clone()));
        }

        // WHERE
        if !self.conditions.is_empty() {
            frag = frag.push_raw(" WHERE ");
            for (i, wc) in self.conditions.iter().enumerate() {
                if i > 0 {
                    frag = frag.push_raw(" AND ");
                }
                frag = frag
                    .append(SqlFragment::ident(&wc.column))
                    .push_raw(" = ")
                    .append(SqlFragment::param(wc.value.clone()));
            }
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
impl ExecutableStep for PgUpdateStep {
    fn id(&self) -> StepId {
        self.id
    }

    fn dependencies(&self) -> &[StepId] {
        &self.deps
    }

    fn fingerprint(&self) -> StepFingerprint {
        StepFingerprint::new(
            TypeId::of::<PgUpdateStep>(),
            self.deps.clone(),
            self.compute_config_hash(),
        )
    }

    async fn execute(
        &self,
        _ctx: &ExecutionContext,
        _inputs: StepInputs,
    ) -> Result<StepOutput, FwGraphError> {
        if self.set_clauses.is_empty() {
            return Err(FwGraphError::ExecutionError(
                "PgUpdateStep: no SET clauses".into(),
            ));
        }

        let (sql, sql_params) = self.build_sql();

        debug!(
            step_id = self.id,
            sql = %sql,
            param_count = sql_params.len(),
            "PgUpdateStep executing query"
        );

        let rows = codec::execute_select(&self.pool, &sql, &sql_params)
            .await
            .map_err(|e| FwGraphError::ExecutionError(format!("PgUpdateStep: {}", e)))?;

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
    async fn update_sql_generation() {
        let step = PgUpdateStep::new(1, mock_pool(), "public", "users")
            .set("name", PgValue::Text("Bob".into()))
            .set("email", PgValue::Text("bob@example.com".into()))
            .where_eq("id", PgValue::Uuid(uuid::Uuid::nil()))
            .returning(vec!["id".into(), "name".into(), "email".into()]);

        let (sql, params) = step.build_sql();
        assert_eq!(
            sql,
            "UPDATE \"public\".\"users\" \
             SET \"name\" = $1, \"email\" = $2 \
             WHERE \"id\" = $3 \
             RETURNING \"id\", \"name\", \"email\""
        );
        assert_eq!(params.len(), 3);
        assert_eq!(params[0].as_text(), Some("Bob"));
        assert_eq!(params[1].as_text(), Some("bob@example.com"));
    }

    #[tokio::test]
    async fn update_returning_star_when_empty() {
        let step = PgUpdateStep::new(1, mock_pool(), "public", "users")
            .set("name", PgValue::Text("Alice".into()));

        let (sql, _) = step.build_sql();
        assert!(sql.ends_with("RETURNING *"));
    }

    #[tokio::test]
    async fn update_fingerprint_same_config() {
        let pool = mock_pool();
        let s1 = PgUpdateStep::new(1, pool.clone(), "public", "users")
            .set("name", PgValue::Text("Alice".into()))
            .where_eq("id", PgValue::Int(1));
        let s2 = PgUpdateStep::new(2, pool, "public", "users")
            .set("name", PgValue::Text("Bob".into()))
            .where_eq("id", PgValue::Int(2));
        // Same columns in SET and WHERE, so same fingerprint
        assert_eq!(s1.fingerprint(), s2.fingerprint());
    }
}
