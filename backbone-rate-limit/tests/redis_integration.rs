//! Redis integration tests for rate limiting
//!
//! These tests require a running Redis instance and are marked `#[ignore]`.
//! Run with: `cargo test -p backbone-rate-limit --features redis -- --ignored`

#[cfg(feature = "redis")]
mod redis_tests {
    use backbone_rate_limit::{
        RedisStorage, StorageBackend, RateLimiter,
        RateLimitConfig, RateLimitMiddleware,
    };

    fn test_config() -> RateLimitConfig {
        RateLimitConfig {
            key: "test".to_string(),
            max_requests: 5,
            window_seconds: 60,
            enabled: true,
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_storage_increment() {
        let storage = RedisStorage::new("redis://localhost:6379")
            .await
            .expect("Redis connection failed");
        let config = test_config();

        // Reset first
        storage.reset("test_incr", &config).await.unwrap();

        let count = storage.increment("test_incr", &config).await.unwrap();
        assert_eq!(count, 1);

        let count = storage.increment("test_incr", &config).await.unwrap();
        assert_eq!(count, 2);

        let count = storage.increment("test_incr", &config).await.unwrap();
        assert_eq!(count, 3);

        // Cleanup
        storage.reset("test_incr", &config).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_storage_get_count() {
        let storage = RedisStorage::new("redis://localhost:6379")
            .await
            .expect("Redis connection failed");
        let config = test_config();

        storage.reset("test_get", &config).await.unwrap();

        let count = storage.get_count("test_get", &config).await.unwrap();
        assert_eq!(count, 0);

        storage.increment("test_get", &config).await.unwrap();
        storage.increment("test_get", &config).await.unwrap();

        let count = storage.get_count("test_get", &config).await.unwrap();
        assert_eq!(count, 2);

        storage.reset("test_get", &config).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_storage_reset() {
        let storage = RedisStorage::new("redis://localhost:6379")
            .await
            .expect("Redis connection failed");
        let config = test_config();

        storage.increment("test_reset", &config).await.unwrap();
        storage.increment("test_reset", &config).await.unwrap();

        storage.reset("test_reset", &config).await.unwrap();

        let count = storage.get_count("test_reset", &config).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_rate_limiter_integration() {
        let storage = RedisStorage::new("redis://localhost:6379")
            .await
            .expect("Redis connection failed");
        let config = RateLimitConfig {
            key: "test_limiter".to_string(),
            max_requests: 3,
            window_seconds: 60,
            enabled: true,
        };

        let limiter = RateLimiter::new(storage.clone(), config.clone());

        // Reset
        limiter.reset("test_limiter_key").await.unwrap();

        // First 3 requests should be allowed
        for i in 1..=3 {
            let response = limiter.check("test_limiter_key").await.unwrap();
            assert!(response.allowed, "Request {} should be allowed", i);
        }

        // 4th request should be rate limited
        let response = limiter.check("test_limiter_key").await.unwrap();
        assert!(!response.allowed, "Request 4 should be rate limited");
        assert!(response.exceeded);

        // Cleanup
        limiter.reset("test_limiter_key").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_middleware_constructor() {
        let config = RateLimitConfig {
            key: "test_mw".to_string(),
            max_requests: 100,
            window_seconds: 60,
            enabled: true,
        };

        let middleware = RateLimitMiddleware::with_redis("redis://localhost:6379", config)
            .await
            .expect("Failed to create Redis middleware");

        let response = middleware.check("test_mw_key").await.unwrap();
        assert!(response.allowed);
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_custom_prefix() {
        let storage = RedisStorage::with_prefix("redis://localhost:6379", "myapp:rl")
            .await
            .expect("Redis connection failed");
        let config = test_config();

        storage.reset("prefix_test", &config).await.unwrap();
        let count = storage.increment("prefix_test", &config).await.unwrap();
        assert_eq!(count, 1);

        storage.reset("prefix_test", &config).await.unwrap();
    }
}
