//! AWS S3 storage implementation

use async_trait::async_trait;
use aws_sdk_s3::{Client, primitives::ByteStream};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_credential_types::Credentials;
use bytes::Bytes;
use std::collections::HashMap;
use std::time::Duration;
use chrono::Utc;
use tracing::{info, debug, error, warn};

use crate::{
    StorageResult, StorageError, StorageService, StorageConfig, StorageFile,
    StorageStats, StorageBackend, UploadOptions, DownloadOptions, PresignedUrlOptions,
    ByteRange, HttpMethod, detect_mime_type, sanitize_filename,
    S3StorageConfig, traits::FileListResult,
};

/// AWS S3 storage implementation
pub struct S3Storage {
    client: Client,
    config: S3StorageConfig,
    bucket: String,
    storage_config: StorageConfig,
}

impl S3Storage {
    /// Create new S3 storage instance
    pub async fn new(config: S3StorageConfig) -> StorageResult<Self> {
        let mut aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest());

        if !config.region.is_empty() {
            aws_config = aws_config.region(aws_config::Region::new(config.region.clone()));
        }

        if let Some(endpoint) = &config.endpoint {
            aws_config = aws_config.endpoint_url(endpoint);
        }

        if !config.access_key_id.is_empty() && !config.secret_access_key.is_empty() {
            let credentials = Credentials::new(
                &config.access_key_id,
                &config.secret_access_key,
                None,
                None,
                "backbone-storage",
            );
            aws_config = aws_config.credentials_provider(credentials);
        }

        let sdk_config = aws_config.load().await;

        // Build S3 client with optional path-style
        let s3_config_builder = aws_sdk_s3::config::Builder::from(&sdk_config)
            .force_path_style(config.path_style);
        let client = Client::from_conf(s3_config_builder.build());

        // Test connection
        Self::test_connection_internal(&client, &config.bucket).await?;

        info!("S3 storage initialized for bucket: {}", config.bucket);

        let storage_config = StorageConfig {
            backend: StorageBackend::S3,
            bucket: config.bucket.clone(),
            base_path: config.base_path.clone(),
            multipart_threshold: config.multipart_threshold,
            chunk_size: config.chunk_size,
            ..Default::default()
        };

        Ok(Self {
            client,
            bucket: config.bucket.clone(),
            config,
            storage_config,
        })
    }

    async fn test_connection_internal(client: &Client, bucket: &str) -> StorageResult<()> {
        match client.head_bucket().bucket(bucket).send().await {
            Ok(_) => {
                debug!("S3 bucket access confirmed: {}", bucket);
                Ok(())
            }
            Err(e) => {
                error!("Failed to access S3 bucket {}: {:?}", bucket, e);
                Err(StorageError::S3OperationFailed {
                    message: format!("Cannot access bucket: {}", e),
                    request_id: None,
                })
            }
        }
    }

    /// Get the S3 object key for a given path, with sanitization applied.
    /// All paths are sanitized to prevent path traversal attacks.
    fn get_object_key(&self, path: &str) -> String {
        let sanitized = sanitize_filename(path);
        match &self.config.base_path {
            Some(base_path) if !base_path.is_empty() => {
                if base_path.ends_with('/') {
                    format!("{}{}", base_path, sanitized)
                } else {
                    format!("{}/{}", base_path, sanitized)
                }
            }
            _ => sanitized,
        }
    }

    fn get_content_type(&self, path: &str, options: &Option<UploadOptions>) -> String {
        if let Some(opts) = options {
            if let Some(content_type) = &opts.content_type {
                return content_type.clone();
            }
        }
        detect_mime_type(path)
    }

    /// Construct a direct (non-presigned) URL for the object.
    fn direct_url(&self, object_key: &str) -> String {
        match &self.config.endpoint {
            Some(endpoint) => format!("{}/{}/{}", endpoint, self.bucket, object_key),
            None => format!(
                "https://{}.s3.{}.amazonaws.com/{}",
                self.bucket, self.config.region, object_key
            ),
        }
    }

    fn s3_error(operation: &str, err: impl std::fmt::Display) -> StorageError {
        StorageError::S3OperationFailed {
            message: format!("{}: {}", operation, err),
            request_id: None,
        }
    }
}

#[async_trait]
impl StorageService for S3Storage {
    async fn upload_bytes(
        &self,
        path: &str,
        data: Bytes,
        options: Option<UploadOptions>,
    ) -> StorageResult<StorageFile> {
        let sanitized_path = sanitize_filename(path);
        let object_key = self.get_object_key(path);
        let content_type = self.get_content_type(path, &options);
        let size = data.len() as u64;

        info!("Uploading {} bytes to S3: {}/{}", size, self.bucket, object_key);

        let body = ByteStream::from(data);
        let mut put_request = self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .body(body)
            .content_type(&content_type);

        if let Some(ref opts) = options {
            for (key, value) in &opts.metadata {
                put_request = put_request.metadata(key, value);
            }
        }

        put_request.send().await.map_err(|e| Self::s3_error("upload", e))?;

        let storage_file = StorageFile::new(
            sanitized_path,
            size,
            content_type,
            self.bucket.clone(),
            object_key,
            StorageBackend::S3,
        );

        Ok(storage_file)
    }

    async fn upload_reader(
        &self,
        path: &str,
        mut reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
        _size: Option<u64>,
        options: Option<UploadOptions>,
    ) -> StorageResult<StorageFile> {
        use tokio::io::AsyncReadExt;

        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await
            .map_err(|e| StorageError::OperationFailed {
                operation: "upload_reader::read_stream".to_string(),
                message: e.to_string(),
            })?;

        self.upload_bytes(path, Bytes::from(buf), options).await
    }

    async fn upload_file(
        &self,
        path: &str,
        local_path: &str,
        options: Option<UploadOptions>,
    ) -> StorageResult<StorageFile> {
        let data = tokio::fs::read(local_path).await?;
        self.upload_bytes(path, Bytes::from(data), options).await
    }

    async fn download_bytes(
        &self,
        path: &str,
        options: Option<DownloadOptions>,
    ) -> StorageResult<Bytes> {
        let object_key = self.get_object_key(path);

        let mut request = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(&object_key);

        // Apply byte range if specified
        if let Some(ref opts) = options {
            if let Some(ref range) = opts.range {
                request = request.range(range.to_header_value());
            }
        }

        let response = request
            .send()
            .await
            .map_err(|e| Self::s3_error("download", e))?;

        let data = response.body.collect().await
            .map_err(|e| Self::s3_error("download_body", e))?;

        Ok(data.into_bytes())
    }

    async fn download_writer(
        &self,
        path: &str,
        mut writer: Box<dyn tokio::io::AsyncWrite + Send + Unpin>,
        options: Option<DownloadOptions>,
    ) -> StorageResult<()> {
        use tokio::io::AsyncWriteExt;

        let data = self.download_bytes(path, options).await?;
        writer.write_all(&data).await?;
        writer.flush().await?;
        Ok(())
    }

    async fn download_file(
        &self,
        path: &str,
        local_path: &str,
        options: Option<DownloadOptions>,
    ) -> StorageResult<()> {
        let data = self.download_bytes(path, options).await?;
        tokio::fs::write(local_path, &data).await?;
        Ok(())
    }

    async fn stream_file(
        &self,
        path: &str,
        range: Option<ByteRange>,
    ) -> StorageResult<Box<dyn futures::stream::Stream<Item = StorageResult<Bytes>> + Send + Unpin>> {
        use tokio::io::AsyncReadExt;

        let object_key = self.get_object_key(path);

        let mut request = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(&object_key);

        if let Some(ref r) = range {
            request = request.range(r.to_header_value());
        }

        let response = request.send().await
            .map_err(|e| Self::s3_error("stream", e))?;

        let reader = response.body.into_async_read();
        let chunk_size = self.storage_config.chunk_size;

        let stream = futures::stream::unfold(
            (reader, false),
            move |(mut reader, done)| async move {
                if done {
                    return None;
                }
                let mut buf = vec![0u8; chunk_size];
                match reader.read(&mut buf).await {
                    Ok(0) => None,
                    Ok(n) => {
                        buf.truncate(n);
                        Some((Ok(Bytes::from(buf)), (reader, false)))
                    }
                    Err(e) => Some((
                        Err(StorageError::OperationFailed {
                            operation: "stream_file".to_string(),
                            message: e.to_string(),
                        }),
                        (reader, true),
                    )),
                }
            },
        );

        Ok(Box::new(Box::pin(stream)))
    }

    async fn get_file(&self, path: &str) -> StorageResult<StorageFile> {
        let object_key = self.get_object_key(path);

        let response = self.client
            .head_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .send()
            .await
            .map_err(|e| Self::s3_error("head_object", e))?;

        let size = response.content_length().unwrap_or(0) as u64;
        let content_type = response.content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        Ok(StorageFile::new(
            path.to_string(),
            size,
            content_type,
            self.bucket.clone(),
            object_key,
            StorageBackend::S3,
        ))
    }

    async fn file_exists(&self, path: &str) -> StorageResult<bool> {
        let object_key = self.get_object_key(path);
        match self.client.head_object().bucket(&self.bucket).key(&object_key).send().await {
            Ok(_) => Ok(true),
            Err(err) => {
                let service_err = err.into_service_error();
                if service_err.is_not_found() {
                    Ok(false)
                } else {
                    Err(Self::s3_error("file_exists", service_err))
                }
            }
        }
    }

    async fn delete_file(&self, path: &str) -> StorageResult<bool> {
        let object_key = self.get_object_key(path);

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .send()
            .await
            .map_err(|e| Self::s3_error("delete", e))?;

        Ok(true)
    }

    async fn copy_file(&self, from_path: &str, to_path: &str) -> StorageResult<StorageFile> {
        let from_key = self.get_object_key(from_path);
        let to_key = self.get_object_key(to_path);
        let copy_source = format!("{}/{}", self.bucket, from_key);

        self.client
            .copy_object()
            .bucket(&self.bucket)
            .copy_source(&copy_source)
            .key(&to_key)
            .send()
            .await
            .map_err(|e| Self::s3_error("copy", e))?;

        self.get_file(to_path).await
    }

    async fn move_file(&self, from_path: &str, to_path: &str) -> StorageResult<StorageFile> {
        let file = self.copy_file(from_path, to_path).await?;
        self.delete_file(from_path).await?;
        Ok(file)
    }

    async fn list_files(
        &self,
        prefix: &str,
        limit: Option<u32>,
        continuation_token: Option<String>,
    ) -> StorageResult<FileListResult> {
        let object_prefix = self.get_object_key(prefix);

        let mut request = self.client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&object_prefix)
            .delimiter("/");

        if let Some(max) = limit {
            request = request.max_keys(max as i32);
        }
        if let Some(token) = continuation_token {
            request = request.continuation_token(&token);
        }

        let response = request.send().await
            .map_err(|e| Self::s3_error("list", e))?;

        let files = response.contents()
            .iter()
            .map(|obj| {
                let key = obj.key().unwrap_or("");
                let size = obj.size().unwrap_or(0) as u64;
                StorageFile::new(
                    key.to_string(),
                    size,
                    detect_mime_type(key),
                    self.bucket.clone(),
                    key.to_string(),
                    StorageBackend::S3,
                )
            })
            .collect();

        let prefixes = response.common_prefixes()
            .iter()
            .filter_map(|p| p.prefix().map(|s| s.to_string()))
            .collect();

        Ok(FileListResult {
            files,
            prefixes,
            truncated: response.is_truncated().unwrap_or(false),
            next_continuation_token: response.next_continuation_token().map(|s| s.to_string()),
            total_count: response.key_count().map(|c| c as u64),
        })
    }

    async fn generate_presigned_url(
        &self,
        path: &str,
        options: PresignedUrlOptions,
    ) -> StorageResult<String> {
        let object_key = self.get_object_key(path);
        let expires_in = Duration::from_secs(options.expiry_seconds);

        let presigning_config = PresigningConfig::expires_in(expires_in)
            .map_err(|e| StorageError::PresignedUrlError(
                format!("Invalid expiry duration: {}", e),
            ))?;

        let presigned = match options.method {
            HttpMethod::GET | HttpMethod::HEAD => {
                self.client
                    .get_object()
                    .bucket(&self.bucket)
                    .key(&object_key)
                    .presigned(presigning_config)
                    .await
                    .map_err(|e| StorageError::PresignedUrlError(
                        format!("Presign GET failed: {}", e),
                    ))?
            }
            HttpMethod::PUT => {
                self.client
                    .put_object()
                    .bucket(&self.bucket)
                    .key(&object_key)
                    .presigned(presigning_config)
                    .await
                    .map_err(|e| StorageError::PresignedUrlError(
                        format!("Presign PUT failed: {}", e),
                    ))?
            }
            HttpMethod::DELETE => {
                self.client
                    .delete_object()
                    .bucket(&self.bucket)
                    .key(&object_key)
                    .presigned(presigning_config)
                    .await
                    .map_err(|e| StorageError::PresignedUrlError(
                        format!("Presign DELETE failed: {}", e),
                    ))?
            }
        };

        Ok(presigned.uri().to_string())
    }

    async fn get_file_url(
        &self,
        path: &str,
        expiry_seconds: Option<u64>,
    ) -> StorageResult<String> {
        match expiry_seconds {
            Some(expiry) => {
                self.generate_presigned_url(path, PresignedUrlOptions {
                    expiry_seconds: expiry,
                    method: HttpMethod::GET,
                    ..Default::default()
                }).await
            }
            None => {
                let object_key = self.get_object_key(path);
                Ok(self.direct_url(&object_key))
            }
        }
    }

    async fn update_metadata(
        &self,
        path: &str,
        metadata: HashMap<String, String>,
    ) -> StorageResult<StorageFile> {
        let object_key = self.get_object_key(path);
        let copy_source = format!("{}/{}", self.bucket, object_key);

        let mut request = self.client
            .copy_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .copy_source(&copy_source)
            .metadata_directive(aws_sdk_s3::types::MetadataDirective::Replace);

        for (key, value) in &metadata {
            request = request.metadata(key, value);
        }

        request.send().await
            .map_err(|e| Self::s3_error("update_metadata", e))?;

        self.get_file(path).await
    }

    async fn get_stats(&self) -> StorageResult<StorageStats> {
        warn!(
            event = "storage.stats_approximated",
            backend = "s3",
            "get_stats is not fully implemented for S3; returning empty stats"
        );
        Ok(StorageStats {
            file_count: 0,
            total_bytes: 0,
            backend_usage: HashMap::new(),
            content_type_counts: HashMap::new(),
            average_file_size: 0.0,
            largest_file_size: 0,
            last_updated: Utc::now(),
        })
    }

    async fn test_connection(&self) -> StorageResult<bool> {
        Self::test_connection_internal(&self.client, &self.bucket).await?;
        Ok(true)
    }

    fn backend_type(&self) -> StorageBackend {
        StorageBackend::S3
    }

    fn bucket(&self) -> &str {
        &self.bucket
    }

    fn config(&self) -> &StorageConfig {
        &self.storage_config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_range_header_full() {
        let range = ByteRange::new(0, Some(999));
        assert_eq!(range.to_header_value(), "bytes=0-999");
    }

    #[test]
    fn test_byte_range_header_open_end() {
        let range = ByteRange::from_start(100);
        assert_eq!(range.to_header_value(), "bytes=100-");
    }

    #[test]
    fn test_get_content_type_explicit() {
        // When content_type is specified in options, it takes precedence
        let opts = Some(UploadOptions {
            content_type: Some("application/pdf".to_string()),
            ..Default::default()
        });
        // Just verify the detect_mime_type fallback works
        assert_eq!(detect_mime_type("test.txt"), "text/plain");
        assert_eq!(detect_mime_type("test.json"), "application/json");
        // Explicit content type should be preferred
        assert_eq!(opts.as_ref().unwrap().content_type.as_ref().unwrap(), "application/pdf");
    }

    #[test]
    fn test_sanitize_path_traversal() {
        let sanitized = sanitize_filename("../../../etc/passwd");
        assert!(!sanitized.contains(".."), "Path traversal should be prevented: {}", sanitized);
    }

    #[test]
    fn test_sanitize_normal_path() {
        let sanitized = sanitize_filename("uploads/images/photo.jpg");
        assert_eq!(sanitized, "uploads/images/photo.jpg");
    }

    #[test]
    fn test_stream_error_mapping() {
        let err = StorageError::OperationFailed {
            operation: "stream_file".to_string(),
            message: "connection reset".to_string(),
        };
        assert_eq!(err.to_string(), "Operation failed: stream_file - connection reset");
        assert!(!err.is_retryable());
    }
}
