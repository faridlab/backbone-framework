//! Multi-format compression engine
//!
//! Provides unified compression interface for different file types including:
//! - Images: JPEG, PNG, WebP optimization
//! - Text: JSON, XML, CSV, source code, logs
//! - Documents: PDF optimization (where supported)
//! - Archives: Better compression for ZIP files
//! - Binary data: Fast compression with LZ4/Snappy

use crate::{
    StorageError, StorageResult,
    compression::{
        FileCategory, CompressionAlgorithm, CompressionConfig, CompressionResult,
        ImageCompressionConfig, TextCompressionConfig, DocumentCompressionConfig,
        ImageCompressor,
    },
};
use bytes::Bytes;
use tracing::{debug, info, warn};
use std::io::Read;

/// Universal compression engine
pub struct CompressionEngine {
    config: CompressionConfig,
    image_compressor: ImageCompressor,
}

impl CompressionEngine {
    /// Create new compression engine with configuration
    pub fn new(config: CompressionConfig) -> Self {
        Self {
            image_compressor: ImageCompressor::new(config.image_config.clone()),
            config,
        }
    }

    /// Create engine with default configuration
    pub fn default() -> Self {
        Self::new(CompressionConfig::default())
    }

    /// Compress any file data based on its type
    pub async fn compress(
        &self,
        data: Vec<u8>,
        content_type: &str,
        file_path: &str,
    ) -> StorageResult<CompressionResult> {
        let original_size = data.len() as u64;

        // Don't compress if disabled
        if !self.config.enabled {
            return Ok(CompressionResult {
                original_data: data.clone(),
                compressed_data: data,
                original_size,
                compressed_size: original_size,
                original_format: image::ImageFormat::Png, // Placeholder
                final_format: image::ImageFormat::Png,
                original_dimensions: (0, 0),
                final_dimensions: (0, 0),
                compression_ratio: 1.0,
                quality_used: 100,
                was_resized: false,
                was_converted: false,
            });
        }

        // Detect file category
        let category = self.detect_category(content_type, file_path);

        // Check if we should compress this file
        if !self.should_compress(category, original_size) {
            debug!("Skipping compression for {}: category={:?}, size={}",
                   file_path, category, original_size);
            return Ok(self.create_result_no_compression(data, original_size));
        }

        debug!("Compressing {}: category={:?}, size={}", file_path, category, original_size);

        // Choose compression method based on category
        match category {
            FileCategory::Image => self.compress_image(data, content_type).await,
            FileCategory::Text => self.compress_text(data, file_path).await,
            FileCategory::Document => self.compress_document(data, file_path).await,
            FileCategory::Archive => self.compress_archive(data, file_path).await,
            FileCategory::Binary => self.compress_binary(data, file_path).await,
            _ => Ok(self.create_result_no_compression(data, original_size)),
        }
    }

    /// Detect file category
    fn detect_category(&self, content_type: &str, file_path: &str) -> FileCategory {
        crate::compression::utils::detect_file_category(content_type, file_path)
    }

    /// Check if file should be compressed
    fn should_compress(&self, category: FileCategory, file_size: u64) -> bool {
        crate::compression::utils::should_compress_file(
            category,
            file_size,
            self.config.min_compression_ratio,
            &self.config,
        )
    }

    /// Compress image data using image-specific algorithms
    async fn compress_image(&self, data: Vec<u8>, content_type: &str) -> StorageResult<CompressionResult> {
        self.image_compressor.compress(data, Some(content_type)).await
    }

    /// Compress text data using text-specific algorithms
    async fn compress_text(&self, data: Vec<u8>, _file_path: &str) -> StorageResult<CompressionResult> {
        let original_size = data.len();
        let compressed_data = match self.config.text_config.enabled {
            true => {
                let algorithm = crate::compression::utils::get_optimal_algorithm(
                    FileCategory::Text,
                    &data
                );

                self.apply_compression_algorithm(&data, algorithm).await?
            }
            false => data.clone(),
        };

        Ok(CompressionResult {
            original_data: data.clone(),
            compressed_data: if compressed_data.len() < data.len() { compressed_data } else { data.clone() },
            original_size: original_size as u64,
            compressed_size: compressed_data.len() as u64,
            original_format: image::ImageFormat::Png,
            final_format: image::ImageFormat::Png,
            original_dimensions: (0, 0),
            final_dimensions: (0, 0),
            compression_ratio: compressed_data.len() as f32 / original_size as f32,
            quality_used: 100,
            was_resized: false,
            was_converted: false,
        })
    }

    /// Compress document data
    async fn compress_document(&self, data: Vec<u8>, _file_path: &str) -> StorageResult<CompressionResult> {
        let original_size = data.len();
        let compressed_data = match self.config.document_config.enabled {
            true => {
                let algorithm = crate::compression::utils::get_optimal_algorithm(
                    FileCategory::Document,
                    &data
                );

                self.apply_compression_algorithm(&data, algorithm).await?
            }
            false => data.clone(),
        };

        Ok(CompressionResult {
            original_data: data.clone(),
            compressed_data: if compressed_data.len() < data.len() { compressed_data } else { data.clone() },
            original_size: original_size as u64,
            compressed_size: compressed_data.len() as u64,
            original_format: image::ImageFormat::Png,
            final_format: image::ImageFormat::Png,
            original_dimensions: (0, 0),
            final_dimensions: (0, 0),
            compression_ratio: compressed_data.len() as f32 / original_size as f32,
            quality_used: 100,
            was_resized: false,
            was_converted: false,
        })
    }

    /// Compress archive data
    async fn compress_archive(&self, data: Vec<u8>, _file_path: &str) -> StorageResult<CompressionResult> {
        // Archives are already compressed, but we can try to improve compression
        let original_size = data.len();
        let compressed_data = self.apply_compression_algorithm(&data, CompressionAlgorithm::Lz4).await?;

        // Only use compressed version if it's significantly smaller
        let final_data = if compressed_data.len() < original_size * 90 / 100 {
            compressed_data
        } else {
            data.clone()
        };

        Ok(CompressionResult {
            original_data: data.clone(),
            compressed_data: final_data.clone(),
            original_size: original_size as u64,
            compressed_size: final_data.len() as u64,
            original_format: image::ImageFormat::Png,
            final_format: image::ImageFormat::Png,
            original_dimensions: (0, 0),
            final_dimensions: (0, 0),
            compression_ratio: final_data.len() as f32 / original_size as f32,
            quality_used: 100,
            was_resized: false,
            was_converted: false,
        })
    }

    /// Compress binary data
    async fn compress_binary(&self, data: Vec<u8>, _file_path: &str) -> StorageResult<CompressionResult> {
        let original_size = data.len();
        let compressed_data = self.apply_compression_algorithm(&data, CompressionAlgorithm::Lz4).await?;

        Ok(CompressionResult {
            original_data: data.clone(),
            compressed_data: if compressed_data.len() < data.len() { compressed_data } else { data.clone() },
            original_size: original_size as u64,
            compressed_size: compressed_data.len() as u64,
            original_format: image::ImageFormat::Png,
            final_format: image::ImageFormat::Png,
            original_dimensions: (0, 0),
            final_dimensions: (0, 0),
            compression_ratio: compressed_data.len() as f32 / original_size as f32,
            quality_used: 100,
            was_resized: false,
            was_converted: false,
        })
    }

    /// Apply specific compression algorithm to data
    async fn apply_compression_algorithm(
        &self,
        data: &[u8],
        algorithm: CompressionAlgorithm,
    ) -> StorageResult<Vec<u8>> {
        match algorithm {
            CompressionAlgorithm::Gzip => self.compress_gzip(data).await,
            CompressionAlgorithm::Brotli => self.compress_brotli(data).await,
            CompressionAlgorithm::Lz4 => self.compress_lz4(data).await,
            CompressionAlgorithm::Snappy => self.compress_snappy(data).await,
            CompressionAlgorithm::None => Ok(data.to_vec()),
            CompressionAlgorithm::Image(_) => {
                // This should be handled by the image compression path
                Ok(data.to_vec())
            }
        }
    }

    /// Compress data using Gzip
    async fn compress_gzip(&self, data: &[u8]) -> StorageResult<Vec<u8>> {
        use flate2::{write::GzEncoder, Compression};
        use std::io::Write;

        let data = data.to_vec();
        tokio::task::spawn_blocking(move || {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&data)
                .map_err(|e| StorageError::OperationFailed {
                    operation: "gzip_compression".to_string(),
                    message: format!("Gzip encoding failed: {}", e),
                })?;
            encoder.finish()
                .map_err(|e| StorageError::OperationFailed {
                    operation: "gzip_compression".to_string(),
                    message: format!("Gzip finish failed: {}", e),
                })
        }).await.map_err(|e| {
            StorageError::OperationFailed {
                operation: "gzip_compression".to_string(),
                message: format!("Gzip compression task failed: {}", e),
            }
        })?
    }

    /// Compress data using Brotli
    async fn compress_brotli(&self, data: &[u8]) -> StorageResult<Vec<u8>> {
        // Temporary fallback - use gzip instead until brotli API is fixed
        tracing::warn!("Brotli compression not available, falling back to gzip");
        self.compress_gzip(data).await
    }

    /// Compress data using LZ4
    async fn compress_lz4(&self, data: &[u8]) -> StorageResult<Vec<u8>> {
        let data = data.to_vec();
        tokio::task::spawn_blocking(move || {
            Ok(lz4_flex::block::compress_prepend_size(&data))
        }).await.map_err(|e| {
            StorageError::OperationFailed {
                operation: "lz4_compression".to_string(),
                message: format!("LZ4 compression task failed: {}", e),
            }
        })?
    }

    /// Compress data using Snappy
    async fn compress_snappy(&self, data: &[u8]) -> StorageResult<Vec<u8>> {
        let data = data.to_vec();
        tokio::task::spawn_blocking(move || {
            snap::raw::Encoder::new()
                .compress_vec(&data)
                .map_err(|e| StorageError::OperationFailed {
                    operation: "snappy_compression".to_string(),
                    message: format!("Snappy compression failed: {}", e),
                })
        }).await.map_err(|e| {
            StorageError::OperationFailed {
                operation: "snappy_compression".to_string(),
                message: format!("Snappy compression task failed: {}", e),
            }
        })?
    }

    /// Create result when no compression was applied
    fn create_result_no_compression(&self, data: Vec<u8>, original_size: u64) -> CompressionResult {
        CompressionResult {
            original_data: data.clone(),
            compressed_data: data,
            original_size,
            compressed_size: original_size,
            original_format: image::ImageFormat::Png,
            final_format: image::ImageFormat::Png,
            original_dimensions: (0, 0),
            final_dimensions: (0, 0),
            compression_ratio: 1.0,
            quality_used: 100,
            was_resized: false,
            was_converted: false,
        }
    }

    /// Get compression statistics
    pub fn get_stats(&self) -> CompressionStats {
        CompressionStats {
            enabled: self.config.enabled,
            supported_categories: self.config.compress_categories.clone(),
            default_algorithm: self.config.algorithm,
            image_compression_enabled: self.config.image_config.enabled,
            text_compression_enabled: self.config.text_config.enabled,
            document_compression_enabled: self.config.document_config.enabled,
        }
    }
}

/// Compression engine statistics
#[derive(Debug, Clone)]
pub struct CompressionStats {
    pub enabled: bool,
    pub supported_categories: Vec<FileCategory>,
    pub default_algorithm: CompressionAlgorithm,
    pub image_compression_enabled: bool,
    pub text_compression_enabled: bool,
    pub document_compression_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_text_compression() {
        let engine = CompressionEngine::default();
        let text_data = b"Hello, world! ".repeat(1000);
        let content_type = "text/plain";
        let file_path = "test.txt";

        let result = engine.compress(text_data, content_type, file_path).await.unwrap();

        assert!(result.compressed_size < result.original_size);
        assert!(result.compression_ratio < 1.0);
    }

    #[test]
    fn test_file_category_detection() {
        let engine = CompressionEngine::default();

        assert_eq!(
            engine.detect_category("image/jpeg", "photo.jpg"),
            FileCategory::Image
        );
        assert_eq!(
            engine.detect_category("application/json", "data.json"),
            FileCategory::Text
        );
        assert_eq!(
            engine.detect_category("application/pdf", "document.pdf"),
            FileCategory::Document
        );
    }

    #[test]
    fn test_compression_config() {
        let config = CompressionConfig::default();
        assert!(config.enabled);
        assert!(config.compress_categories.contains(&FileCategory::Image));
        assert!(config.compress_categories.contains(&FileCategory::Text));
    }
}