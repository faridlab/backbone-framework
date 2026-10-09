//! Authorization types
//!
//! Core types for authorization system.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Authorization error
#[derive(Debug, Clone, thiserror::Error)]
pub enum AuthorizationError {
    /// No user matches the given identifier.
    #[error("User not found: {0}")]
    UserNotFound(String),

    /// The user lacks the permission the request needs.
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// The presented token is malformed, expired or not trusted.
    #[error("Invalid token: {0}")]
    InvalidToken(String),

    /// The authorization setup itself is wrong.
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Reading roles or permissions from storage failed.
    #[error("Database error: {0}")]
    Database(String),
}

/// User information with roles and permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    /// The user's identifier.
    pub user_id: String,
    /// The user's login name.
    pub username: String,
    /// Role names the user holds.
    pub roles: Vec<String>,
    /// Permission names granted directly or through roles.
    pub permissions: Vec<String>,
    /// When the authorization expires, as a Unix timestamp in seconds.
    pub expires_at: Option<i64>,
}

/// Action types for permissions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Create a record.
    Create,
    /// Read one record.
    Read,
    /// Change a record.
    Update,
    /// Remove a record.
    Delete,
    /// List records.
    List,
    /// Bring back a removed record.
    Restore,
}

impl Action {
    /// Every action.
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
    /// User accounts.
    User,
    /// Roles.
    Role,
    /// Permission definitions.
    Permission,
    /// System settings.
    Settings,
}

impl Resource {
    /// Every resource.
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
    /// Every permission, without checks.
    SuperAdmin,
    /// Administers the system.
    Admin,
    /// A signed-in user.
    User,
    /// An anonymous or unprivileged visitor.
    Guest,
}

impl Role {
    /// The names of every standard role.
    pub fn all() -> Vec<&'static str> {
        vec![
            "super_admin",
            "admin",
            "user",
            "guest",
        ]
    }

    /// The role's name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SuperAdmin => "super_admin",
            Self::Admin => "admin",
            Self::User => "user",
            Self::Guest => "guest",
        }
    }

    /// The standard role with this name, if any.
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
    /// Whether the permission was granted.
    pub allowed: bool,
    /// The permission checked.
    pub permission: Permission,
    /// Why it was granted or refused.
    pub reason: String,
}

/// Authorization request
#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    /// Who is asking.
    pub user: AuthUser,
    /// What kind of record the action touches.
    pub resource: Resource,
    /// What the user wants to do.
    pub action: Action,
    /// The one record, when the action targets one.
    pub resource_id: Option<String>,
}

/// Authorization response
#[derive(Debug, Clone)]
pub struct AuthorizationResponse {
    /// Whether the request is allowed.
    pub allowed: bool,
    /// Each permission checked on the way to the answer.
    pub checks: Vec<PermissionCheck>,
}

/// Authorization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationConfig {
    /// How long a cached permission answer stays valid, in seconds.
    pub cache_ttl_seconds: u64,
    /// The role a user without any role is treated as.
    pub default_role: String,
    /// Whether permission answers are cached.
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
    /// Create one.
    Create,
    /// Read one.
    Read,
    /// Change one.
    Update,
    /// Remove one.
    Delete,
    /// List them.
    List,
    /// Reset the user's password.
    ResetPassword,
    /// Change the user's roles.
    ChangeRole,
}

/// Role actions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleAction {
    /// Create one.
    Create,
    /// Read one.
    Read,
    /// Change one.
    Update,
    /// Remove one.
    Delete,
    /// List them.
    List,
    /// Grant a permission to the role.
    AssignPermission,
    /// Take a permission from the role.
    RevokePermission,
}

/// Permission actions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    /// Create one.
    Create,
    /// Read one.
    Read,
    /// Change one.
    Update,
    /// Remove one.
    Delete,
    /// List them.
    List,
}

/// Settings actions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsAction {
    /// Create one.
    Create,
    /// Read one.
    Read,
    /// Change one.
    Update,
    /// Remove one.
    Delete,
    /// List them.
    List,
}

/// Permission definition
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// An action on user accounts.
    User(UserAction),
    /// An action on roles.
    Role(RoleAction),
    /// An action on permission definitions.
    Permission(PermissionAction),
    /// An action on system settings.
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
    /// Every user action.
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
    /// Every role action.
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
    /// Every permission action.
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
    /// Every settings action.
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
