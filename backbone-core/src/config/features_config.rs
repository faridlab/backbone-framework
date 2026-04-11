//! Feature flags configuration
//!
//! Defines application feature toggles and rate limiting.

use serde::{Deserialize, Serialize};

fn default_true() -> bool { true }
fn default_false() -> bool { false }

/// Feature flags configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturesConfig {
    /// Enable user registration
    #[serde(default = "default_true")]
    pub user_registration: bool,
    /// Enable email verification
    #[serde(default = "default_true")]
    pub email_verification: bool,
    /// Enable password reset
    #[serde(default = "default_true")]
    pub password_reset: bool,
    /// Enable two-factor authentication
    #[serde(default = "default_false")]
    pub two_factor_auth: bool,
    /// Enable social login
    #[serde(default = "default_false")]
    pub social_login: bool,
    /// Enable audit logging
    #[serde(default = "default_true")]
    pub audit_logging: bool,
    /// Enable rate limiting
    #[serde(default = "default_true")]
    pub rate_limiting: bool,
    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limiting_config: Option<RateLimitingConfig>,
}

impl Default for FeaturesConfig {
    fn default() -> Self {
        Self {
            user_registration: true,
            email_verification: true,
            password_reset: true,
            two_factor_auth: false,
            social_login: false,
            audit_logging: true,
            rate_limiting: true,
            rate_limiting_config: Some(RateLimitingConfig::default()),
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitingConfig {
    /// Default requests per hour limit
    #[serde(default = "default_rate_limit")]
    pub default_limit: u32,
    /// Burst limit
    #[serde(default = "default_burst_limit")]
    pub burst_limit: u32,
    /// Storage backend (redis, memory)
    #[serde(default = "default_rate_storage")]
    pub storage: String,
}

fn default_rate_limit() -> u32 { 1000 }
fn default_burst_limit() -> u32 { 100 }
fn default_rate_storage() -> String { "redis".to_string() }

impl Default for RateLimitingConfig {
    fn default() -> Self {
        Self {
            default_limit: default_rate_limit(),
            burst_limit: default_burst_limit(),
            storage: default_rate_storage(),
        }
    }
}
