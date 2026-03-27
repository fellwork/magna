//! Postgres step implementations — each step type corresponds to a SQL
//! operation (SELECT, INSERT, UPDATE, DELETE) and implements
//! [`ExecutableStep`](fw_graph_core::ExecutableStep).

pub mod pg_select;
pub mod pg_insert;
pub mod pg_update;
pub mod pg_delete;
