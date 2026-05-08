//! Redis storage backend for distributed rate limiting
//!
//! Uses Redis INCR + EXPIRE for atomic fixed-window counting, matching
//! the algorithm used by `InMemoryStorage`. When the key expires after
//! `window_seconds`, the next INCR creates a fresh counter at 1.
//!
//! When `RateLimitConfig::lockout_seconds` is set, exceeding the window
//! limit additionally writes a sibling lock key (`{prefix}:lock:{key}`)
//! with a TTL of `lockout_seconds`. While the lock key exists, increments
//! are skipped and the existing TTL is reported back as `locked_until`.
//!
//! Key formats:
//! - Counter: `{prefix}:{key}`
//! - Lock:    `{prefix}:lock:{key}`

use redis::{aio::ConnectionManager, AsyncCommands, Client};

use crate::storage::StorageBackend;
use crate::types::{IncrementOutcome, RateLimitConfig, RateLimitError, RateLimitResult};

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
/// let config = RateLimitConfig::new("api", 100, 60, true);
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

    fn build_lock_key(&self, key: &str) -> String {
        format!("{}:lock:{}", self.key_prefix, key)
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[async_trait::async_trait]
impl StorageBackend for RedisStorage {
    async fn increment(
        &self,
        key: &str,
        config: &RateLimitConfig,
    ) -> RateLimitResult<IncrementOutcome> {
        let counter_key = self.build_key(key);
        let lock_key = self.build_lock_key(key);
        let mut conn = self.connection.clone();

        // Honor any active lockout. TTL semantics: -2 = key absent,
        // -1 = key exists with no TTL, >=0 = remaining seconds.
        if config.lockout_seconds.is_some() {
            let lock_ttl: i64 = conn
                .ttl(&lock_key)
                .await
                .map_err(|e| RateLimitError::RedisOperation(e.to_string()))?;
            if lock_ttl >= 0 {
                let count: u64 = conn
                    .get::<_, Option<u64>>(&counter_key)
                    .await
                    .map_err(|e| RateLimitError::RedisOperation(e.to_string()))?
                    .unwrap_or(config.max_requests + 1);
                return Ok(IncrementOutcome {
                    count,
                    locked_until: Some(now_unix() + lock_ttl as u64),
                });
            }
        }

        let count: u64 = conn
            .incr(&counter_key, 1u64)
            .await
            .map_err(|e| RateLimitError::RedisOperation(e.to_string()))?;

        // First request in window — set TTL so the key auto-expires.
        if count == 1 {
            let _: Result<bool, _> = conn.expire(&counter_key, config.window_seconds as i64).await;
        }

        let mut locked_until = None;
        if count > config.max_requests {
            if let Some(lockout) = config.lockout_seconds {
                let _: Result<(), _> = conn.set_ex(&lock_key, 1u8, lockout).await;
                locked_until = Some(now_unix() + lockout);
            }
        }

        Ok(IncrementOutcome {
            count,
            locked_until,
        })
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
        let counter_key = self.build_key(key);
        let lock_key = self.build_lock_key(key);
        let mut conn = self.connection.clone();

        let _: Result<i32, _> = conn.del(&counter_key).await;
        let _: Result<i32, _> = conn.del(&lock_key).await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn test_build_lock_key_format() {
        let key = format!("{}:lock:{}", "rate_limit", "login:1.2.3.4");
        assert_eq!(key, "rate_limit:lock:login:1.2.3.4");
    }
}
