//! Backbone Framework Auth
//!
//! Authentication and authorization system with JWT, password hashing, and RBAC.
//!
//! ## Generic Design
//!
//! This crate provides **generic traits** that modules can implement with their
//! own domain entities. This follows the framework rule: "Backbone is a GENERIC
//! library and must NEVER contain entity-specific code."
//!
//! ### Core Traits
//!
//! - `AuthenticatableUser` - Trait for user entities that can be authenticated
//! - `PermissionLike` - Trait for permission entities
//! - `RoleLike` - Trait for role entities with permissions
//! - `PermissionChecker` - Trait for permission checking services
//! - `UserRepository` - Generic repository trait for user operations
//!
//! ### Default Implementations
//!
//! For testing and simple use cases, default implementations are provided:
//! - `SimpleUser` - Default user struct implementing `AuthenticatableUser`
//! - `SimplePermission` - Default permission struct implementing `PermissionLike`
//! - `SimpleRole` - Default role struct implementing `RoleLike`
//! - `InMemoryPermissionService` - In-memory RBAC service
//!
//! ### Backwards Compatibility
//!
//! Type aliases are provided for backwards compatibility:
//! - `User` = `SimpleUser`
//! - `Permission` = `SimplePermission`
//! - `Role` = `SimpleRole`
//! - `PermissionService` = `InMemoryPermissionService<SimpleRole>`

pub mod audit;
pub mod auth_service;
pub mod jwt;
pub mod password;
pub mod permissions;
pub mod middleware;
pub mod token_generator;
pub mod traits;
pub mod resource_policy;
#[cfg(feature = "axum")]
pub mod company;
pub mod idempotency;

// Re-export commonly used types
pub use audit::AuditEvent;
pub use auth_service::*;
pub use jwt::*;
pub use password::*;
// `AuthContext` here is the IDENTITY context (`user_id` / `roles` / `permissions`) — the one
// `ResourcePolicy` and every generated `*_auth.rs` checks against. `traits` has a same-named struct
// carrying request forensics; it is re-exported below as `RequestAuthContext` so `AuthContext`
// resolves to exactly one type at this crate's root.
pub use middleware::{AuthMiddleware, AuthExtractor, AuthContext};
pub use token_generator::TokenGenerator;

/// The HTTP company guard (feature `axum`): derive `company_id` from a signed token, never a request body.
#[cfg(feature = "axum")]
pub use company::{company_auth, CompanyClaims, CompanyContext, CompanyVerifier};
pub use idempotency::{IdempotencyState, idempotency_middleware, migrate as migrate_idempotency};

// ── Backward-compatibility aliases (deprecated) ──
//
// These were `Tenant*` until ADR-0005 established that `company_id` is a legal-entity/books boundary,
// not the tenant (the tenant is the database). The rename is a breaking change for downstream
// consumers pinned to `main` — e.g. serpa-posman-service references `TenantVerifier`. These aliases
// keep such consumers compiling on sync, with a deprecation warning and a migration path. Remove them
// once no consumer references the old names.
#[cfg(feature = "axum")]
#[deprecated(note = "renamed to CompanyContext (ADR-0005: company_id is a legal-entity boundary, not the tenant)")]
pub use company::CompanyContext as TenantContext;
#[cfg(feature = "axum")]
#[deprecated(note = "renamed to CompanyVerifier (ADR-0005)")]
pub use company::CompanyVerifier as TenantVerifier;
#[cfg(feature = "axum")]
#[deprecated(note = "renamed to CompanyClaims (ADR-0005)")]
pub use company::CompanyClaims as TenantClaims;
#[cfg(feature = "axum")]
#[deprecated(note = "renamed to company_auth (ADR-0005)")]
pub use company::company_auth as tenant_auth;

// Re-export generic traits
pub use permissions::{
    PermissionLike, RoleLike, PermissionChecker,
    SimplePermission, SimpleRole, InMemoryPermissionService,
    // Backwards compatibility
    Permission, Role, PermissionService,
};

pub use traits::{
    AuthenticatableUser, SimpleUser,
    UserRepository, SecurityService,
    // Backwards compatibility
    User,
    // Other types
    RefreshTokenClaims, DeviceInfo, SecurityFlags, SecurityAlertType,
    PasswordPolicy, TwoFactorMethod, TwoFactorChallenge,
    PasswordResetRequest, PasswordResetConfirmation, AuthRequest, AuthResultEnhanced,
};

/// Request forensics (IP, user agent, device fingerprint, session) captured at authentication time.
///
/// Renamed on re-export: this used to be exported as `AuthContext`, which collided with the identity
/// context of the same name in `middleware` and silently shadowed it at the crate root — code that
/// wrote `use backbone_auth::AuthContext` and then read `.permissions` did not compile. Reach for
/// `AuthContext` for *who the caller is*, and this for *where the request came from*.
pub use traits::AuthContext as RequestAuthContext;

pub use resource_policy::{
    ResourceAction, ResourcePolicy, AccessDenied,
    // Simple policy-only guard (used in tests and simple scenarios)
    PermissionGuard,
    // Generic service-integrated guard (used in generated module code)
    ServicePermissionGuard, AuthContextProvider,
    PermitAllResourcePolicy, DenyAllResourcePolicy, RoleRequiredPolicy,
};

/// Auth version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");