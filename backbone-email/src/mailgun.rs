//! Mailgun email provider implementation

use async_trait::async_trait;
use reqwest::{Client, multipart};
use serde::Deserialize;
use crate::{
    EmailResult, EmailError, EmailService, EmailMessage, EmailDeliveryReport,
    EmailStatus, EmailServiceStats, EmailProviderConfig, EmailAttachment
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use chrono::{Utc, DateTime};

/// Mailgun configuration
#[derive(Debug, Clone, serde::Serialize)]
pub struct MailgunConfig {
    /// Mailgun API key
    pub api_key: String,

    /// Mailgun domain
    pub domain: String,

    /// API base URL (optional, defaults to US)
    pub base_url: Option<String>,

    /// Region (us or eu)
    pub region: MailgunRegion,

    /// Enable tracking
    pub enable_tracking: bool,

    /// Test mode (don't actually send emails)
    pub test_mode: bool,

    /// Additional variables
    pub variables: HashMap<String, serde_json::Value>,
}

/// Mailgun regions
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum MailgunRegion {
    US,
    EU,
}

impl MailgunRegion {
    /// Get base URL for region
    pub fn base_url(&self) -> &'static str {
        match self {
            Self::US => "https://api.mailgun.net/v3",
            Self::EU => "https://api.eu.mailgun.net/v3",
        }
    }
}

impl Default for MailgunConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            domain: String::new(),
            base_url: None,
            region: MailgunRegion::US,
            enable_tracking: true,
            test_mode: false,
            variables: HashMap::new(),
        }
    }
}

impl EmailProviderConfig for MailgunConfig {
    fn provider_name(&self) -> &'static str {
        "mailgun"
    }

    fn validate(&self) -> Result<(), String> {
        if self.api_key.is_empty() {
            return Err("Mailgun API key cannot be empty".to_string());
        }

        if self.domain.is_empty() {
            return Err("Mailgun domain cannot be empty".to_string());
        }

        Ok(())
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }
}

/// Mailgun API response
#[derive(Debug, Deserialize)]
struct MailgunResponse {
    pub id: String,
    pub message: String,
}

/// Internal tracking data for Mailgun emails
#[derive(Debug, Clone)]
struct MailgunEmailTracking {
    status: EmailStatus,
    delivered_at: Option<DateTime<Utc>>,
    error: Option<String>,
    opened_at: Option<DateTime<Utc>>,
    clicked_at: Vec<DateTime<Utc>>,
}

/// Mailgun email service
pub struct MailgunEmailService {
    config: MailgunConfig,
    client: Client,
    base_url: String,
    // In-memory tracking for demonstration (in production, use database)
    tracking: Arc<Mutex<HashMap<String, MailgunEmailTracking>>>,
    // Simple statistics tracking
    stats: Arc<Mutex<EmailServiceStats>>,
}

impl MailgunEmailService {
    /// Create new Mailgun email service
    pub fn new(config: MailgunConfig) -> EmailResult<Self> {
        config.validate()
            .map_err(|e| EmailError::ConfigError(e))?;

        let base_url = config.base_url
            .clone()
            .unwrap_or_else(|| config.region.base_url().to_string());

        let client = Client::new();

        Ok(Self {
            config,
            client,
            base_url,
            tracking: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(Mutex::new(EmailServiceStats::default())),
        })
    }

    /// Builder for Mailgun email service
    pub fn builder() -> MailgunServiceBuilder {
        MailgunServiceBuilder::new()
    }

    /// Convert EmailAddress to Mailgun format
    fn convert_address(address: &crate::EmailAddress) -> String {
        match &address.name {
            Some(name) => format!("{} <{}>", name, address.email),
            None => address.email.clone(),
        }
    }

    /// Convert attachment to multipart form
    fn convert_attachment(&self, attachment: &EmailAttachment) -> EmailResult<multipart::Part> {
        let filename = attachment.filename.clone();
        let content_type = attachment.content_type.clone();

        Ok(multipart::Part::bytes(attachment.content.clone())
            .file_name(filename)
            .mime_str(&content_type)
            .map_err(|e| EmailError::AttachmentError(e.to_string()))?)
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

    /// Get real email status from Mailgun API
    async fn get_mailgun_event_status(&self, message_id: &str) -> EmailResult<Option<EmailStatus>> {
        let url = format!("{}/events", self.base_url);

        let response = self.client
            .get(&url)
            .basic_auth("api", Some(&self.config.api_key))
            .query(&[
                ("message-id", message_id),
                ("limit", "1"),
            ])
            .send()
            .await
            .map_err(|e| EmailError::MailgunError(format!("Failed to fetch events: {}", e)))?;

        if response.status().is_success() {
            if let Ok(event_response) = response.json::<serde_json::Value>().await {
                if let Some(items) = event_response.get("items").and_then(|i| i.as_array()) {
                    if let Some(event) = items.first() {
                        if let Some(event_type) = event.get("event").and_then(|e| e.as_str()) {
                            let status = match event_type {
                                "delivered" => EmailStatus::Sent,
                                "opened" => EmailStatus::Opened,
                                "clicked" => EmailStatus::Clicked,
                                "bounced" | "failed" => EmailStatus::Failed,
                                "rejected" => EmailStatus::Bounced,
                                _ => EmailStatus::Pending,
                            };
                            return Ok(Some(status));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Get Mailgun statistics
    async fn get_mailgun_stats(&self) -> EmailResult<EmailServiceStats> {
        let url = format!("{}/stats/total", self.base_url);

        let response = self.client
            .get(&url)
            .basic_auth("api", Some(&self.config.api_key))
            .query(&[
                ("event", "accepted"),
                ("event", "delivered"),
                ("event", "opened"),
                ("event", "clicked"),
                ("event", "bounced"),
                ("event", "rejected"),
            ])
            .send()
            .await
            .map_err(|e| EmailError::MailgunError(format!("Failed to fetch stats: {}", e)))?;

        if response.status().is_success() {
            if let Ok(stats_response) = response.json::<serde_json::Value>().await {
                let mut stats = EmailServiceStats::default();

                // Parse stats from Mailgun response
                if let Some(stats_data) = stats_response.as_object() {
                    for (event, count) in stats_data {
                        if let Some(count_num) = count.as_u64() {
                            let event_str = event.clone();
                        match event_str.as_str() {
                                "accepted" | "delivered" => {
                                    stats.total_sent += count_num;
                                }
                                "bounced" | "rejected" => {
                                    stats.total_bounced += count_num;
                                }
                                "opened" => {
                                    stats.total_opened += count_num;
                                }
                                "clicked" => {
                                    stats.total_clicked += count_num;
                                }
                                _ => {}
                            }
                        }
                    }
                }

                return Ok(stats);
            }
        }

        // Fallback to internal stats
        if let Ok(internal_stats) = self.stats.lock() {
            Ok(internal_stats.clone())
        } else {
            Ok(EmailServiceStats::default())
        }
    }
}

#[async_trait]
impl EmailService for MailgunEmailService {
    async fn send(&self, message: EmailMessage) -> EmailResult<EmailDeliveryReport> {
        // Validate message
        message.validate()
            .map_err(|e| EmailError::Other(e))?;

        // Build form data
        let mut form = multipart::Form::new()
            .text("from", Self::convert_address(&message.from))
            .text("subject", message.subject);

        // Add recipients
        for recipient in &message.recipients.to {
            form = form.text("to", Self::convert_address(recipient));
        }

        for recipient in &message.recipients.cc {
            form = form.text("cc", Self::convert_address(recipient));
        }

        for recipient in &message.recipients.bcc {
            form = form.text("bcc", Self::convert_address(recipient));
        }

        // Add content
        if let Some(html_content) = &message.html {
            form = form.text("html", html_content.clone());
        }

        if let Some(text_content) = &message.text {
            form = form.text("text", text_content.clone());
        }

        // Add reply-to if specified
        if let Some(reply_to) = &message.reply_to {
            form = form.text("h:Reply-To", Self::convert_address(reply_to));
        }

        // Add custom headers
        for (key, value) in &message.headers {
            form = form.text(format!("h:{}", key), value.clone());
        }

        // Add attachments
        for attachment in &message.attachments {
            let form_part = self.convert_attachment(attachment)?;
            form = form.part("attachment", form_part);
        }

        // Add tracking settings
        if self.config.enable_tracking {
            form = form.text("o:tracking", "yes");
            if message.tracking {
                form = form.text("o:tracking-opens", "yes");
                form = form.text("o:tracking-clicks", "yes");
            }
        }

        // Add test mode
        if self.config.test_mode {
            form = form.text("o:testmode", "yes");
        }

        // Add scheduled send time
        if let Some(scheduled_at) = message.scheduled_at {
            form = form.text("o:scheduled", scheduled_at.to_rfc3339());
        }

        // Add template data if present
        if let Some(template_data) = &message.template_data {
            for (key, value) in template_data {
                form = form.text(format!("v:{}", key),
                    serde_json::to_string(value).unwrap_or_default());
            }
        }

        // Add additional variables from config
        for (key, value) in &self.config.variables {
            form = form.text(format!("v:{}", key),
                serde_json::to_string(value).unwrap_or_default());
        }

        // Make API request
        let url = format!("{}/messages", self.base_url);
        let response = self.client
            .post(&url)
            .basic_auth("api", Some(&self.config.api_key))
            .multipart(form)
            .send()
            .await
            .map_err(|e| EmailError::MailgunError(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await
                .unwrap_or_else(|_| "Failed to read error response".to_string());
            return Err(EmailError::MailgunError(format!("API error: {}", error_text)));
        }

        let mailgun_response: MailgunResponse = response.json().await
            .map_err(|e| EmailError::MailgunError(format!("Failed to parse response: {}", e)))?;

        // Extract message ID from response
        let provider_message_id = mailgun_response.id.strip_prefix("<")
            .and_then(|s| s.strip_suffix(">"))
            .map(|s| s.to_string());

        // Store tracking information
        let tracking_data = MailgunEmailTracking {
            status: EmailStatus::Sent,
            delivered_at: Some(Utc::now()),
            error: None,
            opened_at: None,
            clicked_at: Vec::new(),
        };

        if let Ok(mut tracking) = self.tracking.lock() {
            tracking.insert(message.id.clone(), tracking_data);
        }

        // Update statistics
        self.update_stats(EmailStatus::Sent);

        // Create delivery report
        let report = EmailDeliveryReport {
            message_id: message.id.clone(),
            status: EmailStatus::Sent,
            provider_message_id,
            delivered_at: Some(Utc::now()),
            error: None,
            bounce_reason: None,
            opened_at: None,
            clicked_at: Vec::new(),
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("provider".to_string(), "mailgun".to_string());
                meta.insert("domain".to_string(), self.config.domain.clone());
                meta.insert("region".to_string(), format!("{:?}", self.config.region));
                meta.insert("response_message".to_string(), mailgun_response.message);
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
        // Use Mailgun's template functionality
        let mut reports = Vec::new();

        for recipient_email in recipients {
            // Build form data for template sending
            let template_name_owned = template_name.to_string();
            let mut form = multipart::Form::new()
                .text("from", format!("templates@{}", self.config.domain))
                .text("to", recipient_email.clone())
                .text("subject", format!("Template: {}", template_name_owned))
                .text("template", template_name_owned.clone());

            // Add template variables
            for (key, value) in &template_data {
                form = form.text(format!("v:{}", key),
                    serde_json::to_string(value).unwrap_or_default());
            }

            // Add additional variables from config
            for (key, value) in &self.config.variables {
                form = form.text(format!("v:{}", key),
                    serde_json::to_string(value).unwrap_or_default());
            }

            // Add tracking settings
            if self.config.enable_tracking {
                form = form.text("o:tracking", "yes");
                form = form.text("o:tracking-opens", "yes");
                form = form.text("o:tracking-clicks", "yes");
            }

            // Add test mode
            if self.config.test_mode {
                form = form.text("o:testmode", "yes");
            }

            // Make API request
            let url = format!("{}/messages", self.base_url);
            let response = self.client
                .post(&url)
                .basic_auth("api", Some(&self.config.api_key))
                .multipart(form)
                .send()
                .await
                .map_err(|e| EmailError::MailgunError(format!("Template request failed: {}", e)))?;

            if !response.status().is_success() {
                let error_text = response.text().await
                    .unwrap_or_else(|_| "Failed to read error response".to_string());
                return Err(EmailError::MailgunError(format!("Template API error: {}", error_text)));
            }

            let mailgun_response: MailgunResponse = response.json().await
                .map_err(|e| EmailError::MailgunError(format!("Failed to parse template response: {}", e)))?;

            // Extract message ID from response
            let provider_message_id = mailgun_response.id.strip_prefix("<")
                .and_then(|s| s.strip_suffix(">"))
                .map(|s| s.to_string());

            // Create delivery report
            let report = EmailDeliveryReport {
                message_id: format!("{}_{}", template_name_owned, recipient_email),
                status: EmailStatus::Sent,
                provider_message_id,
                delivered_at: Some(Utc::now()),
                error: None,
                bounce_reason: None,
                opened_at: None,
                clicked_at: Vec::new(),
                metadata: {
                    let mut meta = HashMap::new();
                    meta.insert("provider".to_string(), "mailgun".to_string());
                    meta.insert("template_name".to_string(), template_name_owned.clone());
                    meta.insert("domain".to_string(), self.config.domain.clone());
                    meta.insert("region".to_string(), format!("{:?}", self.config.region));
                    meta
                },
            };

            reports.push(report);
            self.update_stats(EmailStatus::Sent);
        }

        Ok(reports)
    }

    async fn get_delivery_status(&self, message_id: &str) -> EmailResult<Option<EmailDeliveryReport>> {
        // Try to get real-time status from Mailgun events API
        if let Some(mailgun_status) = self.get_mailgun_event_status(message_id).await? {
            // Update our tracking with the real status
            if let Ok(mut tracking) = self.tracking.lock() {
                if let Some(tracking_data) = tracking.get_mut(message_id) {
                    tracking_data.status = mailgun_status;
                    match mailgun_status {
                        EmailStatus::Opened => {
                            if tracking_data.opened_at.is_none() {
                                tracking_data.opened_at = Some(Utc::now());
                            }
                        }
                        EmailStatus::Clicked => {
                            tracking_data.clicked_at.push(Utc::now());
                        }
                        _ => {}
                    }
                }
            }

            let report = EmailDeliveryReport {
                message_id: message_id.to_string(),
                status: mailgun_status,
                provider_message_id: None,
                delivered_at: Some(Utc::now()),
                error: None,
                bounce_reason: None,
                opened_at: if mailgun_status == EmailStatus::Opened { Some(Utc::now()) } else { None },
                clicked_at: if mailgun_status == EmailStatus::Clicked { vec![Utc::now()] } else { Vec::new() },
                metadata: {
                    let mut meta = HashMap::new();
                    meta.insert("provider".to_string(), "mailgun".to_string());
                    meta.insert("tracking_source".to_string(), "mailgun_api".to_string());
                    meta
                },
            };
            return Ok(Some(report));
        }

        // Fallback to internal tracking
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
                        meta.insert("provider".to_string(), "mailgun".to_string());
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
        // Mailgun supports canceling scheduled emails via the CANCEL route
        // First, check if we have this message in our tracking
        if let Ok(mut tracking) = self.tracking.lock() {
            if let Some(tracking_data) = tracking.get_mut(message_id) {
                match tracking_data.status {
                    EmailStatus::Pending => {
                        // Mailgun doesn't have a direct cancel API, so we handle it at application level
                        // Mark as cancelled in our tracking
                        tracking_data.status = EmailStatus::Failed;
                        tracking_data.error = Some("Scheduled email cancelled".to_string());
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
        // Use our Mailgun statistics API implementation
        self.get_mailgun_stats().await
    }

    async fn validate_config(&self) -> EmailResult<bool> {
        // Test by making a request to validate domain
        let url = format!("{}/domains/{}", self.base_url, self.config.domain);
        let response = self.client
            .get(&url)
            .basic_auth("api", Some(&self.config.api_key))
            .send()
            .await;

        match response {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(e) => Err(EmailError::MailgunError(format!("Connection test failed: {}", e))),
        }
    }

    async fn test_connection(&self) -> EmailResult<bool> {
        self.validate_config().await
    }
}

/// Mailgun service builder
pub struct MailgunServiceBuilder {
    config: MailgunConfig,
}

impl MailgunServiceBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            config: MailgunConfig::default(),
        }
    }

    /// Set API key
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.config.api_key = api_key.into();
        self
    }

    /// Set domain
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.config.domain = domain.into();
        self
    }

    /// Set region
    pub fn region(mut self, region: MailgunRegion) -> Self {
        self.config.region = region;
        self
    }

    /// Set custom base URL
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.config.base_url = Some(url.into());
        self
    }

    /// Enable/disable tracking
    pub fn tracking(mut self, enabled: bool) -> Self {
        self.config.enable_tracking = enabled;
        self
    }

    /// Enable/disable test mode
    pub fn test_mode(mut self, enabled: bool) -> Self {
        self.config.test_mode = enabled;
        self
    }

    /// Add variable
    pub fn variable(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.config.variables.insert(key.into(), value.into());
        self
    }

    /// Build Mailgun service
    pub fn build(self) -> EmailResult<MailgunEmailService> {
        MailgunEmailService::new(self.config)
    }
}

impl Default for MailgunServiceBuilder {
    fn default() -> Self {
        Self::new()
    }
}