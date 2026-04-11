//! Structured audit event types for authentication security logging
//!
//! These events provide a standardized format for security audit logging
//! across the authentication system. They are emitted via the `tracing`
//! crate and can be captured by any tracing subscriber (stdout, file,
//! OpenTelemetry, etc.).
//!
//! # Security
//!
//! Audit events NEVER contain sensitive data such as passwords, tokens,
//! password hashes, or JWT secrets. Only identifiers (user_id, email),
//! metadata (ip_address, device_type), and outcomes are logged.

use serde::Serialize;

/// Structured security audit events emitted by the auth system
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum AuditEvent {
    /// Authentication attempt started
    AuthAttemptStarted {
        email: String,
        ip_address: Option<String>,
    },

    /// Authentication succeeded
    AuthSuccess {
        user_id: String,
        ip_address: Option<String>,
        risk_score: Option<f32>,
    },

    /// Authentication failed
    AuthFailure {
        email: String,
        reason: String,
        ip_address: Option<String>,
    },

    /// Token generated
    TokenGenerated {
        user_id: String,
        token_type: String,
    },

    /// Token validated successfully
    TokenValidated {
        user_id: String,
    },

    /// Token validation failed
    TokenValidationFailed {
        reason: String,
    },

    /// Password hashed (no sensitive data included)
    PasswordHashed,

    /// Password verification completed
    PasswordVerified {
        success: bool,
    },

    /// Password validation failed requirements
    PasswordValidationFailed {
        reason: String,
    },

    /// Account status checked during auth flow
    AccountStatusChecked {
        user_id: String,
        is_active: bool,
        is_locked: bool,
    },

    /// Two-factor authentication required
    TwoFactorRequired {
        user_id: String,
    },

    /// New device detected during login
    NewDeviceDetected {
        user_id: String,
    },
}

impl AuditEvent {
    /// Get a human-readable description of the event
    pub fn description(&self) -> &'static str {
        match self {
            Self::AuthAttemptStarted { .. } => "Authentication attempt started",
            Self::AuthSuccess { .. } => "Authentication successful",
            Self::AuthFailure { .. } => "Authentication failed",
            Self::TokenGenerated { .. } => "Token generated",
            Self::TokenValidated { .. } => "Token validated",
            Self::TokenValidationFailed { .. } => "Token validation failed",
            Self::PasswordHashed => "Password hashed",
            Self::PasswordVerified { .. } => "Password verified",
            Self::PasswordValidationFailed { .. } => "Password validation failed",
            Self::AccountStatusChecked { .. } => "Account status checked",
            Self::TwoFactorRequired { .. } => "Two-factor authentication required",
            Self::NewDeviceDetected { .. } => "New device detected",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_serialization() {
        let event = AuditEvent::AuthSuccess {
            user_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            ip_address: Some("192.168.1.1".to_string()),
            risk_score: Some(0.1),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("auth_success"));
        assert!(json.contains("192.168.1.1"));
        assert!(json.contains("0.1"));
    }

    #[test]
    fn test_audit_event_failure_serialization() {
        let event = AuditEvent::AuthFailure {
            email: "user@example.com".to_string(),
            reason: "Invalid credentials".to_string(),
            ip_address: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("auth_failure"));
        assert!(json.contains("Invalid credentials"));
        // Verify no sensitive data could sneak in
        assert!(!json.contains("password"));
        assert!(!json.contains("token"));
        assert!(!json.contains("hash"));
    }

    #[test]
    fn test_audit_event_no_sensitive_fields() {
        // The AuditEvent enum by design has no fields for passwords or tokens.
        // This test documents that invariant.
        let event = AuditEvent::PasswordVerified { success: false };
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains("password_value"));
        assert!(!json.contains("secret"));
    }

    #[test]
    fn test_audit_event_description() {
        assert_eq!(
            AuditEvent::AuthSuccess {
                user_id: "123".to_string(),
                ip_address: None,
                risk_score: None,
            }.description(),
            "Authentication successful"
        );
        assert_eq!(
            AuditEvent::AuthFailure {
                email: "test@test.com".to_string(),
                reason: "locked".to_string(),
                ip_address: None,
            }.description(),
            "Authentication failed"
        );
        assert_eq!(
            AuditEvent::TwoFactorRequired {
                user_id: "123".to_string(),
            }.description(),
            "Two-factor authentication required"
        );
    }
}
