//! Storage module types and structures

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Storage backend types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StorageBackend {
    /// Local filesystem storage
    #[default]
    Local,
    /// Amazon S3
    S3,
    /// MinIO (S3-compatible)
    MinIO,
    /// Azure Blob Storage (future)
    AzureBlob,
    /// Google Cloud Storage
    GCS,
}

impl std::fmt::Display for StorageBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => write!(f, "local"),
            Self::S3 => write!(f, "s3"),
            Self::MinIO => write!(f, "minio"),
            Self::AzureBlob => write!(f, "azure_blob"),
            Self::GCS => write!(f, "gcs"),
        }
    }
}

impl std::str::FromStr for StorageBackend {
    type Err = crate::error::StorageError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "s3" | "aws" => Ok(Self::S3),
            "minio" => Ok(Self::MinIO),
            "azure_blob" | "azure" => Ok(Self::AzureBlob),
            "gcs" | "google" => Ok(Self::GCS),
            _ => Err(crate::error::StorageError::InvalidConfiguration(
                format!("Invalid storage backend: {}", s)
            )),
        }
    }
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage backend type
    pub backend: StorageBackend,

    /// Default bucket/container name
    pub bucket: String,

    /// Base path/prefix for all files
    pub base_path: Option<String>,

    /// Multipart upload threshold in bytes
    pub multipart_threshold: u64,

    /// Chunk size for multipart uploads in bytes
    pub chunk_size: usize,

    /// Enable file versioning
    pub versioning: bool,

    /// Enable compression
    pub compression: bool,

    /// Encryption settings
    pub encryption: Option<EncryptionConfig>,

    /// Comprehensive compression settings (requires 'compression' feature)
    #[cfg(feature = "compression")]
    pub compression_config: Option<crate::CompressionConfig>,

    /// Retention policy
    pub retention: Option<RetentionConfig>,

    /// Access control settings
    pub access_control: Option<AccessControlConfig>,

    /// Backend-specific configurations
    pub backend_config: HashMap<String, serde_json::Value>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::Local,
            bucket: "default".to_string(),
            base_path: None,
            multipart_threshold: crate::DEFAULT_MULTIPART_THRESHOLD,
            chunk_size: crate::DEFAULT_CHUNK_SIZE,
            versioning: false,
            compression: false,
            encryption: None,
            #[cfg(feature = "compression")]
            compression_config: None,
            retention: None,
            access_control: None,
            backend_config: HashMap::new(),
        }
    }
}

impl StorageConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> crate::StorageResult<Self> {
        let backend_str = std::env::var("STORAGE_BACKEND")
            .unwrap_or_else(|_| "local".to_string());
        let backend = backend_str.parse()?;

        let mut config = Self {
            backend,
            bucket: std::env::var("STORAGE_BUCKET")
                .unwrap_or_else(|_| "default".to_string()),
            base_path: std::env::var("STORAGE_BASE_PATH").ok(),
            ..Default::default()
        };

        // Load backend-specific configurations
        if config.backend == StorageBackend::Local {
            config.backend_config.insert("base_dir".to_string(),
                serde_json::Value::String(
                    std::env::var("LOCAL_STORAGE_DIR").unwrap_or_else(|_| "./storage".to_string())
                )
            );
        }

        Ok(config)
    }

    
    /// Convert to local storage configuration
    pub fn into_local_config(self) -> Option<LocalStorageConfig> {
        if self.backend != StorageBackend::Local {
            return None;
        }

        Some(LocalStorageConfig {
            base_dir: self.backend_config.get("base_dir")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "./storage".to_string()),
            base_path: self.base_path,
            compression: self.compression,
            encryption: self.encryption,
        })
    }
}


/// Local storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalStorageConfig {
    pub base_dir: String,
    pub base_path: Option<String>,
    pub compression: bool,
    pub encryption: Option<EncryptionConfig>,
}

/// AWS S3 storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3StorageConfig {
    /// AWS region (e.g., "us-east-1")
    pub region: String,
    /// AWS access key ID
    pub access_key_id: String,
    /// AWS secret access key
    pub secret_access_key: String,
    /// S3 bucket name
    pub bucket: String,
    /// Custom endpoint URL (for S3-compatible services)
    pub endpoint: Option<String>,
    /// Base path prefix for all objects
    pub base_path: Option<String>,
    /// Multipart upload threshold in bytes
    pub multipart_threshold: u64,
    /// Chunk size for multipart uploads
    pub chunk_size: usize,
    /// Encryption settings
    pub encryption: Option<EncryptionConfig>,
    /// Use path-style URLs (required for some S3-compatible services)
    pub path_style: bool,
    /// Enable versioning
    pub versioning: bool,
}

impl Default for S3StorageConfig {
    fn default() -> Self {
        Self {
            region: "us-east-1".to_string(),
            access_key_id: String::new(),
            secret_access_key: String::new(),
            bucket: String::new(),
            endpoint: None,
            base_path: None,
            multipart_threshold: crate::DEFAULT_MULTIPART_THRESHOLD,
            chunk_size: crate::DEFAULT_CHUNK_SIZE,
            encryption: None,
            path_style: false,
            versioning: false,
        }
    }
}

/// MinIO storage configuration (S3-compatible)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinIOStorageConfig {
    /// MinIO endpoint URL (e.g., "http://localhost:9000")
    pub endpoint: String,
    /// MinIO access key
    pub access_key: String,
    /// MinIO secret key
    pub secret_key: String,
    /// Bucket name
    pub bucket: String,
    /// Region (usually "us-east-1" for MinIO)
    pub region: String,
    /// Base path prefix for all objects
    pub base_path: Option<String>,
    /// Use SSL/TLS
    pub use_ssl: bool,
    /// Multipart upload threshold in bytes
    pub multipart_threshold: u64,
    /// Chunk size for multipart uploads
    pub chunk_size: usize,
    /// Encryption settings
    pub encryption: Option<EncryptionConfig>,
    /// Enable versioning
    pub versioning: bool,
}

impl Default for MinIOStorageConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:9000".to_string(),
            access_key: String::new(),
            secret_key: String::new(),
            bucket: String::new(),
            region: "us-east-1".to_string(),
            base_path: None,
            use_ssl: false,
            multipart_threshold: crate::DEFAULT_MULTIPART_THRESHOLD,
            chunk_size: crate::DEFAULT_CHUNK_SIZE,
            encryption: None,
            versioning: false,
        }
    }
}

/// Google Cloud Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcsStorageConfig {
    /// GCP project ID
    pub project_id: String,
    /// GCS bucket name
    pub bucket: String,
    /// Path to service account credentials JSON file
    pub credentials_json: Option<String>,
    /// Base path prefix for all objects
    pub base_path: Option<String>,
}

impl Default for GcsStorageConfig {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            bucket: String::new(),
            credentials_json: None,
            base_path: None,
        }
    }
}

/// Encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    /// Encryption algorithm
    pub algorithm: EncryptionAlgorithm,

    /// Encryption key (base64 encoded)
    pub key: Option<String>,

    /// Use server-side encryption for cloud storage
    pub server_side: bool,

    /// Additional encryption parameters
    pub parameters: HashMap<String, String>,
}

/// Image compression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageCompressionConfig {
    /// Enable automatic image compression
    pub enabled: bool,

    /// Compression quality level
    pub quality: u8, // 0-100

    /// Maximum width in pixels (resizes larger images)
    pub max_width: Option<u32>,

    /// Maximum height in pixels (resizes larger images)
    pub max_height: Option<u32>,

    /// Maximum file size in bytes (recompresses if larger)
    pub max_file_size: Option<u64>,

    /// Target formats for conversion (e.g., "webp", "jpeg", "png")
    pub preferred_formats: Vec<String>,

    /// Preserve metadata (EXIF, etc.)
    pub preserve_metadata: bool,

    /// Progressive JPEG encoding
    pub progressive_jpeg: bool,

    /// WebP compression method (0-6, higher = slower but better compression)
    pub webp_method: u8,
}

impl Default for ImageCompressionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            quality: 80, // Medium quality
            max_width: Some(2048),
            max_height: Some(2048),
            max_file_size: Some(2 * 1024 * 1024), // 2MB
            preferred_formats: vec!["webp".to_string(), "jpeg".to_string()],
            preserve_metadata: false,
            progressive_jpeg: true,
            webp_method: 4,
        }
    }
}

/// Encryption algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptionAlgorithm {
    /// AES-256-GCM
    AES256GCM,
    /// AES-256-CBC
    AES256CBC,
    /// ChaCha20-Poly1305
    ChaCha20Poly1305,
}

/// Retention policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    /// Retention mode
    pub mode: RetentionMode,

    /// Retention period in days
    pub period_days: u32,

    /// Apply to all files or specific patterns
    pub patterns: Vec<String>,
}

/// Retention modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetentionMode {
    /// Keep files for N days
    KeepForDays,
    /// Delete files older than N days
    DeleteAfterDays,
    /// Move to glacier after N days
    GlacierAfterDays,
}

/// Access control configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlConfig {
    /// Default access policy
    pub default_policy: AccessPolicy,

    /// CORS configuration
    pub cors: Option<CorsConfig>,
}

/// Access policies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessPolicy {
    /// Private access
    Private,
    /// Public read
    PublicRead,
    /// Public read/write
    PublicReadWrite,
    /// Custom policy
    Custom,
}

/// CORS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    /// Allowed origins
    pub allowed_origins: Vec<String>,

    /// Allowed methods
    pub allowed_methods: Vec<String>,

    /// Allowed headers
    pub allowed_headers: Vec<String>,

    /// Max age in seconds
    pub max_age: Option<u32>,
}

/// Storage file representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageFile {
    /// Unique file identifier
    pub id: String,

    /// File name/path
    pub name: String,

    /// File size in bytes
    pub size: u64,

    /// MIME type
    pub content_type: String,

    /// File hash/checksum
    pub checksum: Option<String>,

    /// Checksum algorithm
    pub checksum_algorithm: Option<String>,

    /// File metadata
    pub metadata: HashMap<String, String>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last modification timestamp
    pub updated_at: Option<DateTime<Utc>>,

    /// File version (if versioning enabled)
    pub version: Option<String>,

    /// Storage backend
    pub backend: StorageBackend,

    /// Bucket/container name
    pub bucket: String,

    /// File path in storage
    pub storage_path: String,

    /// Whether file is encrypted
    pub encrypted: bool,

    /// Whether file is compressed
    pub compressed: bool,

    /// Expiration time (if set)
    pub expires_at: Option<DateTime<Utc>>,
}

impl StorageFile {
    /// Create new storage file
    pub fn new(
        name: String,
        size: u64,
        content_type: String,
        bucket: String,
        storage_path: String,
        backend: StorageBackend,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            size,
            content_type,
            checksum: None,
            checksum_algorithm: None,
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: None,
            version: None,
            backend,
            bucket,
            storage_path,
            encrypted: false,
            compressed: false,
            expires_at: None,
        }
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Set checksum
    pub fn with_checksum(mut self, checksum: String, algorithm: String) -> Self {
        self.checksum = Some(checksum);
        self.checksum_algorithm = Some(algorithm);
        self
    }

    /// Set version
    pub fn with_version(mut self, version: String) -> Self {
        self.version = Some(version);
        self
    }

    /// Set encryption
    pub fn with_encryption(mut self, encrypted: bool) -> Self {
        self.encrypted = encrypted;
        self
    }

    /// Set compression
    pub fn with_compression(mut self, compressed: bool) -> Self {
        self.compressed = compressed;
        self
    }

    /// Set expiration
    pub fn with_expiration(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Set created at timestamp
    pub fn with_created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = created_at;
        self
    }

    /// Set updated at timestamp
    pub fn with_updated_at(mut self, updated_at: DateTime<Utc>) -> Self {
        self.updated_at = Some(updated_at);
        self
    }

    /// Check if file is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    /// Get file extension
    pub fn extension(&self) -> Option<&str> {
        std::path::Path::new(&self.name)
            .extension()
            .and_then(|ext| ext.to_str())
    }
}

/// Upload options
#[derive(Debug, Clone, Default)]
pub struct UploadOptions {
    /// Content type override
    pub content_type: Option<String>,

    /// File metadata
    pub metadata: HashMap<String, String>,

    /// Enable multipart upload for large files
    pub multipart: Option<bool>,

    /// Chunk size for multipart upload
    pub chunk_size: Option<usize>,

    /// Calculate checksum during upload
    pub calculate_checksum: bool,

    /// Checksum algorithm
    pub checksum_algorithm: Option<String>,

    /// Upload with encryption
    pub encrypt: bool,

    /// Upload with compression
    pub compress: bool,

    // Progress callback (removed for simplicity)
    // pub progress_callback: Option<Box<dyn Fn(u64, u64) + Send + Sync>>,

    /// Retry configuration
    pub retry_config: Option<RetryConfig>,
}

/// Download options
#[derive(Debug, Clone, Default)]
pub struct DownloadOptions {
    /// Byte range for download
    pub range: Option<ByteRange>,

    /// Verify checksum after download
    pub verify_checksum: bool,

    /// Expected checksum
    pub expected_checksum: Option<String>,

    // Progress callback (removed for simplicity)
    // pub progress_callback: Option<Box<dyn Fn(u64, u64) + Send + Sync>>,

    /// Retry configuration
    pub retry_config: Option<RetryConfig>,
}

/// Byte range for partial downloads
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    /// Start byte position
    pub start: u64,

    /// End byte position (inclusive, or None for end of file)
    pub end: Option<u64>,
}

impl ByteRange {
    /// Create new byte range
    pub fn new(start: u64, end: Option<u64>) -> Self {
        Self { start, end }
    }

    /// Create range from start byte to end of file
    pub fn from_start(start: u64) -> Self {
        Self { start, end: None }
    }

    /// Create range for last N bytes
    pub fn last_bytes(bytes: u64) -> Self {
        Self {
            start: 0,
            end: Some(bytes - 1)
        }
    }

    /// Convert to HTTP range header value
    pub fn to_header_value(&self) -> String {
        match self.end {
            Some(end) => format!("bytes={}-{}", self.start, end),
            None => format!("bytes={}-", self.start),
        }
    }
}

/// Presigned URL options
#[derive(Debug, Clone)]
pub struct PresignedUrlOptions {
    /// Expiration time in seconds
    pub expiry_seconds: u64,

    /// HTTP method (GET, PUT, DELETE)
    pub method: HttpMethod,

    /// Custom headers to include
    pub headers: HashMap<String, String>,

    /// Query parameters to include
    pub query_params: HashMap<String, String>,
}

impl Default for PresignedUrlOptions {
    fn default() -> Self {
        Self {
            expiry_seconds: crate::DEFAULT_PRESIGNED_EXPIRY,
            method: HttpMethod::GET,
            headers: HashMap::new(),
            query_params: HashMap::new(),
        }
    }
}

/// HTTP methods for presigned URLs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    GET,
    PUT,
    DELETE,
    HEAD,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GET => write!(f, "GET"),
            Self::PUT => write!(f, "PUT"),
            Self::DELETE => write!(f, "DELETE"),
            Self::HEAD => write!(f, "HEAD"),
        }
    }
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,

    /// Initial delay in milliseconds
    pub initial_delay_ms: u64,

    /// Backoff multiplier
    pub backoff_multiplier: f64,

    /// Maximum delay in milliseconds
    pub max_delay_ms: u64,

    /// Retry only on these errors
    pub retry_on_error: Option<Vec<String>>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30000,
            retry_on_error: None,
        }
    }
}

/// File metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// File size in bytes
    pub size: u64,

    /// Last modification timestamp
    pub last_modified: DateTime<Utc>,

    /// Content type
    pub content_type: String,

    /// ETag
    pub etag: Option<String>,

    /// Storage class
    pub storage_class: Option<String>,

    /// Additional metadata
    pub custom_metadata: HashMap<String, String>,
}

/// Storage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    /// Total number of files
    pub file_count: u64,

    /// Total storage used in bytes
    pub total_bytes: u64,

    /// Storage used by each backend
    pub backend_usage: HashMap<String, u64>,

    /// Number of files by content type
    pub content_type_counts: HashMap<String, u64>,

    /// Average file size
    pub average_file_size: f64,

    /// Largest file size
    pub largest_file_size: u64,

    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
}