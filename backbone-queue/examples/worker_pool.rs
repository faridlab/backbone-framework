//! Worker Pool Example
//!
//! This example demonstrates how to build a worker pool that processes messages
//! from a queue with concurrent processing, error handling, and graceful shutdown.

use backbone_queue::{
    QueueService,
    redis::RedisQueueBuilder,
    types::{QueueMessage, QueuePriority, MessageStatus}
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, Semaphore};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

/// Worker configuration
struct WorkerConfig {
    max_concurrent_tasks: usize,
    poll_interval: Duration,
    max_retries: u32,
    visibility_timeout: u64,
    shutdown_timeout: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 10,
            poll_interval: Duration::from_millis(100),
            max_retries: 3,
            visibility_timeout: 30,
            shutdown_timeout: Duration::from_secs(30),
        }
    }
}

/// Message processing result
#[derive(Debug, Clone)]
enum ProcessResult {
    Success,
    RetryableError(String),
    PermanentError(String),
}

/// Worker statistics
#[derive(Debug, Default)]
struct WorkerStats {
    messages_processed: u64,
    messages_failed: u64,
    messages_retried: u64,
    total_processing_time: Duration,
}

/// Queue Worker Pool
struct QueueWorker {
    queue: Arc<dyn QueueService + Send + Sync>,
    config: WorkerConfig,
    stats: Arc<RwLock<WorkerStats>>,
    semaphore: Arc<Semaphore>,
    cancellation_token: CancellationToken,
}

impl QueueWorker {
    fn new(
        queue: Arc<dyn QueueService + Send + Sync>,
        config: WorkerConfig,
    ) -> Self {
        Self {
            queue,
            semaphore: Arc::new(Semaphore::new(config.max_concurrent_tasks)),
            stats: Arc::new(RwLock::new(WorkerStats::default())),
            config,
            cancellation_token: CancellationToken::new(),
        }
    }

    /// Start the worker pool
    async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Starting queue worker pool");
        println!("📊 Configuration:");
        println!("  - Max concurrent tasks: {}", self.config.max_concurrent_tasks);
        println!("  - Poll interval: {:?}", self.config.poll_interval);
        println!("  - Max retries: {}", self.config.max_retries);
        println!("  - Visibility timeout: {} seconds", self.config.visibility_timeout);

        let queue = self.queue.clone();
        let stats = self.stats.clone();
        let semaphore = self.semaphore.clone();
        let config = self.config.clone();
        let cancellation_token = self.cancellation_token.clone();

        // Spawn statistics reporter
        let stats_reporter = tokio::spawn({
            let stats = stats.clone();
            async move {
                let mut interval = tokio::time::interval(Duration::from_secs(10));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            let stats = stats.read().await;
                            Self::print_stats(&*stats);
                        }
                        _ = cancellation_token.cancelled() => {
                            break;
                        }
                    }
                }
            }
        });

        // Main worker loop
        let worker_handle = tokio::spawn(async move {
            let mut consecutive_empty_polls = 0;
            let max_empty_polls = 50; // Back off after many empty polls

            loop {
                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        println!("🛑 Worker pool shutdown requested");
                        break;
                    }
                    _ = sleep(config.poll_interval) => {
                        // Get a permit for concurrent processing
                        let permit = semaphore.clone().acquire_owned().await;

                        // Check if we have permission to process
                        if permit.is_err() {
                            continue; // All workers are busy
                        }

                        match queue.dequeue().await {
                            Ok(Some(message)) => {
                                consecutive_empty_polls = 0;

                                // Clone necessary data for the task
                                let queue_clone = queue.clone();
                                let stats_clone = stats.clone();
                                let config_clone = config.clone();
                                let cancellation_token_clone = cancellation_token.clone();

                                // Spawn processing task
                                tokio::spawn(async move {
                                    let _permit = permit; // Hold permit for the duration of processing

                                    let start_time = std::time::Instant::now();
                                    let result = Self::process_message(&message).await;
                                    let processing_time = start_time.elapsed();

                                    // Update statistics
                                    {
                                        let mut stats = stats_clone.write().await;
                                        stats.total_processing_time += processing_time;

                                        match result {
                                            ProcessResult::Success => {
                                                stats.messages_processed += 1;
                                                println!("✅ Processed message: {}", message.id);
                                            }
                                            ProcessResult::RetryableError(error) => {
                                                if message.receive_count < config_clone.max_retries {
                                                    stats.messages_retried += 1;
                                                    println!("⚠️  Retrying message: {} - {}", message.id, error);
                                                    // Return message to queue with delay
                                                    let _ = queue_clone.nack(&message.id, Some(60)).await;
                                                } else {
                                                    stats.messages_failed += 1;
                                                    println!("❌ Max retries exceeded for message: {} - {}", message.id, error);
                                                    // Move to dead letter queue by acknowledging
                                                    let _ = queue_clone.ack(&message.id).await;
                                                }
                                            }
                                            ProcessResult::PermanentError(error) => {
                                                stats.messages_failed += 1;
                                                println!("❌ Permanent error for message: {} - {}", message.id, error);
                                                // Acknowledge to remove from queue
                                                let _ = queue_clone.ack(&message.id).await;
                                            }
                                        }
                                    }

                                    // Check for shutdown
                                    if cancellation_token_clone.is_cancelled() {
                                        // Return message to queue for later processing
                                        let _ = queue_clone.nack(&message.id, None).await;
                                    }
                                });
                            }
                            Ok(None) => {
                                consecutive_empty_polls += 1;
                                if consecutive_empty_polls >= max_empty_polls {
                                    // Back off polling when queue is empty
                                    sleep(Duration::from_secs(1)).await;
                                    consecutive_empty_polls = 0;
                                }
                            }
                            Err(e) => {
                                eprintln!("❌ Error dequeuing message: {}", e);
                                sleep(Duration::from_secs(1)).await;
                            }
                        }
                    }
                }
            }
        });

        // Setup Ctrl+C handler for graceful shutdown
        let cancellation_token_clone = self.cancellation_token.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            println!("\n🛑 Ctrl+C received, initiating graceful shutdown...");
            cancellation_token_clone.cancel();
        });

        // Wait for worker completion
        tokio::select! {
            result = worker_handle => {
                println!("🏁 Worker completed: {:?}", result);
            }
            _ = tokio::signal::ctrl_c() => {
                println!("🛑 Immediate shutdown requested");
            }
        }

        // Cancel statistics reporter
        stats_reporter.abort();

        // Wait for all processing tasks to complete
        println!("⏳ Waiting for ongoing tasks to complete...");
        let _ = tokio::time::timeout(self.config.shutdown_timeout, async {
            // Wait for all permits to be released
            while self.semaphore.available_permits() < self.config.max_concurrent_tasks {
                sleep(Duration::from_millis(100)).await;
            }
        }).await;

        // Print final statistics
        let final_stats = self.stats.read().await;
        Self::print_final_stats(&*final_stats);

        println!("✅ Worker pool shutdown complete");
        Ok(())
    }

    /// Process a single message
    async fn process_message(message: &QueueMessage) -> ProcessResult {
        // Simulate different processing scenarios based on message content
        let payload = message.payload.as_str().unwrap_or("");

        // Simulate processing time
        let processing_time = match message.priority {
            QueuePriority::Critical => Duration::from_millis(100),
            QueuePriority::High => Duration::from_millis(200),
            QueuePriority::Normal => Duration::from_millis(500),
            QueuePriority::Low => Duration::from_millis(1000),
        };

        sleep(processing_time).await;

        // Check for special message patterns that trigger different behaviors
        if payload.contains("error-retryable") {
            return ProcessResult::RetryableError("Simulated retryable error".to_string());
        }

        if payload.contains("error-permanent") {
            return ProcessResult::PermanentError("Simulated permanent error".to_string());
        }

        if payload.contains("slow") {
            sleep(Duration::from_secs(2)).await;
        }

        // Simulate different success scenarios
        match payload {
            p if p.contains("notification") => {
                println!("📧 Sending notification: {}", p);
                ProcessResult::Success
            }
            p if p.contains("email") => {
                println!("📧 Processing email: {}", p);
                ProcessResult::Success
            }
            p if p.contains("report") => {
                println!("📊 Generating report: {}", p);
                ProcessResult::Success
            }
            p if p.contains("cleanup") => {
                println!("🧹 Performing cleanup: {}", p);
                ProcessResult::Success
            }
            _ => {
                println!("✅ Processed generic task: {}", payload);
                ProcessResult::Success
            }
        }
    }

    /// Print current statistics
    fn print_stats(stats: &WorkerStats) {
        println!("📊 Worker Statistics:");
        println!("  - Messages processed: {}", stats.messages_processed);
        println!("  - Messages failed: {}", stats.messages_failed);
        println!("  - Messages retried: {}", stats.messages_retried);

        if stats.messages_processed > 0 {
            let avg_processing_time = stats.total_processing_time / stats.messages_processed as u32;
            println!("  - Avg processing time: {:?}", avg_processing_time);
        }

        let success_rate = if stats.messages_processed + stats.messages_failed > 0 {
            stats.messages_processed as f64 / (stats.messages_processed + stats.messages_failed) as f64 * 100.0
        } else {
            0.0
        };
        println!("  - Success rate: {:.1}%", success_rate);
    }

    /// Print final statistics summary
    fn print_final_stats(stats: &WorkerStats) {
        println!("\n📋 Final Worker Statistics:");
        println!("===========================");

        let total_messages = stats.messages_processed + stats.messages_failed;

        println!("  📈 Total messages handled: {}", total_messages);
        println!("  ✅ Successfully processed: {}", stats.messages_processed);
        println!("  ❌ Failed: {}", stats.messages_failed);
        println!("  🔄 Retries attempted: {}", stats.messages_retried);

        if total_messages > 0 {
            let success_rate = stats.messages_processed as f64 / total_messages as f64 * 100.0;
            let error_rate = stats.messages_failed as f64 / total_messages as f64 * 100.0;

            println!("  📊 Success rate: {:.1}%", success_rate);
            println!("  📊 Error rate: {:.1}%", error_rate);

            if stats.messages_processed > 0 {
                let avg_time = stats.total_processing_time / stats.messages_processed as u32;
                let throughput = stats.messages_processed as f64 / stats.total_processing_time.as_secs_f64();
                println!("  ⏱️  Average processing time: {:?}", avg_time);
                println!("  🚀 Throughput: {:.2} messages/sec", throughput);
            }
        }

        println!("  ⏳ Total processing time: {:?}", stats.total_processing_time);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🚀 Queue Worker Pool Example");
    println!("============================");

    // Create Redis queue
    println!("📡 Setting up Redis queue...");
    let queue = Arc::new(
        RedisQueueBuilder::new()
            .url("redis://localhost:6379")
            .queue_name("worker_pool_queue")
            .key_prefix("worker")
            .pool_size(20)
            .build()
            .await?
    );

    // Test connection
    if !queue.test_connection().await? {
        eprintln!("❌ Failed to connect to Redis");
        return Ok(());
    }

    // Clear any existing messages
    println!("🧹 Clearing existing messages...");
    queue.purge().await?;

    // Populate queue with test messages
    println!("📤 Populating queue with test messages...");
    let test_messages = vec![
        ("Send welcome email notification", QueuePriority::High),
        ("Generate daily report", QueuePriority::Normal),
        ("Process user cleanup task", QueuePriority::Low),
        ("Send critical alert", QueuePriority::Critical),
        ("Process payment notification", QueuePriority::High),
        ("Generate weekly analytics report", QueuePriority::Normal),
        ("Perform system maintenance cleanup", QueuePriority::Low),
        ("Handle emergency notification", QueuePriority::Critical),
        ("Process bulk email notification", QueuePriority::Normal),
        ("Simulate error-retryable scenario", QueuePriority::Normal),
        ("Simulate error-permanent scenario", QueuePriority::Low),
        ("Process slow report generation task", QueuePriority::Low),
    ];

    let mut message_ids = Vec::new();
    for (i, (payload, priority)) in test_messages.into_iter().enumerate() {
        let message = QueueMessage::builder()
            .id(format!("test-msg-{}", i + 1))
            .payload(payload)
            .priority(priority)
            .max_receive_count(3)
            .visibility_timeout(30)
            .build();

        match queue.enqueue(message).await {
            Ok(id) => {
                message_ids.push(id);
                println!("  ✅ Enqueued: {}", payload);
            }
            Err(e) => {
                eprintln!("  ❌ Failed to enqueue: {}", e);
            }
        }
    }

    println!("📊 Enqueued {} messages", message_ids.len());
    println!("📈 Queue size: {}", queue.size().await?);

    // Configure worker
    let worker_config = WorkerConfig {
        max_concurrent_tasks: 5,
        poll_interval: Duration::from_millis(50),
        max_retries: 2,
        visibility_timeout: 30,
        shutdown_timeout: Duration::from_secs(10),
    };

    // Create and start worker pool
    let worker = QueueWorker::new(queue, worker_config);

    // Start the worker pool
    worker.start().await?;

    println!("\n🎉 Worker pool example completed!");
    Ok(())
}