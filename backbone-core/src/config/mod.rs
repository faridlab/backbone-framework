//! Configuration management system for Backbone Framework
//!
//! Provides a comprehensive, type-safe configuration system with:
//! - YAML/TOML/JSON file loading
//! - Environment variable substitution (`${VAR:default}`)
//! - Validation with detailed error messages
//! - Default implementations for all config sections
//!
//! # Example
//!
//! ```ignore
//! use backbone_core::config::BackboneConfig;
//!
//! // Load from file with environment overrides
//! let config = BackboneConfig::from_file("config/application.yml")?;
//!
//! // Or load from environment only
//! let config = BackboneConfig::from_env()?;
//!
//! // Access configuration
//! println!("Server port: {}", config.server.port);
//! println!("Database URL: {}", config.database.default().url);
//! ```

mod error;
mod loader;
mod schema;
mod bus;

// Domain-specific configuration modules (split from types.rs for maintainability)
mod app_config;
mod contexts_config;
mod database_config;
mod features_config;
mod logging_config;
mod modules_config;
mod monitoring_config;
mod security_config;
mod server_config;

pub use error::{ConfigError, ConfigResult};
pub use loader::ConfigLoader;
pub use schema::{ConfigValidationError, validate_config};

// Re-export all configuration types
pub use app_config::{AppConfig, Environment};
pub use contexts_config::{ContextsConfig, RedisEventBusConfig};
pub use database_config::{CacheConfig, DatabaseConfig};
pub use features_config::{FeaturesConfig, RateLimitingConfig};
pub use logging_config::{LoggingConfig, LoggingFileConfig};
pub use modules_config::{
    ModulesConfig, PasswordHasherConfig, PostmanConfig, SapiensAuthConfig, SapiensConfig,
    SapiensLockoutConfig, SmtpConfig, StorageConfig, BucketConfig, TemplatesConfig,
};
pub use monitoring_config::MonitoringConfig;
pub use security_config::{CsrfConfig, SecurityConfig, SecurityHeadersConfig};
pub use server_config::ServerConfig;

// Configuration Bus for cross-module configuration sharing
pub use bus::{ConfigurationBus, ConfigValue, ConfigChangeEvent};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Main configuration struct for Backbone Framework
///
/// Contains all configuration sections needed to run a Backbone application.
/// Supports loading from YAML, TOML, or JSON files with environment variable
/// substitution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackboneConfig {
    /// Application metadata
    pub app: AppConfig,
    /// Server configuration
    pub server: ServerConfig,
    /// Database configurations (keyed by name)
    #[serde(default)]
    pub database: HashMap<String, DatabaseConfig>,
    /// Cache configurations (keyed by name)
    #[serde(default)]
    pub cache: HashMap<String, CacheConfig>,
    /// Module configurations
    pub modules: ModulesConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
    /// Monitoring configuration
    pub monitoring: MonitoringConfig,
    /// Cross-context communication
    pub contexts: ContextsConfig,
    /// Feature flags
    pub features: FeaturesConfig,
    /// Security settings
    pub security: SecurityConfig,
}

impl BackboneConfig {
    /// Load configuration from a file path
    ///
    /// Supports YAML (.yml, .yaml), TOML (.toml), and JSON (.json) formats.
    /// Environment variables in the format `${VAR:default}` are substituted.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = BackboneConfig::from_file("config/application.yml")?;
    /// ```
    pub fn from_file<P: AsRef<Path>>(path: P) -> ConfigResult<Self> {
        ConfigLoader::load_file(path)
    }

    /// Load configuration with environment-specific overrides
    ///
    /// Loads base config from `{base_path}` and merges with
    /// `{base_path}-{env}.{ext}` if it exists.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Loads config/application.yml + config/application-development.yml
    /// let config = BackboneConfig::from_file_with_env(
    ///     "config/application.yml",
    ///     "development"
    /// )?;
    /// ```
    pub fn from_file_with_env<P: AsRef<Path>>(base_path: P, env: &str) -> ConfigResult<Self> {
        ConfigLoader::load_with_env(base_path, env)
    }

    /// Load configuration from environment variables only
    ///
    /// Uses default values and overrides with environment variables.
    pub fn from_env() -> ConfigResult<Self> {
        let mut config = Self::default();
        config.apply_env_overrides()?;
        config.validate()?;
        Ok(config)
    }

    /// Apply environment variable overrides to configuration
    fn apply_env_overrides(&mut self) -> ConfigResult<()> {
        // Server overrides
        if let Ok(host) = std::env::var("HOST") {
            self.server.host = host;
        }
        if let Ok(port) = std::env::var("PORT") {
            self.server.port = port.parse().map_err(|_| {
                ConfigError::env_var("PORT")
            })?;
        }
        if let Ok(workers) = std::env::var("WORKERS") {
            self.server.workers = Some(workers.parse().map_err(|_| {
                ConfigError::env_var("WORKERS")
            })?);
        }

        // Database overrides
        if let Ok(url) = std::env::var("DATABASE_URL") {
            if let Some(db) = self.database.get_mut("default") {
                db.url = url;
            }
        }

        // JWT secret override
        if let Ok(secret) = std::env::var("JWT_SECRET") {
            if let Some(ref mut auth) = self.modules.sapiens.auth {
                auth.jwt_secret = secret;
            }
        }

        // Redis override
        if let Ok(url) = std::env::var("REDIS_URL") {
            if let Some(cache) = self.cache.get_mut("default") {
                cache.url = url;
            }
        }

        Ok(())
    }

    /// Validate the configuration
    ///
    /// Returns an error if any critical configuration is invalid.
    pub fn validate(&self) -> ConfigResult<()> {
        validate_config(self)
    }

    /// Get the default database configuration
    pub fn default_database(&self) -> Option<&DatabaseConfig> {
        self.database.get("default")
    }

    /// Get a database configuration by name
    pub fn get_database(&self, name: &str) -> Option<&DatabaseConfig> {
        self.database.get(name)
    }

    /// Get the default cache configuration
    pub fn default_cache(&self) -> Option<&CacheConfig> {
        self.cache.get("default")
    }

    /// Get a cache configuration by name
    pub fn get_cache(&self, name: &str) -> Option<&CacheConfig> {
        self.cache.get(name)
    }

    /// Check if a module is enabled
    pub fn is_module_enabled(&self, module_name: &str) -> bool {
        match module_name {
            "sapiens" => self.modules.sapiens.enabled,
            "postman" => self.modules.postman.enabled,
            "bucket" => self.modules.bucket.enabled,
            _ => false,
        }
    }

    /// Get the current environment
    pub fn environment(&self) -> &Environment {
        &self.app.environment
    }

    /// Check if running in production
    pub fn is_production(&self) -> bool {
        matches!(self.app.environment, Environment::Production)
    }

    /// Check if running in development
    pub fn is_development(&self) -> bool {
        matches!(self.app.environment, Environment::Development)
    }

    /// Check if debug mode is enabled
    pub fn is_debug(&self) -> bool {
        self.app.debug
    }

    /// Merge another configuration into this one
    ///
    /// Values from `other` override values in `self`.
    pub fn merge(mut self, other: Self) -> Self {
        // Merge app config
        if other.app.name != self.app.name {
            self.app = other.app;
        }

        // Merge server config
        self.server = other.server;

        // Merge databases (add/override)
        for (name, db_config) in other.database {
            self.database.insert(name, db_config);
        }

        // Merge caches (add/override)
        for (name, cache_config) in other.cache {
            self.cache.insert(name, cache_config);
        }

        // Merge modules
        self.modules = other.modules;

        // Merge logging
        self.logging = other.logging;

        // Merge monitoring
        self.monitoring = other.monitoring;

        // Merge contexts
        self.contexts = other.contexts;

        // Merge features
        self.features = other.features;

        // Merge security
        self.security = other.security;

        self
    }
}

impl Default for BackboneConfig {
    fn default() -> Self {
        let mut database = HashMap::new();
        database.insert(
            "default".to_string(),
            DatabaseConfig::default(),
        );

        let mut cache = HashMap::new();
        cache.insert(
            "default".to_string(),
            CacheConfig::default(),
        );

        Self {
            app: AppConfig::default(),
            server: ServerConfig::default(),
            database,
            cache,
            modules: ModulesConfig::default(),
            logging: LoggingConfig::default(),
            monitoring: MonitoringConfig::default(),
            contexts: ContextsConfig::default(),
            features: FeaturesConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = BackboneConfig::default();

        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 3000);
        assert!(config.default_database().is_some());
        assert!(config.default_cache().is_some());
    }

    #[test]
    fn test_module_enabled() {
        let config = BackboneConfig::default();

        assert!(config.is_module_enabled("sapiens"));
        assert!(!config.is_module_enabled("postman"));
        assert!(!config.is_module_enabled("bucket"));
        assert!(!config.is_module_enabled("unknown"));
    }

    #[test]
    fn test_environment_checks() {
        let mut config = BackboneConfig::default();

        assert!(config.is_development());
        assert!(!config.is_production());

        config.app.environment = Environment::Production;
        assert!(config.is_production());
        assert!(!config.is_development());
    }

    #[test]
    fn test_merge_configs() {
        let mut config1 = BackboneConfig::default();
        config1.server.port = 3000;

        let mut config2 = BackboneConfig::default();
        config2.server.port = 8080;
        config2.server.host = "127.0.0.1".to_string();

        let merged = config1.merge(config2);

        assert_eq!(merged.server.port, 8080);
        assert_eq!(merged.server.host, "127.0.0.1");
    }
}
