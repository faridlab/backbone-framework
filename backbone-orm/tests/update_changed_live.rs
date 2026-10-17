//! Live proof that a generic update writes only what the caller changed, and never revives a
//! row deleted since it was read.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN, e.g.
//! `postgresql://postgres:postgres@localhost:5433/postgres`) — the test creates a schema. When it is
//! unset the test skips, so a checkout without a database still passes.
//!
//! The defects these guard against: a PATCH read the row, merged the patch and SET every column
//! from that read. A concurrent write to another column (a verb moving `status`) was overwritten
//! with the stale value, and a PATCH racing a soft delete wrote the whole row back — the update
//! carried no not-deleted predicate — so the row stayed visible and editable after its delete.

use backbone_orm::generic_repository::{GenericCrudRepository, HardDelete, SoftDelete};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

fn admin_dsn() -> Option<String> {
    std::env::var("BACKBONE_ORM_RLS_DSN").ok()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
struct Ticket {
    id: Uuid,
    notes: String,
    status: String,
    metadata: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
struct Tag {
    id: Uuid,
    notes: String,
    status: String,
}

async fn setup(pool: &PgPool) -> String {
    let schema = format!("orm_upd_{}", &Uuid::new_v4().simple().to_string()[..8]);
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema};
         CREATE TABLE {schema}.tickets (
             id       uuid PRIMARY KEY,
             notes    text NOT NULL,
             status   text NOT NULL,
             metadata jsonb NOT NULL DEFAULT '{{}}'::jsonb
         );
         CREATE TABLE {schema}.tags (
             id     uuid PRIMARY KEY,
             notes  text NOT NULL,
             status text NOT NULL
         );"
    ))
    .execute(pool)
    .await
    .unwrap();
    schema
}

async fn teardown(pool: &PgPool, schema: &str) {
    sqlx::raw_sql(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
        .execute(pool)
        .await
        .unwrap();
}

async fn pool(dsn: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(dsn)
        .await
        .expect("connect")
}

#[tokio::test]
async fn a_patch_never_writes_back_a_column_it_did_not_change() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = pool(&dsn).await;
    let schema = setup(&pool).await;
    let repo: GenericCrudRepository<Ticket, SoftDelete> =
        GenericCrudRepository::new(pool.clone(), &format!("{schema}.tickets"));

    let id = Uuid::new_v4();
    let stored = repo
        .create(&Ticket {
            id,
            notes: "first".into(),
            status: "draft".into(),
            metadata: serde_json::json!({}),
        })
        .await
        .unwrap();

    // A PATCH reads the row...
    let before = repo.find_by_id(&id.to_string()).await.unwrap().unwrap();
    assert_eq!(before, stored);
    // ...a verb moves the status meanwhile...
    sqlx::query(&format!(
        "UPDATE {schema}.tickets SET status = 'posted' WHERE id = $1"
    ))
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    // ...and the PATCH saves its notes edit over the stale read.
    let mut after = before.clone();
    after.notes = "edited".into();
    let saved = repo
        .update_changed(&id.to_string(), &before, &after)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(saved.notes, "edited");
    assert_eq!(
        saved.status, "posted",
        "the PATCH wrote back the status it read, undoing the verb"
    );
    teardown(&pool, &schema).await;
}

#[tokio::test]
async fn a_patch_racing_a_delete_does_not_revive_the_row() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = pool(&dsn).await;
    let schema = setup(&pool).await;
    let repo: GenericCrudRepository<Ticket, SoftDelete> =
        GenericCrudRepository::new(pool.clone(), &format!("{schema}.tickets"));

    let id = Uuid::new_v4();
    repo.create(&Ticket {
        id,
        notes: "first".into(),
        status: "draft".into(),
        metadata: serde_json::json!({}),
    })
    .await
    .unwrap();
    let before = repo.find_by_id(&id.to_string()).await.unwrap().unwrap();
    assert!(repo.soft_delete(&id.to_string()).await.unwrap());

    // The changed-columns path and the full update both refuse a row deleted since the read.
    let mut after = before.clone();
    after.notes = "edited".into();
    assert!(repo
        .update_changed(&id.to_string(), &before, &after)
        .await
        .unwrap()
        .is_none());
    assert!(repo
        .update(&id.to_string(), &before)
        .await
        .unwrap()
        .is_none());
    // A PATCH whose stale read carries metadata without `deleted_at` must not wipe the delete.
    let mut stale_meta = before.clone();
    stale_meta.metadata = serde_json::json!({ "note": "stale" });
    assert!(repo
        .update_changed(&id.to_string(), &before, &stale_meta)
        .await
        .unwrap()
        .is_none());

    assert!(
        repo.find_by_id(&id.to_string()).await.unwrap().is_none(),
        "the delete was undone"
    );
    let trashed = repo
        .find_deleted_by_id(&id.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(trashed.notes, "first");
    teardown(&pool, &schema).await;
}

#[tokio::test]
async fn a_hard_delete_entity_patches_only_its_changes_and_reports_a_gone_row() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = pool(&dsn).await;
    let schema = setup(&pool).await;
    let repo: GenericCrudRepository<Tag, HardDelete> =
        GenericCrudRepository::new(pool.clone(), &format!("{schema}.tags"));

    let id = Uuid::new_v4();
    repo.create(&Tag {
        id,
        notes: "first".into(),
        status: "draft".into(),
    })
    .await
    .unwrap();
    let before = repo.find_by_id(&id.to_string()).await.unwrap().unwrap();
    sqlx::query(&format!(
        "UPDATE {schema}.tags SET status = 'posted' WHERE id = $1"
    ))
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let mut after = before.clone();
    after.notes = "edited".into();
    let saved = repo
        .update_changed(&id.to_string(), &before, &after)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (saved.notes.as_str(), saved.status.as_str()),
        ("edited", "posted")
    );

    // Nothing changed: the current row comes back untouched.
    let same = repo
        .update_changed(&id.to_string(), &saved, &saved)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(same, saved);

    assert!(repo.delete(&id.to_string()).await.unwrap());
    assert!(repo
        .update_changed(&id.to_string(), &before, &after)
        .await
        .unwrap()
        .is_none());
    teardown(&pool, &schema).await;
}
