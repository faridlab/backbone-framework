//! Consolidated token generation service
//!
//! Provides OTP generation, token hashing (SHA-256), refresh token generation,
//! constant-time comparison, and JWT operations through a single service.

use anyhow::Result;
use rand::Rng;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use std::sync::Arc;

use crate::jwt::{JwtService, Claims};
use crate::traits::RefreshTokenClaims;

/// Consolidated token generation and validation service.
///
/// Wraps `JwtService` for JWT operations and provides static methods
/// for crypto operations that don't require JWT context.
pub struct TokenGenerator {
    jwt_service: Arc<JwtService>,
}

impl TokenGenerator {
    /// Create a new TokenGenerator wrapping a shared JwtService.
    pub fn new(jwt_service: Arc<JwtService>) -> Self {
        Self { jwt_service }
    }

    // ── Static crypto methods (no JwtService needed) ──

    /// Generate a 6-digit OTP code.
    pub fn generate_otp() -> String {
        let code: u32 = rand::thread_rng().gen_range(100_000..1_000_000);
        format!("{:06}", code)
    }

    /// SHA-256 hash a token/OTP before storing in the database.
    pub fn hash_token(token: &str) -> String {
        let result = Sha256::digest(token.as_bytes());
        result.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Generate a new refresh token string (UUID v4).
    pub fn generate_refresh_token() -> String {
        Uuid::new_v4().to_string()
    }

    /// Constant-time string comparison to prevent timing side-channel attacks.
    ///
    /// The XOR-fold comparison runs in constant time for inputs of equal length.
    /// The length check on line 1 does leak whether lengths differ via timing,
    /// but this is acceptable because all call sites compare fixed-length values
    /// (6-digit OTP codes or 64-char SHA-256 hex hashes).
    ///
    /// Do NOT use this for variable-length secret comparison.
    pub fn constant_time_eq(a: &str, b: &str) -> bool {
        if a.len() != b.len() {
            return false;
        }
        a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
    }

    // ── JWT methods (delegate to internal JwtService) ──

    /// Create an access token from claims.
    pub fn create_access_token(&self, claims: &Claims) -> Result<String> {
        self.jwt_service.create_token(claims)
    }

    /// Create a refresh token JWT from refresh claims.
    pub fn create_refresh_token_jwt(&self, claims: &RefreshTokenClaims) -> Result<String> {
        self.jwt_service.create_refresh_token(claims)
    }

    /// Validate an access token and return its claims.
    pub fn validate_access_token(&self, token: &str) -> Result<Claims> {
        self.jwt_service.validate_token(token)
    }

    /// Validate a refresh token JWT and return its claims.
    pub fn validate_refresh_token(&self, token: &str) -> Result<RefreshTokenClaims> {
        self.jwt_service.validate_refresh_token(token)
    }

    /// Decode an access token without expiry validation.
    pub fn decode_access_token(&self, token: &str) -> Result<Claims> {
        self.jwt_service.decode_token(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_otp_length() {
        let otp = TokenGenerator::generate_otp();
        assert_eq!(otp.len(), 6);
        assert!(otp.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_hash_token_deterministic() {
        let hash1 = TokenGenerator::hash_token("test-token");
        let hash2 = TokenGenerator::hash_token("test-token");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn test_hash_token_different_inputs() {
        let hash1 = TokenGenerator::hash_token("token-a");
        let hash2 = TokenGenerator::hash_token("token-b");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_generate_refresh_token_is_uuid() {
        let token = TokenGenerator::generate_refresh_token();
        assert!(Uuid::parse_str(&token).is_ok());
    }

    #[test]
    fn test_constant_time_eq_same() {
        assert!(TokenGenerator::constant_time_eq("abc", "abc"));
    }

    #[test]
    fn test_constant_time_eq_different() {
        assert!(!TokenGenerator::constant_time_eq("abc", "abd"));
    }

    #[test]
    fn test_constant_time_eq_different_length() {
        assert!(!TokenGenerator::constant_time_eq("abc", "abcd"));
    }

    #[test]
    fn test_jwt_roundtrip() {
        let jwt = Arc::new(JwtService::new("test-secret-key"));
        let gen = TokenGenerator::new(jwt);

        let claims = Claims {
            sub: "user-123".to_string(),
            exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
            iat: chrono::Utc::now().timestamp() as usize,
            iss: "backbone".to_string(),
        };

        let token = gen.create_access_token(&claims).unwrap();
        let decoded = gen.validate_access_token(&token).unwrap();
        assert_eq!(decoded.sub, "user-123");
    }
}
