//! Basic usage example for backbone-auth
//! Demonstrates simple authentication and authorization

use backbone_auth::*;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Backbone Auth Basic Usage Example ===\n");

    // 1. Initialize Auth Service
    let auth_config = AuthServiceConfig {
        jwt_secret: "demo_secret_please_change_in_production".to_string(),
        token_expiry_hours: 24,
        ..Default::default()
    };
    let auth_service = AuthService::new(auth_config)?;

    // 2. Initialize Permission Service
    let mut permission_service = PermissionService::new();

    // 3. Basic Authentication (Development Mode)
    println!("🔐 Basic Authentication");
    let email = "user@example.com";
    let password = "SecurePass123";

    let auth_result = auth_service.authenticate(email, password).await?;
    println!("✅ User authenticated: {}", auth_result.user_id);
    println!("🔑 Token: {}", auth_result.token.as_ref().unwrap());
    println!();

    // 4. Token Validation
    println!("🔑 Token Validation");
    if let Some(token) = auth_result.token {
        let validation = auth_service.validate_token(&token).await?;
        if validation.valid {
            println!("✅ Token is valid for user: {}", validation.user_id.unwrap());
        } else {
            println!("❌ Token is invalid or expired");
        }
    }
    println!();

    // 5. Basic Permission Management
    println!("🔑 Permission Management");
    let user_id = auth_result.user_id.to_string();

    // Assign roles to user
    permission_service.assign_role(&user_id, "admin")?;
    permission_service.assign_role(&user_id, "user")?;
    println!("✅ Assigned roles: admin, user");

    // Check permissions
    let permissions_to_check = vec![
        "admin:all",
        "user:read",
        "user:write",
        "nonexistent:permission",
    ];

    for permission in permissions_to_check {
        let has_permission = permission_service.has_permission(&user_id, permission)?;
        let status = if has_permission { "✅" } else { "❌" };
        println!("  {} {}: {}", status, permission, has_permission);
    }
    println!();

    // 6. User Role Management
    println!("👤 Role Management");
    let user_roles = permission_service.get_user_roles(&user_id);
    println!("✅ User roles: {:?}", user_roles);

    // Check specific role
    let is_admin = permission_service.has_role(&user_id, "admin")?;
    let is_user = permission_service.has_role(&user_id, "user")?;
    println!("  ✅ Is Admin: {}", is_admin);
    println!("  ✅ Is User: {}", is_user);
    println!();

    // 7. Get All User Permissions
    println!("🔐 User Permissions");
    let user_permissions = permission_service.get_user_permissions(&user_id)?;
    for permission in user_permissions {
        println!("  - {} - {}", permission.name, permission.description);
    }
    println!();

    // 8. Available Roles in System
    println!("🏛️ Available System Roles");
    let all_roles = permission_service.list_roles();
    for role in all_roles {
        println!("  📋 {}: {}", role.name, role.description);
        println!("     Permissions: {} permissions", role.permissions.len());
    }
    println!();

    // 9. Demonstrate Token Generation
    println!("🔧 Manual Token Generation");
    let user_id = Uuid::new_v4();
    let manual_token = auth_service.generate_token(&user_id).await?;
    println!("✅ Generated token for {}: {}", user_id, &manual_token[..20]);

    let validation = auth_service.validate_token(&manual_token).await?;
    println!("✅ Token validation: {}", if validation.valid { "Valid" } else { "Invalid" });
    println!();

    // 10. Password Validation Examples
    println!("🔒 Password Validation Examples");

    let test_passwords = vec![
        ("short", false),
        ("password", false), // common password
        ("Password", false), // common password
        ("ValidPass123", true),
        ("SecureP@ssw0rd", true),
        ("MyPassword123!", true),
        ("NoNumbers!", false),
    ];

    for (password, expected) in test_passwords {
        let is_valid = auth_service.is_valid_password_format(password);
        let status = if is_valid == expected { "✅" } else { "❌" };
        println!("  {} '{}': {} (expected: {})", status, password, is_valid, expected);
    }

    println!("\n=== Example Complete ===");
    println!("🎉 Backbone Auth basic functionality working correctly!");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_auth_flow() -> Result<()> {
        let auth_service = AuthService::with_secret("test_secret");

        // Test authentication
        let auth_result = auth_service.authenticate("test@example.com", "TestPass123").await?;
        assert!(!auth_result.user_id.to_string().is_empty());

        // Test token validation
        if let Some(token) = auth_result.token {
            let validation = auth_service.validate_token(&token).await?;
            assert!(validation.valid);
            assert_eq!(validation.user_id.unwrap(), auth_result.user_id);
        }

        Ok(())
    }

    #[test]
    fn test_permission_system() -> Result<()> {
        let mut permission_service = PermissionService::new();
        let user_id = "test_user_123";

        // Test role assignment
        permission_service.assign_role(user_id, "admin")?;

        // Test admin permissions
        assert!(permission_service.has_permission(user_id, "admin:all")?);

        // Test user permissions (should be false initially)
        assert!(!permission_service.has_permission(user_id, "user:read")?);

        // Assign user role and test again
        permission_service.assign_role(user_id, "user")?;
        assert!(permission_service.has_permission(user_id, "user:read")?);
        assert!(permission_service.has_permission(user_id, "user:write")?);

        Ok(())
    }

    #[test]
    fn test_role_management() -> Result<()> {
        let mut permission_service = PermissionService::new();
        let user_id = "test_user_456";

        // Test role assignment
        permission_service.assign_role(user_id, "admin")?;
        assert!(permission_service.has_role(user_id, "admin")?);
        assert!(!permission_service.has_role(user_id, "nonexistent")?);

        // Test role removal
        permission_service.remove_role(user_id, "admin")?;
        assert!(!permission_service.has_role(user_id, "admin")?);

        // Test role assignment to non-existent role
        let result = permission_service.assign_role(user_id, "nonexistent_role");
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_wildcard_permissions() -> Result<()> {
        let mut permission_service = PermissionService::new();
        let user_id = "test_user_wildcard";

        // Assign admin role with wildcard permissions
        permission_service.assign_role(user_id, "admin")?;

        // Test wildcard patterns
        assert!(permission_service.has_permission(user_id, "any_resource:any_action")?); // Should be true due to admin:*
        assert!(permission_service.has_permission(user_id, "users:*")?); // Resource wildcard
        assert!(permission_service.has_permission(user_id, "*:read")?); // Action wildcard

        Ok(())
    }
}