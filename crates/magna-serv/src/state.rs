//! Application state — shared across all request handlers.

use crate::plan_cache::PlanCache;
use crate::schema_registry::SchemaRegistry;
use magna_config::Preset;
use sqlx::PgPool;
use std::sync::Arc;

/// Shared state injected into every Axum handler via `State<AppState>`.
#[derive(Clone)]
pub struct AppState {
    /// Database connection pool.
    pub pool: PgPool,
    /// Hot-reloadable GraphQL schema registry.
    pub schema_registry: SchemaRegistry,
    /// Memoized execution plan cache.
    pub plan_cache: Arc<PlanCache>,
    /// HMAC secret used to validate incoming JWTs.
    pub jwt_secret: String,
    /// Optional default Postgres role for unauthenticated requests.
    pub default_role: Option<String>,
}

impl AppState {
    /// Construct `AppState` from a connection pool, an initial schema, and a
    /// [`Preset`].
    ///
    /// The plan cache is initialised with a capacity of 512 entries.
    pub fn new(
        pool: PgPool,
        schema: async_graphql::dynamic::Schema,
        preset: &Preset,
    ) -> Self {
        Self {
            pool,
            schema_registry: SchemaRegistry::new(schema),
            plan_cache: Arc::new(PlanCache::new(512)),
            jwt_secret: preset.jwt.secret.clone(),
            default_role: preset.default_role.clone(),
        }
    }
}
