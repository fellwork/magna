//! fw-graph-types — shared types for the fw-graph query planning engine.
//!
//! This crate has zero logic. It defines types, trait bounds, and error types
//! consumed by all other fw-graph crates. Its API must be frozen before
//! downstream crates begin implementation.

pub mod step;
pub mod pg;
pub mod auth;
pub mod error;

pub use step::{StepId, StepResult, StepFlags};
pub use pg::{PgValue, PgRow, PgTypeOid};
pub use auth::{JwtClaims, JwtRole};
pub use error::{FwGraphError, StepError};
