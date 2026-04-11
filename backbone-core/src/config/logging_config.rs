//! Logging configuration
//!
//! Defines logging settings including levels, formats, and file output.

use serde::{Deserialize, Serialize};

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Use structured logging
    #[serde(default = "default_true")]
    pub structured: bool,
    /// Log format (json, pretty, compact)
    #[serde(default = "default_log_format")]
    pub format: String,
    /// Log targets
    #[serde(default = "default_log_targets")]
    pub targets: Vec<String>,
    /// File logging configuration
    #[serde(default)]
    pub file: Option<LoggingFileConfig>,
}

fn default_true() -> bool { true }
fn default_log_level() -> String { "info".to_string() }
fn default_log_format() -> String { "json".to_string() }
fn default_log_targets() -> Vec<String> { vec!["console".to_string()] }

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            structured: true,
            format: default_log_format(),
            targets: default_log_targets(),
            file: None,
        }
    }
}

/// File logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingFileConfig {
    /// Log file path
    pub path: String,
    /// Max file size (e.g., "100MB")
    #[serde(default)]
    pub max_size: Option<String>,
    /// Max number of files to keep
    #[serde(default)]
    pub max_files: Option<u32>,
    /// Compress rotated files
    #[serde(default)]
    pub compress: Option<bool>,
}
