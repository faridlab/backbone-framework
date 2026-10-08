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
pub struct WriteCtx<'a, E> {
    pub kind: WriteKind,
    /// The row as stored before the write; `None` on create.
    pub before: Option<&'a E>,
    /// The row the write would store; `None` on delete and hard delete.
    pub after: Option<&'a E>,
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
