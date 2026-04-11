//! Backbone Framework Rate Limiting Module
//!
//! Provides rate limiting middleware for Axum with pluggable storage backends.
//!
//! # Storage Backends
//!
//! - **InMemoryStorage** (default) — Per-instance rate limiting
//! - **RedisStorage** (feature `redis`) — Distributed rate limiting across instances

mod types;
mod storage;
mod limiter;
mod middleware;

#[cfg(feature = "redis")]
mod redis_storage;

pub use types::{RateLimitConfig, RateLimitResponse, RateLimitState, RateLimitError, RateLimitResult};
pub use limiter::RateLimiter;
pub use storage::{StorageBackend, InMemoryStorage};
pub use middleware::{RateLimitMiddleware, rate_limit_middleware, from_config, new as middleware};

#[cfg(feature = "redis")]
pub use redis_storage::RedisStorage;
