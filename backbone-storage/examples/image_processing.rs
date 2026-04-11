//! Image processing and compression example for Backbone Storage
//!
//! This example demonstrates intelligent image compression, resizing, and optimization
//! features of the storage module.

use backbone_storage::*;
use image::{ImageFormat, RgbImage, ImageBuffer};
use std::io::Cursor;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🖼️  Backbone Storage - Image Processing Example\n");

    // Initialize storage and compression engines
    let storage = LocalStorage::new(LocalStorageConfig::default())?;
    let compression_engine = CompressionEngine::new(CompressionConfig::default());
    let image_compressor = ImageCompressor::new(ImageCompressionConfig::default());

    println!("✅ Storage and compression engines initialized");

    // Example 1: Create and process a sample image
    println!("\n🎨 Example 1: Creating and processing sample image");
    example_sample_image(&storage, &image_compressor).await?;

    // Example 2: Different image formats
    println!("\n📁 Example 2: Multiple image format support");
    example_image_formats(&compression_engine).await?;

    // Example 3: Quality vs compression trade-offs
    println!("\n⚖️  Example 3: Quality vs compression analysis");
    example_quality_analysis(&image_compressor).await?;

    // Example 4: Automatic format detection and optimization
    println!("\n🔍 Example 4: Intelligent format detection");
    example_format_detection(&compression_engine).await?;

    println!("\n✅ All image processing examples completed!");
    Ok(())
}

/// Example 1: Create a sample image and demonstrate compression
async fn example_sample_image(
    storage: &LocalStorage,
    image_compressor: &ImageCompressor,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a simple test image (400x300 RGB)
    let img: RgbImage = ImageBuffer::from_fn(400, 300, |x, y| {
        let r = (x as f32 / 400.0 * 255.0) as u8;
        let g = (y as f32 / 300.0 * 255.0) as u8;
        let b = ((x + y) as f32 / 700.0 * 255.0) as u8;
        image::Rgb([r, g, b])
    });

    // Convert to bytes
    let mut buffer = Vec::new();
    {
        let mut cursor = Cursor::new(&mut buffer);
        img.write_to(&mut cursor, ImageFormat::Jpeg)?;
    }

    println!("📐 Created test image: {}x{} pixels, {} bytes (JPEG)",
        img.width(), img.height(), buffer.len());

    // Upload original image
    let upload_result = storage.upload_bytes("images/original.jpg", &buffer, "image/jpeg").await?;
    println!("📤 Uploaded original: {} bytes", upload_result.size);

    // Apply compression
    let compression_result = image_compressor.compress_image(&buffer, Some(75)).await?;
    println!("🗜️  Compressed with quality 75:");
    println!("   Original: {} bytes", compression_result.original_size);
    println!("   Compressed: {} bytes", compression_result.compressed_size);
    println!("   Ratio: {:.1}%", compression_result.compression_ratio * 100.0);

    // Upload compressed version
    let compressed_upload = storage.upload_bytes("images/compressed.jpg",
        &compression_result.compressed_data, "image/jpeg").await?;
    println!("📤 Uploaded compressed: {} bytes", compressed_upload.size);

    // Extract and display EXIF data if available
    if let Some(exif) = &compression_result.exif_data {
        println!("📷 EXIF data found:");
        if let Some(make) = &exif.make {
            println!("   Camera make: {}", make);
        }
        if let Some(model) = &exif.model {
            println!("   Camera model: {}", model);
        }
        if let Some(datetime) = &exif.datetime {
            println!("   Date taken: {}", datetime);
        }
    }

    Ok(())
}

/// Example 2: Demonstrate support for different image formats
async fn example_image_formats(compression_engine: &CompressionEngine) -> Result<(), Box<dyn std::error::Error>> {
    // Test different image format data (simplified for demonstration)
    let formats = vec![
        ("image/jpeg", "test.jpg"),
        ("image/png", "test.png"),
        ("image/webp", "test.webp"),
        ("image/avif", "test.avif"),
        ("image/gif", "test.gif"),
    ];

    for (mime_type, filename) in formats {
        // Create minimal test data for each format
        let test_data = match mime_type {
            "image/jpeg" => create_jpeg_header(),
            "image/png" => create_png_header(),
            "image/webp" => create_webp_header(),
            "image/avif" => create_avif_header(),
            "image/gif" => create_gif_header(),
            _ => continue,
        };

        println!("📄 Processing {} ({})", filename, mime_type);

        // Detect file category
        let category = compression_engine.detect_file_category(&test_data, filename);
        println!("   Detected category: {:?}", category);

        // Attempt compression
        match compression_engine.compress_file(&test_data, filename).await {
            Ok(result) => {
                println!("   Compression successful: {:.1}% ratio",
                    result.compression_ratio * 100.0);
            }
            Err(e) => {
                println!("   Compression skipped: {}", e);
            }
        }
    }

    Ok(())
}

/// Example 3: Analyze quality vs compression trade-offs
async fn example_quality_analysis(image_compressor: &ImageCompressor) -> Result<(), Box<dyn std::error::Error>> {
    // Create a larger test image for meaningful compression tests
    let img: RgbImage = ImageBuffer::from_fn(800, 600, |x, y| {
        // Create a more complex pattern
        let pattern = ((x as f32).sin() * (y as f32).cos()).abs();
        let value = (pattern * 255.0) as u8;
        image::Rgb([value, 255 - value, (value / 2) + 128])
    });

    // Convert to JPEG bytes
    let mut buffer = Vec::new();
    {
        let mut cursor = Cursor::new(&mut buffer);
        img.write_to(&mut cursor, ImageFormat::Jpeg)?;
    }

    println!("📐 Created test image: {}x{} pixels, {} bytes",
        img.width(), img.height(), buffer.len());

    // Test different quality levels
    let quality_levels = vec![90, 75, 50, 25, 10];

    for quality in quality_levels {
        let result = image_compressor.compress_image(&buffer, Some(quality)).await?;

        println!("🎯 Quality {}: {:.1}% of original size ({:.1}% compression)",
            quality,
            (result.compressed_size as f32 / result.original_size as f32) * 100.0,
            result.compression_ratio * 100.0);
    }

    // Test automatic quality optimization
    println!("🤖 Testing automatic quality optimization...");
    let optimized_result = image_compressor.compress_image(&buffer, None).await?;
    println!("   Optimized compression: {:.1}% ratio",
        optimized_result.compression_ratio * 100.0);

    Ok(())
}

/// Example 4: Intelligent format detection and optimization
async fn example_format_detection(compression_engine: &CompressionEngine) -> Result<(), Box<dyn std::error::Error>> {
    // Test various file types to show intelligent detection
    let test_files = vec![
        ("photo.jpg", create_jpeg_header()),
        ("document.pdf", b"%PDF-1.4\n1 0 obj\n<<\n/Type /Catalog\n>>\nendobj\n"),
        ("data.json", br#"{"users": [{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}]}"#),
        ("script.js", b"function hello() { console.log('Hello, world!'); }"),
        ("archive.zip", create_zip_header()),
        ("text.txt", b"This is a plain text file for testing compression."),
        ("data.csv", "Name,Age,City\nAlice,30,New York\nBob,25,Los Angeles\n"),
    ];

    for (filename, data) in test_files {
        println!("📁 Analyzing: {}", filename);

        // Detect file category
        let category = compression_engine.detect_file_category(data, filename);
        println!("   Category: {:?}", category);

        // Get recommended compression algorithm
        let algorithm = compression_engine.get_recommended_algorithm(data, filename);
        println!("   Recommended algorithm: {:?}", algorithm);

        // Apply compression if recommended
        if algorithm != CompressionAlgorithm::None {
            match compression_engine.compress_file(data, filename).await {
                Ok(result) => {
                    let savings = result.original_size - result.compressed_size;
                    println!("   Compression: {} bytes → {} bytes (saved {} bytes, {:.1}%)",
                        result.original_size, result.compressed_size, savings,
                        result.compression_ratio * 100.0);
                }
                Err(e) => {
                    println!("   Compression failed: {}", e);
                }
            }
        } else {
            println!("   Compression: Not recommended (already optimized)");
        }
        println!();
    }

    Ok(())
}

// Helper functions to create minimal image headers for testing
fn create_jpeg_header() -> Vec<u8> {
    vec![
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01,
        0x01, 0x01, 0x00, 0x48, 0x00, 0x48, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43
    ]
}

fn create_png_header() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
        0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01
    ]
}

fn create_webp_header() -> Vec<u8> {
    b"RIFF\x00\x00\x00\x00WEBP".to_vec()
}

fn create_avif_header() -> Vec<u8> {
    b"\x00\x00\x00\x20ftypavif".to_vec()
}

fn create_gif_header() -> Vec<u8> {
    b"GIF87a\x01\x00\x01\x00".to_vec()
}

fn create_zip_header() -> Vec<u8> {
    b"PK\x03\x04\x14\x00\x00\x00".to_vec()
}