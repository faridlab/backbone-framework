//! Redis-backed permission cache
//!
//! Provides distributed permission caching via Redis.
//! Enable with the `redis` feature flag.

use crate::cache::{CacheError, PermissionCacheBackend};
use crate::types::Permission;
use async_trait::async_trait;
use redis::AsyncCommands;
use std::collections::HashSet;

/// Redis-backed permission cache
///
/// Stores serialized permission sets in Redis with TTL support.
/// Suitable for distributed deployments where multiple app instances
/// need to share permission cache state.
pub struct RedisPermissionCache {
    connection: redis::aio::ConnectionManager,
    key_prefix: String,
}

impl RedisPermissionCache {
    /// Create a new Redis permission cache
    pub async fn new(redis_url: &str) -> Result<Self, CacheError> {
        Self::with_prefix(redis_url, "backbone:authz:perms").await
    }

    /// Create a Redis permission cache with a custom key prefix
    pub async fn with_prefix(redis_url: &str, prefix: &str) -> Result<Self, CacheError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| CacheError::ConnectionError(format!("Failed to create Redis client: {}", e)))?;

        let connection = redis::aio::ConnectionManager::new(client)
            .await
            .map_err(|e| CacheError::ConnectionError(format!("Failed to connect to Redis: {}", e)))?;

        Ok(Self {
            connection,
            key_prefix: prefix.to_string(),
        })
    }

    fn build_key(&self, user_id: &str) -> String {
        format!("{}:{}", self.key_prefix, user_id)
    }
}

#[async_trait]
impl PermissionCacheBackend for RedisPermissionCache {
    async fn get(&self, user_id: &str) -> Result<Option<HashSet<Permission>>, CacheError> {
        let key = self.build_key(user_id);
        let mut conn = self.connection.clone();

        let value: Option<String> = conn.get(&key).await
            .map_err(|e| CacheError::OperationFailed(format!("Redis GET failed: {}", e)))?;

        match value {
            Some(json) => {
                let permissions: HashSet<Permission> = serde_json::from_str(&json)
                    .map_err(|e| CacheError::SerializationError(format!("Failed to deserialize permissions: {}", e)))?;
                Ok(Some(permissions))
            }
            None => Ok(None),
        }
    }

    async fn set(
        &self,
        user_id: &str,
        permissions: &HashSet<Permission>,
        ttl_seconds: u64,
    ) -> Result<(), CacheError> {
        let key = self.build_key(user_id);
        let json = serde_json::to_string(permissions)
            .map_err(|e| CacheError::SerializationError(format!("Failed to serialize permissions: {}", e)))?;

        let mut conn = self.connection.clone();
        conn.set_ex::<_, _, ()>(&key, &json, ttl_seconds)
            .await
            .map_err(|e| CacheError::OperationFailed(format!("Redis SET failed: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, user_id: &str) -> Result<(), CacheError> {
        let key = self.build_key(user_id);
        let mut conn = self.connection.clone();

        conn.del::<_, ()>(&key).await
            .map_err(|e| CacheError::OperationFailed(format!("Redis DEL failed: {}", e)))?;

        Ok(())
    }

    async fn clear(&self) -> Result<(), CacheError> {
        let mut conn = self.connection.clone();
        let pattern = format!("{}:*", self.key_prefix);

        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await
            .map_err(|e| CacheError::OperationFailed(format!("Redis KEYS failed: {}", e)))?;

        if !keys.is_empty() {
            for key in &keys {
                conn.del::<_, ()>(key).await
                    .map_err(|e| CacheError::OperationFailed(format!("Redis DEL failed: {}", e)))?;
            }
        }

        Ok(())
    }
}
