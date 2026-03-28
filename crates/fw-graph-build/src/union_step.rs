//! PgUnionStep — polymorphic type discriminator step.
//!
//! Takes a batch of PgRows from a source step and tags each row with a
//! type name by calling a discriminate function. Propagates NULL/ERROR flags
//! from the source without executing the discriminate function for flagged rows.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use fw_graph_core::{ExecutableStep, ExecutionContext, StepFingerprint, StepInputs, StepOutput};
use fw_graph_types::{FwGraphError, PgRow, StepFlags, StepId};

/// A PgRow tagged with a concrete GraphQL type name.
#[derive(Debug, Clone)]
pub struct TaggedRow {
    /// The discriminated GraphQL type name (e.g. `"Dog"`, `"Cat"`).
    pub type_name: &'static str,
    /// The underlying Postgres row.
    pub row: PgRow,
}

/// A step that discriminates a batch of PgRows into tagged union variants.
///
/// Reads the output of `source_step_id`, calls `discriminate` on each row,
/// and produces a batch of `TaggedRow` values. NULL/ERROR flags propagate
/// without invoking the discriminate closure.
pub struct PgUnionStep {
    id: StepId,
    source_step_id: StepId,
    discriminate: Arc<dyn Fn(&PgRow) -> &'static str + Send + Sync>,
    deps: Vec<StepId>,
}

impl PgUnionStep {
    /// Create a new PgUnionStep.
    ///
    /// - `id` — the unique StepId for this step within the plan.
    /// - `source_step_id` — the step whose output provides the PgRow batch.
    /// - `discriminate` — closure that inspects a PgRow and returns a `&'static str` type name.
    pub fn new(
        id: StepId,
        source_step_id: StepId,
        discriminate: impl Fn(&PgRow) -> &'static str + Send + Sync + 'static,
    ) -> Self {
        Self {
            id,
            source_step_id,
            discriminate: Arc::new(discriminate),
            deps: vec![source_step_id],
        }
    }
}

#[async_trait]
impl ExecutableStep for PgUnionStep {
    fn id(&self) -> StepId {
        self.id
    }

    fn dependencies(&self) -> &[StepId] {
        &self.deps
    }

    fn fingerprint(&self) -> StepFingerprint {
        StepFingerprint::new(
            std::any::TypeId::of::<PgUnionStep>(),
            self.deps.clone(),
            (self.id, self.source_step_id),
        )
    }

    async fn execute(
        &self,
        _ctx: &ExecutionContext,
        inputs: StepInputs,
    ) -> Result<StepOutput, FwGraphError> {
        // The first (and only) dependency is the source step.
        let source = inputs.dep_outputs.first().ok_or_else(|| {
            FwGraphError::ExecutionError(
                "PgUnionStep: missing source step output".to_string(),
            )
        })?;

        let batch_size = source.values.len();
        let mut values: Vec<Arc<dyn Any + Send + Sync>> = Vec::with_capacity(batch_size);
        let mut flags: Vec<StepFlags> = Vec::with_capacity(batch_size);

        for i in 0..batch_size {
            let flag = source.flags[i];

            if !flag.is_value() {
                // NULL or ERROR — propagate without discriminating.
                let null_value: Arc<dyn Any + Send + Sync> = Arc::new(());
                values.push(null_value);
                flags.push(flag);
                continue;
            }

            // Try to downcast the source value to a PgRow.
            let row = source.values[i]
                .downcast_ref::<PgRow>()
                .ok_or_else(|| {
                    FwGraphError::ExecutionError(format!(
                        "PgUnionStep: source value at index {} is not a PgRow",
                        i
                    ))
                })?
                .clone();

            let type_name = (self.discriminate)(&row);
            let tagged: Arc<dyn Any + Send + Sync> = Arc::new(TaggedRow { type_name, row });
            values.push(tagged);
            flags.push(StepFlags::NONE);
        }

        Ok(StepOutput::new(values, flags))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use fw_graph_core::{StepFingerprint, StepInputs, StepOutput};
    use fw_graph_types::{PgValue, StepId};
    use indexmap::IndexMap;
    use std::any::TypeId;

    // ── Helper: a source step that emits a fixed Vec<PgRow> batch ─────────────

    struct RowSourceStep {
        id: StepId,
        rows: Vec<PgRow>,
        flags: Vec<StepFlags>,
    }

    impl RowSourceStep {
        fn new(id: StepId, rows: Vec<PgRow>) -> Arc<dyn ExecutableStep> {
            let flags = vec![StepFlags::NONE; rows.len()];
            Arc::new(Self { id, rows, flags })
        }

        fn with_flags(id: StepId, rows: Vec<PgRow>, flags: Vec<StepFlags>) -> Arc<dyn ExecutableStep> {
            Arc::new(Self { id, rows, flags })
        }
    }

    #[async_trait]
    impl ExecutableStep for RowSourceStep {
        fn id(&self) -> StepId { self.id }
        fn dependencies(&self) -> &[StepId] { &[] }
        fn fingerprint(&self) -> StepFingerprint {
            StepFingerprint::new(TypeId::of::<RowSourceStep>(), vec![], self.id)
        }

        async fn execute(
            &self,
            _ctx: &ExecutionContext,
            _inputs: StepInputs,
        ) -> Result<StepOutput, FwGraphError> {
            let values: Vec<Arc<dyn Any + Send + Sync>> = self
                .rows
                .iter()
                .map(|r| Arc::new(r.clone()) as Arc<dyn Any + Send + Sync>)
                .collect();
            Ok(StepOutput::new(values, self.flags.clone()))
        }
    }

    fn make_exec_ctx() -> Arc<ExecutionContext> {
        Arc::new(ExecutionContext {
            request_id: uuid::Uuid::new_v4(),
            jwt_claims: None,
            variables: Arc::new(serde_json::Value::Null),
        })
    }

    fn make_row(typename: &str) -> PgRow {
        let mut row = IndexMap::new();
        row.insert("__typename".to_string(), PgValue::Text(typename.to_string()));
        row
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_union_step_tags_rows() {
        use fw_graph_core::{Planner, optimize, Executor};

        let rows = vec![make_row("dog"), make_row("cat")];
        let source = RowSourceStep::new(0, rows);
        let union_step = Arc::new(PgUnionStep::new(1, 0, |row| {
            match row.get("__typename").and_then(|v| v.as_text()) {
                Some("dog") => "dog",
                Some("cat") => "cat",
                _ => "unknown",
            }
        }));

        let mut planner = Planner::new(2);
        planner.register(source);
        planner.register(union_step);

        let plan = planner.build().unwrap();
        let optimized = optimize(plan);
        let outputs = Executor::execute(&optimized, make_exec_ctx()).await.unwrap();

        let result = &outputs[&1];
        assert_eq!(result.len(), 2);

        let row0 = result.values[0].downcast_ref::<TaggedRow>().expect("TaggedRow");
        let row1 = result.values[1].downcast_ref::<TaggedRow>().expect("TaggedRow");

        assert_eq!(row0.type_name, "dog");
        assert_eq!(row1.type_name, "cat");

        assert!(result.flags[0].is_value());
        assert!(result.flags[1].is_value());
    }

    #[tokio::test]
    async fn test_union_step_propagates_null_flags() {
        use fw_graph_core::{Planner, optimize, Executor};

        let rows = vec![make_row("dog"), make_row("cat")];
        // First row is normal, second is NULL-flagged.
        let flags = vec![StepFlags::NONE, StepFlags::NULL];
        let source = RowSourceStep::with_flags(0, rows, flags);

        let union_step = Arc::new(PgUnionStep::new(1, 0, |row| {
            match row.get("__typename").and_then(|v| v.as_text()) {
                Some("dog") => "dog",
                Some("cat") => "cat",
                _ => "unknown",
            }
        }));

        let mut planner = Planner::new(2);
        planner.register(source);
        planner.register(union_step);

        let plan = planner.build().unwrap();
        let optimized = optimize(plan);
        let outputs = Executor::execute(&optimized, make_exec_ctx()).await.unwrap();

        let result = &outputs[&1];
        assert_eq!(result.len(), 2);

        // First item: normal TaggedRow
        assert!(result.flags[0].is_value(), "first item should be a value");
        let row0 = result.values[0].downcast_ref::<TaggedRow>().expect("TaggedRow");
        assert_eq!(row0.type_name, "dog");

        // Second item: flag should be NULL, no TaggedRow
        assert!(result.flags[1].is_null(), "second item should carry NULL flag");
    }
}
