//! File security analysis and virus scanning module
//!
//! Provides comprehensive security analysis for uploaded files:
//! - Executable file detection and analysis
//! - Binary file metadata extraction
//! - File signature scanning
//! - Malware pattern detection
//! - Suspicious behavior analysis
//! - Quarantine and threat response

use crate::{StorageError, StorageResult};
use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tracing::{debug, info, warn, error};
use regex::Regex;

/// Security threat levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThreatLevel {
    /// No threat detected
    Safe,
    /// Suspicious but not malicious
    Suspicious,
    /// Potentially malicious
    Warning,
    /// Definitely malicious
    Critical,
}

/// File security analysis result
#[derive(Debug, Clone)]
pub struct SecurityAnalysis {
    /// File hash (SHA256)
    pub file_hash: String,

    /// Threat level detected
    pub threat_level: ThreatLevel,

    /// File type category
    pub file_category: FileCategory,

    /// Is executable file
    pub is_executable: bool,

    /// Is PE (Windows executable)
    pub is_pe_executable: bool,

    /// Is ELF (Linux/Unix executable)
    pub is_elf_executable: bool,

    /// Is Mach-O (macOS executable)
    pub is_macho_executable: bool,

    /// Executable file metadata
    pub executable_metadata: Option<ExecutableMetadata>,

    /// Detected threats
    pub threats: Vec<Threat>,

    /// File size analysis
    pub size_analysis: SizeAnalysis,

    /// Content analysis
    pub content_analysis: ContentAnalysis,

    /// Recommendations
    pub recommendations: Vec<String>,
}

/// File category for security analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCategory {
    Executable,
    Script,
    Document,
    Archive,
    Image,
    Video,
    Audio,
    Text,
    Binary,
    Unknown,
}

/// Executable file metadata
#[derive(Debug, Clone)]
pub struct ExecutableMetadata {
    /// File format (PE, ELF, Mach-O)
    pub format: String,

    /// Architecture (x86, x64, ARM, etc.)
    pub architecture: String,

    /// Entry point
    pub entry_point: Option<u64>,

    /// Import table
    pub imports: Vec<String>,

    /// Export table
    pub exports: Vec<String>,

    /// Sections
    pub sections: Vec<String>,

    /// File size
    pub file_size: u64,

    /// Timestamp
    pub timestamp: Option<String>,

    /// Digital signature info
    pub signature: Option<DigitalSignature>,
}

/// Digital signature information
#[derive(Debug, Clone)]
pub struct DigitalSignature {
    pub is_signed: bool,
    pub signer: Option<String>,
    pub timestamp: Option<String>,
    pub certificate_chain: Vec<String>,
}

/// Detected threat
#[derive(Debug, Clone)]
pub struct Threat {
    /// Threat ID
    pub id: String,

    /// Threat type
    pub threat_type: ThreatType,

    /// Threat description
    pub description: String,

    /// Location in file (offset)
    pub location: Option<u64>,

    /// Pattern matched
    pub pattern: Option<String>,

    /// Confidence level (0.0-1.0)
    pub confidence: f32,

    /// Severity level
    pub severity: ThreatLevel,
}

/// Threat types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreatType {
    KnownMalware,
    SuspiciousBehavior,
    HiddenExecutable,
    EncryptedPayload,
    ObfuscatedCode,
    NetworkActivity,
    FileSystemActivity,
    RegistryActivity,
    SuspiciousImports,
    UnsignedExecutable,
    SuspiciousTimestamp,
    LargeFileSize,
    SuspiciousFilename,
    PackedExecutable,
    AntiDebugging,
    AntiVM,
    Rootkit,
    Backdoor,
    Trojan,
    Worm,
    Virus,
    Ransomware,
    Spyware,
    Adware,
    PUP, // Potentially Unwanted Program
}

/// Size analysis results
#[derive(Debug, Clone)]
pub struct SizeAnalysis {
    pub file_size: u64,
    pub is_oversized: bool,
    pub is_undersized: bool,
    pub size_category: SizeCategory,
}

/// Size categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeCategory {
    Tiny,      // < 1KB
    Small,     // 1KB - 100KB
    Medium,    // 100KB - 10MB
    Large,     // 10MB - 100MB
    Huge,      // > 100MB
}

/// Content analysis results
#[derive(Debug, Clone)]
pub struct ContentAnalysis {
    pub has_obfuscated_code: bool,
    pub has_encrypted_content: bool,
    pub has_suspicious_strings: Vec<String>,
    pub has_network_indicators: Vec<String>,
    pub has_file_system_indicators: Vec<String>,
    pub entropy_score: f32,
    pub string_entropy: f32,
}

/// Security scanning configuration
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Enable security scanning
    pub enabled: bool,

    /// Block malicious files
    pub block_malicious: bool,

    /// Block suspicious files
    pub block_suspicious: bool,

    /// Block unsigned executables
    pub block_unsigned_executables: bool,

    /// Block files over size limit
    pub block_oversized_files: bool,

    /// Maximum file size (bytes)
    pub max_file_size: Option<u64>,

    /// Known malware signatures database
    pub malware_signatures: Vec<MalwareSignature>,

    /// Suspicious patterns to scan for
    pub suspicious_patterns: Vec<SuspiciousPattern>,

    /// File extensions to block
    pub blocked_extensions: Vec<String>,

    /// Allow trusted signed executables
    pub allow_signed_executables: bool,

    /// Quarantine infected files
    pub quarantine_infected: bool,

    /// Log security events
    pub log_security_events: bool,
}

/// Malware signature
#[derive(Debug, Clone)]
pub struct MalwareSignature {
    pub name: String,
    pub pattern: String,
    pub threat_type: ThreatType,
    pub description: String,
}

/// Suspicious pattern
#[derive(Debug, Clone)]
pub struct SuspiciousPattern {
    pub name: String,
    pub pattern: Regex,
    pub threat_type: ThreatType,
    pub description: String,
    pub severity: ThreatLevel,
}

/// Security engine for file analysis
pub struct SecurityEngine {
    pub config: SecurityConfig,
    malware_signatures: Vec<MalwareSignature>,
    suspicious_patterns: Vec<SuspiciousPattern>,
}

impl SecurityEngine {
    /// Create new security engine
    pub fn new(config: SecurityConfig) -> Self {
        let malware_signatures = Self::load_default_malware_signatures();
        let suspicious_patterns = Self::load_default_suspicious_patterns();

        Self {
            config,
            malware_signatures,
            suspicious_patterns,
        }
    }

    /// Get security configuration
    pub fn config(&self) -> &SecurityConfig {
        &self.config
    }

    /// Create security engine with default configuration
    pub fn default() -> Self {
        Self::new(SecurityConfig::default())
    }

    /// Analyze file for security threats
    pub async fn analyze_file(
        &self,
        data: &[u8],
        file_path: &str,
        content_type: Option<&str>,
    ) -> StorageResult<SecurityAnalysis> {
        debug!("Analyzing file security: {}", file_path);

        let file_hash = self.calculate_hash(data);
        let file_size = data.len() as u64;
        let file_category = self.detect_file_category(data, file_path, content_type);

        // Analyze executable content
        let (is_executable, executable_metadata) = self.analyze_executable_content(data).await?;

        // Size analysis
        let size_analysis = self.analyze_file_size(file_size);

        // Content analysis
        let content_analysis = self.analyze_content(data).await?;

        // Scan for threats
        let threats = self.scan_for_threats(data, file_path, &file_category).await?;

        // Determine threat level
        let threat_level = self.determine_threat_level(&threats, &file_category, &executable_metadata);

        // Generate recommendations
        let recommendations = self.generate_recommendations(&threat_level, &file_category, &threats);

        let analysis = SecurityAnalysis {
            file_hash,
            threat_level,
            file_category,
            is_executable,
            is_pe_executable: executable_metadata.as_ref()
                .map(|m| m.format == "PE")
                .unwrap_or(false),
            is_elf_executable: executable_metadata.as_ref()
                .map(|m| m.format == "ELF")
                .unwrap_or(false),
            is_macho_executable: executable_metadata.as_ref()
                .map(|m| m.format == "Mach-O")
                .unwrap_or(false),
            executable_metadata,
            threats,
            size_analysis,
            content_analysis,
            recommendations,
        };

        info!("Security analysis completed for {}: {:?}, {} threats",
              file_path, analysis.threat_level, analysis.threats.len());

        Ok(analysis)
    }

    /// Check if file is safe to store
    pub fn is_file_safe(&self, analysis: &SecurityAnalysis) -> bool {
        match analysis.threat_level {
            ThreatLevel::Safe => true,
            ThreatLevel::Suspicious => !self.config.block_suspicious,
            ThreatLevel::Warning => !self.config.block_suspicious,
            ThreatLevel::Critical => !self.config.block_malicious,
        }
    }

    /// Calculate SHA256 hash of file
    fn calculate_hash(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    /// Detect file category
    fn detect_file_category(&self, data: &[u8], file_path: &str, content_type: Option<&str>) -> FileCategory {
        // Check if it's an executable first
        if self.is_executable_data(data) {
            return FileCategory::Executable;
        }

        // Check for script content
        if self.is_script_content(data, file_path) {
            return FileCategory::Script;
        }

        // Use MIME type if available
        if let Some(ct) = content_type {
            return self.mime_type_to_category(ct);
        }

        // Use file extension
        self.extension_to_category(file_path)
    }

    /// Check if data represents an executable
    fn is_executable_data(&self, data: &[u8]) -> bool {
        if data.len() < 4 {
            return false;
        }

        // Check for common executable magic numbers
        match &data[0..4] {
            b"MZ" => true,  // PE (Windows)
            b"\x7fELF" => true, // ELF (Linux)
            b"\xfe\xed\xfa\xce" => true, // Mach-O (macOS, big-endian)
            b"\xce\xfa\xed\xfe" => true, // Mach-O (macOS, little-endian)
            b"\xca\xfe\xba\xbe" => true, // Java class file
            _ => false,
        }
    }

    /// Check if file is a script
    fn is_script_content(&self, data: &[u8], file_path: &str) -> bool {
        // Check file extension first
        let extension = std::path::Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            "sh" | "bash" | "zsh" | "fish" | "csh" | "tcsh" => return true,
            "py" | "python" | "pyw" | "pyc" | "pyo" => return true,
            "pl" | "pm" | "t" => return true,
            "rb" => return true,
            "php" | "php3" | "php4" | "php5" | "phtml" => return true,
            "js" => return true,
            "bat" | "cmd" => return true,
            "ps1" | "psm1" => return true,
            _ => {}
        }

        // Check content for script shebangs
        if data.len() >= 2 && data[0] == b'#' && data[1] == b'!' {
            return true;
        }

        false
    }

    /// Convert MIME type to category
    fn mime_type_to_category(&self, content_type: &str) -> FileCategory {
        match content_type.to_lowercase().as_str() {
            ct if ct.starts_with("application/x-executable") => FileCategory::Executable,
            ct if ct.starts_with("application/x-msdownload") => FileCategory::Executable,
            ct if ct.starts_with("application/x-msdos-program") => FileCategory::Executable,
            ct if ct.starts_with("text/") => {
                if ct.contains("javascript") || ct.contains("script") {
                    FileCategory::Script
                } else {
                    FileCategory::Text
                }
            },
            "application/json" | "application/xml" | "text/xml" => FileCategory::Text,
            ct if ct.starts_with("image/") => FileCategory::Image,
            ct if ct.starts_with("video/") => FileCategory::Video,
            ct if ct.starts_with("audio/") => FileCategory::Audio,
            ct if ct.starts_with("application/pdf") => FileCategory::Document,
            "application/zip" | "application/x-tar" | "application/gzip" => FileCategory::Archive,
            _ => FileCategory::Unknown,
        }
    }

    /// Convert file extension to category
    fn extension_to_category(&self, file_path: &str) -> FileCategory {
        let extension = std::path::Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            // Executables
            "exe" | "dll" | "sys" | "ocx" | "cpl" | "scr" => FileCategory::Executable,

            // Scripts
            "sh" | "bash" | "py" | "pl" | "rb" | "php" | "js" | "bat" | "cmd" | "ps1" => FileCategory::Script,

            // Documents
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" => FileCategory::Document,

            // Archives
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" => FileCategory::Archive,

            // Images
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "webp" => FileCategory::Image,

            // Videos
            "mp4" | "avi" | "mkv" | "mov" | "wmv" | "flv" => FileCategory::Video,

            // Audio
            "mp3" | "wav" | "flac" | "aac" => FileCategory::Audio,

            // Text
            "txt" | "md" | "json" | "xml" | "yaml" | "toml" | "csv" => FileCategory::Text,

            _ => FileCategory::Unknown,
        }
    }

    /// Analyze executable content
    async fn analyze_executable_content(&self, data: &[u8]) -> StorageResult<(bool, Option<ExecutableMetadata>)> {
        if !self.is_executable_data(data) {
            return Ok((false, None));
        }

        // Parse PE files
        if data.len() >= 2 && &data[0..2] == b"MZ" {
            return self.parse_pe_executable(data).await;
        }

        // Parse ELF files
        if data.len() >= 4 && &data[0..4] == b"\x7fELF" {
            return self.parse_elf_executable(data).await;
        }

        // Parse Mach-O files
        if data.len() >= 4 {
            let magic = &data[0..4];
            if magic == b"\xfe\xed\xfa\xce" || magic == b"\xce\xfa\xed\xfe" {
                return self.parse_macho_executable(data).await;
            }
        }

        Ok((true, Some(ExecutableMetadata {
            format: "Unknown".to_string(),
            architecture: "Unknown".to_string(),
            entry_point: None,
            imports: vec![],
            exports: vec![],
            sections: vec![],
            file_size: data.len() as u64,
            timestamp: None,
            signature: None,
        })))
    }

    /// Parse PE (Windows executable) file
    async fn parse_pe_executable(&self, data: &[u8]) -> StorageResult<(bool, Option<ExecutableMetadata>)> {
        // This is a simplified PE parser - in production, you'd want a more robust implementation
        debug!("Parsing PE executable");

        let metadata = ExecutableMetadata {
            format: "PE".to_string(),
            architecture: "x86".to_string(), // Simplified
            entry_point: None, // Would parse PE header
            imports: vec!["kernel32.dll".to_string()], // Simplified
            exports: vec![],
            sections: vec![".text".to_string(), ".data".to_string()],
            file_size: data.len() as u64,
            timestamp: Some("Unknown".to_string()),
            signature: Some(DigitalSignature {
                is_signed: false,
                signer: None,
                timestamp: None,
                certificate_chain: vec![],
            }),
        };

        Ok((true, Some(metadata)))
    }

    /// Parse ELF (Linux/Unix executable) file
    async fn parse_elf_executable(&self, data: &[u8]) -> StorageResult<(bool, Option<ExecutableMetadata>)> {
        debug!("Parsing ELF executable");

        let metadata = ExecutableMetadata {
            format: "ELF".to_string(),
            architecture: "x86_64".to_string(), // Simplified
            entry_point: None, // Would parse ELF header
            imports: vec!["libc.so.6".to_string()], // Simplified
            exports: vec![],
            sections: vec![".text".to_string(), ".data".to_string()],
            file_size: data.len() as u64,
            timestamp: Some("Unknown".to_string()),
            signature: None, // ELF executables typically not signed
        };

        Ok((true, Some(metadata)))
    }

    /// Parse Mach-O (macOS executable) file
    async fn parse_macho_executable(&self, data: &[u8]) -> StorageResult<(bool, Option<ExecutableMetadata>)> {
        debug!("Parsing Mach-O executable");

        let metadata = ExecutableMetadata {
            format: "Mach-O".to_string(),
            architecture: "x86_64".to_string(), // Simplified
            entry_point: None, // Would parse Mach-O header
            imports: vec!["libSystem.dylib".to_string()], // Simplified
            exports: vec![],
            sections: vec!["__text".to_string(), "__data".to_string()],
            file_size: data.len() as u64,
            timestamp: Some("Unknown".to_string()),
            signature: Some(DigitalSignature {
                is_signed: true, // macOS apps often signed
                signer: Some("Apple".to_string()), // Simplified
                timestamp: None,
                certificate_chain: vec![],
            }),
        };

        Ok((true, Some(metadata)))
    }

    /// Analyze file size
    fn analyze_file_size(&self, size: u64) -> SizeAnalysis {
        let is_oversized = if let Some(max_size) = self.config.max_file_size {
            size > max_size
        } else {
            false
        };

        let is_undersized = size < 10; // Less than 10 bytes is suspicious

        let size_category = match size {
            0..=1024 => SizeCategory::Tiny,
            1025..=102400 => SizeCategory::Small,
            102401..=10485760 => SizeCategory::Medium,
            10485761..=104857600 => SizeCategory::Large,
            _ => SizeCategory::Huge,
        };

        SizeAnalysis {
            file_size: size,
            is_oversized,
            is_undersized,
            size_category,
        }
    }

    /// Analyze file content for suspicious patterns
    async fn analyze_content(&self, data: &[u8]) -> StorageResult<ContentAnalysis> {
        let content_str = std::string::String::from_utf8_lossy(data);

        // Check for obfuscated code (high entropy)
        let entropy_score = self.calculate_entropy(data);
        let has_obfuscated_code = entropy_score > 7.0;

        // Check for encrypted content (very high entropy + no readable strings)
        let string_entropy = self.calculate_string_entropy(&content_str);
        let has_encrypted_content = entropy_score > 7.5 && string_entropy < 3.0;

        // Check for suspicious strings
        let suspicious_strings = self.find_suspicious_strings(&content_str);

        // Check for network indicators
        let network_indicators = self.find_network_indicators(&content_str);

        // Check for file system indicators
        let fs_indicators = self.find_file_system_indicators(&content_str);

        Ok(ContentAnalysis {
            has_obfuscated_code,
            has_encrypted_content,
            has_suspicious_strings: suspicious_strings,
            has_network_indicators: network_indicators,
            has_file_system_indicators: fs_indicators,
            entropy_score,
            string_entropy,
        })
    }

    /// Calculate entropy of data (detects encryption/obfuscation)
    fn calculate_entropy(&self, data: &[u8]) -> f32 {
        let mut byte_counts = [0u32; 256];
        let mut total_bytes = 0u32;

        for &byte in data {
            byte_counts[byte as usize] += 1;
            total_bytes += 1;
        }

        if total_bytes == 0 {
            return 0.0;
        }

        let mut entropy = 0.0f32;
        for &count in &byte_counts {
            if count > 0 {
                let probability = count as f32 / total_bytes as f32;
                entropy -= probability * probability.log2();
            }
        }

        entropy
    }

    /// Calculate entropy of readable strings
    fn calculate_string_entropy(&self, text: &str) -> f32 {
        let mut char_counts = std::collections::HashMap::new();
        let mut total_chars = 0u32;

        for ch in text.chars() {
            *char_counts.entry(ch).or_insert(0) += 1;
            total_chars += 1;
        }

        if total_chars == 0 {
            return 0.0;
        }

        let mut entropy = 0.0f32;
        for &count in char_counts.values() {
            let probability = count as f32 / total_chars as f32;
            entropy -= probability * probability.log2();
        }

        entropy
    }

    /// Find suspicious strings in content
    fn find_suspicious_strings(&self, content: &str) -> Vec<String> {
        let suspicious_keywords = vec![
            "cmd.exe", "powershell.exe", "wscript.exe", "cscript.exe",
            "regsvr32.exe", "rundll32.exe", "mshta.exe",
            "CreateRemoteThread", "WriteProcessMemory", "VirtualAllocEx",
            "SetWindowsHookEx", "GetProcAddress", "LoadLibrary",
            "sockaddr", "bind", "connect", "listen", "accept",
            "RegCreateKey", "RegSetValue", "RegDeleteKey",
            "CryptDecrypt", "CryptEncrypt", "CreateFile",
            "WriteFile", "DeleteFile", "MoveFile",
            "http://", "https://", "ftp://",
            "eval(", "exec(", "system(", "shell_exec(",
            "base64_decode", "atob(", "btoa(",
        ];

        let mut found = vec![];
        for keyword in suspicious_keywords {
            if content.to_lowercase().contains(keyword) {
                found.push(keyword.to_string());
            }
        }

        found
    }

    /// Find network activity indicators
    fn find_network_indicators(&self, content: &str) -> Vec<String> {
        let network_patterns = vec![
            r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}:\d+\b", // IP:Port
            r"https?://[^\s]+", // URLs
            r"ftp://[^\s]+", // FTP URLs
            r"smtp\.|pop3\.|imap\.|telnet\.", // Network protocols
        ];

        let mut found = vec![];
        for pattern in network_patterns {
            if let Ok(regex) = Regex::new(pattern) {
                for cap in regex.find_iter(content) {
                    found.push(cap.as_str().to_string());
                }
            }
        }

        found
    }

    /// Find file system activity indicators
    fn find_file_system_indicators(&self, content: &str) -> Vec<String> {
        let fs_patterns = vec![
            r"C:\\\\Windows\\\\System32\\\\",
            r"C:\\\\Program Files\\\\",
            r"C:\\\\Users\\\\",
            r"/etc/",
            r"/bin/",
            r"/usr/bin/",
            r"/tmp/",
            r"/var/",
        ];

        let mut found = vec![];
        for pattern in fs_patterns {
            if let Ok(regex) = Regex::new(pattern) {
                for cap in regex.find_iter(content) {
                    found.push(cap.as_str().to_string());
                }
            }
        }

        found
    }

    /// Scan for threats using signatures and patterns
    async fn scan_for_threats(
        &self,
        data: &[u8],
        file_path: &str,
        file_category: &FileCategory,
    ) -> StorageResult<Vec<Threat>> {
        let mut threats = vec![];

        // Check blocked extensions
        let extension = std::path::Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();

        if self.config.blocked_extensions.contains(&extension.to_string()) {
            threats.push(Threat {
                id: format!("blocked_ext_{}", extension),
                threat_type: ThreatType::SuspiciousFilename,
                description: format!("Blocked file extension: {}", extension),
                location: None,
                pattern: Some(extension.to_string()),
                confidence: 1.0,
                severity: ThreatLevel::Critical,
            });
        }

        // Check file size limits
        if let Some(max_size) = self.config.max_file_size {
            if data.len() as u64 > max_size {
                threats.push(Threat {
                    id: "oversized_file".to_string(),
                    threat_type: ThreatType::LargeFileSize,
                    description: format!("File exceeds size limit: {} bytes", data.len()),
                    location: None,
                    pattern: None,
                    confidence: 1.0,
                    severity: ThreatLevel::Warning,
                });
            }
        }

        // Scan for malware signatures
        for signature in &self.malware_signatures {
            if let Ok(regex) = Regex::new(&signature.pattern) {
                if regex.is_match(&std::string::String::from_utf8_lossy(data)) {
                    threats.push(Threat {
                        id: signature.name.clone(),
                        threat_type: signature.threat_type.clone(),
                        description: signature.description.clone(),
                        location: None,
                        pattern: Some(signature.pattern.clone()),
                        confidence: 0.9,
                        severity: ThreatLevel::Critical,
                    });
                }
            }
        }

        // Scan for suspicious patterns
        for pattern in &self.suspicious_patterns {
            if pattern.pattern.is_match(&std::string::String::from_utf8_lossy(data)) {
                threats.push(Threat {
                    id: pattern.name.clone(),
                    threat_type: pattern.threat_type.clone(),
                    description: pattern.description.clone(),
                    location: None,
                    pattern: Some(pattern.pattern.to_string()),
                    confidence: 0.7,
                    severity: pattern.severity,
                });
            }
        }

        Ok(threats)
    }

    /// Determine overall threat level
    fn determine_threat_level(
        &self,
        threats: &[Threat],
        file_category: &FileCategory,
        executable_metadata: &Option<ExecutableMetadata>,
    ) -> ThreatLevel {
        if threats.is_empty() {
            // Check for unsigned executables
            if *file_category == FileCategory::Executable && self.config.block_unsigned_executables {
                if let Some(metadata) = executable_metadata {
                    if let Some(signature) = &metadata.signature {
                        if !signature.is_signed {
                            return ThreatLevel::Warning;
                        }
                    } else {
                        return ThreatLevel::Warning;
                    }
                }
            }

            return ThreatLevel::Safe;
        }

        // Find the highest severity threat
        let mut max_level = ThreatLevel::Safe;
        for threat in threats {
            if threat.severity > max_level {
                max_level = threat.severity;
            }
        }

        max_level
    }

    /// Generate security recommendations
    fn generate_recommendations(
        &self,
        threat_level: &ThreatLevel,
        file_category: &FileCategory,
        threats: &[Threat],
    ) -> Vec<String> {
        let mut recommendations = vec![];

        match threat_level {
            ThreatLevel::Safe => {
                recommendations.push("File appears safe for processing".to_string());
            }
            ThreatLevel::Suspicious => {
                recommendations.push("File requires manual review before processing".to_string());
                recommendations.push("Consider scanning with additional antivirus tools".to_string());
            }
            ThreatLevel::Warning => {
                recommendations.push("WARNING: File exhibits suspicious behavior".to_string());
                recommendations.push("Strongly recommend manual review".to_string());
                recommendations.push("Consider blocking this file type".to_string());
            }
            ThreatLevel::Critical => {
                recommendations.push("CRITICAL: Malware detected".to_string());
                recommendations.push("Block this file immediately".to_string());
                recommendations.push("Report to security team".to_string());
                recommendations.push("Quarantine file if possible".to_string());
            }
        }

        // File-specific recommendations
        match file_category {
            FileCategory::Executable => {
                if self.config.block_unsigned_executables {
                    recommendations.push("Consider requiring digital signatures for executables".to_string());
                }
            }
            FileCategory::Script => {
                recommendations.push("Script files should be carefully reviewed".to_string());
                recommendations.push("Consider using sandboxed execution".to_string());
            }
            _ => {}
        }

        recommendations
    }

    /// Load default malware signatures
    fn load_default_malware_signatures() -> Vec<MalwareSignature> {
        vec![
            MalwareSignature {
                name: "Emotet".to_string(),
                pattern: r"Emotet|Heodo|Geodo".to_string(),
                threat_type: ThreatType::Trojan,
                description: "Emotet banking trojan signature".to_string(),
            },
            MalwareSignature {
                name: "WannaCry".to_string(),
                pattern: r"WannaCry|WNCRY".to_string(),
                threat_type: ThreatType::Ransomware,
                description: "WannaCry ransomware signature".to_string(),
            },
            MalwareSignature {
                name: "TrickBot".to_string(),
                pattern: r"TrickBot|TrickLoader".to_string(),
                threat_type: ThreatType::Trojan,
                description: "TrickBot banking trojan signature".to_string(),
            },
        ]
    }

    /// Load default suspicious patterns
    fn load_default_suspicious_patterns() -> Vec<SuspiciousPattern> {
        vec![
            SuspiciousPattern {
                name: "Hidden Executable".to_string(),
                pattern: Regex::new(r"MZ|\x7fELF|\xfe\xed\xfa\xce").unwrap(),
                threat_type: ThreatType::HiddenExecutable,
                description: "Hidden executable in non-executable file".to_string(),
                severity: ThreatLevel::Critical,
            },
            SuspiciousPattern {
                name: "PowerShell Obfuscation".to_string(),
                pattern: Regex::new(r"\$[^\s]+=\s*\$[^\s]+").unwrap(),
                threat_type: ThreatType::ObfuscatedCode,
                description: "Obfuscated PowerShell code detected".to_string(),
                severity: ThreatLevel::Warning,
            },
            SuspiciousPattern {
                name: "Base64 in Executable".to_string(),
                pattern: Regex::new(r"[A-Za-z0-9+/]{50,}={0,2}").unwrap(),
                threat_type: ThreatType::ObfuscatedCode,
                description: "Base64 encoded content in executable".to_string(),
                severity: ThreatLevel::Suspicious,
            },
        ]
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            block_malicious: true,
            block_suspicious: false,
            block_unsigned_executables: false,
            block_oversized_files: true,
            max_file_size: Some(100 * 1024 * 1024), // 100MB
            malware_signatures: vec![],
            suspicious_patterns: vec![],
            blocked_extensions: vec![
                "scr".to_string(),
                "bat".to_string(),
                "cmd".to_string(),
                "com".to_string(),
                "pif".to_string(),
                "vbs".to_string(),
                "js".to_string(), // JavaScript in uploads
                "jar".to_string(),
                "app".to_string(),
                "deb".to_string(),
                "rpm".to_string(),
                "dmg".to_string(),
                "pkg".to_string(),
                "msi".to_string(),
            ],
            allow_signed_executables: true,
            quarantine_infected: true,
            log_security_events: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_executable_detection() {
        let engine = SecurityEngine::default();

        // PE executable
        let pe_data = b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xff\xff";
        let (is_exec, _) = engine.analyze_executable_content(pe_data).await.unwrap();
        assert!(is_exec);

        // ELF executable
        let elf_data = b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        let (is_exec, _) = engine.analyze_executable_content(elf_data).await.unwrap();
        assert!(is_exec);

        // Not executable
        let text_data = b"Hello, World!";
        let (is_exec, _) = engine.analyze_executable_content(text_data).await.unwrap();
        assert!(!is_exec);
    }

    #[test]
    fn test_file_category_detection() {
        let engine = SecurityEngine::default();

        assert_eq!(engine.extension_to_category("program.exe"), FileCategory::Executable);
        assert_eq!(engine.extension_to_category("script.py"), FileCategory::Script);
        assert_eq!(engine.extension_to_category("document.pdf"), FileCategory::Document);
        assert_eq!(engine.extension_to_category("image.jpg"), FileCategory::Image);
    }

    #[test]
    fn test_entropy_calculation() {
        let engine = SecurityEngine::default();

        // High entropy data (random-like)
        let high_entropy_data = vec![0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0].repeat(100);
        let entropy = engine.calculate_entropy(&high_entropy_data);
        assert!(entropy > 6.0);

        // Low entropy data (repetitive)
        let low_entropy_data = vec![0x41; 800]; // 800 'A' characters
        let entropy = engine.calculate_entropy(&low_entropy_data);
        assert!(entropy < 1.0);
    }

    #[tokio::test]
    async fn test_security_analysis() {
        let engine = SecurityEngine::default();
        let data = b"Hello, World!";
        let analysis = engine.analyze_file(data, "test.txt", Some("text/plain")).await.unwrap();

        assert_eq!(analysis.file_category, FileCategory::Text);
        assert!(!analysis.is_executable);
        assert_eq!(analysis.threat_level, ThreatLevel::Safe);
    }
}