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

/// Which introspected relations become **auto-generated** GraphQL fields.
///
/// This narrows the auto-generated surface only. Fields registered by a
/// `SchemaExtension` are unaffected by every variant here, including
/// [`Exposure::ExtensionsOnly`] — an extension field is hand-written, so it is
/// never something the engine decided to expose on your behalf.
///
/// Exposure is evaluated once, in the gather phase, against the qualified
/// `schema.relation` name. A relation filtered out gets no object type, no
/// query field, and no relation field pointing at it from anywhere else,
/// because every downstream pass reads the same filtered resource list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exposure {
    /// Every table and view in [`Preset::pg_schemas`]. The default, and what
    /// makes magna a Postgres gateway.
    All,

    /// Only the named relations, each written `schema.relation`
    /// (e.g. `"public.author"`).
    ///
    /// Matched exactly — no globbing, no case folding. A typo therefore hides
    /// a relation rather than silently widening the surface, which is the
    /// direction a mistake should fail in.
    Only(Vec<String>),

    /// No auto-generated fields at all. The schema contains exactly what
    /// extensions register.
    ///
    /// This is the mode for **embedding magna as a planning and execution
    /// engine** rather than running it as a Postgres gateway: two-phase
    /// planning, fingerprint deduplication, and batched `PgSelectStep` all
    /// still work for extension-registered fields, but no relation is
    /// reachable by default.
    ///
    /// Use this when authorization lives in your application rather than in
    /// Postgres RLS — e.g. a service-role pool with redaction in Rust. In that
    /// setup `Exposure::All` would publish every table in `pg_schemas` with no
    /// row-level protection behind it.
    ExtensionsOnly,
}

impl Exposure {
    /// Is `schema.relation` part of the auto-generated surface?
    pub fn allows(&self, schema: &str, relation: &str) -> bool {
        match self {
            Exposure::All => true,
            Exposure::ExtensionsOnly => false,
            Exposure::Only(names) => names.iter().any(|n| {
                n.split_once('.')
                    .is_some_and(|(s, r)| s == schema && r == relation)
            }),
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

    /// Which relations *within* [`Preset::pg_schemas`] are auto-generated.
    ///
    /// `pg_schemas` is a coarse gate (whole schemas); this is the fine one.
    /// Defaults to [`Exposure::All`] — every relation in the listed schemas.
    pub exposure: Exposure,

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
    /// - Exposure: every relation in those schemas ([`Exposure::All`])
    /// - Default role: `"anon"`
    /// - JWT secret read from `JWT_SECRET` env var
    /// - Subscriptions enabled
    /// - All CRUD mutations enabled
    /// - Relay support enabled
    ///
    /// `Exposure::All` is the gateway default: it is what makes "point magna
    /// at a database and get an API" true. It assumes Postgres is enforcing
    /// authorization — RLS policies plus the per-request role that
    /// `magna-serv` sets. **An embedder that connects with a privileged role
    /// and authorizes in application code must not keep this default**; see
    /// [`Exposure::ExtensionsOnly`].
    fn default() -> Self {
        Self {
            pg_schemas: vec!["public".to_string()],
            exposure: Exposure::All,
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
            .field("exposure", &self.exposure)
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

    // ── Exposure ────────────────────────────────────────────────────────
    // Every negative case below is a case where a *wrong* answer publishes a
    // relation the operator did not name. They are written as the security
    // property, not as coverage.

    #[test]
    fn default_exposure_is_all() {
        // The gateway default. Changing it is a product decision, not a
        // refactor — this test exists so the change is deliberate.
        assert_eq!(Preset::default().exposure, Exposure::All);
    }

    #[test]
    fn exposure_all_admits_every_relation() {
        assert!(Exposure::All.allows("public", "users"));
        assert!(Exposure::All.allows("usr", "notes"));
    }

    #[test]
    fn exposure_extensions_only_admits_nothing() {
        assert!(!Exposure::ExtensionsOnly.allows("public", "users"));
        assert!(!Exposure::ExtensionsOnly.allows("usr", "notes"));
    }

    #[test]
    fn exposure_only_requires_the_schema_to_match_too() {
        let e = Exposure::Only(vec!["public.users".to_string()]);
        assert!(e.allows("public", "users"));
        // The SAME relation name in another schema is a different relation.
        // Matching on the bare name here would expose `usr.users` because
        // someone allowlisted `public.users`.
        assert!(!e.allows("usr", "users"));
        assert!(!e.allows("public", "posts"));
    }

    #[test]
    fn exposure_only_does_not_glob() {
        // `*` is an ordinary character, not a wildcard. Someone reaching for
        // shell globbing must get nothing rather than everything.
        let e = Exposure::Only(vec!["public.*".to_string()]);
        assert!(!e.allows("public", "users"));
    }

    #[test]
    fn exposure_only_does_not_case_fold() {
        // Postgres identifiers are case-sensitive once quoted; folding here
        // would admit a relation the operator did not name.
        let e = Exposure::Only(vec!["public.Users".to_string()]);
        assert!(!e.allows("public", "users"));
        assert!(!e.allows("public", "USERS"));
    }

    #[test]
    fn exposure_only_ignores_unqualified_entries() {
        // A half-written rule names no schema. It must hide a relation, never
        // expose one.
        let e = Exposure::Only(vec!["users".to_string()]);
        assert!(!e.allows("public", "users"));
        assert!(!e.allows("", "users"));
    }

    #[test]
    fn exposure_only_with_an_empty_list_admits_nothing() {
        assert!(!Exposure::Only(vec![]).allows("public", "users"));
    }
}
