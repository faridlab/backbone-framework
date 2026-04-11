//! Configuration error types

use std::path::PathBuf;
use thiserror::Error;

/// Result type for configuration operations
pub type ConfigResult<T> = Result<T, ConfigError>;

/// Configuration error types
#[derive(Debug, Error)]
pub enum ConfigError {
    /// File not found
    #[error("Configuration file not found: {path}")]
    FileNotFound { path: PathBuf },

    /// File read error
    #[error("Failed to read configuration file {path}: {source}")]
    ReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// YAML parse error
    #[error("Failed to parse YAML configuration: {0}")]
    YamlParseError(#[from] serde_yaml::Error),

    /// JSON parse error
    #[error("Failed to parse JSON configuration: {0}")]
    JsonParseError(#[from] serde_json::Error),

    /// TOML parse error
    #[error("Failed to parse TOML configuration: {0}")]
    TomlParseError(#[from] toml::de::Error),

    /// Unsupported file format
    #[error("Unsupported configuration file format: {extension}")]
    UnsupportedFormat { extension: String },

    /// Validation error
    #[error("Configuration validation error: {message}")]
    ValidationError { message: String },

    /// Missing required field
    #[error("Required configuration field missing: {field}")]
    RequiredField { field: String },

    /// Invalid value
    #[error("Invalid configuration value for {field}: {value} - {reason}")]
    InvalidValue {
        field: String,
        value: String,
        reason: String,
    },

    /// Environment variable error
    #[error("Environment variable error for {var}")]
    EnvVarError { var: String },

    /// Environment variable substitution error
    #[error("Failed to substitute environment variable {var}: {reason}")]
    SubstitutionError { var: String, reason: String },

    /// Module configuration error
    #[error("Module '{module}' configuration error: {message}")]
    ModuleError { module: String, message: String },
}

impl ConfigError {
    /// Create a file not found error
    pub fn file_not_found<P: Into<PathBuf>>(path: P) -> Self {
        Self::FileNotFound { path: path.into() }
    }

    /// Create a read error
    pub fn read_error<P: Into<PathBuf>>(path: P, source: std::io::Error) -> Self {
        Self::ReadError {
            path: path.into(),
            source,
        }
    }

    /// Create a validation error
    pub fn validation<S: Into<String>>(message: S) -> Self {
        Self::ValidationError {
            message: message.into(),
        }
    }

    /// Create a required field error
    pub fn required_field<S: Into<String>>(field: S) -> Self {
        Self::RequiredField {
            field: field.into(),
        }
    }

    /// Create an invalid value error
    pub fn invalid_value<S: Into<String>>(field: S, value: S, reason: S) -> Self {
        Self::InvalidValue {
            field: field.into(),
            value: value.into(),
            reason: reason.into(),
        }
    }

    /// Create an environment variable error
    pub fn env_var<S: Into<String>>(var: S) -> Self {
        Self::EnvVarError {
            var: var.into(),
        }
    }

    /// Create a substitution error
    pub fn substitution<S: Into<String>>(var: S, reason: S) -> Self {
        Self::SubstitutionError {
            var: var.into(),
            reason: reason.into(),
        }
    }

    /// Create a module error
    pub fn module<S: Into<String>>(module: S, message: S) -> Self {
        Self::ModuleError {
            module: module.into(),
            message: message.into(),
        }
    }

    /// Create an unsupported format error
    pub fn unsupported_format<S: Into<String>>(extension: S) -> Self {
        Self::UnsupportedFormat {
            extension: extension.into(),
        }
    }
}

// Allow conversion from simple env var error
impl From<std::env::VarError> for ConfigError {
    fn from(_err: std::env::VarError) -> Self {
        Self::EnvVarError {
            var: "unknown".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = ConfigError::file_not_found("test.yml");
        assert!(err.to_string().contains("test.yml"));

        let err = ConfigError::validation("Invalid port");
        assert!(err.to_string().contains("Invalid port"));

        let err = ConfigError::required_field("database.url");
        assert!(err.to_string().contains("database.url"));

        let err = ConfigError::invalid_value("port", "abc", "must be a number");
        assert!(err.to_string().contains("port"));
        assert!(err.to_string().contains("abc"));
    }
}
