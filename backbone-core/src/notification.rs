//! Delivery of `NotificationRequested` rows: the consumer side of the notification contract.
//!
//! A hook trigger's `send_email` / `notify` and a workflow's `send_email` / `notify` /
//! `send_notification` / `send_sms` never tell anyone anything inside the write's transaction: they
//! stage a [`NotificationRequested`](crate::write_hooks::NotificationRequested) row in the
//! module's outbox, which commits with the write. This module is what reads those rows back out:
//!
//! - [`NotificationDelivery`] is the one trait a notification module (mail, push, SMS) implements:
//!   deliver one request, and say whether it was [delivered](DeliveryOutcome::Delivered), should be
//!   [tried again](DeliveryOutcome::Retry) or can [never be delivered](DeliveryOutcome::Reject).
//! - [`NotificationDispatcher`] is the outbox relay's sink: it routes `NotificationRequested`
//!   records to the registered delivery, claims each with `inbox::once` so a redelivered row is not
//!   delivered twice, and maps the outcome onto the relay's own retry and dead-letter semantics.
//! - [`RecordingDelivery`] is a test double a notification module's own tests, and the framework's,
//!   drive the dispatcher with.
//!
//! ```ignore
//! let dispatcher = Arc::new(NotificationDispatcher::new(pool.clone(), "hr", Arc::new(MailDelivery::new(..))));
//! // the relay: every staged event goes to the sink; a record that is not a request is accepted and ignored
//! runner::supervise(pool, RelayConfig::new("hr"), dispatcher.sink(), shutdown);
//! ```
//!
//! # Guarantees
//!
//! **At least once, and one effect when the provider honours the id.** The claim and the delivery
//! share one transaction: it is opened, the `(consumer, event id)` is claimed, the delivery runs, and
//! the transaction commits only when the delivery answered `Delivered`. A redelivery of a delivered
//! row finds the claim and does nothing; two relays handing the same row at once serialize on the
//! claim; a `Retry` or a `Reject` rolls the claim back, so the row is offered again or dead. What no
//! database can do is make a mail provider's send atomic with a commit: a crash after the provider
//! accepted the message and before the commit delivers it again, which is why
//! [`PendingNotification::id`] is handed to the delivery as the idempotency key to give the provider.
//!
//! **Correlation and causation are preserved.** The delivery runs inside the staging write's
//! correlation id (so anything it writes or logs carries it), and [`PendingNotification`] carries the
//! record's correlation id and causation id; [`PendingNotification::id`] is the id to cite as the
//! causation of anything the delivery stages in turn.
//!
//! **Retry is counted here, not in the table.** The relay re-hands a pending row on every pass and
//! keeps no attempt count, so the dispatcher counts [`Retry`](DeliveryOutcome::Retry) results per
//! record and waits between them ([`RetryPolicy`]): within the backoff window a record is left
//! pending without calling the delivery, and after [`RetryPolicy::max_attempts`] the dispatcher
//! reports the record exhausted, which the relay marks dead (`failed_at`, `failure_reason`) and
//! `relay::reemit` hands back once the cause is fixed. The count and the wait live in this
//! process: a restart forgets them, and each instance of a service counts for itself. A `Reject`
//! is dead at once, with the delivery's reason.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use backbone_outbox::{inbox, OutboxError, OutboxRecord};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::write_hooks::{with_correlation_id, NotificationRequested};

/// What a delivery says about one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// The notification was handed to the provider (sent, queued by it, or recorded). The request
    /// is done and will not be offered again.
    Delivered,
    /// The provider could not take it now (a timeout, a 5xx, a rate limit): offer it again later.
    /// The text is the reason, kept for the log and for the dead letter when the retries run out.
    Retry(String),
    /// The request can never be delivered however often it is offered (an unknown template, an
    /// address no provider will accept). It is dead-lettered at once with this reason.
    Reject(String),
}

/// One staged request, as a delivery sees it.
///
/// Built from the outbox record by the dispatcher. Marked `non_exhaustive`: it can gain fields
/// without breaking an implementation, which reads it and never builds it.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PendingNotification {
    /// The outbox row id: unique to this request, and the same on every redelivery. Give it to the
    /// provider as the idempotency key; cite it as the `causation_id` of anything the delivery stages.
    pub id: Uuid,
    /// What was asked: channel, template or body, recipients, variables, and the trigger that asked.
    pub request: NotificationRequested,
    /// The aggregate type the staging write was about.
    pub aggregate_type: String,
    /// The aggregate it was about.
    pub aggregate_id: String,
    /// The company the request belongs to.
    pub company_id: Uuid,
    /// The org unit it was staged under, when one was bound.
    pub org_unit_id: Option<Uuid>,
    /// The correlation id of the write (or request) that staged it.
    pub correlation_id: Option<String>,
    /// The id of what caused the request: the event, trigger or workflow step that staged it.
    pub causation_id: Option<String>,
    /// When the request was staged.
    pub occurred_at: DateTime<Utc>,
    /// Which attempt this is, counting from 1.
    pub attempt: u32,
}

impl PendingNotification {
    /// The request of an outbox record of type [`NotificationRequested::NAME`], as attempt
    /// `attempt`.
    ///
    /// # Errors
    /// The payload is not a [`NotificationRequested`]: nothing offered it again would change that.
    pub fn from_record(record: &OutboxRecord, attempt: u32) -> Result<Self, serde_json::Error> {
        Ok(Self {
            id: record.id,
            request: serde_json::from_value(record.payload.clone())?,
            aggregate_type: record.aggregate_type.clone(),
            aggregate_id: record.aggregate_id.clone(),
            company_id: record.company_id,
            org_unit_id: record.org_unit_id,
            correlation_id: record.correlation_id.clone(),
            causation_id: record.causation_id.clone(),
            occurred_at: record.occurred_at,
            attempt,
        })
    }
}

/// What a notification module implements: perform one request.
///
/// The dispatcher calls it with the request claimed and a database transaction open (see the
/// [module](self) guarantees), so keep it to the provider call: it holds a pool connection until it
/// answers. It must not stage rows on the dispatcher's behalf; to record that something was sent it
/// writes through its own service, inside the correlation the dispatcher bound.
#[async_trait]
pub trait NotificationDelivery: Send + Sync {
    /// The name this delivery claims requests under in the inbox. Two deliveries that must each
    /// handle every request (a mail module and an audit mirror) name themselves apart; a single
    /// delivery keeps the default.
    fn consumer(&self) -> &str {
        "notification-delivery"
    }

    /// Perform `notification`.
    async fn deliver(&self, notification: &PendingNotification) -> DeliveryOutcome;
}

/// How many times, and how far apart, a dispatcher offers a request that answers
/// [`DeliveryOutcome::Retry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// The attempt after which a request that still answers `Retry` is dead-lettered. At least 1.
    pub max_attempts: u32,
    /// The wait after the first `Retry`; each further one doubles it.
    pub base_delay: Duration,
    /// The longest wait between two attempts.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    /// Five attempts, 30 seconds apart at first, doubling to at most 15 minutes.
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay: Duration::from_secs(30),
            max_delay: Duration::from_secs(15 * 60),
        }
    }
}

impl RetryPolicy {
    /// `max_attempts` attempts with no wait between them: the relay's pass interval is the only
    /// spacing. For tests, and for a delivery that does its own pacing.
    pub fn immediate(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            base_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        }
    }

    /// The wait after the `attempts`-th `Retry`.
    fn backoff(&self, attempts: u32) -> Duration {
        let doublings = attempts.saturating_sub(1).min(20);
        self.base_delay
            .saturating_mul(1u32 << doublings)
            .min(self.max_delay)
    }
}

/// What the dispatcher remembers about a record that has answered `Retry`.
struct Attempts {
    made: u32,
    not_before: Instant,
}

/// The outbox relay's sink for `NotificationRequested` rows: see the [module](self) docs.
pub struct NotificationDispatcher {
    pool: backbone_orm::PgPool,
    schema: String,
    delivery: Arc<dyn NotificationDelivery>,
    policy: RetryPolicy,
    retrying: Mutex<HashMap<Uuid, Attempts>>,
}

/// The future a relay sink returns.
pub type SinkFuture = Pin<Box<dyn Future<Output = Result<(), OutboxError>> + Send>>;

impl NotificationDispatcher {
    /// A dispatcher for the requests staged in `schema.outbox_events`, claiming them in
    /// `schema.inbox_consumed` on `pool`, and handing each to `delivery`.
    pub fn new(
        pool: backbone_orm::PgPool,
        schema: impl Into<String>,
        delivery: Arc<dyn NotificationDelivery>,
    ) -> Self {
        Self {
            pool,
            schema: schema.into(),
            delivery,
            policy: RetryPolicy::default(),
            retrying: Mutex::new(HashMap::new()),
        }
    }

    /// Use `policy` for requests that answer `Retry`.
    pub fn with_retry(mut self, policy: RetryPolicy) -> Self {
        self.policy = RetryPolicy {
            max_attempts: policy.max_attempts.max(1),
            ..policy
        };
        self
    }

    /// The sink as a cloneable closure, in the shape `backbone_outbox::runner::supervise` and
    /// `relay::drain_once` take.
    pub fn sink(
        self: &Arc<Self>,
    ) -> impl Fn(OutboxRecord) -> SinkFuture + Clone + Send + Sync + 'static {
        let this = self.clone();
        move |record| {
            let this = this.clone();
            Box::pin(async move { this.deliver(record).await })
        }
    }

    /// The relay's sink: hand `record` to the delivery when it is a notification request.
    ///
    /// A record of any other type is accepted and ignored, so the sink can share a relay with other
    /// consumers. The answer is the relay's: `Ok` marks the row published (delivered, or already
    /// delivered), [`OutboxError::Publish`] leaves it pending for the next pass (a retry, a backoff
    /// not yet over, a database that could not be reached), [`OutboxError::Exhausted`] marks it dead
    /// with the reason (a rejection, the retries used up, a payload that is not a request).
    pub async fn deliver(&self, record: OutboxRecord) -> Result<(), OutboxError> {
        if record.event_type != NotificationRequested::NAME {
            return Ok(());
        }
        let id = record.id;
        let made = {
            let retrying = self.retrying.lock().unwrap_or_else(|e| e.into_inner());
            match retrying.get(&id) {
                // Still inside the wait after its last `Retry`: leave it pending, untouched.
                Some(state) if state.not_before > Instant::now() => {
                    return Err(OutboxError::Publish(format!(
                        "notification {id} waits for its next attempt"
                    )));
                }
                Some(state) => state.made,
                None => 0,
            }
        };

        let notification = match PendingNotification::from_record(&record, made + 1) {
            Ok(n) => n,
            Err(e) => {
                return Err(OutboxError::Exhausted(format!(
                    "the payload is not a NotificationRequested: {e}"
                )))
            }
        };

        // The claim and the delivery share a transaction: committed only when delivered.
        let consumer = self.delivery.consumer();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| OutboxError::Publish(format!("begin: {e}")))?;
        let claimed = inbox::once(&mut *tx, &self.schema, consumer, id)
            .await
            .map_err(|e| OutboxError::Publish(format!("inbox: {e}")))?;
        if !claimed {
            // Delivered before (or being delivered by another relay that committed first).
            tx.rollback().await.ok();
            self.forget(id);
            return Ok(());
        }

        let correlation = record
            .correlation_id
            .clone()
            .unwrap_or_else(|| record.id.to_string());
        let outcome = with_correlation_id(correlation, self.delivery.deliver(&notification)).await;
        match outcome {
            DeliveryOutcome::Delivered => {
                tx.commit()
                    .await
                    .map_err(|e| OutboxError::Publish(format!("commit: {e}")))?;
                self.forget(id);
                Ok(())
            }
            DeliveryOutcome::Retry(reason) => {
                tx.rollback().await.ok();
                let attempts = made + 1;
                if attempts >= self.policy.max_attempts {
                    self.forget(id);
                    tracing::warn!(target: "backbone::notification", %id, attempts, %reason, "notification dead-lettered: retries used up");
                    return Err(OutboxError::Exhausted(format!(
                        "gave up after {attempts} attempts: {reason}"
                    )));
                }
                let wait = self.policy.backoff(attempts);
                self.retrying
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(
                        id,
                        Attempts {
                            made: attempts,
                            not_before: Instant::now() + wait,
                        },
                    );
                tracing::info!(target: "backbone::notification", %id, attempts, wait_secs = wait.as_secs(), %reason, "notification will be retried");
                Err(OutboxError::Publish(reason))
            }
            DeliveryOutcome::Reject(reason) => {
                tx.rollback().await.ok();
                self.forget(id);
                tracing::warn!(target: "backbone::notification", %id, %reason, "notification rejected");
                Err(OutboxError::Exhausted(reason))
            }
        }
    }

    fn forget(&self, id: Uuid) {
        self.retrying
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    }
}

/// A [`NotificationDelivery`] for tests: records every request it is handed and answers from a
/// script.
///
/// Answers `Delivered` once the script runs out, so `RecordingDelivery::new()` delivers everything and
/// `RecordingDelivery::answering(vec![Retry(..), Retry(..), Delivered])` fails twice first.
#[derive(Clone)]
pub struct RecordingDelivery {
    consumer: String,
    received: Arc<Mutex<Vec<PendingNotification>>>,
    script: Arc<Mutex<std::collections::VecDeque<DeliveryOutcome>>>,
    fallback: DeliveryOutcome,
}

impl Default for RecordingDelivery {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordingDelivery {
    /// A delivery that records requests and delivers all of them.
    pub fn new() -> Self {
        Self {
            consumer: "notification-delivery".to_string(),
            received: Arc::default(),
            script: Arc::default(),
            fallback: DeliveryOutcome::Delivered,
        }
    }

    /// A delivery that answers `outcomes` in turn, then `Delivered`.
    pub fn answering(outcomes: Vec<DeliveryOutcome>) -> Self {
        let this = Self::new();
        *this.script.lock().unwrap_or_else(|e| e.into_inner()) = outcomes.into();
        this
    }

    /// A delivery that answers `outcome` to every request.
    pub fn always(outcome: DeliveryOutcome) -> Self {
        Self {
            fallback: outcome,
            ..Self::new()
        }
    }

    /// Claim requests under `consumer` instead of the default name.
    pub fn named(mut self, consumer: impl Into<String>) -> Self {
        self.consumer = consumer.into();
        self
    }

    /// Every request handed to the delivery so far, in order, redeliveries included.
    pub fn received(&self) -> Vec<PendingNotification> {
        self.received
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// How many times the delivery was called.
    pub fn calls(&self) -> usize {
        self.received
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}

#[async_trait]
impl NotificationDelivery for RecordingDelivery {
    fn consumer(&self) -> &str {
        &self.consumer
    }

    async fn deliver(&self, notification: &PendingNotification) -> DeliveryOutcome {
        self.received
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(notification.clone());
        self.script
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
            .unwrap_or_else(|| self.fallback.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn record(event_type: &str, payload: serde_json::Value) -> OutboxRecord {
        OutboxRecord::new(
            event_type,
            "Incident",
            "i-1",
            Uuid::new_v4(),
            payload,
            Utc::now(),
        )
        .with_correlation_id("corr-1")
        .with_causation_id("cause-1")
    }

    fn request() -> serde_json::Value {
        serde_json::to_value(NotificationRequested {
            channel: "email".into(),
            template: Some("incident_opened".into()),
            body: None,
            to: json!(["a@example.test"]),
            data: json!({ "title": "t" }),
            trigger: "Incident.after_create".into(),
        })
        .unwrap()
    }

    #[test]
    fn a_request_is_read_from_its_record_with_its_correlation_and_causation() {
        let rec = record(NotificationRequested::NAME, request());
        let n = PendingNotification::from_record(&rec, 3).unwrap();
        assert_eq!(n.id, rec.id);
        assert_eq!(n.attempt, 3);
        assert_eq!(n.request.channel, "email");
        assert_eq!(n.request.template.as_deref(), Some("incident_opened"));
        assert_eq!(n.correlation_id.as_deref(), Some("corr-1"));
        assert_eq!(n.causation_id.as_deref(), Some("cause-1"));
        assert_eq!(
            (n.aggregate_type.as_str(), n.aggregate_id.as_str()),
            ("Incident", "i-1")
        );
        assert_eq!(n.company_id, rec.company_id);
    }

    #[test]
    fn a_payload_that_is_not_a_request_is_refused() {
        let rec = record(NotificationRequested::NAME, json!({ "nonsense": true }));
        assert!(PendingNotification::from_record(&rec, 1).is_err());
    }

    #[test]
    fn the_wait_doubles_to_a_ceiling() {
        let p = RetryPolicy {
            max_attempts: 9,
            base_delay: Duration::from_secs(10),
            max_delay: Duration::from_secs(70),
        };
        let waits: Vec<u64> = (1..=5).map(|n| p.backoff(n).as_secs()).collect();
        assert_eq!(waits, vec![10, 20, 40, 70, 70]);
        assert_eq!(RetryPolicy::immediate(3).backoff(2), Duration::ZERO);
        // A huge attempt count neither overflows nor panics.
        assert_eq!(p.backoff(u32::MAX), Duration::from_secs(70));
    }

    #[test]
    fn the_default_policy_is_five_attempts_and_a_bounded_wait() {
        let p = RetryPolicy::default();
        assert_eq!(p.max_attempts, 5);
        assert!(p.backoff(5) <= p.max_delay);
    }

    #[tokio::test]
    async fn the_recording_delivery_answers_its_script_then_delivers() {
        let d = RecordingDelivery::answering(vec![
            DeliveryOutcome::Retry("busy".into()),
            DeliveryOutcome::Reject("no such template".into()),
        ]);
        let n =
            PendingNotification::from_record(&record(NotificationRequested::NAME, request()), 1)
                .unwrap();
        assert_eq!(d.deliver(&n).await, DeliveryOutcome::Retry("busy".into()));
        assert_eq!(
            d.deliver(&n).await,
            DeliveryOutcome::Reject("no such template".into())
        );
        assert_eq!(d.deliver(&n).await, DeliveryOutcome::Delivered);
        assert_eq!(d.calls(), 3);
        let always = RecordingDelivery::always(DeliveryOutcome::Retry("down".into())).named("sms");
        assert_eq!(always.consumer(), "sms");
        assert_eq!(
            always.deliver(&n).await,
            DeliveryOutcome::Retry("down".into())
        );
        assert_eq!(
            always.deliver(&n).await,
            DeliveryOutcome::Retry("down".into())
        );
    }
}
