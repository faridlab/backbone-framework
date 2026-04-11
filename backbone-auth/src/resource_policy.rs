//! Resource-level permission guards — bind auth context to entity-level access control.
//!
//! `ResourcePolicy<E>` is the auth-layer counterpart to `DomainPolicy<E>` in
//! `backbone-core`.  Where `DomainPolicy` enforces business invariants (is the
//! entity in a valid state for this operation?), `ResourcePolicy` enforces
//! **identity-based** rules (does *this caller* have permission to touch *this record*?).
//!
//! # Typical wiring
//!
//! ```text
//! HTTP handler
//!   → AuthMiddleware extracts AuthContext
//!   → PermissionGuard<E>::check(action, entity, auth_ctx)
//!       → ResourcePolicy<E>::can(action, entity, auth_ctx)  ← module implements this
//!   → If Err → 403 Forbidden
//!   → Else   → service.execute()
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! pub struct OrderResourcePolicy;
//!
//! #[async_trait]
//! impl ResourcePolicy<Order> for OrderResourcePolicy {
//!     async fn can(&self, action: ResourceAction, entity: &Order, ctx: &AuthContext) -> bool {
//!         match action {
//!             ResourceAction::Read => ctx.user_id == entity.customer_id || ctx.has_role("admin"),
//!             ResourceAction::Update | ResourceAction::Delete => ctx.user_id == entity.customer_id,
//!             _ => ctx.has_role("admin"),
//!         }
//!     }
//! }
//! ```

use async_trait::async_trait;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::middleware::AuthContext;

// ─── Resource actions ─────────────────────────────────────────────────────────

/// Standard CRUD operations that a resource policy can gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceAction {
    /// Read / fetch a single entity.
    Read,
    /// List / query multiple entities.
    List,
    /// Create a new entity.
    Create,
    /// Fully update an entity.
    Update,
    /// Partially update an entity.
    Patch,
    /// Soft-delete an entity.
    Delete,
    /// Restore a soft-deleted entity.
    Restore,
    /// Permanently delete an entity.
    HardDelete,
    /// Custom action for domain-specific operations.
    Custom(&'static str),
}

impl ResourceAction {
    pub fn name(&self) -> &'static str {
        match self {
            ResourceAction::Read => "read",
            ResourceAction::List => "list",
            ResourceAction::Create => "create",
            ResourceAction::Update => "update",
            ResourceAction::Patch => "patch",
            ResourceAction::Delete => "delete",
            ResourceAction::Restore => "restore",
            ResourceAction::HardDelete => "hard_delete",
            ResourceAction::Custom(name) => name,
        }
    }
}

// ─── Policy trait ─────────────────────────────────────────────────────────────

/// Determines whether the caller described by `auth_ctx` may perform `action`
/// on `entity`.
///
/// Return `true` to permit, `false` to deny.  Use `PermissionGuard` to convert
/// this into a `Result<(), AccessDenied>` suitable for HTTP handlers.
///
/// ## Static permission string methods
///
/// The `resource_type()` and `*_permission()` associated functions return the
/// permission string identifiers used by RBAC systems.  They have a
/// `where Self: Sized` bound so they can only be called in generic contexts
/// (not through `dyn ResourcePolicy`), which is intentional — the strings are
/// known at compile time and used by code generators and RBAC setup code.
///
/// ```rust,ignore
/// // Generated usage:
/// let required = OrderResourcePolicy::update_permission(); // "orders:update"
/// rbac.require_permission(ctx, required)?;
/// ```
#[async_trait]
pub trait ResourcePolicy<E: Send + Sync + 'static>: Send + Sync {
    // ── Static permission strings ─────────────────────────────────────────

    /// The resource type name used in permission strings.
    ///
    /// Default: `"resource"`. Override per entity, e.g. `"orders"`.
    fn resource_type() -> &'static str
    where
        Self: Sized,
    {
        "resource"
    }

    /// Permission string required to create this resource.
    fn create_permission() -> &'static str
    where
        Self: Sized,
    {
        "create"
    }

    /// Permission string required to read/fetch this resource.
    fn read_permission() -> &'static str
    where
        Self: Sized,
    {
        "read"
    }

    /// Permission string required to list this resource.
    fn list_permission() -> &'static str
    where
        Self: Sized,
    {
        "list"
    }

    /// Permission string required to fully update this resource.
    fn update_permission() -> &'static str
    where
        Self: Sized,
    {
        "update"
    }

    /// Permission string required to partially patch this resource.
    fn patch_permission() -> &'static str
    where
        Self: Sized,
    {
        "patch"
    }

    /// Permission string required to delete this resource.
    fn delete_permission() -> &'static str
    where
        Self: Sized,
    {
        "delete"
    }

    /// Permission string required to restore a soft-deleted resource.
    fn restore_permission() -> &'static str
    where
        Self: Sized,
    {
        "restore"
    }

    // ── Instance methods ──────────────────────────────────────────────────

    async fn can(
        &self,
        action: ResourceAction,
        entity: &E,
        ctx: &AuthContext,
    ) -> bool;

    /// Optional: deny specific actions for all callers (e.g. hard-delete disabled).
    fn explicitly_disabled_actions(&self) -> Vec<ResourceAction> {
        vec![]
    }
}

// ─── Access denied ────────────────────────────────────────────────────────────

/// Returned when a `PermissionGuard` denies an operation.
#[derive(Debug, thiserror::Error)]
#[error("access denied: caller '{caller}' may not perform '{action}' on this resource")]
pub struct AccessDenied {
    pub caller: String,
    pub action: String,
}

impl AccessDenied {
    pub fn new(caller: impl Into<String>, action: &ResourceAction) -> Self {
        Self {
            caller: caller.into(),
            action: action.name().into(),
        }
    }
}

impl Default for AccessDenied {
    fn default() -> Self {
        Self {
            caller: "anonymous".into(),
            action: "unknown".into(),
        }
    }
}

// ─── Permission guard ─────────────────────────────────────────────────────────

/// Wraps a `ResourcePolicy<E>` and enforces it, returning typed errors.
///
/// Inject one `Arc<PermissionGuard<E>>` per handler family.
pub struct PermissionGuard<E> {
    policy: Arc<dyn ResourcePolicy<E>>,
}

impl<E: Send + Sync + 'static> PermissionGuard<E> {
    pub fn new(policy: Arc<dyn ResourcePolicy<E>>) -> Self {
        Self { policy }
    }

    /// Check whether `ctx` may perform `action` on `entity`.
    ///
    /// Returns `Ok(())` on permit, `Err(AccessDenied)` on deny.
    pub async fn check(
        &self,
        action: ResourceAction,
        entity: &E,
        ctx: &AuthContext,
    ) -> Result<(), AccessDenied> {
        if self.policy.explicitly_disabled_actions().contains(&action) {
            return Err(AccessDenied::new(&ctx.user_id, &action));
        }

        if self.policy.can(action, entity, ctx).await {
            Ok(())
        } else {
            Err(AccessDenied::new(&ctx.user_id, &action))
        }
    }
}

// ─── AuthContextProvider ─────────────────────────────────────────────────────

/// Extracts or provides the current caller's `AuthContext`.
///
/// Implement this in your HTTP middleware or request-scope container so that
/// `ServicePermissionGuard` can retrieve the auth context without being coupled
/// to Axum or any other framework.
#[async_trait]
pub trait AuthContextProvider: Send + Sync {
    async fn current(&self) -> Option<AuthContext>;
}

// ─── ServicePermissionGuard ───────────────────────────────────────────────────

/// Generic permission guard that wraps both a service and an auth context provider.
///
/// Generated modules emit a type alias:
///
/// ```rust,ignore
/// // Generated (Phase 1):
/// impl ResourcePolicy<Order> for OrderPolicy {
///     fn resource_type() -> &'static str { "orders" }
///     fn create_permission() -> &'static str { "orders:create" }
///     // ...
///     async fn can(&self, action, entity, ctx) -> bool { ... }
/// }
///
/// pub type OrderGuard = ServicePermissionGuard<Order, OrderService, OrderPolicy>;
/// ```
///
/// `E` — entity type
/// `S` — underlying service
/// `P` — resource policy implementation
pub struct ServicePermissionGuard<E, S, P>
where
    E: Send + Sync + 'static,
    P: ResourcePolicy<E>,
{
    service: Arc<S>,
    policy: Arc<P>,
    _phantom: std::marker::PhantomData<E>,
}

impl<E, S, P> ServicePermissionGuard<E, S, P>
where
    E: Send + Sync + 'static,
    P: ResourcePolicy<E>,
{
    pub fn new(service: Arc<S>, policy: Arc<P>) -> Self {
        Self {
            service,
            policy,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Access the underlying service.
    pub fn service(&self) -> &Arc<S> {
        &self.service
    }

    /// Access the underlying policy.
    pub fn policy(&self) -> &Arc<P> {
        &self.policy
    }

    /// Check whether `ctx` may perform `action` on `entity`, returning
    /// `Ok(())` on permit or `Err(AccessDenied)` on deny.
    pub async fn check(
        &self,
        action: ResourceAction,
        entity: &E,
        ctx: &AuthContext,
    ) -> Result<(), AccessDenied> {
        if self.policy.explicitly_disabled_actions().contains(&action) {
            return Err(AccessDenied::new(&ctx.user_id, &action));
        }
        if self.policy.can(action, entity, ctx).await {
            Ok(())
        } else {
            Err(AccessDenied::new(&ctx.user_id, &action))
        }
    }
}

// ─── Built-in policies ───────────────────────────────────────────────────────

/// Permits every action for every caller.
/// Use as the generated default — replace in custom decorators.
pub struct PermitAllResourcePolicy<E> {
    _phantom: PhantomData<E>,
}

impl<E> PermitAllResourcePolicy<E> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<E> Default for PermitAllResourcePolicy<E> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<E: Send + Sync + 'static> ResourcePolicy<E> for PermitAllResourcePolicy<E> {
    async fn can(&self, _action: ResourceAction, _entity: &E, _ctx: &AuthContext) -> bool {
        true
    }
}

/// Denies every action for every caller.
/// Use for deprecated or not-yet-exposed resources.
pub struct DenyAllResourcePolicy<E> {
    _phantom: PhantomData<E>,
}

impl<E> DenyAllResourcePolicy<E> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<E: Send + Sync + 'static> ResourcePolicy<E> for DenyAllResourcePolicy<E> {
    async fn can(&self, _action: ResourceAction, _entity: &E, _ctx: &AuthContext) -> bool {
        false
    }
}

/// Requires the caller to have one of the listed roles to perform any action.
pub struct RoleRequiredPolicy<E> {
    required_roles: Vec<String>,
    _phantom: PhantomData<E>,
}

impl<E> RoleRequiredPolicy<E> {
    pub fn new(required_roles: Vec<impl Into<String>>) -> Self {
        Self {
            required_roles: required_roles.into_iter().map(|r| r.into()).collect(),
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<E: Send + Sync + 'static> ResourcePolicy<E> for RoleRequiredPolicy<E> {
    async fn can(&self, _action: ResourceAction, _entity: &E, ctx: &AuthContext) -> bool {
        self.required_roles
            .iter()
            .any(|role| ctx.roles.contains(role))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Document {
        owner_id: String,
    }

    struct OwnerPolicy;

    #[async_trait]
    impl ResourcePolicy<Document> for OwnerPolicy {
        async fn can(
            &self,
            _action: ResourceAction,
            entity: &Document,
            ctx: &AuthContext,
        ) -> bool {
            ctx.user_id == entity.owner_id
        }
    }

    fn auth_ctx(user_id: &str) -> AuthContext {
        AuthContext::new(user_id.to_string())
    }

    #[tokio::test]
    async fn owner_permitted_stranger_denied() {
        let guard = PermissionGuard::new(Arc::new(OwnerPolicy));
        let doc = Document {
            owner_id: "alice".into(),
        };

        assert!(guard
            .check(ResourceAction::Update, &doc, &auth_ctx("alice"))
            .await
            .is_ok());
        assert!(guard
            .check(ResourceAction::Update, &doc, &auth_ctx("bob"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn permit_all_always_ok() {
        let guard: PermissionGuard<Document> =
            PermissionGuard::new(Arc::new(PermitAllResourcePolicy::new()));
        let doc = Document {
            owner_id: "x".into(),
        };
        assert!(guard
            .check(ResourceAction::Delete, &doc, &auth_ctx("anyone"))
            .await
            .is_ok());
    }
}
