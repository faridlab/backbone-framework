//! Server configuration
//!
//! Defines HTTP server settings including host, port, and timeouts.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server host address
    #[serde(default = "default_host")]
    pub host: String,
    /// Server port
    #[serde(default = "default_port")]
    pub port: u16,
    /// Number of worker threads
    #[serde(default)]
    pub workers: Option<usize>,
    /// Graceful shutdown timeout in seconds
    #[serde(default = "default_shutdown_timeout")]
    pub graceful_shutdown_timeout: u64,
    /// Keep-alive timeout in seconds
    #[serde(default = "default_keep_alive")]
    pub keep_alive: u64,
    /// Read timeout in seconds
    #[serde(default = "default_read_timeout")]
    pub read_timeout: u64,
    /// Write timeout in seconds
    #[serde(default = "default_write_timeout")]
    pub write_timeout: u64,
}

fn default_host() -> String { "0.0.0.0".to_string() }
fn default_port() -> u16 { 3000 }
fn default_shutdown_timeout() -> u64 { 30 }
fn default_keep_alive() -> u64 { 75 }
fn default_read_timeout() -> u64 { 30 }
fn default_write_timeout() -> u64 { 30 }

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            workers: Some(4),
            graceful_shutdown_timeout: default_shutdown_timeout(),
            keep_alive: default_keep_alive(),
            read_timeout: default_read_timeout(),
            write_timeout: default_write_timeout(),
        }
    }
}

impl ServerConfig {
    /// Get the socket address string
    pub fn socket_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Get the server URL
    pub fn url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    /// Get graceful shutdown timeout as Duration
    pub fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.graceful_shutdown_timeout)
    }

    /// Get keep-alive timeout as Duration
    pub fn keep_alive_timeout(&self) -> Duration {
        Duration::from_secs(self.keep_alive)
    }
}
