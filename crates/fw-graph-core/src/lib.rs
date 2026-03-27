//! fw-graph-core — two-phase planning and execution engine.
//!
//! This is the core of the fw-graph system. It implements:
//!
//! - **Plan phase:** Synchronous. Accepts step registrations, builds a DAG,
//!   validates acyclicity. No data is fetched during planning.
//!
//! - **Optimize phase:** Deduplicates steps by fingerprint, topologically
//!   sorts, and hoists unary steps for early execution.
//!
//! - **Execute phase:** Async. Walks the DAG concurrently, dispatching steps
//!   whose deps are resolved. Null/Error/Inhibited flags propagate.
//!   Unary steps execute once; their result is broadcast.

pub mod step;
pub mod planner;
pub mod optimizer;
pub mod executor;
pub mod fingerprint;

pub use step::{ExecutableStep, StepOutput, StepInputs, ExecutionContext};
pub use planner::{Planner, ExecutionPlan};
pub use optimizer::{optimize, OptimizedPlan};
pub use executor::Executor;
pub use fingerprint::StepFingerprint;
