//! Authorization types
//!
//! Core types for authorization system.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Authorization error
#[derive(Debug, Clone, thiserror::Error)]
pub enum AuthorizationError {
    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Database error: {0}")]
    Database(String),
}

/// User information with roles and permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub user_id: String,
    pub username: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub expires_at: Option<i64>, // Unix timestamp
}

/// Action types for permissions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Create,
    Read,
    Update,
    Delete,
    List,
    Restore,
}

impl Action {
    pub fn all() -> Vec<Self> {
        vec![
            Self::Create,
            Self::Read,
            Self::Update,
            Self::Delete,
            Self::List,
            Self::Restore,
        ]
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Create => write!(f, "create"),
            Self::Read => write!(f, "read"),
            Self::Update => write!(f, "update"),
            Self::Delete => write!(f, "delete"),
            Self::List => write!(f, "list"),
            Self::Restore => write!(f, "restore"),
        }
    }
}

/// Resource types for permissions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    User,
    Role,
    Permission,
    Settings,
}

impl Resource {
    pub fn all() -> Vec<Self> {
        vec![
            Self::User,
            Self::Role,
            Self::Permission,
            Self::Settings,
        ]
    }
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Role => write!(f, "role"),
            Self::Permission => write!(f, "permission"),
            Self::Settings => write!(f, "settings"),
        }
    }
}

/// Standard roles
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    SuperAdmin,
    Admin,
    User,
    Guest,
}

impl Role {
    pub fn all() -> Vec<&'static str> {
        vec![
            "super_admin",
            "admin",
            "user",
            "guest",
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SuperAdmin => "super_admin",
            Self::Admin => "admin",
            Self::User => "user",
            Self::Guest => "guest",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "super_admin" => Some(Self::SuperAdmin),
            "admin" => Some(Self::Admin),
            "user" => Some(Self::User),
            "guest" => Some(Self::Guest),
            _ => None,
        }
    }
}

/// Permission check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionCheck {
    pub allowed: bool,
    pub permission: Permission,
    pub reason: String,
}

/// Authorization request
#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    pub user: AuthUser,
    pub resource: Resource,
    pub action: Action,
    pub resource_id: Option<String>,
}

/// Authorization response
#[derive(Debug, Clone)]
pub struct AuthorizationResponse {
    pub allowed: bool,
    pub checks: Vec<PermissionCheck>,
}

/// Authorization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationConfig {
    pub cache_ttl_seconds: u64,
    pub default_role: String,
    pub enable_permission_caching: bool,
}

impl Default for AuthorizationConfig {
    fn default() -> Self {
        Self {
            cache_ttl_seconds: 300, // 5 minutes
            default_role: "guest".to_string(),
            enable_permission_caching: true,
        }
    }
}

/// User actions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserAction {
    Create,
    Read,
    Update,
    Delete,
    List,
    ResetPassword,
    ChangeRole,
}

/// Role actions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleAction {
    Create,
    Read,
    Update,
    Delete,
    List,
    AssignPermission,
    RevokePermission,
}

/// Permission actions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    Create,
    Read,
    Update,
    Delete,
    List,
}

/// Settings actions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsAction {
    Create,
    Read,
    Update,
    Delete,
    List,
}

/// Permission definition
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    User(UserAction),
    Role(RoleAction),
    Permission(PermissionAction),
    Settings(SettingsAction),
}

impl Permission {
    /// Get all possible permissions
    pub fn all_permissions() -> HashSet<Permission> {
        let mut perms = HashSet::new();

        // User permissions
        for action in UserAction::all() {
            perms.insert(Permission::User(action));
        }

        // Role permissions
        for action in RoleAction::all() {
            perms.insert(Permission::Role(action));
        }

        // Permission permissions
        for action in PermissionAction::all() {
            perms.insert(Permission::Permission(action));
        }

        // Settings permissions
        for action in SettingsAction::all() {
            perms.insert(Permission::Settings(action));
        }

        perms
    }

    /// Create permission from action and resource
    pub fn from_action_resource(action: Action, resource: Resource) -> Self {
        match resource {
            Resource::User => match action {
                Action::Create => Permission::User(UserAction::Create),
                Action::Read => Permission::User(UserAction::Read),
                Action::Update => Permission::User(UserAction::Update),
                Action::Delete => Permission::User(UserAction::Delete),
                Action::List => Permission::User(UserAction::List),
                Action::Restore => Permission::User(UserAction::ResetPassword),
            },
            Resource::Role => match action {
                Action::Create => Permission::Role(RoleAction::Create),
                Action::Read => Permission::Role(RoleAction::Read),
                Action::Update => Permission::Role(RoleAction::Update),
                Action::Delete => Permission::Role(RoleAction::Delete),
                Action::List => Permission::Role(RoleAction::List),
                Action::Restore => Permission::Role(RoleAction::AssignPermission),
            },
            Resource::Permission => match action {
                Action::Create => Permission::Permission(PermissionAction::Create),
                Action::Read => Permission::Permission(PermissionAction::Read),
                Action::Update => Permission::Permission(PermissionAction::Update),
                Action::Delete => Permission::Permission(PermissionAction::Delete),
                Action::List => Permission::Permission(PermissionAction::List),
                Action::Restore => Permission::Permission(PermissionAction::Update),
            },
            Resource::Settings => match action {
                Action::Create => Permission::Settings(SettingsAction::Create),
                Action::Read => Permission::Settings(SettingsAction::Read),
                Action::Update => Permission::Settings(SettingsAction::Update),
                Action::Delete => Permission::Settings(SettingsAction::Delete),
                Action::List => Permission::Settings(SettingsAction::List),
                Action::Restore => Permission::Settings(SettingsAction::Update),
            },
        }
    }
}

impl UserAction {
    pub fn all() -> Vec<Self> {
        vec![
            Self::Create,
            Self::Read,
            Self::Update,
            Self::Delete,
            Self::List,
            Self::ResetPassword,
            Self::ChangeRole,
        ]
    }
}

impl RoleAction {
    pub fn all() -> Vec<Self> {
        vec![
            Self::Create,
            Self::Read,
            Self::Update,
            Self::Delete,
            Self::List,
            Self::AssignPermission,
            Self::RevokePermission,
        ]
    }
}

impl PermissionAction {
    pub fn all() -> Vec<Self> {
        vec![
            Self::Create,
            Self::Read,
            Self::Update,
            Self::Delete,
            Self::List,
        ]
    }
}

impl SettingsAction {
    pub fn all() -> Vec<Self> {
        vec![
            Self::Create,
            Self::Read,
            Self::Update,
            Self::Delete,
            Self::List,
        ]
    }
}

use std::collections::HashSet;
