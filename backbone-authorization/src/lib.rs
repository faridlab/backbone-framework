//! Backbone Framework Authorization
//!
//! Centralized RBAC authorization service with cross-module support.

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod types;
pub mod traits;
pub mod cache;
pub mod service;
pub mod middleware;

/// Redis-backed permission cache (requires `redis` feature)
#[cfg(feature = "redis")]
pub mod redis_cache;

// Re-exports for convenience
pub use types::*;
pub use traits::*;
pub use cache::{PermissionCacheBackend, InMemoryPermissionCache, CacheError};
pub use service::*;
pub use middleware::*;
