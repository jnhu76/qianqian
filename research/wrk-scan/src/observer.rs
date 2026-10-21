//! ObserverSink — the dual-mode observation boundary between the
//! WorkloadCore/H0 executor and its evidence recording (Gate 授权 §2).
//!
//! ON  = the frozen dual ledger (APP-A `ledger.rs`): full effect records,
//!       oracle-adjudicable via O1–O10.
//! OFF = the minimal receipt (SPEC §7.5: "每项 admission/start/terminal
//!       三个时间戳的最小仪器"): compact per-item state, monotonic
//!       ordinals (assigned under the receipt lock — the OFF-side
//!       serialization boundary), no per-effect records.
//!
//! SAME-IMPLEMENTATION RULE: both modes are the same binary, the same
//! WorkloadCore, the same H0 executor and the same lifecycle code path.
//! The executor calls the SAME sink methods at the SAME effect boundaries
//! in both modes; only the recording behavior differs. No business
//! obligation (wait, CPU blocks, fault, cancel, terminal, join, cleanup)
//! is ever skipped in OFF — OFF changes what is WRITTEN DOWN, never what
//! is DONE.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Instant;

use crate::ledger::{EffectKind, Ledger, TerminalKind};

/// Minimal per-item receipt (OFF mode).
#[derive(Clone, Debug)]
pub struct OffItem {
    pub group: u32,
    pub admitted_ordinal: u64,
    pub admitted_ts: Instant,
    /// Execution start (worker dequeue), OFF-retained timestamp.
    pub start_ts: Option<Instant>,
    /// Wait-effect count — retained so an OFF run can prove it did not
    /// skip the wait obligation (equivalence + shadow audit).
    pub wait_done_count: u32,
    pub terminal: Option<TerminalKind>,
    pub terminal_ordinal: Option<u64>,
    pub terminal_ts: Option<Instant>,
    pub result_contribution: Option<u64>,
    pub owner: u64,
}

/// Minimal session receipt (OFF mode).
#[derive(Clone, Debug, Default)]
pub struct OffReceipt {
    /// Monotonic record ordinal, assigned under the receipt mutex; the
    /// OFF-side serialization boundary (admit/cancel/terminal ordering
    /// judgements inside an OFF run use this, never wall-clock).
    pub ordinal: u64,
    pub items: BTreeMap<u32, OffItem>,
    /// (group, ordinal) of each recorded cancel cut.
    pub cancels: Vec<(u32, u64)>,
    pub workers_spawned: usize,
    pub worker_exits: Vec<u64>,
    pub aggregate: Option<u64>,
    pub quiescence_recorded: bool,
}

enum SinkInner {
    On { batch_id: u64, ledger: Ledger },
    Off { receipt: Mutex<OffReceipt> },
}

/// The dual-mode observer sink. Cheap to clone (Arc-backed).
#[derive(Clone)]
pub struct ObserverSink {
    inner: std::sync::Arc<SinkInner>,
}

impl ObserverSink {
    pub fn on(batch_id: u64) -> Self {
        ObserverSink {
            inner: std::sync::Arc::new(SinkInner::On {
                batch_id,
                ledger: Ledger::new(),
            }),
        }
    }

    pub fn off(_batch_id: u64) -> Self {
        // OFF receipt is batch-anonymous; batch identity comes from the
        // BatchSpec at adjudication time.
        ObserverSink {
            inner: std::sync::Arc::new(SinkInner::Off {
                receipt: Mutex::new(OffReceipt::default()),
            }),
        }
    }

    /// The full ledger (ON mode) for oracle adjudication.
    pub fn ledger(&self) -> Option<&Ledger> {
        match &*self.inner {
            SinkInner::On { ledger, .. } => Some(ledger),
            SinkInner::Off { .. } => None,
        }
    }

    /// Snapshot of the minimal receipt (OFF mode).
    pub fn off_snapshot(&self) -> Option<OffReceipt> {
        match &*self.inner {
            SinkInner::Off { receipt, .. } => Some(receipt.lock().unwrap().clone()),
            SinkInner::On { .. } => None,
        }
    }

    fn off_with<R>(&self, f: impl FnOnce(&mut OffReceipt, u64) -> R) -> Option<R> {
        match &*self.inner {
            SinkInner::Off { receipt, .. } => {
                let mut r = receipt.lock().unwrap();
                r.ordinal += 1;
                let ord = r.ordinal;
                Some(f(&mut r, ord))
            }
            SinkInner::On { .. } => None,
        }
    }

    fn on_effect(&self, item: Option<u32>, kind: EffectKind, payload: u64, owner: u64) {
        if let SinkInner::On { batch_id, ledger } = &*self.inner {
            ledger.effect(*batch_id, item, kind, payload, owner);
        }
    }

    // ---- call-site API (identical for both modes) ----

    /// Admission linearization record.
    pub fn admit(&self, item: u32, group: u32, owner: u64) {
        self.on_effect(Some(item), EffectKind::ItemAdmitted, group as u64, owner);
        self.off_with(|r, ord| {
            r.items.entry(item).or_insert_with(|| OffItem {
                group,
                admitted_ordinal: ord,
                admitted_ts: Instant::now(),
                start_ts: None,
                wait_done_count: 0,
                terminal: None,
                terminal_ordinal: None,
                terminal_ts: None,
                result_contribution: None,
                owner: 0,
            });
        });
    }

    /// Execution start (worker dequeue). OFF retains the timestamp; ON
    /// has no ledger record kind for it (the SPEC start timestamp is an
    /// OFF-side minimal-instrument field).
    pub fn item_start(&self, item: u32) {
        self.off_with(|r, _| {
            if let Some(it) = r.items.get_mut(&item) {
                it.start_ts = Some(Instant::now());
            }
        });
    }

    pub fn wait_done(&self, item: u32, wait_us: u64, owner: u64) {
        self.on_effect(Some(item), EffectKind::IoWaitDone, wait_us, owner);
        self.off_with(|r, _| {
            if let Some(it) = r.items.get_mut(&item) {
                it.wait_done_count += 1;
            }
        });
    }

    pub fn transform_done(&self, item: u32, meta_fold: u64, owner: u64) {
        // ON records the effect; OFF does not retain transform effects
        // (the terminal receipt carries the pinned contribution).
        self.on_effect(Some(item), EffectKind::TransformDone, meta_fold, owner);
    }

    pub fn result_published(&self, item: u32, contribution: u64, owner: u64) {
        self.on_effect(Some(item), EffectKind::ResultPublished, contribution, owner);
    }

    /// Owner terminal ACK. `contribution` is None for Abandoned.
    pub fn terminal_ack(
        &self,
        item: u32,
        terminal: TerminalKind,
        contribution: Option<u64>,
        owner: u64,
    ) {
        if let SinkInner::On { batch_id, ledger } = &*self.inner {
            ledger.terminal_ack(*batch_id, item, terminal, contribution.unwrap_or(0), owner);
        }
        self.off_with(|r, ord| {
            if let Some(it) = r.items.get_mut(&item) {
                it.terminal = Some(terminal);
                it.terminal_ordinal = Some(ord);
                it.terminal_ts = Some(Instant::now());
                it.result_contribution = contribution;
                it.owner = owner;
            }
        });
    }

    /// Cancel intent recorded. In ON mode this appends the CancelRequested
    /// ledger record; the executor only calls it while holding the state
    /// lock, so every later Abandoned ACK sorts after it (seq order).
    pub fn cancel(&self, group: u32) {
        self.on_effect(None, EffectKind::CancelRequested, group as u64, 0);
        self.off_with(|r, ord| {
            r.cancels.push((group, ord));
        });
    }

    pub fn worker_spawned(&self, worker: u64) {
        self.on_effect(None, EffectKind::ExecutorSpawned, worker, 0);
        self.off_with(|r, _| {
            r.workers_spawned += 1;
        });
    }

    pub fn worker_exit(&self, worker: u64) {
        self.on_effect(None, EffectKind::ExecutorExited, worker, worker);
        self.off_with(|r, _| {
            r.worker_exits.push(worker);
        });
    }

    pub fn aggregate(&self, value: u64) {
        self.on_effect(None, EffectKind::AggregatePublished, value, 0);
        self.off_with(|r, _| {
            r.aggregate = Some(value);
        });
    }

    /// Global quiescence receipt — must be the last record (ON) /
    /// the terminal flag (OFF).
    pub fn quiescence_receipt(&self) {
        self.on_effect(None, EffectKind::QuiescenceReceipt, 0, 0);
        self.off_with(|r, _| {
            r.quiescence_recorded = true;
        });
    }
}
