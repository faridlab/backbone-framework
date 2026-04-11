//! Authentication service implementation

use anyhow::Result;
use uuid::Uuid;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use regex::Regex;
use crate::jwt::{JwtService, Claims, KeyRotationConfig, JwtAlgorithm};
use crate::traits::{UserRepository, SecurityService, SecurityFlags, RefreshTokenClaims, User, AuthRequest, AuthResultEnhanced};

/// Lazily-compiled email validation regex (compiled once, reused forever)
fn email_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")
            .expect("email regex is a valid constant pattern")
    })
}

/// Authentication service configuration
#[derive(Debug, Clone)]
pub struct AuthServiceConfig {
    pub jwt_secret: String,
    pub token_expiry_hours: u64,
    /// Optional key rotation configuration. When set, enables JWT key rotation support.
    pub key_rotation: Option<KeyRotationConfig>,
    /// JWT algorithm to use. Defaults to HS256 if not specified.
    pub jwt_algorithm: Option<JwtAlgorithm>,
    /// PEM-encoded RSA private key (required for RS256)
    pub rsa_private_key_pem: Option<String>,
    /// PEM-encoded RSA public key (required for RS256)
    pub rsa_public_key_pem: Option<String>,
}

impl Default for AuthServiceConfig {
    fn default() -> Self {
        Self {
            jwt_secret: "default_secret_change_in_production".to_string(),
            token_expiry_hours: 24,
            key_rotation: None,
            jwt_algorithm: None,
            rsa_private_key_pem: None,
            rsa_public_key_pem: None,
        }
    }
}

/// Authentication service
pub struct AuthService {
    jwt_service: JwtService,
    config: AuthServiceConfig,
}

impl AuthService {
    pub fn new(config: AuthServiceConfig) -> anyhow::Result<Self> {
        let jwt_service = match config.jwt_algorithm.unwrap_or(JwtAlgorithm::HS256) {
            JwtAlgorithm::RS256 => {
                let private_pem = config.rsa_private_key_pem.as_deref()
                    .ok_or_else(|| anyhow::anyhow!("rsa_private_key_pem is required for RS256"))?;
                let public_pem = config.rsa_public_key_pem.as_deref()
                    .ok_or_else(|| anyhow::anyhow!("rsa_public_key_pem is required for RS256"))?;
                match &config.key_rotation {
                    Some(rotation_config) => {
                        JwtService::with_rs256_rotation(private_pem, public_pem, rotation_config.clone())?
                    }
                    None => JwtService::new_rs256(private_pem, public_pem)?,
                }
            }
            JwtAlgorithm::HS256 => {
                match &config.key_rotation {
                    Some(rotation_config) => {
                        JwtService::with_rotation(&config.jwt_secret, rotation_config.clone())
                    }
                    None => JwtService::new(&config.jwt_secret),
                }
            }
        };
        Ok(Self {
            jwt_service,
            config,
        })
    }

    /// Create an AuthService with HS256 and the given secret (infallible).
    pub fn with_secret(jwt_secret: &str) -> Self {
        Self {
            jwt_service: JwtService::new(jwt_secret),
            config: AuthServiceConfig {
                jwt_secret: jwt_secret.to_string(),
                token_expiry_hours: 24,
                key_rotation: None,
                jwt_algorithm: None,
                rsa_private_key_pem: None,
                rsa_public_key_pem: None,
            },
        }
    }

    /// Authenticate user with enhanced security.
    ///
    /// Orchestrates: validation, rate-limiting, credential verification,
    /// security analysis, token generation, and audit logging.
    pub async fn authenticate_enhanced(
        &self,
        request: AuthRequest,
        user_repository: &dyn UserRepository,
        security_service: &dyn SecurityService,
    ) -> Result<AuthResultEnhanced> {
        tracing::info!(
            event = "auth.attempt_started",
            email = %request.email,
            ip_address = ?request.ip_address,
            "Authentication attempt started"
        );

        if let Err(e) = self.validate_auth_request(&request) {
            tracing::warn!(event = "auth.validation_failed", email = %request.email, reason = %e, "Validation failed");
            return Err(e);
        }

        security_service.check_rate_limit(&request.email, request.ip_address.as_deref()).await?;

        let user = self.verify_user_credentials(&request, user_repository, security_service).await?;
        let (security_flags, requires_2fa) = self.evaluate_security_context(&user, &request, security_service).await?;
        self.finalize_auth(&user, &request, security_flags, requires_2fa, security_service).await
    }

    /// Steps 3-5: look up user, check account status, verify password.
    async fn verify_user_credentials(
        &self,
        request: &AuthRequest,
        user_repository: &dyn UserRepository,
        security_service: &dyn SecurityService,
    ) -> Result<User> {
        let user = user_repository.find_by_email(&request.email).await?.ok_or_else(|| {
            tracing::warn!(event = "auth.user_not_found", email = %request.email, "User not found");
            anyhow::anyhow!("Invalid credentials")
        })?;

        if let Err(e) = self.check_account_status(&user) {
            tracing::warn!(event = "auth.account_status_failed", user_id = %user.id, reason = %e, "Account status check failed");
            return Err(e);
        }

        if !self.verify_password(&request.password, &user.password_hash)? {
            tracing::warn!(event = "auth.password_mismatch", user_id = %user.id, "Invalid password");
            security_service.log_failed_auth_attempt(&user.id, request.ip_address.as_deref()).await?;
            return Err(anyhow::anyhow!("Invalid credentials"));
        }

        Ok(user)
    }

    /// Steps 6-7: run security analysis and determine 2FA requirement.
    async fn evaluate_security_context(
        &self,
        user: &User,
        request: &AuthRequest,
        security_service: &dyn SecurityService,
    ) -> Result<(SecurityFlags, bool)> {
        let security_flags = security_service
            .analyze_login_attempt(&user.id, &request.device_info, request.ip_address.as_deref())
            .await?;

        if security_flags.new_device {
            tracing::info!(event = "auth.new_device_detected", user_id = %user.id, "New device detected");
        }

        let requires_2fa = user.two_factor_enabled && !user.two_factor_methods.is_empty();
        if requires_2fa {
            tracing::info!(event = "auth.2fa_required", user_id = %user.id, "2FA required");
        }

        Ok((security_flags, requires_2fa))
    }

    /// Steps 8-9: generate tokens, log success, build result.
    async fn finalize_auth(
        &self,
        user: &User,
        request: &AuthRequest,
        security_flags: SecurityFlags,
        requires_2fa: bool,
        security_service: &dyn SecurityService,
    ) -> Result<AuthResultEnhanced> {
        let remember_me = request.remember_me.unwrap_or(false);
        let token = self.generate_access_token(&user.id)?;
        let refresh_token = self.generate_refresh_token(&user.id, remember_me)?;

        tracing::info!(event = "auth.tokens_generated", user_id = %user.id, has_refresh_token = refresh_token.is_some(), "Tokens generated");

        security_service.log_successful_auth(&user.id, request.ip_address.as_deref()).await?;

        tracing::info!(
            event = "auth.success", user_id = %user.id, ip_address = ?request.ip_address,
            requires_2fa = requires_2fa, risk_score = security_flags.risk_score, "Authentication successful"
        );

        Ok(AuthResultEnhanced {
            user_id: user.id,
            token,
            refresh_token,
            expires_at: chrono::Utc::now() + chrono::Duration::hours(self.config.token_expiry_hours as i64),
            requires_2fa,
            security_flags,
        })
    }

    /// Validate authentication request with comprehensive checks
    fn validate_auth_request(&self, request: &AuthRequest) -> Result<()> {
        // Enhanced email validation
        if !self.is_valid_email(&request.email) {
            return Err(anyhow::anyhow!("Invalid email format"));
        }

        // Enhanced password validation
        if !self.is_valid_password_format(&request.password) {
            return Err(anyhow::anyhow!(
                "Password must be at least 8 characters with uppercase, lowercase, and number"
            ));
        }

        // Check for common passwords
        if self.is_common_password(&request.password) {
            return Err(anyhow::anyhow!("Password is too common"));
        }

        Ok(())
    }

    /// Enhanced email validation with regex
    fn is_valid_email(&self, email: &str) -> bool {
        !email.is_empty() && email_regex().is_match(email) && email.len() <= 254
    }

    /// Enhanced password validation
    fn is_valid_password_format(&self, password: &str) -> bool {
        password.len() >= 8
            && password.len() <= 128
            && password.chars().any(|c| c.is_uppercase())
            && password.chars().any(|c| c.is_lowercase())
            && password.chars().any(|c| c.is_numeric())
    }

    /// Check against common passwords list
    fn is_common_password(&self, password: &str) -> bool {
        let common_passwords = vec![
            "password", "123456", "123456789", "qwerty", "abc123",
            "password123", "admin", "letmein", "welcome", "monkey"
        ];
        common_passwords.contains(&password.to_lowercase().as_str())
    }

    /// Verify password against stored hash
    fn verify_password(&self, password: &str, hash: &str) -> Result<bool> {
        use argon2::{Argon2, PasswordHash, PasswordVerifier};

        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| anyhow::anyhow!("Failed to parse password hash: {}", e))?;

        let argon2 = Argon2::default();

        Ok(argon2.verify_password(password.as_bytes(), &parsed_hash).is_ok())
    }

    /// Check account status (active, suspended, locked, etc.)
    fn check_account_status(&self, user: &User) -> Result<()> {
        if !user.is_active {
            return Err(anyhow::anyhow!("Account is disabled"));
        }

        if user.is_locked {
            return Err(anyhow::anyhow!("Account is locked. Please contact support."));
        }

        if let Some(expires_at) = user.account_expires_at {
            if chrono::Utc::now() > expires_at {
                return Err(anyhow::anyhow!("Account has expired"));
            }
        }

        if user.requires_password_change {
            return Err(anyhow::anyhow!("Password change required"));
        }

        Ok(())
    }

    /// Generate an access token for the given user.
    fn generate_access_token(&self, user_id: &Uuid) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow::anyhow!("System time error: {}", e))?;

        let exp = now + Duration::from_secs(self.config.token_expiry_hours * 3600);

        let claims = Claims {
            sub: user_id.to_string(),
            exp: exp.as_secs() as usize,
            iat: now.as_secs() as usize,
            iss: "backbone".to_string(),
        };

        self.jwt_service.create_token(&claims)
    }

    /// Generate a refresh token if `remember_me` is true; returns `None` otherwise.
    fn generate_refresh_token(&self, user_id: &Uuid, remember_me: bool) -> Result<Option<String>> {
        if !remember_me {
            return Ok(None);
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow::anyhow!("System time error: {}", e))?;

        let refresh_exp = now + Duration::from_secs(30 * 24 * 3600); // 30 days

        let claims = RefreshTokenClaims {
            sub: user_id.to_string(),
            exp: refresh_exp.as_secs() as usize,
            iat: now.as_secs() as usize,
            iss: "backbone".to_string(),
            token_type: "refresh".to_string(),
        };

        Ok(Some(self.jwt_service.create_refresh_token(&claims)?))
    }

    /// Generate JWT token
    pub async fn generate_token(&self, user_id: &Uuid) -> Result<String> {
        let token = self.generate_token_internal(user_id)?;
        tracing::info!(
            event = "auth.token_generated",
            user_id = %user_id,
            token_type = "access",
            "Access token generated"
        );
        Ok(token)
    }

    /// Internal method to generate JWT token
    fn generate_token_internal(&self, user_id: &Uuid) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow::anyhow!("System time error: {}", e))?;

        let exp = now + Duration::from_secs(self.config.token_expiry_hours * 3600);

        let claims = Claims {
            sub: user_id.to_string(),
            exp: exp.as_secs() as usize,
            iat: now.as_secs() as usize,
            iss: "backbone".to_string(),
        };

        self.jwt_service.create_token(&claims)
    }

    /// Validate JWT token
    pub async fn validate_token(&self, token: &str) -> Result<TokenValidation> {
        match self.jwt_service.validate_token(token) {
            Ok(claims) => {
                let user_id = Uuid::parse_str(&claims.sub)
                    .map_err(|e| anyhow::anyhow!("Invalid user ID in token: {}", e))?;

                tracing::debug!(
                    event = "auth.token_validated",
                    user_id = %user_id,
                    "Token validated successfully"
                );

                Ok(TokenValidation {
                    valid: true,
                    user_id: Some(user_id),
                })
            }
            Err(e) => {
                tracing::warn!(
                    event = "auth.token_validation_failed",
                    reason = %e,
                    "Token validation failed"
                );
                Ok(TokenValidation {
                    valid: false,
                    user_id: None,
                })
            }
        }
    }
}

/// Authentication result
#[derive(Debug, Clone)]
pub struct AuthResult {
    pub user_id: Uuid,
    pub token: Option<String>,
}

impl AuthResult {
    pub fn new(user_id: Uuid) -> Self {
        Self {
            user_id,
            token: None,
        }
    }
}

/// Token validation result
#[derive(Debug, Clone)]
pub struct TokenValidation {
    pub valid: bool,
    pub user_id: Option<Uuid>,
}

impl TokenValidation {
    pub fn new(valid: bool) -> Self {
        Self {
            valid,
            user_id: None,
        }
    }
}