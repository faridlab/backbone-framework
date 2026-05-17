//! Email Service Tests

use anyhow::Result;
use backbone_email::{
    EmailAddress, EmailConfig, EmailMessage, EmailProvider, EmailProviderConfig, SmtpConfig,
    SmtpEmailService,
};
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
        port: 1025,
        username: Some("test".to_string()),
        password: Some("test".to_string()),
        use_tls: false,
        use_ssl: false,
        timeout: 30,
        hello_name: Some("test.example.com".to_string()),
    };

    assert!(config.validate().is_ok());
    let _ = SmtpEmailService::new(config);

    Ok(())
}

#[tokio::test]
async fn test_email_message_validation() -> Result<()> {
    let from = EmailAddress {
        email: "sender@example.com".to_string(),
        name: Some("Sender".to_string()),
    };

    let recipient = EmailAddress {
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

    let recipient1 = EmailAddress {
        email: "recipient1@example.com".to_string(),
        name: Some("Recipient 1".to_string()),
    };

    let recipient2 = EmailAddress {
        email: "recipient2@example.com".to_string(),
        name: Some("Recipient 2".to_string()),
    };

    let cc_recipient = EmailAddress {
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
    assert_eq!(message.recipients.cc.len(), 1);
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
}

#[test]
fn test_email_address_validation() {
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
        assert!(!addr.email.is_empty());
    }
}

#[test]
fn test_email_message_id() {
    let message = EmailMessage::builder()
        .from(EmailAddress::new("sender@example.com"))
        .to(EmailAddress::new("recipient@example.com"))
        .subject("Test")
        .text("Test content")
        .build();

    assert!(!message.id.is_empty());
    assert!(Uuid::parse_str(&message.id).is_ok());
}
