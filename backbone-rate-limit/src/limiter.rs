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
                locked_until: None,
            });
        }

        let outcome = self.backend.increment(key, &self.config).await?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Active lockout dominates: reject regardless of window position.
        if let Some(until) = outcome.locked_until {
            if until > now {
                return Ok(RateLimitResponse {
                    allowed: false,
                    remaining: 0,
                    reset_at: Some(until),
                    message: Some("Rate limit exceeded — locked out".to_string()),
                    current_count: outcome.count,
                    exceeded: true,
                    locked_until: Some(until),
                });
            }
        }

        let remaining = if outcome.count >= self.config.max_requests {
            0
        } else {
            self.config.max_requests.saturating_sub(outcome.count)
        };

        let exceeded = outcome.count > self.config.max_requests;

        Ok(RateLimitResponse {
            allowed: !exceeded,
            remaining,
            reset_at: Some(now + self.config.window_seconds),
            message: if exceeded {
                Some("Rate limit exceeded".to_string())
            } else {
                None
            },
            current_count: outcome.count,
            exceeded,
            locked_until: None,
        })
    }

    pub async fn reset(&self, key: &str) -> RateLimitResult<()> {
        self.backend.reset(key, &self.config).await
    }
}
