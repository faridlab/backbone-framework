//! Queue monitoring and metrics collection module

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

use crate::{
    QueueService, QueueStats, QueueHealth, QueueHealthCheck
};

/// Monitoring metrics for queue operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMetrics {
    /// Basic queue metrics
    pub total_messages: u64,
    pub processed_messages: u64,
    pub failed_messages: u64,
    pub dead_letter_messages: u64,
    pub visible_messages: u64,
    pub invisible_messages: u64,

    /// Performance metrics
    pub avg_processing_time_ms: f64,
    pub messages_per_second: f64,
    pub error_rate: f64,
    pub success_rate: f64,

    /// Time-based metrics
    pub queue_depth_trend: Vec<TimestampedValue>,
    pub processing_rate_trend: Vec<TimestampedValue>,
    pub error_rate_trend: Vec<TimestampedValue>,

    /// Health metrics
    pub last_health_check: Option<DateTime<Utc>>,
    pub health_status: QueueHealth,
    pub uptime_percentage: f64,

    /// Alert metrics
    pub active_alerts: Vec<String>,
    pub alert_history: Vec<AlertEvent>,

    /// Metadata
    pub timestamp: DateTime<Utc>,
    pub collection_interval: Duration,
}

impl Default for QueueMetrics {
    fn default() -> Self {
        Self {
            total_messages: 0,
            processed_messages: 0,
            failed_messages: 0,
            dead_letter_messages: 0,
            visible_messages: 0,
            invisible_messages: 0,
            avg_processing_time_ms: 0.0,
            messages_per_second: 0.0,
            error_rate: 0.0,
            success_rate: 100.0,
            queue_depth_trend: Vec::new(),
            processing_rate_trend: Vec::new(),
            error_rate_trend: Vec::new(),
            last_health_check: None,
            health_status: QueueHealth::Healthy,
            uptime_percentage: 100.0,
            active_alerts: Vec::new(),
            alert_history: Vec::new(),
            timestamp: Utc::now(),
            collection_interval: Duration::from_secs(30),
        }
    }
}

impl QueueMetrics {
    /// Update metrics from queue statistics
    pub fn update_from_stats(&mut self, stats: &QueueStats) {
        self.total_messages = stats.total_messages;
        self.processed_messages = stats.total_processed;
        self.failed_messages = stats.total_failed;
        self.dead_letter_messages = stats.dead_letter_messages;
        self.visible_messages = stats.visible_messages;
        self.invisible_messages = stats.invisible_messages;

        // Calculate rates
        let total_attempts = self.processed_messages + self.failed_messages;
        if total_attempts > 0 {
            self.success_rate = (self.processed_messages as f64 / total_attempts as f64) * 100.0;
            self.error_rate = (self.failed_messages as f64 / total_attempts as f64) * 100.0;
        }
    }

    /// Add timestamped value to trend data
    pub fn add_trend_point(&mut self, trend: &mut Vec<TimestampedValue>, value: f64) {
        trend.push(TimestampedValue {
            timestamp: Utc::now(),
            value,
        });

        // Keep only last 100 points
        if trend.len() > 100 {
            trend.remove(0);
        }
    }

    /// Update all trend data
    pub fn update_trends(&mut self) {
        let queue_depth = self.total_messages as f64;
        let processing_rate = self.messages_per_second;
        let error_rate = self.error_rate;

        Self::add_trend_point_static(&mut self.queue_depth_trend, queue_depth);
        Self::add_trend_point_static(&mut self.processing_rate_trend, processing_rate);
        Self::add_trend_point_static(&mut self.error_rate_trend, error_rate);
    }

    /// Static version for use in update_trends to avoid borrowing issues
    fn add_trend_point_static(trend: &mut Vec<TimestampedValue>, value: f64) {
        trend.push(TimestampedValue {
            timestamp: Utc::now(),
            value,
        });

        // Keep only last 100 points
        if trend.len() > 100 {
            trend.remove(0);
        }
    }

    /// Get current health score (0-100)
    pub fn health_score(&self) -> f64 {
        let mut score = 100.0;

        // Deduct for errors
        score -= self.error_rate * 2.0; // Each 1% error reduces score by 2 points

        // Deduct for queue depth (if too high)
        if self.total_messages > 1000 {
            score -= (self.total_messages - 1000) as f64 / 100.0;
        }

        // Add health status bonus
        match self.health_status {
            QueueHealth::Healthy => score += 0.0,
            QueueHealth::Degraded => score -= 20.0,
            QueueHealth::Unhealthy => score -= 50.0,
        }

        score.clamp(0.0, 100.0)
    }

    /// Check if metrics indicate a healthy state
    pub fn is_healthy(&self) -> bool {
        self.health_score() >= 70.0
            && self.error_rate < 10.0
            && self.health_status == QueueHealth::Healthy
    }
}

/// Timestamped value for trend tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampedValue {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
}

/// Alert severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Info => write!(f, "INFO"),
            AlertSeverity::Warning => write!(f, "WARNING"),
            AlertSeverity::Error => write!(f, "ERROR"),
            AlertSeverity::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Alert event for tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEvent {
    pub id: String,
    pub title: String,
    pub message: String,
    pub severity: AlertSeverity,
    pub timestamp: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub metadata: HashMap<String, String>,
}

impl AlertEvent {
    /// Create new alert
    pub fn new(
        title: String,
        message: String,
        severity: AlertSeverity,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title,
            message,
            severity,
            timestamp: Utc::now(),
            resolved_at: None,
            metadata: HashMap::new(),
        }
    }

    /// Resolve the alert
    pub fn resolve(&mut self) {
        self.resolved_at = Some(Utc::now());
    }

    /// Check if alert is active
    pub fn is_active(&self) -> bool {
        self.resolved_at.is_none()
    }
}

/// Alert configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    pub max_queue_size: u64,
    pub max_error_rate: f64,
    pub max_processing_time_ms: f64,
    pub min_messages_per_second: f64,
    pub max_dead_letter_messages: u64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            max_queue_size: 1000,
            max_error_rate: 5.0, // 5%
            max_processing_time_ms: 5000.0, // 5 seconds
            min_messages_per_second: 1.0,
            max_dead_letter_messages: 10,
        }
    }
}

/// Queue monitor service
pub struct QueueMonitorService {
    queue: Arc<dyn QueueService + Send + Sync>,
    metrics: Arc<RwLock<QueueMetrics>>,
    thresholds: AlertThresholds,
    alert_callbacks: Vec<Arc<dyn AlertCallback + Send + Sync>>,
}

/// Alert callback trait for custom alert handling
pub trait AlertCallback: Send + Sync {
    fn on_alert(&self, alert: &AlertEvent);
    fn on_alert_resolved(&self, alert: &AlertEvent);
}


impl QueueMonitorService {
    /// Create new queue monitor
    pub fn new(
        queue: Arc<dyn QueueService + Send + Sync>,
        thresholds: AlertThresholds,
    ) -> Self {
        Self {
            queue,
            metrics: Arc::new(RwLock::new(QueueMetrics::default())),
            thresholds,
            alert_callbacks: Vec::new(),
        }
    }

    /// Add alert callback
    pub fn add_alert_callback(&mut self, callback: Arc<dyn AlertCallback + Send + Sync>) {
        self.alert_callbacks.push(callback);
    }

    /// Get current metrics snapshot
    pub async fn get_metrics(&self) -> QueueMetrics {
        self.metrics.read().await.clone()
    }

    /// Update metrics from queue statistics
    pub async fn update_metrics(&self) -> Result<(), crate::QueueError> {
        let stats = self.queue.get_stats().await?;

        {
            let mut metrics = self.metrics.write().await;
            metrics.update_from_stats(&stats);
            metrics.update_trends();
        }

        Ok(())
    }

    /// Perform health check
    pub async fn health_check(&self) -> Result<QueueHealthCheck, crate::QueueError> {
        let health = self.queue.health_check().await?;

        {
            let mut metrics = self.metrics.write().await;
            metrics.health_status = health.status;
            metrics.last_health_check = Some(Utc::now());
        }

        Ok(health)
    }

    /// Static method to check alerts (used in monitoring loop)
    async fn check_alerts_static(
        metrics: &QueueMetrics,
        thresholds: &AlertThresholds,
    ) -> Vec<AlertEvent> {
        let mut alerts = Vec::new();

        // Queue size alert
        if metrics.total_messages > thresholds.max_queue_size {
            alerts.push(AlertEvent::new(
                "Queue Size Alert".to_string(),
                format!("Queue size {} exceeds threshold {}", metrics.total_messages, thresholds.max_queue_size),
                AlertSeverity::Warning,
            ));
        }

        // Error rate alert
        if metrics.error_rate > thresholds.max_error_rate {
            let severity = if metrics.error_rate > thresholds.max_error_rate * 2.0 {
                AlertSeverity::Critical
            } else {
                AlertSeverity::Error
            };
            alerts.push(AlertEvent::new(
                "Error Rate Alert".to_string(),
                format!("Error rate {:.2}% exceeds threshold {:.2}%", metrics.error_rate * 100.0, thresholds.max_error_rate * 100.0),
                severity,
            ));
        }

        // Processing time alert
        if metrics.avg_processing_time_ms > thresholds.max_processing_time_ms {
            alerts.push(AlertEvent::new(
                "Processing Time Alert".to_string(),
                format!("Average processing time {:.2}ms exceeds threshold {:.2}ms", metrics.avg_processing_time_ms, thresholds.max_processing_time_ms),
                AlertSeverity::Warning,
            ));
        }

        // Processing rate alert
        if metrics.messages_per_second < thresholds.min_messages_per_second {
            alerts.push(AlertEvent::new(
                "Low Processing Rate Alert".to_string(),
                format!("Processing rate {:.2} msg/sec below threshold {:.2} msg/sec", metrics.messages_per_second, thresholds.min_messages_per_second),
                AlertSeverity::Warning,
            ));
        }

        // Dead letter alert
        if metrics.dead_letter_messages > thresholds.max_dead_letter_messages {
            alerts.push(AlertEvent::new(
                "Dead Letter Alert".to_string(),
                format!("Dead letter messages {} exceeds threshold {}", metrics.dead_letter_messages, thresholds.max_dead_letter_messages),
                AlertSeverity::Error,
            ));
        }

        alerts
    }

    /// Check for alerts based on current metrics
    pub async fn check_alerts(&self) -> Vec<AlertEvent> {
        let metrics = self.metrics.read().await.clone();
        let mut alerts = Vec::new();

        // Queue size alert
        if metrics.total_messages > self.thresholds.max_queue_size {
            alerts.push(AlertEvent::new(
                "Queue Size Alert".to_string(),
                format!("Queue size exceeded threshold: {} > {}",
                        metrics.total_messages, self.thresholds.max_queue_size),
                AlertSeverity::Warning,
            ));
        }

        // Error rate alert
        if metrics.error_rate > self.thresholds.max_error_rate {
            let severity = if metrics.error_rate > 20.0 {
                AlertSeverity::Critical
            } else if metrics.error_rate > 10.0 {
                AlertSeverity::Error
            } else {
                AlertSeverity::Warning
            };

            alerts.push(AlertEvent::new(
                "Error Rate Alert".to_string(),
                format!("Error rate exceeded threshold: {:.1}% > {:.1}%",
                        metrics.error_rate, self.thresholds.max_error_rate),
                severity,
            ));
        }

        // Processing time alert
        if metrics.avg_processing_time_ms > self.thresholds.max_processing_time_ms {
            alerts.push(AlertEvent::new(
                "Processing Time Alert".to_string(),
                format!("Average processing time exceeded threshold: {:.1}ms > {:.1}ms",
                        metrics.avg_processing_time_ms, self.thresholds.max_processing_time_ms),
                AlertSeverity::Warning,
            ));
        }

        // Low processing rate alert
        if metrics.messages_per_second < self.thresholds.min_messages_per_second {
            alerts.push(AlertEvent::new(
                "Low Processing Rate Alert".to_string(),
                format!("Processing rate below threshold: {:.2} < {:.2}",
                        metrics.messages_per_second, self.thresholds.min_messages_per_second),
                AlertSeverity::Info,
            ));
        }

        // Dead letter queue alert
        if metrics.dead_letter_messages > self.thresholds.max_dead_letter_messages {
            alerts.push(AlertEvent::new(
                "Dead Letter Queue Alert".to_string(),
                format!("Dead letter messages exceeded threshold: {} > {}",
                        metrics.dead_letter_messages, self.thresholds.max_dead_letter_messages),
                AlertSeverity::Error,
            ));
        }

        // Health status alert
        if metrics.health_status != QueueHealth::Healthy {
            alerts.push(AlertEvent::new(
                "Health Status Alert".to_string(),
                format!("Queue health status: {:?}", metrics.health_status),
                AlertSeverity::Warning,
            ));
        }

        // Trigger callbacks
        for alert in &alerts {
            for callback in &self.alert_callbacks {
                callback.on_alert(alert);
            }
        }

        alerts
    }

    /// Start continuous monitoring
    pub async fn start_monitoring(
        &self,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        let queue = self.queue.clone();
        let metrics = self.metrics.clone();
        let thresholds = self.thresholds.clone();
        let _alert_callbacks = self.alert_callbacks.clone();

        tokio::spawn(async move {
            let mut last_stats: Option<QueueStats> = None;
            let mut start_time = Instant::now();

            loop {
                tokio::time::sleep(interval).await;

                // Update metrics
                if let Ok(current_stats) = queue.get_stats().await {
                    let now = Instant::now();
                    let time_delta = now.duration_since(start_time).as_secs_f64();

                    // Update metrics in write scope
                    {
                        let mut metrics_guard = metrics.write().await;
                        metrics_guard.update_from_stats(&current_stats);
                        metrics_guard.update_trends();

                        // Calculate processing rate
                        if let Some(ref last_stats) = last_stats {
                            let processed_delta = current_stats.total_processed.saturating_sub(last_stats.total_processed);
                            let messages_per_second = processed_delta as f64 / time_delta;
                            metrics_guard.messages_per_second = messages_per_second;
                        }
                    } // metrics_guard is dropped here

                    // Check alerts using current metrics (separate read scope)
                    let alerts = {
                        let current_metrics = metrics.read().await;
                        Self::check_alerts_static(&current_metrics, &thresholds).await
                    };

                    if !alerts.is_empty() {
                        warn!("Generated {} alerts", alerts.len());
                        for alert in &alerts {
                            info!("ALERT: {} - {}", alert.title, alert.message);
                        }
                    }

                    // Update active alerts (final write scope)
                    {
                        let mut metrics_guard = metrics.write().await;
                        metrics_guard.active_alerts = alerts.iter()
                            .filter(|a| a.is_active())
                            .map(|a| format!("{}: {}", a.severity, a.title))
                            .collect();
                    }

                    last_stats = Some(current_stats);
                    start_time = now;
                }
            }
        })
    }

    /// Generate metrics report
    pub async fn generate_report(&self) -> MetricsReport {
        let metrics = self.metrics.read().await.clone();

        MetricsReport {
            summary: MetricsSummary {
                total_messages: metrics.total_messages,
                processed_messages: metrics.processed_messages,
                failed_messages: metrics.failed_messages,
                dead_letter_messages: metrics.dead_letter_messages,
                success_rate: metrics.success_rate,
                error_rate: metrics.error_rate,
                health_score: metrics.health_score(),
                is_healthy: metrics.is_healthy(),
            },
            performance: PerformanceMetrics {
                messages_per_second: metrics.messages_per_second,
                avg_processing_time_ms: metrics.avg_processing_time_ms,
                queue_depth: metrics.total_messages,
                visible_messages: metrics.visible_messages,
                invisible_messages: metrics.invisible_messages,
            },
            trends: TrendMetrics {
                queue_depth_trend: metrics.queue_depth_trend.clone(),
                processing_rate_trend: metrics.processing_rate_trend.clone(),
                error_rate_trend: metrics.error_rate_trend.clone(),
            },
            health: HealthMetrics {
                status: metrics.health_status,
                last_check: metrics.last_health_check,
                uptime_percentage: metrics.uptime_percentage,
                active_alerts_count: metrics.active_alerts.len(),
            },
            alerts: metrics.alert_history.clone(),
            generated_at: Utc::now(),
        }
    }
}

/// Comprehensive metrics report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsReport {
    pub summary: MetricsSummary,
    pub performance: PerformanceMetrics,
    pub trends: TrendMetrics,
    pub health: HealthMetrics,
    pub alerts: Vec<AlertEvent>,
    pub generated_at: DateTime<Utc>,
}

/// Summary metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub total_messages: u64,
    pub processed_messages: u64,
    pub failed_messages: u64,
    pub dead_letter_messages: u64,
    pub success_rate: f64,
    pub error_rate: f64,
    pub health_score: f64,
    pub is_healthy: bool,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub messages_per_second: f64,
    pub avg_processing_time_ms: f64,
    pub queue_depth: u64,
    pub visible_messages: u64,
    pub invisible_messages: u64,
}

/// Trend metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendMetrics {
    pub queue_depth_trend: Vec<TimestampedValue>,
    pub processing_rate_trend: Vec<TimestampedValue>,
    pub error_rate_trend: Vec<TimestampedValue>,
}

/// Health metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetrics {
    pub status: QueueHealth,
    pub last_check: Option<DateTime<Utc>>,
    pub uptime_percentage: f64,
    pub active_alerts_count: usize,
}

/// Console alert callback for logging
pub struct ConsoleAlertCallback;

impl AlertCallback for ConsoleAlertCallback {
    fn on_alert(&self, alert: &AlertEvent) {
        match alert.severity {
            AlertSeverity::Info => info!("📊 ALERT: {} - {}", alert.title, alert.message),
            AlertSeverity::Warning => warn!("⚠️  ALERT: {} - {}", alert.title, alert.message),
            AlertSeverity::Error => error!("❌ ALERT: {} - {}", alert.title, alert.message),
            AlertSeverity::Critical => error!("🚨 ALERT: {} - {}", alert.title, alert.message),
        }
    }

    fn on_alert_resolved(&self, alert: &AlertEvent) {
        info!("✅ ALERT RESOLVED: {} - {}", alert.title, alert.message);
    }
}

/// Webhook alert callback for external notifications
pub struct WebhookAlertCallback {
    pub webhook_url: String,
    pub client: reqwest::Client,
}

impl WebhookAlertCallback {
    pub fn new(webhook_url: String) -> Self {
        Self {
            webhook_url,
            client: reqwest::Client::new(),
        }
    }
}

impl AlertCallback for WebhookAlertCallback {
    fn on_alert(&self, alert: &AlertEvent) {
        let webhook_url = self.webhook_url.clone();
        let client = self.client.clone();
        let alert = alert.clone();

        tokio::spawn(async move {
            if client
                .post(&webhook_url)
                .json(&alert)
                .send()
                .await
                .is_ok()
            {
                debug!("Alert webhook sent successfully");
            } else {
                warn!("Failed to send alert webhook");
            }
        });
    }

    fn on_alert_resolved(&self, alert: &AlertEvent) {
        // Similar implementation for resolved alerts
        info!("Webhook alert resolved: {}", alert.title);
    }
}

