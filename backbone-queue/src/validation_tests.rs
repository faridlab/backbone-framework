//! Validation Module Tests

use std::time::Duration;
use tokio::test;

use crate::{
    validation::{
        ConfigValidator, ValidationResult, ValidationError, ValidationWarning,
        ValidationEnvironment, ErrorHandler,
    },
    QueueConfig,
    compression::CompressionConfig,
    monitoring::AlertThresholds,
};

#[test]
async fn test_valid_queue_config() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let result = validator.validate_queue_config(&config).await;
    assert!(result.is_valid);
    assert!(result.errors.is_empty());
}

#[test]
async fn test_invalid_queue_name() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let mut config = QueueConfig::new(
        "".to_string(), // Empty name
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let result = validator.validate_queue_config(&config).await;
    assert!(!result.is_valid);

    let empty_name_error = result.errors.iter().find(|e| e.code == "EMPTY_QUEUE_NAME");
    assert!(empty_name_error.is_some());
}

#[test]
async fn test_invalid_queue_name_characters() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let mut config = QueueConfig::new(
        "test@queue".to_string(), // Invalid character @
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let result = validator.validate_queue_config(&config).await;
    assert!(!result.is_valid);

    let invalid_chars_error = result.errors.iter().find(|e| e.code == "INVALID_QUEUE_NAME_CHARS");
    assert!(invalid_chars_error.is_some());
}

#[test]
async fn test_invalid_connection_url() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let mut config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "".to_string(), // Empty URL
    );

    let result = validator.validate_queue_config(&config).await;
    assert!(!result.is_valid);

    let empty_url_error = result.errors.iter().find(|e| e.code == "EMPTY_CONNECTION_URL");
    assert!(empty_url_error.is_some());
}

#[test]
async fn test_invalid_redis_url() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "http://localhost:6379".to_string(), // Invalid Redis URL
    );

    let result = validator.validate_queue_config(&config).await;
    assert!(!result.is_valid);

    let invalid_url_error = result.errors.iter().find(|e| e.code == "INVALID_REDIS_URL");
    assert!(invalid_url_error.is_some());
}

#[test]
async fn test_invalid_visibility_timeout() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let mut config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    config.visibility_timeout = 0; // Invalid

    let result = validator.validate_queue_config(&config).await;
    assert!(!result.is_valid);

    let invalid_timeout_error = result.errors.iter().find(|e| e.code == "INVALID_VISIBILITY_TIMEOUT");
    assert!(invalid_timeout_error.is_some());
}

#[test]
async fn test_invalid_max_receive_count() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let mut config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    config.max_receive_count = 0; // Invalid

    let result = validator.validate_queue_config(&config).await;
    assert!(!result.is_valid);

    let invalid_count_error = result.errors.iter().find(|e| e.code == "INVALID_MAX_RECEIVE_COUNT");
    assert!(invalid_count_error.is_some());
}

#[test]
async fn test_unsupported_queue_type() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let config = QueueConfig::new(
        "test-queue".to_string(),
        "unsupported".to_string(),
        "http://localhost:8080".to_string(),
    );

    let result = validator.validate_queue_config(&config).await;
    assert!(!result.is_valid);

    let invalid_type_error = result.errors.iter().find(|e| e.code == "INVALID_QUEUE_TYPE");
    assert!(invalid_type_error.is_some());
}

#[test]
async fn test_sqs_specific_validation() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let config = QueueConfig::new(
        "test-queue".to_string(),
        "sqs".to_string(),
        "http://invalid-sqs-url.com".to_string(),
    );

    let result = validator.validate_queue_config(&config).await;
    assert!(!result.is_valid);

    let invalid_sqs_error = result.errors.iter().find(|e| e.code == "INVALID_SQS_URL");
    assert!(invalid_sqs_error.is_some());
}

#[test]
async fn test_sqs_max_size_validation() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let mut config = QueueConfig::new(
        "test-queue".to_string(),
        "sqs".to_string(),
        "https://sqs.us-east-1.amazonaws.com/123456789012/test-queue".to_string(),
    );

    config.max_size = Some(512 * 1024); // 512KB > SQS limit

    let result = validator.validate_queue_config(&config).await;
    assert!(!result.is_valid);

    let max_size_error = result.errors.iter().find(|e| e.code == "SQS_MAX_SIZE_EXCEEDED");
    assert!(max_size_error.is_some());
}

#[test]
async fn test_fifo_enabled_no_config() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let mut config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    config.fifo_enabled = true;
    // No fifo_config provided

    let result = validator.validate_queue_config(&config).await;
    assert!(!result.is_valid);

    let fifo_config_error = result.errors.iter().find(|e| e.code == "FIFO_ENABLED_NO_CONFIG");
    assert!(fifo_config_error.is_some());
}

#[test]
async fn test_compression_enabled_no_config() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let mut config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    config.compression_enabled = true;
    // No compression_config provided - should only warn

    let result = validator.validate_queue_config(&config).await;
    assert!(result.is_valid); // Should be valid, just warn
    assert!(!result.warnings.is_empty());

    let compression_warning = result.warnings.iter().find(|w| w.code == "COMPRESSION_ENABLED_NO_CONFIG");
    assert!(compression_warning.is_some());
}

#[test]
async fn test_production_environment_validations() {
    let validator = ConfigValidator::new(ValidationEnvironment::Production);

    let mut config = QueueConfig::new(
        "prod-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    config.visibility_timeout = 10; // Too low for production
    config.monitoring_enabled = false; // Should have monitoring

    let result = validator.validate_queue_config(&config).await;

    // Should still be valid but with warnings
    assert!(result.is_valid);
    assert!(!result.warnings.is_empty());

    let visibility_warning = result.warnings.iter().find(|w| w.code == "PROD_LOW_VISIBILITY_TIMEOUT");
    assert!(visibility_warning.is_some());

    let monitoring_warning = result.warnings.iter().find(|w| w.code == "PROD_NO_MONITORING");
    assert!(monitoring_warning.is_some());
}

#[test]
async fn test_development_environment_validations() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let mut config = QueueConfig::new(
        "dev-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    config.visibility_timeout = 600; // High for development

    let result = validator.validate_queue_config(&config).await;

    // Should be valid but with warning
    assert!(result.is_valid);
    assert!(!result.warnings.is_empty());

    let high_visibility_warning = result.warnings.iter().find(|w| w.code == "DEV_HIGH_VISIBILITY_TIMEOUT");
    assert!(high_visibility_warning.is_some());
}

#[test]
async fn test_queue_name_too_long() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let long_name = "a".repeat(300); // > 255 characters
    let config = QueueConfig::new(
        long_name,
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let result = validator.validate_queue_config(&config).await;
    assert!(!result.is_valid);

    let too_long_error = result.errors.iter().find(|e| e.code == "QUEUE_NAME_TOO_LONG");
    assert!(too_long_error.is_some());
}

#[test]
async fn test_high_values_with_warnings() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let mut config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    config.visibility_timeout = 14 * 60 * 60; // 14 hours - very high
    config.max_receive_count = 150; // Very high

    let result = validator.validate_queue_config(&config).await;

    // Should be valid but with warnings
    assert!(result.is_valid);
    assert!(!result.warnings.is_empty());

    let high_visibility_warning = result.warnings.iter().find(|w| w.code == "HIGH_VISIBILITY_TIMEOUT");
    assert!(high_visibility_warning.is_some());

    let high_receive_warning = result.warnings.iter().find(|w| w.code == "HIGH_MAX_RECEIVE_COUNT");
    assert!(high_receive_warning.is_some());
}

#[test]
async fn test_validation_result_recommendations() {
    let validator = ConfigValidator::new(ValidationEnvironment::Development);

    let mut config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    config.max_receive_count = 1;
    config.fifo_enabled = false;
    config.compression_enabled = false;
    config.monitoring_enabled = false;

    let result = validator.validate_queue_config(&config).await;

    // Should have recommendations
    assert!(!result.recommendations.is_empty());

    // Check for expected recommendations
    let has_receive_rec = result.recommendations.iter().any(|r| r.contains("max_receive_count"));
    assert!(has_receive_rec);

    let has_fifo_rec = result.recommendations.iter().any(|r| r.contains("FIFO"));
    assert!(has_fifo_rec);

    let has_compression_rec = result.recommendations.iter().any(|r| r.contains("compression"));
    assert!(has_compression_rec);

    let has_monitoring_rec = result.recommendations.iter().any(|r| r.contains("monitoring"));
    assert!(has_monitoring_rec);
}

#[test]
fn test_error_handler_format_queue_error() {
    use crate::QueueError;

    let redis_error = QueueError::RedisConnection("Connection refused".to_string());
    let (title, detail) = ErrorHandler::format_queue_error(&redis_error);

    assert_eq!(title, "Redis Connection Error");
    assert!(detail.is_some());
    assert!(detail.unwrap().contains("Redis"));
}

#[test]
fn test_error_handler_recovery_suggestions() {
    use crate::QueueError;

    let redis_error = QueueError::RedisConnection("Connection refused".to_string());
    let suggestions = ErrorHandler::get_recovery_suggestions(&redis_error);

    assert!(!suggestions.is_empty());
    assert!(suggestions.iter().any(|s| s.contains("Redis")));
}

#[test]
fn test_error_handler_is_retryable() {
    use crate::QueueError;

    let network_error = QueueError::NetworkError("Timeout".to_string());
    assert!(ErrorHandler::is_retryable(&network_error));

    let config_error = QueueError::ConfigError("Invalid setting".to_string());
    assert!(!ErrorHandler::is_retryable(&config_error));
}

#[test]
fn test_error_handler_retry_delay() {
    use crate::QueueError;

    let network_error = QueueError::NetworkError("Timeout".to_string());
    let delay = ErrorHandler::get_retry_delay(&network_error, 2);

    // Should be greater than base delay
    assert!(delay > Duration::from_secs(5));
}

#[test]
fn test_validation_environment_from_str() {
    assert_eq!(
        ValidationEnvironment::from_str("production"),
        ValidationEnvironment::Production
    );
    assert_eq!(
        ValidationEnvironment::from_str("PROD"),
        ValidationEnvironment::Production
    );
    assert_eq!(
        ValidationEnvironment::from_str("dev"),
        ValidationEnvironment::Development
    );
    assert_eq!(
        ValidationEnvironment::from_str("test"),
        ValidationEnvironment::Testing
    );
    assert_eq!(
        ValidationEnvironment::from_str("staging"),
        ValidationEnvironment::Staging
    );
    assert_eq!(
        ValidationEnvironment::from_str("unknown"),
        ValidationEnvironment::Development
    );
}

#[test]
async fn test_validation_result_all_messages() {
    let mut result = ValidationResult::success();
    result.errors.push(ValidationError::error(
        "TEST_ERROR",
        "Test error message",
        Some("test_field"),
        Some("invalid_value"),
        Some("fix_value"),
    ));
    result.warnings.push(ValidationWarning::new(
        "TEST_WARNING",
        "Test warning message",
        Some("test_field"),
        Some("warning_value"),
        Some("warning_recommendation"),
    ));

    let messages = result.all_messages();
    assert_eq!(messages.len(), 2);
    assert!(messages[0].starts_with("ERROR:"));
    assert!(messages[1].starts_with("WARNING:"));
}

#[test]
async fn test_comprehensive_validation_flow() {
    let validator = ConfigValidator::new(ValidationEnvironment::Production);

    let mut config = QueueConfig::new(
        "critical-orders".to_string(),
        "redis".to_string(),
        "redis://localhost:6379/0".to_string(),
    );

    // Configure with various settings
    config.fifo_enabled = true;
    config.fifo_config = Some(crate::fifo::utils::get_recommended_config(crate::fifo::MessageVolume::Medium));
    config.compression_enabled = true;
    config.compression_config = Some(CompressionConfig::default());
    config.monitoring_enabled = true;
    config.alert_thresholds = Some(AlertThresholds::default());
    config.visibility_timeout = 60;
    config.max_receive_count = 5;

    let result = validator.validate_queue_config(&config).await;

    // Should be valid with proper configuration
    assert!(result.is_valid);
    assert!(result.errors.is_empty());

    // Should have some recommendations for optimization
    assert!(!result.recommendations.is_empty());
}

#[test]
fn test_validation_error_creation() {
    let error = ValidationError::critical(
        "CRITICAL_ERROR",
        "This is a critical error",
        Some("important_field"),
        Some("bad_value"),
        Some("good_value"),
    );

    assert_eq!(error.code, "CRITICAL_ERROR");
    assert_eq!(error.severity, crate::validation::ValidationSeverity::Critical);
    assert_eq!(error.field, Some("important_field".to_string()));
    assert_eq!(error.current_value, Some("bad_value".to_string()));
    assert_eq!(error.suggested_fix, Some("good_value".to_string()));
}

#[test]
fn test_validation_warning_creation() {
    let warning = ValidationWarning::new(
        "PERFORMANCE_WARNING",
        "This may affect performance",
        Some("config_field"),
        Some("current_value"),
        Some("consider_optimization"),
    );

    assert_eq!(warning.code, "PERFORMANCE_WARNING");
    assert_eq!(warning.field, Some("config_field".to_string()));
    assert_eq!(warning.current_value, Some("current_value".to_string()));
    assert_eq!(warning.recommendation, Some("consider_optimization".to_string()));
}