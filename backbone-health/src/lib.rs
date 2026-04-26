//! Backbone Framework Health Check Module
//!
//! Provides basic health monitoring and status endpoints for production applications.
//!
//! ## Features
//!
//! - **Health Status**: Overall application health with component status
//! - **Component Checks**: Mock and custom health checks
//! - **Simple HTTP Server**: Built-in health endpoint server
//! - **JSON Responses**: Ready-to-use health status endpoints
//!
//! ## Quick Start
//!
//! ```rust
//! use backbone_health::{HealthChecker, HealthConfig, MockHealthCheck, SimpleHealthServer};
//! use std::time::Duration;
//!
//! let health_checker = HealthChecker::new(HealthConfig::default());
//!
//! // Add component checks
//! let mock_check = MockHealthCheck::healthy("database".to_string());
//! health_checker.add_component("database".to_string(), Box::new(mock_check)).await?;
//!
//! // Create health server
//! let server = SimpleHealthServer::new(health_checker, 8080);
//! server.run_demo().await?;
//! ```

pub mod checker;
pub mod components_simple;

#[cfg(feature = "cli")]
pub mod cli;

pub mod routes;

// Re-export components with a cleaner name
pub use components_simple as components;
pub mod status;
pub mod health_server;

// Re-export commonly used types
pub use checker::*;
pub use components::*;
pub use status::{HealthCheck, *}; // Export HealthCheck trait explicitly
pub use health_server::*;

/// Health module version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default health check endpoint path
pub const DEFAULT_HEALTH_PATH: &str = "/health";

/// Default readiness check endpoint path
pub const DEFAULT_READINESS_PATH: &str = "/ready";

/// Default liveness check endpoint path
pub const DEFAULT_LIVENESS_PATH: &str = "/live";

/// Default metrics endpoint path
pub const DEFAULT_METRICS_PATH: &str = "/metrics";

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Interval between health checks
    pub check_interval: std::time::Duration,

    /// Timeout for individual health checks
    pub timeout: std::time::Duration,

    /// Number of consecutive failures before marking as unhealthy
    pub failure_threshold: usize,

    /// Number of consecutive successes before marking as healthy
    pub success_threshold: usize,

    /// Whether to include detailed component status in health responses
    pub include_details: bool,

    /// Whether to track metrics
    pub enable_metrics: bool,

    /// Custom application version
    pub app_version: Option<String>,

    /// Custom application name
    pub app_name: Option<String>,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            check_interval: std::time::Duration::from_secs(30),
            timeout: std::time::Duration::from_secs(5),
            failure_threshold: 3,
            success_threshold: 2,
            include_details: true,
            enable_metrics: true,
            app_version: None,
            app_name: None,
        }
    }
}

/// Health check error types
#[derive(thiserror::Error, Debug)]
pub enum HealthError {
    #[error("Health check timeout: {0}")]
    Timeout(String),

    #[error("Health check failed: {0}")]
    CheckFailed(String),

    #[error("Component not found: {0}")]
    ComponentNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for health check operations
pub type HealthResult<T> = Result<T, HealthError>;