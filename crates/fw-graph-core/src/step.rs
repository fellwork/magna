//! ExecutableStep trait and supporting types.
//!
//! A step is the unit of work in an execution plan. Each step declares its
//! dependencies, a fingerprint for deduplication, and an async execute method
//! that processes a batch of inputs.

use crate::fingerprint::StepFingerprint;
use fw_graph_types::{StepFlags, StepId};
use std::any::Any;
use std::sync::Arc;

/// The output batch from one step execution.
/// Index i corresponds to input item i.
#[derive(Debug)]
pub struct StepOutput {
    /// One value per batch item. Each value is type-erased so steps can
    /// produce different concrete types while sharing a common pipeline.
    pub values: Vec<Arc<dyn Any + Send + Sync>>,

    /// Parallel to `values`. Carries NULL / ERROR / INHIBITED flags
    /// for downstream propagation.
    pub flags: Vec<StepFlags>,
}

impl StepOutput {
    /// Create a StepOutput where all items are values (no flags set).
    pub fn from_values(values: Vec<Arc<dyn Any + Send + Sync>>) -> Self {
        let flags = vec![StepFlags::NONE; values.len()];
        Self { values, flags }
    }

    /// Create a StepOutput with explicit flags per item.
    pub fn new(values: Vec<Arc<dyn Any + Send + Sync>>, flags: Vec<StepFlags>) -> Self {
        debug_assert_eq!(values.len(), flags.len(), "values and flags must have same length");
        Self { values, flags }
    }

    /// Number of items in this output batch.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether the output batch is empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// The inputs to one step -- outputs of its dependency steps, zipped.
pub struct StepInputs {
    /// One entry per dependency, each entry is the full output batch
    /// from that dependency. `dep_outputs[i]` corresponds to
    /// `ExecutableStep::dependencies()[i]`.
    pub dep_outputs: Vec<Arc<StepOutput>>,
}

impl StepInputs {
    /// Create empty inputs (for root steps with no dependencies).
    pub fn empty() -> Self {
        Self {
            dep_outputs: Vec::new(),
        }
    }

    /// Create inputs from a list of dependency outputs.
    pub fn new(dep_outputs: Vec<Arc<StepOutput>>) -> Self {
        Self { dep_outputs }
    }
}

/// Context available to all steps during execution.
/// Carries request-scoped data that is the same for every step in a plan.
pub struct ExecutionContext {
    /// Unique identifier for this request execution.
    pub request_id: uuid::Uuid,

    /// Decoded JWT claims from the incoming request, if authenticated.
    pub jwt_claims: Option<Arc<fw_graph_types::JwtClaims>>,

    /// Variables from the GraphQL operation (unary -- same for all batch items).
    pub variables: Arc<serde_json::Value>,
}

/// The trait that all step implementations must satisfy.
///
/// Steps are registered with the Planner during the plan phase, then executed
/// by the Executor during the execute phase. The trait is object-safe so that
/// steps can be stored in a heterogeneous DAG.
#[async_trait::async_trait]
pub trait ExecutableStep: Send + Sync + 'static {
    /// Unique ID assigned by the Planner. Must be stable within a plan.
    fn id(&self) -> StepId;

    /// The step IDs this step depends on. Order must be stable --
    /// `StepInputs.dep_outputs[i]` corresponds to `dependencies()[i]`.
    fn dependencies(&self) -> &[StepId];

    /// A unary step represents exactly ONE value regardless of batch size.
    /// Examples: JWT claims, GraphQL variables, a single looked-up config value.
    /// Unary step output is broadcast to all batch items -- execute() is called once.
    fn is_unary(&self) -> bool {
        false
    }

    /// A stable fingerprint for plan-time deduplication.
    /// Two steps with equal fingerprints produce identical outputs and
    /// one of them can be eliminated during optimization.
    fn fingerprint(&self) -> StepFingerprint;

    /// Execute this step against a batch of inputs.
    ///
    /// For non-unary steps: `output.len()` must equal `inputs.dep_outputs[0].len()`
    /// (or the batch size if there are no dependencies).
    ///
    /// For unary steps: `output.len()` must equal 1.
    async fn execute(
        &self,
        ctx: &ExecutionContext,
        inputs: StepInputs,
    ) -> Result<StepOutput, fw_graph_types::FwGraphError>;
}
