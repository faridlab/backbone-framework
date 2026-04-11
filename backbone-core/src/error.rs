//! Module Error Trait
//!
//! Provides a unified error trait for all Backbone modules.
//! This enables consistent error handling and API responses across modules.
//!
//! # Example
//!
//! ```ignore
//! use backbone_core::ModuleError;
//!
//! #[derive(Debug, thiserror::Error)]
//! pub enum UserError {
//!     #[error("User not found: {0}")]
//!     NotFound(String),
//!
//!     #[error("Email already exists: {0}")]
//!     EmailExists(String),
//!
//!     #[error("Invalid password")]
//!     InvalidPassword,
//! }
//!
//! impl ModuleError for UserError {
//!     fn error_code(&self) -> &str {
//!         match self {
//!             Self::NotFound(_) => "USER_NOT_FOUND",
//!             Self::EmailExists(_) => "EMAIL_EXISTS",
//!             Self::InvalidPassword => "INVALID_PASSWORD",
//!         }
//!     }
//!
//!     fn status_code(&self) -> u16 {
//!         match self {
//!             Self::NotFound(_) => 404,
//!             Self::EmailExists(_) => 409,
//!             Self::InvalidPassword => 401,
//!         }
//!     }
//!
//!     fn user_message(&self) -> &str {
//!         match self {
//!             Self::NotFound(_) => "The requested user was not found",
//!             Self::EmailExists(_) => "This email is already registered",
//!             Self::InvalidPassword => "Invalid password provided",
//!         }
//!     }
//! }
//! ```

use std::fmt::Debug;

/// Unified error trait for Backbone modules.
///
/// Implementing this trait allows errors to be:
/// - Converted to consistent API responses
/// - Logged with proper context
/// - Categorized for monitoring and alerting
pub trait ModuleError: std::error::Error + Send + Sync + Debug {
    /// Machine-readable error code.
    ///
    /// Format: `{ENTITY}_{ERROR_TYPE}` (e.g., `USER_NOT_FOUND`)
    fn error_code(&self) -> &str;

    /// HTTP status code for this error.
    ///
    /// Common codes:
    /// - 400: Bad Request (validation errors)
    /// - 401: Unauthorized
    /// - 403: Forbidden
    /// - 404: Not Found
    /// - 409: Conflict (duplicate, version mismatch)
    /// - 422: Unprocessable Entity
    /// - 500: Internal Server Error
    fn status_code(&self) -> u16;

    /// User-friendly error message.
    ///
    /// This message is safe to display to end users.
    /// Avoid including sensitive information.
    fn user_message(&self) -> &str;

    /// Whether this error is retriable.
    ///
    /// Returns `true` for transient errors like timeouts
    /// or temporary service unavailability.
    fn is_retriable(&self) -> bool {
        false
    }

    /// Whether this error should be logged at error level.
    ///
    /// Returns `true` for unexpected errors (500s).
    /// Returns `false` for expected errors (4xx).
    fn is_loggable(&self) -> bool {
        self.status_code() >= 500
    }

    /// Error category for monitoring.
    fn category(&self) -> ErrorCategory {
        match self.status_code() {
            400..=499 => ErrorCategory::ClientError,
            500..=599 => ErrorCategory::ServerError,
            _ => ErrorCategory::Unknown,
        }
    }

    /// Additional context for debugging.
    ///
    /// This information is logged but not exposed to users.
    fn debug_context(&self) -> Option<String> {
        None
    }
}

/// Error category for monitoring and alerting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Client-side errors (4xx).
    ClientError,
    /// Server-side errors (5xx).
    ServerError,
    /// Validation errors.
    ValidationError,
    /// Authentication/authorization errors.
    AuthError,
    /// Database errors.
    DatabaseError,
    /// External service errors.
    ExternalServiceError,
    /// Unknown category.
    Unknown,
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClientError => write!(f, "client_error"),
            Self::ServerError => write!(f, "server_error"),
            Self::ValidationError => write!(f, "validation_error"),
            Self::AuthError => write!(f, "auth_error"),
            Self::DatabaseError => write!(f, "database_error"),
            Self::ExternalServiceError => write!(f, "external_service_error"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Standard error response for API endpoints.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ErrorResponse {
    /// Machine-readable error code.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
    /// Additional error details (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    /// Create a new error response from a ModuleError.
    pub fn from_error<E: ModuleError>(error: &E) -> Self {
        Self {
            code: error.error_code().to_string(),
            message: error.user_message().to_string(),
            details: None,
        }
    }

    /// Create an error response with details.
    pub fn with_details<E: ModuleError>(error: &E, details: serde_json::Value) -> Self {
        Self {
            code: error.error_code().to_string(),
            message: error.user_message().to_string(),
            details: Some(details),
        }
    }
}

/// Common module errors that can be reused.
#[derive(Debug, thiserror::Error)]
pub enum CommonError {
    /// Entity not found.
    #[error("Entity not found: {entity_type} with id {id}")]
    NotFound {
        entity_type: &'static str,
        id: String,
    },

    /// Duplicate entity.
    #[error("Entity already exists: {entity_type}")]
    AlreadyExists { entity_type: &'static str },

    /// Validation failed.
    #[error("Validation failed: {message}")]
    ValidationFailed { message: String },

    /// Unauthorized access.
    #[error("Unauthorized")]
    Unauthorized,

    /// Forbidden access.
    #[error("Forbidden: {message}")]
    Forbidden { message: String },

    /// Internal error.
    #[error("Internal error: {message}")]
    Internal { message: String },

    /// Conflict (e.g., version mismatch).
    #[error("Conflict: {message}")]
    Conflict { message: String },
}

impl ModuleError for CommonError {
    fn error_code(&self) -> &str {
        match self {
            Self::NotFound { .. } => "NOT_FOUND",
            Self::AlreadyExists { .. } => "ALREADY_EXISTS",
            Self::ValidationFailed { .. } => "VALIDATION_FAILED",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden { .. } => "FORBIDDEN",
            Self::Internal { .. } => "INTERNAL_ERROR",
            Self::Conflict { .. } => "CONFLICT",
        }
    }

    fn status_code(&self) -> u16 {
        match self {
            Self::NotFound { .. } => 404,
            Self::AlreadyExists { .. } => 409,
            Self::ValidationFailed { .. } => 400,
            Self::Unauthorized => 401,
            Self::Forbidden { .. } => 403,
            Self::Internal { .. } => 500,
            Self::Conflict { .. } => 409,
        }
    }

    fn user_message(&self) -> &str {
        match self {
            Self::NotFound { .. } => "The requested resource was not found",
            Self::AlreadyExists { .. } => "A resource with this identifier already exists",
            Self::ValidationFailed { message } => message,
            Self::Unauthorized => "Authentication required",
            Self::Forbidden { message } => message,
            Self::Internal { .. } => "An internal error occurred",
            Self::Conflict { message } => message,
        }
    }

    fn is_retriable(&self) -> bool {
        matches!(self, Self::Internal { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    #[error("Test error")]
    struct TestError;

    impl ModuleError for TestError {
        fn error_code(&self) -> &str {
            "TEST_ERROR"
        }

        fn status_code(&self) -> u16 {
            400
        }

        fn user_message(&self) -> &str {
            "This is a test error"
        }
    }

    #[test]
    fn test_module_error_trait() {
        let error = TestError;
        assert_eq!(error.error_code(), "TEST_ERROR");
        assert_eq!(error.status_code(), 400);
        assert_eq!(error.user_message(), "This is a test error");
        assert!(!error.is_retriable());
        assert!(!error.is_loggable());
        assert_eq!(error.category(), ErrorCategory::ClientError);
    }

    #[test]
    fn test_error_response() {
        let error = TestError;
        let response = ErrorResponse::from_error(&error);
        assert_eq!(response.code, "TEST_ERROR");
        assert_eq!(response.message, "This is a test error");
    }

    #[test]
    fn test_common_error() {
        let error = CommonError::NotFound {
            entity_type: "User",
            id: "123".to_string(),
        };
        assert_eq!(error.error_code(), "NOT_FOUND");
        assert_eq!(error.status_code(), 404);
    }
}
