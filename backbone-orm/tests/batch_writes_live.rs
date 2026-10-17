//! Live proof that batch writes are all-or-nothing and that the trash list searches.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN) — the test creates a schema. When it is
//! unset the test skips, so a checkout without a database still passes.
//!
//! The defects these guard against: `bulk_create` opened a transaction and then inserted every
//! row through the pool, outside it, so a refused row left the rows before it committed; a bulk
//! update wrote every column of the row it had read, undoing a concurrent verb; and the trash
//! list dropped the search term.

use backbone_orm::generic_repository::{
    BatchItemError, EntityRepoMeta, GenericCrudRepository, SoftDelete,
};
use backbone_orm::repository::PaginationParams;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashMap;
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

impl EntityRepoMeta for Ticket {
    fn column_types() -> HashMap<String, String> {
        HashMap::from([("id".to_string(), "uuid".to_string())])
    }
    fn search_fields() -> &'static [&'static str] {
        &["notes"]
    }
}

fn ticket(notes: &str) -> Ticket {
    Ticket {
        id: Uuid::new_v4(),
        notes: notes.into(),
        status: "draft".into(),
        metadata: serde_json::json!({}),
    }
}

async fn setup(pool: &PgPool) -> String {
    let schema = format!("orm_batch_{}", &Uuid::new_v4().simple().to_string()[..8]);
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema};
         CREATE TABLE {schema}.tickets (
             id       uuid PRIMARY KEY,
             notes    text NOT NULL CONSTRAINT ck_tickets__notes_refused CHECK (notes <> 'refused'),
             status   text NOT NULL,
             metadata jsonb NOT NULL DEFAULT '{{}}'::jsonb
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
        .max_connections(3)
        .connect(dsn)
        .await
        .expect("connect")
}

async fn row_count(pool: &PgPool, schema: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {schema}.tickets"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_refused_row_rolls_the_whole_bulk_create_back_and_is_named() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = pool(&dsn).await;
    let schema = setup(&pool).await;
    let repo: GenericCrudRepository<Ticket, SoftDelete> =
        GenericCrudRepository::new(pool.clone(), &format!("{schema}.tickets"));

    let err = repo
        .bulk_create(&[ticket("one"), ticket("refused"), ticket("three")])
        .await
        .unwrap_err();
    let item = err
        .downcast_ref::<BatchItemError>()
        .expect("the error names the failed item");
    assert_eq!(item.index, 1);
    assert!(
        item.error.downcast_ref::<sqlx::Error>().is_some(),
        "the database error stays readable: {}",
        item.error
    );
    assert_eq!(
        row_count(&pool, &schema).await,
        0,
        "the row before the refused one was committed"
    );

    let rows = repo
        .bulk_create(&[ticket("one"), ticket("two")])
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(row_count(&pool, &schema).await, 2);
    teardown(&pool, &schema).await;
}

#[tokio::test]
async fn a_bulk_update_writes_only_the_changed_columns_and_all_or_nothing() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = pool(&dsn).await;
    let schema = setup(&pool).await;
    let repo: GenericCrudRepository<Ticket, SoftDelete> =
        GenericCrudRepository::new(pool.clone(), &format!("{schema}.tickets"));
    let a = repo.create(&ticket("a")).await.unwrap();
    let b = repo.create(&ticket("b")).await.unwrap();

    // A verb moves `a`'s status after the bulk edit read it.
    sqlx::query(&format!(
        "UPDATE {schema}.tickets SET status = 'posted' WHERE id = $1"
    ))
    .bind(a.id)
    .execute(&pool)
    .await
    .unwrap();
    let edit = |t: &Ticket, notes: &str| {
        let mut after = t.clone();
        after.notes = notes.into();
        (t.clone(), after)
    };
    let saved = repo
        .bulk_update_changed(&[edit(&a, "a2"), edit(&b, "b2")])
        .await
        .unwrap();
    assert_eq!(saved[0].notes, "a2");
    assert_eq!(
        saved[0].status, "posted",
        "the bulk edit wrote back the status it read"
    );

    // A row deleted since the read fails the batch, and the row before it is not written.
    repo.soft_delete(&b.id.to_string()).await.unwrap();
    let err = repo
        .bulk_update_changed(&[edit(&saved[0], "a3"), edit(&saved[1], "b3")])
        .await
        .unwrap_err();
    assert_eq!(
        err.downcast_ref::<BatchItemError>().map(|e| e.index),
        Some(1)
    );
    let a_now = repo.find_by_id(&a.id.to_string()).await.unwrap().unwrap();
    assert_eq!(a_now.notes, "a2", "the batch was not rolled back");
    teardown(&pool, &schema).await;
}

#[tokio::test]
async fn the_trash_list_searches_and_filters() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = pool(&dsn).await;
    let schema = setup(&pool).await;
    let repo: GenericCrudRepository<Ticket, SoftDelete> =
        GenericCrudRepository::new(pool.clone(), &format!("{schema}.tickets"));
    for notes in ["alpha", "beta", "gamma"] {
        let t = repo.create(&ticket(notes)).await.unwrap();
        repo.soft_delete(&t.id.to_string()).await.unwrap();
    }
    let page = PaginationParams {
        page: 1,
        per_page: 20,
    };

    let search = HashMap::from([("search".to_string(), "bet".to_string())]);
    let found = repo
        .list_deleted_filtered(page.clone(), Some(&search))
        .await
        .unwrap();
    assert_eq!(
        found.data.len(),
        1,
        "the trash list ignored the search term"
    );
    assert_eq!(found.data[0].notes, "beta");

    let filter = HashMap::from([("notes".to_string(), "gamma".to_string())]);
    let found = repo
        .list_deleted_filtered(page, Some(&filter))
        .await
        .unwrap();
    assert_eq!(found.pagination.total, 1);
    teardown(&pool, &schema).await;
}
