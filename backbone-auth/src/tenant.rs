//! Tenant authentication for guarded HTTP surfaces (feature `axum`).
//!
//! Handlers must NOT trust a client-supplied `company_id`: if the tenant comes off the JSON body, a
//! caller can stamp a record with any tenant and write into another company's data. Here the tenant is
//! derived from a **signed** Bearer access token — [`tenant_auth`] validates the JWT, requires a
//! `company_id` claim, and inserts a [`TenantContext`] into request extensions; handlers read it via the
//! [`TenantContext`] extractor. No `company_id` crosses the wire in a request body.
//!
//! Promoted from `backbone-pos` (which proved the pattern against a cross-tenant maturity-council
//! finding, tests `TG-1`..`TG-4`) once a second guarded module needed it. The guard is **fail-closed**:
//! a token that authenticates a user but carries no `company_id` is rejected, because a request that
//! cannot name its tenant must never reach a writer.
//!
//! # Wiring
//!
//! The composing service builds one [`TenantVerifier`] from its JWT secret and hands it to the module's
//! guarded route composer:
//!
//! ```rust,ignore
//! use axum::{middleware::from_fn_with_state, routing::post, Router};
//! use backbone_auth::tenant::{tenant_auth, TenantVerifier};
//!
//! let verifier = TenantVerifier::hs256(jwt_secret.as_bytes());
//! let router = Router::new()
//!     .route("/pos-sales", post(ring_sale))
//!     .layer(from_fn_with_state(verifier, tenant_auth))
//!     .with_state(state);
//! ```
//!
//! A handler then takes the tenant as an argument and never reads it from the body:
//!
//! ```rust,ignore
//! async fn ring_sale(tenant: TenantContext, Json(body): Json<SaleBody>) -> Response {
//!     // tenant.company_id is proven by the token's signature.
//! }
//! ```

use std::sync::Arc;

use axum::{
    extract::{FromRequestParts, Request, State},
    http::{header, request::Parts, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The tenant + subject proven by a validated access token.
///
/// Populated by [`tenant_auth`] and read by guarded handlers via the [`FromRequestParts`] impl below.
/// Every field here is derived from a signed claim — none of it is client-supplied request data.
#[derive(Debug, Clone)]
pub struct TenantContext {
    /// The tenant the caller is acting for. Proven by the token signature; required.
    pub company_id: Uuid,
    /// The branch within the tenant, when the deployment models an org tree.
    pub branch_id: Option<Uuid>,
    /// The authenticated principal (the token's `sub`).
    pub user_id: String,
}

/// The access-token claims a guarded surface trusts.
///
/// `company_id` is REQUIRED to pass the guard — a token without it is rejected with 401. `branch_id` is
/// optional, for deployments that do not model an org tree.
#[derive(Debug, Serialize, Deserialize)]
pub struct TenantClaims {
    /// Subject (the authenticated user/principal id).
    pub sub: String,
    /// Expiry (seconds since epoch) — standard JWT claim, validated.
    pub exp: usize,
    /// The tenant this token acts for. Absent → the guard rejects the request.
    #[serde(default)]
    pub company_id: Option<Uuid>,
    /// The branch this token acts for, when modelled.
    #[serde(default)]
    pub branch_id: Option<Uuid>,
}

/// Verifier the composing service builds once (from its JWT secret) and clones into guarded routes.
#[derive(Clone)]
pub struct TenantVerifier {
    key: Arc<DecodingKey>,
    validation: Arc<Validation>,
}

impl TenantVerifier {
    /// HS256 verifier over a shared secret (the common single-service deployment).
    pub fn hs256(secret: &[u8]) -> Self {
        Self {
            key: Arc::new(DecodingKey::from_secret(secret)),
            validation: Arc::new(Validation::new(Algorithm::HS256)),
        }
    }

    /// RS256 verifier over a PEM-encoded public key, for deployments where the issuer signs with a
    /// private key this service never holds.
    ///
    /// # Errors
    /// Returns an error if `public_key_pem` is not a valid PEM-encoded RSA public key.
    pub fn rs256(public_key_pem: &[u8]) -> Result<Self, jsonwebtoken::errors::Error> {
        Ok(Self {
            key: Arc::new(DecodingKey::from_rsa_pem(public_key_pem)?),
            validation: Arc::new(Validation::new(Algorithm::RS256)),
        })
    }

    /// Validate a raw token → a tenant context, or `None` if the signature/expiry is bad or the
    /// `company_id` claim is absent.
    fn verify(&self, token: &str) -> Option<TenantContext> {
        let data = decode::<TenantClaims>(token, &self.key, &self.validation).ok()?;
        let c = data.claims;
        Some(TenantContext {
            company_id: c.company_id?,
            branch_id: c.branch_id,
            user_id: c.sub,
        })
    }
}

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "unauthorized", "message": message })),
    )
        .into_response()
}

/// Middleware: validate the Bearer token and insert a [`TenantContext`]; reject with 401 otherwise.
///
/// Mount on guarded write routes via `from_fn_with_state(verifier, tenant_auth)`.
pub async fn tenant_auth(
    State(verifier): State<TenantVerifier>,
    mut req: Request,
    next: Next,
) -> Response {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|raw| {
            raw.strip_prefix("Bearer ")
                .or_else(|| raw.strip_prefix("bearer "))
        });
    let Some(token) = token else {
        return unauthorized("missing bearer token");
    };
    match verifier.verify(token) {
        Some(ctx) => {
            req.extensions_mut().insert(ctx);
            next.run(req).await
        }
        None => unauthorized("invalid token or missing company_id claim"),
    }
}

/// Extractor: pull the [`TenantContext`] the middleware inserted (401 if the route was reached without
/// it — a wiring error, since the middleware rejects unauthenticated requests first).
#[async_trait::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for TenantContext {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<TenantContext>()
            .cloned()
            .ok_or_else(|| unauthorized("unauthenticated"))
    }
}
