//! SMTP email provider implementation

use async_trait::async_trait;
use lettre::{
    Message, SmtpTransport, Transport,
    message::{header::ContentType, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    transport::smtp::client::{Tls, TlsParameters},
};
use crate::{
    EmailResult, EmailError, EmailService, EmailMessage, EmailDeliveryReport,
    EmailStatus, EmailServiceStats, EmailProviderConfig
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use chrono::{Utc, DateTime};

/// SMTP configuration
#[derive(Debug, Clone, serde::Serialize)]
pub struct SmtpConfig {
    /// SMTP server hostname
    pub host: String,

    /// SMTP server port
    pub port: u16,

    /// Username for authentication
    pub username: Option<String>,

    /// Password for authentication
    pub password: Option<String>,

    /// Use TLS/SSL
    pub use_tls: bool,

    /// Use SSL (implicit TLS)
    pub use_ssl: bool,

    /// Connection timeout in seconds
    pub timeout: u64,

    /// Hello name/hostname
    pub hello_name: Option<String>,
}

impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 587,
            username: None,
            password: None,
            use_tls: true,
            use_ssl: false,
            timeout: 30,
            hello_name: None,
        }
    }
}

impl EmailProviderConfig for SmtpConfig {
    fn provider_name(&self) -> &'static str {
        "smtp"
    }

    fn validate(&self) -> Result<(), String> {
        if self.host.is_empty() {
            return Err("SMTP host cannot be empty".to_string());
        }

        if self.port == 0 {
            return Err("SMTP port must be greater than 0".to_string());
        }

        if self.use_tls && self.use_ssl {
            return Err("Cannot use both TLS and SSL".to_string());
        }

        Ok(())
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }
}

/// Internal tracking data for SMTP emails
#[derive(Debug, Clone)]
struct SmtpEmailTracking {
    status: EmailStatus,
    delivered_at: Option<DateTime<Utc>>,
    error: Option<String>,
    opened_at: Option<DateTime<Utc>>,
    clicked_at: Vec<DateTime<Utc>>,
}

/// SMTP email service
pub struct SmtpEmailService {
    config: SmtpConfig,
    transport: SmtpTransport,
    // In-memory tracking for demonstration (in production, use database)
    tracking: Arc<Mutex<HashMap<String, SmtpEmailTracking>>>,
    // Simple statistics tracking
    stats: Arc<Mutex<EmailServiceStats>>,
}

impl SmtpEmailService {
    /// Create new SMTP email service
    pub fn new(config: SmtpConfig) -> EmailResult<Self> {
        config.validate()
            .map_err(EmailError::ConfigError)?;

        // Create TLS parameters with proper certificate chain
        let tls_params = TlsParameters::builder(config.host.clone())
            .build()
            .map_err(|e| EmailError::SmtpConnection(format!("TLS parameters build failed: {}", e)))?;

        // Create transport - handle SSL (port 465) vs STARTTLS (port 587)
        let transport = if config.use_ssl || config.port == 465 {
            // For SSL (port 465), use implicit TLS with builder_dangerous
            tracing::debug!("Building SMTP transport with SSL/TLS wrapper for {}:{}", config.host, config.port);

            let mut builder = SmtpTransport::builder_dangerous(&config.host)
                .port(config.port)
                .tls(Tls::Wrapper(tls_params));

            if let (Some(username), Some(password)) = (&config.username, &config.password) {
                // Security: Don't log password metadata (length, chars) - only username
                tracing::debug!("SMTP configured with username: {}", username);
                let creds = Credentials::new(username.clone(), password.clone());
                builder = builder.credentials(creds);
            }

            builder.build()
        } else if config.use_tls {
            // For STARTTLS (port 587)
            tracing::debug!("Building SMTP transport with opportunistic TLS for {}:{}", config.host, config.port);

            let mut builder = SmtpTransport::relay(&config.host)
                .map_err(|e| EmailError::SmtpConnection(format!("Relay build failed: {}", e)))?
                .port(config.port)
                .tls(Tls::Opportunistic(tls_params));

            if let (Some(username), Some(password)) = (&config.username, &config.password) {
                // Security: Don't log password metadata (length, chars) - only username
                tracing::debug!("SMTP configured with username: {}", username);
                let creds = Credentials::new(username.clone(), password.clone());
                builder = builder.credentials(creds);
            }

            builder.build()
        } else {
            // Plain connection (no TLS)
            tracing::debug!("Building plain SMTP transport for {}:{}", config.host, config.port);

            let mut builder = SmtpTransport::relay(&config.host)
                .map_err(|e| EmailError::SmtpConnection(format!("Relay build failed: {}", e)))?
                .port(config.port);

            if let (Some(username), Some(password)) = (&config.username, &config.password) {
                // Security: Don't log password metadata (length, chars) - only username
                tracing::debug!("SMTP configured with username: {}", username);
                let creds = Credentials::new(username.clone(), password.clone());
                builder = builder.credentials(creds);
            }

            builder.build()
        };

        tracing::info!(
            host = %config.host,
            port = config.port,
            use_ssl = config.use_ssl,
            use_tls = config.use_tls,
            has_credentials = config.username.is_some(),
            "SMTP transport configured successfully"
        );

        Ok(Self {
            config,
            transport,
            tracking: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(Mutex::new(EmailServiceStats::default())),
        })
    }

    /// Builder for SMTP email service
    pub fn builder() -> SmtpServiceBuilder {
        SmtpServiceBuilder::new()
    }

    /// Convert EmailMessage to lettre Message
    fn convert_message(&self, email_message: &EmailMessage) -> EmailResult<Message> {
        let mut message = Message::builder();

        // Set sender - use proper format
        let from_addr = if let Some(name) = &email_message.from.name {
            if !name.is_empty() {
                format!("{} <{}>", name, email_message.from.email)
                    .parse()
                    .map_err(|e| EmailError::InvalidEmail(format!("Invalid from address: {}", e)))?
            } else {
                email_message.from.email.clone()
                    .parse()
                    .map_err(|e| EmailError::InvalidEmail(format!("Invalid from address: {}", e)))?
            }
        } else {
            email_message.from.email.clone()
                .parse()
                .map_err(|e| EmailError::InvalidEmail(format!("Invalid from address: {}", e)))?
        };
        message = message.from(from_addr);

        // Set reply-to if specified
        if let Some(reply_to) = &email_message.reply_to {
            let reply_addr = if let Some(name) = &reply_to.name {
                if !name.is_empty() {
                    format!("{} <{}>", name, reply_to.email)
                        .parse()
                        .map_err(|e| EmailError::InvalidEmail(format!("Invalid reply-to address: {}", e)))?
                } else {
                    reply_to.email.clone()
                        .parse()
                        .map_err(|e| EmailError::InvalidEmail(format!("Invalid reply-to address: {}", e)))?
                }
            } else {
                reply_to.email.clone()
                    .parse()
                    .map_err(|e| EmailError::InvalidEmail(format!("Invalid reply-to address: {}", e)))?
            };
            message = message.reply_to(reply_addr);
        }

        // Add recipients
        for recipient in &email_message.recipients.to {
            let to_addr = if let Some(name) = &recipient.name {
                if !name.is_empty() {
                    format!("{} <{}>", name, recipient.email)
                        .parse()
                        .map_err(|e| EmailError::InvalidEmail(format!("Invalid to address: {}", e)))?
                } else {
                    recipient.email.clone()
                        .parse()
                        .map_err(|e| EmailError::InvalidEmail(format!("Invalid to address: {}", e)))?
                }
            } else {
                recipient.email.clone()
                    .parse()
                    .map_err(|e| EmailError::InvalidEmail(format!("Invalid to address: {}", e)))?
            };
            message = message.to(to_addr);
        }

        for recipient in &email_message.recipients.cc {
            let cc_addr = if let Some(name) = &recipient.name {
                if !name.is_empty() {
                    format!("{} <{}>", name, recipient.email)
                        .parse()
                        .map_err(|e| EmailError::InvalidEmail(format!("Invalid cc address: {}", e)))?
                } else {
                    recipient.email.clone()
                        .parse()
                        .map_err(|e| EmailError::InvalidEmail(format!("Invalid cc address: {}", e)))?
                }
            } else {
                recipient.email.clone()
                    .parse()
                    .map_err(|e| EmailError::InvalidEmail(format!("Invalid cc address: {}", e)))?
            };
            message = message.cc(cc_addr);
        }

        // BCC recipients are added during sending

        // Set subject
        message = message.subject(&email_message.subject);

        // Add custom headers (lettre doesn't support arbitrary custom headers easily)
        // This is a simplified approach - for production, consider using a more advanced email library
        // for now, we'll skip custom headers in SMTP implementation

        // Build message content
        let message = if email_message.html.is_some() && email_message.text.is_some() {
            // Multipart message with both HTML and text
            let text_part = SinglePart::builder()
                .header(ContentType::TEXT_PLAIN)
                .body(email_message.text.as_ref().unwrap().clone());

            let html_part = SinglePart::builder()
                .header(ContentType::TEXT_HTML)
                .body(email_message.html.as_ref().unwrap().clone());

            let multipart = MultiPart::alternative()
                .singlepart(text_part)
                .singlepart(html_part);

            message.multipart(multipart).unwrap()
        } else if let Some(html_content) = &email_message.html {
            // HTML only
            message.singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_HTML)
                    .body(html_content.clone())
            ).unwrap()
        } else if let Some(text_content) = &email_message.text {
            // Text only
            message.singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(text_content.clone())
            ).unwrap()
        } else {
            return Err(EmailError::ConfigError("Email must have either text or HTML content".to_string()));
        };

        Ok(message)
    }

    /// Render template with provided data
    fn render_template(&self, template: &str, data: &HashMap<String, serde_json::Value>) -> EmailResult<String> {
        // Simple template rendering (in production, use a proper template engine like Handlebars or Tera)
        let mut rendered = template.to_string();

        for (key, value) in data {
            let placeholder = format!("{{{{{}}}}}", key);
            let replacement = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            rendered = rendered.replace(&placeholder, &replacement);
        }

        // Also handle conditional blocks {{#if key}}...{{/if}}
        let mut result = rendered;
        for (key, value) in data {
            let if_pattern = format!("{{{{#if {}}}}}", key);
            let end_if_pattern = "{{/if}}";

            if let Some(start_idx) = result.find(&if_pattern) {
                if let Some(end_idx) = result.find(end_if_pattern) {
                    let content_between = &result[start_idx + if_pattern.len()..end_idx];
                    let should_include = match value {
                        serde_json::Value::Bool(true) => true,
                        serde_json::Value::String(s) if !s.is_empty() => true,
                        serde_json::Value::Number(n) if n.as_f64().unwrap_or(0.0) != 0.0 => true,
                        serde_json::Value::Array(a) if !a.is_empty() => true,
                        serde_json::Value::Object(o) if !o.is_empty() => true,
                        _ => false,
                    };

                    let replacement = if should_include { content_between } else { "" };
                    result = result.replace(&result[start_idx..=end_idx + end_if_pattern.len() - 1], replacement);
                }
            }
        }

        Ok(result)
    }

    /// Update statistics
    fn update_stats(&self, status: EmailStatus) {
        if let Ok(mut stats) = self.stats.lock() {
            match status {
                EmailStatus::Sent => {
                    stats.total_sent += 1;
                }
                EmailStatus::Failed => {
                    stats.total_failed += 1;
                }
                EmailStatus::Bounced => {
                    stats.total_bounced += 1;
                }
                EmailStatus::Opened => {
                    stats.total_opened += 1;
                }
                EmailStatus::Clicked => {
                    stats.total_clicked += 1;
                }
                _ => {}
            }
        }
    }
}

#[async_trait]
impl EmailService for SmtpEmailService {
    async fn send(&self, message: EmailMessage) -> EmailResult<EmailDeliveryReport> {
        // Validate message
        message.validate()
            .map_err(EmailError::Other)?;

        // Convert message
        let smtp_message = self.convert_message(&message)?;

        // Note: BCC recipients need to be handled differently in lettre
        // For simplicity, we'll skip BCC in this implementation

        // Send message
        tracing::debug!(
            from = ?message.from,
            from_email = %message.from.email,
            from_name = ?message.from.name,
            to = ?message.recipients.to.iter().map(|r| &r.email).collect::<Vec<_>>(),
            subject = %message.subject,
            "Sending SMTP email"
        );

        let send_result = self.transport.send(&smtp_message);

        tracing::debug!(
            send_result = ?send_result,
            "SMTP send result"
        );

        let (status, error) = match send_result {
            Ok(_) => (EmailStatus::Sent, None),
            Err(e) => (EmailStatus::Failed, Some(e.to_string())),
        };

        // Store tracking information
        let tracking_data = SmtpEmailTracking {
            status,
            delivered_at: if status == EmailStatus::Sent { Some(Utc::now()) } else { None },
            error: error.clone(),
            opened_at: None,
            clicked_at: Vec::new(),
        };

        if let Ok(mut tracking) = self.tracking.lock() {
            tracking.insert(message.id.clone(), tracking_data);
        }

        // Update statistics
        self.update_stats(status);

        // Create delivery report
        let report = EmailDeliveryReport {
            message_id: message.id.clone(),
            status,
            provider_message_id: None, // SMTP doesn't always provide message IDs
            delivered_at: if status == EmailStatus::Sent { Some(Utc::now()) } else { None },
            error,
            bounce_reason: None,
            opened_at: None,
            clicked_at: Vec::new(),
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("provider".to_string(), "smtp".to_string());
                meta.insert("host".to_string(), self.config.host.clone());
                meta.insert("port".to_string(), self.config.port.to_string());
                meta
            },
        };

        Ok(report)
    }

    async fn send_template(
        &self,
        template_name: &str,
        recipients: Vec<String>,
        template_data: HashMap<String, serde_json::Value>,
    ) -> EmailResult<Vec<EmailDeliveryReport>> {
        // For SMTP, we need to implement template rendering ourselves
        // This is a simplified implementation - in production, use a proper template engine

        // Get template content (in a real implementation, this would come from a template store)
        // Double braces are intentional for Handlebars/Mustache templating syntax
        #[allow(clippy::useless_format)]
        let html_template = format!(
            "<html><body><h1>Hello {{{{name}}}}!</h1><p>{{#if message}}<p>{{{{message}}}}</p>{{/if}}</p><p>Best regards,<br>{{{{company}}}}</p></body></html>"
        );

        #[allow(clippy::useless_format)]
        let text_template = format!(
            "Hello {{{{name}}}}!\n\n{{#if message}}<p>{{{{message}}}}</p>{{/if}}\n\nBest regards,\n{{{{company}}}}"
        );

        let mut reports = Vec::new();

        for recipient_email in recipients {
            // Render HTML template
            let rendered_html = self.render_template(&html_template, &template_data.clone())
                .unwrap_or_else(|_| "Template rendering failed".to_string());

            // Render text template
            let rendered_text = self.render_template(&text_template, &template_data.clone())
                .unwrap_or_else(|_| "Template rendering failed".to_string());

            // Create email message
            let email_message = EmailMessage::builder()
                .from("noreply@example.com")
                .to(recipient_email.as_str())
                .subject(format!("Template: {}", template_name))
                .html(rendered_html)
                .text(rendered_text)
                .build();

            // Send the rendered email
            let report = self.send(email_message).await?;
            reports.push(report);
        }

        Ok(reports)
    }

    async fn get_delivery_status(&self, message_id: &str) -> EmailResult<Option<EmailDeliveryReport>> {
        // Use our internal tracking system
        if let Ok(tracking) = self.tracking.lock() {
            if let Some(tracking_data) = tracking.get(message_id) {
                let report = EmailDeliveryReport {
                    message_id: message_id.to_string(),
                    status: tracking_data.status,
                    provider_message_id: None,
                    delivered_at: tracking_data.delivered_at,
                    error: tracking_data.error.clone(),
                    bounce_reason: None,
                    opened_at: tracking_data.opened_at,
                    clicked_at: tracking_data.clicked_at.clone(),
                    metadata: {
                        let mut meta = HashMap::new();
                        meta.insert("provider".to_string(), "smtp".to_string());
                        meta.insert("tracking_source".to_string(), "internal".to_string());
                        meta
                    },
                };
                return Ok(Some(report));
            }
        }
        Ok(None)
    }

    async fn cancel_scheduled(&self, message_id: &str) -> EmailResult<bool> {
        // For SMTP, scheduled emails are typically handled at the application level
        // Check if the message exists and update its status
        if let Ok(mut tracking) = self.tracking.lock() {
            if let Some(tracking_data) = tracking.get_mut(message_id) {
                // If the message is still pending, we can "cancel" it by marking as failed
                match tracking_data.status {
                    EmailStatus::Pending => {
                        tracking_data.status = EmailStatus::Failed;
                        tracking_data.error = Some("Email cancelled by user".to_string());
                        self.update_stats(EmailStatus::Failed);
                        return Ok(true);
                    }
                    _ => return Ok(false), // Cannot cancel already sent emails
                }
            }
        }
        Ok(false)
    }

    async fn get_stats(&self) -> EmailResult<EmailServiceStats> {
        // Return our internal statistics
        if let Ok(stats) = self.stats.lock() {
            Ok(stats.clone())
        } else {
            Ok(EmailServiceStats::default())
        }
    }

    async fn validate_config(&self) -> EmailResult<bool> {
        Ok(self.config.validate().is_ok())
    }

    async fn test_connection(&self) -> EmailResult<bool> {
        // Test connection by sending a test message (without actually sending)
        match self.transport.test_connection() {
            Ok(_) => Ok(true),
            Err(e) => Err(EmailError::SmtpConnection(e.to_string())),
        }
    }
}

/// SMTP service builder
pub struct SmtpServiceBuilder {
    config: SmtpConfig,
}

impl SmtpServiceBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            config: SmtpConfig::default(),
        }
    }

    /// Set SMTP host
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.config.host = host.into();
        self
    }

    /// Set SMTP port
    pub fn port(mut self, port: u16) -> Self {
        self.config.port = port;
        self
    }

    /// Set credentials
    pub fn credentials(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.config.username = Some(username.into());
        self.config.password = Some(password.into());
        self
    }

    /// Enable/disable TLS
    pub fn use_tls(mut self, enabled: bool) -> Self {
        self.config.use_tls = enabled;
        self
    }

    /// Enable/disable SSL
    pub fn use_ssl(mut self, enabled: bool) -> Self {
        self.config.use_ssl = enabled;
        self
    }

    /// Set timeout
    pub fn timeout(mut self, seconds: u64) -> Self {
        self.config.timeout = seconds;
        self
    }

    /// Set hello name
    pub fn hello_name(mut self, name: impl Into<String>) -> Self {
        self.config.hello_name = Some(name.into());
        self
    }

    /// Build SMTP service
    pub fn build(self) -> EmailResult<SmtpEmailService> {
        SmtpEmailService::new(self.config)
    }
}

impl Default for SmtpServiceBuilder {
    fn default() -> Self {
        Self::new()
    }
}