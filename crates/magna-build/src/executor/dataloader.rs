//! DataLoaderRegistry — per-relation DataLoaders keyed by "{resource}:{column}".
//!
//! async-graphql stores schema data by TypeId. Because all RelationDataLoader
//! instances share the same type, we store them all in one HashMap registry.

use std::collections::HashMap;
use std::sync::Arc;

use async_graphql::dataloader::{DataLoader, Loader};
use magna_types::{JwtClaims, PgRow};

use crate::executor::QueryExecutor;
use crate::ir::ResolvedResource;

// ── LoadKey ───────────────────────────────────────────────────────────────────

/// A string key passed into the DataLoader (parent PK value as string).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LoadKey(pub String);

// ── HasManyLoader ─────────────────────────────────────────────────────────────

/// DataLoader for hasMany relations: loads Vec<PgRow> for each parent PK key.
pub struct HasManyLoader {
    pub executor: Arc<QueryExecutor>,
    pub resource: ResolvedResource,
    pub fk_column: String,
    pub fk_column_oid: u32,
    pub claims: Option<Arc<JwtClaims>>,
    pub default_limit: i64,
}

impl Loader<LoadKey> for HasManyLoader {
    type Value = Vec<PgRow>;
    type Error = Arc<crate::executor::args::QueryError>;

    async fn load(&self, keys: &[LoadKey]) -> Result<HashMap<LoadKey, Self::Value>, Self::Error> {
        let ids: Vec<String> = keys.iter().map(|k| k.0.clone()).collect();
        let result = self
            .executor
            .select_by_fk_batch(
                self.claims.as_deref(),
                &self.resource,
                &self.fk_column,
                self.fk_column_oid,
                &ids,
                self.default_limit,
            )
            .await
            .map_err(Arc::new)?;

        Ok(keys
            .iter()
            .map(|k| {
                let rows = result.get(&k.0).cloned().unwrap_or_default();
                (k.clone(), rows)
            })
            .collect())
    }
}

// ── BelongsToLoader ───────────────────────────────────────────────────────────

/// DataLoader for belongsTo relations: loads a single PgRow per FK value.
pub struct BelongsToLoader {
    pub executor: Arc<QueryExecutor>,
    pub resource: ResolvedResource,
    pub pk_column: String,
    pub pk_column_oid: u32,
    pub claims: Option<Arc<JwtClaims>>,
}

impl Loader<LoadKey> for BelongsToLoader {
    type Value = PgRow;
    type Error = Arc<crate::executor::args::QueryError>;

    async fn load(&self, keys: &[LoadKey]) -> Result<HashMap<LoadKey, Self::Value>, Self::Error> {
        let ids: Vec<String> = keys.iter().map(|k| k.0.clone()).collect();
        let result = self
            .executor
            .select_by_pk_batch(
                self.claims.as_deref(),
                &self.resource,
                &self.pk_column,
                self.pk_column_oid,
                &ids,
            )
            .await
            .map_err(Arc::new)?;

        Ok(keys
            .iter()
            .filter_map(|k| result.get(&k.0).map(|row| (k.clone(), row.clone())))
            .collect())
    }
}

// ── DataLoaderRegistry ────────────────────────────────────────────────────────

/// A registry of DataLoaders keyed by relation identifier.
/// Key format: `"{TargetResource}:{lookup_column}"` e.g. `"Post:author_id"`
///
/// Stored once in async-graphql schema data. Resolvers look up their specific
/// loader by key to avoid TypeId collisions.
pub struct DataLoaderRegistry {
    has_many: HashMap<String, Arc<DataLoader<HasManyLoader>>>,
    belongs_to: HashMap<String, Arc<DataLoader<BelongsToLoader>>>,
}

impl DataLoaderRegistry {
    pub fn new() -> Self {
        Self {
            has_many: HashMap::new(),
            belongs_to: HashMap::new(),
        }
    }

    /// Register a hasMany DataLoader. Key: `"{SourceResource}:{fk_column}"`.
    pub fn register_has_many(
        &mut self,
        key: &str,
        executor: Arc<QueryExecutor>,
        resource: ResolvedResource,
        fk_column: String,
        fk_column_oid: u32,
        claims: Option<Arc<JwtClaims>>,
    ) {
        let loader = HasManyLoader {
            executor,
            resource,
            fk_column,
            fk_column_oid,
            claims,
            default_limit: 20,
        };
        self.has_many
            .insert(key.to_string(), Arc::new(DataLoader::new(loader, tokio::spawn)));
    }

    /// Register a belongsTo DataLoader. Key: `"{TargetResource}:{pk_column}"`.
    pub fn register_belongs_to(
        &mut self,
        key: &str,
        executor: Arc<QueryExecutor>,
        resource: ResolvedResource,
        pk_column: String,
        pk_column_oid: u32,
        claims: Option<Arc<JwtClaims>>,
    ) {
        let loader = BelongsToLoader {
            executor,
            resource,
            pk_column,
            pk_column_oid,
            claims,
        };
        self.belongs_to
            .insert(key.to_string(), Arc::new(DataLoader::new(loader, tokio::spawn)));
    }

    pub fn get_has_many(&self, key: &str) -> Option<Arc<DataLoader<HasManyLoader>>> {
        self.has_many.get(key).cloned()
    }

    pub fn get_belongs_to(&self, key: &str) -> Option<Arc<DataLoader<BelongsToLoader>>> {
        self.belongs_to.get(key).cloned()
    }
}

impl Default for DataLoaderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_starts_empty() {
        let reg = DataLoaderRegistry::new();
        assert!(reg.get_has_many("Post:author_id").is_none());
        assert!(reg.get_belongs_to("User:id").is_none());
    }
}
