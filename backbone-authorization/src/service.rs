//! Authorization service implementation
//!
//! Provides RBAC authorization with pluggable permission caching.
//! Default uses in-memory cache; enable `redis` feature for distributed caching.

use crate::cache::{InMemoryPermissionCache, PermissionCacheBackend};
use crate::traits::*;
use crate::types::*;
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Default authorization service with in-memory storage
pub struct AuthorizationService {
    /// Role-based permissions storage
    role_permissions: Arc<RwLock<HashMap<String, HashSet<Permission>>>>,

    /// User roles storage
    user_roles: Arc<RwLock<HashMap<String, HashSet<String>>>>,

    /// Pluggable permission cache backend
    cache: Arc<dyn PermissionCacheBackend>,

    /// Configuration
    config: AuthorizationConfig,
}

impl Default for AuthorizationService {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthorizationService {
    /// Create a new authorization service with default roles and in-memory cache
    pub fn new() -> Self {
        Self::with_cache(Arc::new(InMemoryPermissionCache::new()))
    }

    /// Create a new authorization service with a custom cache backend
    pub fn with_cache(cache: Arc<dyn PermissionCacheBackend>) -> Self {
        Self::with_config(AuthorizationConfig::default(), cache)
    }

    /// Create a new authorization service with custom config and cache backend
    pub fn with_config(config: AuthorizationConfig, cache: Arc<dyn PermissionCacheBackend>) -> Self {
        let service = Self {
            role_permissions: Arc::new(RwLock::new(HashMap::new())),
            user_roles: Arc::new(RwLock::new(HashMap::new())),
            cache,
            config,
        };

        // Initialize with default roles and permissions
        let role_perms = service.role_permissions.clone();
        tokio::spawn(async move {
            Self::initialize_default_roles_static(role_perms).await;
        });

        service
    }

    /// Initialize default roles and permissions (static version for spawn)
    async fn initialize_default_roles_static(
        role_permissions: Arc<RwLock<HashMap<String, HashSet<Permission>>>>,
    ) {
        info!("Initializing default RBAC roles and permissions");

        let mut role_perms = role_permissions.write().await;

        // Super Admin - all permissions
        let super_admin_perms: HashSet<Permission> = Permission::all_permissions();
        role_perms.insert("super_admin".to_string(), super_admin_perms);

        // Admin - most permissions except user management
        let admin_perms: HashSet<Permission> = Permission::all_permissions()
            .into_iter()
            .filter(|p| !matches!(p, Permission::User(_)))
            .collect();
        role_perms.insert("admin".to_string(), admin_perms);

        // User - basic read and self-modify permissions
        let user_perms: HashSet<Permission> = [
            Permission::User(UserAction::Read),
            Permission::Settings(SettingsAction::Read),
            Permission::Role(RoleAction::Read),
            Permission::Permission(PermissionAction::Read),
        ]
        .into_iter()
        .collect();
        role_perms.insert("user".to_string(), user_perms);

        // Guest - read-only
        let guest_perms: HashSet<Permission> = [
            Permission::User(UserAction::Read),
            Permission::Settings(SettingsAction::Read),
        ]
        .into_iter()
        .collect();
        role_perms.insert("guest".to_string(), guest_perms);

        info!("Initialized {} default roles", role_perms.len());
    }

    /// Get cached permissions for a user, or fetch if not cached
    async fn get_cached_permissions(&self, user_id: &str) -> HashSet<Permission> {
        // Check cache first
        if self.config.enable_permission_caching {
            if let Ok(Some(permissions)) = self.cache.get(user_id).await {
                debug!("Using cached permissions for user '{}'", user_id);
                return permissions;
            }
        }

        // Fetch fresh permissions
        let permissions = self.fetch_user_permissions(user_id).await;

        // Cache with configured TTL
        if self.config.enable_permission_caching {
            if let Err(e) = self.cache.set(user_id, &permissions, self.config.cache_ttl_seconds).await {
                tracing::warn!("Failed to cache permissions for user '{}': {}", user_id, e);
            }
        }

        permissions
    }

    /// Fetch all permissions for a user from their roles
    async fn fetch_user_permissions(&self, user_id: &str) -> HashSet<Permission> {
        let roles = self.user_roles.read().await;
        let user_roles = roles.get(user_id).cloned().unwrap_or_default();

        let role_perms = self.role_permissions.read().await;
        let mut permissions = HashSet::new();

        for role in user_roles {
            if let Some(role_perms) = role_perms.get(&role) {
                permissions.extend(role_perms.clone());
            }
        }

        // Default to guest permissions if no roles
        if permissions.is_empty() {
            if let Some(guest_perms) = role_perms.get("guest") {
                permissions.extend(guest_perms.clone());
            }
        }

        permissions
    }

    /// Clear permission cache for a user
    pub async fn clear_user_cache(&self, user_id: &str) {
        if let Err(e) = self.cache.delete(user_id).await {
            tracing::warn!("Failed to clear cache for user '{}': {}", user_id, e);
        }
        debug!("Cleared permission cache for user '{}'", user_id);
    }

    /// Clear all permission caches
    pub async fn clear_all_cache(&self) {
        if let Err(e) = self.cache.clear().await {
            tracing::warn!("Failed to clear all permission caches: {}", e);
        }
        info!("Cleared all permission caches");
    }
}

#[async_trait]
impl AuthorizationServiceTrait for AuthorizationService {
    async fn check_authorization(
        &self,
        request: AuthorizationRequest,
    ) -> Result<AuthorizationResponse, AuthorizationError> {
        let user_id = &request.user.user_id;

        // Get user's permissions
        let permissions = self.get_cached_permissions(user_id).await;

        // Check if user has the required permission
        let required_permission = Permission::from_action_resource(
            request.action.clone(),
            request.resource.clone(),
        );

        let has_permission = permissions.contains(&required_permission);

        let response = AuthorizationResponse {
            allowed: has_permission,
            checks: vec![PermissionCheck {
                allowed: has_permission,
                permission: required_permission.clone(),
                reason: if has_permission {
                    "Permission granted".to_string()
                } else {
                    format!("Missing permission: {:?}", required_permission)
                },
            }],
        };

        debug!(
            "Authorization check for user '{}': {}",
            user_id,
            if has_permission { "ALLOWED" } else { "DENIED" }
        );

        Ok(response)
    }

    async fn grant_permission_to_role(
        &self,
        role: &str,
        permission: Permission,
    ) -> Result<(), AuthorizationError> {
        info!("Granted permission {:?} to role '{}'", permission, role);

        let mut role_perms = self.role_permissions.write().await;
        role_perms
            .entry(role.to_string())
            .or_insert_with(HashSet::new)
            .insert(permission);

        Ok(())
    }

    async fn revoke_permission_from_role(
        &self,
        role: &str,
        permission: &Permission,
    ) -> Result<(), AuthorizationError> {
        let mut role_perms = self.role_permissions.write().await;
        if let Some(perms) = role_perms.get_mut(role) {
            perms.remove(permission);
            info!("Revoked permission {:?} from role '{}'", permission, role);
        }
        Ok(())
    }

    async fn assign_role_to_user(
        &self,
        user_id: &str,
        role: &str,
    ) -> Result<(), AuthorizationError> {
        let mut user_roles = self.user_roles.write().await;
        user_roles
            .entry(user_id.to_string())
            .or_insert_with(HashSet::new)
            .insert(role.to_string());

        // Clear cache for this user
        self.clear_user_cache(user_id).await;

        info!("Assigned role '{}' to user '{}'", role, user_id);
        Ok(())
    }

    async fn remove_role_from_user(
        &self,
        user_id: &str,
        role: &str,
    ) -> Result<(), AuthorizationError> {
        let mut user_roles = self.user_roles.write().await;
        if let Some(roles) = user_roles.get_mut(user_id) {
            roles.remove(role);
        }

        // Clear cache for this user
        self.clear_user_cache(user_id).await;

        info!("Removed role '{}' from user '{}'", role, user_id);
        Ok(())
    }

    async fn get_user_permissions(
        &self,
        user_id: &str,
    ) -> Result<Vec<Permission>, AuthorizationError> {
        let permissions = self.get_cached_permissions(user_id).await;
        Ok(permissions.into_iter().collect())
    }

    async fn user_has_permission(
        &self,
        user_id: &str,
        permission: &Permission,
    ) -> Result<bool, AuthorizationError> {
        let permissions = self.get_cached_permissions(user_id).await;
        Ok(permissions.contains(permission))
    }
}

/// Type alias for in-memory auth service
pub type InMemoryAuthService = AuthorizationService;
