//! Unit tests for message compression module

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{QueueMessage, QueuePriority};
    use serde_json::json;
    use std::collections::HashMap;

    /// Create test message with payload
    fn create_test_message(payload: serde_json::Value) -> QueueMessage {
        QueueMessage {
            id: "test-msg-123".to_string(),
            payload,
            priority: QueuePriority::Normal,
            created_at: chrono::Utc::now(),
            expires_at: None,
            receive_count: 0,
            max_receive_count: 3,
            visibility_timeout: 30,
            delay_until: None,
            metadata: HashMap::new(),
        }
    }

    /// Create large test payload
    fn create_large_payload(size_kb: usize) -> serde_json::Value {
        let base_text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ";
        let mut text = String::new();

        while text.len() < size_kb * 1024 {
            text.push_str(base_text);
        }

        json!({
            "message": text,
            "type": "large_message",
            "metadata": {
                "size_kb": size_kb,
                "compression_test": true
            }
        })
    }

    #[tokio::test]
    async fn test_compression_algorithm_from_str() {
        assert_eq!(CompressionAlgorithm::parse("gzip"), CompressionAlgorithm::Gzip);
        assert_eq!(CompressionAlgorithm::parse("GZIP"), CompressionAlgorithm::Gzip);
        assert_eq!(CompressionAlgorithm::parse("zlib"), CompressionAlgorithm::Zlib);
        assert_eq!(CompressionAlgorithm::parse("ZLIB"), CompressionAlgorithm::Zlib);
        assert_eq!(CompressionAlgorithm::parse("invalid"), CompressionAlgorithm::None);
        assert_eq!(CompressionAlgorithm::parse(""), CompressionAlgorithm::None);
    }

    #[tokio::test]
    async fn test_compression_algorithm_as_str() {
        assert_eq!(CompressionAlgorithm::Gzip.as_str(), "gzip");
        assert_eq!(CompressionAlgorithm::Zlib.as_str(), "zlib");
        assert_eq!(CompressionAlgorithm::None.as_str(), "none");
    }

    #[tokio::test]
    async fn test_compression_config_default() {
        let config = CompressionConfig::default();
        assert_eq!(config.algorithm, CompressionAlgorithm::Gzip);
        assert_eq!(config.level, 6);
        assert_eq!(config.min_size, 1024);
        assert!(!config.force_compression);
        assert_eq!(config.max_attempts, 3);
    }

    #[tokio::test]
    async fn test_message_compressor_creation() {
        let config = CompressionConfig::default();
        let compressor = MessageCompressor::new(config.clone());

        assert_eq!(compressor.get_config().algorithm, config.algorithm);
        assert_eq!(compressor.get_config().level, config.level);
    }

    #[tokio::test]
    async fn test_should_compress_small_message() {
        let compressor = MessageCompressor::default();
        let small_payload = json!({"small": "message"});

        assert!(!compressor.should_compress(&small_payload));
    }

    #[tokio::test]
    async fn test_should_compress_large_message() {
        let compressor = MessageCompressor::default();
        let large_payload = create_large_payload(2); // 2KB

        assert!(compressor.should_compress(&large_payload));
    }

    #[tokio::test]
    async fn test_compress_message_gzip() {
        let config = CompressionConfig {
            algorithm: CompressionAlgorithm::Gzip,
            level: 6,
            min_size: 100,
            force_compression: false,
            max_attempts: 3,
        };

        let compressor = MessageCompressor::new(config);
        let mut message = create_test_message(create_large_payload(2));
        let original_size = serde_json::to_vec(&message.payload).unwrap().len();

        let result = compressor.compress_message(&mut message).await;
        assert!(result.is_ok());

        // Check compression metadata
        assert!(message.metadata.contains_key("compression_algorithm"));
        assert!(message.metadata.contains_key("compression_original_size"));
        assert!(message.metadata.contains_key("compression_compressed_size"));

        // Check payload is now a string (base64 encoded compressed data)
        assert!(message.payload.is_string());

        let compressed_size = message.metadata["compression_compressed_size"].as_u64().unwrap() as usize;
        assert!(compressed_size < original_size, "Compression should reduce size");
    }

    #[tokio::test]
    async fn test_compress_message_zlib() {
        let config = CompressionConfig {
            algorithm: CompressionAlgorithm::Zlib,
            level: 6,
            min_size: 100,
            force_compression: false,
            max_attempts: 3,
        };

        let compressor = MessageCompressor::new(config);
        let mut message = create_test_message(create_large_payload(2));
        let original_size = serde_json::to_vec(&message.payload).unwrap().len();

        let result = compressor.compress_message(&mut message).await;
        assert!(result.is_ok());

        assert_eq!(
            message.metadata["compression_algorithm"],
            json!("zlib")
        );

        let compressed_size = message.metadata["compression_compressed_size"].as_u64().unwrap() as usize;
        assert!(compressed_size < original_size);
    }

    #[tokio::test]
    async fn test_compress_message_none_algorithm() {
        let config = CompressionConfig {
            algorithm: CompressionAlgorithm::None,
            level: 6,
            min_size: 0,
            force_compression: false,
            max_attempts: 3,
        };

        let compressor = MessageCompressor::new(config);
        let mut message = create_test_message(create_large_payload(2));
        let original_payload = message.payload.clone();

        let result = compressor.compress_message(&mut message).await;
        assert!(result.is_ok());

        // Payload should remain unchanged
        assert_eq!(message.payload, original_payload);
        assert!(!message.metadata.contains_key("compression_algorithm"));
    }

    #[tokio::test]
    async fn test_compress_small_message_below_threshold() {
        let config = CompressionConfig {
            algorithm: CompressionAlgorithm::Gzip,
            level: 6,
            min_size: 1024, // 1KB threshold
            force_compression: false,
            max_attempts: 3,
        };

        let compressor = MessageCompressor::new(config);
        let mut message = create_test_message(json!({"small": "message"}));
        let original_payload = message.payload.clone();

        let result = compressor.compress_message(&mut message).await;
        assert!(result.is_ok());

        // Should not compress small message
        assert_eq!(message.payload, original_payload);
        assert!(!message.metadata.contains_key("compression_algorithm"));
    }

    #[tokio::test]
    async fn test_compress_force_compression() {
        let config = CompressionConfig {
            algorithm: CompressionAlgorithm::Gzip,
            level: 6,
            min_size: 1024,
            force_compression: true, // Force compression even if not beneficial
            max_attempts: 3,
        };

        let compressor = MessageCompressor::new(config);
        let mut message = create_test_message(json!({"tiny": "msg"}));

        let result = compressor.compress_message(&mut message).await;
        assert!(result.is_ok());

        // Should compress even small message when forced
        assert!(message.metadata.contains_key("compression_algorithm"));
        assert!(message.payload.is_string());
    }

    #[tokio::test]
    async fn test_decompress_message() {
        let config = CompressionConfig {
            algorithm: CompressionAlgorithm::Gzip,
            level: 6,
            min_size: 100,
            force_compression: false,
            max_attempts: 3,
        };

        let compressor = MessageCompressor::new(config);
        let mut message = create_test_message(create_large_payload(2));
        let original_payload = message.payload.clone();

        // Compress the message
        compressor.compress_message(&mut message).await.unwrap();

        // Verify it's compressed
        assert!(message.payload.is_string());
        assert!(message.metadata.contains_key("compression_algorithm"));

        // Decompress the message
        let result = compressor.decompress_message(&mut message).await;
        assert!(result.is_ok());

        // Verify original payload is restored
        assert_eq!(message.payload, original_payload);
        assert!(!message.metadata.contains_key("compression_algorithm"));
    }

    #[tokio::test]
    async fn test_decompress_uncompressed_message() {
        let compressor = MessageCompressor::default();
        let mut message = create_test_message(json!({"test": "message"}));
        let original_payload = message.payload.clone();

        // Decompress should work on uncompressed messages
        let result = compressor.decompress_message(&mut message).await;
        assert!(result.is_ok());

        // Payload should remain unchanged
        assert_eq!(message.payload, original_payload);
    }

    #[tokio::test]
    async fn test_compression_decompression_roundtrip() {
        let config = CompressionConfig {
            algorithm: CompressionAlgorithm::Gzip,
            level: 9, // Maximum compression
            min_size: 100,
            force_compression: false,
            max_attempts: 3,
        };

        let compressor = MessageCompressor::new(config);
        let mut message = create_test_message(create_large_payload(5)); // 5KB
        let original_payload = message.payload.clone();

        // Compress
        compressor.compress_message(&mut message).await.unwrap();
        assert!(message.payload.is_string());

        // Decompress
        compressor.decompress_message(&mut message).await.unwrap();
        assert!(message.payload.is_object());

        // Verify data integrity
        assert_eq!(message.payload, original_payload);
    }

    #[tokio::test]
    async fn test_compression_stats() {
        let compressor = MessageCompressor::default();

        // Initially stats should be empty
        let stats = compressor.get_stats().await;
        assert_eq!(stats.total_messages, 0);
        assert_eq!(stats.compressed_messages, 0);
        assert_eq!(stats.compression_ratio, 0.0);

        // Compress a message
        let mut message = create_test_message(create_large_payload(2));
        compressor.compress_message(&mut message).await.unwrap();

        // Check stats updated
        let stats = compressor.get_stats().await;
        assert_eq!(stats.total_messages, 1);
        assert_eq!(stats.compressed_messages, 1);
        assert!(stats.original_bytes > 0);
        assert!(stats.compressed_bytes > 0);
        assert!(stats.compression_ratio > 0.0);
        assert!(stats.compression_ratio < 1.0); // Should be compressed
    }

    #[tokio::test]
    async fn test_compression_stats_reset() {
        let compressor = MessageCompressor::default();

        // Compress a message to create stats
        let mut message = create_test_message(create_large_payload(2));
        compressor.compress_message(&mut message).await.unwrap();

        // Verify stats exist
        let stats = compressor.get_stats().await;
        assert_eq!(stats.total_messages, 1);

        // Reset stats
        compressor.reset_stats().await;

        // Verify stats are reset
        let stats = compressor.get_stats().await;
        assert_eq!(stats.total_messages, 0);
        assert_eq!(stats.compressed_messages, 0);
        assert_eq!(stats.original_bytes, 0);
        assert_eq!(stats.compressed_bytes, 0);
    }

    #[tokio::test]
    async fn test_compression_stats_decompression() {
        let compressor = MessageCompressor::default();

        let mut message = create_test_message(create_large_payload(2));

        // Compress then decompress
        compressor.compress_message(&mut message).await.unwrap();
        compressor.decompress_message(&mut message).await.unwrap();

        // Check both compression and decompression stats
        let stats = compressor.get_stats().await;
        assert_eq!(stats.compressed_messages, 1);
        assert_eq!(stats.decompressed_messages, 1);
        assert!(stats.compression_time_ms > 0);
        assert!(stats.decompression_time_ms > 0);
    }

    #[tokio::test]
    async fn test_compressed_message_builder() {
        let compressor = std::sync::Arc::new(MessageCompressor::default());
        let builder = CompressedMessageBuilder::new(compressor);

        let payload = create_large_payload(2);
        let message = builder
            .id("test-builder-123".to_string())
            .payload(payload.clone())
            .priority(QueuePriority::High)
            .metadata("test_key".to_string(), json!("test_value"))
            .build()
            .await
            .unwrap();

        assert_eq!(message.id, "test-builder-123");
        assert_eq!(message.priority, QueuePriority::High);
        assert!(message.payload.is_string()); // Should be compressed
        assert_eq!(message.metadata["test_key"], json!("test_value"));
        assert!(message.metadata.contains_key("compression_algorithm"));
    }

    #[tokio::test]
    async fn test_estimate_compression_ratio() {
        let data = b"Hello, World! Hello, World! Hello, World!";

        // GZIP compression ratio
        let gzip_ratio = utils::estimate_compression_ratio(data, CompressionAlgorithm::Gzip).await.unwrap();
        assert!(gzip_ratio <= 1.0);

        // No compression ratio
        let none_ratio = utils::estimate_compression_ratio(data, CompressionAlgorithm::None).await.unwrap();
        assert_eq!(none_ratio, 1.0);
    }

    #[tokio::test]
    async fn test_test_compression() {
        let payload = create_large_payload(1);
        let results = utils::test_compression(&payload).await;

        assert!(!results.is_empty());

        // Results should be sorted by compression ratio (best first)
        if results.len() > 1 {
            assert!(results[0].1 <= results[1].1);
        }
    }

    #[tokio::test]
    async fn test_get_compression_recommendations() {
        // Small payload recommendation
        let small_config = utils::get_compression_recommendations(512);
        assert_eq!(small_config.algorithm, CompressionAlgorithm::None);

        // Medium payload recommendation
        let medium_config = utils::get_compression_recommendations(2048);
        assert_eq!(medium_config.algorithm, CompressionAlgorithm::Gzip);

        // Large payload recommendation
        let large_config = utils::get_compression_recommendations(50000);
        assert_eq!(large_config.algorithm, CompressionAlgorithm::Zlib);
        assert_eq!(large_config.level, 9);
    }

    #[tokio::test]
    async fn test_compression_levels() {
        for level in 1..=9 {
            let config = CompressionConfig {
                algorithm: CompressionAlgorithm::Gzip,
                level,
                min_size: 100,
                force_compression: false,
                max_attempts: 3,
            };

            let compressor = MessageCompressor::new(config);
            let mut message = create_test_message(create_large_payload(2));

            let result = compressor.compress_message(&mut message).await;
            assert!(result.is_ok(), "Compression should succeed at level {}", level);

            assert!(message.metadata.contains_key("compression_algorithm"));
        }
    }

    #[tokio::test]
    async fn test_highly_compressible_data() {
        let highly_repetitive = "A".repeat(10000); // 10KB of the same character
        let payload = json!({"data": highly_repetitive});

        let compressor = MessageCompressor::default();
        let mut message = create_test_message(payload);

        compressor.compress_message(&mut message).await.unwrap();

        let original_size = message.metadata["compression_original_size"].as_u64().unwrap() as usize;
        let compressed_size = message.metadata["compression_compressed_size"].as_u64().unwrap() as usize;

        // Should achieve very good compression ratio
        let compression_ratio = compressed_size as f64 / original_size as f64;
        assert!(compression_ratio < 0.1, "Highly repetitive data should compress well, got ratio: {}", compression_ratio);
    }

    #[tokio::test]
    async fn test_less_compressible_data() {
        // Random-like data (base64 encoded random bytes)
        let random_data = "H4sICAAAAAAC/0tJLAEAq1YqAMAaAAAA".repeat(200); // 20KB of base64
        let payload = json!({"data": random_data});

        let compressor = MessageCompressor::default();
        let mut message = create_test_message(payload);

        compressor.compress_message(&mut message).await.unwrap();

        let original_size = message.metadata["compression_original_size"].as_u64().unwrap() as usize;
        let compressed_size = message.metadata["compression_compressed_size"].as_u64().unwrap() as usize;

        // Base64 data should not compress well, might even expand
        let compression_ratio = compressed_size as f64 / original_size as f64;
        assert!(compression_ratio > 0.5, "Base64 data should not compress well, got ratio: {}", compression_ratio);
    }

    #[tokio::test]
    async fn test_invalid_compressed_data() {
        let compressor = MessageCompressor::default();
        let mut message = create_test_message(json!({"test": "message"}));

        // Mark as compressed but provide invalid data
        message.metadata.insert(
            "compression_algorithm".to_string(),
            json!("gzip")
        );
        message.metadata.insert(
            "compression_original_size".to_string(),
            json!(100)
        );
        message.payload = json!("invalid_compressed_data");

        let result = compressor.decompress_message(&mut message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_size_mismatch_decompression() {
        let compressor = MessageCompressor::default();
        let mut message = create_test_message(create_large_payload(2));

        // Compress the message first
        compressor.compress_message(&mut message).await.unwrap();

        // Tamper with original size metadata
        message.metadata.insert(
            "compression_original_size".to_string(),
            json!(999999) // Wrong size
        );

        let result = compressor.decompress_message(&mut message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_concurrent_compression() {
        let compressor = std::sync::Arc::new(MessageCompressor::default());
        let mut handles = Vec::new();

        // Spawn multiple concurrent compression tasks
        for i in 0..10 {
            let compressor_clone = compressor.clone();
            let handle = tokio::spawn(async move {
                let mut message = create_test_message(create_large_payload(1));
                message.id = format!("concurrent-test-{}", i);

                let start = std::time::Instant::now();
                let result = compressor_clone.compress_message(&mut message).await;
                let duration = start.elapsed();

                (result, duration, message.metadata.clone())
            });

            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            let (result, duration, metadata) = handle.await.unwrap();
            assert!(result.is_ok());
            assert!(metadata.contains_key("compression_algorithm"));
            assert!(duration.as_millis() > 0);
        }

        // Verify statistics
        let stats = compressor.get_stats().await;
        assert_eq!(stats.compressed_messages, 10);
    }

    #[test]
    fn test_compression_error_display() {
        let error = CompressionError::UnsupportedAlgorithm("xyz".to_string());
        assert!(error.to_string().contains("Unsupported compression algorithm"));

        let error = CompressionError::CompressionFailed("disk full".to_string());
        assert!(error.to_string().contains("Compression failed"));

        let error = CompressionError::DecompressionFailed("invalid data".to_string());
        assert!(error.to_string().contains("Decompression failed"));

        let error = CompressionError::NotBeneficial { ratio: 1.5 };
        assert!(error.to_string().contains("Compression ratio not beneficial"));
    }

    #[test]
    fn test_compression_error_source() {
        // Test that errors can be converted to QueueError
        let compression_error = CompressionError::UnsupportedAlgorithm("test".to_string());
        let queue_error = QueueError::Serialization(compression_error.to_string());

        assert!(queue_error.to_string().contains("Unsupported compression algorithm"));
    }
}