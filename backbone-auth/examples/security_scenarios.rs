//! Security scenarios example for backbone-auth
//! Demonstrates threat detection, attack simulation, and security monitoring

use backbone_auth::*;
use uuid::Uuid;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::{sleep, interval};
use serde::{Serialize, Deserialize};

/// Enhanced security service with threat detection capabilities
struct AdvancedSecurityService {
    failed_attempts: HashMap<String, u32>,  // Track failed attempts by email
    ip_attempts: HashMap<String, u32>,      // Track failed attempts by IP
    blocked_ips: HashMap<String, std::time::Instant>,  // Temporarily blocked IPs
    suspicious_devices: HashMap<String, std::time::Instant>,  // Suspicious device IDs
}

impl AdvancedSecurityService {
    fn new() -> Self {
        Self {
            failed_attempts: HashMap::new(),
            ip_attempts: HashMap::new(),
            blocked_ips: HashMap::new(),
            suspicious_devices: HashMap::new(),
        }
    }

    fn reset_attempt_counters(&mut self) {
        println!("🔄 Resetting security counters");
        self.failed_attempts.clear();
        self.ip_attempts.clear();
    }

    fn is_ip_blocked(&self, ip: &str) -> bool {
        if let Some(blocked_time) = self.blocked_ips.get(ip) {
            blocked_time.elapsed() < Duration::from_secs(300) // 5 minute block
        } else {
            false
        }
    }

    fn block_ip(&mut self, ip: &str) {
        println!("🚫 Blocking IP {} for 5 minutes due to suspicious activity", ip);
        self.blocked_ips.insert(ip.to_string(), std::time::Instant::now());
    }

    fn detect_anomalies(&self, user_id: &Uuid, device_info: &DeviceInfo, ip_address: Option<&str>) -> SecurityFlags {
        let mut flags = SecurityFlags {
            new_device: false,
            suspicious_location: false,
            brute_force_detected: false,
            anomaly_detected: false,
        };

        // Check for suspicious user agent patterns
        if device_info.user_agent.contains("bot") ||
           device_info.user_agent.contains("crawler") ||
           device_info.user_agent.len() < 10 {
            flags.suspicious_location = true;
            println!("🤖 Suspicious user agent detected: {}", device_info.user_agent);
        }

        // Check for rapid login attempts (simulated)
        if let Some(ip) = ip_address {
            if let Some(attempts) = self.ip_attempts.get(ip) {
                if *attempts > 3 {
                    flags.brute_force_detected = true;
                    println!("💥 Brute force attempt detected from IP: {} ({} attempts)", ip, attempts);
                }
            }
        }

        // Check for known suspicious device patterns
        if device_info.device_id.contains("temp") ||
           device_info.device_id.contains("unknown") ||
           device_info.device_id.len() < 5 {
            flags.anomaly_detected = true;
            println!("🔍 Suspicious device ID: {}", device_info.device_id);
        }

        flags
    }
}

#[async_trait::async_trait]
impl SecurityService for AdvancedSecurityService {
    async fn check_rate_limit(&self, email: &str, ip_address: Option<&str>) -> Result<()> {
        // Check if IP is blocked
        if let Some(ip) = ip_address {
            if self.is_ip_blocked(ip) {
                return Err(anyhow::anyhow!("IP address temporarily blocked due to suspicious activity"));
            }
        }

        // Check email-based rate limiting
        if let Some(attempts) = self.failed_attempts.get(email) {
            if *attempts >= 5 {
                return Err(anyhow::anyhow!("Too many failed attempts. Please try again later."));
            }
        }

        println!("✅ Rate limit check passed for {}", email);
        Ok(())
    }

    async fn analyze_login_attempt(
        &self,
        user_id: &Uuid,
        device_info: &DeviceInfo,
        ip_address: Option<&str>
    ) -> Result<SecurityFlags> {
        let mut flags = self.detect_anomalies(user_id, device_info, ip_address);

        // Additional analysis based on time patterns
        let now = chrono::Utc::now();
        let hour = now.hour();

        // Flag logins during unusual hours (2 AM - 5 AM)
        if hour >= 2 && hour <= 5 {
            flags.suspicious_location = true;
            println!("🌙 Login during unusual hours detected: {}:{} UTC", hour, now.minute());
        }

        // Check for geolocation anomalies (simulated)
        if let Some(ip) = ip_address {
            if ip.starts_with("10.") || ip.starts_with("192.168.") {
                // Internal network - less suspicious
                println!("🏢 Internal network access from: {}", ip);
            } else if ip.starts_with("203.0.113.") {
                // Known suspicious IP range (RFC 5737 test range)
                flags.suspicious_location = true;
                println!("🌍 Suspicious geographic location from: {}", ip);
            }
        }

        Ok(flags)
    }

    async fn log_failed_auth_attempt(&self, user_id: &Uuid, ip_address: Option<&str>) -> Result<()> {
        println!("🚨 SECURITY ALERT: Failed authentication for user {} from {:?}", user_id, ip_address);

        // In a real system, this would:
        // 1. Log to security monitoring system
        // 2. Send alerts to security team
        // 3. Update threat intelligence
        // 4. Potentially trigger automated responses

        sleep(Duration::from_millis(50)).await; // Simulate logging
        Ok(())
    }

    async fn log_successful_auth(&self, user_id: &Uuid, ip_address: Option<&str>) -> Result<()> {
        println!("✅ Successful authentication for user {} from {:?}", user_id, ip_address);

        // In a real system, this would:
        // 1. Update last login timestamp
        // 2. Log successful authentication
        // 3. Update user session information
        // 4. Potentially clear previous failed attempts

        sleep(Duration::from_millis(50)).await; // Simulate logging
        Ok(())
    }
}

/// Mock user database with security features
struct SecureUserDatabase {
    users: HashMap<String, User>,
    login_attempts: HashMap<String, Vec<std::time::Instant>>,
}

impl SecureUserDatabase {
    fn new() -> Self {
        let mut users = HashMap::new();
        let user_id = Uuid::new_v4();

        users.insert("admin@startapp.id".to_string(), User {
            id: user_id,
            email: "admin@startapp.id".to_string(),
            password_hash: "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$RdescudvJCsgt3ub+b+dWRWJTmaaJObG".to_string(),
            roles: vec!["admin".to_string()],
            is_active: true,
            is_locked: false,
            two_factor_enabled: true,
            two_factor_methods: vec!["totp".to_string(), "backup_codes".to_string()],
            account_expires_at: None,
            requires_password_change: false,
        });

        Self {
            users,
            login_attempts: HashMap::new(),
        }
    }

    fn record_login_attempt(&mut self, email: &str) {
        let now = std::time::Instant::now();
        let attempts = self.login_attempts.entry(email.to_string()).or_insert_with(Vec::new);
        attempts.push(now);

        // Keep only recent attempts (last hour)
        let cutoff = now - Duration::from_secs(3600);
        attempts.retain(|&time| time > cutoff);
    }

    fn get_recent_attempts(&self, email: &str) -> usize {
        self.login_attempts.get(email).map_or(0, |attempts| attempts.len())
    }
}

#[async_trait::async_trait]
impl UserRepository for SecureUserDatabase {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        Ok(self.users.get(email).cloned())
    }

    async fn save(&self, user: &User) -> Result<()> {
        println!("💾 Secure save operation for user: {}", user.email);
        Ok(())
    }

    async fn update(&self, user: &User) -> Result<()> {
        println!("🔄 Secure update operation for user: {}", user.email);
        Ok(())
    }

    async fn delete(&self, user_id: &Uuid) -> Result<()> {
        println!("🗑️ Secure delete operation for user: {}", user_id);
        Ok(())
    }

    async fn find_by_id(&self, user_id: &Uuid) -> Result<Option<User>> {
        println!("🔍 Secure user lookup by ID: {}", user_id);
        Ok(None)
    }
}

/// Security event types for monitoring
#[derive(Debug, Serialize, Deserialize)]
enum SecurityEvent {
    BruteForceAttack { ip: String, attempts: u32 },
    SuspiciousLogin { user_id: String, device_id: String, ip: String },
    BlockedIP { ip: String, reason: String },
    AccountLocked { user_id: String, reason: String },
    TokenTampering { ip: String },
    MultipleFailedLogins { email: String, count: u32 },
}

/// Security monitoring system
struct SecurityMonitor {
    events: Vec<SecurityEvent>,
}

impl SecurityMonitor {
    fn new() -> Self {
        Self { events: Vec::new() }
    }

    fn log_event(&mut self, event: SecurityEvent) {
        println!("🚨 SECURITY EVENT: {:?}", event);
        self.events.push(event);
    }

    fn generate_report(&self) -> String {
        format!(
            "Security Report:\n\
             Total Events: {}\n\
             Brute Force Attacks: {}\n\
             Suspicious Logins: {}\n\
             Blocked IPs: {}\n\
             Account Locks: {}\n",
            self.events.len(),
            self.events.iter().filter(|e| matches!(e, SecurityEvent::BruteForceAttack { .. })).count(),
            self.events.iter().filter(|e| matches!(e, SecurityEvent::SuspiciousLogin { .. })).count(),
            self.events.iter().filter(|e| matches!(e, SecurityEvent::BlockedIP { .. })).count(),
            self.events.iter().filter(|e| matches!(e, SecurityEvent::AccountLocked { .. })).count(),
        )
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Backbone Auth Security Scenarios ===\n");

    // Initialize security components
    let auth_service = AuthService::new(AuthServiceConfig {
        jwt_secret: "high_security_secret_please_change_in_production_environment".to_string(),
        token_expiry_hours: 4, // Shorter tokens for better security
        ..Default::default()
    })?;

    let user_database = SecureUserDatabase::new();
    let mut security_service = AdvancedSecurityService::new();
    let mut security_monitor = SecurityMonitor::new();

    // 1. Brute Force Attack Simulation
    println!("🎯 Scenario 1: Brute Force Attack Simulation");
    println!("Simulating automated password guessing attack...\n");

    let attack_ips = vec!["192.168.1.100", "203.0.113.45", "203.0.113.67"];
    let mut attack_count = 0;

    for (attempt, ip) in attack_ips.iter().cycle().take(20).enumerate() {
        let auth_request = AuthRequest {
            email: "admin@startapp.id".to_string(),
            password: format!("wrong_password_{}", attempt),
            ip_address: Some(ip.to_string()),
            device_info: DeviceInfo {
                device_id: format!("bot_device_{}", attempt % 3),
                user_agent: "Python-requests/2.28.1".to_string(),
                ip_address: Some(ip.to_string()),
                fingerprint: None,
            },
            remember_me: None,
        };

        // Track attempts for rate limiting simulation
        *security_service.failed_attempts.entry(auth_request.email.clone()).or_insert(0) += 1;
        *security_service.ip_attempts.entry(ip.to_string()).or_insert(0) += 1;

        match auth_service.authenticate_enhanced(
            auth_request,
            &user_database,
            &security_service
        ).await {
            Ok(_) => println!("⚠️ Unexpected success!"),
            Err(e) => {
                attack_count += 1;
                println!("   Attempt {}: {} from {} - {}", attempt + 1, "FAILED", ip, e);

                // Check if we should block this IP
                if attempt % 5 == 4 {
                    security_service.block_ip(ip);
                    security_monitor.log_event(SecurityEvent::BruteForceAttack {
                        ip: ip.to_string(),
                        attempts: 5,
                    });
                }
            }
        }

        sleep(Duration::from_millis(200)).await;
    }

    println!("\n🛡️ Brute force attack completed. Total failed attempts: {}", attack_count);
    println!();

    // 2. Token Tampering Detection
    println!("🔍 Scenario 2: Token Tampering Detection");
    let test_user_id = Uuid::new_v4();
    let valid_token = auth_service.generate_token(&test_user_id).await?;

    println!("✅ Generated valid token for user: {}", test_user_id);

    // Simulate various token tampering attempts
    let tampering_attempts = vec![
        ("Remove last character", &valid_token[..valid_token.len()-1]),
        ("Change header", "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9".to_string() + &valid_token[valid_token.find('.').unwrap_or(0)..]),
        ("Change algorithm", "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzUxMiJ9".to_string() + &valid_token[valid_token.find('.').unwrap_or(0)..]),
        ("Invalid signature", valid_token.replace(valid_token.split('.').last().unwrap_or(""), "invalid_signature")),
        ("Empty token", "".to_string()),
        ("Malformed token", "invalid.jwt.token".to_string()),
    ];

    for (description, tampered_token) in tampering_attempts {
        match auth_service.validate_token(&tampered_token).await {
            Ok(validation) => {
                if validation.valid {
                    println!("   ⚠️ {}: TAMPERING DETECTED - Token should be invalid!", description);
                    security_monitor.log_event(SecurityEvent::TokenTampering {
                        ip: "127.0.0.1".to_string(),
                    });
                } else {
                    println!("   ✅ {}: Tampering correctly detected and rejected", description);
                }
            }
            Err(_) => {
                println!("   ✅ {}: Tampering correctly detected - validation error", description);
            }
        }
    }
    println!();

    // 3. Suspicious Device Detection
    println!("🤖 Scenario 3: Suspicious Device and Bot Detection");
    let suspicious_devices = vec![
        ("Known bot user agent", "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)"),
        ("Empty user agent", ""),
        ("Suspicious pattern", "curl/7.68.0 HackerScript/1.0"),
        ("No device ID", "unknown_device_12345"),
        ("Temporary device", "temp_session_abcdef"),
    ];

    for (description, user_agent) in suspicious_devices {
        let auth_request = AuthRequest {
            email: "admin@startapp.id".to_string(),
            password: "wrong_password".to_string(),
            ip_address: Some("192.168.1.150".to_string()),
            device_info: DeviceInfo {
                device_id: if description.contains("No device ID") { "unknown_device_12345".to_string() } else { "normal_device_123".to_string() },
                user_agent: user_agent.to_string(),
                ip_address: Some("192.168.1.150".to_string()),
                fingerprint: None,
            },
            remember_me: None,
        };

        if let Err(e) = auth_service.authenticate_enhanced(
            auth_request,
            &user_database,
            &security_service
        ).await {
            println!("   {}: {} - {}", description, "BLOCKED", e);
        }
    }
    println!();

    // 4. Session Hijacking Simulation
    println!("🎭 Scenario 4: Session Hijacking Detection");
    let legitimate_ip = "192.168.1.50";
    let hijacker_ip = "203.0.113.100";

    // Legitimate login
    let legit_auth = AuthRequest {
        email: "admin@startapp.id".to_string(),
        password: "SecureAdminPass123".to_string(),
        ip_address: Some(legitimate_ip.to_string()),
        device_info: DeviceInfo {
            device_id: "trusted_laptop_123".to_string(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36".to_string(),
            ip_address: Some(legitimate_ip.to_string()),
            fingerprint: Some("fp_legitimate_user".to_string()),
        },
        remember_me: Some(true),
    };

    match auth_service.authenticate_enhanced(
        legit_auth,
        &user_database,
        &security_service
    ).await {
        Ok(result) => {
            println!("✅ Legitimate login successful from {}", legitimate_ip);

            // Simulate hijacker trying to use the token from different IP
            sleep(Duration::from_millis(1000)).await;

            println!("🚨 Simulating session hijack attempt from {}...", hijacker_ip);
            if let Some(token) = result.token {
                // In a real system, we'd check the token's original IP/device
                println!("🔍 Token validation from suspicious IP...");
                match auth_service.validate_token(&token).await {
                    Ok(validation) => {
                        if validation.valid {
                            println!("⚠️ Token is valid, but IP/location check would detect anomaly");
                            security_monitor.log_event(SecurityEvent::SuspiciousLogin {
                                user_id: result.user_id.to_string(),
                                device_id: "unknown_device".to_string(),
                                ip: hijacker_ip.to_string(),
                            });
                        }
                    }
                    Err(_) => println!("✅ Token validation failed"),
                }
            }
        }
        Err(_) => println!("❌ Legitimate login failed"),
    }
    println!();

    // 5. Time-based Attack Detection
    println!("⏰ Scenario 5: Timing Attack Detection");
    println!("Simulating rapid successive login attempts to detect timing patterns...\n");

    let mut successful_logins = 0;
    let mut timing_attack_detected = false;

    for i in 0..10 {
        let start_time = std::time::Instant::now();

        let auth_request = AuthRequest {
            email: "admin@startapp.id".to_string(),
            password: if i % 2 == 0 { "SecureAdminPass123" } else { "wrong_password" },
            ip_address: Some("192.168.1.200".to_string()),
            device_info: DeviceInfo {
                device_id: "timing_test_device".to_string(),
                user_agent: "TimingAttackBot/1.0".to_string(),
                ip_address: Some("192.168.1.200".to_string()),
                fingerprint: None,
            },
            remember_me: None,
        };

        match auth_service.authenticate_enhanced(
            auth_request,
            &user_database,
            &security_service
        ).await {
            Ok(_) => {
                successful_logins += 1;
                println!("   Attempt {}: SUCCESS", i + 1);
            }
            Err(_) => {
                println!("   Attempt {}: FAILED", i + 1);
            }
        }

        let elapsed = start_time.elapsed();

        // Check for suspiciously consistent timing (indicating timing attack)
        if elapsed.as_millis() > 5000 {
            timing_attack_detected = true;
            println!("   ⚠️ Slow response detected ({}ms) - possible timing attack", elapsed.as_millis());
        }

        sleep(Duration::from_millis(100)).await;
    }

    if timing_attack_detected {
        security_monitor.log_event(SecurityEvent::MultipleFailedLogins {
            email: "admin@startapp.id".to_string(),
            count: 10,
        });
        println!("🚨 Timing attack patterns detected and logged!");
    }
    println!();

    // 6. Generate Security Report
    println!("📊 Final Security Analysis Report");
    println!("=====================================\n");

    // Add some more events to the monitor
    security_monitor.log_event(SecurityEvent::BlockedIP {
        ip: "203.0.113.45".to_string(),
        reason: "Brute force attack".to_string(),
    });

    security_monitor.log_event(SecurityEvent::AccountLocked {
        user_id: "user_123".to_string(),
        reason: "Multiple failed login attempts".to_string(),
    });

    println!("{}", security_monitor.generate_report());
    println!();

    // 7. Security Recommendations
    println!("🛡️ Security Recommendations");
    println!("============================");
    println!("1. Implement IP-based rate limiting with Redis");
    println!("2. Add geolocation checking for unusual access patterns");
    println!("3. Implement device fingerprinting for session validation");
    println!("4. Set up real-time alerting for security events");
    println!("5. Use Web Application Firewall (WAF) for additional protection");
    println!("6. Implement account lockout policies with exponential backoff");
    println!("7. Add CAPTCHA after multiple failed attempts");
    println!("8. Monitor for timing attacks and side-channel vulnerabilities");
    println!("9. Implement progressive authentication for sensitive operations");
    println!("10. Regular security audits and penetration testing");
    println!();

    println!("=== Security Scenarios Complete ===");
    println!("🎉 All security monitoring features demonstrated successfully!");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_security_service_blocking() -> Result<()> {
        let mut security_service = AdvancedSecurityService::new();

        // Simulate multiple failed attempts from same IP
        for _ in 0..=5 {
            *security_service.ip_attempts.entry("192.168.1.100".to_string()).or_insert(0) += 1;
        }

        // Check if rate limiting would block
        let result = security_service.check_rate_limit("test@example.com", Some("192.168.1.100")).await;
        assert!(result.is_err()); // Should be blocked

        // Test IP blocking
        security_service.block_ip("192.168.1.100");
        let result = security_service.check_rate_limit("test@example.com", Some("192.168.1.100")).await;
        assert!(result.is_err()); // Should be blocked

        Ok(())
    }

    #[test]
    fn test_anomaly_detection() -> Result<()> {
        let security_service = AdvancedSecurityService::new();
        let user_id = Uuid::new_v4();

        // Test suspicious user agent detection
        let suspicious_device = DeviceInfo {
            device_id: "normal_device".to_string(),
            user_agent: "BadBot/1.0".to_string(),
            ip_address: Some("192.168.1.100".to_string()),
            fingerprint: None,
        };

        let flags = security_service.detect_anomalies(&user_id, &suspicious_device, Some("192.168.1.100"));
        assert!(flags.suspicious_location);

        // Test suspicious device ID detection
        let suspicious_device_id = DeviceInfo {
            device_id: "temp".to_string(),
            user_agent: "Mozilla/5.0".to_string(),
            ip_address: Some("192.168.1.100".to_string()),
            fingerprint: None,
        };

        let flags = security_service.detect_anomalies(&user_id, &suspicious_device_id, Some("192.168.1.100"));
        assert!(flags.anomaly_detected);

        Ok(())
    }

    #[tokio::test]
    async fn test_token_tampering_detection() -> Result<()> {
        let auth_service = AuthService::with_secret("test_secret");
        let user_id = Uuid::new_v4();
        let valid_token = auth_service.generate_token(&user_id).await?;

        // Test valid token
        let validation = auth_service.validate_token(&valid_token).await?;
        assert!(validation.valid);

        // Test tampered token
        let tampered_token = &valid_token[..valid_token.len()-1];
        let validation = auth_service.validate_token(tampered_token).await?;
        assert!(!validation.valid);

        Ok(())
    }

    #[test]
    fn test_security_monitor() -> Result<()> {
        let mut monitor = SecurityMonitor::new();

        monitor.log_event(SecurityEvent::BruteForceAttack {
            ip: "192.168.1.100".to_string(),
            attempts: 10,
        });

        monitor.log_event(SecurityEvent::BlockedIP {
            ip: "203.0.113.45".to_string(),
            reason: "Suspicious activity".to_string(),
        });

        let report = monitor.generate_report();
        assert!(report.contains("Total Events: 2"));
        assert!(report.contains("Brute Force Attacks: 1"));
        assert!(report.contains("Blocked IPs: 1"));

        Ok(())
    }
}