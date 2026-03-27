//! Preset resolution — merge multiple presets and validate required fields.
//!
//! When composing configuration, later presets override earlier ones. This
//! follows the same semantics as PostGraphile V5: you start from a base
//! preset (usually [`Preset::default()`]) and layer project-specific
//! overrides on top.

use crate::plugin::Plugin;
use crate::preset::{PoolConfig, Preset, SchemaBuildOptions};

/// Errors that can occur during preset validation.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("no pg_schemas configured — at least one schema is required")]
    NoPgSchemas,

    #[error("no JWT secret or JWKS URL configured — set JWT_SECRET env var or provide jwt.secret / jwt.jwks_url")]
    NoJwtConfig,

    #[error("validation error: {0}")]
    Validation(String),
}

// We need thiserror, but the spec says to use workspace deps and it's already
// in workspace.dependencies. However, the user's requested deps are
// fw-graph-types, serde, serde_json, tracing. We'll define ResolveError
// manually to avoid adding thiserror as a dep if it's not in our Cargo.toml.
// Actually, let's just use a simple approach with std::fmt::Display.

/// A partial preset used for overrides. Every field is optional — only
/// fields that are `Some` will override the base preset during merge.
#[derive(Default)]
pub struct PresetOverride {
    pub pg_schemas: Option<Vec<String>>,
    pub default_role: Option<Option<String>>,
    pub jwt_secret: Option<String>,
    pub jwks_url: Option<Option<String>>,
    pub pool: Option<PoolConfig>,
    pub schema: Option<SchemaBuildOptions>,
    pub enable_subscriptions: Option<bool>,
    pub trusted_documents_only: Option<bool>,
    pub introspection_cache_ttl: Option<u64>,
    pub plugins: Option<Vec<Box<dyn Plugin>>>,
}

impl std::fmt::Debug for PresetOverride {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PresetOverride")
            .field("pg_schemas", &self.pg_schemas)
            .field("default_role", &self.default_role)
            .field("jwt_secret", &self.jwt_secret.as_ref().map(|_| "***"))
            .field("jwks_url", &self.jwks_url)
            .field("pool", &self.pool)
            .field("schema", &self.schema)
            .field("enable_subscriptions", &self.enable_subscriptions)
            .field("trusted_documents_only", &self.trusted_documents_only)
            .field("introspection_cache_ttl", &self.introspection_cache_ttl)
            .finish()
    }
}

/// Merge multiple overrides onto a base preset. Later overrides win.
///
/// Plugins are handled specially: if an override supplies a plugin list,
/// those plugins are *appended* to the existing list (not replaced), because
/// plugin ordering matters and composability requires additive behavior.
///
/// # Example
///
/// ```
/// use fw_graph_config::preset::Preset;
/// use fw_graph_config::resolve::{PresetOverride, merge};
///
/// let base = Preset::default();
/// let project_override = PresetOverride {
///     pg_schemas: Some(vec!["public".into(), "extensions".into()]),
///     trusted_documents_only: Some(true),
///     ..Default::default()
/// };
/// let merged = merge(base, &mut [project_override]);
/// assert_eq!(merged.pg_schemas, vec!["public", "extensions"]);
/// assert!(merged.trusted_documents_only);
/// ```
pub fn merge(mut base: Preset, overrides: &mut [PresetOverride]) -> Preset {
    for ov in overrides.iter_mut() {
        if let Some(schemas) = ov.pg_schemas.take() {
            base.pg_schemas = schemas;
        }
        if let Some(role) = ov.default_role.take() {
            base.default_role = role;
        }
        if let Some(secret) = ov.jwt_secret.take() {
            base.jwt.secret = secret;
        }
        if let Some(jwks) = ov.jwks_url.take() {
            base.jwt.jwks_url = jwks;
        }
        if let Some(pool) = ov.pool.take() {
            base.pool = pool;
        }
        if let Some(schema) = ov.schema.take() {
            base.schema = schema;
        }
        if let Some(subs) = ov.enable_subscriptions.take() {
            base.enable_subscriptions = subs;
        }
        if let Some(td) = ov.trusted_documents_only.take() {
            base.trusted_documents_only = td;
        }
        if let Some(ttl) = ov.introspection_cache_ttl.take() {
            base.introspection_cache_ttl = ttl;
        }
        if let Some(mut plugins) = ov.plugins.take() {
            // Additive: append new plugins after existing ones.
            base.plugins.append(&mut plugins);
        }
    }
    base
}

/// Validate that a preset has all required fields for a working deployment.
///
/// Returns `Ok(())` if valid, or a list of errors describing what is missing.
pub fn validate(preset: &Preset) -> Result<(), Vec<ResolveError>> {
    let mut errors = Vec::new();

    if preset.pg_schemas.is_empty() {
        errors.push(ResolveError::NoPgSchemas);
    }

    if preset.jwt.secret.is_empty() && preset.jwt.jwks_url.is_none() {
        errors.push(ResolveError::NoJwtConfig);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;

    struct TestPlugin(String);
    impl Plugin for TestPlugin {
        fn name(&self) -> &str { &self.0 }
        fn as_any(&self) -> &dyn Any { self }
    }

    #[test]
    fn merge_overrides_pg_schemas() {
        let base = Preset::default();
        let mut overrides = vec![PresetOverride {
            pg_schemas: Some(vec!["public".into(), "app".into()]),
            ..Default::default()
        }];
        let merged = merge(base, &mut overrides);
        assert_eq!(merged.pg_schemas, vec!["public", "app"]);
    }

    #[test]
    fn merge_later_override_wins() {
        let base = Preset::default();
        let mut overrides = vec![
            PresetOverride {
                pg_schemas: Some(vec!["first".into()]),
                ..Default::default()
            },
            PresetOverride {
                pg_schemas: Some(vec!["second".into()]),
                ..Default::default()
            },
        ];
        let merged = merge(base, &mut overrides);
        assert_eq!(merged.pg_schemas, vec!["second"]);
    }

    #[test]
    fn merge_preserves_base_when_no_override() {
        let base = Preset::default();
        let mut overrides = vec![PresetOverride {
            trusted_documents_only: Some(true),
            ..Default::default()
        }];
        let merged = merge(base, &mut overrides);
        // pg_schemas should still be the default
        assert_eq!(merged.pg_schemas, vec!["public"]);
        assert!(merged.trusted_documents_only);
    }

    #[test]
    fn merge_appends_plugins() {
        let mut base = Preset::default();
        base.plugins.push(Box::new(TestPlugin("base-plugin".into())));

        let mut overrides = vec![PresetOverride {
            plugins: Some(vec![Box::new(TestPlugin("override-plugin".into()))]),
            ..Default::default()
        }];
        let merged = merge(base, &mut overrides);
        assert_eq!(merged.plugins.len(), 2);
        assert_eq!(merged.plugins[0].name(), "base-plugin");
        assert_eq!(merged.plugins[1].name(), "override-plugin");
    }

    #[test]
    fn merge_overrides_jwt_secret() {
        let base = Preset::default();
        let mut overrides = vec![PresetOverride {
            jwt_secret: Some("my-super-secret".into()),
            ..Default::default()
        }];
        let merged = merge(base, &mut overrides);
        assert_eq!(merged.jwt.secret, "my-super-secret");
    }

    #[test]
    fn merge_overrides_pool() {
        let base = Preset::default();
        let custom_pool = PoolConfig {
            max_connections: 50,
            ..Default::default()
        };
        let mut overrides = vec![PresetOverride {
            pool: Some(custom_pool),
            ..Default::default()
        }];
        let merged = merge(base, &mut overrides);
        assert_eq!(merged.pool.max_connections, 50);
    }

    #[test]
    fn merge_overrides_schema_build_options() {
        let base = Preset::default();
        let custom_schema = SchemaBuildOptions {
            default_mutations: false,
            relay: false,
            ..Default::default()
        };
        let mut overrides = vec![PresetOverride {
            schema: Some(custom_schema),
            ..Default::default()
        }];
        let merged = merge(base, &mut overrides);
        assert!(!merged.schema.default_mutations);
        assert!(!merged.schema.relay);
        assert!(merged.schema.subscriptions); // kept from custom_schema default
    }

    #[test]
    fn merge_overrides_introspection_cache_ttl() {
        let base = Preset::default();
        let mut overrides = vec![PresetOverride {
            introspection_cache_ttl: Some(120),
            ..Default::default()
        }];
        let merged = merge(base, &mut overrides);
        assert_eq!(merged.introspection_cache_ttl, 120);
    }

    #[test]
    fn validate_default_preset_fails_without_jwt() {
        // Default preset reads JWT_SECRET from env — in test it's likely empty.
        let preset = Preset::default();
        // If JWT_SECRET isn't set, validation should fail.
        if preset.jwt.secret.is_empty() {
            let result = validate(&preset);
            assert!(result.is_err());
            let errors = result.unwrap_err();
            assert!(errors.iter().any(|e| matches!(e, ResolveError::NoJwtConfig)));
        }
    }

    #[test]
    fn validate_passes_with_jwt_secret() {
        let mut preset = Preset::default();
        preset.jwt.secret = "test-secret".to_string();
        let result = validate(&preset);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_passes_with_jwks_url() {
        let mut preset = Preset::default();
        preset.jwt.jwks_url = Some("https://example.com/.well-known/jwks.json".into());
        let result = validate(&preset);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_fails_with_empty_schemas() {
        let mut preset = Preset::default();
        preset.jwt.secret = "test-secret".to_string();
        preset.pg_schemas.clear();
        let result = validate(&preset);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(e, ResolveError::NoPgSchemas)));
    }
}
