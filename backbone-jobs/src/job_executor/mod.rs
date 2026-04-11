//! Job execution engine that integrates with backbone-queue

// pub mod mock_queue; // Temporarily disabled - has QueueService trait mismatches

use crate::error::{JobError, JobResult};
use crate::job::Job;
use backbone_queue::{QueueService, QueueMessage};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

// Re-export the mock queue service

/// Job executor that handles running jobs through the queue system
pub struct JobExecutor {
    queue_service: Arc<dyn QueueService>,
    default_timeout: Duration,
}

impl JobExecutor {
    /// Create a new job executor
    pub fn new(queue_service: Arc<dyn QueueService>, default_timeout: Duration) -> Self {
        Self {
            queue_service,
            default_timeout,
        }
    }

    /// Execute a job by sending it to the appropriate queue
    pub async fn execute_job(&self, job: &Job) -> JobResult<()> {
        info!("Executing job {} (attempt {}) on queue {}",
            job.name, job.run_count + 1, job.queue);

        // Create a queue message from the job
        let job_payload = serde_json::to_value(job)
            .map_err(|e| JobError::execution(&format!("Failed to serialize job: {}", e)))?;

        let message = backbone_queue::QueueMessage::builder()
            .id(job.id.as_str().to_string())
            .json_payload(job_payload)
            .priority(match job.priority {
                crate::types::JobPriority::Low => backbone_queue::QueuePriority::Low,
                crate::types::JobPriority::Normal => backbone_queue::QueuePriority::Normal,
                crate::types::JobPriority::High => backbone_queue::QueuePriority::High,
                crate::types::JobPriority::Critical => backbone_queue::QueuePriority::Critical,
            })
            .visibility_timeout(self.default_timeout.as_secs())
            .build();

        // Send the message to the queue
        self.queue_service.enqueue(message).await
            .map_err(|e| JobError::execution(&format!("Failed to enqueue job: {}", e)))?;

        info!("Job {} enqueued successfully", job.name);
        Ok(())
    }
}

impl Clone for JobExecutor {
    fn clone(&self) -> Self {
        Self {
            queue_service: self.queue_service.clone(),
            default_timeout: self.default_timeout,
        }
    }
}

/// Job execution callback trait for custom handling
#[async_trait::async_trait]
pub trait JobExecutionCallback: Send + Sync {
    /// Called before job execution
    async fn on_job_start(&self, job: &Job) -> JobResult<()> {
        info!("Job {} started", job.name);
        Ok(())
    }

    /// Called after successful job execution
    async fn on_job_success(&self, job: &Job, duration: Duration) -> JobResult<()> {
        info!("Job {} completed successfully in {:?}", job.name, duration);
        Ok(())
    }

    /// Called when job execution fails
    async fn on_job_failure(&self, job: &Job, error: &str, duration: Duration) -> JobResult<()> {
        error!("Job {} failed after {:?}: {}", job.name, duration, error);
        Ok(())
    }
}

/// Default implementation of job execution callback
pub struct DefaultJobExecutionCallback;

impl DefaultJobExecutionCallback {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultJobExecutionCallback {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl JobExecutionCallback for DefaultJobExecutionCallback {
    // Use default implementations (logging)
}

/// Job executor that uses callbacks for direct execution
pub struct CallbackJobExecutor {
    callback: Arc<dyn JobExecutionCallback>,
}

impl CallbackJobExecutor {
    /// Create a new callback job executor
    pub fn new(callback: Arc<dyn JobExecutionCallback>) -> Self {
        Self { callback }
    }

    /// Create a callback job executor with the default callback
    pub fn with_default_callback() -> Self {
        Self::new(Arc::new(DefaultJobExecutionCallback))
    }

    /// Execute a job directly using callbacks
    pub async fn execute_job_callback(&self, job: &Job) -> JobResult<()> {
        let start_time = std::time::Instant::now();

        // Call on_job_start callback
        self.callback.on_job_start(job).await?;

        // Simulate job execution (in real implementation, this would call the actual job handler)
        tokio::time::sleep(Duration::from_millis(100)).await;

        let duration = start_time.elapsed();

        // Simulate success (in real implementation, this would depend on job execution result)
        self.callback.on_job_success(job, duration).await?;

        Ok(())
    }
}

impl Clone for CallbackJobExecutor {
    fn clone(&self) -> Self {
        Self {
            callback: self.callback.clone(),
        }
    }
}

// Mock QueueService implementation for testing
use backbone_queue::{QueueStats, QueueHealthCheck, QueueBackend, BatchReceiveResult, QueueResult};
use std::collections::HashMap;

#[derive(Default)]
pub struct MockQueueService {
    messages: std::sync::Arc<tokio::sync::RwLock<Vec<QueueMessage>>>,
    stats: std::sync::Arc<tokio::sync::RwLock<QueueStats>>,
}

impl MockQueueService {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl QueueService for MockQueueService {
    async fn enqueue(&self, message: QueueMessage) -> QueueResult<String> {
        let mut messages = self.messages.write().await;
        let message_id = message.id.clone();
        messages.push(message);

        // Update stats
        let mut stats = self.stats.write().await;
        stats.total_messages += 1;
        stats.visible_messages += 1;

        Ok(message_id)
    }

    async fn dequeue(&self) -> QueueResult<Option<QueueMessage>> {
        let mut messages = self.messages.write().await;
        if let Some(message) = messages.iter_mut().find(|m| m.is_visible() && !m.is_expired()) {
            message.mark_received();

            // Update stats
            let mut stats = self.stats.write().await;
            stats.visible_messages = stats.visible_messages.saturating_sub(1);
            stats.invisible_messages += 1;

            Ok(Some(message.clone()))
        } else {
            Ok(None)
        }
    }

    async fn dequeue_batch(&self, max_messages: usize) -> QueueResult<BatchReceiveResult> {
        let mut result_messages = Vec::new();

        for _ in 0..max_messages {
            if let Some(message) = self.dequeue().await? {
                result_messages.push(message);
            } else {
                break;
            }
        }

        let available = result_messages.len();
        Ok(BatchReceiveResult {
            messages: result_messages,
            requested: max_messages,
            available,
            total_in_queue: self.size().await.unwrap_or(0),
            processing_time_ms: 1,
        })
    }

    async fn ack(&self, message_id: &str) -> QueueResult<bool> {
        let mut messages = self.messages.write().await;
        if let Some(message) = messages.iter_mut().find(|m| m.id == message_id) {
            message.mark_acknowledged();

            // Update stats
            let mut stats = self.stats.write().await;
            stats.invisible_messages = stats.invisible_messages.saturating_sub(1);
            stats.total_processed += 1;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn ack_batch(&self, message_ids: Vec<String>) -> QueueResult<u64> {
        let mut acked_count = 0;
        for message_id in message_ids {
            if self.ack(&message_id).await? {
                acked_count += 1;
            }
        }
        Ok(acked_count)
    }

    async fn nack(&self, message_id: &str, delay_seconds: Option<u64>) -> QueueResult<bool> {
        let mut messages = self.messages.write().await;
        if let Some(message) = messages.iter_mut().find(|m| m.id == message_id) {
            message.mark_failed();
            message.reset_for_retry(delay_seconds);

            // Update stats
            let mut stats = self.stats.write().await;
            stats.invisible_messages = stats.invisible_messages.saturating_sub(1);
            stats.total_failed += 1;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn delete(&self, message_id: &str) -> QueueResult<bool> {
        let mut messages = self.messages.write().await;
        let initial_len = messages.len();
        messages.retain(|m| m.id != message_id);

        let deleted = messages.len() < initial_len;
        if deleted {
            // Update stats
            let mut stats = self.stats.write().await;
            stats.total_messages = stats.total_messages.saturating_sub(1);
        }

        Ok(deleted)
    }

    async fn get_message(&self, message_id: &str) -> QueueResult<Option<QueueMessage>> {
        let messages = self.messages.read().await;
        Ok(messages.iter().find(|m| m.id == message_id).cloned())
    }

    async fn get_stats(&self) -> QueueResult<QueueStats> {
        let stats = self.stats.read().await;
        Ok(stats.clone())
    }

    async fn purge(&self) -> QueueResult<u64> {
        let mut messages = self.messages.write().await;
        let count = messages.len() as u64;
        messages.clear();

        // Reset stats
        let mut stats = self.stats.write().await;
        *stats = QueueStats::default();

        Ok(count)
    }

    async fn size(&self) -> QueueResult<u64> {
        let messages = self.messages.read().await;
        Ok(messages.len() as u64)
    }

    async fn is_empty(&self) -> QueueResult<bool> {
        let messages = self.messages.read().await;
        Ok(messages.is_empty())
    }

    async fn health_check(&self) -> QueueResult<QueueHealthCheck> {
        let now = chrono::Utc::now();
        Ok(QueueHealthCheck {
            status: backbone_queue::QueueHealth::Healthy,
            queue_size: self.size().await.unwrap_or(0),
            avg_processing_time_ms: Some(1.0),
            messages_per_second: Some(100.0),
            error_rate: 0.0,
            last_activity: Some(now),
            checked_at: now,
            details: HashMap::new(),
        })
    }

    async fn validate_config(&self) -> QueueResult<bool> {
        Ok(true) // Mock always has valid config
    }

    async fn test_connection(&self) -> QueueResult<bool> {
        Ok(true) // Mock always has valid connection
    }

    fn backend_type(&self) -> QueueBackend {
        QueueBackend::Redis // Use Redis as mock backend
    }
}