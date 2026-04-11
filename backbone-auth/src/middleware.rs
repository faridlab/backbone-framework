//! Authentication middleware

use anyhow::Result;
use async_trait::async_trait;

/// Authentication middleware trait
#[async_trait]
pub trait AuthMiddleware {
    async fn authenticate(&self, token: &str) -> Result<AuthContext>;
}

/// Trait for extracting authentication information from requests
#[async_trait]
pub trait AuthExtractor {
    async fn extract_token(&self) -> Result<Option<String>>;
    async fn extract_user_context(&self) -> Result<Option<AuthContext>>;
}

/// Authentication context
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

impl AuthContext {
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            roles: Vec::new(),
            permissions: Vec::new(),
        }
    }
}