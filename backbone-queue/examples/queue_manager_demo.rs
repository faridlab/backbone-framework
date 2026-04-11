//! Queue Manager Demo
//!
//! Demonstrates administrative operations using the QueueManager.

use backbone_queue::{
    QueueManager, QueueConfig, QueueAdminService, QueueMessage,
    compression::{CompressionConfig, CompressionAlgorithm},
    fifo::{FifoQueueConfig, utils::MessageVolume},
    monitoring::AlertThresholds,
};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🏗️  Queue Manager Demo");
    println!("========================");

    // Create queue manager
    let manager = QueueManager::new();

    // Demonstrate queue configuration creation
    demo_queue_config_creation().await?;

    // Show queue management operations
    demo_queue_management(&manager).await?;

    // Demonstrate maintenance operations
    demo_maintenance_operations(&manager).await?;

    println!("\n🎉 Queue manager demo completed!");

    Ok(())
}

/// Demonstrate creating different types of queue configurations
async fn demo_queue_config_creation() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📝 Queue Configuration Examples");
    println!("================================");

    // Basic Redis queue
    let redis_config = QueueConfig::new(
        "orders".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );
    println!("✅ Basic Redis config: {} ({})", redis_config.name, redis_config.queue_type);

    // Advanced Redis queue with FIFO and compression
    let mut advanced_config = QueueConfig::new(
        "high-priority-tasks".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    // Enable FIFO
    advanced_config.fifo_enabled = true;
    advanced_config.fifo_config = Some(
        backbone_queue::fifo::utils::get_recommended_config(MessageVolume::High)
    );

    // Enable compression
    advanced_config.compression_enabled = true;
    advanced_config.compression_config = Some(CompressionConfig {
        algorithm: CompressionAlgorithm::Gzip,
        level: 6,
        min_size: 1024,
        force_compression: false,
        max_attempts: 3,
    });

    // Set monitoring thresholds
    advanced_config.monitoring_enabled = true;
    advanced_config.alert_thresholds = Some(AlertThresholds {
        queue_depth_threshold: 1000,
        error_rate_threshold: 5.0,
        latency_threshold_ms: 500,
        throughput_threshold_min: 10.0,
        max_queue_age_minutes: 30,
    });

    // Add metadata
    advanced_config.metadata.insert("team".to_string(), "backend".to_string());
    advanced_config.metadata.insert("environment".to_string(), "production".to_string());
    advanced_config.metadata.insert("criticality".to_string(), "high".to_string());

    println!("✅ Advanced Redis config: {} (FIFO: {}, Compression: {})",
        advanced_config.name,
        advanced_config.fifo_enabled,
        advanced_config.compression_enabled
    );

    // SQS queue configuration
    let sqs_config = QueueConfig::new(
        "notifications".to_string(),
        "sqs".to_string(),
        "https://sqs.us-east-1.amazonaws.com/123456789012/notifications".to_string(),
    );

    // Configure SQS-specific settings
    let mut sqs_with_features = sqs_config;
    sqs_with_features.visibility_timeout = 60;
    sqs_with_features.max_receive_count = 5;
    sqs_with_features.max_size = Some(256 * 1024); // 256KB
    sqs_with_features.dead_letter_queue = Some("notifications-dlq".to_string());
    sqs_with_features.retention_seconds = Some(14 * 24 * 60 * 60); // 14 days

    println!("✅ SQS config: {} (DLQ: {}, Retention: {} days)",
        sqs_with_features.name,
        sqs_with_features.dead_letter_queue.as_ref().unwrap(),
        sqs_with_features.retention_seconds.unwrap() / (24 * 60 * 60)
    );

    // Validate configurations
    println!("\n🔍 Configuration Validation:");

    let validation_errors = redis_config.validate();
    match validation_errors {
        Ok(_) => println!("  ✅ Basic Redis config is valid"),
        Err(errors) => println!("  ❌ Basic Redis config errors: {}", errors),
    }

    let advanced_errors = advanced_config.validate();
    match advanced_errors {
        Ok(_) => println!("  ✅ Advanced Redis config is valid"),
        Err(errors) => println!("  ❌ Advanced Redis config errors: {}", errors),
    }

    let sqs_errors = sqs_with_features.validate();
    match sqs_errors {
        Ok(_) => println!("  ✅ SQS config is valid"),
        Err(errors) => println!("  ❌ SQS config errors: {}", errors),
    }

    Ok(())
}

/// Demonstrate queue management operations
async fn demo_queue_management(manager: &QueueManager) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔧 Queue Management Operations");
    println!("===============================");

    // Note: In a real scenario, you would create actual queue instances
    // For this demo, we'll show the API usage

    println!("📋 Listing queues:");
    let queues = manager.list_queues().await;
    println!("  Current queues: {} registered", queues.len());

    for queue_name in queues {
        println!("  - {}", queue_name);
    }

    // Show configuration management
    println!("\n⚙️  Configuration Management:");

    // Create a sample configuration
    let sample_config = QueueConfig::new(
        "demo-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    println!("  📝 Sample config created for: {}", sample_config.name);
    println!("     - Type: {}", sample_config.queue_type);
    println!("     - Connection: {}", sample_config.connection_url);
    println!("     - Visibility timeout: {}s", sample_config.visibility_timeout);
    println!("     - Max receive count: {}", sample_config.max_receive_count);
    println!("     - FIFO enabled: {}", sample_config.fifo_enabled);
    println!("     - Monitoring enabled: {}", sample_config.monitoring_enabled);

    // Demonstrate admin service operations
    println!("\n👨‍💼 Admin Service Operations:");

    let admin_manager: &dyn QueueAdminService = manager;

    println!("  📊 Getting queue statistics...");
    let queues = admin_manager.list_queues().await?;
    println!("     Total queues: {}", queues.len());

    // Show health checking (would work if queues were actually registered)
    println!("\n🏥 Health Monitoring:");
    println!("  Health checks available for all registered queues");
    println!("  Metrics collection enabled by default");
    println!("  Alert thresholds configurable per queue");

    Ok(())
}

/// Demonstrate maintenance operations
async fn demo_maintenance_operations(manager: &QueueManager) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🛠️  Maintenance Operations");
    println!("===========================");

    let admin_manager: &dyn QueueAdminService = manager;

    // Show maintenance capabilities
    println!("🔧 Available Maintenance Actions:");
    println!("  • Cleanup expired messages");
    println!("  • Cleanup deduplication cache (FIFO queues)");
    println!("  • Update queue metrics");
    println!("  • Compact queue storage");
    println!("  • Rebuild indexes");
    println!("  • Optimize performance");

    println!("\n🚀 Performing Maintenance:");

    // Perform maintenance on all queues (would work if queues were registered)
    println!("  📊 Running maintenance on all queues...");

    // In a real scenario, this would perform actual maintenance
    let sample_results = vec![
        create_sample_maintenance_result("orders-queue".to_string()),
        create_sample_maintenance_result("notifications-queue".to_string()),
        create_sample_maintenance_result("high-priority-tasks".to_string()),
    ];

    println!("  📈 Maintenance Results:");
    for result in sample_results {
        if result.success {
            println!("    ✅ {}: {} actions ({}ms)",
                result.queue_name,
                result.actions.len(),
                result.duration_ms
            );

            for action in result.actions {
                match action {
                    backbone_queue::MaintenanceAction::CleanupExpired => {
                        println!("      🗑️  Cleaned up expired messages");
                    }
                    backbone_queue::MaintenanceAction::CleanupDeduplication(count) => {
                        println!("      🧹 Cleaned up {} deduplication entries", count);
                    }
                    backbone_queue::MaintenanceAction::UpdateMetrics => {
                        println!("      📊 Updated metrics");
                    }
                    backbone_queue::MaintenanceAction::CompactStorage => {
                        println!("      💾 Compacted storage");
                    }
                    backbone_queue::MaintenanceAction::RebuildIndexes => {
                        println!("      🔄 Rebuilt indexes");
                    }
                    backbone_queue::MaintenanceAction::Optimize => {
                        println!("      ⚡ Optimized performance");
                    }
                }
            }
        } else {
            println!("    ❌ {}: {} ({})",
                result.queue_name,
                result.duration_ms,
                result.error_message.unwrap_or_else(|| "Unknown error".to_string())
            );
        }
    }

    println!("\n📅 Maintenance Scheduling:");
    println!("  • Daily cleanup at 2:00 AM");
    println!("  • Weekly compaction on Sundays");
    println!("  • Monthly optimization");
    println!("  • Emergency cleanup when queue depth > 10,000");

    Ok(())
}

/// Create sample maintenance result for demonstration
fn create_sample_maintenance_result(queue_name: String) -> backbone_queue::MaintenanceResult {
    use backbone_queue::{MaintenanceAction, MaintenanceResult};

    let actions = match queue_name.as_str() {
        "orders-queue" => vec![
            MaintenanceAction::CleanupExpired,
            MaintenanceAction::UpdateMetrics,
        ],
        "notifications-queue" => vec![
            MaintenanceAction::CleanupDeduplication(42),
            MaintenanceAction::UpdateMetrics,
            MaintenanceAction::CompactStorage,
        ],
        "high-priority-tasks" => vec![
            MaintenanceAction::CleanupExpired,
            MaintenanceAction::CleanupDeduplication(15),
            MaintenanceAction::UpdateMetrics,
            MaintenanceAction::RebuildIndexes,
            MaintenanceAction::Optimize,
        ],
        _ => vec![MaintenanceAction::UpdateMetrics],
    };

    MaintenanceResult {
        queue_name,
        success: true,
        duration_ms: 150 + (actions.len() as u64 * 25),
        actions,
        error_message: None,
    }
}

/// Demonstrate advanced configuration patterns
async fn demo_advanced_configuration() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n⚡ Advanced Configuration Patterns");
    println!("===================================");

    println!("🎯 Message Volume Recommendations:");

    let volumes = vec![
        (backbone_queue::MessageVolume::Low, "Small service (< 1K msg/day)"),
        (backbone_queue::MessageVolume::Medium, "Medium service (1K-10K msg/day)"),
        (backbone_queue::MessageVolume::High, "Large service (10K+ msg/day)"),
    ];

    for (volume, description) in volumes {
        let fifo_config = backbone_queue::fifo::utils::get_recommended_config(volume);
        println!("  📊 {}: {}", volume as i32, description);
        println!("     - FIFO enabled: {}", fifo_config.enabled);
        println!("     - Deduplication window: {}s", fifo_config.deduplication_window_seconds);
        println!("     - Max message groups: {}", fifo_config.max_message_groups);
        println!("     - Content deduplication: {}", fifo_config.enable_content_deduplication);
    }

    println!("\n🗜️  Compression Recommendations:");

    let compression_recs = vec![
        (512, "Small messages - no compression"),
        (2048, "Medium messages - GZIP recommended"),
        (8192, "Large messages - GZIP high compression"),
        (32768, "Very large messages - ZLIB recommended"),
    ];

    for (size, recommendation) in compression_recs {
        let compression_config = backbone_queue::compression::utils::get_compression_recommendations(size);
        println!("  📦 {} bytes: {}", size, recommendation);
        println!("     - Algorithm: {:?}", compression_config.algorithm);
        println!("     - Level: {}", compression_config.level);
        println!("     - Min size: {}", compression_config.min_size);
    }

    Ok(())
}