//! Redis integration tests for backbone-cache
//!
//! These tests require a running Redis instance and are marked `#[ignore]`.
//! Run with: `cargo test -p backbone-cache -- --ignored`

use backbone_cache::RedisCache;
use backbone_cache::traits::Cache;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

const REDIS_URL: &str = "redis://localhost:6379";
const TEST_PREFIX: &str = "test_cache";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct TestUser {
    id: String,
    name: String,
    email: String,
    age: u32,
}

impl TestUser {
    fn new(id: &str, name: &str, email: &str, age: u32) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            email: email.to_string(),
            age,
        }
    }
}

async fn setup_cache() -> RedisCache {
    let cache = RedisCache::with_config(REDIS_URL, Some(TEST_PREFIX.to_string()))
        .await
        .expect("Redis connection failed");
    cache.clear().await.ok(); // Best-effort cleanup
    cache
}

#[tokio::test]
#[ignore]
async fn test_redis_set_and_get() {
    let cache = setup_cache().await;
    let user = TestUser::new("1", "Alice", "alice@example.com", 30);

    cache.set("user:1", &user, Some(300)).await.unwrap();

    let cached: Option<TestUser> = cache.get("user:1").await.unwrap();
    assert_eq!(cached, Some(user));

    // Cleanup
    cache.delete("user:1").await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_redis_set_with_ttl_expiration() {
    let cache = setup_cache().await;
    let user = TestUser::new("2", "Bob", "bob@example.com", 25);

    cache.set("ttl_test", &user, Some(1)).await.unwrap();

    // Should exist immediately
    let cached: Option<TestUser> = cache.get("ttl_test").await.unwrap();
    assert!(cached.is_some(), "Should exist before expiration");

    // Wait for TTL to expire
    sleep(Duration::from_secs(2)).await;

    let expired: Option<TestUser> = cache.get("ttl_test").await.unwrap();
    assert!(expired.is_none(), "Should be expired after TTL");
}

#[tokio::test]
#[ignore]
async fn test_redis_set_without_ttl() {
    let cache = setup_cache().await;
    let user = TestUser::new("3", "Carol", "carol@example.com", 28);

    // Set without TTL (no expiration)
    cache.set("no_ttl", &user, None).await.unwrap();

    let cached: Option<TestUser> = cache.get("no_ttl").await.unwrap();
    assert_eq!(cached, Some(user));

    // Should have no TTL
    let ttl = cache.ttl("no_ttl").await.unwrap();
    assert_eq!(ttl, None, "No TTL should return None (-1 from Redis)");

    cache.delete("no_ttl").await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_redis_get_nonexistent() {
    let cache = setup_cache().await;

    let result: Option<TestUser> = cache.get("nonexistent_key").await.unwrap();
    assert!(result.is_none(), "Nonexistent key should return None");
}

#[tokio::test]
#[ignore]
async fn test_redis_delete() {
    let cache = setup_cache().await;
    let user = TestUser::new("4", "Dave", "dave@example.com", 35);

    cache.set("del_test", &user, Some(300)).await.unwrap();

    // Delete existing key
    let deleted = cache.delete("del_test").await.unwrap();
    assert!(deleted, "Deleting existing key should return true");

    // Delete nonexistent key
    let deleted = cache.delete("del_test").await.unwrap();
    assert!(!deleted, "Deleting nonexistent key should return false");
}

#[tokio::test]
#[ignore]
async fn test_redis_exists() {
    let cache = setup_cache().await;
    let user = TestUser::new("5", "Eve", "eve@example.com", 22);

    assert!(!cache.exists("exists_test").await.unwrap(), "Key should not exist initially");

    cache.set("exists_test", &user, Some(300)).await.unwrap();
    assert!(cache.exists("exists_test").await.unwrap(), "Key should exist after set");

    cache.delete("exists_test").await.unwrap();
    assert!(!cache.exists("exists_test").await.unwrap(), "Key should not exist after delete");
}

#[tokio::test]
#[ignore]
async fn test_redis_expire_and_ttl() {
    let cache = setup_cache().await;
    let user = TestUser::new("6", "Frank", "frank@example.com", 40);

    // Set with no TTL
    cache.set("ttl_ops", &user, None).await.unwrap();
    let ttl = cache.ttl("ttl_ops").await.unwrap();
    assert_eq!(ttl, None, "No-TTL key should return None");

    // Set expiration
    let expired = cache.expire("ttl_ops", 60).await.unwrap();
    assert!(expired, "expire() should return true for existing key");

    let ttl = cache.ttl("ttl_ops").await.unwrap();
    assert!(ttl.is_some(), "Should now have a TTL");
    assert!(ttl.unwrap() > 0 && ttl.unwrap() <= 60, "TTL should be between 1-60");

    // TTL of nonexistent key
    let ttl = cache.ttl("nonexistent_ttl").await.unwrap();
    assert_eq!(ttl, Some(0), "Nonexistent key TTL should be Some(0)");

    cache.delete("ttl_ops").await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_redis_clear_with_prefix() {
    let cache = setup_cache().await;

    // Set multiple keys
    cache.set("clear_a", &"value_a", Some(300)).await.unwrap();
    cache.set("clear_b", &"value_b", Some(300)).await.unwrap();
    cache.set("clear_c", &"value_c", Some(300)).await.unwrap();

    // Clear all keys with our prefix
    cache.clear().await.unwrap();

    // Verify all cleared
    let a: Option<String> = cache.get("clear_a").await.unwrap();
    let b: Option<String> = cache.get("clear_b").await.unwrap();
    let c: Option<String> = cache.get("clear_c").await.unwrap();
    assert!(a.is_none() && b.is_none() && c.is_none(), "All keys should be cleared");
}

#[tokio::test]
#[ignore]
async fn test_redis_clear_without_prefix_errors() {
    // Create cache without prefix
    let cache = RedisCache::new(REDIS_URL)
        .await
        .expect("Redis connection failed");

    // clear() should fail without prefix (safety check)
    let result = cache.clear().await;
    assert!(result.is_err(), "clear() without prefix should fail for safety");
}

#[tokio::test]
#[ignore]
async fn test_redis_stats() {
    let cache = setup_cache().await;

    // Set some data
    cache.set("stats_a", &"hello", Some(300)).await.unwrap();
    cache.set("stats_b", &"world", Some(300)).await.unwrap();

    let stats = cache.stats().await.unwrap();
    // Stats should have some data (memory_usage from INFO)
    // total_entries counts keys matching our prefix
    assert!(stats.total_entries >= 2, "Should have at least 2 entries");

    // Cleanup
    cache.clear().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_redis_mget_mset_mdelete() {
    let cache = setup_cache().await;

    let entries = vec![
        ("mop_a".to_string(), TestUser::new("a", "A", "a@test.com", 20), Some(300u64)),
        ("mop_b".to_string(), TestUser::new("b", "B", "b@test.com", 21), Some(300)),
        ("mop_c".to_string(), TestUser::new("c", "C", "c@test.com", 22), None),
    ];

    // mset
    cache.mset(entries).await.unwrap();

    // mget
    let keys = vec!["mop_a".to_string(), "mop_b".to_string(), "mop_c".to_string(), "mop_missing".to_string()];
    let results: Vec<(String, Option<TestUser>)> = cache.mget(keys).await.unwrap();

    assert_eq!(results.len(), 4);
    assert!(results[0].1.is_some(), "mop_a should exist");
    assert_eq!(results[0].1.as_ref().unwrap().name, "A");
    assert!(results[1].1.is_some(), "mop_b should exist");
    assert!(results[2].1.is_some(), "mop_c should exist");
    assert!(results[3].1.is_none(), "mop_missing should not exist");

    // mdelete
    let deleted = cache.mdelete(vec!["mop_a".to_string(), "mop_b".to_string()]).await.unwrap();
    assert_eq!(deleted, 2, "Should delete 2 keys");

    // Verify deletion
    let a: Option<TestUser> = cache.get("mop_a").await.unwrap();
    assert!(a.is_none(), "mop_a should be deleted");

    // Cleanup remaining
    cache.delete("mop_c").await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_redis_key_prefix_isolation() {
    // Two caches with different prefixes
    let cache_a = RedisCache::with_config(REDIS_URL, Some("prefix_a".to_string()))
        .await
        .expect("Redis connection failed");
    let cache_b = RedisCache::with_config(REDIS_URL, Some("prefix_b".to_string()))
        .await
        .expect("Redis connection failed");

    cache_a.clear().await.ok();
    cache_b.clear().await.ok();

    // Set same key in both
    cache_a.set("shared_key", &"value_a", Some(300)).await.unwrap();
    cache_b.set("shared_key", &"value_b", Some(300)).await.unwrap();

    // Each should see its own value
    let a: Option<String> = cache_a.get("shared_key").await.unwrap();
    let b: Option<String> = cache_b.get("shared_key").await.unwrap();
    assert_eq!(a, Some("value_a".to_string()));
    assert_eq!(b, Some("value_b".to_string()));

    // Clearing one should not affect the other
    cache_a.clear().await.unwrap();
    let a: Option<String> = cache_a.get("shared_key").await.unwrap();
    let b: Option<String> = cache_b.get("shared_key").await.unwrap();
    assert!(a.is_none(), "cache_a key should be cleared");
    assert!(b.is_some(), "cache_b key should still exist");

    cache_b.clear().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_redis_complex_types() {
    let cache = setup_cache().await;

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    struct ComplexData {
        tags: Vec<String>,
        metadata: HashMap<String, String>,
        score: f64,
        active: bool,
    }

    let data = ComplexData {
        tags: vec!["rust".into(), "cache".into(), "redis".into()],
        metadata: HashMap::from([
            ("env".into(), "test".into()),
            ("version".into(), "2.0".into()),
        ]),
        score: 95.5,
        active: true,
    };

    cache.set("complex", &data, Some(300)).await.unwrap();

    let cached: Option<ComplexData> = cache.get("complex").await.unwrap();
    assert_eq!(cached, Some(data));

    cache.delete("complex").await.unwrap();
}
