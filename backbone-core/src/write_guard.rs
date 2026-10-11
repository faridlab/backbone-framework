//! The write guard: the rules a write must satisfy, asked on every path the
//! generic service writes through.
//!
//! [`GenericCrudService`](crate::service::GenericCrudService) consults its
//! guard on create, update (full and partial, single and bulk), soft delete,
//! restore and hard delete, after the write's resulting row is known and
//! before anything is persisted. A guard answers with violations to refuse and
//! violations to only report: a rule that runs in shadow is logged and the
//! write proceeds, so a newly compiled rule shows what it would refuse before
//! it refuses anything. The schema generator compiles a module's declared rules
//! into guards; without one, every write is allowed ([`AllowAll`]).

use async_trait::async_trait;

use crate::violation::Violation;

/// What kind of write is being checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteKind {
    Create,
    /// A full or partial update.
    Update,
    /// A soft delete.
    Delete,
    Restore,
    /// Removal of a soft-deleted row for good.
    HardDelete,
}

/// One write, as the guard sees it.
#[derive(Debug)]
#[non_exhaustive]
pub struct WriteCtx<'a, E> {
    pub kind: WriteKind,
    /// The row as stored before the write; `None` on create.
    pub before: Option<&'a E>,
    /// The row the write would store; `None` on delete and hard delete.
    pub after: Option<&'a E>,
    /// Who is writing: the request's audit attribution, when the composing
    /// service bound one; `None` for a job, a seed or an unscoped caller.
    pub actor: Option<String>,
    /// When the write happens, the `now()` a rule compares against.
    pub at: chrono::DateTime<chrono::Utc>,
    /// The state-machine verb this write performs (`post`, `approve`), when it runs inside
    /// [`as_transition`]; `None` for a generic write, which must not move a state field.
    pub transition: Option<String>,
}

impl<'a, E> WriteCtx<'a, E> {
    /// The context of one write, attributed to the ambient request's actor
    /// and timed now.
    pub fn new(kind: WriteKind, before: Option<&'a E>, after: Option<&'a E>) -> Self {
        Self {
            kind,
            before,
            after,
            actor: backbone_orm::audit_context::current_request_audit()
                .map(|a| a.actor)
                .filter(|a| !a.is_empty()),
            at: chrono::Utc::now(),
            transition: current_transition(),
        }
    }
}

tokio::task_local! {
    static TRANSITION: String;
}

/// Run `f` as the state-machine verb `verb`: every write it makes is a transition, so a rule
/// that keeps a state field verbs-only lets it through. A generated transition handler wraps
/// its write in this; nothing else should.
pub async fn as_transition<F: std::future::Future>(verb: &str, f: F) -> F::Output {
    TRANSITION.scope(verb.to_string(), f).await
}

/// The verb the current write performs, inside [`as_transition`].
pub fn current_transition() -> Option<String> {
    TRANSITION.try_with(|v| v.clone()).ok()
}

/// A guard's answer.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct GuardOutcome {
    /// Broken rules that refuse the write.
    pub refuse: Vec<Violation>,
    /// Broken rules running in shadow: reported, not refused.
    pub shadow: Vec<Violation>,
}

impl GuardOutcome {
    pub fn allow() -> Self {
        Self::default()
    }

    /// Fold another guard's answer into this one.
    pub fn merge(mut self, other: GuardOutcome) -> Self {
        self.refuse.extend(other.refuse);
        self.shadow.extend(other.shadow);
        self
    }
}

/// The rules one entity's writes must satisfy.
#[async_trait]
pub trait WriteGuard<E>: Send + Sync {
    async fn check(&self, ctx: &WriteCtx<'_, E>) -> GuardOutcome;
}

/// A guard that refuses nothing: the default for an entity whose schema
/// declares no rules.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowAll;

#[async_trait]
impl<E: Sync> WriteGuard<E> for AllowAll {
    async fn check(&self, _ctx: &WriteCtx<'_, E>) -> GuardOutcome {
        GuardOutcome::allow()
    }
}

#[cfg(test)]
mod transition_tests {
    use super::*;

    #[tokio::test]
    async fn a_write_knows_whether_a_verb_performs_it() {
        let row = 1u8;
        assert_eq!(WriteCtx::new(WriteKind::Update, Some(&row), Some(&row)).transition, None);
        let inside = as_transition("post", async { WriteCtx::new(WriteKind::Update, Some(&row), Some(&row)).transition }).await;
        assert_eq!(inside.as_deref(), Some("post"));
    }
}
