//! The [`Preset`] struct — the single configuration object passed to all
//! magna components.
//!
//! Presets are composable: you can start from [`Preset::default()`] (sensible
//! Supabase defaults) and override specific fields. The [`super::resolve`]
//! module provides utilities for merging multiple presets.

use crate::plugin::Plugin;

/// JWT authentication configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JwtConfig {
    /// HMAC secret for HS256-signed JWTs (e.g. Supabase JWT secret).
    /// Falls back to the `JWT_SECRET` environment variable when empty.
    #[serde(default)]
    pub secret: String,

    /// JWKS URL for RS256/ES256 tokens (alternative to `secret`).
    /// When set, the server fetches the JSON Web Key Set from this URL
    /// and validates tokens against the published public keys.
    #[serde(default)]
    pub jwks_url: Option<String>,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: std::env::var("JWT_SECRET").unwrap_or_default(),
            jwks_url: None,
        }
    }
}

/// Connection pool settings for the Postgres connection.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PoolConfig {
    /// Maximum number of connections in the pool.
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,

    /// Minimum number of idle connections to maintain.
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,

    /// Connection acquisition timeout in seconds.
    #[serde(default = "default_acquire_timeout_secs")]
    pub acquire_timeout_secs: u64,

    /// Maximum connection lifetime in seconds before recycling.
    #[serde(default = "default_max_lifetime_secs")]
    pub max_lifetime_secs: u64,

    /// Idle timeout in seconds — connections idle longer than this are closed.
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
}

fn default_max_connections() -> u32 { 10 }
fn default_min_connections() -> u32 { 1 }
fn default_acquire_timeout_secs() -> u64 { 30 }
fn default_max_lifetime_secs() -> u64 { 1800 }
fn default_idle_timeout_secs() -> u64 { 600 }

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: default_max_connections(),
            min_connections: default_min_connections(),
            acquire_timeout_secs: default_acquire_timeout_secs(),
            max_lifetime_secs: default_max_lifetime_secs(),
            idle_timeout_secs: default_idle_timeout_secs(),
        }
    }
}

/// Schema build options — controls which GraphQL features are generated.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SchemaBuildOptions {
    /// Generate create/update/delete mutations for tables with primary keys.
    #[serde(default = "default_true")]
    pub default_mutations: bool,

    /// Generate GraphQL subscriptions for live queries.
    #[serde(default = "default_true")]
    pub subscriptions: bool,

    /// Generate Relay-compatible `Node` interface, `nodeId` fields,
    /// and `node(id: ID!)` / `nodes(ids: [ID!]!)` root queries.
    #[serde(default = "default_true")]
    pub relay: bool,

    /// Generate paginated `Connection` types (Relay cursor pagination).
    #[serde(default = "default_true")]
    pub connections: bool,

    /// Generate non-paginated list fields (e.g. `allUsers: [User!]!`).
    #[serde(default)]
    pub simple_lists: bool,
}

fn default_true() -> bool { true }

impl Default for SchemaBuildOptions {
    fn default() -> Self {
        Self {
            default_mutations: true,
            subscriptions: true,
            relay: true,
            connections: true,
            simple_lists: false,
        }
    }
}

/// The single configuration object passed to all magna components.
///
/// Presets are composable — start from [`Preset::default()`] (Supabase
/// defaults) and override specific fields. Use [`super::resolve::merge`]
/// to combine multiple presets, where later values override earlier ones.
pub struct Preset {
    /// Postgres schemas to expose in the GraphQL API.
    pub pg_schemas: Vec<String>,

    /// The default Postgres role for unauthenticated requests.
    pub default_role: Option<String>,

    /// JWT authentication configuration.
    pub jwt: JwtConfig,

    /// Connection pool settings.
    pub pool: PoolConfig,

    /// Schema build options.
    pub schema: SchemaBuildOptions,

    /// Whether to enable subscriptions globally.
    pub enable_subscriptions: bool,

    /// When true, only operations matching the trusted documents list are
    /// allowed. Strongly recommended for production public APIs.
    pub trusted_documents_only: bool,

    /// How long to cache schema introspection results (seconds).
    pub introspection_cache_ttl: u64,

    /// Ordered list of plugins. Plugins are applied in order — later plugins
    /// may override behavior set by earlier ones.
    pub plugins: Vec<Box<dyn Plugin>>,
}

impl Default for Preset {
    /// Sensible defaults for a Supabase-backed deployment:
    /// - Schema: `["public"]`
    /// - Default role: `"anon"`
    /// - JWT secret read from `JWT_SECRET` env var
    /// - Subscriptions enabled
    /// - All CRUD mutations enabled
    /// - Relay support enabled
    fn default() -> Self {
        Self {
            pg_schemas: vec!["public".to_string()],
            default_role: Some("anon".to_string()),
            jwt: JwtConfig::default(),
            pool: PoolConfig::default(),
            schema: SchemaBuildOptions::default(),
            enable_subscriptions: true,
            trusted_documents_only: false,
            introspection_cache_ttl: 60,
            plugins: vec![],
        }
    }
}

// Manual Debug impl because Vec<Box<dyn Plugin>> doesn't auto-derive Debug
// (the dyn Plugin Debug impl is in plugin.rs).
impl std::fmt::Debug for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Preset")
            .field("pg_schemas", &self.pg_schemas)
            .field("default_role", &self.default_role)
            .field("jwt", &self.jwt)
            .field("pool", &self.pool)
            .field("schema", &self.schema)
            .field("enable_subscriptions", &self.enable_subscriptions)
            .field("trusted_documents_only", &self.trusted_documents_only)
            .field("introspection_cache_ttl", &self.introspection_cache_ttl)
            .field("plugins", &self.plugins)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preset_has_public_schema() {
        let preset = Preset::default();
        assert_eq!(preset.pg_schemas, vec!["public".to_string()]);
    }

    #[test]
    fn default_preset_has_anon_role() {
        let preset = Preset::default();
        assert_eq!(preset.default_role, Some("anon".to_string()));
    }

    #[test]
    fn default_preset_subscriptions_enabled() {
        let preset = Preset::default();
        assert!(preset.enable_subscriptions);
    }

    #[test]
    fn default_preset_mutations_enabled() {
        let preset = Preset::default();
        assert!(preset.schema.default_mutations);
    }

    #[test]
    fn default_preset_relay_enabled() {
        let preset = Preset::default();
        assert!(preset.schema.relay);
    }

    #[test]
    fn default_preset_trusted_docs_disabled() {
        let preset = Preset::default();
        assert!(!preset.trusted_documents_only);
    }

    #[test]
    fn default_pool_settings() {
        let pool = PoolConfig::default();
        assert_eq!(pool.max_connections, 10);
        assert_eq!(pool.min_connections, 1);
        assert_eq!(pool.acquire_timeout_secs, 30);
    }

    #[test]
    fn default_introspection_cache_ttl() {
        let preset = Preset::default();
        assert_eq!(preset.introspection_cache_ttl, 60);
    }

    #[test]
    fn preset_debug_format() {
        let preset = Preset::default();
        let debug_str = format!("{:?}", preset);
        assert!(debug_str.contains("Preset"));
        assert!(debug_str.contains("public"));
    }

    #[test]
    fn jwt_config_default_empty_secret() {
        // In test environment JWT_SECRET is unlikely to be set.
        // We just verify it doesn't panic.
        let jwt = JwtConfig::default();
        // secret is either from env or empty string
        assert!(jwt.jwks_url.is_none());
    }

    #[test]
    fn schema_build_options_defaults() {
        let opts = SchemaBuildOptions::default();
        assert!(opts.default_mutations);
        assert!(opts.subscriptions);
        assert!(opts.relay);
        assert!(opts.connections);
        assert!(!opts.simple_lists);
    }
}
