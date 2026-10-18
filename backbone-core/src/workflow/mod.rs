//! The workflow engine: declared `schema/workflows` that run.
//!
//! A workflow with `config.executor: engine` is an event-started process manager. The schema
//! generator compiles its steps into a [`WorkflowSpec`] (data, with its conditions and templates
//! as [`expr`] trees) beside a [`WorkflowDefinition`] that performs what only typed code can; the
//! [`WorkflowEngine`] runs the spec over Postgres: a persisted instance and step log, durable
//! waits, timers, retries, compensation, and a deadline.
//!
//! - [`expr`]: the closed expression set, and the evaluator.
//! - [`spec`]: the compiled workflow and the definition trait.
//! - [`schema`]: the tables, as SQL.
//! - `engine` (feature `postgres`): the engine itself.
//!
//! This module supersedes [`crate::flow`], the in-memory instance and executor shell nothing used.

pub mod expr;
pub mod schema;
pub mod spec;

#[cfg(feature = "postgres")]
pub mod engine;

pub use expr::{Dur, Expr, Funcs, NoFuncs, Scope, Template, Tpl};
pub use spec::{Call, StepKind, StepSpec, WorkflowDefinition, WorkflowSpec};

#[cfg(feature = "postgres")]
pub use engine::{
    BackgroundTasks, EngineConfig, EventReport, InstanceRow, StepRow, SweepReport, Tenant,
    WorkflowEngine, WorkflowError,
};
