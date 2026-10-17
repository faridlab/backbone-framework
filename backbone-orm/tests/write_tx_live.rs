//! Live proof of the write transaction: one transaction around everything a write does.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN) — the test creates a schema and a login
//! role, and drops both. When it is unset the test skips, so a checkout without a database
//! still passes.
//!
//! What it holds the code to:
//! - a write made inside `in_write_tx` and a later failure roll back everything, bulk writes
//!   included, on a pool connection and on a request-scope connection alike;
//! - the write rides the one connection the request holds (a one-connection pool would hang the
//!   moment anything took a second one);
//! - without `in_write_tx` the repository behaves as it always did;
//! - row-level security decides inside the transaction as it does outside it;
//! - a nested transaction is a savepoint, a swallowed statement error cannot be committed past,
//!   and a dropped future leaves no open transaction behind.

use std::time::Duration;

use backbone_orm::company_scope::{with_company_scope, with_request_scope};
use backbone_orm::generic_repository::{EntityRepoMeta, GenericCrudRepository, SoftDelete};
use backbone_orm::in_write_tx;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

fn admin_dsn() -> Option<String> {
    std::env::var("BACKBONE_ORM_RLS_DSN").ok()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
struct Item {
    id: Uuid,
    company_id: Uuid,
    name: String,
    metadata: serde_json::Value,
}

impl EntityRepoMeta for Item {
    fn column_types() -> HashMap<String, String> {
        HashMap::from([("id".to_string(), "uuid".to_string())])
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
}

fn item(company: Uuid, name: &str) -> Item {
    Item {
        id: Uuid::new_v4(),
        company_id: company,
        name: name.into(),
        metadata: serde_json::json!({}),
    }
}

struct Harness {
    admin: PgPool,
    schema: String,
    role: String,
    dsn: String,
}

impl Harness {
    async fn new(dsn: String) -> Self {
        let admin = PgPoolOptions::new()
            .max_connections(3)
            .connect(&dsn)
            .await
            .unwrap();
        let suffix = &Uuid::new_v4().simple().to_string()[..8];
        let schema = format!("wtx_{suffix}");
        let role = format!("wtx_app_{suffix}");
        sqlx::raw_sql(&format!(
            "CREATE SCHEMA {schema};
             CREATE TABLE {schema}.items (
                 id         uuid PRIMARY KEY,
                 company_id uuid NOT NULL,
                 name       text NOT NULL CONSTRAINT ck_items__name_refused CHECK (name <> 'refused'),
                 metadata   jsonb NOT NULL DEFAULT '{{}}'::jsonb
             );
             ALTER TABLE {schema}.items ENABLE ROW LEVEL SECURITY;
             ALTER TABLE {schema}.items FORCE ROW LEVEL SECURITY;
             CREATE POLICY items_company_isolation ON {schema}.items FOR ALL
                 USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
                 WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
             CREATE ROLE {role} LOGIN PASSWORD 'wtxpw';
             GRANT USAGE ON SCHEMA {schema} TO {role};
             GRANT SELECT, INSERT, UPDATE, DELETE ON {schema}.items TO {role};"
        ))
        .execute(&admin)
        .await
        .unwrap();
        Self {
            admin,
            schema,
            role,
            dsn,
        }
    }

    /// A pool that logs in as the non-superuser request role. One connection when `single`, so
    /// anything that takes a second connection while the first is held would hang.
    async fn app_pool(&self, single: bool) -> PgPool {
        let after_at = self.dsn.rsplit('@').next().unwrap();
        let url = format!("postgresql://{}:wtxpw@{after_at}", self.role);
        PgPoolOptions::new()
            .max_connections(if single { 1 } else { 4 })
            .acquire_timeout(Duration::from_secs(5))
            .connect(&url)
            .await
            .unwrap()
    }

    fn repo(&self, pool: &PgPool) -> GenericCrudRepository<Item, SoftDelete> {
        GenericCrudRepository::new(pool.clone(), &format!("{}.items", self.schema))
    }

    /// Rows as the owner sees them (RLS does not apply to a superuser).
    async fn count(&self) -> i64 {
        sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}.items", self.schema))
            .fetch_one(&self.admin)
            .await
            .unwrap()
    }

    async fn teardown(self) {
        sqlx::raw_sql(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema))
            .execute(&self.admin)
            .await
            .unwrap();
        sqlx::raw_sql(&format!("DROP ROLE IF EXISTS {}", self.role))
            .execute(&self.admin)
            .await
            .unwrap();
    }
}

macro_rules! harness {
    () => {{
        let Some(dsn) = admin_dsn() else {
            eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
            return;
        };
        Harness::new(dsn).await
    }};
}

type Res<T> = anyhow::Result<T>;

#[tokio::test]
async fn a_later_failure_rolls_back_every_write_of_the_transaction_on_a_pool_connection() {
    let h = harness!();
    let pool = h.app_pool(false).await;
    let repo = h.repo(&pool);
    let a = Uuid::new_v4();

    let failed: Res<()> = with_company_scope(
        Some(a),
        in_write_tx(&pool, async {
            let created = repo.create(&item(a, "one")).await?;
            // Inside the transaction the row is visible to the repository, and only to it.
            assert!(repo.find_by_id(&created.id.to_string()).await?.is_some());
            assert_eq!(
                h.count().await,
                0,
                "an uncommitted row must not be visible to another connection"
            );
            repo.bulk_create(&[item(a, "two"), item(a, "three")])
                .await?;
            anyhow::bail!("a later step failed")
        }),
    )
    .await;
    assert!(failed.is_err());
    assert_eq!(
        h.count().await,
        0,
        "the failure must undo the create and the bulk create"
    );

    let committed: Res<()> = with_company_scope(
        Some(a),
        in_write_tx(&pool, async {
            repo.create(&item(a, "one")).await?;
            repo.bulk_create(&[item(a, "two"), item(a, "three")])
                .await?;
            Ok(())
        }),
    )
    .await;
    committed.unwrap();
    assert_eq!(
        h.count().await,
        3,
        "a successful transaction commits all of it"
    );
    h.teardown().await;
}

#[tokio::test]
async fn the_transaction_rides_the_one_connection_of_a_request_scope() {
    let h = harness!();
    // A single connection: the request scope holds it, so a write transaction that took another
    // one would wait for the pool until the acquire times out.
    let pool = h.app_pool(true).await;
    let repo = h.repo(&pool);
    let a = Uuid::new_v4();

    let failed: Res<()> = with_request_scope(&pool, a, async {
        in_write_tx(&pool, async {
            repo.create(&item(a, "one")).await?;
            repo.bulk_create(&[item(a, "two"), item(a, "three")])
                .await?;
            anyhow::bail!("a later step failed")
        })
        .await
    })
    .await
    .unwrap();
    assert!(failed.is_err());
    assert_eq!(h.count().await, 0);

    let done: Res<usize> = with_request_scope(&pool, a, async {
        in_write_tx(&pool, async {
            repo.create(&item(a, "one")).await?;
            let rows = repo
                .bulk_create(&[item(a, "two"), item(a, "three")])
                .await?;
            Ok(rows.len())
        })
        .await
    })
    .await
    .unwrap();
    assert_eq!(done.unwrap(), 2);
    assert_eq!(h.count().await, 3);
    h.teardown().await;
}

#[tokio::test]
async fn without_a_write_transaction_every_write_commits_on_its_own_as_before() {
    let h = harness!();
    let pool = h.app_pool(false).await;
    let repo = h.repo(&pool);
    let a = Uuid::new_v4();

    // Outside any transaction a create is durable the moment it returns, whatever follows.
    let outcome: Res<()> = with_company_scope(Some(a), async {
        repo.create(&item(a, "one")).await?;
        anyhow::bail!("a later step failed")
    })
    .await;
    assert!(outcome.is_err());
    assert_eq!(
        h.count().await,
        1,
        "no write transaction means no rollback of an earlier write"
    );

    // A bulk create is still all-or-nothing on its own.
    let refused = with_company_scope(
        Some(a),
        repo.bulk_create(&[item(a, "two"), item(a, "refused"), item(a, "three")]),
    )
    .await;
    assert!(refused.is_err());
    assert_eq!(h.count().await, 1, "the bulk create rolled itself back");

    let rows = with_company_scope(
        Some(a),
        repo.bulk_create(&[item(a, "two"), item(a, "three")]),
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(h.count().await, 3);
    h.teardown().await;
}

#[tokio::test]
async fn row_level_security_still_decides_inside_the_transaction() {
    let h = harness!();
    let pool = h.app_pool(false).await;
    let repo = h.repo(&pool);
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let foreign = item(b, "b-row");
    sqlx::query(&format!(
        "INSERT INTO {}.items (id, company_id, name) VALUES ($1, $2, $3)",
        h.schema
    ))
    .bind(foreign.id)
    .bind(b)
    .bind(&foreign.name)
    .execute(&h.admin)
    .await
    .unwrap();

    // A's transaction does not see B's row, and a write forging B's company is refused.
    let seen: Res<Option<Item>> = with_company_scope(
        Some(a),
        in_write_tx(&pool, async {
            Ok(repo.find_by_id(&foreign.id.to_string()).await?)
        }),
    )
    .await;
    assert!(
        seen.unwrap().is_none(),
        "the fence must hold inside the transaction"
    );

    let forged: Res<()> = with_company_scope(
        Some(a),
        in_write_tx(&pool, async {
            repo.create(&item(a, "mine")).await?;
            repo.create(&item(b, "forged")).await?;
            Ok(())
        }),
    )
    .await;
    assert!(forged.is_err(), "WITH CHECK must refuse the forged company");
    assert_eq!(
        h.count().await,
        1,
        "the refusal undid A's own earlier write too"
    );

    // No scope at all fails closed: the write is refused, never unfenced.
    let unscoped: Res<()> = in_write_tx(&pool, async {
        repo.create(&item(a, "orphan")).await?;
        Ok(())
    })
    .await;
    assert!(
        unscoped.is_err(),
        "an unscoped write transaction must not write to a fenced table"
    );

    // The request-scope path holds the same line.
    let forged_in_request: Res<()> = with_request_scope(&pool, a, async {
        in_write_tx(&pool, async {
            repo.create(&item(b, "forged")).await?;
            Ok(())
        })
        .await
    })
    .await
    .unwrap();
    assert!(forged_in_request.is_err());
    assert_eq!(h.count().await, 1);
    h.teardown().await;
}

#[tokio::test]
async fn a_nested_transaction_is_a_savepoint_and_a_bulk_failure_leaves_the_outer_one_usable() {
    let h = harness!();
    let pool = h.app_pool(false).await;
    let repo = h.repo(&pool);
    let a = Uuid::new_v4();

    let outcome: Res<()> = with_company_scope(
        Some(a),
        in_write_tx(&pool, async {
            repo.create(&item(a, "outer")).await?;
            // The inner transaction fails: only its own work is undone.
            let inner: Res<()> = in_write_tx(&pool, async {
                repo.create(&item(a, "inner")).await?;
                anyhow::bail!("inner step failed")
            })
            .await;
            assert!(inner.is_err());
            // A bulk create refused partway rolls back its own rows and nothing else.
            let bulk = repo.bulk_create(&[item(a, "x"), item(a, "refused")]).await;
            assert!(bulk.is_err());
            repo.create(&item(a, "after")).await?;
            Ok(())
        }),
    )
    .await;
    outcome.unwrap();
    let names: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT name FROM {}.items ORDER BY name",
        h.schema
    ))
    .fetch_all(&h.admin)
    .await
    .unwrap();
    assert_eq!(names, vec!["after".to_string(), "outer".to_string()]);

    // Joined, the inner work waits for the outer commit: the outer failing undoes it too.
    let outer_fails: Res<()> = with_company_scope(
        Some(a),
        in_write_tx(&pool, async {
            let inner: Res<()> = in_write_tx(&pool, async {
                repo.create(&item(a, "inner-ok")).await?;
                Ok(())
            })
            .await;
            inner?;
            anyhow::bail!("outer step failed")
        }),
    )
    .await;
    assert!(outer_fails.is_err());
    assert_eq!(h.count().await, 2);
    h.teardown().await;
}

#[tokio::test]
async fn a_swallowed_statement_error_cannot_be_committed_past() {
    let h = harness!();
    let pool = h.app_pool(false).await;
    let repo = h.repo(&pool);
    let a = Uuid::new_v4();

    // A hook that catches the database's refusal and carries on leaves the transaction aborted.
    // Postgres would answer COMMIT with a quiet ROLLBACK; the write transaction must not call
    // that a success.
    let outcome: Res<()> = with_company_scope(
        Some(a),
        in_write_tx(&pool, async {
            repo.create(&item(a, "kept?")).await?;
            let _ignored = repo.create(&item(a, "refused")).await;
            Ok(())
        }),
    )
    .await;
    assert!(
        outcome.is_err(),
        "an aborted transaction must surface as an error, not as success"
    );
    assert_eq!(h.count().await, 0);
    h.teardown().await;
}

#[tokio::test]
async fn a_dropped_future_leaves_no_transaction_open_and_no_row_behind() {
    let h = harness!();
    // One connection: the connection the dropped transaction used is the only one there is.
    let pool = h.app_pool(true).await;
    let repo = h.repo(&pool);
    let a = Uuid::new_v4();

    let abandoned = tokio::time::timeout(
        Duration::from_millis(400),
        with_company_scope(
            Some(a),
            in_write_tx(&pool, async {
                repo.create(&item(a, "doomed")).await?;
                // Never finishes: the timeout drops this future mid-transaction.
                std::future::pending::<()>().await;
                Ok::<(), anyhow::Error>(())
            }),
        ),
    )
    .await;
    assert!(abandoned.is_err(), "the write must have been cut off");

    // The rollback runs from a spawned task; the pool hands the connection back only after it.
    let open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pg_stat_activity \
         WHERE usename = current_user AND state LIKE 'idle in transaction%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(open, 0, "a dropped write transaction must not stay open");
    assert_eq!(h.count().await, 0, "and its rows must be gone");
    h.teardown().await;
}
