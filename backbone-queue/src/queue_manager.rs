//! Queue Manager Module
//!
//! Provides administrative operations for managing queues,
//! including creation, configuration, monitoring, and maintenance.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use tokio::sync::RwLock;
use tracing::info;

use crate::{
    QueueService, QueueResult, QueueStats, QueueHealthCheck, QueueError,
    monitoring::{QueueMonitorService, QueueMetrics, AlertThresholds},
    compression::{MessageCompressor, CompressionConfig},
    fifo::{FifoQueueServiceWrapper, FifoQueueConfig, FifoQueueService, utils::MessageVolume},
};

/// Queue configuration information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    /// Queue name
    pub name: String,

    /// Queue type (redis, sqs)
    pub queue_type: String,

    /// Connection URL
    pub connection_url: String,

    /// Maximum queue size
    pub max_size: Option<usize>,

    /// Message retention period in seconds
    pub retention_seconds: Option<u64>,

    /// Dead letter queue name
    pub dead_letter_queue: Option<String>,

    /// Visibility timeout in seconds
    pub visibility_timeout: u64,

    /// Maximum receive count
    pub max_receive_count: u64,

    /// Enable FIFO
    pub fifo_enabled: bool,

    /// FIFO configuration
    pub fifo_config: Option<FifoQueueConfig>,

    /// Enable compression
    pub compression_enabled: bool,

    /// Compression configuration
    pub compression_config: Option<CompressionConfig>,

    /// Enable monitoring
    pub monitoring_enabled: bool,

    /// Monitoring thresholds
    pub alert_thresholds: Option<AlertThresholds>,

    /// Queue metadata
    pub metadata: HashMap<String, String>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

impl QueueConfig {
    /// Create new queue configuration
    pub fn new(name: String, queue_type: String, connection_url: String) -> Self {
        let now = Utc::now();
        Self {
            name,
            queue_type,
            connection_url,
            max_size: None,
            retention_seconds: None,
            dead_letter_queue: None,
            visibility_timeout: 30,
            max_receive_count: 3,
            fifo_enabled: false,
            fifo_config: None,
            compression_enabled: false,
            compression_config: None,
            monitoring_enabled: true,
            alert_thresholds: Some(AlertThresholds::default()),
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Update timestamp
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Queue name cannot be empty".to_string());
        }

        if self.queue_type.is_empty() {
            return Err("Queue type cannot be empty".to_string());
        }

        if self.connection_url.is_empty() {
            return Err("Connection URL cannot be empty".to_string());
        }

        if self.visibility_timeout == 0 {
            return Err("Visibility timeout must be greater than 0".to_string());
        }

        if self.max_receive_count == 0 {
            return Err("Max receive count must be greater than 0".to_string());
        }

        // Validate FIFO configuration
        if self.fifo_enabled {
            if let Some(ref fifo_config) = self.fifo_config {
                let errors = crate::fifo::utils::validate_config(fifo_config);
                if !errors.is_empty() {
                    return Err(format!("FIFO configuration errors: {}", errors.join(", ")));
                }
            }
        }

        Ok(())
    }
}

/// Queue manager for administrative operations
pub struct QueueManager {
    /// Registered queues
    queues: Arc<RwLock<HashMap<String, Arc<dyn QueueService + Send + Sync>>>>,

    /// Queue configurations
    configs: Arc<RwLock<HashMap<String, QueueConfig>>>,

    /// Queue monitors
    monitors: Arc<RwLock<HashMap<String, Arc<QueueMonitorService>>>>,

    /// Message compressors
    compressors: Arc<RwLock<HashMap<String, Arc<MessageCompressor>>>>,

    /// FIFO wrappers
    fifo_wrappers: Arc<RwLock<HashMap<String, Arc<FifoQueueServiceWrapper>>>>,
}

impl QueueManager {
    /// Create new queue manager
    pub fn new() -> Self {
        Self {
            queues: Arc::new(RwLock::new(HashMap::new())),
            configs: Arc::new(RwLock::new(HashMap::new())),
            monitors: Arc::new(RwLock::new(HashMap::new())),
            compressors: Arc::new(RwLock::new(HashMap::new())),
            fifo_wrappers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a queue
    pub async fn register_queue(
        &self,
        queue: Arc<dyn QueueService + Send + Sync>,
        config: QueueConfig,
    ) -> QueueResult<()> {
        // Validate configuration
        config.validate()
            .map_err(QueueError::ConfigError)?;

        let queue_name = config.name.clone();

        // Store queue and configuration
        {
            let mut queues = self.queues.write().await;
            queues.insert(queue_name.clone(), queue.clone());
        }

        {
            let mut configs = self.configs.write().await;
            configs.insert(queue_name.clone(), config.clone());
        }

        // Initialize monitor if enabled
        if config.monitoring_enabled {
            let monitor = Arc::new(QueueMonitorService::new(
                queue.clone(),
                config.alert_thresholds.clone().unwrap_or_default(),
            ));

            let mut monitors = self.monitors.write().await;
            monitors.insert(queue_name.clone(), monitor.clone());

            // Start monitoring in background
            let monitor_clone = monitor.clone();
            let queue_name_clone = queue_name.clone();
            tokio::spawn(async move {
                let _handle = monitor_clone.start_monitoring(Duration::from_secs(30));
                info!("Monitoring started for queue {}", queue_name_clone);
            });
        }

        // Initialize compressor if enabled
        if config.compression_enabled {
            let compressor_config = config.compression_config
                .clone()
                .unwrap_or_default();

            let compressor = Arc::new(MessageCompressor::new(compressor_config));

            let mut compressors = self.compressors.write().await;
            compressors.insert(queue_name.clone(), compressor);
        }

        // Initialize FIFO wrapper if enabled
        if config.fifo_enabled {
            let fifo_config = config.fifo_config
                .clone()
                .unwrap_or_else(|| crate::fifo::utils::get_recommended_config(MessageVolume::Medium));

            let fifo_wrapper = Arc::new(FifoQueueServiceWrapper::new(queue.clone(), fifo_config));

            let mut fifo_wrappers = self.fifo_wrappers.write().await;
            fifo_wrappers.insert(queue_name.clone(), fifo_wrapper);
        }

        info!("Queue '{}' registered successfully", queue_name);
        Ok(())
    }

    /// Unregister a queue
    pub async fn unregister_queue(&self, queue_name: &str) -> QueueResult<()> {
        // Stop monitoring
        {
            let mut monitors = self.monitors.write().await;
            monitors.remove(queue_name);
            info!("Monitoring stopped for queue {}", queue_name);
        }

        // Remove all references
        {
            let mut queues = self.queues.write().await;
            queues.remove(queue_name);
        }

        {
            let mut configs = self.configs.write().await;
            configs.remove(queue_name);
        }

        {
            let mut compressors = self.compressors.write().await;
            compressors.remove(queue_name);
        }

        {
            let mut fifo_wrappers = self.fifo_wrappers.write().await;
            fifo_wrappers.remove(queue_name);
        }

        info!("Queue '{}' unregistered successfully", queue_name);
        Ok(())
    }

    /// Get queue
    pub async fn get_queue(&self, queue_name: &str) -> QueueResult<Arc<dyn QueueService + Send + Sync>> {
        let queues = self.queues.read().await;
        queues.get(queue_name)
            .cloned()
            .ok_or_else(|| QueueError::NotFound(format!("Queue '{}' not found", queue_name)))
    }

    /// Get FIFO queue wrapper
    pub async fn get_fifo_queue(&self, queue_name: &str) -> QueueResult<Arc<FifoQueueServiceWrapper>> {
        let fifo_wrappers = self.fifo_wrappers.read().await;
        fifo_wrappers.get(queue_name)
            .cloned()
            .ok_or_else(|| QueueError::NotFound(format!("FIFO wrapper for queue '{}' not found", queue_name)))
    }

    /// Get queue configuration
    pub async fn get_config(&self, queue_name: &str) -> QueueResult<QueueConfig> {
        let configs = self.configs.read().await;
        configs.get(queue_name)
            .cloned()
            .ok_or_else(|| QueueError::NotFound(format!("Configuration for queue '{}' not found", queue_name)))
    }

    /// Update queue configuration
    pub async fn update_config(&self, queue_name: &str, mut config: QueueConfig) -> QueueResult<()> {
        // Validate configuration
        config.validate()
            .map_err(QueueError::ConfigError)?;

        config.touch();

        {
            let mut configs = self.configs.write().await;
            configs.insert(queue_name.to_string(), config);
        }

        info!("Configuration for queue '{}' updated successfully", queue_name);
        Ok(())
    }

    /// List all registered queues
    pub async fn list_queues(&self) -> Vec<String> {
        let queues = self.queues.read().await;
        queues.keys().cloned().collect()
    }

    /// Get all queue configurations
    pub async fn list_configs(&self) -> Vec<QueueConfig> {
        let configs = self.configs.read().await;
        configs.values().cloned().collect()
    }

    /// Get queue statistics
    pub async fn get_stats(&self, queue_name: &str) -> QueueResult<QueueStats> {
        let queue = self.get_queue(queue_name).await?;
        queue.get_stats().await
    }

    /// Get queue health
    pub async fn get_health(&self, queue_name: &str) -> QueueResult<QueueHealthCheck> {
        let queue = self.get_queue(queue_name).await?;
        queue.health_check().await
    }

    /// Get queue metrics
    pub async fn get_metrics(&self, queue_name: &str) -> QueueResult<QueueMetrics> {
        let monitors = self.monitors.read().await;
        if let Some(monitor) = monitors.get(queue_name) {
            Ok(monitor.get_metrics().await)
        } else {
            Err(QueueError::NotFound(format!("Monitoring for queue '{}' not found", queue_name)))
        }
    }

    /// Purge queue
    pub async fn purge_queue(&self, queue_name: &str) -> QueueResult<()> {
        let queue = self.get_queue(queue_name).await?;
        queue.purge().await?;
        info!("Queue '{}' purged successfully", queue_name);
        Ok(())
    }

    /// Get all queue health statuses
    pub async fn get_all_health(&self) -> HashMap<String, QueueResult<QueueHealthCheck>> {
        let queue_names = self.list_queues().await;
        let mut health_map = HashMap::new();

        for queue_name in queue_names {
            let health = self.get_health(&queue_name).await;
            health_map.insert(queue_name, health);
        }

        health_map
    }

    /// Get all queue metrics
    pub async fn get_all_metrics(&self) -> HashMap<String, QueueResult<QueueMetrics>> {
        let queue_names = self.list_queues().await;
        let mut metrics_map = HashMap::new();

        for queue_name in queue_names {
            let metrics = self.get_metrics(&queue_name).await;
            metrics_map.insert(queue_name, metrics);
        }

        metrics_map
    }

    /// Perform maintenance on all queues
    pub async fn perform_maintenance(&self) -> QueueResult<Vec<MaintenanceResult>> {
        let queue_names = self.list_queues().await;
        let mut results = Vec::new();

        for queue_name in queue_names {
            let result = self.perform_queue_maintenance(&queue_name).await;
            results.push(result);
        }

        Ok(results)
    }

    /// Perform maintenance on a specific queue
    pub async fn perform_queue_maintenance(&self, queue_name: &str) -> MaintenanceResult {
        let start_time = std::time::Instant::now();
        let mut actions = Vec::new();
        let mut success = true;
        let mut error_message = None;

        // Cleanup expired messages (if supported by backend)
        if let Err(e) = self.cleanup_expired_messages(queue_name).await {
            success = false;
            error_message = Some(format!("Failed to cleanup expired messages: {}", e));
        } else {
            actions.push(MaintenanceAction::CleanupExpired);
        }

        // Cleanup deduplication cache (for FIFO queues)
        if let Ok(fifo_queue) = self.get_fifo_queue(queue_name).await {
            match fifo_queue.cleanup_deduplication().await {
                Ok(count) if count > 0 => {
                    actions.push(MaintenanceAction::CleanupDeduplication(count));
                }
                Err(e) => {
                    success = false;
                    error_message = Some(format!("Failed to cleanup deduplication cache: {}", e));
                }
                _ => {}
            }
        }

        // Update metrics
        if let Ok(monitor) = self.get_monitor(queue_name).await {
            if let Err(e) = monitor.update_metrics().await {
                success = false;
                error_message = Some(format!("Failed to update metrics: {}", e));
            } else {
                actions.push(MaintenanceAction::UpdateMetrics);
            }
        }

        MaintenanceResult {
            queue_name: queue_name.to_string(),
            success,
            duration_ms: start_time.elapsed().as_millis() as u64,
            actions,
            error_message,
        }
    }

    /// Helper method to cleanup expired messages
    async fn cleanup_expired_messages(&self, queue_name: &str) -> QueueResult<u64> {
        let _queue = self.get_queue(queue_name).await?;

        // This is a simplified implementation
        // In practice, you'd implement backend-specific cleanup logic
        let config = self.get_config(queue_name).await?;
        if let Some(retention_seconds) = config.retention_seconds {
            let cutoff_time = Utc::now() - chrono::Duration::seconds(retention_seconds as i64);

            // Implementation would depend on backend capabilities
            // For now, just log that cleanup was requested
            info!("Cleanup requested for queue '{}' (messages older than {})", queue_name, cutoff_time);
        }

        Ok(0) // Placeholder
    }

    /// Get monitor for a queue
    async fn get_monitor(&self, queue_name: &str) -> QueueResult<Arc<QueueMonitorService>> {
        let monitors = self.monitors.read().await;
        monitors.get(queue_name)
            .cloned()
            .ok_or_else(|| QueueError::NotFound(format!("Monitor for queue '{}' not found", queue_name)))
    }
}

impl Default for QueueManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Maintenance action performed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MaintenanceAction {
    /// Cleanup expired messages
    CleanupExpired,

    /// Cleanup deduplication cache (with count)
    CleanupDeduplication(u64),

    /// Update metrics
    UpdateMetrics,

    /// Compact queue storage
    CompactStorage,

    /// Rebuild indexes
    RebuildIndexes,

    /// Optimize performance
    Optimize,
}

/// Result of maintenance operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceResult {
    /// Queue name
    pub queue_name: String,

    /// Whether maintenance was successful
    pub success: bool,

    /// Duration in milliseconds
    pub duration_ms: u64,

    /// Actions performed
    pub actions: Vec<MaintenanceAction>,

    /// Error message if failed
    pub error_message: Option<String>,
}

/// Administrative operations trait
#[async_trait::async_trait]
pub trait QueueAdminService: Send + Sync {
    /// Create queue
    async fn create_queue(&self, config: QueueConfig) -> QueueResult<()>;

    /// Delete queue
    async fn delete_queue(&self, queue_name: &str) -> QueueResult<()>;

    /// List queues
    async fn list_queues(&self) -> QueueResult<Vec<String>>;

    /// Get queue configuration
    async fn get_queue_config(&self, queue_name: &str) -> QueueResult<QueueConfig>;

    /// Update queue configuration
    async fn update_queue_config(&self, queue_name: &str, config: QueueConfig) -> QueueResult<()>;

    /// Get queue statistics
    async fn get_queue_stats(&self, queue_name: &str) -> QueueResult<QueueStats>;

    /// Get queue health
    async fn get_queue_health(&self, queue_name: &str) -> QueueResult<QueueHealthCheck>;

    /// Purge queue
    async fn purge_queue(&self, queue_name: &str) -> QueueResult<()>;

    /// Perform maintenance
    async fn perform_maintenance(&self, queue_name: Option<&str>) -> QueueResult<Vec<MaintenanceResult>>;
}

#[async_trait::async_trait]
impl QueueAdminService for QueueManager {
    async fn create_queue(&self, config: QueueConfig) -> QueueResult<()> {
        // Implementation would depend on the queue type
        // This is a simplified placeholder
        info!("Creating queue '{}'", config.name);

        match config.queue_type.as_str() {
            "redis" => {
                // Create Redis queue implementation
                // This would require the Redis queue builder
                return Err(QueueError::ConfigError("Redis queue creation not implemented in manager".to_string()));
            }
            "sqs" => {
                // Create SQS queue implementation
                // This would require the SQS queue builder
                return Err(QueueError::ConfigError("SQS queue creation not implemented in manager".to_string()));
            }
            _ => {
                return Err(QueueError::ConfigError(format!("Unsupported queue type: {}", config.queue_type)));
            }
        }
    }

    async fn delete_queue(&self, queue_name: &str) -> QueueResult<()> {
        self.unregister_queue(queue_name).await
    }

    async fn list_queues(&self) -> QueueResult<Vec<String>> {
        Ok(self.list_queues().await)
    }

    async fn get_queue_config(&self, queue_name: &str) -> QueueResult<QueueConfig> {
        self.get_config(queue_name).await
    }

    async fn update_queue_config(&self, queue_name: &str, config: QueueConfig) -> QueueResult<()> {
        self.update_config(queue_name, config).await
    }

    async fn get_queue_stats(&self, queue_name: &str) -> QueueResult<QueueStats> {
        self.get_stats(queue_name).await
    }

    async fn get_queue_health(&self, queue_name: &str) -> QueueResult<QueueHealthCheck> {
        self.get_health(queue_name).await
    }

    async fn purge_queue(&self, queue_name: &str) -> QueueResult<()> {
        self.purge_queue(queue_name).await
    }

    async fn perform_maintenance(&self, queue_name: Option<&str>) -> QueueResult<Vec<MaintenanceResult>> {
        if let Some(name) = queue_name {
            Ok(vec![self.perform_queue_maintenance(name).await])
        } else {
            self.perform_maintenance().await
        }
    }
}

