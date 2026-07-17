//! Request company scope for the Postgres RLS read/write fence (ADR-0008).
//!
//! The security boundary is the database: every company-scoped table carries a Row-Level-Security
//! policy `USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)`. This
//! module is the *application* half — it carries the caller's company for the duration of a request
//! and sets `app.company_id` on the connection each statement runs on.
//!
//! Why a task-local and not a signature parameter: the ORM executes connection-per-statement
//! against a shared pool (`fetch_all(&self.pool)`), so there is no request-held connection to bind,
//! and threading a scope argument through `CrudService::list` would be a breaking change across ~40
//! modules that *still* would not reach raw `sqlx::query` callers. The task-local rides the async
//! task instead, and the scoped execute helpers below set `app.company_id` transaction-locally so a
//! value never leaks onto a pooled connection reused by the next request.
//!
//! **The task-local is not the fence.** RLS is. A statement that runs without the task-local set
//! (a missed call site, a raw query, a spawned job) sees `app.company_id` unset → the policy matches
//! zero rows. That is fail-closed: such a path *breaks* (returns empty), it never leaks. These
//! helpers exist so the ORM read path returns the caller's rows instead of empty — correctness, not
//! safety.

use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgArguments, PgRow};
use sqlx::query::{Query, QueryAs, QueryScalar};
use sqlx::{FromRow, PgPool, Postgres};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

tokio::task_local! {
    /// The company the current request is scoped to. `Platform` callers and unscoped code leave
    /// this unset; `None` inside the scope means an explicit platform (no-fence) caller.
    static COMPANY: Option<Uuid>;

    /// A connection dedicated to the current request, with `app.company_id` already set at the
    /// SESSION level. When present, every scoped execute helper runs on it — so an ID-only lookup in
    /// a hand-written service (e.g. `SELECT … WHERE id = $1`, with no `company_id` in the query) is
    /// still fenced, because the scope rides the connection rather than the query text. This is the
    /// path that makes custom write services RLS-correct without threading a company argument through
    /// every method. Set by [`with_request_scope`].
    static REQUEST_CONN: Arc<Mutex<PoolConnection<Postgres>>>;
}

/// Run `f` with a request-dedicated connection whose `app.company_id` is set to `company`.
///
/// Acquires one connection from `pool`, sets the session variable on it, and binds it as the
/// request connection for the duration of `f`. Every scoped execute helper called inside `f` — from
/// the ORM or a hand-written service — runs on this connection, so the whole request shares one
/// company scope set exactly once (no per-statement transaction, and ID-only lookups are fenced
/// too). The variable is reset before the connection returns to the pool, so it can never ride into
/// the next request.
///
/// Trade-off: this pins a pooled connection for the request's lifetime (vs. connection-per-statement),
/// so size the pool for peak concurrent requests. Prefer this at the HTTP composition root; leave
/// non-request callers (jobs) on [`with_company_scope`] (per-statement scoping).
pub async fn with_request_scope<F, R>(pool: &PgPool, company: Uuid, f: F) -> Result<R, sqlx::Error>
where
    F: Future<Output = R>,
{
    let mut conn = pool.acquire().await?;
    sqlx::query("SELECT set_config('app.company_id', $1, false)")
        .bind(company.to_string())
        .execute(&mut *conn)
        .await?;

    let holder = Arc::new(Mutex::new(conn));
    let result = REQUEST_CONN.scope(holder.clone(), f).await;

    // The scoped clone is dropped when the scope ends, so we reclaim the sole reference, clear the
    // session var, and only then let the connection return to the pool — clean, never leaking scope.
    if let Ok(mutex) = Arc::try_unwrap(holder) {
        let mut conn = mutex.into_inner();
        let _ = sqlx::query("SELECT set_config('app.company_id', '', false)")
            .execute(&mut *conn)
            .await;
    }
    Ok(result)
}

/// The request-dedicated connection, if [`with_request_scope`] set one for this task.
fn request_conn() -> Option<Arc<Mutex<PoolConnection<Postgres>>>> {
    REQUEST_CONN.try_with(|c| c.clone()).ok()
}

/// Run `f` with the request's company scope bound to the current async task.
///
/// Middleware calls this once per request with the company derived from the signed token, so every
/// query issued while handling the request inherits it. `Some(uuid)` fences to that company;
/// `None` is an explicit platform caller (no `app.company_id` is set → RLS-fenced tables return
/// zero rows unless the connecting role bypasses RLS).
pub async fn with_company_scope<F, R>(company: Option<Uuid>, f: F) -> R
where
    F: Future<Output = R>,
{
    COMPANY.scope(company, f).await
}

/// The company bound to the current task, or `None` when no scope is set (unscoped code path).
///
/// `Ok(Some(id))` — fenced to a company. `Ok(None)` / no scope — no `app.company_id` will be set.
/// The two `None` cases are intentionally indistinguishable here: neither sets the session var, and
/// RLS fails closed for both.
pub fn current_company() -> Option<Uuid> {
    COMPANY.try_with(|c| *c).ok().flatten()
}

/// Set `app.company_id` transaction-locally on `conn`.
///
/// `set_config(_, _, true)` — the `true` scopes it to the surrounding transaction, so it is
/// discarded on commit/rollback and cannot ride a pooled connection into the next request.
async fn bind_company(conn: &mut sqlx::PgConnection, company: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_config('app.company_id', $1, true)")
        .bind(company.to_string())
        .execute(conn)
        .await?;
    Ok(())
}

/// Bind an EXPLICIT company onto an already-open transaction/connection.
///
/// For call sites that know their company directly (it is on the DTO, or was just read off the row)
/// and open their own transaction — the common shape in hand-written write services. Prefer this over
/// [`bind_current_company`] when the company is known: it does not depend on an ambient task-local, so
/// it is correct for non-request callers (event subscribers, jobs) too.
pub async fn bind_company_on(
    conn: &mut sqlx::PgConnection,
    company: Uuid,
) -> Result<(), sqlx::Error> {
    bind_company(conn, company).await
}

/// Bind the current task's company onto an already-open transaction/connection.
///
/// For call sites that manage their own transaction (batch operations run all-or-nothing inside one
/// `pool.begin()`): call this immediately after `begin()` so every statement in the transaction is
/// company-scoped. A no-op when no company is in scope (fail-closed at the DB for fenced tables).
pub async fn bind_current_company(conn: &mut sqlx::PgConnection) -> Result<(), sqlx::Error> {
    if let Some(company) = current_company() {
        bind_company(conn, company).await?;
    }
    Ok(())
}

// ─── Scoped execute helpers ────────────────────────────────────────────────────
//
// Each wraps a fully-bound query. When a company is in scope, the query runs inside a transaction
// that first sets `app.company_id`; otherwise it runs directly against the pool (fail-closed at the
// DB for fenced tables). The extra BEGIN/COMMIT per statement is the cost of connection-per-statement
// pooling; a request-scoped held connection could remove it later (ADR-0008 follow-up).

/// `fetch_all` for a typed row query, company-scoped.
pub async fn fetch_all_scoped<'q, T>(
    pool: &PgPool,
    query: QueryAs<'q, Postgres, T, PgArguments>,
) -> Result<Vec<T>, sqlx::Error>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    if let Some(conn) = request_conn() {
        let mut g = conn.lock().await;
        return query.fetch_all(&mut **g).await;
    }
    match current_company() {
        None => query.fetch_all(pool).await,
        Some(company) => {
            let mut tx = pool.begin().await?;
            bind_company(&mut tx, company).await?;
            let rows = query.fetch_all(&mut *tx).await?;
            tx.commit().await?;
            Ok(rows)
        }
    }
}

/// `fetch_one` for a typed row query, company-scoped.
pub async fn fetch_one_scoped<'q, T>(
    pool: &PgPool,
    query: QueryAs<'q, Postgres, T, PgArguments>,
) -> Result<T, sqlx::Error>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    if let Some(conn) = request_conn() {
        let mut g = conn.lock().await;
        return query.fetch_one(&mut **g).await;
    }
    match current_company() {
        None => query.fetch_one(pool).await,
        Some(company) => {
            let mut tx = pool.begin().await?;
            bind_company(&mut tx, company).await?;
            let row = query.fetch_one(&mut *tx).await?;
            tx.commit().await?;
            Ok(row)
        }
    }
}

/// `fetch_optional` for a typed row query, company-scoped.
pub async fn fetch_optional_scoped<'q, T>(
    pool: &PgPool,
    query: QueryAs<'q, Postgres, T, PgArguments>,
) -> Result<Option<T>, sqlx::Error>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    if let Some(conn) = request_conn() {
        let mut g = conn.lock().await;
        return query.fetch_optional(&mut **g).await;
    }
    match current_company() {
        None => query.fetch_optional(pool).await,
        Some(company) => {
            let mut tx = pool.begin().await?;
            bind_company(&mut tx, company).await?;
            let row = query.fetch_optional(&mut *tx).await?;
            tx.commit().await?;
            Ok(row)
        }
    }
}

/// `fetch_one` for a scalar query (e.g. `COUNT(*)`), company-scoped.
pub async fn fetch_one_scalar_scoped<'q, S>(
    pool: &PgPool,
    query: QueryScalar<'q, Postgres, S, PgArguments>,
) -> Result<S, sqlx::Error>
where
    S: Send + Unpin,
    (S,): for<'r> FromRow<'r, PgRow>,
{
    if let Some(conn) = request_conn() {
        let mut g = conn.lock().await;
        return query.fetch_one(&mut **g).await;
    }
    match current_company() {
        None => query.fetch_one(pool).await,
        Some(company) => {
            let mut tx = pool.begin().await?;
            bind_company(&mut tx, company).await?;
            let val = query.fetch_one(&mut *tx).await?;
            tx.commit().await?;
            Ok(val)
        }
    }
}

/// `fetch_optional` for a scalar query (e.g. `SELECT 1 … LIMIT 1`), company-scoped.
pub async fn fetch_optional_scalar_scoped<'q, S>(
    pool: &PgPool,
    query: QueryScalar<'q, Postgres, S, PgArguments>,
) -> Result<Option<S>, sqlx::Error>
where
    S: Send + Unpin,
    (S,): for<'r> FromRow<'r, PgRow>,
{
    if let Some(conn) = request_conn() {
        let mut g = conn.lock().await;
        return query.fetch_optional(&mut **g).await;
    }
    match current_company() {
        None => query.fetch_optional(pool).await,
        Some(company) => {
            let mut tx = pool.begin().await?;
            bind_company(&mut tx, company).await?;
            let val = query.fetch_optional(&mut *tx).await?;
            tx.commit().await?;
            Ok(val)
        }
    }
}

/// `fetch_optional` for an untyped row query (`sqlx::query(..)` → `PgRow`), company-scoped.
///
/// Hand-written services commonly read ad-hoc column sets as raw rows rather than a typed struct;
/// these mirror the typed helpers so such a service can be scoped without restructuring its queries.
pub async fn fetch_optional_row_scoped<'q>(
    pool: &PgPool,
    query: Query<'q, Postgres, PgArguments>,
) -> Result<Option<PgRow>, sqlx::Error> {
    if let Some(conn) = request_conn() {
        let mut g = conn.lock().await;
        return query.fetch_optional(&mut **g).await;
    }
    match current_company() {
        None => query.fetch_optional(pool).await,
        Some(company) => {
            let mut tx = pool.begin().await?;
            bind_company(&mut tx, company).await?;
            let row = query.fetch_optional(&mut *tx).await?;
            tx.commit().await?;
            Ok(row)
        }
    }
}

/// `fetch_one` for an untyped row query (`sqlx::query(..)` → `PgRow`), company-scoped.
pub async fn fetch_one_row_scoped<'q>(
    pool: &PgPool,
    query: Query<'q, Postgres, PgArguments>,
) -> Result<PgRow, sqlx::Error> {
    if let Some(conn) = request_conn() {
        let mut g = conn.lock().await;
        return query.fetch_one(&mut **g).await;
    }
    match current_company() {
        None => query.fetch_one(pool).await,
        Some(company) => {
            let mut tx = pool.begin().await?;
            bind_company(&mut tx, company).await?;
            let row = query.fetch_one(&mut *tx).await?;
            tx.commit().await?;
            Ok(row)
        }
    }
}

/// `fetch_all` for an untyped row query (`sqlx::query(..)` → `Vec<PgRow>`), company-scoped.
pub async fn fetch_all_rows_scoped<'q>(
    pool: &PgPool,
    query: Query<'q, Postgres, PgArguments>,
) -> Result<Vec<PgRow>, sqlx::Error> {
    if let Some(conn) = request_conn() {
        let mut g = conn.lock().await;
        return query.fetch_all(&mut **g).await;
    }
    match current_company() {
        None => query.fetch_all(pool).await,
        Some(company) => {
            let mut tx = pool.begin().await?;
            bind_company(&mut tx, company).await?;
            let rows = query.fetch_all(&mut *tx).await?;
            tx.commit().await?;
            Ok(rows)
        }
    }
}

/// `execute` for a write/DDL query (INSERT/UPDATE/DELETE), company-scoped.
///
/// Writes are scoped too so the RLS `WITH CHECK` clause sees `app.company_id` and accepts the row
/// (and rejects a forged cross-company write). A request whose company is unset cannot write to a
/// fenced table — fail-closed on writes as well.
pub async fn execute_scoped<'q>(
    pool: &PgPool,
    query: Query<'q, Postgres, PgArguments>,
) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    if let Some(conn) = request_conn() {
        let mut g = conn.lock().await;
        return query.execute(&mut **g).await;
    }
    match current_company() {
        None => query.execute(pool).await,
        Some(company) => {
            let mut tx = pool.begin().await?;
            bind_company(&mut tx, company).await?;
            let res = query.execute(&mut *tx).await?;
            tx.commit().await?;
            Ok(res)
        }
    }
}
