//! fw-graph-introspect — Postgres schema introspection via pg_catalog.
//!
//! Queries system catalogs (pg_class, pg_attribute, pg_constraint, pg_proc,
//! pg_type, pg_enum, pg_index, pg_namespace, pg_description) and assembles
//! a fully-typed [`IntrospectionResult`] describing the database schema.
//!
//! Results are cached with configurable TTL and can be invalidated via
//! `NOTIFY postgraphile_schema_reload`.

pub mod cache;
pub mod introspect;
pub mod queries;
pub mod types;

// Re-export the primary public API.
pub use cache::{IntrospectionCache, RELOAD_CHANNEL};
pub use introspect::introspect;
pub use types::{
    ForeignKeyAction, IntrospectionResult, PgAttribute, PgClass, PgClassKind, PgConstraint,
    PgConstraintKind, PgDescription, PgEnum, PgIndex, PgNamespace, PgProc, PgType,
    ProcVolatility,
};
