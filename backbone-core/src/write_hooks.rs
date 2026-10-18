//! Write hooks: code that runs inside a write's own transaction, with the change in hand.
//!
//! [`GenericCrudService`](crate::service::GenericCrudService) runs every write of an entity
//! (create, update, patch, delete, restore, hard delete, their bulk forms, and a state-machine
//! transition) inside one database transaction ([`backbone_orm::in_write_tx`]), in this order:
//!
//! 1. the write guard,
//! 2. [`ServiceLifecycle::before_*`](crate::service::ServiceLifecycle),
//! 3. [`WriteHooks::before`],
//! 4. the repository write,
//! 5. [`ServiceLifecycle::after_*`](crate::service::ServiceLifecycle),
//! 6. [`WriteHooks::after`],
//! 7. commit.
//!
//! A failure at any step rolls the whole write back: the row, whatever the hooks wrote and every
//! event they staged. After the commit the service's [`CrudEventPublisher`] keeps its
//! best-effort in-process role, and nothing else.
//!
//! The schema generator emits a `WriteHooks` implementation per model from its declared events
//! (`on:` in a hook file's `events:`), which stages them with [`WriteCtx::stage`]. Hand code can
//! implement the trait too, and compose it with [`with_write_hooks`](crate::service::GenericCrudService::with_write_hooks).
//!
//! # What a hook may rely on
//!
//! - Statements it runs through the ORM's scoped helpers, the repository, or
//!   [`WriteCtx::stage`] ride the write's transaction and see its uncommitted rows.
//! - A connection a hook opens itself (a bare `pool.begin()`) is another transaction: it cannot
//!   see the row being written, and a statement of it that waits for a lock the write holds
//!   waits for a commit that is waiting for the hook.
//! - A hook that holds the transaction open for a slow call (HTTP, a queue) holds a database
//!   connection and the row locks for as long. Stage an event and let the relay do the slow part.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub use backbone_outbox::OutboxRecord;

use crate::persistence::traits::PersistentEntity;
use crate::service::{ServiceError, ServiceResult};

tokio::task_local! {
    static CORRELATION: String;
}

/// Run `f` with `id` as the correlation id of every write it makes: the id the events staged by
/// those writes carry, so one request's rows, events and logs share it. The generic write routes
/// do this from the `x-correlation-id` request header (see
/// [`correlation_middleware`](crate::http::correlation_middleware)).
pub async fn with_correlation_id<F: std::future::Future>(id: impl Into<String>, f: F) -> F::Output {
    CORRELATION.scope(id.into(), f).await
}

/// The correlation id [`with_correlation_id`] bound for the current task.
pub fn current_correlation_id() -> Option<String> {
    CORRELATION.try_with(|id| id.clone()).ok()
}

/// What a write hook knows about the write it runs in.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct WriteCtx {
    /// The correlation id of the request (its `x-correlation-id` header, else the audit
    /// attribution's, else one minted for the write). Staged events carry it.
    pub correlation_id: String,
    /// The id of the event, trigger or workflow step that caused this write, when one did.
    /// Staged events carry it as their `causation_id`.
    pub causation_id: Option<String>,
    /// Who is writing: the request's audit attribution; `None` for a job or an unscoped caller.
    pub actor: Option<String>,
    /// The company the request is scoped to, when it is. Events of an entity with no company of
    /// its own are staged under it.
    pub company_id: Option<Uuid>,
    /// The Postgres schema whose `outbox_events` table [`stage`](Self::stage) writes to, as the
    /// hooks named it ([`WriteHooks::outbox_schema`]).
    pub outbox_schema: Option<String>,
}

impl WriteCtx {
    /// The context of a write made now, in the current task.
    pub fn current(outbox_schema: Option<&str>) -> Self {
        let audit = backbone_orm::audit_context::current_request_audit();
        let actor = audit
            .as_ref()
            .map(|a| a.actor.clone())
            .filter(|a| !a.is_empty());
        let correlation_id = current_correlation_id()
            .or_else(|| audit.map(|a| a.correlation_id).filter(|c| !c.is_empty()))
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        Self {
            correlation_id,
            causation_id: None,
            actor,
            company_id: backbone_orm::current_company(),
            outbox_schema: outbox_schema.map(str::to_string),
        }
    }

    /// The company to stage an event under when the entity names none: the request's, else the
    /// nil uuid. The outbox table requires a value, and under org-unit tenancy the fence is the
    /// org unit the table fills from the acting unit, not this column.
    pub fn company_or_nil(&self) -> Uuid {
        self.company_id.unwrap_or_else(Uuid::nil)
    }

    /// The company an event of `row` is staged under: the row's own `company_id`, else the
    /// request's, else the nil uuid.
    pub fn company_of(&self, row: &Value) -> Uuid {
        row.get("company_id")
            .and_then(Value::as_str)
            .and_then(|id| Uuid::parse_str(id).ok())
            .unwrap_or_else(|| self.company_or_nil())
    }

    /// Stage `record` on the write's transaction. It commits with the write and vanishes if
    /// the write rolls back. The record takes this context's correlation id (and causation id)
    /// unless it already carries one.
    ///
    /// # Errors
    /// A [`ServiceError`] when the hooks named no outbox schema, when no write transaction is
    /// active (the repository has no pool, so the service ran the write without one), or when
    /// the insert fails — which rolls the write back, since a write whose event cannot be staged
    /// must not commit.
    pub async fn stage(&self, mut record: OutboxRecord) -> ServiceResult<()> {
        let Some(schema) = self.outbox_schema.as_deref() else {
            return Err(ServiceError::Internal(
                "an event was staged by hooks that name no outbox schema".into(),
            ));
        };
        if record.correlation_id.is_none() {
            record.correlation_id = Some(self.correlation_id.clone());
        }
        if record.causation_id.is_none() {
            record.causation_id = self.causation_id.clone();
        }
        backbone_outbox::outbox::stage_in_write_tx(schema, &record)
            .await
            .map(|_| ())
            .map_err(|e| {
                ServiceError::Internal(format!("staging {} failed: {e}", record.event_type))
            })
    }
}

/// A fact the module declares it publishes (`events:` in a schema file), as the generated typed
/// struct of its payload.
///
/// The schema generator implements this for every declared event, in the module's
/// `domain::event::declared`. An event with an `on:` trigger is staged by the generated write
/// hooks; one without is staged by hand code, which builds its row with [`record`](Self::record)
/// and stages it in its own transaction (`backbone_outbox::outbox::stage`) or on the write's
/// ([`WriteCtx::stage`]).
pub trait DeclaredEvent: serde::Serialize {
    /// The event type name the outbox row carries (`TicketOpened`).
    const NAME: &'static str;
    /// The declared schema version of the payload.
    const VERSION: i32;
    /// The aggregate the event belongs to (`Ticket`), or `""` when the declaration names none.
    const AGGREGATE: &'static str;

    /// The outbox row of this event for aggregate `aggregate_id`, under the company of `ctx`
    /// (or the nil uuid when the request has none). A fresh event id, now as the time it
    /// occurred, the declared version, and the payload serialized as written.
    fn record(&self, aggregate_id: &str, ctx: &WriteCtx) -> OutboxRecord {
        self.record_for(aggregate_id, ctx.company_or_nil(), ctx)
    }

    /// [`record`](Self::record) for an aggregate that carries its own company.
    fn record_for(&self, aggregate_id: &str, company_id: Uuid, ctx: &WriteCtx) -> OutboxRecord {
        let payload = serde_json::to_value(self).unwrap_or(Value::Null);
        let mut record = OutboxRecord::new(
            Self::NAME,
            Self::AGGREGATE,
            aggregate_id,
            company_id,
            payload,
            chrono::Utc::now(),
        )
        .with_version(Self::VERSION)
        .with_correlation_id(ctx.correlation_id.clone());
        if let Some(cause) = &ctx.causation_id {
            record = record.with_causation_id(cause.clone());
        }
        record
    }
}

/// The payload of a declared event, read out of the row the write stored: the fields listed,
/// by their serialized names, deserialized into the event's struct. A listed field the row
/// lacks is left out, so an optional one reads as `None` and a required one is an error — the
/// generator only lists fields the aggregate has.
pub fn payload_from_row<T: serde::de::DeserializeOwned>(
    row: &Value,
    fields: &[&str],
) -> ServiceResult<T> {
    let mut picked = serde_json::Map::new();
    if let Value::Object(map) = row {
        for field in fields {
            if let Some(value) = map.get(*field) {
                picked.insert((*field).to_string(), value.clone());
            }
        }
    }
    serde_json::from_value(Value::Object(picked)).map_err(|e| {
        ServiceError::Internal(format!(
            "an event payload could not be read from the row: {e}"
        ))
    })
}

/// Create the outbox and inbox tables of `schema` (idempotent), where the events its hooks stage
/// are written. The composing service runs it at startup, with a role that may create tables;
/// the generated module offers it for each schema it stages in (`declared::migrate_outbox`).
pub async fn migrate_outbox(pool: &backbone_orm::PgPool, schema: &str) -> anyhow::Result<()> {
    backbone_outbox::outbox::migrate(pool, schema).await?;
    Ok(())
}

/// The kind of a [`Change`], without its rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// A new row.
    Created,
    /// An existing row changed by a generic write.
    Updated,
    /// An existing row moved by a state-machine verb.
    Transitioned,
    /// A row deleted (soft, or for good).
    Deleted,
    /// A soft-deleted row restored.
    Restored,
}

/// One row change, as a write hook sees it.
///
/// In [`WriteHooks::before`] the row to be stored is mutable: a hook may fill a derived field.
/// In [`WriteHooks::after`] it is the row as stored. Matching on it needs a wildcard arm; kinds
/// may be added.
#[derive(Debug)]
#[non_exhaustive]
pub enum Change<'a, E> {
    /// A row is created.
    Created {
        /// The row to store, or as stored.
        after: &'a mut E,
    },
    /// A row is changed by a generic write (PUT, PATCH or their bulk forms).
    Updated {
        /// The row as it was read.
        before: &'a E,
        /// The row to store, or as stored.
        after: &'a mut E,
    },
    /// A row is moved by a state-machine verb.
    Transitioned {
        /// The row as it was read.
        before: &'a E,
        /// The row to store, or as stored.
        after: &'a mut E,
        /// The verb (`close`, `approve`).
        transition: String,
        /// The state field that moved, in the serialized naming (`status`).
        field: String,
        /// The state the row left.
        from: Option<String>,
        /// The state the row entered.
        to: Option<String>,
    },
    /// A row is deleted: soft-deleted, or removed for good when `hard`.
    Deleted {
        /// The row as it was read.
        before: &'a E,
        /// Whether the row is removed for good (from the trash) rather than moved to it.
        hard: bool,
    },
    /// A soft-deleted row is restored.
    Restored {
        /// The row: as it was in the trash before the write, as restored after it.
        entity: &'a E,
    },
}

impl<'a, E> Change<'a, E> {
    /// Which kind of change this is.
    pub fn kind(&self) -> ChangeKind {
        match self {
            Change::Created { .. } => ChangeKind::Created,
            Change::Updated { .. } => ChangeKind::Updated,
            Change::Transitioned { .. } => ChangeKind::Transitioned,
            Change::Deleted { .. } => ChangeKind::Deleted,
            Change::Restored { .. } => ChangeKind::Restored,
        }
    }

    /// The row as it was read before the write; `None` for a create.
    pub fn before(&self) -> Option<&E> {
        match self {
            Change::Updated { before, .. }
            | Change::Transitioned { before, .. }
            | Change::Deleted { before, .. } => Some(before),
            Change::Created { .. } | Change::Restored { .. } => None,
        }
    }

    /// The row the write stores; `None` for a delete.
    pub fn after(&self) -> Option<&E> {
        match self {
            Change::Created { after }
            | Change::Updated { after, .. }
            | Change::Transitioned { after, .. } => Some(after),
            Change::Restored { entity } => Some(entity),
            Change::Deleted { .. } => None,
        }
    }

    /// The row an event of this change is about: what the write stores, or the row deleted.
    pub fn subject(&self) -> &E {
        match self {
            Change::Deleted { before, .. } => before,
            other => other
                .after()
                .expect("every change but a delete stores a row"),
        }
    }
}

impl<'a, E> Change<'a, E> {
    /// The row to store, mutable: for a hook that fills a field before the write. `None` for a
    /// delete or a restore, which store no new row.
    pub fn after_mut(&mut self) -> Option<&mut E> {
        match self {
            Change::Created { after }
            | Change::Updated { after, .. }
            | Change::Transitioned { after, .. } => Some(&mut **after),
            Change::Deleted { .. } | Change::Restored { .. } => None,
        }
    }

    /// The row as it was read and the row to store, the second mutable: for a hook that fills a
    /// field of an update from what it was. `None` unless the change is an update.
    pub fn before_and_after_mut(&mut self) -> Option<(&E, &mut E)> {
        match self {
            Change::Updated { before, after } | Change::Transitioned { before, after, .. } => {
                Some((*before, &mut **after))
            }
            _ => None,
        }
    }
}

impl<'a, E: serde::Serialize> Change<'a, E> {
    /// Whether the serialized field `name` differs between the row before and after. A create
    /// changes every field it sets (non-null); a delete changes none.
    pub fn field_changed(&self, name: &str) -> bool {
        let after = match self.after().and_then(|a| serde_json::to_value(a).ok()) {
            Some(Value::Object(map)) => map,
            _ => return false,
        };
        let now = after.get(name).filter(|v| !v.is_null());
        match self.before().and_then(|b| serde_json::to_value(b).ok()) {
            Some(Value::Object(before)) => before.get(name).filter(|v| !v.is_null()) != now,
            _ => now.is_some(),
        }
    }
}

impl<'a, E: PersistentEntity> Change<'a, E> {
    /// The change an update writes: a [`Change::Transitioned`] when a state-machine verb performs
    /// it ([`as_transition`](crate::write_guard::as_transition)), else a [`Change::Updated`].
    pub(crate) fn update(before: &'a E, after: &'a mut E) -> Self {
        let Some(transition) = crate::write_guard::current_transition() else {
            return Change::Updated { before, after };
        };
        let before_obj = serde_json::to_value(before).ok();
        let after_obj = serde_json::to_value(&*after).ok();
        let state = |obj: &Option<Value>, field: &str| {
            obj.as_ref()
                .and_then(|o| o.get(field))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        // The state field the verb moved: the first whose value differs, else the entity's first.
        let moved = E::state_fields()
            .iter()
            .find(|f| state(&before_obj, f) != state(&after_obj, f))
            .or_else(|| E::state_fields().first())
            .copied();
        match moved {
            Some(field) => Change::Transitioned {
                from: state(&before_obj, field),
                to: state(&after_obj, field),
                field: field.to_string(),
                transition,
                before,
                after,
            },
            // A verb on an entity with no state field: nothing to say about a move.
            None => Change::Updated { before, after },
        }
    }
}

// ── Hook triggers ───────────────────────────────────────────────────────────────

/// How many trigger runs may be nested: a trigger whose hand method writes another entity runs that
/// entity's triggers inside its own, and so on. The run past this depth is refused, so a cycle of
/// triggers ends in an error instead of a stack.
pub const TRIGGER_MAX_DEPTH: u32 = 8;

/// The violation code of a trigger run refused for nesting too deep.
pub const TRIGGER_DEPTH_EXCEEDED: &str = "trigger_depth_exceeded";

tokio::task_local! {
    static TRIGGER_DEPTH: u32;
}

/// How many trigger runs enclose the current task.
pub fn current_trigger_depth() -> u32 {
    TRIGGER_DEPTH.try_with(|d| *d).unwrap_or(0)
}

/// Run `f`, the triggers of one hook, one level deeper than the run that encloses it. The generated
/// write hooks wrap every trigger they run in this, so writes a trigger makes through other
/// entities' services (whose hooks wrap theirs the same way) count their nesting: the
/// [`TRIGGER_MAX_DEPTH`]th level inside another is refused with the violation
/// [`TRIGGER_DEPTH_EXCEEDED`], which rolls the whole write back.
pub async fn in_trigger<T, F>(f: F) -> ServiceResult<T>
where
    F: std::future::Future<Output = ServiceResult<T>>,
{
    let depth = current_trigger_depth();
    if depth >= TRIGGER_MAX_DEPTH {
        return Err(ServiceError::Violations(vec![crate::violation::Violation::new(
            "",
            TRIGGER_DEPTH_EXCEEDED,
            format!("triggers are nested more than {TRIGGER_MAX_DEPTH} deep: a trigger writes a row whose trigger writes a row, and so on"),
        )
        .with_param("max", TRIGGER_MAX_DEPTH)]));
    }
    TRIGGER_DEPTH.scope(depth + 1, f).await
}

/// A scheduled entry of a model's hook triggers (`schedule:` on a trigger): what a composing
/// service's scheduler runs at `schedule`, with the generated `run_<model>_scheduled`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledTrigger {
    /// The entry's name (its key in the hook file).
    pub name: &'static str,
    /// The schedule as written: cron, `every 5.minutes`, `every 1.day at 03:00`.
    pub schedule: &'static str,
    /// What the entry says it is for.
    pub description: &'static str,
}

/// A request to tell someone something, staged by a hook trigger's `send_email` or `notify`. An
/// effect outside the database is never performed inside the write's transaction: the request is an
/// outbox row (`NotificationRequested`) that commits with the write, and a consumer (a mail or
/// notification module's relay subscriber) performs it, at least once and idempotently.
///
/// The payload is this struct as JSON. `channel` is `email` or `notify`; a `send_email` names a
/// `template` and a `notify` a `template` or a literal `body`; `to` is what the trigger's recipient
/// expression evaluated to (an address, an id, or a list of them); `data` is the template's
/// variables; `trigger` is `<Model>.<key>` of the trigger that asked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationRequested {
    /// `email` or `notify`.
    pub channel: String,
    /// The template to render, when the request names one.
    pub template: Option<String>,
    /// The literal text of a `notify`, with its `#{field}` already filled in.
    pub body: Option<String>,
    /// The recipients, as evaluated.
    pub to: Value,
    /// The template variables, as evaluated.
    pub data: Value,
    /// The trigger that asked: `<Model>.<key>`.
    pub trigger: String,
}

impl NotificationRequested {
    /// The event type of the outbox row.
    pub const NAME: &'static str = "NotificationRequested";
    /// The payload version.
    pub const VERSION: i32 = 1;

    /// The outbox row of this request, about `aggregate` (`User`) `aggregate_id`, under `company_id`
    /// (see [`WriteCtx::company_of`]), carrying the write's correlation and causation ids.
    pub fn record(
        &self,
        aggregate: &str,
        aggregate_id: &str,
        company_id: Uuid,
        ctx: &WriteCtx,
    ) -> OutboxRecord {
        let mut record = OutboxRecord::new(
            Self::NAME,
            aggregate,
            aggregate_id,
            company_id,
            serde_json::to_value(self).unwrap_or(Value::Null),
            chrono::Utc::now(),
        )
        .with_version(Self::VERSION)
        .with_correlation_id(ctx.correlation_id.clone());
        if let Some(cause) = &ctx.causation_id {
            record = record.with_causation_id(cause.clone());
        }
        record
    }
}

/// Code that runs inside a write's transaction, before and after the repository write.
///
/// Both methods default to doing nothing. An error from either rolls the write back and answers
/// as the service answers any failed write.
#[async_trait]
pub trait WriteHooks<E: PersistentEntity>: Send + Sync {
    /// The Postgres schema whose `outbox_events` table [`WriteCtx::stage`] writes to. `None`
    /// (the default) means these hooks stage nothing.
    fn outbox_schema(&self) -> Option<&str> {
        None
    }

    /// Before the repository writes `change`: may refuse it, and may fill fields of the row to
    /// be stored. Runs after the guard and [`ServiceLifecycle::before_*`](crate::service::ServiceLifecycle).
    async fn before(&self, ctx: &WriteCtx, change: &mut Change<'_, E>) -> ServiceResult<()> {
        let _ = (ctx, change);
        Ok(())
    }

    /// After the repository wrote `change`, still inside the transaction: the place to stage
    /// events and to write rows that must commit with it. Runs after
    /// [`ServiceLifecycle::after_*`](crate::service::ServiceLifecycle).
    async fn after(&self, ctx: &WriteCtx, change: &Change<'_, E>) -> ServiceResult<()> {
        let _ = (ctx, change);
        Ok(())
    }
}

/// The hooks of a service that has none: every write runs in its transaction and nothing more.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoWriteHooks;

#[async_trait]
impl<E: PersistentEntity> WriteHooks<E> for NoWriteHooks {}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Row {
        id: String,
        status: String,
        note: Option<String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        deleted_at: Option<DateTime<Utc>>,
    }

    impl PersistentEntity for Row {
        fn state_fields() -> &'static [&'static str] {
            &["status"]
        }
        fn entity_id(&self) -> String {
            self.id.clone()
        }
        fn set_entity_id(&mut self, id: String) {
            self.id = id;
        }
        fn created_at(&self) -> Option<DateTime<Utc>> {
            Some(self.created_at)
        }
        fn set_created_at(&mut self, ts: DateTime<Utc>) {
            self.created_at = ts;
        }
        fn updated_at(&self) -> Option<DateTime<Utc>> {
            Some(self.updated_at)
        }
        fn set_updated_at(&mut self, ts: DateTime<Utc>) {
            self.updated_at = ts;
        }
        fn deleted_at(&self) -> Option<DateTime<Utc>> {
            self.deleted_at
        }
        fn set_deleted_at(&mut self, ts: Option<DateTime<Utc>>) {
            self.deleted_at = ts;
        }
    }

    fn row(status: &str, note: Option<&str>) -> Row {
        Row {
            id: "1".into(),
            status: status.into(),
            note: note.map(str::to_string),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
        }
    }

    #[tokio::test]
    async fn an_update_is_a_transition_only_inside_a_verb() {
        let before = row("open", None);
        let mut after = row("closed", None);
        assert_eq!(
            Change::update(&before, &mut after).kind(),
            ChangeKind::Updated
        );

        let kind = crate::write_guard::as_transition("close", async {
            let mut after = row("closed", None);
            match Change::update(&before, &mut after) {
                Change::Transitioned {
                    transition,
                    field,
                    from,
                    to,
                    ..
                } => (transition, field, from, to),
                other => panic!("expected a transition, got {:?}", other.kind()),
            }
        })
        .await;
        assert_eq!(
            kind,
            (
                "close".to_string(),
                "status".to_string(),
                Some("open".to_string()),
                Some("closed".to_string())
            )
        );
    }

    #[test]
    fn a_field_counts_as_changed_only_when_its_value_differs() {
        let before = row("open", Some("a"));
        let mut same = row("open", Some("a"));
        let mut note_changed = row("open", Some("b"));
        let mut created = row("open", None);
        assert!(!Change::Updated {
            before: &before,
            after: &mut same
        }
        .field_changed("note"));
        assert!(Change::Updated {
            before: &before,
            after: &mut note_changed
        }
        .field_changed("note"));
        assert!(!Change::Updated {
            before: &before,
            after: &mut note_changed
        }
        .field_changed("status"));
        // A create sets what it sets: a null field is not a change, a filled one is.
        assert!(Change::Created {
            after: &mut created
        }
        .field_changed("status"));
        assert!(!Change::Created {
            after: &mut row("open", None)
        }
        .field_changed("note"));
    }

    #[test]
    fn a_hook_may_change_the_row_to_store_but_not_the_row_it_read() {
        let before = row("open", None);
        let mut after = row("open", None);
        let mut change = Change::Updated {
            before: &before,
            after: &mut after,
        };
        {
            let (old, entity) = change
                .before_and_after_mut()
                .expect("an update has both rows");
            assert_eq!(old.status, "open");
            entity.note = Some("filled".into());
        }
        assert_eq!(change.after().unwrap().note.as_deref(), Some("filled"));
        assert_eq!(change.before().unwrap().note, None);

        let mut created = row("open", None);
        let mut change = Change::Created {
            after: &mut created,
        };
        assert!(
            change.before_and_after_mut().is_none(),
            "a create has no row before it"
        );
        change.after_mut().unwrap().status = "new".into();
        assert_eq!(change.subject().status, "new");

        let gone = row("open", None);
        let mut change = Change::Deleted {
            before: &gone,
            hard: false,
        };
        assert!(change.after_mut().is_none(), "a delete stores no row");
    }

    #[tokio::test]
    async fn trigger_runs_nest_to_a_cap_and_the_run_past_it_is_refused_with_a_code() {
        assert_eq!(current_trigger_depth(), 0);
        // Each run is one level inside the last; the ninth level inside another is refused.
        fn nest(
            level: u32,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ServiceResult<u32>> + Send>>
        {
            Box::pin(async move {
                in_trigger(async move {
                    assert_eq!(current_trigger_depth(), level + 1);
                    match nest(level + 1).await {
                        Ok(deepest) => Ok(deepest),
                        Err(ServiceError::Violations(v)) if v[0].code == TRIGGER_DEPTH_EXCEEDED => {
                            Ok(level + 1)
                        }
                        Err(other) => Err(other),
                    }
                })
                .await
            })
        }
        let deepest = nest(0).await.unwrap();
        assert_eq!(
            deepest, TRIGGER_MAX_DEPTH,
            "{TRIGGER_MAX_DEPTH} nested runs ran, the next was refused"
        );
        assert_eq!(
            current_trigger_depth(),
            0,
            "the depth does not leak out of the scope"
        );

        // The refusal itself: a violation with the code and the cap.
        let refused = TRIGGER_DEPTH
            .scope(TRIGGER_MAX_DEPTH, async {
                in_trigger(async { Ok(()) }).await
            })
            .await
            .unwrap_err();
        match refused {
            ServiceError::Violations(v) => {
                assert_eq!(v[0].code, TRIGGER_DEPTH_EXCEEDED);
                assert_eq!(v[0].params["max"], TRIGGER_MAX_DEPTH);
            }
            other => panic!("expected a violation, got {other:?}"),
        }
    }

    #[test]
    fn a_notification_request_is_an_outbox_row_about_the_aggregate_with_the_writes_ids() {
        let ctx = WriteCtx {
            correlation_id: "req-1".into(),
            causation_id: Some("cause-1".into()),
            actor: None,
            company_id: None,
            outbox_schema: Some("billing".into()),
        };
        let request = NotificationRequested {
            channel: "email".into(),
            template: Some("welcome".into()),
            body: None,
            to: serde_json::json!("a@b.test"),
            data: serde_json::json!({ "name": "Ann" }),
            trigger: "User.after_create".into(),
        };
        let record = request.record("User", "u-1", Uuid::from_u128(3), &ctx);
        assert_eq!(
            (
                record.event_type.as_str(),
                record.aggregate_type.as_str(),
                record.aggregate_id.as_str(),
                record.version
            ),
            ("NotificationRequested", "User", "u-1", 1)
        );
        assert_eq!(record.company_id, Uuid::from_u128(3));
        assert_eq!(record.correlation_id.as_deref(), Some("req-1"));
        assert_eq!(record.causation_id.as_deref(), Some("cause-1"));
        assert_eq!(record.payload["channel"], "email");
        assert_eq!(record.payload["to"], "a@b.test");
        assert_eq!(record.payload["trigger"], "User.after_create");
        assert_eq!(
            serde_json::from_value::<NotificationRequested>(record.payload).unwrap(),
            request
        );
    }

    #[tokio::test]
    async fn the_correlation_id_comes_from_the_request_scope_else_is_minted() {
        let minted = WriteCtx::current(None);
        assert!(!minted.correlation_id.is_empty());
        let scoped =
            with_correlation_id("req-7", async { WriteCtx::current(Some("billing")) }).await;
        assert_eq!(scoped.correlation_id, "req-7");
        assert_eq!(scoped.outbox_schema.as_deref(), Some("billing"));
    }

    async fn correlation_seen_by(header: Option<&str>) -> String {
        use axum::{body::Body, http::Request, routing::get, Router};
        use tower::ServiceExt;
        let app = Router::new()
            .route(
                "/",
                get(|| async { WriteCtx::current(None).correlation_id }),
            )
            .layer(axum::middleware::from_fn(
                crate::http::correlation_middleware,
            ));
        let mut req = Request::builder().uri("/");
        if let Some(h) = header {
            req = req.header("x-correlation-id", h);
        }
        let res = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn the_request_header_is_the_correlation_id_when_usable() {
        assert_eq!(correlation_seen_by(Some("req-abc-1")).await, "req-abc-1");
        for unusable in ["", "has space", &"x".repeat(129)] {
            let id = correlation_seen_by(Some(unusable)).await;
            assert!(Uuid::parse_str(&id).is_ok(), "{unusable:?} gave {id}");
        }
        assert!(Uuid::parse_str(&correlation_seen_by(None).await).is_ok());
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct RowOpened {
        id: String,
        note: Option<String>,
    }

    impl DeclaredEvent for RowOpened {
        const NAME: &'static str = "RowOpened";
        const VERSION: i32 = 3;
        const AGGREGATE: &'static str = "Row";
    }

    #[test]
    fn a_payload_is_picked_from_the_row_by_field_name() {
        let row = serde_json::json!({ "id": "1", "note": null, "status": "open" });
        let picked: RowOpened = payload_from_row(&row, &["id", "note"]).unwrap();
        assert_eq!(
            picked,
            RowOpened {
                id: "1".into(),
                note: None
            }
        );
        // An optional field the row lacks reads as none; a required one is an error.
        let thin = serde_json::json!({ "id": "1" });
        assert_eq!(
            payload_from_row::<RowOpened>(&thin, &["id", "note"])
                .unwrap()
                .note,
            None
        );
        assert!(payload_from_row::<RowOpened>(&thin, &["note"]).is_err());
    }

    #[test]
    fn a_declared_event_builds_its_outbox_row_from_its_declaration_and_the_context() {
        let ctx = WriteCtx {
            correlation_id: "req-9".into(),
            causation_id: Some("cause-1".into()),
            actor: None,
            company_id: Some(Uuid::from_u128(7)),
            outbox_schema: Some("billing".into()),
        };
        let event = RowOpened {
            id: "1".into(),
            note: Some("n".into()),
        };
        let record = event.record("agg-1", &ctx);
        assert_eq!(
            (
                record.event_type.as_str(),
                record.aggregate_type.as_str(),
                record.aggregate_id.as_str(),
                record.version
            ),
            ("RowOpened", "Row", "agg-1", 3)
        );
        assert_eq!(record.company_id, Uuid::from_u128(7));
        assert_eq!(record.correlation_id.as_deref(), Some("req-9"));
        assert_eq!(record.causation_id.as_deref(), Some("cause-1"));
        assert_eq!(record.payload["note"], "n");
        // An aggregate that carries its own company is staged under it.
        let own = Uuid::from_u128(9);
        let row = serde_json::json!({ "company_id": own.to_string() });
        assert_eq!(ctx.company_of(&row), own);
        assert_eq!(ctx.company_of(&serde_json::json!({})), Uuid::from_u128(7));
    }
}
