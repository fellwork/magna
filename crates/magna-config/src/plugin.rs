//! Plugin trait for extending the magna schema build process.
//!
//! Plugins hook into either the gather phase (modifying introspection output
//! before schema generation) or the schema phase (adding custom types, fields,
//! or directives to the GraphQL schema). Plugins are declarative objects — they
//! describe what they do, not how to do it. The build system calls them in order.

use std::any::Any;
use std::fmt;

/// A plugin hooks into one or both build phases.
///
/// Plugins are stored as `Box<dyn Plugin>` inside a [`super::preset::Preset`],
/// so this trait must remain object-safe: no generic methods, no `Self: Sized`
/// bounds on provided methods.
///
/// # Lifecycle
///
/// 1. **Gather phase** — `gather_hook` is called with the raw introspection
///    output. Plugins may add virtual resources, change behaviors, rename
///    things, or add custom codecs.
/// 2. **Schema phase** — `schema_hook` is called with the schema builder and
///    the finalized gather output. Plugins may add custom types, fields,
///    directives, or scalars.
pub trait Plugin: Send + Sync {
    /// Human-readable name for logging and debugging (e.g. `"relay-plugin"`).
    fn name(&self) -> &str;

    /// Optional description for documentation / CLI output.
    fn description(&self) -> &str {
        ""
    }

    /// Called during the gather phase. Override to mutate gather output
    /// (add virtual resources, change behaviors, rename things, add codecs).
    ///
    /// The default implementation is a no-op so plugins may implement only
    /// the phase they care about.
    fn gather_hook(&self, _gather: &mut GatherContext) {
        // no-op by default
    }

    /// Called during the schema phase. Override to add custom types, fields,
    /// directives, or scalars to the GraphQL schema.
    ///
    /// The default implementation is a no-op.
    fn schema_hook(&self, _schema: &mut SchemaContext) {
        // no-op by default
    }

    /// Downcast helper — allows the build system to check plugin identity.
    fn as_any(&self) -> &dyn Any;
}

impl fmt::Debug for dyn Plugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Plugin")
            .field("name", &self.name())
            .finish()
    }
}

/// Context passed to [`Plugin::gather_hook`].
///
/// This is a thin wrapper that will be extended once `magna-introspect`
/// and `magna-dataplan` types are integrated into this crate.
#[derive(Debug, Default)]
pub struct GatherContext {
    /// Plugin-supplied key-value metadata that downstream phases can read.
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

/// Context passed to [`Plugin::schema_hook`].
///
/// This is a thin wrapper that will be extended once `async-graphql::dynamic`
/// schema builder integration is added in `magna-build`.
#[derive(Debug, Default)]
pub struct SchemaContext {
    /// Extra type definitions to register (type name -> SDL fragment).
    pub extra_type_defs: Vec<String>,
    /// Extra field definitions (parent type name, field SDL fragment).
    pub extra_fields: Vec<(String, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mock plugin for testing the trait.
    struct MockPlugin {
        plugin_name: String,
    }

    impl MockPlugin {
        fn new(name: &str) -> Self {
            Self {
                plugin_name: name.to_string(),
            }
        }
    }

    impl Plugin for MockPlugin {
        fn name(&self) -> &str {
            &self.plugin_name
        }

        fn description(&self) -> &str {
            "A mock plugin for testing"
        }

        fn gather_hook(&self, gather: &mut GatherContext) {
            gather.metadata.insert(
                self.plugin_name.clone(),
                serde_json::Value::Bool(true),
            );
        }

        fn schema_hook(&self, schema: &mut SchemaContext) {
            schema
                .extra_type_defs
                .push(format!("type {}Extension {{ id: ID! }}", self.plugin_name));
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[test]
    fn plugin_is_object_safe() {
        // Prove the trait is object-safe by storing in a Box<dyn Plugin>.
        let plugin: Box<dyn Plugin> = Box::new(MockPlugin::new("test-plugin"));
        assert_eq!(plugin.name(), "test-plugin");
        assert_eq!(plugin.description(), "A mock plugin for testing");
    }

    #[test]
    fn plugin_gather_hook() {
        let plugin = MockPlugin::new("relay");
        let mut ctx = GatherContext::default();
        plugin.gather_hook(&mut ctx);
        assert_eq!(ctx.metadata.get("relay"), Some(&serde_json::Value::Bool(true)));
    }

    #[test]
    fn plugin_schema_hook() {
        let plugin = MockPlugin::new("Audit");
        let mut ctx = SchemaContext::default();
        plugin.schema_hook(&mut ctx);
        assert_eq!(ctx.extra_type_defs.len(), 1);
        assert!(ctx.extra_type_defs[0].contains("AuditExtension"));
    }

    #[test]
    fn plugin_debug_format() {
        let plugin: Box<dyn Plugin> = Box::new(MockPlugin::new("debug-test"));
        let debug_str = format!("{:?}", plugin);
        assert!(debug_str.contains("debug-test"));
    }

    #[test]
    fn plugin_default_hooks_are_noop() {
        // A plugin that only implements name() and as_any() should not panic.
        struct MinimalPlugin;
        impl Plugin for MinimalPlugin {
            fn name(&self) -> &str { "minimal" }
            fn as_any(&self) -> &dyn Any { self }
        }

        let plugin: Box<dyn Plugin> = Box::new(MinimalPlugin);
        let mut gather = GatherContext::default();
        let mut schema = SchemaContext::default();
        plugin.gather_hook(&mut gather);
        plugin.schema_hook(&mut schema);
        assert!(gather.metadata.is_empty());
        assert!(schema.extra_type_defs.is_empty());
    }
}
