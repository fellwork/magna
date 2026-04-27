//! fw-graph — top-level binary that wires all 9 fw-graph crates together.
//!
//! Startup sequence:
//! 1. Load .env, parse CLI, init tracing
//! 2. Build Preset, connect PgPool
//! 3. Introspect database schema (cached)
//! 4. Build resource registry, gather IR, build GraphQL schema
//! 5. Start subscription manager + reload watcher
//! 6. Serve via Axum

mod config;

use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use sqlx::postgres::PgPoolOptions;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};

use config::Config;
use fw_graph_build::{build_schema, gather};
use fw_graph_dataplan::PgResourceRegistry;
use fw_graph_introspect::IntrospectionCache;
use fw_graph_serv::{build_router, AppState};
use fw_graph_subscriptions::PgSubscriptionManager;

#[tokio::main]
async fn main() {
    // 1. Load .env (ignore errors — file may not exist in production).
    dotenvy::dotenv().ok();

    // 2. Parse CLI / env configuration.
    let config = Config::parse();

    // 3. Init tracing with JSON format and env-filter support.
    fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!(port = config.port, "fw-graph starting");

    // 4. Build Preset from CLI config.
    let preset = config.to_preset();

    // 5. Connect PgPool with settings from Preset.
    let pool = PgPoolOptions::new()
        .max_connections(preset.pool.max_connections)
        .min_connections(preset.pool.min_connections)
        .acquire_timeout(Duration::from_secs(preset.pool.acquire_timeout_secs))
        .max_lifetime(Duration::from_secs(preset.pool.max_lifetime_secs))
        .idle_timeout(Duration::from_secs(preset.pool.idle_timeout_secs))
        .connect(&config.database_url)
        .await
        .expect("failed to connect to database");

    info!("database pool connected");

    // 6. Create IntrospectionCache.
    let cache = Arc::new(IntrospectionCache::new(preset.introspection_cache_ttl));

    // 7. Introspect the database schema.
    let schema_refs: Vec<&str> = preset.pg_schemas.iter().map(|s| s.as_str()).collect();
    let introspection = cache
        .get_or_introspect(&pool, &schema_refs)
        .await
        .expect("introspection failed");

    info!(
        tables = introspection.classes.len(),
        "introspection complete"
    );

    // 8. Build resource registry from introspection.
    let registry = PgResourceRegistry::from_introspection(&introspection);

    // 9. Gather phase — produce IR from introspection + registry + preset.
    let gather_output = gather(&introspection, &registry, &preset).expect("gather failed");

    info!(
        resources = gather_output.resources.len(),
        relations = gather_output.relations.len(),
        "gather complete"
    );

    // 10. Build GraphQL schema from gathered output.
    // store_cache = None: the binary runs against live Postgres, not local stores.
    // extensions = &[]: this binary is intentionally domain-neutral — it serves
    // a generic Postgres-derived schema. Fellwork-specific resolvers are wired
    // by `apps/api` via `fw_resolvers::FellworkExtension`. When this crate
    // becomes the public `magna` binary, consumers wire their own extensions.
    let schema =
        build_schema(&gather_output, &gather_output.behaviors, pool.clone(), &[]).expect("build_schema failed");

    info!(
        extensions = "none",
        "graphql schema built (domain-neutral mode — no SchemaExtensions wired). \
         If you expected Fellwork-specific fields like depthInsights or wordGraph, \
         use apps/api instead, which wires fw_resolvers::FellworkExtension."
    );

    // 11. Start subscription manager (background task).
    match PgSubscriptionManager::new(pool.clone()).await {
        Ok(manager) => {
            tokio::spawn(manager.run());
            info!("subscription manager started");
        }
        Err(e) => {
            warn!(error = %e, "failed to start subscription manager — subscriptions disabled");
        }
    }

    // 12. Create AppState.
    let state = AppState::new(pool.clone(), schema, &preset);

    // 13. Spawn reload watcher (background task).
    let reload_state = state.clone();
    let reload_cache = Arc::clone(&cache);
    let reload_pool = pool.clone();
    let reload_schemas = preset.pg_schemas.clone();
    tokio::spawn(async move {
        watch_for_reload(reload_state, reload_cache, reload_pool, reload_schemas).await;
    });

    // 14. Build router and serve.
    let router = build_router(state);

    let parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    info!(port = config.port, parallelism, "serving");

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port))
        .await
        .expect("failed to bind listener");

    axum::serve(listener, router)
        .await
        .expect("server error");
}

/// Background task that listens for schema reload notifications and hot-swaps
/// the GraphQL schema without downtime.
async fn watch_for_reload(
    state: AppState,
    cache: Arc<IntrospectionCache>,
    pool: sqlx::PgPool,
    pg_schemas: Vec<String>,
) {
    let mut rx = cache.subscribe_reload();

    loop {
        match rx.recv().await {
            Ok(target) => {
                info!(target = %target, "schema reload triggered — rebuilding");

                // Invalidate the cache.
                cache.invalidate_all().await;

                // Re-introspect.
                let schema_refs: Vec<&str> = pg_schemas.iter().map(|s| s.as_str()).collect();
                let introspection = match cache.get_or_introspect(&pool, &schema_refs).await {
                    Ok(i) => i,
                    Err(e) => {
                        error!(error = %e, "re-introspection failed — keeping current schema");
                        continue;
                    }
                };

                // Re-gather.
                let registry = PgResourceRegistry::from_introspection(&introspection);
                let preset = fw_graph_config::Preset::default();
                let gather_output = match gather(&introspection, &registry, &preset) {
                    Ok(o) => o,
                    Err(e) => {
                        error!(error = %e, "re-gather failed — keeping current schema");
                        continue;
                    }
                };

                // Re-build schema.
                // store_cache = None: hot-reload stays on live Postgres.
                let new_schema =
                    match build_schema(&gather_output, &gather_output.behaviors, pool.clone(), &[]) {
                        Ok(s) => s,
                        Err(e) => {
                            error!(error = %e, "re-build_schema failed — keeping current schema");
                            continue;
                        }
                    };

                // Hot-swap the schema and clear the plan cache.
                state.schema_registry.reload(new_schema).await;
                state.plan_cache.clear();

                let gen = state.schema_registry.generation().await;
                info!(generation = gen, "schema reloaded successfully");
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                warn!(missed = n, "reload watcher lagged — processing latest");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                info!("reload channel closed — watcher exiting");
                break;
            }
        }
    }
}
