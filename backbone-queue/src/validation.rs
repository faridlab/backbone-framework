//! Configuration Validation and Error Handling Module
//!
//! Provides comprehensive validation for queue configurations and improved
//! error handling with detailed error messages and recovery suggestions.

use std::collections::HashMap;
use chrono::{DateTime, Utc, Duration};
use serde::{Serialize, Deserialize};
use thiserror::Error;

use crate::{
    QueueConfig, QueueError, compression::CompressionConfig, fifo::FifoQueueConfig,
    monitoring::AlertThresholds,
};

/// Validation result with detailed information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Overall validity
    pub is_valid: bool,

    /// Validation errors
    pub errors: Vec<ValidationError>,

    /// Validation warnings
    pub warnings: Vec<ValidationWarning>,

    /// Recommendations for improvement
    pub recommendations: Vec<String>,

    /// Validation timestamp
    pub validated_at: DateTime<Utc>,
}

impl ValidationResult {
    /// Create successful validation result
    pub fn success() -> Self {
        Self {
            is_valid: true,
            errors: vec![],
            warnings: vec![],
            recommendations: vec![],
            validated_at: Utc::now(),
        }
    }

    /// Create validation result with errors
    pub fn failure(errors: Vec<ValidationError>) -> Self {
        Self {
            is_valid: false,
            errors,
            warnings: vec![],
            recommendations: vec![],
            validated_at: Utc::now(),
        }
    }

    /// Add warning to result
    pub fn with_warning(mut self, warning: ValidationWarning) -> Self {
        self.warnings.push(warning);
        self
    }

    /// Add recommendation to result
    pub fn with_recommendation(mut self, recommendation: String) -> Self {
        self.recommendations.push(recommendation);
        self
    }

    /// Get all messages (errors and warnings)
    pub fn all_messages(&self) -> Vec<String> {
        let mut messages = Vec::new();

        for error in &self.errors {
            messages.push(format!("ERROR: {}", error.message));
        }

        for warning in &self.warnings {
            messages.push(format!("WARNING: {}", warning.message));
        }

        messages
    }
}

/// Validation error with detailed information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// Error code
    pub code: String,

    /// Human-readable message
    pub message: String,

    /// Field that caused the error
    pub field: Option<String>,

    /// Current invalid value
    pub current_value: Option<String>,

    /// Suggested fix
    pub suggested_fix: Option<String>,

    /// Error severity
    pub severity: ValidationSeverity,
}

impl ValidationError {
    /// Create new validation error
    pub fn new(
        code: &str,
        message: &str,
        field: Option<&str>,
        current_value: Option<&str>,
        suggested_fix: Option<&str>,
        severity: ValidationSeverity,
    ) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            field: field.map(|s| s.to_string()),
            current_value: current_value.map(|s| s.to_string()),
            suggested_fix: suggested_fix.map(|s| s.to_string()),
            severity,
        }
    }

    /// Create critical error
    pub fn critical(
        code: &str,
        message: &str,
        field: Option<&str>,
        current_value: Option<&str>,
        suggested_fix: Option<&str>,
    ) -> Self {
        Self::new(code, message, field, current_value, suggested_fix, ValidationSeverity::Critical)
    }

    /// Create error with default severity
    pub fn error(
        code: &str,
        message: &str,
        field: Option<&str>,
        current_value: Option<&str>,
        suggested_fix: Option<&str>,
    ) -> Self {
        Self::new(code, message, field, current_value, suggested_fix, ValidationSeverity::Error)
    }
}

/// Validation warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    /// Warning code
    pub code: String,

    /// Warning message
    pub message: String,

    /// Field that caused the warning
    pub field: Option<String>,

    /// Current value
    pub current_value: Option<String>,

    /// Recommendation
    pub recommendation: Option<String>,
}

impl ValidationWarning {
    /// Create new validation warning
    pub fn new(
        code: &str,
        message: &str,
        field: Option<&str>,
        current_value: Option<&str>,
        recommendation: Option<&str>,
    ) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            field: field.map(|s| s.to_string()),
            current_value: current_value.map(|s| s.to_string()),
            recommendation: recommendation.map(|s| s.to_string()),
        }
    }
}

/// Validation severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationSeverity {
    /// Critical - configuration cannot be used
    Critical,

    /// Error - configuration has serious issues
    Error,

    /// Warning - configuration may have suboptimal settings
    Warning,

    /// Info - informational message
    Info,
}

/// Simple validation result for internal use
#[derive(Debug, Clone)]
struct SimpleValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Configuration validator
pub struct ConfigValidator {
    /// Environment context
    environment: ValidationEnvironment,

    /// Validation rules
    rules: HashMap<String, Vec<ValidationRule>>,
}

impl ConfigValidator {
    /// Create new validator
    pub fn new(environment: ValidationEnvironment) -> Self {
        let mut validator = Self {
            environment,
            rules: HashMap::new(),
        };

        validator.register_default_rules();
        validator
    }

    /// Validate queue configuration
    pub async fn validate_queue_config(&self, config: &QueueConfig) -> ValidationResult {
        let mut result = ValidationResult::success();
        let mut errors = Vec::new();

        // Basic field validation
        errors.extend(self.validate_basic_fields(config));

        // Queue type specific validation
        match config.queue_type.as_str() {
            "redis" => errors.extend(self.validate_redis_config(config)),
            "sqs" => errors.extend(self.validate_sqs_config(config)),
            _ => errors.push(ValidationError::critical(
                "INVALID_QUEUE_TYPE",
                &format!("Unsupported queue type: {}", config.queue_type),
                Some("queue_type"),
                Some(&config.queue_type),
                Some("Use 'redis' or 'sqs'"),
            )),
        }

        // FIFO configuration validation
        if config.fifo_enabled {
            if let Some(ref fifo_config) = config.fifo_config {
                let fifo_validation = self.validate_fifo_config(fifo_config);
                for error_msg in fifo_validation.errors {
                    errors.push(ValidationError::error(
                        "FIFO_CONFIG_ERROR",
                        &error_msg,
                        Some("fifo_config"),
                        None,
                        Some("Fix FIFO configuration"),
                    ));
                }
                for warning_msg in fifo_validation.warnings {
                    result.warnings.push(ValidationWarning::new(
                        "FIFO_CONFIG_WARNING",
                        &warning_msg,
                        Some("fifo_config"),
                        None,
                        Some("Review FIFO configuration"),
                    ));
                }
            } else {
                errors.push(ValidationError::error(
                    "FIFO_ENABLED_NO_CONFIG",
                    "FIFO is enabled but no configuration provided",
                    Some("fifo_config"),
                    None,
                    Some("Provide FifoQueueConfig or disable FIFO"),
                ));
            }
        }

        // Compression configuration validation
        if config.compression_enabled {
            if let Some(ref compression_config) = config.compression_config {
                let compression_validation = self.validate_compression_config(compression_config);
                for error_msg in compression_validation.errors {
                    errors.push(ValidationError::error(
                        "COMPRESSION_CONFIG_ERROR",
                        &error_msg,
                        Some("compression_config"),
                        None,
                        Some("Fix compression configuration"),
                    ));
                }
                for warning_msg in compression_validation.warnings {
                    result.warnings.push(ValidationWarning::new(
                        "COMPRESSION_CONFIG_WARNING",
                        &warning_msg,
                        Some("compression_config"),
                        None,
                        Some("Review compression configuration"),
                    ));
                }
            } else {
                result.warnings.push(ValidationWarning::new(
                    "COMPRESSION_ENABLED_NO_CONFIG",
                    "Compression is enabled but no configuration provided",
                    Some("compression_config"),
                    None,
                    Some("Provide CompressionConfig or use default settings"),
                ));
            }
        }

        // Monitoring configuration validation
        if config.monitoring_enabled {
            if let Some(ref thresholds) = config.alert_thresholds {
                let monitoring_validation = self.validate_monitoring_config(thresholds);
                for error_msg in monitoring_validation.errors {
                    errors.push(ValidationError::error(
                        "MONITORING_CONFIG_ERROR",
                        &error_msg,
                        Some("alert_thresholds"),
                        None,
                        Some("Fix monitoring configuration"),
                    ));
                }
                for warning_msg in monitoring_validation.warnings {
                    result.warnings.push(ValidationWarning::new(
                        "MONITORING_CONFIG_WARNING",
                        &warning_msg,
                        Some("alert_thresholds"),
                        None,
                        Some("Review monitoring configuration"),
                    ));
                }
            } else {
                result.warnings.push(ValidationWarning::new(
                    "MONITORING_ENABLED_NO_THRESHOLDS",
                    "Monitoring is enabled but no alert thresholds configured",
                    Some("alert_thresholds"),
                    None,
                    Some("Configure AlertThresholds for better monitoring"),
                ));
            }
        }

        // Environment-specific validation
        errors.extend(self.validate_environment_constraints(config, &mut result));

        if !errors.is_empty() {
            result = ValidationResult::failure(errors);
        }

        // Add recommendations based on configuration
        self.add_recommendations(config, &mut result);

        result.validated_at = Utc::now();
        result
    }

    /// Validate basic required fields
    fn validate_basic_fields(&self, config: &QueueConfig) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        // Queue name validation
        if config.name.is_empty() {
            errors.push(ValidationError::critical(
                "EMPTY_QUEUE_NAME",
                "Queue name cannot be empty",
                Some("name"),
                Some(""),
                Some("Provide a descriptive queue name"),
            ));
        } else if config.name.len() > 255 {
            errors.push(ValidationError::error(
                "QUEUE_NAME_TOO_LONG",
                "Queue name exceeds maximum length",
                Some("name"),
                Some(&config.name.len().to_string()),
                Some("Use a name shorter than 255 characters"),
            ));
        } else if !config.name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
            errors.push(ValidationError::error(
                "INVALID_QUEUE_NAME_CHARS",
                "Queue name contains invalid characters",
                Some("name"),
                Some(&config.name),
                Some("Use only alphanumeric characters, hyphens, underscores, and dots"),
            ));
        }

        // Connection URL validation
        if config.connection_url.is_empty() {
            errors.push(ValidationError::critical(
                "EMPTY_CONNECTION_URL",
                "Connection URL cannot be empty",
                Some("connection_url"),
                Some(""),
                Some("Provide a valid connection URL"),
            ));
        }

        // Visibility timeout validation
        if config.visibility_timeout == 0 {
            errors.push(ValidationError::error(
                "INVALID_VISIBILITY_TIMEOUT",
                "Visibility timeout must be greater than 0",
                Some("visibility_timeout"),
                Some(&config.visibility_timeout.to_string()),
                Some("Set visibility timeout to at least 1 second"),
            ));
        } else if config.visibility_timeout > 12 * 60 * 60 {
            errors.push(ValidationError::warning(
                "HIGH_VISIBILITY_TIMEOUT",
                "Visibility timeout is very high (12+ hours)",
                Some("visibility_timeout"),
                Some(&config.visibility_timeout.to_string()),
                Some("Consider reducing visibility timeout for better responsiveness"),
            ));
        }

        // Max receive count validation
        if config.max_receive_count == 0 {
            errors.push(ValidationError::error(
                "INVALID_MAX_RECEIVE_COUNT",
                "Max receive count must be greater than 0",
                Some("max_receive_count"),
                Some(&config.max_receive_count.to_string()),
                Some("Set max receive count to at least 1"),
            ));
        } else if config.max_receive_count > 100 {
            errors.push(ValidationError::warning(
                "HIGH_MAX_RECEIVE_COUNT",
                "Max receive count is very high (>100)",
                Some("max_receive_count"),
                Some(&config.max_receive_count.to_string()),
                Some("Consider reducing to avoid message processing loops"),
            ));
        }

        errors
    }

    /// Validate Redis-specific configuration
    fn validate_redis_config(&self, config: &QueueConfig) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        // Validate Redis connection URL format
        if !config.connection_url.starts_with("redis://") && !config.connection_url.starts_with("rediss://") {
            errors.push(ValidationError::error(
                "INVALID_REDIS_URL",
                "Redis connection URL must start with redis:// or rediss://",
                Some("connection_url"),
                Some(&config.connection_url),
                Some("Use format: redis://[password@]host:port/database"),
            ));
        }

        // Check for Redis-specific limitations
        if let Some(max_size) = config.max_size {
            if max_size > 1_000_000_000 {
                errors.push(ValidationError::warning(
                    "REDIS_MAX_SIZE_TOO_LARGE",
                    "Redis queue max size is very large",
                    Some("max_size"),
                    Some(&max_size.to_string()),
                    Some("Consider memory limitations and Redis eviction policies"),
                ));
            }
        }

        errors
    }

    /// Validate SQS-specific configuration
    fn validate_sqs_config(&self, config: &QueueConfig) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        // Validate SQS URL format
        if !config.connection_url.contains("amazonaws.com") || !config.connection_url.contains("sqs") {
            errors.push(ValidationError::error(
                "INVALID_SQS_URL",
                "SQS URL format appears invalid",
                Some("connection_url"),
                Some(&config.connection_url),
                Some("Use format: https://sqs.{region}.amazonaws.com/{account-id}/{queue-name}"),
            ));
        }

        // Check SQS-specific constraints
        if let Some(max_size) = config.max_size {
            if max_size > 256 * 1024 {
                errors.push(ValidationError::error(
                    "SQS_MAX_SIZE_EXCEEDED",
                    "SQS max message size is 256KB",
                    Some("max_size"),
                    Some(&max_size.to_string()),
                    Some("Reduce max_size to 262144 bytes (256KB) or less"),
                ));
            }
        }

        // Validate retention period for SQS
        if let Some(retention_seconds) = config.retention_seconds {
            if retention_seconds < 60 {
                errors.push(ValidationError::error(
                    "SQS_RETENTION_TOO_SHORT",
                    "SQS minimum retention period is 60 seconds",
                    Some("retention_seconds"),
                    Some(&retention_seconds.to_string()),
                    Some("Set retention_seconds to at least 60"),
                ));
            } else if retention_seconds > 14 * 24 * 60 * 60 {
                errors.push(ValidationError::error(
                    "SQS_RETENTION_TOO_LONG",
                    "SQS maximum retention period is 14 days",
                    Some("retention_seconds"),
                    Some(&retention_seconds.to_string()),
                    Some("Set retention_seconds to 1209600 (14 days) or less"),
                ));
            }
        }

        errors
    }

    /// Validate FIFO configuration
    fn validate_fifo_config(&self, config: &FifoQueueConfig) -> SimpleValidationResult {
        use crate::fifo::utils::validate_config;

        let errors = validate_config(config);

        SimpleValidationResult {
            is_valid: errors.is_empty(),
            errors,
            warnings: vec![],
        }
    }

    /// Validate compression configuration
    fn validate_compression_config(&self, config: &CompressionConfig) -> SimpleValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Validate compression level
        if config.level > 9 {
            errors.push("Compression level must be between 0 and 9".to_string());
        }

        // Validate minimum size
        if config.min_size == 0 {
            warnings.push("Minimum compression size is 0, may compress all messages".to_string());
        } else if config.min_size > 100_000 {
            warnings.push("Minimum compression size is very large".to_string());
        }

        SimpleValidationResult {
            is_valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    /// Validate monitoring configuration
    fn validate_monitoring_config(&self, thresholds: &AlertThresholds) -> SimpleValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Validate thresholds are reasonable
        if thresholds.queue_depth_threshold == 0 {
            errors.push("Queue depth threshold must be greater than 0".to_string());
        }

        if thresholds.error_rate_threshold < 0.0 || thresholds.error_rate_threshold > 100.0 {
            errors.push("Error rate threshold must be between 0 and 100".to_string());
        }

        if thresholds.latency_threshold_ms == 0 {
            warnings.push("Latency threshold is 0, may trigger frequent alerts".to_string());
        }

        SimpleValidationResult {
            is_valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    /// Validate environment-specific constraints
    fn validate_environment_constraints(&self, config: &QueueConfig, result: &mut ValidationResult) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        match self.environment {
            ValidationEnvironment::Production => {
                // Production-specific validations
                if config.visibility_timeout < 30 {
                    errors.push(ValidationError::warning(
                        "PROD_LOW_VISIBILITY_TIMEOUT",
                        "Production queues should have visibility timeout >= 30s",
                        Some("visibility_timeout"),
                        Some(&config.visibility_timeout.to_string()),
                        Some("Consider increasing visibility timeout for production reliability"),
                    ));
                }

                if !config.monitoring_enabled {
                    result.warnings.push(ValidationWarning::new(
                        "PROD_NO_MONITORING",
                        "Production queue should have monitoring enabled",
                        Some("monitoring_enabled"),
                        Some("false"),
                        Some("Enable monitoring for production observability"),
                    ));
                }

                if config.max_receive_count < 3 {
                    errors.push(ValidationError::warning(
                        "PROD_LOW_MAX_RECEIVE_COUNT",
                        "Production queues should have max_receive_count >= 3",
                        Some("max_receive_count"),
                        Some(&config.max_receive_count.to_string()),
                        Some("Consider increasing for better error handling"),
                    ));
                }
            }

            ValidationEnvironment::Development => {
                // Development-specific validations
                if config.visibility_timeout > 300 {
                    result.warnings.push(ValidationWarning::new(
                        "DEV_HIGH_VISIBILITY_TIMEOUT",
                        "Development queue has very high visibility timeout",
                        Some("visibility_timeout"),
                        Some(&config.visibility_timeout.to_string()),
                        Some("Consider reducing for faster development feedback"),
                    ));
                }
            }

            ValidationEnvironment::Testing => {
                // Testing-specific validations
                if config.retention_seconds.is_some() && config.retention_seconds.unwrap() > 3600 {
                    result.warnings.push(ValidationWarning::new(
                        "TEST_LONG_RETENTION",
                        "Test queue retention is very long",
                        Some("retention_seconds"),
                        Some(&config.retention_seconds.unwrap().to_string()),
                        Some("Consider reducing retention for test environments"),
                    ));
                }
            }
        }

        errors
    }

    /// Add recommendations based on configuration analysis
    fn add_recommendations(&self, config: &QueueConfig, result: &mut ValidationResult) {
        let mut recommendations = Vec::new();

        // Performance recommendations
        if config.visibility_timeout < 10 {
            recommendations.push("Consider increasing visibility timeout for better message processing reliability".to_string());
        }

        if config.max_receive_count == 1 {
            recommendations.push("Consider increasing max_receive_count for better error handling".to_string());
        }

        // Feature recommendations
        if !config.fifo_enabled && config.queue_type == "redis" {
            recommendations.push("Consider enabling FIFO for message ordering guarantees".to_string());
        }

        if !config.compression_enabled {
            recommendations.push("Consider enabling compression for large messages to reduce storage costs".to_string());
        }

        if !config.monitoring_enabled {
            recommendations.push("Enable monitoring for better observability and alerting".to_string());
        }

        // Security recommendations
        if !config.connection_url.contains("ssl") && config.queue_type == "redis" {
            recommendations.push("Consider using rediss:// (SSL) for secure Redis connections".to_string());
        }

        result.recommendations = recommendations;
    }

    /// Register default validation rules
    fn register_default_rules(&mut self) {
        // This can be extended to support custom validation rules
        self.rules.insert("basic".to_string(), vec![]);
        self.rules.insert("redis".to_string(), vec![]);
        self.rules.insert("sqs".to_string(), vec![]);
    }
}

/// Validation environment context
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationEnvironment {
    /// Production environment
    Production,

    /// Development environment
    Development,

    /// Testing environment
    Testing,

    /// Staging environment
    Staging,
}

impl ValidationEnvironment {
    /// Get environment from string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "prod" | "production" => ValidationEnvironment::Production,
            "dev" | "development" => ValidationEnvironment::Development,
            "test" | "testing" => ValidationEnvironment::Testing,
            "staging" | "stage" => ValidationEnvironment::Staging,
            _ => ValidationEnvironment::Development,
        }
    }
}

/// Validation rule for extensible validation
#[derive(Debug, Clone)]
pub struct ValidationRule {
    /// Rule name
    pub name: String,

    /// Validation function
    pub validator: fn(&QueueConfig) -> Vec<ValidationError>,
}

/// Enhanced error handling utilities
pub struct ErrorHandler;

impl ErrorHandler {
    /// Convert QueueError to user-friendly message
    pub fn format_queue_error(error: &QueueError) -> (String, Option<String>) {
        match error {
            QueueError::RedisConnection(msg) => (
                "Redis Connection Error".to_string(),
                Some(format!("Failed to connect to Redis: {}. Check your connection string and ensure Redis is running.", msg)),
            ),
            QueueError::RedisOperation(msg) => (
                "Redis Operation Error".to_string(),
                Some(format!("Redis operation failed: {}. Check Redis permissions and available memory.", msg)),
            ),
            QueueError::SqsError(msg) => (
                "AWS SQS Error".to_string(),
                Some(format!("SQS operation failed: {}. Check AWS credentials and permissions.", msg)),
            ),
            QueueError::Serialization(msg) => (
                "Message Serialization Error".to_string(),
                Some(format!("Failed to serialize message: {}. Check message format and data types.", msg)),
            ),
            QueueError::Deserialization(msg) => (
                "Message Deserialization Error".to_string(),
                Some(format!("Failed to deserialize message: {}. Check message format and encoding.", msg)),
            ),
            QueueError::MessageTooLarge { size, max } => (
                "Message Too Large".to_string(),
                Some(format!("Message size ({} bytes) exceeds maximum ({} bytes). Consider compressing large messages.", size, max)),
            ),
            QueueError::InvalidQueueName(msg) => (
                "Invalid Queue Name".to_string(),
                Some(format!("Queue name is invalid: {}. Use only alphanumeric characters, hyphens, and underscores.", msg)),
            ),
            QueueError::InvalidMessageId(msg) => (
                "Invalid Message ID".to_string(),
                Some(format!("Message ID is invalid: {}. Check message ID format and content.", msg)),
            ),
            QueueError::ConfigError(msg) => (
                "Configuration Error".to_string(),
                Some(format!("Configuration error: {}. Check your queue configuration.", msg)),
            ),
            QueueError::NetworkError(msg) => (
                "Network Error".to_string(),
                Some(format!("Network operation failed: {}. Check network connectivity and firewall settings.", msg)),
            ),
            QueueError::Other(msg) => (
                "Unknown Error".to_string(),
                Some(format!("An unexpected error occurred: {}. Contact support if this persists.", msg)),
            ),
            QueueError::QueueNotFound(msg) => (
                "Queue Not Found".to_string(),
                Some(format!("Queue not found: {}. Verify queue name and that it exists.", msg)),
            ),
            QueueError::NotFound(msg) => (
                "Resource Not Found".to_string(),
                Some(format!("Resource not found: {}. Check resource names and permissions.", msg)),
            ),
        }
    }

    /// Get recovery suggestions for error type
    pub fn get_recovery_suggestions(error: &QueueError) -> Vec<String> {
        match error {
            QueueError::RedisConnection(_) => vec![
                "Check Redis server is running".to_string(),
                "Verify connection string format (redis://host:port)".to_string(),
                "Check network connectivity to Redis server".to_string(),
                "Verify Redis authentication credentials".to_string(),
            ],
            QueueError::SqsError(_) => vec![
                "Verify AWS credentials are configured".to_string(),
                "Check AWS region and endpoint configuration".to_string(),
                "Verify SQS queue permissions".to_string(),
                "Check internet connectivity".to_string(),
            ],
            QueueError::MessageTooLarge { .. } => vec![
                "Enable message compression".to_string(),
                "Split large messages into smaller chunks".to_string(),
                "Use external storage for large data (S3, etc.)".to_string(),
            ],
            QueueError::Serialization(_) | QueueError::Deserialization(_) => vec![
                "Verify message data types are serializable".to_string(),
                "Check for circular references in message data".to_string(),
                "Ensure message structure matches expected schema".to_string(),
            ],
            QueueError::NetworkError(_) => vec![
                "Check internet connectivity".to_string(),
                "Verify firewall settings".to_string(),
                "Try operation again after a brief delay".to_string(),
            ],
            _ => vec![
                "Check logs for detailed error information".to_string(),
                "Verify configuration is correct".to_string(),
                "Contact support if issue persists".to_string(),
            ],
        }
    }

    /// Check if error is retryable
    pub fn is_retryable(error: &QueueError) -> bool {
        match error {
            QueueError::RedisConnection(_) | QueueError::NetworkError(_) => true,
            QueueError::RedisOperation(_) | QueueError::SqsError(_) => {
                // Some SQS/Redis errors are retryable, but need to check specific error codes
                true
            },
            QueueError::MessageTooLarge { .. } | QueueError::Serialization(_) |
            QueueError::Deserialization(_) | QueueError::ConfigError(_) => false,
            _ => true,
        }
    }

    /// Get recommended retry delay
    pub fn get_retry_delay(error: &QueueError, attempt: u32) -> Duration {
        let base_delay = match error {
            QueueError::RedisConnection(_) | QueueError::NetworkError(_) => Duration::seconds(5),
            QueueError::SqsError(_) => Duration::seconds(1),
            _ => Duration::seconds(2),
        };

        // Simple exponential backoff without jitter for now
        let multiplier = 2_u32.pow(attempt.min(6));
        let delay_seconds = base_delay.num_seconds() * multiplier as i64;
        Duration::seconds(delay_seconds)
    }
}

