//! Basic usage example for Backbone Storage
//!
//! This example demonstrates how to use the storage module with different backends
//! and perform common file operations.

use backbone_storage::*;
use std::collections::HashMap;
use tokio::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🗄️  Backbone Storage - Basic Usage Example\n");

    // Example 1: Using environment configuration
    println!("📦 Example 1: Storage from Environment");
    example_from_env().await?;

    println!("\n" + "=".repeat(50).as_str() + "\n");

    // Example 2: Local storage with custom configuration
    println!("📂 Example 2: Local Storage");
    example_local_storage().await?;

    println!("\n" + "=".repeat(50).as_str() + "\n");

    // Example 3: File compression demonstration
    println!("🗜️  Example 3: File Compression");
    example_compression().await?;

    println!("\n" + "=".repeat(50).as_str() + "\n");

    // Example 4: Security scanning demonstration
    println!("🔒 Example 4: Security Scanning");
    example_security_scanning().await?;

    println!("\n✅ All examples completed successfully!");
    Ok(())
}

/// Example 1: Using storage configured from environment variables
async fn example_from_env() -> Result<(), Box<dyn std::error::Error>> {
    // Set environment variables for demonstration
    std::env::set_var("STORAGE_BACKEND", "local");
    std::env::set_var("LOCAL_STORAGE_BASE_DIR", "/tmp/backbone_demo");
    std::env::set_var("COMPRESSION_ENABLED", "true");
    std::env::set_var("SECURITY_ENABLED", "true");

    // Create storage service from environment
    let storage = from_env().await?;

    println!("✅ Storage service created from environment variables");

    // Upload a simple text file
    let content = b"Hello, Backbone Storage! This is a test file.";
    let upload_result = storage.upload_bytes("demo/hello.txt", content, "text/plain").await?;

    println!("📄 Uploaded file: {}", upload_result.file_path);
    println!("📏 File size: {} bytes", upload_result.size);
    println!("🔒 Security analysis: {:?}", upload_result.security_analysis);

    // Download the file back
    let downloaded_content = storage.download_bytes("demo/hello.txt").await?;

    println!("✅ Downloaded {} bytes", downloaded_content.len());
    assert_eq!(content, &downloaded_content[..]);

    // Get file metadata
    let metadata = storage.get_file_metadata("demo/hello.txt").await?;
    println!("📋 File metadata: {} ({} bytes)", metadata.file_path, metadata.size);

    // List files in demo directory
    let files = storage.list_files("demo/", None).await?;
    println!("📁 Files in demo directory: {}", files.files.len());

    Ok(())
}

/// Example 2: Using local storage with custom configuration
async fn example_local_storage() -> Result<(), Box<dyn std::error::Error>> {
    // Create temporary directory for demonstration
    let temp_dir = "/tmp/backbone_local_demo";
    fs::create_dir_all(temp_dir).await?;

    let config = LocalStorageConfig {
        base_dir: temp_dir.into(),
        create_dirs: true,
        max_file_size: 10 * 1024 * 1024, // 10MB
        enable_compression: true,
        enable_security_scan: true,
        ..Default::default()
    };

    let storage = LocalStorage::new(config)?;
    println!("✅ Local storage created with base directory: {}", temp_dir);

    // Upload with custom options
    let mut custom_metadata = HashMap::new();
    custom_metadata.insert("author".to_string(), "Demo User".to_string());
    custom_metadata.insert("purpose".to_string(), "Testing".to_string());

    let upload_options = UploadOptions {
        content_type: Some("application/json".to_string()),
        cache_control: Some("max-age=3600".to_string()),
        metadata: Some(custom_metadata),
        enable_compression: true,
        enable_security_scan: true,
    };

    let json_content = r#"{
        "name": "Backbone Storage",
        "version": "2.0.0",
        "features": ["compression", "security", "multi-backend"]
    }"#;

    let upload_result = storage
        .upload_with_options("demo/config.json", json_content.as_bytes(), upload_options)
        .await?;

    println!("📄 Uploaded JSON file with custom options");
    println!("📏 Original size: {} bytes", json_content.len());
    println!("📏 Stored size: {} bytes", upload_result.size);

    if upload_result.compressed {
        println!("🗜️  File was compressed (ratio: {:.1}%)",
            upload_result.compression_ratio.unwrap_or(0.0) * 100.0);
    }

    // Generate presigned URL (for local storage, this creates a temporary access path)
    let url_options = PresignedUrlOptions {
        expiry_seconds: 300, // 5 minutes
        method: HttpMethod::GET,
    };

    let presigned_url = storage.generate_presigned_download_url("demo/config.json", url_options).await?;
    println!("🔗 Presigned URL generated: {}", presigned_url);

    Ok(())
}

/// Example 3: Demonstrate file compression capabilities
async fn example_compression() -> Result<(), Box<dyn std::error::Error>> {
    let storage = LocalStorage::new(LocalStorageConfig::default())?;
    let compression_engine = CompressionEngine::new(CompressionConfig::default());

    println!("✅ Compression engine initialized");

    // Test text file compression
    let large_text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(1000);
    let text_bytes = large_text.as_bytes();

    let compression_result = compression_engine.compress_file(text_bytes, "document.txt").await?;

    println!("📄 Text compression:");
    println!("   Original size: {} bytes", compression_result.original_size);
    println!("   Compressed size: {} bytes", compression_result.compressed_size);
    println!("   Compression ratio: {:.1}%", compression_result.compression_ratio * 100.0);
    println!("   Algorithm: {:?}", compression_result.algorithm);

    // Test JSON compression
    let json_data = r#"{"users":[{"id":1,"name":"Alice","email":"alice@example.com","roles":["admin","user"]},"#;
    let large_json = format!("{}{}", json_data, r#"[{"id":2,"name":"Bob","email":"bob@example.com","roles":["user"]}]"#).repeat(500);

    let json_compression = compression_engine.compress_file(large_json.as_bytes(), "data.json").await?;

    println!("📊 JSON compression:");
    println!("   Original size: {} bytes", json_compression.original_size);
    println!("   Compressed size: {} bytes", json_compression.compressed_size);
    println!("   Compression ratio: {:.1}%", json_compression.compression_ratio * 100.0);

    // Demonstrate decompression
    let decompressed_data = compression_engine.decompress_file(
        &compression_result.compressed_data,
        compression_result.algorithm
    ).await?;

    println!("✅ Decompression successful: {} bytes restored", decompressed_data.len());
    assert_eq!(text_bytes.len(), decompressed_data.len());

    Ok(())
}

/// Example 4: Demonstrate security scanning capabilities
async fn example_security_scanning() -> Result<(), Box<dyn std::error::Error>> {
    let storage = LocalStorage::new(LocalStorageConfig::default())?;
    let security_engine = SecurityEngine::new(SecurityConfig::default());

    println!("✅ Security engine initialized");

    // Scan a safe text file
    let safe_content = b"This is a safe text file with no executable content.";
    let safe_analysis = security_engine.scan_file(safe_content, "safe.txt").await?;

    println!("📄 Safe file analysis:");
    println!("   File hash: {}", safe_analysis.file_hash);
    println!("   Threat level: {:?}", safe_analysis.threat_level);
    println!("   Is executable: {}", safe_analysis.is_executable);
    println!("   Threats found: {}", safe_analysis.threats.len());
    println!("   Is safe: {}", security_engine.is_file_safe(&safe_analysis));

    // Create a suspicious executable-like file for demonstration
    let suspicious_content = b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xff\xff\x00\x00";
    let suspicious_analysis = security_engine.scan_file(suspicious_content, "suspicious.exe").await?;

    println!("🚨 Suspicious file analysis:");
    println!("   File hash: {}", suspicious_analysis.file_hash);
    println!("   Threat level: {:?}", suspicious_analysis.threat_level);
    println!("   Is executable: {}", suspicious_analysis.is_executable);
    println!("   Threats found: {}", suspicious_analysis.threats.len());
    println!("   Is safe: {}", security_engine.is_file_safe(&suspicious_analysis));

    // Display threat details if any were found
    if !suspicious_analysis.threats.is_empty() {
        println!("   Threat details:");
        for threat in &suspicious_analysis.threats {
            println!("     - {}: {} (severity: {:?})",
                threat.threat_type, threat.description, threat.severity);
        }
    }

    // Test executable metadata extraction (if detected as executable)
    if let Some(metadata) = &suspicious_analysis.executable_metadata {
        println!("🔍 Executable metadata:");
        println!("   File type: {:?}", metadata.file_type);
        println!("   Architecture: {:?}", metadata.architecture);
        if let Some(section_count) = metadata.section_count {
            println!("   Sections: {}", section_count);
        }
    }

    // Demonstrate trying to upload a malicious file (will be blocked)
    if !security_engine.is_file_safe(&suspicious_analysis) {
        println!("🚫 Upload blocked: File contains security threats");

        // This would fail in a real scenario:
        // storage.upload_bytes("malware.exe", suspicious_content, "application/octet-stream").await?;
    }

    Ok(())
}