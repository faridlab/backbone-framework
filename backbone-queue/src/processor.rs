//! Message Processor with Batch Processing Capabilities
//!
//! Provides a flexible message processing framework with support for individual
//! and batch processing, retry logic, error handling, and performance monitoring.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{QueueMessage, QueueResult, QueueError};

/// Message processing result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProcessingOutcome {
    /// Message processed successfully
    Success,
    /// Processing failed but should be retried
    Retry { delay_seconds: u64 },
    /// Processing failed permanently
    Failed,
    /// Message rejected (invalid format, etc.)
    Rejected,
}

/// Batch processing configuration
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum batch size
    pub max_batch_size: usize,

    /// Maximum wait time for batch accumulation
    pub max_wait_time_ms: u64,

    /// Minimum batch size for processing
    pub min_batch_size: usize,

    /// Enable auto-batching
    pub enable_auto_batch: bool,

    /// Batch timeout policy
    pub batch_timeout_policy: BatchTimeoutPolicy,
}

/// Batch timeout policy
#[derive(Debug, Clone, PartialEq)]
pub enum BatchTimeoutPolicy {
    /// Process when min_batch_size is reached
    Immediate,
    /// Wait for max_batch_size or timeout
    WaitFull,
    /// Wait for max_batch_size or timeout, with dynamic timeout
    Adaptive,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            max_wait_time_ms: 5000, // 5 seconds
            min_batch_size: 1,
            enable_auto_batch: true,
            batch_timeout_policy: BatchTimeoutPolicy::WaitFull,
        }
    }
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,

    /// Initial retry delay in milliseconds
    pub initial_delay_ms: u64,

    /// Backoff multiplier for exponential backoff
    pub backoff_multiplier: f64,

    /// Maximum retry delay in milliseconds
    pub max_delay_ms: u64,

    /// Jitter for retry delays (percentage)
    pub jitter_percentage: f64,

    /// Retry policy
    pub retry_policy: RetryPolicy,
}

/// Retry policy
#[derive(Debug, Clone, PartialEq)]
pub enum RetryPolicy {
    /// Fixed delay between retries
    Fixed,
    /// Exponential backoff
    Exponential,
    /// Linear backoff
    Linear,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1000, // 1 second
            backoff_multiplier: 2.0,
            max_delay_ms: 60000, // 1 minute
            jitter_percentage: 0.1, // 10% jitter
            retry_policy: RetryPolicy::Exponential,
        }
    }
}

/// Message processing context
#[derive(Debug, Clone)]
pub struct ProcessingContext {
    /// Unique processing ID
    pub processing_id: String,

    /// Processor identifier
    pub processor_id: String,

    /// Processing start time
    pub start_time: Instant,

    /// Current attempt number
    pub attempt_number: u32,

    /// Maximum allowed attempts
    pub max_attempts: u32,

    /// Batch context (if applicable)
    pub batch_context: Option<BatchContext>,

    /// Metadata
    pub metadata: HashMap<String, String>,
}

/// Batch processing context
#[derive(Debug, Clone)]
pub struct BatchContext {
    /// Batch identifier
    pub batch_id: String,

    /// Batch size
    pub batch_size: usize,

    /// Batch start time
    pub batch_start_time: Instant,

    /// Current position in batch
    pub current_position: usize,
}

/// Processing result with metadata
#[derive(Debug, Clone)]
pub struct ProcessedMessage {
    /// Original message
    pub message: QueueMessage,

    /// Processing outcome
    pub outcome: ProcessingOutcome,

    /// Processing time
    pub processing_time: Duration,

    /// Processing context
    pub context: ProcessingContext,

    /// Error message (if applicable)
    pub error_message: Option<String>,

    /// Additional result data
    pub result_data: Option<serde_json::Value>,
}

/// Batch processing result
#[derive(Debug, Clone)]
pub struct BatchProcessingResult {
    /// Batch identifier
    pub batch_id: String,

    /// Total messages in batch
    pub total_messages: usize,

    /// Successfully processed messages
    pub successful_messages: Vec<ProcessedMessage>,

    /// Failed messages
    pub failed_messages: Vec<ProcessedMessage>,

    /// Retried messages
    pub retried_messages: Vec<ProcessedMessage>,

    /// Total processing time
    pub total_processing_time: Duration,

    /// Batch start time
    pub batch_start_time: DateTime<Utc>,

    /// Batch end time
    pub batch_end_time: DateTime<Utc>,
}

impl BatchProcessingResult {
    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_messages == 0 {
            0.0
        } else {
            self.successful_messages.len() as f64 / self.total_messages as f64
        }
    }

    /// Get failure rate
    pub fn failure_rate(&self) -> f64 {
        1.0 - self.success_rate()
    }

    /// Get average processing time per message
    pub fn avg_processing_time_per_message(&self) -> Duration {
        if self.total_messages == 0 {
            Duration::ZERO
        } else {
            self.total_processing_time / self.total_messages as u32
        }
    }
}

/// Message processor trait
#[async_trait::async_trait]
pub trait MessageProcessor: Send + Sync {
    /// Process a single message
    async fn process_message(&self, message: QueueMessage, context: ProcessingContext) -> ProcessedMessage;

    /// Process a batch of messages
    async fn process_batch(&self, messages: Vec<QueueMessage>, context: ProcessingContext) -> BatchProcessingResult {
        // Default implementation processes messages individually
        let start_time = Instant::now();
        let batch_id = Uuid::new_v4().to_string();

        let mut successful_messages = Vec::new();
        let mut failed_messages = Vec::new();
        let mut retried_messages = Vec::new();

        let batch_context = BatchContext {
            batch_id: batch_id.clone(),
            batch_size: messages.len(),
            batch_start_time: start_time,
            current_position: 0,
        };

        for (index, message) in messages.into_iter().enumerate() {
            let mut context = context.clone();
            context.batch_context = Some(BatchContext {
                current_position: index,
                ..batch_context.clone()
            });

            let processed_message = self.process_message(message, context).await;

            match processed_message.outcome {
                ProcessingOutcome::Success => {
                    successful_messages.push(processed_message);
                }
                ProcessingOutcome::Failed | ProcessingOutcome::Rejected => {
                    failed_messages.push(processed_message);
                }
                ProcessingOutcome::Retry { .. } => {
                    retried_messages.push(processed_message);
                }
            }
        }

        BatchProcessingResult {
            batch_id,
            total_messages: successful_messages.len() + failed_messages.len() + retried_messages.len(),
            successful_messages,
            failed_messages,
            retried_messages,
            total_processing_time: start_time.elapsed(),
            batch_start_time: Utc::now(),
            batch_end_time: Utc::now(),
        }
    }

    /// Get processor name
    fn processor_name(&self) -> &str;

    /// Get processor version
    fn processor_version(&self) -> &str { "1.0.0" }

    /// Check if processor can handle the message
    async fn can_process(&self, _message: &QueueMessage) -> bool {
        true // Default: can process any message
    }

    /// Get processing statistics
    async fn get_stats(&self) -> ProcessorStats {
        ProcessorStats::default()
    }

    /// Reset processor statistics
    async fn reset_stats(&self) {}
}

/// Processor statistics
#[derive(Debug, Clone, Default)]
pub struct ProcessorStats {
    /// Total messages processed
    pub total_messages_processed: u64,

    /// Total messages succeeded
    pub total_messages_succeeded: u64,

    /// Total messages failed
    pub total_messages_failed: u64,

    /// Total messages retried
    pub total_messages_retried: u64,

    /// Total batches processed
    pub total_batches_processed: u64,

    /// Average processing time per message
    pub avg_processing_time_ms: f64,

    /// Average batch processing time
    pub avg_batch_processing_time_ms: f64,

    /// Last processing timestamp
    pub last_processed_at: Option<DateTime<Utc>>,

    /// Processor uptime
    pub uptime_seconds: u64,

    /// Memory usage (if available)
    pub memory_usage_bytes: Option<u64>,

    /// Custom metrics
    pub custom_metrics: HashMap<String, serde_json::Value>,
}

impl ProcessorStats {
    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_messages_processed == 0 {
            0.0
        } else {
            self.total_messages_succeeded as f64 / self.total_messages_processed as f64
        }
    }

    /// Get failure rate
    pub fn failure_rate(&self) -> f64 {
        1.0 - self.success_rate()
    }

    /// Get retry rate
    pub fn retry_rate(&self) -> f64 {
        if self.total_messages_processed == 0 {
            0.0
        } else {
            self.total_messages_retried as f64 / self.total_messages_processed as f64
        }
    }
}

/// Retry handler for processing failures
pub struct RetryHandler {
    config: RetryConfig,
}

impl RetryHandler {
    /// Create new retry handler
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    /// Calculate retry delay for given attempt
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let base_delay = match self.config.retry_policy {
            RetryPolicy::Fixed => self.config.initial_delay_ms,
            RetryPolicy::Exponential => {
                let delay = self.config.initial_delay_ms as f64 * self.config.backoff_multiplier.powi(attempt as i32);
                delay.round() as u64
            }
            RetryPolicy::Linear => {
                self.config.initial_delay_ms * (attempt + 1) as u64
            }
        };

        // Apply jitter
        let jitter_factor = 1.0 + (rand::random::<f64>() - 0.5) * 2.0 * self.config.jitter_percentage;
        let delay_with_jitter = (base_delay as f64 * jitter_factor).round() as u64;

        // Cap at maximum delay
        let final_delay = delay_with_jitter.min(self.config.max_delay_ms);

        Duration::from_millis(final_delay)
    }

    /// Check if retry is allowed for given attempt
    pub fn should_retry(&self, attempt: u32, error: &ProcessingOutcome) -> bool {
        attempt < self.config.max_attempts && matches!(error, ProcessingOutcome::Retry { .. })
    }

    /// Get retry configuration
    pub fn config(&self) -> &RetryConfig {
        &self.config
    }
}

/// Batching processor that accumulates messages and processes them in batches
pub struct BatchingProcessor {
    inner: Arc<dyn MessageProcessor + Send + Sync>,
    batch_config: BatchConfig,
    pending_messages: Arc<RwLock<Vec<QueueMessage>>>,
    stats: Arc<RwLock<ProcessorStats>>,
    start_time: Instant,
}

impl BatchingProcessor {
    /// Create new batching processor
    pub fn new(
        inner: Arc<dyn MessageProcessor + Send + Sync>,
        batch_config: BatchConfig,
    ) -> Self {
        Self {
            inner,
            batch_config,
            pending_messages: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(ProcessorStats::default())),
            start_time: Instant::now(),
        }
    }

    /// Add message to batch
    pub async fn add_to_batch(&self, message: QueueMessage) -> QueueResult<()> {
        let mut pending = self.pending_messages.write().await;
        pending.push(message);

        // Check if we should process the batch immediately
        if pending.len() >= self.batch_config.max_batch_size {
            self.process_batch_if_ready().await?
        }

        Ok(())
    }

    /// Process batch if ready
    async fn process_batch_if_ready(&self) -> QueueResult<()> {
        let mut pending = self.pending_messages.write().await;

        if pending.len() >= self.batch_config.min_batch_size {
            let messages = pending.drain(..).collect();
            drop(pending); // Release lock before processing

            let context = ProcessingContext {
                processing_id: Uuid::new_v4().to_string(),
                processor_id: format!("batching-processor-{}", self.inner.processor_name()),
                start_time: Instant::now(),
                attempt_number: 1,
                max_attempts: 1, // Batching doesn't retry individual messages
                batch_context: None,
                metadata: HashMap::new(),
            };

            let result = self.inner.process_batch(messages, context).await;

            // Update statistics
            let mut stats = self.stats.write().await;
            stats.total_batches_processed += 1;
            stats.total_messages_processed += result.total_messages as u64;
            stats.total_messages_succeeded += result.successful_messages.len() as u64;
            stats.total_messages_failed += result.failed_messages.len() as u64;
            stats.total_messages_retried += result.retried_messages.len() as u64;
            stats.last_processed_at = Some(Utc::now());
            stats.uptime_seconds = self.start_time.elapsed().as_secs();

            // Calculate average processing times
            if stats.total_messages_processed > 0 {
                let total_time = result.total_processing_time.as_millis() as f64;
                stats.avg_batch_processing_time_ms = total_time;
                stats.avg_processing_time_ms = total_time / result.total_messages as f64;
            }
        }

        Ok(())
    }

    /// Force process current batch
    pub async fn flush_batch(&self) -> QueueResult<BatchProcessingResult> {
        let mut pending = self.pending_messages.write().await;
        let messages: Vec<QueueMessage> = pending.drain(..).collect();
        drop(pending);

        if messages.is_empty() {
            return Err(QueueError::Other("No messages in batch to process".to_string()));
        }

        let context = ProcessingContext {
            processing_id: Uuid::new_v4().to_string(),
            processor_id: format!("batching-processor-{}", self.inner.processor_name()),
            start_time: Instant::now(),
            attempt_number: 1,
            max_attempts: 1,
            batch_context: None,
            metadata: HashMap::new(),
        };

        Ok(self.inner.process_batch(messages, context).await)
    }

    /// Get current batch size
    pub async fn current_batch_size(&self) -> usize {
        let pending = self.pending_messages.read().await;
        pending.len()
    }

    /// Start background batch processor
    pub async fn start_background_processor(&self) -> QueueResult<()> {
        if !self.batch_config.enable_auto_batch {
            return Ok(());
        }

        let pending_messages = self.pending_messages.clone();
        let batch_config = self.batch_config.clone();
        let inner = self.inner.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(batch_config.max_wait_time_ms));

            loop {
                interval.tick().await;

                let pending = pending_messages.read().await;
                if pending.len() >= batch_config.min_batch_size {
                    drop(pending);

                    let context = ProcessingContext {
                        processing_id: Uuid::new_v4().to_string(),
                        processor_id: format!("background-batch-{}", inner.processor_name()),
                        start_time: Instant::now(),
                        attempt_number: 1,
                        max_attempts: 1,
                        batch_context: None,
                        metadata: HashMap::new(),
                    };

                    // Process batch in background
                    let messages_to_process: Vec<QueueMessage> = {
                        let mut pending = pending_messages.write().await;
                        pending.drain(..).collect()
                    };

                    if !messages_to_process.is_empty() {
                        inner.process_batch(messages_to_process, context).await;
                    }
                }
            }
        });

        Ok(())
    }
}

#[async_trait::async_trait]
impl MessageProcessor for BatchingProcessor {
    async fn process_message(&self, message: QueueMessage, context: ProcessingContext) -> ProcessedMessage {
        // Delegate to inner processor
        self.inner.process_message(message, context).await
    }

    async fn process_batch(&self, messages: Vec<QueueMessage>, context: ProcessingContext) -> BatchProcessingResult {
        // Delegate to inner processor
        self.inner.process_batch(messages, context).await
    }

    fn processor_name(&self) -> &str {
        "BatchingProcessor"
    }

    async fn get_stats(&self) -> ProcessorStats {
        let mut stats = self.stats.read().await.clone();

        // Merge with inner processor stats if available
        let inner_stats = self.inner.get_stats().await;
        stats.total_messages_processed += inner_stats.total_messages_processed;
        stats.total_messages_succeeded += inner_stats.total_messages_succeeded;
        stats.total_messages_failed += inner_stats.total_messages_failed;
        stats.total_messages_retried += inner_stats.total_messages_retried;

        stats
    }

    async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = ProcessorStats::default();
        self.inner.reset_stats().await;
    }
}

/// Simple message processor for demonstration and testing
pub struct SimpleMessageProcessor {
    name: String,
    stats: Arc<RwLock<ProcessorStats>>,
    start_time: Instant,
}

impl SimpleMessageProcessor {
    /// Create new simple processor
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            stats: Arc::new(RwLock::new(ProcessorStats::default())),
            start_time: Instant::now(),
        }
    }

    /// Create processor that always succeeds
    pub fn success_processor() -> Self {
        Self::new("SuccessProcessor")
    }

    /// Create processor that always fails
    pub fn failure_processor() -> Self {
        Self::new("FailureProcessor")
    }

    /// Create processor that fails randomly
    pub fn random_processor(failure_rate: f64) -> Self {
        Self::new(format!("RandomProcessor-{:.2}", failure_rate))
    }

    /// Create processor with custom name
    pub fn custom(name: impl Into<String>) -> Self {
        Self::new(name)
    }
}

#[async_trait::async_trait]
impl MessageProcessor for SimpleMessageProcessor {
    async fn process_message(&self, message: QueueMessage, context: ProcessingContext) -> ProcessedMessage {
        let start_time = Instant::now();

        let outcome = if self.name.contains("Failure") {
            ProcessingOutcome::Failed
        } else if self.name.starts_with("Random") {
            let failure_rate: f64 = self.name
                .split('-')
                .nth(1)
                .unwrap_or("0.5")
                .parse()
                .unwrap_or(0.5);

            if rand::random::<f64>() < failure_rate {
                ProcessingOutcome::Failed
            } else {
                ProcessingOutcome::Success
            }
        } else {
            ProcessingOutcome::Success
        };

        let processing_time = start_time.elapsed();

        // Update statistics
        let mut stats = self.stats.write().await;
        stats.total_messages_processed += 1;
        stats.last_processed_at = Some(Utc::now());
        stats.uptime_seconds = self.start_time.elapsed().as_secs();

        match outcome {
            ProcessingOutcome::Success => {
                stats.total_messages_succeeded += 1;
            }
            ProcessingOutcome::Failed | ProcessingOutcome::Rejected => {
                stats.total_messages_failed += 1;
            }
            ProcessingOutcome::Retry { .. } => {
                stats.total_messages_retried += 1;
            }
        }

        // Update average processing time
        let total_time = processing_time.as_millis() as f64;
        stats.avg_processing_time_ms =
            (stats.avg_processing_time_ms * (stats.total_messages_processed - 1) as f64 + total_time) /
            stats.total_messages_processed as f64;

        ProcessedMessage {
            message,
            outcome,
            processing_time,
            context,
            error_message: None,
            result_data: Some(serde_json::json!({
                "processor": self.name,
                "processed_at": Utc::now().to_rfc3339()
            })),
        }
    }

    fn processor_name(&self) -> &str {
        &self.name
    }

    async fn get_stats(&self) -> ProcessorStats {
        let mut stats = self.stats.read().await.clone();
        stats.uptime_seconds = self.start_time.elapsed().as_secs();
        stats
    }

    async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = ProcessorStats::default();
    }
}

