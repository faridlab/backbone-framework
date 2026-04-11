//! Generic trigger handler trait and types for entity lifecycle events.
//!
//! Phase 0 generic base for the `trigger.rs` generator (Category C).
//!
//! Every generated `{Model}TriggerHandler` used to re-define the same
//! boilerplate. This module provides the single generic version so that
//! generated files collapse to type aliases:
//!
//! ```rust,ignore
//! use backbone_core::trigger::{TriggerEvent, TriggerContext, TriggerContextMut,
//!                               TriggerRegistry, ActionExecutor, TriggerHandler};
//! use crate::domain::entity::Order;
//!
//! pub type OrderTriggerEvent      = TriggerEvent;
//! pub type OrderTriggerContext    = TriggerContext<Order>;
//! pub type OrderTriggerContextMut = TriggerContextMut<Order>;
//! pub type OrderActionExecutor    = ActionExecutor;
//! pub type OrderTriggerRegistry   = TriggerRegistry<Order>;
//! pub type OrderTriggerHandlerObj = dyn TriggerHandler<TriggerContext<Order>, TriggerEvent>;
//! ```

use std::sync::Arc;
use async_trait::async_trait;

// ─── Handler trait ───────────────────────────────────────────────────────────

/// Generic lifecycle trigger handler.
///
/// - `Ctx` — entity-specific trigger context (e.g. `TriggerContext<Order>`)
/// - `Evt` — trigger event type (e.g. `TriggerEvent`)
#[async_trait]
pub trait TriggerHandler<Ctx, Evt>: Send + Sync {
    /// Events this handler responds to.
    fn events(&self) -> Vec<Evt>;

    /// Handle the trigger event.
    async fn handle(&self, ctx: &Ctx) -> anyhow::Result<()>;

    /// Execution priority — lower values run first. Default: 0.
    fn priority(&self) -> i32 { 0 }

    /// Whether to keep dispatching after this handler errors. Default: false.
    fn continue_on_error(&self) -> bool { false }
}

// ─── Shared event enum ───────────────────────────────────────────────────────

/// Lifecycle trigger events — identical for every entity.
///
/// Generated files use a type alias: `pub type OrderTriggerEvent = TriggerEvent;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerEvent {
    BeforeCreate,
    AfterCreate,
    BeforeUpdate,
    AfterUpdate,
    BeforeDelete,
    AfterDelete,
    OnTransition { from: String, to: String, transition: String },
    OnEnterState(String),
    OnExitState(String),
}

impl TriggerEvent {
    pub fn is_before_event(&self) -> bool {
        matches!(self, Self::BeforeCreate | Self::BeforeUpdate | Self::BeforeDelete)
    }
}

// ─── Generic context ─────────────────────────────────────────────────────────

/// Immutable trigger context passed to handlers.
#[derive(Debug, Clone)]
pub struct TriggerContext<T: Clone> {
    pub event: TriggerEvent,
    pub entity: T,
    pub previous: Option<T>,
    pub user_id: Option<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl<T: Clone> TriggerContext<T> {
    pub fn new(event: TriggerEvent, entity: T) -> Self {
        Self { event, entity, previous: None, user_id: None, metadata: std::collections::HashMap::new() }
    }

    pub fn with_previous(mut self, previous: T) -> Self {
        self.previous = Some(previous);
        self
    }

    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn is_before_event(&self) -> bool {
        self.event.is_before_event()
    }
}

// ─── Generic mutable context ─────────────────────────────────────────────────

/// Mutable trigger context for before-hooks that may modify the entity.
#[derive(Debug)]
pub struct TriggerContextMut<T: Clone> {
    pub event: TriggerEvent,
    pub entity: T,
    pub previous: Option<T>,
    pub user_id: Option<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl<T: Clone> TriggerContextMut<T> {
    pub fn new(event: TriggerEvent, entity: T) -> Self {
        Self { event, entity, previous: None, user_id: None, metadata: std::collections::HashMap::new() }
    }

    pub fn into_entity(self) -> T {
        self.entity
    }

    pub fn into_context(self) -> TriggerContext<T> {
        TriggerContext {
            event: self.event,
            entity: self.entity,
            previous: self.previous,
            user_id: self.user_id,
            metadata: self.metadata,
        }
    }
}

// ─── Shared action executor ───────────────────────────────────────────────────

/// Shared executor for side-effect actions (email, notify, webhook, emit).
///
/// All methods are stub implementations pending real integrations.
pub struct ActionExecutor;

impl ActionExecutor {
    pub fn new() -> Self { Self }

    pub async fn send_email<C>(&self, _ctx: &C, template: &str) -> anyhow::Result<()> {
        tracing::info!("Sending email with template: {}", template);
        Ok(())
    }

    pub async fn notify<C>(&self, _ctx: &C, channel: &str) -> anyhow::Result<()> {
        tracing::info!("Sending notification to channel: {}", channel);
        Ok(())
    }

    pub async fn webhook<C>(&self, _ctx: &C, url: &str) -> anyhow::Result<()> {
        tracing::info!("Calling webhook: {}", url);
        Ok(())
    }

    pub async fn emit<C>(&self, _ctx: &C, event_name: &str) -> anyhow::Result<()> {
        tracing::info!("Emitting event: {}", event_name);
        Ok(())
    }
}

impl Default for ActionExecutor {
    fn default() -> Self { Self::new() }
}

// ─── Generic registry ────────────────────────────────────────────────────────

/// Registry that dispatches trigger events to registered handlers.
pub struct TriggerRegistry<T: Clone + Send + Sync + 'static> {
    handlers: Vec<Arc<dyn TriggerHandler<TriggerContext<T>, TriggerEvent>>>,
}

impl<T: Clone + Send + Sync + 'static> TriggerRegistry<T> {
    pub fn new() -> Self {
        Self { handlers: Vec::new() }
    }

    pub fn register(&mut self, handler: Arc<dyn TriggerHandler<TriggerContext<T>, TriggerEvent>>) {
        self.handlers.push(handler);
        self.handlers.sort_by_key(|h| h.priority());
    }

    pub async fn execute(&self, ctx: &TriggerContext<T>) -> anyhow::Result<()> {
        for handler in &self.handlers {
            if !handler.events().iter().any(|e| event_matches(e, &ctx.event)) {
                continue;
            }
            match handler.handle(ctx).await {
                Ok(()) => {}
                Err(e) => {
                    tracing::error!("Trigger handler error: {:?}", e);
                    if !handler.continue_on_error() {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn with_defaults() -> Self {
        Self::new()
    }

    /// Build a registry by registering handlers via a closure.
    ///
    /// ```rust,ignore
    /// let registry = TriggerRegistry::<Order>::build(|r| {
    ///     r.register(Arc::new(OrderAfterCreateHandler::new()));
    /// });
    /// ```
    pub fn build(f: impl FnOnce(&mut Self)) -> Self {
        let mut r = Self::new();
        f(&mut r);
        r
    }
}

impl<T: Clone + Send + Sync + 'static> Default for TriggerRegistry<T> {
    fn default() -> Self { Self::with_defaults() }
}

fn event_matches(a: &TriggerEvent, b: &TriggerEvent) -> bool {
    match (a, b) {
        (TriggerEvent::BeforeCreate,  TriggerEvent::BeforeCreate)  => true,
        (TriggerEvent::AfterCreate,   TriggerEvent::AfterCreate)   => true,
        (TriggerEvent::BeforeUpdate,  TriggerEvent::BeforeUpdate)  => true,
        (TriggerEvent::AfterUpdate,   TriggerEvent::AfterUpdate)   => true,
        (TriggerEvent::BeforeDelete,  TriggerEvent::BeforeDelete)  => true,
        (TriggerEvent::AfterDelete,   TriggerEvent::AfterDelete)   => true,
        (TriggerEvent::OnEnterState(x), TriggerEvent::OnEnterState(y)) => x == y,
        (TriggerEvent::OnExitState(x),  TriggerEvent::OnExitState(y))  => x == y,
        (TriggerEvent::OnTransition { transition: t1, .. },
         TriggerEvent::OnTransition { transition: t2, .. }) => t1 == t2,
        _ => false,
    }
}
