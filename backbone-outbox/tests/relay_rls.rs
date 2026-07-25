//! Proves the `multi_tenant` outbox fence and the relay bypass work TOGETHER (ADR-0011) — the
//! combination that has never previously been exercised end-to-end.
//!
//! Run with the feature on, against a DISPOSABLE test database where resetting these two roles
//! is acceptable (the test sets their passwords):
//!
//!     DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres \
//!     cargo test --features multi_tenant --test relay_rls -- --nocapture
//!
//! What it proves:
//!   - the APP role (NOBYPASSRLS) with no `app.company_id` sees ZERO rows (the fence works);
//!   - the APP role scoped to company A sees A's rows, not B's (tenant isolation);
//!   - the RELAY role (`metaphor_relay`) with NO scope drains ALL rows (the `current_user` bypass),
//!     including the exact row the fence hid from the unscoped app — so event delivery does not stall.

#![cfg(feature = "multi_tenant")]

use std::sync::{Arc, Mutex};

use backbone_outbox::{outbox, record::OutboxRecord, relay};
use sqlx::postgres::PgConnectOptions;
use sqlx::{PgPool, Row};
use std::str::FromStr;
use uuid::Uuid;

const SCHEMA: &str = "rls_test";
const APP_ROLE: &str = "metaphor_app";
const RELAY_ROLE: &str = "metaphor_relay";
const APP_PW: &str = "app-test-pw";
const RELAY_PW: &str = "relay-test-pw";

fn admin_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".into())
}

async fn admin_pool() -> PgPool {
    PgPool::connect(&admin_url()).await.expect("connect admin (needs superuser/role-create)")
}

/// Bind `app.company_id` on a connection-scoped/tx-local basis.
async fn scope(conn: &mut sqlx::PgConnection, company: Uuid) {
    sqlx::query("SELECT set_config('app.company_id', $1, true)")
        .bind(company.to_string())
        .execute(conn)
        .await
        .unwrap();
}

async fn count(conn: &mut sqlx::PgConnection) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {SCHEMA}.outbox_events"))
        .fetch_one(conn)
        .await
        .unwrap()
}

#[tokio::test]
async fn fence_hides_rows_from_unscoped_app_but_relay_drains_all() {
    let admin = admin_pool().await;

    // 1. Clean slate + create/reset the two NOBYPASSRLS roles (passwords are set — disposable DB only).
    sqlx::query(&format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"))
        .execute(&admin).await.unwrap();
    let _ = sqlx::query(&format!(
        "CREATE ROLE {APP_ROLE} LOGIN PASSWORD '{APP_PW}' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE"
    )).execute(&admin).await; // may already exist
    sqlx::query(&format!("ALTER ROLE {APP_ROLE} LOGIN PASSWORD '{APP_PW}' NOSUPERUSER NOBYPASSRLS"))
        .execute(&admin).await.unwrap();
    let _ = sqlx::query(&format!(
        "CREATE ROLE {RELAY_ROLE} LOGIN PASSWORD '{RELAY_PW}' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE"
    )).execute(&admin).await;
    sqlx::query(&format!("ALTER ROLE {RELAY_ROLE} LOGIN PASSWORD '{RELAY_PW}' NOSUPERUSER NOBYPASSRLS"))
        .execute(&admin).await.unwrap();

    // 2. migrate() with the feature ON → fenced outbox_events + the relay-bypass policy.
    outbox::migrate(&admin, SCHEMA).await.unwrap();

    // 3. Grants: app = USAGE + full DML on outbox_events; relay = USAGE + SELECT/UPDATE only.
    sqlx::query(&format!("GRANT CONNECT ON DATABASE {} TO {APP_ROLE}, {RELAY_ROLE}", current_db(&admin).await))
        .execute(&admin).await.unwrap();
    sqlx::query(&format!("GRANT USAGE ON SCHEMA {SCHEMA} TO {APP_ROLE}, {RELAY_ROLE}"))
        .execute(&admin).await.unwrap();
    sqlx::query(&format!("GRANT SELECT, INSERT, UPDATE, DELETE ON {SCHEMA}.outbox_events TO {APP_ROLE}"))
        .execute(&admin).await.unwrap();
    sqlx::query(&format!("GRANT SELECT, UPDATE ON {SCHEMA}.outbox_events TO {RELAY_ROLE}"))
        .execute(&admin).await.unwrap();

    // 4. Connect as each role (real login — not SET ROLE — so RLS behaves exactly as in production).
    let base = PgConnectOptions::from_str(&admin_url()).unwrap();
    let app = PgPool::connect_with(base.clone().username(APP_ROLE).password(APP_PW)).await.unwrap();
    let relay = PgPool::connect_with(base.clone().username(RELAY_ROLE).password(RELAY_PW)).await.unwrap();

    let company_a = Uuid::new_v4();
    let company_b = Uuid::new_v4();

    // 5. Stage one event for company A as the APP role, scoped to A.
    {
        let mut tx = app.begin().await.unwrap();
        scope(&mut tx, company_a).await;
        let rec = OutboxRecord::new("TestEvent", "Thing", "1", company_a, serde_json::json!({"k":"v"}), chrono::Utc::now());
        outbox::stage(&mut *tx, SCHEMA, &rec).await.unwrap();
        tx.commit().await.unwrap();
    }

    // 6. APP role with NO scope sees ZERO rows (the fence).
    let mut c = app.acquire().await.unwrap();
    let n_unscoped = count(&mut c).await;
    drop(c);
    assert_eq!(n_unscoped, 0, "unscoped app role must see zero rows under the fence");

    // 7. APP role scoped to A sees 1; scoped to B sees 0 (tenant isolation).
    {
        let mut tx = app.begin().await.unwrap();
        scope(&mut tx, company_a).await;
        assert_eq!(count(&mut tx).await, 1, "scoped-to-A sees its row");
    }
    {
        let mut tx = app.begin().await.unwrap();
        scope(&mut tx, company_b).await;
        assert_eq!(count(&mut tx).await, 0, "scoped-to-B sees nothing");
    }

    // 8. RELAY drains with NO scope — the `current_user = 'metaphor_relay'` bypass. It delivers the
    //    exact row the fence hid from the unscoped app (step 6) — proving delivery does not stall.
    let delivered: Arc<Mutex<Vec<OutboxRecord>>> = Arc::new(Mutex::new(vec![]));
    let sink = delivered.clone();
    let n = relay::drain_once(&relay, SCHEMA, 10, move |rec: OutboxRecord| {
        let sink = sink.clone();
        async move { sink.lock().unwrap().push(rec); Ok(()) }
    }).await.unwrap();
    assert_eq!(n, 1, "relay must drain the fenced row via the current_user bypass");
    {
        let got = delivered.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].company_id, company_a);
        assert_eq!(got[0].event_type, "TestEvent");
    }

    // 9. A second drain finds nothing (the row is now published).
    let n2 = relay::drain_once(&relay, SCHEMA, 10, |_| async { Ok(()) }).await.unwrap();
    assert_eq!(n2, 0);

    sqlx::query(&format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE")).execute(&admin).await.unwrap();
}

async fn current_db(pool: &PgPool) -> String {
    let row = sqlx::query("SELECT current_database() AS db").fetch_one(pool).await.unwrap();
    row.get::<String, _>("db")
}
