//! # backbone-gl-posting
//!
//! The GL-posting **wire contract** shared across Backbone producers — the serialized envelope a
//! producer (payment, payment-gateway, selling, inventory, billing, …) emits, and the sink port the
//! composition layer implements over accounting's `PostingService`. This crate owns the contract so
//! each producer stops carrying its own structurally-identical copy (ADR-001 §6's deferred cleanup).
//!
//! - A producer builds an [`AccountingPostEnvelope`] (balanced `Dr … · Cr …` lines) and reaches
//!   accounting ONLY through a [`GlPostSink`] — zero normal Cargo edge into backbone-accounting.
//! - A composition ACL implements `GlPostSink` and maps the envelope into accounting's `PostingRequest`
//!   (injecting the accounting-owned `cost_center`/`project`/`department` the envelope deliberately omits).
//! - Idempotency: accounting dedups on `source_id` (the producer document id, opaque to accounting).
//!
//! This crate is framework plumbing: it depends on `serde` + primitives only, never on a domain module
//! or accounting itself.
//!
//! The companion seam is the **reconciliation contract** ([`ReconcileSink`]): producers settle and
//! clear through reconciliation edges on accounting's journal lines via the same zero-edge posture —
//! a locator-based, connection-taking port implemented by the composition layer.

mod reconcile;

pub use reconcile::*;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One debit/credit line of a GL post.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlPostLine {
    pub account_id: Uuid,
    pub debit: Decimal,
    pub credit: Decimal,
    pub party_type: Option<String>,
    pub party_id: Option<Uuid>,
    pub description: Option<String>,
}

impl GlPostLine {
    pub fn debit(account_id: Uuid, amount: Decimal) -> Self {
        Self { account_id, debit: amount, credit: Decimal::ZERO, party_type: None, party_id: None, description: None }
    }
    pub fn credit(account_id: Uuid, amount: Decimal) -> Self {
        Self { account_id, debit: Decimal::ZERO, credit: amount, party_type: None, party_id: None, description: None }
    }
    pub fn with_party(mut self, party_type: &str, party_id: Uuid) -> Self {
        self.party_type = Some(party_type.to_string());
        self.party_id = Some(party_id);
        self
    }
    pub fn with_description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }
}

/// The serialized GL-post envelope a producer emits. `source_type` discriminates the producer
/// ("payment", "gateway_fee", "sales", …); `source_id` is the producer document id accounting dedups on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountingPostEnvelope {
    pub idempotency_key: String,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    /// Posting source discriminator — e.g. "payment" (settlement), "gateway_fee".
    pub source_type: String,
    /// The producer document id — opaque to accounting, used as the dedup key.
    pub source_id: Uuid,
    pub source_reference: Option<String>,
    pub posting_date: chrono::NaiveDate,
    pub currency: String,
    /// "original" | "reversal".
    pub posting_type: String,
    /// Set only when `posting_type == "reversal"`.
    pub reverses_post_id: Option<Uuid>,
    pub description: Option<String>,
    pub lines: Vec<GlPostLine>,
}

impl AccountingPostEnvelope {
    pub fn totals(&self) -> (Decimal, Decimal) {
        (self.lines.iter().map(|l| l.debit).sum(), self.lines.iter().map(|l| l.credit).sum())
    }
    pub fn is_balanced(&self) -> bool {
        let (d, c) = self.totals();
        d == c && !self.lines.is_empty()
    }
}

/// Accounting's ack: the resulting post + journal, and whether the idempotency key short-circuited.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlPostAck {
    pub post_id: Uuid,
    pub journal_id: Uuid,
    pub idempotent_reuse: bool,
}

/// Why accounting rejected a post (e.g. a closed period). The producer leaves the document retryable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlPostRejected {
    pub code: String,
    pub message: String,
}

/// The GL-posting seam. A composing service implements this over accounting's `PostingService`;
/// tests record. Producers never depend on backbone-accounting — they reach it only through this port.
#[async_trait::async_trait]
pub trait GlPostSink: Send + Sync {
    async fn post(&self, envelope: &AccountingPostEnvelope) -> Result<GlPostAck, GlPostRejected>;
}
