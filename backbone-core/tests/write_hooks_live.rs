//! Live proof that the generic service runs every write inside one transaction with its hooks:
//! guard, lifecycle before, hooks before, the write, lifecycle after, hooks after, commit; and
//! that a failure anywhere takes the row and every event staged for it down together.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN): the test creates a schema and drops it.
//! When the variable is unset the tests skip, so a checkout without a database still passes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use backbone_core::service::{
    ApplyUpdateDto, FromCreateDto, GenericCrudService, ServiceError, ServiceLifecycle,
    ServiceResult,
};
use backbone_core::write_guard::{as_transition, GuardOutcome, WriteCtx as GuardCtx, WriteGuard};
use backbone_core::write_hooks::{with_correlation_id, Change, WriteCtx, WriteHooks};
use backbone_core::PersistentEntity;
use backbone_messaging::crud_event::{CrudEvent, CrudEventPublisher};
use backbone_outbox::{outbox, OutboxRecord};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

// ── The entity, its repository and service ───────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
struct Ticket {
    id: Uuid,
    title: String,
    status: String,
    metadata: serde_json::Value,
}

impl PersistentEntity for Ticket {
    fn entity_id(&self) -> String {
        self.id.to_string()
    }
    fn set_entity_id(&mut self, id: String) {
        self.id = id.parse().unwrap();
    }
    fn created_at(&self) -> Option<DateTime<Utc>> {
        None
    }
    fn set_created_at(&mut self, _ts: DateTime<Utc>) {}
    fn updated_at(&self) -> Option<DateTime<Utc>> {
        None
    }
    fn set_updated_at(&mut self, _ts: DateTime<Utc>) {}
    fn deleted_at(&self) -> Option<DateTime<Utc>> {
        None
    }
    fn set_deleted_at(&mut self, _ts: Option<DateTime<Utc>>) {}
    fn write_protected_fields() -> &'static [&'static str] {
        &["status"]
    }
    fn state_fields() -> &'static [&'static str] {
        &["status"]
    }
}

impl backbone_orm::EntityRepoMeta for Ticket {
    fn column_types() -> HashMap<String, String> {
        HashMap::from([("id".to_string(), "uuid".to_string())])
    }
    fn search_fields() -> &'static [&'static str] {
        &["title"]
    }
}

struct TicketRepo(backbone_orm::GenericCrudRepository<Ticket, backbone_orm::SoftDelete>);

impl std::ops::Deref for TicketRepo {
    type Target = backbone_orm::GenericCrudRepository<Ticket, backbone_orm::SoftDelete>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

backbone_core::impl_crud_repository!(TicketRepo, Ticket, soft_delete);

#[derive(Deserialize)]
struct CreateTicket {
    title: String,
}

#[derive(Deserialize)]
struct UpdateTicket {
    title: String,
}

impl FromCreateDto<CreateTicket> for Ticket {
    fn from_create_dto(dto: CreateTicket) -> ServiceResult<Self> {
        Ok(Ticket {
            id: Uuid::new_v4(),
            title: dto.title,
            status: "open".into(),
            metadata: serde_json::json!({}),
        })
    }
}

impl ApplyUpdateDto<UpdateTicket> for Ticket {
    fn apply_update(mut self, dto: UpdateTicket) -> ServiceResult<Self> {
        self.title = dto.title;
        Ok(self)
    }
}

type TicketService = GenericCrudService<Ticket, CreateTicket, UpdateTicket, TicketRepo>;

// ── Recording lifecycle, guard, hooks and publisher ──────────────────────────

type Trail = Arc<Mutex<Vec<String>>>;

fn note(trail: &Trail, what: &str) {
    trail.lock().unwrap().push(what.to_string());
}

struct TrailLifecycle(Trail);

#[async_trait]
impl ServiceLifecycle<Ticket> for TrailLifecycle {
    async fn before_create(&self, _e: &mut Ticket) -> ServiceResult<()> {
        note(&self.0, "lifecycle.before_create");
        Ok(())
    }
    async fn after_create(&self, _e: &Ticket) -> ServiceResult<()> {
        note(&self.0, "lifecycle.after_create");
        Ok(())
    }
}

struct TrailGuard(Trail);

#[async_trait]
impl WriteGuard<Ticket> for TrailGuard {
    async fn check(&self, ctx: &GuardCtx<'_, Ticket>) -> GuardOutcome {
        note(&self.0, "guard");
        let mut outcome = GuardOutcome::allow();
        if ctx.after.is_some_and(|t| t.title == "guard-refused") {
            outcome.refuse.push(backbone_core::Violation::new(
                "title",
                "refused",
                "refused by the guard",
            ));
        }
        outcome
    }
}

/// Stages an event per change the way generated hooks do, and fails on demand.
struct TicketHooks {
    schema: String,
    trail: Trail,
}

impl TicketHooks {
    fn record(&self, kind: &str, t: &Ticket, ctx: &WriteCtx) -> OutboxRecord {
        OutboxRecord::new(
            kind,
            "Ticket",
            t.id.to_string(),
            ctx.company_or_nil(),
            serde_json::json!({ "title": t.title }),
            Utc::now(),
        )
        .with_version(2)
    }
}

#[async_trait]
impl WriteHooks<Ticket> for TicketHooks {
    fn outbox_schema(&self) -> Option<&str> {
        Some(&self.schema)
    }

    async fn before(&self, _ctx: &WriteCtx, change: &mut Change<'_, Ticket>) -> ServiceResult<()> {
        note(&self.trail, "hooks.before");
        match change {
            Change::Created { after } if after.title == "before-refused" => Err(
                ServiceError::Validation("a hook refused this create".into()),
            ),
            _ => Ok(()),
        }
    }

    async fn after(&self, ctx: &WriteCtx, change: &Change<'_, Ticket>) -> ServiceResult<()> {
        // The row is written, inside the transaction, before the after-hook runs.
        let tx = backbone_orm::current_write_tx().expect("hooks run inside the write transaction");
        let rows: i64 = {
            let mut conn = tx.conn().await;
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}.tickets", self.schema))
                .fetch_one(&mut *conn)
                .await
                .unwrap()
        };
        note(&self.trail, &format!("hooks.after rows_in_tx={rows}"));
        match change {
            Change::Created { after } => {
                ctx.stage(self.record("TicketOpened", after, ctx)).await?;
                if after.title == "after-refused" {
                    return Err(ServiceError::Validation(
                        "a hook refused after staging".into(),
                    ));
                }
            }
            Change::Transitioned {
                after,
                transition,
                from,
                to,
                ..
            } if transition == "close" => {
                assert_eq!(
                    (from.as_deref(), to.as_deref()),
                    (Some("open"), Some("closed"))
                );
                ctx.stage(self.record("TicketClosed", after, ctx)).await?;
            }
            Change::Updated { after, .. } if change.field_changed("title") => {
                ctx.stage(self.record("TicketRetitled", after, ctx)).await?;
            }
            Change::Deleted {
                before,
                hard: false,
            } => {
                ctx.stage(self.record("TicketDeleted", before, ctx)).await?;
            }
            Change::Restored { entity } => {
                ctx.stage(self.record("TicketRestored", entity, ctx))
                    .await?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Default)]
struct CountingPublisher(Mutex<usize>);

#[async_trait]
impl CrudEventPublisher<Ticket> for CountingPublisher {
    async fn publish(
        &self,
        _event: CrudEvent<Ticket>,
    ) -> Result<(), backbone_messaging::EventError> {
        *self.0.lock().unwrap() += 1;
        Ok(())
    }
}

// ── Harness ──────────────────────────────────────────────────────────────────

struct Harness {
    pool: PgPool,
    schema: String,
    trail: Trail,
    published: Arc<CountingPublisher>,
    service: TicketService,
}

impl Harness {
    async fn new() -> Option<Self> {
        let Ok(dsn) = std::env::var("BACKBONE_ORM_RLS_DSN") else {
            eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
            return None;
        };
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(&dsn)
            .await
            .unwrap();
        let schema = format!("wh_{}", &Uuid::new_v4().simple().to_string()[..8]);
        sqlx::raw_sql(&format!(
            "CREATE SCHEMA {schema};
             CREATE TABLE {schema}.tickets (
                 id       uuid PRIMARY KEY,
                 title    text NOT NULL CONSTRAINT ck_tickets__title_refused CHECK (title <> 'refused'),
                 status   text NOT NULL DEFAULT 'open',
                 metadata jsonb NOT NULL DEFAULT '{{}}'::jsonb
             );"
        ))
        .execute(&pool)
        .await
        .unwrap();
        outbox::migrate(&pool, &schema).await.unwrap();
        // The outbox table refuses one event type, to make staging fail on demand.
        sqlx::raw_sql(&format!(
            "ALTER TABLE {schema}.outbox_events ADD CONSTRAINT ck_poison CHECK (event_type <> 'Poison')"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let trail: Trail = Arc::default();
        let published = Arc::new(CountingPublisher::default());
        let repo = Arc::new(TicketRepo(backbone_orm::GenericCrudRepository::new(
            pool.clone(),
            &format!("{schema}.tickets"),
        )));
        let service = TicketService::new(
            repo,
            Arc::new(TrailLifecycle(trail.clone())),
            published.clone(),
        )
        .with_guard(Arc::new(TrailGuard(trail.clone())))
        .with_write_hooks(Arc::new(TicketHooks {
            schema: schema.clone(),
            trail: trail.clone(),
        }));
        Some(Self {
            pool,
            schema,
            trail,
            published,
            service,
        })
    }

    async fn tickets(&self) -> i64 {
        sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}.tickets", self.schema))
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn events(&self) -> Vec<(String, Option<String>, Option<String>, i32)> {
        sqlx::query_as(&format!(
            "SELECT event_type, correlation_id, causation_id, version FROM {}.outbox_events ORDER BY occurred_at, event_type",
            self.schema
        ))
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    async fn event_count(&self) -> i64 {
        sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {}.outbox_events",
            self.schema
        ))
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    fn published(&self) -> usize {
        *self.published.0.lock().unwrap()
    }

    async fn teardown(self) {
        sqlx::raw_sql(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.pool)
            .await
            .unwrap();
    }
}

macro_rules! harness {
    () => {
        match Harness::new().await {
            Some(h) => h,
            None => return,
        }
    };
}

fn create(title: &str) -> CreateTicket {
    CreateTicket {
        title: title.into(),
    }
}

// ── Order ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_create_runs_guard_lifecycle_hooks_write_and_hooks_in_order_then_commits() {
    let h = harness!();
    let saved = with_correlation_id("req-42", h.service.create(create("one")))
        .await
        .unwrap();

    assert_eq!(
        *h.trail.lock().unwrap(),
        vec![
            "guard",
            "lifecycle.before_create",
            "hooks.before",
            "lifecycle.after_create",
            "hooks.after rows_in_tx=1",
        ],
        "the after-hook sees the row the write made, inside the transaction"
    );
    assert_eq!(h.tickets().await, 1);
    assert_eq!(
        h.events().await,
        vec![("TicketOpened".into(), Some("req-42".into()), None, 2)]
    );
    assert_eq!(
        h.published(),
        1,
        "the in-process publisher still fires, after the commit"
    );
    assert_eq!(saved.title, "one");
    h.teardown().await;
}

#[tokio::test]
async fn a_correlation_id_is_minted_when_the_request_carries_none() {
    let h = harness!();
    h.service.create(create("one")).await.unwrap();
    let events = h.events().await;
    assert_eq!(events.len(), 1);
    let corr = events[0]
        .1
        .as_deref()
        .expect("every staged event carries a correlation id");
    assert!(Uuid::parse_str(corr).is_ok(), "{corr}");
    h.teardown().await;
}

// ── Failure semantics ─────────────────────────────────────────────────────────

#[tokio::test]
async fn a_create_the_guard_or_a_check_refuses_stages_nothing() {
    let h = harness!();
    let by_guard = h.service.create(create("guard-refused")).await;
    assert!(
        matches!(by_guard, Err(ServiceError::Violations(_))),
        "{by_guard:?}"
    );
    let by_check = h.service.create(create("refused")).await;
    assert!(by_check.is_err(), "the CHECK refuses the row");
    assert_eq!((h.tickets().await, h.event_count().await), (0, 0));
    assert_eq!(h.published(), 0);
    h.teardown().await;
}

#[tokio::test]
async fn a_hook_that_fails_before_or_after_the_write_rolls_back_the_row_and_its_events() {
    let h = harness!();
    let before = h.service.create(create("before-refused")).await;
    assert!(matches!(before, Err(ServiceError::Validation(_))));
    assert_eq!((h.tickets().await, h.event_count().await), (0, 0));

    // The after-hook staged TicketOpened and then failed: the row and the event both go.
    let after = h.service.create(create("after-refused")).await;
    assert!(matches!(after, Err(ServiceError::Validation(_))));
    assert_eq!(
        (h.tickets().await, h.event_count().await),
        (0, 0),
        "the staged event rolled back with the row"
    );
    assert_eq!(
        h.published(),
        0,
        "nothing was published for a write that did not commit"
    );
    h.teardown().await;
}

#[tokio::test]
async fn an_event_that_cannot_be_staged_rolls_the_write_back() {
    let h = harness!();
    // Make staging fail with the outbox's own CHECK: hooks that stage `Poison` cannot commit.
    struct PoisonHooks(String);
    #[async_trait]
    impl WriteHooks<Ticket> for PoisonHooks {
        fn outbox_schema(&self) -> Option<&str> {
            Some(&self.0)
        }
        async fn after(&self, ctx: &WriteCtx, change: &Change<'_, Ticket>) -> ServiceResult<()> {
            let t = change.subject();
            ctx.stage(OutboxRecord::new(
                "Poison",
                "Ticket",
                t.id.to_string(),
                ctx.company_or_nil(),
                serde_json::json!({}),
                Utc::now(),
            ))
            .await
        }
    }
    let service = h
        .service
        .clone()
        .with_write_hooks(Arc::new(PoisonHooks(h.schema.clone())));
    let err = service.create(create("one")).await.unwrap_err();
    assert!(err.to_string().contains("staging Poison failed"), "{err}");
    assert_eq!(
        h.tickets().await,
        0,
        "a write whose event cannot be staged must not commit"
    );
    h.teardown().await;
}

#[tokio::test]
async fn a_service_write_inside_an_outer_transaction_is_undone_when_the_outer_one_fails() {
    let h = harness!();
    let outcome: Result<(), anyhow::Error> = backbone_orm::in_write_tx(&h.pool, async {
        h.service
            .create(create("inner"))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        anyhow::bail!("the surrounding verb failed later")
    })
    .await;
    assert!(outcome.is_err());
    assert_eq!((h.tickets().await, h.event_count().await), (0, 0));
    h.teardown().await;
}

// ── Updates, transitions, deletes ─────────────────────────────────────────────

#[tokio::test]
async fn an_update_event_fires_only_when_its_field_changed() {
    let h = harness!();
    let t = h.service.create(create("one")).await.unwrap();
    let id = t.id.to_string();

    // A patch that sets the title to what it already is changes nothing.
    h.service
        .partial_update(
            &id,
            HashMap::from([("title".to_string(), serde_json::json!("one"))]),
        )
        .await
        .unwrap();
    assert_eq!(
        h.event_count().await,
        1,
        "only TicketOpened: the title did not change"
    );

    h.service
        .partial_update(
            &id,
            HashMap::from([("title".to_string(), serde_json::json!("two"))]),
        )
        .await
        .unwrap();
    h.service
        .update(
            &id,
            UpdateTicket {
                title: "three".into(),
            },
        )
        .await
        .unwrap();
    let kinds: Vec<String> = h.events().await.into_iter().map(|e| e.0).collect();
    assert_eq!(
        kinds.iter().filter(|k| *k == "TicketRetitled").count(),
        2,
        "{kinds:?}"
    );
    h.teardown().await;
}

#[tokio::test]
async fn a_transition_stages_its_event_and_a_refused_one_stages_nothing() {
    let h = harness!();
    let t = h.service.create(create("one")).await.unwrap();
    let id = t.id.to_string();
    let close = |status: &'static str| {
        let fields = HashMap::from([("status".to_string(), serde_json::json!(status))]);
        as_transition("close", h.service.partial_update(&id, fields))
    };

    // A generic write may not move the state field: refused, nothing staged.
    let generic = h
        .service
        .partial_update(
            &id,
            HashMap::from([("status".to_string(), serde_json::json!("closed"))]),
        )
        .await;
    assert!(generic.is_err());
    assert_eq!(h.event_count().await, 1);

    // The verb may; the hook sees a transition with its from and to states.
    close("closed").await.unwrap().unwrap();
    let kinds: Vec<String> = h.events().await.into_iter().map(|e| e.0).collect();
    assert!(kinds.contains(&"TicketClosed".to_string()), "{kinds:?}");
    h.teardown().await;
}

#[tokio::test]
async fn delete_and_restore_stage_their_events_in_the_same_transaction() {
    let h = harness!();
    let t = h.service.create(create("one")).await.unwrap();
    let id = t.id.to_string();
    assert!(h.service.soft_delete(&id).await.unwrap());
    assert!(h.service.restore(&id).await.unwrap().is_some());
    let kinds: Vec<String> = h.events().await.into_iter().map(|e| e.0).collect();
    for expected in ["TicketOpened", "TicketDeleted", "TicketRestored"] {
        assert!(
            kinds.contains(&expected.to_string()),
            "{expected} missing from {kinds:?}"
        );
    }
    h.teardown().await;
}

// ── Bulk ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_bulk_create_commits_its_rows_and_events_together_or_not_at_all() {
    let h = harness!();
    // The third row breaks the CHECK: no row, and no event for the first two.
    let refused = h
        .service
        .bulk_create(vec![create("a"), create("b"), create("refused")])
        .await;
    assert!(refused.is_err());
    assert_eq!((h.tickets().await, h.event_count().await), (0, 0));

    let saved = h
        .service
        .bulk_create(vec![create("a"), create("b"), create("c")])
        .await
        .unwrap();
    assert_eq!(saved.len(), 3);
    assert_eq!((h.tickets().await, h.event_count().await), (3, 3));
    assert_eq!(h.published(), 3);

    // A bulk patch whose after-hook fails undoes every row's change and every staged event.
    let ids: Vec<String> = saved.iter().map(|t| t.id.to_string()).collect();
    let patch = |title: &str| {
        ids.iter()
            .map(|id| {
                (
                    id.clone(),
                    HashMap::from([("title".to_string(), serde_json::json!(title))]),
                )
            })
            .collect::<Vec<_>>()
    };
    struct FailLast;
    #[async_trait]
    impl WriteHooks<Ticket> for FailLast {
        async fn after(&self, _ctx: &WriteCtx, change: &Change<'_, Ticket>) -> ServiceResult<()> {
            match change.subject().title.as_str() {
                "fails" => Err(ServiceError::Validation(
                    "hook failed on the last row".into(),
                )),
                _ => Ok(()),
            }
        }
    }
    let failing = h.service.clone().with_write_hooks(Arc::new(FailLast));
    assert!(failing.bulk_partial_update(patch("fails")).await.is_err());
    let titles: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT title FROM {}.tickets ORDER BY title",
        h.schema
    ))
    .fetch_all(&h.pool)
    .await
    .unwrap();
    assert_eq!(titles, vec!["a", "b", "c"], "no row kept the failed patch");

    h.service
        .bulk_partial_update(patch("same-for-all"))
        .await
        .unwrap();
    assert_eq!(
        h.event_count().await,
        6,
        "three TicketRetitled joined the three TicketOpened"
    );
    h.teardown().await;
}
