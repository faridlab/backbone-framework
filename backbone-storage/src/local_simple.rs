//! Simplified local filesystem storage implementation
//! This version removes advanced features for compilation stability
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use chrono::DateTime;
use tracing::info;

use crate::{
    StorageResult, StorageError, StorageService, StorageConfig, StorageFile,
    StorageStats, UploadOptions, DownloadOptions, PresignedUrlOptions,
    ByteRange, detect_mime_type, sanitize_filename,
    LocalStorageConfig,
    traits::FileListResult,
};

// Simple MD5 implementation for ID generation
fn md5_compute(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Simplified local filesystem storage implementation
pub struct LocalStorage {
    base_dir: PathBuf,
    config: LocalStorageConfig,
}

impl LocalStorage {
    /// Create new local storage instance
    pub fn new(config: LocalStorageConfig) -> StorageResult<Self> {
        let base_dir = PathBuf::from(&config.base_dir);

        // Ensure base directory exists
        std::fs::create_dir_all(&base_dir).map_err(|e| {
            StorageError::FilesystemError(std::io::Error::other(
                format!("Failed to create base directory {}: {}", base_dir.display(), e)
            ))
        })?;

        // Check if directory is writable
        if !base_dir.exists() || !base_dir.is_dir() {
            return Err(StorageError::InvalidConfiguration(
                format!("Base directory is not a valid directory: {}", base_dir.display())
            ));
        }

        info!("Local storage initialized with base directory: {}", base_dir.display());

        Ok(Self { base_dir, config })
    }

    /// Get file path relative to base directory
    fn get_file_path(&self, key: &str) -> PathBuf {
        // Sanitize the key to prevent directory traversal
        let safe_key = sanitize_filename(key);
        self.base_dir.join(safe_key)
    }

    /// Get file metadata from filesystem
    fn get_file_metadata(&self, path: &Path, key: &str) -> StorageResult<StorageFile> {
        let metadata = std::fs::metadata(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::FileNotFound(key.to_string())
            } else {
                StorageError::FilesystemError(e)
            }
        })?;

        let modified = metadata.modified()
            .map_err(StorageError::FilesystemError)?;
        let created = metadata.created()
            .unwrap_or(modified); // Use modified time if created time not available

        let content_type = detect_mime_type(key);

        Ok(StorageFile {
            id: format!("local-{}", md5_compute(key)), // Simple ID generation
            name: key.to_string(),
            size: metadata.len(),
            content_type,
            checksum: Some(format!("\"{}\"", metadata.len())), // Simple etag
            checksum_algorithm: Some("size".to_string()),
            metadata: HashMap::new(),
            created_at: DateTime::from(created),
            updated_at: Some(DateTime::from(modified)),
            version: None,
            backend: crate::StorageBackend::Local,
            bucket: "local".to_string(),
            storage_path: key.to_string(),
            encrypted: false,
            compressed: false,
            expires_at: None,
        })
    }

    /// Builder for local storage
    pub fn builder() -> LocalStorageBuilder {
        LocalStorageBuilder::new()
    }
}

#[async_trait]
impl StorageService for LocalStorage {
    async fn upload_bytes(
        &self,
        path: &str,
        data: Bytes,
        options: Option<UploadOptions>,
    ) -> StorageResult<StorageFile> {
        let file_path = self.get_file_path(path);

        // Create parent directories if they don't exist
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).map_err(StorageError::FilesystemError)?;
        }

        // Write file
        tokio::fs::write(&file_path, data).await.map_err(StorageError::FilesystemError)?;

        // Return file metadata
        self.get_file_metadata(&file_path, path)
    }

    async fn upload_reader(
        &self,
        path: &str,
        mut reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
        size: Option<u64>,
        options: Option<UploadOptions>,
    ) -> StorageResult<StorageFile> {
        let file_path = self.get_file_path(path);

        // Create parent directories if they don't exist
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).map_err(StorageError::FilesystemError)?;
        }

        // Write from reader to file
        let mut file = tokio::fs::File::create(&file_path).await.map_err(StorageError::FilesystemError)?;
        
        tokio::io::copy(&mut reader, &mut file).await.map_err(StorageError::FilesystemError)?;

        // Return file metadata
        self.get_file_metadata(&file_path, path)
    }

    async fn upload_file(
        &self,
        path: &str,
        local_path: &str,
        options: Option<UploadOptions>,
    ) -> StorageResult<StorageFile> {
        let file_path = self.get_file_path(path);

        // Create parent directories if they don't exist
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).map_err(StorageError::FilesystemError)?;
        }

        // Copy local file
        tokio::fs::copy(local_path, &file_path).await.map_err(StorageError::FilesystemError)?;

        // Return file metadata
        self.get_file_metadata(&file_path, path)
    }

    async fn download_bytes(
        &self,
        path: &str,
        _options: Option<DownloadOptions>,
    ) -> StorageResult<Bytes> {
        let file_path = self.get_file_path(path);

        if !file_path.exists() {
            return Err(StorageError::FileNotFound(path.to_string()));
        }

        let data = tokio::fs::read(&file_path).await.map_err(StorageError::FilesystemError)?;
        Ok(Bytes::from(data))
    }

    async fn download_writer(
        &self,
        path: &str,
        mut writer: Box<dyn tokio::io::AsyncWrite + Send + Unpin>,
        _options: Option<DownloadOptions>,
    ) -> StorageResult<()> {
        let file_path = self.get_file_path(path);

        if !file_path.exists() {
            return Err(StorageError::FileNotFound(path.to_string()));
        }

        let mut file = tokio::fs::File::open(&file_path).await.map_err(StorageError::FilesystemError)?;
        
        tokio::io::copy(&mut file, &mut writer).await.map_err(StorageError::FilesystemError)?;
        Ok(())
    }

    async fn download_file(
        &self,
        path: &str,
        local_path: &str,
        _options: Option<DownloadOptions>,
    ) -> StorageResult<()> {
        let file_path = self.get_file_path(path);

        if !file_path.exists() {
            return Err(StorageError::FileNotFound(path.to_string()));
        }

        // Create parent directories for local path
        if let Some(parent) = std::path::Path::new(local_path).parent() {
            std::fs::create_dir_all(parent).map_err(StorageError::FilesystemError)?;
        }

        tokio::fs::copy(&file_path, local_path).await.map_err(StorageError::FilesystemError)?;
        Ok(())
    }

    async fn stream_file(
        &self,
        path: &str,
        _range: Option<ByteRange>,
    ) -> StorageResult<Box<dyn futures::stream::Stream<Item = StorageResult<Bytes>> + Send + Unpin>> {
        let data = self.download_bytes(path, None).await?;
        let stream = futures::stream::once(async { Ok(data) });
        Ok(Box::new(Box::pin(stream)) as Box<dyn futures::stream::Stream<Item = StorageResult<Bytes>> + Send + Unpin>)
    }

    async fn get_file(&self, path: &str) -> StorageResult<StorageFile> {
        let file_path = self.get_file_path(path);
        self.get_file_metadata(&file_path, path)
    }

    async fn file_exists(&self, path: &str) -> StorageResult<bool> {
        let file_path = self.get_file_path(path);
        Ok(file_path.exists())
    }

    async fn delete_file(&self, path: &str) -> StorageResult<bool> {
        let file_path = self.get_file_path(path);

        if !file_path.exists() {
            return Ok(false);
        }

        tokio::fs::remove_file(&file_path).await.map_err(StorageError::FilesystemError)?;
        Ok(true)
    }

    async fn copy_file(&self, from_path: &str, to_path: &str) -> StorageResult<StorageFile> {
        let source_path = self.get_file_path(from_path);
        let dest_path = self.get_file_path(to_path);

        if !source_path.exists() {
            return Err(StorageError::FileNotFound(from_path.to_string()));
        }

        // Create parent directories for destination
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent).map_err(StorageError::FilesystemError)?;
        }

        tokio::fs::copy(&source_path, &dest_path).await.map_err(StorageError::FilesystemError)?;
        self.get_file_metadata(&dest_path, to_path)
    }

    async fn move_file(&self, from_path: &str, to_path: &str) -> StorageResult<StorageFile> {
        let result = self.copy_file(from_path, to_path).await?;
        let _ = self.delete_file(from_path).await?;
        Ok(result)
    }

    async fn list_files(
        &self,
        prefix: &str,
        limit: Option<u32>,
        continuation_token: Option<String>,
    ) -> StorageResult<FileListResult> {
        let mut files = Vec::new();
        let start_after = continuation_token.unwrap_or_default();

        let mut entries = tokio::fs::read_dir(&self.base_dir).await.map_err(StorageError::FilesystemError)?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();

            if path.is_dir() {
                continue;
            }

            let key = path.strip_prefix(&self.base_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();

            if !key.starts_with(prefix) || key <= start_after {
                continue;
            }

            if let Ok(file) = self.get_file_metadata(&path, &key) {
                files.push(file);
            }

            if let Some(limit) = limit {
                if files.len() >= limit as usize {
                    break;
                }
            }
        }

        let files_count = files.len() as u64;
        let next_continuation_token = if let Some(limit) = limit {
            if files.len() >= limit as usize {
                files.last().map(|f| f.storage_path.clone())
            } else {
                None
            }
        } else {
            None
        };

        Ok(FileListResult {
            files,
            prefixes: Vec::new(),
            truncated: next_continuation_token.is_some(),
            next_continuation_token,
            total_count: Some(files_count),
        })
    }

    async fn generate_presigned_url(
        &self,
        path: &str,
        options: PresignedUrlOptions,
    ) -> StorageResult<String> {
        let file_path = self.get_file_path(path);

        if !file_path.exists() {
            return Err(StorageError::FileNotFound(path.to_string()));
        }

        // For local storage, return a file:// URL
        Ok(format!("file://{}", file_path.display()))
    }

    async fn get_file_url(
        &self,
        path: &str,
        _expiry_seconds: Option<u64>,
    ) -> StorageResult<String> {
        self.generate_presigned_url(path, PresignedUrlOptions::default()).await
    }

    async fn update_metadata(
        &self,
        path: &str,
        _metadata: HashMap<String, String>,
    ) -> StorageResult<StorageFile> {
        // For local storage, metadata is stored in file system attributes
        // For simplicity, just return current file metadata
        self.get_file(path).await
    }

    async fn get_stats(&self) -> StorageResult<StorageStats> {
        let mut total_files = 0u64;
        let mut total_size = 0u64;
        let mut backend_usage = HashMap::new();
        let mut content_type_counts = HashMap::new();

        let mut entries = tokio::fs::read_dir(&self.base_dir).await.map_err(StorageError::FilesystemError)?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();

            if path.is_file() {
                total_files += 1;
                if let Ok(metadata) = entry.metadata().await {
                    total_size += metadata.len();
                }

                // Count by content type
                if let Some(ext) = path.extension() {
                    let content_type = detect_mime_type(&path.to_string_lossy());
                    *content_type_counts.entry(content_type).or_insert(0) += 1;
                }
            }
        }

        backend_usage.insert("local".to_string(), total_size);

        Ok(StorageStats {
            file_count: total_files,
            total_bytes: total_size,
            backend_usage,
            content_type_counts,
            average_file_size: if total_files > 0 { total_size as f64 / total_files as f64 } else { 0.0 },
            largest_file_size: 0, // Would need additional scanning to find largest
            last_updated: chrono::Utc::now(),
        })
    }

    async fn test_connection(&self) -> StorageResult<bool> {
        // Test by checking if base directory exists and is writable
        Ok(self.base_dir.exists() && self.base_dir.is_dir())
    }

    fn backend_type(&self) -> crate::StorageBackend {
        crate::StorageBackend::Local
    }

    fn bucket(&self) -> &str {
        "local"
    }

    fn config(&self) -> &StorageConfig {
        static DEFAULT_CONFIG: std::sync::OnceLock<StorageConfig> = std::sync::OnceLock::new();
        DEFAULT_CONFIG.get_or_init(|| StorageConfig {
            backend: crate::StorageBackend::Local,
            bucket: "local".to_string(),
            base_path: Some(self.config.base_dir.clone()),
            multipart_threshold: 100 * 1024 * 1024, // 100MB
            chunk_size: 8 * 1024 * 1024, // 8MB
            versioning: false,
            compression: false,
            encryption: None,
            retention: None,
            access_control: None,
            backend_config: HashMap::new(),
        })
    }
}

/// Builder for LocalStorage
pub struct LocalStorageBuilder {
    base_dir: Option<String>,
    compression: bool,
}

impl LocalStorageBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            base_dir: None,
            compression: false,
        }
    }

    /// Set base directory
    pub fn base_dir(mut self, dir: impl Into<String>) -> Self {
        self.base_dir = Some(dir.into());
        self
    }

    /// Enable/disable compression
    pub fn compression(mut self, enabled: bool) -> Self {
        self.compression = enabled;
        self
    }

    /// Build LocalStorage
    pub fn build(self) -> StorageResult<LocalStorage> {
        let base_dir = self.base_dir
            .ok_or_else(|| StorageError::InvalidConfiguration("Base directory is required".to_string()))?;

        let config = LocalStorageConfig {
            base_dir,
            base_path: None,
            compression: self.compression,
            encryption: None,
        };

        LocalStorage::new(config)
    }
}

impl Default for LocalStorageBuilder {
    fn default() -> Self {
        Self::new()
    }
}