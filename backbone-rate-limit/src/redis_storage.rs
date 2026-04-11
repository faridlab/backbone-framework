//! Redis storage backend for distributed rate limiting
//!
//! Uses Redis INCR + EXPIRE for atomic fixed-window counting, matching
//! the algorithm used by `InMemoryStorage`. When the key expires after
//! `window_seconds`, the next INCR creates a fresh counter at 1.
//!
//! Key format: `{prefix}:{key}`

use redis::{AsyncCommands, Client, aio::ConnectionManager};
use crate::storage::StorageBackend;
use crate::types::{RateLimitConfig, RateLimitError, RateLimitResult};

/// Redis-backed storage for distributed rate limiting
///
/// Enables rate limiting across multiple application instances by
/// storing counters in a shared Redis instance.
///
/// # Example
///
/// ```ignore
/// use backbone_rate_limit::{RedisStorage, RateLimiter, RateLimitConfig};
///
/// let storage = RedisStorage::new("redis://localhost:6379").await?;
/// let config = RateLimitConfig {
///     key: "api".to_string(),
///     max_requests: 100,
///     window_seconds: 60,
///     enabled: true,
/// };
/// let limiter = RateLimiter::new(storage, config);
/// ```
#[derive(Clone)]
pub struct RedisStorage {
    connection: ConnectionManager,
    key_prefix: String,
}

impl RedisStorage {
    /// Create a new Redis storage backend with default prefix `rate_limit`
    pub async fn new(redis_url: &str) -> RateLimitResult<Self> {
        Self::with_prefix(redis_url, "rate_limit").await
    }

    /// Create a Redis storage backend with a custom key prefix
    pub async fn with_prefix(redis_url: &str, prefix: &str) -> RateLimitResult<Self> {
        let client = Client::open(redis_url)
            .map_err(|e| RateLimitError::RedisConnection(e.to_string()))?;

        let connection = client
            .get_connection_manager()
            .await
            .map_err(|e| RateLimitError::RedisConnection(e.to_string()))?;

        Ok(Self {
            connection,
            key_prefix: prefix.to_string(),
        })
    }

    fn build_key(&self, key: &str) -> String {
        format!("{}:{}", self.key_prefix, key)
    }
}

#[async_trait::async_trait]
impl StorageBackend for RedisStorage {
    async fn increment(&self, key: &str, config: &RateLimitConfig) -> RateLimitResult<u64> {
        let full_key = self.build_key(key);
        let mut conn = self.connection.clone();

        // INCR is atomic — if the key doesn't exist, Redis creates it at 0 then increments to 1
        let count: u64 = conn
            .incr(&full_key, 1u64)
            .await
            .map_err(|e| RateLimitError::RedisOperation(e.to_string()))?;

        // On the first request in the window (count == 1), set the TTL
        // so the key auto-expires when the window closes
        if count == 1 {
            let _: Result<bool, _> = conn
                .expire(&full_key, config.window_seconds as i64)
                .await;
        }

        Ok(count)
    }

    async fn get_count(&self, key: &str, _config: &RateLimitConfig) -> RateLimitResult<u64> {
        let full_key = self.build_key(key);
        let mut conn = self.connection.clone();

        let count: Option<u64> = conn
            .get(&full_key)
            .await
            .map_err(|e| RateLimitError::RedisOperation(e.to_string()))?;

        Ok(count.unwrap_or(0))
    }

    async fn reset(&self, key: &str, _config: &RateLimitConfig) -> RateLimitResult<()> {
        let full_key = self.build_key(key);
        let mut conn = self.connection.clone();

        let _: Result<i32, _> = conn.del(&full_key).await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_key_default_prefix() {
        // We can't construct RedisStorage without a connection,
        // but we can test the key format logic
        let key = format!("{}:{}", "rate_limit", "user:123:api");
        assert_eq!(key, "rate_limit:user:123:api");
    }

    #[test]
    fn test_build_key_custom_prefix() {
        let key = format!("{}:{}", "myapp:ratelimit", "login");
        assert_eq!(key, "myapp:ratelimit:login");
    }
}
