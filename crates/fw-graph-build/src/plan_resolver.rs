//! PlanContext — bridge between plan registration and resolver execution.
//!
//! Provides a single-use context that:
//! 1. Accepts step registrations via `register_step()` during the plan phase.
//! 2. Executes all registered steps once via `execute()`.
//! 3. Allows resolvers to read results via `get_result()` after execution.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use fw_graph_core::{ExecutableStep, ExecutionContext, Executor, Planner, StepOutput, optimize};
use fw_graph_types::{FwGraphError, StepId};

/// A single-use plan/execute context.
///
/// Steps are registered during the plan phase, then `execute()` is called once
/// to build the DAG, optimize it, and run it. After execution, `get_result()`
/// provides access to individual step outputs.
pub struct PlanContext {
    planner: Mutex<Option<Planner>>,
    results: OnceLock<HashMap<StepId, Arc<StepOutput>>>,
    exec_ctx: Arc<ExecutionContext>,
    #[allow(dead_code)]
    batch_size: usize,
}

impl PlanContext {
    /// Create a new PlanContext with the given execution context and batch size.
    pub fn new(exec_ctx: Arc<ExecutionContext>, batch_size: usize) -> Self {
        Self {
            planner: Mutex::new(Some(Planner::new(batch_size))),
            results: OnceLock::new(),
            exec_ctx,
            batch_size,
        }
    }

    /// Register a step for inclusion in the plan.
    ///
    /// Returns the step's ID. Fails if `execute()` has already been called.
    pub fn register_step(&self, step: Arc<dyn ExecutableStep>) -> Result<StepId, FwGraphError> {
        let id = step.id();
        let mut guard = self.planner.lock().map_err(|e| {
            FwGraphError::ExecutionError(format!("planner lock poisoned: {}", e))
        })?;

        match guard.as_mut() {
            Some(planner) => {
                planner.register(step);
                Ok(id)
            }
            None => Err(FwGraphError::ExecutionError(
                "cannot register steps after execute() has been called".to_string(),
            )),
        }
    }

    /// Execute all registered steps.
    ///
    /// Builds the execution plan, optimizes it, then runs it. Can only be called once;
    /// subsequent calls return an error.
    pub async fn execute(&self) -> Result<(), FwGraphError> {
        // Take the planner out of the Option — prevents double-execute and further registrations.
        let planner = {
            let mut guard = self.planner.lock().map_err(|e| {
                FwGraphError::ExecutionError(format!("planner lock poisoned: {}", e))
            })?;
            guard.take().ok_or_else(|| {
                FwGraphError::ExecutionError("execute() has already been called".to_string())
            })?
        };

        let plan = planner.build()?;
        let optimized = optimize(plan);
        let outputs = Executor::execute(&optimized, Arc::clone(&self.exec_ctx)).await?;

        // Store results — OnceLock guarantees this only happens once.
        self.results.set(outputs).map_err(|_| {
            FwGraphError::ExecutionError("results already set (internal error)".to_string())
        })?;

        Ok(())
    }

    /// Retrieve the output for a specific step after `execute()` completes.
    ///
    /// Returns `None` if `execute()` has not been called yet, or if the step ID is unknown.
    pub fn get_result(&self, step_id: StepId) -> Option<Arc<StepOutput>> {
        self.results.get()?.get(&step_id).cloned()
    }

    /// Returns `true` if `execute()` has been called and completed successfully.
    pub fn is_executed(&self) -> bool {
        self.results.get().is_some()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use fw_graph_core::{StepFingerprint, StepInputs};
    use std::any::TypeId;

    // ── ConstStep helper ──────────────────────────────────────────────────────

    struct ConstStep {
        id: StepId,
        value: String,
    }

    impl ConstStep {
        fn new(id: StepId, value: impl Into<String>) -> Arc<dyn ExecutableStep> {
            Arc::new(Self { id, value: value.into() })
        }
    }

    #[async_trait]
    impl ExecutableStep for ConstStep {
        fn id(&self) -> StepId {
            self.id
        }

        fn dependencies(&self) -> &[StepId] {
            &[]
        }

        fn fingerprint(&self) -> StepFingerprint {
            StepFingerprint::new(TypeId::of::<ConstStep>(), vec![], self.id)
        }

        async fn execute(
            &self,
            _ctx: &ExecutionContext,
            _inputs: StepInputs,
        ) -> Result<StepOutput, FwGraphError> {
            Ok(StepOutput::from_values(vec![Arc::new(self.value.clone())]))
        }
    }

    fn make_exec_ctx() -> Arc<ExecutionContext> {
        Arc::new(ExecutionContext {
            request_id: uuid::Uuid::new_v4(),
            jwt_claims: None,
            variables: Arc::new(serde_json::Value::Null),
        })
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_plan_context_register_and_execute() {
        let ctx = PlanContext::new(make_exec_ctx(), 1);
        let step = ConstStep::new(0, "hello");
        ctx.register_step(step).unwrap();

        ctx.execute().await.unwrap();
        assert!(ctx.is_executed());

        let result = ctx.get_result(0).expect("result should exist for step 0");
        let value = result.values[0]
            .downcast_ref::<String>()
            .expect("value should be String");
        assert_eq!(value, "hello");
    }

    #[tokio::test]
    async fn test_plan_context_cannot_register_after_execute() {
        let ctx = PlanContext::new(make_exec_ctx(), 1);
        ctx.register_step(ConstStep::new(0, "first")).unwrap();
        ctx.execute().await.unwrap();

        // Attempt to register after execute — must fail
        let err = ctx.register_step(ConstStep::new(1, "second"));
        assert!(err.is_err(), "should not be able to register after execute");
        match err.unwrap_err() {
            FwGraphError::ExecutionError(msg) => {
                assert!(msg.contains("execute()"), "error message should mention execute()");
            }
            other => panic!("unexpected error variant: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_plan_context_cannot_execute_twice() {
        let ctx = PlanContext::new(make_exec_ctx(), 1);
        ctx.register_step(ConstStep::new(0, "once")).unwrap();
        ctx.execute().await.unwrap();

        // Second call must fail
        let err = ctx.execute().await;
        assert!(err.is_err(), "second execute() should fail");
        match err.unwrap_err() {
            FwGraphError::ExecutionError(msg) => {
                assert!(msg.contains("already been called"), "error should mention already called");
            }
            other => panic!("unexpected error variant: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_plan_context_get_result_before_execute() {
        let ctx = PlanContext::new(make_exec_ctx(), 1);
        ctx.register_step(ConstStep::new(0, "pending")).unwrap();

        // Before execute(), results should be None
        assert!(ctx.get_result(0).is_none(), "result should be None before execute()");
        assert!(!ctx.is_executed());
    }

    #[tokio::test]
    async fn test_plan_context_multiple_steps() {
        let ctx = PlanContext::new(make_exec_ctx(), 1);
        ctx.register_step(ConstStep::new(0, "alpha")).unwrap();
        ctx.register_step(ConstStep::new(1, "beta")).unwrap();

        ctx.execute().await.unwrap();

        let r0 = ctx.get_result(0).expect("result for step 0");
        let r1 = ctx.get_result(1).expect("result for step 1");

        let v0 = r0.values[0].downcast_ref::<String>().unwrap();
        let v1 = r1.values[0].downcast_ref::<String>().unwrap();

        assert_eq!(v0, "alpha");
        assert_eq!(v1, "beta");
    }
}
