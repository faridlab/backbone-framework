//! Backbone Health Module Tests

use backbone_health::{
    HealthChecker, HealthConfig, HealthStatus, HealthReport, SimpleHealthResponse,
    ComponentStatus, MockHealthCheck, CustomHealthCheck, HealthError,
    DEFAULT_HEALTH_PATH, DEFAULT_READINESS_PATH, DEFAULT_LIVENESS_PATH,
};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_health_checker_creation() -> Result<(), Box<dyn std::error::Error>> {
    let config = HealthConfig {
        check_interval: Duration::from_secs(10),
        timeout: Duration::from_secs(2),
        failure_threshold: 3,
        success_threshold: 2,
        app_version: Some("1.0.0".to_string()),
        app_name: Some("TestApp".to_string()),
        ..Default::default()
    };

    let checker = HealthChecker::new(config);

    // Test initial state
    let report = checker.health_report().await;
    assert_eq!(report.version, "1.0.0");
    assert_eq!(report.app_name, Some("TestApp".to_string()));
    assert_eq!(report.status, HealthStatus::Healthy);
    assert_eq!(report.components.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_health_checker_with_mock_components() -> Result<(), Box<dyn std::error::Error>> {
    let checker = HealthChecker::new(HealthConfig::default());

    // Add healthy component
    let healthy_check = MockHealthCheck::healthy("database".to_string());
    checker.add_component("database".to_string(), Box::new(healthy_check)).await?;

    // Add unhealthy component
    let unhealthy_check = MockHealthCheck::unhealthy("cache".to_string());
    checker.add_component("cache".to_string(), Box::new(unhealthy_check)).await?;

    // Run single check
    let report = checker.run_single_check().await?;
    assert_eq!(report.components.len(), 2);

    // Check component statuses
    let db_status = report.components.get("database").unwrap();
    assert_eq!(db_status.status, HealthStatus::Healthy);

    let cache_status = report.components.get("cache").unwrap();
    assert_eq!(cache_status.status, HealthStatus::Unhealthy);

    // Overall status should be degraded (one unhealthy component)
    assert_eq!(report.status, HealthStatus::Degraded);

    Ok(())
}

#[tokio::test]
async fn test_health_checker_component_management() -> Result<(), Box<dyn std::error::Error>> {
    let checker = HealthChecker::new(HealthConfig::default());

    // Add component
    let component = MockHealthCheck::healthy("test".to_string());
    checker.add_component("test".to_string(), Box::new(component)).await?;

    // Verify component exists
    let component_names = checker.component_names().await;
    assert_eq!(component_names.len(), 1);
    assert!(component_names.contains(&"test".to_string()));

    // Get component status
    let status = checker.component_status("test").await;
    assert!(status.is_some());

    // Remove component
    checker.remove_component("test").await?;
    let component_names = checker.component_names().await;
    assert_eq!(component_names.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_health_checker_start_stop() -> Result<(), Box<dyn std::error::Error>> {
    let config = HealthConfig {
        check_interval: Duration::from_millis(100), // Fast for testing
        ..Default::default()
    };

    let checker = HealthChecker::new(config);

    // Add a component
    let component = MockHealthCheck::healthy("test".to_string());
    checker.add_component("test".to_string(), Box::new(component)).await?;

    // Start the checker
    checker.start().await?;

    // Wait for a couple of checks to run
    sleep(Duration::from_millis(250)).await;

    // Verify checks have run
    let status = checker.component_status("test").await.unwrap();
    assert!(status.total_checks > 0);

    // Stop the checker
    checker.stop().await?;

    Ok(())
}

#[tokio::test]
async fn test_health_checker_double_start() -> Result<(), Box<dyn std::error::Error>> {
    let checker = HealthChecker::new(HealthConfig::default());

    // Start the checker
    checker.start().await?;

    // Starting again should fail
    let result = checker.start().await;
    assert!(result.is_err());

    Ok(())
}

#[tokio::test]
async fn test_simple_health_responses() -> Result<(), Box<dyn std::error::Error>> {
    let checker = HealthChecker::new(HealthConfig::default());

    // Test default healthy response
    let health = checker.health_status().await;
    assert_eq!(health.status, HealthStatus::Healthy);
    assert!(health.message.is_some());

    // Test readiness (should be ready with no components)
    let readiness = checker.readiness().await;
    assert_eq!(readiness.status, HealthStatus::Healthy);

    // Test liveness
    let liveness = checker.liveness(None).await;
    assert_eq!(liveness.status, HealthStatus::Healthy);

    Ok(())
}

#[tokio::test]
async fn test_health_response_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let response = SimpleHealthResponse::healthy();
    let json = serde_json::to_string(&response)?;

    assert!(json.contains("healthy"));

    let deserialized: SimpleHealthResponse = serde_json::from_str(&json)?;
    assert_eq!(deserialized.status, HealthStatus::Healthy);

    Ok(())
}

#[tokio::test]
async fn test_component_status_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let mut status = ComponentStatus::new("test".to_string());

    // Test initial state
    assert_eq!(status.status, HealthStatus::Healthy);
    assert_eq!(status.consecutive_failures, 0);
    assert_eq!(status.consecutive_successes, 0);
    assert_eq!(status.total_checks, 0);
    assert_eq!(status.success_rate, 0.0);

    // Record success
    status.record_success(Duration::from_millis(10));
    assert_eq!(status.consecutive_successes, 1);
    assert_eq!(status.consecutive_failures, 0);
    assert_eq!(status.total_checks, 1);
    assert_eq!(status.success_rate, 100.0);

    // Record failure
    status.record_failure("Test error".to_string(), Duration::from_millis(20));
    assert_eq!(status.consecutive_successes, 0);
    assert_eq!(status.consecutive_failures, 1);
    assert_eq!(status.total_checks, 2);
    assert_eq!(status.success_rate, 50.0);

    // Record degraded
    status.record_degraded("Warning".to_string(), Duration::from_millis(15));
    assert_eq!(status.consecutive_successes, 1);
    assert_eq!(status.consecutive_failures, 0);
    assert_eq!(status.total_checks, 3);
    assert_eq!(status.success_rate, 100.0);

    // Test metadata
    status.add_metadata("key".to_string(), "value".to_string());
    assert_eq!(status.get_metadata("key"), Some(&"value".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_health_report_creation() -> Result<(), Box<dyn std::error::Error>> {
    let mut components = std::collections::HashMap::new();

    let mut healthy_component = ComponentStatus::new("healthy".to_string());
    healthy_component.record_success(Duration::from_millis(10));
    components.insert("healthy".to_string(), healthy_component);

    let mut unhealthy_component = ComponentStatus::new("unhealthy".to_string());
    unhealthy_component.record_failure("Error".to_string(), Duration::from_millis(50));
    components.insert("unhealthy".to_string(), unhealthy_component);

    let report = HealthReport::new(
        "1.0.0".to_string(),
        Some("TestApp".to_string()),
        Duration::from_secs(60),
        components,
    );

    assert_eq!(report.status, HealthStatus::Degraded); // One unhealthy component
    assert_eq!(report.version, "1.0.0");
    assert_eq!(report.app_name, Some("TestApp".to_string()));
    assert_eq!(report.components.len(), 2);
    assert_eq!(report.summary.total_components, 2);
    assert_eq!(report.summary.healthy_count, 1);
    assert_eq!(report.summary.unhealthy_count, 1);

    Ok(())
}

#[tokio::test]
async fn test_custom_health_check() -> Result<(), Box<dyn std::error::Error>> {
    let custom_check = CustomHealthCheck::new(
        "custom".to_string(),
        || async {
            let mut status = ComponentStatus::new("custom".to_string());
            status.record_success(Duration::from_millis(5));
            status.add_metadata("custom_data".to_string(), "test".to_string());
            Ok(status)
        }
    );

    let checker = HealthChecker::new(HealthConfig::default());
    checker.add_component("custom".to_string(), Box::new(custom_check)).await?;

    // Run single check
    let report = checker.run_single_check().await?;

    let status = report.components.get("custom").unwrap();
    assert_eq!(status.status, HealthStatus::Healthy);
    assert_eq!(status.get_metadata("custom_data"), Some(&"test".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_health_checker_builder() -> Result<(), Box<dyn std::error::Error>> {
    let checker = HealthChecker::builder()
        .check_interval(Duration::from_secs(5))
        .timeout(Duration::from_secs(2))
        .failure_threshold(2)
        .success_threshold(1)
        .app_version("2.0.0".to_string())
        .app_name("BuilderTest".to_string())
        .component("test".to_string(), Box::new(MockHealthCheck::healthy("test".to_string())))
        .build()
        .await?;

    let report = checker.health_report().await;
    assert_eq!(report.version, "2.0.0");
    assert_eq!(report.app_name, Some("BuilderTest".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_health_checker_timeout_handling() -> Result<(), Box<dyn std::error::Error>> {
    let checker = HealthChecker::new(HealthConfig {
        timeout: Duration::from_millis(10), // Very short timeout
        ..Default::default()
    });

    // Add a component that takes longer than the timeout
    let slow_check = CustomHealthCheck::new(
        "slow".to_string(),
        || async {
            tokio::time::sleep(Duration::from_millis(50)).await; // Longer than timeout
            let mut status = ComponentStatus::new("slow".to_string());
            status.record_success(Duration::from_millis(50));
            Ok(status)
        }
    );

    checker.add_component("slow".to_string(), Box::new(slow_check)).await?;

    // Run check - should handle timeout gracefully
    let report = checker.run_single_check().await?;
    let status = report.components.get("slow").unwrap();

    // Should be marked as unhealthy due to timeout
    assert_eq!(status.status, HealthStatus::Unhealthy);
    assert!(status.error.as_ref().unwrap().contains("timed out"));

    Ok(())
}

#[test]
fn test_health_config_default() {
    let config = HealthConfig::default();
    assert_eq!(config.check_interval, Duration::from_secs(30));
    assert_eq!(config.timeout, Duration::from_secs(5));
    assert_eq!(config.failure_threshold, 3);
    assert_eq!(config.success_threshold, 2);
    assert!(config.include_details);
    assert!(config.enable_metrics);
}

#[test]
fn test_health_constants() {
    assert_eq!(DEFAULT_HEALTH_PATH, "/health");
    assert_eq!(DEFAULT_READINESS_PATH, "/ready");
    assert_eq!(DEFAULT_LIVENESS_PATH, "/live");
}

#[test]
fn test_health_error_types() {
    let timeout_error = HealthError::Timeout("Test timeout".to_string());
    assert!(timeout_error.to_string().contains("timeout"));

    let check_failed_error = HealthError::CheckFailed("Check failed".to_string());
    assert!(check_failed_error.to_string().contains("failed"));

    let component_not_found_error = HealthError::ComponentNotFound("test".to_string());
    assert!(component_not_found_error.to_string().contains("not found"));

    let config_error = HealthError::InvalidConfiguration("Invalid config".to_string());
    assert!(config_error.to_string().contains("configuration"));

    let service_error = HealthError::ServiceUnavailable("Service down".to_string());
    assert!(service_error.to_string().contains("unavailable"));

    let internal_error = HealthError::Internal("Internal error".to_string());
    assert!(internal_error.to_string().contains("Internal"));
}

#[tokio::test]
async fn test_consecutive_failure_threshold() -> Result<(), Box<dyn std::error::Error>> {
    let checker = HealthChecker::new(HealthConfig {
        failure_threshold: 2,
        ..Default::default()
    });

    let unhealthy_check = MockHealthCheck::unhealthy("failing".to_string());
    checker.add_component("failing".to_string(), Box::new(unhealthy_check)).await?;

    // First failure should still be healthy (below threshold)
    let report1 = checker.run_single_check().await?;
    let status1 = report1.components.get("failing").unwrap();
    assert_eq!(status1.status, HealthStatus::Unhealthy);
    assert_eq!(status1.consecutive_failures, 1);

    // Second failure should remain unhealthy
    let report2 = checker.run_single_check().await?;
    let status2 = report2.components.get("failing").unwrap();
    assert_eq!(status2.status, HealthStatus::Unhealthy);
    assert_eq!(status2.consecutive_failures, 2);

    Ok(())
}

#[tokio::test]
async fn test_success_threshold_recovery() -> Result<(), Box<dyn std::error::Error>> {
    let checker = HealthChecker::new(HealthConfig {
        success_threshold: 2,
        failure_threshold: 2,
        ..Default::default()
    });

    // Start with failing check
    let failing_check = CustomHealthCheck::new(
        "recovering".to_string(),
        || async {
            let mut status = ComponentStatus::new("recovering".to_string());
            status.record_failure("Simulated failure".to_string(), Duration::from_millis(10));
            Ok(status)
        }
    );

    checker.add_component("recovering".to_string(), Box::new(failing_check)).await?;

    // First check fails
    let report1 = checker.run_single_check().await?;
    let status1 = report1.components.get("recovering").unwrap();
    assert_eq!(status1.consecutive_failures, 1);

    Ok(())
}