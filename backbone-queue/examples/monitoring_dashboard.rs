//! Queue Monitoring Dashboard Example
//!
//! This example demonstrates how to build a comprehensive monitoring system
//! for queue operations with real-time metrics, alerts, and health checks.

use backbone_queue::{
    QueueService,
    redis::RedisQueueBuilder,
    types::{QueueMessage, QueuePriority, QueueHealth}
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;

/// Monitoring metrics
#[derive(Debug, Default)]
struct QueueMetrics {
    // Basic queue metrics
    total_messages: u64,
    processed_messages: u64,
    failed_messages: u64,
    dead_letter_messages: u64,

    // Performance metrics
    avg_processing_time: f64,
    messages_per_second: f64,
    error_rate: f64,

    // Health metrics
    last_health_check: Option<Instant>,
    health_status: QueueHealth,

    // Time series data
    timestamp: Instant,
    queue_size_history: Vec<(Instant, u64)>,
    processing_rate_history: Vec<(Instant, f64)>,
    error_rate_history: Vec<(Instant, f64)>,
}

impl QueueMetrics {
    fn new() -> Self {
        Self {
            timestamp: Instant::now(),
            queue_size_history: Vec::new(),
            processing_rate_history: Vec::new(),
            error_rate_history: Vec::new(),
            ..Default::default()
        }
    }

    fn update_from_stats(&mut self, stats: &backbone_queue::QueueStats) {
        self.total_messages = stats.total_messages;
        self.processed_messages = stats.total_processed;
        self.failed_messages = stats.total_failed;
        self.dead_letter_messages = stats.dead_letter_messages;
    }

    fn add_queue_size_point(&mut self, size: u64) {
        let now = Instant::now();
        self.queue_size_history.push((now, size));

        // Keep only last 100 points
        if self.queue_size_history.len() > 100 {
            self.queue_size_history.remove(0);
        }
    }

    fn add_processing_rate_point(&mut self, rate: f64) {
        let now = Instant::now();
        self.processing_rate_history.push((now, rate));

        // Keep only last 100 points
        if self.processing_rate_history.len() > 100 {
            self.processing_rate_history.remove(0);
        }
    }

    fn add_error_rate_point(&mut self, rate: f64) {
        let now = Instant::now();
        self.error_rate_history.push((now, rate));

        // Keep only last 100 points
        if self.error_rate_history.len() > 100 {
            self.error_rate_history.remove(0);
        }
    }
}

/// Alert configuration
#[derive(Debug, Clone)]
struct AlertThresholds {
    max_queue_size: u64,
    max_error_rate: f64,
    max_processing_time: f64,
    min_messages_per_second: f64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            max_queue_size: 1000,
            max_error_rate: 0.1, // 10%
            max_processing_time: 5000.0, // 5 seconds
            min_messages_per_second: 1.0,
        }
    }
}

/// Monitoring dashboard
struct MonitoringDashboard {
    queue: Arc<dyn QueueService + Send + Sync>,
    metrics: Arc<RwLock<QueueMetrics>>,
    alert_thresholds: AlertThresholds,
    active_alerts: Arc<RwLock<Vec<String>>>,
}

impl MonitoringDashboard {
    fn new(
        queue: Arc<dyn QueueService + Send + Sync>,
        alert_thresholds: AlertThresholds,
    ) -> Self {
        Self {
            queue,
            metrics: Arc::new(RwLock::new(QueueMetrics::new())),
            alert_thresholds,
            active_alerts: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start the monitoring dashboard
    async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🖥️  Starting Queue Monitoring Dashboard");
        println!("=====================================");

        let queue = self.queue.clone();
        let metrics = self.metrics.clone();
        let alert_thresholds = self.alert_thresholds.clone();
        let active_alerts = self.active_alerts.clone();

        // Metrics collection task
        let metrics_handle = tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            let mut last_processed = 0u64;
            let mut last_failed = 0u64;
            let mut last_timestamp = Instant::now();

            loop {
                interval.tick().await;

                // Collect queue statistics
                if let Ok(stats) = queue.get_stats().await {
                    let now = Instant::now();
                    let time_delta = now.duration_since(last_timestamp).as_secs_f64();

                    // Update metrics
                    {
                        let mut metrics = metrics.write().await;
                        metrics.update_from_stats(&stats);
                        metrics.add_queue_size_point(stats.total_messages);

                        // Calculate processing rate
                        let processed_delta = stats.total_processed.saturating_sub(last_processed);
                        let failed_delta = stats.total_failed.saturating_sub(last_failed);
                        let total_delta = processed_delta + failed_delta;

                        let messages_per_second = if time_delta > 0.0 {
                            total_delta as f64 / time_delta
                        } else {
                            0.0
                        };

                        let error_rate = if total_delta > 0 {
                            failed_delta as f64 / total_delta as f64
                        } else {
                            0.0
                        };

                        metrics.add_processing_rate_point(messages_per_second);
                        metrics.add_error_rate_point(error_rate);
                        metrics.messages_per_second = messages_per_second;
                        metrics.error_rate = error_rate;

                        last_processed = stats.total_processed;
                        last_failed = stats.total_failed;
                        last_timestamp = now;
                    }

                    // Check for alerts
                    Self::check_alerts(
                        &stats,
                        messages_per_second,
                        error_rate,
                        &alert_thresholds,
                        &active_alerts,
                    ).await;
                }

                // Perform health check
                if let Ok(health) = queue.health_check().await {
                    {
                        let mut metrics = metrics.write().await;
                        metrics.health_status = health.status;
                        metrics.last_health_check = Some(Instant::now());
                    }
                }
            }
        });

        // Dashboard display task
        let display_handle = tokio::spawn({
            let metrics = self.metrics.clone();
            let active_alerts = self.active_alerts.clone();
            async move {
                let mut interval = interval(Duration::from_secs(10));

                loop {
                    interval.tick().await;

                    {
                        let metrics = metrics.read().await;
                        let alerts = active_alerts.read().await;

                        Self::display_dashboard(&metrics, &alerts);
                    }
                }
            }
        });

        // Health check task
        let health_handle = tokio::spawn({
            let queue = self.queue.clone();
            let metrics = self.metrics.clone();
            async move {
                let mut interval = interval(Duration::from_secs(30));

                loop {
                    interval.tick().await;

                    if let Ok(health) = queue.health_check().await {
                        {
                            let mut metrics = metrics.write().await;
                            metrics.health_status = health.status;
                            metrics.last_health_check = Some(Instant::now());
                        }

                        if health.status != QueueHealth::Healthy {
                            println!("⚠️  Health Check Alert: Queue status is {:?}", health.status);
                        }
                    }
                }
            }
        });

        // Handle shutdown
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\n🛑 Shutting down monitoring dashboard...");
            }
        }

        // Cancel tasks
        metrics_handle.abort();
        display_handle.abort();
        health_handle.abort();

        println!("✅ Monitoring dashboard stopped");
        Ok(())
    }

    /// Check for alerts based on thresholds
    async fn check_alerts(
        stats: &backbone_queue::QueueStats,
        messages_per_second: f64,
        error_rate: f64,
        thresholds: &AlertThresholds,
        active_alerts: &Arc<RwLock<Vec<String>>>,
    ) {
        let mut alerts = Vec::new();

        // Queue size alert
        if stats.total_messages > thresholds.max_queue_size {
            alerts.push(format!(
                "🚨 Queue size alert: {} messages (threshold: {})",
                stats.total_messages, thresholds.max_queue_size
            ));
        }

        // Error rate alert
        if error_rate > thresholds.max_error_rate {
            alerts.push(format!(
                "🚨 Error rate alert: {:.1}% (threshold: {:.1}%)",
                error_rate * 100.0, thresholds.max_error_rate * 100.0
            ));
        }

        // Processing rate alert
        if messages_per_second < thresholds.min_messages_per_second {
            alerts.push(format!(
                "🚨 Low processing rate alert: {:.2} msg/sec (threshold: {:.2} msg/sec)",
                messages_per_second, thresholds.min_messages_per_second
            ));
        }

        // Dead letter queue alert
        if stats.dead_letter_messages > 0 {
            alerts.push(format!(
                "🚨 Dead letter queue alert: {} messages in DLQ",
                stats.dead_letter_messages
            ));
        }

        // Update active alerts
        {
            let mut active = active_alerts.write().await;
            if !alerts.is_empty() {
                *active = alerts;
                for alert in &*active {
                    println!("{}", alert);
                }
            } else if !active.is_empty() {
                active.clear();
                println!("✅ All alerts cleared");
            }
        }
    }

    /// Display the monitoring dashboard
    fn display_dashboard(metrics: &QueueMetrics, alerts: &[String]) {
        // Clear screen (platform-specific)
        print!("\x1B[2J\x1B[1;1H");

        println!("🖥️  Queue Monitoring Dashboard");
        println!("=============================");
        println!("⏰ Last updated: {:?}", Instant::now().duration_since(metrics.timestamp));

        if let Some(last_check) = metrics.last_health_check {
            println!("🏥 Last health check: {:?} ago", Instant::now().duration_since(last_check));
        }

        // Health status
        let health_icon = match metrics.health_status {
            QueueHealth::Healthy => "🟢",
            QueueHealth::Degraded => "🟡",
            QueueHealth::Unhealthy => "🔴",
        };
        println!("🏥 Health Status: {} {:?}", health_icon, metrics.health_status);

        println!();

        // Queue metrics
        println!("📊 Queue Metrics:");
        println!("  📦 Total messages: {}", metrics.total_messages);
        println!("  ✅ Processed: {}", metrics.processed_messages);
        println!("  ❌ Failed: {}", metrics.failed_messages);
        println!("  💀 Dead letter: {}", metrics.dead_letter_messages);

        // Calculate success rate
        let total_attempts = metrics.processed_messages + metrics.failed_messages;
        if total_attempts > 0 {
            let success_rate = metrics.processed_messages as f64 / total_attempts as f64 * 100.0;
            println!("  📈 Success rate: {:.1}%", success_rate);
        }

        println!();

        // Performance metrics
        println!("⚡ Performance:");
        println!("  🚀 Messages/sec: {:.2}", metrics.messages_per_second);
        println!("  ⚠️  Error rate: {:.1}%", metrics.error_rate * 100.0);
        println!("  ⏱️  Avg processing time: {:.1}ms", metrics.avg_processing_time);

        println!();

        // Alerts
        if !alerts.is_empty() {
            println!("🚨 Active Alerts:");
            for alert in alerts {
                println!("  {}", alert);
            }
            println!();
        } else {
            println!("✅ No active alerts");
            println!();
        }

        // Simple text graphs (last 20 data points)
        let graph_points = 20;

        // Queue size graph
        if !metrics.queue_size_history.is_empty() {
            println!("📈 Queue Size (last {} points):", graph_points.min(metrics.queue_size_history.len()));
            Self::print_simple_graph(
                &metrics.queue_size_history,
                graph_points,
                "Size"
            );
            println!();
        }

        // Processing rate graph
        if !metrics.processing_rate_history.is_empty() {
            println!("📊 Processing Rate (last {} points):", graph_points.min(metrics.processing_rate_history.len()));
            Self::print_simple_graph(
                &metrics.processing_rate_history,
                graph_points,
                "Rate"
            );
            println!();
        }

        // Error rate graph
        if !metrics.error_rate_history.is_empty() {
            println!("📉 Error Rate (last {} points):", graph_points.min(metrics.error_rate_history.len()));
            Self::print_simple_graph(
                &metrics.error_rate_history,
                graph_points,
                "Error%"
            );
            println!();
        }
    }

    /// Print a simple text graph
    fn print_simple_graph<T>(data: &[(Instant, T)], max_points: usize, label: &str)
    where
        T: PartialOrd + Copy,
    {
        if data.is_empty() {
            return;
        }

        let step = if data.len() > max_points {
            data.len() / max_points
        } else {
            1
        };

        let mut graph_data = Vec::new();
        for (i, &(_, value)) in data.iter().enumerate() {
            if i % step == 0 || i == data.len() - 1 {
                graph_data.push(value);
            }
        }

        if graph_data.is_empty() {
            return;
        }

        let max_value = *graph_data.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
        let min_value = *graph_data.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
        let range = max_value - min_value;

        println!("  Min: {:?}, Max: {:?}", min_value, max_value);
        print!("  ");

        for &value in &graph_data {
            let normalized = if range > 0 {
                ((value - min_value) as f64 / range as f64 * 20.0) as usize
            } else {
                10
            };

            print!("█");
            for _ in 0..(20 - normalized) {
                print!(" ");
            }
            print!("│");
        }
        println!();
        print!("  ");
        for _ in &graph_data {
            print!("─");
        }
        println!();
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🚀 Queue Monitoring Dashboard Example");
    println!("=====================================");

    // Create Redis queue
    println!("📡 Setting up Redis queue...");
    let queue = Arc::new(
        RedisQueueBuilder::new()
            .url("redis://localhost:6379")
            .queue_name("monitoring_queue")
            .key_prefix("monitor")
            .pool_size(10)
            .build()
            .await?
    );

    // Test connection
    if !queue.test_connection().await? {
        eprintln!("❌ Failed to connect to Redis");
        return Ok(());
    }

    // Configure alert thresholds
    let alert_thresholds = AlertThresholds {
        max_queue_size: 500,
        max_error_rate: 0.05, // 5%
        max_processing_time: 2000.0, // 2 seconds
        min_messages_per_second: 0.5,
    };

    // Create monitoring dashboard
    let dashboard = MonitoringDashboard::new(queue, alert_thresholds);

    // Start monitoring
    dashboard.start().await?;

    println!("\n🎉 Monitoring dashboard example completed!");
    Ok(())
}