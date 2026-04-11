//! Workflow step and context traits for saga-pattern workflows.
//!
//! Phase 0 generic base for the `flow.rs` generator (Category C).
//!
//! Every generated `{Name}Workflow` used to re-define the same
//! structural boilerplate. This module provides the single generic version so
//! that generated files collapse to type aliases:
//!
//! ```rust,ignore
//! use backbone_core::flow::{
//!     FlowError, FlowStatus, FlowInstance, FlowExecutor,
//!     WorkflowStep, WorkflowContext,
//! };
//! use crate::application::workflows::order_processing_workflow::OrderProcessingFlowStep;
//!
//! pub type OrderProcessingFlowError    = FlowError;
//! pub type OrderProcessingFlowStatus   = FlowStatus;
//! pub type OrderProcessingFlowInstance = FlowInstance<OrderProcessingFlowStep>;
//!
//! pub struct OrderProcessingFlowExecutor<H>(FlowExecutor<H>);
//! // impl execute_step() — the only unique part — stays in the generated file
//! ```

use std::sync::Arc;

// ─── FlowError ───────────────────────────────────────────────────────────────

/// Common workflow execution error — identical for every workflow.
///
/// Generated files use a type alias: `pub type OrderProcessingFlowError = FlowError;`
#[derive(Debug, Clone, thiserror::Error)]
pub enum FlowError {
    #[error("No current step to execute")]
    NoCurrentStep,

    #[error("Step execution failed: {0}")]
    StepFailed(String),

    #[error("Condition evaluation failed: {0}")]
    ConditionFailed(String),

    #[error("Compensation failed: {0}")]
    CompensationFailed(String),

    #[error("Flow timed out")]
    Timeout,

    #[error("Flow cancelled")]
    Cancelled,

    #[error("Invalid state transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },
}

// ─── FlowStatus ──────────────────────────────────────────────────────────────

/// Workflow execution status — identical for every workflow.
///
/// Generated files use a type alias: `pub type OrderProcessingFlowStatus = FlowStatus;`
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowStatus {
    /// Flow is pending execution
    Pending,
    /// Flow is currently running
    Running,
    /// Flow is waiting for an event or condition
    Waiting,
    /// Flow completed successfully
    Completed,
    /// Flow failed
    Failed,
    /// Flow was cancelled
    Cancelled,
    /// Flow is compensating (rolling back)
    Compensating,
}

// ─── FlowInstance ─────────────────────────────────────────────────────────────

/// Generic workflow instance parametrised over the step enum `S`.
///
/// Generated files use a type alias:
/// `pub type OrderProcessingFlowInstance = FlowInstance<OrderProcessingFlowStep>;`
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(bound(
    serialize = "S: serde::Serialize",
    deserialize = "S: serde::de::DeserializeOwned",
))]
pub struct FlowInstance<S>
where
    S: std::fmt::Debug + Clone + serde::Serialize + serde::de::DeserializeOwned,
{
    pub id: String,
    pub status: FlowStatus,
    pub current_step: Option<S>,
    pub context: serde_json::Value,
    pub completed_steps: Vec<S>,
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl<S> FlowInstance<S>
where
    S: std::fmt::Debug + Clone + serde::Serialize + serde::de::DeserializeOwned,
{
    pub fn new(id: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: id.into(),
            status: FlowStatus::Pending,
            current_step: None,
            context: serde_json::json!({}),
            completed_steps: Vec::new(),
            error: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(
            self.status,
            FlowStatus::Completed | FlowStatus::Failed | FlowStatus::Cancelled
        )
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status, FlowStatus::Running | FlowStatus::Waiting)
    }

    pub fn set_context(&mut self, key: &str, value: serde_json::Value) {
        if let serde_json::Value::Object(ref mut map) = self.context {
            map.insert(key.to_string(), value);
        }
        self.updated_at = chrono::Utc::now();
    }

    pub fn get_context(&self, key: &str) -> Option<&serde_json::Value> {
        if let serde_json::Value::Object(ref map) = self.context {
            map.get(key)
        } else {
            None
        }
    }
}

// ─── WorkflowContext impl on FlowInstance ────────────────────────────────────

impl<S> WorkflowContext<FlowInstance<S>> for FlowInstance<S>
where
    S: std::fmt::Debug + Clone + serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    fn entity(&self) -> &FlowInstance<S> { self }
    fn set_var(&mut self, key: &str, value: serde_json::Value) { self.set_context(key, value); }
    fn get_var(&self, key: &str) -> Option<&serde_json::Value> { self.get_context(key) }
}

// ─── FlowExecutor ────────────────────────────────────────────────────────────

/// Generic executor shell — holds the handler, provides cancel/fail helpers.
///
/// Generated files wrap this with a newtype and add the unique `execute_step()`:
///
/// ```rust,ignore
/// pub struct OrderProcessingFlowExecutor<H: OrderProcessingStepHandler>(FlowExecutor<H>);
///
/// impl<H: OrderProcessingStepHandler> OrderProcessingFlowExecutor<H> {
///     pub fn new(handler: Arc<H>) -> Self { Self(FlowExecutor::new(handler)) }
///     pub fn handler(&self) -> &Arc<H> { &self.0.handler }
///
///     pub async fn execute_step(&self, instance: &mut OrderProcessingFlowInstance)
///         -> Result<(), FlowError> { /* unique dispatch */ }
///
///     pub async fn start(&self, id: impl Into<String>)
///         -> Result<OrderProcessingFlowInstance, FlowError> { self.0.start(id) }
///
///     pub async fn run(&self, instance: &mut OrderProcessingFlowInstance)
///         -> Result<(), FlowError> {
///         while !instance.is_complete() && instance.status != FlowStatus::Waiting {
///             self.execute_step(instance).await?;
///         }
///         Ok(())
///     }
///
///     pub fn cancel(&self, instance: &mut OrderProcessingFlowInstance) {
///         self.0.cancel(instance);
///     }
///     pub fn fail(&self, instance: &mut OrderProcessingFlowInstance, error: impl Into<String>) {
///         self.0.fail(instance, error);
///     }
/// }
/// ```
pub struct FlowExecutor<H> {
    pub handler: Arc<H>,
}

impl<H: Send + Sync + 'static> FlowExecutor<H> {
    pub fn new(handler: Arc<H>) -> Self {
        Self { handler }
    }

    /// Start a new flow instance at a given step.
    pub fn start<S>(&self, id: impl Into<String>, first_step: S) -> FlowInstance<S>
    where
        S: std::fmt::Debug + Clone + serde::Serialize + serde::de::DeserializeOwned,
    {
        let mut instance = FlowInstance::new(id);
        instance.status = FlowStatus::Running;
        instance.current_step = Some(first_step);
        instance
    }

    /// Cancel the flow instance.
    pub fn cancel<S>(&self, instance: &mut FlowInstance<S>)
    where
        S: std::fmt::Debug + Clone + serde::Serialize + serde::de::DeserializeOwned,
    {
        instance.status = FlowStatus::Cancelled;
        instance.updated_at = chrono::Utc::now();
    }

    /// Mark the flow instance as failed with a message.
    pub fn fail<S>(&self, instance: &mut FlowInstance<S>, error: impl Into<String>)
    where
        S: std::fmt::Debug + Clone + serde::Serialize + serde::de::DeserializeOwned,
    {
        instance.status = FlowStatus::Failed;
        instance.error = Some(error.into());
        instance.updated_at = chrono::Utc::now();
    }
}

// ─── Traits ──────────────────────────────────────────────────────────────────

/// Workflow execution context for an entity `E`.
pub trait WorkflowContext<E>: Send + Sync {
    fn entity(&self) -> &E;
    fn set_var(&mut self, key: &str, value: serde_json::Value);
    fn get_var(&self, key: &str) -> Option<&serde_json::Value>;
}

/// A named step within a workflow.
pub trait WorkflowStep: Send + Sync {
    fn name(&self) -> &'static str;
}
