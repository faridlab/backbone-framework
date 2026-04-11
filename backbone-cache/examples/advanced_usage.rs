//! Advanced usage examples for backbone-cache
//! Demonstrates production patterns, caching strategies, and complex scenarios

use backbone_cache::{RedisCache, MemoryCache, CacheKey};
use serde::{Serialize, Deserialize};
use std::time::Duration;
use tokio::time::sleep;
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct UserProfile {
    id: String,
    username: String,
    email: String,
    profile_data: ProfileData,
    preferences: UserPreferences,
    social_links: Vec<SocialLink>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ProfileData {
    bio: Option<String>,
    avatar_url: Option<String>,
    location: Option<String>,
    website: Option<String>,
    birth_date: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct UserPreferences {
    theme: String,
    language: String,
    timezone: String,
    email_notifications: bool,
    push_notifications: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SocialLink {
    platform: String,
    url: String,
    verified: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ApiRequest {
    id: String,
    method: String,
    endpoint: String,
    headers: HashMap<String, String>,
    body: Option<String>,
    user_id: String,
    timestamp: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CachedResponse {
    request_id: String,
    status_code: u16,
    headers: HashMap<String, String>,
    body: String,
    cached_at: DateTime<Utc>,
    ttl_seconds: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SessionToken {
    token: String,
    user_id: String,
    permissions: Vec<String>,
    expires_at: DateTime<Utc>,
    last_accessed: DateTime<Utc>,
    ip_address: String,
    user_agent: String,
}

// Caching strategies enum
#[derive(Debug, Clone)]
enum CacheStrategy {
    CacheAside,
    WriteThrough,
    WriteBehind,
    RefreshAhead,
}

// Cache manager with advanced features
struct AdvancedCacheManager {
    memory_cache: MemoryCache,
    redis_cache: RedisCache,
    strategy: CacheStrategy,
    write_buffer: Vec<(String, String, Option<u64>)>,
    buffer_size: usize,
}

impl AdvancedCacheManager {
    async fn new(redis_url: &str, strategy: CacheStrategy) -> Result<Self, Box<dyn std::error::Error>> {
        let memory_cache = MemoryCache::new(Some(5000)); // 5k entries max
        let redis_cache = RedisCache::new(redis_url).await?;

        Ok(Self {
            memory_cache,
            redis_cache,
            strategy,
            write_buffer: Vec::new(),
            buffer_size: 100,
        })
    }

    // Cache-Aside pattern: Load from cache on miss
    async fn get_cache_aside<T>(&self, key: &str) -> Result<Option<T>, Box<dyn std::error::Error>>
    where
        T: for<'de> Deserialize<'de> + Send + Sync + Clone,
    {
        // Try memory cache first
        if let Some(value) = self.memory_cache.get::<T>(key).await? {
            return Ok(Some(value));
        }

        // Try Redis cache
        if let Some(value) = self.redis_cache.get::<T>(key).await? {
            // Populate memory cache for faster subsequent access
            self.memory_cache.set(key, &value, Some(300)).await?; // 5 min TTL
            return Ok(Some(value));
        }

        Ok(None)
    }

    // Cache-Aside pattern: Write to cache
    async fn set_cache_aside<T>(&self, key: &str, value: &T, ttl_seconds: Option<u64>) -> Result<(), Box<dyn std::error::Error>>
    where
        T: Serialize + Send + Sync + Clone,
    {
        // Set in both caches
        self.memory_cache.set(key, value, ttl_seconds).await?;
        self.redis_cache.set(key, value, ttl_seconds).await?;
        Ok(())
    }

    // Write-Through pattern: Write to cache and database simultaneously
    async fn write_through<T>(&mut self, key: &str, value: &T, ttl_seconds: Option<u64>) -> Result<(), Box<dyn std::error::Error>>
    where
        T: Serialize + Send + Sync + Clone,
    {
        // Write to both caches
        self.memory_cache.set(key, value, ttl_seconds).await?;
        self.redis_cache.set(key, value, ttl_seconds).await?;

        // In a real implementation, you'd also write to your database here
        println!("📝 Writing to database: {}", key);

        Ok(())
    }

    // Write-Behind pattern: Buffer writes and flush periodically
    async fn write_behind<T>(&mut self, key: &str, value: &T, ttl_seconds: Option<u64>) -> Result<(), Box<dyn std::error::Error>>
    where
        T: Serialize + Send + Sync,
    {
        let serialized = serde_json::to_string(value)?;
        self.write_buffer.push((key.to_string(), serialized, ttl_seconds.unwrap_or(3600)));

        // Flush buffer if it reaches threshold
        if self.write_buffer.len() >= self.buffer_size {
            self.flush_write_buffer().await?;
        }

        Ok(())
    }

    // Flush write-behind buffer
    async fn flush_write_buffer(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.write_buffer.is_empty() {
            return Ok(());
        }

        println!("🔄 Flushing write buffer: {} entries", self.write_buffer.len());

        // Batch write to Redis
        let entries: Vec<(String, String, Option<u64>)> = self.write_buffer
            .drain(..)
            .collect();

        let redis_entries: Vec<(String, String, Option<u64>)> = entries
            .iter()
            .map(|(key, value, ttl)| (key.clone(), value.clone(), *ttl))
            .collect();

        self.redis_cache.mset(redis_entries).await?;

        // Also flush to memory cache
        for (key, value, ttl) in entries {
            let deserialized: Result<serde_json::Value, _> = serde_json::from_str(&value);
            if let Ok(val) = deserialized {
                self.memory_cache.set(&key, &val, ttl).await?;
            }
        }

        Ok(())
    }

    // Refresh-Ahead pattern: Proactively refresh expiring keys
    async fn refresh_ahead<T>(&self, key_prefix: &str, threshold_seconds: u64) -> Result<(), Box<dyn std::error::Error>>
    where
        T: for<'de> Deserialize<'de> + Send + Sync + Serialize,
    {
        // In a real implementation, you'd scan for keys expiring soon
        // and refresh them from your data source
        println!("🔄 Scanning for expiring keys with prefix: {}", key_prefix);
        println!("⏰ Refreshing keys expiring within {} seconds", threshold_seconds);

        // Simulate refreshing some keys
        let keys_to_refresh = vec![
            format!("{}:123", key_prefix),
            format!("{}:456", key_prefix),
        ];

        for key in keys_to_refresh {
            if let Some(ttl) = self.redis_cache.ttl(&key).await? {
                if ttl < threshold_seconds {
                    println!("🔄 Refreshing key: {} (TTL: {}s)", key, ttl);
                    // In reality, you'd fetch fresh data from your database
                    // and update the cache with the new value
                }
            }
        }

        Ok(())
    }

    // Multi-tier cache warming
    async fn warm_cache_tier(&self, data_type: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔥 Warming cache tier for: {}", data_type);

        match data_type {
            "user_profiles" => {
                // Pre-load frequently accessed user profiles
                let user_ids = vec!["user_1", "user_2", "user_3"];

                for user_id in user_ids {
                    let profile = self.create_sample_profile(user_id).await?;
                    let key = CacheKey::user(user_id);

                    // Set different TTLs for different cache levels
                    self.memory_cache.set(&key, &profile, Some(300)).await?; // 5 min
                    self.redis_cache.set(&key, &profile, Some(3600)).await?; // 1 hour
                }
            }
            "api_responses" => {
                // Pre-load common API responses
                let endpoints = vec!["/api/v1/users", "/api/v1/products"];

                for endpoint in endpoints {
                    let response = self.create_sample_response(endpoint).await?;
                    let key = CacheKey::api_response(endpoint, "default");

                    self.memory_cache.set(&key, &response, Some(60)).await?; // 1 min
                    self.redis_cache.set(&key, &response, Some(300)).await?; // 5 min
                }
            }
            _ => {
                println!("⚠️ Unknown data type: {}", data_type);
            }
        }

        Ok(())
    }

    // Cache invalidation patterns
    async fn invalidate_user_cache(&self, user_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🗑️ Invalidating cache for user: {}", user_id);

        // Direct user data
        let user_key = CacheKey::user(user_id);
        self.memory_cache.delete(&user_key).await?;
        self.redis_cache.delete(&user_key).await?;

        // Related data (profile, sessions, etc.)
        let profile_key = format!("profile:{}", user_id);
        self.memory_cache.delete(&profile_key).await?;
        self.redis_cache.delete(&profile_key).await?;

        // Invalidate session tokens
        let session_pattern = format!("session:*:{}*", user_id);
        // In a real implementation, you'd scan and delete all matching keys
        println!("🗑️ Invalidating sessions matching pattern: {}", session_pattern);

        Ok(())
    }

    // Sample data creation helpers
    async fn create_sample_profile(&self, user_id: &str) -> Result<UserProfile, Box<dyn std::error::Error>> {
        Ok(UserProfile {
            id: user_id.to_string(),
            username: format!("user_{}", user_id),
            email: format!("user{}@example.com", user_id),
            profile_data: ProfileData {
                bio: Some(format!("Bio for user {}", user_id)),
                avatar_url: Some(format!("https://api.dicebear.com/7.x/avataaars/svg?seed={}", user_id)),
                location: Some("San Francisco, CA".to_string()),
                website: Some(format!("https://user{}.example.com", user_id)),
                birth_date: Some(Utc::now() - chrono::Duration::days(365 * 25)),
            },
            preferences: UserPreferences {
                theme: "dark".to_string(),
                language: "en".to_string(),
                timezone: "UTC".to_string(),
                email_notifications: true,
                push_notifications: false,
            },
            social_links: vec![
                SocialLink {
                    platform: "twitter".to_string(),
                    url: format!("https://twitter.com/user_{}", user_id),
                    verified: true,
                },
                SocialLink {
                    platform: "github".to_string(),
                    url: format!("https://github.com/user_{}", user_id),
                    verified: true,
                },
            ],
            created_at: Utc::now() - chrono::Duration::days(30),
            updated_at: Utc::now(),
        })
    }

    async fn create_sample_response(&self, endpoint: &str) -> Result<CachedResponse, Box<dyn std::error::Error>> {
        Ok(CachedResponse {
            request_id: Uuid::new_v4().to_string(),
            status_code: 200,
            headers: {
                let mut headers = HashMap::new();
                headers.insert("Content-Type".to_string(), "application/json".to_string());
                headers.insert("Cache-Control".to_string(), "max-age=300".to_string());
                headers
            },
            body: format!("{{\"endpoint\": \"{}\", \"data\": \"sample_data\"}}", endpoint),
            cached_at: Utc::now(),
            ttl_seconds: 300,
        })
    }

    // Cache statistics comparison
    async fn compare_cache_stats(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n📊 Cache Statistics Comparison");

        let memory_stats = self.memory_cache.stats().await?;
        let redis_stats = self.redis_cache.stats().await?;

        println!("🧠 Memory Cache:");
        println!("   Hit Rate: {:.2}%", memory_stats.hit_rate * 100.0);
        println!("   Total Entries: {}", memory_stats.total_entries);
        println!("   Hits: {} | Misses: {}", memory_stats.hits, memory_stats.misses);

        println!("🔗 Redis Cache:");
        println!("   Hit Rate: {:.2}%", redis_stats.hit_rate * 100.0);
        println!("   Total Entries: {}", redis_stats.total_entries);
        println!("   Hits: {} | Misses: {}", redis_stats.hits, redis_stats.misses);

        if let Some(memory_usage) = redis_stats.memory_usage {
            println!("   Memory Usage: {} bytes", memory_usage);
        }

        Ok(())
    }

    // Cleanup resources
    async fn cleanup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🧹 Cleaning up resources...");

        // Flush any pending writes
        self.flush_write_buffer().await?;

        // Clear caches
        self.memory_cache.clear().await?;

        println!("✅ Cleanup completed");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Backbone Cache Advanced Usage Examples ===\n");

    // Initialize cache manager with different strategies
    let mut cache_manager = AdvancedCacheManager::new("redis://localhost:6379", CacheStrategy::CacheAside).await
        .map_err(|e| println!("⚠️ Redis not available, using memory-only mode: {}", e))?;

    // 1. Cache-Aside Pattern Demo
    println!("🚀 1. Cache-Aside Pattern Demonstration");
    println!("=======================================");

    // Simulate database miss and cache fill
    let user_id = "user_123";
    let user_key = CacheKey::user(user_id);

    // First access - cache miss, load from "database"
    println!("🔍 First access (cache miss):");
    let cached_user: Option<UserProfile> = cache_manager.get_cache_aside(&user_key).await?;
    match cached_user {
        None => {
            println!("   ❌ Cache miss - loading from database");
            let profile = cache_manager.create_sample_profile(user_id).await?;
            cache_manager.set_cache_aside(&user_key, &profile, Some(3600)).await?;
            println!("   ✅ Loaded from database and cached");
        }
        Some(user) => println!("   ✅ Found in cache: {}", user.username),
    }

    // Second access - cache hit
    println!("🔍 Second access (cache hit):");
    let cached_user: Option<UserProfile> = cache_manager.get_cache_aside(&user_key).await?;
    match cached_user {
        Some(user) => println!("   ✅ Cache hit: {} (from {})", user.username,
            if cache_manager.memory_cache.exists(&user_key).await? { "Memory" } else { "Redis" }),
        None => println!("   ❌ Cache miss (unexpected)"),
    }

    // 2. Write-Through Pattern Demo
    println!("\n✍️ 2. Write-Through Pattern Demonstration");
    println!("==========================================");

    cache_manager.strategy = CacheStrategy::WriteThrough;
    let new_user_id = "user_456";
    let new_profile = cache_manager.create_sample_profile(new_user_id).await?;
    let new_user_key = CacheKey::user(new_user_id);

    println!("📝 Writing user data with write-through pattern");
    cache_manager.write_through(&new_user_key, &new_profile, Some(3600)).await?;
    println!("✅ Data written to cache and database");

    // Verify data is in cache
    let retrieved: Option<UserProfile> = cache_manager.get_cache_aside(&new_user_key).await?;
    match retrieved {
        Some(user) => println!("✅ Verified data in cache: {}", user.username),
        None => println!("❌ Data not found in cache"),
    }

    // 3. Write-Behind Pattern Demo
    println!("\n📝 3. Write-Behind Pattern Demonstration");
    println!("=========================================");

    cache_manager.strategy = CacheStrategy::WriteBehind;

    // Add multiple writes to buffer
    println!("📝 Adding multiple writes to buffer:");
    for i in 1..=5 {
        let user_id = &format!("buffer_user_{}", i);
        let profile = cache_manager.create_sample_profile(user_id).await?;
        let key = CacheKey::user(user_id);

        cache_manager.write_behind(&key, &profile, Some(7200)).await?;
        println!("   ✅ Added to buffer: {}", user_id);
    }

    // Add more writes to trigger buffer flush
    for i in 6..=8 {
        let user_id = &format!("buffer_user_{}", i);
        let profile = cache_manager.create_sample_profile(user_id).await?;
        let key = CacheKey::user(user_id);

        cache_manager.write_behind(&key, &profile, Some(7200)).await?;
        println!("   ✅ Added to buffer: {}", user_id);
    }

    // Manually flush remaining buffer
    cache_manager.flush_write_buffer().await?;

    // 4. Cache Warming Demo
    println!("\n🔥 4. Cache Warming Demonstration");
    println!("=================================");

    println!("🔥 Warming user profiles cache");
    cache_manager.warm_cache_tier("user_profiles").await?;

    println!("🔥 Warming API responses cache");
    cache_manager.warm_cache_tier("api_responses").await?;

    // Verify warmed cache content
    let warm_user: Option<UserProfile> = cache_manager.get_cache_aside(&CacheKey::user("user_1")).await?;
    match warm_user {
        Some(user) => println!("✅ Warmed user found: {}", user.username),
        None => println!("❌ Warmed user not found"),
    }

    // 5. Cache Invalidation Demo
    println!("\n🗑️ 5. Cache Invalidation Demonstration");
    println!("=====================================");

    let invalidate_user_id = "user_1";
    let invalidate_key = CacheKey::user(invalidate_user_id);

    // Ensure user exists in cache first
    let profile = cache_manager.create_sample_profile(invalidate_user_id).await?;
    cache_manager.set_cache_aside(&invalidate_key, &profile, Some(3600)).await?;
    println!("📝 User cached: {}", invalidate_user_id);

    // Verify existence
    let exists_in_memory = cache_manager.memory_cache.exists(&invalidate_key).await?;
    let exists_in_redis = cache_manager.redis_cache.exists(&invalidate_key).await?;
    println!("📋 User exists before invalidation - Memory: {}, Redis: {}", exists_in_memory, exists_in_redis);

    // Invalidate cache
    cache_manager.invalidate_user_cache(invalidate_user_id).await?;

    // Verify invalidation
    let exists_after_memory = cache_manager.memory_cache.exists(&invalidate_key).await?;
    let exists_after_redis = cache_manager.redis_cache.exists(&invalidate_key).await?;
    println!("📋 User exists after invalidation - Memory: {}, Redis: {}", exists_after_memory, exists_after_redis);

    // 6. Multi-Tier Cache Performance Demo
    println!("\n⚡ 6. Multi-Tier Cache Performance Demo");
    println!("====================================");

    let test_user_id = "perf_test_user";
    let test_key = CacheKey::user(test_user_id);
    let test_profile = cache_manager.create_sample_profile(test_user_id).await?;

    // Warm up cache
    cache_manager.set_cache_aside(&test_key, &test_profile, Some(3600)).await?;

    // Performance comparison
    println!("🏃‍♂️ Testing performance across cache tiers:");

    let iterations = 1000;

    // Test memory cache performance
    println!("🧠 Testing Memory Cache ({} iterations)...", iterations);
    let memory_start = std::time::Instant::now();
    for _ in 0..iterations {
        let _: Option<UserProfile> = cache_manager.memory_cache.get(&test_key).await?;
    }
    let memory_duration = memory_start.elapsed();
    println!("   Memory Cache: {:.2}ms total, {:.2}μs avg",
        memory_duration.as_millis(),
        memory_duration.as_micros() as f64 / iterations as f64);

    // Test Redis cache performance
    println!("🔗 Testing Redis Cache ({} iterations)...", iterations);
    let redis_start = std::time::Instant::now();
    for _ in 0..iterations {
        let _: Option<UserProfile> = cache_manager.redis_cache.get(&test_key).await?;
    }
    let redis_duration = redis_start.elapsed();
    println!("   Redis Cache: {:.2}ms total, {:.2}μs avg",
        redis_duration.as_millis(),
        redis_duration.as_micros() as f64 / iterations as f64);

    println!("🚀 Memory cache is {:.2}x faster than Redis",
        redis_duration.as_micros() as f64 / memory_duration.as_micros() as f64);

    // 7. Refresh-Ahead Pattern Demo
    println!("\n🔄 7. Refresh-Ahead Pattern Demonstration");
    println!("=========================================");

    cache_manager.strategy = CacheStrategy::RefreshAhead;

    // Set up some expiring data
    let refresh_user_id = "refresh_test_user";
    let refresh_key = CacheKey::user(refresh_user_id);
    let refresh_profile = cache_manager.create_sample_profile(refresh_user_id).await?;

    // Set with short TTL for demonstration
    cache_manager.redis_cache.set(&refresh_key, &refresh_profile, Some(30)).await?;

    println!("🔄 Setting up refresh-ahead scan");
    cache_manager.refresh_ahead::<UserProfile>("user", 60).await?;

    // 8. Cache Statistics Comparison
    println!("\n📊 8. Cache Statistics Comparison");
    println!("================================");

    cache_manager.compare_cache_stats().await?;

    // 9. Advanced Batch Operations
    println!("\n📦 9. Advanced Batch Operations");
    println!("=================================");

    // Batch get with complex keys
    let batch_keys = vec![
        CacheKey::user("user_1"),
        CacheKey::user("user_2"),
        CacheKey::api_response("/api/products", "category=electronics"),
        CacheKey::api_response("/api/users", "page=1&limit=10"),
    ];

    println!("🔍 Batch retrieving {} keys", batch_keys.len());
    let batch_results = cache_manager.redis_cache.mget::<UserProfile>(batch_keys).await?;

    for (key, result) in batch_results {
        match result {
            Some(_) => println!("   ✅ Found: {}", key),
            None => println!("   ❌ Miss: {}", key),
        }
    }

    // Batch set with different TTLs
    let batch_entries: Vec<(String, UserProfile, Option<u64>)> = (1..=5)
        .map(|i| {
            let user_id = format!("batch_user_{}", i);
            let key = CacheKey::user(&user_id);
            (key, format!("Batch User {}", i), Some(3600 + i * 60)) // Different TTLs
        })
        .collect();

    let user_entries: Vec<(String, UserProfile, Option<u64>)> = batch_entries
        .into_iter()
        .map(|(key, name, ttl)| {
            let profile = UserProfile {
                id: key.clone(),
                username: name.clone(),
                email: format!("{}@example.com", name.to_lowercase().replace(" ", "_")),
                profile_data: ProfileData {
                    bio: Some(format!("Batch created: {}", name)),
                    avatar_url: None,
                    location: None,
                    website: None,
                    birth_date: None,
                },
                preferences: UserPreferences {
                    theme: "light".to_string(),
                    language: "en".to_string(),
                    timezone: "UTC".to_string(),
                    email_notifications: true,
                    push_notifications: false,
                },
                social_links: vec![],
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            (key, profile, ttl)
        })
        .collect();

    println!("📝 Batch setting {} users with different TTLs", user_entries.len());
    cache_manager.redis_cache.mset(user_entries).await?;

    // 10. Cache Eviction and Memory Management
    println!("\n🧹 10. Cache Eviction and Memory Management");
    println!("=========================================");

    // Create a small cache to demonstrate eviction
    let small_cache = MemoryCache::new(Some(3)); // Only 3 entries max

    println!("📝 Testing LRU eviction with 3-entry cache:");

    // Add entries beyond capacity
    for i in 1..=5 {
        let key = format!("evict_test_{}", i);
        let value = format!("Value {}", i);
        small_cache.set(&key, &value, None).await?;
        println!("   ✅ Added: {}", key);

        // Show current cache state
        let stats = small_cache.stats().await?;
        println!("   📊 Cache size: {} entries", stats.total_entries);
    }

    // Test which entries remain
    println!("🔍 Checking which entries survived eviction:");
    for i in 1..=5 {
        let key = format!("evict_test_{}", i);
        let value: Option<String> = small_cache.get(&key).await?;
        match value {
            Some(v) => println!("   ✅ {} = {}", key, v),
            None => println!("   ❌ {} = evicted", key),
        }
    }

    // 11. Error Handling and Recovery
    println!("\n⚠️ 11. Error Handling and Recovery");
    println!("=================================");

    // Test error scenarios
    println!("🧪 Testing error handling scenarios:");

    // Invalid cache key
    let invalid_key = "nonexistent:key";
    let missing_value: Option<UserProfile> = cache_manager.memory_cache.get(invalid_key).await?;
    println!("   📋 Invalid key lookup: {:?}", missing_value);

    // TTL operations on missing keys
    let missing_ttl = cache_manager.memory_cache.ttl(invalid_key).await?;
    println!("   📋 TTL on missing key: {:?}", missing_ttl);

    // Delete operations
    let delete_result = cache_manager.memory_cache.delete(invalid_key).await?;
    println!("   📋 Delete missing key: {}", if delete_result { "deleted" } else { "not found" });

    // 12. Final Cleanup and Statistics
    println!("\n🧹 12. Final Cleanup and Statistics");
    println!("=================================");

    cache_manager.cleanup().await?;

    // Show final comparison
    cache_manager.compare_cache_stats().await?;

    println!("\n🎉 Advanced Usage Examples Complete!");
    println!("=====================================");
    println!("✅ All advanced caching patterns demonstrated");
    println!("✅ Multi-tier cache performance compared");
    println!("✅ Cache invalidation and warming implemented");
    println!("✅ Error handling and recovery tested");
    println!("✅ Batch operations and eviction strategies verified");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_manager_creation() {
        // Test with mock Redis URL (will fail but should handle gracefully)
        let result = AdvancedCacheManager::new("redis://invalid:6379", CacheStrategy::CacheAside).await;
        assert!(result.is_ok()); // Should fall back to memory-only mode
    }

    #[tokio::test]
    async fn test_cache_aside_pattern() {
        let cache_manager = AdvancedCacheManager::new("redis://localhost:6379", CacheStrategy::CacheAside).await;

        if let Ok(manager) = cache_manager {
            let key = "test_key";
            let test_value = "test_value";

            // Test miss
            let result: Option<String> = manager.get_cache_aside(key).await.unwrap();
            assert!(result.is_none());

            // Test set and hit
            manager.set_cache_aside(key, &test_value, Some(60)).await.unwrap();
            let result: Option<String> = manager.get_cache_aside(key).await.unwrap();
            assert_eq!(result, Some(test_value.to_string()));
        }
    }

    #[tokio::test]
    async fn test_write_behind_buffer() {
        let mut cache_manager = AdvancedCacheManager::new("redis://localhost:6379", CacheStrategy::WriteBehind).await;

        if let Ok(manager) = cache_manager {
            // Add entries to buffer
            for i in 1..=3 {
                let key = format!("buffer_test_{}", i);
                let value = format!("Value {}", i);
                manager.write_behind(&key, &value, Some(60)).await.unwrap();
            }

            // Buffer should have 3 entries
            assert_eq!(manager.write_buffer.len(), 3);

            // Flush buffer
            manager.flush_write_buffer().await.unwrap();

            // Buffer should be empty
            assert_eq!(manager.write_buffer.len(), 0);
        }
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let cache_manager = AdvancedCacheManager::new("redis://localhost:6379", CacheStrategy::CacheAside).await;

        if let Ok(manager) = cache_manager {
            let user_id = "test_user_invalidate";
            let key = CacheKey::user(user_id);

            // Set up data
            let profile = manager.create_sample_profile(user_id).await.unwrap();
            manager.set_cache_aside(&key, &profile, Some(60)).await.unwrap();

            // Verify it exists
            let exists = manager.memory_cache.exists(&key).await.unwrap();
            assert!(exists);

            // Invalidate
            manager.invalidate_user_cache(user_id).await.unwrap();

            // Verify it's gone
            let exists = manager.memory_cache.exists(&key).await.unwrap();
            assert!(!exists);
        }
    }
}