//! The workflow engine: instances, steps, waits, timers and compensation over Postgres.
//!
//! An event starts an instance ([`WorkflowEngine::on_event`]); the instance runs step by step, each
//! step in **one** write transaction ([`backbone_orm::in_write_tx`]) that holds the step's own
//! writes (through the entity services, so guards, triggers and declared events all apply), the
//! step-log row and the instance update, so a step either happened with its record or did not
//! happen. A `wait` stores what it waits for and stops; a matching event ([`WorkflowEngine::on_event`])
//! or the sweep ([`WorkflowEngine::sweep`], for timeouts, retries and deadlines) moves it on.
//!
//! Everything is at-least-once and idempotent: a start is `UNIQUE (workflow, trigger_event_id)`,
//! a resume is claimed with [`backbone_outbox::inbox::once`] in the resuming transaction, a step
//! that crashed leaves the instance `running` on the step it was about to run, and the sweep picks
//! it up again. Every statement runs under the event's company, so the row-level fence on both
//! tables decides which instances an event or a request can see.
//!
//! See [ADR-0035](../../../../docs/handbook/adr/0035-workflow-engine.md) for the decisions.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use backbone_orm::company_scope::with_company_scope;
use backbone_orm::{current_write_tx, in_write_tx, PgPool, WriteTx};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

use super::expr::{self, Funcs, Scope};
use super::schema::valid_schema;
use super::spec::{
    Backoff, Call, CompStep, FailureSpec, Outcome, RetrySpec, StepKind, StepSpec, Terminal,
    WorkflowDefinition, WorkflowSpec,
};
use crate::service::ServiceError;
use crate::write_hooks::{with_correlation_id, NotificationRequested, OutboxRecord, WriteCtx};

/// How the engine paces retries and finds work nobody is driving.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Attempts of a step that fails for a transient reason, when neither the step nor the
    /// workflow says.
    pub default_attempts: u32,
    /// The first wait of an exponential backoff.
    pub backoff_base: Duration,
    /// The longest wait of an exponential backoff.
    pub backoff_max: Duration,
    /// How long a `running` instance may sit untouched before the sweep drives it again.
    pub stall_after: Duration,
    /// How many due instances one sweep takes.
    pub sweep_batch: i64,
    /// Steps one run of an instance may take before it is failed (a cycle in the workflow).
    pub max_steps: i32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            default_attempts: 3,
            backoff_base: Duration::from_secs(2),
            backoff_max: Duration::from_secs(300),
            stall_after: Duration::from_secs(60),
            sweep_batch: 50,
            max_steps: 5000,
        }
    }
}

/// What can go wrong outside a step: the database, a service, a spec.
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    /// A database error.
    #[error("workflow database error: {0}")]
    Db(#[from] sqlx::Error),
    /// A service refused something the engine asked.
    #[error("workflow service error: {0}")]
    Service(#[from] ServiceError),
    /// The engine or a spec is misconfigured.
    #[error("workflow configuration error: {0}")]
    Config(String),
}

/// What [`WorkflowEngine::on_event`] did with an event.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EventReport {
    /// Instances this event started.
    pub started: Vec<Uuid>,
    /// Starts this event asked for that already existed (a replay): nothing was done for them.
    pub duplicates: usize,
    /// Waiting instances this event resumed.
    pub resumed: Vec<Uuid>,
}

/// What [`WorkflowEngine::sweep`] did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SweepReport {
    /// Waits whose timeout passed and took their `on_timeout` branch.
    pub timed_out: Vec<Uuid>,
    /// Instances past their deadline, now `expired`.
    pub expired: Vec<Uuid>,
    /// Failed steps whose backoff passed and ran again.
    pub retried: Vec<Uuid>,
    /// Instances found `running` with nobody driving them, driven on.
    pub recovered: Vec<Uuid>,
    /// Instances found `compensating`, compensation finished.
    pub compensated: Vec<Uuid>,
}

impl SweepReport {
    /// How many instances the sweep touched.
    pub fn total(&self) -> usize {
        self.timed_out.len()
            + self.expired.len()
            + self.retried.len()
            + self.recovered.len()
            + self.compensated.len()
    }
}

/// An instance as stored.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InstanceRow {
    /// The instance id.
    pub id: Uuid,
    /// The workflow's name.
    pub workflow: String,
    /// The workflow's version when the instance started.
    pub version: i32,
    /// The company that owns it.
    pub company_id: Uuid,
    /// `running`, `waiting`, `completed`, `failed`, `expired`, `compensating`, `compensated` or `cancelled`.
    pub status: String,
    /// The step it is on (the next to run, or the wait it sits in).
    pub current_step: Option<String>,
    /// Its variables.
    pub context: Value,
    /// The event that started it.
    pub trigger_event_id: Uuid,
    /// The correlation id every row it stages carries.
    pub correlation_id: Option<String>,
    /// When it expires.
    pub deadline_at: Option<DateTime<Utc>>,
    /// The event a wait is waiting for.
    pub wait_event: Option<String>,
    /// What the event must match.
    pub wait_key: Option<String>,
    /// When a wait times out, or a failed step runs again.
    pub wait_due_at: Option<DateTime<Utc>>,
    /// Failed attempts of the current step.
    pub attempts: i32,
    /// The last failure.
    pub last_error: Option<String>,
    /// When it was created.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    pub updated_at: DateTime<Utc>,
}

impl InstanceRow {
    fn is_final(&self) -> bool {
        matches!(
            self.status.as_str(),
            "completed" | "failed" | "expired" | "compensated" | "cancelled"
        )
    }
}

/// One row of the step log.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StepRow {
    /// Position in the instance's log.
    pub seq: i32,
    /// The step, or `compensate:<name>`.
    pub step: String,
    /// `ok`, `skipped`, `waiting`, `resumed`, `timed_out`, `terminal`, `failed`, `expired`,
    /// `cancelled`, `compensated` or `compensation_failed`.
    pub outcome: String,
    /// The failure, when there was one.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailKind {
    Business,
    Transient,
    TimedOut,
}

#[derive(Debug)]
struct StepFailure {
    step: String,
    kind: FailKind,
    message: String,
}

enum RunError {
    Failure(StepFailure),
    Wf(WorkflowError),
}

impl From<sqlx::Error> for RunError {
    fn from(e: sqlx::Error) -> Self {
        RunError::Wf(WorkflowError::Db(e))
    }
}

impl From<WorkflowError> for RunError {
    fn from(e: WorkflowError) -> Self {
        RunError::Wf(e)
    }
}

enum StepEnd {
    Advanced,
    Paused,
    Finished,
    Compensate,
    Gone,
}

fn fail(step: &str, kind: FailKind, message: impl Into<String>) -> RunError {
    RunError::Failure(StepFailure {
        step: step.to_string(),
        kind,
        message: message.into(),
    })
}

fn classify(e: &ServiceError) -> FailKind {
    match e {
        ServiceError::NotFound
        | ServiceError::AlreadyExists(_)
        | ServiceError::Validation(_)
        | ServiceError::Violations(_) => FailKind::Business,
        _ => FailKind::Transient,
    }
}

/// Functions a workflow declares, called through its definition.
struct DefFuncs<'a> {
    def: &'a dyn WorkflowDefinition,
    ctx: &'a WriteCtx,
}

#[async_trait]
impl Funcs for DefFuncs<'_> {
    async fn call(&self, name: &str, args: &[Value]) -> Result<Value, String> {
        self.def
            .function(self.ctx, name, args)
            .await
            .map_err(|e| e.to_string())
    }
}

/// The engine of one module's `executor: engine` workflows.
pub struct WorkflowEngine {
    pool: PgPool,
    schema: String,
    defs: Vec<Arc<dyn WorkflowDefinition>>,
    cfg: EngineConfig,
}

const INSTANCE_COLUMNS: &str = "id, workflow, version, company_id, status, current_step, context, trigger_event_id, \
     correlation_id, deadline_at, wait_event, wait_key, wait_due_at, attempts, last_error, created_at, updated_at";

impl WorkflowEngine {
    /// An engine over `pool` whose tables live in `schema` (the module's schema), running `defs`.
    ///
    /// # Panics
    /// When `schema` is not a plain lowercase identifier.
    pub fn new(pool: PgPool, schema: &str, defs: Vec<Arc<dyn WorkflowDefinition>>) -> Self {
        assert!(
            valid_schema(schema),
            "`{schema}` is not a plain schema name"
        );
        Self {
            pool,
            schema: schema.to_string(),
            defs,
            cfg: EngineConfig::default(),
        }
    }

    /// Replace the pacing.
    pub fn with_config(mut self, cfg: EngineConfig) -> Self {
        self.cfg = cfg;
        self
    }

    /// The event types the engine reacts to: every trigger and every awaited event. A composing
    /// service forwards the outbox records of these to [`on_event`](Self::on_event).
    pub fn subscribed_events(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for def in &self.defs {
            for e in def.spec().events() {
                if !out.iter().any(|x| x == e) {
                    out.push(e.to_string());
                }
            }
        }
        out
    }

    fn def_named(&self, name: &str) -> Option<&Arc<dyn WorkflowDefinition>> {
        self.defs.iter().find(|d| d.spec().name == name)
    }

    // ── events ────────────────────────────────────────────────────────────────────────────

    /// Handle an event from the outbox relay: start the workflows it triggers and resume the
    /// instances waiting for it, all under the event's company. Safe to call again for the same
    /// event: a start that already exists and a resume already claimed do nothing.
    ///
    /// # Errors
    /// A database or configuration error. The event is then not consumed; deliver it again.
    pub async fn on_event(&self, record: &OutboxRecord) -> Result<EventReport, WorkflowError> {
        Box::pin(self.handle_event(record)).await
    }

    async fn handle_event(&self, record: &OutboxRecord) -> Result<EventReport, WorkflowError> {
        let mut report = EventReport::default();
        for def in &self.defs {
            if def.spec().trigger_event == record.event_type {
                match Box::pin(self.start(def, record)).await? {
                    Some(id) => report.started.push(id),
                    None => report.duplicates += 1,
                }
            }
        }
        for def in &self.defs {
            let waits = def.spec().steps.iter().any(
                |s| matches!(&s.kind, StepKind::Wait { event: Some(e), .. } if *e == record.event_type),
            );
            if waits {
                report
                    .resumed
                    .extend(Box::pin(self.resume(def, record)).await?);
            }
        }
        // Drive what this event started or resumed until it pauses.
        let to_drive: Vec<(Arc<dyn WorkflowDefinition>, Uuid)> = {
            let mut v = Vec::new();
            for id in report.started.iter().chain(report.resumed.iter()) {
                if let Some(def) = self.def_for_instance(record.company_id, *id).await? {
                    v.push((def, *id));
                }
            }
            v
        };
        for (def, id) in to_drive {
            Box::pin(self.drive(&def, record.company_id, id)).await?;
        }
        Ok(report)
    }

    async fn def_for_instance(
        &self,
        company: Uuid,
        id: Uuid,
    ) -> Result<Option<Arc<dyn WorkflowDefinition>>, WorkflowError> {
        let name: Option<String> = self
            .in_tx(company, async {
                let tx = write_tx()?;
                let mut conn = tx.conn().await;
                Ok(sqlx::query_scalar(&format!(
                    "SELECT workflow FROM {}.bb_workflow_instances WHERE id = $1",
                    self.schema
                ))
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?)
            })
            .await?;
        Ok(name.and_then(|n| self.def_named(&n).cloned()))
    }

    /// `work` in one write transaction under `company`'s scope; a failure of the work is an error.
    async fn in_tx<T>(
        &self,
        company: Uuid,
        work: impl std::future::Future<Output = Result<T, RunError>>,
    ) -> Result<T, WorkflowError> {
        match with_company_scope(Some(company), in_write_tx(&self.pool, work)).await {
            Ok(v) => Ok(v),
            Err(RunError::Wf(e)) => Err(e),
            Err(RunError::Failure(f)) => Err(WorkflowError::Config(f.message)),
        }
    }

    /// Start an instance of `def` for `record`. `None` when this event already started one.
    async fn start(
        &self,
        def: &Arc<dyn WorkflowDefinition>,
        record: &OutboxRecord,
    ) -> Result<Option<Uuid>, WorkflowError> {
        let spec = def.spec();
        let first = spec
            .steps
            .first()
            .ok_or_else(|| WorkflowError::Config(format!("workflow `{}` has no steps", spec.name)))?
            .name
            .clone();
        let id = Uuid::new_v4();
        let correlation = record
            .correlation_id
            .clone()
            .unwrap_or_else(|| record.id.to_string());
        let created: Result<Option<Uuid>, RunError> = with_company_scope(
            Some(record.company_id),
            in_write_tx(&self.pool, async {
                let ctx = self.ctx_for(record.company_id, &correlation, record.id);
                let funcs = DefFuncs { def: &**def, ctx: &ctx };
                let mut context = serde_json::Map::new();
                for (k, v) in &spec.context {
                    context.insert(k.clone(), v.clone());
                }
                let done = HashSet::new();
                let initial = Value::Object(context.clone());
                let scope = Scope {
                    context: &initial,
                    event: &record.payload,
                    result: &Value::Null,
                    error: &Value::Null,
                    config: &Value::Null,
                    completed: &done,
                    now: Utc::now(),
                };
                for (k, e) in &spec.extract {
                    let v = expr::eval(e, &scope, &funcs)
                        .await
                        .map_err(|m| fail("start", FailKind::Business, format!("extract `{k}`: {m}")))?;
                    context.insert(k.clone(), v);
                }
                // The latest event, for steps that read `event.*` after the start.
                context.insert("_event".into(), record.payload.clone());
                let deadline = match &spec.timeout {
                    Some(d) => Some(
                        expr::render_duration(d, &scope, &funcs)
                            .await
                            .map_err(|m| fail("start", FailKind::Business, format!("timeout: {m}")))?,
                    ),
                    None => None,
                };
                let tx = write_tx()?;
                let row: Option<Uuid> = {
                    let mut conn = tx.conn().await;
                    sqlx::query_scalar(&format!(
                    "INSERT INTO {}.bb_workflow_instances
                       (id, workflow, version, company_id, status, current_step, context, trigger_event_id,
                        correlation_id, deadline_at)
                     VALUES ($1, $2, $3, $4, 'running', $5, $6, $7, $8,
                             CASE WHEN $9::double precision IS NULL THEN NULL
                                  ELSE now() + make_interval(secs => $9::double precision) END)
                     ON CONFLICT (workflow, trigger_event_id) DO NOTHING
                     RETURNING id",
                    self.schema
                ))
                .bind(id)
                .bind(&spec.name)
                .bind(spec.version)
                .bind(record.company_id)
                .bind(&first)
                .bind(Value::Object(context))
                .bind(record.id)
                .bind(&correlation)
                .bind(deadline.map(|s| s as f64))
                .fetch_optional(&mut *conn)
                .await?
                };
                // A workflow without persistence runs end to end here, in this transaction: a
                // failure takes the instance with it, and the event is delivered again.
                if row.is_some() && !spec.persistence {
                    with_correlation_id(correlation.clone(), self.run_inline(def, id)).await?;
                }
                Ok(row)
            }),
        )
        .await;
        match created {
            Ok(row) => Ok(row),
            Err(RunError::Wf(e)) => Err(e),
            Err(RunError::Failure(f)) => Err(WorkflowError::Config(format!(
                "workflow `{}` could not start: {}",
                spec.name, f.message
            ))),
        }
    }

    /// Resume the instances of `def` waiting for `record`.
    async fn resume(
        &self,
        def: &Arc<dyn WorkflowDefinition>,
        record: &OutboxRecord,
    ) -> Result<Vec<Uuid>, WorkflowError> {
        let spec = def.spec();
        // The event-side key of each distinct set of fields a wait of this event matches on.
        let mut keys: Vec<String> = Vec::new();
        for step in &spec.steps {
            if let StepKind::Wait {
                event: Some(e),
                key,
                ..
            } = &step.kind
            {
                if *e == record.event_type {
                    let values: Vec<Value> = key
                        .iter()
                        .map(|k| {
                            record
                                .payload
                                .get(&k.event_field)
                                .cloned()
                                .unwrap_or(Value::Null)
                        })
                        .collect();
                    let k = expr::wait_key(&values);
                    if !keys.contains(&k) {
                        keys.push(k);
                    }
                }
            }
        }
        let consumer = format!("workflow:{}", spec.name);
        let correlation = record
            .correlation_id
            .clone()
            .unwrap_or_else(|| record.id.to_string());
        let resumed: Result<Vec<Uuid>, RunError> = with_company_scope(
            Some(record.company_id),
            in_write_tx(&self.pool, async {
                let tx = write_tx()?;
                let claimed = {
                    let mut conn = tx.conn().await;
                    backbone_outbox::inbox::once(&mut *conn, &self.schema, &consumer, record.id)
                        .await
                        .map_err(|e| WorkflowError::Config(format!("inbox: {e}")))?
                };
                if !claimed {
                    return Ok(Vec::new());
                }
                let rows: Vec<InstanceRow> = {
                    let mut conn = tx.conn().await;
                    sqlx::query_as(&format!(
                        "SELECT {INSTANCE_COLUMNS} FROM {}.bb_workflow_instances
                          WHERE workflow = $1 AND status = 'waiting' AND wait_event = $2 AND wait_key = ANY($3)
                          ORDER BY created_at, id
                            FOR UPDATE",
                        self.schema
                    ))
                    .bind(&spec.name)
                    .bind(&record.event_type)
                    .bind(&keys)
                    .fetch_all(&mut *conn)
                    .await?
                };
                let mut out = Vec::new();
                for inst in rows {
                    let Some(step) = inst.current_step.as_deref().and_then(|s| spec.step(s)) else { continue };
                    let StepKind::Wait { key, on_event, .. } = &step.kind else { continue };
                    // The instance's own wait must match this event, not just share a key text.
                    let values: Vec<Value> =
                        key.iter().map(|k| record.payload.get(&k.event_field).cloned().unwrap_or(Value::Null)).collect();
                    if inst.wait_key.as_deref() != Some(expr::wait_key(&values).as_str()) {
                        continue;
                    }
                    let outcome = on_event.clone().unwrap_or_default();
                    let ctx = self.ctx_for(inst.company_id, &correlation, inst.trigger_event_id);
                    Box::pin(self.leave_wait(def, &tx, &inst, &step.name, &outcome, "resumed", Some(&record.payload), &ctx)).await?;
                    out.push(inst.id);
                }
                Ok(out)
            }),
        )
        .await;
        match resumed {
            Ok(ids) => Ok(ids),
            Err(RunError::Wf(e)) => Err(e),
            Err(RunError::Failure(f)) => Err(WorkflowError::Config(format!(
                "workflow `{}` could not resume on `{}`: {}",
                spec.name, record.event_type, f.message
            ))),
        }
    }

    /// Take a waiting instance out of its wait along `outcome`: apply the outcome's `set`, log the
    /// wait step as `outcome_label`, and make the instance `running` on the next step.
    #[allow(clippy::too_many_arguments)]
    async fn leave_wait(
        &self,
        def: &Arc<dyn WorkflowDefinition>,
        tx: &Arc<WriteTx>,
        inst: &InstanceRow,
        step_name: &str,
        outcome: &Outcome,
        outcome_label: &str,
        event: Option<&Value>,
        ctx: &WriteCtx,
    ) -> Result<(), RunError> {
        let spec = def.spec();
        let completed = self.completed(tx, inst.id).await?;
        let funcs = DefFuncs { def: &**def, ctx };
        let null = Value::Null;
        let scope = Scope {
            context: &inst.context,
            event: event.unwrap_or(&null),
            result: inst.context.get("result").unwrap_or(&null),
            error: inst.context.get("error").unwrap_or(&null),
            config: &null,
            completed: &completed,
            now: Utc::now(),
        };
        let (mut context, next) =
            Box::pin(self.apply_outcome(spec, step_name, inst, outcome, &scope, &funcs, None))
                .await?;
        if let (Some(payload), Some(map)) = (event, context.as_object_mut()) {
            map.insert("_event".into(), payload.clone());
        }
        let seq = self.next_seq(tx, inst.id).await?;
        self.log_step(tx, inst, seq, step_name, outcome_label, None)
            .await?;
        let mut conn = tx.conn().await;
        sqlx::query(&format!(
            "UPDATE {}.bb_workflow_instances
                SET status = 'running', current_step = $2, context = $3, wait_event = NULL, wait_key = NULL,
                    wait_due_at = NULL, attempts = 0, last_error = NULL, updated_at = now()
              WHERE id = $1",
            self.schema
        ))
        .bind(inst.id)
        .bind(next)
        .bind(context)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    // ── running ───────────────────────────────────────────────────────────────────────────

    /// Run the instance's steps until it pauses (a wait, a retry's backoff) or ends. Safe to call
    /// for an instance somebody else is driving: each step takes the row lock and reads the
    /// instance again.
    ///
    /// # Errors
    /// A database error; the instance stays on the step it was about to run, and the sweep
    /// drives it again.
    pub async fn run(&self, company: Uuid, id: Uuid) -> Result<(), WorkflowError> {
        match self.def_for_instance(company, id).await? {
            Some(def) => Box::pin(self.drive(&def, company, id)).await,
            None => Ok(()),
        }
    }

    async fn drive(
        &self,
        def: &Arc<dyn WorkflowDefinition>,
        company: Uuid,
        id: Uuid,
    ) -> Result<(), WorkflowError> {
        if !def.spec().persistence {
            // Ran to its end inside the transaction that started it.
            return Ok(());
        }
        for _ in 0..self.cfg.max_steps.max(1) {
            match Box::pin(self.step_once(def, company, id)).await? {
                StepEnd::Advanced => continue,
                StepEnd::Compensate => {
                    Box::pin(self.compensate_company(def, company, id)).await?;
                    return Ok(());
                }
                StepEnd::Paused | StepEnd::Finished | StepEnd::Gone => return Ok(()),
            }
        }
        self.fail_runaway(def, company, id).await
    }

    /// One step, in one transaction; a failure is handled in another.
    async fn step_once(
        &self,
        def: &Arc<dyn WorkflowDefinition>,
        company: Uuid,
        id: Uuid,
    ) -> Result<StepEnd, WorkflowError> {
        let correlation = self.correlation_of(company, id).await?;
        let ran: Result<StepEnd, RunError> = with_company_scope(
            Some(company),
            with_correlation_id(
                correlation,
                in_write_tx(&self.pool, Box::pin(self.run_one(def, id))),
            ),
        )
        .await;
        match ran {
            Ok(end) => Ok(end),
            Err(RunError::Wf(e)) => Err(e),
            Err(RunError::Failure(f)) => Box::pin(self.after_failure(def, company, id, f)).await,
        }
    }

    async fn correlation_of(&self, company: Uuid, id: Uuid) -> Result<String, WorkflowError> {
        let found: Option<Option<String>> = self
            .in_tx(company, async {
                let tx = write_tx()?;
                let mut conn = tx.conn().await;
                Ok(sqlx::query_scalar(&format!(
                    "SELECT correlation_id FROM {}.bb_workflow_instances WHERE id = $1",
                    self.schema
                ))
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?)
            })
            .await?;
        Ok(found.flatten().unwrap_or_else(|| id.to_string()))
    }

    fn ctx_for(&self, company: Uuid, correlation: &str, cause: Uuid) -> WriteCtx {
        WriteCtx {
            correlation_id: correlation.to_string(),
            causation_id: Some(cause.to_string()),
            actor: None,
            company_id: Some(company),
            outbox_schema: Some(self.schema.clone()),
        }
    }

    async fn lock(&self, tx: &WriteTx, id: Uuid) -> Result<Option<InstanceRow>, sqlx::Error> {
        let mut conn = tx.conn().await;
        sqlx::query_as(&format!(
            "SELECT {INSTANCE_COLUMNS} FROM {}.bb_workflow_instances WHERE id = $1 FOR UPDATE",
            self.schema
        ))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
    }

    async fn completed(&self, tx: &WriteTx, id: Uuid) -> Result<HashSet<String>, sqlx::Error> {
        let mut conn = tx.conn().await;
        let rows: Vec<String> = sqlx::query_scalar(&format!(
            "SELECT DISTINCT step FROM {}.bb_workflow_steps
              WHERE instance_id = $1 AND outcome IN ('ok', 'resumed', 'timed_out')",
            self.schema
        ))
        .bind(id)
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows.into_iter().collect())
    }

    async fn next_seq(&self, tx: &WriteTx, id: Uuid) -> Result<i32, sqlx::Error> {
        let mut conn = tx.conn().await;
        sqlx::query_scalar(&format!(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM {}.bb_workflow_steps WHERE instance_id = $1",
            self.schema
        ))
        .bind(id)
        .fetch_one(&mut *conn)
        .await
    }

    async fn log_step(
        &self,
        tx: &WriteTx,
        inst: &InstanceRow,
        seq: i32,
        step: &str,
        outcome: &str,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let mut conn = tx.conn().await;
        sqlx::query(&format!(
            "INSERT INTO {}.bb_workflow_steps (instance_id, company_id, seq, step, outcome, error)
             VALUES ($1, $2, $3, $4, $5, $6)",
            self.schema
        ))
        .bind(inst.id)
        .bind(inst.company_id)
        .bind(seq)
        .bind(step)
        .bind(outcome)
        .bind(error)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Apply an outcome: evaluate its `set` over `scope`, write the keys into a copy of the
    /// context, and name the next step (which must exist).
    #[allow(clippy::too_many_arguments)]
    async fn apply_outcome(
        &self,
        spec: &WorkflowSpec,
        step: &str,
        inst: &InstanceRow,
        outcome: &Outcome,
        scope: &Scope<'_>,
        funcs: &dyn Funcs,
        result: Option<&Value>,
    ) -> Result<(Value, String), RunError> {
        let mut context = inst.context.clone();
        if !context.is_object() {
            context = json!({});
        }
        let map = context.as_object_mut().expect("an object");
        if let Some(r) = result {
            map.insert("result".into(), r.clone());
        }
        let mut sets = Vec::new();
        for (k, tpl) in &outcome.set {
            let v = expr::render(tpl, scope, funcs)
                .await
                .map_err(|m| fail(step, FailKind::Business, format!("set `{k}`: {m}")))?;
            sets.push((k.clone(), v));
        }
        for (k, v) in sets {
            map.insert(k, v);
        }
        let next = outcome
            .next
            .clone()
            .ok_or_else(|| fail(step, FailKind::Business, "the step names no next step"))?;
        if spec.step(&next).is_none() {
            return Err(fail(
                step,
                FailKind::Business,
                format!("the next step `{next}` does not exist"),
            ));
        }
        Ok((context, next))
    }

    /// Run the instance's current step inside the ambient write transaction.
    async fn run_one(
        &self,
        def: &Arc<dyn WorkflowDefinition>,
        id: Uuid,
    ) -> Result<StepEnd, RunError> {
        let tx = write_tx()?;
        let Some(inst) = self.lock(&tx, id).await? else {
            return Ok(StepEnd::Gone);
        };
        if inst.status != "running" {
            return Ok(StepEnd::Paused);
        }
        let spec = def.spec();
        let step_name = inst.current_step.clone().ok_or_else(|| {
            fail(
                "(none)",
                FailKind::Business,
                "the instance has no current step",
            )
        })?;
        let step = spec.step(&step_name).ok_or_else(|| {
            fail(
                &step_name,
                FailKind::Business,
                "the workflow has no such step",
            )
        })?;
        let correlation = inst
            .correlation_id
            .clone()
            .unwrap_or_else(|| inst.id.to_string());
        let ctx = self.ctx_for(inst.company_id, &correlation, inst.trigger_event_id);
        let funcs = DefFuncs {
            def: &**def,
            ctx: &ctx,
        };
        let completed = self.completed(&tx, id).await?;
        let seq = self.next_seq(&tx, id).await?;
        let null = Value::Null;
        let scope = Scope {
            context: &inst.context,
            event: inst.context.get("_event").unwrap_or(&null),
            result: inst.context.get("result").unwrap_or(&null),
            error: inst.context.get("error").unwrap_or(&null),
            config: &null,
            completed: &completed,
            now: Utc::now(),
        };

        // A guard that does not hold skips the step.
        if let Some(guard) = &step.guard {
            let holds = expr::holds(guard, &scope, &funcs)
                .await
                .map_err(|m| fail(&step_name, FailKind::Business, format!("guard: {m}")))?;
            if !holds {
                let outcome = match &step.kind {
                    StepKind::Action { on_success, .. }
                    | StepKind::Transition { on_success, .. } => on_success,
                    _ => {
                        return Err(fail(
                            &step_name,
                            FailKind::Business,
                            "only an action or a transition has a guard",
                        ))
                    }
                };
                let skip = Outcome {
                    set: Vec::new(),
                    next: outcome.next.clone(),
                };
                let (context, next) = Box::pin(
                    self.apply_outcome(spec, &step_name, &inst, &skip, &scope, &funcs, None),
                )
                .await?;
                self.log_step(&tx, &inst, seq, &step_name, "skipped", None)
                    .await?;
                self.advance(&tx, &inst, &next, &context).await?;
                return Ok(StepEnd::Advanced);
            }
        }

        match &step.kind {
            StepKind::Action {
                action,
                entity,
                id: id_tpl,
                params,
                on_success,
                on_timeout,
            } => {
                let rendered =
                    Box::pin(self.render_params(&step_name, params, &scope, &funcs)).await?;
                let record_id =
                    match id_tpl {
                        Some(t) => Some(expr::render(t, &scope, &funcs).await.map_err(|m| {
                            fail(&step_name, FailKind::Business, format!("id: {m}"))
                        })?),
                        None => rendered.get("id").cloned(),
                    }
                    .filter(|v| !v.is_null())
                    .map(|v| expr::text_of(&v));
                let result = self
                    .perform_action(
                        def,
                        &ctx,
                        step,
                        action,
                        entity.as_deref(),
                        record_id,
                        None,
                        &rendered,
                        &inst,
                        on_timeout.is_some(),
                    )
                    .await?;
                let (context, next) = Box::pin(self.apply_outcome_with_result(
                    spec, &step_name, &inst, on_success, &scope, &funcs, &result,
                ))
                .await?;
                self.log_step(&tx, &inst, seq, &step_name, "ok", None)
                    .await?;
                self.advance(&tx, &inst, &next, &context).await?;
                Ok(StepEnd::Advanced)
            }
            StepKind::Transition {
                entity,
                id: id_tpl,
                verb,
                params,
                on_success,
            } => {
                let rendered =
                    Box::pin(self.render_params(&step_name, params, &scope, &funcs)).await?;
                let record_id = expr::render(id_tpl, &scope, &funcs)
                    .await
                    .map_err(|m| fail(&step_name, FailKind::Business, format!("id: {m}")))?;
                if record_id.is_null() {
                    return Err(fail(
                        &step_name,
                        FailKind::Business,
                        "the transition's id is empty",
                    ));
                }
                let call = Call {
                    step: &step_name,
                    action: "transition",
                    entity: Some(entity),
                    id: Some(expr::text_of(&record_id)),
                    verb: Some(verb),
                    params: &rendered,
                    context: &inst.context,
                };
                let result = Box::pin(self.call_def(def, &ctx, step, &call, false)).await?;
                let (context, next) = Box::pin(self.apply_outcome_with_result(
                    spec, &step_name, &inst, on_success, &scope, &funcs, &result,
                ))
                .await?;
                self.log_step(&tx, &inst, seq, &step_name, "ok", None)
                    .await?;
                self.advance(&tx, &inst, &next, &context).await?;
                Ok(StepEnd::Advanced)
            }
            StepKind::Condition { branches } => {
                for branch in branches {
                    let taken = match &branch.when {
                        Some(w) => expr::holds(w, &scope, &funcs).await.map_err(|m| {
                            fail(&step_name, FailKind::Business, format!("condition: {m}"))
                        })?,
                        None => true,
                    };
                    if taken {
                        let outcome = Outcome {
                            set: branch.set.clone(),
                            next: Some(branch.next.clone()),
                        };
                        let (context, next) = Box::pin(self.apply_outcome(
                            spec, &step_name, &inst, &outcome, &scope, &funcs, None,
                        ))
                        .await?;
                        self.log_step(&tx, &inst, seq, &step_name, "ok", None)
                            .await?;
                        self.advance(&tx, &inst, &next, &context).await?;
                        return Ok(StepEnd::Advanced);
                    }
                }
                Err(fail(
                    &step_name,
                    FailKind::Business,
                    "no branch of the condition holds and it has no else",
                ))
            }
            StepKind::Wait {
                event,
                key,
                timeout,
                ..
            } => {
                if !spec.persistence {
                    return Err(fail(
                        &step_name,
                        FailKind::Business,
                        "a workflow without persistence cannot wait",
                    ));
                }
                let wait_key = match event {
                    Some(_) => {
                        let mut values = Vec::new();
                        for k in key {
                            values.push(expr::eval(&k.context, &scope, &funcs).await.map_err(
                                |m| fail(&step_name, FailKind::Business, format!("wait key: {m}")),
                            )?);
                        }
                        Some(expr::wait_key(&values))
                    }
                    None => None,
                };
                let due = match timeout {
                    Some(d) => Some(expr::render_duration(d, &scope, &funcs).await.map_err(
                        |m| fail(&step_name, FailKind::Business, format!("timeout: {m}")),
                    )?),
                    None => None,
                };
                if event.is_none() && due.is_none() {
                    return Err(fail(
                        &step_name,
                        FailKind::Business,
                        "a wait names neither an event nor a timeout",
                    ));
                }
                self.log_step(&tx, &inst, seq, &step_name, "waiting", None)
                    .await?;
                let mut conn = tx.conn().await;
                sqlx::query(&format!(
                    "UPDATE {}.bb_workflow_instances
                        SET status = 'waiting', wait_event = $2, wait_key = $3,
                            wait_due_at = CASE WHEN $4::double precision IS NULL THEN NULL
                                               ELSE now() + make_interval(secs => $4::double precision) END,
                            attempts = 0, last_error = NULL, updated_at = now()
                      WHERE id = $1",
                    self.schema
                ))
                .bind(id)
                .bind(event.as_deref())
                .bind(wait_key)
                .bind(due.map(|s| s as f64))
                .execute(&mut *conn)
                .await?;
                Ok(StepEnd::Paused)
            }
            StepKind::Terminal {
                status,
                reason,
                emit,
                compensate,
                result,
            } => {
                let mut context = inst.context.clone();
                if let Some(tpl) = result {
                    let value = expr::render(tpl, &scope, &funcs).await.map_err(|m| {
                        fail(&step_name, FailKind::Business, format!("result: {m}"))
                    })?;
                    if let Some(map) = context.as_object_mut() {
                        map.insert("result".into(), value);
                    }
                }
                if let Some(e) = emit {
                    let data = expr::render(&e.data, &scope, &funcs)
                        .await
                        .map_err(|m| fail(&step_name, FailKind::Business, format!("emit: {m}")))?;
                    self.stage_event(&ctx, &spec.name, inst.id, &e.event, data)
                        .await
                        .map_err(|e| fail(&step_name, classify(&e), format!("emit: {e}")))?;
                }
                let reason_text = match reason {
                    Some(t) => Some(expr::text_of(
                        &expr::render(t, &scope, &funcs).await.map_err(|m| {
                            fail(&step_name, FailKind::Business, format!("reason: {m}"))
                        })?,
                    )),
                    None => None,
                };
                let compensating =
                    *compensate && *status == Terminal::Failed && !spec.compensation.is_empty();
                let final_status = if compensating {
                    "compensating"
                } else {
                    match status {
                        Terminal::Completed => "completed",
                        Terminal::Failed => "failed",
                        Terminal::Expired => "expired",
                        Terminal::Cancelled => "cancelled",
                    }
                };
                self.log_step(
                    &tx,
                    &inst,
                    seq,
                    &step_name,
                    "terminal",
                    reason_text
                        .as_deref()
                        .filter(|_| *status != Terminal::Completed),
                )
                .await?;
                let mut conn = tx.conn().await;
                sqlx::query(&format!(
                    "UPDATE {}.bb_workflow_instances
                        SET status = $2, current_step = $3, wait_event = NULL, wait_key = NULL, wait_due_at = NULL,
                            last_error = COALESCE($4, last_error), context = $5, updated_at = now()
                      WHERE id = $1",
                    self.schema
                ))
                .bind(id)
                .bind(final_status)
                .bind(&step_name)
                .bind(reason_text.filter(|_| *status == Terminal::Failed))
                .bind(context)
                .execute(&mut *conn)
                .await?;
                Ok(if compensating {
                    StepEnd::Compensate
                } else {
                    StepEnd::Finished
                })
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn apply_outcome_with_result(
        &self,
        spec: &WorkflowSpec,
        step: &str,
        inst: &InstanceRow,
        outcome: &Outcome,
        scope: &Scope<'_>,
        funcs: &dyn Funcs,
        result: &Value,
    ) -> Result<(Value, String), RunError> {
        // `result` inside the outcome's expressions is this step's own result.
        let inner = Scope {
            context: scope.context,
            event: scope.event,
            result,
            error: scope.error,
            config: scope.config,
            completed: scope.completed,
            now: scope.now,
        };
        Box::pin(self.apply_outcome(spec, step, inst, outcome, &inner, funcs, Some(result))).await
    }

    async fn advance(
        &self,
        tx: &WriteTx,
        inst: &InstanceRow,
        next: &str,
        context: &Value,
    ) -> Result<(), sqlx::Error> {
        let mut conn = tx.conn().await;
        sqlx::query(&format!(
            "UPDATE {}.bb_workflow_instances
                SET current_step = $2, context = $3, attempts = 0, last_error = NULL,
                    wait_event = NULL, wait_key = NULL, wait_due_at = NULL, updated_at = now()
              WHERE id = $1",
            self.schema
        ))
        .bind(inst.id)
        .bind(next)
        .bind(context)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    async fn render_params(
        &self,
        step: &str,
        params: &[(String, expr::Tpl)],
        scope: &Scope<'_>,
        funcs: &dyn Funcs,
    ) -> Result<Value, RunError> {
        let mut map = serde_json::Map::new();
        for (k, tpl) in params {
            map.insert(
                k.clone(),
                expr::render(tpl, scope, funcs)
                    .await
                    .map_err(|m| fail(step, FailKind::Business, format!("param `{k}`: {m}")))?,
            );
        }
        Ok(Value::Object(map))
    }

    /// Perform an action: an engine action here, anything else through the definition.
    #[allow(clippy::too_many_arguments)]
    async fn perform_action(
        &self,
        def: &Arc<dyn WorkflowDefinition>,
        ctx: &WriteCtx,
        step: &StepSpec,
        action: &str,
        entity: Option<&str>,
        record_id: Option<String>,
        verb: Option<&str>,
        params: &Value,
        inst: &InstanceRow,
        has_on_timeout: bool,
    ) -> Result<Value, RunError> {
        let workflow = def.spec().name.as_str();
        match action {
            "audit_log" => {
                tracing::info!(
                    target: "backbone_core::workflow::audit",
                    workflow,
                    instance = %inst.id,
                    step = %step.name,
                    correlation_id = %ctx.correlation_id,
                    audit = %params,
                    "workflow audit"
                );
                Ok(Value::Null)
            }
            "emit_event" => {
                let Some(event) = params.get("event").and_then(Value::as_str) else {
                    return Err(fail(
                        &step.name,
                        FailKind::Business,
                        "`emit_event` needs an `event` parameter",
                    ));
                };
                let data = params.get("data").cloned().unwrap_or(Value::Null);
                self.stage_event(ctx, workflow, inst.id, event, data)
                    .await
                    .map_err(|e| fail(&step.name, classify(&e), format!("emit_event: {e}")))?;
                Ok(json!({ "event": event }))
            }
            "send_email" | "notify" | "send_notification" | "send_sms" => {
                let channel = match action {
                    "send_email" => "email".to_string(),
                    "send_sms" => "sms".to_string(),
                    "send_notification" => "notify".to_string(),
                    _ => params
                        .get("channel")
                        .and_then(Value::as_str)
                        .unwrap_or("notify")
                        .to_string(),
                };
                let mut data = params.get("data").cloned().unwrap_or_else(|| json!({}));
                if let (Value::Object(map), Value::Object(all)) = (&mut data, params) {
                    for (k, v) in all {
                        if !matches!(
                            k.as_str(),
                            "data" | "template" | "to" | "recipients" | "channel" | "body"
                        ) {
                            map.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                    }
                }
                let request = NotificationRequested {
                    channel,
                    template: params
                        .get("template")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    body: params
                        .get("body")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    to: params
                        .get("to")
                        .or_else(|| params.get("recipients"))
                        .cloned()
                        .unwrap_or(Value::Null),
                    data,
                    trigger: format!("{workflow}.{}", step.name),
                };
                ctx.stage(request.record(workflow, &inst.id.to_string(), inst.company_id, ctx))
                    .await
                    .map_err(|e| fail(&step.name, classify(&e), format!("{action}: {e}")))?;
                Ok(Value::Null)
            }
            other => {
                let call = Call {
                    step: &step.name,
                    action: other,
                    entity,
                    id: record_id,
                    verb,
                    params,
                    context: &inst.context,
                };
                Box::pin(self.call_def(def, ctx, step, &call, has_on_timeout)).await
            }
        }
    }

    /// Call the definition, bounded by the step's `timeout`.
    async fn call_def(
        &self,
        def: &Arc<dyn WorkflowDefinition>,
        ctx: &WriteCtx,
        step: &StepSpec,
        call: &Call<'_>,
        _has_on_timeout: bool,
    ) -> Result<Value, RunError> {
        let run = def.perform(ctx, call);
        let answer = match step.call_timeout {
            Some(secs) => {
                match tokio::time::timeout(Duration::from_secs(secs.max(0) as u64), run).await {
                    Ok(a) => a,
                    Err(_) => {
                        return Err(fail(
                            &step.name,
                            FailKind::TimedOut,
                            format!("`{}` took longer than {secs}s", call.action),
                        ));
                    }
                }
            }
            None => run.await,
        };
        answer.map_err(|e| fail(&step.name, classify(&e), e.to_string()))
    }

    async fn stage_event(
        &self,
        ctx: &WriteCtx,
        workflow: &str,
        instance: Uuid,
        event: &str,
        data: Value,
    ) -> Result<(), ServiceError> {
        let company = ctx.company_or_nil();
        ctx.stage(OutboxRecord::new(
            event,
            workflow,
            instance.to_string(),
            company,
            data,
            Utc::now(),
        ))
        .await
    }

    // ── failure ───────────────────────────────────────────────────────────────────────────

    /// A step's transaction rolled back. Record the failure and decide: route, retry later, or fail.
    async fn after_failure(
        &self,
        def: &Arc<dyn WorkflowDefinition>,
        company: Uuid,
        id: Uuid,
        failure: StepFailure,
    ) -> Result<StepEnd, WorkflowError> {
        let spec = def.spec();
        let decided: Result<StepEnd, RunError> = with_company_scope(
            Some(company),
            in_write_tx(&self.pool, async {
                let tx = write_tx()?;
                let Some(inst) = self.lock(&tx, id).await? else { return Ok(StepEnd::Gone) };
                if inst.status != "running" {
                    return Ok(StepEnd::Paused);
                }
                let step = spec.step(&failure.step);
                let policy: FailureSpec = step.and_then(|s| s.on_failure.clone()).unwrap_or_default();
                let on_timeout = step.and_then(|s| match &s.kind {
                    StepKind::Action { on_timeout, .. } => on_timeout.clone(),
                    _ => None,
                });
                let seq = self.next_seq(&tx, id).await?;
                self.log_step(&tx, &inst, seq, &failure.step, "failed", Some(&failure.message)).await?;

                let retry: RetrySpec = policy
                    .retry
                    .clone()
                    .or_else(|| spec.retry.clone())
                    .unwrap_or(RetrySpec { max_attempts: self.cfg.default_attempts, backoff: Backoff::Exponential });
                let attempts = inst.attempts + 1;
                let correlation = inst.correlation_id.clone().unwrap_or_else(|| inst.id.to_string());
                let ctx = self.ctx_for(inst.company_id, &correlation, inst.trigger_event_id);
                let funcs = DefFuncs { def: &**def, ctx: &ctx };

                // A hand call that outlasted its step's timeout takes `on_timeout` when there is one.
                let route: Option<Outcome> = match failure.kind {
                    FailKind::TimedOut if on_timeout.is_some() => on_timeout,
                    FailKind::Business => policy.next.clone().map(|n| Outcome { set: Vec::new(), next: Some(n) }),
                    FailKind::Transient | FailKind::TimedOut => {
                        if attempts < retry.max_attempts as i32 {
                            let delay = match retry.backoff {
                                Backoff::Fixed(s) => s.max(0) as f64,
                                Backoff::Exponential => {
                                    let base = self.cfg.backoff_base.as_secs_f64();
                                    (base * 2f64.powi(attempts - 1)).min(self.cfg.backoff_max.as_secs_f64())
                                }
                            };
                            let mut conn = tx.conn().await;
                            sqlx::query(&format!(
                                "UPDATE {}.bb_workflow_instances
                                    SET attempts = $2, last_error = $3,
                                        wait_due_at = now() + make_interval(secs => $4::double precision),
                                        updated_at = now()
                                  WHERE id = $1",
                                self.schema
                            ))
                            .bind(id)
                            .bind(attempts)
                            .bind(&failure.message)
                            .bind(delay)
                            .execute(&mut *conn)
                            .await?;
                            return Ok(StepEnd::Paused);
                        }
                        policy.on_exhausted.clone().or_else(|| policy.next.clone().map(|n| Outcome { set: Vec::new(), next: Some(n) }))
                    }
                };

                if let Some(outcome) = route {
                    let completed = self.completed(&tx, id).await?;
                    let mut context = inst.context.clone();
                    if let Some(map) = context.as_object_mut() {
                        map.insert("error".into(), Value::String(failure.message.clone()));
                    }
                    let carrier = InstanceRow { context, ..inst.clone() };
                    let null = Value::Null;
                    let err_text = Value::String(failure.message.clone());
                    let scope = Scope {
                        context: &carrier.context,
                        event: inst.context.get("_event").unwrap_or(&null),
                        result: carrier.context.get("result").unwrap_or(&null),
                        error: &err_text,
                        config: &null,
                        completed: &completed,
                        now: Utc::now(),
                    };
                    let (context, next) =
                        Box::pin(self.apply_outcome(spec, &failure.step, &carrier, &outcome, &scope, &funcs, None)).await?;
                    self.advance(&tx, &carrier, &next, &context).await?;
                    return Ok(StepEnd::Advanced);
                }

                // Nowhere to go: the instance fails, and compensates what completed.
                let status = if spec.compensation.is_empty() { "failed" } else { "compensating" };
                let mut conn = tx.conn().await;
                sqlx::query(&format!(
                    "UPDATE {}.bb_workflow_instances
                        SET status = $2, attempts = $3, last_error = $4, wait_event = NULL, wait_key = NULL,
                            wait_due_at = NULL, updated_at = now()
                      WHERE id = $1",
                    self.schema
                ))
                .bind(id)
                .bind(status)
                .bind(attempts)
                .bind(&failure.message)
                .execute(&mut *conn)
                .await?;
                Ok(if status == "compensating" { StepEnd::Compensate } else { StepEnd::Finished })
            }),
        )
        .await;
        match decided {
            Ok(end) => Ok(end),
            Err(RunError::Wf(e)) => Err(e),
            Err(RunError::Failure(f)) => Err(WorkflowError::Config(format!(
                "the failure of step `{}` could not be routed: {}",
                failure.step, f.message
            ))),
        }
    }

    async fn fail_runaway(
        &self,
        def: &Arc<dyn WorkflowDefinition>,
        company: Uuid,
        id: Uuid,
    ) -> Result<(), WorkflowError> {
        let message = format!(
            "more than {} steps ran without the instance pausing or ending",
            self.cfg.max_steps
        );
        let end = self
            .after_failure(
                def,
                company,
                id,
                StepFailure {
                    step: "(runaway)".into(),
                    kind: FailKind::Business,
                    message,
                },
            )
            .await?;
        if matches!(end, StepEnd::Compensate) {
            Box::pin(self.compensate_company(def, company, id)).await?;
        }
        Ok(())
    }

    // ── compensation ──────────────────────────────────────────────────────────────────────

    /// Run the compensation steps of a failed instance whose condition holds, last declared first,
    /// each in a transaction of its own, skipping any already compensated. Ends `compensated`, or
    /// `failed` with the reason when a step could not be undone. Calling it again does only what is
    /// left to do.
    ///
    /// # Errors
    /// A database error between steps; call it again.
    pub async fn compensate(&self, company: Uuid, id: Uuid) -> Result<(), WorkflowError> {
        match self.def_for_instance(company, id).await? {
            Some(def) => Box::pin(self.compensate_company(&def, company, id)).await,
            None => Ok(()),
        }
    }

    async fn compensate_company(
        &self,
        def: &Arc<dyn WorkflowDefinition>,
        company: Uuid,
        id: Uuid,
    ) -> Result<(), WorkflowError> {
        let spec = def.spec();
        let mut failures: Vec<String> = Vec::new();
        for comp in spec.compensation.iter().rev() {
            let label = format!("compensate:{}", comp.name);
            let ran: Result<Option<()>, RunError> = with_company_scope(
                Some(company),
                in_write_tx(
                    &self.pool,
                    Box::pin(self.compensate_one(def, id, comp, &label)),
                ),
            )
            .await;
            match ran {
                Ok(_) => {}
                Err(RunError::Wf(e)) => return Err(e),
                Err(RunError::Failure(f)) => {
                    failures.push(format!("{}: {}", comp.name, f.message));
                    // Record the failure in a transaction of its own (the step's rolled back).
                    let _: Result<(), RunError> = with_company_scope(
                        Some(company),
                        in_write_tx(&self.pool, async {
                            let tx = write_tx()?;
                            if let Some(inst) = self.lock(&tx, id).await? {
                                let seq = self.next_seq(&tx, id).await?;
                                self.log_step(
                                    &tx,
                                    &inst,
                                    seq,
                                    &label,
                                    "compensation_failed",
                                    Some(&f.message),
                                )
                                .await?;
                            }
                            Ok(())
                        }),
                    )
                    .await;
                }
            }
        }
        let outcome: Result<(), RunError> = with_company_scope(
            Some(company),
            in_write_tx(&self.pool, async {
                let tx = write_tx()?;
                let Some(inst) = self.lock(&tx, id).await? else {
                    return Ok(());
                };
                if !matches!(inst.status.as_str(), "compensating" | "failed") {
                    return Ok(());
                }
                // Anything a previous run could not undo stays visible.
                let pending: Vec<String> = {
                    let mut conn = tx.conn().await;
                    sqlx::query_scalar(&format!(
                        "SELECT DISTINCT step FROM {s}.bb_workflow_steps
                          WHERE instance_id = $1 AND outcome = 'compensation_failed'
                            AND step NOT IN (SELECT step FROM {s}.bb_workflow_steps
                                              WHERE instance_id = $1 AND outcome = 'compensated')",
                        s = self.schema
                    ))
                    .bind(id)
                    .fetch_all(&mut *conn)
                    .await?
                };
                // `pending` already holds this run's failures (logged before this transaction);
                // `failures` carries their messages for the reason, and covers a log write that failed.
                let (status, error) = if failures.is_empty() && pending.is_empty() {
                    ("compensated", None)
                } else if !failures.is_empty() {
                    (
                        "failed",
                        Some(format!("compensation incomplete: {}", failures.join("; "))),
                    )
                } else {
                    (
                        "failed",
                        Some(format!("compensation incomplete: {}", pending.join(", "))),
                    )
                };
                let mut conn = tx.conn().await;
                sqlx::query(&format!(
                    "UPDATE {}.bb_workflow_instances
                        SET status = $2, last_error = COALESCE($3, last_error), updated_at = now()
                      WHERE id = $1",
                    self.schema
                ))
                .bind(id)
                .bind(status)
                .bind(error)
                .execute(&mut *conn)
                .await?;
                Ok(())
            }),
        )
        .await;
        match outcome {
            Ok(()) => Ok(()),
            Err(RunError::Wf(e)) => Err(e),
            Err(RunError::Failure(f)) => Err(WorkflowError::Config(f.message)),
        }
    }

    async fn compensate_one(
        &self,
        def: &Arc<dyn WorkflowDefinition>,
        id: Uuid,
        comp: &CompStep,
        label: &str,
    ) -> Result<Option<()>, RunError> {
        let tx = write_tx()?;
        let Some(inst) = self.lock(&tx, id).await? else {
            return Ok(None);
        };
        if !matches!(
            inst.status.as_str(),
            "compensating" | "failed" | "compensated"
        ) {
            return Ok(None);
        }
        let already: i64 = {
            let mut conn = tx.conn().await;
            sqlx::query_scalar(&format!(
                "SELECT count(*) FROM {}.bb_workflow_steps WHERE instance_id = $1 AND step = $2 AND outcome = 'compensated'",
                self.schema
            ))
            .bind(id)
            .bind(label)
            .fetch_one(&mut *conn)
            .await?
        };
        if already > 0 {
            return Ok(None);
        }
        let completed = self.completed(&tx, id).await?;
        let correlation = inst
            .correlation_id
            .clone()
            .unwrap_or_else(|| inst.id.to_string());
        let ctx = self.ctx_for(inst.company_id, &correlation, inst.trigger_event_id);
        let funcs = DefFuncs {
            def: &**def,
            ctx: &ctx,
        };
        let null = Value::Null;
        let scope = Scope {
            context: &inst.context,
            event: inst.context.get("_event").unwrap_or(&null),
            result: inst.context.get("result").unwrap_or(&null),
            error: inst.context.get("error").unwrap_or(&null),
            config: &null,
            completed: &completed,
            now: Utc::now(),
        };
        if let Some(when) = &comp.when {
            let holds = expr::holds(when, &scope, &funcs)
                .await
                .map_err(|m| fail(label, FailKind::Business, format!("condition: {m}")))?;
            if !holds {
                return Ok(None);
            }
        }
        let params = Box::pin(self.render_params(label, &comp.params, &scope, &funcs)).await?;
        let record_id = match &comp.id {
            Some(t) => Some(
                expr::render(t, &scope, &funcs)
                    .await
                    .map_err(|m| fail(label, FailKind::Business, format!("id: {m}")))?,
            ),
            None => params.get("id").cloned(),
        }
        .filter(|v| !v.is_null())
        .map(|v| expr::text_of(&v));
        let step = StepSpec {
            name: label.to_string(),
            guard: None,
            kind: StepKind::Action {
                action: comp.action.clone(),
                entity: comp.entity.clone(),
                id: None,
                params: Vec::new(),
                on_success: Outcome::default(),
                on_timeout: None,
            },
            on_failure: None,
            call_timeout: None,
        };
        self.perform_action(
            def,
            &ctx,
            &step,
            &comp.action,
            comp.entity.as_deref(),
            record_id,
            comp.verb.as_deref(),
            &params,
            &inst,
            false,
        )
        .await?;
        let seq = self.next_seq(&tx, id).await?;
        self.log_step(&tx, &inst, seq, label, "compensated", None)
            .await?;
        Ok(Some(()))
    }

    // ── timers ────────────────────────────────────────────────────────────────────────────

    /// Find what is due and move it on: waits whose timeout passed take their `on_timeout`,
    /// instances past their deadline expire, a failed step whose backoff passed runs again, and an
    /// instance found `running` or `compensating` with nobody driving it is driven on. Run it from a
    /// scheduled job (`workflow_timer_sweep`, every minute); concurrent sweeps are safe, each
    /// instance is claimed with `FOR UPDATE SKIP LOCKED`.
    ///
    /// # Errors
    /// A database error; the next sweep finds the same work.
    pub async fn sweep(&self) -> Result<SweepReport, WorkflowError> {
        Box::pin(self.sweep_due()).await
    }

    async fn sweep_due(&self) -> Result<SweepReport, WorkflowError> {
        let mut report = SweepReport::default();
        let candidates: Vec<(Uuid, Uuid)> = {
            let mut tx = self.pool.begin().await?;
            sqlx::query("SELECT set_config('app.workflow_sweep', 'on', true)")
                .execute(&mut *tx)
                .await?;
            let rows = sqlx::query_as(&format!(
                "SELECT id, company_id FROM {s}.bb_workflow_instances
                  WHERE {due}
                  ORDER BY COALESCE(wait_due_at, deadline_at, updated_at)
                  LIMIT $2",
                s = self.schema,
                due = Self::due_predicate()
            ))
            .bind(self.cfg.stall_after.as_secs_f64())
            .bind(self.cfg.sweep_batch)
            .fetch_all(&mut *tx)
            .await?;
            tx.commit().await?;
            rows
        };
        for (id, company) in candidates {
            let Some(def) = self.def_for_instance(company, id).await? else {
                continue;
            };
            let claimed: Result<Option<Claim>, RunError> = with_company_scope(
                Some(company),
                in_write_tx(&self.pool, Box::pin(self.claim_due(&def, id))),
            )
            .await;
            let claim = match claimed {
                Ok(c) => c,
                Err(RunError::Wf(e)) => return Err(e),
                Err(RunError::Failure(f)) => {
                    tracing::error!(instance = %id, error = %f.message, "workflow sweep could not claim an instance");
                    continue;
                }
            };
            match claim {
                None => {}
                Some(Claim::TimedOut) => {
                    report.timed_out.push(id);
                    Box::pin(self.drive(&def, company, id)).await?;
                }
                Some(Claim::Expired) => report.expired.push(id),
                Some(Claim::Retry) => {
                    report.retried.push(id);
                    Box::pin(self.drive(&def, company, id)).await?;
                }
                Some(Claim::Recover) => {
                    report.recovered.push(id);
                    Box::pin(self.drive(&def, company, id)).await?;
                }
                Some(Claim::Compensate) => {
                    report.compensated.push(id);
                    Box::pin(self.compensate_company(&def, company, id)).await?;
                }
            }
        }
        Ok(report)
    }

    /// What makes an instance due; `$1` is the stall threshold in seconds.
    fn due_predicate() -> &'static str {
        "((status = 'waiting' AND ((wait_due_at IS NOT NULL AND wait_due_at <= now())
                                   OR (deadline_at IS NOT NULL AND deadline_at <= now())))
          OR (status = 'running' AND ((wait_due_at IS NOT NULL AND wait_due_at <= now())
                                      OR (wait_due_at IS NULL AND updated_at <= now() - make_interval(secs => $1::double precision))))
          OR (status = 'compensating' AND updated_at <= now() - make_interval(secs => $1::double precision)))"
    }

    async fn claim_due(
        &self,
        def: &Arc<dyn WorkflowDefinition>,
        id: Uuid,
    ) -> Result<Option<Claim>, RunError> {
        let tx = write_tx()?;
        let inst: Option<InstanceRow> = {
            let mut conn = tx.conn().await;
            sqlx::query_as(&format!(
                "SELECT {INSTANCE_COLUMNS} FROM {s}.bb_workflow_instances
                  WHERE id = $2 AND {due}
                    FOR UPDATE SKIP LOCKED",
                s = self.schema,
                due = Self::due_predicate()
            ))
            .bind(self.cfg.stall_after.as_secs_f64())
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
        };
        let Some(inst) = inst else { return Ok(None) };
        let spec = def.spec();
        let now = Utc::now();
        let wait_due = inst.wait_due_at.is_some_and(|t| t <= now);
        match inst.status.as_str() {
            "waiting" if wait_due => {
                let Some(step) = inst.current_step.as_deref().and_then(|s| spec.step(s)) else {
                    return Err(fail(
                        "(sweep)",
                        FailKind::Business,
                        "a waiting instance's step is gone",
                    ));
                };
                let StepKind::Wait { on_timeout, .. } = &step.kind else {
                    return Err(fail(
                        &step.name,
                        FailKind::Business,
                        "a waiting instance is not on a wait step",
                    ));
                };
                let outcome = on_timeout.clone().unwrap_or_default();
                let correlation = inst
                    .correlation_id
                    .clone()
                    .unwrap_or_else(|| inst.id.to_string());
                let ctx = self.ctx_for(inst.company_id, &correlation, inst.trigger_event_id);
                Box::pin(self.leave_wait(
                    def,
                    &tx,
                    &inst,
                    &step.name,
                    &outcome,
                    "timed_out",
                    None,
                    &ctx,
                ))
                .await?;
                Ok(Some(Claim::TimedOut))
            }
            "waiting" => {
                let seq = self.next_seq(&tx, inst.id).await?;
                self.log_step(
                    &tx,
                    &inst,
                    seq,
                    inst.current_step.as_deref().unwrap_or("(deadline)"),
                    "expired",
                    Some("the instance passed its deadline"),
                )
                .await?;
                let mut conn = tx.conn().await;
                sqlx::query(&format!(
                    "UPDATE {}.bb_workflow_instances
                        SET status = 'expired', wait_event = NULL, wait_key = NULL, wait_due_at = NULL,
                            last_error = 'the instance passed its deadline', updated_at = now()
                      WHERE id = $1",
                    self.schema
                ))
                .bind(inst.id)
                .execute(&mut *conn)
                .await?;
                Ok(Some(Claim::Expired))
            }
            "running" => {
                let retry = inst.wait_due_at.is_some();
                let mut conn = tx.conn().await;
                sqlx::query(&format!(
                    "UPDATE {}.bb_workflow_instances SET wait_due_at = NULL, updated_at = now() WHERE id = $1",
                    self.schema
                ))
                .bind(inst.id)
                .execute(&mut *conn)
                .await?;
                Ok(Some(if retry { Claim::Retry } else { Claim::Recover }))
            }
            "compensating" => {
                let mut conn = tx.conn().await;
                sqlx::query(&format!(
                    "UPDATE {}.bb_workflow_instances SET updated_at = now() WHERE id = $1",
                    self.schema
                ))
                .bind(inst.id)
                .execute(&mut *conn)
                .await?;
                Ok(Some(Claim::Compensate))
            }
            _ => Ok(None),
        }
    }

    // ── operating ─────────────────────────────────────────────────────────────────────────

    /// Cancel an instance that has not ended. Compensation does not run.
    ///
    /// # Errors
    /// A database error.
    pub async fn cancel(&self, company: Uuid, id: Uuid) -> Result<bool, WorkflowError> {
        let done: Result<bool, RunError> = with_company_scope(
            Some(company),
            in_write_tx(&self.pool, async {
                let tx = write_tx()?;
                let Some(inst) = self.lock(&tx, id).await? else { return Ok(false) };
                if inst.is_final() {
                    return Ok(false);
                }
                let seq = self.next_seq(&tx, id).await?;
                self.log_step(&tx, &inst, seq, inst.current_step.as_deref().unwrap_or("(cancel)"), "cancelled", None).await?;
                let mut conn = tx.conn().await;
                sqlx::query(&format!(
                    "UPDATE {}.bb_workflow_instances
                        SET status = 'cancelled', wait_event = NULL, wait_key = NULL, wait_due_at = NULL, updated_at = now()
                      WHERE id = $1",
                    self.schema
                ))
                .bind(id)
                .execute(&mut *conn)
                .await?;
                Ok(true)
            }),
        )
        .await;
        match done {
            Ok(b) => Ok(b),
            Err(RunError::Wf(e)) => Err(e),
            Err(RunError::Failure(f)) => Err(WorkflowError::Config(f.message)),
        }
    }

    /// An instance and its step log, as the company sees them (`None` when it has none by that id).
    ///
    /// # Errors
    /// A database error.
    pub async fn describe(
        &self,
        company: Uuid,
        id: Uuid,
    ) -> Result<Option<(InstanceRow, Vec<StepRow>)>, WorkflowError> {
        self.in_tx(company, async {
            let tx = write_tx()?;
            let mut conn = tx.conn().await;
            let inst: Option<InstanceRow> = sqlx::query_as(&format!(
                "SELECT {INSTANCE_COLUMNS} FROM {}.bb_workflow_instances WHERE id = $1",
                self.schema
            ))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
            let Some(inst) = inst else { return Ok(None) };
            let steps: Vec<StepRow> = sqlx::query_as(&format!(
                "SELECT seq, step, outcome, error FROM {}.bb_workflow_steps WHERE instance_id = $1 ORDER BY seq",
                self.schema
            ))
            .bind(id)
            .fetch_all(&mut *conn)
            .await?;
            Ok(Some((inst, steps)))
        })
        .await
    }

    // ── a flow without persistence ────────────────────────────────────────────────────────

    /// The body of a flow without persistence, inside the transaction that created the instance:
    /// every step in a savepoint of it. A step's failure takes its `on_failure.next` when it is a
    /// business failure and has one; any other failure ends the whole flow with an error.
    async fn run_inline(
        &self,
        def: &Arc<dyn WorkflowDefinition>,
        id: Uuid,
    ) -> Result<(), RunError> {
        for _ in 0..self.cfg.max_steps.max(1) {
            match in_write_tx(&self.pool, Box::pin(self.run_one(def, id))).await {
                Ok(StepEnd::Advanced) => continue,
                Ok(_) => return Ok(()),
                Err(RunError::Wf(e)) => return Err(RunError::Wf(e)),
                Err(RunError::Failure(f)) => {
                    let routed = def
                        .spec()
                        .step(&f.step)
                        .and_then(|s| s.on_failure.as_ref())
                        .and_then(|p| p.next.clone());
                    let Some(next) = routed.filter(|_| f.kind == FailKind::Business) else {
                        return Err(RunError::Failure(f));
                    };
                    let tx = write_tx()?;
                    if let Some(inst) = self.lock(&tx, id).await? {
                        let seq = self.next_seq(&tx, id).await?;
                        self.log_step(&tx, &inst, seq, &f.step, "failed", Some(&f.message))
                            .await?;
                        let mut context = inst.context.clone();
                        if let Some(map) = context.as_object_mut() {
                            map.insert("error".into(), Value::String(f.message.clone()));
                        }
                        self.advance(&tx, &inst, &next, &context).await?;
                    }
                }
            }
        }
        Err(fail(
            "(runaway)",
            FailKind::Business,
            "the flow did not end",
        ))
    }
}

enum Claim {
    TimedOut,
    Expired,
    Retry,
    Recover,
    Compensate,
}

fn write_tx() -> Result<Arc<WriteTx>, RunError> {
    current_write_tx().ok_or_else(|| {
        RunError::Wf(WorkflowError::Config(
            "the engine ran outside a write transaction".into(),
        ))
    })
}

/// What a composing service wires for a module's engine workflows: the relay's sink and the
/// scheduled sweep. The module builder only hands this over (`background_tasks()`); registering
/// the relay and the job is the composing service's, as for every other background task.
///
/// ```ignore
/// let tasks = module.background_tasks();
/// // the relay: deliver every staged event the engine cares about
/// runner::supervise(pool, RelayConfig::new("hr"), move |rec| tasks.deliver(rec), shutdown);
/// // the scheduler: BackgroundTasks::TIMER_SWEEP_JOB, every BackgroundTasks::TIMER_SWEEP_SCHEDULE
/// scheduler.every_minute(BackgroundTasks::TIMER_SWEEP_JOB, move || tasks.timer_sweep());
/// ```
pub struct BackgroundTasks {
    engine: Arc<WorkflowEngine>,
}

impl BackgroundTasks {
    /// The name of the scheduled job that runs [`timer_sweep`](Self::timer_sweep).
    pub const TIMER_SWEEP_JOB: &'static str = "workflow_timer_sweep";
    /// When it runs: every minute. A concurrent run is safe (instances are claimed with
    /// `FOR UPDATE SKIP LOCKED`), so the interval is a floor on how late a timeout may fire, not a
    /// contract.
    pub const TIMER_SWEEP_SCHEDULE: &'static str = "* * * * *";

    /// Wrap `engine`.
    pub fn new(engine: Arc<WorkflowEngine>) -> Self {
        Self { engine }
    }

    /// The engine itself, for operating on instances.
    pub fn engine(&self) -> &Arc<WorkflowEngine> {
        &self.engine
    }

    /// The event types the engine reacts to.
    pub fn subscribed_events(&self) -> Vec<String> {
        self.engine.subscribed_events()
    }

    /// The relay's sink: hand an outbox record to the engine. A record of no subscribed type is
    /// accepted and ignored, so the sink can be shared with other consumers; a failure is a
    /// publish error, which leaves the row for the relay's next pass.
    pub async fn deliver(&self, record: OutboxRecord) -> Result<(), backbone_outbox::OutboxError> {
        if !self
            .engine
            .subscribed_events()
            .iter()
            .any(|e| *e == record.event_type)
        {
            return Ok(());
        }
        self.engine
            .on_event(&record)
            .await
            .map(|_| ())
            .map_err(|e| backbone_outbox::OutboxError::Publish(e.to_string()))
    }

    /// The scheduled job: move on whatever is due.
    ///
    /// # Errors
    /// See [`WorkflowEngine::sweep`].
    pub async fn timer_sweep(&self) -> Result<SweepReport, WorkflowError> {
        self.engine.sweep().await
    }
}
