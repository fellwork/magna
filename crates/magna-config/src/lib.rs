//! magna-config — Plugin and preset configuration system for the magna
//! engine.
//!
//! This crate defines:
//!
//! - [`Preset`] — the single configuration object passed to all magna
//!   components. Presets are composable: start from sensible Supabase defaults
//!   and override specific fields.
//! - [`Plugin`] — an object-safe trait that allows extending the schema build
//!   process. Plugins hook into the gather phase (modifying introspection
//!   output) or the schema phase (adding custom types and fields).
//! - [`resolve::merge`] — merge multiple presets where later overrides win.
//! - [`resolve::validate`] — validate that a preset has all required fields.

pub mod plugin;
pub mod preset;
pub mod resolve;

// Re-export primary public API.
pub use plugin::{GatherContext, Plugin, SchemaContext};
pub use preset::{JwtConfig, PoolConfig, Preset, SchemaBuildOptions};
pub use resolve::{merge, validate, PresetOverride, ResolveError};
