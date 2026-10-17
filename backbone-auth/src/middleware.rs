//! Authentication middleware

use anyhow::Result;
use async_trait::async_trait;

/// Authentication middleware trait
#[async_trait]
pub trait AuthMiddleware {
    async fn authenticate(&self, token: &str) -> Result<AuthContext>;
}

/// Trait for extracting authentication information from requests
#[async_trait]
pub trait AuthExtractor {
    async fn extract_token(&self) -> Result<Option<String>>;
    async fn extract_user_context(&self) -> Result<Option<AuthContext>>;
}

/// Authentication context
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

impl AuthContext {
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            roles: Vec::new(),
            permissions: Vec::new(),
        }
    }

    /// Whether `permission` (a canonical `<resource>:<action>`) is among what the caller holds.
    ///
    /// Reads `self.permissions` and nothing else: with no grant resolver installed that is whatever
    /// the host's authentication put there, with one installed it is what the resolver loaded. An
    /// empty list holds nothing. Wildcards (`ticket:*`, `*:read`, `*:*`) and the legacy spellings
    /// are matched as [`crate::grants`] describes.
    pub fn has_permission(&self, permission: &str) -> bool {
        crate::grants::check_held(self.permissions.iter(), permission, &[])
    }

    /// Refuse unless the caller holds `permission`.
    ///
    /// The generated routes, transitions and verbs all call this. The error carries the permission,
    /// and answers `403` with the violation `permission_required` and `params.permission`.
    pub fn require(&self, permission: &str) -> Result<(), crate::grants::PermissionRequired> {
        self.require_with(permission, &[])
    }

    /// [`require`](Self::require), also accepting any of `alternates` (legacy spellings a route
    /// still honours for one release, such as the module-wide `<module>:<action>`). The refusal
    /// always names `permission`.
    pub fn require_with(
        &self,
        permission: &str,
        alternates: &[&str],
    ) -> Result<(), crate::grants::PermissionRequired> {
        if crate::grants::check_held(self.permissions.iter(), permission, alternates) {
            Ok(())
        } else {
            Err(crate::grants::PermissionRequired { permission: permission.to_string() })
        }
    }
}