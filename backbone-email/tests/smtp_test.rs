// Test email sending with SMTP
use backbone_email::{SmtpEmailService, SmtpConfig, EmailMessage, EmailService};
use std::env;

/// HTML template for test emails
const TEST_EMAIL_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Test Email</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 40px; background-color: #f5f5f5; }
        .container { max-width: 600px; margin: 0 auto; background-color: white; padding: 30px; border-radius: 8px; }
        .code { background-color: #f0f0f0; border: 2px dashed #007bff; padding: 15px; font-size: 24px; font-weight: bold; text-align: center; letter-spacing: 3px; margin: 20px 0; border-radius: 4px; }
    </style>
</head>
<body>
    <div class="container">
        <h1>🧪 Test Email from Bersihir</h1>
        <p>Your verification code is:</p>
        <div class="code">123456</div>
        <p>This is a test email from SMTP email integration.</p>
    </div>
</body>
</html>
"#;

#[tokio::test]
async fn test_smtp_send_email() {
    // Get environment variables for SMTP configuration
    let host = env::var("SMTP_HOST").unwrap_or_else(|_| "smtp.hostinger.com".to_string());
    let port = env::var("SMTP_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(465);
    let username = env::var("SMTP_USER").ok();
    let password = env::var("SMTP_PASSWORD").ok();
    let from_email = env::var("SMTP_FROM").unwrap_or_else(|_| "noreply@example.com".to_string());
    let from_name = env::var("SMTP_FROM_NAME").unwrap_or_else(|_| "Bersihir".to_string());
    let secure = env::var("SMTP_SECURE")
        .ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(true);
    let test_email = env::var("TEST_EMAIL").ok();

    eprintln!("Testing SMTP email sending:");
    eprintln!("  Host: {}", host);
    eprintln!("  Port: {}", port);
    eprintln!("  From: {} <{}>", from_name, from_email);
    eprintln!("  Username: {:?}", username.as_deref());
    eprintln!("  Secure: {}", secure);

    let mut config = SmtpConfig::default();
    config.host = host.clone();
    config.port = port;
    config.username = username;
    config.password = password;
    // For port 465 (SMTPS), use SSL only. For other ports, use TLS
    config.use_ssl = port == 465;
    config.use_tls = secure && port != 465;

    let smtp_service = SmtpEmailService::new(config).expect("Failed to create SMTP service");

    // Test connection
    eprintln!("\n1. Testing SMTP connection...");
    let test_result = smtp_service.test_connection().await;
    match test_result {
        Ok(_) => eprintln!("✅ SMTP connection test successful"),
        Err(e) => {
            eprintln!("❌ SMTP connection test failed: {:?}", e);
            return;
        }
    }

    // Try sending a test email
    if let Some(recipient) = test_email {
        eprintln!("\n2. Sending test email to {}...", recipient);

        let email_message = EmailMessage::builder()
            .from(from_email.as_str())
            .to(recipient.as_str())
            .subject("Test Email from Bersihir")
            .html(TEST_EMAIL_HTML)
            .build();

        match EmailService::send(&smtp_service, email_message).await {
            Ok(report) => {
                eprintln!("✅ Email sent successfully!");
                eprintln!("   Message ID: {:?}", report.message_id);
                eprintln!("   Status: {:?}", report.status);
            }
            Err(e) => {
                eprintln!("❌ Failed to send email: {:?}", e);
            }
        }
    } else {
        eprintln!("\n⚠️  No TEST_EMAIL environment variable set. Skipping email send test.");
        eprintln!("   To test email sending, run:");
        eprintln!("   TEST_EMAIL=your_email@example.com cargo test -p backbone-email smtp_test");
    }
}
