//! Advanced Queue Monitoring Example
//!
//! This example demonstrates the full monitoring capabilities of the Backbone Queue module:
//! - Real-time metrics collection
//! - Alert system with multiple callback types
//! - Comprehensive monitoring dashboard
//! - Performance analysis and trend tracking
//! - Health monitoring and alerting

use backbone_queue::{
    QueueService,
    redis::RedisQueueBuilder,
    monitoring::{
        QueueMonitor, QueueMetrics, AlertEvent, AlertSeverity, AlertThresholds,
        ConsoleAlertCallback, WebhookAlertCallback, MetricsReport
    },
    types::{QueueMessage, QueuePriority}
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, sleep};
use tracing::{info, warn, error, Level};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Custom alert callback for database logging
struct DatabaseAlertCallback;

impl backbone_queue::monitoring::AlertCallback for DatabaseAlertCallback {
    fn on_alert(&self, alert: &AlertEvent) {
        // Simulate database storage
        info!("🗄️  DB ALERT: [{}] {} - {}",
              alert.severity, alert.title, alert.message);
        info!("🗄️  Stored alert with ID: {}", alert.id);
    }

    fn on_alert_resolved(&self, alert: &AlertEvent) {
        info!("🗄️  DB ALERT RESOLVED: {} - {}", alert.title, alert.id);
    }
}

/// Alert aggregation system
struct AlertAggregator {
    alerts: Arc<tokio::sync::RwLock<Vec<AlertEvent>>>,
    aggregation_window: Duration,
}

impl AlertAggregator {
    fn new(aggregation_window: Duration) -> Self {
        Self {
            alerts: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            aggregation_window,
        }
    }

    async fn add_alert(&self, alert: AlertEvent) {
        let mut alerts = self.alerts.write().await;
        alerts.push(alert.clone());

        // Trigger aggregation if needed
        if alerts.len() % 5 == 0 {
            self.aggregate_alerts().await;
        }
    }

    async fn aggregate_alerts(&self) {
        let alerts = self.alerts.read().await;

        let severity_counts = HashMap::new();
        let mut recent_alerts = Vec::new();

        // Get recent alerts within aggregation window
        let cutoff = chrono::Utc::now() - chrono::Duration::from_std(self.aggregation_window);

        for alert in alerts.iter() {
            if alert.timestamp > cutoff {
                recent_alerts.push(alert.clone());
                *severity_counts.entry(alert.severity).or_insert(0) += 1;
            }
        }

        info!("📊 Alert Aggregation Summary:");
        for (severity, count) in severity_counts {
            info!("  {:?}: {} alerts", severity, count);
        }

        // Clean old alerts
        drop(alerts);
        let mut alerts = self.alerts.write().await;
        *alerts = recent_alerts;
    }
}

/// Performance analyzer
struct PerformanceAnalyzer {
    metrics_history: Arc<tokio::sync::RwLock<Vec<QueueMetrics>>>,
    analysis_window: usize,
}

impl PerformanceAnalyzer {
    fn new(analysis_window: usize) -> Self {
        Self {
            metrics_history: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            analysis_window,
        }
    }

    async fn add_metrics(&self, metrics: QueueMetrics) {
        let mut history = self.metrics_history.write().await;
        history.push(metrics);

        // Keep only recent metrics
        if history.len() > self.analysis_window {
            history.remove(0);
        }

        // Trigger analysis if we have enough data
        if history.len() >= 10 {
            self.analyze_performance().await;
        }
    }

    async fn analyze_performance(&self) {
        let history = self.metrics_history.read().await;

        if history.len() < 2 {
            return;
        }

        // Calculate trends
        let mut processing_rates = Vec::new();
        let mut error_rates = Vec::new();
        let mut queue_depths = Vec::new();

        for metrics in history.iter() {
            processing_rates.push(metrics.messages_per_second);
            error_rates.push(metrics.error_rate);
            queue_depths.push(metrics.total_messages as f64);
        }

        // Calculate average values
        let avg_processing_rate = processing_rates.iter().sum::<f64>() / processing_rates.len() as f64;
        let avg_error_rate = error_rates.iter().sum::<f64>() / error_rates.len() as f64;
        let avg_queue_depth = queue_depths.iter().sum::<f64>() / queue_depths.len() as f64;

        // Calculate trends (simple linear approximation)
        let processing_rate_trend = if processing_rates.len() >= 3 {
            let recent = processing_rates.iter().rev().take(3).sum::<f64>() / 3.0;
            let older = processing_rates.iter().take(3).sum::<f64>() / 3.0;
            recent - older
        } else {
            0.0
        };

        let error_rate_trend = if error_rates.len() >= 3 {
            let recent = error_rates.iter().rev().take(3).sum::<f64>() / 3.0;
            let older = error_rates.iter().take(3).sum::<f64>() / 3.0;
            recent - older
        } else {
            0.0
        };

        info!("📈 Performance Analysis Results:");
        info!("  📊 Average Processing Rate: {:.2} msg/sec", avg_processing_rate);
        info!("  📊 Average Error Rate: {:.2}%", avg_error_rate);
        info!("  📊 Average Queue Depth: {:.1}", avg_queue_depth);
        info!("  📈 Processing Rate Trend: {:+.2} msg/sec", processing_rate_trend);
        info!("  📈 Error Rate Trend: {:+.2}%", error_rate_trend);

        // Generate insights
        if processing_rate_trend < -5.0 {
            warn!("⚠️  Performance degradation detected - processing rate decreasing");
        } else if processing_rate_trend > 5.0 {
            info!("🚀 Performance improvement detected - processing rate increasing");
        }

        if error_rate_trend > 2.0 {
            warn!("⚠️  Error rate increasing - investigate potential issues");
        } else if error_rate_trend < -2.0 {
            info!("✅ Error rate improving - system stabilizing");
        }
    }
}

/// Metrics exporter for different formats
struct MetricsExporter {
    export_formats: Vec<String>,
    export_directory: String,
}

impl MetricsExporter {
    fn new(export_formats: Vec<String>, export_directory: String) -> Self {
        Self {
            export_formats,
            export_directory,
        }
    }

    async fn export_metrics(&self, report: &MetricsReport) -> Result<(), Box<dyn std::error::Error>> {
        for format in &self.export_formats {
            match format.as_str() {
                "json" => self.export_json(report).await?,
                "csv" => self.export_csv(report).await?,
                "prometheus" => self.export_prometheus(report).await?,
                _ => warn!("Unknown export format: {}", format),
            }
        }
        Ok(())
    }

    async fn export_json(&self, report: &MetricsReport) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string_pretty(report)?;
        let filename = format!("{}/metrics_{}.json",
                                 self.export_directory,
                                 report.generated_at.format("%Y%m%d_%H%M%S"));

        tokio::fs::write(&filename, json).await?;
        info!("📄 Exported metrics to JSON: {}", filename);
        Ok(())
    }

    async fn export_csv(&self, report: &MetricsReport) -> Result<(), Box<dyn std::error::Error>> {
        let mut csv_data = String::new();

        // CSV header
        csv_data.push_str("timestamp,total_messages,processed_messages,failed_messages,success_rate,error_rate,messages_per_second,health_score\n");

        // CSV data row
        csv_data.push_str(&format!(
            "{},{},{},{},{:.2},{:.2},{:.2},{:.1}\n",
            report.generated_at.format("%Y-%m-%d %H:%M:%S"),
            report.summary.total_messages,
            report.summary.processed_messages,
            report.summary.failed_messages,
            report.summary.success_rate,
            report.summary.error_rate,
            report.performance.messages_per_second,
            report.summary.health_score
        ));

        let filename = format!("{}/metrics_{}.csv",
                                 self.export_directory,
                                 report.generated_at.format("%Y%m%d_%H%M%S"));

        tokio::fs::write(&filename, csv_data).await?;
        info!("📊 Exported metrics to CSV: {}", filename);
        Ok(())
    }

    async fn export_prometheus(&self, report: &MetricsReport) -> Result<(), Box<dyn std::error::Error>> {
        let mut prometheus_data = String::new();

        // Prometheus metrics format
        prometheus_data.push_str("# HELP queue_total_messages Total number of messages in queue\n");
        prometheus_data.push_str("# TYPE queue_total_messages gauge\n");
        prometheus_data.push_str(&format!(
            "queue_total_messages {}\n",
            report.summary.total_messages
        ));

        prometheus_data.push_str("# HELP queue_processed_messages Total processed messages\n");
        prometheus_data.push_str("# TYPE queue_processed_messages counter\n");
        prometheus_data.push_str(&format!(
            "queue_processed_messages {}\n",
            report.summary.processed_messages
        ));

        prometheus_data.push_str("# HELP queue_error_rate Error rate percentage\n");
        prometheus_data.push_str("# TYPE queue_error_rate gauge\n");
        prometheus_data.push_str(&format!(
            "queue_error_rate {:.2}\n",
            report.summary.error_rate
        ));

        prometheus_data.push_str("# HELP queue_health_score Health score (0-100)\n");
        prometheus_data.push_str("# TYPE queue_health_score gauge\n");
        prometheus_data.push_str(&format!(
            "queue_health_score {:.1}\n",
            report.summary.health_score
        ));

        let filename = format!("{}/metrics_{}.prometheus",
                                 self.export_directory,
                                 report.generated_at.format("%Y%m%d_%H%M%S"));

        tokio::fs::write(&filename, prometheus_data).await?;
        info!("📊 Exported metrics to Prometheus: {}", filename);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing subscriber
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();

    println!("🚀 Advanced Queue Monitoring Example");
    println!("===================================");
    println!("This example demonstrates comprehensive monitoring capabilities");
    println!();

    // Create Redis queue
    println!("📡 Setting up Redis queue...");
    let queue = Arc::new(
        RedisQueueBuilder::new()
            .url("redis://localhost:6379")
            .queue_name("advanced_monitoring_queue")
            .key_prefix("monitoring")
            .pool_size(20)
            .build()
            .await?
    );

    // Test connection
    if !queue.test_connection().await? {
        eprintln!("❌ Failed to connect to Redis");
        return Ok(());
    }

    // Clear existing messages
    println!("🧹 Clearing existing messages...");
    queue.purge().await?;

    // Configure alert thresholds
    let alert_thresholds = AlertThresholds {
        max_queue_size: 500,
        max_error_rate: 3.0, // 3%
        max_processing_time_ms: 2000.0, // 2 seconds
        min_messages_per_second: 2.0,
        max_dead_letter_messages: 5,
    };

    println!("⚙️  Configured alert thresholds:");
    println!("  - Max queue size: {}", alert_thresholds.max_queue_size);
    println!("  - Max error rate: {:.1}%", alert_thresholds.max_error_rate);
    println!("  - Max processing time: {:.1}ms", alert_thresholds.max_processing_time_ms);
    println!("  - Min processing rate: {:.1} msg/sec", alert_thresholds.min_messages_per_second);

    // Create monitoring components
    let monitor = Arc::new(QueueMonitor::new(queue.clone(), alert_thresholds));

    // Add alert callbacks
    let mut monitor_mut = Arc::try_unwrap(Arc::into_inner(monitor))?; // This is just for the example
    monitor_mut.add_alert_callback(Box::new(ConsoleAlertCallback));
    monitor_mut.add_alert_callback(Box::new(WebhookAlertCallback::new(
        "https://hooks.slack.com/services/YOUR/WEBHOOK/URL".to_string()
    )));
    monitor_mut.add_alert_callback(Box::new(DatabaseAlertCallback));

    let monitor = Arc::new(monitor_mut);

    // Create additional monitoring components
    let alert_aggregator = AlertAggregator::new(Duration::from_secs(60));
    let performance_analyzer = PerformanceAnalyzer::new(20);
    let metrics_exporter = MetricsExporter::new(
        vec!["json".to_string(), "csv".to_string(), "prometheus".to_string()],
        "./metrics_exports".to_string(),
    );

    // Create export directory
    tokio::fs::create_dir_all("./metrics_exports").await?;

    // Start continuous monitoring
    println!("📊 Starting continuous monitoring...");
    let monitoring_handle = monitor.start_monitoring(Duration::from_secs(5));

    // Start metrics collection and analysis
    let metrics_handle = {
        let monitor = monitor.clone();
        let alert_aggregator = alert_aggregator.clone();
        let performance_analyzer = performance_analyzer.clone();
        let exporter = metrics_exporter.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10));
            let mut export_counter = 0;

            loop {
                interval.tick().await;

                // Update metrics
                if let Err(e) = monitor.update_metrics().await {
                    error!("Failed to update metrics: {}", e);
                }

                // Perform health check
                if let Err(e) = monitor.health_check().await {
                    error!("Health check failed: {}", e);
                }

                // Check for alerts
                let alerts = monitor.check_alerts().await;
                if !alerts.is_empty() {
                    for alert in alerts {
                        alert_aggregator.add_alert(alert).await;
                    }
                }

                // Get current metrics for analysis
                let metrics = monitor.get_metrics().await;
                performance_analyzer.add_metrics(metrics.clone()).await;

                // Export metrics periodically
                export_counter += 1;
                if export_counter % 6 == 0 { // Every minute
                    let report = monitor.generate_report().await;

                    if let Err(e) = exporter.export_metrics(&report).await {
                        error!("Failed to export metrics: {}", e);
                    } else {
                        info!("📁 Metrics exported successfully");
                    }
                }

                // Print current status
                if export_counter % 3 == 0 { // Every 30 seconds
                    Self::print_monitoring_status(&metrics).await;
                }
            }
        })
    };

    // Generate test traffic
    println!("🚗 Generating test traffic patterns...");
    let traffic_handle = {
        let queue = queue.clone();
        let monitor = monitor.clone();

        tokio::spawn(async move {
            let mut message_counter = 0;
            let mut interval = interval(Duration::from_millis(200));

            loop {
                interval.tick().await;
                message_counter += 1;

                // Create different types of messages
                let (priority, payload) = match message_counter % 10 {
                    0..=3 => (QueuePriority::Normal, format!("Regular task #{}", message_counter)),
                    4..=6 => (QueuePriority::High, format!("High priority task #{}", message_counter)),
                    7 => (QueuePriority::Critical, "CRITICAL: System alert".to_string()),
                    8 => (QueuePriority::Low, format!("Low priority background task #{}", message_counter)),
                    _ => (QueuePriority::Normal, format!("Mixed task #{}", message_counter)),
                };

                // Add some errors occasionally
                if message_counter % 20 == 0 {
                    let error_message = QueueMessage::builder()
                        .id("") // Invalid ID to trigger validation error
                        .payload("This will fail validation")
                        .build();

                    if let Err(e) = queue.enqueue(error_message).await {
                        warn!("Expected validation error: {}", e);
                    }
                } else {
                    let message = QueueMessage::builder()
                        .id(format!("msg-{}", message_counter))
                        .payload(payload)
                        .priority(priority)
                        .build();

                    if let Err(e) = queue.enqueue(message).await {
                        error!("Failed to enqueue message: {}", e);
                    }
                }

                // Print progress occasionally
                if message_counter % 50 == 0 {
                    info!("📤 Generated {} test messages", message_counter);
                }

                // Slow down occasionally to create backlog
                if message_counter % 100 == 0 {
                    sleep(Duration::from_secs(2)).await;
                }
            }
        })
    };

    // Dashboard display task
    let dashboard_handle = {
        let monitor = monitor.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(15));

            loop {
                interval.tick().await;

                let metrics = monitor.get_metrics().await;
                let report = monitor.generate_report().await;

                // Clear screen and show dashboard
                print!("\x1B[2J\x1B[1;1H");

                println!("🖥️  Advanced Monitoring Dashboard");
                println!("===============================");
                println!("⏰ Last updated: {}", report.generated_at.format("%Y-%m-%d %H:%M:%S UTC"));

                // Summary section
                println!("\n📊 Summary Metrics:");
                println!("  📦 Total Messages: {}", report.summary.total_messages);
                println!("  ✅ Processed: {}", report.summary.processed_messages);
                println!("  ❌ Failed: {}", report.summary.failed_messages);
                println!("  💀 Dead Letter: {}", report.summary.dead_letter_messages);
                println!("  📈 Success Rate: {:.1}%", report.summary.success_rate);
                println!("  ⚠️  Error Rate: {:.1}%", report.summary.error_rate);
                println!("  ❤️  Health Score: {:.1}/100", report.summary.health_score);
                println!("  🏥 Status: {}", if report.summary.is_healthy { "✅ Healthy" } else { "❌ Unhealthy" });

                // Performance section
                println!("\n⚡ Performance:");
                println!("  🚀 Processing Rate: {:.2} msg/sec", report.performance.messages_per_second);
                println!("  ⏱️  Avg Processing Time: {:.1}ms", report.performance.avg_processing_time_ms);
                println!("  📊 Queue Depth: {}", report.performance.queue_depth);
                println!("  👁️ Visible: {}", report.performance.visible_messages);
                println!("  👻‍♂️ Invisible: {}", report.performance.invisible_messages);

                // Health section
                println!("\n🏥 Health Status:");
                println!("  Status: {:?}", report.health.status);
                if let Some(last_check) = report.health.last_check {
                    println!("  Last Check: {}", last_check.format("%H:%M:%S"));
                }
                println!("  Uptime: {:.1}%", report.health.uptime_percentage);
                println!("  Active Alerts: {}", report.health.active_alerts_count);

                // Recent alerts
                if !report.alerts.is_empty() {
                    println!("\n🚨 Recent Alerts:");
                    let recent_alerts = report.alerts.iter()
                        .rev()
                        .take(5)
                        .collect::<Vec<_>>();

                    for alert in recent_alerts {
                        println!("  {:?} [{}]: {}", alert.severity,
                                     alert.timestamp.format("%H:%M:%S"),
                                     alert.title);
                    }
                }

                // System status
                println!("\n🖥️  System Status:");
                println!("  📊 Monitoring Active: ✅");
                println!("  📤 Traffic Generation: ✅");
                println!("  📁 Metrics Export: ✅");
                println!("  📊 Analysis: ✅");
                println!("  🚨 Alerting: ✅");
            }
        })
    };

    // Wait for user input
    println!("\n🎯 Advanced monitoring is running!");
    println!("💡 Press Ctrl+C to stop monitoring and see final report");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("\n🛑 Shutting down monitoring system...");
        }
    }

    // Cancel background tasks
    monitoring_handle.abort();
    metrics_handle.abort();
    traffic_handle.abort();
    dashboard_handle.abort();

    // Generate final comprehensive report
    println!("\n📋 Generating Final Report");
    println!("=========================");

    let final_report = monitor.generate_report().await;

    // Display final summary
    println!("📊 Final Metrics Summary:");
    println!("  📦 Total Messages Processed: {}", final_report.summary.total_messages);
    println!("  ✅ Successful Processing: {}", final_report.summary.processed_messages);
    println!("  ❌ Failed Processing: {}", final_report.summary.final_report.failed_messages);
    println!("  📈 Overall Success Rate: {:.1}%", final_report.summary.success_rate);
    println!("  ❤️  Final Health Score: {:.1}/100", final_report.summary.health_score);

    // Export final metrics
    println!("\n📁 Exporting final metrics...");
    if let Err(e) = metrics_exporter.export_metrics(&final_report).await {
        error!("Failed to export final metrics: {}", e);
    } else {
        println!("✅ Final metrics exported to ./metrics_exports/");
    }

    // Performance analysis
    println!("\n📈 Performance Analysis:");
    if final_report.performance.messages_per_second > 10.0 {
        println!("  🚀 High throughput: {:.2} msg/sec", final_report.performance.messages_per_second);
    } else if final_report.performance.messages_per_second > 5.0 {
        println!("  ⚡ Good throughput: {:.2} msg/sec", final_report.performance.messages_per_second);
    } else {
        println!("  🐌 Low throughput: {:.2} msg/sec", final_report.performance.messages_per_second);
    }

    if final_report.summary.error_rate > 5.0 {
        println!("  ⚠️ High error rate: {:.1}%", final_report.summary.error_rate);
    } else if final_report.summary.error_rate > 1.0 {
        println!("  ⚡ Moderate error rate: {:.1}%", final_report.summary.error_rate);
    } else {
        println!("  ✅ Low error rate: {:.1}%", final_report.summary.error_rate);
    }

    // Recommendations
    println!("\n💡 Recommendations:");
    if final_report.summary.health_score < 70.0 {
        println!("  🚨 System health is poor - investigate immediately");
    } else if final_report.summary.health_score < 90.0 {
        println!("  ⚠️ System needs attention - monitor closely");
    } else {
        println!("  ✅ System is performing well - continue monitoring");
    }

    if final_report.performance.avg_processing_time_ms > 1000.0 {
        println!("  ⏱️  Consider optimizing processing time (current: {:.1}ms)",
                 final_report.performance.avg_processing_time_ms);
    }

    if final_report.performance.queue_depth > 200 {
        println!("  📊 Consider scaling consumers (queue depth: {})", final_report.performance.queue_depth);
    }

    println!("\n🎉 Advanced Monitoring Example Completed!");
    println!("===================================");
    println!("📁 Check ./metrics_exports/ for exported metrics files");
    println!("📊 Monitoring components demonstrated:");
    println!("  - Real-time metrics collection");
    println!("  - Multi-channel alerting (Console, Webhook, Database)");
    println!("  - Performance analysis and trend tracking");
    println!("  - Health monitoring and alerting");
    println!("  - Metrics export (JSON, CSV, Prometheus)");
    println!("  - Alert aggregation and correlation");

    Ok(())
}

impl QueueMonitor {
    async fn print_monitoring_status(metrics: &QueueMetrics) {
        println!("📈 Current Status:");
        println!("  📦 Queue Depth: {}", metrics.total_messages);
        println!("  ✅ Success Rate: {:.1}%", metrics.success_rate);
        println!("  ⚠️  Error Rate: {:.1}%", metrics.error_rate);
        println!("  🚀 Throughput: {:.2} msg/sec", metrics.messages_per_second);
        println!("  ❤️  Health Score: {:.1}/100", metrics.health_score());
        println!("  🚨 Active Alerts: {}", metrics.active_alerts.len());
    }
}