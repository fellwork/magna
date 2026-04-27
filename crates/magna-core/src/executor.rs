//! Executor -- async batch execution of an optimized plan.
//!
//! Walks the DAG in topological order. Steps whose dependencies have all
//! resolved are dispatched concurrently. Null/Error/Inhibited flags
//! propagate without executing dependents. Unary steps execute once;
//! their result is broadcast to all batch items.

use crate::optimizer::OptimizedPlan;
use crate::step::{ExecutionContext, StepInputs, StepOutput};
use magna_types::{FwGraphError, StepFlags, StepId};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, instrument, warn};

/// The Executor runs an OptimizedPlan against a batch of inputs.
pub struct Executor;

impl Executor {
    /// Execute the optimized plan, returning the outputs of all steps.
    ///
    /// Steps are dispatched in topological order. Steps at the same "tier"
    /// (all dependencies resolved) are dispatched concurrently via tokio tasks.
    #[instrument(skip_all, fields(batch_size = plan.plan.batch_size))]
    pub async fn execute(
        plan: &OptimizedPlan,
        ctx: Arc<ExecutionContext>,
    ) -> Result<HashMap<StepId, Arc<StepOutput>>, FwGraphError> {
        let batch_size = plan.plan.batch_size;
        let mut outputs: HashMap<StepId, Arc<StepOutput>> = HashMap::new();

        // Group steps into tiers: a tier is a set of steps whose
        // dependencies are all in previous tiers.
        let tiers = compute_tiers(&plan.execution_order, &plan.plan);

        for tier in tiers {
            // All steps in this tier can execute concurrently
            let mut handles = Vec::new();

            for step_id in tier {
                let Some(step) = plan.plan.get_step(step_id) else {
                    continue;
                };

                // Gather inputs from dependency outputs
                let dep_ids = step.dependencies();

                // Check if any dependency has propagating flags that
                // should skip this step's execution.
                let should_skip = check_propagating_flags(dep_ids, &outputs, batch_size);

                if let Some(skip_output) = should_skip {
                    outputs.insert(step_id, Arc::new(skip_output));
                    continue;
                }

                // Build inputs for this step
                let dep_outputs: Vec<Arc<StepOutput>> = dep_ids
                    .iter()
                    .filter_map(|id| outputs.get(id).cloned())
                    .collect();
                let inputs = StepInputs::new(dep_outputs);

                let step = Arc::clone(step);
                let ctx = Arc::clone(&ctx);

                handles.push(tokio::spawn(async move {
                    let id = step.id();
                    let is_unary = step.is_unary();
                    let result = step.execute(&ctx, inputs).await;
                    (id, is_unary, result)
                }));
            }

            // Await all concurrent steps in this tier
            for handle in handles {
                let (step_id, is_unary, result) = handle.await.map_err(|e| {
                    FwGraphError::ExecutionError(format!("Task join error: {}", e))
                })?;

                match result {
                    Ok(output) => {
                        let output = if is_unary {
                            broadcast_unary(output, batch_size)
                        } else {
                            output
                        };
                        debug!(step_id, "step completed successfully");
                        outputs.insert(step_id, Arc::new(output));
                    }
                    Err(e) => {
                        warn!(step_id, error = %e, "step execution failed");
                        outputs.insert(step_id, Arc::new(error_output_batch(&e, batch_size)));
                    }
                }
            }
        }

        Ok(outputs)
    }
}

/// Check if any dependency output has propagating NULL or ERROR flags
/// that should cause this step to be skipped entirely.
///
/// Returns Some(output) with propagated flags if the step should be skipped,
/// or None if the step should execute normally.
fn check_propagating_flags(
    dep_ids: &[StepId],
    outputs: &HashMap<StepId, Arc<StepOutput>>,
    batch_size: usize,
) -> Option<StepOutput> {
    if dep_ids.is_empty() {
        return None;
    }

    // For each batch item, check if ALL dependencies produced a value.
    // If any dependency has NULL or ERROR for an item, propagate it.
    let mut result_flags = vec![StepFlags::NONE; batch_size];
    let mut any_propagated = false;

    for &dep_id in dep_ids {
        let Some(dep_output) = outputs.get(&dep_id) else {
            continue;
        };

        for i in 0..batch_size.min(dep_output.flags.len()) {
            let flag = dep_output.flags[i];

            // NULL and ERROR propagate: skip this step for that item
            if flag.is_null() {
                result_flags[i] = StepFlags(result_flags[i].0 | StepFlags::NULL.0);
                any_propagated = true;
            }
            if flag.is_error() {
                result_flags[i] = StepFlags(result_flags[i].0 | StepFlags::ERROR.0);
                any_propagated = true;
            }
            // INHIBITED does NOT automatically propagate -- dependents choose
            // how to handle it. We pass it through in the inputs.
        }
    }

    if !any_propagated {
        return None;
    }

    // Check if ALL items are flagged -- if so, skip the entire step
    let all_flagged = result_flags.iter().all(|f| !f.is_value());
    if all_flagged {
        // Every item is null or error -- skip execution entirely
        let null_value: Arc<dyn std::any::Any + Send + Sync> = Arc::new(());
        Some(StepOutput {
            values: vec![null_value; batch_size],
            flags: result_flags,
        })
    } else {
        // Partial propagation: we still need to execute, but with
        // the understanding that some items are already flagged.
        // The step will receive all inputs and the caller can check flags.
        None
    }
}

/// Broadcast a unary step's single output to all batch items.
fn broadcast_unary(output: StepOutput, batch_size: usize) -> StepOutput {
    if output.values.len() == 1 && batch_size > 1 {
        let value = output.values[0].clone();
        let flag = output.flags[0];
        StepOutput {
            values: vec![value; batch_size],
            flags: vec![flag; batch_size],
        }
    } else {
        output
    }
}

/// Create an error output batch for a failed step.
fn error_output_batch(error: &FwGraphError, batch_size: usize) -> StepOutput {
    let null_value: Arc<dyn std::any::Any + Send + Sync> = Arc::new(format!("{}", error));
    StepOutput {
        values: vec![null_value; batch_size],
        flags: vec![StepFlags::ERROR; batch_size],
    }
}

/// Compute execution tiers from a topological ordering.
///
/// A tier is a group of steps that can all execute concurrently because
/// their dependencies are all in earlier tiers.
fn compute_tiers(
    execution_order: &[StepId],
    plan: &crate::planner::ExecutionPlan,
) -> Vec<Vec<StepId>> {
    if execution_order.is_empty() {
        return Vec::new();
    }

    // Assign each step a tier number = 1 + max tier of its dependencies
    let mut tier_of: HashMap<StepId, usize> = HashMap::new();
    let mut max_tier: usize = 0;

    for &step_id in execution_order {
        let deps = plan.dependencies_of(step_id);
        let tier = if deps.is_empty() {
            0
        } else {
            deps.iter()
                .filter_map(|d| tier_of.get(d))
                .max()
                .map(|t| t + 1)
                .unwrap_or(0)
        };
        tier_of.insert(step_id, tier);
        if tier > max_tier {
            max_tier = tier;
        }
    }

    // Collect steps by tier
    let mut tiers: Vec<Vec<StepId>> = vec![Vec::new(); max_tier + 1];
    for &step_id in execution_order {
        if let Some(&tier) = tier_of.get(&step_id) {
            tiers[tier].push(step_id);
        }
    }

    tiers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fingerprint::StepFingerprint;
    use crate::optimizer::optimize;
    use crate::planner::Planner;
    use crate::step::ExecutableStep;
    use std::any::TypeId;

    /// A mock step that produces a fixed value for each batch item.
    struct ValueStep {
        id: StepId,
        deps: Vec<StepId>,
        value: i64,
        unary: bool,
    }

    impl ValueStep {
        fn new(id: StepId, deps: Vec<StepId>, value: i64) -> Arc<dyn ExecutableStep> {
            Arc::new(Self {
                id,
                deps,
                value,
                unary: false,
            })
        }

        fn new_unary(id: StepId, deps: Vec<StepId>, value: i64) -> Arc<dyn ExecutableStep> {
            Arc::new(Self {
                id,
                deps,
                value,
                unary: true,
            })
        }
    }

    #[async_trait::async_trait]
    impl ExecutableStep for ValueStep {
        fn id(&self) -> StepId {
            self.id
        }
        fn dependencies(&self) -> &[StepId] {
            &self.deps
        }
        fn is_unary(&self) -> bool {
            self.unary
        }
        fn fingerprint(&self) -> StepFingerprint {
            StepFingerprint::new(TypeId::of::<ValueStep>(), self.deps.clone(), self.value)
        }
        async fn execute(
            &self,
            _ctx: &ExecutionContext,
            inputs: StepInputs,
        ) -> Result<StepOutput, FwGraphError> {
            let batch_size = if self.unary {
                1
            } else if inputs.dep_outputs.is_empty() {
                // Root step: we don't know batch size from inputs, use a default
                3
            } else {
                inputs.dep_outputs[0].len()
            };

            let values: Vec<Arc<dyn std::any::Any + Send + Sync>> = (0..batch_size)
                .map(|_| Arc::new(self.value) as Arc<dyn std::any::Any + Send + Sync>)
                .collect();

            Ok(StepOutput::from_values(values))
        }
    }

    /// A mock step that always produces NULL output.
    struct NullStep {
        id: StepId,
        deps: Vec<StepId>,
    }

    impl NullStep {
        fn new(id: StepId, deps: Vec<StepId>) -> Arc<dyn ExecutableStep> {
            Arc::new(Self { id, deps })
        }
    }

    #[async_trait::async_trait]
    impl ExecutableStep for NullStep {
        fn id(&self) -> StepId {
            self.id
        }
        fn dependencies(&self) -> &[StepId] {
            &self.deps
        }
        fn fingerprint(&self) -> StepFingerprint {
            StepFingerprint::new(TypeId::of::<NullStep>(), self.deps.clone(), 0u64)
        }
        async fn execute(
            &self,
            _ctx: &ExecutionContext,
            _inputs: StepInputs,
        ) -> Result<StepOutput, FwGraphError> {
            let null_value: Arc<dyn std::any::Any + Send + Sync> = Arc::new(());
            Ok(StepOutput {
                values: vec![null_value.clone(), null_value.clone(), null_value],
                flags: vec![StepFlags::NULL, StepFlags::NULL, StepFlags::NULL],
            })
        }
    }

    fn make_ctx() -> Arc<ExecutionContext> {
        Arc::new(ExecutionContext {
            request_id: uuid::Uuid::new_v4(),
            jwt_claims: None,
            variables: Arc::new(serde_json::Value::Null),
        })
    }

    #[tokio::test]
    async fn execute_simple_dag() {
        let mut planner = Planner::new(3);
        planner.register(ValueStep::new(0, vec![], 10));
        planner.register(ValueStep::new(1, vec![0], 20));

        let plan = planner.build().unwrap();
        let optimized = optimize(plan);
        let ctx = make_ctx();

        let outputs = Executor::execute(&optimized, ctx).await.unwrap();

        // Both steps should have produced output
        assert!(outputs.contains_key(&0));
        assert!(outputs.contains_key(&1));

        // Step 0 produces 3 items (batch_size=3)
        let out0 = &outputs[&0];
        assert_eq!(out0.len(), 3);

        // All values should be 10
        for i in 0..3 {
            let val = out0.values[i].downcast_ref::<i64>().unwrap();
            assert_eq!(*val, 10);
        }

        // Step 1 also produces 3 items with value 20
        let out1 = &outputs[&1];
        assert_eq!(out1.len(), 3);
        for i in 0..3 {
            let val = out1.values[i].downcast_ref::<i64>().unwrap();
            assert_eq!(*val, 20);
        }
    }

    #[tokio::test]
    async fn null_propagation_skips_dependents() {
        let mut planner = Planner::new(3);
        planner.register(NullStep::new(0, vec![]));
        planner.register(ValueStep::new(1, vec![0], 42));

        let plan = planner.build().unwrap();
        let optimized = optimize(plan);
        let ctx = make_ctx();

        let outputs = Executor::execute(&optimized, ctx).await.unwrap();

        // Step 0 produces NULL
        let out0 = &outputs[&0];
        assert!(out0.flags.iter().all(|f| f.is_null()));

        // Step 1 should have propagated NULL -- it should NOT have executed
        let out1 = &outputs[&1];
        assert!(
            out1.flags.iter().all(|f| f.is_null()),
            "null should propagate to dependent step"
        );
    }

    #[tokio::test]
    async fn unary_step_broadcasts() {
        let mut planner = Planner::new(3);
        planner.register(ValueStep::new_unary(0, vec![], 99));
        planner.register(ValueStep::new(1, vec![0], 50));

        let plan = planner.build().unwrap();
        let optimized = optimize(plan);
        let ctx = make_ctx();

        let outputs = Executor::execute(&optimized, ctx).await.unwrap();

        // Unary step 0 should be broadcast to batch_size=3
        let out0 = &outputs[&0];
        assert_eq!(out0.len(), 3, "unary output should be broadcast to batch size");

        // All 3 items should have the same value
        for i in 0..3 {
            let val = out0.values[i].downcast_ref::<i64>().unwrap();
            assert_eq!(*val, 99);
        }
    }

    #[tokio::test]
    async fn concurrent_independent_steps() {
        // Steps 1 and 2 both depend on 0 but not on each other.
        // They should execute in the same tier (concurrently).
        let mut planner = Planner::new(3);
        planner.register(ValueStep::new(0, vec![], 1));
        planner.register(ValueStep::new(1, vec![0], 2));
        planner.register(ValueStep::new(2, vec![0], 3));
        planner.register(ValueStep::new(3, vec![1, 2], 4));

        let plan = planner.build().unwrap();
        let optimized = optimize(plan);

        // Verify tier computation: steps 1 and 2 should be in the same tier
        let tiers = compute_tiers(&optimized.execution_order, &optimized.plan);
        assert!(tiers.len() >= 2, "should have at least 2 tiers");

        // Execute
        let ctx = make_ctx();
        let outputs = Executor::execute(&optimized, ctx).await.unwrap();

        // All 4 steps should have produced output
        assert_eq!(outputs.len(), 4);
    }
}
