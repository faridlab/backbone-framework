//! Cache Module Tests

use backbone_cache::{MemoryCache, CacheConfig, CacheKey, CacheError, CacheResult};
use backbone_cache::traits::{Cache, CacheEntry, CacheStats};
use serde::{Serialize, Deserialize};
use std::time::Duration;
use tokio::time::sleep;
use chrono::Utc;

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

#[tokio::test]
async fn test_memory_cache_basic_operations() -> CacheResult<()> {
    let cache = MemoryCache::new(Some(100));

    // Test data
    let user = TestUser::new("1", "John Doe", "john@example.com", 30);
    let key = "user:1";

    // Set operation
    cache.set(key, &user, Some(3600)).await?;

    // Get operation
    let cached_user: Option<TestUser> = cache.get(key).await?;
    assert!(cached_user.is_some(), "User should be found in cache");
    assert_eq!(cached_user.unwrap(), user, "Cached user should match original");

    // Exists operation
    assert!(cache.exists(key).await?, "Key should exist");

    // Delete operation
    let deleted = cache.delete(key).await?;
    assert!(deleted, "Delete should return true for existing key");

    // Verify deletion
    let cached_user: Option<TestUser> = cache.get(key).await?;
    assert!(cached_user.is_none(), "User should be deleted from cache");
    assert!(!cache.exists(key).await?, "Key should not exist after deletion");

    Ok(())
}

#[tokio::test]
async fn test_memory_cache_ttl_expiration() -> CacheResult<()> {
    let cache = MemoryCache::new(None);

    let user = TestUser::new("2", "Jane Smith", "jane@example.com", 25);
    let key = "user:2";

    // Set with very short TTL (1 second)
    cache.set(key, &user, Some(1)).await?;

    // Should exist immediately
    assert!(cache.exists(key).await?, "Key should exist immediately");
    let cached_user: Option<TestUser> = cache.get(key).await?;
    assert!(cached_user.is_some(), "User should be retrievable immediately");

    // Wait for expiration
    sleep(Duration::from_secs(2)).await;

    // Should be expired now
    assert!(!cache.exists(key).await?, "Key should not exist after TTL expiration");
    let cached_user: Option<TestUser> = cache.get(key).await?;
    assert!(cached_user.is_none(), "User should not be retrievable after expiration");

    Ok(())
}

#[tokio::test]
async fn test_memory_cache_no_expiration() -> CacheResult<()> {
    let cache = MemoryCache::new(None);

    let user = TestUser::new("3", "Bob Wilson", "bob@example.com", 40);
    let key = "user:3";

    // Set without TTL (no expiration)
    cache.set(key, &user, None).await?;

    // Should exist indefinitely (at least for our test)
    assert!(cache.exists(key).await?, "Key should exist without TTL");

    // Wait and verify it still exists
    sleep(Duration::from_millis(100)).await;
    assert!(cache.exists(key).await?, "Key should still exist after delay");

    Ok(())
}

#[tokio::test]
async fn test_memory_cache_max_entries_eviction() -> CacheResult<()> {
    // Create cache with max 3 entries
    let cache = MemoryCache::new(Some(3));

    // Fill cache to capacity
    for i in 1..=3 {
        let user = TestUser::new(&i.to_string(), &format!("User {}", i), "user@example.com", i as u32);
        cache.set(&format!("user:{}", i), &user, None).await?;
    }

    // Verify all entries exist
    for i in 1..=3 {
        let key = format!("user:{}", i);
        assert!(cache.exists(&key).await?, "User {} should exist", i);
    }

    // Add one more entry (should trigger eviction)
    let user4 = TestUser::new("4", "User 4", "user4@example.com", 4);
    cache.set("user:4", &user4, None).await?;

    // At least one of the original entries should be evicted
    let mut existing_count = 0;
    for i in 1..=3 {
        let key = format!("user:{}", i);
        if cache.exists(&key).await? {
            existing_count += 1;
        }
    }

    assert!(existing_count < 3, "At least one entry should be evicted");
    assert!(cache.exists("user:4").await?, "New entry should exist");

    Ok(())
}

#[tokio::test]
async fn test_memory_cache_lru_eviction() -> CacheResult<()> {
    // Create cache with max 2 entries
    let cache = MemoryCache::new(Some(2));

    let user1 = TestUser::new("1", "User 1", "user1@example.com", 1);
    let user2 = TestUser::new("2", "User 2", "user2@example.com", 2);

    // Add two entries
    cache.set("user:1", &user1, None).await?;
    cache.set("user:2", &user2, None).await?;

    // Access user1 to make it recently used
    let _cached_user: Option<TestUser> = cache.get("user:1").await?;

    // Add third entry (should evict user2 as LRU)
    let user3 = TestUser::new("3", "User 3", "user3@example.com", 3);
    cache.set("user:3", &user3, None).await?;

    // User1 should still exist (recently accessed)
    assert!(cache.exists("user:1").await?, "Recently accessed user should exist");

    // User2 should be evicted (least recently used)
    assert!(!cache.exists("user:2").await?, "LRU user should be evicted");

    // User3 should exist (newly added)
    assert!(cache.exists("user:3").await?, "New user should exist");

    Ok(())
}

#[tokio::test]
async fn test_memory_cache_multi_operations() -> CacheResult<()> {
    let cache = MemoryCache::new(None);

    // Prepare test data
    let users = vec![
        ("user:1", TestUser::new("1", "User 1", "user1@example.com", 1)),
        ("user:2", TestUser::new("2", "User 2", "user2@example.com", 2)),
        ("user:3", TestUser::new("3", "User 3", "user3@example.com", 3)),
    ];

    // Test mset (multiple set)
    let mset_entries: Vec<(String, TestUser, Option<u64>)> = users
        .iter()
        .map(|(key, user)| (key.to_string(), user.clone(), Some(3600)))
        .collect();
    cache.mset(mset_entries).await?;

    // Test mget (multiple get)
    let keys = vec!["user:1".to_string(), "user:2".to_string(), "user:4".to_string()];
    let results = cache.mget::<TestUser>(keys).await?;

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].0, "user:1");
    assert!(results[0].1.is_some());
    assert_eq!(results[0].1.as_ref().unwrap().id, "1");

    assert_eq!(results[1].0, "user:2");
    assert!(results[1].1.is_some());
    assert_eq!(results[1].1.as_ref().unwrap().id, "2");

    assert_eq!(results[2].0, "user:4");
    assert!(results[2].1.is_none()); // Non-existent key

    // Test mdelete (multiple delete)
    let delete_keys = vec!["user:1".to_string(), "user:3".to_string()];
    let deleted_count = cache.mdelete(delete_keys).await?;
    assert_eq!(deleted_count, 2);

    // Verify deletions
    assert!(!cache.exists("user:1").await?, "user:1 should be deleted");
    assert!(cache.exists("user:2").await?, "user:2 should still exist");
    assert!(!cache.exists("user:3").await?, "user:3 should be deleted");

    Ok(())
}

#[tokio::test]
async fn test_memory_cache_ttl_operations() -> CacheResult<()> {
    let cache = MemoryCache::new(None);

    let user = TestUser::new("1", "Test User", "test@example.com", 30);
    let key = "user:1";

    // Set with 10 second TTL
    cache.set(key, &user, Some(10)).await?;

    // Check initial TTL
    let ttl = cache.ttl(key).await?;
    assert!(ttl.is_some(), "TTL should be set");
    assert!(ttl.unwrap() > 0, "TTL should be positive");
    assert!(ttl.unwrap() <= 10, "TTL should not exceed initial value");

    // Update TTL to 5 seconds
    let updated = cache.expire(key, 5).await?;
    assert!(updated, "TTL update should succeed");

    // Check updated TTL
    let ttl = cache.ttl(key).await?;
    assert!(ttl.is_some(), "TTL should still be set");
    assert!(ttl.unwrap() <= 5, "TTL should reflect updated value");

    // Test TTL on non-existent key
    let ttl = cache.ttl("nonexistent").await?;
    assert!(ttl.is_none(), "TTL should be None for non-existent key");

    // Test expire on non-existent key
    let updated = cache.expire("nonexistent", 10).await?;
    assert!(!updated, "Expire should fail for non-existent key");

    Ok(())
}

#[tokio::test]
async fn test_memory_cache_statistics() -> CacheResult<()> {
    let cache = MemoryCache::new(None);

    // Initial stats
    let stats = cache.stats().await?;
    assert_eq!(stats.hits, 0);
    assert_eq!(stats.misses, 0);
    assert_eq!(stats.sets, 0);
    assert_eq!(stats.deletes, 0);
    assert_eq!(stats.total_entries, 0);
    assert_eq!(stats.hit_rate, 0.0);

    let user = TestUser::new("1", "Test User", "test@example.com", 30);

    // Set operation
    cache.set("user:1", &user, None).await?;
    let stats = cache.stats().await?;
    assert_eq!(stats.sets, 1);
    assert_eq!(stats.total_entries, 1);

    // Hit operation
    let _cached_user: Option<TestUser> = cache.get("user:1").await?;
    let stats = cache.stats().await?;
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 0);
    assert!(stats.hit_rate > 0.0);

    // Miss operation
    let _cached_user: Option<TestUser> = cache.get("nonexistent").await?;
    let stats = cache.stats().await?;
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 1);
    assert!(stats.hit_rate < 1.0);

    // Delete operation
    cache.delete("user:1").await?;
    let stats = cache.stats().await?;
    assert_eq!(stats.deletes, 1);
    assert_eq!(stats.total_entries, 0);

    // Clear operation
    cache.set("user:2", &user, None).await?;
    cache.set("user:3", &user, None).await?;
    cache.clear().await?;
    let stats = cache.stats().await?;
    assert_eq!(stats.total_entries, 0);

    Ok(())
}

#[tokio::test]
async fn test_memory_cache_clear() -> CacheResult<()> {
    let cache = MemoryCache::new(None);

    // Add some entries
    for i in 1..=5 {
        let user = TestUser::new(&i.to_string(), &format!("User {}", i), "user@example.com", i as u32);
        cache.set(&format!("user:{}", i), &user, None).await?;
    }

    // Verify entries exist
    let stats = cache.stats().await?;
    assert_eq!(stats.total_entries, 5);

    // Clear cache
    cache.clear().await?;

    // Verify all entries are gone
    let stats = cache.stats().await?;
    assert_eq!(stats.total_entries, 0);

    for i in 1..=5 {
        let key = format!("user:{}", i);
        assert!(!cache.exists(&key).await?, "Key {} should not exist after clear", i);
    }

    Ok(())
}

#[test]
fn test_cache_key_builder() {
    // Test basic key building
    assert_eq!(CacheKey::build("user", "123"), "user:123");
    assert_eq!(CacheKey::build("session", "abc123"), "session:abc123");

    // Test specific key builders
    assert_eq!(CacheKey::user("123"), "user:123");
    assert_eq!(CacheKey::session("abc123"), "session:abc123");
    assert_eq!(CacheKey::api_response("/users", "page=1"), "api:/users:page=1");
}

#[test]
fn test_cache_config_default() {
    let config = CacheConfig::default();
    assert_eq!(config.default_ttl, 3600);
    assert_eq!(config.max_memory_entries, Some(10000));
    assert_eq!(config.redis_pool_size, Some(10));
    assert!(config.key_prefix.is_none());
}

#[test]
fn test_cache_config_custom() {
    let config = CacheConfig {
        default_ttl: 7200,
        max_memory_entries: Some(5000),
        redis_pool_size: Some(20),
        key_prefix: Some("test".to_string()),
    };

    assert_eq!(config.default_ttl, 7200);
    assert_eq!(config.max_memory_entries, Some(5000));
    assert_eq!(config.redis_pool_size, Some(20));
    assert_eq!(config.key_prefix.as_ref().unwrap(), "test");
}

#[test]
fn test_cache_entry_creation() {
    let now = Utc::now();
    let data = "test data".to_string();

    // Entry without TTL
    let entry = CacheEntry::new(data.clone(), None);
    assert_eq!(entry.data, data);
    assert!(entry.expires_at.is_none());
    assert_eq!(entry.access_count, 0);
    assert!(entry.last_accessed.is_none());
    assert!(!entry.is_expired());

    // Entry with TTL
    let entry = CacheEntry::new(data.clone(), Some(3600));
    assert_eq!(entry.data, data);
    assert!(entry.expires_at.is_some());
    assert!(entry.expires_at.unwrap() > now);
    assert!(!entry.is_expired());

    // Entry with past TTL (expired)
    let past_time = now - chrono::Duration::seconds(3600);
    let mut entry = CacheEntry::new(data, Some(3600));
    entry.expires_at = Some(past_time);
    assert!(entry.is_expired());
}

#[test]
fn test_cache_entry_access_tracking() {
    let mut entry = CacheEntry::new("test data".to_string(), None);

    // Initial state
    assert_eq!(entry.access_count, 0);
    assert!(entry.last_accessed.is_none());

    // Mark as accessed
    entry.mark_accessed();
    assert_eq!(entry.access_count, 1);
    assert!(entry.last_accessed.is_some());

    // Mark as accessed again
    entry.mark_accessed();
    assert_eq!(entry.access_count, 2);
}

#[test]
fn test_cache_error_types() {
    let error = CacheError::RedisConnection("Connection failed".to_string());
    assert!(error.to_string().contains("Redis connection error"));

    let error = CacheError::RedisOperation("Operation failed".to_string());
    assert!(error.to_string().contains("Redis operation error"));

    let error = CacheError::Serialization("Serialization failed".to_string());
    assert!(error.to_string().contains("Serialization error"));

    let error = CacheError::Deserialization("Deserialization failed".to_string());
    assert!(error.to_string().contains("Deserialization error"));

    let error = CacheError::NotFound("key not found".to_string());
    assert!(error.to_string().contains("Cache key not found"));

    let error = CacheError::Other("General error".to_string());
    assert!(error.to_string().contains("Cache error"));
}

#[test]
fn test_cache_stats_default_and_updates() {
    let mut stats = CacheStats::default();

    assert_eq!(stats.total_entries, 0);
    assert_eq!(stats.expired_entries, 0);
    assert_eq!(stats.hits, 0);
    assert_eq!(stats.misses, 0);
    assert_eq!(stats.sets, 0);
    assert_eq!(stats.deletes, 0);
    assert_eq!(stats.hit_rate, 0.0);

    // Test recording operations
    stats.record_hit();
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.hit_rate, 1.0);

    stats.record_miss();
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.hit_rate, 0.5);

    stats.record_set();
    assert_eq!(stats.sets, 1);

    stats.record_delete();
    assert_eq!(stats.deletes, 1);

    // Test hit rate calculation edge case
    let mut stats = CacheStats::default();
    stats.record_miss(); // Only misses, no hits
    assert_eq!(stats.hit_rate, 0.0); // Should not panic with division by zero
}

#[tokio::test]
async fn test_cache_complex_data_types() -> CacheResult<()> {
    let cache = MemoryCache::new(None);

    // Test with complex nested data
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct ComplexData {
        id: u32,
        name: String,
        tags: Vec<String>,
        metadata: std::collections::HashMap<String, String>,
        created_at: chrono::DateTime<chrono::Utc>,
    }

    let mut metadata = std::collections::HashMap::new();
    metadata.insert("source".to_string(), "test".to_string());
    metadata.insert("version".to_string(), "1.0".to_string());

    let complex_data = ComplexData {
        id: 123,
        name: "Complex Item".to_string(),
        tags: vec!["tag1".to_string(), "tag2".to_string(), "tag3".to_string()],
        metadata,
        created_at: Utc::now(),
    };

    cache.set("complex:1", &complex_data, Some(3600)).await?;

    let retrieved: Option<ComplexData> = cache.get("complex:1").await?;
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), complex_data);

    Ok(())
}

#[tokio::test]
async fn test_cache_concurrent_access() -> CacheResult<()> {
    let cache = std::sync::Arc::new(MemoryCache::new(None));
    let mut handles = vec![];

    // Spawn multiple concurrent tasks
    for i in 0..10 {
        let cache_clone = cache.clone();
        let handle = tokio::spawn(async move {
            let key = format!("key:{}", i);
            let value = format!("value:{}", i);

            // Set
            cache_clone.set(&key, &value, None).await.unwrap();

            // Get
            let retrieved: Option<String> = cache_clone.get(&key).await.unwrap();
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap(), value);

            // Delete
            let deleted = cache_clone.delete(&key).await.unwrap();
            assert!(deleted);
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify cache is empty
    let stats = cache.stats().await?;
    assert_eq!(stats.total_entries, 0);

    Ok(())
}

#[tokio::test]
async fn test_cache_error_handling() -> CacheResult<()> {
    let cache = MemoryCache::new(None);

    // Test with invalid data (this should work with serde)
    let valid_data = TestUser::new("1", "Valid User", "valid@example.com", 30);
    cache.set("valid", &valid_data, None).await?;

    // Test retrieving with wrong type (should fail gracefully)
    let result: CacheResult<Option<String>> = cache.get("valid").await;
    match result {
        Ok(_) => {
            // This might succeed if serde can convert, or fail if it can't
            // The important thing is that it doesn't panic
        }
        Err(CacheError::Deserialization(_)) => {
            // Expected error for type mismatch
        }
        Err(_) => {
            // Unexpected error type
            panic!("Expected deserialization error or success");
        }
    }

    Ok(())
}