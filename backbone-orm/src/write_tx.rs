//! The write transaction: one database transaction around everything a write does.
//!
//! A repository write commits on its own, statement by statement: a create commits, and only
//! then does the service run its after-hooks and publish its events. A crash between the two
//! loses the effect, and a hook that fails afterwards cannot undo the row. [`in_write_tx`] closes
//! that gap. It opens one transaction, publishes it as a task-local, and commits it only when the
//! whole write succeeds; every repository write path and every scoped statement helper then runs
//! on it instead of committing by itself.
//!
//! Behaviour without a write transaction is exactly what it was: a hand-written caller that
//! never opens one sees no change.
//!
//! # Which connection
//!
//! - When a request scope holds a dedicated connection
//!   ([`with_request_scope`](crate::company_scope::with_request_scope) or the org request scope),
//!   the transaction is begun on that connection. The fence variables it carries (company, org
//!   units, acting unit, audit attribution) were set at session level, so they hold inside the
//!   transaction without being set again, and ID-only lookups stay fenced.
//! - Otherwise a pool connection is taken, the transaction is begun, and the company scope of
//!   the task, the org scope and the audit attribution are bound transaction-locally, as
//!   [`bind_current_company`](crate::company_scope::bind_current_company) does for a bulk write.
//!
//! Row-level security therefore decides inside the transaction exactly what it decided outside.
//! A row it refuses refuses the write and, since Postgres aborts the transaction, the whole of it.
//!
//! # Nesting and savepoints
//!
//! A write transaction opened while one is already active joins it as a savepoint: if the inner
//! work fails, only the inner work is undone and the outer transaction can carry on; if the inner
//! work succeeds, its effects wait for the outer commit. The bulk repository paths use the same
//! mechanism, so a bulk write that fails inside a service's transaction rolls back its own rows
//! and reports the error without poisoning anything the caller already holds.
//!
//! # Cancellation
//!
//! If the future running inside [`in_write_tx`] is dropped (a client disconnect, a timeout layer),
//! the transaction is rolled back by a task spawned from the guard's `Drop`, and the connection
//! is closed instead of returned to the pool if that rollback fails. A transaction never returns
//! to the pool open.
//!
//! # Reading through it
//!
//! While a write transaction is active, [`request_conn`](crate::company_scope) answers with its
//! connection, so every scoped read and write helper of this crate, and every repository method,
//! sees the transaction's own uncommitted rows. Code that opens a connection of its own (a bare
//! `pool.begin()` inside a hook) does not, and cannot see them: that is the one behaviour a hook
//! has to know about.

use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use sqlx::pool::PoolConnection;
use sqlx::postgres::PgConnectOptions;
use sqlx::{Database, PgConnection, PgPool, Postgres, TransactionManager};
use tokio::sync::{Mutex, MutexGuard};

type Tm = <Postgres as Database>::TransactionManager;

tokio::task_local! {
    /// The write transaction of the current task, published by [`in_write_tx`].
    static WRITE_TX: Arc<WriteTx>;
}

/// The write transaction of the current task: a handle to the connection it runs on.
///
/// Cloning the `Arc` is cheap; the connection is behind a mutex, so statements of one task run
/// one at a time, in order. After the transaction ends a clone that is still alive runs
/// statements outside any transaction, unscoped on a pool connection (fail-closed) or under the
/// request scope's own variables.
pub struct WriteTx {
    conn: Arc<Mutex<PoolConnection<Postgres>>>,
    /// The pool the transaction took its own connection from; `None` when it rides the request
    /// connection. A write transaction opened for another pool (a database per tenant) must not
    /// join this one.
    owned_pool: Option<Arc<PgConnectOptions>>,
}

impl WriteTx {
    /// Lock the transaction's connection to run statements on it: `query.execute(&mut *tx.conn().await)`.
    ///
    /// Do not hold the guard across a call that runs repository methods or opens a savepoint:
    /// they lock the same connection and would wait for it.
    pub async fn conn(&self) -> WriteTxConn<'_> {
        WriteTxConn(self.conn.lock().await)
    }

    fn accepts(&self, pool: &PgPool) -> bool {
        match &self.owned_pool {
            None => true,
            Some(own) => Arc::ptr_eq(own, &pool.connect_options()),
        }
    }
}

/// The connection of a write transaction, locked. Dereferences to the Postgres connection.
pub struct WriteTxConn<'a>(MutexGuard<'a, PoolConnection<Postgres>>);

impl Deref for WriteTxConn<'_> {
    type Target = PgConnection;
    fn deref(&self) -> &PgConnection {
        &self.0
    }
}

impl DerefMut for WriteTxConn<'_> {
    fn deref_mut(&mut self) -> &mut PgConnection {
        &mut self.0
    }
}

/// The write transaction of the current task, if [`in_write_tx`] opened one.
pub fn current_write_tx() -> Option<Arc<WriteTx>> {
    WRITE_TX.try_with(|tx| tx.clone()).ok()
}

/// Whether the current task runs inside a write transaction.
pub fn in_write_tx_scope() -> bool {
    WRITE_TX.try_with(|_| ()).is_ok()
}

/// The connection of the active write transaction, for the scoped statement helpers.
pub(crate) fn current_conn() -> Option<Arc<Mutex<PoolConnection<Postgres>>>> {
    WRITE_TX.try_with(|tx| tx.conn.clone()).ok()
}

/// Run `f` inside one transaction: committed when `f` returns `Ok`, rolled back when it returns
/// `Err` or is dropped.
///
/// Every repository write and scoped statement `f` runs, in this crate or in a hand-written
/// service that uses its helpers, rides the transaction (see the [module](self) docs). Opened
/// inside another write transaction for the same pool it is a savepoint of it.
///
/// # Errors
/// `f`'s own error; or a database error from beginning, binding the scope or committing, converted
/// through `E: From<sqlx::Error>`. A failed commit (a deferred constraint, a serialization
/// failure) is returned as an error and nothing of the transaction survives.
pub async fn in_write_tx<F, T, E>(pool: &PgPool, f: F) -> Result<T, E>
where
    F: Future<Output = Result<T, E>>,
    E: From<sqlx::Error>,
{
    if let Some(ambient) = current_write_tx() {
        if ambient.accepts(pool) {
            // A savepoint of the transaction already open.
            let unit = TxUnit::begin_on(ambient.conn.clone()).await?;
            let result = f.await;
            return unit.finish(result).await;
        }
    }

    let (holder, owned_pool) = match crate::company_scope::request_conn_only() {
        // The request's own connection: session-level fence variables already hold on it.
        Some(conn) => (conn, None),
        None => {
            let conn = pool.acquire().await?;
            (Arc::new(Mutex::new(conn)), Some(pool.connect_options()))
        }
    };
    let owned = owned_pool.is_some();
    let unit = TxUnit::begin_on(holder.clone()).await?;
    if owned {
        // A transaction on a connection of its own carries nothing of the request: bind the
        // scope of the task onto it, transaction-locally, so the fence and the audit actor hold.
        let bound = {
            let mut conn = holder.lock().await;
            bind_task_scope(&mut conn).await
        };
        if let Err(e) = bound {
            return unit.finish(Err(e.into())).await;
        }
    }
    let tx = Arc::new(WriteTx {
        conn: holder,
        owned_pool,
    });
    let result = WRITE_TX.scope(tx, f).await;
    unit.finish(result).await
}

/// Why a write transaction run through [`in_write_tx_result`] failed: the work's own error, or
/// the database while beginning, binding the scope or committing.
#[derive(Debug)]
pub enum WriteTxFailure<E> {
    /// The work returned this error; the transaction was rolled back.
    Inner(E),
    /// The database refused to begin, bind the scope or commit.
    Db(sqlx::Error),
}

impl<E> From<sqlx::Error> for WriteTxFailure<E> {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e)
    }
}

/// [`in_write_tx`] for work whose error type cannot absorb a `sqlx::Error`: the two kinds of
/// failure stay apart in [`WriteTxFailure`], so a caller maps each as it must.
pub async fn in_write_tx_result<F, T, E>(pool: &PgPool, f: F) -> Result<T, WriteTxFailure<E>>
where
    F: Future<Output = Result<T, E>>,
{
    in_write_tx(pool, async { f.await.map_err(WriteTxFailure::Inner) }).await
}

/// Bind the company or org scope of the current task, and its audit attribution, onto a
/// connection that is inside a transaction.
async fn bind_task_scope(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    match crate::org_scope::current_org_scope() {
        Some(scope) => crate::org_scope::bind_org_scope_on(conn, &scope).await,
        None => crate::company_scope::bind_current_company(conn).await,
    }
}

/// Run `f` as a savepoint of the active write transaction, so that a failure inside it, which
/// Postgres would otherwise let abort the whole transaction, is undone on its own and can be
/// handled. Outside a write transaction `f` simply runs.
///
/// For a probe that is allowed to fail: a lookup that tries one table spelling and then another,
/// or a count whose failure means "cannot tell".
pub(crate) async fn within_savepoint<T, F>(f: F) -> Result<T, sqlx::Error>
where
    F: Future<Output = Result<T, sqlx::Error>>,
{
    let Some(conn) = current_conn() else {
        return f.await;
    };
    let unit = TxUnit::begin_on(conn).await?;
    let result = f.await;
    unit.finish(result).await
}

/// One transactional unit of a bulk write: begun here, finished with [`TxUnit::finish`].
///
/// Joins the active write transaction as a savepoint; otherwise begins a transaction on the
/// request connection when `use_request_conn` and one is bound (the shape `bulk_create` always
/// had), or on a pool connection with the current company bound (the shape of the other bulk
/// paths). Behaviour without a write transaction is unchanged.
pub(crate) async fn begin_unit(
    pool: &PgPool,
    use_request_conn: bool,
) -> Result<TxUnit, sqlx::Error> {
    if let Some(conn) = current_conn() {
        return TxUnit::begin_on(conn).await;
    }
    if use_request_conn {
        if let Some(conn) = crate::company_scope::request_conn_only() {
            return TxUnit::begin_on(conn).await;
        }
    }
    let conn = pool.acquire().await?;
    let unit = TxUnit::begin_on(Arc::new(Mutex::new(conn))).await?;
    let bound = {
        let mut conn = unit.conn().await;
        crate::company_scope::bind_current_company(&mut conn).await
    };
    match bound {
        Ok(()) => Ok(unit),
        Err(e) => Err(unit.finish::<(), sqlx::Error>(Err(e)).await.unwrap_err()),
    }
}

/// A begun transaction or savepoint on a held connection. Finish it with [`finish`](Self::finish):
/// dropping it unfinished rolls it back from a spawned task, as cancellation requires.
pub(crate) struct TxUnit {
    holder: Arc<Mutex<PoolConnection<Postgres>>>,
    /// The transaction depth before this unit began; the unit's own rollback returns to it.
    base_depth: usize,
    done: bool,
}

impl TxUnit {
    async fn begin_on(holder: Arc<Mutex<PoolConnection<Postgres>>>) -> Result<Self, sqlx::Error> {
        let base_depth = {
            let mut conn = holder.lock().await;
            let base = Tm::get_transaction_depth(&conn);
            Tm::begin(&mut conn, None).await?;
            base
        };
        Ok(Self {
            holder,
            base_depth,
            done: false,
        })
    }

    /// The unit's connection, locked for one statement or one short run of them.
    pub(crate) async fn conn(&self) -> WriteTxConn<'_> {
        WriteTxConn(self.holder.lock().await)
    }

    /// Commit on `Ok`, roll back on `Err`, and return `result` (a failed commit replaces an `Ok`).
    pub(crate) async fn finish<T, E>(mut self, result: Result<T, E>) -> Result<T, E>
    where
        E: From<sqlx::Error>,
    {
        self.done = true;
        let mut conn = self.holder.lock().await;
        match result {
            Ok(value) => {
                // Postgres answers COMMIT on an aborted transaction with a quiet ROLLBACK, so a
                // statement that failed and whose error a hook swallowed would otherwise read as
                // a committed write. Only the outermost unit needs the probe: releasing a
                // savepoint of an aborted transaction is refused on its own.
                if self.base_depth == 0 {
                    if let Err(e) = sqlx::query("SELECT 1").execute(&mut **conn).await {
                        rollback_to(&mut conn, self.base_depth).await;
                        return Err(e.into());
                    }
                }
                match Tm::commit(&mut conn).await {
                    Ok(()) => Ok(value),
                    Err(e) => {
                        // A failed COMMIT has already ended the transaction on the server; bring
                        // the driver's own depth back in line, or close the connection if it
                        // cannot.
                        rollback_to(&mut conn, self.base_depth).await;
                        Err(e.into())
                    }
                }
            }
            Err(e) => {
                rollback_to(&mut conn, self.base_depth).await;
                Err(e)
            }
        }
    }
}

impl Drop for TxUnit {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        let holder = self.holder.clone();
        let base = self.base_depth;
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(async move {
                    let mut conn = holder.lock().await;
                    rollback_to(&mut conn, base).await;
                });
            }
            // No runtime left to run the rollback: never return the connection open.
            Err(_) => {
                if let Ok(mut conn) = holder.try_lock() {
                    conn.close_on_drop();
                }
            }
        }
    }
}

/// Roll back until the connection is at `base` depth; close the connection if that fails.
async fn rollback_to(conn: &mut PoolConnection<Postgres>, base: usize) {
    while Tm::get_transaction_depth(conn) > base {
        if let Err(e) = Tm::rollback(conn).await {
            tracing::error!(
                target: "backbone_orm::write_tx",
                error = %e,
                "a write transaction could not be rolled back; closing its connection instead of \
                 returning it to the pool",
            );
            conn.close_on_drop();
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_write_transaction_outside_a_scope() {
        assert!(current_write_tx().is_none());
        assert!(!in_write_tx_scope());
        assert!(current_conn().is_none());
    }

    /// A transaction on a connection of its own carries the request's actor, so the audit triggers
    /// of every row it writes name who made the change, and the actor dies with the transaction.
    #[tokio::test]
    async fn a_transaction_on_its_own_connection_carries_the_ambient_audit_actor() {
        let Ok(dsn) = std::env::var("BACKBONE_ORM_RLS_DSN") else {
            eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
            return;
        };
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&dsn)
            .await
            .unwrap();
        let audit = crate::audit_context::RequestAuditContext::new("tx-actor");
        let actor_inside: Result<String, sqlx::Error> =
            crate::audit_context::with_request_audit(audit, async {
                in_write_tx(&pool, async {
                    let tx = current_write_tx().expect("a write transaction is active");
                    let mut conn = tx.conn().await;
                    sqlx::query_scalar("SELECT coalesce(current_setting('app.actor', true), '')")
                        .fetch_one(&mut *conn)
                        .await
                })
                .await
            })
            .await;
        assert_eq!(actor_inside.unwrap(), "tx-actor");
        let actor_after: String =
            sqlx::query_scalar("SELECT coalesce(current_setting('app.actor', true), '')")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            actor_after, "",
            "the actor must not ride the pooled connection into the next request"
        );
    }
}
