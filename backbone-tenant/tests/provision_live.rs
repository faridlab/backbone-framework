//! Live provisioning test — exercises `TenantProvisioner` against a real Postgres.
//!
//! Gated on `BACKBONE_TENANT_ADMIN_DSN` (a DSN to a maintenance database the role may
//! `CREATE DATABASE` from, e.g. `postgresql://postgres:postgres@localhost:5433/postgres`). When it is
//! unset the test skips, so a checkout without a database still passes `cargo test`.
//!
//! It creates a throwaway tenant, proves the database and a migrated table exist, proves provisioning
//! is idempotent, then drops the tenant and proves it is gone — leaving no residue.

#![cfg(feature = "provision")]

use backbone_tenant::provision::TenantProvisioner;
use backbone_tenant::TenantId;
use sqlx::{Connection, PgConnection};

fn admin_dsn() -> Option<String> {
    std::env::var("BACKBONE_TENANT_ADMIN_DSN").ok()
}

#[tokio::test]
async fn provisions_migrates_and_deprovisions_a_real_tenant() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: set BACKBONE_TENANT_ADMIN_DSN to run the live provisioning test");
        return;
    };
    let p = TenantProvisioner::new(dsn.clone());
    // A distinctive, self-identifying tenant so a failed run is easy to spot and clean up by hand.
    let tenant = TenantId::from("provtest_bt");

    // Start from a clean slate in case a previous run aborted mid-way.
    let _ = p.deprovision(&tenant).await;

    // 1. Provision: create + migrate one table.
    let db = p
        .provision(
            &tenant,
            &["CREATE TABLE marker (id int primary key, note text)",
              "INSERT INTO marker (id, note) VALUES (1, 'provisioned')"],
        )
        .await
        .expect("provision should succeed");
    assert_eq!(db, "tenant_provtest_bt");
    assert!(p.exists(&tenant).await.unwrap(), "the tenant database must exist after provisioning");

    // 2. The migration actually ran in the tenant's OWN database — connect and read the row.
    let tenant_dsn = p.tenant_dsn(&tenant).unwrap();
    {
        let mut conn = PgConnection::connect(&tenant_dsn).await.expect("connect to tenant db");
        let (note,): (String,) = sqlx::query_as("SELECT note FROM marker WHERE id = 1")
            .fetch_one(&mut conn)
            .await
            .expect("the migrated table + row must be present in the tenant database");
        assert_eq!(note, "provisioned");
    }

    // 3. Idempotent: creating again is a no-op, not an error.
    p.create_database(&tenant).await.expect("create_database must be idempotent");

    // 4. Deprovision: the database is dropped and gone.
    let dropped = p.deprovision(&tenant).await.expect("deprovision should succeed");
    assert!(dropped, "deprovision should report a database was removed");
    assert!(!p.exists(&tenant).await.unwrap(), "the tenant database must be gone after deprovisioning");
}

#[tokio::test]
async fn an_unsafe_tenant_id_never_touches_the_database() {
    // The safety guard must fire before any connection is opened — no DSN required.
    let p = TenantProvisioner::new("postgresql://u:p@localhost:5432/postgres");
    let bad = TenantId::from("x\"; DROP DATABASE postgres; --");
    assert!(p.database_name(&bad).is_err());
    // `exists` also refuses the id rather than querying with it.
    assert!(p.exists(&bad).await.is_err());
}
