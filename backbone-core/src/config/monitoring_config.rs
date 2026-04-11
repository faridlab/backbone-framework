//! Monitoring configuration
//!
//! Defines metrics, tracing, and health check settings.

use serde::{Deserialize, Serialize};

fn default_true() -> bool { true }

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable monitoring
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Enable metrics collection
    #[serde(default = "default_true")]
    pub metrics_enabled: bool,
    /// Enable distributed tracing
    #[serde(default = "default_true")]
    pub tracing_enabled: bool,
    /// Enable health checks
    #[serde(default = "default_true")]
    pub health_check_enabled: bool,
    /// Prometheus port
    #[serde(default)]
    pub prometheus_port: Option<u16>,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            metrics_enabled: true,
            tracing_enabled: true,
            health_check_enabled: true,
            prometheus_port: Some(9090),
        }
    }
}
