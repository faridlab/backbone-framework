//! Security scanning and threat detection example for Backbone Storage
//!
//! This example demonstrates comprehensive security analysis capabilities including
//! executable file analysis, malware detection, and threat assessment.

use backbone_storage::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🔒 Backbone Storage - Security Scanning Example\n");

    // Initialize security engine with comprehensive configuration
    let security_config = SecurityConfig {
        scan_executables: true,
        max_file_size_scan: 10 * 1024 * 1024, // 10MB
        block_suspicious_files: true,
        log_security_events: true,
        threat_level_threshold: ThreatLevel::Low,
        enable_deep_scan: true,
        quarantine_suspicious: false, // Don't quarantine in demo
        ..Default::default()
    };

    let security_engine = SecurityEngine::new(security_config);
    println!("✅ Security engine initialized with comprehensive scanning");

    // Example 1: Scan various file types
    println!("\n📁 Example 1: Scanning different file types");
    example_file_type_scanning(&security_engine).await?;

    // Example 2: Executable file analysis
    println!("\n🔍 Example 2: Executable file analysis");
    example_executable_analysis(&security_engine).await?;

    // Example 3: Malware signature detection
    println!("\n🦠 Example 3: Malware signature detection");
    example_malware_detection(&security_engine).await?;

    // Example 4: Suspicious pattern detection
    println!("\n⚠️  Example 4: Suspicious pattern detection");
    example_suspicious_patterns(&security_engine).await?;

    // Example 5: Security policy enforcement
    println!("\n🚫 Example 5: Security policy enforcement");
    example_policy_enforcement().await?;

    println!("\n✅ All security scanning examples completed!");
    Ok(())
}

/// Example 1: Scan various file types to demonstrate security analysis
async fn example_file_type_scanning(security_engine: &SecurityEngine) -> Result<(), Box<dyn std::error::Error>> {
    let test_files = vec![
        ("document.txt", b"This is a safe text file with normal content."),
        ("image.jpg", b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00\x01\x01\x01\x00H\x00H\x00\x00"),
        ("script.py", b"print('Hello, world!')\nimport os\nprint(os.getcwd())"),
        ("config.json", br#"{"database": "localhost", "port": 5432, "ssl": true}"#),
        ("data.csv", b"Name,Age,City\nAlice,30,New York\nBob,25,Los Angeles\n"),
    ];

    for (filename, content) in test_files {
        println!("📄 Scanning: {}", filename);

        let analysis = security_engine.scan_file(content, filename).await?;

        println!("   Hash: {}", &analysis.file_hash[..16]);
        println!("   Threat level: {:?}", analysis.threat_level);
        println!("   Executable: {}", analysis.is_executable);
        println!("   Threats: {}", analysis.threats.len());

        // Show additional analysis details
        if !analysis.threats.is_empty() {
            println!("   Threats detected:");
            for threat in &analysis.threats {
                println!("     - {} ({:?}): {}",
                    threat.threat_type, threat.severity, threat.description);
            }
        }

        if let Some(size_analysis) = &analysis.size_analysis {
            println!("   Size analysis: {} bytes, entropy: {:.2}",
                size_analysis.file_size, size_analysis.entropy);
        }

        if let Some(content_analysis) = &analysis.content_analysis {
            println!("   Content: {} chars, {} lines, {} distinct",
                content_analysis.character_count, content_analysis.line_count,
                content_analysis.distinct_patterns);
        }

        println!("   ✅ Safe to store: {}", security_engine.is_file_safe(&analysis));
        println!();
    }

    Ok(())
}

/// Example 2: Deep analysis of executable files
async fn example_executable_analysis(security_engine: &SecurityEngine) -> Result<(), Box<dyn std::error::Error>> {
    // Simulate different executable file headers
    let executables = vec![
        ("windows.exe", create_pe_header()),
        ("linux.elf", create_elf_header()),
        ("macos.macho", create_macho_header()),
        ("suspicious.bin", create_suspicious_executable()),
    ];

    for (filename, content) in executables {
        println!("🔍 Analyzing executable: {}", filename);

        let analysis = security_engine.scan_file(content, filename).await?;

        println!("   Executable: {}", analysis.is_executable);
        println!("   Threat level: {:?}", analysis.threat_level);
        println!("   Hash: {}", &analysis.file_hash[..16]);

        // Show executable metadata if detected
        if let Some(metadata) = &analysis.executable_metadata {
            println!("   File type: {:?}", metadata.file_type);
            println!("   Architecture: {:?}", metadata.architecture);
            println!("   Entry point: {:#x}", metadata.entry_point);
            println!("   Sections: {}", metadata.section_count.unwrap_or(0));

            if !metadata.imports.is_empty() {
                println!("   Key imports:");
                for import in &metadata.imports[..5.min(metadata.imports.len())] {
                    println!("     - {}", import);
                }
                if metadata.imports.len() > 5 {
                    println!("     ... and {} more", metadata.imports.len() - 5);
                }
            }

            if let Some(size) = metadata.code_size {
                println!("   Code size: {} bytes", size);
            }
        }

        // Show size and entropy analysis
        if let Some(size_analysis) = &analysis.size_analysis {
            println!("   Entropy: {:.2} ({} suspicious sections)",
                size_analysis.entropy, size_analysis.suspicious_sections);

            if size_analysis.is_packed {
                println!("   ⚠️  File appears to be packed or obfuscated");
            }
        }

        // Show content analysis
        if let Some(content_analysis) = &analysis.content_analysis {
            if !content_analysis.suspicious_strings.is_empty() {
                println!("   Suspicious strings:");
                for string in &content_analysis.suspicious_strings {
                    println!("     - {}", string);
                }
            }
        }

        println!("   Threats: {}", analysis.threats.len());
        for threat in &analysis.threats {
            println!("     - {}: {}", threat.threat_type, threat.description);
        }

        println!("   ✅ Safe to store: {}", security_engine.is_file_safe(&analysis));
        println!();
    }

    Ok(())
}

/// Example 3: Malware signature detection
async fn example_malware_detection(security_engine: &SecurityEngine) -> Result<(), Box<dyn std::error::Error>> {
    // Create files with known malware-like patterns
    let malware_samples = vec![
        ("trojan_sample.bin", create_trojan_signature()),
        ("backdoor.exe", create_backdoor_signature()),
        ("rootkit.sys", create_rootkit_signature()),
        ("ransomware", create_ransomware_signature()),
    ];

    for (filename, content) in malware_samples {
        println!("🦠 Scanning potential malware: {}", filename);

        let analysis = security_engine.scan_file(content, filename).await?;

        println!("   Threat level: {:?}", analysis.threat_level);
        println!("   Executable: {}", analysis.is_executable);
        println!("   Threats detected: {}", analysis.threats.len());

        for threat in &analysis.threats {
            match threat.threat_type {
                ThreatType::MalwareSignature => {
                    println!("     🦠 Malware signature matched: {}", threat.description);
                }
                ThreatType::PackedExecutable => {
                    println!("     📦 Packed executable: {}", threat.description);
                }
                ThreatType::SuspiciousImports => {
                    println!("     ⚠️  Suspicious imports: {}", threat.description);
                }
                ThreatType::HighEntropy => {
                    println!("     🔀 High entropy content: {}", threat.description);
                }
                _ => {
                    println!("     🚨 {}: {}", threat.threat_type, threat.description);
                }
            }
            println!("       Severity: {:?}", threat.severity);
            println!("       Confidence: {:.1}%", threat.confidence * 100.0);
        }

        println!("   ✅ Safe to store: {}", security_engine.is_file_safe(&analysis));
        println!();
    }

    Ok(())
}

/// Example 4: Suspicious pattern detection
async fn example_suspicious_patterns(security_engine: &SecurityEngine) -> Result<(), Box<dyn std::error::Error>> {
    // Files with suspicious but not necessarily malicious content
    let suspicious_files = vec![
        ("obfuscated.txt", create_obfuscated_content()),
        ("shell_script.sh", b"#!/bin/bash\nrm -rf /\n# Dangerous commands"),
        ("powershell.ps1", b"Invoke-Expression -Command 'Get-Process | Stop-Process'"),
        ("batch_file.bat", b"@echo off\ndel /f /q C:\\Windows\\System32\\*.*"),
        ("macro.vba", b"Sub AutoOpen()\nShell(\"cmd.exe /c format C:\")\nEnd Sub"),
    ];

    for (filename, content) in suspicious_files {
        println!("⚠️  Scanning suspicious file: {}", filename);

        let analysis = security_engine.scan_file(content, filename).await?;

        println!("   Threat level: {:?}", analysis.threat_level);
        println!("   Suspicious threats: {}", analysis.threats.len());

        if let Some(content_analysis) = &analysis.content_analysis {
            println!("   Suspicious patterns found: {}", content_analysis.suspicious_patterns);

            if !content_analysis.suspicious_strings.is_empty() {
                println!("   Suspicious strings:");
                for string in &content_analysis.suspicious_strings {
                    println!("     - \"{}\"", string);
                }
            }
        }

        for threat in &analysis.threats {
            println!("     🚨 {}: {}", threat.threat_type, threat.description);
        }

        println!("   ✅ Safe to store: {}", security_engine.is_file_safe(&analysis));
        println!();
    }

    Ok(())
}

/// Example 5: Demonstrate security policy enforcement in upload workflow
async fn example_policy_enforcement() -> Result<(), Box<dyn std::error::Error>> {
    let config = LocalStorageConfig {
        base_dir: "/tmp/security_demo".into(),
        enable_security_scan: true,
        block_suspicious_files: true,
        ..Default::default()
    };

    let storage = LocalStorage::new(config)?;
    println!("🔒 Storage initialized with security enforcement enabled");

    // Test uploading safe files (should succeed)
    let safe_files = vec![
        ("safe_document.txt", b"This is a completely safe document."),
        ("image_data.png", b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00"),
        ("config.json", br#"{"name": "test", "enabled": true}"#),
    ];

    println!("\n📤 Testing safe file uploads:");
    for (filename, content) in safe_files {
        match storage.upload_bytes(filename, content, "application/octet-stream").await {
            Ok(result) => {
                println!("   ✅ {}: {} bytes uploaded successfully", filename, result.size);
            }
            Err(e) => {
                println!("   ❌ {}: Upload failed - {}", filename, e);
            }
        }
    }

    // Test uploading suspicious files (should be blocked)
    let suspicious_files = vec![
        ("suspicious.exe", create_suspicious_executable()),
        ("malware.bin", create_trojan_signature()),
        ("dangerous.bat", b"@echo off\ndel /f /q C:\\*.*"),
    ];

    println!("\n🚫 Testing suspicious file uploads (should be blocked):");
    for (filename, content) in suspicious_files {
        match storage.upload_bytes(filename, content, "application/octet-stream").await {
            Ok(result) => {
                println!("   ⚠️  {}: Unexpectedly uploaded {} bytes", filename, result.size);
            }
            Err(StorageError::SecurityError { operation, message }) => {
                println!("   ✅ {}: Blocked by security - {}", filename, message);
            }
            Err(e) => {
                println!("   ❌ {}: Upload failed with unexpected error - {}", filename, e);
            }
        }
    }

    Ok(())
}

// Helper functions to create various file signatures and patterns for testing

fn create_pe_header() -> Vec<u8> {
    vec![
        0x4D, 0x5A, 0x90, 0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
        0xFF, 0xFF, 0x00, 0x00, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
    ]
}

fn create_elf_header() -> Vec<u8> {
    vec![
        0x7F, 0x45, 0x4C, 0x46, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x3E, 0x00, 0x01, 0x00, 0x00, 0x00
    ]
}

fn create_macho_header() -> Vec<u8> {
    vec![
        0xFE, 0xED, 0xFA, 0xCF, 0x07, 0x00, 0x00, 0x01, 0x03, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
    ]
}

fn create_suspicious_executable() -> Vec<u8> {
    let mut data = create_pe_header();
    // Add suspicious imports and patterns
    data.extend_from_slice(b"CreateRemoteThread\x00WriteProcessMemory\x00VirtualAllocEx\x00");
    data.extend_from_slice(b"password\x00keylog\x00steal\x00");
    data
}

fn create_trojan_signature() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(create_pe_header().as_slice());
    data.extend_from_slice(b"TROJAN_BACKDOOR\x00");
    data.extend_from_slice(b"cmd.exe\x00/C\x00net user hacker password /add\x00");
    data
}

fn create_backdoor_signature() -> Vec<u8> {
    let mut data = create_pe_header();
    data.extend_from_slice(b"BACKDOOR_SERVER\x00");
    data.extend_from_slice(b"Connect\x00Listen\x00Accept\x00ShellExecute\x00");
    data
}

fn create_rootkit_signature() -> Vec<u8> {
    let mut data = create_elf_header();
    data.extend_from_slice(b"ROOTKIT_KERNEL\x00");
    data.extend_from_slice(b"hide_process\x00stealth\x00hook\x00");
    data
}

fn create_ransomware_signature() -> Vec<u8> {
    let mut data = create_pe_header();
    data.extend_from_slice(b"RANSOMWARE\x00");
    data.extend_from_slice(b"encrypt_file\x00decrypt_instructions\x00bitcoin\x00");
    data
}

fn create_obfuscated_content() -> Vec<u8> {
    // Create high-entropy content that looks obfuscated
    let mut data = Vec::new();
    for i in 0..1000 {
        data.push((i * 137 + 42) as u8);
    }
    data.extend_from_slice(b"EVAL(INTEGER(CHAR(65)+CHAR(66)+CHAR(67)))");
    data
}