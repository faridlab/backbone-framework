//! Redis cache implementation

use redis::{Client, AsyncCommands, aio::ConnectionManager};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use crate::{CacheResult, CacheError, CacheStats, Cache};

/// Redis cache implementation
pub struct RedisCache {
    #[allow(dead_code)] // Client kept for future use (health checks, etc.)
    client: Client,
    connection: ConnectionManager,
    key_prefix: Option<String>,
}

impl RedisCache {
    /// Create new Redis cache
    pub async fn new(redis_url: &str) -> CacheResult<Self> {
        let client = Client::open(redis_url)
            .map_err(|e| CacheError::RedisConnection(e.to_string()))?;

        let connection = client.get_connection_manager()
            .await
            .map_err(|e| CacheError::RedisConnection(e.to_string()))?;

        Ok(Self {
            client,
            connection,
            key_prefix: None,
        })
    }

    /// Create Redis cache with configuration
    pub async fn with_config(redis_url: &str, key_prefix: Option<String>) -> CacheResult<Self> {
        let mut cache = Self::new(redis_url).await?;
        cache.key_prefix = key_prefix;
        Ok(cache)
    }

    /// Build full key with prefix
    fn build_key(&self, key: &str) -> String {
        match &self.key_prefix {
            Some(prefix) => format!("{}:{}", prefix, key),
            None => key.to_string(),
        }
    }

    /// Serialize value to JSON
    fn serialize<T: Serialize>(value: &T) -> CacheResult<String> {
        serde_json::to_string(value)
            .map_err(|e| CacheError::Serialization(e.to_string()))
    }

    /// Deserialize value from JSON
    fn deserialize<T: for<'de> Deserialize<'de>>(value: &str) -> CacheResult<T> {
        serde_json::from_str(value)
            .map_err(|e| CacheError::Deserialization(e.to_string()))
    }
}

#[async_trait]
impl Cache for RedisCache {
    async fn set<T>(&self, key: &str, value: &T, ttl_seconds: Option<u64>) -> CacheResult<()>
    where
        T: Serialize + Send + Sync,
    {
        let full_key = self.build_key(key);
        let serialized = Self::serialize(value)?;

        let mut conn = self.connection.clone();
        let result: redis::RedisResult<()> = match ttl_seconds {
            Some(ttl) => conn.set_ex(&full_key, serialized, ttl).await,
            None => conn.set(&full_key, serialized).await,
        };

        result.map_err(|e| CacheError::RedisOperation(e.to_string()))
    }

    async fn get<T>(&self, key: &str) -> CacheResult<Option<T>>
    where
        T: for<'de> Deserialize<'de> + Send + Sync,
    {
        let full_key = self.build_key(key);
        let mut conn = self.connection.clone();

        let result: Option<String> = conn.get(&full_key).await
            .map_err(|e| CacheError::RedisOperation(e.to_string()))?;

        let Some(value) = result else {
            return Ok(None);
        };

        let deserialized = Self::deserialize::<T>(&value)?;
        Ok(Some(deserialized))
    }

    async fn delete(&self, key: &str) -> CacheResult<bool> {
        let full_key = self.build_key(key);
        let mut conn = self.connection.clone();

        let count: i32 = conn.del(&full_key).await
            .map_err(|e| CacheError::RedisOperation(e.to_string()))?;

        Ok(count > 0)
    }

    async fn exists(&self, key: &str) -> CacheResult<bool> {
        let full_key = self.build_key(key);

        let mut conn = self.connection.clone();
        let result: redis::RedisResult<bool> = conn.exists(&full_key).await;

        result.map_err(|e| CacheError::RedisOperation(e.to_string()))
    }

    async fn expire(&self, key: &str, ttl_seconds: u64) -> CacheResult<bool> {
        let full_key = self.build_key(key);

        let mut conn = self.connection.clone();
        let result: redis::RedisResult<bool> = conn.expire(&full_key, ttl_seconds as i64).await;

        result.map_err(|e| CacheError::RedisOperation(e.to_string()))
    }

    async fn ttl(&self, key: &str) -> CacheResult<Option<u64>> {
        let full_key = self.build_key(key);
        let mut conn = self.connection.clone();

        let ttl: i64 = conn.ttl(&full_key).await
            .map_err(|e| CacheError::RedisOperation(e.to_string()))?;

        match ttl {
            -1 => Ok(None), // No expiration
            -2 => Ok(Some(0)), // Key doesn't exist
            ttl if ttl >= 0 => Ok(Some(ttl as u64)),
            _ => Err(CacheError::RedisOperation(format!("Invalid TTL: {}", ttl))),
        }
    }

    async fn clear(&self) -> CacheResult<()> {
        let mut conn = self.connection.clone();

        // Only delete keys with our prefix to avoid clearing entire Redis database
        let Some(prefix) = &self.key_prefix else {
            return Err(CacheError::Other(
                "Cannot clear Redis cache without key prefix for safety".to_string()
            ));
        };

        let pattern = format!("{}:*", prefix);
        let keys: redis::RedisResult<Vec<String>> = conn.keys(&pattern).await;

        let key_list = keys.map_err(|e| CacheError::RedisOperation(e.to_string()))?;

        // Delete keys if any exist
        if !key_list.is_empty() {
            let _: redis::RedisResult<i32> = conn.del(&key_list).await;
        }

        Ok(())
    }

    async fn stats(&self) -> CacheResult<CacheStats> {
        let mut conn = self.connection.clone();
        let mut stats = CacheStats::default();

        // Get Redis info
        let info: redis::RedisResult<String> = redis::cmd("INFO")
            .arg("memory")
            .arg("stats")
            .query_async(&mut conn)
            .await;

        // Parse Redis info for stats if successful
        if let Ok(info_str) = info {
            for line in info_str.lines() {
                let Some(value_str) = line.split(':').nth(1) else {
                    continue;
                };

                let Ok(value) = value_str.parse::<u64>() else {
                    continue;
                };

                match line {
                    l if l.starts_with("used_memory:") => stats.memory_usage = Some(value),
                    l if l.starts_with("keyspace_hits:") => stats.hits = value,
                    l if l.starts_with("keyspace_misses:") => stats.misses = value,
                    _ => {}
                }
            }
        }

        // Get total keys count (for our namespace only if prefix is set)
        if let Some(prefix) = &self.key_prefix {
            let pattern = format!("{}:*", prefix);
            let keys: redis::RedisResult<Vec<String>> = conn.keys(&pattern).await;

            if let Ok(key_list) = keys {
                stats.total_entries = key_list.len() as u64;
            }
        }

        stats.update_hit_rate();
        Ok(stats)
    }

    async fn mget<T>(&self, keys: Vec<String>) -> CacheResult<Vec<(String, Option<T>)>>
    where
        T: for<'de> Deserialize<'de> + Send + Sync,
    {
        let full_keys: Vec<String> = keys.iter()
            .map(|key| self.build_key(key))
            .collect();

        let mut conn = self.connection.clone();
        let values: Vec<Option<String>> = conn.mget(&full_keys).await
            .map_err(|e| CacheError::RedisOperation(e.to_string()))?;

        let mut results = Vec::with_capacity(keys.len());

        for (i, key) in keys.into_iter().enumerate() {
            let result = match &values[i] {
                Some(value) => {
                    Self::deserialize::<T>(value)
                        .map(Some)
                        .unwrap_or(None)
                }
                None => None,
            };
            results.push((key, result));
        }

        Ok(results)
    }

    async fn mset<T>(&self, entries: Vec<(String, T, Option<u64>)>) -> CacheResult<()>
    where
        T: Serialize + Send + Sync,
    {
        if entries.is_empty() {
            return Ok(());
        }

        // Group entries by TTL to optimize Redis operations
        let mut with_ttl = Vec::new();
        let mut without_ttl = Vec::new();

        for (key, value, ttl) in entries {
            let full_key = self.build_key(&key);
            let serialized = Self::serialize(&value)?;

            match ttl {
                Some(ttl_seconds) => with_ttl.push((full_key, serialized, ttl_seconds)),
                None => without_ttl.push((full_key, serialized)),
            }
        }

        let mut conn = self.connection.clone();

        // Set entries without TTL using MSET
        if !without_ttl.is_empty() {
            let mut pipeline = redis::pipe();
            for (key, value) in without_ttl {
                pipeline.set(&key, value);
            }

            pipeline.query_async::<_, ()>(&mut conn).await
                .map_err(|e| CacheError::RedisOperation(e.to_string()))?;
        }

        // Set entries with TTL individually (Redis doesn't support MSET with TTL)
        for (key, value, ttl_seconds) in with_ttl {
            conn.set_ex::<_, _, ()>(&key, value, ttl_seconds).await
                .map_err(|e| CacheError::RedisOperation(e.to_string()))?;
        }

        Ok(())
    }

    async fn mdelete(&self, keys: Vec<String>) -> CacheResult<u64> {
        if keys.is_empty() {
            return Ok(0);
        }

        let full_keys: Vec<String> = keys.iter()
            .map(|key| self.build_key(key))
            .collect();

        let mut conn = self.connection.clone();
        let count: i32 = conn.del(&full_keys).await
            .map_err(|e| CacheError::RedisOperation(e.to_string()))?;

        Ok(count as u64)
    }
}