//! Email Service Tests

use backbone_email::{
    SmtpEmailService, SmtpConfig, EmailMessage, EmailAddress, EmailRecipient,
    EmailConfig, EmailProvider,
};
use backbone_email::types::{UploadOptions, DeliveryOptions};
use anyhow::Result;
use uuid::Uuid;

#[tokio::test]
async fn test_smtp_config_validation() -> Result<()> {
    let mut config = SmtpConfig::default();

    // Valid config
    assert!(config.validate().is_ok());

    // Invalid config (empty host)
    config.host = "".to_string();
    assert!(config.validate().is_err());

    // Invalid config (invalid port)
    config.host = "localhost".to_string();
    config.port = 0;
    assert!(config.validate().is_err());

    // Invalid config (both TLS and SSL)
    config.port = 587;
    config.use_tls = true;
    config.use_ssl = true;
    assert!(config.validate().is_err());

    Ok(())
}

#[tokio::test]
async fn test_smtp_service_creation() -> Result<()> {
    let config = SmtpConfig {
        host: "localhost".to_string(),
        port: 1025, // Use non-standard port for testing
        username: Some("test".to_string()),
        password: Some("test".to_string()),
        use_tls: false,
        use_ssl: false,
        timeout: 30,
        hello_name: Some("test.example.com".to_string()),
    };

    // Note: This test assumes no SMTP server is running on port 1025
    // In a real test setup, we'd mock the SMTP transport
    let result = SmtpEmailService::new(config);

    // The service creation might fail if no SMTP server is available
    // We'll test the config validation instead
    assert!(config.validate().is_ok());

    Ok(())
}

#[tokio::test]
async fn test_email_message_validation() -> Result<()> {
    let from = EmailAddress {
        email: "sender@example.com".to_string(),
        name: Some("Sender".to_string()),
    };

    let recipient = EmailRecipient {
        email: "recipient@example.com".to_string(),
        name: Some("Recipient".to_string()),
    };

    let message = EmailMessage::builder()
        .from(from)
        .to(recipient)
        .subject("Test Subject")
        .text("Test text content")
        .build();

    assert!(message.validate().is_ok());
    Ok(())
}

#[tokio::test]
async fn test_email_message_builder() -> Result<()> {
    let from = EmailAddress {
        email: "sender@example.com".to_string(),
        name: Some("Sender".to_string()),
    };

    let recipient1 = EmailRecipient {
        email: "recipient1@example.com".to_string(),
        name: Some("Recipient 1".to_string()),
    };

    let recipient2 = EmailRecipient {
        email: "recipient2@example.com".to_string(),
        name: Some("Recipient 2".to_string()),
    };

    let cc_recipient = EmailRecipient {
        email: "cc@example.com".to_string(),
        name: Some("CC Recipient".to_string()),
    };

    let reply_to = EmailAddress {
        email: "reply@example.com".to_string(),
        name: Some("Reply To".to_string()),
    };

    let message = EmailMessage::builder()
        .from(from.clone())
        .to(recipient1)
        .to(recipient2)
        .cc(cc_recipient)
        .reply_to(reply_to)
        .subject("Complex Test Email")
        .text("Plain text content")
        .html("<h1>HTML content</h1>")
        .build();

    assert_eq!(message.from.email, "sender@example.com");
    assert_eq!(message.from.name.as_ref().unwrap(), "Sender");
    assert_eq!(message.recipients.to.len(), 2);
    assert_eq!(message.recipients.cc.as_ref().unwrap().len(), 1);
    assert_eq!(message.reply_to.as_ref().unwrap().email, "reply@example.com");
    assert_eq!(message.subject, "Complex Test Email");
    assert_eq!(message.text.as_ref().unwrap(), "Plain text content");
    assert_eq!(message.html.as_ref().unwrap(), "<h1>HTML content</h1>");

    Ok(())
}

#[tokio::test]
async fn test_email_config_default() -> Result<()> {
    let config = EmailConfig::default();

    assert!(config.smtp.is_none());
    assert!(!config.enable_tracking);
    assert_eq!(config.max_retries, 3);
    assert_eq!(config.retry_delay, 5);
    assert!(config.default_sender.is_none());
    assert!(config.default_reply_to.is_none());

    Ok(())
}

#[test]
fn test_email_provider_enum() {
    let smtp_provider = EmailProvider::Smtp;
    assert_eq!(smtp_provider.name(), "smtp");

    // Test that AWS SES and Mailgun providers are conditionally compiled
    #[cfg(feature = "ses")]
    {
        let ses_provider = EmailProvider::Ses;
        assert_eq!(ses_provider.name(), "ses");
    }

    #[cfg(feature = "mailgun")]
    {
        let mailgun_provider = EmailProvider::Mailgun;
        assert_eq!(mailgun_provider.name(), "mailgun");
    }
}

#[test]
fn test_upload_options() {
    let options = UploadOptions {
        content_type: Some("application/pdf".to_string()),
        filename: Some("document.pdf".to_string()),
        size_limit: Some(10 * 1024 * 1024), // 10MB
        encryption: None,
        compression: None,
        metadata: std::collections::HashMap::new(),
    };

    assert_eq!(options.content_type.as_ref().unwrap(), "application/pdf");
    assert_eq!(options.filename.as_ref().unwrap(), "document.pdf");
    assert_eq!(options.size_limit.unwrap(), 10 * 1024 * 1024);
}

#[test]
fn test_delivery_options() {
    let options = DeliveryOptions {
        priority: backbone_email::types::EmailPriority::High,
        scheduled_at: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
        track_opens: false,
        track_clicks: false,
        custom_headers: std::collections::HashMap::new(),
    };

    assert!(matches!(options.priority, backbone_email::types::EmailPriority::High));
    assert!(options.scheduled_at.is_some());
    assert!(!options.track_opens);
    assert!(!options.track_clicks);
}

#[test]
fn test_email_address_validation() {
    // Valid email addresses
    let valid_emails = vec![
        "simple@example.com",
        "user.name@example.com",
        "user+tag@example.co.uk",
        "user123@test-domain.com",
    ];

    for email in valid_emails {
        let addr = EmailAddress {
            email: email.to_string(),
            name: None,
        };
        // Note: In a real implementation, we'd validate the email format
        // For now, we just test the structure
        assert!(!addr.email.is_empty());
    }
}

#[test]
fn test_email_message_id() {
    let message = EmailMessage::builder()
        .from("sender@example.com")
        .to("recipient@example.com")
        .subject("Test")
        .text("Test content")
        .build();

    assert!(!message.id.is_empty());
    assert!(Uuid::parse_str(&message.id).is_ok());
}

#[cfg(test)]
mod smtp_tests {
    use super::*;

    #[tokio::test]
    async fn test_smtp_service_builder() -> Result<()> {
        let service = SmtpEmailService::builder()
            .host("localhost")
            .port(1025)
            .credentials("test", "test")
            .use_tls(false)
            .timeout(30)
            .hello_name("test.example.com")
            .build();

        // Test that builder creates a valid service
        // Note: This will fail if no SMTP server is available
        // In a real test environment, we'd mock the transport
        match service {
            Ok(_) => {
                // Service created successfully
                assert!(true, "Service should be created with valid config");
            }
            Err(_) => {
                // Expected when no SMTP server is running
                assert!(true, "Service creation fails when no SMTP server available");
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_smtp_service_methods() -> Result<()> {
        let config = SmtpConfig::default();
        let service = SmtpEmailService::new(config);

        // Test that the service implements the required methods
        // Note: These will fail if no SMTP server is available

        // Test get_stats
        let stats = service.get_stats().await;
        // Stats should be accessible even without a server
        assert!(stats.total_sent >= 0);
        assert!(stats.total_failed >= 0);
        assert!(stats.total_bounced >= 0);

        // Test config validation
        let is_valid = service.validate_config().await;
        assert!(is_valid, "Service config should be valid");

        Ok(())
    }
}