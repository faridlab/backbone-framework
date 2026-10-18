//! What the schema generator writes for a workflow: its steps as data, and the one trait that
//! performs what only typed code can.
//!
//! A [`WorkflowSpec`] is the compiled form of a `schema/workflows/<name>.workflow.yaml` with
//! `config.executor: engine`. The generated module carries it as JSON (`WorkflowSpec::from_json`)
//! beside a [`WorkflowDefinition`] implementation, which does the three things a spec cannot say:
//! call an entity's generated service (create, update, delete, query, fetch), move a record
//! through its state machine, and call the hand-written verbs and functions of the workflow's
//! `Actions` trait.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::expr::{Dur, Expr, Tpl};
use crate::service::ServiceResult;
use crate::write_hooks::WriteCtx;

/// How long to wait between attempts of a step that failed for a transient reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backoff {
    /// Doubling from the engine's base up to its cap.
    Exponential,
    /// The same wait every time, in seconds.
    Fixed(i64),
}

/// The retry policy of a workflow or of one step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrySpec {
    /// Attempts in all, the first included.
    pub max_attempts: u32,
    /// The wait between them.
    pub backoff: Backoff,
}

/// What a step does when it succeeds (or when a wait is answered or times out).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    /// Context keys to set, evaluated in order.
    #[serde(default)]
    pub set: Vec<(String, Tpl)>,
    /// The step to go to. `None` ends the run of this step without a successor, which is an
    /// error for anything but a terminal.
    #[serde(default)]
    pub next: Option<String>,
}

/// What a step does when it fails.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FailureSpec {
    /// The step to go to.
    #[serde(default)]
    pub next: Option<String>,
    /// Attempts for a transient failure; the workflow's, else the engine's, when absent.
    #[serde(default)]
    pub retry: Option<RetrySpec>,
    /// What to do once the attempts are used up, when it differs from `next`.
    #[serde(default)]
    pub on_exhausted: Option<Outcome>,
}

/// One branch of a condition step; the first whose `when` holds is taken, a branch with no `when`
/// is the `else`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Branch {
    /// The condition.
    #[serde(default)]
    pub when: Option<Expr>,
    /// Context keys to set when it is taken.
    #[serde(default)]
    pub set: Vec<(String, Tpl)>,
    /// Where it goes.
    pub next: String,
}

/// One equality of a wait's condition: the event's field must equal the context expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaitKey {
    /// A top-level field of the event's payload.
    pub event_field: String,
    /// What it must equal, read from the context when the wait is entered.
    pub context: Expr,
}

/// How a terminal step ends the instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Terminal {
    /// `completed`.
    Completed,
    /// `failed`.
    Failed,
    /// `expired`.
    Expired,
    /// `cancelled`.
    Cancelled,
}

/// An event a terminal step stages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmitSpec {
    /// The event type.
    pub event: String,
    /// The payload.
    pub data: Tpl,
}

/// What a step is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StepKind {
    /// An action: an engine action (`audit_log`, `emit_event`, `send_email`, `notify`,
    /// `send_notification`, `send_sms`), or one the [`WorkflowDefinition`] performs.
    Action {
        /// The verb.
        action: String,
        /// The entity of an entity action.
        #[serde(default)]
        entity: Option<String>,
        /// The record an update, delete or fetch is about.
        #[serde(default)]
        id: Option<Tpl>,
        /// The parameters, rendered when the step runs.
        #[serde(default)]
        params: Vec<(String, Tpl)>,
        /// What a success does.
        on_success: Outcome,
        /// What a hand call that outlasts the step's `timeout` does, instead of failing.
        #[serde(default)]
        on_timeout: Option<Outcome>,
    },
    /// A state-machine verb on a record.
    Transition {
        /// The entity.
        entity: String,
        /// The record.
        id: Tpl,
        /// The verb.
        verb: String,
        /// Extra fields written with the move.
        #[serde(default)]
        params: Vec<(String, Tpl)>,
        /// What a success does.
        on_success: Outcome,
    },
    /// A branch on conditions, in order.
    Condition {
        /// The branches.
        branches: Vec<Branch>,
    },
    /// A durable wait.
    Wait {
        /// The event waited for; `None` is a pure timer.
        #[serde(default)]
        event: Option<String>,
        /// The equalities that match an event to this instance.
        #[serde(default)]
        key: Vec<WaitKey>,
        /// How long to wait.
        #[serde(default)]
        timeout: Option<Dur>,
        /// What an event does.
        #[serde(default)]
        on_event: Option<Outcome>,
        /// What the timeout does.
        #[serde(default)]
        on_timeout: Option<Outcome>,
    },
    /// An end.
    Terminal {
        /// How it ends.
        status: Terminal,
        /// Why, for a person reading the log.
        #[serde(default)]
        reason: Option<Tpl>,
        /// An event to stage.
        #[serde(default)]
        emit: Option<EmitSpec>,
        /// Run compensation after ending as failed.
        #[serde(default)]
        compensate: bool,
        /// A value stored as `context.result` when the instance ends.
        #[serde(default)]
        result: Option<Tpl>,
    },
}

/// One step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepSpec {
    /// Its name, unique in the workflow.
    pub name: String,
    /// A guard: when it does not hold the step is skipped and the run goes to `on_success.next`.
    #[serde(default)]
    pub guard: Option<Expr>,
    /// What it is.
    #[serde(flatten)]
    pub kind: StepKind,
    /// What a failure does.
    #[serde(default)]
    pub on_failure: Option<FailureSpec>,
    /// Seconds a hand call may take.
    #[serde(default)]
    pub timeout: Option<i64>,
}

/// One compensation step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompStep {
    /// Its name; the step log records it as `compensate:<name>`.
    pub name: String,
    /// It runs only when this holds (`steps.<name>.completed == true`).
    #[serde(default)]
    pub when: Option<Expr>,
    /// The verb.
    pub action: String,
    /// The entity of an entity action.
    #[serde(default)]
    pub entity: Option<String>,
    /// The record.
    #[serde(default)]
    pub id: Option<Tpl>,
    /// The parameters.
    #[serde(default)]
    pub params: Vec<(String, Tpl)>,
    /// The state-machine verb, when the action is `transition`.
    #[serde(default)]
    pub verb: Option<String>,
}

/// The compiled workflow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowSpec {
    /// Its name (`PasswordReset`).
    pub name: String,
    /// Its version, from the file.
    pub version: i32,
    /// The event that starts an instance.
    pub trigger_event: String,
    /// Context keys filled from the start event: `company_id: event.company_id`.
    #[serde(default)]
    pub extract: Vec<(String, Expr)>,
    /// The context an instance starts with, before `extract`.
    #[serde(default)]
    pub context: Vec<(String, Value)>,
    /// `false` runs the whole flow in one transaction and allows no wait.
    pub persistence: bool,
    /// How long an instance may live; passing it while waiting ends it `expired`.
    #[serde(default)]
    pub timeout: Option<Dur>,
    /// The workflow-wide retry policy.
    #[serde(default)]
    pub retry: Option<RetrySpec>,
    /// The first step is the first of these.
    pub steps: Vec<StepSpec>,
    /// Compensation, in declaration order; it runs in reverse.
    #[serde(default)]
    pub compensation: Vec<CompStep>,
}

impl WorkflowSpec {
    /// Read the JSON a generated module carries.
    ///
    /// # Panics
    /// When the text is not a spec: the generator wrote it, so that is a bug of the generator or a
    /// version mismatch between it and this crate, and it is loudest at startup.
    pub fn from_json(text: &str) -> Self {
        match serde_json::from_str(text) {
            Ok(spec) => spec,
            Err(e) => panic!("a generated workflow spec does not parse (generator and backbone-core disagree): {e}"),
        }
    }

    /// The step called `name`.
    pub fn step(&self, name: &str) -> Option<&StepSpec> {
        self.steps.iter().find(|s| s.name == name)
    }

    /// The events this workflow reacts to: its trigger and every event a wait names.
    pub fn events(&self) -> Vec<&str> {
        let mut out = vec![self.trigger_event.as_str()];
        for step in &self.steps {
            if let StepKind::Wait { event: Some(e), .. } = &step.kind {
                if !out.contains(&e.as_str()) {
                    out.push(e.as_str());
                }
            }
        }
        out
    }
}

/// What a [`WorkflowDefinition`] is asked to perform.
#[derive(Debug)]
pub struct Call<'a> {
    /// The step (or `compensate:<name>`) asking. A generated definition dispatches on it.
    pub step: &'a str,
    /// The verb.
    pub action: &'a str,
    /// The entity of an entity action or a transition.
    pub entity: Option<&'a str>,
    /// The record, rendered from the step's `id`, or from `params.id`.
    pub id: Option<String>,
    /// The state-machine verb of a transition step.
    pub verb: Option<&'a str>,
    /// The parameters, rendered.
    pub params: &'a Value,
    /// The instance's context (the `wf` of a hand verb).
    pub context: &'a Value,
}

/// A workflow the engine can run: its spec, and what only typed code performs.
#[async_trait]
pub trait WorkflowDefinition: Send + Sync {
    /// The compiled workflow.
    fn spec(&self) -> &WorkflowSpec;

    /// Perform an action the engine does not: an entity action through the entity's generated
    /// service, a transition, or a hand-written verb. The answer is the step's `result`. An error
    /// of a kind a caller can fix (`NotFound`, `Validation`, `Violations`, `AlreadyExists`) is a
    /// business failure and is not retried; any other is transient and is.
    async fn perform(&self, ctx: &WriteCtx, call: &Call<'_>) -> ServiceResult<Value>;

    /// Call a function the workflow declares under `functions:`.
    async fn function(&self, ctx: &WriteCtx, name: &str, args: &[Value]) -> ServiceResult<Value> {
        let _ = (ctx, args);
        Err(crate::service::ServiceError::Internal(format!(
            "function `{name}` is not implemented"
        )))
    }
}
