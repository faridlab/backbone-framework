//! Live proof of what the generated hook triggers rely on: a `before` hook that fills a field is
//! persisted, an `after` hook that stages a `NotificationRequested` commits it with the row, triggers
//! that write through the entity's own service nest up to a cap and the run past it rolls the whole
//! write back with a coded violation, and a trigger that fails after staging takes the row and
//! everything staged down together.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN): the test creates a schema and drops it.
//! When the variable is unset the tests skip, so a checkout without a database still passes.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use backbone_core::service::{
    ApplyUpdateDto, FromCreateDto, GenericCrudService, ServiceError, ServiceResult,
};
use backbone_core::write_hooks::{
    current_trigger_depth, in_trigger, with_correlation_id, Change, NotificationRequested,
    WriteCtx, WriteHooks, TRIGGER_DEPTH_EXCEEDED, TRIGGER_MAX_DEPTH,
};
use backbone_core::PersistentEntity;
use backbone_outbox::outbox;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

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

/// What the generated hooks do, by hand: the `before` fills the title (a `set:`), the `after` runs
/// its triggers in `in_trigger`. A title `chain:<n>` makes the trigger write the next ticket
/// through the service (a hand method writing another row), up to `limit`; a title containing
/// `fail` makes it fail after staging.
struct TriggerHooks {
    schema: String,
    service: Arc<OnceLock<TicketService>>,
    limit: u32,
}

#[async_trait]
impl WriteHooks<Ticket> for TriggerHooks {
    fn outbox_schema(&self) -> Option<&str> {
        Some(&self.schema)
    }

    async fn before(&self, _ctx: &WriteCtx, change: &mut Change<'_, Ticket>) -> ServiceResult<()> {
        in_trigger(async {
            if matches!(change, Change::Created { .. }) {
                if let Some(entity) = change.after_mut() {
                    entity.title = format!("set:{}", entity.title);
                }
            }
            Ok(())
        })
        .await
    }

    async fn after(&self, ctx: &WriteCtx, change: &Change<'_, Ticket>) -> ServiceResult<()> {
        in_trigger(async {
            let Change::Created { after } = change else {
                return Ok(());
            };
            let request = NotificationRequested {
                channel: "email".into(),
                template: Some("created".into()),
                body: None,
                to: serde_json::json!(["a@b.test"]),
                data: serde_json::json!({ "title": after.title, "depth": current_trigger_depth() }),
                trigger: "Ticket.after_create".into(),
            };
            ctx.stage(request.record("Ticket", &after.entity_id(), ctx.company_or_nil(), ctx))
                .await?;
            if after.title.contains("fail") {
                return Err(ServiceError::Violations(vec![
                    backbone_core::Violation::new(
                        "title",
                        "trigger_refused",
                        "a trigger refused after staging",
                    ),
                ]));
            }
            let chain = after
                .title
                .strip_prefix("set:chain:")
                .and_then(|n| n.parse::<u32>().ok());
            if let Some(n) = chain.filter(|n| *n < self.limit) {
                let service = self.service.get().expect("the service is wired");
                service
                    .create(CreateTicket {
                        title: format!("chain:{}", n + 1),
                    })
                    .await?;
            }
            Ok(())
        })
        .await
    }
}

struct Harness {
    pool: PgPool,
    schema: String,
    service: TicketService,
}

impl Harness {
    async fn new(limit: u32) -> Option<Self> {
        let Ok(dsn) = std::env::var("BACKBONE_ORM_RLS_DSN") else {
            eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
            return None;
        };
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(&dsn)
            .await
            .unwrap();
        let schema = format!("th_{}", &Uuid::new_v4().simple().to_string()[..8]);
        sqlx::raw_sql(&format!(
            "CREATE SCHEMA {schema};
             CREATE TABLE {schema}.tickets (
                 id       uuid PRIMARY KEY,
                 title    text NOT NULL,
                 status   text NOT NULL DEFAULT 'open',
                 metadata jsonb NOT NULL DEFAULT '{{}}'::jsonb
             );"
        ))
        .execute(&pool)
        .await
        .unwrap();
        outbox::migrate(&pool, &schema).await.unwrap();
        let repo = Arc::new(TicketRepo(backbone_orm::GenericCrudRepository::new(
            pool.clone(),
            &format!("{schema}.tickets"),
        )));
        let cell: Arc<OnceLock<TicketService>> = Arc::default();
        let service =
            TicketService::with_repository(repo).with_write_hooks(Arc::new(TriggerHooks {
                schema: schema.clone(),
                service: cell.clone(),
                limit,
            }));
        cell.set(service.clone()).ok();
        Some(Self {
            pool,
            schema,
            service,
        })
    }

    async fn titles(&self) -> Vec<String> {
        sqlx::query_scalar(&format!(
            "SELECT title FROM {}.tickets ORDER BY title",
            self.schema
        ))
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    async fn events(&self) -> Vec<(String, Option<String>, serde_json::Value)> {
        sqlx::query_as(&format!(
            "SELECT event_type, correlation_id, payload FROM {}.outbox_events ORDER BY payload->>'title'",
            self.schema
        ))
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    async fn teardown(self) {
        sqlx::raw_sql(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.pool)
            .await
            .unwrap();
    }
}

macro_rules! harness {
    ($limit:expr) => {
        match Harness::new($limit).await {
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

#[tokio::test]
async fn a_before_trigger_fills_the_row_and_an_after_trigger_stages_its_request_with_the_write() {
    let h = harness!(0);
    let made = with_correlation_id("req-1", h.service.create(create("plain")))
        .await
        .unwrap();
    assert_eq!(
        made.title, "set:plain",
        "the value the before hook set is what the write stored"
    );
    assert_eq!(h.titles().await, vec!["set:plain"]);
    let events = h.events().await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "NotificationRequested");
    assert_eq!(
        events[0].1.as_deref(),
        Some("req-1"),
        "the staged row carries the request's correlation id"
    );
    assert_eq!(events[0].2["channel"], "email");
    assert_eq!(events[0].2["trigger"], "Ticket.after_create");
    assert_eq!(current_trigger_depth(), 0);
    h.teardown().await;
}

#[tokio::test]
async fn triggers_that_write_through_the_service_nest_and_commit_together_within_the_cap() {
    let h = harness!(4);
    with_correlation_id("req-2", h.service.create(create("chain:1")))
        .await
        .unwrap();
    assert_eq!(
        h.titles().await,
        vec!["set:chain:1", "set:chain:2", "set:chain:3", "set:chain:4"]
    );
    let events = h.events().await;
    assert_eq!(
        events.len(),
        4,
        "one request per row, all in the one transaction"
    );
    assert!(
        events.iter().all(|e| e.1.as_deref() == Some("req-2")),
        "one correlation id for the whole chain"
    );
    // The nesting shows in the depth each run saw: 1 for the outermost after-hook, 2 for the next...
    let depths: Vec<u64> = events
        .iter()
        .map(|e| e.2["data"]["depth"].as_u64().unwrap())
        .collect();
    assert_eq!(depths, vec![1, 2, 3, 4]);
    h.teardown().await;
}

#[tokio::test]
async fn a_chain_deeper_than_the_cap_is_refused_and_the_whole_write_rolls_back() {
    let h = harness!(50);
    let err = h.service.create(create("chain:1")).await.unwrap_err();
    match err {
        ServiceError::Violations(v) => {
            assert_eq!(v[0].code, TRIGGER_DEPTH_EXCEEDED);
            assert_eq!(v[0].params["max"], TRIGGER_MAX_DEPTH);
        }
        other => panic!("expected the depth violation, got {other:?}"),
    }
    assert!(h.titles().await.is_empty(), "no row of the chain survives");
    assert!(
        h.events().await.is_empty(),
        "no request of the chain survives"
    );
    assert_eq!(current_trigger_depth(), 0);
    h.teardown().await;
}

#[tokio::test]
async fn a_trigger_that_fails_after_staging_takes_the_row_and_its_request_down() {
    let h = harness!(0);
    let err = h.service.create(create("will fail")).await.unwrap_err();
    match err {
        ServiceError::Violations(v) => assert_eq!(v[0].code, "trigger_refused"),
        other => panic!("expected the trigger's violation, got {other:?}"),
    }
    assert!(h.titles().await.is_empty());
    assert!(
        h.events().await.is_empty(),
        "the request staged before the failure rolled back too"
    );
    h.teardown().await;
}
