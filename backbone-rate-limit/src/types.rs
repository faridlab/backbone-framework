//! Type definitions for rate limiting
//!
//! This module provides all types used across the rate limiting system.

use serde::{Deserialize, Serialize};

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RateLimitConfig {
    /// Unique key for this rate limiter
    pub key: String,

    /// Maximum requests allowed within time window
    pub max_requests: u64,

    /// Time window in seconds
    pub window_seconds: u64,

    /// Whether rate limiting is enabled
    pub enabled: bool,

    /// Optional hard-lockout duration (seconds). When `Some`, exceeding
    /// `max_requests` within the window puts the key into a lockout state
    /// for this many seconds — every subsequent request is rejected until
    /// the lockout expires, regardless of window position. When `None`,
    /// behavior is the original window-only semantics.
    #[serde(default)]
    pub lockout_seconds: Option<u64>,
}

impl RateLimitConfig {
    /// Convenience constructor matching the original 4-field shape, with
    /// no lockout. Prefer this over struct-literal syntax in new code.
    pub fn new(
        key: impl Into<String>,
        max_requests: u64,
        window_seconds: u64,
        enabled: bool,
    ) -> Self {
        Self {
            key: key.into(),
            max_requests,
            window_seconds,
            enabled,
            lockout_seconds: None,
        }
    }

    /// Set hard-lockout duration. Once `max_requests` is exceeded inside
    /// the window, the key is locked for `seconds` and all further
    /// requests are rejected until the lock expires.
    pub fn with_lockout(mut self, seconds: u64) -> Self {
        self.lockout_seconds = Some(seconds);
        self
    }
}

/// Rate limit state stored in backend
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RateLimitState {
    /// Number of requests made within current window
    pub count: u64,

    /// First request timestamp in window
    pub window_start: Option<u64>,

    /// Last request timestamp in window
    pub window_end: Option<u64>,

    /// Whether rate limit has been exceeded
    pub exceeded: bool,
}

/// Outcome of a single increment call against a storage backend.
///
/// Carries the post-increment counter plus an optional `locked_until`
/// timestamp when hard-lockout is configured and active. The limiter
/// turns this into a `RateLimitResponse` for callers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IncrementOutcome {
    /// Current count for the key (post-increment, or unchanged if locked).
    pub count: u64,

    /// If set, the key is locked out until this unix timestamp. While
    /// locked, increments are no-ops and this same timestamp is returned.
    pub locked_until: Option<u64>,
}

/// Response from rate limit check
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RateLimitResponse {
    /// Whether request is allowed
    pub allowed: bool,

    /// Remaining requests in window
    pub remaining: u64,

    /// When window resets (Unix timestamp). When the key is locked out,
    /// this reflects the lockout-expiry timestamp.
    pub reset_at: Option<u64>,

    /// Optional message
    pub message: Option<String>,

    /// Current request count
    pub current_count: u64,

    /// Whether limit was exceeded
    pub exceeded: bool,

    /// When set, the key is currently in hard lockout until this unix
    /// timestamp. `allowed` will be `false` for the duration.
    #[serde(default)]
    pub locked_until: Option<u64>,
}

/// Result type for rate limiting operations
pub type RateLimitResult<T> = Result<T, RateLimitError>;

/// Error types for rate limiting
#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    #[error("Rate limit exceeded")]
    Exceeded,

    #[error("Failed to increment counter")]
    IncrementError,

    #[error("Failed to get counter")]
    GetError,

    #[error("Failed to reset counter")]
    ResetError,

    #[error("Redis connection error: {0}")]
    RedisConnection(String),

    #[error("Redis operation error: {0}")]
    RedisOperation(String),
}
