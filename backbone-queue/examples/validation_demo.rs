//! Configuration Validation Demo
//!
//! Demonstrates comprehensive validation capabilities for queue configurations.

use backbone_queue::{
    ConfigValidator, ValidationResult, ValidationEnvironment, QueueConfig,
    compression::CompressionConfig,
    monitoring::AlertThresholds,
    fifo::FifoQueueConfig, utils::MessageVolume,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🔍 Configuration Validation Demo");
    println!("==============================");

    // Test different configurations
    demo_valid_configuration().await?;
    demo_invalid_configuration().await?;
    demo_comprehensive_validation().await?;
    demo_environment_specific_validation().await?;

    println!("\n🎉 Validation demo completed!");

    Ok(())
}

async fn demo_valid_configuration() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n✅ Valid Configuration Example");
    println!("==========================");

    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let config = QueueConfig::new(
        "orders".to_string(),
        "redis".to_string(),
        "redis://localhost:6379/0".to_string(),
    );

    let result = validator.validate_queue_config(&config).await;

    println!("Queue: {}", config.name);
    println!("Type: {}", config.queue_type);
    println!("Valid: {}", result.is_valid);

    if result.is_valid {
        println!("✅ Configuration is valid and ready to use!");
    } else {
        println!("❌ Configuration has errors:");
        for (i, error) in result.errors.iter().enumerate() {
            println!("  {}. {} ({})", i + 1, error.message, error.code);
        }
    }

    if !result.warnings.is_empty() {
        println!("⚠️  Warnings:");
        for (i, warning) in result.warnings.iter().enumerate() {
            println!("  {}. {}", i + 1, warning.message);
        }
    }

    if !result.recommendations.is_empty() {
        println!("💡 Recommendations:");
        for (i, rec) in result.recommendations.iter().enumerate() {
            println!("  {}. {}", i + 1, rec);
        }
    }

    Ok(())
}

async fn demo_invalid_configuration() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n❌ Invalid Configuration Example");
    println!("===============================");

    let validator = ConfigValidator::new(ValidationEnvironment::Production);

    let mut config = QueueConfig::new(
        "".to_string(), // Invalid: empty name
        "invalid".to_string(), // Invalid: unsupported type
        "".to_string(), // Invalid: empty URL
    );

    config.visibility_timeout = 0; // Invalid: must be > 0
    config.max_receive_count = 0; // Invalid: must be > 0

    let result = validator.validate_queue_config(&config).await;

    println!("Queue: {}", config.name);
    println!("Type: {}", config.queue_type);
    println!("Valid: {}", result.is_valid);

    if !result.is_valid {
        println!("🚫 Configuration errors:");
        for (i, error) in result.errors.iter().enumerate() {
            println!("  {}. {}", i + 1, error.message);
            if let Some(field) = &error.field {
                println!("     Field: {}", field);
            }
            if let Some(suggested) = &error.suggested_fix {
                println!("     Suggested fix: {}", suggested);
            }
        }
    }

    Ok(())
}

async fn demo_comprehensive_validation() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔧 Comprehensive Configuration Example");
    println!("=====================================");

    let validator = ConfigValidator::new(ValidationEnvironment::Production);

    let mut config = QueueConfig::new(
        "critical-payments".to_string(),
        "sqs".to_string(),
        "https://sqs.us-east-1.amazonaws.com/123456789012/critical-payments".to_string(),
    );

    // Enable all advanced features
    config.fifo_enabled = true;
    config.fifo_config = Some(FifoQueueConfig {
        enabled: true,
        deduplication_window_seconds: 300,
        max_message_groups: 1000,
        enable_content_deduplication: true,
        content_deduplication_window_seconds: 60,
        max_deduplicated_messages: 10000,
    });

    config.compression_enabled = true;
    config.compression_config = Some(CompressionConfig {
        algorithm: backbone_queue::compression::CompressionAlgorithm::Gzip,
        level: 6,
        min_size: 2048,
        force_compression: false,
        max_attempts: 3,
    });

    config.monitoring_enabled = true;
    config.alert_thresholds = Some(AlertThresholds {
        queue_depth_threshold: 500,
        error_rate_threshold: 5.0,
        latency_threshold_ms: 200,
        throughput_threshold_min: 100.0,
        max_queue_age_minutes: 30,
    });

    // Production-ready settings
    config.visibility_timeout = 30;
    config.max_receive_count = 3;
    config.max_size = Some(256 * 1024); // SQS limit
    config.retention_seconds = Some(14 * 24 * 60 * 60); // 14 days

    let result = validator.validate_queue_config(&config).await;

    println!("Queue: {}", config.name);
    println!("Type: {}", config.queue_type);
    println!("Features: FIFO={}, Compression={}, Monitoring={}",
        config.fifo_enabled, config.compression_enabled, config.monitoring_enabled);
    println!("Valid: {}", result.is_valid);

    if result.is_valid {
        println!("🎉 Production-ready configuration!");
    } else {
        println!("❌ Configuration has issues to fix");
    }

    // Show detailed results
    if !result.errors.is_empty() {
        println!("\n🚫 Errors to fix:");
        for error in &result.errors {
            println!("  • {}: {}", error.code, error.message);
        }
    }

    if !result.warnings.is_empty() {
        println!("\n⚠️  Warnings:");
        for warning in &result.warnings {
            println!("  • {}: {}", warning.code, warning.message);
        }
    }

    if !result.recommendations.is_empty() {
        println!("\n💡 Optimizations:");
        for rec in &result.recommendations {
            println!("  • {}", rec);
        }
    }

    Ok(())
}

async fn demo_environment_specific_validation() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🌍 Environment-Specific Validation");
    println!("=================================");

    let environments = vec![
        (ValidationEnvironment::Development, "Development"),
        (ValidationEnvironment::Testing, "Testing"),
        (ValidationEnvironment::Staging, "Staging"),
        (ValidationEnvironment::Production, "Production"),
    ];

    for (env, env_name) in environments {
        println!("\n📋 {} Environment", env_name);
        println!("---------------------");

        let validator = ConfigValidator::new(env);

        let mut config = QueueConfig::new(
            format!("{}-queue", env_name.to_lowercase()),
            "redis".to_string(),
            "redis://localhost:6379/0".to_string(),
        );

        // Configure based on environment
        match env {
            ValidationEnvironment::Development => {
                config.visibility_timeout = 5;
                config.monitoring_enabled = false;
            }
            ValidationEnvironment::Testing => {
                config.visibility_timeout = 1;
                config.retention_seconds = Some(300); // 5 minutes
            }
            ValidationEnvironment::Staging => {
                config.visibility_timeout = 30;
                config.monitoring_enabled = true;
            }
            ValidationEnvironment::Production => {
                config.visibility_timeout = 60;
                config.monitoring_enabled = true;
                config.fifo_enabled = true;
            }
        }

        let result = validator.validate_queue_config(&config).await;

        println!("Queue: {}", config.name);
        println!("Valid: {}", result.is_valid);

        if !result.warnings.is_empty() {
            println!("Environment-specific warnings:");
            for warning in &result.warnings {
                println!("  • {}", warning.message);
            }
        }

        if result.is_valid {
            println!("✅ Configuration passes validation");
        } else {
            println!("❌ Configuration needs fixes");
        }
    }

    Ok(())
}

fn demonstrate_validation_error_examples() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🚫 Common Validation Errors");
    println!("========================");

    // Example error scenarios with explanations
    let error_examples = vec![
        ("empty_queue_name", "Queue name cannot be empty", "Provide a descriptive name"),
        ("invalid_url", "Connection URL is malformed", "Use proper URL format (redis://host:port)"),
        ("zero_visibility", "Visibility timeout cannot be zero", "Set to at least 1 second"),
        ("high_retry", "Max receive count too high", "Consider lower retry limits"),
        ("missing_fifo_config", "FIFO enabled but no config", "Provide FIFO configuration"),
        ("oversized_message", "Message exceeds size limit", "Compress or split large messages"),
    ];

    for (code, message, suggestion) in error_examples {
        println!("❌ {}: {}", code, message);
        println!("   💡 {}", suggestion);
    }

    Ok(())
}

fn show_validation_best_practices() {
    println!("\n💡 Configuration Validation Best Practices");
    println!("==================================");

    let best_practices = vec![
        "Always validate configuration before creating queues",
        "Use environment-specific validation rules",
        "Configure appropriate timeouts for your use case",
        "Set reasonable retry limits to prevent infinite loops",
        "Enable monitoring in production environments",
        "Use compression for large messages (>1KB)",
        "Configure FIFO when message order matters",
        "Set up alerts for production queues",
        "Test configurations in development first",
        "Document configuration decisions",
        "Use configuration validation in CI/CD pipelines",
        "Implement configuration rollback capabilities",
        "Monitor queue health metrics",
        "Set appropriate retention periods",
        "Configure dead letter queues for failed messages",
    ];

    for (i, practice) in best_practices.iter().enumerate() {
        println!("{}. {}", i + 1, practice);
    }
}