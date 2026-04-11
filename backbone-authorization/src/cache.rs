//! Permission cache backend trait and in-memory implementation
//!
//! Provides a pluggable cache backend for permission caching.
//! The default is `InMemoryPermissionCache`; enable the `redis` feature
//! for distributed caching via `RedisPermissionCache`.

use crate::types::Permission;
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Errors from cache operations
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// Cache operation failed
    #[error("Cache operation failed: {0}")]
    OperationFailed(String),

    /// Serialization error (e.g., when using Redis)
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Connection error (e.g., Redis connection lost)
    #[error("Connection error: {0}")]
    ConnectionError(String),
}

/// Trait for permission cache backends
///
/// Implementations must be Send + Sync for use across async tasks.
#[async_trait]
pub trait PermissionCacheBackend: Send + Sync {
    /// Get cached permissions for a user
    ///
    /// Returns `None` if the user's permissions are not cached or have expired.
    async fn get(&self, user_id: &str) -> Result<Option<HashSet<Permission>>, CacheError>;

    /// Cache permissions for a user with a TTL
    async fn set(
        &self,
        user_id: &str,
        permissions: &HashSet<Permission>,
        ttl_seconds: u64,
    ) -> Result<(), CacheError>;

    /// Delete cached permissions for a specific user
    async fn delete(&self, user_id: &str) -> Result<(), CacheError>;

    /// Clear all cached permissions
    async fn clear(&self) -> Result<(), CacheError>;
}

/// Cached entry with expiration
struct CachedEntry {
    permissions: HashSet<Permission>,
    expires_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory permission cache using HashMap with TTL
///
/// This is the default cache backend. Permissions are stored in-memory
/// with per-entry expiration timestamps.
pub struct InMemoryPermissionCache {
    cache: Arc<RwLock<HashMap<String, CachedEntry>>>,
}

impl InMemoryPermissionCache {
    /// Create a new in-memory permission cache
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryPermissionCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PermissionCacheBackend for InMemoryPermissionCache {
    async fn get(&self, user_id: &str) -> Result<Option<HashSet<Permission>>, CacheError> {
        let cache = self.cache.read().await;
        if let Some(entry) = cache.get(user_id) {
            if entry.expires_at > chrono::Utc::now() {
                return Ok(Some(entry.permissions.clone()));
            }
        }
        Ok(None)
    }

    async fn set(
        &self,
        user_id: &str,
        permissions: &HashSet<Permission>,
        ttl_seconds: u64,
    ) -> Result<(), CacheError> {
        let expires_at = chrono::Utc::now() + chrono::Duration::seconds(ttl_seconds as i64);
        let entry = CachedEntry {
            permissions: permissions.clone(),
            expires_at,
        };
        let mut cache = self.cache.write().await;
        cache.insert(user_id.to_string(), entry);
        Ok(())
    }

    async fn delete(&self, user_id: &str) -> Result<(), CacheError> {
        let mut cache = self.cache.write().await;
        cache.remove(user_id);
        Ok(())
    }

    async fn clear(&self) -> Result<(), CacheError> {
        let mut cache = self.cache.write().await;
        cache.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Permission, UserAction, RoleAction};

    fn test_permissions() -> HashSet<Permission> {
        let mut perms = HashSet::new();
        perms.insert(Permission::User(UserAction::Read));
        perms.insert(Permission::Role(RoleAction::List));
        perms
    }

    #[tokio::test]
    async fn test_inmemory_get_set() {
        let cache = InMemoryPermissionCache::new();
        let perms = test_permissions();

        cache.set("user1", &perms, 300).await.unwrap();

        let result = cache.get("user1").await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), perms);

        // Non-existent user returns None
        let result = cache.get("user2").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_inmemory_ttl_expiry() {
        let cache = InMemoryPermissionCache::new();
        let perms = test_permissions();

        // Set with 1 second TTL
        cache.set("user1", &perms, 1).await.unwrap();

        // Should be present immediately
        assert!(cache.get("user1").await.unwrap().is_some());

        // Wait for expiry
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Should be expired
        assert!(cache.get("user1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_inmemory_delete() {
        let cache = InMemoryPermissionCache::new();
        let perms = test_permissions();

        cache.set("user1", &perms, 300).await.unwrap();
        assert!(cache.get("user1").await.unwrap().is_some());

        cache.delete("user1").await.unwrap();
        assert!(cache.get("user1").await.unwrap().is_none());

        // Deleting non-existent key is not an error
        cache.delete("nonexistent").await.unwrap();
    }

    #[tokio::test]
    async fn test_inmemory_clear() {
        let cache = InMemoryPermissionCache::new();
        let perms = test_permissions();

        cache.set("user1", &perms, 300).await.unwrap();
        cache.set("user2", &perms, 300).await.unwrap();
        cache.set("user3", &perms, 300).await.unwrap();

        cache.clear().await.unwrap();

        assert!(cache.get("user1").await.unwrap().is_none());
        assert!(cache.get("user2").await.unwrap().is_none());
        assert!(cache.get("user3").await.unwrap().is_none());
    }
}
