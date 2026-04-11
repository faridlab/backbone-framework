//! Permission and role-based access control (RBAC)
//!
//! This module provides a **generic** RBAC system using traits.
//! Modules can implement these traits with their own domain entities.
//!
//! ## Generic Design
//!
//! Instead of hardcoded entity structs, this module uses traits:
//! - `PermissionLike` - Any type representing a permission
//! - `RoleLike` - Any type representing a role with permissions
//! - `PermissionChecker` - Trait for checking permissions
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Module implements traits for its domain entities
//! impl PermissionLike for MyPermission {
//!     fn name(&self) -> &str { &self.name }
//!     fn resource(&self) -> &str { &self.resource }
//!     fn action(&self) -> &str { &self.action }
//! }
//! ```

use anyhow::Result;
use std::collections::HashMap;

// ============================================================================
// GENERIC TRAITS - Modules implement these with their domain entities
// ============================================================================

/// Trait for permission-like types
///
/// Implement this trait for your domain Permission entity to use with RBAC.
pub trait PermissionLike: Clone + Send + Sync {
    /// Permission name (e.g., "user:read", "admin:all")
    fn name(&self) -> &str;

    /// Resource this permission applies to (e.g., "user", "*")
    fn resource(&self) -> &str;

    /// Action allowed (e.g., "read", "write", "*")
    fn action(&self) -> &str;

    /// Optional description
    fn description(&self) -> Option<&str> { None }
}

/// Trait for role-like types
///
/// Implement this trait for your domain Role entity.
pub trait RoleLike: Clone + Send + Sync {
    /// The permission type this role uses
    type Permission: PermissionLike;

    /// Role name (e.g., "admin", "user")
    fn name(&self) -> &str;

    /// Optional description
    fn description(&self) -> Option<&str> { None }

    /// Permissions assigned to this role
    fn permissions(&self) -> &[Self::Permission];
}

/// Trait for checking permissions
///
/// Implement this for your permission checking service.
pub trait PermissionChecker: Send + Sync {
    /// Check if a user has a specific permission
    fn has_permission(&self, user_id: &str, permission: &str) -> Result<bool>;

    /// Check if a user has a specific role
    fn has_role(&self, user_id: &str, role_name: &str) -> Result<bool>;

    /// Get all roles for a user
    fn get_user_roles(&self, user_id: &str) -> Vec<String>;
}

// ============================================================================
// DEFAULT IMPLEMENTATIONS - Simple in-memory RBAC for testing/demos
// ============================================================================

/// Simple permission for default RBAC implementation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimplePermission {
    pub name: String,
    pub resource: String,
    pub action: String,
    pub description: Option<String>,
}

impl PermissionLike for SimplePermission {
    fn name(&self) -> &str { &self.name }
    fn resource(&self) -> &str { &self.resource }
    fn action(&self) -> &str { &self.action }
    fn description(&self) -> Option<&str> { self.description.as_deref() }
}

/// Simple role for default RBAC implementation
#[derive(Debug, Clone)]
pub struct SimpleRole {
    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<SimplePermission>,
}

impl RoleLike for SimpleRole {
    type Permission = SimplePermission;

    fn name(&self) -> &str { &self.name }
    fn description(&self) -> Option<&str> { self.description.as_deref() }
    fn permissions(&self) -> &[SimplePermission] { &self.permissions }
}

/// In-memory permission service for testing and simple use cases
///
/// For production, modules should implement their own PermissionChecker
/// backed by a database.
pub struct InMemoryPermissionService<R: RoleLike = SimpleRole> {
    roles: HashMap<String, R>,
    user_roles: HashMap<String, Vec<String>>, // user_id -> role_names
}

impl InMemoryPermissionService<SimpleRole> {
    /// Create a new in-memory permission service with default roles
    pub fn new() -> Self {
        let mut roles = HashMap::new();

        // Add default roles
        roles.insert("admin".to_string(), SimpleRole {
            name: "admin".to_string(),
            description: Some("Administrator with full access".to_string()),
            permissions: vec![
                SimplePermission {
                    name: "admin:all".to_string(),
                    description: Some("Full administrative access".to_string()),
                    resource: "*".to_string(),
                    action: "*".to_string(),
                },
            ],
        });

        roles.insert("user".to_string(), SimpleRole {
            name: "user".to_string(),
            description: Some("Regular user with basic permissions".to_string()),
            permissions: vec![
                SimplePermission {
                    name: "user:read".to_string(),
                    description: Some("Read own user data".to_string()),
                    resource: "user".to_string(),
                    action: "read".to_string(),
                },
                SimplePermission {
                    name: "user:write".to_string(),
                    description: Some("Update own user data".to_string()),
                    resource: "user".to_string(),
                    action: "write".to_string(),
                },
            ],
        });

        Self {
            roles,
            user_roles: HashMap::new(),
        }
    }
}

impl<R: RoleLike> InMemoryPermissionService<R> {
    /// Create an empty permission service
    pub fn empty() -> Self {
        Self {
            roles: HashMap::new(),
            user_roles: HashMap::new(),
        }
    }

    /// Check if user has permission using generic role type
    fn check_permission_internal(&self, user_id: &str, permission: &str) -> Result<bool> {
        let user_role_names = match self.user_roles.get(user_id) {
            Some(roles) => roles,
            None => return Ok(false),
        };

        for role_name in user_role_names {
            if let Some(role) = self.roles.get(role_name) {
                for perm in role.permissions() {
                    // Check exact match
                    if perm.name() == permission {
                        return Ok(true);
                    }

                    // Check wildcard permissions
                    if perm.resource() == "*" && perm.action() == "*" {
                        return Ok(true);
                    }

                    // Check resource:action format
                    let required_parts: Vec<&str> = permission.split(':').collect();
                    let perm_parts: Vec<&str> = perm.name().split(':').collect();

                    if required_parts.len() == 2 && perm_parts.len() == 2 {
                        let resource_match = perm_parts[0] == "*" || perm_parts[0] == required_parts[0];
                        let action_match = perm_parts[1] == "*" || perm_parts[1] == required_parts[1];

                        if resource_match && action_match {
                            return Ok(true);
                        }
                    }
                }
            }
        }

        Ok(false)
    }

    /// Get user permissions as SimplePermission (for backwards compatibility)
    pub fn get_user_permissions(&self, user_id: &str) -> Result<Vec<SimplePermission>> {
        let mut permissions = Vec::new();
        let mut permission_names = std::collections::HashSet::new();

        let user_role_names = match self.user_roles.get(user_id) {
            Some(roles) => roles,
            None => return Ok(permissions),
        };

        for role_name in user_role_names {
            if let Some(role) = self.roles.get(role_name) {
                for perm in role.permissions() {
                    if permission_names.insert(perm.name().to_string()) {
                        permissions.push(SimplePermission {
                            name: perm.name().to_string(),
                            resource: perm.resource().to_string(),
                            action: perm.action().to_string(),
                            description: perm.description().map(String::from),
                        });
                    }
                }
            }
        }

        Ok(permissions)
    }

    /// Add role
    pub fn add_role(&mut self, role: R) -> Result<()> {
        self.roles.insert(role.name().to_string(), role);
        Ok(())
    }

    /// Get role
    pub fn get_role(&self, role_name: &str) -> Option<&R> {
        self.roles.get(role_name)
    }

    /// List all roles
    pub fn list_roles(&self) -> Vec<&R> {
        self.roles.values().collect()
    }

    /// Assign role to user
    pub fn assign_role(&mut self, user_id: &str, role_name: &str) -> Result<()> {
        if !self.roles.contains_key(role_name) {
            return Err(anyhow::anyhow!("Role '{}' does not exist", role_name));
        }

        let user_roles = self.user_roles.entry(user_id.to_string()).or_default();

        if user_roles.contains(&role_name.to_string()) {
            return Err(anyhow::anyhow!("User already has role '{}'", role_name));
        }

        user_roles.push(role_name.to_string());
        Ok(())
    }

    /// Remove role from user
    pub fn remove_role(&mut self, user_id: &str, role_name: &str) -> Result<()> {
        let user_roles = self.user_roles.get_mut(user_id)
            .ok_or_else(|| anyhow::anyhow!("User has no roles assigned"))?;

        let original_len = user_roles.len();
        user_roles.retain(|r| r != role_name);

        if user_roles.len() == original_len {
            return Err(anyhow::anyhow!("User does not have role '{}'", role_name));
        }

        if user_roles.is_empty() {
            self.user_roles.remove(user_id);
        }

        Ok(())
    }

    /// Check if user has specific role
    fn has_role_internal(&self, user_id: &str, role_name: &str) -> bool {
        self.user_roles.get(user_id)
            .map(|roles| roles.contains(&role_name.to_string()))
            .unwrap_or(false)
    }

    /// Get user roles (internal)
    fn get_user_roles_internal(&self, user_id: &str) -> Vec<String> {
        self.user_roles.get(user_id)
            .cloned()
            .unwrap_or_default()
    }
}

// Implement PermissionChecker trait for InMemoryPermissionService
impl<R: RoleLike> PermissionChecker for InMemoryPermissionService<R> {
    fn has_permission(&self, user_id: &str, permission: &str) -> Result<bool> {
        self.check_permission_internal(user_id, permission)
    }

    fn has_role(&self, user_id: &str, role_name: &str) -> Result<bool> {
        Ok(self.has_role_internal(user_id, role_name))
    }

    fn get_user_roles(&self, user_id: &str) -> Vec<String> {
        self.get_user_roles_internal(user_id)
    }
}

impl Default for InMemoryPermissionService<SimpleRole> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// BACKWARDS COMPATIBILITY - Type aliases for existing code
// ============================================================================

/// Type alias for backwards compatibility
///
/// DEPRECATED: Use `InMemoryPermissionService` directly or implement
/// `PermissionChecker` trait for your own service.
pub type PermissionService = InMemoryPermissionService<SimpleRole>;

/// Type alias for backwards compatibility
///
/// DEPRECATED: Use `SimplePermission` or implement `PermissionLike` trait.
pub type Permission = SimplePermission;

/// Type alias for backwards compatibility
///
/// DEPRECATED: Use `SimpleRole` or implement `RoleLike` trait.
pub type Role = SimpleRole;