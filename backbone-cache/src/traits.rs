//! Cache trait definitions

use async_trait::async_trait;
use crate::CacheResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Cache entry metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry<T> {
    /// Cached data
    pub data: T,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Expiration timestamp (None for no expiration)
    pub expires_at: Option<DateTime<Utc>>,

    /// Number of times this entry was accessed
    pub access_count: u64,

    /// Last access timestamp
    pub last_accessed: Option<DateTime<Utc>>,
}

impl<T> CacheEntry<T> {
    /// Create new cache entry
    pub fn new(data: T, ttl_seconds: Option<u64>) -> Self {
        let now = Utc::now();
        let expires_at = ttl_seconds.map(|ttl| now + chrono::Duration::seconds(ttl as i64));

        Self {
            data,
            created_at: now,
            expires_at,
            access_count: 0,
            last_accessed: None,
        }
    }

    /// Check if entry is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|expires_at| Utc::now() > expires_at)
            .unwrap_or(false)
    }

    /// Mark entry as accessed
    pub fn mark_accessed(&mut self) {
        self.access_count += 1;
        self.last_accessed = Some(Utc::now());
    }
}

/// Generic cache trait
#[async_trait]
pub trait Cache: Send + Sync {
    /// Store value in cache
    async fn set<T>(&self, key: &str, value: &T, ttl_seconds: Option<u64>) -> CacheResult<()>
    where
        T: Serialize + Send + Sync;

    /// Get value from cache
    async fn get<T>(&self, key: &str) -> CacheResult<Option<T>>
    where
        T: for<'de> Deserialize<'de> + Send + Sync;

    /// Delete value from cache
    async fn delete(&self, key: &str) -> CacheResult<bool>;

    /// Check if key exists in cache
    async fn exists(&self, key: &str) -> CacheResult<bool>;

    /// Set TTL for existing key
    async fn expire(&self, key: &str, ttl_seconds: u64) -> CacheResult<bool>;

    /// Get TTL for key (None if key doesn't exist or has no expiration)
    async fn ttl(&self, key: &str) -> CacheResult<Option<u64>>;

    /// Clear all cache entries
    async fn clear(&self) -> CacheResult<()>;

    /// Get cache statistics
    async fn stats(&self) -> CacheResult<CacheStats>;

    /// Get multiple values from cache
    async fn mget<T>(&self, keys: Vec<String>) -> CacheResult<Vec<(String, Option<T>)>>
    where
        T: for<'de> Deserialize<'de> + Send + Sync;

    /// Set multiple values in cache
    async fn mset<T>(&self, entries: Vec<(String, T, Option<u64>)>) -> CacheResult<()>
    where
        T: Serialize + Send + Sync;

    /// Delete multiple keys from cache
    async fn mdelete(&self, keys: Vec<String>) -> CacheResult<u64>;
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Total number of entries in cache
    pub total_entries: u64,

    /// Number of entries currently expired but not yet cleaned up
    pub expired_entries: u64,

    /// Total cache size in bytes (if available)
    pub memory_usage: Option<u64>,

    /// Hit rate (0.0 to 1.0)
    pub hit_rate: f64,

    /// Number of successful get operations
    pub hits: u64,

    /// Number of failed get operations (misses)
    pub misses: u64,

    /// Number of set operations
    pub sets: u64,

    /// Number of delete operations
    pub deletes: u64,
}

impl Default for CacheStats {
    fn default() -> Self {
        Self {
            total_entries: 0,
            expired_entries: 0,
            memory_usage: None,
            hit_rate: 0.0,
            hits: 0,
            misses: 0,
            sets: 0,
            deletes: 0,
        }
    }
}

impl CacheStats {
    /// Update hit rate based on hits and misses
    pub fn update_hit_rate(&mut self) {
        let total_requests = self.hits + self.misses;
        if total_requests == 0 {
            return;
        }
        self.hit_rate = self.hits as f64 / total_requests as f64;
    }

    /// Record a cache hit
    pub fn record_hit(&mut self) {
        self.hits += 1;
        self.update_hit_rate();
    }

    /// Record a cache miss
    pub fn record_miss(&mut self) {
        self.misses += 1;
        self.update_hit_rate();
    }

    /// Record a cache set
    pub fn record_set(&mut self) {
        self.sets += 1;
    }

    /// Record a cache delete
    pub fn record_delete(&mut self) {
        self.deletes += 1;
    }
}