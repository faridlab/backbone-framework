//! Org-tree session guard for the entitlement-union fence (feature `axum`, ADR-0028), and the
//! issuer that mints the tokens it trusts.
//!
//! The twin of [`crate::company::company_auth`] for org-re-keyed surfaces. A handler must not
//! trust a client-supplied `org_unit_id`: here the acting node comes off a **signed** Bearer
//! access token, and the visible scope is not the token's at all — it is **resolved from the
//! tenant's own org tree** (the union of the subtrees under the acting node and any additional
//! entitled nodes, plus the root). A token that names a unit this tenant's tree does not hold is
//! refused: 403, not a narrower session.
//!
//! The guard depends on the tenant router having run **outside** it: it reads the
//! `backbone_orm::PgPool` the router inserted — THIS tenant's pool — resolves the scope over it,
//! and wraps the whole downstream handler in
//! [`with_org_request_scope`](backbone_orm::org_scope::with_org_request_scope), which carries
//! `app.scope_unit_ids` (and the legacy `app.company_id` bridge) on one request-dedicated
//! connection for the entire request. ID-only lookups and raw-`sqlx` statements are thereby
//! fenced by the connection, not the query text.
//!
//! Fail-closed, same contract as the company guard: a token without an `org_unit_id` claim is
//! 401 — a request that cannot name its node must never reach a writer.
//!
//! # Wiring
//!
//! ```rust,ignore
//! use axum::{middleware::from_fn_with_state, routing::post, Router};
//! use backbone_auth::org::{org_auth, OrgVerifier};
//!
//! let verifier = OrgVerifier::hs256(jwt_secret.as_bytes());
//! let router = Router::new()
//!     .route("/stock-moves", post(record_move))
//!     // tenant_route mounts OUTSIDE org_auth — it picks the pool this guard scopes over.
//!     .layer(from_fn_with_state(verifier, org_auth))
//!     .with_state(state);
//! ```
//!
//! A handler takes the context and never reads an org unit from the body:
//!
//! ```rust,ignore
//! async fn record_move(org: OrgContext, Json(body): Json<MoveBody>) -> Response {
//!     // org.acting_unit_id is proven by the token; the fence is proven by the connection.
//! }
//! ```

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{
    extract::{FromRequestParts, Request, State},
    http::{header, request::Parts, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Marks a token's purpose. Access tokens carry `"access"` and open scoped sessions; refresh
/// tokens carry `"refresh"` and are rotation credentials ONLY — the verifier refuses them, so a
/// long-lived token can never ride a guarded route.
pub const TOKEN_TYPE_ACCESS: &str = "access";
pub const TOKEN_TYPE_REFRESH: &str = "refresh";

/// The acting node proven by a validated access token.
///
/// Populated by [`org_auth`] and read by guarded handlers via the [`FromRequestParts`] impl
/// below. `acting_unit_id` and `entitled_units` are signed claims; the *resolved* scope (subtree
/// union + root) is NOT here — it lives in the request's database scope, because only the
/// tenant's tree can compute it.
#[derive(Debug, Clone)]
pub struct OrgContext {
    /// The org node this session acts at — the signed token's `org_unit_id`.
    pub acting_unit_id: Uuid,
    /// Further nodes the token holds entitlements for (a group admin's sister companies). Each
    /// contributes its whole subtree at resolution time.
    pub entitled_units: Vec<Uuid>,
    /// The authenticated principal (the token's `sub`).
    pub user_id: String,
}

/// The access-token claims an org-guarded surface trusts.
///
/// `org_unit_id` is REQUIRED to pass the guard — a token without it is rejected with 401.
/// `entitled_units` is optional (empty default) for the common single-node session. `typ` is
/// checked when present: anything other than [`TOKEN_TYPE_ACCESS`] is refused, so a refresh
/// token never passes as an access credential.
#[derive(Debug, Serialize, Deserialize)]
pub struct OrgClaims {
    /// Subject (the authenticated user/principal id).
    pub sub: String,
    /// Expiry (seconds since epoch) — standard JWT claim, validated.
    pub exp: usize,
    /// The org node this token acts for. Absent → the guard rejects the request.
    #[serde(default)]
    pub org_unit_id: Option<Uuid>,
    /// Additional entitled nodes, when the session spans more than its acting node.
    #[serde(default)]
    pub entitled_units: Vec<Uuid>,
    /// Token purpose. Absent on tokens minted before this field existed (accepted); a present
    /// value other than `"access"` — e.g. a refresh token — is refused.
    #[serde(default)]
    pub typ: Option<String>,
}

/// Verifier the composing service builds once (from its JWT secret) and clones into guarded
/// routes — the org twin of [`crate::company::CompanyVerifier`].
#[derive(Clone)]
pub struct OrgVerifier {
    key: Arc<DecodingKey>,
    validation: Arc<Validation>,
}

impl OrgVerifier {
    /// HS256 verifier over a shared secret (the common single-service deployment).
    pub fn hs256(secret: &[u8]) -> Self {
        Self {
            key: Arc::new(DecodingKey::from_secret(secret)),
            validation: Arc::new(Validation::new(Algorithm::HS256)),
        }
    }

    /// RS256 verifier over a PEM-encoded public key, for deployments where the issuer signs with
    /// a private key this service never holds.
    ///
    /// # Errors
    /// Returns an error if `public_key_pem` is not a valid PEM-encoded RSA public key.
    pub fn rs256(public_key_pem: &[u8]) -> Result<Self, jsonwebtoken::errors::Error> {
        Ok(Self {
            key: Arc::new(DecodingKey::from_rsa_pem(public_key_pem)?),
            validation: Arc::new(Validation::new(Algorithm::RS256)),
        })
    }

    /// Validate a raw access token → an org context, or `None` if the signature/expiry is bad,
    /// the `org_unit_id` claim is absent, or the token is not an access credential.
    ///
    /// A `typ` claim of anything other than `"access"` fails verification: a refresh token is a
    /// rotation credential, and presenting it to a guarded route must not open a scoped session.
    /// Tokens minted before `typ` existed carry no claim and stay accepted.
    pub fn verify(&self, token: &str) -> Option<OrgContext> {
        let data = decode::<OrgClaims>(token, &self.key, &self.validation).ok()?;
        let c = data.claims;
        if let Some(typ) = &c.typ {
            if typ != TOKEN_TYPE_ACCESS {
                return None;
            }
        }
        Some(OrgContext {
            acting_unit_id: c.org_unit_id?,
            entitled_units: c.entitled_units,
            user_id: c.sub,
        })
    }
}

/// The mint twin of [`OrgVerifier`]: signs the session pair an org-guarded surface later
/// verifies (ADR-0027/0028).
///
/// An org session is born in exactly one place — here. The acting unit and entitlements are
/// resolved from the tenant's own data (membership tables, the org spine) by the calling
/// service and sealed into a signed token; a client never gets to state its org unit.
/// `org_unit_id` is a required argument, mirroring the guard's 401-on-absent claim: a token
/// that cannot name its node is never minted, let alone verified.
///
/// Issued tokens carry a `typ` claim (`"access"` / `"refresh"`); [`OrgVerifier::verify`]
/// refuses anything present that is not `"access"`, so a refresh token cannot ride a guarded
/// route even though it shares the signing key.
///
/// # Wiring
///
/// ```rust,ignore
/// use backbone_auth::org::OrgIssuer;
/// use std::time::Duration;
///
/// let issuer = OrgIssuer::hs256(jwt_secret.as_bytes());
/// // acting unit + entitlements resolved from the tenant's membership data beforehand.
/// let access  = issuer.issue_access(&user_id, acting_unit, &entitled, Duration::from_secs(3600))?;
/// let refresh = issuer.issue_refresh(&user_id, acting_unit, &entitled, Duration::from_secs(7 * 24 * 3600))?;
/// ```
#[derive(Clone)]
pub struct OrgIssuer {
    key: Arc<EncodingKey>,
    algorithm: Algorithm,
}

/// The claims the issuer seals — a wire superset of [`OrgClaims`] (the verifier ignores `iat`;
/// `typ` it checks). Kept private so the decode contract stays minimal and the mint shape
/// cannot drift from what this issuer produces.
#[derive(Serialize)]
struct IssuedSessionClaims {
    sub: String,
    exp: usize,
    iat: usize,
    typ: &'static str,
    org_unit_id: Uuid,
    entitled_units: Vec<Uuid>,
}

impl OrgIssuer {
    /// HS256 issuer over a shared secret (the common single-service deployment).
    pub fn hs256(secret: &[u8]) -> Self {
        Self {
            key: Arc::new(EncodingKey::from_secret(secret)),
            algorithm: Algorithm::HS256,
        }
    }

    /// RS256 issuer over a PEM-encoded RSA private key, for deployments where a separate
    /// verifier holds only the public key.
    ///
    /// # Errors
    /// Returns an error if `private_key_pem` is not a valid PEM-encoded RSA private key.
    pub fn rs256(private_key_pem: &[u8]) -> Result<Self, jsonwebtoken::errors::Error> {
        Ok(Self {
            key: Arc::new(EncodingKey::from_rsa_pem(private_key_pem)?),
            algorithm: Algorithm::RS256,
        })
    }

    /// Mint an access token: the credential guarded routes accept.
    ///
    /// # Errors
    /// Returns an error if the claims cannot be encoded/signed (key or serialization failure).
    pub fn issue_access(
        &self,
        user_id: &str,
        acting_unit: Uuid,
        entitled_units: &[Uuid],
        ttl: Duration,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        self.issue(user_id, acting_unit, entitled_units, ttl, TOKEN_TYPE_ACCESS)
    }

    /// Mint a refresh token: a rotation credential only. [`OrgVerifier::verify`] refuses it;
    /// the issuing service's refresh endpoint is its sole consumer.
    ///
    /// # Errors
    /// Returns an error if the claims cannot be encoded/signed (key or serialization failure).
    pub fn issue_refresh(
        &self,
        user_id: &str,
        acting_unit: Uuid,
        entitled_units: &[Uuid],
        ttl: Duration,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        self.issue(user_id, acting_unit, entitled_units, ttl, TOKEN_TYPE_REFRESH)
    }

    fn issue(
        &self,
        user_id: &str,
        acting_unit: Uuid,
        entitled_units: &[Uuid],
        ttl: Duration,
        typ: &'static str,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        // A clock before the Unix epoch cannot happen on a real host; if it somehow does, the
        // zero `now` makes the token already-expired at verification — fail-closed, never a
        // token valid from the epoch.
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let claims = IssuedSessionClaims {
            sub: user_id.to_string(),
            exp: now.saturating_add(ttl).as_secs() as usize,
            iat: now.as_secs() as usize,
            typ,
            org_unit_id: acting_unit,
            entitled_units: entitled_units.to_vec(),
        };
        encode(&Header::new(self.algorithm), &claims, &self.key)
    }
}

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "unauthorized", "message": message })),
    )
        .into_response()
}

fn forbidden(message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({ "error": "forbidden", "message": message })),
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

/// Middleware: validate the Bearer token, resolve the session's org scope over the request's
/// tenant database, and run the handler inside that scope.
///
/// Mount on guarded routes via `from_fn_with_state(verifier, org_auth)`, INSIDE the tenant
/// router — the scope is resolved on the pool the router attached, so a token for another
/// tenant's tree simply does not resolve here.
///
/// Rejections, in order:
/// - 401 — missing/expired/malformed token, or a token with no `org_unit_id` claim;
/// - 403 — a valid token whose acting node is not in THIS tenant's org tree (the cross-tenant
///   case: identity is fine, the tenant is wrong);
/// - 500 — no pool on the request (router wiring), the org spine absent (deployment), or the
///   database unreachable.
pub async fn org_auth(
    State(verifier): State<OrgVerifier>,
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
    let Some(ctx) = verifier.verify(token) else {
        return unauthorized("invalid token or missing org_unit_id claim");
    };

    // The scope can only be resolved against the tenant's own tree, so this guard has no
    // task-local fallback (unlike the company guard, whose company id rides the token). No pool
    // on the request means the tenant router did not run outside this layer — a wiring error.
    let Some(pool) = req.extensions().get::<backbone_orm::PgPool>().cloned() else {
        return internal_error(
            "no tenant database on the request — mount the tenant router outside org_auth",
        );
    };

    let scope = {
        let mut conn = match pool.acquire().await {
            Ok(conn) => conn,
            Err(e) => {
                tracing::error!(target: "backbone_auth::org", error = %e, "could not reach the tenant database to resolve the org scope");
                return internal_error("could not reach the tenant database");
            }
        };
        match backbone_orm::org_scope::resolve_org_scope(
            &mut *conn,
            ctx.acting_unit_id,
            &ctx.entitled_units,
        )
        .await
        {
            Ok(scope) => scope,
            // A unit the token names but this tree does not hold: the token belongs to another
            // tenant (or a stale tree). Never a narrower session — refuse.
            Err(backbone_orm::org_scope::OrgScopeError::UnknownActingUnit(unit)) => {
                return forbidden(&format!(
                    "org unit {unit} is not in this tenant's organization"
                ))
            }
            Err(backbone_orm::org_scope::OrgScopeError::MissingSpineHelpers) => {
                return internal_error(
                    "the org spine is not migrated in this tenant's database",
                )
            }
            Err(e) => {
                tracing::error!(target: "backbone_auth::org", error = %e, "org scope resolution failed");
                return internal_error("could not resolve the org scope");
            }
        }
    };

    req.extensions_mut().insert(ctx);
    match backbone_orm::org_scope::with_org_request_scope(&pool, scope, next.run(req)).await {
        Ok(resp) => resp,
        Err(_) => internal_error("could not establish the request org scope"),
    }
}

/// Extractor: pull the [`OrgContext`] the middleware inserted (401 if the route was reached
/// without it — a wiring error, since the middleware rejects unauthenticated requests first).
#[async_trait::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for OrgContext {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<OrgContext>()
            .cloned()
            .ok_or_else(|| unauthorized("unauthenticated"))
    }
}
