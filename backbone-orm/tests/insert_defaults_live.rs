//! Live proof that a generic `create` lets a column DEFAULT fire.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN, e.g.
//! `postgresql://postgres:postgres@localhost:5433/postgres`) — the test creates a schema. When it is
//! unset the test skips, so a checkout without a database still passes.
//!
//! The defect these guard against: the insert named no columns and selected every column of the row
//! type, so a column the entity does not know about arrived as an explicit NULL. An explicit NULL is
//! not an absent value — it overrides the DEFAULT. Composition-installed tenancy (ADR-0029) defaults
//! `org_unit_id` from the acting unit, so generic creates over a scoped table wrote NULL and the
//! write-path guard refused them.

use backbone_orm::repository::{DatabaseOperations, PostgresRepository};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

fn admin_dsn() -> Option<String> {
    std::env::var("BACKBONE_ORM_RLS_DSN").ok()
}

/// What the module's entity knows about: an id and a name. Everything else on the table was added
/// by a decorator the module never sees.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct Widget {
    id: Uuid,
    name: String,
}

/// An entity that does know about a nullable column, and deliberately sends nothing for it.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct Labelled {
    id: Uuid,
    name: String,
    label: Option<String>,
}

async fn setup(admin: &PgPool, schema: &str) {
    sqlx::raw_sql(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
        .execute(admin)
        .await
        .unwrap();
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema};
         CREATE TABLE {schema}.widget (
             id          uuid PRIMARY KEY,
             name        text NOT NULL,
             -- A decorator column the entity does not carry. NOT NULL with a default is the shape
             -- that turns the defect into a hard refusal rather than a silent NULL.
             tier        text NOT NULL DEFAULT 'standard',
             -- The tenancy shape: defaulted from the session, exactly as ADR-0029 installs it.
             org_unit_id uuid NOT NULL
                 DEFAULT NULLIF(current_setting('app.acting_unit_id', true), '')::uuid
         );
         CREATE TABLE {schema}.labelled (
             id    uuid PRIMARY KEY,
             name  text NOT NULL,
             label text DEFAULT 'from-default'
         );"
    ))
    .execute(admin)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_column_the_entity_does_not_carry_takes_its_default() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    // One connection on purpose: a session GUC belongs to the connection that set it, so a pool
    // handing out a second one would leave the decorator's default reading an unset value. This is
    // the same rule the composition follows when it scopes a request to one connection.
    let pool = PgPoolOptions::new().max_connections(1).connect(&dsn).await.expect("connect");
    let schema = format!("orm_def_{}", &Uuid::new_v4().simple().to_string()[..8]);
    setup(&pool, &schema).await;

    // The acting unit the decorator's default reads, bound on that one connection.
    let unit = Uuid::new_v4();
    sqlx::query("SELECT set_config('app.acting_unit_id', $1, false)")
        .bind(unit.to_string())
        .execute(&pool)
        .await
        .unwrap();

    let repo: PostgresRepository<Widget> =
        PostgresRepository::new(pool.clone(), &format!("{schema}.widget"));
    let id = Uuid::new_v4();
    let created = repo
        .create(&Widget { id, name: "first".into() })
        .await
        .expect("a create over a decorated table must not be refused");
    assert_eq!(created.id, id);

    let (tier, org_unit): (String, Uuid) =
        sqlx::query_as(&format!("SELECT tier, org_unit_id FROM {schema}.widget WHERE id=$1"))
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tier, "standard", "a plain default fires instead of being overridden with NULL");
    assert_eq!(org_unit, unit, "the tenancy default resolves the acting unit");

    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE")).execute(&pool).await.unwrap();
}

#[tokio::test]
async fn a_column_the_entity_sends_as_null_stays_null() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = PgPool::connect(&dsn).await.expect("connect");
    let schema = format!("orm_def_{}", &Uuid::new_v4().simple().to_string()[..8]);
    setup(&pool, &schema).await;

    let repo: PostgresRepository<Labelled> =
        PostgresRepository::new(pool.clone(), &format!("{schema}.labelled"));
    let id = Uuid::new_v4();
    repo.create(&Labelled { id, name: "first".into(), label: None })
        .await
        .expect("create");

    let label: Option<String> =
        sqlx::query_scalar(&format!("SELECT label FROM {schema}.labelled WHERE id=$1"))
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        label, None,
        "a column the caller explicitly sent as null stays null — the default must not silently \
         overrule what the caller asked for"
    );

    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE")).execute(&pool).await.unwrap();
}

/// An entity carrying a field the table has no column for.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct Mismatched {
    id: Uuid,
    name: String,
    #[sqlx(skip)]
    computed_total: i64,
}

#[tokio::test]
async fn a_field_with_no_column_says_so() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = PgPool::connect(&dsn).await.expect("connect");
    let schema = format!("orm_def_{}", &Uuid::new_v4().simple().to_string()[..8]);
    setup(&pool, &schema).await;

    let repo: PostgresRepository<Mismatched> =
        PostgresRepository::new(pool.clone(), &format!("{schema}.labelled"));
    let err = repo
        .create(&Mismatched { id: Uuid::new_v4(), name: "first".into(), computed_total: 7 })
        .await
        .expect_err("a serialized field with no column cannot be inserted");

    let text = format!("{err:#}");
    assert!(
        text.contains("serializes a field with no matching column"),
        "the failure explains the mismatch instead of leaving a bare database error: {text}"
    );

    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE")).execute(&pool).await.unwrap();
}
