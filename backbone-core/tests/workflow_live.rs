//! Live proof of the workflow engine, on a scratch database.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN): the test creates a database and a login role
//! of its own, runs the engine as that non-superuser role (so the row-level fence holds), and
//! drops both. When the variable is unset the tests skip, so a checkout without a database still
//! passes.
//!
//! What it holds the engine to:
//! - a replayed start event makes one instance, and a replayed resume event resumes once;
//! - a step's writes, its step-log row and the instance update commit together, and a step that
//!   fails leaves none of its writes;
//! - a wait resumes on the matching event only, takes its `on_timeout` branch through the sweep, and
//!   an instance past its deadline expires;
//! - a transient failure retries with a backoff, three times, then fails and compensates in
//!   reverse; a business failure is not retried; compensation is idempotent;
//! - a step interrupted mid-flight leaves the instance on that step, and the sweep resumes it
//!   without repeating a completed one;
//! - instances are fenced by company: another company can neither see nor resume them, while the
//!   sweep still finds what is due in every company;
//! - a flow without persistence runs in one transaction and leaves nothing when it fails.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use backbone_core::service::{ServiceError, ServiceResult};
use backbone_core::workflow::expr::{BinOp, Dur, Expr, Part, Root, Seg, Template, Tpl};
use backbone_core::workflow::spec::{
    Backoff, Branch, Call, CompStep, EmitSpec, FailureSpec, Outcome, RetrySpec, StepKind, StepSpec,
    Terminal, WaitKey, WorkflowDefinition, WorkflowSpec,
};
use backbone_core::workflow::{schema, EngineConfig, WorkflowEngine};
use backbone_core::write_hooks::{OutboxRecord, WriteCtx};
use backbone_outbox::outbox;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

// ── building specs ───────────────────────────────────────────────────────────────────────

fn ctx(keys: &[&str]) -> Expr {
    Expr::Path {
        root: Root::Context,
        path: keys.iter().map(|k| Seg::Key((*k).into())).collect(),
    }
}

fn hole(e: Expr) -> Tpl {
    Tpl::Template(Template(vec![Part::Expr(e)]))
}

fn lit(v: Value) -> Tpl {
    Tpl::Lit(v)
}

fn next(step: &str) -> Outcome {
    Outcome {
        set: Vec::new(),
        next: Some(step.into()),
    }
}

fn action(name: &str, verb: &str, params: Vec<(&str, Tpl)>, on_success: Outcome) -> StepSpec {
    StepSpec {
        name: name.into(),
        guard: None,
        kind: StepKind::Action {
            action: verb.into(),
            entity: None,
            id: None,
            params: params
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            on_success,
            on_timeout: None,
        },
        on_failure: None,
        call_timeout: None,
    }
}

fn terminal(name: &str, status: Terminal) -> StepSpec {
    StepSpec {
        name: name.into(),
        guard: None,
        kind: StepKind::Terminal {
            status,
            reason: None,
            emit: None,
            compensate: false,
            result: None,
        },
        on_failure: None,
        call_timeout: None,
    }
}

fn spec(name: &str, trigger: &str, persistence: bool, steps: Vec<StepSpec>) -> WorkflowSpec {
    WorkflowSpec {
        name: name.into(),
        version: 1,
        trigger_event: trigger.into(),
        extract: vec![(
            "ticket_id".into(),
            Expr::Path {
                root: Root::Event,
                path: vec![Seg::Key("ticket_id".into())],
            },
        )],
        context: Vec::new(),
        persistence,
        timeout: None,
        retry: None,
        steps,
        compensation: Vec::new(),
    }
}

/// The escalation flow: write a note, wait for an acknowledgement (10 minutes), then write the
/// acknowledgement or escalate.
fn escalation() -> WorkflowSpec {
    let ticket = || vec![("ticket", hole(ctx(&["ticket_id"])))];
    let mut s = spec(
        "Escalation",
        "TicketRaised",
        true,
        vec![
            action(
                "note_open",
                "create_note",
                [ticket(), vec![("text", lit(json!("opened")))]].concat(),
                next("await_ack"),
            ),
            StepSpec {
                name: "await_ack".into(),
                guard: None,
                kind: StepKind::Wait {
                    event: Some("TicketAcked".into()),
                    key: vec![WaitKey {
                        event_field: "ticket_id".into(),
                        context: ctx(&["ticket_id"]),
                    }],
                    timeout: Some(Dur::Secs(600)),
                    on_event: Some(Outcome {
                        set: vec![("acked".into(), lit(json!(true)))],
                        next: Some("note_ack".into()),
                    }),
                    on_timeout: Some(next("escalate")),
                },
                on_failure: None,
                call_timeout: None,
            },
            action(
                "note_ack",
                "create_note",
                [ticket(), vec![("text", lit(json!("acknowledged")))]].concat(),
                next("done"),
            ),
            action(
                "escalate",
                "create_note",
                [ticket(), vec![("text", lit(json!("escalated")))]].concat(),
                next("done"),
            ),
            terminal("done", Terminal::Completed),
        ],
    );
    s.compensation = vec![CompStep {
        name: "remove_notes".into(),
        when: Some(Expr::Bin {
            op: BinOp::Eq,
            l: Box::new(Expr::Path {
                root: Root::Steps,
                path: vec![Seg::Key("note_open".into()), Seg::Key("completed".into())],
            }),
            r: Box::new(Expr::Lit(json!(true))),
        }),
        action: "delete_notes".into(),
        entity: None,
        id: None,
        params: ticket()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        verb: None,
    }];
    s
}

/// A flow that fails: write a note, then run a verb that fails, then end.
fn failing(verb: &str, retry: Option<RetrySpec>) -> WorkflowSpec {
    let mut s = spec(
        "Failing",
        "TicketFailing",
        true,
        vec![
            action(
                "note_open",
                "create_note",
                vec![
                    ("ticket", hole(ctx(&["ticket_id"]))),
                    ("text", lit(json!("opened"))),
                ],
                next("risky"),
            ),
            action(
                "risky",
                verb,
                vec![("ticket", hole(ctx(&["ticket_id"])))],
                next("done"),
            ),
            terminal("done", Terminal::Completed),
        ],
    );
    s.retry = retry;
    s.compensation = vec![CompStep {
        name: "remove_notes".into(),
        when: Some(Expr::Bin {
            op: BinOp::Eq,
            l: Box::new(Expr::Path {
                root: Root::Steps,
                path: vec![Seg::Key("note_open".into()), Seg::Key("completed".into())],
            }),
            r: Box::new(Expr::Lit(json!(true))),
        }),
        action: "delete_notes".into(),
        entity: None,
        id: None,
        params: vec![("ticket".into(), hole(ctx(&["ticket_id"])))],
        verb: None,
    }];
    s
}

// ── the definition: typed code a generated module would write ──────────────────────────────

#[derive(Default)]
struct Knobs {
    /// How many more times `flaky` fails before it succeeds.
    flaky_failures: AtomicI32,
    /// Calls of `flaky`.
    flaky_calls: AtomicI32,
    /// Calls of `delete_notes`.
    deletes: AtomicI32,
    /// Signalled by `hang` once it is inside the step's transaction.
    hung: tokio::sync::Notify,
}

struct Flow {
    spec: WorkflowSpec,
    knobs: Arc<Knobs>,
}

#[async_trait]
impl WorkflowDefinition for Flow {
    fn spec(&self) -> &WorkflowSpec {
        &self.spec
    }

    async fn perform(&self, wctx: &WriteCtx, call: &Call<'_>) -> ServiceResult<Value> {
        let ticket = call
            .params
            .get("ticket")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let company = wctx
            .company_id
            .expect("the engine scopes every step to a company");
        let write_note = |text: String| {
            let ticket = ticket.clone();
            async move {
                let tx =
                    backbone_orm::current_write_tx().expect("a step runs in a write transaction");
                let mut conn = tx.conn().await;
                sqlx::query(
                    "INSERT INTO wf.notes (id, company_id, ticket, text) VALUES ($1, $2, $3, $4)",
                )
                .bind(Uuid::new_v4())
                .bind(company)
                .bind(ticket)
                .bind(text)
                .execute(&mut *conn)
                .await
                .map_err(|e| ServiceError::Internal(e.to_string()))
            }
        };
        match call.action {
            "create_note" => {
                let text = call
                    .params
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                write_note(text).await?;
                Ok(json!({ "noted": true }))
            }
            "delete_notes" => {
                self.knobs.deletes.fetch_add(1, Ordering::SeqCst);
                let tx = backbone_orm::current_write_tx().unwrap();
                let mut conn = tx.conn().await;
                sqlx::query("DELETE FROM wf.notes WHERE ticket = $1")
                    .bind(&ticket)
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| ServiceError::Internal(e.to_string()))?;
                Ok(Value::Null)
            }
            // Writes a note, then fails transiently: the note must not survive.
            "boom_after_write" => {
                write_note("never".into()).await?;
                Err(ServiceError::Internal(
                    "the verb broke after writing".into(),
                ))
            }
            "flaky" => {
                self.knobs.flaky_calls.fetch_add(1, Ordering::SeqCst);
                if self.knobs.flaky_failures.fetch_sub(1, Ordering::SeqCst) > 0 {
                    return Err(ServiceError::Internal("flaky".into()));
                }
                write_note("flaky succeeded".into()).await?;
                Ok(Value::Null)
            }
            "refuse" => Err(ServiceError::Validation("not allowed".into())),
            "hang" => {
                write_note("hung".into()).await?;
                self.knobs.hung.notify_one();
                tokio::time::sleep(Duration::from_secs(3600)).await;
                Ok(Value::Null)
            }
            other => Err(ServiceError::Internal(format!("no verb `{other}`"))),
        }
    }
}

// ── the harness ────────────────────────────────────────────────────────────────────────────

struct Harness {
    admin_root: PgPool,
    admin: PgPool,
    app: PgPool,
    db: String,
    role: String,
    knobs: Arc<Knobs>,
}

impl Harness {
    async fn new() -> Option<Self> {
        let Ok(dsn) = std::env::var("BACKBONE_ORM_RLS_DSN") else {
            eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
            return None;
        };
        let suffix = Uuid::new_v4().simple().to_string()[..8].to_string();
        let db = format!("wf_live_{suffix}");
        let role = format!("wf_app_{suffix}");
        let admin_root = PgPoolOptions::new()
            .max_connections(2)
            .connect(&dsn)
            .await
            .unwrap();
        sqlx::raw_sql(&format!("CREATE DATABASE {db}"))
            .execute(&admin_root)
            .await
            .unwrap();
        sqlx::raw_sql(&format!(
            "CREATE ROLE {role} LOGIN PASSWORD 'wfpw' NOSUPERUSER NOBYPASSRLS"
        ))
        .execute(&admin_root)
        .await
        .unwrap();
        let (base, _) = dsn.rsplit_once('/').unwrap();
        let admin = PgPoolOptions::new()
            .max_connections(4)
            .connect(&format!("{base}/{db}"))
            .await
            .unwrap();
        sqlx::raw_sql(&schema::migration_sql("wf"))
            .execute(&admin)
            .await
            .unwrap();
        outbox::migrate(&admin, "wf").await.unwrap();
        sqlx::raw_sql(&format!(
            "CREATE TABLE wf.notes (id uuid PRIMARY KEY, company_id uuid NOT NULL, ticket text NOT NULL, text text NOT NULL);
             ALTER TABLE wf.notes ENABLE ROW LEVEL SECURITY;
             ALTER TABLE wf.notes FORCE ROW LEVEL SECURITY;
             CREATE POLICY notes_company ON wf.notes FOR ALL
               USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
               WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
             GRANT USAGE ON SCHEMA wf TO {role};
             GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA wf TO {role};"
        ))
        .execute(&admin)
        .await
        .unwrap();
        let (_, host_and_db) = dsn.rsplit_once('@').unwrap();
        let host = host_and_db.rsplit_once('/').map_or(host_and_db, |(h, _)| h);
        let app = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&format!("postgresql://{role}:wfpw@{host}/{db}"))
            .await
            .unwrap();
        Some(Self {
            admin_root,
            admin,
            app,
            db,
            role,
            knobs: Arc::default(),
        })
    }

    fn engine(&self, specs: Vec<WorkflowSpec>) -> WorkflowEngine {
        self.engine_with(specs, fast())
    }

    fn engine_with(&self, specs: Vec<WorkflowSpec>, cfg: EngineConfig) -> WorkflowEngine {
        let defs: Vec<Arc<dyn WorkflowDefinition>> = specs
            .into_iter()
            .map(|s| {
                Arc::new(Flow {
                    spec: s,
                    knobs: self.knobs.clone(),
                }) as Arc<dyn WorkflowDefinition>
            })
            .collect();
        WorkflowEngine::new(self.app.clone(), "wf", defs).with_config(cfg)
    }

    async fn notes(&self, ticket: &str) -> Vec<String> {
        sqlx::query_scalar("SELECT text FROM wf.notes WHERE ticket = $1 ORDER BY text")
            .bind(ticket)
            .fetch_all(&self.admin)
            .await
            .unwrap()
    }

    async fn instances(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM wf.bb_workflow_instances")
            .fetch_one(&self.admin)
            .await
            .unwrap()
    }

    /// Move a waiting instance's timer into the past.
    async fn force_due(&self, id: Uuid) {
        sqlx::query("UPDATE wf.bb_workflow_instances SET wait_due_at = now() - interval '1 second' WHERE id = $1")
            .bind(id)
            .execute(&self.admin)
            .await
            .unwrap();
    }

    async fn teardown(self) {
        self.app.close().await;
        self.admin.close().await;
        let _ = sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {} WITH (FORCE)", self.db))
            .execute(&self.admin_root)
            .await;
        let _ = sqlx::raw_sql(&format!("DROP ROLE IF EXISTS {}", self.role))
            .execute(&self.admin_root)
            .await;
    }
}

fn fast() -> EngineConfig {
    EngineConfig {
        backoff_base: Duration::from_millis(50),
        backoff_max: Duration::from_millis(400),
        stall_after: Duration::ZERO,
        ..EngineConfig::default()
    }
}

fn event(kind: &str, company: Uuid, payload: Value) -> OutboxRecord {
    OutboxRecord::new(kind, "Ticket", "t", company, payload, chrono::Utc::now())
        .with_correlation_id("corr-1")
}

macro_rules! harness {
    () => {
        match Harness::new().await {
            Some(h) => h,
            None => return,
        }
    };
}

// ── probes ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_event_starts_one_instance_and_a_replay_does_nothing() {
    let h = harness!();
    let engine = h.engine(vec![escalation()]);
    let company = Uuid::new_v4();
    let raised = event("TicketRaised", company, json!({ "ticket_id": "T-1" }));

    let first = engine.on_event(&raised).await.unwrap();
    assert_eq!(first.started.len(), 1);
    let replay = engine.on_event(&raised).await.unwrap();
    assert!(replay.started.is_empty(), "the replay started nothing");
    assert_eq!(replay.duplicates, 1);
    assert_eq!(h.instances().await, 1, "still one instance");
    assert_eq!(
        h.notes("T-1").await,
        vec!["opened"],
        "the first step ran once"
    );

    let (inst, steps) = engine
        .describe(company, first.started[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inst.status, "waiting");
    assert_eq!(inst.current_step.as_deref(), Some("await_ack"));
    assert_eq!(
        inst.correlation_id.as_deref(),
        Some("corr-1"),
        "the start event's correlation id"
    );
    assert_eq!(inst.context["ticket_id"], "T-1");
    let log: Vec<(&str, &str)> = steps
        .iter()
        .map(|s| (s.step.as_str(), s.outcome.as_str()))
        .collect();
    assert_eq!(log, vec![("note_open", "ok"), ("await_ack", "waiting")]);
    h.teardown().await;
}

#[tokio::test]
async fn a_wait_resumes_on_the_matching_event_only_and_a_replayed_resume_does_nothing() {
    let h = harness!();
    let engine = h.engine(vec![escalation()]);
    let company = Uuid::new_v4();
    let id = engine
        .on_event(&event(
            "TicketRaised",
            company,
            json!({ "ticket_id": "T-2" }),
        ))
        .await
        .unwrap()
        .started[0];

    // Another ticket's acknowledgement does not move it.
    let other = engine
        .on_event(&event(
            "TicketAcked",
            company,
            json!({ "ticket_id": "T-other" }),
        ))
        .await
        .unwrap();
    assert!(other.resumed.is_empty());
    assert_eq!(
        engine
            .describe(company, id)
            .await
            .unwrap()
            .unwrap()
            .0
            .status,
        "waiting"
    );

    let ack = event("TicketAcked", company, json!({ "ticket_id": "T-2" }));
    let resumed = engine.on_event(&ack).await.unwrap();
    assert_eq!(resumed.resumed, vec![id]);
    let (inst, steps) = engine.describe(company, id).await.unwrap().unwrap();
    assert_eq!(inst.status, "completed");
    assert_eq!(inst.context["acked"], true, "the wait's `set` ran");
    assert_eq!(h.notes("T-2").await, vec!["acknowledged", "opened"]);
    let log: Vec<&str> = steps.iter().map(|s| s.outcome.as_str()).collect();
    assert_eq!(log, vec!["ok", "waiting", "resumed", "ok", "terminal"]);

    // The same event again: claimed already, nothing happens.
    let again = engine.on_event(&ack).await.unwrap();
    assert!(again.resumed.is_empty());
    assert_eq!(h.notes("T-2").await.len(), 2, "no step ran twice");
    h.teardown().await;
}

#[tokio::test]
async fn a_wait_that_times_out_takes_its_branch_through_the_sweep() {
    let h = harness!();
    let engine = h.engine(vec![escalation()]);
    let company = Uuid::new_v4();
    let id = engine
        .on_event(&event(
            "TicketRaised",
            company,
            json!({ "ticket_id": "T-3" }),
        ))
        .await
        .unwrap()
        .started[0];

    // Nothing is due yet.
    assert_eq!(engine.sweep().await.unwrap().total(), 0);
    h.force_due(id).await;
    let report = engine.sweep().await.unwrap();
    assert_eq!(report.timed_out, vec![id]);
    let (inst, steps) = engine.describe(company, id).await.unwrap().unwrap();
    assert_eq!(inst.status, "completed");
    assert_eq!(
        h.notes("T-3").await,
        vec!["escalated", "opened"],
        "the on_timeout branch ran"
    );
    assert!(steps
        .iter()
        .any(|s| s.step == "await_ack" && s.outcome == "timed_out"));
    // A second sweep finds nothing.
    assert_eq!(engine.sweep().await.unwrap().total(), 0);
    h.teardown().await;
}

#[tokio::test]
async fn an_instance_past_its_deadline_expires() {
    let h = harness!();
    let mut s = escalation();
    s.timeout = Some(Dur::Secs(3600));
    let engine = h.engine(vec![s]);
    let company = Uuid::new_v4();
    let id = engine
        .on_event(&event(
            "TicketRaised",
            company,
            json!({ "ticket_id": "T-4" }),
        ))
        .await
        .unwrap()
        .started[0];
    let deadline = engine
        .describe(company, id)
        .await
        .unwrap()
        .unwrap()
        .0
        .deadline_at;
    assert!(deadline.is_some(), "the workflow timeout set a deadline");
    // The wait's own timer is later than the deadline here: clear it, pass the deadline.
    sqlx::query("UPDATE wf.bb_workflow_instances SET wait_due_at = NULL, deadline_at = now() - interval '1 second' WHERE id = $1")
        .bind(id)
        .execute(&h.admin)
        .await
        .unwrap();
    let report = engine.sweep().await.unwrap();
    assert_eq!(report.expired, vec![id]);
    let (inst, steps) = engine.describe(company, id).await.unwrap().unwrap();
    assert_eq!(inst.status, "expired");
    assert!(steps.iter().any(|s| s.outcome == "expired"));
    h.teardown().await;
}

#[tokio::test]
async fn a_step_that_fails_after_writing_leaves_none_of_its_writes() {
    let h = harness!();
    let engine = h.engine(vec![failing(
        "boom_after_write",
        Some(RetrySpec {
            max_attempts: 5,
            backoff: Backoff::Fixed(60),
        }),
    )]);
    let company = Uuid::new_v4();
    let id = engine
        .on_event(&event(
            "TicketFailing",
            company,
            json!({ "ticket_id": "T-5" }),
        ))
        .await
        .unwrap()
        .started[0];
    assert_eq!(
        h.notes("T-5").await,
        vec!["opened"],
        "the failed step's note rolled back, the earlier step's stayed"
    );
    let (inst, steps) = engine.describe(company, id).await.unwrap().unwrap();
    assert_eq!(inst.status, "running", "waiting out its backoff");
    assert_eq!(
        inst.current_step.as_deref(),
        Some("risky"),
        "still on the step that failed"
    );
    assert_eq!(inst.attempts, 1);
    assert!(inst.wait_due_at.is_some(), "the backoff is stored");
    assert!(inst
        .last_error
        .as_deref()
        .unwrap()
        .contains("broke after writing"));
    let failed: Vec<_> = steps.iter().filter(|s| s.outcome == "failed").collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].step, "risky");
    h.teardown().await;
}

#[tokio::test]
async fn a_transient_failure_retries_three_times_then_fails_and_compensates_in_reverse() {
    let h = harness!();
    h.knobs.flaky_failures.store(100, Ordering::SeqCst);
    let slow_backoff = EngineConfig {
        backoff_base: Duration::from_secs(30),
        backoff_max: Duration::from_secs(300),
        stall_after: Duration::ZERO,
        ..EngineConfig::default()
    };
    let engine = h.engine_with(vec![failing("flaky", None)], slow_backoff);
    let company = Uuid::new_v4();
    let id = engine
        .on_event(&event(
            "TicketFailing",
            company,
            json!({ "ticket_id": "T-6" }),
        ))
        .await
        .unwrap()
        .started[0];

    // First failure: backoff stored, nothing else moved.
    let (inst, _) = engine.describe(company, id).await.unwrap().unwrap();
    assert_eq!((inst.status.as_str(), inst.attempts), ("running", 1));
    let first_due = inst.wait_due_at.unwrap();

    // Two more attempts, each only when its backoff has passed.
    assert_eq!(
        engine.sweep().await.unwrap().total(),
        0,
        "the backoff has not passed"
    );
    h.force_due(id).await;
    assert_eq!(engine.sweep().await.unwrap().retried, vec![id]);
    let (inst, _) = engine.describe(company, id).await.unwrap().unwrap();
    assert_eq!(inst.attempts, 2);
    assert!(
        inst.wait_due_at.unwrap() > first_due,
        "the second backoff is longer than the first"
    );
    h.force_due(id).await;
    engine.sweep().await.unwrap();

    assert_eq!(h.knobs.flaky_calls.load(Ordering::SeqCst), 3, "three tries");
    let (inst, steps) = engine.describe(company, id).await.unwrap().unwrap();
    assert_eq!(inst.status, "compensated");
    assert!(inst.last_error.as_deref().unwrap().contains("flaky"));
    assert_eq!(
        h.notes("T-6").await,
        Vec::<String>::new(),
        "compensation removed what the completed step wrote"
    );
    let log: Vec<(&str, &str)> = steps
        .iter()
        .map(|s| (s.step.as_str(), s.outcome.as_str()))
        .collect();
    assert_eq!(
        log,
        vec![
            ("note_open", "ok"),
            ("risky", "failed"),
            ("risky", "failed"),
            ("risky", "failed"),
            ("compensate:remove_notes", "compensated"),
        ]
    );

    // Compensating again does nothing: the step is already logged as undone.
    let deletes = h.knobs.deletes.load(Ordering::SeqCst);
    engine.compensate(company, id).await.unwrap();
    assert_eq!(
        h.knobs.deletes.load(Ordering::SeqCst),
        deletes,
        "idempotent per instance"
    );
    assert_eq!(
        engine.describe(company, id).await.unwrap().unwrap().1.len(),
        steps.len()
    );
    h.teardown().await;
}

#[tokio::test]
async fn a_business_failure_is_not_retried_and_a_routed_one_follows_its_branch() {
    let h = harness!();
    let engine = h.engine(vec![failing("refuse", None)]);
    let company = Uuid::new_v4();
    let id = engine
        .on_event(&event(
            "TicketFailing",
            company,
            json!({ "ticket_id": "T-7" }),
        ))
        .await
        .unwrap()
        .started[0];
    let (inst, steps) = engine.describe(company, id).await.unwrap().unwrap();
    assert_eq!(
        inst.status, "compensated",
        "refused, nowhere to route: failed and compensated at once"
    );
    assert_eq!(
        steps.iter().filter(|s| s.outcome == "failed").count(),
        1,
        "no retry of a refusal"
    );

    // The same verb with the failure routed to a step.
    let mut routed = failing("refuse", None);
    routed.name = "Routed".into();
    routed.trigger_event = "TicketRouted".into();
    routed.steps[1].on_failure = Some(FailureSpec {
        next: Some("recover".into()),
        ..Default::default()
    });
    routed.steps.insert(
        2,
        action(
            "recover",
            "create_note",
            vec![
                ("ticket", hole(ctx(&["ticket_id"]))),
                ("text", lit(json!("recovered"))),
            ],
            next("done"),
        ),
    );
    let engine = h.engine(vec![routed]);
    let id = engine
        .on_event(&event(
            "TicketRouted",
            company,
            json!({ "ticket_id": "T-8" }),
        ))
        .await
        .unwrap()
        .started[0];
    let (inst, _) = engine.describe(company, id).await.unwrap().unwrap();
    assert_eq!(inst.status, "completed");
    assert!(
        inst.context["error"]
            .as_str()
            .unwrap()
            .contains("not allowed"),
        "the failure message reaches the context"
    );
    assert_eq!(h.notes("T-8").await, vec!["opened", "recovered"]);
    h.teardown().await;
}

#[tokio::test]
async fn a_step_cut_short_leaves_the_instance_on_it_and_the_sweep_resumes_without_repeating() {
    let h = harness!();
    let engine = h.engine(vec![failing("hang", None)]);
    let company = Uuid::new_v4();
    // The `hang` verb writes a note and then never returns: cancel the whole run while it is inside
    // the step's transaction, as a crash or a dropped task would.
    let started = event("TicketFailing", company, json!({ "ticket_id": "T-9" }));
    let run = engine.on_event(&started);
    tokio::select! {
        _ = run => panic!("the hanging verb returned"),
        _ = h.knobs.hung.notified() => {}
    }
    // The run was dropped inside the step's transaction.
    // Let the cancelled transaction's rollback settle.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        h.notes("T-9").await,
        vec!["opened"],
        "the cut step's note rolled back; the completed step's stayed"
    );
    let id: Uuid = sqlx::query_scalar("SELECT id FROM wf.bb_workflow_instances")
        .fetch_one(&h.admin)
        .await
        .unwrap();
    let (inst, steps) = engine.describe(company, id).await.unwrap().unwrap();
    assert_eq!(inst.status, "running");
    assert_eq!(inst.current_step.as_deref(), Some("risky"));
    assert_eq!(steps.len(), 1, "only the completed step is logged");

    // Swap the verb for one that succeeds (a restarted process with the bug fixed) and sweep.
    h.knobs.flaky_failures.store(0, Ordering::SeqCst);
    let engine = h.engine(vec![failing("flaky", None)]);
    let report = engine.sweep().await.unwrap();
    assert_eq!(report.recovered, vec![id]);
    let (inst, steps) = engine.describe(company, id).await.unwrap().unwrap();
    assert_eq!(inst.status, "completed");
    assert_eq!(
        h.notes("T-9").await,
        vec!["flaky succeeded", "opened"],
        "the completed step did not run again"
    );
    let log: Vec<&str> = steps.iter().map(|s| s.step.as_str()).collect();
    assert_eq!(log, vec!["note_open", "risky", "done"]);
    h.teardown().await;
}

#[tokio::test]
async fn instances_are_fenced_by_company_and_the_sweep_still_serves_every_company() {
    let h = harness!();
    let engine = h.engine(vec![escalation()]);
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let ida = engine
        .on_event(&event("TicketRaised", a, json!({ "ticket_id": "T-A" })))
        .await
        .unwrap()
        .started[0];
    let idb = engine
        .on_event(&event("TicketRaised", b, json!({ "ticket_id": "T-A" })))
        .await
        .unwrap()
        .started[0];

    // Company B cannot see company A's instance or its steps.
    assert!(engine.describe(b, ida).await.unwrap().is_none());
    assert!(engine.describe(a, idb).await.unwrap().is_none());
    // The app role with no company scope sees nothing at all (the fence, not the application).
    let unscoped: i64 = sqlx::query_scalar("SELECT count(*) FROM wf.bb_workflow_instances")
        .fetch_one(&h.app)
        .await
        .unwrap();
    assert_eq!(unscoped, 0, "an unscoped statement sees no instance");
    let steps_unscoped: i64 = sqlx::query_scalar("SELECT count(*) FROM wf.bb_workflow_steps")
        .fetch_one(&h.app)
        .await
        .unwrap();
    assert_eq!(steps_unscoped, 0, "nor any step");

    // An event of company B for the same ticket key resumes B's instance, never A's.
    let resumed = engine
        .on_event(&event("TicketAcked", b, json!({ "ticket_id": "T-A" })))
        .await
        .unwrap();
    assert_eq!(resumed.resumed, vec![idb]);
    assert_eq!(
        engine.describe(a, ida).await.unwrap().unwrap().0.status,
        "waiting",
        "A's instance did not move"
    );
    assert_eq!(
        engine.describe(b, idb).await.unwrap().unwrap().0.status,
        "completed"
    );

    // Company A's timer is due: the sweep, which sees every company, runs it under A.
    h.force_due(ida).await;
    let report = engine.sweep().await.unwrap();
    assert_eq!(report.timed_out, vec![ida]);
    assert_eq!(
        engine.describe(a, ida).await.unwrap().unwrap().0.status,
        "completed"
    );
    let notes_a: i64 = sqlx::query_scalar("SELECT count(*) FROM wf.notes WHERE company_id = $1")
        .bind(a)
        .fetch_one(&h.admin)
        .await
        .unwrap();
    assert_eq!(notes_a, 2, "A's step wrote under A's company");
    h.teardown().await;
}

#[tokio::test]
async fn a_flow_without_persistence_runs_in_one_transaction_and_leaves_nothing_when_it_fails() {
    let h = harness!();
    let ok = {
        let mut s = spec(
            "Inline",
            "InlineStarted",
            false,
            vec![
                action(
                    "a",
                    "create_note",
                    vec![
                        ("ticket", hole(ctx(&["ticket_id"]))),
                        ("text", lit(json!("a"))),
                    ],
                    next("b"),
                ),
                action(
                    "b",
                    "create_note",
                    vec![
                        ("ticket", hole(ctx(&["ticket_id"]))),
                        ("text", lit(json!("b"))),
                    ],
                    next("done"),
                ),
                terminal("done", Terminal::Completed),
            ],
        );
        s.compensation = Vec::new();
        s
    };
    let broken = {
        let mut s = spec(
            "InlineBroken",
            "InlineBroken",
            false,
            vec![
                action(
                    "a",
                    "create_note",
                    vec![
                        ("ticket", hole(ctx(&["ticket_id"]))),
                        ("text", lit(json!("a"))),
                    ],
                    next("b"),
                ),
                action(
                    "b",
                    "boom_after_write",
                    vec![("ticket", hole(ctx(&["ticket_id"])))],
                    next("done"),
                ),
                terminal("done", Terminal::Completed),
            ],
        );
        s.compensation = Vec::new();
        s
    };
    let engine = h.engine(vec![ok, broken]);
    let company = Uuid::new_v4();
    let report = engine
        .on_event(&event(
            "InlineStarted",
            company,
            json!({ "ticket_id": "T-I" }),
        ))
        .await
        .unwrap();
    assert_eq!(report.started.len(), 1);
    assert_eq!(h.notes("T-I").await, vec!["a", "b"]);
    let (inst, _) = engine
        .describe(company, report.started[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inst.status, "completed");

    let err = engine
        .on_event(&event(
            "InlineBroken",
            company,
            json!({ "ticket_id": "T-J" }),
        ))
        .await;
    assert!(
        err.is_err(),
        "the failure is returned so the event is delivered again"
    );
    assert!(
        h.notes("T-J").await.is_empty(),
        "the first step's note went with the failed flow"
    );
    assert_eq!(h.instances().await, 1, "and so did the instance");
    h.teardown().await;
}

#[tokio::test]
async fn a_terminal_step_stages_its_event_with_the_instance_correlation_and_the_trigger_as_cause() {
    let h = harness!();
    let mut s = spec(
        "Emitting",
        "EmitStarted",
        true,
        vec![
            action(
                "note",
                "create_note",
                vec![
                    ("ticket", hole(ctx(&["ticket_id"]))),
                    ("text", lit(json!("n"))),
                ],
                next("announce"),
            ),
            action(
                "announce",
                "emit_event",
                vec![
                    ("event", lit(json!("TicketNoted"))),
                    (
                        "data",
                        Tpl::Map(vec![("ticket".into(), hole(ctx(&["ticket_id"])))]),
                    ),
                ],
                next("done"),
            ),
            StepSpec {
                name: "done".into(),
                guard: None,
                kind: StepKind::Terminal {
                    status: Terminal::Completed,
                    reason: None,
                    emit: Some(EmitSpec {
                        event: "FlowDone".into(),
                        data: Tpl::Map(vec![("ticket".into(), hole(ctx(&["ticket_id"])))]),
                    }),
                    compensate: false,
                    result: None,
                },
                on_failure: None,
                call_timeout: None,
            },
        ],
    );
    s.compensation = Vec::new();
    // A condition step with no else that matches nothing fails the instance with a named error.
    let engine = h.engine(vec![s]);
    let company = Uuid::new_v4();
    let start = event("EmitStarted", company, json!({ "ticket_id": "T-E" }));
    let id = engine.on_event(&start).await.unwrap().started[0];
    let staged: Vec<(String, Option<String>, Option<String>, Value)> = sqlx::query_as(
        "SELECT event_type, correlation_id, causation_id, payload FROM wf.outbox_events WHERE aggregate_id = $1 ORDER BY event_type",
    )
    .bind(id.to_string())
    .fetch_all(&h.admin)
    .await
    .unwrap();
    assert_eq!(staged.len(), 2);
    assert_eq!(staged[0].0, "FlowDone");
    assert_eq!(staged[1].0, "TicketNoted");
    for row in &staged {
        assert_eq!(
            row.1.as_deref(),
            Some("corr-1"),
            "the start event's correlation id"
        );
        assert_eq!(
            row.2.as_deref(),
            Some(start.id.to_string().as_str()),
            "caused by the triggering event"
        );
        assert_eq!(row.3["ticket"], "T-E");
    }
    h.teardown().await;
}

#[tokio::test]
async fn a_condition_with_no_matching_branch_and_no_else_fails_the_instance_by_name() {
    let h = harness!();
    let mut s = spec(
        "Branching",
        "BranchStarted",
        true,
        vec![
            StepSpec {
                name: "pick".into(),
                guard: None,
                kind: StepKind::Condition {
                    branches: vec![Branch {
                        when: Some(Expr::Bin {
                            op: BinOp::Eq,
                            l: Box::new(ctx(&["ticket_id"])),
                            r: Box::new(Expr::Lit(json!("never"))),
                        }),
                        set: Vec::new(),
                        next: "done".into(),
                    }],
                },
                on_failure: None,
                call_timeout: None,
            },
            terminal("done", Terminal::Completed),
        ],
    );
    s.compensation = Vec::new();
    let engine = h.engine(vec![s]);
    let company = Uuid::new_v4();
    let id = engine
        .on_event(&event(
            "BranchStarted",
            company,
            json!({ "ticket_id": "T-C" }),
        ))
        .await
        .unwrap()
        .started[0];
    let (inst, _) = engine.describe(company, id).await.unwrap().unwrap();
    assert_eq!(inst.status, "failed");
    assert!(
        inst.last_error.as_deref().unwrap().contains("no branch"),
        "{:?}",
        inst.last_error
    );
    h.teardown().await;
}

#[tokio::test]
async fn the_background_tasks_deliver_subscribed_events_and_ignore_the_rest() {
    let h = harness!();
    let tasks =
        backbone_core::workflow::BackgroundTasks::new(Arc::new(h.engine(vec![escalation()])));
    assert_eq!(
        tasks.subscribed_events(),
        vec!["TicketRaised", "TicketAcked"]
    );
    let company = Uuid::new_v4();
    tasks
        .deliver(event(
            "SomethingElse",
            company,
            json!({ "ticket_id": "T-B" }),
        ))
        .await
        .unwrap();
    assert_eq!(
        h.instances().await,
        0,
        "an event nobody subscribed to starts nothing"
    );
    tasks
        .deliver(event(
            "TicketRaised",
            company,
            json!({ "ticket_id": "T-B" }),
        ))
        .await
        .unwrap();
    assert_eq!(h.instances().await, 1);
    assert_eq!(tasks.timer_sweep().await.unwrap().total(), 0);
    h.teardown().await;
}

#[tokio::test]
async fn the_latest_event_is_readable_by_the_steps_after_a_wait() {
    let h = harness!();
    let mut s = escalation();
    // The acknowledgement's own `note` is written by the step that follows the wait.
    for step in &mut s.steps {
        if let StepKind::Action { params, .. } = &mut step.kind {
            if step.name == "note_ack" {
                for (k, v) in params.iter_mut() {
                    if k == "text" {
                        *v = hole(Expr::Path {
                            root: Root::Event,
                            path: vec![Seg::Key("note".into())],
                        });
                    }
                }
            }
        }
    }
    let engine = h.engine(vec![s]);
    let company = Uuid::new_v4();
    engine
        .on_event(&event(
            "TicketRaised",
            company,
            json!({ "ticket_id": "T-EV", "note": "from the start event" }),
        ))
        .await
        .unwrap();
    engine
        .on_event(&event(
            "TicketAcked",
            company,
            json!({ "ticket_id": "T-EV", "note": "from the acknowledgement" }),
        ))
        .await
        .unwrap();
    assert_eq!(
        h.notes("T-EV").await,
        vec!["from the acknowledgement", "opened"],
        "after a wait, `event` is the event that ended it, not the one that started the instance"
    );
    h.teardown().await;
}
