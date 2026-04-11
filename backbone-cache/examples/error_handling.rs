//! Comprehensive error handling examples for backbone-cache
//! Demonstrates various error scenarios and recovery strategies

use backbone_cache::{RedisCache, MemoryCache, CacheKey, CacheError, CacheResult};
use serde::{Serialize, Deserialize};
use std::time::Duration;
use tokio::time::sleep;
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

// Test data structures
#[derive(Serialize, Deserialize, Debug, Clone)]
struct TestData {
    id: String,
    name: String,
    data: String,
    created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ComplexData {
    id: String,
    nested: NestedStruct,
    optional_field: Option<String>,
    vector_data: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct NestedStruct {
    field1: i32,
    field2: f64,
    field3: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct UnserializableData {
    #[serde(skip)]
    runtime_data: std::time::Instant, // This won't serialize
    id: String,
}

// Error handling service
struct ErrorHandlingService {
    memory_cache: MemoryCache,
    redis_cache: Option<RedisCache>,
    fallback_enabled: bool,
    retry_attempts: u32,
}

impl ErrorHandlingService {
    async fn new(redis_url: Option<&str>, fallback_enabled: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let memory_cache = MemoryCache::new(Some(1000));

        let redis_cache = if let Some(url) = redis_url {
            match RedisCache::new(url).await {
                Ok(cache) => Some(cache),
                Err(e) => {
                    println!("⚠️ Redis initialization failed: {}", e);
                    if fallback_enabled {
                        println!("🔄 Operating in memory-only mode with fallback");
                        None
                    } else {
                        return Err(e.into());
                    }
                }
            }
        } else {
            None
        };

        Ok(Self {
            memory_cache,
            redis_cache,
            fallback_enabled,
            retry_attempts: 3,
        })
    }

    // SCENARIO 1: Connection Error Handling
    async fn handle_connection_errors(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n🔌 SCENARIO 1: Connection Error Handling");
        println!("=======================================");

        let test_data = TestData {
            id: "conn_test".to_string(),
            name: "Connection Test".to_string(),
            data: "Test data for connection scenarios".to_string(),
            created_at: Utc::now(),
        };

        // Test Redis connection (if available)
        if let Some(ref redis_cache) = self.redis_cache {
            println!("🔗 Testing Redis connection...");

            // Try to set data in Redis
            match redis_cache.set("connection_test", &test_data, Some(3600)).await {
                Ok(_) => println!("✅ Redis connection successful"),
                Err(e) => {
                    println!("❌ Redis connection failed: {}", e);

                    // Handle different types of Redis errors
                    match e {
                        CacheError::RedisConnection(msg) => {
                            println!("🔧 Redis connection error: {}", msg);
                            println!("💡 Suggestion: Check Redis server status and network connectivity");
                        }
                        CacheError::RedisOperation(msg) => {
                            println!("🔧 Redis operation error: {}", msg);
                            println!("💡 Suggestion: Check Redis configuration and permissions");
                        }
                        _ => {
                            println!("🔧 Unknown Redis error: {}", e);
                        }
                    }

                    // Fall back to memory cache if enabled
                    if self.fallback_enabled {
                        println!("🔄 Falling back to memory cache");
                        self.memory_cache.set("connection_test_fallback", &test_data, Some(3600)).await?;
                        println!("✅ Data stored in memory cache fallback");
                    }
                }
            }

            // Test Redis health check
            self.test_redis_health(redis_cache).await?;
        } else {
            println!("⚠️ Redis not configured, skipping connection tests");
        }

        Ok(())
    }

    async fn test_redis_health(&self, redis_cache: &RedisCache) -> Result<(), Box<dyn std::error::Error>> {
        println!("🏥 Testing Redis health check...");

        // Simple health check using PING command
        let health_result = redis_cache.exists("health_check_key").await;

        match health_result {
            Ok(exists) => {
                println!("✅ Redis health check passed (key exists: {})", exists);
            }
            Err(e) => {
                println!("❌ Redis health check failed: {}", e);

                // Try to recover by attempting a simple operation
                println!("🔄 Attempting recovery...");
                match redis_cache.set("recovery_test", "recovery_data", Some(60)).await {
                    Ok(_) => println!("✅ Redis recovery successful"),
                    Err(recovery_error) => println!("❌ Redis recovery failed: {}", recovery_error),
                }
            }
        }

        Ok(())
    }

    // SCENARIO 2: Serialization Error Handling
    async fn handle_serialization_errors(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n📦 SCENARIO 2: Serialization Error Handling");
        println!("========================================");

        // Test with data that won't serialize properly
        let unserializable_data = UnserializableData {
            runtime_data: std::time::Instant::now(),
            id: "unserializable".to_string(),
        };

        println!("🧪 Testing serialization with problematic data...");

        // Try to store unserializable data in memory cache
        match self.memory_cache.set("unserializable_test", &unserializable_data, Some(3600)).await {
            Ok(_) => println!("✅ Unexpected success with unserializable data"),
            Err(e) => {
                println!("❌ Expected serialization error: {}", e);

                match e {
                    CacheError::Serialization(msg) => {
                        println!("📦 Serialization error detected: {}", msg);
                        println!("💡 Suggestion: Check data types implement Serialize trait correctly");
                    }
                    _ => {
                        println!("🔧 Unexpected error type: {}", e);
                    }
                }

                // Demonstrate recovery with valid data
                let valid_data = TestData {
                    id: "valid_data".to_string(),
                    name: "Valid Data".to_string(),
                    data: "This should serialize fine".to_string(),
                    created_at: Utc::now(),
                };

                match self.memory_cache.set("valid_test", &valid_data, Some(3600)).await {
                    Ok(_) => println!("✅ Recovery successful with valid data"),
                    Err(e) => println!("❌ Recovery failed: {}", e),
                }
            }
        }

        // Test deserialization errors
        println!("\n🧪 Testing deserialization error scenarios...");

        // Store valid data first
        let valid_data = TestData {
            id: "deser_test".to_string(),
            name: "Deserialization Test".to_string(),
            data: "Test data".to_string(),
            created_at: Utc::now(),
        };

        self.memory_cache.set("deser_test", &valid_data, Some(3600)).await?;

        // Try to retrieve as wrong type
        match self.memory_cache.get::<ComplexData>("deser_test").await {
            Ok(data) => {
                match data {
                    Some(_) => println!("⚠️ Unexpected success with wrong type"),
                    None => println!("✅ Expected deserialization failure handled gracefully"),
                }
            }
            Err(e) => {
                println!("❌ Deserialization error: {}", e);

                match e {
                    CacheError::Deserialization(msg) => {
                        println!("📦 Deserialization error: {}", msg);
                        println!("💡 Suggestion: Ensure data types match between storage and retrieval");
                    }
                    _ => println!("🔧 Unexpected error type: {}", e),
                }
            }
        }

        Ok(())
    }

    // SCENARIO 3: Data Corruption and Recovery
    async fn handle_data_corruption(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n💀 SCENARIO 3: Data Corruption and Recovery");
        println!("=========================================");

        // Simulate corrupted data detection
        println!("🧪 Testing data corruption detection...");

        let test_data = TestData {
            id: "corruption_test".to_string(),
            name: "Corruption Test".to_string(),
            data: "This data should be intact".to_string(),
            created_at: Utc::now(),
        };

        // Store data
        self.memory_cache.set("corruption_test", &test_data, Some(3600)).await?;

        // Retrieve and verify data integrity
        match self.memory_cache.get::<TestData>("corruption_test").await {
            Ok(Some(retrieved_data)) => {
                // Verify data integrity
                if retrieved_data.id == test_data.id &&
                   retrieved_data.name == test_data.name &&
                   retrieved_data.data == test_data.data {
                    println!("✅ Data integrity verified");
                } else {
                    println!("❌ Data corruption detected!");
                    println!("   Original: {} - {} - {}", test_data.id, test_data.name, test_data.data);
                    println!("   Retrieved: {} - {} - {}", retrieved_data.id, retrieved_data.name, retrieved_data.data);

                    // Handle corruption: delete corrupted data and reload from source
                    self.memory_cache.delete("corruption_test").await?;
                    println!("🗑️ Corrupted data deleted, should be reloaded from database");
                }
            }
            Ok(None) => {
                println!("❌ Data not found (unexpected corruption)");
            }
            Err(e) => {
                println!("❌ Error retrieving data: {}", e);
            }
        }

        // Simulate partial corruption recovery
        println!("\n🔄 Testing partial corruption recovery...");

        let complex_data = ComplexData {
            id: "complex_test".to_string(),
            nested: NestedStruct {
                field1: 42,
                field2: 3.14159,
                field3: true,
            },
            optional_field: Some("optional_value".to_string()),
            vector_data: vec!["item1".to_string(), "item2".to_string(), "item3".to_string()],
        };

        self.memory_cache.set("complex_test", &complex_data, Some(3600)).await?;

        // Verify with partial data validation
        match self.memory_cache.get::<ComplexData>("complex_test").await {
            Ok(Some(data)) => {
                let mut integrity_issues = Vec::new();

                // Validate nested data
                if data.nested.field1 != 42 {
                    integrity_issues.push("nested.field1 mismatch".to_string());
                }
                if (data.nested.field2 - 3.14159).abs() > 0.0001 {
                    integrity_issues.push("nested.field2 mismatch".to_string());
                }
                if data.nested.field3 != true {
                    integrity_issues.push("nested.field3 mismatch".to_string());
                }

                // Validate vector data
                if data.vector_data.len() != 3 {
                    integrity_issues.push("vector_data length mismatch".to_string());
                }

                if integrity_issues.is_empty() {
                    println!("✅ Complex data integrity verified");
                } else {
                    println!("❌ Complex data integrity issues detected:");
                    for issue in &integrity_issues {
                        println!("   - {}", issue);
                    }

                    // Implement recovery strategy
                    println!("🔄 Implementing data recovery...");
                    self.memory_cache.delete("complex_test").await?;
                    println!("✅ Corrupted complex data removed for reload");
                }
            }
            Ok(None) => println!("❌ Complex data not found"),
            Err(e) => println!("❌ Error retrieving complex data: {}", e),
        }

        Ok(())
    }

    // SCENARIO 4: Memory Pressure and Eviction
    async fn handle_memory_pressure(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n💾 SCENARIO 4: Memory Pressure and Eviction");
        println!("==========================================");

        // Create a small cache to demonstrate eviction
        let small_cache = MemoryCache::new(Some(5)); // Only 5 entries

        println!("📊 Testing LRU eviction with 5-entry cache...");

        // Fill cache beyond capacity
        for i in 1..=8 {
            let data = TestData {
                id: format!("eviction_test_{}", i),
                name: format!("Eviction Test {}", i),
                data: format!("Data for test {}", i),
                created_at: Utc::now(),
            };

            let key = format!("eviction_{}", i);

            match small_cache.set(&key, &data, None).await {
                Ok(_) => println!("✅ Added entry {} to cache", i),
                Err(e) => println!("❌ Failed to add entry {}: {}", i, e),
            }

            // Show cache status
            let stats = small_cache.stats().await?;
            println!("   Cache status: {} entries", stats.total_entries);
        }

        println!("\n🔍 Checking which entries survived eviction...");

        for i in 1..=8 {
            let key = format!("eviction_{}", i);
            match small_cache.get::<TestData>(&key).await {
                Ok(Some(data)) => println!("✅ Entry {} survived: {}", i, data.name),
                Ok(None) => println!("❌ Entry {} was evicted", i),
                Err(e) => println!("❌ Error checking entry {}: {}", i, e),
            }
        }

        // Test memory pressure handling
        println!("\n💾 Testing memory pressure handling...");

        let large_cache = MemoryCache::new(Some(1000));
        let mut operation_count = 0;

        // Simulate high memory usage scenario
        for i in 1..=1200 {
            let large_data = TestData {
                id: format!("pressure_test_{}", i),
                name: format!("Pressure Test {}", i),
                data: "x".repeat(1000), // 1KB per entry
                created_at: Utc::now(),
            };

            let key = format!("pressure_{}", i);

            match large_cache.set(&key, &large_data, None).await {
                Ok(_) => operation_count += 1,
                Err(e) => {
                    println!("❌ Operation {} failed: {}", i, e);
                    break;
                }
            }

            // Check cache health periodically
            if i % 200 == 0 {
                let stats = large_cache.stats().await?;
                println!("   Progress: {} operations, {} entries in cache", i, stats.total_entries);
            }
        }

        println!("✅ Memory pressure test completed: {} successful operations", operation_count);

        Ok(())
    }

    // SCENARIO 5: Network Timeout and Retry Logic
    async fn handle_network_timeouts(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n🕐 SCENARIO 5: Network Timeout and Retry Logic");
        println!("===========================================");

        if self.redis_cache.is_none() {
            println!("⚠️ Redis not available, skipping timeout tests");
            return Ok(());
        }

        let redis_cache = self.redis_cache.as_ref().unwrap();
        let test_data = TestData {
            id: "timeout_test".to_string(),
            name: "Timeout Test".to_string(),
            data: "Test data for timeout scenarios".to_string(),
            created_at: Utc::now(),
        };

        println!("🧪 Testing Redis operations with retry logic...");

        // Test with exponential backoff
        let mut attempt = 0;
        let mut last_error: Option<CacheError> = None;

        while attempt < self.retry_attempts {
            attempt += 1;
            println!("🔄 Attempt {} of {}", attempt, self.retry_attempts);

            match redis_cache.set("timeout_test", &test_data, Some(3600)).await {
                Ok(_) => {
                    println!("✅ Operation successful on attempt {}", attempt);
                    break;
                }
                Err(e) => {
                    last_error = Some(e.clone());
                    println!("❌ Attempt {} failed: {}", attempt, e);

                    // Check if we should retry based on error type
                    match e {
                        CacheError::RedisConnection(_) | CacheError::RedisOperation(_) => {
                            if attempt < self.retry_attempts {
                                let delay = Duration::from_millis(100 * 2_u64.pow(attempt - 1)); // Exponential backoff
                                println!("⏳ Retrying in {:?}...", delay);
                                sleep(delay).await;
                            }
                        }
                        _ => {
                            println!("🛑 Non-retryable error, stopping retries");
                            break;
                        }
                    }
                }
            }
        }

        if let Some(error) = last_error {
            if attempt >= self.retry_attempts {
                println!("❌ All retry attempts failed, last error: {}", error);

                // Implement fallback strategy
                if self.fallback_enabled {
                    println!("🔄 Implementing fallback to memory cache");
                    self.memory_cache.set("timeout_test_fallback", &test_data, Some(3600)).await?;
                    println!("✅ Data stored in memory cache as fallback");
                }
            }
        }

        // Test circuit breaker pattern simulation
        println!("\n⚡ Testing circuit breaker pattern simulation...");

        let mut consecutive_failures = 0;
        let circuit_breaker_threshold = 3;
        let mut circuit_open = false;

        for i in 1..=10 {
            if circuit_open {
                println!("⚡ Circuit open, skipping operation {} (using fallback)", i);
                if self.fallback_enabled {
                    let fallback_key = format!("circuit_breaker_test_{}", i);
                    self.memory_cache.set(&fallback_key, &test_data, Some(60)).await?;
                }
                continue;
            }

            match redis_cache.get::<TestData>("nonexistent_key").await {
                Ok(_) => {
                    consecutive_failures = 0;
                    println!("✅ Operation {} successful", i);
                }
                Err(e) => {
                    consecutive_failures += 1;
                    println!("❌ Operation {} failed: {} (consecutive failures: {})", i, e, consecutive_failures);

                    if consecutive_failures >= circuit_breaker_threshold {
                        circuit_open = true;
                        println!("⚡ Circuit breaker opened after {} consecutive failures", consecutive_failures);
                        println!("🔄 Subsequent operations will use fallback");
                    }
                }
            }
        }

        Ok(())
    }

    // SCENARIO 6: TTL Expiration Edge Cases
    async fn handle_ttl_expiration(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n⏰ SCENARIO 6: TTL Expiration Edge Cases");
        println!("=======================================");

        let test_data = TestData {
            id: "ttl_test".to_string(),
            name: "TTL Test".to_string(),
            data: "Test data for TTL scenarios".to_string(),
            created_at: Utc::now(),
        };

        // Test immediate expiration
        println!("🧪 Testing immediate expiration (1 second TTL)...");
        self.memory_cache.set("immediate_expire", &test_data, Some(1)).await?;

        println!("⏳ Checking immediately...");
        let immediate_check: Option<TestData> = self.memory_cache.get("immediate_expire").await?;
        println!("   Immediate check: {}", if immediate_check.is_some() { "Found" } else { "Not found" });

        println!("⏳ Waiting 2 seconds for expiration...");
        sleep(Duration::from_secs(2)).await;

        let expired_check: Option<TestData> = self.memory_cache.get("immediate_expire").await?;
        println!("   After expiration: {}", if expired_check.is_some() { "Found (ERROR)" } else { "Not found (OK)" });

        // Test very long TTL
        println!("\n🧪 Testing very long TTL (10 years)...");
        let ten_years_seconds = 10 * 365 * 24 * 60 * 60;
        self.memory_cache.set("long_ttl", &test_data, Some(ten_years_seconds)).await?;

        match self.memory_cache.ttl("long_ttl").await {
            Ok(Some(ttl)) => {
                if ttl >= ten_years_seconds - 1000 { // Allow small variance
                    println!("✅ Long TTL set correctly: {} seconds", ttl);
                } else {
                    println!("⚠️ TTL may be truncated: {} seconds (expected {})", ttl, ten_years_seconds);
                }
            }
            Ok(None) => println!("❌ TTL not found"),
            Err(e) => println!("❌ Error checking TTL: {}", e),
        }

        // Test TTL edge cases with Redis if available
        if let Some(ref redis_cache) = self.redis_cache {
            println!("\n🔗 Testing Redis TTL edge cases...");

            // Test zero TTL (immediate expiration)
            match redis_cache.set("zero_ttl", &test_data, Some(0)).await {
                Ok(_) => println!("✅ Zero TTL set in Redis"),
                Err(e) => println!("❌ Zero TTL failed: {}", e),
            }

            // Check if it exists immediately
            let zero_ttl_exists = redis_cache.exists("zero_ttl").await?;
            println!("   Zero TTL exists immediately: {}", zero_ttl_exists);

            // Test very large TTL
            let large_ttl = u64::MAX / 1000; // Large but reasonable
            match redis_cache.set("large_ttl", &test_data, Some(large_ttl)).await {
                Ok(_) => println!("✅ Large TTL set in Redis"),
                Err(e) => println!("❌ Large TTL failed: {}", e),
            }

            // Test TTL on non-existent key
            match redis_cache.ttl("nonexistent_ttl_test").await {
                Ok(Some(0)) => println!("✅ Non-existent key returns TTL 0"),
                Ok(None) => println!("⚠️ Non-existent key returns None TTL"),
                Err(e) => println!("❌ Error checking non-existent key TTL: {}", e),
            }
        }

        Ok(())
    }

    // SCENARIO 7: Concurrent Access and Race Conditions
    async fn handle_concurrent_access(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n🏃 SCENARIO 7: Concurrent Access and Race Conditions");
        println!("===============================================");

        let test_data = TestData {
            id: "concurrent_test".to_string(),
            name: "Concurrent Test".to_string(),
            data: "Test data for concurrent access".to_string(),
            created_at: Utc::now(),
        };

        println!("🧪 Testing concurrent read/write operations...");

        let mut handles = Vec::new();
        let concurrent_operations = 10;

        // Spawn concurrent writers
        for i in 0..concurrent_operations {
            let cache = self.memory_cache.clone();
            let data = test_data.clone();
            let key = format!("concurrent_write_{}", i);

            let handle = tokio::spawn(async move {
                match cache.set(&key, &data, Some(3600)).await {
                    Ok(_) => {
                        // Verify immediately
                        match cache.get::<TestData>(&key).await {
                            Ok(Some(retrieved)) => {
                                if retrieved.id == data.id {
                                    Some(())
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        }
                    }
                    Err(_) => None,
                }
            });

            handles.push(handle);
        }

        // Spawn concurrent readers
        for i in 0..concurrent_operations {
            let cache = self.memory_cache.clone();
            let key = format!("concurrent_write_{}", i % 3); // Some will read existing data

            let handle = tokio::spawn(async move {
                match cache.get::<TestData>(&key).await {
                    Ok(data) => data.is_some(),
                    Err(_) => false,
                }
            });

            handles.push(handle);
        }

        // Wait for all operations and collect results
        let mut successful_operations = 0;
        let mut failed_operations = 0;

        for handle in handles {
            match handle.await {
                Ok(Some(_)) => successful_operations += 1,
                Ok(None) => failed_operations += 1,
                Err(e) => {
                    println!("❌ Task failed: {}", e);
                    failed_operations += 1;
                }
            }
        }

        println!("📊 Concurrent operations results:");
        println!("   Successful: {} | Failed: {}", successful_operations, failed_operations);
        println!("   Total: {} operations", successful_operations + failed_operations);

        // Test race condition in cache eviction
        println!("\n🏃 Testing race condition in cache eviction...");

        let small_cache = MemoryCache::new(Some(3)); // Very small cache
        let cache = Arc::new(small_cache);

        let mut eviction_handles = Vec::new();

        // Spawn many concurrent writes to trigger eviction
        for i in 0..20 {
            let cache_ref = cache.clone();
            let handle = tokio::spawn(async move {
                let data = TestData {
                    id: format!("eviction_race_{}", i),
                    name: format!("Eviction Race {}", i),
                    data: "x".repeat(100),
                    created_at: Utc::now(),
                };

                let key = format!("eviction_{}", i);
                match cache_ref.set(&key, &data, None).await {
                    Ok(_) => Some(()),
                    Err(_) => None,
                }
            });

            eviction_handles.push(handle);
        }

        let mut eviction_successes = 0;
        for handle in eviction_handles {
            if let Ok(Some(_)) = handle.await {
                eviction_successes += 1;
            }
        }

        println!("✅ Eviction race condition test: {} successful operations", eviction_successes);

        Ok(())
    }

    // SCENARIO 8: Data Type Mismatch Recovery
    async fn handle_data_type_mismatch(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n🔄 SCENARIO 8: Data Type Mismatch Recovery");
        println!("==========================================");

        // Store data as one type
        let test_string = "This is a string value".to_string();
        self.memory_cache.set("type_mismatch_test", &test_string, Some(3600)).await?;

        println!("🧪 Stored string value in cache");

        // Try to retrieve as different type
        println!("🔍 Attempting to retrieve as different type...");

        match self.memory_cache.get::<TestData>("type_mismatch_test").await {
            Ok(data) => {
                match data {
                    Some(_) => println!("⚠️ Unexpected success with wrong type"),
                    None => {
                        println!("✅ Type mismatch handled gracefully (returned None)");

                        // Implement type-aware recovery
                        println!("🔄 Attempting recovery with correct type...");
                        match self.memory_cache.get::<String>("type_mismatch_test").await {
                            Ok(Some(recovered_string)) => {
                                println!("✅ Recovery successful: \"{}\"", recovered_string);
                            }
                            Ok(None) => println!("❌ Recovery failed: data not found"),
                            Err(e) => println!("❌ Recovery error: {}", e),
                        }
                    }
                }
            }
            Err(e) => {
                println!("❌ Error during type mismatch test: {}", e);

                match e {
                    CacheError::Deserialization(msg) => {
                        println!("📦 Deserialization error: {}", msg);
                        println!("💡 This is expected when retrieving wrong type");
                    }
                    _ => println!("🔧 Unexpected error type: {}", e),
                }
            }
        }

        // Test with numeric types
        println!("\n🔢 Testing numeric type mismatches...");

        let test_number = 42i32;
        self.memory_cache.set("number_test", &test_number, Some(3600)).await?;

        // Try to retrieve as string
        match self.memory_cache.get::<String>("number_test").await {
            Ok(data) => {
                match data {
                    Some(_) => println!("⚠️ Unexpected success retrieving number as string"),
                    None => {
                        println!("✅ Number-to-string mismatch handled correctly");

                        // Try correct retrieval
                        match self.memory_cache.get::<i32>("number_test").await {
                            Ok(Some(number)) => println!("✅ Correct retrieval: {}", number),
                            Ok(None) => println!("❌ Number not found"),
                            Err(e) => println!("❌ Error retrieving number: {}", e),
                        }
                    }
                }
            }
            Err(e) => println!("❌ Error in number type test: {}", e),
        }

        Ok(())
    }

    // Comprehensive error handling report
    async fn generate_error_report(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n📋 Comprehensive Error Handling Report");
        println!("======================================");

        // Test all error scenarios and generate summary
        let mut error_scenarios = Vec::new();

        // Test 1: Missing key
        match self.memory_cache.get::<TestData>("definitely_missing_key").await {
            Ok(None) => {
                error_scenarios.push(("Missing key", "✅ Handled correctly", "Returned None"));
            }
            _ => {
                error_scenarios.push(("Missing key", "❌ Unexpected behavior", "Should return None"));
            }
        }

        // Test 2: Invalid TTL
        match self.memory_cache.ttl("any_key").await {
            Ok(None) => {
                error_scenarios.push(("Non-existent TTL", "✅ Handled correctly", "Returned None"));
            }
            _ => {
                error_scenarios.push(("Non-existent TTL", "❌ Unexpected behavior", "Should return None"));
            }
        }

        // Test 3: Delete non-existent key
        match self.memory_cache.delete("nonexistent_delete_test").await {
            Ok(false) => {
                error_scenarios.push(("Delete non-existent", "✅ Handled correctly", "Returned false"));
            }
            _ => {
                error_scenarios.push(("Delete non-existent", "❌ Unexpected behavior", "Should return false"));
            }
        }

        // Test 4: Clear empty cache
        match self.memory_cache.clear().await {
            Ok(_) => {
                error_scenarios.push(("Clear empty cache", "✅ Handled correctly", "Success"));
            }
            Err(e) => {
                error_scenarios.push(("Clear empty cache", "❌ Error", &e.to_string()));
            }
        }

        // Test 5: Cache statistics on empty cache
        match self.memory_cache.stats().await {
            Ok(stats) => {
                error_scenarios.push(("Empty cache stats", "✅ Handled correctly",
                    &format!("{} entries, {}% hit rate", stats.total_entries, stats.hit_rate * 100.0)));
            }
            Err(e) => {
                error_scenarios.push(("Empty cache stats", "❌ Error", &e.to_string()));
            }
        }

        println!("\n📊 Error Handling Test Results:");
        for (scenario, status, details) in error_scenarios {
            println!("   {}: {} - {}", scenario, status, details);
        }

        // Test Redis-specific errors if available
        if let Some(ref redis_cache) = self.redis_cache {
            println!("\n🔗 Redis-specific Error Handling:");

            // Test Redis connection health
            match redis_cache.exists("redis_health_check").await {
                Ok(_) => {
                    println!("   ✅ Redis connection healthy");
                }
                Err(e) => {
                    println!("   ❌ Redis connection issue: {}", e);
                }
            }

            // Test Redis statistics
            match redis_cache.stats().await {
                Ok(stats) => {
                    println!("   ✅ Redis stats accessible: {} entries", stats.total_entries);
                }
                Err(e) => {
                    println!("   ❌ Redis stats error: {}", e);
                }
            }
        }

        println!("\n🛡️ Error Handling Recommendations:");
        println!("   1. Always check for None returns from get() operations");
        println!("   2. Handle deserialization errors gracefully");
        println!("   3. Implement retry logic for network operations");
        println!("   4. Use fallback mechanisms for high availability");
        println!("   5. Monitor cache hit rates and error patterns");
        println!("   6. Implement circuit breakers for unreliable backends");
        println!("   7. Validate data integrity after critical operations");

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Backbone Cache Error Handling Examples ===\n");

    // Initialize error handling service with fallback enabled
    let error_service = ErrorHandlingService::new(Some("redis://localhost:6379"), true).await?;

    println!("🛡️ Starting comprehensive error handling tests...\n");

    // Run all error handling scenarios
    error_service.handle_connection_errors().await?;
    error_service.handle_serialization_errors().await?;
    error_service.handle_data_corruption().await?;
    error_service.handle_memory_pressure().await?;
    error_service.handle_network_timeouts().await?;
    error_service.handle_ttl_expiration().await?;
    error_service.handle_concurrent_access().await?;
    error_service.handle_data_type_mismatch().await?;
    error_service.generate_error_report().await?;

    println!("\n🎉 Error Handling Examples Complete!");
    println!("====================================");
    println!("✅ Connection error handling and fallback strategies");
    println!("✅ Serialization/deserialization error recovery");
    println!("✅ Data corruption detection and recovery");
    println!("✅ Memory pressure and cache eviction handling");
    println!("✅ Network timeout and retry logic with circuit breaker");
    println!("✅ TTL expiration edge cases");
    println!("✅ Concurrent access and race condition handling");
    println!("✅ Data type mismatch recovery");
    println!("✅ Comprehensive error reporting and recommendations");

    println!("\n💡 Key Takeaways:");
    println!("- Always handle None returns from cache operations");
    println!("- Implement fallback strategies for high availability");
    println!("- Use retry logic with exponential backoff for network operations");
    println!("- Validate data integrity after critical operations");
    println!("- Monitor cache performance and error patterns");
    println!("- Implement circuit breakers to prevent cascading failures");
    println!("- Test error scenarios regularly in production environments");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_error_handling_service_creation() {
        // Test with invalid Redis URL (should fall back to memory-only)
        let service = ErrorHandlingService::new(Some("redis://invalid:6379"), true).await;
        assert!(service.is_ok()); // Should succeed with fallback
    }

    #[tokio::test]
    async fn test_serialization_error_handling() {
        let service = ErrorHandlingService::new(None, false).await.unwrap();

        // This should handle the error gracefully
        let result = service.handle_serialization_errors().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_data_type_mismatch() {
        let service = ErrorHandlingService::new(None, false).await.unwrap();

        // This should handle type mismatches gracefully
        let result = service.handle_data_type_mismatch().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ttl_expiration() {
        let service = ErrorHandlingService::new(None, false).await.unwrap();

        // This should test TTL edge cases
        let result = service.handle_ttl_expiration().await;
        assert!(result.is_ok());
    }
}