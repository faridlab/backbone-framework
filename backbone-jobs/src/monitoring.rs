//! Monitoring and health check functionality

use crate::types::JobStatistics;
use axum::{extract::State, response::Json, routing::get, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Health check status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl HealthStatus {
    /// Get status as string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Unhealthy => "unhealthy",
        }
    }

    /// Get HTTP status code for this health status
    pub fn http_status_code(&self) -> u16 {
        match self {
            Self::Healthy => 200,
            Self::Degraded => 200,
            Self::Unhealthy => 503,
        }
    }
}

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
    pub uptime_seconds: u64,
    pub version: String,
    pub components: HashMap<String, ComponentHealth>,
}

/// Component health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub status: String,
    pub message: Option<String>,
    pub response_time_ms: Option<u64>,
    pub last_check: DateTime<Utc>,
}

/// Metrics data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    pub timestamp: DateTime<Utc>,
    pub job_statistics: JobStatistics,
    pub system_metrics: SystemMetrics,
    pub custom_metrics: HashMap<String, f64>,
}

/// System metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: u64,
    pub memory_usage_percent: f64,
    pub disk_usage_percent: f64,
    pub network_io_bytes: u64,
}

/// Monitoring service
pub struct MonitoringService {
    scheduler_start_time: DateTime<Utc>,
    health_checks: RwLock<HashMap<String, ComponentHealth>>,
    metrics_history: RwLock<Vec<Metrics>>,
    max_history_size: usize,
}

// Clone is not appropriate for MonitoringService as it contains RwLocks
// Instead, use Arc<MonitoringService> for sharing

impl MonitoringService {
    /// Create a new monitoring service
    pub fn new() -> Self {
        Self {
            scheduler_start_time: Utc::now(),
            health_checks: RwLock::new(HashMap::new()),
            metrics_history: RwLock::new(Vec::new()),
            max_history_size: 1000,
        }
    }

    /// Create monitoring service with custom history size
    pub fn with_history_size(max_history_size: usize) -> Self {
        Self {
            scheduler_start_time: Utc::now(),
            health_checks: RwLock::new(HashMap::new()),
            metrics_history: RwLock::new(Vec::new()),
            max_history_size,
        }
    }

    /// Record a component health check
    pub async fn record_health_check(
        &self,
        component: String,
        status: HealthStatus,
        message: Option<String>,
        response_time: Option<Duration>,
    ) {
        let mut health_checks = self.health_checks.write().await;
        health_checks.insert(
            component,
            ComponentHealth {
                status: status.as_str().to_string(),
                message,
                response_time_ms: response_time.map(|d| d.as_millis() as u64),
                last_check: Utc::now(),
            },
        );
    }

    /// Record metrics
    pub async fn record_metrics(&self, job_statistics: JobStatistics, custom_metrics: HashMap<String, f64>) {
        let metrics = Metrics {
            timestamp: Utc::now(),
            job_statistics,
            system_metrics: self.collect_system_metrics(),
            custom_metrics,
        };

        let mut metrics_history = self.metrics_history.write().await;
        metrics_history.push(metrics);

        // Trim history if too large
        let len = metrics_history.len();
        if len > self.max_history_size {
            let drain_count = len - self.max_history_size;
            metrics_history.drain(0..drain_count);
        }
    }

    /// Get current health status
    pub async fn get_health_status(&self) -> HealthCheckResponse {
        let health_checks = self.health_checks.read().await;
        let uptime = (Utc::now() - self.scheduler_start_time).num_seconds() as u64;

        // Determine overall health
        let overall_status = if health_checks.values().any(|h| h.status == "unhealthy") {
            HealthStatus::Unhealthy
        } else if health_checks.values().any(|h| h.status == "degraded") {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        // Convert the HashMap to owned data
        let components_map: HashMap<String, ComponentHealth> = health_checks
            .iter()
            .map(|(k, v)| (k.clone(), (*v).clone()))
            .collect();

        HealthCheckResponse {
            status: overall_status.as_str().to_string(),
            timestamp: Utc::now(),
            uptime_seconds: uptime,
            version: env!("CARGO_PKG_VERSION").to_string(),
            components: components_map,
        }
    }

    /// Get latest metrics
    pub async fn get_latest_metrics(&self) -> Option<Metrics> {
        let metrics_history = self.metrics_history.read().await;
        metrics_history.last().cloned()
    }

    /// Get metrics history
    pub async fn get_metrics_history(&self, limit: Option<usize>) -> Vec<Metrics> {
        let metrics_history = self.metrics_history.read().await;
        match limit {
            Some(limit) => metrics_history
                .iter()
                .rev()
                .take(limit)
                .cloned()
                .collect(),
            None => metrics_history.clone(),
        }
    }

    /// Clear old metrics
    pub async fn clear_old_metrics(&self, older_than: DateTime<Utc>) {
        let mut metrics_history = self.metrics_history.write().await;
        metrics_history.retain(|m| m.timestamp > older_than);
    }

    /// Collect system metrics
    fn collect_system_metrics(&self) -> SystemMetrics {
        // This is a placeholder implementation
        // In a real application, you would use sysinfo or similar crate
        SystemMetrics {
            cpu_usage_percent: 0.0,
            memory_usage_mb: 0,
            memory_usage_percent: 0.0,
            disk_usage_percent: 0.0,
            network_io_bytes: 0,
        }
    }

    /// Create HTTP router for health checks and metrics
    ///
    /// Note: This method requires the service to be wrapped in Arc before calling
    pub fn create_router(self: Arc<Self>) -> Router {
        Router::new()
            .route("/health", get(health_check_handler))
            .route("/metrics", get(metrics_handler))
            .route("/metrics/history", get(metrics_history_handler))
            .with_state(self)
    }
}

/// Health check HTTP handler
async fn health_check_handler(
    State(monitoring): State<Arc<MonitoringService>>,
) -> Json<HealthCheckResponse> {
    Json(monitoring.get_health_status().await)
}

/// Metrics HTTP handler
async fn metrics_handler(
    State(monitoring): State<Arc<MonitoringService>>,
) -> Json<Option<Metrics>> {
    Json(monitoring.get_latest_metrics().await)
}

/// Metrics history HTTP handler
async fn metrics_history_handler(
    State(monitoring): State<Arc<MonitoringService>>,
    axum::extract::Query(params): axum::extract::Query<HistoryQuery>,
) -> Json<Vec<Metrics>> {
    let limit = params.limit.map(|l| l as usize);
    Json(monitoring.get_metrics_history(limit).await)
}

/// Query parameters for metrics history
#[derive(serde::Deserialize)]
struct HistoryQuery {
    limit: Option<u64>,
}

impl Default for MonitoringService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check_recording() {
        let monitoring = MonitoringService::new();

        // Record a healthy component
        monitoring
            .record_health_check(
                "database".to_string(),
                HealthStatus::Healthy,
                Some("Connected successfully".to_string()),
                Some(Duration::from_millis(50)),
            )
            .await;

        // Record a degraded component
        monitoring
            .record_health_check(
                "queue".to_string(),
                HealthStatus::Degraded,
                Some("High latency".to_string()),
                Some(Duration::from_millis(500)),
            )
            .await;

        // Check overall health status
        let health = monitoring.get_health_status().await;
        assert_eq!(health.status, "degraded");
        assert_eq!(health.components.len(), 2);
        assert_eq!(health.components["database"].status, "healthy");
        assert_eq!(health.components["queue"].status, "degraded");
    }

    #[tokio::test]
    async fn test_metrics_recording() {
        let monitoring = MonitoringService::new();

        let job_stats = JobStatistics::default();
        let custom_metrics = HashMap::from([
            ("custom_counter".to_string(), 42.0),
            ("custom_gauge".to_string(), 3.14),
        ]);

        monitoring.record_metrics(job_stats, custom_metrics).await;

        let latest = monitoring.get_latest_metrics().await;
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().custom_metrics["custom_counter"], 42.0);
    }

    #[tokio::test]
    async fn test_metrics_history() {
        let monitoring = MonitoringService::with_history_size(3);

        // Record some metrics
        for i in 0..5 {
            let job_stats = JobStatistics::default();
            let custom_metrics = HashMap::from([("counter".to_string(), i as f64)]);
            monitoring.record_metrics(job_stats, custom_metrics).await;
        }

        // Check that only the last 3 are kept
        let history = monitoring.get_metrics_history(None).await;
        assert_eq!(history.len(), 3);

        // Check that the values are the most recent ones
        assert_eq!(history.last().unwrap().custom_metrics["counter"], 4.0);
    }
}