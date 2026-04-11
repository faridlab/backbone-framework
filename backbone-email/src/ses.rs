//! AWS SES email provider implementation

use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_ses::{Client as SesClient, types::{Destination, Message, Body, Content}};
use crate::{
    EmailResult, EmailError, EmailService, EmailMessage, EmailDeliveryReport,
    EmailStatus, EmailServiceStats, EmailProviderConfig
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use chrono::{Utc, DateTime};

/// AWS SES configuration
#[derive(Debug, Clone, serde::Serialize)]
pub struct SesConfig {
    /// AWS region
    pub region: String,

    /// AWS access key ID (optional if using instance profile)
    pub access_key_id: Option<String>,

    /// AWS secret access key (optional if using instance profile)
    pub secret_access_key: Option<String>,

    /// Configuration set name (optional)
    pub configuration_set_name: Option<String>,

    /// Email sending pool name (optional)
    pub sending_pool_name: Option<String>,

    /// Template name (for template emails)
    pub template_name: Option<String>,

    /// Tags for tracking
    pub tags: HashMap<String, String>,
}

impl Default for SesConfig {
    fn default() -> Self {
        Self {
            region: "us-east-1".to_string(),
            access_key_id: None,
            secret_access_key: None,
            configuration_set_name: None,
            sending_pool_name: None,
            template_name: None,
            tags: HashMap::new(),
        }
    }
}

impl EmailProviderConfig for SesConfig {
    fn provider_name(&self) -> &'static str {
        "ses"
    }

    fn validate(&self) -> Result<(), String> {
        if self.region.is_empty() {
            return Err("AWS region cannot be empty".to_string());
        }

        Ok(())
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }
}

/// Internal tracking data for SES emails
#[derive(Debug, Clone)]
struct SesEmailTracking {
    status: EmailStatus,
    delivered_at: Option<DateTime<Utc>>,
    error: Option<String>,
    opened_at: Option<DateTime<Utc>>,
    clicked_at: Vec<DateTime<Utc>>,
}

/// AWS SES email service
pub struct SesEmailService {
    config: SesConfig,
    client: SesClient,
    // In-memory tracking for demonstration (in production, use database)
    tracking: Arc<Mutex<HashMap<String, SesEmailTracking>>>,
    // Simple statistics tracking
    stats: Arc<Mutex<EmailServiceStats>>,
}

impl SesEmailService {
    /// Create new SES email service
    pub async fn new(config: SesConfig) -> EmailResult<Self> {
        config.validate()
            .map_err(|e| EmailError::ConfigError(e))?;

        let mut aws_config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_sdk_ses::config::Region::new(config.region.clone()));

        // Set credentials if provided
        if let (Some(access_key), Some(secret_key)) = (&config.access_key_id, &config.secret_access_key) {
            aws_config = aws_config.credentials_provider(
                aws_sdk_ses::config::Credentials::new(
                    access_key.clone(),
                    secret_key.clone(),
                    None,
                    None,
                    "backbone-email",
                )
            );
        }

        let sdk_config = aws_config.load().await;
        let client = aws_sdk_ses::Client::new(&sdk_config);

        Ok(Self {
            config,
            client,
            tracking: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(Mutex::new(EmailServiceStats::default())),
        })
    }

    /// Builder for SES email service
    pub fn builder() -> SesServiceBuilder {
        SesServiceBuilder::new()
    }

    /// Convert EmailMessage to SES format
    fn convert_destination(&self, recipients: &crate::EmailRecipients) -> Destination {
        let to_addresses = recipients.to.iter().map(|r| r.email.clone()).collect::<Vec<_>>();

        let mut destination = Destination::builder()
            .to_addresses(to_addresses);

        if !recipients.cc.is_empty() {
            let cc_addresses = recipients.cc.iter().map(|r| r.email.clone()).collect::<Vec<_>>();
            destination = destination.cc_addresses(cc_addresses);
        }

        if !recipients.bcc.is_empty() {
            let bcc_addresses = recipients.bcc.iter().map(|r| r.email.clone()).collect::<Vec<_>>();
            destination = destination.bcc_addresses(bcc_addresses);
        }

        destination.build()
    }

    /// Convert EmailMessage to SES Message
    fn convert_message(&self, email_message: &EmailMessage) -> EmailResult<Message> {
        let subject = Content::builder()
            .data(email_message.subject.clone())
            .charset("UTF-8".to_string())
            .build();

        let body = if let Some(html_content) = &email_message.html {
            Body::builder()
                .html(
                    Content::builder()
                        .data(html_content.clone())
                        .charset("UTF-8")
                        .build()?
                )
                .text(
                    Content::builder()
                        .data(email_message.text.as_ref().unwrap_or(&"".to_string()).clone())
                        .charset("UTF-8")
                        .build()?
                )
                .build()
        } else if let Some(text_content) = &email_message.text {
            Body::builder()
                .text(
                    Content::builder()
                        .data(text_content.clone())
                        .charset("UTF-8")
                        .build()?
                )
                .build()
        } else {
            return Err(EmailError::Other("Email must have either text or HTML content".to_string()));
        };

        let message = Message::builder()
            .subject(
                Content::builder()
                    .data(email_message.subject.clone())
                    .charset("UTF-8")
                    .build()?
            )
            .body(body)
            .build()?;

        Ok(message)
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
impl EmailService for SesEmailService {
    async fn send(&self, message: EmailMessage) -> EmailResult<EmailDeliveryReport> {
        // Validate message
        message.validate()
            .map_err(|e| EmailError::Other(e))?;

        // Convert message
        let destination = self.convert_destination(&message.recipients);
        let ses_message = self.convert_message(&message)?;

        // Build send email request
        let mut request = self.client
            .send_email()
            .destination(destination)
            .message(ses_message)
            .source(message.from.email.clone());

        // Set configuration set if specified
        if let Some(config_set) = &self.config.configuration_set_name {
            request = request.configuration_set_name(config_set);
        }

        // Set sending pool if specified
        if let Some(pool) = &self.config.sending_pool_name {
            request = request.sending_pool_name(pool);
        }

        // Add tags
        for (key, value) in &self.config.tags {
            request = request.tags(aws_sdk_ses::types::MessageTag::builder()
                .name(key)
                .value(value)
                .build());
        }

        // Send email
        let result = request.send().await;

        let (status, error, provider_message_id) = match result {
            Ok(response) => {
                let message_id = response.message_id().map(|id| id.to_string());
                (EmailStatus::Sent, None, message_id)
            }
            Err(e) => (EmailStatus::Failed, Some(e.to_string()), None),
        };

        // Store tracking information
        let tracking_data = SesEmailTracking {
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
            provider_message_id,
            delivered_at: if status == EmailStatus::Sent { Some(Utc::now()) } else { None },
            error,
            bounce_reason: None,
            opened_at: None,
            clicked_at: Vec::new(),
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("provider".to_string(), "ses".to_string());
                meta.insert("region".to_string(), self.config.region.clone());
                if let Some(config_set) = &self.config.configuration_set_name {
                    meta.insert("configuration_set".to_string(), config_set.clone());
                }
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
        // Use SES template functionality
        let mut reports = Vec::new();

        for recipient_email in recipients {
            // Convert template data to SES format
            let ses_template_data = serde_json::to_string(&template_data)
                .map_err(|e| EmailError::Other(format!("Failed to serialize template data: {}", e)))?;

            // Build send templated email request
            let result = self.client
                .send_templated_email()
                .destination(
                    Destination::builder()
                        .to_addresses(recipient_email.clone())
                        .build()
                )
                .template(template_name)
                .template_data(ses_template_data)
                .send()
                .await;

            let (status, error, provider_message_id) = match result {
                Ok(response) => {
                    let message_id = response.message_id().map(|id| id.to_string());
                    (EmailStatus::Sent, None, message_id)
                }
                Err(e) => (EmailStatus::Failed, Some(e.to_string()), None),
            };

            // Create delivery report
            let report = EmailDeliveryReport {
                message_id: format!("{}_{}", template_name, recipient_email), // Generate a composite ID
                status,
                provider_message_id,
                delivered_at: if status == EmailStatus::Sent { Some(Utc::now()) } else { None },
                error,
                bounce_reason: None,
                opened_at: None,
                clicked_at: Vec::new(),
                metadata: {
                    let mut meta = HashMap::new();
                    meta.insert("provider".to_string(), "ses".to_string());
                    meta.insert("template_name".to_string(), template_name.to_string());
                    meta.insert("region".to_string(), self.config.region.clone());
                    meta
                },
            };

            reports.push(report);
            self.update_stats(status);
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
                        meta.insert("provider".to_string(), "ses".to_string());
                        meta.insert("tracking_source".to_string(), "internal".to_string());
                        meta.insert("region".to_string(), self.config.region.clone());
                        meta
                    },
                };
                return Ok(Some(report));
            }
        }
        Ok(None)
    }

    async fn cancel_scheduled(&self, message_id: &str) -> EmailResult<bool> {
        // For SES, scheduled emails are typically handled at the application level
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
        // Try to get real statistics from SES, fallback to internal stats
        match self.client.get_send_statistics().send().await {
            Ok(response) => {
                // Parse SES statistics response
                let mut stats = EmailServiceStats::default();

                if let Some(send_data_points) = response.send_data_points {
                    for point in send_data_points {
                        // SES provides 15-minute data points
                        stats.total_sent += point.delivery_attempts as u64;
                        stats.total_bounced += point.bounces as u64;
                        stats.total_failed += (point.complaints + point.rejects) as u64;
                    }
                }

                Ok(stats)
            }
            Err(_) => {
                // Fallback to internal statistics
                if let Ok(internal_stats) = self.stats.lock() {
                    Ok(EmailServiceStats {
                        total_sent: internal_stats.total_sent,
                        total_failed: internal_stats.total_failed,
                        total_bounced: internal_stats.total_bounced,
                        total_opened: internal_stats.total_opened,
                        total_clicked: internal_stats.total_clicked,
                        ..Default::default()
                    })
                } else {
                    Ok(EmailServiceStats::default())
                }
            }
        }
    }

    async fn validate_config(&self) -> EmailResult<bool> {
        // Test by trying to get SES quotas
        match self.client.get_send_quota().send().await {
            Ok(_) => Ok(true),
            Err(e) => Err(EmailError::SesError(e.to_string())),
        }
    }

    async fn test_connection(&self) -> EmailResult<bool> {
        // Test by trying to get SES quotas
        match self.client.get_send_quota().send().await {
            Ok(_) => Ok(true),
            Err(e) => Err(EmailError::SesError(e.to_string())),
        }
    }
}

/// SES service builder
pub struct SesServiceBuilder {
    config: SesConfig,
}

impl SesServiceBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            config: SesConfig::default(),
        }
    }

    /// Set AWS region
    pub fn region(mut self, region: impl Into<String>) -> Self {
        self.config.region = region.into();
        self
    }

    /// Set AWS credentials
    pub fn credentials(mut self, access_key_id: impl Into<String>, secret_access_key: impl Into<String>) -> Self {
        self.config.access_key_id = Some(access_key_id.into());
        self.config.secret_access_key = Some(secret_access_key.into());
        self
    }

    /// Set configuration set name
    pub fn configuration_set(mut self, name: impl Into<String>) -> Self {
        self.config.configuration_set_name = Some(name.into());
        self
    }

    /// Set sending pool name
    pub fn sending_pool(mut self, name: impl Into<String>) -> Self {
        self.config.sending_pool_name = Some(name.into());
        self
    }

    /// Add tag
    pub fn tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.tags.insert(key.into(), value.into());
        self
    }

    /// Build SES service
    pub async fn build(self) -> EmailResult<SesEmailService> {
        SesEmailService::new(self.config).await
    }
}

impl Default for SesServiceBuilder {
    fn default() -> Self {
        Self::new()
    }
}