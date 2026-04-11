//! Metrics collection with Prometheus export
//!
//! Provides production-ready metrics collection with Prometheus HTTP endpoint.

use std::net::SocketAddr;
use std::sync::OnceLock;
use thiserror::Error;
use crate::config::MetricsConfig;
use tokio::sync::oneshot;

// Re-export http types used in middleware
pub use http::{Method, StatusCode, Response, Request};

// Prometheus registry (available when feature is enabled)
#[cfg(feature = "prometheus-metrics")]
pub use prometheus::Registry;

/// Metrics initialization error
#[derive(Debug, Error)]
pub enum MetricsError {
    /// Failed to create the metrics exporter
    #[error("Failed to create exporter: {0}")]
    ExporterFailed(String),

    /// Failed to setup metrics (registration, configuration)
    #[error("Metrics setup error: {0}")]
    SetupError(String),

    /// Invalid configuration provided
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// IO error during metrics operation
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Metrics handle - keeps the metrics server alive
pub struct MetricsHandle {
    /// Shutdown channel for the metrics server
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// The address where metrics are served
    pub metrics_addr: SocketAddr,
}

impl MetricsHandle {
    /// Shutdown the metrics server gracefully
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Initialize metrics collection with Prometheus exporter
///
/// # Arguments
/// * `config` - Metrics configuration
///
/// # Returns
/// Returns a handle that keeps the metrics server running
///
/// # Example
/// ```no_run
/// use backbone_observability::{MetricsConfig, MetricsExporterType, init_metrics};
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = MetricsConfig {
///     enabled: true,
///     exporter: MetricsExporterType::Prometheus,
///     port: 9090,
/// };
///
/// let metrics_handle = init_metrics(&config)?;
///
/// // Metrics are now available at http://localhost:9090/metrics
///
/// // Keep the handle alive while your app runs
/// // tokio::select! {
/// //     _ = shutdown_signal() => {}
/// //     _ = metrics_handle.shutdown() => {}
/// // }
/// # Ok(())
/// # }
/// ```
pub fn init_metrics(config: &MetricsConfig) -> Result<MetricsHandle, MetricsError> {
    const DEFAULT_METRICS_ADDR: &str = "0.0.0.0:0";

    if !config.enabled {
        return Ok(MetricsHandle {
            shutdown_tx: None,
            metrics_addr: DEFAULT_METRICS_ADDR
                .parse()
                .map_err(|e| MetricsError::InvalidConfig(format!("Invalid default metrics address: {}", e)))?,
        });
    }

    match config.exporter {
        crate::config::MetricsExporterType::Prometheus => {
            init_prometheus(config)
        }
        crate::config::MetricsExporterType::Stdout => {
            tracing::info!("Metrics exporter: stdout (logging via tracing)");
            Ok(MetricsHandle {
                shutdown_tx: None,
                metrics_addr: "0.0.0.0:0".parse().unwrap(),
            })
        }
    }
}

/// Initialize Prometheus exporter with HTTP server
#[cfg(feature = "prometheus-metrics")]
fn init_prometheus(config: &MetricsConfig) -> Result<MetricsHandle, MetricsError> {
    use prometheus::Registry;
    use std::net::SocketAddr;

    // Create a Prometheus registry
    let registry = Registry::new();

    // Create default metrics
    setup_default_metrics(&registry)?;

    // Get the port to bind to
    let port = config.port;

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    // Spawn the metrics server task
    tokio::spawn(async move {
        metrics_server_task(registry, port, shutdown_rx).await;
    });

    // Return immediately with the handle
    // The actual address will be set by the server task
    Ok(MetricsHandle {
        shutdown_tx: Some(shutdown_tx),
        metrics_addr: SocketAddr::from(([127, 0, 0, 1], port)),
    })
}

/// Metrics server task
#[cfg(feature = "prometheus-metrics")]
async fn metrics_server_task(
    registry: Registry,
    port: u16,
    shutdown_rx: oneshot::Receiver<()>,
) {
    use axum::{routing::get, Router};
    use tokio::net::TcpListener;

    // Bind to the configured port
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    match TcpListener::bind(addr).await {
        Ok(listener) => {
            let actual_addr = listener.local_addr().unwrap_or(addr);

            tracing::info!("Prometheus metrics server listening on http://{}", actual_addr);
            tracing::info!("Metrics endpoint: http://{}/metrics", actual_addr);

            // Build the metrics router
            let app = Router::new()
                .route("/metrics", get(metrics_handler))
                .route("/health", get(health_handler))
                .with_state(registry);

            // Run the server
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                    tracing::info!("Metrics server shutting down gracefully");
                })
                .await
                .map_err(|e| tracing::error!("Metrics server error: {}", e));
        }
        Err(e) => {
            tracing::error!("Failed to bind metrics server: {}", e);
        }
    }
}

/// Setup default Prometheus metrics
///
/// Registers default metrics to the provided registry for HTTP request tracking,
/// database query monitoring, and connection state.
#[cfg(feature = "prometheus-metrics")]
fn setup_default_metrics(registry: &Registry) -> Result<(), MetricsError> {
    use prometheus::{
        IntCounterVec, Histogram, IntGaugeVec,
        Opts, HistogramOpts,
    };

    // HTTP request counter
    let _http_requests_total = IntCounterVec::new(
        Opts::new("http_requests_total", "Total number of HTTP requests"),
        &["method", "path", "status"]
    )
    .map_err(|e| MetricsError::SetupError(format!("Failed to create http_requests_total: {}", e)))?;
    registry.register(Box::new(_http_requests_total.clone()))
        .map_err(|e| MetricsError::SetupError(format!("Failed to register http_requests_total: {}", e)))?;

    // HTTP request duration histogram
    let _http_request_duration_seconds = Histogram::with_opts(
        HistogramOpts::new("http_request_duration_seconds", "HTTP request latency in seconds")
            .buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0])
            .into(),
    )
    .map_err(|e| MetricsError::SetupError(format!("Failed to create http_request_duration_seconds: {}", e)))?;
    registry.register(Box::new(_http_request_duration_seconds.clone()))
        .map_err(|e| MetricsError::SetupError(format!("Failed to register http_request_duration_seconds: {}", e)))?;

    // Database query counter
    let _db_queries_total = IntCounterVec::new(
        Opts::new("db_queries_total", "Total number of database queries"),
        &["operation", "table"]
    )
    .map_err(|e| MetricsError::SetupError(format!("Failed to create db_queries_total: {}", e)))?;
    registry.register(Box::new(_db_queries_total.clone()))
        .map_err(|e| MetricsError::SetupError(format!("Failed to register db_queries_total: {}", e)))?;

    // Database query duration histogram
    let _db_query_duration_seconds = Histogram::with_opts(
        HistogramOpts::new("db_query_duration_seconds", "Database query latency in seconds")
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0])
            .into(),
    )
    .map_err(|e| MetricsError::SetupError(format!("Failed to create db_query_duration_seconds: {}", e)))?;
    registry.register(Box::new(_db_query_duration_seconds.clone()))
        .map_err(|e| MetricsError::SetupError(format!("Failed to register db_query_duration_seconds: {}", e)))?;

    // Active connections gauge
    let _http_active_connections = IntGaugeVec::new(
        Opts::new("http_active_connections", "Number of active HTTP connections"),
        &["state"]
    )
    .map_err(|e| MetricsError::SetupError(format!("Failed to create http_active_connections: {}", e)))?;
    registry.register(Box::new(_http_active_connections.clone()))
        .map_err(|e| MetricsError::SetupError(format!("Failed to register http_active_connections: {}", e)))?;

    tracing::debug!("Default Prometheus metrics registered successfully");

    Ok(())
}

/// Metrics endpoint handler
#[cfg(feature = "prometheus-metrics")]
async fn metrics_handler(
    axum::extract::State(registry): axum::extract::State<Registry>,
) -> Result<axum::response::Response, String> {
    use prometheus::{Encoder, TextEncoder};
    use http::header;

    let metrics_families = registry.gather();
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();

    encoder
        .encode(&metrics_families, &mut buffer)
        .map_err(|e| format!("Failed to encode metrics: {}", e))?;

    Ok(axum::response::Response::builder()
        .status(axum::http::StatusCode::OK)
        .header(header::CONTENT_TYPE, encoder.format_type())
        .body(axum::body::Body::from(buffer))
        .unwrap())
}

/// Health check endpoint for metrics server
#[cfg(feature = "prometheus-metrics")]
async fn health_handler() -> &'static str {
    "OK"
}

/// Stub for when prometheus-metrics feature is not enabled
#[cfg(not(feature = "prometheus-metrics"))]
fn init_prometheus(_config: &MetricsConfig) -> Result<MetricsHandle, MetricsError> {
    tracing::warn!("Prometheus metrics requested but 'prometheus-metrics' feature is not enabled");
    tracing::warn!("Enable with: cargo build --features prometheus-metrics");
    Ok(MetricsHandle {
        shutdown_tx: None,
        metrics_addr: "0.0.0.0:0".parse().unwrap(),
    })
}

// ==============================================================================
// DATABASE POOL METRICS
// ==============================================================================

/// Database connection pool statistics
///
/// Callers (e.g., backbone-orm) extract pool stats from their connection pool
/// implementation (e.g., `sqlx::PgPool`) and pass them here for Prometheus recording.
/// This avoids a hard dependency on sqlx in the observability crate.
#[derive(Debug, Clone, Copy)]
pub struct PoolStats {
    /// Total number of connections in the pool (active + idle)
    pub total: u32,
    /// Number of connections currently in use
    pub active: u32,
    /// Number of idle connections available
    pub idle: u32,
    /// Maximum number of connections configured for the pool
    pub max: u32,
}

/// Global DB pool gauges (lazy-initialized on first use)
#[cfg(feature = "prometheus-metrics")]
static DB_POOL_TOTAL: OnceLock<prometheus::IntGaugeVec> = OnceLock::new();
#[cfg(feature = "prometheus-metrics")]
static DB_POOL_ACTIVE: OnceLock<prometheus::IntGaugeVec> = OnceLock::new();
#[cfg(feature = "prometheus-metrics")]
static DB_POOL_IDLE: OnceLock<prometheus::IntGaugeVec> = OnceLock::new();
#[cfg(feature = "prometheus-metrics")]
static DB_POOL_MAX: OnceLock<prometheus::IntGaugeVec> = OnceLock::new();

/// Record database pool metrics for the default pool
///
/// Updates Prometheus gauges for database connection pool statistics.
/// Should be called periodically (e.g., every 15 seconds) by the ORM layer.
#[cfg(feature = "prometheus-metrics")]
pub fn record_pool_metrics(stats: &PoolStats) {
    record_pool_metrics_named("default", stats);
}

/// Record database pool metrics for a named pool
///
/// Allows tracking metrics for multiple database pools (e.g., "primary", "replica").
#[cfg(feature = "prometheus-metrics")]
pub fn record_pool_metrics_named(pool_name: &str, stats: &PoolStats) {
    use prometheus::{IntGaugeVec, Opts};

    let total = DB_POOL_TOTAL.get_or_init(|| {
        let metric = IntGaugeVec::new(
            Opts::new(common::DB_POOL_CONNECTIONS_TOTAL, "Total connections in database pool"),
            &[common::LABEL_POOL],
        ).expect("Failed to create db_pool_connections_total metric");
        let _ = prometheus::register(Box::new(metric.clone()));
        metric
    });

    let active = DB_POOL_ACTIVE.get_or_init(|| {
        let metric = IntGaugeVec::new(
            Opts::new(common::DB_POOL_CONNECTIONS_ACTIVE, "Active connections in database pool"),
            &[common::LABEL_POOL],
        ).expect("Failed to create db_pool_connections_active metric");
        let _ = prometheus::register(Box::new(metric.clone()));
        metric
    });

    let idle = DB_POOL_IDLE.get_or_init(|| {
        let metric = IntGaugeVec::new(
            Opts::new(common::DB_POOL_CONNECTIONS_IDLE, "Idle connections in database pool"),
            &[common::LABEL_POOL],
        ).expect("Failed to create db_pool_connections_idle metric");
        let _ = prometheus::register(Box::new(metric.clone()));
        metric
    });

    let max = DB_POOL_MAX.get_or_init(|| {
        let metric = IntGaugeVec::new(
            Opts::new(common::DB_POOL_MAX_CONNECTIONS, "Max configured connections for database pool"),
            &[common::LABEL_POOL],
        ).expect("Failed to create db_pool_max_connections metric");
        let _ = prometheus::register(Box::new(metric.clone()));
        metric
    });

    total.with_label_values(&[pool_name]).set(stats.total as i64);
    active.with_label_values(&[pool_name]).set(stats.active as i64);
    idle.with_label_values(&[pool_name]).set(stats.idle as i64);
    max.with_label_values(&[pool_name]).set(stats.max as i64);
}

/// No-op stub when prometheus-metrics feature is disabled
#[cfg(not(feature = "prometheus-metrics"))]
pub fn record_pool_metrics(_stats: &PoolStats) {}

/// No-op stub when prometheus-metrics feature is disabled
#[cfg(not(feature = "prometheus-metrics"))]
pub fn record_pool_metrics_named(_pool_name: &str, _stats: &PoolStats) {}

// Common metrics definitions
///
/// Common metric names and label names used throughout the observability system.
pub mod common {
    /// Request counter metric name
    pub const HTTP_REQUESTS_TOTAL: &str = "http_requests_total";

    /// Request duration metric name
    pub const HTTP_REQUEST_DURATION_SECONDS: &str = "http_request_duration_seconds";

    /// Active connections metric name
    pub const HTTP_ACTIVE_CONNECTIONS: &str = "http_active_connections";

    /// Database query counter metric name
    pub const DB_QUERIES_TOTAL: &str = "db_queries_total";

    /// Database query duration metric name
    pub const DB_QUERY_DURATION_SECONDS: &str = "db_query_duration_seconds";

    /// Metric labels for HTTP requests
    pub const LABEL_METHOD: &str = "method";
    /// Label for the HTTP method (GET, POST, etc.)
    pub const LABEL_PATH: &str = "path";
    /// Label for the HTTP request path
    pub const LABEL_STATUS: &str = "status";
    /// Label for the HTTP status code

    /// Metric labels for DB queries
    pub const LABEL_OPERATION: &str = "operation";
    /// Label for the database operation (SELECT, INSERT, etc.)
    pub const LABEL_TABLE: &str = "table";

    /// Metric labels for connection state
    pub const LABEL_STATE: &str = "state";

    /// Database pool total connections metric name
    pub const DB_POOL_CONNECTIONS_TOTAL: &str = "db_pool_connections_total";
    /// Database pool active connections metric name
    pub const DB_POOL_CONNECTIONS_ACTIVE: &str = "db_pool_connections_active";
    /// Database pool idle connections metric name
    pub const DB_POOL_CONNECTIONS_IDLE: &str = "db_pool_connections_idle";
    /// Database pool max connections metric name
    pub const DB_POOL_MAX_CONNECTIONS: &str = "db_pool_max_connections";
    /// Label for pool name (supports multiple pools)
    pub const LABEL_POOL: &str = "pool";
}
