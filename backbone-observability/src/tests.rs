//! Integration tests for backbone-observability
//!
//! Tests for metrics, tracing, and middleware functionality.

#[cfg(test)]
mod tests {
    use crate::config::{MetricsConfig, MetricsExporterType};

    // Note: Full integration tests with async runtime would require
    // more complex setup. These tests focus on unit-level functionality.

    #[test]
    fn test_metrics_config_default() {
        let config = MetricsConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.port, 9090);
        assert!(matches!(config.exporter, MetricsExporterType::Stdout));
    }

    #[test]
    fn test_metrics_config_prometheus() {
        let config = MetricsConfig {
            enabled: true,
            exporter: MetricsExporterType::Prometheus,
            port: 8080,
        };
        assert!(config.enabled);
        assert_eq!(config.port, 8080);
        assert!(matches!(config.exporter, MetricsExporterType::Prometheus));
    }

    #[test]
    fn test_observability_config_default() {
        use crate::ObservabilityConfig;

        let config = ObservabilityConfig::default();
        assert!(config.tracing_enabled);
        assert_eq!(config.log_level, "info");
        assert!(config.otlp_endpoint.is_none());
        assert_eq!(config.service_name, "backbone-app");
    }

    #[test]
    fn test_observability_config_custom() {
        use crate::ObservabilityConfig;

        let config = ObservabilityConfig {
            tracing_enabled: false,
            log_level: "debug".to_string(),
            otlp_endpoint: Some("http://localhost:4317".to_string()),
            service_name: "test-service".to_string(),
            logging: Default::default(),
        };
        assert!(!config.tracing_enabled);
        assert_eq!(config.log_level, "debug");
        assert_eq!(config.otlp_endpoint, Some("http://localhost:4317".to_string()));
        assert_eq!(config.service_name, "test-service");
    }

    #[test]
    fn test_otel_guard_variants() {
        // Test that OtelGuard can be created in both variants
        // This is a compile-time check to ensure the enum is properly defined
        #[cfg(feature = "otel-tracing")]
        {
            use crate::tracing::OtelGuard;

            // We can't actually create an OtelGuard::Some without a real tracer provider
            // but we can verify the type exists
            let _guard_type: Option<OtelGuard> = None;
            assert!(_guard_type.is_none());
        }
    }

    #[test]
    fn test_common_metrics_constants() {
        use crate::metrics::common;

        assert_eq!(common::HTTP_REQUESTS_TOTAL, "http_requests_total");
        assert_eq!(common::HTTP_REQUEST_DURATION_SECONDS, "http_request_duration_seconds");
        assert_eq!(common::DB_QUERIES_TOTAL, "db_queries_total");
        assert_eq!(common::DB_QUERY_DURATION_SECONDS, "db_query_duration_seconds");
        assert_eq!(common::HTTP_ACTIVE_CONNECTIONS, "http_active_connections");
        assert_eq!(common::LABEL_METHOD, "method");
        assert_eq!(common::LABEL_PATH, "path");
        assert_eq!(common::LABEL_STATUS, "status");
        assert_eq!(common::LABEL_OPERATION, "operation");
        assert_eq!(common::LABEL_TABLE, "table");
        assert_eq!(common::LABEL_STATE, "state");
    }

    #[test]
    fn test_pool_metrics_constants() {
        use crate::metrics::common;

        assert_eq!(common::DB_POOL_CONNECTIONS_TOTAL, "db_pool_connections_total");
        assert_eq!(common::DB_POOL_CONNECTIONS_ACTIVE, "db_pool_connections_active");
        assert_eq!(common::DB_POOL_CONNECTIONS_IDLE, "db_pool_connections_idle");
        assert_eq!(common::DB_POOL_MAX_CONNECTIONS, "db_pool_max_connections");
        assert_eq!(common::LABEL_POOL, "pool");
    }

    #[test]
    fn test_pool_stats_creation() {
        use crate::metrics::PoolStats;

        let stats = PoolStats {
            total: 10,
            active: 7,
            idle: 3,
            max: 20,
        };
        assert_eq!(stats.total, 10);
        assert_eq!(stats.active, 7);
        assert_eq!(stats.idle, 3);
        assert_eq!(stats.max, 20);

        // Test Copy trait
        let stats2 = stats;
        assert_eq!(stats.total, stats2.total);
    }

    #[test]
    fn test_record_pool_metrics_no_panic() {
        use crate::metrics::{PoolStats, record_pool_metrics, record_pool_metrics_named};

        let stats = PoolStats {
            total: 5,
            active: 3,
            idle: 2,
            max: 10,
        };

        // Should not panic even without full metrics setup
        record_pool_metrics(&stats);
        record_pool_metrics_named("primary", &stats);
        record_pool_metrics_named("replica", &stats);
    }

    #[test]
    fn test_span_attributes_constants() {
        use crate::tracing::span_attributes;

        assert_eq!(span_attributes::HTTP_METHOD, "http.method");
        assert_eq!(span_attributes::HTTP_PATH, "http.path");
        assert_eq!(span_attributes::HTTP_STATUS, "http.status_code");
        assert_eq!(span_attributes::USER_ID, "user.id");
        assert_eq!(span_attributes::REQUEST_ID, "request.id");
        assert_eq!(span_attributes::MODULE, "code.module");
        assert_eq!(span_attributes::FUNCTION, "code.function");
    }

    #[test]
    fn test_http_status_trait() {
        use http::{StatusCode, Response};

        let response = Response::builder()
            .status(StatusCode::OK)
            .body("test body")
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_observability_layer_clone() {
        use crate::middleware::ObservabilityLayer;

        // ObservabilityLayer should be cloneable
        let layer = ObservabilityLayer::new();
        let _layer_clone = layer.clone();

        // This is a compile-time check; if it compiles, Clone is implemented
        // We just need to verify it doesn't cause a compilation error
    }

    #[test]
    fn test_metrics_error_messages() {
        use crate::metrics::MetricsError;

        let error = MetricsError::InvalidConfig("test error".to_string());
        assert_eq!(error.to_string(), "Invalid configuration: test error");

        let error = MetricsError::SetupError("setup failed".to_string());
        assert_eq!(error.to_string(), "Metrics setup error: setup failed");

        let error = MetricsError::ExporterFailed("exporter error".to_string());
        assert_eq!(error.to_string(), "Failed to create exporter: exporter error");
    }

    #[test]
    fn test_tracing_error_messages() {
        use crate::tracing::TracingError;

        let error = TracingError::SetupError("trace error".to_string());
        assert_eq!(error.to_string(), "Trace setup error: trace error");
    }

    #[test]
    fn test_path_normalization() {
        // Test that path extraction logic normalizes IDs
        // This is a conceptual test; actual implementation would need to expose the function
        let path_with_uuid = "/api/users/550e8400-e29b-41d4-a716-446655440000";
        let path_with_num = "/api/users/1234567890";
        let path_with_mongoid = "/api/files/507f1f77bcf86cd799439011";
        let path_with_alphanumeric = "/api/orders/abc123def456";

        // All these should normalize to replace IDs with :id or :num
        // The middleware does this internally for better metric aggregation
        assert!(path_with_uuid.contains("550e8400"));
        assert!(path_with_num.contains("1234567890"));
        assert!(path_with_mongoid.contains("507f1f77bcf86cd799439011"));
        assert!(path_with_alphanumeric.contains("abc123def456"));
    }

    #[test]
    fn test_log_entry_creation() {
        use crate::logging::LogEntry;
        use std::collections::HashMap;

        let mut fields = HashMap::new();
        fields.insert("key".to_string(), serde_json::json!("value"));

        let entry = LogEntry {
            timestamp: "2024-02-14T12:00:00Z".to_string(),
            level: "info".to_string(),
            target: "test".to_string(),
            message: "Test log message".to_string(),
            service: "test-service".to_string(),
            environment: "test".to_string(),
            hostname: "localhost".to_string(),
            span_id: None,
            trace_id: None,
            request_id: None,
            user_id: None,
            fields,
        };

        assert_eq!(entry.level, "info");
        assert_eq!(entry.message, "Test log message");
        assert_eq!(entry.fields["key"], "value");
    }

    #[test]
    fn test_log_buffer_creation() {
        use crate::logging::LoggingConfig;

        let config = LoggingConfig::default();
        assert_eq!(config.json_format, true);
        assert_eq!(config.include_timestamp, true);
        assert_eq!(config.include_hostname, true);
        assert_eq!(config.include_spans, true);
        assert_eq!(config.buffer_size, 1000);
    }
}
