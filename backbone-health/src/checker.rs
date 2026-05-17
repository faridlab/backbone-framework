//! Health checker implementation

use super::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration, Instant};

/// Main health checker that manages and runs health checks
pub struct HealthChecker {
    config: HealthConfig,
    components: Arc<RwLock<HashMap<String, Box<dyn HealthCheck>>>>,
    statuses: Arc<RwLock<HashMap<String, ComponentStatus>>>,
    start_time: Instant,
    is_running: Arc<RwLock<bool>>,
}

impl HealthChecker {
    /// Create a new health checker with the given configuration
    pub fn new(config: HealthConfig) -> Self {
        Self {
            config,
            components: Arc::new(RwLock::new(HashMap::new())),
            statuses: Arc::new(RwLock::new(HashMap::new())),
            start_time: Instant::now(),
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    /// Add a health check component
    pub async fn add_component(&self, name: String, component: Box<dyn HealthCheck>) -> HealthResult<()> {
        let mut components = self.components.write().await;
        components.insert(name.clone(), component);

        // Initialize status for the new component
        let mut statuses = self.statuses.write().await;
        statuses.insert(name.clone(), ComponentStatus::new(name.clone()));

        Ok(())
    }

    /// Remove a health check component
    pub async fn remove_component(&self, name: &str) -> HealthResult<()> {
        let mut components = self.components.write().await;
        components.remove(name).ok_or(HealthError::ComponentNotFound(name.to_string()))?;

        let mut statuses = self.statuses.write().await;
        statuses.remove(name);

        Ok(())
    }

    /// Start the health checker (runs checks periodically)
    pub async fn start(&self) -> HealthResult<()> {
        let mut is_running = self.is_running.write().await;
        if *is_running {
            return Err(HealthError::Internal("Health checker is already running".to_string()));
        }

        *is_running = true;
        drop(is_running);

        let checker = self.clone();
        tokio::spawn(async move {
            checker.run_checks_periodically().await;
        });

        Ok(())
    }

    /// Stop the health checker
    pub async fn stop(&self) -> HealthResult<()> {
        let mut is_running = self.is_running.write().await;
        *is_running = false;
        Ok(())
    }

    /// Get current health report
    pub async fn health_report(&self) -> HealthReport {
        let statuses = self.statuses.read().await;
        let components = statuses.clone();

        HealthReport::new(
            self.config.app_version.clone().unwrap_or_else(|| "unknown".to_string()),
            self.config.app_name.clone(),
            self.start_time.elapsed(),
            components,
        )
    }

    /// Get simple health status (for basic endpoints)
    pub async fn health_status(&self) -> SimpleHealthResponse {
        let report = self.health_report().await;
        let message = match report.status {
            HealthStatus::Healthy => Some("All systems operational".to_string()),
            HealthStatus::Degraded => Some("Some systems degraded".to_string()),
            HealthStatus::Unhealthy => Some("Some systems unhealthy".to_string()),
        };

        SimpleHealthResponse::new(report.status, message)
    }

    /// Check readiness (no unhealthy components)
    pub async fn readiness(&self) -> SimpleHealthResponse {
        let report = self.health_report().await;

        if report.is_ready() {
            SimpleHealthResponse::healthy()
        } else {
            SimpleHealthResponse::unhealthy("Application is not ready".to_string())
        }
    }

    /// Check liveness (has been checked recently)
    pub async fn liveness(&self, max_age: Option<Duration>) -> SimpleHealthResponse {
        let report = self.health_report().await;
        let max_age = max_age.unwrap_or(Duration::from_secs(60));

        if report.is_alive(max_age) {
            SimpleHealthResponse::healthy()
        } else {
            SimpleHealthResponse::unhealthy("Application is not alive".to_string())
        }
    }

    
    /// Run a single health check for all components
    pub async fn run_single_check(&self) -> HealthResult<HealthReport> {
        let components = self.components.read().await;
        let mut statuses = self.statuses.write().await;

        for (name, component) in components.iter() {
            let timeout = component.timeout().unwrap_or(self.config.timeout);
            let check_result = tokio::time::timeout(timeout, component.check()).await;

            // Preserve running counters across checks by reusing the prior
            // status when one exists; otherwise start fresh.
            let mut component_status = statuses
                .get(name)
                .cloned()
                .unwrap_or_else(|| ComponentStatus::new(name.clone()));

            match check_result {
                Ok(Ok(check_status)) => {
                    let response_time = check_status.response_time;
                    match check_status.status {
                        HealthStatus::Healthy => component_status.record_success(response_time),
                        HealthStatus::Degraded => component_status.record_degraded(
                            check_status
                                .message
                                .clone()
                                .unwrap_or_else(|| "Degraded".to_string()),
                            response_time,
                        ),
                        HealthStatus::Unhealthy => component_status.record_failure(
                            check_status
                                .error
                                .clone()
                                .or_else(|| check_status.message.clone())
                                .unwrap_or_else(|| "Unhealthy".to_string()),
                            response_time,
                        ),
                    }
                    // Carry over any metadata the check attached.
                    for (k, v) in check_status.metadata {
                        component_status.add_metadata(k, v);
                    }
                }
                Ok(Err(e)) => {
                    component_status.record_failure(format!("Health check error: {}", e), timeout);
                }
                Err(_) => {
                    component_status
                        .record_failure("Health check timed out".to_string(), timeout);
                }
            }

            // Apply failure/success thresholds to overall component status.
            if component_status.consecutive_failures >= self.config.failure_threshold {
                component_status.status = HealthStatus::Unhealthy;
            } else if component_status.consecutive_successes >= self.config.success_threshold {
                component_status.status = HealthStatus::Healthy;
            }

            statuses.insert(name.clone(), component_status);
        }

        // Drop both locks before calling `health_report`, which re-acquires
        // `statuses.read()`. Without this, the prior `statuses.write()` causes
        // a self-deadlock against tokio's fair RwLock.
        drop(statuses);
        drop(components);

        Ok(self.health_report().await)
    }

    /// Run health checks periodically in the background
    async fn run_checks_periodically(self) {
        let mut interval = interval(self.config.check_interval);
        interval.tick().await; // Skip first tick

        loop {
            {
                let is_running = self.is_running.read().await;
                if !*is_running {
                    break;
                }
            }

            if let Err(e) = self.run_single_check().await {
                eprintln!("Health check error: {}", e);
            }

            interval.tick().await;
        }
    }

    /// Get a list of all component names
    pub async fn component_names(&self) -> Vec<String> {
        let components = self.components.read().await;
        components.keys().cloned().collect()
    }

    /// Get status of a specific component
    pub async fn component_status(&self, name: &str) -> Option<ComponentStatus> {
        let statuses = self.statuses.read().await;
        statuses.get(name).cloned()
    }

    /// Create a new health checker builder
    pub fn builder() -> HealthCheckerBuilder {
        HealthCheckerBuilder::new()
    }
}

impl Clone for HealthChecker {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            components: Arc::clone(&self.components),
            statuses: Arc::clone(&self.statuses),
            start_time: self.start_time,
            is_running: Arc::clone(&self.is_running),
        }
    }
}

impl std::fmt::Debug for HealthChecker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthChecker")
            .field("config", &self.config)
            .field("start_time", &self.start_time)
            .field("is_running", &self.is_running)
            .field("has_components", &true)
            .finish()
    }
}

/// Health checker builder
pub struct HealthCheckerBuilder {
    config: HealthConfig,
    components: Vec<(String, Box<dyn HealthCheck>)>,
}

impl HealthCheckerBuilder {
    /// Create a new health checker builder
    pub fn new() -> Self {
        Self {
            config: HealthConfig::default(),
            components: Vec::new(),
        }
    }

    /// Set the check interval
    pub fn check_interval(mut self, interval: Duration) -> Self {
        self.config.check_interval = interval;
        self
    }

    /// Set the timeout for individual checks
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Set the failure threshold
    pub fn failure_threshold(mut self, threshold: usize) -> Self {
        self.config.failure_threshold = threshold;
        self
    }

    /// Set the success threshold
    pub fn success_threshold(mut self, threshold: usize) -> Self {
        self.config.success_threshold = threshold;
        self
    }

    /// Set whether to include details in responses
    pub fn include_details(mut self, include: bool) -> Self {
        self.config.include_details = include;
        self
    }

    /// Set whether to enable metrics
    pub fn enable_metrics(mut self, enable: bool) -> Self {
        self.config.enable_metrics = enable;
        self
    }

    /// Set application version
    pub fn app_version(mut self, version: String) -> Self {
        self.config.app_version = Some(version);
        self
    }

    /// Set application name
    pub fn app_name(mut self, name: String) -> Self {
        self.config.app_name = Some(name);
        self
    }

    /// Add a component
    pub fn component(mut self, name: String, component: Box<dyn HealthCheck>) -> Self {
        self.components.push((name, component));
        self
    }

    /// Build the health checker
    pub async fn build(self) -> HealthResult<HealthChecker> {
        let checker = HealthChecker::new(self.config);

        // Add all components
        for (name, component) in self.components {
            checker.add_component(name, component).await?;
        }

        Ok(checker)
    }
}

impl Default for HealthCheckerBuilder {
    fn default() -> Self {
        Self::new()
    }
}