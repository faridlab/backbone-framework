//! Axum middleware for rate limiting

use crate::limiter::RateLimiter;
use crate::storage::{InMemoryStorage, StorageBackend};
use crate::types::{RateLimitConfig, RateLimitResponse};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    middleware::Next,
};
use std::sync::Arc;

/// Rate limiting middleware with pluggable storage backend
///
/// Generic over `B: StorageBackend` to support both in-memory and
/// Redis-backed rate limiting. Use `new()` for in-memory (default)
/// or `with_backend()` / `with_redis()` for other backends.
#[derive(Clone)]
pub struct RateLimitMiddleware<B: StorageBackend = InMemoryStorage> {
    limiter: Arc<RateLimiter<B>>,
}

impl<B: StorageBackend> RateLimitMiddleware<B> {
    /// Create middleware from a storage backend and config
    pub fn with_backend(backend: B, config: RateLimitConfig) -> Self {
        let limiter = Arc::new(RateLimiter::new(backend, config));
        Self { limiter }
    }

    /// Check rate limit for a given key
    pub async fn check(&self, key: &str) -> Result<RateLimitResponse, crate::RateLimitError> {
        self.limiter.check(key).await
    }
}

// Backward-compatible constructors for InMemoryStorage
impl RateLimitMiddleware<InMemoryStorage> {
    /// Create rate limiting middleware with in-memory storage (default)
    pub fn new(config: RateLimitConfig) -> Self {
        let storage = InMemoryStorage::new();
        Self::with_backend(storage, config)
    }
}

#[cfg(feature = "redis")]
impl RateLimitMiddleware<crate::redis_storage::RedisStorage> {
    /// Create rate limiting middleware with Redis storage for distributed limiting
    ///
    /// # Arguments
    ///
    /// * `redis_url` - Redis connection URL (e.g., `redis://localhost:6379`)
    /// * `config` - Rate limit configuration
    ///
    /// # Example
    ///
    /// ```ignore
    /// let middleware = RateLimitMiddleware::with_redis(
    ///     "redis://localhost:6379",
    ///     RateLimitConfig {
    ///         key: "api".to_string(),
    ///         max_requests: 100,
    ///         window_seconds: 60,
    ///         enabled: true,
    ///     },
    /// ).await?;
    /// ```
    pub async fn with_redis(
        redis_url: &str,
        config: RateLimitConfig,
    ) -> crate::types::RateLimitResult<Self> {
        let storage = crate::redis_storage::RedisStorage::new(redis_url).await?;
        Ok(Self::with_backend(storage, config))
    }

    /// Create rate limiting middleware with Redis storage and custom key prefix
    pub async fn with_redis_prefix(
        redis_url: &str,
        prefix: &str,
        config: RateLimitConfig,
    ) -> crate::types::RateLimitResult<Self> {
        let storage = crate::redis_storage::RedisStorage::with_prefix(redis_url, prefix).await?;
        Ok(Self::with_backend(storage, config))
    }
}

/// Rate limiting middleware function for Axum
///
/// Works with any storage backend. Extracts client key from
/// `x-rate-limit-key` header or falls back to "unknown".
pub async fn rate_limit_middleware<B: StorageBackend + 'static>(
    State(middleware): State<Arc<RateLimitMiddleware<B>>>,
    req: Request,
    next: Next,
) -> Result<Response, Response> {
    let key = extract_key(&req);

    match middleware.check(&key).await {
        Ok(response) if response.allowed => {
            Ok(next.run(req).await)
        }
        Ok(response) => {
            let mut res = Json(response).into_response();
            *res.status_mut() = StatusCode::TOO_MANY_REQUESTS;
            Err(res)
        }
        Err(_) => {
            tracing::error!("Rate limit check error for key: {}", key);
            // Fail open: allow the request if rate limit check fails
            Ok(next.run(req).await)
        }
    }
}

fn extract_key(req: &Request) -> String {
    req.headers()
        .get("x-rate-limit-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Use IP address as fallback
            "unknown".to_string()
        })
}

/// Create rate limit middleware from config (backward compatible, in-memory)
pub fn from_config(config: RateLimitConfig) -> Arc<RateLimitMiddleware<InMemoryStorage>> {
    Arc::new(RateLimitMiddleware::new(config))
}

/// Create rate limit middleware with simple parameters (backward compatible, in-memory)
pub fn new(max_requests: u64, window_seconds: u64) -> Arc<RateLimitMiddleware<InMemoryStorage>> {
    let config = RateLimitConfig {
        key: "default".to_string(),
        max_requests,
        window_seconds,
        enabled: true,
    };
    Arc::new(RateLimitMiddleware::new(config))
}
