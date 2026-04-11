//! Configuration validation and schema
//!
//! Provides comprehensive validation for all configuration sections.

use super::{BackboneConfig, ConfigError, ConfigResult};
use thiserror::Error;

/// Configuration validation error
#[derive(Debug, Error)]
pub enum ConfigValidationError {
    /// Unknown module
    #[error("Unknown module: {0}")]
    UnknownModule(String),

    /// Invalid configuration
    #[error("Invalid configuration for {module}: {error}")]
    InvalidConfig { module: String, error: String },

    /// Required field missing
    #[error("Required field missing: {field}")]
    RequiredField { field: String },

    /// Invalid value
    #[error("Invalid value for field {field}: {value}")]
    InvalidValue { field: String, value: String },

    /// Multiple errors
    #[error("Configuration has {0} errors")]
    MultipleErrors(usize),
}

/// Validate the entire configuration
///
/// Returns detailed errors for any invalid configuration.
pub fn validate_config(config: &BackboneConfig) -> ConfigResult<()> {
    let mut errors: Vec<String> = Vec::new();

    // Validate server configuration
    if let Err(e) = validate_server(&config.server) {
        errors.push(e);
    }

    // Validate database configurations
    for (name, db_config) in &config.database {
        if let Err(e) = validate_database(name, db_config) {
            errors.push(e);
        }
    }

    // Validate at least one database exists
    if config.database.is_empty() {
        errors.push("At least one database configuration is required".to_string());
    }

    // Validate default database exists
    if !config.database.contains_key("default") {
        errors.push("Default database configuration is required".to_string());
    }

    // Validate modules
    if let Err(e) = validate_sapiens_module(&config.modules.sapiens, config.app.environment) {
        errors.push(e);
    }

    if let Err(e) = validate_postman_module(&config.modules.postman) {
        errors.push(e);
    }

    if let Err(e) = validate_bucket_module(&config.modules.bucket) {
        errors.push(e);
    }

    // Validate logging
    if let Err(e) = validate_logging(&config.logging) {
        errors.push(e);
    }

    // Validate security
    if let Err(e) = validate_security(&config.security) {
        errors.push(e);
    }

    // Return errors if any
    if !errors.is_empty() {
        let error_msg = errors.join("; ");
        return Err(ConfigError::validation(error_msg));
    }

    Ok(())
}

/// Validate server configuration
fn validate_server(config: &super::ServerConfig) -> Result<(), String> {
    if config.port == 0 {
        return Err("Server port must be greater than 0".to_string());
    }

    // Note: u16 max is 65535, so this check is implicit in the type

    if config.host.is_empty() {
        return Err("Server host cannot be empty".to_string());
    }

    if let Some(workers) = config.workers {
        if workers == 0 {
            return Err("Server workers must be greater than 0".to_string());
        }
        if workers > 1024 {
            return Err(format!("Server workers {} is too high (max 1024)", workers));
        }
    }

    Ok(())
}

/// Validate database configuration
fn validate_database(name: &str, config: &super::DatabaseConfig) -> Result<(), String> {
    if config.url.is_empty() {
        return Err(format!("Database '{}' URL cannot be empty", name));
    }

    // Basic URL format check
    if !config.url.starts_with("postgresql://") && !config.url.starts_with("postgres://") {
        return Err(format!(
            "Database '{}' URL must start with postgresql:// or postgres://",
            name
        ));
    }

    if config.max_connections == 0 {
        return Err(format!("Database '{}' max_connections must be > 0", name));
    }

    if config.min_connections > config.max_connections {
        return Err(format!(
            "Database '{}' min_connections ({}) cannot exceed max_connections ({})",
            name, config.min_connections, config.max_connections
        ));
    }

    Ok(())
}

/// Validate Sapiens module configuration
fn validate_sapiens_module(config: &super::SapiensConfig, env: super::Environment) -> Result<(), String> {
    if !config.enabled {
        return Ok(());
    }

    // Validate auth configuration if present
    if let Some(ref auth) = config.auth {
        // JWT secret required in production
        #[allow(clippy::collapsible_if)]
        if auth.jwt_secret.is_empty() {
            if matches!(env, super::Environment::Production) {
                return Err(
                    "Sapiens: JWT secret is required in production. Set JWT_SECRET environment variable."
                        .to_string(),
                );
            }
        }

        // Validate password hasher
        let hasher = &auth.password_hasher;
        if hasher.iterations < 1000 || hasher.iterations > 1_000_000 {
            return Err(format!(
                "Sapiens: password_hasher.iterations ({}) must be between 1000 and 1000000",
                hasher.iterations
            ));
        }

        if hasher.memory == 0 || hasher.memory > 1024 {
            return Err(format!(
                "Sapiens: password_hasher.memory ({}) must be between 1 and 1024",
                hasher.memory
            ));
        }

        if hasher.parallelism == 0 || hasher.parallelism > 128 {
            return Err(format!(
                "Sapiens: password_hasher.parallelism ({}) must be between 1 and 128",
                hasher.parallelism
            ));
        }

        if hasher.hash_length < 16 || hasher.hash_length > 128 {
            return Err(format!(
                "Sapiens: password_hasher.hash_length ({}) must be between 16 and 128",
                hasher.hash_length
            ));
        }

        // Validate token expiration
        if auth.token_expiration_hours == 0 {
            return Err("Sapiens: token_expiration_hours must be > 0".to_string());
        }

        if auth.refresh_token_expiration_days == 0 {
            return Err("Sapiens: refresh_token_expiration_days must be > 0".to_string());
        }
    }

    // Validate lockout configuration if present
    if let Some(ref lockout) = config.lockout {
        if lockout.max_attempts == 0 {
            return Err("Sapiens: lockout.max_attempts must be > 0".to_string());
        }

        if lockout.duration_minutes == 0 {
            return Err("Sapiens: lockout.duration_minutes must be > 0".to_string());
        }
    }

    Ok(())
}

/// Validate Postman module configuration
fn validate_postman_module(config: &super::PostmanConfig) -> Result<(), String> {
    if !config.enabled {
        return Ok(());
    }

    // SMTP configuration required when enabled
    match &config.smtp {
        None => {
            return Err("Postman: SMTP configuration is required when module is enabled".to_string());
        }
        Some(smtp) => {
            if smtp.host.is_empty() {
                return Err("Postman: smtp.host is required".to_string());
            }

            if smtp.port == 0 {
                return Err(format!("Postman: smtp.port ({}) is invalid", smtp.port));
            }

            if smtp.from_email.is_empty() {
                return Err("Postman: smtp.from_email is required".to_string());
            }

            // Basic email format check
            if !smtp.from_email.contains('@') {
                return Err(format!(
                    "Postman: smtp.from_email '{}' is not a valid email",
                    smtp.from_email
                ));
            }
        }
    }

    Ok(())
}

/// Validate Bucket module configuration
fn validate_bucket_module(config: &super::BucketConfig) -> Result<(), String> {
    if !config.enabled {
        return Ok(());
    }

    // Storage configuration required when enabled
    match &config.storage {
        None => {
            return Err("Bucket: storage configuration is required when module is enabled".to_string());
        }
        Some(storage) => {
            let valid_drivers = ["local", "s3", "minio"];
            if !valid_drivers.contains(&storage.driver.as_str()) {
                return Err(format!(
                    "Bucket: storage.driver '{}' is invalid. Must be one of: {:?}",
                    storage.driver, valid_drivers
                ));
            }

            // Local storage requires base_path
            #[allow(clippy::collapsible_if)]
            if storage.driver == "local" {
                if storage.base_path.as_ref().is_none_or(|p| p.is_empty()) {
                    return Err(
                        "Bucket: storage.base_path is required for local driver".to_string()
                    );
                }
            }
        }
    }

    Ok(())
}

/// Validate logging configuration
fn validate_logging(config: &super::LoggingConfig) -> Result<(), String> {
    let valid_levels = ["trace", "debug", "info", "warn", "error"];
    if !valid_levels.contains(&config.level.to_lowercase().as_str()) {
        return Err(format!(
            "Logging: level '{}' is invalid. Must be one of: {:?}",
            config.level, valid_levels
        ));
    }

    let valid_formats = ["json", "pretty", "compact"];
    if !valid_formats.contains(&config.format.to_lowercase().as_str()) {
        return Err(format!(
            "Logging: format '{}' is invalid. Must be one of: {:?}",
            config.format, valid_formats
        ));
    }

    if config.targets.is_empty() {
        return Err("Logging: at least one target is required".to_string());
    }

    let valid_targets = ["console", "file", "stdout", "stderr"];
    for target in &config.targets {
        if !valid_targets.contains(&target.to_lowercase().as_str()) {
            return Err(format!(
                "Logging: target '{}' is invalid. Must be one of: {:?}",
                target, valid_targets
            ));
        }
    }

    // File config validation
    #[allow(clippy::collapsible_if)]
    if config.targets.contains(&"file".to_string()) {
        if config.file.is_none() {
            return Err("Logging: file configuration is required when 'file' target is enabled".to_string());
        }
    }

    Ok(())
}

/// Validate security configuration
fn validate_security(config: &super::SecurityConfig) -> Result<(), String> {
    if config.cors_enabled {
        if config.cors_origins.is_empty() {
            return Err("Security: cors_origins cannot be empty when CORS is enabled".to_string());
        }

        if config.cors_methods.is_empty() {
            return Err("Security: cors_methods cannot be empty when CORS is enabled".to_string());
        }

        // Validate HTTP methods
        let valid_methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"];
        for method in &config.cors_methods {
            if !valid_methods.contains(&method.to_uppercase().as_str()) {
                return Err(format!(
                    "Security: cors_method '{}' is invalid. Must be one of: {:?}",
                    method, valid_methods
                ));
            }
        }
    }

    // CSRF validation
    if let Some(ref csrf) = config.csrf {
        if csrf.enabled {
            if csrf.token_length < 16 || csrf.token_length > 128 {
                return Err(format!(
                    "Security: csrf.token_length ({}) must be between 16 and 128",
                    csrf.token_length
                ));
            }

            if csrf.expires_in == 0 {
                return Err("Security: csrf.expires_in must be > 0".to_string());
            }
        }
    }

    Ok(())
}

/// Validate a specific module by name
#[allow(dead_code)]
pub fn validate_module(module_name: &str, config: &BackboneConfig) -> Result<(), ConfigValidationError> {
    match module_name {
        "sapiens" => {
            validate_sapiens_module(&config.modules.sapiens, config.app.environment)
                .map_err(|e| ConfigValidationError::InvalidConfig {
                    module: "sapiens".to_string(),
                    error: e,
                })
        }
        "postman" => {
            validate_postman_module(&config.modules.postman)
                .map_err(|e| ConfigValidationError::InvalidConfig {
                    module: "postman".to_string(),
                    error: e,
                })
        }
        "bucket" => {
            validate_bucket_module(&config.modules.bucket)
                .map_err(|e| ConfigValidationError::InvalidConfig {
                    module: "bucket".to_string(),
                    error: e,
                })
        }
        _ => Err(ConfigValidationError::UnknownModule(module_name.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;

    #[test]
    fn test_validate_server_valid() {
        let config = ServerConfig::default();
        assert!(validate_server(&config).is_ok());
    }

    #[test]
    fn test_validate_server_invalid_port() {
        let mut config = ServerConfig::default();
        config.port = 0;
        assert!(validate_server(&config).is_err());
    }

    #[test]
    fn test_validate_database_valid() {
        let config = DatabaseConfig::default();
        assert!(validate_database("default", &config).is_ok());
    }

    #[test]
    fn test_validate_database_invalid_url() {
        let mut config = DatabaseConfig::default();
        config.url = "mysql://localhost".to_string();
        assert!(validate_database("default", &config).is_err());
    }

    #[test]
    fn test_validate_sapiens_disabled() {
        let mut config = SapiensConfig::default();
        config.enabled = false;
        assert!(validate_sapiens_module(&config, Environment::Development).is_ok());
    }

    #[test]
    fn test_validate_sapiens_invalid_hasher() {
        let mut config = SapiensConfig::default();
        config.enabled = true;
        if let Some(ref mut auth) = config.auth {
            auth.password_hasher.iterations = 100; // Too low
        }
        assert!(validate_sapiens_module(&config, Environment::Development).is_err());
    }

    #[test]
    fn test_validate_postman_disabled() {
        let config = PostmanConfig::default();
        assert!(validate_postman_module(&config).is_ok());
    }

    #[test]
    fn test_validate_postman_enabled_no_smtp() {
        let mut config = PostmanConfig::default();
        config.enabled = true;
        config.smtp = None;
        assert!(validate_postman_module(&config).is_err());
    }

    #[test]
    fn test_validate_logging_valid() {
        let config = LoggingConfig::default();
        assert!(validate_logging(&config).is_ok());
    }

    #[test]
    fn test_validate_logging_invalid_level() {
        let mut config = LoggingConfig::default();
        config.level = "invalid".to_string();
        assert!(validate_logging(&config).is_err());
    }

    #[test]
    fn test_validate_security_valid() {
        let config = SecurityConfig::default();
        assert!(validate_security(&config).is_ok());
    }

    #[test]
    fn test_validate_security_empty_origins() {
        let mut config = SecurityConfig::default();
        config.cors_enabled = true;
        config.cors_origins = vec![];
        assert!(validate_security(&config).is_err());
    }

    #[test]
    fn test_validate_full_config() {
        let config = BackboneConfig::default();
        assert!(validate_config(&config).is_ok());
    }
}
