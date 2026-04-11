//! Security configuration
//!
//! Defines CORS, CSRF, and security headers settings.

use serde::{Deserialize, Serialize};

fn default_true() -> bool { true }

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable CORS
    #[serde(default = "default_true")]
    pub cors_enabled: bool,
    /// Allowed CORS origins
    #[serde(default = "default_cors_origins")]
    pub cors_origins: Vec<String>,
    /// Allowed CORS methods
    #[serde(default = "default_cors_methods")]
    pub cors_methods: Vec<String>,
    /// Allowed CORS headers
    #[serde(default = "default_cors_headers")]
    pub cors_headers: Vec<String>,
    /// CSRF configuration
    #[serde(default)]
    pub csrf: Option<CsrfConfig>,
    /// Security headers configuration
    #[serde(default)]
    pub headers: Option<SecurityHeadersConfig>,
}

fn default_cors_origins() -> Vec<String> { vec!["http://localhost:3000".to_string()] }
fn default_cors_methods() -> Vec<String> {
    vec!["GET".to_string(), "POST".to_string(), "PUT".to_string(), "DELETE".to_string(), "PATCH".to_string(), "OPTIONS".to_string()]
}
fn default_cors_headers() -> Vec<String> {
    vec!["Content-Type".to_string(), "Authorization".to_string()]
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            cors_enabled: true,
            cors_origins: default_cors_origins(),
            cors_methods: default_cors_methods(),
            cors_headers: default_cors_headers(),
            csrf: None,
            headers: Some(SecurityHeadersConfig::default()),
        }
    }
}

/// CSRF configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsrfConfig {
    /// Enable CSRF protection
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// CSRF token length
    #[serde(default = "default_csrf_token_length")]
    pub token_length: u32,
    /// Token expiration in seconds
    #[serde(default = "default_csrf_expires")]
    pub expires_in: u64,
}

fn default_csrf_token_length() -> u32 { 32 }
fn default_csrf_expires() -> u64 { 3600 }

impl Default for CsrfConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            token_length: default_csrf_token_length(),
            expires_in: default_csrf_expires(),
        }
    }
}

/// Security headers configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityHeadersConfig {
    /// X-Frame-Options header
    #[serde(default = "default_x_frame_options")]
    pub x_frame_options: Option<String>,
    /// X-Content-Type-Options header
    #[serde(default = "default_x_content_type_options")]
    pub x_content_type_options: Option<String>,
    /// X-XSS-Protection header
    #[serde(default = "default_x_xss_protection")]
    pub x_xss_protection: Option<String>,
    /// Strict-Transport-Security header
    #[serde(default = "default_hsts")]
    pub strict_transport_security: Option<String>,
}

fn default_x_frame_options() -> Option<String> { Some("DENY".to_string()) }
fn default_x_content_type_options() -> Option<String> { Some("nosniff".to_string()) }
fn default_x_xss_protection() -> Option<String> { Some("1; mode=block".to_string()) }
fn default_hsts() -> Option<String> { Some("max-age=31536000; includeSubDomains".to_string()) }

impl Default for SecurityHeadersConfig {
    fn default() -> Self {
        Self {
            x_frame_options: default_x_frame_options(),
            x_content_type_options: default_x_content_type_options(),
            x_xss_protection: default_x_xss_protection(),
            strict_transport_security: default_hsts(),
        }
    }
}
