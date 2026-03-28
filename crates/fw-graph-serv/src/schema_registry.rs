//! Schema registry — holds the current live schema and supports hot-reload.

use std::sync::Arc;
use tokio::sync::RwLock;

/// A snapshot of a schema at a specific generation.
pub struct SchemaGeneration {
    /// Monotonically increasing counter; starts at 1 and increments on reload.
    pub generation: u64,
    /// The compiled dynamic GraphQL schema.
    pub schema: async_graphql::dynamic::Schema,
}

/// Thread-safe registry that holds the active [`SchemaGeneration`].
///
/// Callers acquire an `Arc<SchemaGeneration>` for the duration of a single
/// request; a concurrent reload swaps the inner Arc without invalidating
/// outstanding references.
#[derive(Clone)]
pub struct SchemaRegistry {
    current: Arc<RwLock<Arc<SchemaGeneration>>>,
}

impl SchemaRegistry {
    /// Create a new registry with `schema` at generation 1.
    pub fn new(schema: async_graphql::dynamic::Schema) -> Self {
        let gen = Arc::new(SchemaGeneration {
            generation: 1,
            schema,
        });
        Self {
            current: Arc::new(RwLock::new(gen)),
        }
    }

    /// Return a clone of the current [`SchemaGeneration`] Arc.
    ///
    /// The returned Arc keeps the generation alive for the request lifetime
    /// even if a concurrent reload has already swapped the inner pointer.
    pub async fn current_schema(&self) -> Arc<SchemaGeneration> {
        Arc::clone(&*self.current.read().await)
    }

    /// Replace the schema with `new_schema` and increment the generation counter.
    pub async fn reload(&self, new_schema: async_graphql::dynamic::Schema) {
        let mut guard = self.current.write().await;
        let next_gen = guard.generation + 1;
        *guard = Arc::new(SchemaGeneration {
            generation: next_gen,
            schema: new_schema,
        });
    }

    /// Return the current generation number.
    pub async fn generation(&self) -> u64 {
        self.current.read().await.generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::dynamic::{Field, FieldFuture, Object, Schema, TypeRef};

    fn test_schema() -> async_graphql::dynamic::Schema {
        let query = Object::new("Query").field(Field::new(
            "ok",
            TypeRef::named(TypeRef::BOOLEAN),
            |_| FieldFuture::from_value(Some(async_graphql::Value::Boolean(true))),
        ));
        Schema::build("Query", None, None)
            .register(query)
            .finish()
            .unwrap()
    }

    #[tokio::test]
    async fn new_registry_starts_at_generation_1() {
        let registry = SchemaRegistry::new(test_schema());
        assert_eq!(registry.generation().await, 1);
    }

    #[tokio::test]
    async fn reload_increments_generation() {
        let registry = SchemaRegistry::new(test_schema());

        registry.reload(test_schema()).await;
        assert_eq!(registry.generation().await, 2);

        registry.reload(test_schema()).await;
        assert_eq!(registry.generation().await, 3);
    }

    #[tokio::test]
    async fn current_schema_arc_stays_valid_after_reload() {
        let registry = SchemaRegistry::new(test_schema());

        // Acquire a reference at generation 1
        let old_gen = registry.current_schema().await;
        assert_eq!(old_gen.generation, 1);

        // Reload replaces the inner pointer
        registry.reload(test_schema()).await;

        // The new current is at generation 2 …
        assert_eq!(registry.generation().await, 2);

        // … but the old Arc we held is still accessible and correct
        assert_eq!(old_gen.generation, 1);
    }
}
