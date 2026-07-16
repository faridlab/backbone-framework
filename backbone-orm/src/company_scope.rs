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

use sqlx::postgres::{PgArguments, PgRow};
use sqlx::query::{Query, QueryAs, QueryScalar};
use sqlx::{FromRow, PgPool, Postgres};
use std::future::Future;
use uuid::Uuid;

tokio::task_local! {
    /// The company the current request is scoped to. `Platform` callers and unscoped code leave
    /// this unset; `None` inside the scope means an explicit platform (no-fence) caller.
    static COMPANY: Option<Uuid>;
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

/// `execute` for a write/DDL query (INSERT/UPDATE/DELETE), company-scoped.
///
/// Writes are scoped too so the RLS `WITH CHECK` clause sees `app.company_id` and accepts the row
/// (and rejects a forged cross-company write). A request whose company is unset cannot write to a
/// fenced table — fail-closed on writes as well.
pub async fn execute_scoped<'q>(
    pool: &PgPool,
    query: Query<'q, Postgres, PgArguments>,
) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
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
