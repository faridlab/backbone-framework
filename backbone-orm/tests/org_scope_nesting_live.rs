//! Live proof that a nested org request scope reuses the ambient one instead of opening a second.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN, e.g.
//! `postgresql://postgres:postgres@localhost:5433/postgres`). When it is unset the tests skip, so a
//! checkout without a database still passes.
//!
//! The defect these guard against: every nesting level acquired its own pool connection and re-ran
//! the fence binds, which cost roughly 600 KB of stack per level in a debug build — two levels plus
//! the caller's depth overflowed a default 2 MB worker thread and killed the task mid-delivery. The
//! second connection is the visible half of the same waste, and it is what these tests measure: a
//! one-connection pool cannot serve a nested scope unless the inner call reuses the outer's.

use backbone_orm::org_scope::{fetch_optional_row_scoped, with_org_request_scope, OrgScope};
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

fn admin_dsn() -> Option<String> {
    std::env::var("BACKBONE_ORM_RLS_DSN").ok()
}

/// One connection, and a short acquire timeout so a regression fails fast instead of hanging.
async fn single_connection_pool(dsn: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(3))
        .connect(dsn)
        .await
        .expect("connect")
}

fn scope(unit: Uuid) -> OrgScope {
    OrgScope::for_company_unit(unit)
}

/// Read the bound acting unit ON the request connection, which is the only place the scope's
/// session variables live — a read through the pool would land on a different connection.
async fn acting_unit(pool: &PgPool) -> String {
    let row = fetch_optional_row_scoped(
        pool,
        sqlx::query("SELECT current_setting('app.acting_unit_id', true) AS unit"),
    )
    .await
    .unwrap()
    .expect("one row");
    row.get::<Option<String>, _>("unit").unwrap_or_default()
}

#[tokio::test]
async fn a_nested_scope_of_the_same_shape_reuses_the_ambient_connection() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = single_connection_pool(&dsn).await;
    let unit = Uuid::new_v4();

    // The inner call asks for the scope already in force. With one connection in the pool, it can
    // only succeed by reusing the outer's — acquiring a second would wait for a connection that
    // the outer scope is holding, and time out.
    let outcome = with_org_request_scope(&pool, scope(unit), async {
        with_org_request_scope(&pool, scope(unit), async { "inner ran" }).await
    })
    .await
    .expect("outer scope")
    .expect("the nested scope must reuse the ambient connection, not wait for a second");

    assert_eq!(outcome, "inner ran");
}

#[tokio::test]
async fn three_levels_still_only_need_one_connection() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = single_connection_pool(&dsn).await;
    let unit = Uuid::new_v4();

    let depth = with_org_request_scope(&pool, scope(unit), async {
        with_org_request_scope(&pool, scope(unit), async {
            with_org_request_scope(&pool, scope(unit), async { 3 }).await
        })
        .await
    })
    .await
    .expect("level 1")
    .expect("level 2")
    .expect("level 3");

    assert_eq!(depth, 3);
}

#[tokio::test]
async fn a_nested_scope_of_a_different_shape_binds_its_own() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    // Two connections: the inner scope is a different one, so it legitimately needs its own.
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&dsn)
        .await
        .expect("connect");

    let outer_unit = Uuid::new_v4();
    let inner_unit = Uuid::new_v4();

    let inner_seen = with_org_request_scope(&pool, scope(outer_unit), async {
        with_org_request_scope(&pool, scope(inner_unit), async {
            // Reading through the pool inside the inner scope sees the inner binds.
            acting_unit(&pool).await
        })
        .await
    })
    .await
    .expect("outer")
    .expect("inner");

    assert_eq!(
        inner_seen,
        inner_unit.to_string(),
        "a different scope must bind its own session, never inherit the ambient one"
    );
}

#[tokio::test]
async fn the_ambient_scope_survives_a_nested_call() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = single_connection_pool(&dsn).await;
    let unit = Uuid::new_v4();

    let after = with_org_request_scope(&pool, scope(unit), async {
        with_org_request_scope(&pool, scope(unit), async {}).await.expect("inner");
        // The inner call reused this connection and must not have reset it on the way out.
        acting_unit(&pool).await
    })
    .await
    .expect("outer");

    assert_eq!(after, unit.to_string(), "the reused connection keeps the ambient binds");
}

/// A four-level nest fits in a 1 MB thread.
///
/// The measurement that opened this issue put a two-level nest at about 1.23 MB between the caller
/// frame and the wrapped future's first statement, which is why a 2 MB worker died. Running four
/// levels inside a deliberately small thread is the direct proof that a nesting level no longer
/// carries the scope's own machinery.
#[test]
fn a_deep_nest_fits_in_a_small_stack() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };

    let worker = std::thread::Builder::new()
        .stack_size(1 << 20) // 1 MB — half what a default worker gets.
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            rt.block_on(async {
                let pool = single_connection_pool(&dsn).await;
                let unit = Uuid::new_v4();
                with_org_request_scope(&pool, scope(unit), async {
                    with_org_request_scope(&pool, scope(unit), async {
                        with_org_request_scope(&pool, scope(unit), async {
                            with_org_request_scope(&pool, scope(unit), async { 4 }).await
                        })
                        .await
                    })
                    .await
                })
                .await
                .expect("level 1")
                .expect("level 2")
                .expect("level 3")
                .expect("level 4")
            })
        })
        .expect("spawn");

    assert_eq!(worker.join().expect("the nest must not overflow a 1 MB stack"), 4);
}
