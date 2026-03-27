//! Error types for the fw-graph engine.

use crate::step::StepId;

/// Errors that occur during step execution — attached to individual results.
#[derive(Debug, Clone, thiserror::Error)]
pub enum StepError {
  #[error("Database error: {0}")]
  Database(String),

  #[error("Not found")]
  NotFound,

  #[error("Permission denied")]
  PermissionDenied,

  #[error("Invalid input: {0}")]
  InvalidInput(String),

  #[error("Internal error: {0}")]
  Internal(String),
}

/// Errors in the fw-graph planning and execution engine.
#[derive(Debug, thiserror::Error)]
pub enum FwGraphError {
  #[error("Plan cycle detected involving steps: {0:?}")]
  PlanCycle(Vec<StepId>),

  #[error("Step {0} not found in plan")]
  StepNotFound(StepId),

  #[error("Execution error: {0}")]
  ExecutionError(String),

  #[error("Introspection error: {0}")]
  IntrospectionError(String),

  #[error("Configuration error: {0}")]
  ConfigError(String),

  #[error("SQL build error: {0}")]
  SqlBuildError(String),

  #[error("Schema build error: {0}")]
  SchemaBuildError(String),
}
