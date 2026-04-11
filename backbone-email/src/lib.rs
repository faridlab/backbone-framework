//! Backbone Framework Email Module
//!
//! Provides email functionality with SMTP support.
//!
//! ## Features
//!
//! - **SMTP Provider**: Direct SMTP server support
//! - **Template Support**: Handlebars template rendering
//! - **Async/Await**: Full async support with tokio
//! - **Attachments**: File attachment support
//! - **Bulk Email**: Send to multiple recipients
//! - **Email Tracking**: Track delivery status
//!
//! ## Quick Start
//!
//! ```rust
//! use backbone_email::{EmailService, SmtpProvider, EmailMessage};
//!
//! // SMTP email service
//! let email_service = SmtpProvider::new("smtp.gmail.com", 587)
//!     .with_credentials("user@example.com", "password")
//!     .build();
//!
//! // Send email
//! let message = EmailMessage::builder()
//!     .from("sender@example.com")
//!     .to("recipient@example.com")
//!     .subject("Hello")
//!     .text("This is a test email")
//!     .build();
//!
//! let result = email_service.send(message).await?;
//! ```

pub mod smtp;
pub mod traits;
pub mod types;

pub use traits::*;
pub use types::*;
pub use smtp::*;

/// Email module version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default email port for SMTP
pub const DEFAULT_SMTP_PORT: u16 = 587;

/// Maximum attachment size (25MB)
pub const MAX_ATTACHMENT_SIZE: usize = 25 * 1024 * 1024;

/// Email error types
#[derive(thiserror::Error, Debug)]
pub enum EmailError {
    #[error("SMTP connection error: {0}")]
    SmtpConnection(String),

    #[error("SMTP authentication error: {0}")]
    SmtpAuth(String),

    #[error("SMTP send error: {0}")]
    SmtpSend(String),

    #[error("Template rendering error: {0}")]
    TemplateError(String),

    #[error("Invalid email address: {0}")]
    InvalidEmail(String),

    #[error("Attachment error: {0}")]
    AttachmentError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Email error: {0}")]
    Other(String),
}

/// Result type for email operations
pub type EmailResult<T> = Result<T, EmailError>;

/// Email service configuration
#[derive(Debug, Clone)]
pub struct EmailConfig {
    /// SMTP configuration (optional)
    pub smtp: Option<SmtpConfig>,

    /// Default sender email
    pub default_sender: Option<String>,

    /// Default reply-to email
    pub default_reply_to: Option<String>,

    /// Enable email tracking
    pub enable_tracking: bool,

    /// Maximum retry attempts
    pub max_retries: u32,

    /// Retry delay in seconds
    pub retry_delay: u64,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            smtp: None,
            default_sender: None,
            default_reply_to: None,
            enable_tracking: false,
            max_retries: 3,
            retry_delay: 5,
        }
    }
}

/// Email provider types
#[derive(Debug, Clone)]
pub enum EmailProvider {
    Smtp,
}

impl EmailProvider {
    /// Get provider name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Smtp => "smtp",
        }
    }
}