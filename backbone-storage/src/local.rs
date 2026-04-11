//! Local filesystem storage implementation

use async_trait::async_trait;
use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, AsyncSeekExt};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use tracing::{info, warn, error, debug};

use crate::{
    StorageResult, StorageError, StorageService, StorageConfig, StorageFile,
    StorageStats, StorageBackend, UploadOptions, DownloadOptions, PresignedUrlOptions,
    ByteRange, detect_mime_type, sanitize_filename,
    LocalStorageConfig,
    traits::FileListResult,
};

#[cfg(feature = "compression")]
use crate::compression::{ImageCompressor, utils::is_compressible_image};

#[cfg(feature = "security")]
use crate::security::{SecurityEngine, ThreatLevel};

#[cfg(feature = "security")]
use crate::types::EncryptionConfig;

#[cfg(feature = "compression")]
use crate::compression::ImageCompressionConfig;

/// Local filesystem storage implementation
pub struct LocalStorage {
    base_dir: PathBuf,
    config: LocalStorageConfig,
    #[cfg(feature = "security")]
    security_engine: SecurityEngine,
}

impl LocalStorage {
    /// Create new local storage instance
    pub fn new(config: LocalStorageConfig) -> StorageResult<Self> {
        let base_dir = PathBuf::from(&config.base_dir);

        // Ensure base directory exists
        std::fs::create_dir_all(&base_dir).map_err(|e| {
            StorageError::FilesystemError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create base directory {}: {}", base_dir.display(), e),
            ))
        })?;

        // Check if directory is writable
        if !base_dir.exists() || !base_dir.is_dir() {
            return Err(StorageError::InvalidConfiguration(
                format!("Base directory is not a valid directory: {}", base_dir.display())
            ));
        }

        info!("Local storage initialized with base directory: {}", base_dir.display());

        // Initialize security engine
        let security_engine = SecurityEngine::default();

        Ok(Self {
            base_dir,
            config,
            security_engine,
        })
    }

    /// Get full file path
    fn get_full_path(&self, path: &str) -> PathBuf {
        let sanitized_path = sanitize_filename(path);

        match &self.config.base_path {
            Some(base_path) => {
                if base_path.is_empty() {
                    self.base_dir.join(sanitized_path)
                } else {
                    self.base_dir.join(base_path).join(sanitized_path)
                }
            }
            None => self.base_dir.join(sanitized_path),
        }
    }

    /// Get relative path from full path
    fn get_relative_path(&self, full_path: &Path) -> String {
        full_path
            .strip_prefix(&self.base_dir)
            .ok()
            .and_then(|p| p.to_str())
            .unwrap_or_else(|| full_path.to_str().unwrap_or("unknown"))
            .to_string()
    }

    /// Ensure parent directory exists
    async fn ensure_parent_dir(&self, file_path: &Path) -> StorageResult<()> {
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                StorageError::FilesystemError(e)
            })?;
        }
        Ok(())
    }

    /// Calculate file checksum
    async fn calculate_checksum(&self, data: &[u8]) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    /// Get file metadata from path
    async fn get_file_metadata(&self, file_path: &Path) -> StorageResult<std::fs::Metadata> {
        tokio::fs::metadata(file_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::FileNotFound(file_path.to_string_lossy().to_string())
            } else {
                StorageError::FilesystemError(e)
            }
        })
    }

    /// Scan file for security threats
    async fn scan_file_security(
        &self,
        data: &[u8],
        file_path: &str,
        content_type: &str,
    ) -> StorageResult<crate::security::SecurityAnalysis> {
        debug!("Security scanning file: {}", file_path);

        let analysis = self.security_engine
            .analyze_file(data, file_path, Some(content_type))
            .await
            .map_err(|e| StorageError::SecurityError {
                operation: "file_scan".to_string(),
                message: format!("Security scan failed: {}", e),
            })?;

        // Log security events
        if self.security_engine.config().log_security_events {
            match analysis.threat_level {
                ThreatLevel::Critical => {
                    error!("CRITICAL security threat detected in {}: {:?}", file_path, analysis.threats);
                }
                ThreatLevel::Warning => {
                    warn!("Security warning for {}: {:?}", file_path, analysis.threats);
                }
                ThreatLevel::Suspicious => {
                    info!("Suspicious file detected: {}: {:?}", file_path, analysis.threats);
                }
                ThreatLevel::Safe => {
                    debug!("File passed security scan: {}", file_path);
                }
            }
        }

        Ok(analysis)
    }

    /// Compress image data if applicable
    async fn compress_image_if_needed(
        &self,
        data: Vec<u8>,
        content_type: &str,
        compression_config: Option<&ImageCompressionConfig>,
    ) -> StorageResult<(Vec<u8>, String)> {
        let Some(config) = compression_config else {
            return Ok((data, content_type.to_string()));
        };

        if !config.enabled || !is_compressible_image(content_type) {
            return Ok((data, content_type.to_string()));
        }

        debug!("Attempting to compress image of type: {}", content_type);

        let compressor = ImageCompressor::new(crate::compression::ImageCompressionConfig {
            enabled: config.enabled,
            quality: crate::compression::CompressionQuality::Custom(config.quality),
            max_width: config.max_width,
            max_height: config.max_height,
            max_file_size: config.max_file_size,
            preferred_formats: config.preferred_formats.iter()
                .filter_map(|f| match f.as_str() {
                    "webp" => Some(image::ImageFormat::WebP),
                    "jpeg" | "jpg" => Some(image::ImageFormat::Jpeg),
                    "png" => Some(image::ImageFormat::Png),
                    "avif" => Some(image::ImageFormat::Avif),
                    _ => None,
                })
                .collect(),
            preserve_metadata: config.preserve_metadata,
            progressive_jpeg: config.progressive_jpeg,
            webp_method: config.webp_method,
        });

        match compressor.compress(data, Some(content_type)).await {
            Ok(result) => {
                if result.is_beneficial() {
                    info!(
                        "Image compression successful: {} -> {} bytes ({:.1}% reduction)",
                        result.original_size,
                        result.compressed_size,
                        result.size_reduction_percent()
                    );

                    // Determine new content type
                    let new_content_type = match result.final_format {
                        image::ImageFormat::Jpeg => "image/jpeg",
                        image::ImageFormat::WebP => "image/webp",
                        image::ImageFormat::Png => "image/png",
                        image::ImageFormat::Avif => "image/avif",
                        _ => content_type,
                    };

                    Ok((result.compressed_data, new_content_type.to_string()))
                } else {
                    debug!("Image compression didn't reduce size, using original");
                    Ok((result.original_data, content_type.to_string()))
                }
            }
            Err(e) => {
                warn!("Image compression failed, using original: {:?}", e);
                Ok((data, content_type.to_string()))
            }
        }
    }

    /// Create storage file from path
    async fn create_storage_file_from_path(
        &self,
        relative_path: &str,
        full_path: &Path,
    ) -> StorageResult<StorageFile> {
        let metadata = self.get_file_metadata(full_path).await?;
        let modified = metadata.modified()
            .map_err(|e| StorageError::FilesystemError(e))?;
        let modified_datetime: DateTime<Utc> = modified.into();

        let size = metadata.len();
        let content_type = detect_mime_type(relative_path);

        // Calculate checksum
        let data = tokio::fs::read(full_path).await.map_err(|e| {
            StorageError::FilesystemError(e)
        })?;
        let checksum = self.calculate_checksum(&data).await;

        Ok(StorageFile::new(
            relative_path.to_string(),
            size,
            content_type,
            "local".to_string(),
            self.get_relative_path(full_path),
            StorageBackend::Local,
        )
        .with_checksum(checksum, "sha256".to_string())
        .with_updated_at(modified_datetime)
        .with_created_at(modified_datetime))
    }

    /// Create a simple stream from file
    async fn create_file_stream(
        &self,
        file_path: &Path,
        range: Option<ByteRange>,
    ) -> StorageResult<Box<dyn futures::stream::Stream<Item = StorageResult<Bytes>> + Send + Unpin>> {
        use tokio::fs::File;

        let mut file = File::open(file_path).await.map_err(|e| {
            StorageError::FilesystemError(e)
        })?;

        // Handle range requests
        if let Some(range) = range {
            let file_size = file.metadata().await?.len() as u64;
            let start = range.start;
            let end = range.end.unwrap_or(file_size - 1);

            if start >= file_size {
                return Err(StorageError::InvalidRange(
                    format!("Start byte {} exceeds file size {}", start, file_size)
                ));
            }

            file.seek(std::io::SeekFrom::Start(start)).await.map_err(|e| {
                StorageError::FilesystemError(e)
            })?;

            let length = (end - start + 1) as usize;
            let mut buffer = vec![0u8; length];
            file.read_exact(&mut buffer).await.map_err(|e| {
                StorageError::FilesystemError(e)
            })?;

            let bytes = Bytes::from(buffer);
            use futures::stream::{once};
            let stream = once(async { Ok(bytes) });
            return Ok(Box::pin(stream));
        }

        // Read entire file (simplified approach)
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await.map_err(|e| {
            StorageError::FilesystemError(e)
        })?;

        let bytes = Bytes::from(buffer);
        use futures::stream::{once};
        let stream = once(async { Ok(bytes) });

        Ok(Box::pin(stream))
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
        let full_path = self.get_full_path(path);

        // Ensure parent directory exists
        self.ensure_parent_dir(&full_path).await?;

        // Detect content type
        let content_type = options
            .as_ref()
            .and_then(|opts| opts.content_type.as_ref())
            .cloned()
            .unwrap_or_else(|| detect_mime_type(path));

        // Convert Bytes to Vec for security scanning
        let file_data = data.to_vec();

        // Perform security scan first
        let security_analysis = self.scan_file_security(&file_data, path, &content_type).await?;

        // Check if file is safe to proceed
        if !self.security_engine.is_file_safe(&security_analysis) {
            error!("File upload blocked due to security threats: {} - {:?} with {} threats",
                   path, security_analysis.threat_level, security_analysis.threats.len());

            return Err(StorageError::SecurityError {
                operation: "file_upload".to_string(),
                message: format!("File blocked due to security threats: {:?}", security_analysis.threat_level),
            });
        }

        // Log security scan results
        info!("Security scan passed for {}: level={:?}, threats={}, executable={}",
              path, security_analysis.threat_level,
              security_analysis.threats.len(), security_analysis.is_executable);

        // Apply image compression if applicable
        let (processed_data, final_content_type) = if options
            .as_ref()
            .map(|opts| opts.compress)
            .unwrap_or(false) {
            // Use compression from local config
            let compression_enabled = self.config.compression;
            if compression_enabled {
                self.compress_image_if_needed(file_data.clone(), &content_type, None).await?
            } else {
                (file_data.clone(), content_type.clone())
            }
        } else {
            (file_data, content_type.clone())
        };

        let final_size = processed_data.len();
        let checksum = self.calculate_checksum(&processed_data).await;

        // Write processed data
        let mut file = tokio::fs::File::create(&full_path).await.map_err(|e| {
            StorageError::FilesystemError(e)
        })?;

        file.write_all(&processed_data).await.map_err(|e| {
            StorageError::FilesystemError(e)
        })?;

        file.flush().await.map_err(|e| {
            StorageError::FilesystemError(e)
        })?;

        // Create storage file
        let mut storage_file = StorageFile::new(
            path.to_string(),
            final_size as u64,
            final_content_type,
            "local".to_string(),
            self.get_relative_path(&full_path),
            StorageBackend::Local,
        )
        .with_checksum(checksum, "sha256".to_string())
        .with_created_at(Utc::now());

        // Add metadata from options
        if let Some(opts) = options {
            for (key, value) in opts.metadata {
                storage_file = storage_file.with_metadata(key, value);
            }

            if opts.compress {
                storage_file = storage_file.with_compression(true);
            }

            if opts.encrypt {
                storage_file = storage_file.with_encryption(true);
            }
        }

        info!("Successfully uploaded {} bytes to local storage: {}",
              final_size, full_path.display());

        Ok(storage_file)
    }

    async fn upload_reader(
        &self,
        path: &str,
        mut reader: Box<dyn AsyncRead + Send + Unpin>,
        size: Option<u64>,
        options: Option<UploadOptions>,
    ) -> StorageResult<StorageFile> {
        let full_path = self.get_full_path(path);

        // Ensure parent directory exists
        self.ensure_parent_dir(&full_path).await?;

        // Read all data from reader
        let mut buffer = Vec::new();
        let bytes_read = reader.read_to_end(&mut buffer).await.map_err(|e| {
            StorageError::UploadInterrupted(format!("Failed to read from stream: {}", e))
        })?;

        let data = Bytes::from(buffer);

        // Write to file
        self.upload_bytes(path, data, options).await
    }

    async fn upload_file(
        &self,
        path: &str,
        local_path: &str,
        options: Option<UploadOptions>,
    ) -> StorageResult<StorageFile> {
        let file_data = tokio::fs::read(local_path).await.map_err(|e| {
            StorageError::FilesystemError(e)
        })?;

        let data = Bytes::from(file_data);
        self.upload_bytes(path, data, options).await
    }

    async fn download_bytes(
        &self,
        path: &str,
        options: Option<DownloadOptions>,
    ) -> StorageResult<Bytes> {
        let full_path = self.get_full_path(path);

        // Handle range requests
        if let Some(opts) = &options {
            if let Some(range) = opts.range {
                use tokio::fs::File;
                let mut file = File::open(&full_path).await.map_err(|e| {
                    StorageError::FilesystemError(e)
                })?;

                let file_size = file.metadata().await?.len() as u64;
                let start = range.start;
                let end = range.end.unwrap_or(file_size - 1);

                if start >= file_size {
                    return Err(StorageError::InvalidRange(
                        format!("Start byte {} exceeds file size {}", start, file_size)
                    ));
                }

                file.seek(std::io::SeekFrom::Start(start)).await.map_err(|e| {
                    StorageError::FilesystemError(e)
                })?;

                let length = (end - start + 1) as usize;
                let mut buffer = vec![0u8; length];
                file.read_exact(&mut buffer).await.map_err(|e| {
                    StorageError::FilesystemError(e)
                })?;

                return Ok(Bytes::from(buffer));
            }
        }

        // Read entire file
        let data = tokio::fs::read(&full_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::FileNotFound(full_path.to_string_lossy().to_string())
            } else {
                StorageError::FilesystemError(e)
            }
        })?;

        // Verify checksum if requested
        if let Some(opts) = options {
            if opts.verify_checksum {
                if let Some(expected_checksum) = opts.expected_checksum {
                    let actual_checksum = self.calculate_checksum(&data).await;
                    if expected_checksum != actual_checksum {
                        return Err(StorageError::ChecksumMismatch {
                            expected: expected_checksum,
                            actual: actual_checksum,
                        });
                    }
                }
            }
        }

        info!("Successfully downloaded {} bytes from local storage: {}",
              data.len(), full_path.display());

        Ok(Bytes::from(data))
    }

    async fn download_writer(
        &self,
        path: &str,
        mut writer: Box<dyn AsyncWrite + Send + Unpin>,
        options: Option<DownloadOptions>,
    ) -> StorageResult<()> {
        let data = self.download_bytes(path, options).await?;

        writer.write_all(&data).await.map_err(|e| {
            StorageError::DownloadInterrupted(format!("Failed to write to output: {}", e))
        })?;

        writer.flush().await.map_err(|e| {
            StorageError::DownloadInterrupted(format!("Failed to flush output: {}", e))
        })?;

        Ok(())
    }

    async fn download_file(
        &self,
        path: &str,
        local_path: &str,
        options: Option<DownloadOptions>,
    ) -> StorageResult<()> {
        let data = self.download_bytes(path, options).await?;

        // Ensure parent directory exists for target
        if let Some(parent) = Path::new(local_path).parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                StorageError::FilesystemError(e)
            })?;
        }

        tokio::fs::write(local_path, &data).await.map_err(|e| {
            StorageError::FilesystemError(e)
        })?;

        info!("Successfully downloaded file to: {}", local_path);
        Ok(())
    }

    async fn stream_file(
        &self,
        path: &str,
        range: Option<ByteRange>,
    ) -> StorageResult<Box<dyn futures::stream::Stream<Item = StorageResult<Bytes>> + Send + Unpin>> {
        let full_path = self.get_full_path(path);
        self.create_file_stream(&full_path, range).await
    }

    async fn get_file(&self, path: &str) -> StorageResult<StorageFile> {
        let full_path = self.get_full_path(path);

        if !full_path.exists() {
            return Err(StorageError::FileNotFound(
                full_path.to_string_lossy().to_string()
            ));
        }

        self.create_storage_file_from_path(path, &full_path).await
    }

    async fn file_exists(&self, path: &str) -> StorageResult<bool> {
        let full_path = self.get_full_path(path);
        Ok(full_path.exists() && full_path.is_file())
    }

    async fn delete_file(&self, path: &str) -> StorageResult<bool> {
        let full_path = self.get_full_path(path);

        if !full_path.exists() {
            return Ok(false);
        }

        tokio::fs::remove_file(&full_path).await.map_err(|e| {
            StorageError::FilesystemError(e)
        })?;

        info!("Successfully deleted file: {}", full_path.display());
        Ok(true)
    }

    async fn copy_file(&self, from_path: &str, to_path: &str) -> StorageResult<StorageFile> {
        let from_full = self.get_full_path(from_path);
        let to_full = self.get_full_path(to_path);

        // Ensure source exists
        if !from_full.exists() {
            return Err(StorageError::FileNotFound(
                from_full.to_string_lossy().to_string()
            ));
        }

        // Ensure target directory exists
        self.ensure_parent_dir(&to_full).await?;

        // Copy file
        tokio::fs::copy(&from_full, &to_full).await.map_err(|e| {
            StorageError::FilesystemError(e)
        })?;

        info!("Successfully copied file: {} -> {}",
              from_full.display(), to_full.display());

        // Return the copied file information
        self.create_storage_file_from_path(to_path, &to_full).await
    }

    async fn move_file(&self, from_path: &str, to_path: &str) -> StorageResult<StorageFile> {
        let from_full = self.get_full_path(from_path);
        let to_full = self.get_full_path(to_path);

        // Ensure source exists
        if !from_full.exists() {
            return Err(StorageError::FileNotFound(
                from_full.to_string_lossy().to_string()
            ));
        }

        // Ensure target directory exists
        self.ensure_parent_dir(&to_full).await?;

        // Move file
        tokio::fs::rename(&from_full, &to_full).await.map_err(|e| {
            StorageError::FilesystemError(e)
        })?;

        info!("Successfully moved file: {} -> {}",
              from_full.display(), to_full.display());

        // Return the moved file information
        self.create_storage_file_from_path(to_path, &to_full).await
    }

    async fn list_files(
        &self,
        prefix: &str,
        limit: Option<u32>,
        _continuation_token: Option<String>,
    ) -> StorageResult<FileListResult> {
        let search_path = self.get_full_path(prefix);
        let mut files = Vec::new();
        let mut prefixes = Vec::new();

        // Walk through directory
        let mut entries = tokio::fs::read_dir(&search_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                return StorageError::FileNotFound(search_path.to_string_lossy().to_string());
            }
            StorageError::FilesystemError(e)
        })?;

        let mut count = 0;
        let limit = limit.unwrap_or(u32::MAX);

        while let Ok(Some(entry)) = entries.next_entry().await {
            if count >= limit {
                break;
            }

            let path = entry.path();

            if path.is_file() {
                // Convert to relative path
                let relative_path = self.get_relative_path(&path);

                // Create storage file
                match self.create_storage_file_from_path(&relative_path, &path).await {
                    Ok(file) => files.push(file),
                    Err(e) => warn!("Failed to get metadata for {}: {:?}", path.display(), e),
                }

                count += 1;
            } else if path.is_dir() {
                // Add directory as prefix
                let relative_path = self.get_relative_path(&path);
                if !relative_path.is_empty() {
                    prefixes.push(relative_path);
                }
            }
        }

        let total_count = files.len() as u64;
        Ok(FileListResult {
            files,
            prefixes,
            truncated: false, // Simplified - no continuation support
            next_continuation_token: None,
            total_count: Some(total_count),
        })
    }

    async fn generate_presigned_url(
        &self,
        _path: &str,
        _options: PresignedUrlOptions,
    ) -> StorageResult<String> {
        // Local storage doesn't support presigned URLs
        // Return file:// URL as alternative
        Err(StorageError::UnsupportedOperation {
            operation: "generate_presigned_url".to_string(),
            backend: "local".to_string(),
        })
    }

    async fn get_file_url(
        &self,
        path: &str,
        _expiry_seconds: Option<u64>,
    ) -> StorageResult<String> {
        let full_path = self.get_full_path(path);

        if full_path.exists() {
            Ok(format!("file://{}", full_path.display()))
        } else {
            Err(StorageError::FileNotFound(
                full_path.to_string_lossy().to_string()
            ))
        }
    }

    async fn update_metadata(
        &self,
        path: &str,
        metadata: HashMap<String, String>,
    ) -> StorageResult<StorageFile> {
        // Local storage doesn't have native metadata support
        // We could use extended file attributes or sidecar files
        // For now, just return the existing file with updated metadata in memory
        let mut storage_file = self.get_file(path).await?;

        for (key, value) in metadata {
            storage_file = storage_file.with_metadata(key, value);
        }

        Ok(storage_file)
    }

    async fn get_stats(&self) -> StorageResult<StorageStats> {
        let mut file_count = 0u64;
        let mut total_bytes = 0u64;
        let mut content_type_counts = HashMap::new();

        // Walk through all files recursively
        let mut walk_dir = tokio::fs::read_dir(&self.base_dir).await.map_err(|e| {
            StorageError::FilesystemError(e)
        })?;

        while let Ok(Some(entry)) = walk_dir.next_entry().await {
            let path = entry.path();

            if path.is_file() {
                file_count += 1;

                let metadata = entry.metadata().await.map_err(|e| {
                    StorageError::FilesystemError(e)
                })?;

                total_bytes += metadata.len();

                // Count by extension
                if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
                    *content_type_counts.entry(format!("ext:{}", extension))
                        .or_insert(0) += 1;
                }
            }
        }

        let mut backend_usage = HashMap::new();
        backend_usage.insert("local".to_string(), total_bytes);

        let average_file_size = if file_count > 0 {
            total_bytes as f64 / file_count as f64
        } else {
            0.0
        };

        Ok(StorageStats {
            file_count,
            total_bytes,
            backend_usage,
            content_type_counts,
            average_file_size,
            largest_file_size: total_bytes, // Simplified
            last_updated: Utc::now(),
        })
    }

    async fn test_connection(&self) -> StorageResult<bool> {
        // Check if base directory is accessible and writable
        let test_file = self.base_dir.join(".backbone_storage_test");

        match tokio::fs::write(&test_file, "test").await {
            Ok(_) => {
                let _ = tokio::fs::remove_file(&test_file).await;
                Ok(true)
            }
            Err(e) => {
                warn!("Local storage connection test failed: {:?}", e);
                Ok(false)
            }
        }
    }

    fn backend_type(&self) -> StorageBackend {
        StorageBackend::Local
    }

    fn bucket(&self) -> &str {
        "local"
    }

    fn config(&self) -> &StorageConfig {
        // For now, return a reference to a static default config
        // In a real implementation, you would store the config in the struct
        static DEFAULT_CONFIG: std::sync::OnceLock<StorageConfig> = std::sync::OnceLock::new();
        DEFAULT_CONFIG.get_or_init(|| StorageConfig::default())
    }
}

/// Local storage builder
pub struct LocalStorageBuilder {
    config: LocalStorageConfig,
}

impl LocalStorageBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            config: LocalStorageConfig {
                base_dir: "./storage".to_string(),
                base_path: None,
                compression: false,
                encryption: None,
            },
        }
    }

    /// Set base directory
    pub fn base_dir(mut self, base_dir: impl Into<String>) -> Self {
        self.config.base_dir = base_dir.into();
        self
    }

    /// Set base path
    pub fn base_path(mut self, base_path: impl Into<String>) -> Self {
        self.config.base_path = Some(base_path.into());
        self
    }

    /// Enable compression
    pub fn compression(mut self, enabled: bool) -> Self {
        self.config.compression = enabled;
        self
    }

    /// Set encryption
    pub fn encryption(mut self, encryption: EncryptionConfig) -> Self {
        self.config.encryption = Some(encryption);
        self
    }

    /// Build local storage
    pub fn build(self) -> StorageResult<LocalStorage> {
        LocalStorage::new(self.config)
    }
}

impl Default for LocalStorageBuilder {
    fn default() -> Self {
        Self::new()
    }
}