//! Cross-context communication configuration
//!
//! Defines event bus and context routing settings.

use serde::{Deserialize, Serialize};

/// Cross-context communication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextsConfig {
    /// Event bus type (in_memory, redis, kafka)
    #[serde(default = "default_event_bus")]
    pub event_bus: String,
    /// Authentication context
    #[serde(default = "default_auth_context")]
    pub authentication: String,
    /// File storage context
    #[serde(default = "default_storage_context")]
    pub file_storage: String,
    /// Redis event bus configuration
    #[serde(default)]
    pub redis_event_bus: Option<RedisEventBusConfig>,
}

fn default_event_bus() -> String { "in_memory".to_string() }
fn default_auth_context() -> String { "sapiens".to_string() }
fn default_storage_context() -> String { "bucket".to_string() }

impl Default for ContextsConfig {
    fn default() -> Self {
        Self {
            event_bus: default_event_bus(),
            authentication: default_auth_context(),
            file_storage: default_storage_context(),
            redis_event_bus: None,
        }
    }
}

/// Redis event bus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisEventBusConfig {
    /// Redis URL
    pub url: String,
    /// Channel prefix
    #[serde(default)]
    pub channel_prefix: Option<String>,
    /// Max connections
    #[serde(default)]
    pub max_connections: Option<u32>,
}
