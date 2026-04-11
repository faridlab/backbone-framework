//! Basic usage example for backbone-cache
//! Demonstrates fundamental caching operations

use backbone_cache::{RedisCache, MemoryCache, CacheKey};
use serde::{Serialize, Deserialize};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct User {
    id: String,
    name: String,
    email: String,
    age: u32,
    active: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Session {
    id: String,
    user_id: String,
    created_at: chrono::DateTime<chrono::Utc>,
    last_activity: chrono::DateTime<chrono::Utc>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Backbone Cache Basic Usage Example ===\n");

    // 1. Setup Memory Cache
    println!("🗄️  Setting up Memory Cache");
    let memory_cache = MemoryCache::new(Some(1000)); // 1000 entries max
    println!("✅ Memory cache created with 1000 max entries\n");

    // 2. Setup Redis Cache
    println!("🔗  Setting up Redis Cache");
    let redis_cache = RedisCache::new("redis://localhost:6379").await
        .map_err(|e| println!("⚠️  Redis not available, using memory only: {}", e))?;

    // 3. Basic Cache Operations
    println!("📦 Basic Cache Operations");

    // Create test user
    let user = User {
        id: "123".to_string(),
        name: "John Doe".to_string(),
        email: "john@example.com".to_string(),
        age: 30,
        active: true,
    };

    // Store in memory cache
    println!("📝 Storing user in memory cache");
    memory_cache.set("user:123", &user, Some(3600)).await?; // 1 hour TTL
    println!("✅ User stored in memory cache with 1 hour TTL\n");

    // Store in Redis cache (if available)
    if let Ok(redis_cache) = &redis_cache {
        println!("📝 Storing user in Redis cache");
        redis_cache.set("user:123", &user, Some(7200)).await?; // 2 hours TTL
        println!("✅ User stored in Redis cache with 2 hours TTL\n");
    }

    // 4. Retrieve from Cache
    println!("🔍 Retrieving from Cache");

    let cached_user: Option<User> = memory_cache.get("user:123").await?;
    match cached_user {
        Some(user) => {
            println!("✅ Retrieved user from memory cache: {} ({} years old)", user.name, user.age);
        }
        None => {
            println!("❌ User not found in memory cache");
        }
    }
    println!();

    // 5. Cache Key Management
    println!("🔑 Cache Key Management");

    // Built-in key builders
    let user_key = CacheKey::user("456");
    let session_key = CacheKey::session("abc123");
    let api_key = CacheKey::api_response("/api/users", "page=1");

    println!("🏷️  User key: {}", user_key);
    println!("🎫 Session key: {}", session_key);
    println!("🌐 API response key: {}", api_key);

    // Store using built-in keys
    let session = Session {
        id: "abc123".to_string(),
        user_id: "123".to_string(),
        created_at: chrono::Utc::now(),
        last_activity: chrono::Utc::now(),
    };

    memory_cache.set(&session_key, &session, Some(1800)).await?;
    println!("✅ Session cached with 30 minute TTL\n");

    // 6. Cache Existence and TTL
    println!("🔍 Cache Existence and TTL");

    // Check if key exists
    let user_exists = memory_cache.exists("user:123").await?;
    println!("📋 User exists in cache: {}", user_exists);

    let session_exists = memory_cache.exists(&session_key).await?;
    println!("📋 Session exists in cache: {}", session_exists);

    // Check TTL
    let user_ttl = memory_cache.ttl("user:123").await?;
    match user_ttl {
        Some(seconds) => println!("⏰ User expires in {} seconds", seconds),
        None => println!("⏰ User has no expiration"),
    }
    println!();

    // 7. Cache Statistics
    println!("📊 Cache Statistics");

    let stats = memory_cache.stats().await?;
    println!("📈 Cache Statistics:");
    println!("   Total Entries: {}", stats.total_entries);
    println!("   Hit Rate: {:.2}%", stats.hit_rate * 100.0);
    println!("   Hits: {}", stats.hits);
    println!("   Misses: {}", stats.misses);
    println!("   Sets: {}", stats.sets);
    println!("   Deletes: {}", stats.deletes);
    println!();

    // 8. Batch Operations
    println!("📦 Batch Operations");

    // Create multiple users
    let users = vec![
        ("user:200", User {
            id: "200".to_string(),
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
            age: 25,
            active: true,
        }),
        ("user:201", User {
            id: "201".to_string(),
            name: "Bob".to_string(),
            email: "bob@example.com".to_string(),
            age: 32,
            active: true,
        }),
        ("user:202", User {
            id: "202".to_string(),
            name: "Charlie".to_string(),
            email: "charlie@example.com".to_string(),
            age: 28,
            active: false,
        }),
    ];

    // Batch store
    println!("📝 Storing {} users in batch", users.len());
    let entries: Vec<(String, User, Option<u64>)] = users
        .into_iter()
        .map(|(key, user, ttl)| (key.to_string(), user, Some(ttl.unwrap_or(1800))))
        .collect();

    memory_cache.mset(entries).await?;
    println!("✅ All users stored in batch\n");

    // Batch retrieve
    println!("🔍 Retrieving users in batch");
    let keys: Vec<String> = users.iter().map(|(key, _, _)| key.to_string()).collect();
    let results = memory_cache.mget::<User>(keys).await?;

    for (key, user) in results {
        match user {
            Some(user_data) => println!("✅ {}: {}", key, user_data.name),
            None => println!("❌ {}: Not found", key),
        }
    }
    println!();

    // 9. TTL Expiration Demo
    println!("⏰ TTL Expiration Demo");

    // Store with short TTL for demonstration
    let temp_user = User {
        id: "999".to_string(),
        name: "Temporary User".to_string(),
        email: "temp@example.com".to_string(),
        age: 99,
        active: false,
    };

    println!("📝 Storing temp user with 3 second TTL");
    memory_cache.set("temp_user", &temp_user, Some(3)).await?;

    println!("🔍 Checking immediately...");
    let immediate_check: Option<User> = memory_cache.get("temp_user").await?;
    println!("   Immediate check: {}", if immediate_check.is_some() { "Found" } else { "Not found" });

    println!("⏳ Waiting 4 seconds for expiration...");
    sleep(Duration::from_secs(4)).await;

    println!("🔍 Checking after expiration...");
    let expired_check: Option<User> = memory_cache.get("temp_user").await?;
    println!("   After expiration: {}", if expired_check.is_some() { "Found" } else { "Not found (expected)" });
    println!();

    // 10. Key Overwriting Demo
    println!("🔄 Key Overwriting Demo");

    // Store original user
    let original_user = user.clone();
    println!("📝 Storing original user: {}", original_user.name);
    memory_cache.set("user:123", &original_user, None).await?;

    // Retrieve original
    let retrieved_original: Option<User> = memory_cache.get("user:123").await?;
    println!("   Retrieved: {}", retrieved_original.as_ref().map(|u| u.name.as_str()).unwrap_or("None"));

    // Update user
    let mut updated_user = original_user.clone();
    updated_user.name = "John Smith".to_string();
    updated_user.age = 31;

    println!("📝 Overwriting with updated user: {}", updated_user.name);
    memory_cache.set("user:123", &updated_user, None).await?;

    // Retrieve updated
    let retrieved_updated: Option<User> = memory_cache.get("user:123").await?;
    println!("   Retrieved: {}", retrieved_updated.as_ref().map(|u| u.name.as_str()).unwrap_or("None"));
    println!();

    // 11. Delete Operations
    println!("🗑️  Delete Operations");

    // Check before delete
    let before_delete = memory_cache.exists("user:123").await?;
    println!("📋 User 123 exists before delete: {}", before_delete);

    // Delete the user
    let deleted = memory_cache.delete("user:123").await?;
    println!("🗑️  Delete operation result: {}", if deleted { "Success" } else { "Not found" });

    // Check after delete
    let after_delete = memory_cache.exists("user:123").await?;
    println!("📋 User 123 exists after delete: {}", after_delete);
    println!();

    // 12. Clear Cache
    println!("🧹 Clear Cache");

    // Get stats before clear
    let stats_before = memory_cache.stats().await?;
    println!("📊 Stats before clear:");
    println!("   Total entries: {}", stats_before.total_entries);
    println!("   Hits: {}", stats_before.hits);
    println!("   Misses: {}", stats_before.misses);

    // Clear cache
    memory_cache.clear().await?;
    println!("🧹 Cache cleared");

    // Get stats after clear
    let stats_after = memory_cache.stats().await?;
    println!("📊 Stats after clear:");
    println!("   Total entries: {}", stats_after.total_entries);
    println!("   Hits: {}", stats_after.hits);
    println!("   Misses: {}", stats_after.misses);
    println!();

    // 13. Error Handling
    println!("⚠️  Error Handling");

    // Try to get non-existent key
    let missing_user: Option<User> = memory_cache.get("nonexistent:key").await?;
    println!("🔍 Missing user lookup: {:?}", missing_user);

    // Invalid TTL
    let invalid_ttl_result = memory_cache.ttl("invalid:key").await?;
    println!("🔍 Invalid TTL check: {:?}", invalid_ttl_result);
    println!();

    println!("=== Basic Usage Example Complete ===");
    println!("🎉 All basic cache operations demonstrated successfully!");

    // Final statistics
    let final_stats = memory_cache.stats().await?;
    println!("\n📊 Final Cache Statistics:");
    println!("   Total Operations: {}", final_stats.hits + final_stats.misses);
    println!("   Success Rate: {:.2}%", final_stats.hit_rate * 100.0);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_memory_cache_operations() {
        let cache = MemoryCache::new(None);
        let test_data = "test_value";

        // Test set and get
        cache.set("test_key", test_data, None).await.unwrap();
        let retrieved: Option<String> = cache.get("test_key").await.unwrap();
        assert_eq!(retrieved, Some(test_data.to_string()));

        // Test exists
        assert!(cache.exists("test_key").await.unwrap());

        // Test delete
        assert!(cache.delete("test_key").await.unwrap());
        assert!(!cache.exists("test_key").await.unwrap());
    }

    #[tokio::test]
    async fn test_ttl_functionality() {
        let cache = MemoryCache::new(None);

        // Set with TTL
        cache.set("ttl_test", "expires_soon", Some(1)).await.unwrap();

        // Should exist immediately
        assert!(cache.exists("ttl_test").await.unwrap());

        // Check TTL
        let ttl = cache.ttl("ttl_test").await.unwrap();
        assert!(ttl.is_some());
        assert!(ttl.unwrap() > 0);
    }

    #[tokio::test]
    async fn test_key_builder_utilities() {
        let user_key = CacheKey::user("123");
        assert_eq!(user_key, "user:123");

        let session_key = CacheKey::session("abc123");
        assert_eq!(session_key, "session:abc123");

        let api_key = CacheKey::api_response("/api/users", "page=1");
        assert_eq!(api_key, "api:/api/users:page=1");
    }

    #[tokio::test]
    async fn test_serialization_roundtrip() {
        let cache = MemoryCache::new(None);

        let original = User {
            id: "test".to_string(),
            name: "Test User".to_string(),
            email: "test@example.com".to_string(),
            age: 25,
            active: true,
        };

        // Store and retrieve
        cache.set("roundtrip_test", &original, None).await.unwrap();
        let retrieved: Option<User> = cache.get("roundtrip_test").await.unwrap();

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, original.id);
        assert_eq!(retrieved.name, original.name);
        assert_eq!(retrieved.email, original.email);
        assert_eq!(retrieved.age, original.age);
        assert_eq!(retrieved.active, original.active);
    }
}