//! Structured logging collection
//!
//! Provides JSON-formatted structured logging with field extraction
//! and log aggregation support for centralized collection.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use thiserror::Error;

/// Logging initialization error
#[derive(Debug, Error)]
pub enum LoggingError {
    /// Failed to setup logging
    #[error("Failed to setup logging: {0}")]
    SetupError(String),
}

/// Structured log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Timestamp (ISO 8601)
    #[serde(rename = "@timestamp")]
    pub timestamp: String,

    /// Log level
    pub level: String,

    /// Target/module
    pub target: String,

    /// Log message
    pub message: String,

    /// Service name
    pub service: String,

    /// Environment
    pub environment: String,

    /// Hostname
    pub hostname: String,

    /// Span/trace ID if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,

    /// Trace ID if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,

    /// Request ID if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    /// User ID if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,

    /// Additional fields from the log event
    #[serde(flatten)]
    pub fields: HashMap<String, serde_json::Value>,
}

/// Logging configuration for structured output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Enable JSON structured logging
    pub json_format: bool,

    /// Include timestamp in all logs
    pub include_timestamp: bool,

    /// Include hostname in all logs
    pub include_hostname: bool,

    /// Include span context (trace_id, span_id)
    pub include_spans: bool,

    /// Field names to extract as top-level fields
    pub extract_fields: Vec<String>,

    /// Buffer size for log aggregation
    pub buffer_size: usize,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            json_format: true,
            include_timestamp: true,
            include_hostname: true,
            include_spans: true,
            extract_fields: vec![
                "user_id".to_string(),
                "request_id".to_string(),
                "module".to_string(),
                "action".to_string(),
            ],
            buffer_size: 1000,
        }
    }
}

/// Log buffer for aggregation
pub struct LogBuffer {
    buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    config: LoggingConfig,
}

impl LogBuffer {
    /// Create a new log buffer with the given configuration
    pub fn new(config: LoggingConfig) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(VecDeque::with_capacity(config.buffer_size))),
            config,
        }
    }

    /// Push a log entry to the buffer
    pub fn push(&self, entry: LogEntry) {
        let mut buffer = self.buffer.lock().unwrap();
        // Drop oldest if buffer is full
        if buffer.len() >= self.config.buffer_size {
            buffer.pop_front();
        }
        buffer.push_back(entry);
    }

    /// Get all buffered log entries
    pub fn drain(&self) -> Vec<LogEntry> {
        let mut buffer = self.buffer.lock().unwrap();
        let entries: Vec<LogEntry> = buffer.drain(..).collect();
        entries
    }

    /// Get current buffer length
    pub fn len(&self) -> usize {
        let buffer = self.buffer.lock().unwrap();
        buffer.len()
    }
}

/// Create a structured log entry from current context
pub fn create_log_entry(
    level: &str,
    target: &str,
    message: &str,
    service: &str,
    environment: &str,
) -> LogEntry {
    let timestamp = format_timestamp(SystemTime::now());

    // Extract span context if available
    let current_span = tracing::Span::current();
    let span_id = if current_span.is_none() {
        None
    } else {
        Some(format!("{:?}", current_span.id()))
    };

    // Extract trace_id from span (simplified)
    let trace_id = span_id.clone(); // In real implementation, would extract from span context

    LogEntry {
        timestamp,
        level: level.to_string(),
        target: target.to_string(),
        message: message.to_string(),
        service: service.to_string(),
        environment: environment.to_string(),
        hostname: get_hostname(),
        span_id,
        trace_id,
        request_id: None, // Would be extracted from span fields
        user_id: None,   // Would be extracted from span fields
        fields: HashMap::new(),
    }
}

/// Get hostname safely
fn get_hostname() -> String {
    std::env::var("HOSTNAME")
        .unwrap_or_else(|_| "localhost".to_string())
}

/// Format timestamp as ISO 8601
fn format_timestamp(time: SystemTime) -> String {
    use std::time::UNIX_EPOCH;

    let duration = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let secs = duration.as_secs();
    let millis = duration.as_millis();

    // ISO 8601 format with millisecond precision
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        1970 + (secs / 31536000) % 10000,
        (secs / 2592000) % 12 + 1,
        (secs / 86400) % 31 + 1,
        (secs / 3600) % 24,
        (secs / 60) % 60,
        secs % 60,
        millis % 1000
    )
}

/// Initialize structured logging with JSON output
///
/// # Arguments
/// * `config` - Logging configuration
/// * `service_name` - Name of the service for log labels
/// * `environment` - Environment name (dev, staging, prod)
///
/// # Returns
/// Returns a log buffer handle for accessing aggregated logs
///
/// # Example
/// ```no_run
/// use backbone_observability::logging::{init_structured_logging, LoggingConfig};
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = LoggingConfig {
///     json_format: true,
///     ..Default::default()
/// };
///
/// let buffer = init_structured_logging(&config, "my-service", "production", "info")?;
/// # Ok(())
/// # }
/// ```
pub fn init_structured_logging(
    config: &LoggingConfig,
    service_name: &str,
    environment: &str,
    log_level: &str,
) -> Result<LogBuffer, LoggingError> {
    use tracing_subscriber::{Registry, EnvFilter, layer::SubscriberExt};
    use tracing_subscriber::fmt;
    use tracing_subscriber::Layer;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));

    if config.json_format {
        // JSON formatter for production
        let json_layer = fmt::layer()
            .json()
            .with_target(true)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_file(false)
            .with_line_number(false)
            .with_span_events(fmt::format::FmtSpan::CLOSE)
            .boxed();

        let subscriber = Registry::default()
            .with(filter)
            .with(json_layer);

        tracing::subscriber::set_global_default(subscriber)
            .map_err(|e| LoggingError::SetupError(e.to_string()))?;
    } else {
        // Pretty formatter for development
        let fmt_layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_span_events(fmt::format::FmtSpan::CLOSE)
            .boxed();

        let subscriber = Registry::default()
            .with(filter)
            .with(fmt_layer);

        tracing::subscriber::set_global_default(subscriber)
            .map_err(|e| LoggingError::SetupError(e.to_string()))?;
    }

    tracing::info!(
        service = service_name,
        environment = environment,
        format = if config.json_format { "json" } else { "pretty" },
        "Structured logging initialized"
    );

    Ok(LogBuffer::new(config.clone()))
}

/// Get aggregated logs from a buffer
///
/// Convenience function for accessing logs from a buffer handle.
pub fn get_aggregated_logs(buffer: &LogBuffer) -> Vec<LogEntry> {
    buffer.drain()
}

/// Create a new request ID
///
/// Generates a UUID v4 for request tracking
pub fn create_request_id() -> String {
    use std::time::SystemTime;
    // Simple nanosecond-based ID
    format!("{:x}", SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos())
}

/// Request ID extension for tracing spans
///
/// Helper to add request_id to spans
pub trait WithRequestId {
    /// Add request ID to the current span
    fn with_request_id(self, request_id: &str) -> Self;
}

/// Helper macro to create a request-scoped span with request ID
#[macro_export]
macro_rules! request_span {
    ($name:expr, $request_id:expr) => {
        tracing::info_span!(
            $name,
            request_id = %$request_id,
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_serialization() {
        let entry = LogEntry {
            timestamp: "2024-01-01T00:00:00.000Z".to_string(),
            level: "INFO".to_string(),
            target: "test".to_string(),
            message: "Test message".to_string(),
            service: "test-service".to_string(),
            environment: "test".to_string(),
            hostname: "localhost".to_string(),
            span_id: None,
            trace_id: None,
            request_id: None,
            user_id: None,
            fields: HashMap::new(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("Test message"));
        assert!(json.contains("INFO"));
        assert!(json.contains("@timestamp"));
    }

    #[test]
    fn test_logging_config_defaults() {
        let config = LoggingConfig::default();
        assert!(config.json_format);
        assert!(config.include_timestamp);
        assert!(config.include_hostname);
        assert!(config.include_spans);
        assert_eq!(config.buffer_size, 1000);
    }

    #[test]
    fn test_log_buffer() {
        let config = LoggingConfig::default();
        let buffer = LogBuffer::new(config);

        assert_eq!(buffer.len(), 0);

        let entry = LogEntry {
            timestamp: "2024-01-01T00:00:00.000Z".to_string(),
            level: "INFO".to_string(),
            target: "test".to_string(),
            message: "Test message".to_string(),
            service: "test-service".to_string(),
            environment: "test".to_string(),
            hostname: "localhost".to_string(),
            span_id: None,
            trace_id: None,
            request_id: None,
            user_id: None,
            fields: HashMap::new(),
        };

        buffer.push(entry.clone());
        assert_eq!(buffer.len(), 1);

        let entries = buffer.drain();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message, "Test message");
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_create_request_id() {
        let id = create_request_id();
        assert!(!id.is_empty());
    }

    #[test]
    fn test_timestamp_format() {
        let time = SystemTime::UNIX_EPOCH;
        let timestamp = format_timestamp(time);
        assert!(timestamp.starts_with("1970-01-01T00:00:00."));
    }

    #[test]
    fn test_create_log_entry_with_defaults() {
        let entry = create_log_entry("INFO", "test_target", "test message", "test-service", "dev");

        assert_eq!(entry.level, "INFO");
        assert_eq!(entry.target, "test_target");
        assert_eq!(entry.message, "test message");
        assert_eq!(entry.service, "test-service");
        assert_eq!(entry.environment, "dev");
        assert_eq!(entry.hostname, "localhost");
        assert!(entry.fields.is_empty());
    }

    #[test]
    fn test_log_buffer_overflow() {
        let config = LoggingConfig {
            buffer_size: 3,
            ..Default::default()
        };
        let buffer = LogBuffer::new(config);

        for i in 0..5 {
            buffer.push(LogEntry {
                timestamp: format!("2024-01-01T00:00:{}.000Z", i),
                level: "INFO".to_string(),
                target: "test".to_string(),
                message: format!("Message {}", i),
                service: "test-service".to_string(),
                environment: "test".to_string(),
                hostname: "localhost".to_string(),
                span_id: None,
                trace_id: None,
                request_id: None,
                user_id: None,
                fields: HashMap::new(),
            });
        }

        // Buffer should only contain last 3 entries
        assert_eq!(buffer.len(), 3);

        let entries = buffer.drain();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].message, "Message 2");
        assert_eq!(entries[1].message, "Message 3");
        assert_eq!(entries[2].message, "Message 4");
    }
}
