//! Storage service traits

use async_trait::async_trait;
use crate::{
    StorageResult, StorageFile, StorageConfig, StorageStats,
    UploadOptions, DownloadOptions, PresignedUrlOptions, ByteRange,
    types::EncryptionConfig,
};
use std::collections::HashMap;
use bytes::Bytes;

/// Generic storage service trait
#[async_trait]
pub trait StorageService: Send + Sync {
    /// Upload a file from bytes
    async fn upload_bytes(
        &self,
        path: &str,
        data: Bytes,
        options: Option<UploadOptions>,
    ) -> StorageResult<StorageFile>;

    /// Upload a file from a reader
    async fn upload_reader(
        &self,
        path: &str,
        reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
        size: Option<u64>,
        options: Option<UploadOptions>,
    ) -> StorageResult<StorageFile>;

    /// Upload a file from local path
    async fn upload_file(
        &self,
        path: &str,
        local_path: &str,
        options: Option<UploadOptions>,
    ) -> StorageResult<StorageFile>;

    /// Download a file to bytes
    async fn download_bytes(
        &self,
        path: &str,
        options: Option<DownloadOptions>,
    ) -> StorageResult<Bytes>;

    /// Download a file to a writer
    async fn download_writer(
        &self,
        path: &str,
        writer: Box<dyn tokio::io::AsyncWrite + Send + Unpin>,
        options: Option<DownloadOptions>,
    ) -> StorageResult<()>;

    /// Download a file to local path
    async fn download_file(
        &self,
        path: &str,
        local_path: &str,
        options: Option<DownloadOptions>,
    ) -> StorageResult<()>;

    /// Stream a file (useful for large files)
    async fn stream_file(
        &self,
        path: &str,
        range: Option<ByteRange>,
    ) -> StorageResult<Box<dyn futures::stream::Stream<Item = StorageResult<Bytes>> + Send + Unpin>>;

    /// Get file metadata
    async fn get_file(&self, path: &str) -> StorageResult<StorageFile>;

    /// Check if file exists
    async fn file_exists(&self, path: &str) -> StorageResult<bool>;

    /// Delete a file
    async fn delete_file(&self, path: &str) -> StorageResult<bool>;

    /// Copy a file
    async fn copy_file(&self, from_path: &str, to_path: &str) -> StorageResult<StorageFile>;

    /// Move a file
    async fn move_file(&self, from_path: &str, to_path: &str) -> StorageResult<StorageFile>;

    /// List files in a directory/prefix
    async fn list_files(
        &self,
        prefix: &str,
        limit: Option<u32>,
        continuation_token: Option<String>,
    ) -> StorageResult<FileListResult>;

    /// Generate presigned URL for file operations
    async fn generate_presigned_url(
        &self,
        path: &str,
        options: PresignedUrlOptions,
    ) -> StorageResult<String>;

    /// Get file URL (public access or presigned)
    async fn get_file_url(
        &self,
        path: &str,
        expiry_seconds: Option<u64>,
    ) -> StorageResult<String>;

    /// Update file metadata
    async fn update_metadata(
        &self,
        path: &str,
        metadata: HashMap<String, String>,
    ) -> StorageResult<StorageFile>;

    /// Get storage statistics
    async fn get_stats(&self) -> StorageResult<StorageStats>;

    /// Test storage connection
    async fn test_connection(&self) -> StorageResult<bool>;

    /// Get storage backend type
    fn backend_type(&self) -> crate::StorageBackend;

    /// Get bucket/container name
    fn bucket(&self) -> &str;

    /// Get configuration
    fn config(&self) -> &StorageConfig;
}

/// Storage management trait for administrative operations
#[async_trait]
pub trait StorageManager: Send + Sync {
    /// Create a new bucket/container
    async fn create_bucket(&self, bucket_name: &str, config: Option<BucketConfig>) -> StorageResult<bool>;

    /// Delete a bucket/container
    async fn delete_bucket(&self, bucket_name: &str, force: bool) -> StorageResult<bool>;

    /// List all buckets/containers
    async fn list_buckets(&self) -> StorageResult<Vec<String>>;

    /// Get bucket/container information
    async fn get_bucket_info(&self, bucket_name: &str) -> StorageResult<BucketInfo>;

    /// Set bucket lifecycle rules
    async fn set_lifecycle_rules(
        &self,
        bucket_name: &str,
        rules: Vec<LifecycleRule>,
    ) -> StorageResult<bool>;

    /// Enable bucket versioning
    async fn enable_versioning(&self, bucket_name: &str) -> StorageResult<bool>;

    /// Disable bucket versioning
    async fn disable_versioning(&self, bucket_name: &str) -> StorageResult<bool>;

    /// Set bucket access control
    async fn set_access_control(
        &self,
        bucket_name: &str,
        policy: AccessPolicy,
    ) -> StorageResult<bool>;

    /// Get bucket access control
    async fn get_access_control(&self, bucket_name: &str) -> StorageResult<AccessPolicy>;

    /// Restore from backup
    async fn restore_backup(
        &self,
        backup_path: &str,
        target_bucket: &str,
        target_prefix: Option<String>,
    ) -> StorageResult<RestoreResult>;

    /// Create backup
    async fn create_backup(
        &self,
        bucket_name: &str,
        backup_path: &str,
        prefix_filter: Option<String>,
    ) -> StorageResult<BackupResult>;
}

/// Storage monitoring trait
#[async_trait]
pub trait StorageMonitor: Send + Sync {
    /// Start monitoring storage operations
    async fn start_monitoring(&self, bucket_name: &str) -> StorageResult<()>;

    /// Stop monitoring storage operations
    async fn stop_monitoring(&self, bucket_name: &str) -> StorageResult<()>;

    /// Get real-time metrics
    async fn get_metrics(&self, bucket_name: &str) -> StorageResult<HashMap<String, f64>>;

    /// Set alert thresholds
    async fn set_alert_thresholds(
        &self,
        bucket_name: &str,
        thresholds: AlertThresholds,
    ) -> StorageResult<()>;

    /// Get current alerts
    async fn get_alerts(&self, bucket_name: &str) -> StorageResult<Vec<StorageAlert>>;

    /// Get usage trends
    async fn get_usage_trends(
        &self,
        bucket_name: &str,
        period: TrendPeriod,
    ) -> StorageResult<UsageTrend>;

    /// Generate storage report
    async fn generate_report(
        &self,
        bucket_name: &str,
        report_type: ReportType,
    ) -> StorageResult<StorageReport>;
}

/// Result type for file listing operations
#[derive(Debug, Clone)]
pub struct FileListResult {
    /// List of files
    pub files: Vec<StorageFile>,

    /// Common prefixes (directories)
    pub prefixes: Vec<String>,

    /// Whether there are more results
    pub truncated: bool,

    /// Token for next page of results
    pub next_continuation_token: Option<String>,

    /// Total number of files (if available)
    pub total_count: Option<u64>,
}

/// Bucket configuration
#[derive(Debug, Clone)]
pub struct BucketConfig {
    /// Geographic region
    pub region: Option<String>,

    /// Access policy
    pub access_policy: Option<AccessPolicy>,

    /// Storage class
    pub storage_class: Option<String>,

    /// Enable versioning
    pub versioning: Option<bool>,

    /// Lifecycle rules
    pub lifecycle_rules: Option<Vec<LifecycleRule>>,

    /// Tags
    pub tags: Option<HashMap<String, String>>,

    /// Encryption settings
    pub encryption: Option<EncryptionConfig>,
}

/// Bucket information
#[derive(Debug, Clone)]
pub struct BucketInfo {
    /// Bucket name
    pub name: String,

    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Geographic region
    pub region: String,

    /// Storage class
    pub storage_class: String,

    /// Versioning status
    pub versioning: VersioningStatus,

    /// Access policy
    pub access_policy: AccessPolicy,

    /// Total size in bytes
    pub size_bytes: u64,

    /// Number of objects
    pub object_count: u64,

    /// Tags
    pub tags: HashMap<String, String>,
}

/// Versioning status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersioningStatus {
    Enabled,
    Suspended,
    Disabled,
}

/// Lifecycle rule
#[derive(Debug, Clone)]
pub struct LifecycleRule {
    /// Rule ID
    pub id: String,

    /// Rule status
    pub status: LifecycleRuleStatus,

    /// Filter for objects
    pub filter: LifecycleFilter,

    /// Action to perform
    pub action: LifecycleAction,
}

/// Lifecycle rule status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleRuleStatus {
    Enabled,
    Disabled,
}

/// Lifecycle filter
#[derive(Debug, Clone)]
pub struct LifecycleFilter {
    /// Object prefix
    pub prefix: Option<String>,

    /// Object tags
    pub tags: Option<HashMap<String, String>>,

    /// Object size range
    pub size_range: Option<(u64, u64)>,
}

/// Lifecycle action
#[derive(Debug, Clone)]
pub struct LifecycleAction {
    /// Action type
    pub action_type: LifecycleActionType,

    /// Number of days for transition/expiration
    pub days: Option<u32>,

    /// Storage class for transition
    pub storage_class: Option<String>,
}

/// Lifecycle action types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleActionType {
    /// Delete object
    Delete,

    /// Transition to different storage class
    Transition,

    /// Expire object
    Expire,

    /// Archive to glacier
    Archive,
}

/// Access policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessPolicy {
    Private,
    PublicRead,
    PublicReadWrite,
    Custom,
}

/// Restore operation result
#[derive(Debug, Clone)]
pub struct RestoreResult {
    /// Number of files restored
    pub files_restored: u64,

    /// Total bytes restored
    pub bytes_restored: u64,

    /// Number of files failed
    pub files_failed: u64,

    /// Errors encountered
    pub errors: Vec<String>,

    /// Duration in seconds
    pub duration_seconds: u64,
}

/// Backup operation result
#[derive(Debug, Clone)]
pub struct BackupResult {
    /// Number of files backed up
    pub files_backed_up: u64,

    /// Total bytes backed up
    pub bytes_backed_up: u64,

    /// Backup file path
    pub backup_path: String,

    /// Checksum of backup
    pub backup_checksum: String,

    /// Number of files skipped
    pub files_skipped: u64,

    /// Errors encountered
    pub errors: Vec<String>,

    /// Duration in seconds
    pub duration_seconds: u64,
}

/// Alert thresholds
#[derive(Debug, Clone)]
pub struct AlertThresholds {
    /// Storage usage percentage (0-100)
    pub storage_usage_percent: Option<f64>,

    /// File count limit
    pub file_count_limit: Option<u64>,

    /// Large file size threshold in bytes
    pub large_file_threshold: Option<u64>,

    /// Error rate percentage (0-100)
    pub error_rate_percent: Option<f64>,

    /// Average response time in milliseconds
    pub avg_response_time_ms: Option<f64>,
}

/// Storage alert
#[derive(Debug, Clone)]
pub struct StorageAlert {
    /// Alert ID
    pub id: String,

    /// Alert type
    pub alert_type: StorageAlertType,

    /// Alert severity
    pub severity: AlertSeverity,

    /// Alert message
    pub message: String,

    /// Alert timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Associated bucket
    pub bucket: String,

    /// Additional details
    pub details: HashMap<String, String>,
}

/// Storage alert types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageAlertType {
    StorageUsage,
    FileCount,
    LargeFile,
    ErrorRate,
    ResponseTime,
    ConnectionFailure,
    PermissionDenied,
}

/// Alert severity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Trend period for analytics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendPeriod {
    LastHour,
    Last24Hours,
    Last7Days,
    Last30Days,
    Custom { hours: u32 },
}

/// Usage trend data
#[derive(Debug, Clone)]
pub struct UsageTrend {
    /// Trend period
    pub period: TrendPeriod,

    /// Data points
    pub data_points: Vec<TrendDataPoint>,

    /// Growth rate (percentage)
    pub growth_rate: f64,

    /// Predicted usage for next period
    pub predicted_usage: Option<u64>,
}

/// Trend data point
#[derive(Debug, Clone)]
pub struct TrendDataPoint {
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Storage usage in bytes
    pub usage_bytes: u64,

    /// File count
    pub file_count: u64,

    /// Number of operations
    pub operations: u64,
}

/// Report type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportType {
    Usage,
    Performance,
    Cost,
    Security,
    Compliance,
}

/// Storage report
#[derive(Debug, Clone)]
pub struct StorageReport {
    /// Report type
    pub report_type: ReportType,

    /// Generated at timestamp
    pub generated_at: chrono::DateTime<chrono::Utc>,

    /// Report data
    pub data: serde_json::Value,

    /// Summary statistics
    pub summary: HashMap<String, String>,

    /// Recommendations
    pub recommendations: Vec<String>,
}