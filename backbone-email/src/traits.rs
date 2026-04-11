//! Email service traits

use async_trait::async_trait;
use crate::{
    EmailResult, EmailMessage, EmailDeliveryReport,
    EmailTemplate, EmailStatus
};
use std::collections::HashMap;

/// Generic email service trait
#[async_trait]
pub trait EmailService: Send + Sync {
    /// Send single email message
    async fn send(&self, message: EmailMessage) -> EmailResult<EmailDeliveryReport>;

    /// Send multiple email messages (bulk send)
    async fn send_bulk(&self, messages: Vec<EmailMessage>) -> EmailResult<Vec<EmailDeliveryReport>> {
        let mut reports = Vec::with_capacity(messages.len());

        for message in messages {
            let report = self.send(message).await?;
            reports.push(report);
        }

        Ok(reports)
    }

    /// Send email using template
    async fn send_template(
        &self,
        template_name: &str,
        recipients: Vec<String>,
        template_data: HashMap<String, serde_json::Value>,
    ) -> EmailResult<Vec<EmailDeliveryReport>>;

    /// Get delivery status for a message
    async fn get_delivery_status(&self, message_id: &str) -> EmailResult<Option<EmailDeliveryReport>>;

    /// Cancel scheduled email
    async fn cancel_scheduled(&self, message_id: &str) -> EmailResult<bool>;

    /// Get email service statistics
    async fn get_stats(&self) -> EmailResult<EmailServiceStats>;

    /// Validate email configuration
    async fn validate_config(&self) -> EmailResult<bool>;

    /// Test email delivery
    async fn test_connection(&self) -> EmailResult<bool>;
}

/// Template management trait
#[async_trait]
pub trait TemplateManager: Send + Sync {
    /// Create new email template
    async fn create_template(&self, template: EmailTemplate) -> EmailResult<String>;

    /// Get email template by name
    async fn get_template(&self, name: &str) -> EmailResult<Option<EmailTemplate>>;

    /// Update email template
    async fn update_template(&self, name: &str, template: EmailTemplate) -> EmailResult<bool>;

    /// Delete email template
    async fn delete_template(&self, name: &str) -> EmailResult<bool>;

    /// List all templates
    async fn list_templates(&self) -> EmailResult<Vec<String>>;

    /// Render template with data
    async fn render_template(
        &self,
        template_name: &str,
        data: &HashMap<String, serde_json::Value>,
    ) -> EmailResult<TemplateRenderResult>;
}

/// Template rendering result
#[derive(Debug, Clone)]
pub struct TemplateRenderResult {
    /// Rendered subject
    pub subject: String,

    /// Rendered HTML content
    pub html: String,

    /// Rendered plain text content
    pub text: Option<String>,

    /// Render metadata
    pub metadata: HashMap<String, String>,
}

/// Email service statistics
#[derive(Debug, Clone, Default)]
pub struct EmailServiceStats {
    /// Total emails sent
    pub total_sent: u64,

    /// Total emails delivered
    pub total_delivered: u64,

    /// Total emails failed
    pub total_failed: u64,

    /// Total emails bounced
    pub total_bounced: u64,

    /// Total emails opened (if tracking enabled)
    pub total_opened: u64,

    /// Total emails clicked (if tracking enabled)
    pub total_clicked: u64,

    /// Average delivery time in milliseconds
    pub avg_delivery_time_ms: Option<f64>,

    /// Current queue size (scheduled emails)
    pub queue_size: u64,

    /// Rate limit remaining (if applicable)
    pub rate_limit_remaining: Option<u64>,

    /// Last activity timestamp
    pub last_activity: Option<chrono::DateTime<chrono::Utc>>,

    /// Provider-specific statistics
    pub provider_stats: HashMap<String, serde_json::Value>,
}

impl EmailServiceStats {
    /// Calculate delivery rate
    pub fn delivery_rate(&self) -> f64 {
        if self.total_sent > 0 {
            self.total_delivered as f64 / self.total_sent as f64
        } else {
            0.0
        }
    }

    /// Calculate failure rate
    pub fn failure_rate(&self) -> f64 {
        if self.total_sent > 0 {
            self.total_failed as f64 / self.total_sent as f64
        } else {
            0.0
        }
    }

    /// Calculate bounce rate
    pub fn bounce_rate(&self) -> f64 {
        if self.total_delivered > 0 {
            self.total_bounced as f64 / self.total_delivered as f64
        } else {
            0.0
        }
    }

    /// Calculate open rate (if tracking enabled)
    pub fn open_rate(&self) -> f64 {
        if self.total_delivered > 0 {
            self.total_opened as f64 / self.total_delivered as f64
        } else {
            0.0
        }
    }

    /// Calculate click rate (if tracking enabled)
    pub fn click_rate(&self) -> f64 {
        if self.total_opened > 0 {
            self.total_clicked as f64 / self.total_opened as f64
        } else {
            0.0
        }
    }

    /// Update statistics with new delivery report
    pub fn update_with_delivery_report(&mut self, report: &EmailDeliveryReport) {
        match report.status {
            EmailStatus::Sent => self.total_sent += 1,
            EmailStatus::Failed => {
                self.total_sent += 1;
                self.total_failed += 1;
            }
            EmailStatus::Bounced => {
                self.total_delivered += 1;
                self.total_bounced += 1;
            }
            EmailStatus::Opened => self.total_opened += 1,
            EmailStatus::Clicked => self.total_clicked += 1,
            EmailStatus::Pending => {} // No update for pending
        }

        // Update last activity
        use chrono::Utc;
        self.last_activity = Some(Utc::now());
    }
}

/// Email provider configuration trait
pub trait EmailProviderConfig {
    /// Get provider name
    fn provider_name(&self) -> &'static str;

    /// Validate configuration
    fn validate(&self) -> Result<(), String>;

    /// Get configuration as JSON
    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error>;
}

/// Email delivery webhook handler trait
#[async_trait]
pub trait WebhookHandler: Send + Sync {
    /// Handle delivery webhook
    async fn handle_delivery_webhook(
        &self,
        payload: serde_json::Value,
        signature: Option<String>,
    ) -> EmailResult<EmailDeliveryReport>;

    /// Handle bounce webhook
    async fn handle_bounce_webhook(
        &self,
        payload: serde_json::Value,
        signature: Option<String>,
    ) -> EmailResult<EmailDeliveryReport>;

    /// Handle open webhook
    async fn handle_open_webhook(
        &self,
        payload: serde_json::Value,
        signature: Option<String>,
    ) -> EmailResult<EmailDeliveryReport>;

    /// Handle click webhook
    async fn handle_click_webhook(
        &self,
        payload: serde_json::Value,
        signature: Option<String>,
    ) -> EmailResult<EmailDeliveryReport>;

    /// Verify webhook signature
    fn verify_signature(&self, payload: &str, signature: &str) -> bool;
}

/// Email queue management trait
#[async_trait]
pub trait EmailQueue: Send + Sync {
    /// Add email to queue
    async fn enqueue(&self, message: EmailMessage, scheduled_at: Option<chrono::DateTime<chrono::Utc>>) -> EmailResult<String>;

    /// Get next email from queue
    async fn dequeue(&self) -> EmailResult<Option<EmailMessage>>;

    /// Get queue size
    async fn size(&self) -> EmailResult<usize>;

    /// Clear queue
    async fn clear(&self) -> EmailResult<()>;

    /// Remove specific email from queue
    async fn remove(&self, message_id: &str) -> EmailResult<bool>;

    /// Get scheduled emails
    async fn get_scheduled(&self, limit: usize) -> EmailResult<Vec<EmailMessage>>;
}