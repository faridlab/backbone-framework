//! Backbone Framework Cache Module
//!
//! Provides caching functionality with Redis and in-memory support.
//!
//! ## Features
//!
//! - **Redis Backend**: Distributed caching with Redis
//! - **Memory Backend**: In-process caching for development/testing
//! - **TTL Support**: Time-to-live for cache entries
//! - **Async/Await**: Full async support with tokio
//! - **Generic Types**: Cache any serializable data
//!
//! ## Quick Start
//!
//! ```rust
//! use backbone_cache::{Cache, RedisCache};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize)]
//! struct User {
//!     id: String,
//!     name: String,
//! }
//!
//! // Redis cache
//! let cache = RedisCache::new("redis://localhost:6379").await?;
//!
//! // Store user in cache
//! let user = User { id: "123".to_string(), name: "John".to_string() };
//! cache.set("user:123", &user, Some(3600)).await?;
//!
//! // Retrieve user from cache
//! let cached_user: Option<User> = cache.get("user:123").await?;
//! ```

pub mod memory;
pub mod redis;
pub mod traits;

pub use traits::*;
pub use memory::*;
pub use redis::*;

/// Cache module version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default cache TTL in seconds (1 hour)
pub const DEFAULT_TTL: u64 = 3600;

/// Cache error types
#[derive(thiserror::Error, Debug)]
pub enum CacheError {
    #[error("Redis connection error: {0}")]
    RedisConnection(String),

    #[error("Redis operation error: {0}")]
    RedisOperation(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Cache key not found: {0}")]
    NotFound(String),

    #[error("Cache error: {0}")]
    Other(String),
}

/// Result type for cache operations
pub type CacheResult<T> = Result<T, CacheError>;

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// TTL in seconds for cache entries
    pub default_ttl: u64,

    /// Maximum number of entries in memory cache
    pub max_memory_entries: Option<usize>,

    /// Connection pool size for Redis
    pub redis_pool_size: Option<u32>,

    /// Key prefix for cache entries
    pub key_prefix: Option<String>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            default_ttl: DEFAULT_TTL,
            max_memory_entries: Some(10000),
            redis_pool_size: Some(10),
            key_prefix: None,
        }
    }
}

/// Cache key builder utilities
pub struct CacheKey;

impl CacheKey {
    /// Build cache key with namespace
    pub fn build(namespace: &str, key: &str) -> String {
        format!("{}:{}", namespace, key)
    }

    /// Build user cache key
    pub fn user(user_id: &str) -> String {
        Self::build("user", user_id)
    }

    /// Build session cache key
    pub fn session(session_id: &str) -> String {
        Self::build("session", session_id)
    }

    /// Build API response cache key
    pub fn api_response(path: &str, params: &str) -> String {
        Self::build("api", &format!("{}:{}", path, params))
    }
}