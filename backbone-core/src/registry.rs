//! Module Service Registry - Discovery and access to module services
//!
//! The Service Registry provides a centralized way for bounded contexts (modules)
//! to expose services that other modules can discover and use. This enables
//! loose-coupled inter-module communication beyond events.
//!
//! # Architecture
//!
//! ```text
//!                    ┌────────────────────────────┐
//!                    │     ServiceRegistry        │
//!                    │  (Application Bootstrap)   │
//!                    └────────────────────────────┘
//!                              │
//!              ┌───────────────┼───────────────┐
//!              │               │               │
//!              ▼               ▼               ▼
//!     ┌────────────┐  ┌────────────┐  ┌────────────┐
//!     │  Sapiens   │  │  Postman   │  │  Bucket  │
//!     │  Services  │  │  Services  │  │  Services  │
//!     └────────────┘  └────────────┘  └────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use backbone_core::registry::{ServiceRegistry, ModuleService};
//!
//! // Create registry in main app
//! let registry = ServiceRegistry::new();
//!
//! // Register services from modules
//! registry.register(user_query_service).await;
//! registry.register(email_service).await;
//!
//! // Look up services
//! if let Some(service) = registry.get("sapiens.user_query").await {
//!     // Use the service
//! }
//! ```

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;

// ============================================================
// Core Traits
// ============================================================

/// Trait for module services that can be registered and discovered
///
/// Services implement this trait to be registered in the ServiceRegistry.
/// The registry provides service discovery and health checking.
///
/// # Example
///
/// ```rust,ignore
/// use backbone_core::registry::ModuleService;
///
/// struct UserQueryService { /* ... */ }
///
/// #[async_trait]
/// impl ModuleService for UserQueryService {
///     fn service_id(&self) -> &'static str {
///         "sapiens.user_query"
///     }
///
///     fn service_type(&self) -> &'static str {
///         "query"
///     }
///
///     fn module_name(&self) -> &'static str {
///         "sapiens"
///     }
///
///     async fn health_check(&self) -> Result<ServiceHealth, String> {
///         Ok(ServiceHealth::healthy())
///     }
/// }
/// ```
#[async_trait]
pub trait ModuleService: Send + Sync {
    /// Unique service identifier (e.g., "sapiens.user_query", "postman.email_sender")
    ///
    /// Format: `{module}.{service_name}`
    fn service_id(&self) -> &'static str;

    /// Service type (e.g., "query", "command", "notification")
    fn service_type(&self) -> &'static str;

    /// Module that owns this service (e.g., "sapiens", "postman")
    fn module_name(&self) -> &'static str;

    /// Health check for the service
    async fn health_check(&self) -> Result<ServiceHealth, String>;

    /// Optional: Get the service as Any for downcasting
    ///
    /// Override this to enable type-safe service retrieval via `get_typed`.
    fn as_any(&self) -> Option<&dyn Any> {
        None
    }
}

/// Health status of a service
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceHealth {
    pub status: HealthStatus,
    pub message: Option<String>,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub details: HashMap<String, String>,
}

impl ServiceHealth {
    pub fn healthy() -> Self {
        Self {
            status: HealthStatus::Healthy,
            message: None,
            last_check: chrono::Utc::now(),
            details: HashMap::new(),
        }
    }

    pub fn unhealthy(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Unhealthy,
            message: Some(message.into()),
            last_check: chrono::Utc::now(),
            details: HashMap::new(),
        }
    }

    pub fn degraded(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Degraded,
            message: Some(message.into()),
            last_check: chrono::Utc::now(),
            details: HashMap::new(),
        }
    }

    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

// ============================================================
// Service Registry
// ============================================================

/// Registry for discovering and accessing module services
///
/// The ServiceRegistry provides:
/// - Service registration from modules
/// - Service discovery by ID or pattern
/// - Health checking for all services
/// - Module-level service grouping
#[derive(Clone)]
pub struct ServiceRegistry {
    services: Arc<RwLock<HashMap<String, Arc<dyn ModuleService>>>>,
}

impl ServiceRegistry {
    /// Create a new empty service registry
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a service
    ///
    /// # Arguments
    /// * `service` - The service to register (must implement ModuleService)
    pub async fn register<S: ModuleService + 'static>(&self, service: S) {
        let service_id = service.service_id().to_string();
        tracing::info!(
            service_id = %service_id,
            module = %service.module_name(),
            service_type = %service.service_type(),
            "Registering service"
        );
        self.services.write().await.insert(service_id, Arc::new(service));
    }

    /// Register a service wrapped in Arc
    pub async fn register_arc(&self, service: Arc<dyn ModuleService>) {
        let service_id = service.service_id().to_string();
        tracing::info!(
            service_id = %service_id,
            module = %service.module_name(),
            service_type = %service.service_type(),
            "Registering service (Arc)"
        );
        self.services.write().await.insert(service_id, service);
    }

    /// Get a service by ID
    ///
    /// Returns None if the service is not registered.
    pub async fn get(&self, service_id: &str) -> Option<Arc<dyn ModuleService>> {
        self.services.read().await.get(service_id).cloned()
    }

    /// Unregister a service
    pub async fn unregister(&self, service_id: &str) -> bool {
        self.services.write().await.remove(service_id).is_some()
    }

    /// List all registered service IDs
    pub async fn list(&self) -> Vec<String> {
        self.services.read().await.keys().cloned().collect()
    }

    /// List services by module
    pub async fn list_by_module(&self, module_name: &str) -> Vec<Arc<dyn ModuleService>> {
        self.services
            .read()
            .await
            .values()
            .filter(|s| s.module_name() == module_name)
            .cloned()
            .collect()
    }

    /// List services by type
    pub async fn list_by_type(&self, service_type: &str) -> Vec<Arc<dyn ModuleService>> {
        self.services
            .read()
            .await
            .values()
            .filter(|s| s.service_type() == service_type)
            .cloned()
            .collect()
    }

    /// Check if a service is registered
    pub async fn has(&self, service_id: &str) -> bool {
        self.services.read().await.contains_key(service_id)
    }

    /// Get the count of registered services
    pub async fn count(&self) -> usize {
        self.services.read().await.len()
    }

    /// Health check all services
    pub async fn health_check_all(&self) -> RegistryHealth {
        let services = self.services.read().await;
        let mut results = HashMap::new();
        let mut healthy_count = 0;
        let mut unhealthy_count = 0;

        for (id, service) in services.iter() {
            match service.health_check().await {
                Ok(health) => {
                    if health.status == HealthStatus::Healthy {
                        healthy_count += 1;
                    } else {
                        unhealthy_count += 1;
                    }
                    results.insert(id.clone(), Ok(health));
                }
                Err(e) => {
                    unhealthy_count += 1;
                    results.insert(id.clone(), Err(e));
                }
            }
        }

        let overall_status = if unhealthy_count == 0 {
            HealthStatus::Healthy
        } else if healthy_count > 0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Unhealthy
        };

        RegistryHealth {
            status: overall_status,
            total_services: services.len(),
            healthy_count,
            unhealthy_count,
            services: results,
            checked_at: chrono::Utc::now(),
        }
    }

    /// Get registry statistics
    pub async fn statistics(&self) -> RegistryStatistics {
        let services = self.services.read().await;

        let mut by_module: HashMap<String, usize> = HashMap::new();
        let mut by_type: HashMap<String, usize> = HashMap::new();

        for service in services.values() {
            *by_module.entry(service.module_name().to_string()).or_default() += 1;
            *by_type.entry(service.service_type().to_string()).or_default() += 1;
        }

        RegistryStatistics {
            total_services: services.len(),
            services_by_module: by_module,
            services_by_type: by_type,
        }
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Health report for the entire registry
#[derive(Debug, Clone, serde::Serialize)]
pub struct RegistryHealth {
    pub status: HealthStatus,
    pub total_services: usize,
    pub healthy_count: usize,
    pub unhealthy_count: usize,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub services: HashMap<String, Result<ServiceHealth, String>>,
    pub checked_at: chrono::DateTime<chrono::Utc>,
}

/// Statistics about the registry
#[derive(Debug, Clone, serde::Serialize)]
pub struct RegistryStatistics {
    pub total_services: usize,
    pub services_by_module: HashMap<String, usize>,
    pub services_by_type: HashMap<String, usize>,
}

// ============================================================
// Service Descriptor (for documentation/discovery)
// ============================================================

/// Metadata about a registered service
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceDescriptor {
    pub service_id: String,
    pub service_type: String,
    pub module_name: String,
    pub description: Option<String>,
    pub version: Option<String>,
}

impl ServiceDescriptor {
    pub fn from_service(service: &dyn ModuleService) -> Self {
        Self {
            service_id: service.service_id().to_string(),
            service_type: service.service_type().to_string(),
            module_name: service.module_name().to_string(),
            description: None,
            version: None,
        }
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct TestService {
        id: &'static str,
        healthy: bool,
    }

    #[async_trait]
    impl ModuleService for TestService {
        fn service_id(&self) -> &'static str {
            self.id
        }

        fn service_type(&self) -> &'static str {
            "test"
        }

        fn module_name(&self) -> &'static str {
            "test_module"
        }

        async fn health_check(&self) -> Result<ServiceHealth, String> {
            if self.healthy {
                Ok(ServiceHealth::healthy())
            } else {
                Err("Service unhealthy".to_string())
            }
        }
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = ServiceRegistry::new();

        let service = TestService {
            id: "test.service",
            healthy: true,
        };

        registry.register(service).await;

        assert!(registry.has("test.service").await);
        assert!(!registry.has("nonexistent").await);

        let retrieved = registry.get("test.service").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().service_id(), "test.service");
    }

    #[tokio::test]
    async fn test_list_services() {
        let registry = ServiceRegistry::new();

        registry.register(TestService { id: "test.a", healthy: true }).await;
        registry.register(TestService { id: "test.b", healthy: true }).await;

        let list = registry.list().await;
        assert_eq!(list.len(), 2);
        assert!(list.contains(&"test.a".to_string()));
        assert!(list.contains(&"test.b".to_string()));
    }

    #[tokio::test]
    async fn test_unregister() {
        let registry = ServiceRegistry::new();

        registry.register(TestService { id: "test.service", healthy: true }).await;
        assert!(registry.has("test.service").await);

        let removed = registry.unregister("test.service").await;
        assert!(removed);
        assert!(!registry.has("test.service").await);
    }

    #[tokio::test]
    async fn test_health_check_all() {
        let registry = ServiceRegistry::new();

        registry.register(TestService { id: "test.healthy", healthy: true }).await;
        registry.register(TestService { id: "test.unhealthy", healthy: false }).await;

        let health = registry.health_check_all().await;

        assert_eq!(health.total_services, 2);
        assert_eq!(health.healthy_count, 1);
        assert_eq!(health.unhealthy_count, 1);
        assert_eq!(health.status, HealthStatus::Degraded);
    }

    #[tokio::test]
    async fn test_statistics() {
        let registry = ServiceRegistry::new();

        registry.register(TestService { id: "test.a", healthy: true }).await;
        registry.register(TestService { id: "test.b", healthy: true }).await;

        let stats = registry.statistics().await;

        assert_eq!(stats.total_services, 2);
        assert_eq!(stats.services_by_module.get("test_module"), Some(&2));
        assert_eq!(stats.services_by_type.get("test"), Some(&2));
    }

    #[tokio::test]
    async fn test_list_by_module() {
        let registry = ServiceRegistry::new();

        registry.register(TestService { id: "test.a", healthy: true }).await;

        let by_module = registry.list_by_module("test_module").await;
        assert_eq!(by_module.len(), 1);

        let by_other = registry.list_by_module("other_module").await;
        assert!(by_other.is_empty());
    }

    #[test]
    fn test_service_health_builders() {
        let healthy = ServiceHealth::healthy();
        assert_eq!(healthy.status, HealthStatus::Healthy);
        assert!(healthy.message.is_none());

        let unhealthy = ServiceHealth::unhealthy("Something went wrong");
        assert_eq!(unhealthy.status, HealthStatus::Unhealthy);
        assert_eq!(unhealthy.message, Some("Something went wrong".to_string()));

        let degraded = ServiceHealth::degraded("Slow response")
            .with_detail("latency_ms", "500");
        assert_eq!(degraded.status, HealthStatus::Degraded);
        assert_eq!(degraded.details.get("latency_ms"), Some(&"500".to_string()));
    }
}
