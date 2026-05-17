//! Health status types and utilities

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::time::Duration;
use crate::HealthResult;

/// Trait for health check components
#[async_trait]
pub trait HealthCheck: Send + Sync {
    /// Perform a health check and return the component status
    async fn check(&self) -> HealthResult<ComponentStatus>;

    /// Get the name of this health check
    fn name(&self) -> &str;

    /// Get the timeout for this health check (optional)
    fn timeout(&self) -> Option<Duration> {
        None
    }
}

/// Overall health status of the application
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// Application is healthy and all components are functioning
    Healthy,
    /// Application is degraded but still functional (some components failing)
    Degraded,
    /// Application is unhealthy (critical components failing)
    Unhealthy,
}

/// Status of an individual component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatus {
    /// Name of the component
    pub name: String,

    /// Current status of the component
    pub status: HealthStatus,

    /// Last time this component was checked
    pub last_checked: DateTime<Utc>,

    /// How long the last check took
    pub response_time: Duration,

    /// Response time in milliseconds (convenience field)
    pub response_time_ms: u64,

    /// Optional error message if the component is unhealthy
    pub error: Option<String>,

    /// Human-readable status message
    pub message: Option<String>,

    /// Additional details about component status
    pub details: Option<String>,

    /// Number of consecutive failures
    pub consecutive_failures: usize,

    /// Number of successful checks in the current streak
    pub consecutive_successes: usize,

    /// Total number of checks performed
    pub total_checks: u64,

    /// Percentage of successful checks
    pub success_rate: f64,

    /// Additional metadata about the component
    pub metadata: HashMap<String, String>,
}

impl ComponentStatus {
    /// Create a new component status
    pub fn new(name: String) -> Self {
        Self {
            name,
            status: HealthStatus::Healthy,
            last_checked: Utc::now(),
            response_time: Duration::from_millis(0),
            response_time_ms: 0,
            error: None,
            message: None,
            details: None,
            consecutive_failures: 0,
            consecutive_successes: 0,
            total_checks: 0,
            success_rate: 0.0,
            metadata: HashMap::new(),
        }
    }

    /// Record a successful health check
    pub fn record_success(&mut self, response_time: Duration) {
        self.status = HealthStatus::Healthy;
        self.last_checked = Utc::now();
        self.response_time = response_time;
        self.response_time_ms = response_time.as_millis() as u64;
        self.error = None;
        self.message = Some("Healthy".to_string());
        self.consecutive_successes += 1;
        self.consecutive_failures = 0;
        self.total_checks += 1;
        self.update_success_rate();
    }

    /// Record a failed health check
    pub fn record_failure(&mut self, error: String, response_time: Duration) {
        self.status = HealthStatus::Unhealthy;
        self.last_checked = Utc::now();
        self.response_time = response_time;
        self.response_time_ms = response_time.as_millis() as u64;
        self.error = Some(error.clone());
        self.message = Some(error);
        self.consecutive_failures += 1;
        self.consecutive_successes = 0;
        self.total_checks += 1;
        self.update_success_rate();
    }

    /// Record a degraded health check
    pub fn record_degraded(&mut self, warning: String, response_time: Duration) {
        self.status = HealthStatus::Degraded;
        self.last_checked = Utc::now();
        self.response_time = response_time;
        self.response_time_ms = response_time.as_millis() as u64;
        self.error = Some(warning.clone());
        self.message = Some(warning);
        self.consecutive_failures = 0;
        self.consecutive_successes += 1;
        self.total_checks += 1;
        self.update_success_rate();
    }

    /// Update the success rate based on total checks and failures
    fn update_success_rate(&mut self) {
        if self.total_checks == 0 {
            self.success_rate = 0.0;
        } else {
            let failures = self.consecutive_failures as u64;
            self.success_rate =
                ((self.total_checks - failures) as f64 / self.total_checks as f64) * 100.0;
        }
    }

    /// Check if the component should be considered healthy based on thresholds
    pub fn is_healthy(&self, failure_threshold: usize) -> bool {
        self.consecutive_failures < failure_threshold
    }

    /// Add metadata to the component
    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }
}

/// Comprehensive health report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    /// Overall health status
    pub status: HealthStatus,

    /// Application version
    pub version: String,

    /// Application name
    pub app_name: Option<String>,

    /// Timestamp when the report was generated
    pub timestamp: DateTime<Utc>,

    /// Uptime of the application
    pub uptime: Duration,

    /// Individual component statuses
    pub components: HashMap<String, ComponentStatus>,

    /// Summary statistics
    pub summary: HealthSummary,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl HealthReport {
    /// Create a new health report
    pub fn new(
        version: String,
        app_name: Option<String>,
        uptime: Duration,
        components: HashMap<String, ComponentStatus>,
    ) -> Self {
        let status = Self::calculate_overall_status(&components);
        let summary = Self::calculate_summary(&components);

        Self {
            status,
            version,
            app_name,
            timestamp: Utc::now(),
            uptime,
            components,
            summary,
            metadata: HashMap::new(),
        }
    }

    /// Calculate overall health status from component statuses
    fn calculate_overall_status(components: &HashMap<String, ComponentStatus>) -> HealthStatus {
        if components.is_empty() {
            return HealthStatus::Healthy;
        }

        let mut unhealthy_count = 0;
        let mut degraded_count = 0;

        for component in components.values() {
            match component.status {
                HealthStatus::Unhealthy => unhealthy_count += 1,
                HealthStatus::Degraded => degraded_count += 1,
                HealthStatus::Healthy => {}
            }
        }

        // All components unhealthy → overall Unhealthy.
        // Any (but not all) unhealthy or degraded → overall Degraded.
        if unhealthy_count == components.len() {
            HealthStatus::Unhealthy
        } else if unhealthy_count > 0 || degraded_count > 0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }

    /// Calculate summary statistics
    fn calculate_summary(components: &HashMap<String, ComponentStatus>) -> HealthSummary {
        let total_components = components.len();
        let mut healthy_count = 0;
        let mut degraded_count = 0;
        let mut unhealthy_count = 0;
        let mut total_response_time = Duration::from_millis(0);
        let mut components_with_checks = 0;

        for component in components.values() {
            match component.status {
                HealthStatus::Healthy => healthy_count += 1,
                HealthStatus::Degraded => degraded_count += 1,
                HealthStatus::Unhealthy => unhealthy_count += 1,
            }

            if component.total_checks > 0 {
                total_response_time += component.response_time;
                components_with_checks += 1;
            }
        }

        let average_response_time = if components_with_checks > 0 {
            total_response_time / components_with_checks as u32
        } else {
            Duration::from_millis(0)
        };

        HealthSummary {
            total_components,
            healthy_count,
            degraded_count,
            unhealthy_count,
            average_response_time,
        }
    }

    /// Add metadata to the health report
    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }

    /// Check if the application is healthy
    pub fn is_healthy(&self) -> bool {
        matches!(self.status, HealthStatus::Healthy)
    }

    /// Check if the application is ready (has no unhealthy components)
    pub fn is_ready(&self) -> bool {
        self.summary.unhealthy_count == 0
    }

    /// Check if the application is alive (has been checked recently)
    pub fn is_alive(&self, max_age: Duration) -> bool {
        // With no registered components, the application is trivially alive.
        if self.components.is_empty() {
            return true;
        }
        // Otherwise: alive if any component was checked within the max age.
        self.components.values().any(|c| {
            Utc::now()
                .signed_duration_since(c.last_checked)
                .to_std()
                .unwrap_or(Duration::MAX)
                <= max_age
        })
    }
}

/// Summary statistics for the health report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSummary {
    /// Total number of components
    pub total_components: usize,

    /// Number of healthy components
    pub healthy_count: usize,

    /// Number of degraded components
    pub degraded_count: usize,

    /// Number of unhealthy components
    pub unhealthy_count: usize,

    /// Average response time across all components
    pub average_response_time: Duration,
}

/// Simple health status response for basic endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleHealthResponse {
    /// Overall health status
    pub status: HealthStatus,

    /// Timestamp when the response was generated
    pub timestamp: DateTime<Utc>,

    /// Optional message
    pub message: Option<String>,
}

impl SimpleHealthResponse {
    /// Create a new simple health response
    pub fn new(status: HealthStatus, message: Option<String>) -> Self {
        Self {
            status,
            timestamp: Utc::now(),
            message,
        }
    }

    /// Create a healthy response
    pub fn healthy() -> Self {
        Self::new(HealthStatus::Healthy, Some("All systems operational".to_string()))
    }

    /// Create an unhealthy response
    pub fn unhealthy(message: String) -> Self {
        Self::new(HealthStatus::Unhealthy, Some(message))
    }

    /// Create a degraded response
    pub fn degraded(message: String) -> Self {
        Self::new(HealthStatus::Degraded, Some(message))
    }
}

impl Default for SimpleHealthResponse {
    fn default() -> Self {
        Self::healthy()
    }
}