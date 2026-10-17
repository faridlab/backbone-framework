//! Axum middleware for rate limiting

use crate::limiter::RateLimiter;
use crate::storage::{InMemoryStorage, StorageBackend};
use crate::proxy::TrustedProxies;
use crate::types::{RateLimitConfig, RateLimitResponse};
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    middleware::Next,
};
use std::net::SocketAddr;
use std::sync::Arc;

/// What the middleware does when the counter store fails (e.g. Redis is unreachable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FailMode {
    /// Refuse the request with 503 — the default, so a store outage cannot switch the limit off.
    #[default]
    Closed,
    /// Let the request through. Choose it only for routes where availability matters more than
    /// the limit, and never for login, password reset or other guessing targets.
    Open,
}

/// Derives a rate-limit key from a request; `None` falls back to the client address.
pub type KeyExtractor = Arc<dyn Fn(&Request) -> Option<String> + Send + Sync>;

/// Rate limiting middleware with pluggable storage backend
///
/// Generic over `B: StorageBackend` to support both in-memory and
/// Redis-backed rate limiting. Use `new()` for in-memory (default)
/// or `with_backend()` / `with_redis()` for other backends.
///
/// Requests are counted per client address: the connection's peer address, which needs the
/// server to run with `into_make_service_with_connect_info::<SocketAddr>()`. Behind a reverse
/// proxy, declare the proxy with [`with_trusted_proxies`](Self::with_trusted_proxies) so the
/// forwarded client address is used instead of the proxy's. An authenticated route can count
/// per principal with [`with_key_extractor`](Self::with_key_extractor).
#[derive(Clone)]
pub struct RateLimitMiddleware<B: StorageBackend = InMemoryStorage> {
    limiter: Arc<RateLimiter<B>>,
    trusted_proxies: TrustedProxies,
    key_extractor: Option<KeyExtractor>,
    fail_mode: FailMode,
}

impl<B: StorageBackend> RateLimitMiddleware<B> {
    /// Create middleware from a storage backend and config
    pub fn with_backend(backend: B, config: RateLimitConfig) -> Self {
        let limiter = Arc::new(RateLimiter::new(backend, config));
        Self {
            limiter,
            trusted_proxies: TrustedProxies::default(),
            key_extractor: None,
            fail_mode: FailMode::default(),
        }
    }

    /// Trust `X-Forwarded-For` / `X-Real-IP` from these peers (the reverse proxy in front of the
    /// service). From any other peer the headers are ignored, since a client can write them.
    #[must_use]
    pub fn with_trusted_proxies(mut self, proxies: TrustedProxies) -> Self {
        self.trusted_proxies = proxies;
        self
    }

    /// Key requests with `extractor` — e.g. the authenticated user id an auth layer put in the
    /// request extensions. A request it returns `None` for is keyed by client address. Mount the
    /// limiter inside the layer that proves the value, never on a client-supplied one.
    #[must_use]
    pub fn with_key_extractor<F>(mut self, extractor: F) -> Self
    where
        F: Fn(&Request) -> Option<String> + Send + Sync + 'static,
    {
        self.key_extractor = Some(Arc::new(extractor));
        self
    }

    /// What to do when the counter store fails; [`FailMode::Closed`] unless set.
    #[must_use]
    pub fn with_fail_mode(mut self, mode: FailMode) -> Self {
        self.fail_mode = mode;
        self
    }

    /// Check rate limit for a given key
    pub async fn check(&self, key: &str) -> Result<RateLimitResponse, crate::RateLimitError> {
        self.limiter.check(key).await
    }

    /// The key a request is counted against: the extractor's key when it yields one, else the
    /// client address (see [`TrustedProxies`]), else one shared bucket for requests whose peer
    /// address is unknown.
    pub fn client_key(&self, req: &Request) -> String {
        if let Some(key) = self.key_extractor.as_ref().and_then(|extract| extract(req)) {
            return format!("key:{key}");
        }
        let peer = req
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip());
        match peer {
            Some(peer) => format!("ip:{}", self.trusted_proxies.client_ip(peer, req.headers())),
            None => UNKNOWN_PEER_KEY.to_string(),
        }
    }
}

/// The shared bucket for requests that carry no peer address — a server run without connect
/// info. All of them share one budget, which limits rather than exempts them.
const UNKNOWN_PEER_KEY: &str = "peer:unknown";

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
/// Works with any storage backend. Counts each request against
/// [`RateLimitMiddleware::client_key`]; a refused request gets 429, and a counter-store failure
/// gets 503 unless the middleware was built with [`FailMode::Open`].
pub async fn rate_limit_middleware<B: StorageBackend + 'static>(
    State(middleware): State<Arc<RateLimitMiddleware<B>>>,
    req: Request,
    next: Next,
) -> Result<Response, Response> {
    let key = middleware.client_key(&req);

    match middleware.check(&key).await {
        Ok(response) if response.allowed => {
            Ok(next.run(req).await)
        }
        Ok(response) => {
            let mut res = Json(response).into_response();
            *res.status_mut() = StatusCode::TOO_MANY_REQUESTS;
            Err(res)
        }
        Err(error) => match middleware.fail_mode {
            FailMode::Open => {
                tracing::error!(%error, key = %key, "rate limit check failed; letting the request through");
                Ok(next.run(req).await)
            }
            FailMode::Closed => {
                tracing::error!(%error, key = %key, "rate limit check failed; refusing the request");
                let body = serde_json::json!({
                    "error": "rate_limit_unavailable",
                    "message": "The request could not be checked against the rate limit. Try again shortly.",
                });
                let mut res = Json(body).into_response();
                *res.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
                res.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
                Err(res)
            }
        },
    }
}

/// Create rate limit middleware from config (backward compatible, in-memory)
pub fn from_config(config: RateLimitConfig) -> Arc<RateLimitMiddleware<InMemoryStorage>> {
    Arc::new(RateLimitMiddleware::new(config))
}

/// Create rate limit middleware with simple parameters (backward compatible, in-memory)
pub fn new(max_requests: u64, window_seconds: u64) -> Arc<RateLimitMiddleware<InMemoryStorage>> {
    let config = RateLimitConfig::new("default", max_requests, window_seconds, true);
    Arc::new(RateLimitMiddleware::new(config))
}
