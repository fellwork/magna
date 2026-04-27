//! PgSelectStep — the Grafast equivalent of a DataLoader.
//!
//! Given a table name, conditions, and selected columns, builds SQL via
//! magna-sql, executes via sqlx, and returns `Vec<PgRow>`. Supports
//! `WHERE fk_col = ANY($1)` batching for list lookups.

use magna_core::{ExecutableStep, ExecutionContext, StepFingerprint, StepInputs, StepOutput};
use magna_sql::{SqlBuilder, SqlFragment};
use magna_types::{FwGraphError, PgRow, PgValue, StepFlags, StepId};
use sqlx::PgPool;
use std::any::TypeId;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tracing::debug;

use crate::codec;

/// A tweak is a closure that transforms an `SqlBuilder` at execute-time.
/// Tweaks are registered at plan-time by field resolvers to add conditions,
/// ordering, and pagination that depend on argument values.
///
/// Because `SqlBuilder` uses a consuming builder pattern (methods take `self`),
/// tweaks transform the builder rather than mutating it.
pub type SqlTweak = Box<dyn Fn(SqlBuilder) -> SqlBuilder + Send + Sync>;

/// Executes a SELECT query against a Postgres table.
///
/// This is the most important step in the data-plan layer. It serves as the
/// Grafast equivalent of a DataLoader: a single SQL query services an entire
/// batch of parent lookups.
///
/// # Batching
///
/// When `parent_step_id` and `fk_col` are set, the step extracts parent IDs
/// from the upstream dependency output and generates:
///
/// ```sql
/// SELECT ... FROM "schema"."table" WHERE "fk_col" = ANY($1)
/// ```
///
/// The result rows are then partitioned by the FK column value so each
/// parent receives its matching children.
pub struct PgSelectStep {
    id: StepId,
    pool: PgPool,

    /// The schema and table to query — established at plan-time.
    schema: String,
    table: String,

    /// Columns to select — established at plan-time.
    /// If empty, SELECT * is used.
    selected_cols: Vec<String>,

    /// The step that produces the parent IDs for this batch.
    /// If None, this is a root query (no parent context).
    parent_step_id: Option<StepId>,

    /// The column on this table that matches the parent's PK.
    /// Used to generate: WHERE "fk_col" = ANY($1)
    fk_col: Option<String>,

    /// The name of the PK column on the parent table whose values are
    /// extracted from the parent output rows.  Defaults to `"id"`.
    parent_pk_col: String,

    /// Runtime tweaks — registered at plan-time, applied at execute-time.
    /// These are closures that transform the SqlBuilder just before build().
    tweaks: Vec<SqlTweak>,

    deps: Vec<StepId>,
}

impl PgSelectStep {
    /// Create a new PgSelectStep for the given table.
    pub fn new(
        id: StepId,
        pool: PgPool,
        schema: impl Into<String>,
        table: impl Into<String>,
        selected_cols: Vec<String>,
    ) -> Self {
        Self {
            id,
            pool,
            schema: schema.into(),
            table: table.into(),
            selected_cols,
            parent_step_id: None,
            fk_col: None,
            parent_pk_col: "id".to_string(),
            tweaks: Vec::new(),
            deps: Vec::new(),
        }
    }

    /// Set the parent step for batched FK lookups.
    ///
    /// `parent_step_id` is the step producing parent rows.
    /// `fk_col` is the column on *this* table that references the parent PK.
    pub fn with_parent(mut self, parent_step_id: StepId, fk_col: impl Into<String>) -> Self {
        self.parent_step_id = Some(parent_step_id);
        self.fk_col = Some(fk_col.into());
        self.deps.push(parent_step_id);
        self
    }

    /// Override the parent PK column name (defaults to `"id"`).
    pub fn with_parent_pk_col(mut self, col: impl Into<String>) -> Self {
        self.parent_pk_col = col.into();
        self
    }

    /// Register a runtime tweak. Called at plan-time by field resolvers
    /// to add conditions, ordering, and pagination that depend on
    /// argument values only knowable at runtime.
    ///
    /// Tweaks are applied in registration order.
    pub fn apply(&mut self, tweak: impl Fn(SqlBuilder) -> SqlBuilder + Send + Sync + 'static) {
        self.tweaks.push(Box::new(tweak));
    }

    /// Add an explicit dependency.
    pub fn add_dep(&mut self, dep: StepId) {
        if !self.deps.contains(&dep) {
            self.deps.push(dep);
        }
    }

    /// Plan-time fingerprint hash: schema + table + selected_cols + fk_col.
    /// Does NOT include tweaks — tweaks are runtime, not plan-time.
    fn compute_config_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.schema.hash(&mut h);
        self.table.hash(&mut h);
        self.selected_cols.hash(&mut h);
        self.fk_col.hash(&mut h);
        h.finish()
    }

    /// Build the base SqlBuilder for this step's query.
    fn base_builder(&self) -> SqlBuilder {
        let mut builder = SqlBuilder::from(&self.schema, &self.table);
        if self.selected_cols.is_empty() {
            builder = builder.column_star();
        } else {
            for col in &self.selected_cols {
                builder = builder.column(SqlFragment::ident(col), None);
            }
        }
        builder
    }
}

#[async_trait::async_trait]
impl ExecutableStep for PgSelectStep {
    fn id(&self) -> StepId {
        self.id
    }

    fn dependencies(&self) -> &[StepId] {
        &self.deps
    }

    fn fingerprint(&self) -> StepFingerprint {
        StepFingerprint::new(
            TypeId::of::<PgSelectStep>(),
            self.deps.clone(),
            self.compute_config_hash(),
        )
    }

    async fn execute(
        &self,
        _ctx: &ExecutionContext,
        inputs: StepInputs,
    ) -> Result<StepOutput, FwGraphError> {
        // 1. Build the plan-time SQL skeleton
        let mut builder = self.base_builder();

        // 2. If batching by parent IDs, add the ANY($1) condition
        let _parent_ids: Vec<PgValue> =
            if let (Some(fk), Some(parent_out)) = (&self.fk_col, inputs.dep_outputs.first()) {
                let ids: Vec<PgValue> = parent_out
                    .values
                    .iter()
                    .zip(parent_out.flags.iter())
                    .filter(|(_, flag)| flag.is_value())
                    .filter_map(|(v, _)| v.downcast_ref::<PgRow>())
                    .filter_map(|row| row.get(&self.parent_pk_col).cloned())
                    .filter(|v| !v.is_null())
                    .collect();

                if !ids.is_empty() {
                    builder = builder.where_clause(
                        SqlFragment::ident(fk)
                            .push_raw(" = ANY(")
                            .append(SqlFragment::param(PgValue::Array(ids.clone())))
                            .push_raw(")"),
                    );
                }

                ids
            } else {
                Vec::new()
            };

        // 3. Apply runtime tweaks (filters, ordering, pagination)
        for tweak in &self.tweaks {
            builder = tweak(builder);
        }

        // 4. Build final SQL
        let fragment = builder.build();
        let (sql, sql_params) = fragment.build();

        debug!(
            step_id = self.id,
            sql = %sql,
            param_count = sql_params.len(),
            "PgSelectStep executing query"
        );

        // 5. Execute via sqlx
        let rows = codec::execute_select(&self.pool, &sql, &sql_params)
            .await
            .map_err(|e| FwGraphError::ExecutionError(format!("PgSelectStep: {}", e)))?;

        // 6. Build output
        if self.parent_step_id.is_some() && self.fk_col.is_some() {
            // Batched mode: partition rows by FK value, one output per parent
            let fk_col = self.fk_col.as_deref().unwrap();
            let parent_output = inputs.dep_outputs.first().unwrap();
            let batch_size = parent_output.len();

            let mut values: Vec<Arc<dyn std::any::Any + Send + Sync>> =
                Vec::with_capacity(batch_size);
            let mut flags: Vec<StepFlags> = Vec::with_capacity(batch_size);

            for i in 0..batch_size {
                if !parent_output.flags[i].is_value() {
                    // Propagate null/error from parent
                    values.push(Arc::new(Vec::<PgRow>::new()));
                    flags.push(parent_output.flags[i]);
                    continue;
                }

                let parent_id = parent_output.values[i]
                    .downcast_ref::<PgRow>()
                    .and_then(|row| row.get(&self.parent_pk_col).cloned());

                // Collect rows matching this parent
                let matching: Vec<PgRow> = if let Some(ref pid) = parent_id {
                    rows.iter()
                        .filter(|row| {
                            row.get(fk_col).map_or(false, |v| pg_value_eq(v, pid))
                        })
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };

                values.push(Arc::new(matching));
                flags.push(StepFlags::NONE);
            }

            Ok(StepOutput::new(values, flags))
        } else {
            // Root mode: return all rows as a single value
            let output_value: Arc<dyn std::any::Any + Send + Sync> = Arc::new(rows);
            Ok(StepOutput::from_values(vec![output_value]))
        }
    }
}

/// Compare two PgValues for equality (used for FK matching).
fn pg_value_eq(a: &PgValue, b: &PgValue) -> bool {
    match (a, b) {
        (PgValue::Null, PgValue::Null) => false, // NULL != NULL
        (PgValue::Bool(a), PgValue::Bool(b)) => a == b,
        (PgValue::Int(a), PgValue::Int(b)) => a == b,
        (PgValue::Float(a), PgValue::Float(b)) => a == b,
        (PgValue::Text(a), PgValue::Text(b)) => a == b,
        (PgValue::Uuid(a), PgValue::Uuid(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a mock PgPool. We cannot execute queries without a real DB,
    /// but we can test fingerprinting and builder logic.
    fn mock_pool() -> PgPool {
        // PgPool::connect_lazy requires a valid-looking URL but does not
        // actually connect until a query is executed.
        let opts = sqlx::postgres::PgPoolOptions::new().max_connections(1);
        opts.connect_lazy("postgres://localhost/test_does_not_exist")
            .expect("connect_lazy should not fail")
    }

    #[tokio::test]
    async fn same_table_and_cols_produce_same_fingerprint() {
        let pool = mock_pool();
        let step1 = PgSelectStep::new(
            1,
            pool.clone(),
            "public",
            "users",
            vec!["id".into(), "name".into()],
        );
        let step2 = PgSelectStep::new(
            2,
            pool,
            "public",
            "users",
            vec!["id".into(), "name".into()],
        );
        assert_eq!(step1.fingerprint(), step2.fingerprint());
    }

    #[tokio::test]
    async fn different_tables_produce_different_fingerprints() {
        let pool = mock_pool();
        let step1 = PgSelectStep::new(1, pool.clone(), "public", "users", vec!["id".into()]);
        let step2 = PgSelectStep::new(2, pool, "public", "posts", vec!["id".into()]);
        assert_ne!(step1.fingerprint(), step2.fingerprint());
    }

    #[tokio::test]
    async fn different_cols_produce_different_fingerprints() {
        let pool = mock_pool();
        let step1 = PgSelectStep::new(
            1,
            pool.clone(),
            "public",
            "users",
            vec!["id".into(), "name".into()],
        );
        let step2 = PgSelectStep::new(
            2,
            pool,
            "public",
            "users",
            vec!["id".into(), "email".into()],
        );
        assert_ne!(step1.fingerprint(), step2.fingerprint());
    }

    #[tokio::test]
    async fn fk_col_affects_fingerprint() {
        let pool = mock_pool();
        let step1 = PgSelectStep::new(1, pool.clone(), "public", "posts", vec!["id".into()])
            .with_parent(0, "author_id");
        let step2 = PgSelectStep::new(2, pool, "public", "posts", vec!["id".into()]);
        assert_ne!(step1.fingerprint(), step2.fingerprint());
    }

    #[tokio::test]
    async fn base_builder_generates_correct_sql() {
        let pool = mock_pool();
        let step = PgSelectStep::new(
            1,
            pool,
            "public",
            "users",
            vec!["id".into(), "name".into(), "email".into()],
        );
        let builder = step.base_builder();
        let (sql, params) = builder.build().build();
        assert_eq!(
            sql,
            "SELECT \"id\", \"name\", \"email\" FROM \"public\".\"users\""
        );
        assert!(params.is_empty());
    }

    #[tokio::test]
    async fn base_builder_with_no_cols_uses_star() {
        let pool = mock_pool();
        let step = PgSelectStep::new(1, pool, "public", "users", vec![]);
        let (sql, _) = step.base_builder().build().build();
        assert_eq!(sql, "SELECT * FROM \"public\".\"users\"");
    }

    #[tokio::test]
    async fn tweaks_modify_builder() {
        let pool = mock_pool();
        let mut step = PgSelectStep::new(
            1,
            pool,
            "public",
            "users",
            vec!["id".into(), "name".into()],
        );
        step.apply(|b| b.limit(10));
        step.apply(|b| b.order_by(SqlFragment::ident("name"), true));

        // Apply tweaks manually to verify they work
        let mut builder = step.base_builder();
        for tweak in &step.tweaks {
            builder = tweak(builder);
        }
        let (sql, params) = builder.build().build();
        assert!(sql.contains("LIMIT $1"));
        assert!(sql.contains("ORDER BY \"name\" ASC"));
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].as_i64(), Some(10));
    }

    #[test]
    fn pg_value_eq_works() {
        assert!(pg_value_eq(&PgValue::Int(1), &PgValue::Int(1)));
        assert!(!pg_value_eq(&PgValue::Int(1), &PgValue::Int(2)));
        assert!(pg_value_eq(
            &PgValue::Text("a".into()),
            &PgValue::Text("a".into())
        ));
        assert!(!pg_value_eq(&PgValue::Null, &PgValue::Null));
        assert!(!pg_value_eq(&PgValue::Int(1), &PgValue::Text("1".into())));

        let u = uuid::Uuid::new_v4();
        assert!(pg_value_eq(&PgValue::Uuid(u), &PgValue::Uuid(u)));
    }
}
