//! Live proof of the delivery path of `NotificationRequested` rows: a request staged on a write's
//! transaction is read back by the outbox relay and handed to a registered delivery exactly once, a
//! replayed row is not delivered again, a `Retry` leaves the row pending and kills it after the
//! limit, a `Reject` kills it at once with the delivery's reason, and two relays that hand the same
//! row at the same time deliver it once.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN): the tests create a schema and drop it. When the
//! variable is unset they skip, so a checkout without a database still passes.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use backbone_core::notification::{
    DeliveryOutcome, NotificationDelivery, NotificationDispatcher, PendingNotification,
    RecordingDelivery, RetryPolicy,
};
use backbone_core::write_hooks::NotificationRequested;
use backbone_orm::in_write_tx;
use backbone_outbox::{inbox, outbox, relay, OutboxRecord};
use chrono::Utc;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

struct Db {
    pool: PgPool,
    schema: String,
    company: Uuid,
}

impl Db {
    async fn open() -> Option<Self> {
        let dsn = std::env::var("BACKBONE_ORM_RLS_DSN").ok()?;
        let pool = PgPoolOptions::new()
            .max_connections(6)
            .connect(&dsn)
            .await
            .expect("connect");
        let schema = format!("notif_{}", &Uuid::new_v4().simple().to_string()[..8]);
        outbox::migrate(&pool, &schema)
            .await
            .expect("migrate the outbox");
        Some(Self {
            pool,
            schema,
            company: Uuid::new_v4(),
        })
    }

    async fn drop(self) {
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.pool)
            .await
            .unwrap();
    }

    fn request(template: &str) -> NotificationRequested {
        NotificationRequested {
            channel: "email".into(),
            template: Some(template.into()),
            body: None,
            to: json!(["ops@example.test"]),
            data: json!({ "title": "Disk full" }),
            trigger: "Incident.after_create".into(),
        }
    }

    fn record(&self, request: &NotificationRequested) -> OutboxRecord {
        OutboxRecord::new(
            NotificationRequested::NAME,
            "Incident",
            "inc-1",
            self.company,
            serde_json::to_value(request).unwrap(),
            Utc::now(),
        )
        .with_version(NotificationRequested::VERSION)
        .with_correlation_id("corr-live")
        .with_causation_id("cause-live")
    }

    /// Stage `record` on a write transaction, as a hook does, and commit it.
    async fn stage(&self, record: &OutboxRecord) {
        let schema = self.schema.clone();
        let record = record.clone();
        in_write_tx::<_, _, sqlx::Error>(&self.pool, async move {
            outbox::stage_in_write_tx(&schema, &record)
                .await
                .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
            Ok(())
        })
        .await
        .expect("stage in a write transaction");
    }

    fn dispatcher(
        &self,
        delivery: impl NotificationDelivery + 'static,
        policy: RetryPolicy,
    ) -> Arc<NotificationDispatcher> {
        Arc::new(
            NotificationDispatcher::new(self.pool.clone(), self.schema.clone(), Arc::new(delivery))
                .with_retry(policy),
        )
    }

    /// One relay pass through `dispatcher`'s sink: how many rows it marked published.
    async fn pass(&self, dispatcher: &Arc<NotificationDispatcher>) -> usize {
        relay::drain_once(&self.pool, &self.schema, 50, dispatcher.sink())
            .await
            .expect("a relay pass")
    }

    async fn pending(&self) -> i64 {
        outbox::pending_count(&self.pool, &self.schema)
            .await
            .unwrap()
    }

    async fn dead(&self) -> Vec<relay::DeadRecord> {
        relay::list_dead(&self.pool, &self.schema, 10)
            .await
            .unwrap()
    }

    async fn consumed(&self, consumer: &str, id: Uuid) -> bool {
        inbox::was_consumed(&self.pool, &self.schema, consumer, id)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn a_request_staged_in_a_write_tx_is_delivered_once_and_a_replay_is_deduped() {
    let Some(db) = Db::open().await else { return };
    let delivery = RecordingDelivery::new();
    let dispatcher = db.dispatcher(delivery.clone(), RetryPolicy::immediate(3));
    let record = db.record(&Db::request("incident_opened"));

    // A write that fails after staging leaves nothing to deliver.
    let schema = db.schema.clone();
    let doomed = record.clone();
    let failed = in_write_tx::<_, (), sqlx::Error>(&db.pool, async move {
        outbox::stage_in_write_tx(&schema, &doomed)
            .await
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        Err(sqlx::Error::Protocol(
            "the write failed after staging".into(),
        ))
    })
    .await;
    assert!(failed.is_err());
    assert_eq!(db.pass(&dispatcher).await, 0);
    assert_eq!(delivery.calls(), 0, "a rolled-back write requests nothing");

    db.stage(&record).await;
    assert_eq!(db.pending().await, 1);
    assert_eq!(
        db.pass(&dispatcher).await,
        1,
        "the relay marks the delivered row published"
    );
    assert_eq!(delivery.calls(), 1, "the delivery was called exactly once");
    assert_eq!(db.pending().await, 0);
    assert!(
        db.consumed("notification-delivery", record.id).await,
        "the claim committed with the delivery"
    );

    let got = &delivery.received()[0];
    assert_eq!(got.id, record.id, "the event id is the idempotency key");
    assert_eq!(got.attempt, 1);
    assert_eq!(got.request, Db::request("incident_opened"));
    assert_eq!(
        got.correlation_id.as_deref(),
        Some("corr-live"),
        "correlation is preserved"
    );
    assert_eq!(
        got.causation_id.as_deref(),
        Some("cause-live"),
        "causation is preserved"
    );
    assert_eq!(
        (got.aggregate_type.as_str(), got.aggregate_id.as_str()),
        ("Incident", "inc-1")
    );
    assert_eq!(got.company_id, db.company);

    // The relay hands the same row again (a crash between the delivery and the mark): the claim holds.
    sqlx::query(&format!(
        "UPDATE {}.outbox_events SET published_at = NULL",
        db.schema
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    assert_eq!(db.pending().await, 1);
    assert_eq!(
        db.pass(&dispatcher).await,
        1,
        "a replay is accepted and marked published"
    );
    assert_eq!(
        delivery.calls(),
        1,
        "the replay was deduped: the delivery is not called again"
    );
    db.drop().await;
}

#[tokio::test]
async fn a_retry_leaves_the_row_pending_then_dead_after_the_limit() {
    let Some(db) = Db::open().await else { return };
    let delivery = RecordingDelivery::always(DeliveryOutcome::Retry("provider busy".into()));
    let dispatcher = db.dispatcher(delivery.clone(), RetryPolicy::immediate(3));
    let record = db.record(&Db::request("incident_opened"));
    db.stage(&record).await;

    assert_eq!(db.pass(&dispatcher).await, 0);
    assert_eq!(
        (delivery.calls(), db.pending().await, db.dead().await.len()),
        (1, 1, 0),
        "after the first retry the row waits"
    );
    assert_eq!(db.pass(&dispatcher).await, 0);
    assert_eq!(
        (delivery.calls(), db.pending().await, db.dead().await.len()),
        (2, 1, 0),
        "after the second it still waits"
    );
    assert_eq!(db.pass(&dispatcher).await, 0);
    assert_eq!(delivery.calls(), 3);
    assert_eq!(
        db.pending().await,
        0,
        "the limit was reached: the row is no longer pending"
    );
    let dead = db.dead().await;
    assert_eq!(dead.len(), 1, "it is dead, not published");
    let reason = dead[0].failure_reason.clone().unwrap_or_default();
    assert!(
        reason.contains("gave up after 3 attempts") && reason.contains("provider busy"),
        "{reason}"
    );
    assert_eq!(
        delivery
            .received()
            .iter()
            .map(|n| n.attempt)
            .collect::<Vec<_>>(),
        vec![1, 2, 3],
        "each offer is counted"
    );
    assert!(
        !db.consumed("notification-delivery", record.id).await,
        "a retry never claims the row"
    );

    db.pass(&dispatcher).await;
    assert_eq!(delivery.calls(), 3, "a dead row is not offered again");

    // Once the cause is fixed the row is handed back and delivered.
    let fixed = RecordingDelivery::new();
    let dispatcher = db.dispatcher(fixed.clone(), RetryPolicy::immediate(3));
    assert!(relay::reemit(&db.pool, &db.schema, record.id)
        .await
        .unwrap());
    assert_eq!(db.pass(&dispatcher).await, 1);
    assert_eq!(fixed.calls(), 1);
    assert!(db.consumed("notification-delivery", record.id).await);
    db.drop().await;
}

#[tokio::test]
async fn a_retry_waits_out_its_backoff_before_the_delivery_is_called_again() {
    let Some(db) = Db::open().await else { return };
    let delivery = RecordingDelivery::always(DeliveryOutcome::Retry("rate limited".into()));
    let policy = RetryPolicy {
        max_attempts: 4,
        base_delay: Duration::from_secs(3600),
        max_delay: Duration::from_secs(3600),
    };
    let dispatcher = db.dispatcher(delivery.clone(), policy);
    db.stage(&db.record(&Db::request("incident_opened"))).await;

    db.pass(&dispatcher).await;
    db.pass(&dispatcher).await;
    db.pass(&dispatcher).await;
    assert_eq!(
        delivery.calls(),
        1,
        "inside the wait the row is left pending without calling the delivery"
    );
    assert_eq!((db.pending().await, db.dead().await.len()), (1, 0));
    db.drop().await;
}

#[tokio::test]
async fn a_retry_then_a_delivery_is_delivered_once() {
    let Some(db) = Db::open().await else { return };
    let delivery = RecordingDelivery::answering(vec![DeliveryOutcome::Retry("timeout".into())]);
    let dispatcher = db.dispatcher(delivery.clone(), RetryPolicy::immediate(5));
    let record = db.record(&Db::request("incident_opened"));
    db.stage(&record).await;

    assert_eq!(db.pass(&dispatcher).await, 0);
    assert_eq!(db.pass(&dispatcher).await, 1);
    assert_eq!(db.pass(&dispatcher).await, 0, "nothing is left to offer");
    assert_eq!(delivery.calls(), 2);
    assert_eq!(delivery.received()[1].attempt, 2);
    assert!(db.consumed("notification-delivery", record.id).await);
    assert!(db.dead().await.is_empty());
    db.drop().await;
}

#[tokio::test]
async fn a_reject_dead_letters_with_the_reason() {
    let Some(db) = Db::open().await else { return };
    let delivery =
        RecordingDelivery::always(DeliveryOutcome::Reject("no template named `ghost`".into()));
    let dispatcher = db.dispatcher(delivery.clone(), RetryPolicy::default());
    let record = db.record(&Db::request("ghost"));
    db.stage(&record).await;

    assert_eq!(db.pass(&dispatcher).await, 0);
    assert_eq!(delivery.calls(), 1, "a rejection is not retried");
    assert_eq!(db.pending().await, 0);
    let dead = db.dead().await;
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].id, record.id);
    assert_eq!(
        dead[0].failure_reason.as_deref(),
        Some("no template named `ghost`")
    );
    assert!(
        !db.consumed("notification-delivery", record.id).await,
        "a rejection never claims the row"
    );
    db.drop().await;
}

#[tokio::test]
async fn records_that_are_not_requests_are_ignored_and_a_malformed_request_is_dead() {
    let Some(db) = Db::open().await else { return };
    let delivery = RecordingDelivery::new();
    let dispatcher = db.dispatcher(delivery.clone(), RetryPolicy::default());

    // The sink can share a relay: another event type is accepted and left to its own consumer.
    let other = OutboxRecord::new(
        "IncidentOpened",
        "Incident",
        "inc-1",
        db.company,
        json!({}),
        Utc::now(),
    );
    db.stage(&other).await;
    assert_eq!(db.pass(&dispatcher).await, 1);
    assert_eq!(delivery.calls(), 0);

    let garbage = OutboxRecord::new(
        NotificationRequested::NAME,
        "Incident",
        "inc-2",
        db.company,
        json!({ "nonsense": 1 }),
        Utc::now(),
    );
    db.stage(&garbage).await;
    assert_eq!(db.pass(&dispatcher).await, 0);
    assert_eq!(
        delivery.calls(),
        0,
        "a payload that is not a request never reaches the delivery"
    );
    let dead = db.dead().await;
    assert_eq!(dead.len(), 1);
    assert!(dead[0]
        .failure_reason
        .clone()
        .unwrap_or_default()
        .contains("not a NotificationRequested"));
    db.drop().await;
}

/// A delivery that takes a while, so two relays are inside it together.
struct Slow(RecordingDelivery);

#[async_trait]
impl NotificationDelivery for Slow {
    async fn deliver(&self, notification: &PendingNotification) -> DeliveryOutcome {
        tokio::time::sleep(Duration::from_millis(250)).await;
        self.0.deliver(notification).await
    }
}

#[tokio::test]
async fn two_relays_handing_the_same_row_at_once_deliver_it_once() {
    let Some(db) = Db::open().await else { return };
    let recording = RecordingDelivery::new();
    let first = db.dispatcher(Slow(recording.clone()), RetryPolicy::default());
    let second = db.dispatcher(Slow(recording.clone()), RetryPolicy::default());
    let record = db.record(&Db::request("incident_opened"));

    let (a, b) = tokio::join!(
        first.deliver(record.clone()),
        second.deliver(record.clone())
    );
    assert!(a.is_ok() && b.is_ok(), "{a:?} {b:?}");
    assert_eq!(
        recording.calls(),
        1,
        "the second relay waited on the claim, then found it taken"
    );
    assert!(db.consumed("notification-delivery", record.id).await);
    db.drop().await;
}

#[tokio::test]
async fn deliveries_named_apart_each_handle_the_request_once() {
    let Some(db) = Db::open().await else { return };
    let mail = RecordingDelivery::new().named("mail");
    let mirror = RecordingDelivery::new().named("mirror");
    let by_mail = db.dispatcher(mail.clone(), RetryPolicy::default());
    let by_mirror = db.dispatcher(mirror.clone(), RetryPolicy::default());
    let record = db.record(&Db::request("incident_opened"));

    for _ in 0..2 {
        by_mail.deliver(record.clone()).await.unwrap();
        by_mirror.deliver(record.clone()).await.unwrap();
    }
    assert_eq!((mail.calls(), mirror.calls()), (1, 1));
    assert!(db.consumed("mail", record.id).await && db.consumed("mirror", record.id).await);
    db.drop().await;
}
