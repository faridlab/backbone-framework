# Backbone Email

🦴 **Production-ready SMTP email service library with template support**

Backbone Email is a focused email service library that provides reliable SMTP email functionality with advanced features like template sending, file attachments, and comprehensive error handling. Designed for production use with robust error handling and tracking capabilities.

## 📋 Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Usage](#usage)
- [SMTP Configuration](#smtp-configuration)
- [Technical Details](#technical-details)
- [Examples](#examples)
- [Error Handling](#error-handling)
- [Testing](#testing)

## 🎯 Overview

Backbone Email provides a clean, efficient SMTP email service that handles all common email needs without the complexity of multiple provider abstractions. It's designed for production use with comprehensive error handling and template support.

### Key Design Principles

- **SMTP Focused**: Optimized for reliable SMTP email delivery
- **Template Ready**: Built-in template rendering with Handlebars
- **Production Ready**: Robust error handling with retry mechanisms
- **Async First**: Full async/await support with tokio
- **File Attachments**: Support for multiple file types and sizes

## 🚀 Features

### 📧 **Core Email Features**
- **SMTP Support**: Traditional email servers with TLS/SSL support
- **Template System**: Built-in Handlebars template rendering
- **File Attachments**: Multiple file attachment support with type detection
- **Rich Content**: HTML, text, and multipart messages
- **Recipient Management**: To, CC, and BCC support

### 🛡️ **Production-Ready**
- **Error Handling**: Comprehensive error types with context
- **Connection Testing**: Pre-flight SMTP connection validation
- **Retry Logic**: Configurable retry mechanisms for failed deliveries
- **Async Operations**: Non-blocking email sending and receiving

### 📊 **Message Capabilities**
- **Multiple Recipients**: Send to multiple email addresses
- **Custom Headers**: Add custom SMTP headers
- **Priority Levels**: Set message priority (High, Normal, Low)
- **Reply-To**: Configure reply-to addresses
- **Bulk Sending**: Send multiple emails efficiently

## 📖 Usage

### Quick Start

```toml
[dependencies]
backbone-email = "2.0.0"
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
uuid = { version = "1.0", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
```

### Basic Email Sending

```rust
use backbone_email::{EmailService, SmtpProvider, EmailMessage};

// Create SMTP email service
let email_service = SmtpProvider::new("smtp.gmail.com", 587)
    .with_credentials("user@gmail.com", "password")
    .with_tls(true)
    .build()?;

// Create email message
let message = EmailMessage::builder()
    .from("sender@example.com")
    .to("recipient@example.com")
    .subject("Welcome to Backbone Email!")
    .text("This is a plain text email")
    .html("<h1>Welcome!</h1><p>This is HTML content.</p>")
    .build()?;

// Send email
let result = email_service.send(message).await?;
println!("Email sent with message ID: {}", result.message_id);
```

### Template Sending

```rust
use backbone_email::{EmailService, SmtpProvider};
use std::collections::HashMap;

// Create email service
let email_service = SmtpProvider::new("smtp.example.com", 587)
    .with_credentials("user", "password")
    .build()?;

// Template data
let mut template_data = HashMap::new();
template_data.insert("name".to_string(), "John Doe".into());
template_data.insert("company".to_string(), "ACME Corp".into());

// Send template email
let result = email_service.send_template(
    "welcome-template",
    vec!["user@example.com"],
    template_data
).await?;

println!("Template email sent: {}", result.message_id);
```

### Email with Attachments

```rust
use backbone_email::{EmailService, SmtpProvider, EmailAttachment};

// Create email service
let email_service = SmtpProvider::new("smtp.example.com", 587)
    .with_credentials("user", "password")
    .build()?;

// Create attachment
let attachment = EmailAttachment::builder()
    .filename("document.pdf")
    .content(pdf_bytes)
    .content_type("application/pdf")
    .build()?;

// Send email with attachment
let message = EmailMessage::builder()
    .from("sender@example.com")
    .to("recipient@example.com")
    .subject("Document Attached")
    .text("Please find the attached document")
    .attachment(attachment)
    .build()?;

let result = email_service.send(message).await?;
println!("Email with attachment sent: {}", result.message_id);
```

## 🔧 SMTP Configuration

### Basic Configuration

```rust
use backbone_email::SmtpProvider;

// Simple SMTP configuration
let email_service = SmtpProvider::new("smtp.example.com", 587)
    .with_credentials("username", "password")
    .build()?;
```

### Advanced Configuration

```rust
use backbone_email::SmtpProvider;

// Advanced SMTP configuration with TLS and custom settings
let email_service = SmtpProvider::new("smtp.gmail.com", 587)
    .with_credentials("user@gmail.com", "app_password")
    .with_tls(true)
    .with_timeout(30)
    .with_connection_pool_size(5)
    .with_default_sender("notifications@company.com")
    .build()?;
```

### Common SMTP Providers

#### Gmail/Google Workspace
```rust
let email_service = SmtpProvider::new("smtp.gmail.com", 587)
    .with_credentials("user@company.com", "app_password")
    .with_tls(true)
    .build()?;
```

#### Outlook/Office 365
```rust
let email_service = SmtpProvider::new("smtp.office365.com", 587)
    .with_credentials("user@company.com", "password")
    .with_tls(true)
    .build()?;
```

#### SendGrid
```rust
let email_service = SmtpProvider::new("smtp.sendgrid.net", 587)
    .with_credentials("apikey", "YOUR_SENDGRID_API_KEY")
    .with_tls(true)
    .build()?;
```

## 🔧 Technical Details

### Core Traits

#### EmailService Trait

```rust
#[async_trait]
pub trait EmailService: Send + Sync {
    async fn send(&self, message: EmailMessage) -> EmailResult<EmailDeliveryReport>;
    async fn send_template(
        &self,
        template_name: &str,
        recipients: Vec<String>,
        template_data: HashMap<String, serde_json::Value>,
    ) -> EmailResult<EmailDeliveryReport>;

    async fn test_connection(&self) -> EmailResult<bool>;
    async fn validate_config(&self) -> EmailResult<bool>;
}
```

### Message Types

#### EmailMessage
```rust
pub struct EmailMessage {
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub text_content: Option<String>,
    pub html_content: Option<String>,
    pub attachments: Vec<EmailAttachment>,
    pub headers: HashMap<String, String>,
    pub reply_to: Option<String>,
    pub priority: EmailPriority,
}
```

#### EmailDeliveryReport
```rust
pub struct EmailDeliveryReport {
    pub message_id: String,
    pub status: EmailStatus,
    pub provider_message_id: Option<String>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub bounce_reason: Option<String>,
}
```

### Error Handling

```rust
#[derive(Debug, thiserror::Error)]
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
}
```

## 📚 Examples

### 1. Welcome Email Service

```rust
use backbone_email::{EmailService, SmtpProvider};
use std::collections::HashMap;

async fn send_welcome_email(email_service: &dyn EmailService, user_email: &str, user_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut template_data = HashMap::new();
    template_data.insert("name".to_string(), user_name.into());
    template_data.insert("email".to_string(), user_email.into());

    let report = email_service.send_template(
        "welcome-email",
        vec![user_email],
        template_data
    ).await?;

    println!("Welcome email sent to {}: {}", user_email, report.message_id);
    Ok(())
}
```

### 2. Bulk Email Newsletter

```rust
use backbone_email::{EmailService, SmtpProvider, EmailMessage};

async fn send_newsletter(
    email_service: &dyn EmailService,
    subscribers: Vec<String>,
    subject: &str,
    content: &str
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut sent_count = 0;

    for subscriber in subscribers {
        let message = EmailMessage::builder()
            .from("newsletter@company.com")
            .to(subscriber.clone())
            .subject(subject)
            .html(content)
            .build()?;

        match email_service.send(message).await {
            Ok(_) => {
                sent_count += 1;
                println!("Newsletter sent to {}", subscriber);
            }
            Err(e) => {
                eprintln!("Failed to send to {}: {}", subscriber, e);
            }
        }
    }

    println!("Newsletter sent to {} subscribers", sent_count);
    Ok(sent_count)
}
```

### 3. Email Service Health Check

```rust
use backbone_email::EmailService;

async fn check_email_health(email_service: &dyn EmailService) -> Result<bool, Box<dyn std::error::Error>> {
    // Test configuration
    if !email_service.validate_config().await? {
        println!("❌ Email service configuration is invalid");
        return Ok(false);
    }

    // Test connection
    if !email_service.test_connection().await? {
        println!("❌ Email service connection failed");
        return Ok(false);
    }

    println!("✅ Email service is healthy");
    Ok(true)
}
```

## 🔧 Error Handling

### Error Recovery

```rust
use backbone_email::{EmailService, EmailError};

async fn send_with_retry(
    email_service: &dyn EmailService,
    message: backbone_email::EmailMessage,
    max_retries: u32
) -> Result<backbone_email::EmailDeliveryReport, Box<dyn std::error::Error>> {
    let mut retries = 0;

    loop {
        match email_service.send(message.clone()).await {
            Ok(report) => return Ok(report),
            Err(EmailError::SmtpSend(err)) if retries < max_retries => {
                retries += 1;
                println!("Send failed (attempt {}), retrying: {}", retries, err);
                tokio::time::sleep(tokio::time::Duration::from_secs(2_u64.pow(retries))).await;
            }
            Err(e) => return Err(e.into()),
        }
    }
}
```

## 🧪 Testing

### Unit Tests

```bash
# Run all backbone-email tests
cargo test --package backbone-email

# Run specific tests
cargo test --package backbone-email -- -- smtp
cargo test --package backbone-email -- -- template
```

### Mock Testing

```rust
use backbone_email::{EmailService, EmailMessage, EmailDeliveryReport};
use std::collections::HashMap;

struct MockEmailService {
    should_fail: bool,
}

#[async_trait]
impl EmailService for MockEmailService {
    async fn send(&self, message: EmailMessage) -> backbone_email::EmailResult<EmailDeliveryReport> {
        if self.should_fail {
            Err(backbone_email::EmailError::SmtpSend("Mock failure".to_string()))
        } else {
            Ok(EmailDeliveryReport {
                message_id: "mock-123".to_string(),
                status: backbone_email::EmailStatus::Sent,
                provider_message_id: Some("mock-id-123".to_string()),
                delivered_at: Some(chrono::Utc::now()),
                error: None,
                bounce_reason: None,
            })
        }
    }

    // Implement other required methods...
}
```

## 🔗 Configuration

### Environment Variables

```bash
# SMTP Configuration
SMTP_HOST=smtp.gmail.com
SMTP_PORT=587
SMTP_USERNAME=user@gmail.com
SMTP_PASSWORD=app_password
SMTP_USE_TLS=true
SMTP_DEFAULT_SENDER=notifications@company.com
```

### YAML Configuration

```yaml
# backbone-email configuration
email:
  provider: "smtp"

  smtp:
    host: "${SMTP_HOST}"
    port: ${SMTP_PORT}
    username: "${SMTP_USERNAME}"
    password: "${SMTP_PASSWORD}"
    use_tls: ${SMTP_USE_TLS}
    timeout: 30
    connection_pool_size: 5

  # Template settings
  templates:
    engine: "handlebars"
    directory: "./templates"

  # Default settings
  defaults:
    sender: "${SMTP_DEFAULT_SENDER}"
    priority: "normal"
```

## 📊 Performance Considerations

### Connection Pooling

```rust
// Reuse the email service instance
let email_service = Arc::new(
    SmtpProvider::new("smtp.gmail.com", 587)
        .with_credentials("user", "password")
        .with_connection_pool_size(10)
        .build()?
);

// Thread-safe usage across multiple tasks
for i in 0..100 {
    let service = email_service.clone();
    tokio::spawn(async move {
        let message = EmailMessage::builder()
            .from("test@example.com")
            .to(format!("user{}@example.com", i))
            .subject(format!("Test Email {}", i))
            .text("This is a test email")
            .build()?;

        let _ = service.send(message).await;
    });
}
```

## 🔄 Version History

### Current Version: 2.0.0

**Features:**
- ✅ SMTP email delivery with TLS support
- ✅ Handlebars template rendering
- ✅ File attachment support
- ✅ Connection testing and validation
- ✅ Comprehensive error handling
- ✅ Async/await support
- ✅ Builder pattern configuration

**Breaking Changes from v1.x:**
- Simplified to SMTP-only implementation
- Updated API to use builder patterns
- Enhanced error handling with context
- Removed multi-provider complexity

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🔗 Related Projects

- **[Backbone Core](../backbone-core/)** - Generic CRUD foundation
- **[Backbone CLI](../backbone-cli/)** - Code generation tools
- **[Backbone Storage](../backbone-storage/)** - File storage services
- **[Framework Documentation](../../docs/technical/)** - Complete framework guide

---

**🦴 Backbone Email - Simple, reliable SMTP email delivery**

*SMTP Focused • Template Ready • Production Ready*