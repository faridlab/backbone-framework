//! Live proof that the id-list batch operations ride the connection a write runs on, as `bulk_create`
//! does: the write transaction when one is open (as a savepoint of it), else the request's own
//! connection, else a connection of their own.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN, e.g.
//! `postgresql://root:password@localhost:5434/postgres`): the tests create a schema and a login role
//! and drop them. When it is unset they skip, so a checkout without a database still passes.
//!
//! The defect these guard against: `bulk_soft_delete`, `bulk_restore`, `bulk_permanent_delete`,
//! `bulk_delete` and the full-row `bulk_update` took a pooled connection with only the company
//! bound, so under a row-level fence on the org unit they saw no row, matched none of their ids and
//! failed the whole batch ("rolled back"), while the newer batch paths worked; and a batch that ran
//! inside a write transaction had to be proven to commit and roll back with it.

use backbone_orm::generic_repository::{
    EntityRepoMeta, GenericCrudRepository, HardDelete, SoftDelete,
};
use backbone_orm::org_scope::{with_org_request_scope, OrgScope};
use backbone_orm::{current_write_tx, in_write_tx};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

fn admin_dsn() -> Option<String> {
    std::env::var("BACKBONE_ORM_RLS_DSN").ok()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
struct Thing {
    id: Uuid,
    org_unit_id: Uuid,
    name: String,
    metadata: serde_json::Value,
}

impl EntityRepoMeta for Thing {
    fn column_types() -> HashMap<String, String> {
        HashMap::from([("id".to_string(), "uuid".to_string())])
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
}

fn thing(unit: Uuid, name: &str) -> Thing {
    Thing {
        id: Uuid::new_v4(),
        org_unit_id: unit,
        name: name.into(),
        metadata: serde_json::json!({}),
    }
}

struct Fixture {
    admin: PgPool,
    /// Connects as a role that is neither superuser nor BYPASSRLS, so the fence applies.
    app: PgPool,
    schema: String,
    role: String,
    unit: Uuid,
    other: Uuid,
}

impl Fixture {
    async fn open() -> Option<Self> {
        let dsn = admin_dsn()?;
        let admin = PgPoolOptions::new()
            .max_connections(3)
            .connect(&dsn)
            .await
            .expect("admin connect");
        let tag = Uuid::new_v4().simple().to_string()[..8].to_string();
        let (schema, role) = (format!("orm_fence_{tag}"), format!("orm_fence_app_{tag}"));
        let predicate =
            "org_unit_id = ANY(string_to_array(current_setting('app.scope_unit_ids', true), ',')::uuid[])";
        sqlx::raw_sql(&format!(
            "CREATE SCHEMA {schema};
             CREATE TABLE {schema}.things (
                 id          uuid PRIMARY KEY,
                 org_unit_id uuid NOT NULL,
                 name        text NOT NULL,
                 metadata    jsonb NOT NULL DEFAULT '{{}}'::jsonb
             );
             CREATE TABLE {schema}.things_log (
                 id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
                 org_unit_id uuid NOT NULL,
                 what        text NOT NULL
             );
             ALTER TABLE {schema}.things ENABLE ROW LEVEL SECURITY;
             ALTER TABLE {schema}.things FORCE ROW LEVEL SECURITY;
             CREATE POLICY things_org ON {schema}.things FOR ALL USING ({predicate}) WITH CHECK ({predicate});
             ALTER TABLE {schema}.things_log ENABLE ROW LEVEL SECURITY;
             ALTER TABLE {schema}.things_log FORCE ROW LEVEL SECURITY;
             CREATE POLICY things_log_org ON {schema}.things_log FOR ALL USING ({predicate}) WITH CHECK ({predicate});
             CREATE ROLE {role} LOGIN PASSWORD 'fencepw';
             GRANT USAGE ON SCHEMA {schema} TO {role};
             GRANT SELECT, INSERT, UPDATE, DELETE ON {schema}.things, {schema}.things_log TO {role};"
        ))
        .execute(&admin)
        .await
        .expect("set up the fenced schema");
        let after_at = dsn.rsplit('@').next().expect("dsn has an @host part");
        let app = PgPoolOptions::new()
            .max_connections(4)
            .connect(&format!("postgresql://{role}:fencepw@{after_at}"))
            .await
            .expect("connect as the app role");
        Some(Self {
            admin,
            app,
            schema,
            role,
            unit: Uuid::new_v4(),
            other: Uuid::new_v4(),
        })
    }

    fn table(&self) -> String {
        format!("{}.things", self.schema)
    }

    fn soft(&self) -> GenericCrudRepository<Thing, SoftDelete> {
        GenericCrudRepository::new(self.app.clone(), &self.table())
    }

    fn hard(&self) -> GenericCrudRepository<Thing, HardDelete> {
        GenericCrudRepository::new(self.app.clone(), &self.table())
    }

    /// Insert rows as the owner (which bypasses the fence).
    async fn seed(&self, unit: Uuid, names: &[&str]) -> Vec<Thing> {
        let rows: Vec<Thing> = names.iter().map(|n| thing(unit, n)).collect();
        for row in &rows {
            sqlx::query(&format!(
                "INSERT INTO {} (id, org_unit_id, name) VALUES ($1, $2, $3)",
                self.table()
            ))
            .bind(row.id)
            .bind(row.org_unit_id)
            .bind(&row.name)
            .execute(&self.admin)
            .await
            .unwrap();
        }
        rows
    }

    fn scope(&self) -> OrgScope {
        OrgScope::for_company_unit(self.unit)
    }

    async fn deleted(&self, id: Uuid) -> Option<bool> {
        sqlx::query_scalar::<_, bool>(&format!(
            "SELECT (metadata->>'deleted_at') IS NOT NULL FROM {} WHERE id = $1",
            self.table()
        ))
        .bind(id)
        .fetch_optional(&self.admin)
        .await
        .unwrap()
    }

    async fn logged(&self) -> i64 {
        sqlx::query_scalar(&format!("SELECT count(*) FROM {}.things_log", self.schema))
            .fetch_one(&self.admin)
            .await
            .unwrap()
    }

    /// Write a log row on the connection the current write transaction holds, as a hook that stages
    /// an event does.
    async fn log_in_write_tx(&self, what: &str) -> Result<(), sqlx::Error> {
        let tx = current_write_tx().expect("inside a write transaction");
        let mut conn = tx.conn().await;
        sqlx::query(&format!(
            "INSERT INTO {}.things_log (org_unit_id, what) VALUES ($1, $2)",
            self.schema
        ))
        .bind(self.unit)
        .bind(what)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    async fn drop(self) {
        self.app.close().await;
        sqlx::raw_sql(&format!(
            "DROP SCHEMA {} CASCADE; DROP ROLE {};",
            self.schema, self.role
        ))
        .execute(&self.admin)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn the_id_list_batches_work_under_the_org_fence_inside_a_request_scope() {
    let Some(f) = Fixture::open().await else {
        return;
    };
    let rows = f.seed(f.unit, &["a", "b", "c"]).await;
    let foreign = f.seed(f.other, &["x"]).await;
    let ids: Vec<String> = rows.iter().map(|r| r.id.to_string()).collect();
    let repo = f.soft();

    let affected = with_org_request_scope(&f.app, f.scope(), repo.bulk_soft_delete(&ids))
        .await
        .expect("scope")
        .expect("a soft delete under the org fence");
    assert_eq!(affected, 3);
    assert!(f.deleted(rows[0].id).await.unwrap());

    let restored = with_org_request_scope(&f.app, f.scope(), repo.bulk_restore(&ids[..2]))
        .await
        .expect("scope")
        .expect("a restore under the org fence");
    assert_eq!(restored.len(), 2);
    assert!(!f.deleted(rows[0].id).await.unwrap());
    assert!(
        f.deleted(rows[2].id).await.unwrap(),
        "the third id stayed in the trash"
    );

    let purged = with_org_request_scope(&f.app, f.scope(), repo.bulk_permanent_delete(&ids[2..]))
        .await
        .expect("scope")
        .expect("a permanent delete under the org fence");
    assert_eq!(purged, 1);
    assert!(f.deleted(rows[2].id).await.is_none());

    let edited: Vec<Thing> = restored
        .iter()
        .map(|r| Thing {
            name: format!("{}!", r.name),
            ..r.clone()
        })
        .collect();
    let updated = with_org_request_scope(&f.app, f.scope(), repo.bulk_update(&edited))
        .await
        .expect("scope")
        .expect("a full-row bulk update under the org fence");
    assert_eq!(
        updated.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
        vec!["a!", "b!"]
    );

    // The hard-delete mode's batch rides the same connection.
    let hard = f.hard();
    let doomed = f.seed(f.unit, &["h1", "h2"]).await;
    let doomed_ids: Vec<String> = doomed.iter().map(|r| r.id.to_string()).collect();
    let gone = with_org_request_scope(&f.app, f.scope(), hard.bulk_delete(&doomed_ids))
        .await
        .expect("scope")
        .expect("a hard bulk delete under the org fence");
    assert_eq!(gone, 2);

    // The fence still holds: another unit's row is not reachable by id, and the batch that names it
    // is refused whole.
    let mixed = vec![ids[0].clone(), foreign[0].id.to_string()];
    let refused = with_org_request_scope(&f.app, f.scope(), repo.bulk_soft_delete(&mixed))
        .await
        .expect("scope");
    assert!(refused.is_err(), "an id the fence hides is not found");
    assert!(
        !f.deleted(rows[0].id).await.unwrap(),
        "the refused batch changed nothing"
    );
    assert!(!f.deleted(foreign[0].id).await.unwrap());
    f.drop().await;
}

#[tokio::test]
async fn a_batch_inside_a_write_transaction_commits_and_rolls_back_with_it() {
    let Some(f) = Fixture::open().await else {
        return;
    };
    let rows = f.seed(f.unit, &["a", "b"]).await;
    let ids: Vec<String> = rows.iter().map(|r| r.id.to_string()).collect();
    let repo = f.soft();

    // The write fails after the batch and the event were staged: neither survives.
    let failed: Result<(), sqlx::Error> = with_org_request_scope(&f.app, f.scope(), async {
        in_write_tx::<_, (), sqlx::Error>(&f.app, async {
            repo.bulk_soft_delete(&ids)
                .await
                .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
            f.log_in_write_tx("deleted").await?;
            Err(sqlx::Error::Protocol("a hook refused the write".into()))
        })
        .await
    })
    .await
    .expect("scope");
    assert!(failed.is_err());
    assert!(
        !f.deleted(rows[0].id).await.unwrap() && !f.deleted(rows[1].id).await.unwrap(),
        "the batch was rolled back with the write"
    );
    assert_eq!(f.logged().await, 0, "and so was the event staged beside it");

    // The write succeeds: both commit together.
    with_org_request_scope(&f.app, f.scope(), async {
        in_write_tx::<_, (), sqlx::Error>(&f.app, async {
            repo.bulk_soft_delete(&ids)
                .await
                .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
            f.log_in_write_tx("deleted").await?;
            Ok(())
        })
        .await
    })
    .await
    .expect("scope")
    .expect("the write commits");
    assert!(f.deleted(rows[0].id).await.unwrap() && f.deleted(rows[1].id).await.unwrap());
    assert_eq!(f.logged().await, 1);

    // A batch the database refuses inside the write is a savepoint: it undoes itself, and the
    // caller that handles the refusal keeps the rest of the write.
    let survived: Result<(), sqlx::Error> = with_org_request_scope(&f.app, f.scope(), async {
        in_write_tx::<_, (), sqlx::Error>(&f.app, async {
            f.log_in_write_tx("before the refused batch").await?;
            let bad = vec![ids[0].clone(), Uuid::new_v4().to_string()];
            assert!(
                repo.bulk_restore(&bad).await.is_err(),
                "an id that is not in the trash refuses the batch"
            );
            f.log_in_write_tx("after the refused batch").await?;
            Ok(())
        })
        .await
    })
    .await
    .expect("scope");
    survived.expect("the write carries on after the refused batch");
    assert_eq!(f.logged().await, 3, "the refused batch undid only itself");
    assert!(
        f.deleted(rows[0].id).await.unwrap(),
        "the refused restore restored nothing"
    );
    f.drop().await;
}

#[tokio::test]
async fn without_a_write_transaction_or_a_request_scope_a_batch_is_all_or_nothing_as_before() {
    let Some(f) = Fixture::open().await else {
        return;
    };
    // The owner pool: no fence applies, no request scope is open, no write transaction is open.
    let rows = f.seed(f.unit, &["a", "b"]).await;
    let ids: Vec<String> = rows.iter().map(|r| r.id.to_string()).collect();
    let repo: GenericCrudRepository<Thing, SoftDelete> =
        GenericCrudRepository::new(f.admin.clone(), &f.table());

    let mixed = vec![ids[0].clone(), Uuid::new_v4().to_string()];
    assert!(
        repo.bulk_soft_delete(&mixed).await.is_err(),
        "a missing id refuses the whole batch"
    );
    assert!(
        !f.deleted(rows[0].id).await.unwrap(),
        "and nothing was changed"
    );
    assert_eq!(repo.bulk_soft_delete(&ids).await.unwrap(), 2);
    assert_eq!(repo.bulk_restore(&ids).await.unwrap().len(), 2);
    assert_eq!(repo.bulk_soft_delete(&ids).await.unwrap(), 2);
    assert_eq!(repo.bulk_permanent_delete(&ids).await.unwrap(), 2);
    f.drop().await;
}
