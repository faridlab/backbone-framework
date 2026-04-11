//! Message compression and decompression module
//!
//! Provides transparent compression for large messages to optimize
//! storage and network transfer costs.

use std::io::{Read, Write};
use flate2::read::{GzDecoder, ZlibDecoder};
use flate2::write::{GzEncoder, ZlibEncoder};
use flate2::Compression;
use serde::{Serialize, Deserialize};
use base64::{Engine as _, engine::general_purpose};
use thiserror::Error;

use crate::{QueueMessage, QueueError, QueueResult};

/// Compression algorithms supported
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CompressionAlgorithm {
    /// No compression
    None,
    /// GZIP compression (good balance of speed and ratio)
    #[default]
    Gzip,
    /// ZLIB compression (compatible with GZIP)
    Zlib,
}

impl CompressionAlgorithm {
    /// Parse compression algorithm from string
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "gzip" => CompressionAlgorithm::Gzip,
            "zlib" => CompressionAlgorithm::Zlib,
            _ => CompressionAlgorithm::None,
        }
    }

    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            CompressionAlgorithm::None => "none",
            CompressionAlgorithm::Gzip => "gzip",
            CompressionAlgorithm::Zlib => "zlib",
        }
    }
}

/// Compression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Compression algorithm to use
    pub algorithm: CompressionAlgorithm,

    /// Compression level (1-9, where 9 is highest compression)
    pub level: u32,

    /// Minimum message size to compress (in bytes)
    pub min_size: usize,

    /// Compress messages even if they might not benefit
    pub force_compression: bool,

    /// Maximum compression attempts before giving up
    pub max_attempts: u32,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            algorithm: CompressionAlgorithm::Gzip,
            level: 6, // Default compression level
            min_size: 1024, // Compress messages larger than 1KB
            force_compression: false,
            max_attempts: 3,
        }
    }
}

/// Compression statistics
#[derive(Debug, Clone, Default)]
pub struct CompressionStats {
    /// Total messages processed
    pub total_messages: u64,

    /// Messages compressed
    pub compressed_messages: u64,

    /// Messages decompressed
    pub decompressed_messages: u64,

    /// Total bytes before compression
    pub original_bytes: u64,

    /// Total bytes after compression
    pub compressed_bytes: u64,

    /// Compression ratio (compressed/original)
    pub compression_ratio: f64,

    /// Compression time in milliseconds
    pub compression_time_ms: u64,

    /// Decompression time in milliseconds
    pub decompression_time_ms: u64,
}

impl CompressionStats {
    /// Calculate compression ratio
    pub fn update_ratio(&mut self) {
        if self.original_bytes > 0 {
            self.compression_ratio = self.compressed_bytes as f64 / self.original_bytes as f64;
        }
    }

    /// Record compression operation
    pub fn record_compression(&mut self, original_size: usize, compressed_size: usize, time_ms: u64) {
        self.total_messages += 1;
        self.compressed_messages += 1;
        self.original_bytes += original_size as u64;
        self.compressed_bytes += compressed_size as u64;
        self.compression_time_ms += time_ms;
        self.update_ratio();
    }

    /// Record decompression operation
    pub fn record_decompression(&mut self, time_ms: u64) {
        self.total_messages += 1;
        self.decompressed_messages += 1;
        self.decompression_time_ms += time_ms;
    }

    /// Get average compression time
    pub fn avg_compression_time_ms(&self) -> f64 {
        if self.compressed_messages > 0 {
            self.compression_time_ms as f64 / self.compressed_messages as f64
        } else {
            0.0
        }
    }

    /// Get average decompression time
    pub fn avg_decompression_time_ms(&self) -> f64 {
        if self.decompressed_messages > 0 {
            self.decompression_time_ms as f64 / self.decompressed_messages as f64
        } else {
            0.0
        }
    }
}

/// Compression errors
#[derive(Debug, Error)]
pub enum CompressionError {
    #[error("Unsupported compression algorithm: {0}")]
    UnsupportedAlgorithm(String),

    #[error("Compression failed: {0}")]
    CompressionFailed(String),

    #[error("Decompression failed: {0}")]
    DecompressionFailed(String),

    #[error("Invalid compression data: {0}")]
    InvalidData(String),

    #[error("Message too large for compression: {size} bytes")]
    MessageTooLarge { size: usize },

    #[error("Compression ratio not beneficial: {ratio:.2}")]
    NotBeneficial { ratio: f64 },
}

impl From<CompressionError> for crate::QueueError {
    fn from(error: CompressionError) -> Self {
        crate::QueueError::Other(error.to_string())
    }
}

/// Message compressor service
pub struct MessageCompressor {
    config: CompressionConfig,
    stats: std::sync::Arc<tokio::sync::RwLock<CompressionStats>>,
}

impl Default for MessageCompressor {
    fn default() -> Self {
        Self::new(CompressionConfig::default())
    }
}

impl MessageCompressor {
    /// Create new message compressor
    pub fn new(config: CompressionConfig) -> Self {
        Self {
            config,
            stats: std::sync::Arc::new(tokio::sync::RwLock::new(CompressionStats::default())),
        }
    }

    /// Compress message payload
    pub async fn compress_message(&self, message: &mut QueueMessage) -> QueueResult<()> {
        let start_time = std::time::Instant::now();

        // Check if compression should be applied
        if !self.should_compress(&message.payload) {
            return Ok(());
        }

        let original_size = serde_json::to_vec(&message.payload).unwrap().len();

        // Convert payload to bytes
        let payload_bytes = match serde_json::to_vec(&message.payload) {
            Ok(bytes) => bytes,
            Err(e) => return Err(QueueError::Serialization(e.to_string())),
        };

        // Perform compression
        let compressed_data = self.compress_data(&payload_bytes).await?;
        let compressed_size = compressed_data.len();

        // Check if compression is beneficial
        if !self.config.force_compression && compressed_size >= original_size {
            return Err(QueueError::Serialization(
                format!("Compression not beneficial: {} -> {} bytes", original_size, compressed_size)
            ));
        }

        // Update message with compressed payload
        message.payload = serde_json::Value::String(
            general_purpose::STANDARD.encode(&compressed_data)
        );

        // Mark as compressed and store metadata
        message.compressed = true;
        message.original_size = Some(original_size);
        message.attributes.insert(
            "compression_algorithm".to_string(),
            self.config.algorithm.as_str().to_string()
        );
        message.attributes.insert(
            "compression_compressed_size".to_string(),
            compressed_size.to_string()
        );

        // Record statistics
        let time_ms = start_time.elapsed().as_millis() as u64;
        {
            let mut stats = self.stats.write().await;
            stats.record_compression(original_size, compressed_size, time_ms);
        }

        Ok(())
    }

    /// Decompress message payload
    pub async fn decompress_message(&self, message: &mut QueueMessage) -> QueueResult<()> {
        let start_time = std::time::Instant::now();

        // Check if message is compressed
        let compression_info = self.get_compression_info(message)?;
        if compression_info.is_none() {
            return Ok(()); // Not compressed
        }

        let (algorithm, original_size) = compression_info.unwrap();

        // Extract compressed data
        let compressed_data = match &message.payload {
            serde_json::Value::String(encoded_data) => {
                match general_purpose::STANDARD.decode(encoded_data) {
                    Ok(data) => data,
                    Err(e) => return Err(QueueError::Deserialization(
                        format!("Failed to decode compressed data: {}", e)
                    )),
                }
            }
            _ => {
                return Err(QueueError::Deserialization(
                    "Compressed payload must be a string".to_string()
                ));
            }
        };

        // Perform decompression
        let decompressed_data = self.decompress_data(&compressed_data, algorithm).await?;

        // Validate decompressed size
        if original_size > 0 && decompressed_data.len() != original_size {
            return Err(QueueError::Deserialization(
                format!("Decompressed size mismatch: expected {}, got {}",
                       original_size, decompressed_data.len())
            ));
        }

        // Restore original payload
        message.payload = match serde_json::from_slice(&decompressed_data) {
            Ok(payload) => payload,
            Err(e) => return Err(QueueError::Deserialization(e.to_string())),
        };

        // Remove compression metadata
        message.compressed = false;
        message.original_size = None;
        message.attributes.remove("compression_algorithm");
        message.attributes.remove("compression_compressed_size");

        // Record statistics
        let time_ms = start_time.elapsed().as_millis() as u64;
        {
            let mut stats = self.stats.write().await;
            stats.record_decompression(time_ms);
        }

        Ok(())
    }

    /// Compress raw data
    async fn compress_data(&self, data: &[u8]) -> Result<Vec<u8>, CompressionError> {
        let compression = Compression::new(self.config.level);

        match self.config.algorithm {
            CompressionAlgorithm::Gzip => {
                let mut encoder = GzEncoder::new(Vec::new(), compression);
                encoder.write_all(data)
                    .map_err(|e| CompressionError::CompressionFailed(e.to_string()))?;
                encoder.finish()
                    .map_err(|e| CompressionError::CompressionFailed(e.to_string()))
            }
            CompressionAlgorithm::Zlib => {
                let mut encoder = ZlibEncoder::new(Vec::new(), compression);
                encoder.write_all(data)
                    .map_err(|e| CompressionError::CompressionFailed(e.to_string()))?;
                encoder.finish()
                    .map_err(|e| CompressionError::CompressionFailed(e.to_string()))
            }
            CompressionAlgorithm::None => {
                Ok(data.to_vec())
            }
        }
    }

    /// Decompress raw data
    async fn decompress_data(&self, data: &[u8], algorithm: CompressionAlgorithm) -> Result<Vec<u8>, CompressionError> {
        match algorithm {
            CompressionAlgorithm::Gzip => {
                let mut decoder = GzDecoder::new(data);
                let mut decompressed = Vec::new();
                decoder.read_to_end(&mut decompressed)
                    .map_err(|e| CompressionError::DecompressionFailed(e.to_string()))?;
                Ok(decompressed)
            }
            CompressionAlgorithm::Zlib => {
                let mut decoder = ZlibDecoder::new(data);
                let mut decompressed = Vec::new();
                decoder.read_to_end(&mut decompressed)
                    .map_err(|e| CompressionError::DecompressionFailed(e.to_string()))?;
                Ok(decompressed)
            }
            CompressionAlgorithm::None => {
                Ok(data.to_vec())
            }
        }
    }

    /// Check if message should be compressed
    fn should_compress(&self, payload: &serde_json::Value) -> bool {
        if self.config.algorithm == CompressionAlgorithm::None {
            return false;
        }

        // Skip if already compressed
        if payload.is_string() && payload.as_str().unwrap_or("").starts_with("H4sI") { // GZIP magic header in base64
            return false;
        }

        // Check minimum size requirement
        let payload_size = match serde_json::to_vec(payload) {
            Ok(vec) => vec.len(),
            Err(_) => return false,
        };

        payload_size >= self.config.min_size || self.config.force_compression
    }

    /// Get compression information from message metadata
    fn get_compression_info(&self, message: &QueueMessage) -> Result<Option<(CompressionAlgorithm, usize)>, QueueError> {
        // Check if message is marked as compressed
        if !message.compressed {
            return Ok(None);
        }

        let algorithm_str = match message.attributes.get("compression_algorithm") {
            Some(s) => s,
            _ => return Ok(None),
        };

        let algorithm = CompressionAlgorithm::parse(algorithm_str);
        if algorithm == CompressionAlgorithm::None {
            return Ok(None);
        }

        let original_size = message.original_size.unwrap_or(0);

        Ok(Some((algorithm, original_size)))
    }

    /// Get compression statistics
    pub async fn get_stats(&self) -> CompressionStats {
        self.stats.read().await.clone()
    }

    /// Reset compression statistics
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = CompressionStats::default();
    }

    /// Update configuration
    pub fn update_config(&mut self, config: CompressionConfig) {
        self.config = config;
    }

    /// Get current configuration
    pub fn get_config(&self) -> &CompressionConfig {
        &self.config
    }
}

/// Compression-aware message builder
pub struct CompressedMessageBuilder {
    inner: crate::QueueMessage,
    compressor: std::sync::Arc<MessageCompressor>,
}

impl CompressedMessageBuilder {
    /// Create new compressed message builder
    pub fn new(compressor: std::sync::Arc<MessageCompressor>) -> Self {
        Self {
            inner: crate::QueueMessage {
                id: String::new(),
                payload: serde_json::Value::Null,
                priority: crate::QueuePriority::Normal,
                receive_count: 0,
                max_receive_count: 3,
                enqueued_at: chrono::Utc::now(),
                created_at: chrono::Utc::now(),
                visible_at: chrono::Utc::now(),
                expires_at: None,
                visibility_timeout: 30,
                status: crate::MessageStatus::Pending,
                delay_seconds: None,
                attributes: std::collections::HashMap::new(),
                headers: std::collections::HashMap::new(),
                message_group_id: None,
                message_deduplication_id: None,
                routing_key: None,
                compressed: false,
                original_size: None,
            },
            compressor,
        }
    }

    /// Set message payload
    pub fn payload(mut self, payload: serde_json::Value) -> Self {
        self.inner.payload = payload;
        self
    }

    /// Set message priority
    pub fn priority(mut self, priority: crate::QueuePriority) -> Self {
        self.inner.priority = priority;
        self
    }

    /// Set message ID
    pub fn id(mut self, id: String) -> Self {
        self.inner.id = id;
        self
    }

    /// Add metadata (attributes)
    pub fn metadata(mut self, key: String, value: serde_json::Value) -> Self {
        // Convert JSON value to string for attributes
        let value_str = match value {
            serde_json::Value::String(s) => s,
            other => other.to_string(),
        };
        self.inner.attributes.insert(key, value_str);
        self
    }

    /// Build compressed message
    pub async fn build(mut self) -> QueueResult<QueueMessage> {
        self.compressor.compress_message(&mut self.inner).await?;
        Ok(self.inner)
    }
}

/// Utility functions
pub mod utils {
    use super::*;

    /// Estimate compression ratio for data
    pub async fn estimate_compression_ratio(data: &[u8], algorithm: CompressionAlgorithm) -> Result<f64, CompressionError> {
        if data.is_empty() {
            return Ok(1.0);
        }

        let compressor = MessageCompressor::new(CompressionConfig {
            algorithm,
            level: 6,
            min_size: 0,
            force_compression: true,
            max_attempts: 1,
        });

        let compressed = compressor.compress_data(data).await?;
        Ok(compressed.len() as f64 / data.len() as f64)
    }

    /// Test compression on sample data
    pub async fn test_compression(payload: &serde_json::Value) -> Vec<(CompressionAlgorithm, f64, usize)> {
        let mut results = Vec::new();
        let payload_bytes = serde_json::to_vec(payload).unwrap_or_default();

        for algorithm in [CompressionAlgorithm::Gzip, CompressionAlgorithm::Zlib] {
            if let Ok(ratio) = estimate_compression_ratio(&payload_bytes, algorithm).await {
                let compressed_size = (payload_bytes.len() as f64 * ratio) as usize;
                results.push((algorithm, ratio, compressed_size));
            }
        }

        results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        results
    }

    /// Get compression recommendations
    pub fn get_compression_recommendations(size: usize) -> CompressionConfig {
        let algorithm = if size < 1024 {
            CompressionAlgorithm::None // Small messages don't benefit from compression
        } else if size < 10240 {
            CompressionAlgorithm::Gzip // Medium messages: GZIP for speed
        } else {
            CompressionAlgorithm::Zlib // Large messages: ZLIB for better ratio
        };

        let level = match size {
            0..=1024 => 0, // No compression for small messages
            1025..=10240 => 6, // Balanced compression
            10241..=102400 => 8, // Higher compression for larger messages
            _ => 9, // Maximum compression for very large messages
        };

        CompressionConfig {
            algorithm,
            level,
            min_size: 1024,
            force_compression: false,
            max_attempts: 3,
        }
    }
}

