//! Integration tests for magna-introspect.
//!
//! These tests require a running Postgres instance and are marked `#[ignore]`
//! by default. Run them with:
//!
//! ```sh
//! DATABASE_URL=postgres://... cargo test -p magna-introspect -- --ignored
//! ```

use magna_introspect::{introspect, IntrospectionCache};
use std::sync::Arc;

/// Helper to get a PgPool from DATABASE_URL.
async fn test_pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests");
    sqlx::PgPool::connect(&url).await.expect("failed to connect to database")
}

#[tokio::test]
#[ignore]
async fn introspect_public_schema() {
    let pool = test_pool().await;
    let result = introspect(&pool, &["public"]).await.expect("introspection failed");

    // The public schema should always exist.
    assert!(!result.namespaces.is_empty(), "expected at least one namespace");
    assert_eq!(result.namespaces[0].name, "public");
}

#[tokio::test]
#[ignore]
async fn introspect_returns_tables_and_columns() {
    let pool = test_pool().await;
    let result = introspect(&pool, &["public"]).await.expect("introspection failed");

    // If the database has any tables, we should see classes and attributes.
    if !result.classes.is_empty() {
        assert!(!result.attributes.is_empty(), "tables exist but no columns found");
    }
}

#[tokio::test]
#[ignore]
async fn introspect_returns_constraints() {
    let pool = test_pool().await;
    let result = introspect(&pool, &["public"]).await.expect("introspection failed");

    // If the database has any tables with primary keys, constraints should be present.
    for constraint in &result.constraints {
        // Every constraint should have a non-empty name.
        assert!(!constraint.name.is_empty());
    }
}

#[tokio::test]
#[ignore]
async fn cache_get_or_introspect() {
    let pool = test_pool().await;
    let cache = Arc::new(IntrospectionCache::new(300));

    // First call — cache miss, runs queries.
    let result1 = cache
        .get_or_introspect(&pool, &["public"])
        .await
        .expect("first introspection failed");

    // Second call — cache hit, should return the same Arc.
    let result2 = cache
        .get_or_introspect(&pool, &["public"])
        .await
        .expect("second introspection failed");

    assert!(Arc::ptr_eq(&result1, &result2), "expected cache hit to return same Arc");
}

#[tokio::test]
#[ignore]
async fn cache_invalidation_forces_re_introspect() {
    let pool = test_pool().await;
    let cache = Arc::new(IntrospectionCache::new(300));

    let result1 = cache
        .get_or_introspect(&pool, &["public"])
        .await
        .expect("first introspection failed");

    // Invalidate and re-introspect.
    cache.invalidate("public").await;

    let result2 = cache
        .get_or_introspect(&pool, &["public"])
        .await
        .expect("second introspection failed");

    // After invalidation, a new Arc should be created.
    assert!(!Arc::ptr_eq(&result1, &result2), "expected fresh result after invalidation");
}
