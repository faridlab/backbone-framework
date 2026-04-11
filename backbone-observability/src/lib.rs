//! Backbone Framework Observability
//!
//! Tracing, logging, and metrics setup for Backbone applications.

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod config;
pub mod logging;
pub mod metrics;
pub mod middleware;
pub mod tracing;

#[cfg(test)]
mod tests;

// Re-exports for convenience
pub use config::{MetricsConfig, MetricsExporterType, ObservabilityConfig};
pub use config::LoggingConfig as StructuredLoggingConfig;
pub use logging::*;
pub use metrics::*;
pub use middleware::{ObservabilityLayer, ObservabilityMiddleware};

/// Initialize structured logging with JSON output
///
/// # Arguments
/// * `config` - Logging configuration
/// * `service_name` - Name of the service for log labels
/// * `environment` - Environment name (dev, staging, prod)
/// * `log_level` - Log level (trace, debug, info, warn, error)
///
/// # Returns
/// Returns a log buffer handle for accessing aggregated logs
pub fn init_structured_logging(
    config: &StructuredLoggingConfig,
    service_name: &str,
    environment: &str,
    log_level: &str,
) -> Result<LogBuffer, anyhow::Error> {
    // Import the logging LogBuffer and LoggingConfig explicitly
    use crate::logging::{LogBuffer, LoggingConfig};

    // Convert the aliased config to the logging module's config
    let logging_config = LoggingConfig {
        json_format: config.json_format,
        include_timestamp: config.include_timestamp,
        include_hostname: config.include_hostname,
        include_spans: config.include_spans,
        extract_fields: config.extract_fields.clone(),
        buffer_size: config.buffer_size,
    };

    let mut buffer_config = logging_config.clone();
    buffer_config.extract_fields.push("service".to_string());
    buffer_config.extract_fields.push("environment".to_string());

    let buffer = LogBuffer::new(buffer_config);

    // Initialize the actual tracing subscriber with the provided log level
    crate::logging::init_structured_logging(
        &logging_config,
        service_name,
        environment,
        log_level,
    )?;

    Ok(buffer)
}

/// Get aggregated logs from a buffer
///
/// Convenience function for accessing logs from a buffer handle.
pub fn get_aggregated_logs(buffer: &LogBuffer) -> Vec<LogEntry> {
    buffer.drain()
}

// ---------------------------------------------------------------------------
// Unified observability initialization (fixes double-subscriber bug B-12)
// ---------------------------------------------------------------------------

/// Opaque guard that flushes & shuts down the OTel tracer provider on drop.
/// When `otel-tracing` is disabled this is a no-op.
pub enum OtelShutdownGuard {
    /// OTel tracing is active — provider will be shut down on drop.
    #[cfg(feature = "otel-tracing")]
    Active(opentelemetry_sdk::trace::TracerProvider),
    /// OTel tracing not active (feature disabled or no config).
    Inactive,
}

impl Drop for OtelShutdownGuard {
    fn drop(&mut self) {
        match self {
            #[cfg(feature = "otel-tracing")]
            Self::Active(_provider) => {
                ::tracing::debug!("OtelShutdownGuard dropped — tracer provider shutdown");
            }
            Self::Inactive => {}
        }
    }
}

/// Unified observability initialization.
///
/// Combines structured logging (fmt layer) and optional OpenTelemetry tracing
/// into a **single** global subscriber. This replaces the old pattern of calling
/// `init_structured_logging()` + `init_tracing_with_otel()` separately, which
/// caused a double-subscriber bug where the second `set_global_default` call
/// silently failed and the OTel layer was never active.
///
/// # Arguments
/// * `logging_config` — Structured logging settings (JSON/pretty, fields, buffer)
/// * `otel_config` — Optional OTel configuration. When `Some` and `tracing_enabled`,
///   an OTel layer is added (with optional OTLP exporter).
/// * `service_name` — Identifies the service in logs and traces.
/// * `environment` — e.g. "development", "production".
/// * `log_level` — Filter directive such as "info" or "backbone=debug".
pub fn init_observability(
    logging_config: &StructuredLoggingConfig,
    otel_config: Option<&ObservabilityConfig>,
    service_name: &str,
    environment: &str,
    log_level: &str,
) -> Result<(LogBuffer, OtelShutdownGuard), anyhow::Error> {
    use ::tracing_subscriber::{Registry, EnvFilter, layer::SubscriberExt};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));

    // Build OTel tracer + provider when the feature is enabled and config is active.
    // We prepare the tracer here so the OTel layer's generic `S` type parameter is
    // inferred at subscriber-composition time (avoids the Box<dyn Layer<Registry>> mismatch).
    #[cfg(feature = "otel-tracing")]
    let (otel_tracer, guard) = match otel_config {
        Some(config) if config.tracing_enabled => {
            let (tracer, provider) = crate::tracing::build_otel_tracer(config)?;
            (Some(tracer), OtelShutdownGuard::Active(provider))
        }
        _ => (None, OtelShutdownGuard::Inactive),
    };
    #[cfg(not(feature = "otel-tracing"))]
    let (otel_tracer, guard): (Option<()>, _) = { let _ = otel_config; (None, OtelShutdownGuard::Inactive) };

    // Compose subscriber: filter + fmt + optional OTel.
    // The OTel layer is created inside each branch so its generic `S` type
    // parameter is inferred from the specific fmt layer type in that branch.
    macro_rules! set_subscriber {
        ($filter:expr, $fmt:expr, $otel_tracer:expr) => {{
            #[cfg(feature = "otel-tracing")]
            {
                let otel = $otel_tracer
                    .map(|t| tracing_opentelemetry::layer().with_tracer(t));
                let subscriber = Registry::default().with($filter).with($fmt).with(otel);
                ::tracing::subscriber::set_global_default(subscriber)
                    .map_err(|e| anyhow::anyhow!("Failed to set global subscriber: {e}"))?;
            }
            #[cfg(not(feature = "otel-tracing"))]
            {
                let _ = $otel_tracer;
                let subscriber = Registry::default().with($filter).with($fmt);
                ::tracing::subscriber::set_global_default(subscriber)
                    .map_err(|e| anyhow::anyhow!("Failed to set global subscriber: {e}"))?;
            }
        }};
    }

    if logging_config.json_format {
        let json = ::tracing_subscriber::fmt::layer()
            .json()
            .with_target(true)
            .with_thread_ids(false)
            .with_file(false)
            .with_line_number(false)
            .with_span_events(::tracing_subscriber::fmt::format::FmtSpan::CLOSE);
        set_subscriber!(filter, json, otel_tracer);
    } else {
        let pretty = ::tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_span_events(::tracing_subscriber::fmt::format::FmtSpan::CLOSE);
        set_subscriber!(filter, pretty, otel_tracer);
    }

    // Build log buffer
    let buffer_config = crate::logging::LoggingConfig {
        json_format: logging_config.json_format,
        include_timestamp: logging_config.include_timestamp,
        include_hostname: logging_config.include_hostname,
        include_spans: logging_config.include_spans,
        extract_fields: logging_config.extract_fields.clone(),
        buffer_size: logging_config.buffer_size,
    };
    let buffer = LogBuffer::new(buffer_config);

    ::tracing::info!(
        service = service_name,
        environment = environment,
        format = if logging_config.json_format { "json" } else { "pretty" },
        otel = if matches!(guard, OtelShutdownGuard::Inactive) { "disabled" } else { "enabled" },
        "Observability initialized (single subscriber)"
    );

    Ok((buffer, guard))
}
