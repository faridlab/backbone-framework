//! Backbone Storage Module
//!
//! Provides file storage capabilities with multiple backend support:
//! - Local filesystem storage
//! - AWS S3 / S3-compatible (MinIO) storage (feature: `s3`)
//! - File compression and optimization (feature: `compression`)
//! - Security and virus scanning (feature: `security`)

pub mod error;
pub mod types;
pub mod traits;

// Use simple local storage for stability
#[path = "local_simple.rs"]
pub mod local;

// S3 / MinIO (feature-gated)
#[cfg(feature = "s3")]
pub mod s3;
#[cfg(feature = "s3")]
pub mod minio;

// Conditionally compile advanced features
#[cfg(feature = "compression")]
pub mod compression;

#[cfg(feature = "security")]
pub mod security;

// Re-export main types for convenient use
pub use error::{StorageError, StorageResult};
pub use types::{
    StorageFile, StorageConfig, StorageBackend, UploadOptions,
    DownloadOptions, PresignedUrlOptions, FileMetadata, StorageStats,
    HttpMethod, ByteRange, LocalStorageConfig,
    S3StorageConfig, MinIOStorageConfig, GcsStorageConfig,
};
pub use traits::{StorageService, StorageManager, StorageMonitor, FileListResult};

#[cfg(feature = "compression")]
pub use compression::{
    ImageCompressor, CompressionResult, CompressionQuality,
    FileCategory, CompressionAlgorithm, CompressionConfig,
    TextCompressionConfig, DocumentCompressionConfig,
};

#[cfg(feature = "security")]
pub use security::{
    SecurityEngine, SecurityConfig, ThreatLevel, SecurityAnalysis,
    ExecutableMetadata, DigitalSignature,
    Threat, ThreatType, SizeAnalysis, ContentAnalysis,
};


// Default configurations
pub const DEFAULT_MULTIPART_THRESHOLD: u64 = 100 * 1024 * 1024; // 100MB
pub const DEFAULT_CHUNK_SIZE: usize = 8 * 1024 * 1024; // 8MB
pub const DEFAULT_PRESIGNED_EXPIRY: u64 = 3600; // 1 hour

/// Storage module version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Get default storage configuration
pub fn default_config() -> StorageConfig {
    StorageConfig::default()
}

/// Require an environment variable to be set and non-empty.
#[cfg(any(feature = "s3", test))]
fn require_env_non_empty(var_name: &str) -> StorageResult<String> {
    let val = std::env::var(var_name)
        .map_err(|_| StorageError::InvalidConfiguration(format!("{var_name} is required but not set")))?;
    if val.trim().is_empty() {
        return Err(StorageError::InvalidConfiguration(format!("{var_name} is required but empty")));
    }
    Ok(val)
}

/// Log a warning if a credential env var is empty (empty may be valid for IAM roles).
#[cfg(feature = "s3")]
fn warn_if_empty(var_name: &str, value: &str) {
    if value.is_empty() {
        tracing::warn!(var = var_name, "Credential is empty; falling back to SDK default chain");
    }
}

/// Create storage service from environment variables
pub async fn from_env() -> StorageResult<Box<dyn StorageService>> {
    let backend = std::env::var("STORAGE_BACKEND")
        .unwrap_or_else(|_| "local".to_string())
        .parse()
        .map_err(|_| StorageError::InvalidConfiguration("STORAGE_BACKEND".to_string()))?;

    let config = StorageConfig::from_env()?;

    match backend {
        StorageBackend::Local => {
            let local_config = config.into_local_config()
                .ok_or_else(|| StorageError::InvalidConfiguration("Local storage configuration missing".to_string()))?;
            Ok(Box::new(crate::local::LocalStorage::new(local_config)?))
        }
        #[cfg(feature = "s3")]
        StorageBackend::S3 => {
            let bucket = std::env::var("S3_BUCKET").unwrap_or_else(|_| config.bucket.clone());
            if bucket.is_empty() || bucket == "default" {
                return Err(StorageError::InvalidConfiguration(
                    "S3_BUCKET (or STORAGE_BUCKET) is required for S3 backend".to_string(),
                ));
            }
            let access_key_id = std::env::var("AWS_ACCESS_KEY_ID").unwrap_or_default();
            let secret_access_key = std::env::var("AWS_SECRET_ACCESS_KEY").unwrap_or_default();
            warn_if_empty("AWS_ACCESS_KEY_ID", &access_key_id);
            warn_if_empty("AWS_SECRET_ACCESS_KEY", &secret_access_key);

            let s3_config = S3StorageConfig {
                region: std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
                access_key_id,
                secret_access_key,
                bucket,
                endpoint: std::env::var("S3_ENDPOINT").ok(),
                base_path: config.base_path.clone(),
                path_style: std::env::var("S3_PATH_STYLE").map(|v| v == "true").unwrap_or(false),
                ..Default::default()
            };
            Ok(Box::new(crate::s3::S3Storage::new(s3_config).await?))
        }
        #[cfg(not(feature = "s3"))]
        StorageBackend::S3 => {
            Err(StorageError::UnsupportedOperation {
                operation: "storage creation".to_string(),
                backend: "s3 (enable the 's3' feature)".to_string(),
            })
        }
        #[cfg(feature = "s3")]
        StorageBackend::MinIO => {
            let endpoint = require_env_non_empty("MINIO_ENDPOINT")?;
            let access_key = std::env::var("MINIO_ACCESS_KEY")
                .or_else(|_| std::env::var("AWS_ACCESS_KEY_ID"))
                .unwrap_or_default();
            let secret_key = std::env::var("MINIO_SECRET_KEY")
                .or_else(|_| std::env::var("AWS_SECRET_ACCESS_KEY"))
                .unwrap_or_default();
            if access_key.is_empty() {
                return Err(StorageError::InvalidConfiguration(
                    "MINIO_ACCESS_KEY (or AWS_ACCESS_KEY_ID) is required for MinIO backend".to_string(),
                ));
            }
            if secret_key.is_empty() {
                return Err(StorageError::InvalidConfiguration(
                    "MINIO_SECRET_KEY (or AWS_SECRET_ACCESS_KEY) is required for MinIO backend".to_string(),
                ));
            }
            let bucket = std::env::var("MINIO_BUCKET").unwrap_or_else(|_| config.bucket.clone());
            if bucket.is_empty() || bucket == "default" {
                return Err(StorageError::InvalidConfiguration(
                    "MINIO_BUCKET (or STORAGE_BUCKET) is required for MinIO backend".to_string(),
                ));
            }

            let minio_config = MinIOStorageConfig {
                endpoint,
                access_key,
                secret_key,
                bucket,
                region: std::env::var("MINIO_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
                base_path: config.base_path.clone(),
                ..Default::default()
            };
            Ok(Box::new(crate::minio::MinIOStorage::new(minio_config).await?))
        }
        #[cfg(not(feature = "s3"))]
        StorageBackend::MinIO => {
            Err(StorageError::UnsupportedOperation {
                operation: "storage creation".to_string(),
                backend: "minio (enable the 's3' feature)".to_string(),
            })
        }
        StorageBackend::AzureBlob => {
            Err(StorageError::UnsupportedOperation {
                operation: "storage creation".to_string(),
                backend: "azure_blob".to_string(),
            })
        }
        StorageBackend::GCS => {
            Err(StorageError::UnsupportedOperation {
                operation: "storage creation".to_string(),
                backend: "gcs (not yet implemented)".to_string(),
            })
        }
    }
}

/// Utility function to detect file MIME type
pub fn detect_mime_type(file_path: &str) -> String {
    mime_guess::from_path(file_path)
        .first_or_octet_stream()
        .to_string()
}

/// Utility function to generate safe file names
pub fn sanitize_filename(filename: &str) -> String {
    use std::path::Component;

    let path = std::path::Path::new(filename);
    let mut result = String::new();

    for component in path.components() {
        match component {
            Component::Normal(s) => {
                if let Some(s) = s.to_str() {
                    // Replace potentially problematic characters
                    let sanitized = s
                        .chars()
                        .map(|c| match c {
                            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                            c if c.is_control() => '_',
                            c => c,
                        })
                        .collect::<String>();

                    if !result.is_empty() {
                        result.push('/');
                    }
                    result.push_str(&sanitized);
                }
            }
            Component::RootDir => continue,
            Component::CurDir => continue,
            Component::ParentDir => {
                if !result.is_empty() {
                    result.push('/');
                }
                result.push_str("_parent");
            }
            Component::Prefix(_) => continue,
        }
    }

    // Ensure the result is not empty
    if result.is_empty() {
        result.push_str("file");
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_type_detection() {
        assert_eq!(detect_mime_type("test.txt"), "text/plain");
        assert_eq!(detect_mime_type("test.json"), "application/json");
        assert_eq!(detect_mime_type("test.jpg"), "image/jpeg");
        assert_eq!(detect_mime_type("unknown.totally_unknown_ext"), "application/octet-stream");
    }

    #[test]
    fn test_filename_sanitization() {
        assert_eq!(sanitize_filename("normal_file.txt"), "normal_file.txt");
        assert_eq!(sanitize_filename("file/with\\slashes"), "file/with_slashes");
        assert_eq!(sanitize_filename("../parent/directory"), "_parent/parent/directory");
        assert_eq!(sanitize_filename("file:with*special?chars"), "file_with_special_chars");
        assert_eq!(sanitize_filename(""), "file");
    }

    #[test]
    fn test_default_config() {
        let config = default_config();
        assert_eq!(config.backend, StorageBackend::Local);
        assert_eq!(config.multipart_threshold, DEFAULT_MULTIPART_THRESHOLD);
    }

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_require_env_non_empty_missing() {
        // Use a unique env var name that definitely doesn't exist
        let result = require_env_non_empty("__BACKBONE_TEST_NONEXISTENT_VAR__");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("required but not set"), "got: {msg}");
    }

    #[test]
    fn test_require_env_non_empty_blank() {
        let key = "__BACKBONE_TEST_EMPTY_VAR__";
        std::env::set_var(key, "   ");
        let result = require_env_non_empty(key);
        std::env::remove_var(key);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("required but empty"), "got: {msg}");
    }

    #[test]
    fn test_require_env_non_empty_ok() {
        let key = "__BACKBONE_TEST_OK_VAR__";
        std::env::set_var(key, "value");
        let result = require_env_non_empty(key);
        std::env::remove_var(key);
        assert_eq!(result.unwrap(), "value");
    }
}