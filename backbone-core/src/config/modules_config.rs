//! Module configurations
//!
//! Defines configuration for bounded context modules (Sapiens, Postman, Bucket).

use serde::{Deserialize, Serialize};

// =============================================================================
// Module Container
// =============================================================================

/// Module configurations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModulesConfig {
    /// Sapiens (user management) module
    #[serde(default)]
    pub sapiens: SapiensConfig,
    /// Postman (email) module
    #[serde(default)]
    pub postman: PostmanConfig,
    /// Bucket (file storage) module
    #[serde(default)]
    pub bucket: BucketConfig,
}

#[allow(clippy::derivable_impls)]
impl Default for ModulesConfig {
    fn default() -> Self {
        Self {
            sapiens: SapiensConfig::default(),
            postman: PostmanConfig::default(),
            bucket: BucketConfig::default(),
        }
    }
}

// =============================================================================
// Sapiens Module
// =============================================================================

/// Sapiens module configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SapiensConfig {
    /// Enable module
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Database name to use
    #[serde(default)]
    pub database: Option<String>,
    /// Cache name to use
    #[serde(default)]
    pub cache: Option<String>,
    /// Bounded context name
    #[serde(default = "default_sapiens_context")]
    pub bounded_context: String,
    /// Domain version
    #[serde(default = "default_domain_version")]
    pub domain_version: String,
    /// Events this context publishes
    #[serde(default)]
    pub publishes: Vec<String>,
    /// Services this context provides
    #[serde(default)]
    pub provides: Vec<String>,
    /// Authentication configuration
    #[serde(default)]
    pub auth: Option<SapiensAuthConfig>,
    /// Lockout configuration
    #[serde(default)]
    pub lockout: Option<SapiensLockoutConfig>,
}

fn default_true() -> bool { true }
fn default_false() -> bool { false }
fn default_sapiens_context() -> String { "user-management".to_string() }
fn default_domain_version() -> String { "1.0.0".to_string() }

impl Default for SapiensConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            database: Some("default".to_string()),
            cache: Some("default".to_string()),
            bounded_context: default_sapiens_context(),
            domain_version: default_domain_version(),
            publishes: vec![
                "sapiens.user.created".to_string(),
                "sapiens.user.updated".to_string(),
                "sapiens.user.deleted".to_string(),
            ],
            provides: vec![
                "user.authentication".to_string(),
                "user.authorization".to_string(),
                "user.profile".to_string(),
            ],
            auth: Some(SapiensAuthConfig::default()),
            lockout: Some(SapiensLockoutConfig::default()),
        }
    }
}

/// Sapiens authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SapiensAuthConfig {
    /// JWT secret key
    pub jwt_secret: String,
    /// Token expiration in hours
    #[serde(default = "default_token_expiration")]
    pub token_expiration_hours: u64,
    /// Refresh token expiration in days
    #[serde(default = "default_refresh_expiration")]
    pub refresh_token_expiration_days: u64,
    /// Password hasher configuration
    #[serde(default)]
    pub password_hasher: PasswordHasherConfig,
}

fn default_token_expiration() -> u64 { 24 }
fn default_refresh_expiration() -> u64 { 30 }

impl Default for SapiensAuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: String::new(), // Must be set via env
            token_expiration_hours: default_token_expiration(),
            refresh_token_expiration_days: default_refresh_expiration(),
            password_hasher: PasswordHasherConfig::default(),
        }
    }
}

/// Password hasher configuration (Argon2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordHasherConfig {
    /// Memory cost (m_cost)
    #[serde(default = "default_iterations")]
    pub iterations: u32,
    /// Time cost (t_cost)
    #[serde(default = "default_memory")]
    pub memory: u32,
    /// Parallelism (p_cost)
    #[serde(default = "default_parallelism")]
    pub parallelism: u32,
    /// Output hash length
    #[serde(default = "default_hash_length")]
    pub hash_length: u32,
}

fn default_iterations() -> u32 { 19456 }
fn default_memory() -> u32 { 2 }
fn default_parallelism() -> u32 { 1 }
fn default_hash_length() -> u32 { 32 }

impl Default for PasswordHasherConfig {
    fn default() -> Self {
        Self {
            iterations: default_iterations(),
            memory: default_memory(),
            parallelism: default_parallelism(),
            hash_length: default_hash_length(),
        }
    }
}

/// Sapiens lockout configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SapiensLockoutConfig {
    /// Maximum failed attempts before lockout
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    /// Lockout duration in minutes
    #[serde(default = "default_lockout_duration")]
    pub duration_minutes: u32,
    /// Reset counter after hours
    #[serde(default)]
    pub reset_after_hours: Option<u32>,
}

fn default_max_attempts() -> u32 { 5 }
fn default_lockout_duration() -> u32 { 15 }

impl Default for SapiensLockoutConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_max_attempts(),
            duration_minutes: default_lockout_duration(),
            reset_after_hours: Some(24),
        }
    }
}

// =============================================================================
// Postman Module
// =============================================================================

/// Postman module configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanConfig {
    /// Enable module
    #[serde(default = "default_false")]
    pub enabled: bool,
    /// Database name to use
    #[serde(default)]
    pub database: Option<String>,
    /// Bounded context name
    #[serde(default = "default_postman_context")]
    pub bounded_context: String,
    /// Domain version
    #[serde(default = "default_domain_version")]
    pub domain_version: String,
    /// Events this context listens to
    #[serde(default)]
    pub listens_to: Vec<String>,
    /// SMTP configuration
    #[serde(default)]
    pub smtp: Option<SmtpConfig>,
    /// Email templates configuration
    #[serde(default)]
    pub templates: Option<TemplatesConfig>,
}

fn default_postman_context() -> String { "email-notification".to_string() }

impl Default for PostmanConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            database: Some("default".to_string()),
            bounded_context: default_postman_context(),
            domain_version: default_domain_version(),
            listens_to: vec![
                "sapiens.user.created".to_string(),
                "sapiens.user.email.changed".to_string(),
            ],
            smtp: None,
            templates: None,
        }
    }
}

/// SMTP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    /// SMTP host
    pub host: String,
    /// SMTP port
    #[serde(default = "default_smtp_port")]
    pub port: u16,
    /// SMTP username
    #[serde(default)]
    pub username: Option<String>,
    /// SMTP password
    #[serde(default)]
    pub password: Option<String>,
    /// Use TLS
    #[serde(default = "default_use_tls")]
    pub use_tls: Option<bool>,
    /// From email address
    pub from_email: String,
    /// From name
    #[serde(default)]
    pub from_name: Option<String>,
}

fn default_use_tls() -> Option<bool> { Some(true) }
fn default_smtp_port() -> u16 { 587 }

/// Email templates configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TemplatesConfig {
    /// Welcome email template path
    #[serde(default)]
    pub welcome_email: Option<String>,
    /// Password reset template path
    #[serde(default)]
    pub password_reset: Option<String>,
    /// Email verification template path
    #[serde(default)]
    pub email_verification: Option<String>,
}

// =============================================================================
// Bucket Module
// =============================================================================

/// Bucket module configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketConfig {
    /// Enable module
    #[serde(default = "default_false")]
    pub enabled: bool,
    /// Database name to use
    #[serde(default)]
    pub database: Option<String>,
    /// Bounded context name
    #[serde(default = "default_bucket_context")]
    pub bounded_context: String,
    /// Domain version
    #[serde(default = "default_domain_version")]
    pub domain_version: String,
    /// Storage configuration
    #[serde(default)]
    pub storage: Option<StorageConfig>,
}

fn default_bucket_context() -> String { "file-storage".to_string() }

impl Default for BucketConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            database: Some("default".to_string()),
            bounded_context: default_bucket_context(),
            domain_version: default_domain_version(),
            storage: None,
        }
    }
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage driver (local, s3, minio)
    #[serde(default = "default_storage_driver")]
    pub driver: String,
    /// Base path for local storage
    #[serde(default)]
    pub base_path: Option<String>,
    /// Maximum file size (e.g., "100MB")
    #[serde(default)]
    pub max_file_size: Option<String>,
    /// Allowed file extensions
    #[serde(default)]
    pub allowed_extensions: Option<Vec<String>>,
}

fn default_storage_driver() -> String { "local".to_string() }
