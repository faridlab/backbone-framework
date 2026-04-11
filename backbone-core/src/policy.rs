//! Domain policy — enforces pure business rules that govern whether CRUD
//! operations are valid for a given entity state.
//!
//! `DomainPolicy<E>` is intentionally free of authentication context.
//! It answers questions like "is this entity in a state that allows deletion?"
//! or "is this transition valid?".  Identity-based rules live in
//! `backbone-auth::resource_policy::ResourcePolicy<E>`.
//!
//! Policies are composable via `AllOfPolicy` / `AnyOfPolicy`.
//!
//! # Example
//!
//! ```rust,ignore
//! use backbone_core::policy::{DomainPolicy, PolicyDecision};
//!
//! struct OrderCancelPolicy;
//!
//! #[async_trait::async_trait]
//! impl DomainPolicy<Order> for OrderCancelPolicy {
//!     async fn can_delete(&self, entity: &Order) -> PolicyDecision {
//!         if entity.status == OrderStatus::Delivered {
//!             Err("cannot cancel a delivered order".into())
//!         } else {
//!             Ok(true)
//!         }
//!     }
//! }
//! ```

use async_trait::async_trait;
use std::marker::PhantomData;
use std::sync::Arc;

/// The result of a policy evaluation.
///
/// - `Ok(true)`  — operation is **permitted**.
/// - `Ok(false)` — operation is **denied** (soft deny, no message).
/// - `Err(msg)`  — operation is **denied** with a human-readable reason.
pub type PolicyDecision = Result<bool, String>;

// ─── PolicyContext (kept for compatibility / optional use in custom policies) ─

/// Contextual information available to custom policies that need it.
///
/// `DomainPolicy` methods do NOT take `PolicyContext` — this struct is
/// available for custom policy implementations that need richer context
/// beyond the entity state (e.g. tenant quota checks).
#[derive(Debug, Clone, Default)]
pub struct PolicyContext {
    /// The authenticated user's ID, if any.
    pub user_id: Option<String>,
    /// Roles held by the current principal.
    pub roles: Vec<String>,
    /// Permissions held by the current principal.
    pub permissions: Vec<String>,
    /// Arbitrary key-value metadata (tenant id, feature flags, etc.).
    pub metadata: std::collections::HashMap<String, String>,
}

impl PolicyContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.roles.push(role.into());
        self
    }

    pub fn with_permission(mut self, perm: impl Into<String>) -> Self {
        self.permissions.push(perm.into());
        self
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.iter().any(|p| p == perm)
    }
}

// ─── PolicyOutcome (kept for compatibility) ───────────────────────────────────

/// Legacy outcome type — retained for code that uses it directly.
/// Prefer `PolicyDecision` (`Result<bool, String>`) in new code.
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyOutcome {
    /// The operation is allowed.
    Permit,
    /// The operation is denied.  The string is a human-readable reason.
    Deny(String),
}

impl PolicyOutcome {
    pub fn is_permit(&self) -> bool {
        matches!(self, PolicyOutcome::Permit)
    }

    pub fn is_deny(&self) -> bool {
        matches!(self, PolicyOutcome::Deny(_))
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            PolicyOutcome::Deny(reason) => Some(reason.as_str()),
            PolicyOutcome::Permit => None,
        }
    }

    pub fn into_decision(self) -> PolicyDecision {
        match self {
            PolicyOutcome::Permit => Ok(true),
            PolicyOutcome::Deny(reason) => Err(reason),
        }
    }
}

// ─── DomainPolicy ─────────────────────────────────────────────────────────────

/// Core domain policy trait — governs whether CRUD operations are valid
/// for a given entity `E` based on **entity state alone** (no auth context).
///
/// All methods default to `Ok(true)` (permit).  Override only the methods
/// relevant to the entity's domain invariants.
#[async_trait]
pub trait DomainPolicy<E: Send + Sync + 'static>: Send + Sync {
    /// Can a new entity in the given state be persisted?
    async fn can_create(&self, _entity: &E) -> PolicyDecision {
        Ok(true)
    }

    /// Can the entity transition from `current` to `updated` state?
    async fn can_update(&self, _current: &E, _updated: &E) -> PolicyDecision {
        Ok(true)
    }

    /// Can the entity be soft-deleted?
    async fn can_delete(&self, _entity: &E) -> PolicyDecision {
        Ok(true)
    }

    /// Can a soft-deleted entity be restored?
    async fn can_restore(&self, _entity: &E) -> PolicyDecision {
        Ok(true)
    }

    /// Convenience: enforce `can_create` and map to `Ok(())` or `Err(reason)`.
    async fn enforce_create(&self, entity: &E) -> Result<(), String> {
        self.can_create(entity).await?.then_some(()).ok_or_else(|| "create denied".into())
    }

    /// Convenience: enforce `can_delete` and map to `Ok(())` or `Err(reason)`.
    async fn enforce_delete(&self, entity: &E) -> Result<(), String> {
        self.can_delete(entity).await?.then_some(()).ok_or_else(|| "delete denied".into())
    }
}

// ─── Built-in implementations ───────────────────────────────────────────────

/// Permits every operation — useful as the generated default.
/// Replace with a real policy in your custom decorator.
pub struct PermitAllPolicy<E> {
    _phantom: PhantomData<E>,
}

impl<E> PermitAllPolicy<E> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<E> Default for PermitAllPolicy<E> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<E: Send + Sync + 'static> DomainPolicy<E> for PermitAllPolicy<E> {
    // All methods use the `Ok(true)` defaults — no override needed.
}

/// Denies every operation — useful for protecting deprecated endpoints.
pub struct DenyAllPolicy<E> {
    reason: String,
    _phantom: PhantomData<E>,
}

impl<E> DenyAllPolicy<E> {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<E: Send + Sync + 'static> DomainPolicy<E> for DenyAllPolicy<E> {
    async fn can_create(&self, _entity: &E) -> PolicyDecision {
        Err(self.reason.clone())
    }
    async fn can_update(&self, _current: &E, _updated: &E) -> PolicyDecision {
        Err(self.reason.clone())
    }
    async fn can_delete(&self, _entity: &E) -> PolicyDecision {
        Err(self.reason.clone())
    }
    async fn can_restore(&self, _entity: &E) -> PolicyDecision {
        Err(self.reason.clone())
    }
}

/// Combines multiple policies with AND semantics:
/// all policies must permit an operation.
pub struct AllOfPolicy<E> {
    policies: Vec<Arc<dyn DomainPolicy<E>>>,
}

impl<E: Send + Sync + 'static> AllOfPolicy<E> {
    pub fn new(policies: Vec<Arc<dyn DomainPolicy<E>>>) -> Self {
        Self { policies }
    }
}

#[async_trait]
impl<E: Send + Sync + 'static> DomainPolicy<E> for AllOfPolicy<E> {
    async fn can_create(&self, entity: &E) -> PolicyDecision {
        for p in &self.policies {
            match p.can_create(entity).await {
                Ok(true) => {}
                other => return other,
            }
        }
        Ok(true)
    }

    async fn can_update(&self, current: &E, updated: &E) -> PolicyDecision {
        for p in &self.policies {
            match p.can_update(current, updated).await {
                Ok(true) => {}
                other => return other,
            }
        }
        Ok(true)
    }

    async fn can_delete(&self, entity: &E) -> PolicyDecision {
        for p in &self.policies {
            match p.can_delete(entity).await {
                Ok(true) => {}
                other => return other,
            }
        }
        Ok(true)
    }

    async fn can_restore(&self, entity: &E) -> PolicyDecision {
        for p in &self.policies {
            match p.can_restore(entity).await {
                Ok(true) => {}
                other => return other,
            }
        }
        Ok(true)
    }
}

/// Combines multiple policies with OR semantics:
/// any policy permitting is sufficient.
pub struct AnyOfPolicy<E> {
    policies: Vec<Arc<dyn DomainPolicy<E>>>,
    deny_reason: String,
}

impl<E: Send + Sync + 'static> AnyOfPolicy<E> {
    pub fn new(policies: Vec<Arc<dyn DomainPolicy<E>>>, deny_reason: impl Into<String>) -> Self {
        Self {
            policies,
            deny_reason: deny_reason.into(),
        }
    }
}

#[async_trait]
impl<E: Send + Sync + 'static> DomainPolicy<E> for AnyOfPolicy<E> {
    async fn can_create(&self, entity: &E) -> PolicyDecision {
        for p in &self.policies {
            if matches!(p.can_create(entity).await, Ok(true)) {
                return Ok(true);
            }
        }
        Err(self.deny_reason.clone())
    }

    async fn can_update(&self, current: &E, updated: &E) -> PolicyDecision {
        for p in &self.policies {
            if matches!(p.can_update(current, updated).await, Ok(true)) {
                return Ok(true);
            }
        }
        Err(self.deny_reason.clone())
    }

    async fn can_delete(&self, entity: &E) -> PolicyDecision {
        for p in &self.policies {
            if matches!(p.can_delete(entity).await, Ok(true)) {
                return Ok(true);
            }
        }
        Err(self.deny_reason.clone())
    }

    async fn can_restore(&self, entity: &E) -> PolicyDecision {
        for p in &self.policies {
            if matches!(p.can_restore(entity).await, Ok(true)) {
                return Ok(true);
            }
        }
        Err(self.deny_reason.clone())
    }
}

/// Inverts a policy.
pub struct NotPolicy<E> {
    inner: Arc<dyn DomainPolicy<E>>,
    deny_reason: String,
}

impl<E: Send + Sync + 'static> NotPolicy<E> {
    pub fn new(inner: Arc<dyn DomainPolicy<E>>, deny_reason: impl Into<String>) -> Self {
        Self {
            inner,
            deny_reason: deny_reason.into(),
        }
    }
}

#[async_trait]
impl<E: Send + Sync + 'static> DomainPolicy<E> for NotPolicy<E> {
    async fn can_create(&self, entity: &E) -> PolicyDecision {
        match self.inner.can_create(entity).await {
            Ok(true) => Err(self.deny_reason.clone()),
            _ => Ok(true),
        }
    }

    async fn can_delete(&self, entity: &E) -> PolicyDecision {
        match self.inner.can_delete(entity).await {
            Ok(true) => Err(self.deny_reason.clone()),
            _ => Ok(true),
        }
    }

    async fn can_update(&self, current: &E, updated: &E) -> PolicyDecision {
        match self.inner.can_update(current, updated).await {
            Ok(true) => Err(self.deny_reason.clone()),
            _ => Ok(true),
        }
    }

    async fn can_restore(&self, entity: &E) -> PolicyDecision {
        match self.inner.can_restore(entity).await {
            Ok(true) => Err(self.deny_reason.clone()),
            _ => Ok(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct FakeEntity {
        owner_id: String,
        is_locked: bool,
    }

    struct LockedPolicy;

    #[async_trait]
    impl DomainPolicy<FakeEntity> for LockedPolicy {
        async fn can_delete(&self, entity: &FakeEntity) -> PolicyDecision {
            if entity.is_locked {
                Err("entity is locked".into())
            } else {
                Ok(true)
            }
        }
    }

    #[tokio::test]
    async fn permit_all_always_permits() {
        let policy = PermitAllPolicy::<FakeEntity>::new();
        let entity = FakeEntity {
            owner_id: "u1".into(),
            is_locked: false,
        };
        assert!(matches!(policy.can_create(&entity).await, Ok(true)));
        assert!(matches!(policy.can_delete(&entity).await, Ok(true)));
    }

    #[tokio::test]
    async fn deny_all_always_denies_with_reason() {
        let policy = DenyAllPolicy::<FakeEntity>::new("deprecated");
        let entity = FakeEntity {
            owner_id: "u1".into(),
            is_locked: false,
        };
        assert!(policy.can_create(&entity).await.is_err());
        assert!(policy.can_delete(&entity).await.is_err());
    }

    #[tokio::test]
    async fn locked_policy_denies_delete_for_locked_entity() {
        let policy = LockedPolicy;
        let locked = FakeEntity {
            owner_id: "u1".into(),
            is_locked: true,
        };
        let unlocked = FakeEntity {
            owner_id: "u1".into(),
            is_locked: false,
        };
        assert!(policy.can_delete(&locked).await.is_err());
        assert!(matches!(policy.can_delete(&unlocked).await, Ok(true)));
    }

    #[tokio::test]
    async fn all_of_denies_when_any_denies() {
        let entity = FakeEntity {
            owner_id: "u1".into(),
            is_locked: true,
        };
        let policy: AllOfPolicy<FakeEntity> = AllOfPolicy::new(vec![
            Arc::new(PermitAllPolicy::new()),
            Arc::new(LockedPolicy),
        ]);
        assert!(policy.can_delete(&entity).await.is_err());
    }

    #[tokio::test]
    async fn any_of_permits_when_one_permits() {
        let entity = FakeEntity {
            owner_id: "u1".into(),
            is_locked: false,
        };
        let policy: AnyOfPolicy<FakeEntity> = AnyOfPolicy::new(
            vec![Arc::new(LockedPolicy)],
            "all policies denied",
        );
        assert!(matches!(policy.can_delete(&entity).await, Ok(true)));
    }
}
