//! Builder Pattern Dependency Injection
//!
//! This module provides the core components for flexible dependency injection
//! using the Builder pattern, allowing modules to be composed and configured
//! in a clean, type-safe manner.

use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use axum::Router;
use anyhow::Result;

#[cfg(feature = "database")]
use sqlx::PgPool;

/// Errors that can occur during module operations
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("Module initialization failed: {0}")]
    InitializationFailed(String),
    #[error("Module shutdown failed: {0}")]
    ShutdownFailed(String),
    #[error("Dependency injection failed: {0}")]
    DependencyInjectionFailed(String),
    #[error("Service not found: {0}")]
    ServiceNotFound(String),
}

/// Result type for module operations
pub type ModuleResult<T> = Result<T, ModuleError>;

/// Trait that all business modules must implement
///
/// This trait defines the contract for modules in the Backbone Framework,
/// enabling modular composition and lifecycle management.
#[async_trait]
pub trait Module: Send + Sync {
    /// Returns the name of the module
    fn name(&self) -> &'static str;

    /// Returns the version of the module
    fn version(&self) -> &'static str;

    /// Configures routes for this module
    ///
    /// # Arguments
    /// * `router` - The Axum router to configure routes on
    ///
    /// # Returns
    /// The configured router with module-specific routes added
    async fn configure_routes(&self, router: Router) -> Router;

    /// Configures services in the dependency injection container
    ///
    /// # Arguments
    /// * `container` - The service container to register services in
    async fn configure_services(&self, container: &mut ServiceContainer);

    /// Initializes the module
    ///
    /// This is called after all services are configured and before
    /// the module starts serving requests. Use this for any
    /// setup work that needs to be done.
    async fn initialize(&self) -> ModuleResult<()>;

    /// Shuts down the module
    ///
    /// This is called when the application is shutting down.
    /// Use this for cleanup work.
    async fn shutdown(&self) -> ModuleResult<()>;

    /// Performs a health check for this module
    ///
    /// # Returns
    /// true if the module is healthy, false otherwise
    async fn health_check(&self) -> bool {
        true // Default implementation assumes healthy
    }
}

/// Dependency injection container for managing services
///
/// This container provides a simple way to register and retrieve
/// services using type erasure. Services are stored as Arc<dyn Any>
/// and can be retrieved by their type.
#[derive(Debug, Default)]
pub struct ServiceContainer {
    services: HashMap<String, Arc<dyn std::any::Any + Send + Sync>>,
}

impl ServiceContainer {
    /// Creates a new service container
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }

    /// Registers a service in the container
    ///
    /// # Type Parameters
    /// * `T` - The service type to register
    ///
    /// # Arguments
    /// * `service` - The service instance to register
    pub fn register<T: 'static + Send + Sync>(&mut self, service: T) {
        let type_name = std::any::type_name::<T>().to_string();
        self.services.insert(type_name, Arc::new(service));
    }

    /// Registers an Arc<T> service in the container
    ///
    /// # Type Parameters
    /// * `T` - The service type to register
    ///
    /// # Arguments
    /// * `service` - The Arc-wrapped service instance to register
    pub fn register_arc<T: 'static + Send + Sync>(&mut self, service: Arc<T>) {
        let type_name = std::any::type_name::<T>().to_string();
        self.services.insert(type_name, service);
    }

    /// Retrieves a service from the container
    ///
    /// # Type Parameters
    /// * `T` - The service type to retrieve
    ///
    /// # Returns
    /// An Option containing a reference to the service if found
    pub fn get<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        let type_name = std::any::type_name::<T>();
        self.services
            .get(type_name)
            .and_then(|service| service.clone().downcast::<T>().ok())
    }

    /// Retrieves a service from the container, returning an error if not found
    ///
    /// # Type Parameters
    /// * `T` - The service type to retrieve
    ///
    /// # Returns
    /// A Result containing the service if found
    pub fn require<T: 'static + Send + Sync>(&self) -> ModuleResult<Arc<T>> {
        self.get::<T>()
            .ok_or_else(|| ModuleError::ServiceNotFound(std::any::type_name::<T>().to_string()))
    }

    /// Lists all registered service types
    pub fn list_services(&self) -> Vec<String> {
        self.services.keys().cloned().collect()
    }

    /// Checks if a service type is registered
    pub fn has<T: 'static + Send + Sync>(&self) -> bool {
        let type_name = std::any::type_name::<T>();
        self.services.contains_key(type_name)
    }
}

/// Application structure that holds modules and dependencies
///
/// This struct represents the main application with all its
/// modules, services, and database connections configured.
pub struct Application {
    modules: Vec<Box<dyn Module>>,
    service_container: ServiceContainer,
    #[cfg(feature = "database")]
    database_pool: Option<PgPool>,
    router: Router,
}

impl Application {
    /// Gets a reference to the service container
    pub fn service_container(&self) -> &ServiceContainer {
        &self.service_container
    }

    /// Gets a reference to the database pool
    #[cfg(feature = "database")]
    pub fn database_pool(&self) -> Option<&PgPool> {
        self.database_pool.as_ref()
    }

    /// Gets a reference to the configured router
    pub fn router(&self) -> &Router {
        &self.router
    }

    /// Performs health checks on all modules
    pub async fn health_check(&self) -> HashMap<String, bool> {
        let mut results = HashMap::new();

        for module in &self.modules {
            let healthy = module.health_check().await;
            results.insert(module.name().to_string(), healthy);
        }

        results
    }

    /// Runs the application on the specified address
    ///
    /// This starts the Axum HTTP server and blocks until shutdown.
    ///
    /// # Arguments
    /// * `addr` - The socket address to bind to (e.g., "0.0.0.0:3000")
    ///
    /// # Example
    /// ```ignore
    /// let app = ApplicationBuilder::new()
    ///     .with_module(my_module)
    ///     .build()
    ///     .await?;
    ///
    /// app.run("0.0.0.0:3000").await?;
    /// ```
    pub async fn run(self, addr: &str) -> ModuleResult<()> {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| ModuleError::InitializationFailed(format!("Failed to bind to {}: {}", addr, e)))?;

        tracing::info!("Starting server on {}", addr);

        axum::serve(listener, self.router)
            .await
            .map_err(|e| ModuleError::InitializationFailed(format!("Server error: {}", e)))?;

        Ok(())
    }

    /// Runs the application with graceful shutdown support
    ///
    /// This starts the Axum HTTP server and handles graceful shutdown
    /// when a shutdown signal is received.
    ///
    /// # Arguments
    /// * `addr` - The socket address to bind to
    /// * `shutdown_signal` - A future that completes when shutdown is requested
    ///
    /// # Example
    /// ```ignore
    /// let app = ApplicationBuilder::new()
    ///     .with_module(my_module)
    ///     .build()
    ///     .await?;
    ///
    /// app.run_with_shutdown("0.0.0.0:3000", async {
    ///     tokio::signal::ctrl_c().await.ok();
    /// }).await?;
    /// ```
    pub async fn run_with_shutdown<F>(self, addr: &str, shutdown_signal: F) -> ModuleResult<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| ModuleError::InitializationFailed(format!("Failed to bind to {}: {}", addr, e)))?;

        tracing::info!("Starting server on {} (with graceful shutdown)", addr);

        axum::serve(listener, self.router)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(|e| ModuleError::InitializationFailed(format!("Server error: {}", e)))?;

        // Shutdown all modules
        for module in &self.modules {
            if let Err(e) = module.shutdown().await {
                tracing::warn!("Module {} shutdown error: {}", module.name(), e);
            }
        }

        tracing::info!("Server shutdown complete");
        Ok(())
    }

    /// Returns the configured router for use with custom server setup
    ///
    /// Use this when you need more control over the server configuration.
    pub fn into_router(self) -> Router {
        self.router
    }
}

/// Builder for creating Application instances
///
/// This builder provides a fluent API for configuring and building
/// applications with their modules and dependencies.
pub struct ApplicationBuilder {
    modules: Vec<Box<dyn Module>>,
    #[cfg(feature = "database")]
    database_pool: Option<PgPool>,
    service_container: ServiceContainer,
}

impl ApplicationBuilder {
    /// Creates a new application builder
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
            #[cfg(feature = "database")]
            database_pool: None,
            service_container: ServiceContainer::new(),
        }
    }

    /// Adds a database connection pool to the application
    ///
    /// # Arguments
    /// * `pool` - The PostgreSQL connection pool
    #[cfg(feature = "database")]
    pub fn with_database(mut self, pool: PgPool) -> Self {
        self.database_pool = Some(pool);
        self
    }

    /// Adds a module to the application
    ///
    /// # Type Parameters
    /// * `M` - The module type (must implement Module)
    ///
    /// # Arguments
    /// * `module` - The module instance to add
    pub fn with_module<M: Module + 'static>(mut self, module: M) -> Self {
        self.modules.push(Box::new(module));
        self
    }

    /// Adds a service to the application's service container
    ///
    /// # Type Parameters
    /// * `T` - The service type
    ///
    /// # Arguments
    /// * `service` - The service instance
    pub fn with_service<T: 'static + Send + Sync>(mut self, service: T) -> Self {
        self.service_container.register(service);
        self
    }

    /// Adds an Arc-wrapped service to the application's service container
    ///
    /// # Type Parameters
    /// * `T` - The service type
    ///
    /// # Arguments
    /// * `service` - The Arc-wrapped service instance
    pub fn with_service_arc<T: 'static + Send + Sync>(mut self, service: Arc<T>) -> Self {
        self.service_container.register_arc(service);
        self
    }

    /// Builds the application
    ///
    /// This method:
    /// 1. Configures services for all modules
    /// 2. Initializes all modules
    /// 3. Configures routes from all modules
    ///
    /// # Returns
    /// A Result containing the built application
    pub async fn build(self) -> ModuleResult<Application> {
        let mut service_container = self.service_container;
        let mut router = Router::new();

        // Configure services for all modules
        for module in &self.modules {
            module.configure_services(&mut service_container).await;
        }

        // Initialize all modules
        for module in &self.modules {
            module.initialize().await?;
        }

        // Configure routes from all modules
        for module in &self.modules {
            router = module.configure_routes(router).await;
        }

        Ok(Application {
            modules: self.modules,
            service_container,
            #[cfg(feature = "database")]
            database_pool: self.database_pool,
            router,
        })
    }
}

impl Default for ApplicationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct TestService {
        counter: Arc<AtomicU32>,
    }

    struct TestModule {
        name: &'static str,
        version: &'static str,
    }

    #[async_trait]
    impl Module for TestModule {
        fn name(&self) -> &'static str {
            self.name
        }

        fn version(&self) -> &'static str {
            self.version
        }

        async fn configure_routes(&self, router: Router) -> Router {
            router
        }

        async fn configure_services(&self, container: &mut ServiceContainer) {
            // Configure test services
        }

        async fn initialize(&self) -> ModuleResult<()> {
            Ok(())
        }

        async fn shutdown(&self) -> ModuleResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_service_container() {
        let mut container = ServiceContainer::new();

        // Register a service
        container.register("test".to_string());

        // Retrieve the service
        let service: Option<Arc<String>> = container.get();
        assert!(service.is_some());
        assert_eq!(service.unwrap().as_str(), "test");
    }

    #[tokio::test]
    async fn test_application_builder() {
        let module = TestModule {
            name: "test",
            version: "1.0.0",
        };

        let builder = ApplicationBuilder::new()
            .with_module(module);

        let app = builder.build().await;
        assert!(app.is_ok());

        let app = app.unwrap();
        assert_eq!(app.modules.len(), 1);
    }
}