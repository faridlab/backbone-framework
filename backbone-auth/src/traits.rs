//! Trait definitions for authentication system integration
//!
//! This module provides **generic traits** for authentication.
//! Modules can implement these traits with their own domain entities.
//!
//! ## Generic Design
//!
//! Instead of hardcoded entity structs, this module uses traits:
//! - `AuthenticatableUser` - Trait for user entities that can be authenticated
//! - `UserRepository` - Generic repository trait for user operations
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Module implements trait for its domain User entity
//! impl AuthenticatableUser for MyUser {
//!     fn id(&self) -> &Uuid { &self.id }
//!     fn email(&self) -> &str { &self.email }
//!     fn password_hash(&self) -> &str { &self.password_hash }
//!     // ... other methods
//! }
//! ```

use anyhow::Result;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

// ============================================================================
// GENERIC TRAITS - Modules implement these with their domain entities
// ============================================================================

/// Trait for user entities that can be authenticated
///
/// Implement this trait for your domain User entity to use with authentication.
pub trait AuthenticatableUser: Clone + Send + Sync {
    /// User ID
    fn id(&self) -> &Uuid;

    /// User email address
    fn email(&self) -> &str;

    /// Password hash for verification
    fn password_hash(&self) -> &str;

    /// Whether the user account is active
    fn is_active(&self) -> bool;

    /// Whether the user account is locked
    fn is_locked(&self) -> bool;

    /// User roles (e.g., ["admin", "user"])
    fn roles(&self) -> &[String];

    /// Whether 2FA is enabled
    fn two_factor_enabled(&self) -> bool { false }

    /// 2FA methods available to user
    fn two_factor_methods(&self) -> &[String] { &[] }

    /// Account expiration timestamp
    fn account_expires_at(&self) -> Option<DateTime<Utc>> { None }

    /// Whether user must change password
    fn requires_password_change(&self) -> bool { false }

    /// Last login timestamp
    fn last_login_at(&self) -> Option<DateTime<Utc>> { None }

    /// Number of failed login attempts
    fn failed_login_attempts(&self) -> u32 { 0 }

    /// When account lockout expires
    fn locked_until(&self) -> Option<DateTime<Utc>> { None }
}

// ============================================================================
// DEFAULT IMPLEMENTATIONS - Simple structs for testing/demos
// ============================================================================

/// Simple user struct for default authentication implementation
///
/// For production, modules should implement `AuthenticatableUser` trait
/// for their own domain User entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleUser {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub is_active: bool,
    pub is_locked: bool,
    pub roles: Vec<String>,
    pub two_factor_enabled: bool,
    pub two_factor_methods: Vec<String>,
    pub account_expires_at: Option<DateTime<Utc>>,
    pub requires_password_change: bool,
    pub last_login_at: Option<DateTime<Utc>>,
    pub failed_login_attempts: u32,
    pub locked_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AuthenticatableUser for SimpleUser {
    fn id(&self) -> &Uuid { &self.id }
    fn email(&self) -> &str { &self.email }
    fn password_hash(&self) -> &str { &self.password_hash }
    fn is_active(&self) -> bool { self.is_active }
    fn is_locked(&self) -> bool { self.is_locked }
    fn roles(&self) -> &[String] { &self.roles }
    fn two_factor_enabled(&self) -> bool { self.two_factor_enabled }
    fn two_factor_methods(&self) -> &[String] { &self.two_factor_methods }
    fn account_expires_at(&self) -> Option<DateTime<Utc>> { self.account_expires_at }
    fn requires_password_change(&self) -> bool { self.requires_password_change }
    fn last_login_at(&self) -> Option<DateTime<Utc>> { self.last_login_at }
    fn failed_login_attempts(&self) -> u32 { self.failed_login_attempts }
    fn locked_until(&self) -> Option<DateTime<Utc>> { self.locked_until }
}

// ============================================================================
// BACKWARDS COMPATIBILITY - Type alias for existing code
// ============================================================================

/// Type alias for backwards compatibility
///
/// DEPRECATED: Use `SimpleUser` or implement `AuthenticatableUser` trait.
pub type User = SimpleUser;

/// Refresh token claims for token refresh functionality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshTokenClaims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
    pub iss: String,
    pub token_type: String,
}

/// Generic user repository trait for database operations
///
/// The generic type `U` must implement `AuthenticatableUser`.
#[async_trait]
pub trait UserRepository<U: AuthenticatableUser = SimpleUser>: Send + Sync {
    /// Find user by email address
    async fn find_by_email(&self, email: &str) -> Result<Option<U>>;

    /// Find user by ID
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<U>>;

    /// Create new user
    async fn create(&self, user: &U) -> Result<U>;

    /// Update user
    async fn update(&self, user: &U) -> Result<U>;

    /// Update user's last login timestamp
    async fn update_last_login(&self, user_id: &Uuid) -> Result<()>;

    /// Increment failed login attempts
    async fn increment_failed_attempts(&self, user_id: &Uuid) -> Result<()>;

    /// Reset failed login attempts
    async fn reset_failed_attempts(&self, user_id: &Uuid) -> Result<()>;

    /// Lock user account
    async fn lock_account(&self, user_id: &Uuid, locked_until: Option<DateTime<Utc>>) -> Result<()>;

    /// Unlock user account
    async fn unlock_account(&self, user_id: &Uuid) -> Result<()>;

    /// Check if user exists by email
    async fn exists_by_email(&self, email: &str) -> Result<bool>;
}

/// Security service trait for authentication security features
#[async_trait]
pub trait SecurityService: Send + Sync {
    /// Check rate limiting for authentication attempts
    async fn check_rate_limit(&self, email: &str, ip_address: Option<&str>) -> Result<()>;

    /// Log failed authentication attempt for security monitoring
    async fn log_failed_auth_attempt(&self, user_id: &Uuid, ip_address: Option<&str>) -> Result<()>;

    /// Log successful authentication for audit trail
    async fn log_successful_auth(&self, user_id: &Uuid, ip_address: Option<&str>) -> Result<()>;

    /// Analyze login attempt for security threats
    async fn analyze_login_attempt(
        &self,
        user_id: &Uuid,
        device_info: &Option<DeviceInfo>,
        ip_address: Option<&str>
    ) -> Result<SecurityFlags>;

    /// Generate password reset token
    async fn generate_password_reset_token(&self, user_id: &Uuid) -> Result<String>;

    /// Validate password reset token
    async fn validate_password_reset_token(&self, token: &str) -> Result<Option<Uuid>>;

    /// Send security alert (new device, suspicious activity, etc.)
    async fn send_security_alert(&self, user_id: &Uuid, alert_type: SecurityAlertType, details: &str) -> Result<()>;
}

/// Device information for security tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: Option<String>,
    pub device_type: String, // "web", "mobile", "api"
    pub platform: Option<String>, // "ios", "android", "windows", etc.
    pub user_agent: Option<String>,
    pub fingerprint: Option<String>,
}

/// Security flags for authentication result
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecurityFlags {
    pub new_device: bool,
    pub new_location: bool,
    pub suspicious_activity: bool,
    pub requires_password_change: bool,
    pub risk_score: f32, // 0.0 to 1.0
}

/// Security alert types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityAlertType {
    NewDeviceLogin,
    NewLocationLogin,
    SuspiciousActivity,
    AccountLocked,
    PasswordReset,
    MultipleFailedAttempts,
}

/// Authentication context for enhanced security
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub device_fingerprint: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub session_id: Option<String>,
}

impl Default for AuthContext {
    fn default() -> Self {
        Self {
            ip_address: None,
            user_agent: None,
            device_fingerprint: None,
            timestamp: Utc::now(),
            session_id: None,
        }
    }
}

/// Password strength requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordPolicy {
    pub min_length: usize,
    pub max_length: usize,
    pub require_uppercase: bool,
    pub require_lowercase: bool,
    pub require_numbers: bool,
    pub require_special_chars: bool,
    pub forbidden_patterns: Vec<String>,
    pub common_passwords: Vec<String>,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_length: 8,
            max_length: 128,
            require_uppercase: true,
            require_lowercase: true,
            require_numbers: true,
            require_special_chars: false,
            forbidden_patterns: vec![
                "password".to_string(),
                "123456".to_string(),
                "qwerty".to_string(),
            ],
            common_passwords: vec![
                "password".to_string(),
                "123456".to_string(),
                "123456789".to_string(),
                "qwerty".to_string(),
                "abc123".to_string(),
                "password123".to_string(),
                "admin".to_string(),
                "letmein".to_string(),
                "welcome".to_string(),
                "monkey".to_string(),
            ],
        }
    }
}

/// Two-factor authentication methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TwoFactorMethod {
    TOTP, // Time-based One-Time Password
    SMS,  // SMS verification
    Email, // Email verification
    BackupCode, // Backup codes
}

/// Two-factor authentication challenge
#[derive(Debug, Clone)]
pub struct TwoFactorChallenge {
    pub user_id: Uuid,
    pub method: TwoFactorMethod,
    pub challenge: String,
    pub expires_at: DateTime<Utc>,
}

/// Password reset request
#[derive(Debug, Clone)]
pub struct PasswordResetRequest {
    pub email: String,
    pub context: AuthContext,
}

/// Password reset confirmation
#[derive(Debug, Clone)]
pub struct PasswordResetConfirmation {
    pub token: String,
    pub new_password: String,
    pub context: AuthContext,
}

/// Enhanced authentication request
#[derive(Debug, Clone)]
pub struct AuthRequest {
    pub email: String,
    pub password: String,
    pub remember_me: Option<bool>,
    pub device_info: Option<DeviceInfo>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

/// Enhanced authentication result with security info
#[derive(Debug, Clone)]
pub struct AuthResultEnhanced {
    pub user_id: Uuid,
    pub token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub requires_2fa: bool,
    pub security_flags: SecurityFlags,
}