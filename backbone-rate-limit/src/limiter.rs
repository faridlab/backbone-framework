//! Core rate limiter implementation

use crate::storage::StorageBackend;
use crate::types::{RateLimitConfig, RateLimitResponse, RateLimitResult};

/// Core rate limiter with pluggable storage backend
pub struct RateLimiter<B: StorageBackend> {
    backend: B,
    config: RateLimitConfig,
}

impl<B: StorageBackend> RateLimiter<B> {
    pub fn new(backend: B, config: RateLimitConfig) -> Self {
        Self { backend, config }
    }

    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    pub async fn check(&self, key: &str) -> RateLimitResult<RateLimitResponse> {
        if !self.config.enabled {
            return Ok(RateLimitResponse {
                allowed: true,
                remaining: self.config.max_requests,
                reset_at: None,
                message: None,
                current_count: 0,
                exceeded: false,
            });
        }

        let count = self.backend.increment(key, &self.config).await?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let remaining = if count >= self.config.max_requests {
            0
        } else {
            self.config.max_requests.saturating_sub(count)
        };

        let exceeded = count > self.config.max_requests;

        Ok(RateLimitResponse {
            allowed: !exceeded,
            remaining,
            reset_at: Some(now + self.config.window_seconds),
            message: if exceeded {
                Some("Rate limit exceeded".to_string())
            } else {
                None
            },
            current_count: count,
            exceeded,
        })
    }

    pub async fn reset(&self, key: &str) -> RateLimitResult<()> {
        self.backend.reset(key, &self.config).await
    }
}
