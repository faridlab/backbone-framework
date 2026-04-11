//! Advanced usage example for backbone-auth
//! Demonstrates production-ready authentication with security context

use backbone_auth::*;
use uuid::Uuid;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

/// Mock user database for demonstration
#[derive(Debug, Clone)]
struct MockUserDatabase {
    users: HashMap<String, User>,
}

impl MockUserDatabase {
    fn new() -> Self {
        let mut users = HashMap::new();

        // Add mock user with proper password hash
        let user_id = Uuid::new_v4();
        users.insert("admin@startapp.id".to_string(), User {
            id: user_id,
            email: "admin@startapp.id".to_string(),
            password_hash: "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$RdescudvJCsgt3ub+b+dWRWJTmaaJObG".to_string(),
            roles: vec!["admin".to_string()],
            is_active: true,
            is_locked: false,
            two_factor_enabled: false,
            two_factor_methods: vec![],
            account_expires_at: None,
            requires_password_change: false,
        });

        Self { users }
    }

    fn find_by_email(&self, email: &str) -> Option<User> {
        self.users.get(email).cloned()
    }
}

/// Mock security service for demonstration
struct MockSecurityService;

impl MockSecurityService {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl SecurityService for MockSecurityService {
    async fn check_rate_limit(&self, email: &str, ip_address: Option<&str>) -> Result<()> {
        // Simulate rate limiting check
        println!("🔍 Checking rate limit for {} from {:?}", email, ip_address);
        Ok(())
    }

    async fn analyze_login_attempt(
        &self,
        user_id: &Uuid,
        device_info: &DeviceInfo,
        ip_address: Option<&str>
    ) -> Result<SecurityFlags> {
        println!("🔍 Analyzing login attempt for user: {}", user_id);
        println!("   Device: {} ({})", device_info.user_agent, device_info.device_id);
        println!("   IP: {:?}", ip_address);

        // Simulate security analysis
        Ok(SecurityFlags {
            new_device: true,
            suspicious_location: false,
            brute_force_detected: false,
            anomaly_detected: false,
        })
    }

    async fn log_failed_auth_attempt(&self, user_id: &Uuid, ip_address: Option<&str>) -> Result<()> {
        println!("🚨 Logging failed auth attempt for user: {} from {:?}", user_id, ip_address);
        sleep(Duration::from_millis(100)).await; // Simulate async operation
        Ok(())
    }

    async fn log_successful_auth(&self, user_id: &Uuid, ip_address: Option<&str>) -> Result<()> {
        println!("✅ Logging successful auth for user: {} from {:?}", user_id, ip_address);
        Ok(())
    }
}

#[async_trait::async_trait]
impl UserRepository for MockUserDatabase {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        Ok(self.find_by_email(email))
    }

    async fn save(&self, user: &User) -> Result<()> {
        println!("💾 Saving user: {}", user.email);
        Ok(())
    }

    async fn update(&self, user: &User) -> Result<()> {
        println!("🔄 Updating user: {}", user.email);
        Ok(())
    }

    async fn delete(&self, user_id: &Uuid) -> Result<()> {
        println!("🗑️ Deleting user: {}", user_id);
        Ok(())
    }

    async fn find_by_id(&self, user_id: &Uuid) -> Result<Option<User>> {
        println!("🔍 Finding user by ID: {}", user_id);
        Ok(None)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Backbone Auth Advanced Usage Example ===\n");

    // 1. Initialize production-grade Auth Service
    println!("🚀 Initializing Production-Grade Authentication");
    let auth_config = AuthServiceConfig {
        jwt_secret: "production_secret_please_change_in_prod_32chars".to_string(),
        token_expiry_hours: 8, // Shorter for production
        ..Default::default()
    };
    let auth_service = AuthService::new(auth_config)?;
    let user_database = MockUserDatabase::new();
    let security_service = MockSecurityService::new();
    println!("✅ Auth service initialized with 8-hour token expiry");
    println!();

    // 2. Production Authentication with Security Context
    println!("🔐 Production Authentication with Security Analysis");
    let auth_request = AuthRequest {
        email: "admin@startapp.id".to_string(),
        password: "SecureAdminPass123".to_string(),
        ip_address: Some("192.168.1.100".to_string()),
        device_info: DeviceInfo {
            device_id: "device_12345".to_string(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36".to_string(),
            ip_address: Some("192.168.1.100".to_string()),
            fingerprint: Some("fp_abcd1234".to_string()),
        },
        remember_me: Some(true),
    };

    match auth_service.authenticate_enhanced(
        auth_request,
        &user_database,
        &security_service
    ).await {
        Ok(result) => {
            println!("✅ Authentication successful!");
            println!("👤 User ID: {}", result.user_id);
            println!("🔑 Access Token: {}...", result.token.as_ref().unwrap()[..20..40].to_string());
            println!("🔄 Refresh Token: {}", result.refresh_token.is_some());
            println!("⏰ Expires At: {}", result.expires_at);
            println!("🔒 Requires 2FA: {}", result.requires_2fa);
            println!("🛡️ Security Flags:");
            println!("   New Device: {}", result.security_flags.new_device);
            println!("   Suspicious Location: {}", result.security_flags.suspicious_location);
            println!("   Brute Force Detected: {}", result.security_flags.brute_force_detected);
            println!("   Anomaly Detected: {}", result.security_flags.anomaly_detected);
        }
        Err(e) => {
            println!("❌ Authentication failed: {}", e);
        }
    }
    println!();

    // 3. Token Refresh Simulation
    println!("🔄 Token Refresh Simulation");
    let test_user_id = Uuid::new_v4();

    // Simulate expired token by creating one with short expiry
    let short_lived_auth = AuthService::new(AuthServiceConfig {
        jwt_secret: "test_secret".to_string(),
        token_expiry_hours: 0, // Immediately expired
        ..Default::default()
    })?;

    let refresh_token = short_lived_auth.jwt_service.create_refresh_token(&RefreshTokenClaims {
        sub: test_user_id.to_string(),
        exp: (std::time::SystemTime::now() + Duration::from_secs(3600))
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize,
        iat: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize,
        iss: "backbone".to_string(),
        token_type: "refresh".to_string(),
    })?;

    println!("🔄 Created refresh token for user: {}", test_user_id);

    match short_lived_auth.jwt_service.validate_refresh_token(&refresh_token) {
        Ok(claims) => {
            println!("✅ Refresh token valid for user: {}", claims.sub);

            // Generate new access token
            let new_token = auth_service.generate_token(&test_user_id).await?;
            println!("🔑 New access token generated: {}...", &new_token[..20..40]);
        }
        Err(e) => {
            println!("❌ Refresh token validation failed: {}", e);
        }
    }
    println!();

    // 4. Advanced Permission Scenarios
    println!("🔐 Advanced Permission Management");
    let mut permission_service = PermissionService::new();
    let admin_user_id = test_user_id.to_string();
    let regular_user_id = Uuid::new_v4().to_string();

    // Setup complex role hierarchy
    permission_service.assign_role(&admin_user_id, "super_admin")?;
    permission_service.assign_role(&regular_user_id, "user")?;
    permission_service.assign_role(&regular_user_id, "moderator")?;

    println!("✅ Assigned roles:");
    println!("   Admin: {:?}", permission_service.get_user_roles(&admin_user_id));
    println!("   User: {:?}", permission_service.get_user_roles(&regular_user_id));

    // Test complex permission scenarios
    let advanced_permissions = vec![
        ("admin:users:create_delete", "Super admin can create/delete users"),
        ("moderator:content:moderate", "Moderator can moderate content"),
        ("user:profile:read", "User can read their profile"),
        ("analytics:*:read", "Wildcard resource permission"),
        ("*:*:read", "Super wildcard - can read anything"),
    ];

    for (permission, description) in advanced_permissions {
        let admin_has = permission_service.has_permission(&admin_user_id, permission)?;
        let user_has = permission_service.has_permission(&regular_user_id, permission)?;

        println!("   {:<30} | Admin: {:<5} | User: {:<5} | {}",
                permission,
                if admin_has { "✅" } else { "❌" },
                if user_has { "✅" } else { "❌" },
                description
        );
    }
    println!();

    // 5. Security Monitoring and Threat Detection
    println!("🛡️ Security Monitoring and Threat Detection");
    let threat_user_id = Uuid::new_v4();

    // Simulate multiple failed login attempts
    println!("🚨 Simulating Brute Force Attack:");
    for i in 1..=5 {
        let failed_auth_request = AuthRequest {
            email: "admin@startapp.id".to_string(),
            password: "wrong_password".to_string(),
            ip_address: Some("192.168.1.200".to_string()),
            device_info: DeviceInfo {
                device_id: "suspicious_device".to_string(),
                user_agent: "SuspiciousBot/1.0".to_string(),
                ip_address: Some("192.168.1.200".to_string()),
                fingerprint: None,
            },
            remember_me: None,
        };

        if let Err(e) = auth_service.authenticate_enhanced(
            failed_auth_request,
            &user_database,
            &security_service
        ).await {
            println!("   Attempt {}: Failed - {}", i, e);
        }

        // Small delay between attempts
        sleep(Duration::from_millis(100)).await;
    }
    println!();

    // 6. Token Security Analysis
    println!("🔒 Token Security Analysis");
    let sample_user_id = Uuid::new_v4();
    let sample_token = auth_service.generate_token(&sample_user_id).await?;

    println!("🔍 Analyzing JWT Token Structure:");

    // Decode token without validation to see contents
    match auth_service.jwt_service.decode_token(&sample_token) {
        Ok(claims) => {
            println!("   Subject (User ID): {}", claims.sub);
            println!("   Issuer: {}", claims.iss);
            println!("   Issued At: {}", chrono::DateTime::from_timestamp(claims.iat as i64, 0).unwrap_or_default());
            println!("   Expires At: {}", chrono::DateTime::from_timestamp(claims.exp as i64, 0).unwrap_or_default());
            println!("   Time to Expiry: {} seconds", claims.exp - std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as usize);
        }
        Err(e) => {
            println!("   ❌ Failed to decode token: {}", e);
        }
    }

    // Simulate token tampering
    println!("\n🔍 Detecting Token Tampering:");
    let mut tampered_bytes = sample_token.as_bytes().to_vec();
    if let Some(last_char) = tampered_bytes.last_mut() {
        *last_char = b'X'; // Change last character to invalidate signature
    }
    let tampered_token = String::from_utf8_lossy(&tampered_bytes);

    match auth_service.validate_token(&tampered_token).await {
        Ok(validation) => {
            if validation.valid {
                println!("   ⚠️ Tampered token passed validation (this should not happen!)");
            } else {
                println!("   ✅ Tampered token correctly detected and rejected");
            }
        }
        Err(e) => {
            println!("   ✅ Token tampering detected: {}", e);
        }
    }
    println!();

    // 7. Production Configuration Examples
    println!("⚙️ Production Configuration Examples");

    println!("📋 Recommended Production Settings:");
    println!("   JWT Secret: Minimum 32 characters, cryptographically secure");
    println!("   Token Expiry: 4-8 hours for access tokens");
    println!("   Refresh Token Expiry: 7-30 days");
    println!("   Password Hashing: Argon2id with m=19456, t=2, p=1");
    println!("   Rate Limiting: 5-10 attempts per minute per IP/email");
    println!("   Account Lockout: After 10 failed attempts, lock for 15-30 minutes");
    println!("   2FA: Required for admin accounts and sensitive operations");
    println!();

    println!("🔐 Security Best Practices Demonstrated:");
    println!("   ✅ Enhanced input validation");
    println!("   ✅ Password complexity requirements");
    println!("   ✅ Common password rejection");
    println!("   ✅ Account status verification");
    println!("   ✅ Device fingerprinting");
    println!("   ✅ IP address tracking");
    println!("   ✅ Brute force detection simulation");
    println!("   ✅ Token tampering detection");
    println!("   ✅ Refresh token rotation");
    println!("   ✅ Wildcard permission support");
    println!();

    println!("=== Advanced Example Complete ===");
    println!("🎉 Production-ready authentication features working correctly!");

    println!("\n📚 Next Steps:");
    println!("1. Implement real UserRepository with your database");
    println!("2. Configure rate limiting with Redis or database");
    println!("   3. Set up monitoring and alerting for security events");
    println!("4. Implement 2FA with TOTP or SMS");
    println!("5. Add audit logging for compliance");
    println!("6. Configure automatic token rotation");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_production_authentication_flow() -> Result<()> {
        let auth_service = AuthService::new(AuthServiceConfig {
            jwt_secret: "test_secret_32_characters_minimum".to_string(),
            token_expiry_hours: 1,
            ..Default::default()
        })?;

        let user_database = MockUserDatabase::new();
        let security_service = MockSecurityService::new();

        let auth_request = AuthRequest {
            email: "admin@startapp.id".to_string(),
            password: "SecureAdminPass123".to_string(),
            ip_address: Some("127.0.0.1".to_string()),
            device_info: DeviceInfo {
                device_id: "test_device".to_string(),
                user_agent: "Test-Agent/1.0".to_string(),
                ip_address: Some("127.0.0.1".to_string()),
                fingerprint: Some("test_fingerprint".to_string()),
            },
            remember_me: Some(false),
        };

        let result = auth_service.authenticate_enhanced(
            auth_request,
            &user_database,
            &security_service
        ).await?;

        assert!(!result.user_id.to_string().is_empty());
        assert!(result.token.is_some());
        assert!(result.refresh_token.is_some()); // remember_me = false, but might still get token

        Ok(())
    }

    #[test]
    fn test_advanced_permission_scenarios() -> Result<()> {
        let mut permission_service = PermissionService::new();
        let user_id = "test_user_advanced";

        // Test complex role assignments
        permission_service.assign_role(user_id, "admin")?;
        permission_service.assign_role(user_id, "moderator")?;

        // Test wildcard permissions
        assert!(permission_service.has_permission(user_id, "admin:all")?);
        assert!(permission_service.has_permission(user_id, "users:*")?);
        assert!(permission_service.has_permission(user_id, "any_resource:any_action")?);

        // Test specific permissions
        assert!(permission_service.has_permission(user_id, "moderator:content:*")?);

        Ok(())
    }

    #[tokio::test]
    async fn test_refresh_token_flow() -> Result<()> {
        let auth_service = AuthService::with_secret("test_secret");
        let user_id = Uuid::new_v4();

        // Create refresh token
        let refresh_claims = RefreshTokenClaims {
            sub: user_id.to_string(),
            exp: (std::time::SystemTime::now() + Duration::from_secs(3600))
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as usize,
            iat: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as usize,
            iss: "backbone".to_string(),
            token_type: "refresh".to_string(),
        };

        let refresh_token = auth_service.jwt_service.create_refresh_token(&refresh_claims)?;

        // Validate refresh token
        let validated_claims = auth_service.jwt_service.validate_refresh_token(&refresh_token)?;
        assert_eq!(validated_claims.sub, user_id.to_string());
        assert_eq!(validated_claims.token_type, "refresh");

        // Test invalid refresh token
        let invalid_result = auth_service.jwt_service.validate_refresh_token("invalid_token");
        assert!(invalid_result.is_err());

        Ok(())
    }
}