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

    // Unconditional reset. We do NOT gate on `Arc::try_unwrap(holder)` (sole-reference check):
    // `REQUEST_CONN` is a clonable `Arc`, and every scoped helper takes a clone via `request_conn()`.
    // A clone that outlives the scope (a `tokio::spawn` capturing it, a value held across an `.await`
    // that outlives the request future, or cancellation with a lingering task) would make `try_unwrap`
    // return `Err`, skipping this block entirely and leaving `app.company_id` set at SESSION level —
    // the connection then returns to the pool dirty, and the next acquire reads the PREVIOUS tenant's
    // rows. That is a non-deterministic cross-tenant leak, not fail-closed. (Regression test:
    // `lingering_request_conn_clone_does_not_dirty_the_pooled_connection`.)
    //
    // Locking the mutex here serializes behind any in-flight clone query, then clears the session var
    // regardless of how many clones exist or when they drop. The `PoolConnection` only returns to the
    // pool when the LAST `Arc` reference drops — by which point the var is already cleared here. A
    // clone that runs further queries after this reset does so unscoped (fail-closed), which is the
    // correct behaviour for work that outlived its request scope.
    {
        let mut guard = holder.lock().await;
        if let Err(e) = sqlx::query("SELECT set_config('app.company_id', '', false)")
            .execute(&mut **guard)
            .await
        {
            // A reset failure (transient DB error) could leave the session var set. We cannot `detach`
            // the connection from a `&mut` borrow, so log loud at ERROR — this must surface in ops as
            // a fence-hygiene alert, not be swallowed silently. (Previously `let _ =` hid this.)
            tracing::error!(
                target: "backbone_orm::company_scope",
                error = %e,
                "failed to reset app.company_id on request connection; the pool connection may carry \
                 the previous tenant's company_id — treat as a fence-hygiene incident",
            );
        }
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

#[cfg(test)]
mod tests {
    //! Regression: the request-scope connection's `app.company_id` MUST be reset even when a clone
    //! of the task-local `REQUEST_CONN` Arc outlives the scope (a `tokio::spawn`, a value held across
    //! an `.await` that outlives the request future, cancellation). Before the fix, the reset was
    //! gated on `Arc::try_unwrap` succeeding; a lingering clone made it return `Err`, the reset was
    //! skipped SILENTLY, and the pooled connection returned dirty — leaking the previous tenant's
    //! `company_id` to the next acquire (a non-deterministic cross-tenant leak).
    //!
    //! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN). Skips when unset.
    use super::{request_conn, with_request_scope};
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use uuid::Uuid;

    fn dsn() -> Option<String> {
        std::env::var("BACKBONE_ORM_RLS_DSN").ok()
    }

    async fn admin_pool(dsn: &str) -> PgPool {
        PgPoolOptions::new().max_connections(4).connect(dsn).await.unwrap()
    }

    /// A single-connection pool as the non-super `rls_reset_app` role. max_connections=1 forces a
    /// re-acquire after the leaked clone drops to land on the SAME connection the scope dirtied.
    async fn app_pool(dsn: &str) -> PgPool {
        let after_at = dsn.rsplit('@').next().unwrap();
        let url = format!("postgresql://rls_reset_app:rlspw@{after_at}");
        PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap()
    }

    async fn setup(admin: &PgPool) {
        sqlx::raw_sql(
            "DROP SCHEMA IF EXISTS rls_reset_test CASCADE; \
             DROP ROLE IF EXISTS rls_reset_app; \
             CREATE SCHEMA rls_reset_test; \
             CREATE ROLE rls_reset_app LOGIN PASSWORD 'rlspw'; \
             GRANT USAGE ON SCHEMA rls_reset_test TO rls_reset_app; \
             CREATE TABLE rls_reset_test.t (id uuid PRIMARY KEY, company_id uuid NOT NULL); \
             GRANT SELECT, INSERT, UPDATE, DELETE ON rls_reset_test.t TO rls_reset_app;",
        )
        .execute(admin).await.unwrap();
    }

    /// Hold a clone of REQUEST_CONN past the scope, then verify the pooled connection comes back clean.
    #[tokio::test]
    async fn lingering_request_conn_clone_does_not_dirty_the_pooled_connection() {
        let Some(dsn) = dsn() else { eprintln!("skipping: set BACKBONE_ORM_RLS_DSN"); return; };
        let admin = admin_pool(&dsn).await;
        setup(&admin).await;
        let pool = app_pool(&dsn).await;
        let company_a = Uuid::new_v4();

        // Smuggle a clone of REQUEST_CONN OUT of the scope via a channel, so it outlives `f`.
        let (tx, rx) = tokio::sync::oneshot::channel();

        // Drive a request scope for company A; inside it, capture a clone of the request connection
        // (the exact thing a `tokio::spawn` or a held-across-await would do) and hand it out.
        with_request_scope(&pool, company_a, async {
            if let Some(conn) = request_conn() {
                let _ = tx.send(conn);
            }
        })
        .await
        .unwrap();

        // The clone now outlives the scope. With the OLD (try_unwrap-gated) code the reset was
        // skipped here and the connection would return to the pool dirty. Hold then drop the clone so
        // the single pooled connection is returned, then re-acquire that SAME connection.
        let held = rx.await.unwrap();
        drop(held);

        let mut conn = pool.acquire().await.unwrap();
        let setting: String =
            sqlx::query_scalar("SELECT current_setting('app.company_id', true)")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        // Before the fix this was `company_a.to_string()` (cross-tenant LEAK). It must be empty now.
        assert_eq!(
            setting, "",
            "app.company_id leaked onto the pooled connection after scope exit — a lingering \
             REQUEST_CONN clone must not bypass the session-var reset (cross-tenant leak)"
        );
    }
}
