//! In-memory cache implementation

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use crate::{CacheResult, CacheError, CacheEntry, CacheStats, Cache};
use chrono::Utc;

/// In-memory cache implementation
pub struct MemoryCache {
    entries: Arc<RwLock<HashMap<String, CacheEntry<Vec<u8>>>>>,
    stats: Arc<RwLock<CacheStats>>,
    max_entries: Option<usize>,
}

impl MemoryCache {
    /// Create new memory cache
    pub fn new(max_entries: Option<usize>) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(CacheStats::default())),
            max_entries,
        }
    }

    /// Remove expired entries
    async fn cleanup_expired(&self) {
        let mut entries = self.entries.write().await;
        let mut expired_count = 0;

        entries.retain(|_, entry| {
            if entry.is_expired() {
                expired_count += 1;
                return false;
            }
            true
        });

        if expired_count > 0 {
            let mut stats = self.stats.write().await;
            stats.expired_entries = 0; // Reset since we just cleaned up
        }
    }

    /// Ensure we don't exceed max entries (LRU eviction)
    async fn evict_if_needed(&self) {
        let Some(max_entries) = self.max_entries else {
            return;
        };

        let mut entries = self.entries.write().await;
        if entries.len() < max_entries {
            return;
        }

        // Find least recently used entry. Entries that have never been
        // accessed fall back to their insertion time, so a freshly inserted
        // entry is considered older than one that has been touched since.
        let Some(key_to_remove) = entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed.unwrap_or(entry.created_at))
            .map(|(k, _)| k.clone())
        else {
            return;
        };

        entries.remove(&key_to_remove);
    }
}

#[async_trait]
impl Cache for MemoryCache {
    async fn set<T>(&self, key: &str, value: &T, ttl_seconds: Option<u64>) -> CacheResult<()>
    where
        T: Serialize + Send + Sync,
    {
        // Cleanup expired entries periodically
        self.cleanup_expired().await;

        // Evict if we're at capacity
        self.evict_if_needed().await;

        // Serialize the value
        let data = serde_json::to_vec(value)
            .map_err(|e| CacheError::Serialization(e.to_string()))?;

        // Create cache entry
        let entry = CacheEntry::new(data, ttl_seconds);

        // Store the entry
        {
            let mut entries = self.entries.write().await;
            entries.insert(key.to_string(), entry);
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.record_set();
            stats.total_entries = self.entries.read().await.len() as u64;
        }

        Ok(())
    }

    async fn get<T>(&self, key: &str) -> CacheResult<Option<T>>
    where
        T: for<'de> Deserialize<'de> + Send + Sync,
    {
        // Cleanup expired entries periodically
        self.cleanup_expired().await;

        let entry = {
            let entries = self.entries.read().await;
            entries.get(key).cloned()
        };

        let Some(mut cache_entry) = entry else {
            // Update stats for miss
            {
                let mut stats = self.stats.write().await;
                stats.record_miss();
            }
            return Ok(None);
        };

        // Check if entry is expired
        if cache_entry.is_expired() {
            // Remove expired entry
            {
                let mut entries = self.entries.write().await;
                entries.remove(key);
            }

            // Update stats for miss
            {
                let mut stats = self.stats.write().await;
                stats.record_miss();
                stats.total_entries = self.entries.read().await.len() as u64;
            }

            return Ok(None);
        }

        // Mark as accessed
        cache_entry.mark_accessed();

        // Deserialize the value before moving
        let value = serde_json::from_slice(&cache_entry.data)
            .map_err(|e| CacheError::Deserialization(e.to_string()))?;

        // Update the entry in the cache
        {
            let mut entries = self.entries.write().await;
            entries.insert(key.to_string(), cache_entry);
        }

        // Update stats for hit
        {
            let mut stats = self.stats.write().await;
            stats.record_hit();
        }

        Ok(Some(value))
    }

    async fn delete(&self, key: &str) -> CacheResult<bool> {
        let removed = {
            let mut entries = self.entries.write().await;
            entries.remove(key).is_some()
        };

        if removed {
            // Update stats
            {
                let mut stats = self.stats.write().await;
                stats.record_delete();
                stats.total_entries = self.entries.read().await.len() as u64;
            }
        }

        Ok(removed)
    }

    async fn exists(&self, key: &str) -> CacheResult<bool> {
        self.cleanup_expired().await;

        let entries = self.entries.read().await;
        Ok(entries.get(key).is_some_and(|entry| !entry.is_expired()))
    }

    async fn expire(&self, key: &str, ttl_seconds: u64) -> CacheResult<bool> {
        let mut entries = self.entries.write().await;

        let Some(entry) = entries.get_mut(key) else {
            return Ok(false);
        };

        entry.expires_at = Some(Utc::now() + chrono::Duration::seconds(ttl_seconds as i64));
        Ok(true)
    }

    async fn ttl(&self, key: &str) -> CacheResult<Option<u64>> {
        self.cleanup_expired().await;

        let entries = self.entries.read().await;
        let Some(entry) = entries.get(key) else {
            return Ok(None); // Key doesn't exist
        };

        let Some(expires_at) = entry.expires_at else {
            return Ok(None); // No expiration
        };

        let ttl = (expires_at - Utc::now()).num_seconds();
        if ttl > 0 {
            return Ok(Some(ttl as u64));
        }
        Ok(Some(0)) // Expired
    }

    async fn clear(&self) -> CacheResult<()> {
        {
            let mut entries = self.entries.write().await;
            entries.clear();
        }

        {
            let mut stats = self.stats.write().await;
            *stats = CacheStats::default();
        }

        Ok(())
    }

    async fn stats(&self) -> CacheResult<CacheStats> {
        self.cleanup_expired().await;

        let stats = self.stats.read().await;
        let entries_count = self.entries.read().await.len() as u64;

        Ok(CacheStats {
            total_entries: entries_count,
            expired_entries: 0, // We just cleaned up
            memory_usage: None, // Could calculate based on serialized size
            hit_rate: stats.hit_rate,
            hits: stats.hits,
            misses: stats.misses,
            sets: stats.sets,
            deletes: stats.deletes,
        })
    }

    async fn mget<T>(&self, keys: Vec<String>) -> CacheResult<Vec<(String, Option<T>)>>
    where
        T: for<'de> Deserialize<'de> + Send + Sync,
    {
        let mut results = Vec::with_capacity(keys.len());

        for key in keys {
            let value = self.get::<T>(&key).await?;
            results.push((key, value));
        }

        Ok(results)
    }

    async fn mset<T>(&self, entries: Vec<(String, T, Option<u64>)>) -> CacheResult<()>
    where
        T: Serialize + Send + Sync,
    {
        for (key, value, ttl) in entries {
            self.set(&key, &value, ttl).await?;
        }

        Ok(())
    }

    async fn mdelete(&self, keys: Vec<String>) -> CacheResult<u64> {
        let mut deleted_count = 0;

        for key in keys {
            if self.delete(&key).await? {
                deleted_count += 1;
            }
        }

        Ok(deleted_count)
    }
}