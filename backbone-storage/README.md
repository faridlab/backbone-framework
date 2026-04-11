# Backbone Storage

🦴 **Production-ready local filesystem storage with comprehensive file management**

Backbone Storage provides a robust, feature-rich local filesystem storage solution with support for file uploads, downloads, metadata management, streaming, and security features. Designed for production use with comprehensive error handling and async support.

## 📋 Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Usage](#usage)
- [Configuration](#configuration)
- [Technical Details](#technical-details)
- [Examples](#examples)
- [Error Handling](#error-handling)
- [Testing](#testing)

## 🎯 Overview

Backbone Storage provides a complete file storage solution focused on local filesystem operations. It includes all essential storage features like file uploads, downloads, metadata management, directory traversal, and security scanning capabilities.

### Key Design Principles

- **Local First**: Optimized for reliable local filesystem storage
- **Async Native**: Full async/await support with tokio
- **Security Aware**: Built-in virus scanning and file validation
- **Metadata Rich**: Comprehensive file metadata and statistics
- **Production Ready**: Robust error handling and recovery

## 🚀 Features

### 📁 **Core Storage Features**
- **File Upload/Download**: Support for bytes, readers, and file uploads
- **Streaming**: Async file streaming with range support
- **Metadata Management**: Rich file metadata with timestamps and hashes
- **Directory Operations**: File listing with pagination and filtering
- **File Operations**: Copy, move, delete with proper error handling

### 🛡️ **Security & Safety**
- **Path Sanitization**: Prevents directory traversal attacks
- **File Type Detection**: MIME type detection and validation
- **Hash Verification**: File integrity checking
- **Virus Scanning**: Integration with security analysis (optional)
- **Permission Validation**: Filesystem permission checking

### 📊 **Advanced Features**
- **Compression**: Built-in file compression support (optional)
- **Presigned URLs**: Generate file access URLs
- **Statistics**: Storage usage and file count analytics
- **Builder Pattern**: Fluent configuration API
- **Generic Trait**: Extensible StorageService trait

## 📖 Usage

### Quick Start

```toml
[dependencies]
backbone-storage = "2.0.0"
tokio = { version = "1.0", features = ["full"] }
bytes = "1.0"
serde = { version = "1.0", features = ["derive"] }
uuid = { version = "1.0", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
```

### Basic File Operations

```rust
use backbone_storage::{StorageService, LocalStorage};
use bytes::Bytes;

// Create local storage service
let storage = LocalStorage::new(config)?;

// Upload file from bytes
let file_data = Bytes::from("Hello, Backbone Storage!");
let uploaded_file = storage.upload_bytes("greeting.txt", file_data, None).await?;
println!("File uploaded: {}", uploaded_file.name);

// Download file as bytes
let downloaded_data = storage.download_bytes("greeting.txt", None).await?;
println!("File content: {}", String::from_utf8_lossy(&downloaded_data));

// Check if file exists
let exists = storage.file_exists("greeting.txt").await?;
println!("File exists: {}", exists);
```

### File Upload from Reader

```rust
use backbone_storage::{StorageService, LocalStorage};
use tokio::fs::File;
use tokio::io::BufReader;

// Create storage service
let storage = LocalStorage::new(config)?;

// Upload file from async reader
let file = File::open("large_file.zip").await?;
let reader = Box::new(BufReader::new(file));

let uploaded_file = storage.upload_reader(
    "uploads/large_file.zip",
    reader,
    Some(1024 * 1024 * 50), // 50MB
    None
).await?;

println!("Large file uploaded: {} ({} bytes)",
         uploaded_file.name, uploaded_file.size);
```

### File Listing and Metadata

```rust
use backbone_storage::{StorageService, LocalStorage};

// Create storage service
let storage = LocalStorage::new(config)?;

// List files in directory
let files = storage.list_files("", Some(20), None).await?;
println!("Found {} files:", files.total_count.unwrap_or(0));

for file in &files.files {
    println!("  {} - {} bytes ({})",
             file.name, file.size, file.content_type);
}

// Get specific file metadata
let file = storage.get_file("documents/report.pdf").await?;
println!("File: {}", file.name);
println!("  Size: {} bytes", file.size);
println!("  Type: {}", file.content_type);
println!("  Created: {}", file.created_at);
println!("  Modified: {:?}", file.updated_at);
println!("  Checksum: {}", file.checksum.as_ref().unwrap_or(&"N/A".to_string()));
```

### File Streaming

```rust
use backbone_storage::{StorageService, LocalStorage};
use futures::StreamExt;

// Create storage service
let storage = LocalStorage::new(config)?;

// Stream file content
let mut stream = storage.stream_file("large_video.mp4", None).await?;

// Process stream in chunks
let mut total_bytes = 0;
while let Some(chunk_result) = stream.next().await {
    match chunk_result {
        Ok(chunk) => {
            total_bytes += chunk.len();
            println!("Received {} bytes (total: {})", chunk.len(), total_bytes);
            // Process chunk...
        }
        Err(e) => {
            eprintln!("Stream error: {}", e);
            break;
        }
    }
}

println!("Streamed {} bytes total", total_bytes);
```

### File Operations

```rust
use backbone_storage::{StorageService, LocalStorage};

// Create storage service
let storage = LocalStorage::new(config)?;

// Copy file
let copied_file = storage.copy_file(
    "source/document.pdf",
    "backup/document_copy.pdf"
).await?;
println!("File copied: {}", copied_file.name);

// Move file
let moved_file = storage.move_file(
    "temp/upload.tmp",
    "documents/final.pdf"
).await?;
println!("File moved: {}", moved_file.name);

// Delete file
let deleted = storage.delete_file("old_file.txt").await?;
println!("File deleted: {}", deleted);
```

### Storage Statistics

```rust
use backbone_storage::{StorageService, LocalStorage};

// Create storage service
let storage = LocalStorage::new(config)?;

// Get storage statistics
let stats = storage.get_stats().await?;
println!("Storage Statistics:");
println!("  Total files: {}", stats.file_count);
println!("  Total size: {} bytes", stats.total_bytes);
println!("  Average file size: {:.2} bytes", stats.average_file_size);
println!("  Last updated: {}", stats.last_updated);

// Backend usage
for (backend, bytes) in &stats.backend_usage {
    println!("  {}: {} bytes", backend, bytes);
}

// Content type distribution
for (content_type, count) in &stats.content_type_counts {
    println!("  {}: {} files", content_type, count);
}
```

## 🔧 Configuration

### Basic Configuration

```rust
use backbone_storage::{LocalStorage, LocalStorageConfig};

// Simple configuration
let config = LocalStorageConfig {
    base_dir: "./storage".to_string(),
    base_path: None,
    compression: false,
    encryption: None,
};

let storage = LocalStorage::new(config)?;
```

### Builder Pattern Configuration

```rust
use backbone_storage::{LocalStorage, LocalStorageBuilder};

// Advanced configuration with builder
let storage = LocalStorage::builder()
    .base_dir("./app_storage")
    .compression(true)
    .build()?;
```

### Directory Structure

```
./storage/
├── uploads/
│   ├── documents/
│   │   ├── report.pdf
│   │   └── invoice.docx
│   ├── images/
│   │   ├── logo.png
│   │   └── banner.jpg
│   └── temp/
│       └── upload.tmp
├── backups/
└── archives/
```

## 🔧 Technical Details

### Core Traits

#### StorageService Trait

```rust
#[async_trait]
pub trait StorageService: Send + Sync {
    // Upload operations
    async fn upload_bytes(&self, path: &str, data: Bytes, options: Option<UploadOptions>) -> StorageResult<StorageFile>;
    async fn upload_reader(&self, path: &str, reader: Box<dyn AsyncRead + Send + Unpin>, size: Option<u64>, options: Option<UploadOptions>) -> StorageResult<StorageFile>;
    async fn upload_file(&self, path: &str, local_path: &str, options: Option<UploadOptions>) -> StorageResult<StorageFile>;

    // Download operations
    async fn download_bytes(&self, path: &str, options: Option<DownloadOptions>) -> StorageResult<Bytes>;
    async fn download_writer(&self, path: &str, writer: Box<dyn AsyncWrite + Send + Unpin>, options: Option<DownloadOptions>) -> StorageResult<()>;
    async fn download_file(&self, path: &str, local_path: &str, options: Option<DownloadOptions>) -> StorageResult<()>;
    async fn stream_file(&self, path: &str, range: Option<ByteRange>) -> StorageResult<Box<dyn Stream<Item = StorageResult<Bytes>> + Send + Unpin>>;

    // File operations
    async fn get_file(&self, path: &str) -> StorageResult<StorageFile>;
    async fn file_exists(&self, path: &str) -> StorageResult<bool>;
    async fn delete_file(&self, path: &str) -> StorageResult<bool>;
    async fn copy_file(&self, from_path: &str, to_path: &str) -> StorageResult<StorageFile>;
    async fn move_file(&self, from_path: &str, to_path: &str) -> StorageResult<StorageFile>;

    // Directory operations
    async fn list_files(&self, prefix: &str, limit: Option<u32>, continuation_token: Option<String>) -> StorageResult<FileListResult>;

    // Utility operations
    async fn generate_presigned_url(&self, path: &str, options: PresignedUrlOptions) -> StorageResult<String>;
    async fn get_stats(&self) -> StorageResult<StorageStats>;
    async fn test_connection(&self) -> StorageResult<bool>;
}
```

### Data Types

#### StorageFile
```rust
pub struct StorageFile {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub content_type: String,
    pub checksum: Option<String>,
    pub checksum_algorithm: Option<String>,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub version: Option<String>,
    pub backend: StorageBackend,
    pub bucket: String,
    pub storage_path: String,
    pub encrypted: bool,
    pub compressed: bool,
    pub expires_at: Option<DateTime<Utc>>,
}
```

#### StorageStats
```rust
pub struct StorageStats {
    pub file_count: u64,
    pub total_bytes: u64,
    pub backend_usage: HashMap<String, u64>,
    pub content_type_counts: HashMap<String, u64>,
    pub average_file_size: f64,
    pub largest_file_size: u64,
    pub last_updated: DateTime<Utc>,
}
```

### Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Configuration error: {0}")]
    InvalidConfiguration(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("File already exists: {0}")]
    FileAlreadyExists(String),

    #[error("Filesystem error: {0}")]
    FilesystemError(#[from] std::io::Error),

    #[error("File too large: {size} bytes, maximum allowed: {max_size} bytes")]
    FileTooLarge { size: u64, max_size: u64 },

    #[error("Invalid file format: {0}")]
    InvalidFileFormat(String),

    #[error("Security error: {operation} - {message}")]
    SecurityError { operation: String, message: String },

    #[error("Unsupported operation '{operation}' for backend '{backend}'")]
    UnsupportedOperation { operation: String, backend: String },
}
```

## 📚 Examples

### 1. File Upload Service

```rust
use backbone_storage::{StorageService, LocalStorage};
use tokio::fs::File;

async fn upload_user_file(
    storage: &dyn StorageService,
    user_id: &str,
    filename: &str,
    file_path: &str
) -> Result<backbone_storage::StorageFile, Box<dyn std::error::Error>> {
    // Sanitize and construct storage path
    let storage_path = format!("users/{}/files/{}", user_id, filename);

    // Upload file
    let uploaded_file = storage.upload_file(&storage_path, file_path, None).await?;

    println!("File uploaded successfully:");
    println!("  Original: {}", filename);
    println!("  Storage: {}", uploaded_file.storage_path);
    println!("  Size: {} bytes", uploaded_file.size);

    Ok(uploaded_file)
}
```

### 2. Backup Service

```rust
use backbone_storage::{StorageService, LocalStorage};

async fn create_backup(
    storage: &dyn StorageService,
    source_path: &str,
    backup_name: &str
) -> Result<backbone_storage::StorageFile, Box<dyn std::error::Error>> {
    // Generate backup timestamp
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_path = format!("backups/{}/{}_{}", backup_name, timestamp, source_path);

    // Copy file to backup location
    let backup_file = storage.copy_file(source_path, &backup_path).await?;

    println!("Backup created: {}", backup_file.storage_path);
    println!("Backup size: {} bytes", backup_file.size);

    Ok(backup_file)
}
```

### 3. File Processing Pipeline

```rust
use backbone_storage::{StorageService, LocalStorage};
use futures::StreamExt;

async fn process_uploaded_file(
    storage: &dyn StorageService,
    file_path: &str
) -> Result<(), Box<dyn std::error::Error>> {
    // Get file metadata
    let file = storage.get_file(file_path).await?;
    println!("Processing file: {} ({} bytes)", file.name, file.size);

    // Stream file for processing
    let mut stream = storage.stream_file(file_path, None).await?;
    let mut processed_bytes = 0;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                // Process chunk (e.g., scan for viruses, transform, etc.)
                processed_bytes += chunk.len();
                println!("Processed {} bytes", processed_bytes);
            }
            Err(e) => {
                eprintln!("Error processing file: {}", e);
                return Err(e.into());
            }
        }
    }

    // Move to processed directory
    let processed_path = file_path.replace("uploads/", "processed/");
    let final_file = storage.move_file(file_path, &processed_path).await?;

    println!("File processing complete: {}", final_file.storage_path);
    Ok(())
}
```

### 4. Storage Monitoring

```rust
use backbone_storage::StorageService;

async fn monitor_storage_usage(storage: &dyn StorageService) -> Result<(), Box<dyn std::error::Error>> {
    let stats = storage.get_stats().await?;

    println!("📊 Storage Usage Report");
    println!("======================");
    println!("Total files: {}", stats.file_count);
    println!("Total size: {:.2} MB", stats.total_bytes as f64 / 1024.0 / 1024.0);
    println!("Average file size: {:.2} KB", stats.average_file_size / 1024.0);
    println!("Last updated: {}", stats.last_updated);

    println!("\n📁 File Types:");
    let mut content_types: Vec<_> = stats.content_type_counts.iter().collect();
    content_types.sort_by(|a, b| b.1.cmp(a.1)); // Sort by count

    for (content_type, count) in content_types.iter().take(10) {
        println!("  {}: {} files", content_type, count);
    }

    // Storage usage warning
    const SIZE_LIMIT_MB: u64 = 1024; // 1GB limit
    let size_mb = stats.total_bytes / 1024 / 1024;

    if size_mb > SIZE_LIMIT_MB {
        println!("\n⚠️  Storage usage warning: {:.2} MB used (limit: {} MB)",
                 size_mb, SIZE_LIMIT_MB);
    }

    Ok(())
}
```

## 🔧 Error Handling

### Comprehensive Error Recovery

```rust
use backbone_storage::{StorageService, StorageError};

async fn safe_file_operation(
    storage: &dyn StorageService,
    operation: String
) -> Result<(), Box<dyn std::error::Error>> {
    match storage.test_connection().await {
        Ok(true) => println!("✅ Storage service is healthy"),
        Ok(false) => {
            println!("❌ Storage service connection failed");
            return Err("Storage service unavailable".into());
        }
        Err(e) => {
            println!("❌ Storage service error: {}", e);
            return Err(e.into());
        }
    }

    // Perform operation with error handling
    match storage.file_exists("test.txt").await {
        Ok(exists) => println!("File exists check: {}", exists),
        Err(StorageError::FileNotFound(path)) => {
            println!("File not found: {}", path);
        }
        Err(StorageError::FilesystemError(e)) => {
            eprintln!("Filesystem error: {}", e);
            return Err(e.into());
        }
        Err(e) => {
            eprintln!("Unexpected error: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}
```

## 🧪 Testing

### Unit Tests

```bash
# Run all backbone-storage tests
cargo test --package backbone-storage

# Run specific tests
cargo test --package backbone-storage -- -- upload
cargo test --package backbone-storage -- -- download
cargo test --package backbone-storage -- -- metadata
```

### Integration Tests

```rust
use backbone_storage::{StorageService, LocalStorage};
use tempfile::TempDir;

#[tokio::test]
async fn test_file_operations() -> Result<(), Box<dyn std::error::Error>> {
    // Create temporary directory for testing
    let temp_dir = TempDir::new()?;
    let config = backbone_storage::LocalStorageConfig {
        base_dir: temp_dir.path().to_string_lossy().to_string(),
        base_path: None,
        compression: false,
        encryption: None,
    };

    let storage = LocalStorage::new(config)?;

    // Test upload
    let data = bytes::Bytes::from("Hello, World!");
    let uploaded = storage.upload_bytes("test.txt", data.clone(), None).await?;
    assert_eq!(uploaded.name, "test.txt");
    assert_eq!(uploaded.size, data.len() as u64);

    // Test download
    let downloaded = storage.download_bytes("test.txt", None).await?;
    assert_eq!(downloaded, data);

    // Test file exists
    assert!(storage.file_exists("test.txt").await?);

    // Test delete
    let deleted = storage.delete_file("test.txt").await?;
    assert!(deleted);
    assert!(!storage.file_exists("test.txt").await?);

    Ok(())
}
```

## 🔗 Configuration

### Environment Variables

```bash
# Local Storage Configuration
LOCAL_STORAGE_DIR=./storage
LOCAL_STORAGE_BASE_PATH=/app/storage
LOCAL_STORAGE_COMPRESSION=true
LOCAL_STORAGE_MAX_FILE_SIZE=104857600  # 100MB
```

### YAML Configuration

```yaml
# backbone-storage configuration
storage:
  backend: "local"

  local:
    base_dir: "${LOCAL_STORAGE_DIR}"
    base_path: "${LOCAL_STORAGE_BASE_PATH}"
    compression: ${LOCAL_STORAGE_COMPRESSION}
    max_file_size: ${LOCAL_STORAGE_MAX_FILE_SIZE}

  # Upload settings
  upload:
    chunk_size: 8388608  # 8MB
    multipart_threshold: 104857600  # 100MB
    calculate_checksum: true

  # Security settings
  security:
    enable_virus_scanning: false
    allowed_file_types: ["pdf", "doc", "docx", "jpg", "png"]
    max_file_size: 104857600  # 100MB

  # Compression settings
  compression:
    enabled: true
    algorithms: ["gzip", "brotli"]
    quality: 6
```

## 📊 Performance Considerations

### File I/O Optimization

```rust
use backbone_storage::{StorageService, LocalStorage};
use std::sync::Arc;

// Create storage with optimal settings
let storage = Arc::new(
    LocalStorage::builder()
        .base_dir("./fast_storage")
        .compression(false) // Disable compression for performance
        .build()?
);

// Concurrent file operations
let mut handles = vec![];

for i in 0..10 {
    let storage = storage.clone();
    let handle = tokio::spawn(async move {
        let data = format!("File content {}", i);
        let bytes = bytes::Bytes::from(data);

        storage.upload_bytes(&format!("file_{}.txt", i), bytes, None).await
    });

    handles.push(handle);
}

// Wait for all operations to complete
for handle in handles {
    let result = handle.await?;
    println!("Upload result: {:?}", result);
}
```

## 🔄 Version History

### Current Version: 2.0.0

**Features:**
- ✅ Complete local filesystem storage implementation
- ✅ Async file streaming with range support
- ✅ Rich metadata and statistics collection
- ✅ Security features and path sanitization
- ✅ Builder pattern configuration
- ✅ Comprehensive error handling
- ✅ File upload/download/copy/move/delete operations
- ✅ Directory listing with pagination

**Breaking Changes from v1.x:**
- Simplified to local storage only
- Updated API to use async/await throughout
- Enhanced metadata structure
- Improved error handling with context
- Removed cloud storage complexity

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🔗 Related Projects

- **[Backbone Core](../backbone-core/)** - Generic CRUD foundation
- **[Backbone Email](../backbone-email/)** - SMTP email services
- **[Backbone CLI](../backbone-cli/)** - Code generation tools
- **[Framework Documentation](../../docs/technical/)** - Complete framework guide

---

**🦴 Backbone Storage - Reliable local filesystem storage**

*Local First • Async Native • Production Ready*