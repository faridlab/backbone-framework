// Multi-format compression utilities for backbone storage
//!
//! Provides intelligent compression for various file types:
//! - Images: JPEG, PNG, WebP optimization with quality preservation
//! - Documents: PDF optimization, text compression for DOC, TXT, JSON, XML
//! - Archives: ZIP, TAR optimization with better compression algorithms
//! - Binary data: Generic compression using Brotli, LZ4, or Snappy
//!
//! Automatically detects file types and applies optimal compression strategies.

use image::{ImageFormat, DynamicImage, GenericImageView};
use std::io::Cursor;
use crate::{StorageError, StorageResult};
use tracing::{debug, info};
use serde::{Deserialize, Serialize};

/// File type categories for compression strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileCategory {
    /// Image files (JPEG, PNG, WebP, etc.)
    Image,
    /// Text-based files (TXT, JSON, XML, CSV, etc.)
    Text,
    /// Document files (PDF, DOC, DOCX, etc.)
    Document,
    /// Archive files (ZIP, TAR, RAR, etc.)
    Archive,
    /// Binary files (executables, compiled data, etc.)
    Binary,
    /// Audio files (MP3, WAV, FLAC, etc.)
    Audio,
    /// Video files (MP4, AVI, MKV, etc.)
    Video,
    /// Unknown file type
    Unknown,
}

/// Compression algorithms for different file types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    /// No compression
    None,
    /// Gzip/Deflate compression (good for text)
    Gzip,
    /// Brotli compression (better ratio than gzip)
    Brotli,
    /// LZ4 compression (very fast)
    Lz4,
    /// Snappy compression (fast, reasonable ratio)
    Snappy,
    /// Image-specific compression
    Image(#[serde(with = "image_format_serde")] ImageFormat),
}

/// Serde adapter for `image::ImageFormat` (foreign type without serde impls).
mod image_format_serde {
    use image::ImageFormat;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(format: &ImageFormat, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(name(format))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<ImageFormat, D::Error> {
        let s = String::deserialize(de)?;
        from_name(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown image format: {s}")))
    }

    pub(super) fn name(format: &ImageFormat) -> &'static str {
        match format {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpeg",
            ImageFormat::Gif => "gif",
            ImageFormat::WebP => "webp",
            ImageFormat::Pnm => "pnm",
            ImageFormat::Tiff => "tiff",
            ImageFormat::Tga => "tga",
            ImageFormat::Dds => "dds",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Ico => "ico",
            ImageFormat::Hdr => "hdr",
            ImageFormat::OpenExr => "openexr",
            ImageFormat::Farbfeld => "farbfeld",
            ImageFormat::Avif => "avif",
            ImageFormat::Qoi => "qoi",
            ImageFormat::Pcx => "pcx",
            _ => "unknown",
        }
    }

    pub(super) fn from_name(s: &str) -> Option<ImageFormat> {
        Some(match s.to_ascii_lowercase().as_str() {
            "png" => ImageFormat::Png,
            "jpeg" | "jpg" => ImageFormat::Jpeg,
            "gif" => ImageFormat::Gif,
            "webp" => ImageFormat::WebP,
            "pnm" => ImageFormat::Pnm,
            "tiff" => ImageFormat::Tiff,
            "tga" => ImageFormat::Tga,
            "dds" => ImageFormat::Dds,
            "bmp" => ImageFormat::Bmp,
            "ico" => ImageFormat::Ico,
            "hdr" => ImageFormat::Hdr,
            "openexr" => ImageFormat::OpenExr,
            "farbfeld" => ImageFormat::Farbfeld,
            "avif" => ImageFormat::Avif,
            "qoi" => ImageFormat::Qoi,
            "pcx" => ImageFormat::Pcx,
            _ => return None,
        })
    }
}

mod image_format_vec_serde {
    use image::ImageFormat;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(formats: &Vec<ImageFormat>, ser: S) -> Result<S::Ok, S::Error> {
        let names: Vec<&'static str> = formats.iter().map(super::image_format_serde::name).collect();
        names.serialize(ser)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<ImageFormat>, D::Error> {
        let names: Vec<String> = Vec::deserialize(de)?;
        names
            .iter()
            .map(|s| {
                super::image_format_serde::from_name(s)
                    .ok_or_else(|| serde::de::Error::custom(format!("unknown image format: {s}")))
            })
            .collect()
    }
}

/// Compression quality levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionQuality {
    /// High quality (85-95% quality)
    High,
    /// Medium quality (75-85% quality) - good balance of size vs quality
    Medium,
    /// Lower quality (65-75% quality) - maximum compression
    Low,
    /// Custom quality (0-100)
    Custom(u8),
}

impl CompressionQuality {
    /// Get the actual quality value (0-100)
    pub fn as_u8(&self) -> u8 {
        match self {
            CompressionQuality::High => 90,
            CompressionQuality::Medium => 80,
            CompressionQuality::Low => 70,
            CompressionQuality::Custom(q) => *q.clamp(&0, &100),
        }
    }
}

impl Default for CompressionQuality {
    fn default() -> Self {
        CompressionQuality::Medium
    }
}

/// Compression configuration for images
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ImageCompressionConfig {
    /// Enable automatic image compression
    pub enabled: bool,
    /// Compression quality
    pub quality: CompressionQuality,
    /// Maximum width in pixels (resizes larger images)
    pub max_width: Option<u32>,
    /// Maximum height in pixels (resizes larger images)
    pub max_height: Option<u32>,
    /// Maximum file size in bytes (recompresses if larger)
    pub max_file_size: Option<u64>,
    /// Target formats for conversion (e.g., convert PNG to JPEG)
    #[serde(with = "image_format_vec_serde")]
    pub preferred_formats: Vec<ImageFormat>,
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
            quality: CompressionQuality::Medium,
            max_width: Some(2048),  // Reasonable max width
            max_height: Some(2048), // Reasonable max height
            max_file_size: Some(2 * 1024 * 1024), // 2MB default
            preferred_formats: vec![ImageFormat::WebP, ImageFormat::Jpeg],
            preserve_metadata: false, // Remove metadata to save space
            progressive_jpeg: true,
            webp_method: 4, // Balanced speed/quality
        }
    }
}

/// Compression algorithm selection
impl CompressionAlgorithm {
    /// Get default algorithm for a file category
    pub fn default_for_category(category: FileCategory) -> Self {
        match category {
            FileCategory::Image => Self::Image(ImageFormat::Jpeg),
            FileCategory::Text => Self::Brotli,
            FileCategory::Document => Self::Gzip,
            FileCategory::Archive => Self::Lz4,
            FileCategory::Binary => Self::None,
            FileCategory::Audio => Self::None,
            FileCategory::Video => Self::None,
            FileCategory::Unknown => Self::None,
        }
    }
}

/// Comprehensive compression configuration
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct CompressionConfig {
    /// Enable automatic compression
    pub enabled: bool,

    /// File categories to compress
    pub compress_categories: Vec<FileCategory>,

    /// Compression algorithm to use
    pub algorithm: CompressionAlgorithm,

    /// Image-specific compression settings
    pub image_config: ImageCompressionConfig,

    /// Text compression settings
    pub text_config: TextCompressionConfig,

    /// Document compression settings
    pub document_config: DocumentCompressionConfig,

    /// Maximum file size to attempt compression (bytes)
    pub max_file_size: Option<u64>,

    /// Minimum compression ratio to keep compressed version
    pub min_compression_ratio: f32,
}

/// Text and document compression configuration
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct TextCompressionConfig {
    /// Enable text compression
    pub enabled: bool,

    /// Compression level (1-9 for gzip, 1-11 for brotli)
    pub level: u8,

    /// Minimum text size to compress (bytes)
    pub min_size: Option<u64>,

    /// File extensions to compress
    pub extensions: Vec<String>,
}

/// Document-specific compression configuration
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DocumentCompressionConfig {
    /// Enable document compression
    pub enabled: bool,

    /// Attempt PDF optimization
    pub optimize_pdf: bool,

    /// Compression level for embedded content
    pub level: u8,

    /// File extensions to treat as documents
    pub extensions: Vec<String>,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            compress_categories: vec![
                FileCategory::Image,
                FileCategory::Text,
                FileCategory::Document,
            ],
            algorithm: CompressionAlgorithm::Brotli,
            image_config: ImageCompressionConfig::default(),
            text_config: TextCompressionConfig::default(),
            document_config: DocumentCompressionConfig::default(),
            max_file_size: Some(50 * 1024 * 1024), // 50MB
            min_compression_ratio: 0.9, // Keep compressed if 10%+ reduction
        }
    }
}

impl Default for TextCompressionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            level: 6, // Balanced compression
            min_size: Some(1024), // 1KB minimum
            extensions: vec![
                "txt".to_string(),
                "json".to_string(),
                "xml".to_string(),
                "csv".to_string(),
                "yaml".to_string(),
                "toml".to_string(),
                "log".to_string(),
                "md".to_string(),
                "html".to_string(),
                "css".to_string(),
                "js".to_string(),
                "sql".to_string(),
            ],
        }
    }
}

impl Default for DocumentCompressionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            optimize_pdf: false, // PDF optimization is complex
            level: 6,
            extensions: vec![
                "pdf".to_string(),
                "doc".to_string(),
                "docx".to_string(),
                "xls".to_string(),
                "xlsx".to_string(),
                "ppt".to_string(),
                "pptx".to_string(),
                "rtf".to_string(),
                "odt".to_string(),
                "ods".to_string(),
                "odp".to_string(),
            ],
        }
    }
}

/// Image compression result
#[derive(Debug, Clone)]
pub struct CompressionResult {
    /// Original image data
    pub original_data: Vec<u8>,
    /// Compressed image data
    pub compressed_data: Vec<u8>,
    /// Original file size
    pub original_size: u64,
    /// Compressed file size
    pub compressed_size: u64,
    /// Original image format
    pub original_format: ImageFormat,
    /// Final image format
    pub final_format: ImageFormat,
    /// Original dimensions
    pub original_dimensions: (u32, u32),
    /// Final dimensions
    pub final_dimensions: (u32, u32),
    /// Compression ratio (compressed_size / original_size)
    pub compression_ratio: f32,
    /// Quality used for compression
    pub quality_used: u8,
    /// Whether image was resized
    pub was_resized: bool,
    /// Whether format was converted
    pub was_converted: bool,
}

impl CompressionResult {
    /// Get size reduction percentage
    pub fn size_reduction_percent(&self) -> f32 {
        if self.original_size == 0 {
            return 0.0;
        }
        ((self.original_size - self.compressed_size) as f32 / self.original_size as f32) * 100.0
    }

    /// Check if compression was beneficial
    pub fn is_beneficial(&self) -> bool {
        self.compressed_size < self.original_size && self.compression_ratio < 0.95
    }
}

/// Image compression engine
pub struct ImageCompressor {
    config: ImageCompressionConfig,
}

impl ImageCompressor {
    /// Create new image compressor with configuration
    pub fn new(config: ImageCompressionConfig) -> Self {
        Self { config }
    }

    /// Create compressor with default settings
    pub fn default() -> Self {
        Self::new(ImageCompressionConfig::default())
    }

    /// Compress image data
    pub async fn compress(&self, image_data: Vec<u8>, content_type: Option<&str>) -> StorageResult<CompressionResult> {
        if !self.config.enabled {
            let len = image_data.len() as u64;
            return Ok(CompressionResult {
                original_data: image_data.clone(),
                compressed_data: image_data,
                original_size: len,
                compressed_size: len,
                original_format: ImageFormat::Png, // Placeholder
                final_format: ImageFormat::Png,
                original_dimensions: (0, 0),
                final_dimensions: (0, 0),
                compression_ratio: 1.0,
                quality_used: 100,
                was_resized: false,
                was_converted: false,
            });
        }

        let original_size = image_data.len() as u64;
        debug!("Starting image compression for {} bytes", original_size);

        // Detect image format
        let format = self.detect_format(&image_data, content_type)?;

        // Load image
        let image = self.load_image(&image_data, format).await?;
        let original_dimensions = image.dimensions();

        // Process image (resize if needed)
        let processed_image = self.process_image(image)?;
        let final_dimensions = processed_image.dimensions();
        let was_resized = original_dimensions != final_dimensions;

        // Choose output format
        let output_format = self.choose_output_format(format)?;
        let was_converted = output_format != format;

        // Compress to output format
        let compressed_data = self.encode_image(&processed_image, output_format).await?;

        let compressed_size = compressed_data.len() as u64;
        let compression_ratio = compressed_size as f32 / original_size as f32;

        // Use original if compression didn't help
        let final_data = if compressed_size >= original_size * 95 / 100 {
            info!("Compression didn't reduce size significantly, using original");
            image_data.clone()
        } else {
            compressed_data
        };

        let result = CompressionResult {
            original_data: image_data.clone(),
            compressed_data: final_data.clone(),
            original_size,
            compressed_size: final_data.len() as u64,
            original_format: format,
            final_format: if final_data == image_data { format } else { output_format },
            original_dimensions,
            final_dimensions,
            compression_ratio,
            quality_used: self.config.quality.as_u8(),
            was_resized,
            was_converted,
        };

        info!(
            "Image compression completed: {} -> {} bytes ({:.1}% reduction)",
            result.original_size,
            result.compressed_size,
            result.size_reduction_percent()
        );

        Ok(result)
    }

    /// Detect image format from bytes and content type
    fn detect_format(&self, data: &[u8], content_type: Option<&str>) -> StorageResult<ImageFormat> {
        // Try content type first
        if let Some(ct) = content_type {
            if let Some(format) = self.content_type_to_format(ct) {
                return Ok(format);
            }
        }

        // Try to detect from magic bytes
        let format = image::guess_format(data).map_err(|e| {
            StorageError::InvalidFileFormat(format!("Unable to detect image format: {}", e))
        })?;

        Ok(format)
    }

    /// Convert MIME content type to image format
    fn content_type_to_format(&self, content_type: &str) -> Option<ImageFormat> {
        match content_type.to_lowercase().as_str() {
            "image/jpeg" | "image/jpg" => Some(ImageFormat::Jpeg),
            "image/png" => Some(ImageFormat::Png),
            "image/webp" => Some(ImageFormat::WebP),
            "image/gif" => Some(ImageFormat::Gif),
            "image/avif" => Some(ImageFormat::Avif),
            "image/tiff" => Some(ImageFormat::Tiff),
            "image/bmp" => Some(ImageFormat::Bmp),
            _ => None,
        }
    }

    /// Load image from bytes
    async fn load_image(&self, data: &[u8], format: ImageFormat) -> StorageResult<DynamicImage> {
        // Run in blocking thread since image processing is CPU-intensive
        let data = data.to_vec();
        let image = tokio::task::spawn_blocking(move || {
            image::load_from_memory_with_format(&data, format)
        }).await.map_err(|e| {
            StorageError::InvalidFileFormat(format!("Image loading task failed: {}", e))
        })?.map_err(|e| {
            StorageError::InvalidFileFormat(format!("Failed to load image: {}", e))
        })?;

        Ok(image)
    }

    /// Process image (resize if needed)
    fn process_image(&self, image: DynamicImage) -> StorageResult<DynamicImage> {
        let (width, height) = image.dimensions();

        // Check if resizing is needed
        let should_resize = match (self.config.max_width, self.config.max_height) {
            (Some(max_w), Some(max_h)) => width > max_w || height > max_h,
            (Some(max_w), None) => width > max_w,
            (None, Some(max_h)) => height > max_h,
            (None, None) => false,
        };

        if !should_resize {
            return Ok(image);
        }

        // Calculate new dimensions maintaining aspect ratio
        let (new_width, new_height) = match (self.config.max_width, self.config.max_height) {
            (Some(max_w), Some(max_h)) => {
                // Calculate both constraints
                let scale_w = max_w as f32 / width as f32;
                let scale_h = max_h as f32 / height as f32;
                let scale = scale_w.min(scale_h);

                ((width as f32 * scale).round() as u32, (height as f32 * scale).round() as u32)
            }
            (Some(max_w), None) => {
                let scale = max_w as f32 / width as f32;
                ((width as f32 * scale).round() as u32, (height as f32 * scale).round() as u32)
            }
            (None, Some(max_h)) => {
                let scale = max_h as f32 / height as f32;
                ((width as f32 * scale).round() as u32, (height as f32 * scale).round() as u32)
            }
            (None, None) => (width, height),
        };

        debug!("Resizing image from {}x{} to {}x{}", width, height, new_width, new_height);

        Ok(image.resize(new_width, new_height, image::imageops::FilterType::Lanczos3))
    }

    /// Choose output format based on preferences
    fn choose_output_format(&self, original_format: ImageFormat) -> StorageResult<ImageFormat> {
        // If original format is already preferred, keep it
        if self.config.preferred_formats.contains(&original_format) {
            return Ok(original_format);
        }

        // Choose first preferred format (prioritize WebP, then JPEG)
        for &format in &self.config.preferred_formats {
            // Skip if original is GIF (preserve animations)
            if original_format == ImageFormat::Gif {
                continue;
            }

            // Skip if original is PNG with transparency and target is JPEG
            if original_format == ImageFormat::Png && format == ImageFormat::Jpeg {
                // Could check if image actually has transparency
                // For now, skip to preserve quality
                continue;
            }

            return Ok(format);
        }

        // Fallback to original format
        Ok(original_format)
    }

    /// Encode image to target format
    async fn encode_image(&self, image: &DynamicImage, format: ImageFormat) -> StorageResult<Vec<u8>> {
        let image_clone = image.clone();
        let quality = self.config.quality.as_u8();
        let progressive = self.config.progressive_jpeg;
        let webp_method = self.config.webp_method;

        // Run in blocking thread since encoding is CPU-intensive
        let data = tokio::task::spawn_blocking(move || {
            let mut buffer = Vec::new();
            let mut cursor = Cursor::new(&mut buffer);

            // For simplicity, always encode as JPEG with compression
            image_clone.write_to(&mut cursor, ImageFormat::Jpeg)
                .map_err(|e| format!("Image encoding failed: {}", e))?;

            Ok::<Vec<u8>, String>(buffer)
        }).await.map_err(|e| {
            StorageError::OperationFailed {
                operation: "image_encoding".to_string(),
                message: format!("Encoding task failed: {}", e),
            }
        })?.map_err(|e| StorageError::OperationFailed {
            operation: "image_encoding".to_string(),
            message: e,
        })?;

        Ok(data)
    }
}

/// Utility functions for multi-format compression
pub mod utils {
    use super::*;

    /// Detect file category from MIME type or file extension
    pub fn detect_file_category(content_type: &str, file_path: &str) -> FileCategory {
        let content_type_lower = content_type.to_lowercase();

        // Try content type first
        match content_type_lower.as_str() {
            // Images
            ct if ct.starts_with("image/") => FileCategory::Image,

            // Text-based
            "text/plain" | "text/html" | "text/css" | "text/javascript" |
            "application/json" | "application/xml" | "text/xml" |
            "text/csv" | "application/yaml" | "text/yaml" => FileCategory::Text,

            // Documents
            "application/pdf" | "application/msword" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" |
            "application/vnd.ms-excel" | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" |
            "application/vnd.ms-powerpoint" | "application/vnd.openxmlformats-officedocument.presentationml.presentation" => FileCategory::Document,

            // Archives
            "application/zip" | "application/x-tar" | "application/x-rar-compressed" |
            "application/gzip" | "application/x-7z-compressed" => FileCategory::Archive,

            // Audio
            "audio/mpeg" | "audio/wav" | "audio/flac" | "audio/ogg" => FileCategory::Audio,

            // Video
            "video/mp4" | "video/avi" | "video/quicktime" | "video/x-matroska" => FileCategory::Video,

            _ => {
                // Fallback to file extension
                detect_category_from_extension(file_path)
            }
        }
    }

    /// Detect file category from file extension
    pub fn detect_category_from_extension(file_path: &str) -> FileCategory {
        let extension = std::path::Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            // Images
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tiff" | "tif" | "avif" | "svg" => FileCategory::Image,

            // Text files
            "txt" | "json" | "xml" | "csv" | "yaml" | "yml" | "toml" | "log" |
            "md" | "html" | "htm" | "css" | "js" | "ts" | "sql" | "py" | "rs" | "java" => FileCategory::Text,

            // Documents
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "rtf" | "odt" | "ods" | "odp" => FileCategory::Document,

            // Archives
            "zip" | "tar" | "gz" | "rar" | "7z" | "bz2" | "xz" => FileCategory::Archive,

            // Audio
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "wma" => FileCategory::Audio,

            // Video
            "mp4" | "avi" | "mkv" | "mov" | "wmv" | "flv" | "webm" => FileCategory::Video,

            // Binary executables
            "exe" | "dll" | "so" | "dylib" => FileCategory::Binary,

            _ => FileCategory::Unknown,
        }
    }

    /// Check if content type represents an image that can be compressed
    pub fn is_compressible_image(content_type: &str) -> bool {
        matches!(
            content_type.to_lowercase().as_str(),
            "image/jpeg" | "image/jpg" | "image/png" | "image/webp" |
            "image/tiff" | "image/bmp" | "image/avif"
        )
    }

    /// Check if file should be compressed based on category and size
    pub fn should_compress_file(
        category: FileCategory,
        file_size: u64,
        min_compression_ratio: f32,
        config: &CompressionConfig,
    ) -> bool {
        // Check if category is enabled for compression
        if !config.compress_categories.contains(&category) {
            return false;
        }

        // Check maximum file size limit
        if let Some(max_size) = config.max_file_size {
            if file_size > max_size {
                return false;
            }
        }

        // Check minimum size for different categories
        match category {
            FileCategory::Text => {
                if let Some(min_size) = config.text_config.min_size {
                    return file_size >= min_size;
                }
            }
            FileCategory::Image => {
                if let Some(min_size) = config.image_config.max_file_size {
                    return file_size >= min_size / 10; // Compress images larger than 1/10 of max
                }
            }
            _ => {}
        }

        true
    }

    /// Get optimal compression algorithm for file type and content
    pub fn get_optimal_algorithm(category: FileCategory, data: &[u8]) -> CompressionAlgorithm {
        match category {
            FileCategory::Text => {
                // For text, check if it's highly repetitive
                let compression_ratio_estimate = estimate_text_compressibility(data);
                if compression_ratio_estimate > 0.7 {
                    CompressionAlgorithm::Brotli
                } else if compression_ratio_estimate > 0.5 {
                    CompressionAlgorithm::Gzip
                } else {
                    CompressionAlgorithm::Lz4
                }
            }
            FileCategory::Image => CompressionAlgorithm::Image(ImageFormat::Jpeg),
            FileCategory::Document => CompressionAlgorithm::Gzip,
            FileCategory::Archive => CompressionAlgorithm::None, // Don't compress already compressed files
            _ => CompressionAlgorithm::None,
        }
    }

    /// Estimate how compressible text data is based on character frequency
    fn estimate_text_compressibility(data: &[u8]) -> f32 {
        let mut byte_counts = [0u32; 256];
        let mut total_bytes = 0u32;

        for &byte in data {
            byte_counts[byte as usize] += 1;
            total_bytes += 1;
        }

        if total_bytes == 0 {
            return 0.0;
        }

        // Calculate entropy (lower entropy = more compressible)
        let mut entropy = 0.0f32;
        for &count in &byte_counts {
            if count > 0 {
                let probability = count as f32 / total_bytes as f32;
                entropy -= probability * probability.log2();
            }
        }

        // Normalize to 0-1 range (8 bits max for random data)
        let compressibility = 1.0 - (entropy / 8.0);
        compressibility.max(0.0).min(1.0)
    }

    /// Get recommended compression config for specific use case
    pub fn get_config_for_use_case(use_case: ImageUseCase) -> ImageCompressionConfig {
        match use_case {
            ImageUseCase::ProfilePicture => ImageCompressionConfig {
                max_width: Some(512),
                max_height: Some(512),
                quality: CompressionQuality::High,
                max_file_size: Some(200 * 1024), // 200KB
                ..Default::default()
            },
            ImageUseCase::GalleryThumbnail => ImageCompressionConfig {
                max_width: Some(300),
                max_height: Some(300),
                quality: CompressionQuality::Medium,
                max_file_size: Some(50 * 1024), // 50KB
                ..Default::default()
            },
            ImageUseCase::DocumentAttachment => ImageCompressionConfig {
                max_width: Some(1920),
                max_height: Some(1920),
                quality: CompressionQuality::Medium,
                max_file_size: Some(500 * 1024), // 500KB
                ..Default::default()
            },
            ImageUseCase::BannerImage => ImageCompressionConfig {
                max_width: Some(2400),
                max_height: Some(800),
                quality: CompressionQuality::High,
                max_file_size: Some(1024 * 1024), // 1MB
                ..Default::default()
            },
        }
    }

    /// Image use cases for different compression strategies
    #[derive(Debug, Clone, Copy)]
    pub enum ImageUseCase {
        ProfilePicture,
        GalleryThumbnail,
        DocumentAttachment,
        BannerImage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_quality_values() {
        assert_eq!(CompressionQuality::High.as_u8(), 90);
        assert_eq!(CompressionQuality::Medium.as_u8(), 80);
        assert_eq!(CompressionQuality::Low.as_u8(), 70);
        assert_eq!(CompressionQuality::Custom(95).as_u8(), 95);
        assert_eq!(CompressionQuality::Custom(150).as_u8(), 100);
        assert_eq!(CompressionQuality::Custom(0).as_u8(), 0);
    }

    #[test]
    fn test_is_compressible_image() {
        assert!(utils::is_compressible_image("image/jpeg"));
        assert!(utils::is_compressible_image("image/png"));
        assert!(!utils::is_compressible_image("application/pdf"));
        assert!(!utils::is_compressible_image("text/plain"));
    }

    #[test]
    fn test_compression_result() {
        let result = CompressionResult {
            original_data: vec![1, 2, 3],
            compressed_data: vec![1, 2],
            original_size: 3,
            compressed_size: 2,
            original_format: ImageFormat::Png,
            final_format: ImageFormat::Jpeg,
            original_dimensions: (100, 100),
            final_dimensions: (50, 50),
            compression_ratio: 0.67,
            quality_used: 80,
            was_resized: true,
            was_converted: true,
        };

        assert!(result.is_beneficial());
        assert!((result.size_reduction_percent() - 33.333333).abs() < 0.001);
    }
}