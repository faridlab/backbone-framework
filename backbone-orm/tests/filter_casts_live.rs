//! Live proof that a generic list filter compares a typed column as its own type, whether or not
//! the entity's generated `column_types()` carries a cast hint for it.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN, e.g.
//! `postgresql://postgres:postgres@localhost:5433/postgres`) — the test creates a schema. When it is
//! unset the test skips, so a checkout without a database still passes.
//!
//! The defect these guard against: filter values arrive as text and are bound as text, and
//! PostgreSQL has no implicit comparison between text and boolean, integer, numeric, uuid, date,
//! time or timestamptz. The cast used to come only from the generated hint map, which never covered
//! booleans or numbers, covered only uuids named `id` / `*_id`, and covered temporal columns only in
//! modules generated after the generator learned them — so `folded[eq]=false`,
//! `scheduled_at[gte]=2026-10-03` or `requested_by[eq]=<uuid>` answered
//! "operator does not exist: boolean = text" on any entity the hints missed.

use backbone_orm::repository::{PaginationParams, PostgresRepository};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

fn admin_dsn() -> Option<String> {
    std::env::var("BACKBONE_ORM_RLS_DSN").ok()
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
struct Stage {
    id: Uuid,
    name: String,
}

const REQUESTER: &str = "6f1c2a8e-0b7d-4e44-9a51-3c2d1e0f9a77";

async fn setup(admin: &PgPool, schema: &str) {
    sqlx::raw_sql(&format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE;
         CREATE SCHEMA {schema};
         CREATE TYPE {schema}.stage_kind AS ENUM ('open_call', 'closed');
         CREATE TABLE {schema}.stage (
             id            uuid PRIMARY KEY,
             name          text NOT NULL,
             folded        boolean NOT NULL,
             sequence      integer NOT NULL,
             big_count     bigint NOT NULL,
             salary        numeric(14,2) NOT NULL,
             requested_by  uuid NOT NULL,
             deadline      date NOT NULL,
             starts_at     time NOT NULL,
             scheduled_at  timestamptz NOT NULL,
             kind          {schema}.stage_kind NOT NULL,
             tags          text[] NOT NULL DEFAULT '{{}}'
         );
         INSERT INTO {schema}.stage VALUES
           ('00000000-0000-0000-0000-000000000001', 'Screening', false, 10, 100, 1000.50,
            '{REQUESTER}', '2026-09-19', '08:00', '2026-10-01 09:00+00', 'open_call', '{{a}}'),
           ('00000000-0000-0000-0000-000000000002', 'Offer', true, 20, 200, 2500.00,
            '{REQUESTER}', '2026-10-03', '13:30', '2026-10-05 14:00+00', 'closed', '{{b}}'),
           ('00000000-0000-0000-0000-000000000003', 'Hired', true, 30, 300, 4000.00,
            '00000000-0000-0000-0000-0000000000ff', '2026-10-10', '17:00', '2026-10-09 10:00+00',
            'closed', '{{c}}');"
    ))
    .execute(admin)
    .await
    .unwrap();
}

/// Run one filtered list with NO generated hints and return the matching names, sorted.
async fn names(repo: &PostgresRepository<Stage>, filters: &[(&str, &str)]) -> Vec<String> {
    let filters: HashMap<String, String> =
        filters.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let page = repo
        .list_with_filters(PaginationParams::new(1, 50), &filters, &HashMap::new(), &[])
        .await
        .unwrap_or_else(|e| panic!("filter {filters:?} was refused: {e:#}"));
    let mut out: Vec<String> = page.data.into_iter().map(|s| s.name).collect();
    out.sort();
    out
}

#[tokio::test]
async fn typed_columns_compare_as_their_own_type_without_a_generated_hint() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = PgPoolOptions::new().max_connections(2).connect(&dsn).await.expect("connect");
    let schema = format!("orm_cast_{}", &Uuid::new_v4().simple().to_string()[..8]);
    setup(&pool, &schema).await;
    let repo: PostgresRepository<Stage> =
        PostgresRepository::new(pool.clone(), &format!("{schema}.stage"));

    // boolean
    assert_eq!(names(&repo, &[("folded[eq]", "false")]).await, ["Screening"]);
    assert_eq!(names(&repo, &[("folded", "true")]).await, ["Hired", "Offer"]);
    // integer / bigint / numeric
    assert_eq!(names(&repo, &[("sequence[gte]", "20")]).await, ["Hired", "Offer"]);
    assert_eq!(names(&repo, &[("big_count[lt]", "200")]).await, ["Screening"]);
    assert_eq!(names(&repo, &[("salary[gt]", "2000")]).await, ["Hired", "Offer"]);
    // A numeric compared as text would put "1000.50" above "2500.00" lexically only by luck;
    // "900" > "4000.00" as text, so this one only passes as a number.
    assert_eq!(names(&repo, &[("salary[lt]", "900")]).await, Vec::<String>::new());
    // uuid not named `*_id` — the column the generator never hinted
    assert_eq!(names(&repo, &[("requested_by[eq]", REQUESTER)]).await, ["Offer", "Screening"]);
    // date / time / timestamptz
    assert_eq!(names(&repo, &[("deadline[eq]", "2026-10-03")]).await, ["Offer"]);
    assert_eq!(names(&repo, &[("starts_at[gte]", "13:00")]).await, ["Hired", "Offer"]);
    assert_eq!(names(&repo, &[("scheduled_at[gte]", "2026-10-03")]).await, ["Hired", "Offer"]);
    assert_eq!(names(&repo, &[("scheduled_at[lt]", "2026-10-02")]).await, ["Screening"]);
    // an enum the hints never named: compared in the enum, value normalized like a hinted one
    assert_eq!(names(&repo, &[("kind[eq]", "OpenCall")]).await, ["Screening"]);
    // in / notin / between / or carry the same cast
    assert_eq!(names(&repo, &[("sequence[in]", "10,30")]).await, ["Hired", "Screening"]);
    assert_eq!(names(&repo, &[("deadline[notin]", "2026-10-03")]).await, ["Hired", "Screening"]);
    assert_eq!(
        names(&repo, &[("scheduled_at[between]", "2026-10-02,2026-10-06")]).await,
        ["Offer"]
    );
    assert_eq!(names(&repo, &[("folded[eq]", "false"), ("sequence[or]", "30")]).await, ["Hired", "Screening"]);
    // combined, as the approvals "My requests" screen asks it
    assert_eq!(
        names(&repo, &[("requested_by[eq]", REQUESTER), ("folded[eq]", "true")]).await,
        ["Offer"]
    );
    // text columns keep comparing as text, and pattern operators keep working
    assert_eq!(names(&repo, &[("name[eq]", "Offer")]).await, ["Offer"]);
    assert_eq!(names(&repo, &[("name[contain]", "ffe")]).await, ["Offer"]);

    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE")).execute(&pool).await.unwrap();
}

#[tokio::test]
async fn a_generated_hint_still_decides_the_cast_for_its_column() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = PgPoolOptions::new().max_connections(2).connect(&dsn).await.expect("connect");
    let schema = format!("orm_cast_{}", &Uuid::new_v4().simple().to_string()[..8]);
    setup(&pool, &schema).await;
    let repo: PostgresRepository<Stage> =
        PostgresRepository::new(pool.clone(), &format!("{schema}.stage"));

    let mut hints = HashMap::new();
    hints.insert("id".to_string(), "uuid".to_string());
    hints.insert("kind".to_string(), format!("{schema}.stage_kind"));
    let mut filters = HashMap::new();
    filters.insert("id[in]".to_string(), "00000000-0000-0000-0000-000000000001,00000000-0000-0000-0000-000000000003".to_string());
    filters.insert("kind[eq]".to_string(), "Closed".to_string());
    let page = repo
        .list_with_filters(PaginationParams::new(1, 50), &filters, &hints, &[])
        .await
        .expect("hinted filters keep working");
    let got: Vec<String> = page.data.into_iter().map(|s| s.name).collect();
    assert_eq!(got, ["Hired"]);

    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE")).execute(&pool).await.unwrap();
}
