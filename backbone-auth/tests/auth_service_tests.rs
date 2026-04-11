//! Authentication Service Tests
//!
//! These tests verify the backbone-auth generic authentication system.

use backbone_auth::{
    AuthService, AuthServiceConfig,
    // Generic types
    SimpleUser, SimplePermission, SimpleRole,
    // Backwards compatibility aliases
    User, Role, Permission,
    // Traits
    PermissionLike, RoleLike,
};
use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

#[tokio::test]
async fn test_auth_service_creation() -> Result<()> {
    let config = AuthServiceConfig {
        jwt_secret: "test_secret_key_for_testing_purposes_only".to_string(),
        token_expiry_hours: 24,
        ..Default::default()
    };

    let _auth_service = AuthService::new(config)?;

    // Auth service should be created successfully
    assert!(true, "Auth service creation test");

    Ok(())
}

#[tokio::test]
async fn test_auth_service_with_secret() -> Result<()> {
    let auth_service = AuthService::with_secret("test_secret");

    // Test token generation
    let user_id = Uuid::new_v4();
    let token = auth_service.generate_token(&user_id).await?;

    assert!(!token.is_empty(), "Token should not be empty");

    Ok(())
}

#[tokio::test]
async fn test_password_hashing() -> Result<()> {
    use backbone_auth::password::PasswordService;

    let password_service = PasswordService::new();

    let password = "secure_password_123";
    let hashed = password_service.hash_password(password)?;

    // Password should be hashed (not the same as original)
    assert_ne!(password, hashed, "Password should be hashed");
    assert!(hashed.len() > 50, "Hash should be significantly longer than original");

    // Verify password
    let is_valid = password_service.verify_password(password, &hashed)?;
    assert!(is_valid, "Password verification should succeed");

    // Test wrong password
    let is_invalid = password_service.verify_password("wrong_password", &hashed)?;
    assert!(!is_invalid, "Wrong password should return false");

    Ok(())
}

#[tokio::test]
async fn test_jwt_validation() -> Result<()> {
    use backbone_auth::jwt::JwtService;
    use std::time::{SystemTime, UNIX_EPOCH};

    let jwt_service = JwtService::new("test_secret_key");

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as usize;
    let claims = backbone_auth::jwt::Claims {
        sub: "user_id".to_string(),
        exp: now + 3600, // 1 hour from now
        iat: now,
        iss: "backbone-auth".to_string(),
    };

    let token = jwt_service.create_token(&claims)?;

    // Validate valid token
    let validated_claims = jwt_service.validate_token(&token)?;
    assert_eq!(validated_claims.sub, "user_id");

    // Test invalid token
    let invalid_token = "invalid.jwt.token";
    let validation_result = jwt_service.validate_token(invalid_token);
    assert!(validation_result.is_err(), "Invalid token should fail validation");

    Ok(())
}

#[test]
fn test_simple_user_model() {
    let now = Utc::now();
    let user = SimpleUser {
        id: Uuid::new_v4(),
        email: "test@example.com".to_string(),
        password_hash: "hashed_password".to_string(),
        is_active: true,
        is_locked: false,
        roles: vec!["user".to_string()],
        two_factor_enabled: false,
        two_factor_methods: vec![],
        account_expires_at: None,
        requires_password_change: false,
        last_login_at: None,
        failed_login_attempts: 0,
        locked_until: None,
        created_at: now,
        updated_at: now,
    };

    assert_eq!(user.email, "test@example.com");
    assert!(user.is_active);
    assert!(!user.password_hash.is_empty());
    assert_eq!(user.roles.len(), 1);
}

#[test]
fn test_simple_role_model() {
    let role = SimpleRole {
        name: "admin".to_string(),
        description: Some("Administrator role".to_string()),
        permissions: vec![
            SimplePermission {
                name: "users:read".to_string(),
                resource: "users".to_string(),
                action: "read".to_string(),
                description: Some("Read users".to_string()),
            },
            SimplePermission {
                name: "users:write".to_string(),
                resource: "users".to_string(),
                action: "write".to_string(),
                description: Some("Write users".to_string()),
            },
        ],
    };

    assert_eq!(role.name, "admin");
    assert_eq!(role.description, Some("Administrator role".to_string()));
    assert_eq!(role.permissions.len(), 2);
}

#[test]
fn test_simple_permission_model() {
    let permission = SimplePermission {
        name: "read_users".to_string(),
        resource: "users".to_string(),
        action: "read".to_string(),
        description: Some("Read users".to_string()),
    };

    assert_eq!(permission.name, "read_users");
    assert_eq!(permission.resource, "users");
    assert_eq!(permission.action, "read");
}

#[test]
fn test_permission_service_default_roles() {
    use backbone_auth::{PermissionService, PermissionChecker};

    let service = PermissionService::default();

    // Should have admin and user roles by default
    let admin = service.get_role("admin");
    assert!(admin.is_some(), "Admin role should exist");

    let user = service.get_role("user");
    assert!(user.is_some(), "User role should exist");

    // List roles
    let roles = service.list_roles();
    assert_eq!(roles.len(), 2, "Should have 2 default roles");
}

#[test]
fn test_permission_checker_trait() {
    use backbone_auth::{PermissionService, PermissionChecker};

    let mut service = PermissionService::default();

    // Assign admin role to a user
    let user_id = "test-user-123";
    service.assign_role(user_id, "admin").unwrap();

    // Check permission using trait method
    let has_perm = service.has_permission(user_id, "admin:all").unwrap();
    assert!(has_perm, "Admin should have admin:all permission");

    // Check role using trait method
    let has_role = service.has_role(user_id, "admin").unwrap();
    assert!(has_role, "User should have admin role");

    // Get user roles using trait method
    let roles = service.get_user_roles(user_id);
    assert!(roles.contains(&"admin".to_string()), "Roles should include admin");
}

#[test]
fn test_backwards_compatibility_aliases() {
    // Test that type aliases work for backwards compatibility
    let _user: User = SimpleUser {
        id: Uuid::new_v4(),
        email: "test@example.com".to_string(),
        password_hash: "hash".to_string(),
        is_active: true,
        is_locked: false,
        roles: vec![],
        two_factor_enabled: false,
        two_factor_methods: vec![],
        account_expires_at: None,
        requires_password_change: false,
        last_login_at: None,
        failed_login_attempts: 0,
        locked_until: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let _role: Role = SimpleRole {
        name: "test".to_string(),
        description: None,
        permissions: vec![],
    };

    let _permission: Permission = SimplePermission {
        name: "test".to_string(),
        resource: "test".to_string(),
        action: "test".to_string(),
        description: None,
    };

    assert!(true, "Type aliases work correctly");
}

#[test]
fn test_permission_like_trait() {
    let permission = SimplePermission {
        name: "users:read".to_string(),
        resource: "users".to_string(),
        action: "read".to_string(),
        description: Some("Read users".to_string()),
    };

    // Test trait methods
    assert_eq!(permission.name(), "users:read");
    assert_eq!(permission.resource(), "users");
    assert_eq!(permission.action(), "read");
    assert_eq!(permission.description(), Some("Read users"));
}

#[test]
fn test_role_like_trait() {
    let role = SimpleRole {
        name: "admin".to_string(),
        description: Some("Administrator".to_string()),
        permissions: vec![
            SimplePermission {
                name: "admin:all".to_string(),
                resource: "*".to_string(),
                action: "*".to_string(),
                description: None,
            },
        ],
    };

    // Test trait methods
    assert_eq!(role.name(), "admin");
    assert_eq!(role.description(), Some("Administrator"));
    assert_eq!(role.permissions().len(), 1);
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_complete_auth_flow() -> Result<()> {
        let auth_service = AuthService::with_secret("integration_test_secret");

        // Test token generation
        let user_id = Uuid::new_v4();
        let token = auth_service.generate_token(&user_id).await?;
        assert!(!token.is_empty());

        // Test token validation
        let validation = auth_service.validate_token(&token).await?;
        assert!(validation.valid);
        assert_eq!(validation.user_id, Some(user_id));

        Ok(())
    }

    #[test]
    fn test_permission_service_flow() -> Result<()> {
        use backbone_auth::{PermissionService, PermissionChecker};

        let mut service = PermissionService::default();
        let user_id = "test-user";

        // Assign role
        service.assign_role(user_id, "admin")?;

        // Check permission
        assert!(service.has_permission(user_id, "admin:all")?);

        // Remove role
        service.remove_role(user_id, "admin")?;

        // Permission should be gone
        assert!(!service.has_permission(user_id, "admin:all")?);

        Ok(())
    }
}
