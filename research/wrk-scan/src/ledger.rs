//! Dual ledgers for the independent oracle (SPEC §6.1, APP-A `ledger.rs`,
//! file SHA256 33d154de…, reused verbatim).
//!
//! A. RAW_EFFECT_LOG — appended at real effect boundaries by executor
//!    code (never "I am done" self-reports: each record corresponds to a
//!    definite effect that just happened on the appending thread's stack).
//! B. OWNER_TERMINAL_ACK_LOG — per-item terminal ACKs appended by the
//!    item's actual terminal owner, plus the global quiescence receipt.
//!
//! Causal order: ONE ledger mutex assigns a single global sequence to
//! every record. The sequence IS the serialization order — a verifiable
//! synchronization boundary, not a wall-clock timestamp. Cross-thread
//! ordering claims below are only ever made in terms of this sequence.

use std::sync::Mutex;

/// Which log a record belongs to (both live in one serialization domain).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogKind {
    Effect,
    OwnerAck,
}

/// Effect kinds recorded at real effect boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectKind {
    /// Admission linearization: the mechanism took ownership of the item
    /// (bounded slot acquired / enqueue that creates the obligation).
    ItemAdmitted,
    /// Synthetic logical I/O wait completed for the item.
    IoWaitDone,
    /// CPU transformation completed for the item (payload = metadata mix).
    TransformDone,
    /// Item result published into the fan-in sink (payload = result digest).
    ResultPublished,
    /// Aggregate (fan-in) published for a batch.
    AggregatePublished,
    /// Cancel intent recorded for a group (payload = group id).
    CancelRequested,
    /// Executor thread spawned / exited at its true lifecycle boundary.
    ExecutorSpawned,
    ExecutorExited,
    /// Global quiescence receipt (must be the last record, appended by
    /// the shutdown owner only after every executor exit is recorded).
    QuiescenceReceipt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalKind {
    CompletedOk,
    CompletedErr(u32),
    Abandoned,
}

/// One record in the unified serialization domain.
#[derive(Clone, Debug)]
pub struct LedgerRecord {
    /// Global sequence — assigned under the ledger mutex; defines the
    /// causal/serialization order used by every oracle check.
    pub seq: u64,
    pub log: LogKind,
    pub batch_id: u64,
    /// None for executor-level records (spawn/exit/quiescence).
    pub item_id: Option<u32>,
    pub kind: EffectKind,
    /// Payload digest (u64 folded) for content checks; group id for
    /// CancelRequested; executor id for Executor{Spawned,Exited}.
    pub payload: u64,
    /// Terminal classification (ACK records only).
    pub terminal: Option<TerminalKind>,
    /// Identity of the appending owner (executor id or coordinator id).
    pub owner: u64,
}

struct LedgerState {
    next_seq: u64,
    records: Vec<LedgerRecord>,
}

/// The dual ledger. Appends are serialized under one mutex; `seq` order
/// is the single source of cross-thread ordering truth.
pub struct Ledger {
    inner: Mutex<LedgerState>,
}

impl Ledger {
    pub fn new() -> Self {
        Ledger {
            inner: Mutex::new(LedgerState {
                next_seq: 1,
                records: Vec::new(),
            }),
        }
    }

    fn append_locked(state: &mut LedgerState, mut rec: LedgerRecord) -> u64 {
        let seq = state.next_seq;
        state.next_seq += 1;
        rec.seq = seq;
        state.records.push(rec);
        seq
    }

    /// Append an effect-boundary record; returns its global sequence.
    pub fn effect(
        &self,
        batch_id: u64,
        item_id: Option<u32>,
        kind: EffectKind,
        payload: u64,
        owner: u64,
    ) -> u64 {
        let mut st = self.inner.lock().unwrap();
        Self::append_locked(
            &mut st,
            LedgerRecord {
                seq: 0,
                log: LogKind::Effect,
                batch_id,
                item_id,
                kind,
                payload,
                terminal: None,
                owner,
            },
        )
    }

    /// Append an owner terminal ACK record; returns its global sequence.
    pub fn terminal_ack(
        &self,
        batch_id: u64,
        item_id: u32,
        terminal: TerminalKind,
        result_digest: u64,
        owner: u64,
    ) -> u64 {
        let mut st = self.inner.lock().unwrap();
        Self::append_locked(
            &mut st,
            LedgerRecord {
                seq: 0,
                log: LogKind::OwnerAck,
                batch_id,
                item_id: Some(item_id),
                kind: EffectKind::ResultPublished, // ACKs mirror the result publication
                payload: result_digest,
                terminal: Some(terminal),
                owner,
            },
        )
    }

    /// Snapshot all records in sequence order (post-run adjudication).
    pub fn snapshot(&self) -> Vec<LedgerRecord> {
        self.inner.lock().unwrap().records.clone()
    }
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}
