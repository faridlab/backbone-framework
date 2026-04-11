# Backbone Auth

<div align="center">

[![Crates.io](https://img.shields.io/crates/v/backbone-auth)](https://crates.io/crates/backbone-auth)
[![Documentation](https://docs.rs/backbone-auth/badge.svg)](https://docs.rs/backbone-auth)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Enterprise-Grade Authentication & Authorization System for the Backbone Framework**

Production-ready security with JWT, password hashing, Role-Based Access Control (RBAC), and comprehensive threat protection.

</div>

## 🚀 Overview

Backbone Auth is a complete authentication and authorization system designed for modern Rust applications. It provides enterprise-grade security features including multi-factor authentication, device fingerprinting, rate limiting, and comprehensive audit logging while maintaining developer-friendly APIs.

### 🎯 Key Features

- **🔐 Enterprise Security**: Multi-layered authentication with JWT, Argon2 password hashing, and 2FA support
- **🛡️ Threat Protection**: Rate limiting, device fingerprinting, IP tracking, and brute force prevention
- **🔑 Flexible Authorization**: Advanced RBAC with wildcards, hierarchical permissions, and dynamic role management
- **📊 Security Monitoring**: Comprehensive audit trails, security alerts, and risk scoring
- **⚡ High Performance**: Async/await throughout for scalable, high-concurrency applications
- **🔧 Production Ready**: Database integration patterns, configuration management, and deployment guides

### 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    BACKBONE AUTH SYSTEM                    │
├─────────────────────────────────────────────────────────────┤
│  🌐 Web Layer                                             │
│  ├─ HTTP/gRPC Endpoints                                     │
│  ├─ Middleware (CORS, Security Headers)                    │
│  └─ Request/Response Validation                            │
├─────────────────────────────────────────────────────────────┤
│  🔐 Authentication Layer                                    │
│  ├─ AuthService (JWT, Password Hashing)                    │
│  ├─ Enhanced Authentication (Security Context)              │
│  ├─ Token Management (Access + Refresh Tokens)             │
│  └─ Session Management                                     │
├─────────────────────────────────────────────────────────────┤
│  🛡️ Security Layer                                         │
│  ├─ Rate Limiting & Brute Force Protection                 │
│  ├─ Device Fingerprinting & IP Tracking                    │
│  ├─ Threat Detection & Risk Scoring                         │
│  ├─ Security Monitoring & Alerts                           │
│  └─ Audit Logging & Compliance                             │
├─────────────────────────────────────────────────────────────┤
│  🔑 Authorization Layer (RBAC)                             │
│  ├─ Role Management (Dynamic, Hierarchical)                │
│  ├─ Permission System (Granular, Wildcards)                │
│  ├─ User-Role Assignment                                   │
│  └─ Access Control Evaluation                              │
├─────────────────────────────────────────────────────────────┤
│  💾 Data Layer                                             │
│  ├─ UserRepository (PostgreSQL, MongoDB)                   │
│  ├─ SecurityService (Redis, Rate Limiting)                │
│  ├─ Configuration Management                               │
│  └─ Caching & Session Storage                              │
└─────────────────────────────────────────────────────────────┘
```

## 🚀 Quick Start

### Basic Usage

```rust
use backbone_auth::*;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize Auth Service
    let auth_config = AuthServiceConfig {
        jwt_secret: std::env::var("JWT_SECRET")?,
        token_expiry_hours: 24,
    };
    let auth_service = AuthService::new(auth_config);

    // 2. Basic Authentication
    let auth_result = auth_service.authenticate("user@example.com", "secure_password").await?;
    println!("✅ User authenticated: {}", auth_result.user_id);

    // 3. Token Validation
    if let Some(token) = auth_result.token {
        let validation = auth_service.validate_token(&token).await?;
        if validation.valid {
            println!("✅ Token valid for user: {}", validation.user_id.unwrap());
        }
    }

    // 4. Permission Management
    let mut permission_service = PermissionService::new();
    permission_service.assign_role(&auth_result.user_id.to_string(), "admin")?;

    // 5. Authorization Check
    let has_access = permission_service.has_permission(
        &auth_result.user_id.to_string(),
        "admin:users"
    )?;
    println!("🔑 Admin access: {}", has_access);

    Ok(())
}
```

### Production-Ready Authentication

```rust
use backbone_auth::*;

async fn production_login(
    auth_service: &AuthService,
    user_repository: &dyn UserRepository,
    security_service: &dyn SecurityService,
) -> Result<AuthResultEnhanced> {
    // Enhanced authentication request with security context
    let auth_request = AuthRequest {
        email: "user@company.com".to_string(),
        password: "SecureP@ssw0rd123!".to_string(),
        remember_me: Some(true),
        device_info: Some(DeviceInfo {
            device_id: Some("device_12345".to_string()),
            device_type: "web".to_string(),
            platform: Some("macos".to_string()),
            user_agent: Some("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)".to_string()),
            fingerprint: None,
        }),
        ip_address: Some("192.168.1.100".to_string()),
        user_agent: Some("Mozilla/5.0...".to_string()),
    };

    // Complete production authentication with all security layers
    let result = auth_service.authenticate_enhanced(
        auth_request,
        user_repository,
        security_service
    ).await?;

    println!("🔐 Authenticated user: {}", result.user_id);
    println!("🔑 Access token: {}", &result.token[..20]);
    println!("⚠️ 2FA Required: {}", result.requires_2fa);
    println!("🛡️ Security flags: {:?}", result.security_flags);

    Ok(result)
}
```

## 🔐 Authentication

### Service Configuration

```rust
// Development configuration
let config = AuthServiceConfig {
    jwt_secret: "dev_secret_change_in_production".to_string(),
    token_expiry_hours: 24,
};

// Production configuration (recommended)
let config = AuthServiceConfig {
    jwt_secret: std::env::var("JWT_SECRET")?, // From environment variable
    token_expiry_hours: 1, // Shorter for higher security
};

let auth_service = AuthService::new(config);
```

### Enhanced Authentication Flow

The enhanced authentication provides comprehensive security:

```rust
async fn authenticate_with_security(
    auth_service: &AuthService,
    request: AuthRequest,
) -> Result<AuthResultEnhanced> {
    // 1. Input Validation (email format, password complexity)
    // 2. Rate Limiting Check (anti-brute force)
    // 3. Database User Lookup
    // 4. Account Status Validation (active, locked, expired)
    // 5. Password Hash Verification (Argon2)
    // 6. Security Analysis (new device, location, patterns)
    // 7. 2FA Requirement Check
    // 8. Enhanced JWT Token Generation
    // 9. Security Audit Logging

    auth_service.authenticate_enhanced(
        request,
        &user_repository, // Your database implementation
        &security_service // Your security service (Redis, etc.)
    ).await
}
```

### Token Management

```rust
// Access Token Generation
let access_token = auth_service.generate_token(&user_id).await?;

// Enhanced Token Generation (with refresh token)
let auth_result = auth_service.authenticate_enhanced(request, &user_repo, &security_service).await?;
println!("Access Token: {}", auth_result.token);
println!("Refresh Token: {:?}", auth_result.refresh_token);

// Token Validation
let validation = auth_service.validate_token(&token).await?;
if validation.valid {
    println!("Token valid for user: {}", validation.user_id.unwrap());
} else {
    println!("Token invalid or expired");
}
```

### Password Security

```rust
use argon2::{Argon2, PasswordHash, PasswordHasher, password_hash::{SaltString, rand_core::OsRng}};

// Secure password hashing (done during user registration)
fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(password_hash.to_string())
}

// Password verification (handled automatically by AuthService)
let is_valid = auth_service.verify_password(
    "user_password",
    "$argon2id$v=19$m=65536,t=3,p=4$..."
)?;
```

## 🔑 Authorization (RBAC)

### Permission System Architecture

The permission system supports multiple access control patterns:

```rust
// Permission formats supported
"resource:action"        // Exact match: "users:read"
"resource:*"            // Resource wildcard: "users:*" (all actions on users)
"*:action"              // Action wildcard: "*:read" (read on all resources)
"*:*"                   // Double wildcard: "admin:*" (full access)
```

### Role Management

```rust
let mut permission_service = PermissionService::new();

// Create custom role with specific permissions
let developer_role = Role {
    name: "developer".to_string(),
    description: "Full development access".to_string(),
    permissions: vec![
        Permission {
            name: "code:read".to_string(),
            description: "Read source code".to_string(),
            resource: "code".to_string(),
            action: "read".to_string(),
        },
        Permission {
            name: "code:write".to_string(),
            description: "Write and commit code".to_string(),
            resource: "code".to_string(),
            action: "write".to_string(),
        },
        Permission {
            name: "deploy:*".to_string(),
            description: "Deploy applications".to_string(),
            resource: "deploy".to_string(),
            action: "*".to_string(),
        },
    ],
};

// Add role to system
permission_service.add_role(developer_role)?;

// Assign multiple roles to user
let user_id = "user_12345";
permission_service.assign_role(user_id, "developer")?;
permission_service.assign_role(user_id, "user")?;
```

### Permission Checking

```rust
// Check specific permissions
let can_read_code = permission_service.has_permission(user_id, "code:read")?;
let can_deploy = permission_service.has_permission(user_id, "deploy:production")?;
let admin_access = permission_service.has_permission(user_id, "admin:*")?;

// Get all user permissions
let user_permissions = permission_service.get_user_permissions(user_id)?;
for permission in user_permissions {
    println!("🔑 {} - {}", permission.name, permission.description);
}

// Role-based checks
let is_admin = permission_service.has_role(user_id, "admin")?;
let is_developer = permission_service.has_role(user_id, "developer")?;

// Get user roles
let roles = permission_service.get_user_roles(user_id);
println!("👤 User roles: {:?}", roles);
```

### Hierarchical Permissions

```rust
// Create hierarchical role structure
let team_lead_role = Role {
    name: "team_lead".to_string(),
    description: "Team lead with extended permissions".to_string(),
    permissions: vec![
        // Team management
        Permission {
            name: "team:read".to_string(),
            resource: "team".to_string(),
            action: "read".to_string(),
            description: "View team members".to_string(),
        },
        Permission {
            name: "team:write".to_string(),
            resource: "team".to_string(),
            action: "write".to_string(),
            description: "Manage team members".to_string(),
        },
        // Inherited user permissions
        Permission {
            name: "user:*".to_string(),
            resource: "user".to_string(),
            action: "*".to_string(),
            description: "Full user management".to_string(),
        },
        // Project access
        Permission {
            name: "project:*".to_string(),
            resource: "project".to_string(),
            action: "read".to_string(),
            description: "Read all projects".to_string(),
        },
    ],
};
```

## 🛡️ Security Features

### Rate Limiting & Brute Force Protection

```rust
use async_trait::async_trait;
use redis::Client;

struct RedisSecurityService {
    redis: Client,
}

#[async_trait]
impl SecurityService for RedisSecurityService {
    async fn check_rate_limit(&self, email: &str, ip_address: Option<&str>) -> Result<()> {
        // Implement rate limiting with Redis
        let key = format!("auth_rate_limit:{}", email);
        let attempts: i32 = self.redis.get(&key).await.unwrap_or(0);

        if attempts >= 5 {
            return Err(anyhow::anyhow!("Too many authentication attempts"));
        }

        self.redis.incr(&key).await?;
        self.redis.expire(&key, 900).await?; // 15 minutes

        Ok(())
    }

    async fn log_failed_auth_attempt(&self, user_id: &Uuid, ip_address: Option<&str>) -> Result<()> {
        // Log security events for monitoring
        tracing::warn!(
            user_id = %user_id,
            ip_address = %ip_address.unwrap_or("unknown"),
            "Failed authentication attempt"
        );
        Ok(())
    }

    async fn analyze_login_attempt(
        &self,
        user_id: &Uuid,
        device_info: &Option<DeviceInfo>,
        ip_address: Option<&str>
    ) -> Result<SecurityFlags> {
        // Analyze for security threats
        let mut flags = SecurityFlags::default();

        // Check for new device
        if let Some(device) = device_info {
            flags.new_device = self.is_new_device(user_id, device).await?;
        }

        // Check for new location
        if let Some(ip) = ip_address {
            flags.new_location = self.is_new_location(user_id, ip).await?;
        }

        // Calculate risk score
        flags.risk_score = self.calculate_risk_score(&flags).await?;

        Ok(flags)
    }
}
```

### Device Fingerprinting

```rust
fn generate_device_fingerprint(user_agent: &str, ip: &str) -> String {
    use sha2::{Sha256, Digest};

    let mut hasher = Sha256::new();
    hasher.update(user_agent.as_bytes());
    hasher.update(ip.as_bytes());

    format!("{:x}", hasher.finalize())
}

// In authentication request
let device_info = DeviceInfo {
    device_id: Some(generate_device_fingerprint(&user_agent, &ip_address)),
    device_type: detect_device_type(&user_agent),
    platform: detect_platform(&user_agent),
    user_agent: Some(user_agent.to_string()),
    fingerprint: Some(generate_browser_fingerprint(&user_agent)),
};
```

### Security Monitoring

```rust
// Comprehensive security event logging
tracing::info!(
    user_id = %user_id,
    ip_address = %request.ip_address,
    device_type = %request.device_info.as_ref().map(|d| &d.device_type).unwrap_or("unknown"),
    risk_score = %result.security_flags.risk_score,
    "User authentication completed"
);

tracing::warn!(
    user_id = %user_id,
    ip_address = %request.ip_address,
    reason = "new_device_detected",
    risk_score = %result.security_flags.risk_score,
    "Security alert: New device login"
);

tracing::error!(
    user_id = %user_id,
    ip_address = %request.ip_address,
    reason = "multiple_failed_attempts",
    "Security threat detected"
);
```

## 🏗️ Database Integration

### PostgreSQL Implementation

```rust
use sqlx::PgPool;
use async_trait::async_trait;

struct PostgresUserRepository {
    pool: PgPool,
}

#[async_trait]
impl UserRepository for PostgresUserRepository {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        let user = sqlx::query_as!(
            User,
            r#"
            SELECT
                id, email, password_hash, is_active, is_locked, roles,
                two_factor_enabled, two_factor_methods, account_expires_at,
                requires_password_change, last_login_at, failed_login_attempts,
                locked_until, created_at, updated_at
            FROM users
            WHERE email = $1 AND deleted_at IS NULL
            "#,
            email
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    async fn create(&self, user: &User) -> Result<User> {
        let created_user = sqlx::query_as!(
            User,
            r#"
            INSERT INTO users (
                id, email, password_hash, is_active, is_locked, roles,
                two_factor_enabled, two_factor_methods, account_expires_at,
                requires_password_change, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW(), NOW())
            RETURNING *
            "#,
            user.id,
            user.email,
            user.password_hash,
            user.is_active,
            user.is_locked,
            &user.roles,
            user.two_factor_enabled,
            &user.two_factor_methods,
            user.account_expires_at,
            user.requires_password_change
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(created_user)
    }

    async fn update_last_login(&self, user_id: &Uuid) -> Result<()> {
        sqlx::query!(
            "UPDATE users SET last_login_at = NOW() WHERE id = $1",
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn increment_failed_attempts(&self, user_id: &Uuid) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE users
            SET failed_login_attempts = failed_login_attempts + 1,
                updated_at = NOW()
            WHERE id = $1
            "#,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ... implement other methods
}
```

### MongoDB Implementation

```rust
use mongodb::{Client, Collection};
use mongodb::bson::{doc, Document};

struct MongoUserRepository {
    collection: Collection<User>,
}

#[async_trait]
impl UserRepository for MongoUserRepository {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        let user = self.collection
            .find_one(
                doc! { "email": email, "deleted_at": null },
                None
            )
            .await?;

        Ok(user)
    }

    async fn create(&self, user: &User) -> Result<User> {
        let user_clone = user.clone();
        self.collection.insert_one(user_clone, None).await?;
        Ok(user.clone())
    }

    // ... implement other methods
}
```

### Database Schema (PostgreSQL)

```sql
-- Users table with comprehensive security fields
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    is_active BOOLEAN DEFAULT true,
    is_locked BOOLEAN DEFAULT false,
    roles JSONB DEFAULT '[]'::jsonb,
    two_factor_enabled BOOLEAN DEFAULT false,
    two_factor_methods JSONB DEFAULT '[]'::jsonb,
    account_expires_at TIMESTAMP WITH TIME ZONE,
    requires_password_change BOOLEAN DEFAULT false,
    last_login_at TIMESTAMP WITH TIME ZONE,
    failed_login_attempts INTEGER DEFAULT 0,
    locked_until TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    deleted_at TIMESTAMP WITH TIME ZONE
);

-- Indexes for performance
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_active ON users(is_active) WHERE deleted_at IS NULL;
CREATE INDEX idx_users_last_login ON users(last_login_at) DESC;

-- Security audit log
CREATE TABLE auth_audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID,
    email VARCHAR(255),
    action VARCHAR(50) NOT NULL,
    ip_address INET,
    user_agent TEXT,
    device_fingerprint VARCHAR(255),
    success BOOLEAN NOT NULL,
    failure_reason TEXT,
    security_flags JSONB,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_auth_audit_user ON auth_audit_log(user_id);
CREATE INDEX idx_auth_audit_created ON auth_audit_log(created_at) DESC;
```

## ⚙️ Configuration

### Environment Configuration

```bash
# JWT Configuration
JWT_SECRET=your_super_secure_random_secret_at_least_32_characters
JWT_EXPIRY_HOURS=1

# Database Configuration
DATABASE_URL=postgresql://user:password@localhost:5432/backbone_auth
REDIS_URL=redis://localhost:6379

# Security Configuration
MAX_LOGIN_ATTEMPTS=5
LOGIN_ATTEMPT_WINDOW=900  # 15 minutes in seconds
SESSION_TIMEOUT=3600      # 1 hour in seconds
ENABLE_2FA=true
ACCOUNT_LOCKOUT_DURATION=3600  # 1 hour

# Email Configuration (for security alerts)
SMTP_SERVER=smtp.gmail.com:587
SMTP_USER=noreply@yourapp.com
SMTP_PASS=your_smtp_password
```

### Application Configuration (YAML)

```yaml
# config/auth.yml
auth:
  jwt:
    secret: "${JWT_SECRET}"
    expiry_hours: 1
    refresh_token_days: 30

  password:
    min_length: 8
    max_length: 128
    require_uppercase: true
    require_lowercase: true
    require_numbers: true
    require_special_chars: true

  security:
    max_login_attempts: 5
    attempt_window_seconds: 900
    account_lockout_duration: 3600
    enable_device_fingerprinting: true
    enable_ip_tracking: true
    risk_score_threshold: 0.7

  two_factor:
    enabled: true
    methods: ["totp", "sms", "email"]
    issuer: "YourApp"

  sessions:
    timeout_seconds: 3600
    refresh_threshold_seconds: 300

database:
  url: "${DATABASE_URL}"
  max_connections: 20
  connection_timeout: 30

redis:
  url: "${REDIS_URL}"
  pool_size: 10
```

### Configuration Loading

```rust
use serde::Deserialize;
use config::{Config, ConfigError, Environment};

#[derive(Debug, Deserialize)]
struct AuthConfig {
    jwt: JwtConfig,
    password: PasswordConfig,
    security: SecurityConfig,
    two_factor: TwoFactorConfig,
    database: DatabaseConfig,
    redis: RedisConfig,
}

impl AuthConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(config::File::with_name("config/auth"))
            .add_source(Environment::with_prefix("auth"))
            .build()?;

        config.try_deserialize()
    }
}

let config = AuthConfig::from_env()?;
let auth_service = AuthService::new(AuthServiceConfig {
    jwt_secret: config.jwt.secret,
    token_expiry_hours: config.jwt.expiry_hours,
});
```

## 🔧 Advanced Usage

### Two-Factor Authentication

```rust
use totp_lite::{totp, Stepper};

struct TOTPSecurityService {
    issuer: String,
}

impl TOTPSecurityService {
    fn generate_secret(&self) -> String {
        // Generate secure random secret
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..32)
            .map(|_| rng.gen_range(0..10))
            .map(|d| char::from_digit(d, 10).unwrap())
            .collect()
    }

    fn generate_qr_code(&self, secret: &str, user_email: &str) -> Result<String> {
        let uri = format!(
            "otpauth://totp/{}:{}?secret={}&issuer={}",
            urlencoding::encode(&self.issuer),
            urlencoding::encode(user_email),
            secret,
            urlencoding::encode(&self.issuer)
        );

        // Generate QR code image
        // Use qrcode library or external service

        Ok(uri)
    }

    fn verify_totp(&self, secret: &str, code: &str) -> Result<bool> {
        let stepper = Stepper::new(30, 6);
        let decoded_secret = base32::decode(base32::Alphabet::RFC4648 { padding: true }, secret)?;

        let expected = totp(&stepper, &decoded_secret, std::time::SystemTime::now());

        let code = u32::from_str_radix(code, 10)?;

        Ok(expected == code)
    }
}
```

### Session Management

```rust
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserSession {
    user_id: Uuid,
    session_id: String,
    device_fingerprint: String,
    ip_address: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    last_activity: DateTime<Utc>,
    is_active: bool,
}

struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, UserSession>>>,
}

impl SessionManager {
    async fn create_session(&self, user_id: &Uuid, device_fingerprint: &str, ip_address: &str) -> Result<String> {
        let session_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = now + Duration::hours(1);

        let session = UserSession {
            user_id: *user_id,
            session_id: session_id.clone(),
            device_fingerprint: device_fingerprint.to_string(),
            ip_address: ip_address.to_string(),
            created_at: now,
            expires_at,
            last_activity: now,
            is_active: true,
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id.clone(), session);

        Ok(session_id)
    }

    async fn validate_session(&self, session_id: &str) -> Option<UserSession> {
        let sessions = self.sessions.read().await;

        if let Some(session) = sessions.get(session_id) {
            if session.expires_at > Utc::now() && session.is_active {
                return Some(session.clone());
            }
        }

        None
    }

    async fn revoke_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.is_active = false;
        }
        Ok(())
    }
}
```

### API Integration Example

```rust
use axum::{extract::State, http::StatusCode, response::Json, Router};
use serde_json::json;

type AuthState = Arc<AppState>;

struct AppState {
    auth_service: AuthService,
    user_repository: Arc<dyn UserRepository>,
    security_service: Arc<dyn SecurityService>,
    permission_service: Arc<Mutex<PermissionService>>,
}

// Login endpoint
async fn login(
    State(state): State<AuthState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let auth_request = AuthRequest {
        email: request.email,
        password: request.password,
        remember_me: request.remember_me,
        device_info: request.device_info,
        ip_address: request.ip_address,
        user_agent: request.user_agent,
    };

    match state.auth_service.authenticate_enhanced(
        auth_request,
        state.user_repository.as_ref(),
        state.security_service.as_ref()
    ).await {
        Ok(result) => Ok(Json(LoginResponse {
            user_id: result.user_id,
            token: result.token,
            refresh_token: result.refresh_token,
            expires_at: result.expires_at,
            requires_2fa: result.requires_2fa,
            security_flags: result.security_flags,
        })),
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}

// Protected endpoint
async fn protected_endpoint(
    State(state): State<AuthState>,
    auth: AuthMiddleware,
) -> Result<Json<Value>, StatusCode> {
    // Check specific permission
    let user_id = auth.user_id.to_string();
    let has_permission = state.permission_service
        .lock()
        .await
        .has_permission(&user_id, "admin:users")?;

    if !has_permission {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(Json(json!({
        "message": "Access granted",
        "user_id": auth.user_id
    })))
}

let app = Router::new()
    .route("/login", axum::routing::post(login))
    .route("/protected", axum::routing::get(protected_endpoint))
    .layer(axum::middleware::from_fn(auth_middleware));
```

## 🧪 Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    #[tokio::test]
    async fn test_jwt_flow() -> Result<()> {
        let auth_service = AuthService::with_secret("test_secret");

        // Create token
        let user_id = Uuid::new_v4();
        let token = auth_service.generate_token(&user_id).await?;

        // Validate token
        let validation = auth_service.validate_token(&token).await?;

        assert!(validation.valid);
        assert_eq!(validation.user_id.unwrap(), user_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_permission_system() -> Result<()> {
        let mut permission_service = PermissionService::new();
        let user_id = "test_user";

        // Assign admin role
        permission_service.assign_role(user_id, "admin")?;

        // Check admin permissions
        assert!(permission_service.has_permission(user_id, "admin:all")?);
        assert!(permission_service.has_permission(user_id, "random:permission")?); // Admin should have all

        // Remove admin role
        permission_service.remove_role(user_id, "admin")?;

        // Should no longer have admin permissions
        assert!(!permission_service.has_permission(user_id, "admin:all")?);

        Ok(())
    }

    #[test]
    fn test_password_validation() {
        let auth_service = AuthService::with_secret("test");

        // Valid passwords
        assert!(auth_service.is_valid_password_format("SecurePass123"));
        assert!(auth_service.is_valid_password_format("MyP@ssw0rd!"));

        // Invalid passwords
        assert!(!auth_service.is_valid_password_format("short"));
        assert!(!auth_service.is_valid_password_format("nouppercase1"));
        assert!(!auth_service.is_valid_password_format("NOLOWERCASE1"));
        assert!(!auth_service.is_valid_password_format("NoNumbers!"));
    }

    #[test]
    fn test_email_validation() {
        let auth_service = AuthService::with_secret("test");

        // Valid emails
        assert!(auth_service.is_valid_email("user@example.com"));
        assert!(auth_service.is_valid_email("test.email+tag@domain.co.uk"));

        // Invalid emails
        assert!(!auth_service.is_valid_email("invalid-email"));
        assert!(!auth_service.is_valid_email("@domain.com"));
        assert!(!auth_service.is_valid_email("user@"));
    }
}
```

### Integration Tests

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use sqlx::PgPool;

    async fn setup_test_db() -> PgPool {
        dotenv::dotenv().ok();

        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/backbone_auth_test".to_string());

        let pool = PgPool::connect(&database_url).await.unwrap();

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        pool
    }

    #[tokio::test]
    async fn test_complete_auth_flow() -> Result<()> {
        let pool = setup_test_db().await;
        let user_repository = Arc::new(PostgresUserRepository::new(pool));
        let security_service = Arc::new(MockSecurityService::new());

        let auth_service = AuthService::new(AuthServiceConfig {
            jwt_secret: "test_secret".to_string(),
            token_expiry_hours: 1,
        });

        // Create test user
        let user_id = Uuid::new_v4();
        let user = User {
            id: user_id,
            email: "test@example.com".to_string(),
            password_hash: hash_password("SecurePass123")?,
            is_active: true,
            is_locked: false,
            roles: vec!["user".to_string()],
            // ... other fields
        };

        user_repository.create(&user).await?;

        // Test authentication
        let auth_request = AuthRequest {
            email: "test@example.com".to_string(),
            password: "SecurePass123".to_string(),
            remember_me: Some(false),
            device_info: None,
            ip_address: Some("127.0.0.1".to_string()),
            user_agent: Some("test-agent".to_string()),
        };

        let result = auth_service.authenticate_enhanced(
            auth_request,
            user_repository.as_ref(),
            security_service.as_ref()
        ).await?;

        assert_eq!(result.user_id, user_id);
        assert!(!result.token.is_empty());
        assert!(!result.requires_2fa);

        // Test token validation
        let validation = auth_service.validate_token(&result.token).await?;
        assert!(validation.valid);
        assert_eq!(validation.user_id.unwrap(), user_id);

        Ok(())
    }
}
```

## 📊 Performance & Security

### Security Headers

```rust
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

// Security middleware
let app = Router::new()
    .layer(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    )
    .layer(
        tower_http::compression::CompressionLayer::new()
    )
    .layer(
        tower_http::trace::TraceLayer::new_for_http()
    )
    .layer(axum::middleware::from_fn(security_headers_middleware));

async fn security_headers_middleware(
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;

    let headers = response.headers_mut();

    // Security headers
    headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
    headers.insert("X-Frame-Options", "DENY".parse().unwrap());
    headers.insert("X-XSS-Protection", "1; mode=block".parse().unwrap());
    headers.insert(
        "Strict-Transport-Security",
        "max-age=31536000; includeSubDomains".parse().unwrap()
    );
    headers.insert("Content-Security-Policy", "default-src 'self'".parse().unwrap());

    response
}
```

### Monitoring & Metrics

```rust
use prometheus::{Counter, Histogram, register_counter, register_histogram};

lazy_static! {
    static ref AUTH_REQUESTS_TOTAL: Counter = register_counter!(
        "auth_requests_total",
        "Total number of authentication requests"
    ).unwrap();

    static ref AUTH_DURATION: Histogram = register_histogram!(
        "auth_request_duration_seconds",
        "Authentication request duration in seconds"
    ).unwrap();

    static ref AUTH_FAILURES_TOTAL: Counter = register_counter!(
        "auth_failures_total",
        "Total number of authentication failures"
    ).unwrap();
}

// In authentication handler
async fn authenticate_with_metrics(
    auth_service: &AuthService,
    request: AuthRequest,
) -> Result<AuthResultEnhanced> {
    let timer = AUTH_DURATION.start_timer();
    AUTH_REQUESTS_TOTAL.inc();

    let result = auth_service.authenticate_enhanced(request, &user_repo, &security_service).await;

    if result.is_err() {
        AUTH_FAILURES_TOTAL.inc();
    }

    timer.observe_duration();

    result
}
```

## 🚀 Deployment

### Docker Configuration

```dockerfile
# Dockerfile
FROM rust:1.75-slim as builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/backbone-auth /usr/local/bin/backbone-auth

EXPOSE 8080

ENV JWT_SECRET=your_production_secret_here
ENV DATABASE_URL=postgresql://user:password@db:5432/backbone_auth
ENV REDIS_URL=redis://redis:6379

CMD ["backbone-auth"]
```

```yaml
# docker-compose.yml
version: '3.8'

services:
  app:
    build: .
    ports:
      - "8080:8080"
    environment:
      - JWT_SECRET=${JWT_SECRET}
      - DATABASE_URL=postgresql://postgres:${POSTGRES_PASSWORD}@db:5432/backbone_auth
      - REDIS_URL=redis://redis:6379
    depends_on:
      - db
      - redis

  db:
    image: postgres:15
    environment:
      - POSTGRES_DB=backbone_auth
      - POSTGRES_USER=postgres
      - POSTGRES_PASSWORD=${POSTGRES_PASSWORD}
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./migrations:/docker-entrypoint-initdb.d

  redis:
    image: redis:7-alpine
    volumes:
      - redis_data:/data

volumes:
  postgres_data:
  redis_data:
```

### Kubernetes Deployment

```yaml
# k8s/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: backbone-auth
spec:
  replicas: 3
  selector:
    matchLabels:
      app: backbone-auth
  template:
    metadata:
      labels:
        app: backbone-auth
    spec:
      containers:
      - name: backbone-auth
        image: backbone-auth:latest
        ports:
        - containerPort: 8080
        env:
        - name: JWT_SECRET
          valueFrom:
            secretKeyRef:
              name: auth-secrets
              key: jwt-secret
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: db-credentials
              key: url
        resources:
          requests:
            cpu: 100m
            memory: 128Mi
          limits:
            cpu: 500m
            memory: 512Mi
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 5
```

## 📚 API Reference

For complete API documentation, see:

- **[docs.rs](https://docs.rs/backbone-auth)** - Full Rust documentation
- **[Examples](../../../examples/backbone-auth/)** - Complete working examples
- **[Integration Guide](../../../docs/technical/INTEGRATION.md)** - Database integration patterns
- **[Security Guide](../../../docs/technical/SECURITY.md)** - Security best practices

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Development Setup

```bash
# Clone the repository
git clone https://github.com/backbone-framework/backbone-auth.git
cd backbone-auth

# Install dependencies
cargo build

# Run tests
cargo test

# Run examples
cargo run --example basic_usage

# Check code formatting
cargo fmt --check

# Run clippy
cargo clippy -- -D warnings
```

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🔗 Related Projects

- **[Backbone Framework](https://github.com/backbone-framework)** - Complete modular monolith framework
- **[Backbone Core](https://github.com/backbone-framework/backbone-core)** - Core components and utilities
- **[Backbone ORM](https://github.com/backbone-framework/backbone-orm)** - Database abstraction layer

---

<div align="center">

**Built with ❤️ by the Backbone Framework Team**

For enterprise support, consulting, or custom development, please contact us at **team@backbone.dev**

</div>