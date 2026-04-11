//! Validation Utilities
//!
//! Utility functions and modules for various validation operations.

use serde::{Serialize, Deserialize};

/// Compression validation utilities
pub mod compression {
    use super::*;

    /// Compression validation error
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ValidationError {
        pub code: String,
        pub message: String,
        pub field: Option<String>,
        pub current_value: Option<String>,
        pub suggested_fix: Option<String>,
    }

    impl ValidationError {
        pub fn new(
            code: &str,
            message: &str,
            field: Option<&str>,
            current_value: Option<&str>,
            suggested_fix: Option<&str>,
        ) -> Self {
            Self {
                code: code.to_string(),
                message: message.to_string(),
                field: field.map(|s| s.to_string()),
                current_value: current_value.map(|s| s.to_string()),
                suggested_fix: suggested_fix.map(|s| s.to_string()),
            }
        }
    }

    /// Compression validation warning
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ValidationWarning {
        pub code: String,
        pub message: String,
        pub field: Option<String>,
        pub current_value: Option<String>,
        pub recommendation: Option<String>,
    }

    impl ValidationWarning {
        pub fn new(
            code: &str,
            message: &str,
            field: Option<&str>,
            current_value: Option<&str>,
            recommendation: Option<&str>,
        ) -> Self {
            Self {
                code: code.to_string(),
                message: message.to_string(),
                field: field.map(|s| s.to_string()),
                current_value: current_value.map(|s| s.to_string()),
                recommendation: recommendation.map(|s| s.to_string()),
            }
        }
    }

    /// Compression validation result
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ValidationResult {
        pub is_valid: bool,
        pub errors: Vec<ValidationError>,
        pub warnings: Vec<ValidationWarning>,
    }
}

/// Monitoring validation utilities
pub mod monitoring {
    use super::*;

    /// Monitoring validation error
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ValidationError {
        pub code: String,
        pub message: String,
        pub field: Option<String>,
        pub current_value: Option<String>,
        pub suggested_fix: Option<String>,
    }

    impl ValidationError {
        pub fn new(
            code: &str,
            message: &str,
            field: Option<&str>,
            current_value: Option<&str>,
            suggested_fix: Option<&str>,
        ) -> Self {
            Self {
                code: code.to_string(),
                message: message.to_string(),
                field: field.map(|s| s.to_string()),
                current_value: current_value.map(|s| s.to_string()),
                suggested_fix: suggested_fix.map(|s| s.to_string()),
            }
        }
    }

    /// Monitoring validation warning
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ValidationWarning {
        pub code: String,
        pub message: String,
        pub field: Option<String>,
        pub current_value: Option<String>,
        pub recommendation: Option<String>,
    }

    impl ValidationWarning {
        pub fn new(
            code: &str,
            message: &str,
            field: Option<&str>,
            current_value: Option<&str>,
            recommendation: Option<&str>,
        ) -> Self {
            Self {
                code: code.to_string(),
                message: message.to_string(),
                field: field.map(|s| s.to_string()),
                current_value: current_value.map(|s| s.to_string()),
                recommendation: recommendation.map(|s| s.to_string()),
            }
        }
    }

    /// Monitoring validation result
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ValidationResult {
        pub is_valid: bool,
        pub errors: Vec<ValidationError>,
        pub warnings: Vec<ValidationWarning>,
    }
}

/// FIFO validation utilities
pub mod fifo {
    use super::*;

    /// FIFO validation result
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ValidationResult {
        pub is_valid: bool,
        pub errors: Vec<String>,
    }

    impl ValidationResult {
        pub fn new(is_valid: bool, errors: Vec<String>) -> Self {
            Self { is_valid, errors }
        }
    }
}

/// Common validation utilities
pub struct ValidationUtils;

impl ValidationUtils {
    /// Validate string length
    pub fn validate_string_length(
        value: &str,
        field_name: &str,
        min_length: usize,
        max_length: usize,
    ) -> Vec<String> {
        let mut errors = Vec::new();

        if value.len() < min_length {
            errors.push(format!(
                "{} must be at least {} characters long (got {})",
                field_name,
                min_length,
                value.len()
            ));
        }

        if value.len() > max_length {
            errors.push(format!(
                "{} must be at most {} characters long (got {})",
                field_name,
                max_length,
                value.len()
            ));
        }

        errors
    }

    /// Validate numeric range
    pub fn validate_numeric_range<T>(
        value: T,
        field_name: &str,
        min_value: T,
        max_value: T,
    ) -> Vec<String>
    where
        T: PartialOrd + std::fmt::Display,
    {
        let mut errors = Vec::new();

        if value < min_value {
            errors.push(format!(
                "{} must be at least {} (got {})",
                field_name, min_value, value
            ));
        }

        if value > max_value {
            errors.push(format!(
                "{} must be at most {} (got {})",
                field_name, max_value, value
            ));
        }

        errors
    }

    /// Validate URL format
    pub fn validate_url(url: &str, field_name: &str, allowed_schemes: &[&str]) -> Vec<String> {
        let mut errors = Vec::new();

        if url.is_empty() {
            errors.push(format!("{} cannot be empty", field_name));
            return errors;
        }

        // Check scheme
        let has_valid_scheme = allowed_schemes.iter().any(|&scheme| {
            url.starts_with(&format!("{}://", scheme)) || url.starts_with(&format!("{}://", scheme.to_uppercase()))
        });

        if !has_valid_scheme {
            errors.push(format!(
                "{} must start with one of: {}",
                field_name,
                allowed_schemes.join(", ")
            ));
        }

        errors
    }

    /// Validate email format
    pub fn validate_email(email: &str, field_name: &str) -> Vec<String> {
        let mut errors = Vec::new();

        if email.is_empty() {
            errors.push(format!("{} cannot be empty", field_name));
            return errors;
        }

        // Basic email validation
        if !email.contains('@') {
            errors.push(format!("{} must contain '@' symbol", field_name));
        }

        if email.starts_with('@') || email.ends_with('@') {
            errors.push(format!("{} cannot start or end with '@'", field_name));
        }

        if !email.contains('.') {
            errors.push(format!("{} must contain domain with '.'", field_name));
        }

        errors
    }

    /// Validate JSON structure
    pub fn validate_json(json_str: &str, field_name: &str) -> Vec<String> {
        let mut errors = Vec::new();

        match serde_json::from_str::<serde_json::Value>(json_str) {
            Ok(_) => {}, // Valid JSON
            Err(e) => {
                errors.push(format!("{} contains invalid JSON: {}", field_name, e));
            }
        }

        errors
    }

    /// Check if string contains only allowed characters
    pub fn validate_allowed_characters(
        value: &str,
        field_name: &str,
        allowed_chars: &str,
    ) -> Vec<String> {
        let mut errors = Vec::new();

        for (i, c) in value.chars().enumerate() {
            if !allowed_chars.contains(c) {
                errors.push(format!(
                    "{} contains invalid character '{}' at position {}",
                    field_name, c, i
                ));
            }
        }

        errors
    }

    /// Validate against regex pattern
    pub fn validate_regex(
        value: &str,
        field_name: &str,
        pattern: &str,
        error_message: Option<&str>,
    ) -> Vec<String> {
        let mut errors = Vec::new();

        match regex::Regex::new(pattern) {
            Ok(re) => {
                if !re.is_match(value) {
                    errors.push(format!(
                        "{} does not match required pattern{}",
                        field_name,
                        error_message.map(|msg| format!(": {}", msg)).unwrap_or_default()
                    ));
                }
            }
            Err(e) => {
                errors.push(format!("Invalid regex pattern for {}: {}", field_name, e));
            }
        }

        errors
    }
}

