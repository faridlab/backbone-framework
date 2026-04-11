//! Message Deduplication and Exactly-Once Processing
//!
//! Provides comprehensive deduplication capabilities for queue messages,
//! ensuring exactly-once processing semantics across different queue backends.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{QueueError, QueueMessage, QueueResult};

/// Deduplication strategy
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeduplicationStrategy {
    /// No deduplication
    None,
    /// Based on message deduplication ID
    MessageId,
    /// Based on message content hash
    Content,
    /// Both message ID and content
    Both,
}

/// Deduplication cache backend
#[derive(Clone)]
pub enum DeduplicationCache {
    /// In-memory cache (default)
    Memory,
    /// Redis-based cache
    Redis(redis::Client),
    /// Custom cache backend
    Custom(Arc<dyn DeduplicationCacheBackend + Send + Sync>),
}

impl std::fmt::Debug for DeduplicationCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeduplicationCache::Memory => write!(f, "Memory"),
            DeduplicationCache::Redis(_) => write!(f, "Redis"),
            DeduplicationCache::Custom(_) => write!(f, "Custom"),
        }
    }
}

/// Configuration for message deduplication
#[derive(Clone)]
pub struct DeduplicationConfig {
    /// Deduplication strategy to use
    pub strategy: DeduplicationStrategy,

    /// Deduplication window in seconds
    pub deduplication_window_seconds: u64,

    /// Cache backend to use
    pub cache_backend: DeduplicationCache,

    /// Maximum number of entries to cache
    pub max_cache_entries: usize,

    /// Cleanup interval in seconds
    pub cleanup_interval_seconds: u64,

    /// Enable exactly-once processing
    pub enable_exactly_once: bool,

    /// Exactly-once processing window in seconds
    pub exactly_once_window_seconds: u64,

    /// Enable persistence for exactly-once tracking
    pub enable_persistence: bool,

    /// Storage backend for exactly-once tracking
    pub exactly_once_storage: Option<Arc<dyn ExactlyOnceStorage + Send + Sync>>,
}

impl std::fmt::Debug for DeduplicationConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeduplicationConfig")
            .field("strategy", &self.strategy)
            .field("deduplication_window_seconds", &self.deduplication_window_seconds)
            .field("cache_backend", &self.cache_backend)
            .field("max_cache_entries", &self.max_cache_entries)
            .field("cleanup_interval_seconds", &self.cleanup_interval_seconds)
            .field("enable_exactly_once", &self.enable_exactly_once)
            .field("exactly_once_window_seconds", &self.exactly_once_window_seconds)
            .field("enable_persistence", &self.enable_persistence)
            .field("exactly_once_storage", &self.exactly_once_storage.is_some())
            .finish()
    }
}

impl Default for DeduplicationConfig {
    fn default() -> Self {
        Self {
            strategy: DeduplicationStrategy::MessageId,
            deduplication_window_seconds: 300, // 5 minutes
            cache_backend: DeduplicationCache::Memory,
            max_cache_entries: 100000,
            cleanup_interval_seconds: 60, // 1 minute
            enable_exactly_once: true,
            exactly_once_window_seconds: 3600, // 1 hour
            enable_persistence: false,
            exactly_once_storage: None,
        }
    }
}

/// Deduplication entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduplicationEntry {
    /// Deduplication key
    pub key: String,

    /// Message ID
    pub message_id: String,

    /// Content hash (if applicable)
    pub content_hash: Option<String>,

    /// Timestamp when entry was created
    pub created_at: DateTime<Utc>,

    /// Timestamp when entry expires
    pub expires_at: DateTime<Utc>,

    /// Processing status
    pub status: ProcessingStatus,

    /// Number of times this key was attempted
    pub attempt_count: u64,

    /// Last attempt timestamp
    pub last_attempted_at: Option<DateTime<Utc>>,
}

/// Processing status for deduplication entries
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProcessingStatus {
    /// Pending processing
    Pending,
    /// Processing in progress
    InProgress,
    /// Successfully processed
    Completed,
    /// Failed processing
    Failed,
    /// Expired
    Expired,
}

/// Exactly-once processing record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExactlyOnceRecord {
    /// Unique processing ID
    pub processing_id: String,

    /// Original message ID
    pub message_id: String,

    /// Deduplication key
    pub deduplication_key: String,

    /// Processor identifier
    pub processor_id: String,

    /// Processing started timestamp
    pub started_at: DateTime<Utc>,

    /// Processing completed timestamp (if applicable)
    pub completed_at: Option<DateTime<Utc>>,

    /// Processing result
    pub result: ProcessingResult,

    /// Retry count
    pub retry_count: u32,

    /// Error message (if failed)
    pub error_message: Option<String>,
}

/// Processing result for exactly-once records
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProcessingResult {
    /// Success
    Success,
    /// Failed (will retry)
    Failed,
    /// Failed permanently (no retry)
    PermanentlyFailed,
    /// In progress
    InProgress,
}

/// Deduplication statistics
#[derive(Debug, Clone, Default)]
pub struct DeduplicationStats {
    /// Total messages processed
    pub total_messages: u64,

    /// Total duplicates detected
    pub duplicates_detected: u64,

    /// Total exactly-once violations prevented
    pub exactly_once_violations_prevented: u64,

    /// Cache hits
    pub cache_hits: u64,

    /// Cache misses
    pub cache_misses: u64,

    /// Cache size
    pub cache_size: usize,

    /// Active processing records
    pub active_processing_records: u64,

    /// Completed processing records
    pub completed_processing_records: u64,

    /// Failed processing records
    pub failed_processing_records: u64,

    /// Last cleanup timestamp
    pub last_cleanup_at: Option<DateTime<Utc>>,

    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
}

impl DeduplicationStats {
    /// Update last updated timestamp
    pub fn update_timestamp(&mut self) {
        self.last_updated = Utc::now();
    }

    /// Get duplicate rate
    pub fn duplicate_rate(&self) -> f64 {
        if self.total_messages == 0 {
            0.0
        } else {
            self.duplicates_detected as f64 / self.total_messages as f64
        }
    }

    /// Get cache hit rate
    pub fn cache_hit_rate(&self) -> f64 {
        let total_requests = self.cache_hits + self.cache_misses;
        if total_requests == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total_requests as f64
        }
    }
}

/// Trait for deduplication cache backends
#[async_trait::async_trait]
pub trait DeduplicationCacheBackend {
    /// Check if a key exists in cache
    async fn contains_key(&self, key: &str) -> QueueResult<bool>;

    /// Get entry from cache
    async fn get_entry(&self, key: &str) -> QueueResult<Option<DeduplicationEntry>>;

    /// Store entry in cache
    async fn store_entry(&self, entry: DeduplicationEntry) -> QueueResult<()>;

    /// Remove entry from cache
    async fn remove_entry(&self, key: &str) -> QueueResult<()>;

    /// Clean up expired entries
    async fn cleanup_expired(&self) -> QueueResult<u64>;

    /// Get cache size
    async fn size(&self) -> QueueResult<usize>;

    /// Clear all entries
    async fn clear(&self) -> QueueResult<()>;
}

/// Trait for exactly-once storage backends
#[async_trait::async_trait]
pub trait ExactlyOnceStorage {
    /// Store processing record
    async fn store_record(&self, record: ExactlyOnceRecord) -> QueueResult<()>;

    /// Get processing record
    async fn get_record(&self, processing_id: &str) -> QueueResult<Option<ExactlyOnceRecord>>;

    /// Update processing record
    async fn update_record(&self, record: ExactlyOnceRecord) -> QueueResult<()>;

    /// Check if message is being processed
    async fn is_processing(&self, deduplication_key: &str) -> QueueResult<bool>;

    /// Mark processing as completed
    async fn mark_completed(&self, processing_id: &str, result: ProcessingResult) -> QueueResult<()>;

    /// Clean up old records
    async fn cleanup_old_records(&self, older_than: DateTime<Utc>) -> QueueResult<u64>;

    /// Get records for a deduplication key
    async fn get_records_for_key(&self, deduplication_key: &str) -> QueueResult<Vec<ExactlyOnceRecord>>;
}

/// In-memory deduplication cache implementation
pub struct MemoryDeduplicationCache {
    entries: Arc<RwLock<HashMap<String, DeduplicationEntry>>>,
    max_entries: usize,
}

impl MemoryDeduplicationCache {
    /// Create new memory cache
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
        }
    }
}

#[async_trait::async_trait]
impl DeduplicationCacheBackend for MemoryDeduplicationCache {
    async fn contains_key(&self, key: &str) -> QueueResult<bool> {
        let entries = self.entries.read().await;
        Ok(entries.contains_key(key))
    }

    async fn get_entry(&self, key: &str) -> QueueResult<Option<DeduplicationEntry>> {
        let entries = self.entries.read().await;
        Ok(entries.get(key).cloned())
    }

    async fn store_entry(&self, entry: DeduplicationEntry) -> QueueResult<()> {
        let mut entries = self.entries.write().await;

        // Remove expired entries if cache is full
        if entries.len() >= self.max_entries {
            let now = Utc::now();
            entries.retain(|_, entry| entry.expires_at > now);
        }

        // If still full, remove oldest entry
        if entries.len() >= self.max_entries {
            if let Some(oldest_key) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.created_at)
                .map(|(key, _)| key.clone())
            {
                entries.remove(&oldest_key);
            }
        }

        entries.insert(entry.key.clone(), entry);
        Ok(())
    }

    async fn remove_entry(&self, key: &str) -> QueueResult<()> {
        let mut entries = self.entries.write().await;
        entries.remove(key);
        Ok(())
    }

    async fn cleanup_expired(&self) -> QueueResult<u64> {
        let mut entries = self.entries.write().await;
        let now = Utc::now();
        let initial_count = entries.len();

        entries.retain(|_, entry| entry.expires_at > now);

        Ok((initial_count - entries.len()) as u64)
    }

    async fn size(&self) -> QueueResult<usize> {
        let entries = self.entries.read().await;
        Ok(entries.len())
    }

    async fn clear(&self) -> QueueResult<()> {
        let mut entries = self.entries.write().await;
        entries.clear();
        Ok(())
    }
}

/// In-memory exactly-once storage implementation
pub struct MemoryExactlyOnceStorage {
    records: Arc<RwLock<HashMap<String, ExactlyOnceRecord>>>,
    key_index: Arc<RwLock<HashMap<String, Vec<String>>>>, // deduplication_key -> processing_ids
}

impl Default for MemoryExactlyOnceStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryExactlyOnceStorage {
    /// Create new memory storage
    pub fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(HashMap::new())),
            key_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl ExactlyOnceStorage for MemoryExactlyOnceStorage {
    async fn store_record(&self, record: ExactlyOnceRecord) -> QueueResult<()> {
        let mut records = self.records.write().await;
        let mut key_index = self.key_index.write().await;

        // Add to records
        records.insert(record.processing_id.clone(), record.clone());

        // Add to key index
        key_index
            .entry(record.deduplication_key.clone())
            .or_insert_with(Vec::new)
            .push(record.processing_id.clone());

        Ok(())
    }

    async fn get_record(&self, processing_id: &str) -> QueueResult<Option<ExactlyOnceRecord>> {
        let records = self.records.read().await;
        Ok(records.get(processing_id).cloned())
    }

    async fn update_record(&self, record: ExactlyOnceRecord) -> QueueResult<()> {
        let mut records = self.records.write().await;
        records.insert(record.processing_id.clone(), record);
        Ok(())
    }

    async fn is_processing(&self, deduplication_key: &str) -> QueueResult<bool> {
        let key_index = self.key_index.read().await;
        if let Some(processing_ids) = key_index.get(deduplication_key) {
            let records = self.records.read().await;
            for processing_id in processing_ids {
                if let Some(record) = records.get(processing_id) {
                    if matches!(record.result, ProcessingResult::InProgress) {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    async fn mark_completed(&self, processing_id: &str, result: ProcessingResult) -> QueueResult<()> {
        let mut records = self.records.write().await;
        if let Some(record) = records.get_mut(processing_id) {
            record.result = result.clone();
            record.completed_at = Some(Utc::now());
        }
        Ok(())
    }

    async fn cleanup_old_records(&self, older_than: DateTime<Utc>) -> QueueResult<u64> {
        let mut records = self.records.write().await;
        let mut key_index = self.key_index.write().await;

        let initial_count = records.len();

        // Remove old records
        records.retain(|_, record| record.started_at > older_than);

        // Rebuild key index
        key_index.clear();
        for (processing_id, record) in records.iter() {
            key_index
                .entry(record.deduplication_key.clone())
                .or_insert_with(Vec::new)
                .push(processing_id.clone());
        }

        Ok((initial_count - records.len()) as u64)
    }

    async fn get_records_for_key(&self, deduplication_key: &str) -> QueueResult<Vec<ExactlyOnceRecord>> {
        let key_index = self.key_index.read().await;
        let records = self.records.read().await;

        if let Some(processing_ids) = key_index.get(deduplication_key) {
            let mut result = Vec::new();
            for processing_id in processing_ids {
                if let Some(record) = records.get(processing_id) {
                    result.push(record.clone());
                }
            }
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }
}

/// Message deduplicator service
pub struct MessageDeduplicator {
    config: DeduplicationConfig,
    cache: Arc<dyn DeduplicationCacheBackend + Send + Sync>,
    exactly_once_storage: Option<Arc<dyn ExactlyOnceStorage + Send + Sync>>,
    stats: Arc<RwLock<DeduplicationStats>>,
}

impl MessageDeduplicator {
    /// Create new message deduplicator
    pub fn new(config: DeduplicationConfig) -> QueueResult<Self> {
        let cache: Arc<dyn DeduplicationCacheBackend + Send + Sync> = match &config.cache_backend {
            DeduplicationCache::Memory => Arc::new(MemoryDeduplicationCache::new(config.max_cache_entries)),
            DeduplicationCache::Redis(_) => {
                return Err(QueueError::ConfigError(
                    "Redis deduplication cache not yet implemented".to_string()
                ));
            }
            DeduplicationCache::Custom(cache) => cache.clone(),
        };

        let exactly_once_storage = if config.enable_exactly_once {
            Some(config.exactly_once_storage.clone().unwrap_or_else(|| {
                Arc::new(MemoryExactlyOnceStorage::new()) as Arc<dyn ExactlyOnceStorage + Send + Sync>
            }))
        } else {
            None
        };

        Ok(Self {
            config,
            cache,
            exactly_once_storage,
            stats: Arc::new(RwLock::new(DeduplicationStats::default())),
        })
    }

    /// Check if message is duplicated
    pub async fn is_duplicate(&self, message: &QueueMessage) -> QueueResult<bool> {
        let mut stats = self.stats.write().await;
        stats.total_messages += 1;

        match self.config.strategy {
            DeduplicationStrategy::None => Ok(false),
            DeduplicationStrategy::MessageId => {
                if let Some(dedup_id) = &message.message_deduplication_id {
                    self.is_key_duplicate(dedup_id).await
                } else {
                    Ok(false)
                }
            }
            DeduplicationStrategy::Content => {
                let content_hash = self.generate_content_hash(&message.payload);
                self.is_key_duplicate(&content_hash).await
            }
            DeduplicationStrategy::Both => {
                let mut is_duplicate = false;

                if let Some(dedup_id) = &message.message_deduplication_id {
                    if self.is_key_duplicate(dedup_id).await? {
                        is_duplicate = true;
                    }
                }

                let content_hash = self.generate_content_hash(&message.payload);
                if self.is_key_duplicate(&content_hash).await? {
                    is_duplicate = true;
                }

                Ok(is_duplicate)
            }
        }
    }

    /// Check if key is duplicate
    async fn is_key_duplicate(&self, key: &str) -> QueueResult<bool> {
        let cache_exists = self.cache.contains_key(key).await?;

        let mut stats = self.stats.write().await;
        if cache_exists {
            stats.cache_hits += 1;
            stats.duplicates_detected += 1;
            stats.update_timestamp();
            Ok(true)
        } else {
            stats.cache_misses += 1;
            stats.update_timestamp();
            Ok(false)
        }
    }

    /// Record message for deduplication
    pub async fn record_message(&self, message: &QueueMessage) -> QueueResult<String> {
        let deduplication_key = self.get_deduplication_key(message)?;
        let content_hash = if matches!(self.config.strategy, DeduplicationStrategy::Content | DeduplicationStrategy::Both) {
            Some(self.generate_content_hash(&message.payload))
        } else {
            None
        };

        let entry = DeduplicationEntry {
            key: deduplication_key.clone(),
            message_id: message.id.clone(),
            content_hash,
            created_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(self.config.deduplication_window_seconds as i64),
            status: ProcessingStatus::Pending,
            attempt_count: 1,
            last_attempted_at: Some(Utc::now()),
        };

        self.cache.store_entry(entry).await?;

        Ok(deduplication_key)
    }

    /// Start exactly-once processing
    pub async fn start_processing(&self, message: &QueueMessage, processor_id: &str) -> QueueResult<String> {
        if !self.config.enable_exactly_once {
            return Err(QueueError::ConfigError(
                "Exactly-once processing is not enabled".to_string()
            ));
        }

        let storage = self.exactly_once_storage.as_ref().ok_or(QueueError::ConfigError(
            "Exactly-once storage not configured".to_string()
        ))?;

        let deduplication_key = self.get_deduplication_key(message)?;
        let processing_id = Uuid::new_v4().to_string();

        // Check if already being processed
        if storage.is_processing(&deduplication_key).await? {
            let mut stats = self.stats.write().await;
            stats.exactly_once_violations_prevented += 1;
            stats.update_timestamp();
            return Err(QueueError::Other(
                "Message is already being processed".to_string()
            ));
        }

        let record = ExactlyOnceRecord {
            processing_id: processing_id.clone(),
            message_id: message.id.clone(),
            deduplication_key,
            processor_id: processor_id.to_string(),
            started_at: Utc::now(),
            completed_at: None,
            result: ProcessingResult::InProgress,
            retry_count: 0,
            error_message: None,
        };

        storage.store_record(record).await?;

        Ok(processing_id)
    }

    /// Complete exactly-once processing
    pub async fn complete_processing(
        &self,
        processing_id: &str,
        result: ProcessingResult,
        error_message: Option<String>,
    ) -> QueueResult<()> {
        if !self.config.enable_exactly_once {
            return Ok(());
        }

        let storage = self.exactly_once_storage.as_ref().ok_or(QueueError::ConfigError(
            "Exactly-once storage not configured".to_string()
        ))?;

        // Update record
        if let Some(mut record) = storage.get_record(processing_id).await? {
            record.result = result.clone();
            record.completed_at = Some(Utc::now());
            record.error_message = error_message;

            storage.update_record(record).await?;
        }

        storage.mark_completed(processing_id, result.clone()).await?;

        // Update stats
        let mut stats = self.stats.write().await;
        match result {
            ProcessingResult::Success => {
                stats.completed_processing_records += 1;
            }
            ProcessingResult::PermanentlyFailed => {
                stats.failed_processing_records += 1;
            }
            ProcessingResult::Failed => {
                stats.failed_processing_records += 1;
            }
            ProcessingResult::InProgress => {
                stats.active_processing_records += 1;
            }
        }
        stats.update_timestamp();

        Ok(())
    }

    /// Get deduplication statistics
    pub async fn get_stats(&self) -> DeduplicationStats {
        let mut stats = self.stats.write().await;

        // Update cache size
        if let Ok(cache_size) = self.cache.size().await {
            stats.cache_size = cache_size;
        }

        stats.clone()
    }

    /// Clean up expired entries
    pub async fn cleanup_expired(&self) -> QueueResult<u64> {
        let mut total_cleaned = 0u64;

        // Clean deduplication cache
        let cache_cleaned = self.cache.cleanup_expired().await?;
        total_cleaned += cache_cleaned;

        // Clean exactly-once records
        if let Some(storage) = &self.exactly_once_storage {
            let cutoff = Utc::now() - chrono::Duration::seconds(self.config.exactly_once_window_seconds as i64);
            let storage_cleaned = storage.cleanup_old_records(cutoff).await?;
            total_cleaned += storage_cleaned;
        }

        // Update stats
        let mut stats = self.stats.write().await;
        stats.last_cleanup_at = Some(Utc::now());
        stats.update_timestamp();

        Ok(total_cleaned)
    }

    /// Get deduplication key for message
    fn get_deduplication_key(&self, message: &QueueMessage) -> QueueResult<String> {
        match self.config.strategy {
            DeduplicationStrategy::MessageId | DeduplicationStrategy::Both => {
                if let Some(dedup_id) = &message.message_deduplication_id {
                    Ok(dedup_id.clone())
                } else {
                    Err(QueueError::ConfigError(
                        "Message deduplication ID required for this strategy".to_string()
                    ))
                }
            }
            DeduplicationStrategy::Content => {
                let content_hash = self.generate_content_hash(&message.payload);
                Ok(content_hash)
            }
            DeduplicationStrategy::None => {
                Err(QueueError::ConfigError(
                    "Cannot generate deduplication key for None strategy".to_string()
                ))
            }
        }
    }

    /// Generate content hash
    fn generate_content_hash(&self, content: &serde_json::Value) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

