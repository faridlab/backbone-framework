//! Storage module error types

use thiserror::Error;

/// Storage module result type
pub type StorageResult<T> = Result<T, StorageError>;

/// Storage error types
#[derive(Error, Debug)]
pub enum StorageError {
    /// Configuration error
    #[error("Configuration error: {0}")]
    InvalidConfiguration(String),

    /// Authentication error
    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    /// Permission error
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// File already exists
    #[error("File already exists: {0}")]
    FileAlreadyExists(String),

    /// Network connection error
    #[error("Network error: {0}")]
    NetworkError(String),

    
    /// Local filesystem error
    #[error("Filesystem error: {0}")]
    FilesystemError(#[from] std::io::Error),

    /// URL parsing error
    #[error("Invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Base64 encoding/decoding error
    #[error("Base64 error: {0}")]
    Base64Error(#[from] base64::DecodeError),

    /// Invalid file format
    #[error("Invalid file format: {0}")]
    InvalidFileFormat(String),

    /// File too large
    #[error("File too large: {size} bytes, maximum allowed: {max_size} bytes")]
    FileTooLarge {
        size: u64,
        max_size: u64,
    },

    /// Checksum mismatch
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch {
        expected: String,
        actual: String,
    },

    /// Upload interrupted
    #[error("Upload interrupted: {0}")]
    UploadInterrupted(String),

    /// Download interrupted
    #[error("Download interrupted: {0}")]
    DownloadInterrupted(String),

    /// Multipart upload error
    #[error("Multipart upload error: {0}")]
    MultipartUploadError(String),

    /// Presigned URL generation error
    #[error("Presigned URL generation failed: {0}")]
    PresignedUrlError(String),

    /// Temporary storage error
    #[error("Temporary storage error: {0}")]
    TemporaryStorageError(String),

    /// Storage quota exceeded
    #[error("Storage quota exceeded: {used}/{limit} bytes")]
    QuotaExceeded {
        used: u64,
        limit: u64,
    },

    /// Rate limited
    #[error("Rate limited: try again in {retry_after} seconds")]
    RateLimited {
        retry_after: u64,
    },

    /// Invalid range request
    #[error("Invalid range request: {0}")]
    InvalidRange(String),

    /// Timeout error
    #[error("Operation timed out after {timeout} seconds")]
    Timeout {
        timeout: u64,
    },

    /// Generic operation error
    #[error("Operation failed: {operation} - {message}")]
    OperationFailed {
        operation: String,
        message: String,
    },

    /// Unsupported operation for backend
    #[error("Unsupported operation '{operation}' for backend '{backend}'")]
    UnsupportedOperation {
        operation: String,
        backend: String,
    },

    /// S3 operation error
    #[error("S3 operation failed: {message}")]
    S3OperationFailed {
        message: String,
        request_id: Option<String>,
    },

    /// GCS operation error
    #[error("GCS error: {0}")]
    GcsError(String),

    /// MinIO specific errors
    #[error("MinIO error: {0}")]
    MinIOError(String),

    /// HTTP client error
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    /// Security-related error
    #[error("Security error: {operation} - {message}")]
    SecurityError {
        operation: String,
        message: String,
    },

    /// Generic error
    #[error("{0}")]
    Other(String),
}

impl StorageError {
    /// Create a configuration error
    pub fn config_error(message: impl Into<String>) -> Self {
        Self::InvalidConfiguration(message.into())
    }

    /// Create an authentication error
    pub fn auth_error(message: impl Into<String>) -> Self {
        Self::AuthenticationError(message.into())
    }

    /// Create a file not found error
    pub fn not_found(file_path: impl Into<String>) -> Self {
        Self::FileNotFound(file_path.into())
    }

    /// Create an operation failed error
    pub fn operation_failed(operation: impl Into<String>, message: impl Into<String>) -> Self {
        Self::OperationFailed {
            operation: operation.into(),
            message: message.into(),
        }
    }

    
    /// Check if this is a retryable error
    pub fn is_retryable(&self) -> bool {
        matches!(self,
            Self::NetworkError(_)
            | Self::HttpError(_)
            | Self::Timeout { .. }
            | Self::RateLimited { .. }
        )
    }

    /// Check if this is a client error (4xx)
    pub fn is_client_error(&self) -> bool {
        match self {
            Self::AuthenticationError(_)
            | Self::PermissionDenied(_)
            | Self::FileNotFound(_)
            | Self::FileAlreadyExists(_)
            | Self::InvalidUrl(_)
            | Self::InvalidFileFormat(_)
            | Self::FileTooLarge { .. }
            | Self::ChecksumMismatch { .. }
            | Self::InvalidRange(_)
            | Self::UnsupportedOperation { .. } => true,
            Self::HttpError(e) => e.status().is_some_and(|s| s.is_client_error()),
            _ => false,
        }
    }

    /// Check if this is a server error (5xx)
    pub fn is_server_error(&self) -> bool {
        match self {
            Self::NetworkError(_)
            | Self::TemporaryStorageError(_) => true,
            Self::HttpError(e) => e.status().is_some_and(|s| s.is_server_error()),
            _ => false,
        }
    }

    /// Get error category for logging/metrics
    pub fn category(&self) -> &'static str {
        match self {
            Self::InvalidConfiguration(_) => "configuration",
            Self::AuthenticationError(_) | Self::PermissionDenied(_) => "authentication",
            Self::FileNotFound(_) | Self::FileAlreadyExists(_) => "file_system",
            Self::NetworkError(_) | Self::HttpError(_) => "network",
            Self::FilesystemError(_) => "filesystem",
            Self::SerializationError(_) | Self::Base64Error(_) => "serialization",
            Self::FileTooLarge { .. } | Self::QuotaExceeded { .. } => "quota",
            Self::RateLimited { .. } => "rate_limit",
            Self::Timeout { .. } => "timeout",
            _ => "other",
        }
    }
}

/// Convert HTTP status codes to storage errors
impl From<reqwest::Response> for StorageError {
    fn from(response: reqwest::Response) -> Self {
        let status = response.status();
        let url = response.url().clone();

        match status.as_u16() {
            400 => Self::InvalidConfiguration(format!("Bad request to {}", url)),
            401 => Self::AuthenticationError("Unauthorized".to_string()),
            403 => Self::PermissionDenied(format!("Access denied to {}", url)),
            404 => Self::FileNotFound(format!("Resource not found at {}", url)),
            409 => Self::FileAlreadyExists(format!("Resource already exists at {}", url)),
            413 => Self::FileTooLarge { size: 0, max_size: 0 },
            416 => Self::InvalidRange("Invalid range request".to_string()),
            429 => Self::RateLimited { retry_after: 60 },
            500..=599 => Self::NetworkError(format!("Server error {} from {}", status, url)),
            _ => Self::Other(format!("HTTP {} from {}", status, url)),
        }
    }
}