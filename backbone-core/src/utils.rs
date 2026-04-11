//! Backbone Framework Utilities
//!
//! Common utility functions and helpers for the Backbone framework.
//! These utilities are designed to be reused across modules.

use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};

// ============================================================
// Timestamp Utilities
// ============================================================

/// Convert prost_types::Timestamp to chrono::DateTime<Utc>
#[cfg(feature = "prost")]
pub fn prost_timestamp_to_datetime(ts: &prost_types::Timestamp) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
}

/// Convert chrono::DateTime<Utc> to prost_types::Timestamp
#[cfg(feature = "prost")]
pub fn datetime_to_prost_timestamp(dt: DateTime<Utc>) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

/// Get current timestamp
pub fn now() -> DateTime<Utc> {
    Utc::now()
}

/// Check if a timestamp is in the past
pub fn is_past(dt: DateTime<Utc>) -> bool {
    dt < Utc::now()
}

/// Check if a timestamp is in the future
pub fn is_future(dt: DateTime<Utc>) -> bool {
    dt > Utc::now()
}

/// Get timestamp for N days ago
pub fn days_ago(days: i64) -> DateTime<Utc> {
    Utc::now() - Duration::days(days)
}

/// Get timestamp for N days from now
pub fn days_from_now(days: i64) -> DateTime<Utc> {
    Utc::now() + Duration::days(days)
}

/// Get timestamp for N hours ago
pub fn hours_ago(hours: i64) -> DateTime<Utc> {
    Utc::now() - Duration::hours(hours)
}

/// Get timestamp for N hours from now
pub fn hours_from_now(hours: i64) -> DateTime<Utc> {
    Utc::now() + Duration::hours(hours)
}

/// Get timestamp for N minutes from now
pub fn minutes_from_now(minutes: i64) -> DateTime<Utc> {
    Utc::now() + Duration::minutes(minutes)
}

// ============================================================
// ID Utilities
// ============================================================

use uuid::Uuid;

/// Generate a new UUID v4
pub fn new_id() -> Uuid {
    Uuid::new_v4()
}

/// Generate a new UUID v4 as string
pub fn new_id_string() -> String {
    Uuid::new_v4().to_string()
}

/// Parse UUID from string
pub fn parse_id(s: &str) -> Option<Uuid> {
    Uuid::parse_str(s).ok()
}

/// Validate UUID format
pub fn is_valid_uuid(s: &str) -> bool {
    Uuid::parse_str(s).is_ok()
}

// ============================================================
// Pagination Utilities
// ============================================================

/// Standard pagination parameters
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PaginationParams {
    pub page: u32,
    pub limit: u32,
    pub offset: u32,
}

impl PaginationParams {
    /// Create pagination params with defaults
    pub fn new(page: u32, limit: u32) -> Self {
        let page = if page == 0 { 1 } else { page };
        let limit = limit.clamp(1, 100); // Max 100 per page
        let offset = (page - 1) * limit;

        Self { page, limit, offset }
    }

    /// Default pagination (page 1, 20 items)
    pub fn default_pagination() -> Self {
        Self::new(1, 20)
    }

    /// Calculate total pages
    pub fn total_pages(&self, total_items: u64) -> u32 {
        ((total_items as f64) / (self.limit as f64)).ceil() as u32
    }
}

/// Pagination metadata for responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationMeta {
    pub total: u64,
    pub page: u32,
    pub limit: u32,
    pub total_pages: u32,
    pub has_next: bool,
    pub has_prev: bool,
}

impl PaginationMeta {
    pub fn new(total: u64, page: u32, limit: u32) -> Self {
        let total_pages = ((total as f64) / (limit as f64)).ceil() as u32;
        Self {
            total,
            page,
            limit,
            total_pages,
            has_next: page < total_pages,
            has_prev: page > 1,
        }
    }
}

// ============================================================
// String Utilities
// ============================================================

/// Trim and lowercase a string
pub fn normalize_string(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Validate email format (basic)
pub fn is_valid_email(email: &str) -> bool {
    let email = email.trim();
    if email.is_empty() || email.len() > 254 {
        return false;
    }

    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return false;
    }

    let local = parts[0];
    let domain = parts[1];

    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

/// Validate username format
pub fn is_valid_username(username: &str) -> bool {
    let username = username.trim();
    if username.len() < 3 || username.len() > 50 {
        return false;
    }

    username.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

// ============================================================
// Error Utilities
// ============================================================

/// Common error types for backbone operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum BackboneError {
    #[error("Entity not found: {entity_type} with id {id}")]
    NotFound { entity_type: String, id: String },

    #[error("Validation error on field '{field}': {message}")]
    Validation { field: String, message: String },

    #[error("Conflict: {message}")]
    Conflict { message: String },

    #[error("Unauthorized: {message}")]
    Unauthorized { message: String },

    #[error("Forbidden: {message}")]
    Forbidden { message: String },

    #[error("Internal error: {message}")]
    Internal { message: String },

    #[error("Database error: {message}")]
    Database { message: String },
}

impl BackboneError {
    pub fn not_found(entity_type: impl Into<String>, id: impl Into<String>) -> Self {
        Self::NotFound {
            entity_type: entity_type.into(),
            id: id.into(),
        }
    }

    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Validation {
            field: field.into(),
            message: message.into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict {
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    pub fn database(message: impl Into<String>) -> Self {
        Self::Database {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_params() {
        let params = PaginationParams::new(2, 25);
        assert_eq!(params.page, 2);
        assert_eq!(params.limit, 25);
        assert_eq!(params.offset, 25);
    }

    #[test]
    fn test_pagination_meta() {
        let meta = PaginationMeta::new(100, 2, 20);
        assert_eq!(meta.total_pages, 5);
        assert!(meta.has_next);
        assert!(meta.has_prev);
    }

    #[test]
    fn test_email_validation() {
        assert!(is_valid_email("test@example.com"));
        assert!(is_valid_email("user.name@domain.co.uk"));
        assert!(!is_valid_email("invalid"));
        assert!(!is_valid_email("@domain.com"));
        assert!(!is_valid_email("user@"));
    }

    #[test]
    fn test_username_validation() {
        assert!(is_valid_username("john_doe"));
        assert!(is_valid_username("user-123"));
        assert!(!is_valid_username("ab")); // too short
        assert!(!is_valid_username("user name")); // space
    }

    #[test]
    fn test_uuid_generation() {
        let id = new_id();
        assert!(is_valid_uuid(&id.to_string()));
    }
}
