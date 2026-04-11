//! Domain Service Trait
//!
//! Base trait for all domain services in the Backbone Framework.
//! Domain services encapsulate business logic that doesn't naturally
//! belong to a single entity.
//!
//! # Example
//!
//! ```ignore
//! use backbone_core::DomainService;
//!
//! pub struct PaymentService {
//!     // dependencies
//! }
//!
//! #[async_trait::async_trait]
//! impl DomainService for PaymentService {
//!     fn service_id(&self) -> &'static str {
//!         "payment.payment_service"
//!     }
//!
//!     async fn health_check(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!         // Check payment gateway connectivity
//!         Ok(())
//!     }
//! }
//! ```

use async_trait::async_trait;

/// Base trait for all domain services.
///
/// Domain services are stateless operations that coordinate between
/// multiple entities or handle complex business logic.
#[async_trait]
pub trait DomainService: Send + Sync {
    /// Unique identifier for this service.
    ///
    /// Format: `{module}.{service_name}`
    /// Example: `"corpus.organization_service"`
    fn service_id(&self) -> &'static str;

    /// Perform a health check for this service.
    ///
    /// Returns `Ok(())` if the service is healthy,
    /// or an error describing the issue.
    async fn health_check(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    /// Get the service name (derived from service_id).
    fn service_name(&self) -> &'static str {
        self.service_id()
            .split('.')
            .next_back()
            .unwrap_or(self.service_id())
    }

    /// Get the module name (derived from service_id).
    fn module_name(&self) -> &'static str {
        self.service_id()
            .split('.')
            .next()
            .unwrap_or("unknown")
    }
}

/// Marker trait for domain services that support transactions.
#[async_trait]
pub trait TransactionalDomainService: DomainService {
    /// Execute an operation within a transaction context.
    async fn with_transaction<F, R, E>(&self, operation: F) -> Result<R, E>
    where
        F: FnOnce() -> Result<R, E> + Send,
        R: Send,
        E: Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestService;

    #[async_trait]
    impl DomainService for TestService {
        fn service_id(&self) -> &'static str {
            "test.test_service"
        }
    }

    #[test]
    fn test_service_name_extraction() {
        let service = TestService;
        assert_eq!(service.service_name(), "test_service");
        assert_eq!(service.module_name(), "test");
    }
}
