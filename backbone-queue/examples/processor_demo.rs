//! Message Processor with Batch Processing Demo
//!
//! Demonstrates comprehensive message processing capabilities including
//! individual processing, batch processing, retry logic, and performance monitoring.

use backbone_queue::{
    MessageProcessor, ProcessingOutcome, BatchConfig, BatchTimeoutPolicy, RetryConfig, RetryPolicy,
    RetryHandler, BatchingProcessor, SimpleMessageProcessor, QueueMessage,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("⚙️  Message Processor with Batch Processing Demo");
    println!("===============================================");

    // Test different processing scenarios
    demo_individual_processing().await?;
    demo_batch_processing().await?;
    demo_batching_processor().await?;
    demo_retry_logic().await?;
    demo_performance_monitoring().await?;
    demo_custom_processor().await?;

    println!("\n🎉 Message processor demo completed!");

    Ok(())
}

async fn demo_individual_processing() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n✅ Individual Message Processing");
    println!("=================================");

    let processor = SimpleMessageProcessor::success_processor();

    let messages = vec![
        QueueMessage::new("msg-1".to_string(), serde_json::json!({"task": "send_email"})),
        QueueMessage::new("msg-2".to_string(), serde_json::json!({"task": "process_payment"})),
        QueueMessage::new("msg-3".to_string(), serde_json::json!({"task": "update_inventory"})),
    ];

    let mut total_time = Duration::ZERO;
    let mut success_count = 0;

    for message in messages {
        let context = create_processing_context(&processor);
        let start_time = std::time::Instant::now();

        let processed = processor.process_message(message, context).await;
        total_time += start_time.elapsed();

        if processed.outcome == ProcessingOutcome::Success {
            success_count += 1;
        }

        println!("  Processed message: {} in {}ms",
            processed.message.id,
            processed.processing_time.as_millis()
        );
    }

    let stats = processor.get_stats().await;
    println!("Individual processing results:");
    println!("  Total messages: {}", stats.total_messages_processed);
    println!("  Success rate: {:.2}%", stats.success_rate() * 100.0);
    println!("  Average processing time: {:.2}ms", stats.avg_processing_time_ms);
    println!("  Total time: {}ms", total_time.as_millis());

    Ok(())
}

async fn demo_batch_processing() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📦 Batch Message Processing");
    println!("===========================");

    let processor = Arc::new(SimpleMessageProcessor::success_processor());

    let mut messages = Vec::new();
    for i in 1..=50 {
        messages.push(QueueMessage::new(
            format!("batch-msg-{}", i),
            serde_json::json!({
                "batch_id": 1,
                "item_id": i,
                "operation": "process_order"
            })
        ));
    }

    let context = create_processing_context(&*processor);

    println!("Processing batch of {} messages...", messages.len());
    let start_time = std::time::Instant::now();

    let batch_result = processor.process_batch(messages, context).await;
    let total_time = start_time.elapsed();

    println!("Batch processing results:");
    println!("  Total messages: {}", batch_result.total_messages);
    println!("  Successful: {}", batch_result.successful_messages.len());
    println!("  Failed: {}", batch_result.failed_messages.len());
    println!("  Retried: {}", batch_result.retried_messages.len());
    println!("  Success rate: {:.2}%", batch_result.success_rate() * 100.0);
    println!("  Total batch time: {}ms", batch_result.total_processing_time.as_millis());
    println!("  Avg time per message: {:.2}ms", batch_result.avg_processing_time_per_message().as_millis());
    println!("  Overall processing time: {}ms", total_time.as_millis());

    Ok(())
}

async fn demo_batching_processor() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔄 Batching Processor Demo");
    println!("===========================");

    let inner = Arc::new(SimpleMessageProcessor::success_processor());

    let batch_config = BatchConfig {
        max_batch_size: 10,
        min_batch_size: 3,
        max_wait_time_ms: 2000, // 2 seconds
        enable_auto_batch: true,
        batch_timeout_policy: BatchTimeoutPolicy::WaitFull,
    };

    let batch_processor = Arc::new(BatchingProcessor::new(inner.clone(), batch_config));

    // Start background processor
    batch_processor.start_background_processor().await?;

    println!("Adding messages to batch processor...");

    // Add messages one by one
    for i in 1..=25 {
        let message = QueueMessage::new(
            format!("auto-batch-{}", i),
            serde_json::json!({
                "auto_batch": true,
                "message_number": i,
                "timestamp": chrono::Utc::now()
            })
        );

        batch_processor.add_to_batch(message).await?;

        if i % 5 == 0 {
            println!("  Added {} messages to batch, current batch size: {}",
                i, batch_processor.current_batch_size().await);
        }

        // Small delay between messages
        sleep(Duration::from_millis(100)).await;
    }

    println!("Final batch size: {}", batch_processor.current_batch_size().await);

    // Wait for background processing
    println!("Waiting for background processing...");
    sleep(Duration::from_secs(3)).await;

    let stats = batch_processor.get_stats().await;
    println!("Batching processor stats:");
    println!("  Total batches processed: {}", stats.total_batches_processed);
    println!("  Total messages processed: {}", stats.total_messages_processed);
    println!("  Success rate: {:.2}%", stats.success_rate() * 100.0);
    println!("  Average batch processing time: {:.2}ms", stats.avg_batch_processing_time_ms);

    Ok(())
}

async fn demo_retry_logic() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔄 Retry Logic Demo");
    println!("===================");

    let retry_config = RetryConfig {
        max_attempts: 5,
        initial_delay_ms: 500, // 0.5 second
        backoff_multiplier: 2.0,
        max_delay_ms: 10000, // 10 seconds
        jitter_percentage: 0.1, // 10% jitter
        retry_policy: RetryPolicy::Exponential,
    };

    let retry_handler = RetryHandler::new(retry_config);

    println!("Retry configuration:");
    println!("  Max attempts: {}", retry_handler.config().max_attempts);
    println!("  Initial delay: {}ms", retry_handler.config().initial_delay_ms);
    println!("  Backoff multiplier: {:.1}", retry_handler.config().backoff_multiplier);
    println!("  Max delay: {}ms", retry_handler.config().max_delay_ms);
    println!("  Jitter: {:.1}%", retry_handler.config().jitter_percentage * 100.0);

    println!("\nCalculated retry delays:");
    for attempt in 0..5 {
        let delay = retry_handler.calculate_delay(attempt);
        println!("  Attempt {}: {}ms", attempt, delay.as_millis());
    }

    // Simulate retry scenario
    let processor = SimpleMessageProcessor::failure_processor(); // Always fails
    let message = QueueMessage::new("retry-test".to_string(), serde_json::json!({"test": true}));

    let mut attempt = 0;
    let mut outcome = ProcessingOutcome::Retry { delay_seconds: 1 };

    while retry_handler.should_retry(attempt, &outcome) && attempt < retry_handler.config().max_attempts {
        attempt += 1;

        println!("\nRetry attempt {}:", attempt);

        let context = create_processing_context_with_attempt(&processor, attempt, retry_handler.config().max_attempts);
        let processed = processor.process_message(message.clone(), context).await;

        println!("  Processing outcome: {:?}", processed.outcome);
        println!("  Processing time: {}ms", processed.processing_time.as_millis());

        if processed.outcome == ProcessingOutcome::Success {
            println!("  ✅ Processing succeeded!");
            break;
        } else if processed.outcome == ProcessingOutcome::Failed {
            println!("  ❌ Processing failed permanently");
            break;
        } else {
            let delay = retry_handler.calculate_delay(attempt);
            println!("  ⏳ Retrying in {}ms...", delay.as_millis());
            sleep(delay).await;
        }

        outcome = processed.outcome;
    }

    let final_stats = processor.get_stats().await;
    println!("\nRetry demo stats:");
    println!("  Total processing attempts: {}", final_stats.total_messages_processed);
    println!("  Total failures: {}", final_stats.total_messages_failed);

    Ok(())
}

async fn demo_performance_monitoring() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📊 Performance Monitoring Demo");
    println!("==============================");

    let processor = SimpleMessageProcessor::random_processor(0.3); // 30% failure rate

    println!("Processing 100 messages with 30% random failure rate...");

    let start_time = std::time::Instant::now();

    // Process messages in batches to demonstrate performance
    for batch_num in 1..=10 {
        let mut messages = Vec::new();
        for i in 1..=10 {
            messages.push(QueueMessage::new(
                format!("perf-msg-{}-{}", batch_num, i),
                serde_json::json!({
                    "batch": batch_num,
                    "message": i,
                    "data": "performance test".to_string().repeat(10)
                })
            ));
        }

        let context = create_processing_context(&processor);
        let batch_result = processor.process_batch(messages, context).await;

        println!("  Batch {}: {} successful, {} failed, {} retried ({}ms)",
            batch_num,
            batch_result.successful_messages.len(),
            batch_result.failed_messages.len(),
            batch_result.retried_messages.len(),
            batch_result.total_processing_time.as_millis()
        );
    }

    let total_time = start_time.elapsed();

    let stats = processor.get_stats().await;
    println!("\nPerformance Monitoring Results:");
    println!("  Total processing time: {}ms", total_time.as_millis());
    println!("  Total messages processed: {}", stats.total_messages_processed);
    println!("  Successful messages: {}", stats.total_messages_succeeded);
    println!("  Failed messages: {}", stats.total_messages_failed);
    println!("  Success rate: {:.2}%", stats.success_rate() * 100.0);
    println!("  Failure rate: {:.2}%", stats.failure_rate() * 100.0);
    println!("  Average processing time per message: {:.2}ms", stats.avg_processing_time_ms);
    println!("  Messages per second: {:.2}", stats.total_messages_processed as f64 / total_time.as_secs_f64());
    println!("  Processor uptime: {}s", stats.uptime_seconds);

    if let Some(last_processed) = stats.last_processed_at {
        println!("  Last processed: {}", last_processed.format("%Y-%m-%d %H:%M:%S UTC"));
    }

    Ok(())
}

async fn demo_custom_processor() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🛠️  Custom Processor Demo");
    println!("=========================");

    // Create a custom processor
    let custom_processor = CustomOrderProcessor::new();
    let processor: Arc<dyn MessageProcessor> = Arc::new(custom_processor);

    // Create test order messages
    let orders = vec![
        OrderMessage {
            id: "order-001".to_string(),
            customer_id: "cust-001".to_string(),
            items: vec!["item-001", "item-002"],
            total_amount: 99.99,
            priority: "high".to_string(),
        },
        OrderMessage {
            id: "order-002".to_string(),
            customer_id: "cust-002".to_string(),
            items: vec!["item-003"],
            total_amount: 49.99,
            priority: "normal".to_string(),
        },
        OrderMessage {
            id: "order-003".to_string(),
            customer_id: "cust-003".to_string(),
            items: vec!["item-004", "item-005", "item-006"],
            total_amount: 199.99,
            priority: "low".to_string(),
        },
    ];

    println!("Processing custom order messages...");

    for order in orders {
        let message = QueueMessage::new(
            order.id.clone(),
            serde_json::to_value(&order).unwrap()
        );

        let context = create_processing_context(&*processor);

        if processor.can_process(&message).await {
            let processed = processor.process_message(message, context).await;

            println!("  Order {}: {}",
                processed.message.id,
                match processed.outcome {
                    ProcessingOutcome::Success => "✅ Processed",
                    ProcessingOutcome::Failed => "❌ Failed",
                    ProcessingOutcome::Retry { .. } => "🔄 Retrying",
                    ProcessingOutcome::Rejected => "⚠️ Rejected"
                }
            );

            if let Some(result_data) = &processed.result_data {
                if let Some(order_result) = result_data.get("order_result") {
                    println!("    Result: {}", order_result);
                }
            }
        } else {
            println!("  Order {}: ⚠️ Rejected - cannot process", order.id);
        }
    }

    let stats = processor.get_stats().await;
    println!("\nCustom processor stats:");
    println!("  Processor name: {}", processor.processor_name());
    println!("  Processor version: {}", processor.processor_version());
    println!("  Messages processed: {}", stats.total_messages_processed);
    println!("  Success rate: {:.2}%", stats.success_rate() * 100.0);

    Ok(())
}

fn create_processing_context(processor: &dyn MessageProcessor) -> backbone_queue::ProcessingContext {
    backbone_queue::ProcessingContext {
        processing_id: Uuid::new_v4().to_string(),
        processor_id: processor.processor_name().to_string(),
        start_time: std::time::Instant::now(),
        attempt_number: 1,
        max_attempts: 3,
        batch_context: None,
        metadata: std::collections::HashMap::new(),
    }
}

fn create_processing_context_with_attempt(
    processor: &dyn MessageProcessor,
    attempt: u32,
    max_attempts: u32,
) -> backbone_queue::ProcessingContext {
    backbone_queue::ProcessingContext {
        processing_id: Uuid::new_v4().to_string(),
        processor_id: processor.processor_name().to_string(),
        start_time: std::time::Instant::now(),
        attempt_number,
        max_attempts,
        batch_context: None,
        metadata: std::collections::HashMap::new(),
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct OrderMessage {
    id: String,
    customer_id: String,
    items: Vec<String>,
    total_amount: f64,
    priority: String,
}

struct CustomOrderProcessor {
    stats: Arc<tokio::sync::RwLock<backbone_queue::ProcessorStats>>,
    start_time: std::time::Instant,
}

impl CustomOrderProcessor {
    fn new() -> Self {
        Self {
            stats: Arc::new(tokio::sync::RwLock::new(backbone_queue::ProcessorStats::default())),
            start_time: std::time::Instant::now(),
        }
    }
}

#[async_trait::async_trait]
impl MessageProcessor for CustomOrderProcessor {
    async fn process_message(
        &self,
        message: QueueMessage,
        context: backbone_queue::ProcessingContext,
    ) -> backbone_queue::ProcessedMessage {
        let start_time = std::time::Instant::now();

        // Parse order message
        let order: Result<OrderMessage, _> = serde_json::from_value(message.payload.clone());

        let (outcome, result_data) = match order {
            Ok(order) => {
                // Business logic for order processing
                if order.total_amount > 0.0 && !order.items.is_empty() {
                    // Simulate order processing based on priority
                    let processing_time = match order.priority.as_str() {
                        "high" => 50,
                        "normal" => 100,
                        "low" => 200,
                        _ => 100,
                    };

                    sleep(std::time::Duration::from_millis(processing_time)).await;

                    let outcome = ProcessingOutcome::Success;
                    let result_data = serde_json::json!({
                        "order_result": "processed",
                        "processed_items": order.items.len(),
                        "total_amount": order.total_amount,
                        "customer": order.customer_id
                    });

                    (outcome, Some(result_data))
                } else {
                    let outcome = ProcessingOutcome::Rejected;
                    let result_data = serde_json::json!({
                        "order_result": "rejected",
                        "reason": "Invalid order: zero amount or no items"
                    });

                    (outcome, Some(result_data))
                }
            }
            Err(_) => {
                let outcome = ProcessingOutcome::Rejected;
                let result_data = serde_json::json!({
                    "order_result": "rejected",
                    "reason": "Invalid order format"
                });

                (outcome, Some(result_data))
            }
        };

        let processing_time = start_time.elapsed();

        // Update statistics
        let mut stats = self.stats.write().await;
        stats.total_messages_processed += 1;
        stats.last_processed_at = Some(chrono::Utc::now());
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

        backbone_queue::ProcessedMessage {
            message,
            outcome,
            processing_time,
            context,
            error_message: None,
            result_data,
        }
    }

    fn processor_name(&self) -> &str {
        "CustomOrderProcessor"
    }

    async fn get_stats(&self) -> backbone_queue::ProcessorStats {
        let mut stats = self.stats.read().await.clone();
        stats.uptime_seconds = self.start_time.elapsed().as_secs();
        stats
    }

    async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = backbone_queue::ProcessorStats::default();
    }
}

fn show_processor_features() {
    println!("\n🔧 Message Processor Features");
    println!("=============================");

    let features = vec![
        ("Individual Processing", "Process messages one by one with full control"),
        ("Batch Processing", "Process multiple messages efficiently in batches"),
        ("Retry Logic", "Configurable retry strategies with exponential backoff"),
        ("Performance Monitoring", "Built-in statistics and performance metrics"),
        ("Custom Processors", "Implement custom business logic processors"),
        ("Background Processing", "Automated batch processing in the background"),
        ("Error Handling", "Comprehensive error handling and recovery"),
        ("Memory Efficient", "Optimized memory usage for large batches"),
    ];

    for (feature, description) in features {
        println!("  • {}: {}", feature, description);
    }
}

fn show_retry_strategies() {
    println!("\n🔄 Retry Strategies");
    println!("=====================");

    let strategies = vec![
        ("Fixed Delay", "Same delay between all retry attempts"),
        ("Exponential Backoff", "Increasing delay with each retry (recommended)"),
        ("Linear Backoff", "Linearly increasing delay between retries"),
        ("Custom Jitter", "Add randomness to prevent thundering herd"),
    ];

    for (strategy, description) in strategies {
        println!("  • {}: {}", strategy, description);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_processor_demo_completes() {
        // This test ensures the demo runs without panicking
        demo_individual_processing().await.unwrap();
        demo_batch_processing().await.unwrap();
        demo_retry_logic().await.unwrap();
        demo_performance_monitoring().await.unwrap();
        demo_custom_processor().await.unwrap();
    }

    #[tokio::test]
    async fn test_batch_config_factory() {
        // Show common batch configurations
        let low_volume_config = BatchConfig {
            max_batch_size: 10,
            min_batch_size: 1,
            max_wait_time_ms: 1000,
            enable_auto_batch: true,
            batch_timeout_policy: BatchTimeoutPolicy::Immediate,
        };

        let high_volume_config = BatchConfig {
            max_batch_size: 1000,
            min_batch_size: 50,
            max_wait_time_ms: 5000,
            enable_auto_batch: true,
            batch_timeout_policy: BatchTimeoutPolicy::WaitFull,
        };

        assert!(low_volume_config.max_batch_size < high_volume_config.max_batch_size);
        assert!(low_volume_config.min_batch_size < high_volume_config.min_batch_size);
    }
}