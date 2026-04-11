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

/// Response from rate limit check
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RateLimitResponse {
    /// Whether request is allowed
    pub allowed: bool,

    /// Remaining requests in window
    pub remaining: u64,

    /// When window resets (Unix timestamp)
    pub reset_at: Option<u64>,

    /// Optional message
    pub message: Option<String>,

    /// Current request count
    pub current_count: u64,

    /// Whether limit was exceeded
    pub exceeded: bool,
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
