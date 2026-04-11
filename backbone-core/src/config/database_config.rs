//! Database and cache configuration
//!
//! Defines database connection pool settings and cache configuration.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database connection URL
    pub url: String,
    /// Maximum connections in pool
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// Minimum connections in pool
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    /// Connection timeout in seconds
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout: u32,
    /// Idle connection timeout in seconds
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout: u32,
    /// Maximum connection lifetime in seconds
    #[serde(default = "default_max_lifetime")]
    pub max_lifetime: u32,
    /// SSL mode (disable, allow, prefer, require)
    #[serde(default = "default_ssl_mode")]
    pub ssl_mode: Option<String>,
}

fn default_max_connections() -> u32 { 20 }
fn default_min_connections() -> u32 { 5 }
fn default_connect_timeout() -> u32 { 30 }
fn default_idle_timeout() -> u32 { 600 }
fn default_max_lifetime() -> u32 { 1800 }
fn default_ssl_mode() -> Option<String> { Some("prefer".to_string()) }

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgresql://postgres:password@localhost:5432/bersihirdb".to_string(),
            max_connections: default_max_connections(),
            min_connections: default_min_connections(),
            connect_timeout: default_connect_timeout(),
            idle_timeout: default_idle_timeout(),
            max_lifetime: default_max_lifetime(),
            ssl_mode: default_ssl_mode(),
        }
    }
}

impl DatabaseConfig {
    /// Get connection timeout as Duration
    pub fn connect_timeout_duration(&self) -> Duration {
        Duration::from_secs(self.connect_timeout as u64)
    }

    /// Get idle timeout as Duration
    pub fn idle_timeout_duration(&self) -> Duration {
        Duration::from_secs(self.idle_timeout as u64)
    }

    /// Get max lifetime as Duration
    pub fn max_lifetime_duration(&self) -> Duration {
        Duration::from_secs(self.max_lifetime as u64)
    }
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Cache driver (redis, memory)
    #[serde(default = "default_cache_driver")]
    pub driver: String,
    /// Cache connection URL
    #[serde(default = "default_cache_url")]
    pub url: String,
    /// Maximum connections
    #[serde(default = "default_cache_max_connections")]
    pub max_connections: Option<u32>,
    /// Default TTL in seconds
    #[serde(default = "default_cache_ttl")]
    pub default_ttl: Option<u64>,
}

fn default_cache_driver() -> String { "redis".to_string() }
fn default_cache_url() -> String { "redis://localhost:6379".to_string() }
fn default_cache_max_connections() -> Option<u32> { Some(10) }
fn default_cache_ttl() -> Option<u64> { Some(3600) }

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            driver: default_cache_driver(),
            url: default_cache_url(),
            max_connections: default_cache_max_connections(),
            default_ttl: default_cache_ttl(),
        }
    }
}

impl CacheConfig {
    /// Get default TTL as Duration
    pub fn ttl_duration(&self) -> Option<Duration> {
        self.default_ttl.map(Duration::from_secs)
    }
}
