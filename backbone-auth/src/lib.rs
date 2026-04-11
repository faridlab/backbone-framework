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

// Re-export commonly used types
pub use audit::AuditEvent;
pub use auth_service::*;
pub use jwt::*;
pub use password::*;
pub use middleware::{AuthMiddleware, AuthExtractor};
pub use token_generator::TokenGenerator;

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
    AuthContext, PasswordPolicy, TwoFactorMethod, TwoFactorChallenge,
    PasswordResetRequest, PasswordResetConfirmation, AuthRequest, AuthResultEnhanced,
};

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