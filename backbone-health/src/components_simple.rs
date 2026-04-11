//! Simplified health check components (without external dependencies)

use async_trait::async_trait;
use std::time::Duration;
use super::{HealthResult, ComponentStatus, HealthCheck};

/// Simple mock health check for testing
#[derive(Debug)]
pub struct MockHealthCheck {
    name: String,
    response_time: Duration,
    always_healthy: bool,
}

impl MockHealthCheck {
    /// Create a new mock health check
    pub fn new(name: String, response_time: Duration, always_healthy: bool) -> Self {
        Self {
            name,
            response_time,
            always_healthy,
        }
    }

    /// Create a healthy mock check
    pub fn healthy(name: String) -> Self {
        Self::new(name, Duration::from_millis(10), true)
    }

    /// Create an unhealthy mock check
    pub fn unhealthy(name: String) -> Self {
        Self::new(name, Duration::from_millis(50), false)
    }
}

#[async_trait]
impl HealthCheck for MockHealthCheck {
    async fn check(&self) -> HealthResult<ComponentStatus> {
        tokio::time::sleep(self.response_time).await;

        let mut status = ComponentStatus::new(self.name.clone());

        if self.always_healthy {
            status.record_success(self.response_time);
        } else {
            status.record_failure("Mock component is unhealthy".to_string(), self.response_time);
        }

        Ok(status)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Custom health check from a closure
pub struct CustomHealthCheck {
    name: String,
    _check_fn: Box<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = HealthResult<ComponentStatus>> + Send>> + Send + Sync>,
}

impl CustomHealthCheck {
    /// Create a new custom health check
    pub fn new<F, Fut>(name: String, check_fn: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = HealthResult<ComponentStatus>> + Send + 'static,
    {
        Self {
            name,
            _check_fn: Box::new(move || Box::pin(check_fn())),
        }
    }
}

#[async_trait]
impl HealthCheck for CustomHealthCheck {
    async fn check(&self) -> HealthResult<ComponentStatus> {
        (self._check_fn)().await
    }

    fn name(&self) -> &str {
        &self.name
    }
}