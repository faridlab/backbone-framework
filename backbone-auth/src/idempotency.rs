//! HTTP request idempotency via the `Idempotency-Key` header.
//!
//! A self-contained Axum middleware that makes mutating client requests replay-safe: a client that
//! retries a `POST /pos-sales` (network blip, timeout) with the same `Idempotency-Key` gets the
//! *same* response back and the write is applied **once**, not duplicated.
//!
//! Dedup scope: `(company_id, key)`. `company_id` is proven from the signed Bearer token (this
//! middleware decodes it itself, so it can run as an OUTER layer — no need to mount inner to
//! `company_auth`). The key is the client-supplied `Idempotency-Key` header (a UUID per intent).
//!
//! Semantics:
//! - Only mutating methods (`POST`/`PUT`/`PATCH`/`DELETE`) carrying the header are deduped; all
//!   others pass through untouched.
//! - Only **2xx** responses are cached + replayed. Non-2xx (4xx/5xx) are not cached — the client
//!   is expected to retry, and the next attempt re-runs the handler.
//! - A request with the header but no/invalid token passes through (so `company_auth` returns its
//!   normal 401 — we don't cache auth failures).
//!
//! Storage: `public.idempotency_requests` (created by [`migrate`]). No RLS — the table is filtered
//! explicitly by the proven `company_id`, so a tenant can only dedup its own keys.

use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{header, Method, StatusCode},
    middleware::Next,
    response::Response,
};
use sqlx::PgPool;

use crate::company::{CompanyVerifier, CompanyContext};

/// Max response body size to capture+replay (64 KiB). Larger 2xx responses pass through uncached —
/// idempotency matters for mutating writes, whose responses are small (an id / ack).
const MAX_BODY: usize = 64 * 1024;

#[derive(Clone)]
pub struct IdempotencyState {
    pub verifier: CompanyVerifier,
    pub pool: PgPool,
}

impl IdempotencyState {
    pub fn new(verifier: CompanyVerifier, pool: PgPool) -> Self {
        Self { verifier, pool }
    }
}

/// Create the `public.idempotency_requests` table. Idempotent. Run once at startup.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS public.idempotency_requests (
               company_id  uuid        NOT NULL,
               key         text        NOT NULL,
               status_code int         NOT NULL,
               body        text        NOT NULL,
               created_at  timestamptz NOT NULL DEFAULT now(),
               PRIMARY KEY (company_id, key)
           )"#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The idempotency middleware. Mount as an outer layer via
/// `from_fn_with_state(IdempotencyState::new(verifier, pool), idempotency_middleware)`.
pub async fn idempotency_middleware(
    State(st): State<IdempotencyState>,
    req: Request,
    next: Next,
) -> Response {
    // Only mutating methods opt in.
    if !matches!(*req.method(), Method::POST | Method::PUT | Method::PATCH | Method::DELETE) {
        return next.run(req).await;
    }
    // Only when the client sent a key.
    let key = match req.headers().get("idempotency-key").and_then(|v| v.to_str().ok()) {
        Some(k) => k.to_string(),
        None => return next.run(req).await,
    };
    // company_id from the signed token (pass through if absent/invalid — let company_auth 401).
    let company_id = match company_from_request(&st.verifier, &req) {
        Some(c) => c.company_id,
        None => return next.run(req).await,
    };

    // Cache hit → replay.
    if let Some((code, body)) = lookup(&st.pool, company_id, &key).await {
        return Response::builder()
            .status(StatusCode::from_u16(code).unwrap_or(StatusCode::OK))
            .header(header::CONTENT_TYPE, "application/json")
            .header("idempotent-replay", "true")
            .body(Body::from(body))
            .unwrap();
    }

    // Cache miss → forward, capture 2xx, store.
    let resp = next.run(req).await;
    if !resp.status().is_success() {
        return resp; // don't cache non-2xx
    }
    let code = resp.status().as_u16();
    let (parts, body) = resp.into_parts();
    let bytes = to_bytes(body, MAX_BODY).await.unwrap_or_default();
    if bytes.len() <= MAX_BODY {
        let _ = store(&st.pool, company_id, &key, code, &bytes).await;
    }
    Response::from_parts(parts, Body::from(bytes))
}

fn company_from_request(verifier: &CompanyVerifier, req: &Request) -> Option<CompanyContext> {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|raw| raw.strip_prefix("Bearer ").or_else(|| raw.strip_prefix("bearer ")))?;
    verifier.verify(token)
}

async fn lookup(pool: &PgPool, company_id: uuid::Uuid, key: &str) -> Option<(u16, String)> {
    sqlx::query_as::<_, (i32, String)>(
        "SELECT status_code, body FROM public.idempotency_requests WHERE company_id=$1 AND key=$2",
    )
    .bind(company_id)
    .bind(key)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|(code, body)| (code as u16, body))
}

async fn store(
    pool: &PgPool,
    company_id: uuid::Uuid,
    key: &str,
    code: u16,
    body: &[u8],
) -> Result<(), sqlx::Error> {
    let body_text = std::str::from_utf8(body).unwrap_or("").to_string();
    sqlx::query(
        r#"INSERT INTO public.idempotency_requests (company_id, key, status_code, body)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (company_id, key) DO NOTHING"#,
    )
    .bind(company_id)
    .bind(key)
    .bind(code as i32)
    .bind(body_text)
    .execute(pool)
    .await?;
    Ok(())
}
