//! Password hashing and verification

use anyhow::Result;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, SaltString},
    Argon2,
};

/// Password service
pub struct PasswordService {
    argon2: Argon2<'static>,
}

impl PasswordService {
    pub fn new() -> Self {
        // Use secure Argon2 parameters
        let argon2 = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(19456, 2, 1, Some(32)).unwrap(),
        );

        Self { argon2 }
    }

    /// Hash password
    #[tracing::instrument(skip_all)]
    pub fn hash_password(&self, password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = self.argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| {
                tracing::error!(event = "auth.password_hash_failed", "Password hashing failed");
                anyhow::anyhow!("Failed to hash password: {}", e)
            })?;

        tracing::debug!(event = "auth.password_hashed", "Password hashed successfully");
        Ok(password_hash.to_string())
    }

    /// Verify password
    #[tracing::instrument(skip_all)]
    pub fn verify_password(&self, password: &str, hash: &str) -> Result<bool> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| anyhow::anyhow!("Failed to parse password hash: {}", e))?;

        let result = argon2::PasswordVerifier::verify_password(&self.argon2, password.as_bytes(), &parsed_hash).is_ok();

        tracing::debug!(
            event = "auth.password_verified",
            success = result,
            "Password verification completed"
        );

        Ok(result)
    }

    /// Generate random password
    pub fn generate_password(&self, length: usize) -> String {
        use rand::Rng;
        use rand::distributions::Alphanumeric;

        let mut rng = rand::thread_rng();
        (0..length)
            .map(|_| char::from(rng.sample(Alphanumeric)))
            .collect()
    }
}

impl Default for PasswordService {
    fn default() -> Self {
        Self::new()
    }
}

/// Password validation rules
pub struct PasswordValidator {
    min_length: usize,
    require_uppercase: bool,
    require_lowercase: bool,
    require_numbers: bool,
    require_symbols: bool,
}

impl PasswordValidator {
    pub fn new() -> Self {
        Self {
            min_length: 8,
            require_uppercase: true,
            require_lowercase: true,
            require_numbers: true,
            require_symbols: true,
        }
    }

    #[tracing::instrument(skip_all)]
    pub fn validate(&self, password: &str) -> Result<()> {
        if password.len() < self.min_length {
            tracing::warn!(event = "auth.password_validation_failed", reason = "too_short", "Password validation failed: too short");
            return Err(anyhow::anyhow!(
                "Password must be at least {} characters long",
                self.min_length
            ));
        }

        if self.require_uppercase && !password.chars().any(|c| c.is_uppercase()) {
            tracing::warn!(event = "auth.password_validation_failed", reason = "no_uppercase", "Password validation failed: no uppercase");
            return Err(anyhow::anyhow!("Password must contain at least one uppercase letter"));
        }

        if self.require_lowercase && !password.chars().any(|c| c.is_lowercase()) {
            tracing::warn!(event = "auth.password_validation_failed", reason = "no_lowercase", "Password validation failed: no lowercase");
            return Err(anyhow::anyhow!("Password must contain at least one lowercase letter"));
        }

        if self.require_numbers && !password.chars().any(|c| c.is_numeric()) {
            tracing::warn!(event = "auth.password_validation_failed", reason = "no_number", "Password validation failed: no number");
            return Err(anyhow::anyhow!("Password must contain at least one number"));
        }

        if self.require_symbols
            && !password.chars().any(|c| !c.is_alphanumeric())
        {
            tracing::warn!(event = "auth.password_validation_failed", reason = "no_symbol", "Password validation failed: no special character");
            return Err(anyhow::anyhow!("Password must contain at least one special character"));
        }

        Ok(())
    }
}

impl Default for PasswordValidator {
    fn default() -> Self {
        Self::new()
    }
}