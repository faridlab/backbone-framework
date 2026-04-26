//! Dual-backend storage with graceful Redis fallback to in-memory.
//!
//! In production you want Redis (shared state across replicas), but in dev
//! you don't want to require a Redis container just to start the service.
//! `DualStorage` lets you ask for Redis and silently fall back to
//! `InMemoryStorage` (with a structured warning) when Redis is unreachable.
//!
//! ## Quick start
//!
//! ```no_run
//! # #[cfg(feature = "redis")]
//! # async fn _example() -> anyhow::Result<()> {
//! use backbone_rate_limit::{
//!     dual::{from_config_with_fallback, FallbackPolicy},
//!     RateLimitConfig,
//! };
//!
//! let config = RateLimitConfig {
//!     key: "api".to_string(),
//!     max_requests: 100,
//!     window_seconds: 60,
//!     enabled: true,
//! };
//! let middleware = from_config_with_fallback(
//!     config,
//!     Some("redis://localhost:6379"),
//!     FallbackPolicy::FallbackInDev,
//! ).await?;
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::storage::{InMemoryStorage, StorageBackend};
use crate::types::{RateLimitConfig, RateLimitError, RateLimitResult};

#[cfg(feature = "redis")]
use crate::redis_storage::RedisStorage;

/// Storage backend that dispatches to either Redis or in-memory.
///
/// Constructed by [`from_config_with_fallback`]; consumers normally don't
/// build this directly.
#[derive(Clone)]
pub enum DualStorage {
    /// Backed by Redis (distributed across instances).
    #[cfg(feature = "redis")]
    Redis(RedisStorage),
    /// Backed by in-process state (single instance only).
    InMemory(InMemoryStorage),
}

#[async_trait::async_trait]
impl StorageBackend for DualStorage {
    async fn increment(&self, key: &str, config: &RateLimitConfig) -> RateLimitResult<u64> {
        match self {
            #[cfg(feature = "redis")]
            DualStorage::Redis(s) => s.increment(key, config).await,
            DualStorage::InMemory(s) => s.increment(key, config).await,
        }
    }

    async fn get_count(&self, key: &str, config: &RateLimitConfig) -> RateLimitResult<u64> {
        match self {
            #[cfg(feature = "redis")]
            DualStorage::Redis(s) => s.get_count(key, config).await,
            DualStorage::InMemory(s) => s.get_count(key, config).await,
        }
    }

    async fn reset(&self, key: &str, config: &RateLimitConfig) -> RateLimitResult<()> {
        match self {
            #[cfg(feature = "redis")]
            DualStorage::Redis(s) => s.reset(key, config).await,
            DualStorage::InMemory(s) => s.reset(key, config).await,
        }
    }
}

/// What to do when Redis is requested but unreachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackPolicy {
    /// Always fall back to in-memory and log a warning. Best for dev where
    /// any Redis blip shouldn't kill the process.
    AlwaysFallback,

    /// Only fall back when the supplied environment string looks like dev
    /// (`"dev" | "development" | "local"`). In any other env, return the
    /// original Redis connection error so the operator notices.
    FallbackInDev,

    /// Never fall back — always return the connection error. Best when
    /// rate-limit correctness is a hard requirement (e.g. abuse prevention).
    NeverFallback,
}

/// Build a [`DualStorage`] from optional Redis URL + fallback policy. When
/// `redis_url` is `None` the in-memory backend is selected directly without
/// emitting a warning (this is the explicit "no Redis configured" path).
pub async fn build(
    redis_url: Option<&str>,
    policy: FallbackPolicy,
    env: &str,
) -> RateLimitResult<DualStorage> {
    let Some(url) = redis_url.filter(|u| !u.is_empty()) else {
        ::tracing::info!("Rate limiter: no Redis URL configured, using in-memory backend");
        return Ok(DualStorage::InMemory(InMemoryStorage::new()));
    };

    #[cfg(feature = "redis")]
    {
        match RedisStorage::new(url).await {
            Ok(redis) => {
                ::tracing::info!(
                    "Rate limiter: connected to Redis at {} — distributed mode",
                    url
                );
                Ok(DualStorage::Redis(redis))
            }
            Err(e) => apply_fallback(policy, env, e),
        }
    }
    #[cfg(not(feature = "redis"))]
    {
        let _ = url;
        // Crate built without `redis` feature — only in-memory is possible.
        // This is a build/configuration error (URL provided but no Redis
        // support compiled in), not a runtime connection failure. Fallback
        // policy still controls whether we warn or fail loud.
        let err = RateLimitError::RedisConnection(
            "redis URL provided but the `redis` feature was not compiled in — \
             rebuild with `--features redis` or unset the URL"
                .to_string(),
        );
        apply_fallback(policy, env, err)
    }
}

fn apply_fallback(
    policy: FallbackPolicy,
    env: &str,
    err: RateLimitError,
) -> RateLimitResult<DualStorage> {
    match policy {
        FallbackPolicy::AlwaysFallback => {
            ::tracing::warn!(
                "Rate limiter: Redis unavailable ({}), falling back to in-memory backend",
                err
            );
            Ok(DualStorage::InMemory(InMemoryStorage::new()))
        }
        FallbackPolicy::FallbackInDev if is_dev_env(env) => {
            ::tracing::warn!(
                "Rate limiter: Redis unavailable in dev env '{}' ({}), \
                 falling back to in-memory backend",
                env,
                err
            );
            Ok(DualStorage::InMemory(InMemoryStorage::new()))
        }
        _ => {
            ::tracing::error!(
                "Rate limiter: Redis unavailable in env '{}' ({}). \
                 Refusing to fall back per FallbackPolicy.",
                env,
                err
            );
            Err(err)
        }
    }
}

fn is_dev_env(env: &str) -> bool {
    matches!(
        env.to_ascii_lowercase().as_str(),
        "dev" | "development" | "local"
    )
}

/// Convenience: build a fully-wired [`crate::RateLimitMiddleware`] with the
/// dual backend. Reads the deployment env from the `APP_ENV` env var
/// (defaulting to `"dev"`) — this is the most common call site for consumer
/// apps that already follow the framework's `APP_ENV` convention.
///
/// If your service uses a different env-var name, or loads the env from
/// config, prefer [`from_config_with_fallback_env`] and pass it explicitly.
pub async fn from_config_with_fallback(
    config: RateLimitConfig,
    redis_url: Option<&str>,
    policy: FallbackPolicy,
) -> RateLimitResult<Arc<crate::RateLimitMiddleware<DualStorage>>> {
    let env = std::env::var("APP_ENV").unwrap_or_else(|_| "dev".to_string());
    from_config_with_fallback_env(config, redis_url, policy, &env).await
}

/// Same as [`from_config_with_fallback`] but takes the deployment env
/// explicitly instead of reading `APP_ENV`. Use this when your service loads
/// the env from config or uses a non-standard env-var name.
pub async fn from_config_with_fallback_env(
    config: RateLimitConfig,
    redis_url: Option<&str>,
    policy: FallbackPolicy,
    env: &str,
) -> RateLimitResult<Arc<crate::RateLimitMiddleware<DualStorage>>> {
    let storage = build(redis_url, policy, env).await?;
    Ok(Arc::new(crate::RateLimitMiddleware::with_backend(
        storage, config,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> RateLimitConfig {
        RateLimitConfig {
            key: "t".to_string(),
            max_requests: 10,
            window_seconds: 60,
            enabled: true,
        }
    }

    #[tokio::test]
    async fn no_redis_url_returns_in_memory_silently() {
        let s = build(None, FallbackPolicy::NeverFallback, "production")
            .await
            .unwrap();
        assert!(matches!(s, DualStorage::InMemory(_)));
        // Sanity-check it actually works
        let c = s.increment("k", &test_config()).await.unwrap();
        assert_eq!(c, 1);
    }

    #[tokio::test]
    async fn empty_redis_url_returns_in_memory_silently() {
        let s = build(Some(""), FallbackPolicy::NeverFallback, "production")
            .await
            .unwrap();
        assert!(matches!(s, DualStorage::InMemory(_)));
    }

    #[tokio::test]
    async fn fallback_in_dev_falls_back_when_dev() {
        // Use an unreachable Redis URL to force connection failure
        let s = build(
            Some("redis://127.0.0.1:1"),
            FallbackPolicy::FallbackInDev,
            "dev",
        )
        .await;
        // Either we have the redis feature and connection failed → fell back,
        // or we don't have the redis feature → fallback path took effect.
        assert!(matches!(s.unwrap(), DualStorage::InMemory(_)));
    }

    #[tokio::test]
    async fn fallback_in_dev_errors_in_prod() {
        let s = build(
            Some("redis://127.0.0.1:1"),
            FallbackPolicy::FallbackInDev,
            "production",
        )
        .await;
        assert!(s.is_err());
    }

    #[tokio::test]
    async fn never_fallback_errors_when_redis_unreachable() {
        let s = build(
            Some("redis://127.0.0.1:1"),
            FallbackPolicy::NeverFallback,
            "dev",
        )
        .await;
        assert!(s.is_err());
    }

    #[tokio::test]
    async fn always_fallback_falls_back_in_any_env() {
        let s = build(
            Some("redis://127.0.0.1:1"),
            FallbackPolicy::AlwaysFallback,
            "production",
        )
        .await
        .unwrap();
        assert!(matches!(s, DualStorage::InMemory(_)));
    }
}
