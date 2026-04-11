//! Configuration management for Backbone Framework Observability

use serde::{Deserialize, Serialize};

/// Observability configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObservabilityConfig {
    /// Enable distributed tracing
    pub tracing_enabled: bool,

    /// Default log level (trace, debug, info, warn, error)
    pub log_level: String,

    /// OpenTelemetry endpoint (e.g., Jaeger, OTLP)
    pub otlp_endpoint: Option<String>,

    /// Service name for tracing
    pub service_name: String,

    /// Structured logging configuration
    pub logging: LoggingConfig,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            tracing_enabled: true,
            log_level: "info".to_string(),
            otlp_endpoint: None,
            service_name: "backbone-app".to_string(),
            logging: LoggingConfig::default(),
        }
    }
}

/// Structured logging configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    /// Enable JSON structured logging
    pub json_format: bool,

    /// Include timestamp in all logs
    pub include_timestamp: bool,

    /// Include hostname in all logs
    pub include_hostname: bool,

    /// Include span context
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

/// Metrics configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,

    /// Metrics exporter type
    pub exporter: MetricsExporterType,

    /// Metrics export port (for Prometheus)
    pub port: u16,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            exporter: MetricsExporterType::Stdout,
            port: 9090,
        }
    }
}

/// Metrics exporter type
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricsExporterType {
    /// Export to stdout (logging)
    Stdout,

    /// Export via Prometheus endpoint
    Prometheus,
}
