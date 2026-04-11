//! Email types and structures

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Email address representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAddress {
    /// Email address (e.g., "user@example.com")
    pub email: String,

    /// Display name (optional, e.g., "John Doe")
    pub name: Option<String>,
}

impl EmailAddress {
    /// Create new email address
    /// Parses both "user@example.com" and "Name <user@example.com>" formats
    pub fn new(email: impl Into<String>) -> Self {
        let email_str = email.into();
        Self::parse(&email_str)
    }

    /// Parse email string, supporting both "user@example.com" and "Name <user@example.com>" formats
    fn parse(email_str: &str) -> Self {
        let trimmed = email_str.trim();

        // Check for "Name <email>" format
        if let Some(start) = trimmed.find('<') {
            if let Some(end) = trimmed.find('>') {
                if end > start {
                    let email_part = trimmed[start + 1..end].trim();
                    let name_part = trimmed[..start].trim().trim_matches('"').to_string();

                    return Self {
                        email: email_part.to_string(),
                        name: if name_part.is_empty() { None } else { Some(name_part) },
                    };
                }
            }
        }

        // Simple email format
        Self {
            email: trimmed.to_string(),
            name: None,
        }
    }

    /// Create email address with display name
    pub fn with_name(email: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            name: Some(name.into()),
        }
    }

    /// Validate email format (basic validation)
    pub fn is_valid(&self) -> bool {
        self.email.contains('@') && self.email.contains('.')
    }

    /// Get email as string format
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        match &self.name {
            Some(name) => format!("{} <{}>", name, self.email),
            None => self.email.clone(),
        }
    }
}

impl From<&str> for EmailAddress {
    fn from(email: &str) -> Self {
        Self::new(email)
    }
}

impl From<String> for EmailAddress {
    fn from(email: String) -> Self {
        Self::new(email)
    }
}

impl From<&String> for EmailAddress {
    fn from(email: &String) -> Self {
        Self::new(email.as_str())
    }
}

/// Email attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAttachment {
    /// File name
    pub filename: String,

    /// Content type (MIME type)
    pub content_type: String,

    /// File content (base64 encoded)
    pub content: Vec<u8>,

    /// Content ID for inline attachments
    pub content_id: Option<String>,

    /// Whether this is an inline attachment
    pub inline: bool,
}

impl EmailAttachment {
    /// Create new attachment from bytes
    pub fn new(
        filename: impl Into<String>,
        content_type: impl Into<String>,
        content: Vec<u8>,
    ) -> Self {
        Self {
            filename: filename.into(),
            content_type: content_type.into(),
            content,
            content_id: None,
            inline: false,
        }
    }

    /// Create inline attachment
    pub fn inline(
        filename: impl Into<String>,
        content_type: impl Into<String>,
        content: Vec<u8>,
        content_id: impl Into<String>,
    ) -> Self {
        Self {
            filename: filename.into(),
            content_type: content_type.into(),
            content,
            content_id: Some(content_id.into()),
            inline: true,
        }
    }

    /// Get file size in bytes
    pub fn size(&self) -> usize {
        self.content.len()
    }
}

/// Email message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    /// Unique message ID
    pub id: String,

    /// Sender email address
    pub from: EmailAddress,

    /// Reply-to email address (optional)
    pub reply_to: Option<EmailAddress>,

    /// Recipients (to, cc, bcc)
    pub recipients: EmailRecipients,

    /// Email subject
    pub subject: String,

    /// Plain text content
    pub text: Option<String>,

    /// HTML content
    pub html: Option<String>,

    /// Attachments
    pub attachments: Vec<EmailAttachment>,

    /// Custom headers
    pub headers: HashMap<String, String>,

    /// Template variables (for template rendering)
    pub template_data: Option<HashMap<String, serde_json::Value>>,

    /// Message creation timestamp
    pub created_at: DateTime<Utc>,

    /// Scheduled send time (optional)
    pub scheduled_at: Option<DateTime<Utc>>,

    /// Priority level
    pub priority: EmailPriority,

    /// Whether to track opens and clicks
    pub tracking: bool,
}

/// Email recipients grouped by type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailRecipients {
    /// To recipients
    pub to: Vec<EmailAddress>,

    /// CC recipients (carbon copy)
    pub cc: Vec<EmailAddress>,

    /// BCC recipients (blind carbon copy)
    pub bcc: Vec<EmailAddress>,
}

impl EmailRecipients {
    /// Create new recipients with only 'to' addresses
    pub fn new(to: Vec<EmailAddress>) -> Self {
        Self {
            to,
            cc: Vec::new(),
            bcc: Vec::new(),
        }
    }

    /// Create recipients with to, cc, and bcc
    pub fn with_cc_bcc(
        to: Vec<EmailAddress>,
        cc: Vec<EmailAddress>,
        bcc: Vec<EmailAddress>,
    ) -> Self {
        Self { to, cc, bcc }
    }

    /// Add 'to' recipient
    pub fn add_to(&mut self, recipient: EmailAddress) {
        self.to.push(recipient);
    }

    /// Add CC recipient
    pub fn add_cc(&mut self, recipient: EmailAddress) {
        self.cc.push(recipient);
    }

    /// Add BCC recipient
    pub fn add_bcc(&mut self, recipient: EmailAddress) {
        self.bcc.push(recipient);
    }

    /// Get all recipients
    pub fn all(&self) -> Vec<&EmailAddress> {
        self.to.iter()
            .chain(self.cc.iter())
            .chain(self.bcc.iter())
            .collect()
    }

    /// Get total recipient count
    pub fn count(&self) -> usize {
        self.to.len() + self.cc.len() + self.bcc.len()
    }
}

/// Email priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EmailPriority {
    /// Low priority
    Low = 1,

    /// Normal priority (default)
    #[default]
    Normal = 3,

    /// High priority
    High = 5,
}

/// Email delivery status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmailStatus {
    /// Pending to be sent
    Pending,

    /// Successfully sent
    Sent,

    /// Delivery failed
    Failed,

    /// Bounced (returned by recipient server)
    Bounced,

    /// Opened by recipient
    Opened,

    /// Clicked (for tracking emails)
    Clicked,
}

/// Email delivery report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailDeliveryReport {
    /// Message ID
    pub message_id: String,

    /// Delivery status
    pub status: EmailStatus,

    /// Provider-specific message ID
    pub provider_message_id: Option<String>,

    /// Delivery timestamp
    pub delivered_at: Option<DateTime<Utc>>,

    /// Error message (if failed)
    pub error: Option<String>,

    /// Bounce reason (if bounced)
    pub bounce_reason: Option<String>,

    /// Open tracking timestamp
    pub opened_at: Option<DateTime<Utc>>,

    /// Click tracking timestamps
    pub clicked_at: Vec<DateTime<Utc>>,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Email template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailTemplate {
    /// Template name
    pub name: String,

    /// Template subject (can include template variables)
    pub subject: String,

    /// HTML template content
    pub html_template: String,

    /// Plain text template content
    pub text_template: Option<String>,

    /// Template description
    pub description: Option<String>,

    /// Template variables schema
    pub variables: Option<HashMap<String, String>>,
}

impl EmailMessage {
    /// Create new email message builder
    pub fn builder() -> EmailMessageBuilder {
        EmailMessageBuilder::new()
    }

    /// Validate email message
    pub fn validate(&self) -> Result<(), String> {
        if !self.from.is_valid() {
            return Err("Invalid sender email address".to_string());
        }

        if self.recipients.to.is_empty() {
            return Err("At least one 'to' recipient is required".to_string());
        }

        for recipient in self.recipients.all() {
            if !recipient.is_valid() {
                return Err(format!("Invalid recipient email address: {}", recipient.email));
            }
        }

        if self.subject.is_empty() {
            return Err("Email subject cannot be empty".to_string());
        }

        if self.text.is_none() && self.html.is_none() {
            return Err("Email must have either text or HTML content".to_string());
        }

        Ok(())
    }

    /// Get primary content (HTML preferred, fallback to text)
    pub fn primary_content(&self) -> Option<&str> {
        self.html.as_deref().or(self.text.as_deref())
    }

    /// Check if message has attachments
    pub fn has_attachments(&self) -> bool {
        !self.attachments.is_empty()
    }

    /// Get total attachment size
    pub fn attachment_size(&self) -> usize {
        self.attachments.iter().map(|a| a.size()).sum()
    }
}

/// Email message builder
pub struct EmailMessageBuilder {
    message: EmailMessage,
}

impl EmailMessageBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            message: EmailMessage {
                id: Uuid::new_v4().to_string(),
                from: EmailAddress::new(""),
                reply_to: None,
                recipients: EmailRecipients::new(Vec::new()),
                subject: String::new(),
                text: None,
                html: None,
                attachments: Vec::new(),
                headers: HashMap::new(),
                template_data: None,
                created_at: Utc::now(),
                scheduled_at: None,
                priority: EmailPriority::Normal,
                tracking: false,
            },
        }
    }

    /// Set sender
    pub fn from(mut self, from: impl Into<EmailAddress>) -> Self {
        self.message.from = from.into();
        self
    }

    /// Set reply-to
    pub fn reply_to(mut self, reply_to: impl Into<EmailAddress>) -> Self {
        self.message.reply_to = Some(reply_to.into());
        self
    }

    /// Add 'to' recipient
    pub fn to(mut self, to: impl Into<EmailAddress>) -> Self {
        self.message.recipients.to.push(to.into());
        self
    }

    /// Add multiple 'to' recipients
    pub fn to_many(mut self, to: Vec<EmailAddress>) -> Self {
        self.message.recipients.to.extend(to);
        self
    }

    /// Add CC recipient
    pub fn cc(mut self, cc: impl Into<EmailAddress>) -> Self {
        self.message.recipients.cc.push(cc.into());
        self
    }

    /// Add BCC recipient
    pub fn bcc(mut self, bcc: impl Into<EmailAddress>) -> Self {
        self.message.recipients.bcc.push(bcc.into());
        self
    }

    /// Set subject
    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.message.subject = subject.into();
        self
    }

    /// Set text content
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.message.text = Some(text.into());
        self
    }

    /// Set HTML content
    pub fn html(mut self, html: impl Into<String>) -> Self {
        self.message.html = Some(html.into());
        self
    }

    /// Add attachment
    pub fn attachment(mut self, attachment: EmailAttachment) -> Self {
        self.message.attachments.push(attachment);
        self
    }

    /// Add custom header
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.message.headers.insert(key.into(), value.into());
        self
    }

    /// Set template data
    pub fn template_data(mut self, data: HashMap<String, serde_json::Value>) -> Self {
        self.message.template_data = Some(data);
        self
    }

    /// Set scheduled send time
    pub fn scheduled_at(mut self, time: DateTime<Utc>) -> Self {
        self.message.scheduled_at = Some(time);
        self
    }

    /// Set priority
    pub fn priority(mut self, priority: EmailPriority) -> Self {
        self.message.priority = priority;
        self
    }

    /// Enable/disable tracking
    pub fn tracking(mut self, enabled: bool) -> Self {
        self.message.tracking = enabled;
        self
    }

    /// Build the email message
    pub fn build(self) -> EmailMessage {
        self.message
    }
}

impl Default for EmailMessageBuilder {
    fn default() -> Self {
        Self::new()
    }
}