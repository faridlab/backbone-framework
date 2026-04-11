//! Authorization traits
//!
//! Defines the core authorization trait that all authorization services must implement.

use crate::types::*;
use async_trait::async_trait;

/// Authorization service trait
#[async_trait]
pub trait AuthorizationServiceTrait: Send + Sync {
    /// Check if a user is authorized to perform an action on a resource
    async fn check_authorization(
        &self,
        request: AuthorizationRequest,
    ) -> Result<AuthorizationResponse, AuthorizationError>;

    /// Grant a permission to a role
    async fn grant_permission_to_role(
        &self,
        role: &str,
        permission: Permission,
    ) -> Result<(), AuthorizationError>;

    /// Revoke a permission from a role
    async fn revoke_permission_from_role(
        &self,
        role: &str,
        permission: &Permission,
    ) -> Result<(), AuthorizationError>;

    /// Assign a role to a user
    async fn assign_role_to_user(
        &self,
        user_id: &str,
        role: &str,
    ) -> Result<(), AuthorizationError>;

    /// Remove a role from a user
    async fn remove_role_from_user(
        &self,
        user_id: &str,
        role: &str,
    ) -> Result<(), AuthorizationError>;

    /// Get all permissions for a user
    async fn get_user_permissions(
        &self,
        user_id: &str,
    ) -> Result<Vec<Permission>, AuthorizationError>;

    /// Check if user has a specific permission
    async fn user_has_permission(
        &self,
        user_id: &str,
        permission: &Permission,
    ) -> Result<bool, AuthorizationError>;
}
