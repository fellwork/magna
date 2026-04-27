//! magna-sql — composable SQL AST builder producing parameterized Postgres queries.
//!
//! This is the Rust equivalent of `pg-sql2`. It has zero async code and zero
//! database connections — it is pure data manipulation.
//!
//! # Architecture
//!
//! - [`fragment::SqlFragment`] is the internal IR. Fragments compose without
//!   worrying about `$N` numbering; parameter indices are renumbered automatically
//!   when fragments are joined.
//! - [`builder::SqlBuilder`] provides a high-level fluent API for constructing
//!   SELECT queries with columns, WHERE, JOINs, ORDER BY, LIMIT, and OFFSET.
//! - [`render`] provides convenience functions to turn builders/fragments into
//!   the final `(String, Vec<PgValue>)` tuple suitable for sqlx.
//!
//! # Safety
//!
//! All user-provided values are parameterized (`$1`, `$2`, ...) and **never**
//! interpolated into the query string. Table and column names are always
//! double-quoted to handle reserved words safely.

pub mod fragment;
pub mod builder;
pub mod render;
pub mod insert_builder;
pub mod update_builder;
pub mod delete_builder;

// Re-export the most commonly used types at crate root for convenience.
pub use fragment::{SqlFragment, SqlPart, raw, param, ident, qualified_ident};
pub use builder::{SqlBuilder, JoinType, JoinClause};
pub use render::{render, render_fragment, debug_format};
pub use insert_builder::InsertBuilder;
pub use update_builder::UpdateBuilder;
pub use delete_builder::DeleteBuilder;
