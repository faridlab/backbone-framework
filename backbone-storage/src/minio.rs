//! MinIO storage implementation (S3-compatible)
//!
//! MinIO is an S3-compatible object storage server.
//! This implementation delegates all operations to an inner S3Storage
//! instance, since MinIO is fully S3-compatible.

use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;
use tracing::info;

use crate::{
    StorageResult, StorageService, StorageConfig, StorageFile,
    StorageStats, StorageBackend, UploadOptions, DownloadOptions, PresignedUrlOptions,
    ByteRange, S3StorageConfig, MinIOStorageConfig, traits::FileListResult,
    s3::S3Storage,
};

/// MinIO storage implementation (S3-compatible)
///
/// Wraps [`S3Storage`] internally since MinIO is fully S3-compatible.
/// The only differences are `backend_type()` (returns `MinIO`) and
/// the constructor which converts `MinIOStorageConfig` to `S3StorageConfig`.
pub struct MinIOStorage {
    inner: S3Storage,
    storage_config: StorageConfig,
}

impl MinIOStorage {
    /// Create new MinIO storage instance
    pub async fn new(config: MinIOStorageConfig) -> StorageResult<Self> {
        // Convert MinIO config to S3 config
        let s3_config = S3StorageConfig {
            region: config.region.clone(),
            access_key_id: config.access_key.clone(),
            secret_access_key: config.secret_key.clone(),
            bucket: config.bucket.clone(),
            endpoint: Some(config.endpoint.clone()),
            base_path: config.base_path.clone(),
            multipart_threshold: config.multipart_threshold,
            chunk_size: config.chunk_size,
            encryption: config.encryption.clone(),
            path_style: true, // MinIO always uses path-style addressing
            versioning: config.versioning,
        };

        let inner = S3Storage::new(s3_config).await?;

        info!("MinIO storage initialized for bucket: {} at {}", config.bucket, config.endpoint);

        let storage_config = StorageConfig {
            backend: StorageBackend::MinIO,
            bucket: config.bucket.clone(),
            base_path: config.base_path,
            multipart_threshold: config.multipart_threshold,
            chunk_size: config.chunk_size,
            ..Default::default()
        };

        Ok(Self { inner, storage_config })
    }
}

#[async_trait]
impl StorageService for MinIOStorage {
    async fn upload_bytes(&self, path: &str, data: Bytes, options: Option<UploadOptions>) -> StorageResult<StorageFile> {
        self.inner.upload_bytes(path, data, options).await
    }

    async fn upload_reader(&self, path: &str, reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>, size: Option<u64>, options: Option<UploadOptions>) -> StorageResult<StorageFile> {
        self.inner.upload_reader(path, reader, size, options).await
    }

    async fn upload_file(&self, path: &str, local_path: &str, options: Option<UploadOptions>) -> StorageResult<StorageFile> {
        self.inner.upload_file(path, local_path, options).await
    }

    async fn download_bytes(&self, path: &str, options: Option<DownloadOptions>) -> StorageResult<Bytes> {
        self.inner.download_bytes(path, options).await
    }

    async fn download_writer(&self, path: &str, writer: Box<dyn tokio::io::AsyncWrite + Send + Unpin>, options: Option<DownloadOptions>) -> StorageResult<()> {
        self.inner.download_writer(path, writer, options).await
    }

    async fn download_file(&self, path: &str, local_path: &str, options: Option<DownloadOptions>) -> StorageResult<()> {
        self.inner.download_file(path, local_path, options).await
    }

    async fn stream_file(&self, path: &str, range: Option<ByteRange>) -> StorageResult<Box<dyn futures::stream::Stream<Item = StorageResult<Bytes>> + Send + Unpin>> {
        self.inner.stream_file(path, range).await
    }

    async fn get_file(&self, path: &str) -> StorageResult<StorageFile> {
        self.inner.get_file(path).await
    }

    async fn file_exists(&self, path: &str) -> StorageResult<bool> {
        self.inner.file_exists(path).await
    }

    async fn delete_file(&self, path: &str) -> StorageResult<bool> {
        self.inner.delete_file(path).await
    }

    async fn copy_file(&self, from_path: &str, to_path: &str) -> StorageResult<StorageFile> {
        self.inner.copy_file(from_path, to_path).await
    }

    async fn move_file(&self, from_path: &str, to_path: &str) -> StorageResult<StorageFile> {
        self.inner.move_file(from_path, to_path).await
    }

    async fn list_files(&self, prefix: &str, limit: Option<u32>, continuation_token: Option<String>) -> StorageResult<FileListResult> {
        self.inner.list_files(prefix, limit, continuation_token).await
    }

    async fn generate_presigned_url(&self, path: &str, options: PresignedUrlOptions) -> StorageResult<String> {
        self.inner.generate_presigned_url(path, options).await
    }

    async fn get_file_url(&self, path: &str, expiry_seconds: Option<u64>) -> StorageResult<String> {
        self.inner.get_file_url(path, expiry_seconds).await
    }

    async fn update_metadata(&self, path: &str, metadata: HashMap<String, String>) -> StorageResult<StorageFile> {
        self.inner.update_metadata(path, metadata).await
    }

    async fn get_stats(&self) -> StorageResult<StorageStats> {
        self.inner.get_stats().await
    }

    async fn test_connection(&self) -> StorageResult<bool> {
        self.inner.test_connection().await
    }

    fn backend_type(&self) -> StorageBackend {
        StorageBackend::MinIO
    }

    fn bucket(&self) -> &str {
        self.inner.bucket()
    }

    fn config(&self) -> &StorageConfig {
        &self.storage_config
    }
}
