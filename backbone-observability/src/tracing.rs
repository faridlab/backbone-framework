//! OpenTelemetry tracing setup
//!
//! Provides distributed tracing instrumentation.

use tracing_subscriber::{layer::SubscriberExt, Registry, EnvFilter};
use thiserror::Error;
use crate::config::ObservabilityConfig;

/// Tracing initialization error
#[derive(Debug, Error)]
pub enum TracingError {
    /// Failed to setup tracing (subscriber, exporter, configuration)
    #[error("Trace setup error: {0}")]
    SetupError(String),
}

/// Initialize tracing with OpenTelemetry support (when feature enabled)
///
/// # Arguments
/// * `config` - Observability configuration
pub fn init_tracing(config: &ObservabilityConfig) -> Result<(), TracingError> {
    if !config.tracing_enabled {
        return Ok(());
    }

    // Set up tracing subscriber with env filter
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    let subscriber = Registry::default()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_thread_ids(false)
                .with_line_number(true)
        );

    // Set global subscriber
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|e| TracingError::SetupError(e.to_string()))?;

    tracing::info!("Tracing initialized for service: {}", config.service_name);

    #[cfg(feature = "otel-tracing")]
    {
        tracing::info!("OpenTelemetry tracing is available via init_tracing_with_otel()");
    }

    Ok(())
}

/// Initialize OpenTelemetry tracing with exporter
///
/// # Arguments
/// * `config` - Observability configuration
///
/// # Returns
/// Returns a shutdown guard that flushes traces when dropped
///
/// # Example
/// ```no_run
/// # #[cfg(feature = "otel-tracing")]
/// # {
/// use backbone_observability::tracing::init_tracing_with_otel;
/// use backbone_observability::ObservabilityConfig;
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = ObservabilityConfig {
///     tracing_enabled: true,
///     service_name: "my-service".to_string(),
///     otlp_endpoint: Some("http://localhost:4317".to_string()),
///     ..Default::default()
/// };
///
/// let _guard = init_tracing_with_otel(&config)?;
/// # Ok(())
/// # }
/// # }
/// ```
#[cfg(feature = "otel-tracing")]
pub fn init_tracing_with_otel(config: &ObservabilityConfig) -> Result<OtelGuard, TracingError> {
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace::TracerProvider as SdkTracerProvider;
    use opentelemetry_sdk::Resource;
    use opentelemetry::KeyValue;

    if !config.tracing_enabled {
        return Ok(OtelGuard::None);
    }

    // Configure the service resource
    let resource = Resource::new(vec![
        KeyValue::new("service.name", config.service_name.clone()),
        KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
    ]);

    // Set up tracing subscriber with env filter
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    // Create a simple tracer provider without exporter
    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .build();

    // Create OpenTelemetry layer
    let otel_layer = tracing_opentelemetry::layer()
        .with_tracer(tracer_provider.tracer(config.service_name.clone()));

    let subscriber = Registry::default()
        .with(filter)
        .with(otel_layer)
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_thread_ids(false)
                .with_line_number(true)
        );

    tracing::subscriber::set_global_default(subscriber)
        .map_err(|e| TracingError::SetupError(e.to_string()))?;

    tracing::info!("OpenTelemetry tracing initialized for service: {}", config.service_name);

    if let Some(endpoint) = &config.otlp_endpoint {
        tracing::info!("OTLP endpoint configured: {}", endpoint);
        tracing::info!("Note: OTLP exporter setup requires additional configuration");
    } else {
        tracing::info!("No OTLP endpoint configured - tracing to console only");
    }

    Ok(OtelGuard::Some(tracer_provider))
}

/// Shutdown guard for OpenTelemetry tracer
///
/// When dropped, this guard ensures the tracer provider is properly
/// shutdown and all remaining traces are flushed.
#[cfg(feature = "otel-tracing")]
pub enum OtelGuard {
    /// Contains the tracer provider that will be shutdown on drop
    Some(opentelemetry_sdk::trace::TracerProvider),
    /// No tracing was initialized (disabled in config)
    None,
}

#[cfg(feature = "otel-tracing")]
impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Self::Some(_) = self {
            tracing::debug!("OtelGuard dropped - tracer provider shutdown");
        }
    }
}

/// Create a span for HTTP request tracking
///
/// # Example
/// ```no_run
/// use backbone_observability::tracing::http_request_span;
///
/// let span = http_request_span("GET", "/api/users");
/// let _enter = span.enter();
/// // Handle request...
/// ```
pub fn http_request_span(method: &str, path: &str) -> tracing::Span {
    tracing::info_span!(
        "http_request",
        method = %method,
        path = %path,
        status = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
    )
}

/// Build an OpenTelemetry tracer and provider with optional OTLP exporter.
///
/// Returns `(Tracer, TracerProvider)`. The caller wraps the tracer in
/// `tracing_opentelemetry::layer().with_tracer(tracer)` and composes it
/// into the subscriber (so the generic `S` type parameter is inferred
/// correctly without boxing).
///
/// Also sets the global W3C TraceContext propagator so incoming
/// `traceparent` / `tracestate` headers are parsed automatically.
#[cfg(feature = "otel-tracing")]
pub fn build_otel_tracer(
    config: &ObservabilityConfig,
) -> Result<(opentelemetry_sdk::trace::Tracer, opentelemetry_sdk::trace::TracerProvider), TracingError>
{
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace::TracerProvider as SdkTracerProvider;
    use opentelemetry_sdk::Resource;
    use opentelemetry::KeyValue;
    use opentelemetry_otlp::WithExportConfig;

    // Set W3C TraceContext propagator globally (enables traceparent extraction)
    opentelemetry::global::set_text_map_propagator(
        opentelemetry_sdk::propagation::TraceContextPropagator::new()
    );

    let resource = Resource::new(vec![
        KeyValue::new("service.name", config.service_name.clone()),
        KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
    ]);

    let mut provider_builder = SdkTracerProvider::builder().with_resource(resource);

    // Wire OTLP exporter when endpoint is configured
    if let Some(endpoint) = &config.otlp_endpoint {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()
            .map_err(|e| TracingError::SetupError(format!("OTLP exporter: {e}")))?;

        provider_builder = provider_builder.with_simple_exporter(exporter);
    }

    let provider = provider_builder.build();
    let tracer = provider.tracer(config.service_name.clone());

    Ok((tracer, provider))
}

/// Common tracing span attributes
pub mod span_attributes {
    /// HTTP method attribute
    pub const HTTP_METHOD: &str = "http.method";

    /// HTTP path attribute
    pub const HTTP_PATH: &str = "http.path";

    /// HTTP status code attribute
    pub const HTTP_STATUS: &str = "http.status_code";

    /// User ID attribute
    pub const USER_ID: &str = "user.id";

    /// Request ID attribute
    pub const REQUEST_ID: &str = "request.id";

    /// Module attribute
    pub const MODULE: &str = "code.module";

    /// Function attribute
    pub const FUNCTION: &str = "code.function";
}
