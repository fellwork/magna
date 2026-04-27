//! CLI configuration — maps command-line flags and environment variables into
//! a [`magna_config::Preset`].

use clap::Parser;
use magna_config::{JwtConfig, PoolConfig, Preset};

/// magna — GraphQL-from-Postgres server.
#[derive(Parser, Debug)]
#[command(name = "magna", version, about)]
pub struct Config {
    /// Port to bind the HTTP server on.
    #[arg(long, env = "FW_GRAPH_PORT", default_value_t = 4800)]
    pub port: u16,

    /// PostgreSQL connection string.
    #[arg(long, env = "DATABASE_URL")]
    pub database_url: String,

    /// HMAC secret for JWT verification (HS256).
    #[arg(long, env = "JWT_SECRET", default_value = "")]
    pub jwt_secret: String,

    /// Comma-separated list of Postgres schemas to expose.
    #[arg(long, env = "PG_SCHEMAS", default_value = "public")]
    pub pg_schemas: String,

    /// Default Postgres role for unauthenticated requests.
    #[arg(long, env = "DEFAULT_ROLE", default_value = "anon")]
    pub default_role: String,

    /// Maximum number of connections in the pool.
    #[arg(long, env = "POOL_MAX", default_value_t = 10)]
    pub pool_max: u32,

    /// How long to cache introspection results (seconds).
    #[arg(long, env = "INTROSPECTION_CACHE_TTL", default_value_t = 300)]
    pub introspection_cache_ttl: u64,
}

impl Config {
    /// Convert CLI config into a [`Preset`] suitable for all magna components.
    pub fn to_preset(&self) -> Preset {
        let pg_schemas: Vec<String> = self
            .pg_schemas
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Preset {
            pg_schemas,
            default_role: Some(self.default_role.clone()),
            jwt: JwtConfig {
                secret: self.jwt_secret.clone(),
                jwks_url: None,
            },
            pool: PoolConfig {
                max_connections: self.pool_max,
                ..PoolConfig::default()
            },
            introspection_cache_ttl: self.introspection_cache_ttl,
            ..Preset::default()
        }
    }
}
