//! Unit tests for queue monitoring module

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        QueueStats, QueueHealth, QueueHealthCheck,
        redis::RedisQueueBuilder,
        types::{QueueMessage, QueuePriority, MessageStatus}
    };
    use std::collections::HashMap;
    use std::time::Duration;
    use chrono::{Utc, DateTime};

    /// Create test queue for monitoring tests
    async fn create_test_queue() -> Result<RedisQueue, Box<dyn std::error::Error>> {
        let queue = RedisQueueBuilder::new()
            .url("redis://localhost:6379")
            .queue_name("monitoring_test_queue")
            .key_prefix("test_monitoring")
            .build()
            .await?;

        // Clear any existing data
        queue.purge().await?;
        Ok(queue)
    }

    /// Create test queue monitor
    fn create_test_monitor(queue: RedisQueue) -> QueueMonitor {
        let thresholds = AlertThresholds {
            max_queue_size: 100,
            max_error_rate: 5.0,
            max_processing_time_ms: 1000.0,
            min_messages_per_second: 1.0,
            max_dead_letter_messages: 5,
        };

        QueueMonitor::new(Arc::new(queue), thresholds)
    }

    #[tokio::test]
    async fn test_queue_metrics_creation() -> Result<(), Box<dyn std::error::Error>> {
        let metrics = QueueMetrics::new();

        assert_eq!(metrics.total_messages, 0);
        assert_eq!(metrics.processed_messages, 0);
        assert_eq!(metrics.failed_messages, 0);
        assert_eq!(metrics.success_rate, 100.0);
        assert_eq!(metrics.error_rate, 0.0);
        assert_eq!(metrics.health_score(), 100.0);
        assert!(metrics.is_healthy());

        Ok(())
    }

    #[tokio::test]
    async fn test_queue_metrics_update_from_stats() -> Result<(), Box<dyn std::error::Error>> {
        let mut metrics = QueueMetrics::new();

        let stats = QueueStats {
            total_messages: 150,
            visible_messages: 100,
            invisible_messages: 50,
            dead_letter_messages: 5,
            total_processed: 140,
            total_failed: 10,
            ..Default::default()
        };

        metrics.update_from_stats(&stats);

        assert_eq!(metrics.total_messages, 150);
        assert_eq!(metrics.processed_messages, 140);
        assert_eq!(metrics.failed_messages, 10);
        assert_eq!(metrics.success_rate, 93.3); // 140/150 * 100
        assert_eq!(metrics.error_rate, 6.7);     // 10/150 * 100

        Ok(())
    }

    #[tokio::test]
    async fn test_queue_metrics_trends() -> Result<(), Box<dyn std::error::Error>> {
        let mut metrics = QueueMetrics::new();

        // Add trend points
        metrics.add_trend_point(&mut metrics.queue_depth_trend, 10.0);
        metrics.add_trend_point(&mut metrics.queue_depth_trend, 20.0);
        metrics.add_trend_point(&mut metrics.queue_depth_trend, 15.0);

        assert_eq!(metrics.queue_depth_trend.len(), 3);
        assert_eq!(metrics.queue_depth_trend[0].value, 10.0);
        assert_eq!(metrics.queue_depth_trend[1].value, 20.0);
        assert_eq!(metrics.queue_depth_trend[2].value, 15.0);

        // Test trend truncation
        for i in 0..110 {
            metrics.add_trend_point(&mut metrics.queue_depth_trend, i as f64);
        }

        assert_eq!(metrics.queue_depth_trend.len(), 100);
        // Should keep only last 100 points
        assert_eq!(metrics.queue_depth_trend[0].value, 10.0); // First point removed
        assert_eq!(metrics.queue_depth_trend[99].value, 109.0); // Last added

        Ok(())
    }

    #[tokio::test]
    async fn test_queue_metrics_health_score() -> Result<(), Box<dyn std::error::Error>> {
        let mut metrics = QueueMetrics::new();

        // Perfect health score
        assert_eq!(metrics.health_score(), 100.0);

        // Add some errors
        metrics.error_rate = 5.0; // 5% error rate
        assert_eq!(metrics.health_score(), 90.0); // 100 - (5 * 2)

        metrics.total_messages = 1500; // Over threshold
        assert_eq!(metrics.health_score(), 80.0); // 90 - (1500-1000)/100

        // Set unhealthy status
        metrics.health_status = QueueHealth::Unhealthy;
        assert_eq!(metrics.health_score(), 30.0); // 80 - 50

        // Check healthy status
        assert!(!metrics.is_healthy());

        Ok(())
    }

    #[tokio::test]
    async fn test_alert_event_creation() -> Result<(), Box<dyn std::error::Error>> {
        let alert = AlertEvent::new(
            "Test Alert".to_string(),
            "This is a test alert".to_string(),
            AlertSeverity::Warning,
        );

        assert!(!alert.id.is_empty());
        assert_eq!(alert.title, "Test Alert");
        assert_eq!(alert.message, "This is a test alert");
        assert_eq!(alert.severity, AlertSeverity::Warning);
        assert!(alert.is_active());
        assert!(alert.resolved_at.is_none());

        // Resolve alert
        let mut resolved_alert = alert;
        resolved_alert.resolve();
        assert!(resolved_alert.is_resolved());

        Ok(())
    }

    #[tokio::test]
    async fn test_alert_thresholds_default() -> Result<(), Box<dyn std::error::Error>> {
        let thresholds = AlertThresholds::default();

        assert_eq!(thresholds.max_queue_size, 1000);
        assert_eq!(thresholds.max_error_rate, 5.0);
        assert_eq!(thresholds.max_processing_time_ms, 5000.0);
        assert_eq!(thresholds.min_messages_per_second, 1.0);
        assert_eq!(thresholds.max_dead_letter_messages, 10);

        Ok(())
    }

    #[tokio::test]
    async fn test_queue_monitor_creation() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_queue().await?;
        let monitor = create_test_monitor(queue);

        // Monitor should be created successfully
        Ok(())
    }

    #[tokio::test]
    async fn test_queue_monitor_metrics_update() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_queue().await?;
        let monitor = create_test_monitor(queue);

        // Enqueue some test messages
        let message = QueueMessage::builder()
            .payload("Test message")
            .build();

        queue.enqueue(message).await?;

        // Update metrics
        monitor.update_metrics().await?;

        let metrics = monitor.get_metrics().await;
        assert!(metrics.total_messages >= 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_queue_monitor_health_check() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_queue().await?;
        let monitor = create_test_monitor(queue);

        // Perform health check
        let health = monitor.health_check().await?;

        assert!(matches!(health.status, QueueHealth::Healthy | QueueHealth::Degraded));

        Ok(())
    }

    #[tokio::test]
    async fn test_queue_monitor_alerts_no_issues() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_queue().await?;
        let monitor = create_test_monitor(queue);

        // Update metrics with good values
        {
            let mut metrics = monitor.metrics.write().await;
            metrics.total_messages = 50;
            metrics.processed_messages = 48;
            metrics.failed_messages = 2;
            metrics.messages_per_second = 5.0;
            metrics.health_status = QueueHealth::Healthy;
        }

        let alerts = monitor.check_alerts().await;
        assert!(alerts.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_queue_monitor_alerts_with_issues() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_queue().await?;
        let monitor = create_test_monitor(queue);

        // Update metrics with problematic values
        {
            let mut metrics = monitor.metrics.write().await;
            metrics.total_messages = 2000; // Over threshold
            metrics.processed_messages = 1600;
            metrics.failed_messages = 400; // High error rate
            metrics.messages_per_second = 0.5; // Low processing rate
            metrics.health_status = QueueHealth::Degraded;
        }

        let alerts = monitor.check_alerts().await;
        assert!(!alerts.is_empty());

        // Check for expected alerts
        let alert_titles: Vec<String> = alerts.iter().map(|a| a.title.clone()).collect();
        assert!(alert_titles.contains(&"Queue Size Alert".to_string()));
        assert!(alert_titles.contains(&"Error Rate Alert".to_string()));
        assert!(alert_titles.contains(&"Low Processing Rate Alert".to_string()));
        assert!(alert_titles.contains(&"Health Status Alert".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_queue_monitor_severity_levels() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_queue().await?;
        let monitor = create_test_monitor(queue);

        // Test critical error rate
        {
            let mut metrics = monitor.metrics.write().await;
            metrics.total_messages = 100;
            metrics.processed_messages = 50;
            metrics.failed_messages = 50; // 50% error rate
        }

        let alerts = monitor.check_alerts().await;
        let critical_alerts: Vec<_> = alerts.iter()
            .filter(|a| a.severity == AlertSeverity::Critical)
            .collect();

        assert!(!critical_alerts.is_empty());
        assert!(critical_alerts[0].title.contains("Error Rate Alert"));

        Ok(())
    }

    #[tokio::test]
    async fn test_console_alert_callback() -> Result<(), Box<dyn std::error::Error>> {
        let callback = ConsoleAlertCallback;
        let alert = AlertEvent::new(
            "Test Alert".to_string(),
            "Test message".to_string(),
            AlertSeverity::Info,
        );

        // Test callback (should not panic)
        callback.on_alert(&alert);
        callback.on_alert_resolved(&alert);

        Ok(())
    }

    #[tokio::test]
    async fn test_webhook_alert_callback() -> Result<(), Box<dyn std::error::Error>> {
        let callback = WebhookAlertCallback::new(
            "https://example.com/webhook".to_string()
        );

        let alert = AlertEvent::new(
            "Test Webhook Alert".to_string(),
            "Test webhook message".to_string(),
            AlertSeverity::Warning,
        );

        // Test callback (should not panic)
        callback.on_alert(&alert);
        callback.on_alert_resolved(&alert);

        Ok(())
    }

    #[tokio::test]
    async fn test_metrics_report_generation() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_queue().await?;
        let monitor = create_test_monitor(queue);

        // Update metrics with test data
        {
            let mut metrics = monitor.metrics.write().await;
            metrics.total_messages = 500;
            metrics.processed_messages = 450;
            metrics.failed_messages = 50;
            metrics.messages_per_second = 10.0;
            metrics.avg_processing_time_ms = 150.0;
            metrics.health_status = QueueHealth::Healthy;
            metrics.last_health_check = Some(Utc::now());
        }

        // Generate report
        let report = monitor.generate_report().await;

        // Verify report structure
        assert_eq!(report.summary.total_messages, 500);
        assert_eq!(report.summary.processed_messages, 450);
        assert_eq!(report.summary.failed_messages, 50);
        assert_eq!(report.performance.messages_per_second, 10.0);
        assert_eq!(report.health.status, QueueHealth::Healthy);

        Ok(())
    }

    #[tokio::test]
    async fn test_timestamped_value() -> Result<(), Box<dyn std::error::Error>> {
        let timestamp = Utc::now();
        let value = TimestampedValue {
            timestamp,
            value: 42.5,
        };

        assert_eq!(value.value, 42.5);
        assert!(value.timestamp <= Utc::now());

        Ok(())
    }

    #[tokio::test]
    async fn test_metrics_report_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_queue().await?;
        let monitor = create_test_monitor(queue);

        // Generate report
        let report = monitor.generate_report().await;

        // Test JSON serialization
        let json = serde_json::to_string(&report)?;
        let deserialized: MetricsReport = serde_json::from_str(&json)?;

        assert_eq!(deserialized.summary.total_messages, report.summary.total_messages);
        assert_eq!(deserialized.performance.messages_per_second, report.performance.messages_per_second);

        Ok(())
    }

    #[tokio::test]
    async fn test_monitoring_integration() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_queue().await?;
        let monitor = create_test_monitor(queue);

        // Test complete monitoring workflow
        // 1. Add some messages to queue
        for i in 0..5 {
            let message = QueueMessage::builder()
                .id(format!("test-msg-{}", i))
                .payload(format!("Test message {}", i))
                .priority(QueuePriority::Normal)
                .build();

            queue.enqueue(message).await?;
        }

        // 2. Update metrics
        monitor.update_metrics().await?;
        let metrics_after = monitor.get_metrics().await;
        assert!(metrics_after.total_messages >= 5);

        // 3. Check health
        let health = monitor.health_check().await?;
        assert!(matches!(health.status, QueueHealth::Healthy | QueueHealth::Degraded));

        // 4. Check alerts (should be minimal for normal case)
        let alerts = monitor.check_alerts().await;

        // 5. Generate report
        let report = monitor.generate_report().await;
        assert_eq!(report.summary.total_messages, metrics_after.total_messages);

        Ok(())
    }

    #[tokio::test]
    async fn test_monitoring_with_alert_callback() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_queue().await?;

        let mut monitor = create_test_monitor(queue);

        // Add custom alert callback that tracks alerts
        let alert_log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let alert_log_clone = alert_log.clone();

        struct TestCallback {
            alert_log: Arc<std::sync::Mutex<Vec<AlertEvent>>>,
        }

        impl backbone_queue::monitoring::AlertCallback for TestCallback {
            fn on_alert(&self, alert: &AlertEvent) {
                let mut log = self.alert_log.lock().unwrap();
                log.push(alert.clone());
            }

            fn on_alert_resolved(&self, alert: &AlertEvent) {
                let mut log = self.alert_log.lock().unwrap();
                log.push(alert.clone());
            }
        }

        monitor.add_alert_callback(Box::new(TestCallback {
            alert_log: alert_log_clone,
        }));

        // Generate an alert by setting problematic metrics
        {
            let mut metrics = monitor.metrics.write().await;
            metrics.total_messages = 2000; // Over threshold
        }

        let alerts = monitor.check_alerts().await;
        let log = alert_log.lock().unwrap();

        // Should have captured the alert
        assert!(!log.is_empty());
        assert_eq!(log.len(), alerts.len());

        Ok(())
    }

    #[tokio::test]
    async fn test_monitoring_continuous_updates() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_queue().await?;
        let monitor = create_test_monitor(queue);

        // Start continuous monitoring
        let handle = monitor.start_monitoring(Duration::from_millis(100));

        // Wait a bit for some monitoring cycles
        tokio::time::sleep(Duration::from_millis(250)).await;

        // Add some messages
        for i in 0..3 {
            let message = QueueMessage::builder()
                .payload(format!("Continuous test {}", i))
                .build();

            queue.enqueue(message).await?;
        }

        // Wait for monitoring to process
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Check that monitoring is running
        let metrics = monitor.get_metrics().await;
        assert!(metrics.total_messages >= 3);

        // Stop monitoring
        handle.abort();

        Ok(())
    }
}