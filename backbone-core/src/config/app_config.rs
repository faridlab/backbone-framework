//! Application configuration
//!
//! Defines application metadata and environment settings.

use serde::{Deserialize, Serialize};

/// Application metadata configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Application name
    pub name: String,
    /// Application version
    pub version: String,
    /// Application description
    #[serde(default)]
    pub description: Option<String>,
    /// Application author
    #[serde(default)]
    pub author: Option<String>,
    /// Enable debug mode
    #[serde(default)]
    pub debug: bool,
    /// Current environment
    #[serde(default)]
    pub environment: Environment,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: "Backbone Application".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            author: None,
            debug: true,
            environment: Environment::Development,
        }
    }
}

/// Application environment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    #[default]
    Development,
    Staging,
    Production,
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Environment::Development => write!(f, "development"),
            Environment::Staging => write!(f, "staging"),
            Environment::Production => write!(f, "production"),
        }
    }
}

impl std::str::FromStr for Environment {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "development" | "dev" => Ok(Environment::Development),
            "staging" | "stage" => Ok(Environment::Staging),
            "production" | "prod" => Ok(Environment::Production),
            _ => Err(format!("Unknown environment: {}", s)),
        }
    }
}
