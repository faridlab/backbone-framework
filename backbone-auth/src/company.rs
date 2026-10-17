//! Company authentication for guarded HTTP surfaces (feature `axum`).
//!
//! Handlers must NOT trust a client-supplied `company_id`: if the company comes off the JSON body, a
//! caller can stamp a record with any company and write into another company's data. Here the company is
//! derived from a **signed** Bearer access token — [`company_auth`] validates the JWT, requires a
//! `company_id` claim, and inserts a [`CompanyContext`] into request extensions; handlers read it via the
//! [`CompanyContext`] extractor. No `company_id` crosses the wire in a request body.
//!
//! Promoted from `backbone-pos` (which proved the pattern against a cross-company maturity-council
//! finding, tests `TG-1`..`TG-4`) once a second guarded module needed it. The guard is **fail-closed**:
//! a token that authenticates a user but carries no `company_id` is rejected, because a request that
//! cannot name its company must never reach a writer.
//!
//! # Wiring
//!
//! The composing service builds one [`CompanyVerifier`] from its JWT secret and hands it to the module's
//! guarded route composer:
//!
//! ```rust,ignore
//! use axum::{middleware::from_fn_with_state, routing::post, Router};
//! use backbone_auth::company::{company_auth, CompanyVerifier};
//!
//! let verifier = CompanyVerifier::hs256(jwt_secret.as_bytes());
//! let router = Router::new()
//!     .route("/pos-sales", post(ring_sale))
//!     .layer(from_fn_with_state(verifier, company_auth))
//!     .with_state(state);
//! ```
//!
//! A handler then takes the company as an argument and never reads it from the body:
//!
//! ```rust,ignore
//! async fn ring_sale(company: CompanyContext, Json(body): Json<SaleBody>) -> Response {
//!     // company.company_id is proven by the token's signature.
//! }
//! ```

use std::sync::Arc;

use axum::{
    extract::{FromRequestParts, Request, State},
    http::{header, request::Parts, Extensions, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::audience::AudienceBinding;
use crate::org::{session_validation, TOKEN_TYPE_ACCESS};

/// The company + subject proven by a validated access token.
///
/// Populated by [`company_auth`] and read by guarded handlers via the [`FromRequestParts`] impl below.
/// Every field here is derived from a signed claim — none of it is client-supplied request data.
#[derive(Debug, Clone)]
pub struct CompanyContext {
    /// The company the caller is acting for. Proven by the token signature; required.
    pub company_id: Uuid,
    /// The branch within the company, when the deployment models an org tree.
    pub branch_id: Option<Uuid>,
    /// The authenticated principal (the token's `sub`).
    pub user_id: String,
}

/// The access-token claims a guarded surface trusts.
///
/// `company_id` is REQUIRED to pass the guard — a token without it is rejected with 401. `branch_id` is
/// optional, for deployments that do not model an org tree. `typ` is checked when present:
/// anything other than `"access"` is refused, so a refresh token never passes as an access
/// credential.
#[derive(Debug, Serialize, Deserialize)]
pub struct CompanyClaims {
    /// Subject (the authenticated user/principal id).
    pub sub: String,
    /// Expiry (seconds since epoch) — standard JWT claim, validated.
    pub exp: usize,
    /// The company this token acts for. Absent → the guard rejects the request.
    #[serde(default)]
    pub company_id: Option<Uuid>,
    /// The branch this token acts for, when modelled.
    #[serde(default)]
    pub branch_id: Option<Uuid>,
    /// Token purpose. Absent on tokens minted before this field existed (accepted); a present
    /// value other than `"access"` — e.g. a refresh token — is refused.
    #[serde(default)]
    pub typ: Option<String>,
    /// The tenants this token was minted for (the standard `aud` claim, a string or an array).
    /// Empty on tokens minted without a tenant — see [`crate::audience`].
    #[serde(default, deserialize_with = "crate::audience::deserialize_audience")]
    pub aud: Vec<String>,
}

/// Verifier the composing service builds once (from its JWT secret) and clones into guarded routes.
///
/// Bind it to the request's tenant with [`bind_audience`](Self::bind_audience) in any deployment
/// where tenants share a signing key.
#[derive(Clone)]
pub struct CompanyVerifier {
    key: Arc<DecodingKey>,
    validation: Arc<Validation>,
    audience: Option<AudienceBinding>,
}

impl CompanyVerifier {
    /// HS256 verifier over a shared secret (the common single-service deployment).
    pub fn hs256(secret: &[u8]) -> Self {
        Self {
            key: Arc::new(DecodingKey::from_secret(secret)),
            validation: Arc::new(session_validation(Algorithm::HS256)),
            audience: None,
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
            validation: Arc::new(session_validation(Algorithm::RS256)),
            audience: None,
        })
    }

    /// Bind the guard to the request's tenant: `source` reads the tenant the request was routed
    /// to from its extensions, and [`company_auth`] refuses (401) a token whose `aud` does not
    /// name it. A surface that resolves no tenant accepts only tokens that name none. With
    /// `backbone-tenant`, pass `backbone_tenant::axum_router::tenant_audience`.
    #[must_use]
    pub fn bind_audience<F>(mut self, source: F) -> Self
    where
        F: Fn(&Extensions) -> Option<String> + Send + Sync + 'static,
    {
        self.audience = Some(AudienceBinding::new(source));
        self
    }

    /// Accept tokens that name no tenant on a tenant's surface, as before the binding existed.
    /// Off by default; see [`crate::org::OrgVerifier::accept_unbound_tokens`].
    #[deprecated(
        note = "accepts tokens that can be replayed across tenants; refresh the session instead"
    )]
    #[must_use]
    pub fn accept_unbound_tokens(mut self, accept: bool) -> Self {
        if let Some(binding) = self.audience.as_mut() {
            binding.accept_unbound = accept;
        }
        self
    }

    /// Validate a raw access token → a company context, or `None` if the signature/expiry is
    /// bad, the `company_id` claim is absent, or the token is not an access credential.
    ///
    /// A `typ` claim of anything other than `"access"` fails verification: a refresh token is a
    /// rotation credential, and presenting it to a guarded route must not open a scoped session.
    /// Tokens minted before `typ` existed carry no claim and stay accepted.
    ///
    /// This does NOT check the token's tenant — [`company_auth`] does, through the binding; use
    /// [`verify_for_audience`](Self::verify_for_audience) outside a request.
    pub fn verify(&self, token: &str) -> Option<CompanyContext> {
        self.verify_claims(token).map(|(ctx, _)| ctx)
    }

    /// [`verify`](Self::verify), plus the tenant check: `tenant` is the tenant the caller serves
    /// (`None` for a surface that serves none), and the token must have been minted for it.
    pub fn verify_for_audience(&self, token: &str, tenant: Option<&str>) -> Option<CompanyContext> {
        let (ctx, aud) = self.verify_claims(token)?;
        crate::audience::admits(tenant, &aud, false).then_some(ctx)
    }

    fn verify_claims(&self, token: &str) -> Option<(CompanyContext, Vec<String>)> {
        let data = decode::<CompanyClaims>(token, &self.key, &self.validation).ok()?;
        let c = data.claims;
        if let Some(typ) = &c.typ {
            if typ != TOKEN_TYPE_ACCESS {
                return None;
            }
        }
        let ctx = CompanyContext {
            company_id: c.company_id?,
            branch_id: c.branch_id,
            user_id: c.sub,
        };
        Some((ctx, c.aud))
    }
}

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "unauthorized", "message": message })),
    )
        .into_response()
}

fn internal_error(message: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "internal_error", "message": message })),
    )
        .into_response()
}

/// Middleware: validate the Bearer token and insert a [`CompanyContext`]; reject with 401 otherwise.
///
/// Mount on guarded write routes via `from_fn_with_state(verifier, company_auth)`.
pub async fn company_auth(
    State(verifier): State<CompanyVerifier>,
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
    match verifier.verify_claims(token) {
        // Tenants share the signing key, so the token must name THIS request's tenant.
        Some((_, aud))
            if verifier
                .audience
                .as_ref()
                .is_some_and(|binding| !binding.admits(req.extensions(), &aud)) =>
        {
            unauthorized("token was not issued for this tenant")
        }
        Some((ctx, _)) => {
            // Bind the request's company for the whole downstream handler so the RLS fence (ADR-0008)
            // returns this company's rows: every ORM statement runs with `app.company_id` set to
            // `ctx.company_id` — the same signed id the write guard trusts.
            //
            // If the app inserted its `PgPool` as an extension, upgrade to a request-DEDICATED
            // connection (`with_request_scope`): the scope rides one connection for the whole request,
            // so even raw-`sqlx` custom services and ID-only lookups (`WHERE id = $1`, no company in
            // the query) are fenced. Without the pool extension, fall back to per-statement scoping —
            // correct for the ORM path, but custom raw services would fail closed. This keeps the
            // middleware backward-compatible: apps opt into the stronger scope by inserting the pool.
            let company_id = ctx.company_id;
            let pool = req.extensions().get::<backbone_orm::PgPool>().cloned();
            req.extensions_mut().insert(ctx);
            match pool {
                Some(pool) => {
                    match backbone_orm::company_scope::with_request_scope(
                        &pool,
                        company_id,
                        next.run(req),
                    )
                    .await
                    {
                        Ok(resp) => resp,
                        Err(_) => internal_error("could not establish the request database scope"),
                    }
                }
                None => backbone_orm::with_company_scope(Some(company_id), next.run(req)).await,
            }
        }
        None => unauthorized("invalid token or missing company_id claim"),
    }
}

/// Extractor: pull the [`CompanyContext`] the middleware inserted (401 if the route was reached without
/// it — a wiring error, since the middleware rejects unauthenticated requests first).
#[async_trait::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for CompanyContext {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<CompanyContext>()
            .cloned()
            .ok_or_else(|| unauthorized("unauthenticated"))
    }
}
