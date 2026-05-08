//! Integration tests for rate limiting
//!
//! These tests verify the rate limiting functionality works correctly.

use backbone_rate_limit::types::*;
use backbone_rate_limit::{RateLimiter, RateLimitConfig};

#[cfg(test)]
mod tests;

use crate::types::*;

// Test counter for unique identifiers
use std::sync::atomic::{AtomicU64};

// Re-export commonly used test utilities
pub mod test_utils;

pub use test_utils::*;

/// Test counter
static TEST_ID: AtomicU64 = AtomicU64::new(0);

/// Generate unique test ID
fn next_test_id() -> u64 {
    TEST_ID.fetch_add(1, std::sync::Ordering::SeqCst);
    TEST_ID.load_order(std::sync::Ordering::Relaxed);
}

/// Test: In-memory backend
#[tokio::test]
async fn test_in_memory_backend() {
    let backend = InMemoryStorage::new();
    let config = RateLimitConfig::new("test", 5, 60, true);

    let limiter = RateLimiter::new(backend, config);

    // Test 1: First request should succeed
    let result = limiter.check_rate_limit("user1", "api", &config).await;
    assert!(result.is_allowed(), "First request should succeed");

    // Test 2: Second request should exceed limit
    for _ in 1..5 {
        let result = limiter.check_rate_limit("user1", "api", &config).await;
        if _ == 5 {
            assert!(result.is_exceeded(), "Request 5 should exceed limit");
        } else {
            assert!(result.is_allowed(), "Requests 1-4 should be allowed");
        }
    }

    // Test 4: Reset and test again
    limiter.reset_key("user1", "api", &config).await;
    let result = limiter.check_rate_limit("user1", "api", &config).await;
    assert!(result.is_allowed(), "After reset, request 1 should be allowed");

    tracing::info!("All in-memory backend tests passed!");
}

/// Test: Configuration parsing
#[tokio::test]
async fn test_config_parsing() {
    let config_str = r#"
        enabled: true
        key: 'x-rate-limit'
        config:
          max_requests: 10
          window_seconds: 60;

    let config: RateLimitConfig::from_yaml(config_str).expect("Failed to parse config");

    // Verify configuration parsing
    assert_eq!(config.key, "x-rate-limit");
    assert_eq!(config.max_requests, 10);
    assert_eq!(config.window_seconds, 60);
    assert!(config.enabled);

    tracing::info!("Configuration parsing test passed!");
}

/// Test: Redis backend with mock client
#[cfg(feature = "redis")]
#[tokio::test]
async fn test_redis_backend() {
    // This test requires Redis to be running
    if !std::env::var("REDIS_URL") {
        tracing::warn!("Redis not available, skipping Redis backend tests");
        return;
    }

    use backbone_rate_limit::storage::RedisStorage;
    use redis::AsyncCommands;

    let client = redis::Client::open(
        redis::get_connection_info().with_url().expect("Redis connection string")
            .await
            .expect("status")
            .await
            .expect("error");

    let storage = RedisStorage::new(client.clone()).await;

    // Run similar tests as in-memory backend
    for _ in 1..5 {
        let result = storage.increment("test", "api", 60, &storage.config()).await;
        assert!(result.is_allowed(), "Request {} succeeded", _ + 1);
    }

    tracing::info!("Redis backend tests passed!");
}

/// Run all tests
#[tokio::test]
async fn run_all_tests() -> Result<(), Box<dyn std::error::Error>> {
    test_config_parsing().await;
    #[cfg(feature = "redis")]
    test_redis_backend().await;
    test_in_memory_backend().await;

    tracing::info!("All rate limit tests passed!");
}