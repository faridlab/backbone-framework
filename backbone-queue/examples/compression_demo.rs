//! Message Compression Demo
//!
//! This example demonstrates how to use the message compression functionality
//! to optimize storage and network transfer of large messages.

use backbone_queue::{
    QueueService,
    redis::RedisQueueBuilder,
    compression::{MessageCompressor, CompressionConfig, CompressionAlgorithm, CompressedMessageBuilder},
    types::{QueueMessage, QueuePriority}
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use serde_json::json;

/// Create large test payload
fn create_large_payload(size_mb: usize) -> serde_json::Value {
    let base_data = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. ";
    let mut data = String::new();

    let target_size = size_mb * 1024 * 1024; // MB to bytes
    while data.len() < target_size {
        data.push_str(base_data);
    }

    json!({
        "id": "large-document-123",
        "title": "Large Document",
        "content": data,
        "metadata": {
            "size_mb": size_mb,
            "type": "document",
            "compression_test": true,
            "created_at": chrono::Utc::now()
        },
        "attachments": vec![
            {
                "name": "file1.pdf",
                "size": 2048576,
                "type": "application/pdf"
            },
            {
                "name": "image1.png",
                "size": 1024000,
                "type": "image/png"
            }
        ]
    })
}

/// Create small test payload
fn create_small_payload() -> serde_json::Value {
    json!({
        "id": "small-message-456",
        "text": "Hello, World!",
        "type": "notification",
        "timestamp": chrono::Utc::now()
    })
}

/// Benchmark compression performance
async fn benchmark_compression() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔧 Compression Benchmark");
    println!("=======================");

    let mut results = HashMap::new();

    // Test different payload sizes
    let sizes = vec![1, 5, 10, 25]; // MB

    for size_mb in sizes {
        println!("\n📊 Testing {}MB payload:", size_mb);

        let payload = create_large_payload(size_mb);
        let raw_size = serde_json::to_vec(&payload).unwrap().len();

        // Test GZIP compression
        {
            let config = CompressionConfig {
                algorithm: CompressionAlgorithm::Gzip,
                level: 6,
                min_size: 0,
                force_compression: true,
                max_attempts: 3,
            };
            let compressor = MessageCompressor::new(config);

            let mut message = QueueMessage {
                id: format!("gzip-test-{}mb", size_mb),
                payload: payload.clone(),
                priority: QueuePriority::Normal,
                created_at: chrono::Utc::now(),
                expires_at: None,
                receive_count: 0,
                max_receive_count: 3,
                visibility_timeout: 30,
                delay_until: None,
                metadata: HashMap::new(),
            };

            let start = Instant::now();
            compressor.compress_message(&mut message).await?;
            let compression_time = start.elapsed();

            let compressed_size = message.metadata["compression_compressed_size"]
                .as_u64().unwrap() as usize;
            let compression_ratio = compressed_size as f64 / raw_size as f64;

            println!("  📦 GZIP:  {} -> {} bytes ({:.1}% compression) in {:?}",
                raw_size, compressed_size, compression_ratio * 100.0, compression_time);

            results.insert(format!("gzip_{}mb", size_mb),
                (compressed_size, compression_ratio, compression_time));
        }

        // Test ZLIB compression
        {
            let config = CompressionConfig {
                algorithm: CompressionAlgorithm::Zlib,
                level: 6,
                min_size: 0,
                force_compression: true,
                max_attempts: 3,
            };
            let compressor = MessageCompressor::new(config);

            let mut message = QueueMessage {
                id: format!("zlib-test-{}mb", size_mb),
                payload: payload.clone(),
                priority: QueuePriority::Normal,
                created_at: chrono::Utc::now(),
                expires_at: None,
                receive_count: 0,
                max_receive_count: 3,
                visibility_timeout: 30,
                delay_until: None,
                metadata: HashMap::new(),
            };

            let start = Instant::now();
            compressor.compress_message(&mut message).await?;
            let compression_time = start.elapsed();

            let compressed_size = message.metadata["compression_compressed_size"]
                .as_u64().unwrap() as usize;
            let compression_ratio = compressed_size as f64 / raw_size as f64;

            println!("  📦 ZLIB:  {} -> {} bytes ({:.1}% compression) in {:?}",
                raw_size, compressed_size, compression_ratio * 100.0, compression_time);

            results.insert(format!("zlib_{}mb", size_mb),
                (compressed_size, compression_ratio, compression_time));
        }

        // Test different compression levels
        println!("  🔧 GZIP Compression Levels:");
        for level in [1, 3, 6, 9] {
            let config = CompressionConfig {
                algorithm: CompressionAlgorithm::Gzip,
                level,
                min_size: 0,
                force_compression: true,
                max_attempts: 3,
            };
            let compressor = MessageCompressor::new(config);

            let mut message = QueueMessage {
                id: format!("gzip-lvl{}-{}mb", level, size_mb),
                payload: payload.clone(),
                priority: QueuePriority::Normal,
                created_at: chrono::Utc::now(),
                expires_at: None,
                receive_count: 0,
                max_receive_count: 3,
                visibility_timeout: 30,
                delay_until: None,
                metadata: HashMap::new(),
            };

            let start = Instant::now();
            compressor.compress_message(&mut message).await?;
            let compression_time = start.elapsed();

            let compressed_size = message.metadata["compression_compressed_size"]
                .as_u64().unwrap() as usize;
            let compression_ratio = compressed_size as f64 / raw_size as f64;

            println!("    Level {}: {} bytes ({:.1}% ratio) in {:?}",
                level, compressed_size, compression_ratio * 100.0, compression_time);
        }
    }

    // Find best compression for each size
    println!("\n🏆 Best Compression Results:");
    for size_mb in sizes {
        let gzip_key = format!("gzip_{}mb", size_mb);
        let zlib_key = format!("zlib_{}mb", size_mb);

        if let (Some(gzip_result), Some(zlib_result)) = (results.get(&gzip_key), results.get(&zlib_key)) {
            let gzip_ratio = gzip_result.1;
            let zlib_ratio = zlib_result.1;

            if gzip_ratio < zlib_ratio {
                println!("  {}MB: GZIP wins ({:.1}% vs {:.1}%)",
                    size_mb, gzip_ratio * 100.0, zlib_ratio * 100.0);
            } else {
                println!("  {}MB: ZLIB wins ({:.1}% vs {:.1}%)",
                    size_mb, zlib_ratio * 100.0, gzip_ratio * 100.0);
            }
        }
    }

    Ok(())
}

/// Test automatic compression thresholds
async fn test_automatic_compression() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🤖 Automatic Compression Test");
    println!("===============================");

    let config = CompressionConfig {
        algorithm: CompressionAlgorithm::Gzip,
        level: 6,
        min_size: 1024, // 1KB threshold
        force_compression: false,
        max_attempts: 3,
    };

    let compressor = MessageCompressor::new(config);

    // Test small message (should not be compressed)
    {
        let mut small_message = QueueMessage {
            id: "small-test".to_string(),
            payload: create_small_payload(),
            priority: QueuePriority::Normal,
            created_at: chrono::Utc::now(),
            expires_at: None,
            receive_count: 0,
            max_receive_count: 3,
            visibility_timeout: 30,
            delay_until: None,
            metadata: HashMap::new(),
        };

        let original_payload = small_message.payload.clone();
        compressor.compress_message(&mut small_message).await?;

        println!("📨 Small message: {} -> {} bytes",
            serde_json::to_vec(&original_payload).unwrap().len(),
            serde_json::to_vec(&small_message.payload).unwrap().len());

        if small_message.metadata.contains_key("compression_algorithm") {
            println!("  ❌ Unexpectedly compressed");
        } else {
            println!("  ✅ Correctly not compressed (below threshold)");
        }
    }

    // Test large message (should be compressed)
    {
        let mut large_message = QueueMessage {
            id: "large-test".to_string(),
            payload: create_large_payload(2),
            priority: QueuePriority::Normal,
            created_at: chrono::Utc::now(),
            expires_at: None,
            receive_count: 0,
            max_receive_count: 3,
            visibility_timeout: 30,
            delay_until: None,
            metadata: HashMap::new(),
        };

        let original_size = serde_json::to_vec(&large_message.payload).unwrap().len();
        compressor.compress_message(&mut large_message).await?;

        let compressed_size = serde_json::to_vec(&large_message.payload).unwrap().len();

        println!("📦 Large message: {} -> {} bytes", original_size, compressed_size);

        if large_message.metadata.contains_key("compression_algorithm") {
            let ratio = compressed_size as f64 / original_size as f64;
            println!("  ✅ Correctly compressed ({:.1}% of original)", ratio * 100.0);
        } else {
            println!("  ❌ Unexpectedly not compressed");
        }
    }

    Ok(())
}

/// Test compression roundtrip
async fn test_compression_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔄 Compression Roundtrip Test");
    println!("=============================");

    let compressor = Arc::new(MessageCompressor::default());

    let test_cases = vec![
        ("small_json", create_small_payload()),
        ("large_text", create_large_payload(1)),
        ("mixed_data", json!({
            "text": "Hello, World!".repeat(1000),
            "numbers": vec![1i32; 10000],
            "nested": {
                "data": "test".repeat(5000),
                "array": vec![json!({"item": "value"}); 1000]
            }
        })),
    ];

    for (name, payload) in test_cases {
        println!("\n📋 Testing case: {}", name);

        // Using CompressedMessageBuilder
        let builder = CompressedMessageBuilder::new(compressor.clone())
            .id(format!("roundtrip-{}", name))
            .payload(payload.clone())
            .priority(QueuePriority::High)
            .metadata("test_case".to_string(), json!(name));

        let mut compressed_message = builder.build().await?;
        println!("  ✅ Message compressed");

        // Verify compression metadata
        assert!(compressed_message.metadata.contains_key("compression_algorithm"));
        assert!(compressed_message.metadata.contains_key("compression_original_size"));
        assert!(compressed_message.metadata.contains_key("compression_compressed_size"));

        let original_size = compressed_message.metadata["compression_original_size"]
            .as_u64().unwrap() as usize;
        let compressed_size = compressed_message.metadata["compression_compressed_size"]
            .as_u64().unwrap() as usize;

        println!("  📊 Size: {} -> {} bytes ({:.1}% ratio)",
            original_size, compressed_size, (compressed_size as f64 / original_size as f64) * 100.0);

        // Decompress
        compressor.decompress_message(&mut compressed_message).await?;
        println!("  ✅ Message decompressed");

        // Verify data integrity
        let decompressed_payload = compressed_message.payload;
        if decompressed_payload == payload {
            println!("  ✅ Data integrity verified");
        } else {
            println!("  ❌ Data integrity check failed");
            return Err("Data integrity check failed".into());
        }

        // Verify metadata cleanup
        assert!(!compressed_message.metadata.contains_key("compression_algorithm"));
    }

    Ok(())
}

/// Test compression statistics
async fn test_compression_stats() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📈 Compression Statistics Test");
    println!("=============================");

    let compressor = MessageCompressor::default();

    let mut messages = Vec::new();

    // Create and compress multiple messages
    for i in 0..10 {
        let mut message = QueueMessage {
            id: format!("stats-test-{}", i),
            payload: create_large_payload(1), // 1MB each
            priority: QueuePriority::Normal,
            created_at: chrono::Utc::now(),
            expires_at: None,
            receive_count: 0,
            max_receive_count: 3,
            visibility_timeout: 30,
            delay_until: None,
            metadata: HashMap::new(),
        };

        compressor.compress_message(&mut message).await?;
        messages.push(message);
    }

    // Get statistics
    let stats = compressor.get_stats().await;
    println!("📊 Compression Statistics:");
    println!("  Total messages: {}", stats.total_messages);
    println!("  Compressed messages: {}", stats.compressed_messages);
    println!("  Original bytes: {}", stats.original_bytes);
    println!("  Compressed bytes: {}", stats.compressed_bytes);
    println!("  Compression ratio: {:.3}", stats.compression_ratio);
    println!("  Avg compression time: {:.2}ms", stats.avg_compression_time_ms());

    let space_saved = stats.original_bytes - stats.compressed_bytes;
    println!("  Space saved: {} bytes ({:.1}%)",
        space_saved, (space_saved as f64 / stats.original_bytes as f64) * 100.0);

    // Decompress half the messages and check stats
    for message in messages.iter().take(5) {
        let mut message_clone = message.clone();
        compressor.decompress_message(&mut message_clone).await?;
    }

    let final_stats = compressor.get_stats().await;
    println!("  Decompressed messages: {}", final_stats.decompressed_messages);
    println!("  Avg decompression time: {:.2}ms", final_stats.avg_decompression_time_ms());

    Ok(())
}

/// Integration with Redis queue
async fn test_redis_integration() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔗 Redis Integration Test");
    println!("=========================");

    // Create Redis queue
    let queue = RedisQueueBuilder::new()
        .url("redis://localhost:6379")
        .queue_name("compression_test_queue")
        .key_prefix("compression")
        .build()
        .await?;

    // Test connection
    if !queue.test_connection().await? {
        println!("❌ Failed to connect to Redis, skipping integration test");
        return Ok(());
    }

    // Clear queue
    queue.purge().await?;

    // Create compressor
    let compressor = Arc::new(MessageCompressor::default());

    // Enqueue compressed message
    let large_payload = create_large_payload(2);
    let builder = CompressedMessageBuilder::new(compressor.clone())
        .id("redis-compression-test".to_string())
        .payload(large_payload)
        .priority(QueuePriority::High);

    let compressed_message = builder.build().await?;
    println!("✅ Created compressed message");

    let message_id = queue.enqueue(compressed_message).await?;
    println!("✅ Enqueued compressed message: {}", message_id);

    // Dequeue and decompress
    if let Some(mut message) = queue.dequeue().await? {
        println!("✅ Dequeued message");

        // Check if it's compressed
        if message.metadata.contains_key("compression_algorithm") {
            println!("📦 Message is compressed, decompressing...");
            compressor.decompress_message(&mut message).await?;
            println!("✅ Message decompressed successfully");

            // Verify payload is restored
            if let Some(content) = message.payload.get("content") {
                if content.is_string() && content.as_str().unwrap().len() > 1000000 {
                    println!("✅ Large content restored ({} chars)", content.as_str().unwrap().len());
                }
            }
        }

        queue.ack(&message.id).await?;
        println!("✅ Message acknowledged");
    }

    // Cleanup
    queue.purge().await?;
    println!("🧹 Queue cleaned up");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🚀 Message Compression Demo");
    println!("==========================");

    // Run compression benchmarks
    benchmark_compression().await?;

    // Test automatic compression
    test_automatic_compression().await?;

    // Test compression roundtrip
    test_compression_roundtrip().await?;

    // Test compression statistics
    test_compression_stats().await?;

    // Test Redis integration (if available)
    test_redis_integration().await?;

    println!("\n🎉 Compression demo completed!");

    println!("\n📚 Key Takeaways:");
    println!("  • Compression is most effective for repetitive/large data");
    println!("  • GZIP provides good balance of speed and ratio");
    println!("  • ZLIB offers slightly better compression for some data");
    println!("  • Automatic thresholds prevent unnecessary compression");
    println!("  • Statistics help monitor compression efficiency");
    println!("  • Integration with queue operations is seamless");

    Ok(())
}