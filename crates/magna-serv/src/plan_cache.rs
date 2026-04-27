//! Plan cache — memoize `ExecutionPlan` instances keyed by operation hash.

use dashmap::DashMap;
use magna_core::ExecutionPlan;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Stable hash key for a GraphQL operation (document + optional operation name).
pub type OperationHash = u64;

/// Thread-safe LRU-style cache of compiled [`ExecutionPlan`]s.
///
/// When the cache reaches `max_size`, the oldest entry is evicted to make room
/// (first key in iteration order — deterministic enough for a compilation cache).
pub struct PlanCache {
    inner: Arc<DashMap<OperationHash, Arc<ExecutionPlan>>>,
    max_size: usize,
}

impl PlanCache {
    /// Create a new cache with the given maximum capacity.
    pub fn new(max_size: usize) -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            max_size,
        }
    }

    /// Return the cached plan for `hash`, or build one with `build` and cache it.
    ///
    /// If the cache is at capacity, one entry is evicted before inserting.
    pub fn get_or_plan(
        &self,
        hash: OperationHash,
        build: impl FnOnce() -> ExecutionPlan,
    ) -> Arc<ExecutionPlan> {
        if let Some(existing) = self.inner.get(&hash) {
            return Arc::clone(&existing);
        }

        // Evict if at capacity. We use retain() to remove exactly one entry
        // rather than iter() + remove(), because DashMap's iter() can hold
        // shard locks that deadlock with a subsequent remove() on Windows.
        if self.inner.len() >= self.max_size && self.max_size > 0 {
            let mut evicted = false;
            self.inner.retain(|_, _| {
                if evicted {
                    true // keep
                } else {
                    evicted = true;
                    false // remove this one
                }
            });
        }

        let plan = Arc::new(build());
        self.inner.insert(hash, Arc::clone(&plan));
        plan
    }

    /// Number of cached plans.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True when the cache holds no plans.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Remove all cached plans.
    pub fn clear(&self) {
        self.inner.clear();
    }
}

/// Normalize a GraphQL document string by collapsing all whitespace.
///
/// Two documents that differ only in whitespace will produce the same
/// normalized form and therefore the same hash.
pub fn normalize_operation(document: &str) -> String {
    document.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Hash a (document, operation_name) pair to a stable [`OperationHash`].
pub fn hash_operation(document: &str, operation_name: Option<&str>) -> OperationHash {
    let normalized = normalize_operation(document);
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    operation_name.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use magna_core::Planner;

    fn dummy_plan() -> ExecutionPlan {
        Planner::new(1).build().expect("empty planner should build")
    }

    #[test]
    fn cache_miss_builds_plan() {
        let cache = PlanCache::new(10);
        let hash = hash_operation("{ hello }", None);

        let mut built = false;
        let _plan = cache.get_or_plan(hash, || {
            built = true;
            dummy_plan()
        });

        assert!(built, "build closure should have been called on a miss");
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn cache_hit_returns_same_arc() {
        let cache = PlanCache::new(10);
        let hash = hash_operation("{ hello }", None);

        let plan1 = cache.get_or_plan(hash, dummy_plan);
        let plan2 = cache.get_or_plan(hash, || panic!("should not rebuild"));

        assert!(Arc::ptr_eq(&plan1, &plan2), "should return the same Arc");
    }

    #[test]
    fn eviction_when_full() {
        let cache = PlanCache::new(2);

        let h1 = hash_operation("{ a }", None);
        let h2 = hash_operation("{ b }", None);
        let h3 = hash_operation("{ c }", None);

        cache.get_or_plan(h1, dummy_plan);
        cache.get_or_plan(h2, dummy_plan);
        assert_eq!(cache.len(), 2);

        // Adding a third entry should evict one
        cache.get_or_plan(h3, dummy_plan);
        assert_eq!(cache.len(), 2, "cache should stay at max_size after eviction");
    }

    #[test]
    fn normalize_makes_whitespace_different_queries_hash_equal() {
        let doc1 = "{ hello   world }";
        let doc2 = "{\n  hello\n  world\n}";

        let h1 = hash_operation(doc1, None);
        let h2 = hash_operation(doc2, None);

        assert_eq!(h1, h2, "normalized documents should hash identically");
    }

    #[test]
    fn different_operation_names_produce_different_hashes() {
        let doc = "query Foo { hello } query Bar { hello }";

        let h_foo = hash_operation(doc, Some("Foo"));
        let h_bar = hash_operation(doc, Some("Bar"));
        let h_none = hash_operation(doc, None);

        assert_ne!(h_foo, h_bar);
        assert_ne!(h_foo, h_none);
        assert_ne!(h_bar, h_none);
    }

    #[test]
    fn clear_empties_cache() {
        let cache = PlanCache::new(10);

        cache.get_or_plan(hash_operation("{ a }", None), dummy_plan);
        cache.get_or_plan(hash_operation("{ b }", None), dummy_plan);
        assert!(!cache.is_empty());

        cache.clear();

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }
}
