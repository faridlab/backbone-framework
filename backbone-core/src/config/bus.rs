//! Configuration Bus - Cross-Module Configuration Sharing
//!
//! The Configuration Bus provides a mechanism for modules to:
//! - Share configuration values across bounded contexts
//! - Subscribe to configuration changes
//! - Access module-specific configuration without tight coupling
//!
//! # Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────┐
//! │                    Configuration Bus                           │
//! │  ┌─────────────────────────────────────────────────────────┐  │
//! │  │              Shared Configuration Store                  │  │
//! │  │  sapiens.auth.jwt_ttl = 3600                            │  │
//! │  │  postman.smtp.host = "smtp.example.com"                 │  │
//! │  │  bucket.storage.max_size = 1073741824                 │  │
//! │  └─────────────────────────────────────────────────────────┘  │
//! │                              │                                 │
//! │        ┌────────────────────┼────────────────────┐            │
//! │        │                    │                    │            │
//! │        ▼                    ▼                    ▼            │
//! │   ┌─────────┐         ┌─────────┐         ┌─────────┐        │
//! │   │ Sapiens │         │ Postman │         │Bucket │        │
//! │   │ Module  │         │ Module  │         │ Module  │        │
//! │   └─────────┘         └─────────┘         └─────────┘        │
//! └────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use backbone_core::config::ConfigurationBus;
//!
//! // Create shared configuration bus
//! let config_bus = ConfigurationBus::new();
//!
//! // Set module configuration
//! config_bus.set("sapiens.auth.jwt_ttl", ConfigValue::Integer(3600)).await;
//! config_bus.set("postman.smtp.host", ConfigValue::String("smtp.example.com".into())).await;
//!
//! // Get configuration from any module
//! if let Some(ttl) = config_bus.get_integer("sapiens.auth.jwt_ttl").await {
//!     println!("JWT TTL: {}", ttl);
//! }
//!
//! // Subscribe to configuration changes
//! let rx = config_bus.subscribe("sapiens.*").await;
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use serde::{Deserialize, Serialize};

// ============================================================
// Configuration Value Types
// ============================================================

/// Configuration value that can hold different types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ConfigValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<ConfigValue>),
    Object(HashMap<String, ConfigValue>),
    Null,
}

impl ConfigValue {
    /// Get as string
    pub fn as_string(&self) -> Option<&str> {
        match self {
            ConfigValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get as integer
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            ConfigValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Get as float
    pub fn as_float(&self) -> Option<f64> {
        match self {
            ConfigValue::Float(f) => Some(*f),
            ConfigValue::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Get as boolean
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            ConfigValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// Check if null
    pub fn is_null(&self) -> bool {
        matches!(self, ConfigValue::Null)
    }
}

impl From<String> for ConfigValue {
    fn from(s: String) -> Self {
        ConfigValue::String(s)
    }
}

impl From<&str> for ConfigValue {
    fn from(s: &str) -> Self {
        ConfigValue::String(s.to_string())
    }
}

impl From<i64> for ConfigValue {
    fn from(i: i64) -> Self {
        ConfigValue::Integer(i)
    }
}

impl From<i32> for ConfigValue {
    fn from(i: i32) -> Self {
        ConfigValue::Integer(i as i64)
    }
}

impl From<f64> for ConfigValue {
    fn from(f: f64) -> Self {
        ConfigValue::Float(f)
    }
}

impl From<bool> for ConfigValue {
    fn from(b: bool) -> Self {
        ConfigValue::Boolean(b)
    }
}

// ============================================================
// Configuration Change Event
// ============================================================

/// Event emitted when configuration changes
#[derive(Debug, Clone)]
pub struct ConfigChangeEvent {
    /// The configuration key that changed
    pub key: String,
    /// The old value (None if newly created)
    pub old_value: Option<ConfigValue>,
    /// The new value (None if deleted)
    pub new_value: Option<ConfigValue>,
    /// Timestamp of the change
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// ============================================================
// Configuration Bus
// ============================================================

/// Shared configuration bus for cross-module configuration
///
/// Provides a thread-safe, async-friendly way for modules to share
/// configuration without direct dependencies.
#[derive(Clone)]
pub struct ConfigurationBus {
    /// The configuration store
    store: Arc<RwLock<HashMap<String, ConfigValue>>>,
    /// Broadcast channel for configuration changes
    change_tx: broadcast::Sender<ConfigChangeEvent>,
}

impl ConfigurationBus {
    /// Create a new configuration bus
    pub fn new() -> Self {
        let (change_tx, _) = broadcast::channel(1000);
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            change_tx,
        }
    }

    /// Set a configuration value
    ///
    /// Notifies all subscribers of the change.
    pub async fn set(&self, key: impl Into<String>, value: impl Into<ConfigValue>) {
        let key = key.into();
        let value = value.into();

        let old_value = {
            let mut store = self.store.write().await;
            store.insert(key.clone(), value.clone())
        };

        // Emit change event
        let event = ConfigChangeEvent {
            key,
            old_value,
            new_value: Some(value),
            timestamp: chrono::Utc::now(),
        };
        let _ = self.change_tx.send(event);
    }

    /// Set multiple configuration values at once
    pub async fn set_many(&self, values: HashMap<String, ConfigValue>) {
        let mut store = self.store.write().await;
        for (key, value) in values {
            let old_value = store.insert(key.clone(), value.clone());
            let event = ConfigChangeEvent {
                key,
                old_value,
                new_value: Some(value),
                timestamp: chrono::Utc::now(),
            };
            let _ = self.change_tx.send(event);
        }
    }

    /// Get a configuration value
    pub async fn get(&self, key: &str) -> Option<ConfigValue> {
        self.store.read().await.get(key).cloned()
    }

    /// Get a string configuration value
    pub async fn get_string(&self, key: &str) -> Option<String> {
        self.get(key).await.and_then(|v| v.as_string().map(|s| s.to_string()))
    }

    /// Get an integer configuration value
    pub async fn get_integer(&self, key: &str) -> Option<i64> {
        self.get(key).await.and_then(|v| v.as_integer())
    }

    /// Get a float configuration value
    pub async fn get_float(&self, key: &str) -> Option<f64> {
        self.get(key).await.and_then(|v| v.as_float())
    }

    /// Get a boolean configuration value
    pub async fn get_boolean(&self, key: &str) -> Option<bool> {
        self.get(key).await.and_then(|v| v.as_boolean())
    }

    /// Get a configuration value with default
    pub async fn get_or_default(&self, key: &str, default: ConfigValue) -> ConfigValue {
        self.get(key).await.unwrap_or(default)
    }

    /// Delete a configuration value
    ///
    /// Returns the deleted value if it existed.
    pub async fn delete(&self, key: &str) -> Option<ConfigValue> {
        let old_value = {
            let mut store = self.store.write().await;
            store.remove(key)
        };

        if let Some(ref old) = old_value {
            let event = ConfigChangeEvent {
                key: key.to_string(),
                old_value: Some(old.clone()),
                new_value: None,
                timestamp: chrono::Utc::now(),
            };
            let _ = self.change_tx.send(event);
        }

        old_value
    }

    /// Check if a configuration key exists
    pub async fn contains(&self, key: &str) -> bool {
        self.store.read().await.contains_key(key)
    }

    /// List all configuration keys
    pub async fn keys(&self) -> Vec<String> {
        self.store.read().await.keys().cloned().collect()
    }

    /// List configuration keys matching a prefix
    ///
    /// For example, `keys_with_prefix("sapiens.")` returns all sapiens configuration.
    pub async fn keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        self.store
            .read()
            .await
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect()
    }

    /// Get all configuration values matching a prefix
    pub async fn get_with_prefix(&self, prefix: &str) -> HashMap<String, ConfigValue> {
        self.store
            .read()
            .await
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Subscribe to configuration changes
    ///
    /// Returns a broadcast receiver that receives all configuration changes.
    pub fn subscribe(&self) -> broadcast::Receiver<ConfigChangeEvent> {
        self.change_tx.subscribe()
    }

    /// Get the total number of configuration entries
    pub async fn len(&self) -> usize {
        self.store.read().await.len()
    }

    /// Check if the configuration store is empty
    pub async fn is_empty(&self) -> bool {
        self.store.read().await.is_empty()
    }

    /// Clear all configuration values
    pub async fn clear(&self) {
        let mut store = self.store.write().await;
        store.clear();
    }

    /// Dump all configuration as a HashMap
    pub async fn dump(&self) -> HashMap<String, ConfigValue> {
        self.store.read().await.clone()
    }

    /// Load configuration from a HashMap
    ///
    /// Merges with existing configuration (overwrites duplicates).
    pub async fn load(&self, values: HashMap<String, ConfigValue>) {
        let mut store = self.store.write().await;
        for (key, value) in values {
            store.insert(key, value);
        }
    }
}

impl Default for ConfigurationBus {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_and_get() {
        let bus = ConfigurationBus::new();

        bus.set("test.key", "value").await;
        let value = bus.get_string("test.key").await;

        assert_eq!(value, Some("value".to_string()));
    }

    #[tokio::test]
    async fn test_typed_getters() {
        let bus = ConfigurationBus::new();

        bus.set("string", ConfigValue::String("hello".into())).await;
        bus.set("integer", ConfigValue::Integer(42)).await;
        bus.set("float", ConfigValue::Float(3.14)).await;
        bus.set("boolean", ConfigValue::Boolean(true)).await;

        assert_eq!(bus.get_string("string").await, Some("hello".to_string()));
        assert_eq!(bus.get_integer("integer").await, Some(42));
        assert_eq!(bus.get_float("float").await, Some(3.14));
        assert_eq!(bus.get_boolean("boolean").await, Some(true));
    }

    #[tokio::test]
    async fn test_keys_with_prefix() {
        let bus = ConfigurationBus::new();

        bus.set("sapiens.auth.jwt_ttl", 3600).await;
        bus.set("sapiens.auth.jwt_secret", "secret").await;
        bus.set("postman.smtp.host", "smtp.example.com").await;

        let sapiens_keys = bus.keys_with_prefix("sapiens.").await;
        assert_eq!(sapiens_keys.len(), 2);

        let postman_keys = bus.keys_with_prefix("postman.").await;
        assert_eq!(postman_keys.len(), 1);
    }

    #[tokio::test]
    async fn test_delete() {
        let bus = ConfigurationBus::new();

        bus.set("to.delete", "value").await;
        assert!(bus.contains("to.delete").await);

        let deleted = bus.delete("to.delete").await;
        assert!(deleted.is_some());
        assert!(!bus.contains("to.delete").await);
    }

    #[tokio::test]
    async fn test_subscription() {
        let bus = ConfigurationBus::new();
        let mut rx = bus.subscribe();

        bus.set("new.key", "new_value").await;

        let event = rx.try_recv().unwrap();
        assert_eq!(event.key, "new.key");
        assert!(event.old_value.is_none());
        assert_eq!(event.new_value, Some(ConfigValue::String("new_value".into())));
    }

    #[tokio::test]
    async fn test_get_with_prefix() {
        let bus = ConfigurationBus::new();

        bus.set("app.name", "MyApp").await;
        bus.set("app.version", "1.0.0").await;
        bus.set("db.url", "postgres://localhost").await;

        let app_config = bus.get_with_prefix("app.").await;
        assert_eq!(app_config.len(), 2);
        assert_eq!(app_config.get("app.name"), Some(&ConfigValue::String("MyApp".into())));
    }

    #[test]
    fn test_config_value_conversions() {
        let s: ConfigValue = "hello".into();
        assert_eq!(s.as_string(), Some("hello"));

        let i: ConfigValue = 42i64.into();
        assert_eq!(i.as_integer(), Some(42));

        let f: ConfigValue = 3.14f64.into();
        assert_eq!(f.as_float(), Some(3.14));

        let b: ConfigValue = true.into();
        assert_eq!(b.as_boolean(), Some(true));
    }
}
