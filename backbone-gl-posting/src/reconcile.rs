//! The reconciliation wire contract — the seam producers use to create and unlink
//! debit↔credit reconciliation edges on accounting's journal lines, mirroring how
//! [`crate::GlPostSink`] carries GL posts.
//!
//! - A caller identifies each side by **locator** (producer source + document id + the reconcilable
//!   account both lines must share), never by journal-line id — producers stay blind to accounting
//!   internals. The sink resolves a locator to exactly one posted line.
//! - [`ReconcileSink::reconcile_pair_on`] is **connection-taking**: the edge commits atomically
//!   with the caller's unit of work (settlement, clearing). All schemas co-locate in one database,
//!   so a caller-held transaction may write accounting tables; the caller's company binding applies.
//! - [`ReconcileSink::unreconcile_pair_on`] is **side-effecting** (the Odoo unlink contract): it
//!   reverses the moves the partial generated (exchange difference today, cash-basis tax deferrals
//!   when those land), dissolves or repairs full-reconcile groups, and only then removes the edge —
//!   never a bare DELETE.
//! - Amounts are clamped to the smaller residual on each side; an over-settlement therefore leaves
//!   the unapplied remainder unreconciled on the credit line (the on-account credit).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Locates one side of a reconciliation edge by producer identity. Resolves to exactly one posted
/// journal line on `account_id` — the reconcilable control account BOTH sides must share.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconcileLine {
    /// Producer discriminator stamped on the journal — e.g. "order" (sales invoice),
    /// "expense" (purchase invoice), "payment", "settlement" (bank clearance), "manual".
    pub source_type: String,
    /// The producer document id the journal was posted from.
    pub source_id: Uuid,
    /// The reconcilable account both lines of the pair sit on.
    pub account_id: Uuid,
    /// `true` resolves to the reversal journal of the source (journals.is_reversing), not the original.
    pub reversing: bool,
}

impl ReconcileLine {
    pub fn new(source_type: &str, source_id: Uuid, account_id: Uuid) -> Self {
        Self { source_type: source_type.to_string(), source_id, account_id, reversing: false }
    }
    pub fn reversing(mut self) -> Self {
        self.reversing = true;
        self
    }
}

/// What created the edge — stamped on the partial for traceability.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReconcileOrigin {
    /// A settlement applied an invoice receivable/payable against a payment.
    Settlement,
    /// A bank clearance matched a payment's clearing line against the clearance leg.
    Clearing,
    /// An accountant's manual match.
    Manual,
}

/// Create (or grow) the debit↔credit edge between two located lines.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconcilePairRequest {
    pub company_id: Uuid,
    pub debit: ReconcileLine,
    pub credit: ReconcileLine,
    /// Requested amount in company currency; the sink clamps to the smaller line residual.
    pub amount: Decimal,
    pub origin: ReconcileOrigin,
}

/// The resulting edge. `applied` is the post-clamp amount actually reconciled.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconcileEdgeAck {
    /// `None` when the requested amount clamped to zero — no edge exists, and the
    /// unapplied remainder stays unreconciled on the credit line (the on-account credit).
    pub partial_id: Option<Uuid>,
    pub applied: Decimal,
    /// Set when this edge completed the pair — every connected line reached zero residual.
    pub full_reconcile_id: Option<Uuid>,
}

/// Unlink every partial between the located pair (side-effecting — see the module docs).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnreconcilePairRequest {
    pub company_id: Uuid,
    pub debit: ReconcileLine,
    pub credit: ReconcileLine,
}

/// Why the sink refused (guard violation, unresolvable locator, unconfigured FX account …).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconcileRejected {
    pub code: String,
    pub message: String,
}

/// The reconciliation seam. A composing service implements this over accounting's reconciliation
/// write service; producers never depend on backbone-accounting — they reach the graph only
/// through this port, exactly as they reach posting through [`crate::GlPostSink`].
#[async_trait::async_trait]
pub trait ReconcileSink: Send + Sync {
    /// Create a partial edge riding the caller's transaction.
    async fn reconcile_pair_on(
        &self,
        conn: &mut sqlx::PgConnection,
        req: &ReconcilePairRequest,
    ) -> Result<ReconcileEdgeAck, ReconcileRejected>;

    /// Side-effecting unlink riding the caller's transaction: reverses generated moves,
    /// repairs groups, removes the partials between the pair.
    ///
    /// **Contract: reversals are pair-complete.** The unlink removes EVERY partial between
    /// the located pair, whatever their origin and amount — it is not scoped to a subset of
    /// the pair's history. A producer must therefore only call this for a pair whose edges it
    /// is undoing IN FULL: cancel events must carry the complete allocation set for each
    /// affected pair (an all-or-nothing document reversal), never a partial subset — a
    /// subset would restore the caller's cached bookkeeping by less than the graph reopens,
    /// silently diverging the two. The void return reflects today's producers (each pair
    /// carries one allocation, one edge); a future producer needing amount-scoped reversal
    /// requires a richer ack first.
    async fn unreconcile_pair_on(
        &self,
        conn: &mut sqlx::PgConnection,
        req: &UnreconcilePairRequest,
    ) -> Result<(), ReconcileRejected>;
}
