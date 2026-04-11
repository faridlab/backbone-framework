//! Cross-module integration — adapters, anti-corruption layer, and event bridges.
//!
//! Modules MUST NOT share entities.  When module A needs data from module B,
//! it defines an `External` projection and a `ModuleAdapter` that maps from
//! B's published types into A's internal types.
//!
//! # Pattern
//!
//! ```text
//! Module B publishes:  UserCreatedEvent { user_id, email }
//! Module A adapts:     CustomerCreatedEvent { customer_id, contact_email }
//!
//! impl ModuleAdapter<UserCreatedEvent, CustomerCreatedEvent>
//!     for SapiensToCorpusAdapter { ... }
//! ```
//!
//! This keeps bounded contexts decoupled while allowing event-driven integration.
//!
//! # Module identification
//!
//! Adapters expose `source_module()` and `target_module()` so the framework
//! can log, trace, and validate cross-context data flows.

use async_trait::async_trait;
use std::marker::PhantomData;

// ─── Core adapter trait ───────────────────────────────────────────────────────

/// Bidirectional adapter between an external module's type and this module's type.
///
/// - `External` — the foreign type (from another bounded context).
/// - `Internal`  — this module's representation.
///
/// Implement `source_module()` and `target_module()` to identify the bounded
/// contexts involved.  These are used for logging and cross-context tracing.
#[async_trait]
pub trait ModuleAdapter<External, Internal>: Send + Sync
where
    External: Send + 'static,
    Internal: Send + 'static,
{
    /// The error type for failed conversions.
    type Error: std::error::Error + Send + Sync;

    /// The bounded context that owns `External` (the source).
    ///
    /// Example: `"sapiens"` when the external type comes from the Sapiens module.
    fn source_module() -> &'static str
    where
        Self: Sized,
    {
        "unknown"
    }

    /// The bounded context that owns `Internal` (the target / this module).
    ///
    /// Example: `"corpus"` when this adapter lives in the Corpus module.
    fn target_module() -> &'static str
    where
        Self: Sized,
    {
        "unknown"
    }

    /// Convert an external type into this module's internal representation.
    async fn to_internal(&self, external: External) -> Result<Internal, Self::Error>;

    /// Convert this module's internal type into the external format (optional).
    ///
    /// Override when the module publishes events consumed by other modules.
    async fn to_external(&self, _internal: Internal) -> Result<External, Self::Error> {
        Err(self.not_implemented("to_external"))
    }

    fn not_implemented(&self, method: &str) -> Self::Error;
}

// ─── Synchronous projection adapter ──────────────────────────────────────────

/// Synchronous adapter for pure data projections (no async needed).
pub trait ProjectionAdapter<External, Internal>: Send + Sync {
    fn project(&self, external: &External) -> Internal;
}

// ─── Integration error ────────────────────────────────────────────────────────

/// Standard error for integration/adapter failures.
#[derive(Debug, thiserror::Error)]
pub enum IntegrationError {
    #[error("mapping failed: {0}")]
    MappingFailed(String),

    #[error("external service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("schema mismatch: {0}")]
    SchemaMismatch(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),
}

// ─── Event bridge ─────────────────────────────────────────────────────────────

/// Bridges domain events from module A into module B by adapting and re-publishing.
///
/// Wire this at startup to translate event streams across bounded context boundaries.
pub struct EventBridge<External, Internal, A>
where
    External: Send + 'static,
    Internal: Send + 'static,
    A: ModuleAdapter<External, Internal>,
{
    adapter: A,
    _phantom: PhantomData<(External, Internal)>,
}

impl<External, Internal, A> EventBridge<External, Internal, A>
where
    External: Send + 'static,
    Internal: Send + 'static,
    A: ModuleAdapter<External, Internal>,
{
    pub fn new(adapter: A) -> Self {
        Self {
            adapter,
            _phantom: PhantomData,
        }
    }

    /// Process one inbound external event, returning the adapted internal event.
    pub async fn process(
        &self,
        event: External,
    ) -> Result<Internal, <A as ModuleAdapter<External, Internal>>::Error> {
        self.adapter.to_internal(event).await
    }
}

// ─── Identity adapter (for same-module projections) ───────────────────────────

/// An adapter that maps a type to itself — useful when module A re-publishes
/// its own event with no transformation.
pub struct IdentityAdapter<T>(PhantomData<T>);

impl<T> IdentityAdapter<T> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T> Default for IdentityAdapter<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<T: Clone + Send + Sync + 'static> ModuleAdapter<T, T> for IdentityAdapter<T>
{
    type Error = IntegrationError;

    async fn to_internal(&self, external: T) -> Result<T, Self::Error> {
        Ok(external)
    }

    async fn to_external(&self, internal: T) -> Result<T, Self::Error> {
        Ok(internal)
    }

    fn not_implemented(&self, method: &str) -> Self::Error {
        IntegrationError::NotImplemented(method.into())
    }
}

/// Helper: build an `IdentityAdapter` without importing the type name.
pub fn identity_adapter<T: Clone + Send + Sync + 'static>() -> IdentityAdapter<T> {
    IdentityAdapter::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct ExternalUserEvent {
        user_id: String,
        email: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct InternalCustomerEvent {
        customer_id: String,
        contact_email: String,
    }

    struct UserToCustomerAdapter;

    #[async_trait]
    impl ModuleAdapter<ExternalUserEvent, InternalCustomerEvent> for UserToCustomerAdapter
    {
        type Error = IntegrationError;

        fn source_module() -> &'static str {
            "sapiens"
        }

        fn target_module() -> &'static str {
            "corpus"
        }

        async fn to_internal(
            &self,
            external: ExternalUserEvent,
        ) -> Result<InternalCustomerEvent, Self::Error> {
            Ok(InternalCustomerEvent {
                customer_id: external.user_id,
                contact_email: external.email,
            })
        }

        fn not_implemented(&self, method: &str) -> Self::Error {
            IntegrationError::NotImplemented(method.into())
        }
    }

    #[tokio::test]
    async fn adapter_maps_fields_correctly() {
        let bridge = EventBridge::new(UserToCustomerAdapter);
        let external = ExternalUserEvent {
            user_id: "u-1".into(),
            email: "user@example.com".into(),
        };

        let internal = bridge.process(external).await.unwrap();
        assert_eq!(internal.customer_id, "u-1");
        assert_eq!(internal.contact_email, "user@example.com");
    }

    #[tokio::test]
    async fn identity_adapter_roundtrips() {
        let adapter: IdentityAdapter<String> = IdentityAdapter::new();
        let value = "hello".to_string();
        let out = adapter.to_internal(value.clone()).await.unwrap();
        assert_eq!(out, value);
    }

    #[test]
    fn module_names_are_exposed() {
        assert_eq!(UserToCustomerAdapter::source_module(), "sapiens");
        assert_eq!(UserToCustomerAdapter::target_module(), "corpus");
    }
}
