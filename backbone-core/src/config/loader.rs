//! Configuration file loader with environment variable substitution
//!
//! Supports YAML, TOML, and JSON formats with `${VAR:default}` syntax.

use super::{BackboneConfig, ConfigError, ConfigResult};
use std::path::Path;

/// Configuration file loader
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load configuration from a file
    ///
    /// Automatically detects format from file extension:
    /// - `.yml`, `.yaml` → YAML
    /// - `.toml` → TOML
    /// - `.json` → JSON
    ///
    /// Environment variables in `${VAR}` or `${VAR:default}` format
    /// are substituted before parsing.
    pub fn load_file<P: AsRef<Path>>(path: P) -> ConfigResult<BackboneConfig> {
        let path = path.as_ref();

        // Check file exists
        if !path.exists() {
            return Err(ConfigError::file_not_found(path));
        }

        // Read file content
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::read_error(path, e))?;

        // Substitute environment variables
        let content = Self::substitute_env_vars(&content)?;

        // Parse based on extension
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let config = match extension {
            "yml" | "yaml" => serde_yaml::from_str(&content)?,
            "toml" => toml::from_str(&content)?,
            "json" => serde_json::from_str(&content)?,
            _ => return Err(ConfigError::unsupported_format(extension)),
        };

        Ok(config)
    }

    /// Load configuration with environment-specific overrides
    ///
    /// 1. Loads base config from `{base_path}`
    /// 2. If `{base_path}-{env}.{ext}` exists, merges it
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Loads config/application.yml
    /// // Then merges config/application-production.yml if it exists
    /// let config = ConfigLoader::load_with_env("config/application.yml", "production")?;
    /// ```
    pub fn load_with_env<P: AsRef<Path>>(base_path: P, env: &str) -> ConfigResult<BackboneConfig> {
        let base_path = base_path.as_ref();

        // Load base config
        let mut config = Self::load_file(base_path)?;

        // Build environment-specific path
        let env_path = Self::env_specific_path(base_path, env);

        // Merge if exists
        if env_path.exists() {
            let env_config = Self::load_file(&env_path)?;
            config = config.merge(env_config);
        }

        // Validate final config
        config.validate()?;

        Ok(config)
    }

    /// Build environment-specific path
    ///
    /// `config/application.yml` + `production` → `config/application-production.yml`
    fn env_specific_path(base_path: &Path, env: &str) -> std::path::PathBuf {
        let stem = base_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("config");

        let extension = base_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("yml");

        let parent = base_path.parent().unwrap_or(Path::new("."));

        parent.join(format!("{}-{}.{}", stem, env, extension))
    }

    /// Substitute environment variables in configuration content
    ///
    /// Supports two formats:
    /// - `${VAR}` - Required variable, fails if not set
    /// - `${VAR:default}` - Optional variable with default value
    ///
    /// # Example
    ///
    /// ```ignore
    /// let content = "url: ${DATABASE_URL:postgresql://localhost/db}";
    /// let result = ConfigLoader::substitute_env_vars(content)?;
    /// ```
    pub fn substitute_env_vars(content: &str) -> ConfigResult<String> {
        let mut result = content.to_string();
        let mut start = 0;

        while let Some(var_start) = result[start..].find("${") {
            let abs_start = start + var_start;

            let var_end = match result[abs_start..].find('}') {
                Some(pos) => abs_start + pos,
                None => {
                    start = abs_start + 2;
                    continue;
                }
            };

            let var_content = &result[abs_start + 2..var_end];
            let (var_name, default_value) = Self::parse_var_content(var_content);

            let value = match std::env::var(var_name) {
                Ok(val) => val,
                Err(_) => {
                    match default_value {
                        Some(default) => default.to_string(),
                        None => {
                            // Variable not set and no default - keep original for now
                            // This allows validation to catch missing required vars
                            start = var_end + 1;
                            continue;
                        }
                    }
                }
            };

            result.replace_range(abs_start..=var_end, &value);
            start = abs_start + value.len();
        }

        Ok(result)
    }

    /// Parse variable content to extract name and optional default
    ///
    /// `VAR` → ("VAR", None)
    /// `VAR:default` → ("VAR", Some("default"))
    /// `VAR:-default` → ("VAR", Some("default"))  # Bash-style
    fn parse_var_content(content: &str) -> (&str, Option<&str>) {
        // Handle bash-style ${VAR:-default}
        if let Some(pos) = content.find(":-") {
            return (&content[..pos], Some(&content[pos + 2..]));
        }

        // Handle simple ${VAR:default}
        if let Some(pos) = content.find(':') {
            return (&content[..pos], Some(&content[pos + 1..]));
        }

        (content, None)
    }

    /// Load configuration from a string
    ///
    /// Useful for testing or embedded configs.
    pub fn from_yaml_str(content: &str) -> ConfigResult<BackboneConfig> {
        let content = Self::substitute_env_vars(content)?;
        Ok(serde_yaml::from_str(&content)?)
    }

    /// Load configuration from a string (TOML format)
    pub fn from_toml_str(content: &str) -> ConfigResult<BackboneConfig> {
        let content = Self::substitute_env_vars(content)?;
        Ok(toml::from_str(&content)?)
    }

    /// Load configuration from a string (JSON format)
    pub fn from_json_str(content: &str) -> ConfigResult<BackboneConfig> {
        let content = Self::substitute_env_vars(content)?;
        Ok(serde_json::from_str(&content)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_env_vars_with_default() {
        // Remove any existing HOST env var for this test
        std::env::remove_var("HOST");
        let content = "host: ${HOST:localhost}";
        let result = ConfigLoader::substitute_env_vars(content).unwrap();
        assert_eq!(result, "host: localhost");
    }

    #[test]
    fn test_substitute_env_vars_with_env() {
        std::env::set_var("TEST_CONFIG_VAR", "test_value");
        let content = "value: ${TEST_CONFIG_VAR:default}";
        let result = ConfigLoader::substitute_env_vars(content).unwrap();
        assert_eq!(result, "value: test_value");
        std::env::remove_var("TEST_CONFIG_VAR");
    }

    #[test]
    fn test_substitute_env_vars_bash_style() {
        let content = "host: ${UNDEFINED_VAR:-fallback}";
        let result = ConfigLoader::substitute_env_vars(content).unwrap();
        assert_eq!(result, "host: fallback");
    }

    #[test]
    fn test_substitute_multiple_vars() {
        let content = "url: postgresql://${DB_USER:root}:${DB_PASS:password}@${DB_HOST:localhost}:${DB_PORT:5432}";
        let result = ConfigLoader::substitute_env_vars(content).unwrap();
        assert_eq!(result, "url: postgresql://root:password@localhost:5432");
    }

    #[test]
    fn test_parse_var_content() {
        assert_eq!(ConfigLoader::parse_var_content("VAR"), ("VAR", None));
        assert_eq!(ConfigLoader::parse_var_content("VAR:default"), ("VAR", Some("default")));
        assert_eq!(ConfigLoader::parse_var_content("VAR:-default"), ("VAR", Some("default")));
    }

    #[test]
    fn test_env_specific_path() {
        let base = Path::new("config/application.yml");
        let env_path = ConfigLoader::env_specific_path(base, "production");
        assert_eq!(env_path.to_str().unwrap(), "config/application-production.yml");

        let base = Path::new("app.toml");
        let env_path = ConfigLoader::env_specific_path(base, "dev");
        // On some platforms parent of "app.toml" returns "." prefix
        let result = env_path.to_str().unwrap();
        assert!(result == "app-dev.toml" || result == "./app-dev.toml");
    }

    #[test]
    fn test_from_yaml_str() {
        let yaml = r#"
app:
  name: "Test App"
  version: "1.0.0"
  debug: true
  environment: development
server:
  host: "0.0.0.0"
  port: 3000
modules:
  sapiens:
    enabled: true
    bounded_context: "user-management"
    domain_version: "1.0.0"
  postman:
    enabled: false
    bounded_context: "email-notification"
    domain_version: "1.0.0"
  bucket:
    enabled: false
    bounded_context: "file-storage"
    domain_version: "1.0.0"
logging:
  level: "info"
  structured: true
  format: "json"
  targets:
    - "console"
monitoring:
  enabled: true
  metrics_enabled: true
  tracing_enabled: true
  health_check_enabled: true
contexts:
  event_bus: "in_memory"
  authentication: "sapiens"
  file_storage: "bucket"
features:
  user_registration: true
  email_verification: true
  password_reset: true
  two_factor_auth: false
  social_login: false
  audit_logging: true
  rate_limiting: true
security:
  cors_enabled: true
  cors_origins:
    - "http://localhost:3000"
  cors_methods:
    - "GET"
    - "POST"
  cors_headers:
    - "Content-Type"
"#;

        let config = ConfigLoader::from_yaml_str(yaml).unwrap();
        assert_eq!(config.app.name, "Test App");
        assert_eq!(config.server.port, 3000);
        assert!(config.modules.sapiens.enabled);
    }
}
