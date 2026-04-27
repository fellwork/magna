//! `magna-remote` — Tier 3 of magna's four-tier extension surface.
//!
//! Tier 3 lets you back a single GraphQL field with an HTTP webhook. The
//! engine `POST`s a JSON payload of `{ args, context, parent }` to your
//! configured URL, awaits the JSON response, and returns it as the field's
//! resolved value. The auth tokens of the originating request are forwarded
//! by default; each call is wrapped in a `tracing` span so you can see
//! upstream latency in your observability stack.
//!
//! This crate is **experimental** at v0.1. The wire format and the
//! `RemoteResolver` config schema may change in any minor version of the
//! 0.x track. v1.0 freezes both.
//!
//! # Use case
//!
//! You have an auto-generated GraphQL API over your Postgres database, but
//! one field needs to call an external service (a weather API, a price
//! oracle, a feature-flag client). Writing a Rust plugin (Tier 1) is the
//! right answer if you control the call site and want to compose with
//! type-safe resolvers. But if your field is owned by a different team or
//! a different language, an HTTP webhook is the simpler path.
//!
//! Tier 3 is also the bridge for `magna-wasm` (Tier 4, v0.5): WASM plugins
//! and HTTP webhooks share the same wire format, just different transports.
//!
//! # Status
//!
//! This crate compiles and exposes the `RemoteExtension` skeleton needed
//! to wire YAML-configured webhook resolvers into a magna schema. The
//! request transport (`reqwest`-based POST), response decoding, and span
//! integration are scaffolded; the full implementation is targeted for
//! the 0.1.x patch series. See `docs/feature-matrix.md` for the current
//! support level and `docs/extension-guide.md` (forthcoming) for the
//! configuration schema.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors emitted by the webhook resolver pipeline.
#[derive(Debug, Error)]
pub enum RemoteError {
    /// Network-level failure: connection refused, timeout, TLS error.
    #[error("transport error calling webhook: {0}")]
    Transport(#[from] reqwest::Error),

    /// Webhook returned a non-2xx status with a parseable error body.
    #[error("webhook returned status {status}: {body}")]
    Status { status: u16, body: String },

    /// Webhook returned 2xx but the JSON body did not match the declared
    /// `returns` shape on the field.
    #[error("webhook response decode failed: {0}")]
    Decode(String),

    /// The configured URL is malformed.
    #[error("invalid webhook URL: {0}")]
    InvalidUrl(String),
}

/// Configuration for a single remote (webhook-backed) resolver.
///
/// One `RemoteResolver` corresponds to exactly one GraphQL field. The
/// engine reads a list of these from `magna.yaml` and registers each as
/// a Query / Mutation field that calls the webhook on resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteResolver {
    /// The fully-qualified field name this resolver backs, e.g.
    /// `Query.weather` or `Mutation.sendPasswordReset`.
    pub field: String,

    /// The URL the engine `POST`s to. Must be `https://` in production;
    /// `http://` is allowed in development for local-only testing.
    pub url: String,

    /// The schema of the JSON args object the field accepts. v0.1 only
    /// supports a flat scalar map; nested types and input-object args
    /// land with the v0.2 schema-builder integration.
    pub args_schema: serde_json::Value,

    /// The GraphQL return type, written as a type-ref string
    /// (e.g. `"WeatherReport"`, `"[Order!]!"`). The engine validates the
    /// webhook's response against this type at resolution time.
    pub returns: String,

    /// Optional per-call timeout. Defaults to 5 seconds.
    #[serde(default = "default_timeout")]
    pub timeout: Duration,

    /// If true (default), forward the originating request's
    /// `Authorization` header to the webhook. Set to `false` for fields
    /// that should run as the webhook's own service identity.
    #[serde(default = "default_forward_auth")]
    pub forward_auth: bool,
}

fn default_timeout() -> Duration {
    Duration::from_secs(5)
}

fn default_forward_auth() -> bool {
    true
}

/// The wire-format envelope sent to every webhook.
#[derive(Debug, Serialize)]
pub struct WebhookRequest<'a> {
    /// The args the GraphQL caller passed to the field.
    pub args: &'a serde_json::Value,
    /// The request context: at minimum the GraphQL operation name and
    /// the resolved auth principal, if any. v0.1 does not yet propagate
    /// the full async-graphql `Context`; that lands with the schema-builder
    /// integration.
    pub context: serde_json::Value,
    /// The parent value when this field is nested inside another type.
    /// `null` for top-level Query/Mutation fields.
    pub parent: Option<&'a serde_json::Value>,
}

/// Helper to construct a webhook client with sensible defaults.
///
/// Connection pooling is enabled by default; the HTTP/2 transport is used
/// when the webhook supports it. TLS uses rustls (no system OpenSSL needed).
pub fn build_client() -> Result<reqwest::Client, RemoteError> {
    reqwest::Client::builder()
        .user_agent(concat!("magna-remote/", env!("CARGO_PKG_VERSION")))
        .pool_idle_timeout(Some(Duration::from_secs(30)))
        .build()
        .map_err(RemoteError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_resolver_round_trips_through_json() {
        // YAML is the configured format in `magna.yaml`; here we use the
        // serde_json equivalent to keep the dep set minimal during v0.1
        // skeleton stage. The `magna-config` crate handles the YAML
        // surface and deserializes into this same struct.
        let parsed: RemoteResolver = serde_json::from_str(
            r#"{
                "field": "Query.weather",
                "url": "https://example.com/resolver",
                "args_schema": {"city": {"type": "String!"}},
                "returns": "WeatherReport"
            }"#,
        )
        .expect("parse should succeed");
        assert_eq!(parsed.field, "Query.weather");
        assert_eq!(parsed.returns, "WeatherReport");
        assert_eq!(parsed.timeout, default_timeout());
        assert!(parsed.forward_auth);
    }

    #[test]
    fn build_client_succeeds_with_defaults() {
        let _client = build_client().expect("client builds with default config");
    }
}
