//! magna-dataplan — Postgres data-plan steps for the magna execution engine.
//!
//! This crate bridges [`magna_core`] (the two-phase planning and execution
//! engine) with Postgres. It implements [`ExecutableStep`] for all Postgres
//! data operations: SELECT, INSERT, UPDATE, and DELETE.
//!
//! Each step type owns a plan-time [`SqlBuilder`](magna_sql::SqlBuilder)
//! configuration and accepts runtime tweaks via a callback pattern. Steps
//! execute as batched queries — one SQL query serves an entire subtree
//! of a GraphQL request.
//!
//! # Architecture
//!
//! - **No async-graphql dependency.** This is a data-layer crate. Step
//!   implementations return `PgRow` and `PgValue`. The GraphQL schema layer
//!   (magna-build) maps those into GraphQL types.
//!
//! - **PgSelectStep** is the most important step — it is the Grafast
//!   equivalent of a DataLoader. It supports `WHERE col = ANY($1)` for
//!   batched lookups (multiple parent IDs to child rows in one query).
//!
//! - **PgCodec** converts between sqlx `Row` types and `PgValue`/`PgRow`
//!   from magna-types.
//!
//! - **PgResourceRegistry** maps introspected tables to PgSelectStep
//!   configurations, auto-creating resources from introspection data.

pub mod codec;
pub mod registry;
pub mod steps;

pub use codec::{PgCodec, decode_row, default_codecs};
pub use registry::{PgResource, PgResourceRegistry};
pub use steps::pg_select::PgSelectStep;
pub use steps::pg_insert::PgInsertStep;
pub use steps::pg_update::PgUpdateStep;
pub use steps::pg_delete::PgDeleteStep;
