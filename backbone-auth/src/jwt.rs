//! JWT (JSON Web Token) handling with key rotation and multi-algorithm support
//!
//! Supports HS256 (symmetric HMAC) and RS256 (asymmetric RSA) algorithms.
//! Supports multiple signing keys for zero-downtime key rotation.
//! Old keys remain valid during a configurable grace period after rotation.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use std::sync::RwLock;
use zeroize::Zeroize;
use crate::traits::RefreshTokenClaims;

/// JWT token claims
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
    pub iss: String,
}

/// JWT signing algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JwtAlgorithm {
    /// HMAC-SHA256 (symmetric) — requires shared secret
    HS256,
    /// RSA-SHA256 (asymmetric) — sign with private key, verify with public key
    RS256,
}

/// A single HMAC signing key with lifecycle metadata
#[derive(Clone)]
pub struct JwtKey {
    /// Unique key identifier (included in JWT header as `kid`)
    pub kid: String,
    /// The HMAC secret
    pub secret: String,
    /// When this key was created
    pub created_at: DateTime<Utc>,
    /// When this key was retired (rotated out)
    pub retired_at: Option<DateTime<Utc>>,
}

impl Drop for JwtKey {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

/// An RSA key pair for asymmetric JWT signing
#[derive(Clone)]
pub struct RsaKeyPair {
    /// Unique key identifier (included in JWT header as `kid`)
    pub kid: String,
    /// PEM-encoded RSA private key (for signing)
    pub private_key_pem: String,
    /// PEM-encoded RSA public key (for verification)
    pub public_key_pem: String,
    /// When this key was created
    pub created_at: DateTime<Utc>,
    /// When this key was retired (rotated out)
    pub retired_at: Option<DateTime<Utc>>,
}

impl Drop for RsaKeyPair {
    fn drop(&mut self) {
        self.private_key_pem.zeroize();
    }
}

/// Internal key material abstraction for algorithm-agnostic operation
#[derive(Clone)]
enum KeyMaterial {
    Hmac(JwtKey),
    Rsa(RsaKeyPair),
}

impl KeyMaterial {
    fn kid(&self) -> &str {
        match self {
            KeyMaterial::Hmac(k) => &k.kid,
            KeyMaterial::Rsa(k) => &k.kid,
        }
    }

    fn retired_at(&self) -> Option<DateTime<Utc>> {
        match self {
            KeyMaterial::Hmac(k) => k.retired_at,
            KeyMaterial::Rsa(k) => k.retired_at,
        }
    }

    fn set_retired(&mut self) {
        let now = Utc::now();
        match self {
            KeyMaterial::Hmac(k) => k.retired_at = Some(now),
            KeyMaterial::Rsa(k) => k.retired_at = Some(now),
        }
    }

    fn algorithm(&self) -> jsonwebtoken::Algorithm {
        match self {
            KeyMaterial::Hmac(_) => jsonwebtoken::Algorithm::HS256,
            KeyMaterial::Rsa(_) => jsonwebtoken::Algorithm::RS256,
        }
    }
}

/// Configuration for key rotation behavior
#[derive(Debug, Clone)]
pub struct KeyRotationConfig {
    /// How long retired keys remain valid for token validation
    pub grace_period: chrono::Duration,
}

impl Default for KeyRotationConfig {
    fn default() -> Self {
        Self {
            grace_period: chrono::Duration::hours(24),
        }
    }
}

/// JWT service with key rotation and multi-algorithm support
///
/// Maintains an active signing key and a list of retired keys.
/// Tokens are always signed with the active key.
/// Validation tries the active key first, then retired keys within the grace period.
///
/// Supports both HS256 (shared secret) and RS256 (RSA key pair).
pub struct JwtService {
    active_key: RwLock<KeyMaterial>,
    retired_keys: RwLock<Vec<KeyMaterial>>,
    rotation_config: KeyRotationConfig,
}

impl JwtService {
    // =========================================================================
    // Constructors — HS256
    // =========================================================================

    /// Create a new JWT service with HS256 (backward compatible)
    pub fn new(secret: &str) -> Self {
        Self::with_rotation(secret, KeyRotationConfig::default())
    }

    /// Create a new JWT service with HS256 and explicit rotation configuration
    pub fn with_rotation(secret: &str, config: KeyRotationConfig) -> Self {
        let key = JwtKey {
            kid: uuid::Uuid::new_v4().to_string(),
            secret: secret.to_string(),
            created_at: Utc::now(),
            retired_at: None,
        };

        Self {
            active_key: RwLock::new(KeyMaterial::Hmac(key)),
            retired_keys: RwLock::new(Vec::new()),
            rotation_config: config,
        }
    }

    // =========================================================================
    // Constructors — RS256
    // =========================================================================

    /// Create a new JWT service with RS256 (asymmetric)
    ///
    /// Validates the RSA key pair on construction — returns an error if the keys
    /// are malformed or cannot be used for signing/verification.
    pub fn new_rs256(private_key_pem: &str, public_key_pem: &str) -> Result<Self> {
        Self::with_rs256_rotation(private_key_pem, public_key_pem, KeyRotationConfig::default())
    }

    /// Create a new JWT service with RS256 and explicit rotation configuration
    ///
    /// Validates the RSA key pair on construction — returns an error if the keys
    /// are malformed or cannot be used for signing/verification.
    pub fn with_rs256_rotation(
        private_key_pem: &str,
        public_key_pem: &str,
        config: KeyRotationConfig,
    ) -> Result<Self> {
        // Validate keys upfront — fail fast if they are invalid
        jsonwebtoken::EncodingKey::from_rsa_pem(private_key_pem.as_bytes())
            .map_err(|e| anyhow::anyhow!("Invalid RSA private key PEM: {}", e))?;
        jsonwebtoken::DecodingKey::from_rsa_pem(public_key_pem.as_bytes())
            .map_err(|e| anyhow::anyhow!("Invalid RSA public key PEM: {}", e))?;

        let key = RsaKeyPair {
            kid: uuid::Uuid::new_v4().to_string(),
            private_key_pem: private_key_pem.to_string(),
            public_key_pem: public_key_pem.to_string(),
            created_at: Utc::now(),
            retired_at: None,
        };

        Ok(Self {
            active_key: RwLock::new(KeyMaterial::Rsa(key)),
            retired_keys: RwLock::new(Vec::new()),
            rotation_config: config,
        })
    }

    // =========================================================================
    // Key info & export
    // =========================================================================

    /// Get the current active key ID
    pub fn active_kid(&self) -> String {
        self.active_key.read()
            .unwrap_or_else(|e| e.into_inner())
            .kid()
            .to_string()
    }

    /// Get the algorithm used by this service
    pub fn algorithm(&self) -> JwtAlgorithm {
        match &*self.active_key.read().unwrap_or_else(|e| e.into_inner()) {
            KeyMaterial::Hmac(_) => JwtAlgorithm::HS256,
            KeyMaterial::Rsa(_) => JwtAlgorithm::RS256,
        }
    }

    /// Export the public key PEM (RS256 only, returns None for HS256)
    pub fn public_key_pem(&self) -> Option<String> {
        match &*self.active_key.read().unwrap_or_else(|e| e.into_inner()) {
            KeyMaterial::Rsa(k) => Some(k.public_key_pem.clone()),
            KeyMaterial::Hmac(_) => None,
        }
    }

    // =========================================================================
    // Key rotation
    // =========================================================================

    /// Rotate the HS256 signing key.
    /// Returns the `kid` of the new active key.
    pub fn rotate_key(&self, new_secret: &str) -> Result<String> {
        let new_key = KeyMaterial::Hmac(JwtKey {
            kid: uuid::Uuid::new_v4().to_string(),
            secret: new_secret.to_string(),
            created_at: Utc::now(),
            retired_at: None,
        });
        self.rotate_key_material(new_key)
    }

    /// Rotate the RS256 key pair.
    /// Validates the new key pair before rotating. Returns the `kid` of the new active key.
    pub fn rotate_rsa_key(&self, private_key_pem: &str, public_key_pem: &str) -> Result<String> {
        // Validate keys before rotating
        jsonwebtoken::EncodingKey::from_rsa_pem(private_key_pem.as_bytes())
            .map_err(|e| anyhow::anyhow!("Invalid RSA private key PEM for rotation: {}", e))?;
        jsonwebtoken::DecodingKey::from_rsa_pem(public_key_pem.as_bytes())
            .map_err(|e| anyhow::anyhow!("Invalid RSA public key PEM for rotation: {}", e))?;

        let new_key = KeyMaterial::Rsa(RsaKeyPair {
            kid: uuid::Uuid::new_v4().to_string(),
            private_key_pem: private_key_pem.to_string(),
            public_key_pem: public_key_pem.to_string(),
            created_at: Utc::now(),
            retired_at: None,
        });
        self.rotate_key_material(new_key)
    }

    /// Internal: swap active key, retire old one, prune expired
    fn rotate_key_material(&self, new_key: KeyMaterial) -> Result<String> {
        let new_kid = new_key.kid().to_string();

        let mut active = self.active_key.write()
            .map_err(|_| anyhow::anyhow!("JWT active key lock poisoned"))?;
        let mut old_key = new_key;
        std::mem::swap(&mut *active, &mut old_key);

        old_key.set_retired();

        let mut retired = self.retired_keys.write()
            .map_err(|_| anyhow::anyhow!("JWT retired keys lock poisoned"))?;
        retired.push(old_key);
        Self::prune_expired_keys(&mut retired, &self.rotation_config.grace_period);

        tracing::info!(
            event = "auth.key_rotated",
            new_kid = %new_kid,
            retired_keys_count = retired.len(),
            "JWT signing key rotated"
        );

        Ok(new_kid)
    }

    // =========================================================================
    // Token creation
    // =========================================================================

    /// Create JWT token (signs with the active key, includes `kid` in header)
    #[tracing::instrument(skip_all, fields(sub = %claims.sub))]
    pub fn create_token(&self, claims: &Claims) -> Result<String> {
        let active = self.active_key.read()
            .map_err(|_| anyhow::anyhow!("JWT key lock poisoned"))?;
        let (header, encoding_key) = Self::make_encoding_parts(&active)?;

        let token = jsonwebtoken::encode(&header, claims, &encoding_key)
            .map_err(|e| {
                tracing::error!(event = "auth.jwt_create_failed", "Failed to create JWT token");
                anyhow::anyhow!("Failed to create JWT token: {}", e)
            })?;

        tracing::debug!(event = "auth.jwt_created", kid = %active.kid(), "JWT access token created");
        Ok(token)
    }

    /// Create refresh token (signs with the active key)
    #[tracing::instrument(skip_all, fields(sub = %claims.sub))]
    pub fn create_refresh_token(&self, claims: &RefreshTokenClaims) -> Result<String> {
        let active = self.active_key.read()
            .map_err(|_| anyhow::anyhow!("JWT key lock poisoned"))?;
        let (header, encoding_key) = Self::make_encoding_parts(&active)?;

        let token = jsonwebtoken::encode(&header, claims, &encoding_key)
            .map_err(|e| {
                tracing::error!(event = "auth.refresh_token_create_failed", "Failed to create refresh token");
                anyhow::anyhow!("Failed to create refresh token: {}", e)
            })?;

        tracing::debug!(event = "auth.refresh_token_created", "Refresh token created");
        Ok(token)
    }

    // =========================================================================
    // Token validation
    // =========================================================================

    /// Validate JWT token (tries active key first, then retired keys)
    #[tracing::instrument(skip_all)]
    pub fn validate_token(&self, token: &str) -> Result<Claims> {
        self.validate_token_generic::<Claims>(token, None)
    }

    /// Decode JWT token without expiration validation
    #[tracing::instrument(skip_all)]
    pub fn decode_token(&self, token: &str) -> Result<Claims> {
        let algorithm = self.active_key.read()
            .map_err(|_| anyhow::anyhow!("JWT key lock poisoned"))?
            .algorithm();
        let mut validation = jsonwebtoken::Validation::new(algorithm);
        validation.validate_exp = false;
        validation.validate_nbf = false;
        self.validate_token_generic::<Claims>(token, Some(validation))
    }

    /// Validate refresh token (tries all valid keys)
    #[tracing::instrument(skip_all)]
    pub fn validate_refresh_token(&self, token: &str) -> Result<RefreshTokenClaims> {
        let claims = self.validate_token_generic::<RefreshTokenClaims>(token, None)?;

        if claims.token_type != "refresh" {
            tracing::warn!(event = "auth.invalid_token_type", "Expected refresh token, got different type");
            return Err(anyhow::anyhow!("Invalid token type: expected refresh token"));
        }

        tracing::debug!(
            event = "auth.refresh_token_validated",
            sub = %claims.sub,
            "Refresh token validated"
        );
        Ok(claims)
    }

    // =========================================================================
    // Private helpers
    // =========================================================================

    /// Build encoding header + key from KeyMaterial
    fn make_encoding_parts(key: &KeyMaterial) -> Result<(jsonwebtoken::Header, jsonwebtoken::EncodingKey)> {
        match key {
            KeyMaterial::Hmac(k) => {
                let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
                header.kid = Some(k.kid.clone());
                let encoding_key = jsonwebtoken::EncodingKey::from_secret(k.secret.as_ref());
                Ok((header, encoding_key))
            }
            KeyMaterial::Rsa(k) => {
                let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
                header.kid = Some(k.kid.clone());
                let encoding_key = jsonwebtoken::EncodingKey::from_rsa_pem(k.private_key_pem.as_bytes())
                    .map_err(|e| anyhow::anyhow!("Invalid RSA private key: {}", e))?;
                Ok((header, encoding_key))
            }
        }
    }

    /// Generic token validation that tries active key, then retired keys
    fn validate_token_generic<T: serde::de::DeserializeOwned>(
        &self,
        token: &str,
        custom_validation: Option<jsonwebtoken::Validation>,
    ) -> Result<T> {
        let algorithm = self.active_key.read()
            .map_err(|_| anyhow::anyhow!("JWT key lock poisoned"))?
            .algorithm();
        let validation = custom_validation.unwrap_or_else(|| jsonwebtoken::Validation::new(algorithm));

        // Extract kid from token header (if present)
        let token_kid = Self::extract_kid(token);

        // Try active key first
        let active = self.active_key.read()
            .map_err(|_| anyhow::anyhow!("JWT key lock poisoned"))?;
        if token_kid.as_ref().map_or(true, |kid| kid == active.kid()) {
            if let Some(claims) = Self::try_validate_with_key::<T>(token, &active, &validation) {
                return Ok(claims);
            }
        }
        drop(active);

        // Try retired keys (within grace period)
        let mut retired = self.retired_keys.write()
            .map_err(|_| anyhow::anyhow!("JWT retired keys lock poisoned"))?;
        Self::prune_expired_keys(&mut retired, &self.rotation_config.grace_period);

        for key in retired.iter() {
            if token_kid.as_ref().map_or(true, |kid| kid == key.kid()) {
                if let Some(claims) = Self::try_validate_with_key::<T>(token, key, &validation) {
                    tracing::debug!(
                        event = "auth.validated_with_retired_key",
                        kid = %key.kid(),
                        "Token validated with retired key"
                    );
                    return Ok(claims);
                }
            }
        }

        tracing::warn!(event = "auth.jwt_validation_failed", "JWT token validation failed");
        Err(anyhow::anyhow!("Failed to validate JWT token"))
    }

    /// Try to validate a token with a specific key
    fn try_validate_with_key<T: serde::de::DeserializeOwned>(
        token: &str,
        key: &KeyMaterial,
        validation: &jsonwebtoken::Validation,
    ) -> Option<T> {
        let decoding_key = match key {
            KeyMaterial::Hmac(k) => jsonwebtoken::DecodingKey::from_secret(k.secret.as_ref()),
            KeyMaterial::Rsa(k) => {
                match jsonwebtoken::DecodingKey::from_rsa_pem(k.public_key_pem.as_bytes()) {
                    Ok(dk) => dk,
                    Err(e) => {
                        tracing::debug!(
                            event = "auth.rsa_key_parse_failed",
                            kid = %k.kid,
                            error = %e,
                            "Failed to parse RSA public key PEM during token validation"
                        );
                        return None;
                    }
                }
            }
        };

        match jsonwebtoken::decode::<T>(token, &decoding_key, validation) {
            Ok(data) => Some(data.claims),
            Err(e) => {
                tracing::debug!(
                    event = "auth.token_decode_failed",
                    kid = %key.kid(),
                    error = %e,
                    "Token validation failed for key"
                );
                None
            }
        }
    }

    /// Extract the `kid` from a token's header without full validation
    fn extract_kid(token: &str) -> Option<String> {
        jsonwebtoken::decode_header(token)
            .ok()
            .and_then(|header| header.kid)
    }

    /// Remove retired keys that have exceeded the grace period
    fn prune_expired_keys(retired: &mut Vec<KeyMaterial>, grace_period: &chrono::Duration) {
        let now = Utc::now();
        retired.retain(|key| {
            if let Some(retired_at) = key.retired_at() {
                now - retired_at < *grace_period
            } else {
                true
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test RSA key pair (2048-bit, generated for testing only — PKCS#8 format)
    const TEST_RSA_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDlctXcg7jOYn8J
nklV8rCyEV2AMHqtFKJJz5CvJ5oL3KRegKxJygyBPfVhBdoG9kmNKdlSXEeWDwan
BzQ/Vv4i3569uIY1Z35S+6nrlviTlm1rzQq7n8G0zMW7iYBj8QZp0p6XnBUh8UJI
rzyo/m4NsQ3BPcu19ijdl5emH1MTGqUHrIdZJgU52SMp6XI6aqsUA92PUjBE6PtM
P0JQXDOxEwX0I9gpwopwLOg+TyIWItZsos4HB7x1GEo7bytOCik7O7CqRKh+n705
WkEZPll6apA7A3MXpAWq4TUsrrk+4lQzNaiCsUvFfcCmukm/J9BfoO2rKj67fJT8
DBq3oroNAgMBAAECggEAG+XtE+1eLJX0TVaWIyGpk1UiMcJzQBU4sFHRDUL664NN
5wGtMSGkiJhgfAYKnvsWMVhLyMRYnenAzNFG7IamytW2xumnQ9oMFYns/Ky0F7nc
HxXkvrBrjJCzYByVZFF7jqVhzBxZw3FCtnS8Iu4gsoB7JCpf2QWPrXXPpg67+p/x
Od7o2ylCQrS+Sbki7swSBkf3ID2CAwfOhiAl3KZz4Jy960Br/BBlp5xO67QAJ3D+
LY0eAYNmeHi18m8KjWCBN8lDmk2qx470xc1tng5Wk12dkLNEkdwy7ePBy9SadENF
sWRM9HEGddf8qHbkZtWUXI81pl1xSZKFxvjTKwumQQKBgQD/Nvg8NMGpS9WKBKR2
1ko42HR8kn64zlIJqq5LOhkhISyDjXbVKQaX8Kb7G+5zPYxKDC3siGOcCBWKaVQF
ArN/RcOIJhZD0u55FGlLC/j+OgCr+5tC9SEQIDw2FqAzAHUIrw9VeKsWPgzHolik
x4qdIYgCxeMSPfMCYx1L4CGYLQKBgQDmJ5HpL6osKKUQ8kVlBnIeFQIzst3V3aAI
rx496pNvoWfRSbPtahCPuZfNvw+mA1wbPe1adnvnoVrkFj8p8XqYJsSfdmgGiGTX
uaHeR1toNUaSGyx0yPdkcHzQ9zwN92+oX2GSEEYD9kukRNuRmo9te8MJZbd/yz3O
oD0D83n1YQKBgQCzsMFgqoh8KX+lGJWvcjt6ALUrjH2aovHSCpGDN9m/oLrVuQGl
Haidy/vVq5ndG8Wt0Rt4gaMYlfyMopJcoMU+5CDCuIZOpLHxIDTuePSMEsysSo/L
ugnYb8nVD2Ml5bmBLriuJjLXi0K2QJEHG9N1xMkdorS5AFMcKCrVZG5i1QKBgQDI
JkdEs7fYmSwbVaU1mupo8LoufXFfiFGg27GABNxcqs/e+KppX+CxLKQwP+R66dcS
tcMQ9ZMBN5gUXKhncGG9qZE6X71NWRXhaMS0yfda42HQs6LwmMhT52MUUr0+JB1N
Hk16uX45+dmELIGJ2RC8FHHjXTq7/uJsK3uEURuRYQKBgHKvrd5M8pOoTnBx7nE5
YXaBBAmeOYRKkczMF7ppj34u2UDcVEscEd0QNOPlPvoKmmEKi5sHeUdcj3sqMvj/
xFr+oxoyZ0KF6+G2rOT7hjLpTuYSwqstJgApx52iqaNjkN7bPtQSk+QyXJJNEK0+
bsxa1iX8xcbwZU2JO+Z2ZoT2
-----END PRIVATE KEY-----";

    const TEST_RSA_PUBLIC_KEY: &str = "-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA5XLV3IO4zmJ/CZ5JVfKw
shFdgDB6rRSiSc+QryeaC9ykXoCsScoMgT31YQXaBvZJjSnZUlxHlg8Gpwc0P1b+
It+evbiGNWd+Uvup65b4k5Zta80Ku5/BtMzFu4mAY/EGadKel5wVIfFCSK88qP5u
DbENwT3LtfYo3ZeXph9TExqlB6yHWSYFOdkjKelyOmqrFAPdj1IwROj7TD9CUFwz
sRMF9CPYKcKKcCzoPk8iFiLWbKLOBwe8dRhKO28rTgopOzuwqkSofp+9OVpBGT5Z
emqQOwNzF6QFquE1LK65PuJUMzWogrFLxX3AprpJvyfQX6Dtqyo+u3yU/Awat6K6
DQIDAQAB
-----END PUBLIC KEY-----";

    // Second RSA key pair for rotation tests (PKCS#8 format)
    const TEST_RSA_PRIVATE_KEY_2: &str = "-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDZ4IoevJ7RmSUo
QmvDOsu/btA217fvnRWYzHQdhxhAf9B3wa8kgIEIRF40uZrsL/udxk8grbv/hhEb
upWb1rouz9htyqLhMq+ti1PmFgeg4BvHsPlQhAggHEvUgmRWRdmwM8qiUbNOkVKT
L/khqricMRyxra1+Tp43CXOsJtehIfvbOURa5HC0qWGAvHdIe820s+Z++AfD943C
9IiZ5vRjtmaZ1JUMEh3Y2IspY5kWdlbgH5Ea8nkiUajwVyFgfUx/WyHHB65yUz6n
5PxxR12Z3ucmtMRS2SxxSM229dxDbwDLKCsVlYNENSIfU/ZBFWThikvd3Hbz8uLl
byarm2UrAgMBAAECggEAMlnfYZoSk/qx6Rtsfwoz8vIfgUUaF3B0gMLjJL3HP4Sq
PzrOCIAAEdKG+OVZ5bJzEjO1rqYn17X6dy+ICqM1lMLoz/qv6J5HljIoOfimW6nf
EaeW/mH85LrVVW+q03tCAyP89MUvzHzuGeDQ0NR85G+/I1qxSQrPKoXvKv4w/+YG
/Iq170RHOBu3uo4YTrqQHi7k7k28NyGE0dOkD3nqATUDzhMk3VTGA7l9Mrvm0Pth
coAsDmbrulODZG2qOmt0tK9wVBQTPy1IxJ+QgBlKMixGZFi/4BKX0XRQmn3sfdhk
1qO1nn2UAPm1QNTA2wn+I5OtU5HZHZ+uFDZ+5fKmVQKBgQDvZ5usmAKR+OPaI2Pv
Oboo0fB/Xh2nIYRk86UHPEWd8NvRG2e1lbRDYn5IxPbWWPtDv6ILhKPewz1b6mvw
JZYjPWsS8mkT+Cejo11+bBs746ndhIb/gBWbnpfZBCY9wdvc+VWB1aQqwDb0TjsX
xqWs5YtH7ajbpaLAPTxbG58vBQKBgQDo+uj2X+NS7LS9GikYqc6i68FdOFHUH+XW
MSTlQBWwFOiC94rietxDTSkds4CjL0zfUnYO7cmTlK6ixYS3i8msGVw+VdenLOP/
hdRSSkYy+n6fWwy+4o9fOsrzhMArnSdKAJVb7Mlaos+3z1J+5Z56HogQbwUJXCSV
BcdtYamabwKBgBaK3/q5eYx7LiFNMczF18SeOBIWL56cJlZHJuPuhfOgSWKAPRy5
EvdBX/jEKyX1zPsNIVoKTE/efHmaMj2znFaHIvzuvHw34qui51vPHCVgg48rOnb2
fZJgtZWmsV8hUO2WwLlv/3xTCxmoACJ1/wWvu5SzSTIdf5ywZ22AxVVtAoGAfKLe
Rg9+GTqwZgm8uoj9FoNw6mHaxNRbrH6V8l6aO5yz1nx/PDHl68s3l8ATrTj8suv2
ZH4pPF5qHoH0QgzyUrMuedqKh9CoGGaL84nwjA0d+DpJU0T41kUplaUK+UoVXq15
Obgu7+Hxpa+vvlswsLvspn39/8ffeimhSo7YoNkCgYEAkgLqzZ4bJTH7rfuodGkG
M+oOYGxdLep3fg6DE1xwSiGkMqFkodGtR6LNK2Fc/6DIlho3M7VsQkdfDBeDhYjP
krcc7GlWFVqoFLRP6DsNbRzW1bOCOAZLfOVq0AEPN+2VXvXmrqiRoBXN7cAwH/E6
FO3LPObNpUirt6dAlT6Fy4o=
-----END PRIVATE KEY-----";

    const TEST_RSA_PUBLIC_KEY_2: &str = "-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA2eCKHrye0ZklKEJrwzrL
v27QNte3750VmMx0HYcYQH/Qd8GvJICBCEReNLma7C/7ncZPIK27/4YRG7qVm9a6
Ls/Ybcqi4TKvrYtT5hYHoOAbx7D5UIQIIBxL1IJkVkXZsDPKolGzTpFSky/5Iaq4
nDEcsa2tfk6eNwlzrCbXoSH72zlEWuRwtKlhgLx3SHvNtLPmfvgHw/eNwvSImeb0
Y7ZmmdSVDBId2NiLKWOZFnZW4B+RGvJ5IlGo8FchYH1Mf1shxweuclM+p+T8cUdd
md7nJrTEUtkscUjNtvXcQ28AyygrFZWDRDUiH1P2QRVk4YpL3dx28/Li5W8mq5tl
KwIDAQAB
-----END PUBLIC KEY-----";

    fn make_test_claims() -> Claims {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        Claims {
            sub: "user-123".to_string(),
            exp: (now.as_secs() + 3600) as usize,
            iat: now.as_secs() as usize,
            iss: "backbone".to_string(),
        }
    }

    fn make_test_refresh_claims() -> RefreshTokenClaims {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        RefreshTokenClaims {
            sub: "user-123".to_string(),
            exp: (now.as_secs() + 3600) as usize,
            iat: now.as_secs() as usize,
            iss: "backbone".to_string(),
            token_type: "refresh".to_string(),
        }
    }

    // =========================================================================
    // HS256 tests (backward compatibility)
    // =========================================================================

    #[test]
    fn test_single_key_backward_compat() {
        let service = JwtService::new("test_secret_key");
        let claims = make_test_claims();

        let token = service.create_token(&claims).unwrap();
        let validated = service.validate_token(&token).unwrap();

        assert_eq!(validated.sub, "user-123");
        assert_eq!(validated.iss, "backbone");
    }

    #[test]
    fn test_token_has_kid_header() {
        let service = JwtService::new("test_secret_key");
        let claims = make_test_claims();

        let token = service.create_token(&claims).unwrap();

        let header = jsonwebtoken::decode_header(&token).unwrap();
        assert!(header.kid.is_some());
        assert_eq!(header.kid.unwrap(), service.active_kid());
    }

    #[test]
    fn test_rotate_key_old_token_valid() {
        let service = JwtService::new("original_secret");
        let claims = make_test_claims();

        // Create token with original key
        let old_token = service.create_token(&claims).unwrap();

        // Rotate to new key
        service.rotate_key("new_secret_after_rotation").unwrap();

        // Old token should still validate (within grace period)
        let validated = service.validate_token(&old_token).unwrap();
        assert_eq!(validated.sub, "user-123");
    }

    #[test]
    fn test_rotate_key_new_token_valid() {
        let service = JwtService::new("original_secret");

        // Rotate key
        service.rotate_key("new_secret_after_rotation").unwrap();

        // New tokens should validate
        let claims = make_test_claims();
        let new_token = service.create_token(&claims).unwrap();
        let validated = service.validate_token(&new_token).unwrap();
        assert_eq!(validated.sub, "user-123");
    }

    #[test]
    fn test_grace_period_expiry() {
        // Use zero grace period
        let config = KeyRotationConfig {
            grace_period: chrono::Duration::zero(),
        };
        let service = JwtService::with_rotation("original_secret", config);
        let claims = make_test_claims();

        let old_token = service.create_token(&claims).unwrap();

        // Rotate — with zero grace, old key is immediately expired
        service.rotate_key("new_secret").unwrap();

        // Old token should fail validation
        let result = service.validate_token(&old_token);
        assert!(result.is_err());
    }

    #[test]
    fn test_backward_compat_no_kid() {
        // Simulate a token created without kid (e.g., before rotation was enabled)
        use jsonwebtoken::{encode, EncodingKey, Header};

        let secret = "test_secret_key";
        let service = JwtService::new(secret);

        // Create token manually without kid
        let claims = make_test_claims();
        let header = Header::default(); // No kid
        let token = encode(&header, &claims, &EncodingKey::from_secret(secret.as_ref())).unwrap();

        // Should still validate (tries active key since no kid to match)
        let validated = service.validate_token(&token).unwrap();
        assert_eq!(validated.sub, "user-123");
    }

    #[test]
    fn test_multiple_rotations() {
        let config = KeyRotationConfig {
            grace_period: chrono::Duration::hours(24),
        };
        let service = JwtService::with_rotation("secret_v1", config);

        let claims = make_test_claims();
        let token_v1 = service.create_token(&claims).unwrap();

        service.rotate_key("secret_v2").unwrap();
        let token_v2 = service.create_token(&claims).unwrap();

        service.rotate_key("secret_v3").unwrap();
        let token_v3 = service.create_token(&claims).unwrap();

        // All tokens should still validate (within 24h grace)
        assert!(service.validate_token(&token_v1).is_ok());
        assert!(service.validate_token(&token_v2).is_ok());
        assert!(service.validate_token(&token_v3).is_ok());
    }

    #[test]
    fn test_refresh_token_rotation() {
        let service = JwtService::new("original_secret");
        let claims = make_test_refresh_claims();

        let old_token = service.create_refresh_token(&claims).unwrap();

        // Rotate key
        service.rotate_key("new_secret").unwrap();

        // Old refresh token should still validate
        let validated = service.validate_refresh_token(&old_token).unwrap();
        assert_eq!(validated.sub, "user-123");
        assert_eq!(validated.token_type, "refresh");

        // New refresh token should also validate
        let new_token = service.create_refresh_token(&claims).unwrap();
        let validated = service.validate_refresh_token(&new_token).unwrap();
        assert_eq!(validated.sub, "user-123");
    }

    // =========================================================================
    // RS256 tests
    // =========================================================================

    #[test]
    fn test_rs256_create_validate() {
        let service = JwtService::new_rs256(TEST_RSA_PRIVATE_KEY, TEST_RSA_PUBLIC_KEY).unwrap();
        let claims = make_test_claims();

        let token = service.create_token(&claims).unwrap();
        let validated = service.validate_token(&token).unwrap();

        assert_eq!(validated.sub, "user-123");
        assert_eq!(validated.iss, "backbone");

        // Verify the token header uses RS256
        let header = jsonwebtoken::decode_header(&token).unwrap();
        assert_eq!(header.alg, jsonwebtoken::Algorithm::RS256);
    }

    #[test]
    fn test_rs256_validate_with_public_key_only() {
        // Create service with full key pair for signing
        let signing_service = JwtService::new_rs256(TEST_RSA_PRIVATE_KEY, TEST_RSA_PUBLIC_KEY).unwrap();
        let claims = make_test_claims();
        let token = signing_service.create_token(&claims).unwrap();

        // Validate using only the public key (simulates a different service)
        // We use the public key as "private" too — validation only needs the public key
        let validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        let decoding_key = jsonwebtoken::DecodingKey::from_rsa_pem(TEST_RSA_PUBLIC_KEY.as_bytes()).unwrap();
        let decoded = jsonwebtoken::decode::<Claims>(&token, &decoding_key, &validation).unwrap();

        assert_eq!(decoded.claims.sub, "user-123");
    }

    #[test]
    fn test_rs256_key_rotation() {
        let service = JwtService::new_rs256(TEST_RSA_PRIVATE_KEY, TEST_RSA_PUBLIC_KEY).unwrap();
        let claims = make_test_claims();

        // Create token with original key pair
        let old_token = service.create_token(&claims).unwrap();

        // Rotate to new key pair
        service.rotate_rsa_key(TEST_RSA_PRIVATE_KEY_2, TEST_RSA_PUBLIC_KEY_2).unwrap();

        // Old token should still validate (within grace period)
        let validated = service.validate_token(&old_token).unwrap();
        assert_eq!(validated.sub, "user-123");

        // New tokens should also validate
        let new_token = service.create_token(&claims).unwrap();
        let validated = service.validate_token(&new_token).unwrap();
        assert_eq!(validated.sub, "user-123");
    }

    #[test]
    fn test_rs256_grace_period_expiry() {
        let config = KeyRotationConfig {
            grace_period: chrono::Duration::zero(),
        };
        let service = JwtService::with_rs256_rotation(TEST_RSA_PRIVATE_KEY, TEST_RSA_PUBLIC_KEY, config).unwrap();
        let claims = make_test_claims();

        let old_token = service.create_token(&claims).unwrap();

        // Rotate with zero grace period
        service.rotate_rsa_key(TEST_RSA_PRIVATE_KEY_2, TEST_RSA_PUBLIC_KEY_2).unwrap();

        // Old token should fail
        assert!(service.validate_token(&old_token).is_err());

        // New token should work
        let new_token = service.create_token(&claims).unwrap();
        assert!(service.validate_token(&new_token).is_ok());
    }

    #[test]
    fn test_cross_algorithm_rejection() {
        // Token signed with HS256 should not validate on RS256 service
        let hs_service = JwtService::new("test_secret");
        let rs_service = JwtService::new_rs256(TEST_RSA_PRIVATE_KEY, TEST_RSA_PUBLIC_KEY).unwrap();
        let claims = make_test_claims();

        let hs_token = hs_service.create_token(&claims).unwrap();
        let rs_token = rs_service.create_token(&claims).unwrap();

        // Cross-validation should fail
        assert!(rs_service.validate_token(&hs_token).is_err());
        assert!(hs_service.validate_token(&rs_token).is_err());
    }

    #[test]
    fn test_public_key_export() {
        // RS256 service should export public key
        let rs_service = JwtService::new_rs256(TEST_RSA_PRIVATE_KEY, TEST_RSA_PUBLIC_KEY).unwrap();
        let public_key = rs_service.public_key_pem();
        assert!(public_key.is_some());
        assert!(public_key.unwrap().contains("BEGIN PUBLIC KEY"));

        // HS256 service should return None
        let hs_service = JwtService::new("secret");
        assert!(hs_service.public_key_pem().is_none());
    }

    #[test]
    fn test_rs256_backward_compat() {
        // HS256 API should be completely unchanged
        let service = JwtService::new("my_secret");
        assert_eq!(service.algorithm(), JwtAlgorithm::HS256);
        assert!(service.public_key_pem().is_none());

        let claims = make_test_claims();
        let token = service.create_token(&claims).unwrap();
        let validated = service.validate_token(&token).unwrap();
        assert_eq!(validated.sub, "user-123");

        // Rotation still works
        service.rotate_key("new_secret").unwrap();
        assert!(service.validate_token(&token).is_ok());
    }
}
