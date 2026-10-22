//! H1 — `Bounded MPMC Channel + P Long-lived Workers` (Gate II, issue
//! #308; Owner GO `GATE_II_H1_GO`, CHANNEL_LIBRARY = crossbeam-channel
//! 0.5, RESEARCH_ONLY, `Q = B`).
//!
//! Mechanism identity (what makes this a different execution candidate
//! from H0, not a wrapper):
//! - Work dispatch is a REAL bounded MPMC channel (`crossbeam_channel`,
//!   capacity `Q = B`): true multi-producer/multi-consumer queue with the
//!   channel's own lock-free-ish park/unpark (`Parker`-based) wakeup —
//!   no shared `VecDeque`, no `work_cv`, no H0 state. Each of the P
//!   long-lived workers holds its own `Receiver` clone; every message is
//!   delivered to exactly one worker.
//! - Normal exit is the disconnect funnel: `finish_admission` drops the
//!   LAST actual sender; workers drain the queued items and then observe
//!   `Disconnected`, record `ExecutorExited` and return.
//! - Abort is a control plane, not a queue operation: an authoritative
//!   `shutdown_requested` flag under the session state lock plus one
//!   wake signal per worker on a dedicated bounded control channel. A
//!   plain `Select{work, ctl}` does NOT give abort priority by itself
//!   (crossbeam selects randomly among ready operations), so the
//!   post-recv state step RE-CHECKS the flag before any execution: a
//!   worker that pulls a work item after the abort linearization leaves
//!   the obligation outstanding and exits — work can never starve abort,
//!   and exit is bounded (at most one recv per worker after the flag).
//! - Admission follows the frozen ADMISSION_RECIPE (SPEC §4.2): the B
//!   permit wait on a condvar, then ONE cancel-order critical section
//!   that registers the obligation, enqueues with a non-blocking
//!   `try_send`, and appends `ItemAdmitted`. The permit is virtual at
//!   the cut check (never consumed on a cut, N1(b)); the B permit is
//!   released only after the authoritative terminal ACK.
//!
//! The three budgets are three separate accountings (SPEC §4.1; Owner GO
//! §1.B — "容量等于 B 不代表两种状态可用一个计数器替代"):
//! ```text
//! B = admitted_not_terminal   (queue resident + running + completed-not-ACK)
//! Q = channel capacity = B    (topology; the (Q+1)-th concurrent message
//!                              is structurally impossible)
//! P = workers simultaneously executing item work (<= P structurally)
//! ```
//! Q=B proof obligation (`queue_resident <= admitted_not_terminal <= B
//! = Q`): every send happens under the state lock after registering the
//! obligation (`admitted_not_terminal += 1` first, send second), and the
//! resident witness counter decrements at the worker's post-recv state
//! step — strictly before that item can produce any effect or ACK. So at
//! every lock step every counted resident item is admitted-not-terminal:
//! `CHANNEL_QUEUE_RESIDENT <= ADMITTED_NOT_TERMINAL <= B = Q`. A
//! `try_send` that returns `Full`/`Disconnected` is therefore an
//! invariant break or an injected fault: the admission ROLLS BACK its
//! not-yet-published registration (no ledger record exists yet) and the
//！ session fails loudly (defect latch, abort-join, no receipt). It is
//! never retried after a sleep, never waited on while holding the lock,
//! never dropped silently, never widened.
//!
//! The seven close/shutdown events are provably distinct (Gate II §10):
//! ```text
//! ProducerDone        producer_done flag under the state lock
//! LastSenderDropped   finish_admission takes work_tx (Option) and drops it
//! ChannelDisconnected recv observes Err(Disconnected) — AFTER the drain
//! AllItemsSettled     pending empty (every admitted item ACKed)
//! AllWorkersExited    each worker recorded ExecutorExited pre-return
//! AllWorkersJoined    join_all returned P handles
//! QuiescenceReceipt   last ledger record, after the join and aggregate
//! ```
//! `Queue Empty` is not Settlement (settlement is pending-based, never
//! queue-based); `Disconnected` is not Quiescence; `Join` is not
//! aggregation.
//!
//! Sender-clone ownership enumeration (Gate II §10 — no hidden clone may
//! keep the receiver from ever seeing disconnect):
//! 1. `work_tx: Option<Sender<u32>>` — the ONLY work sender, held by the
//!    executor, moved out and dropped by `finish_admission`. Never
//!    cloned (the test-only leak control is the documented exception
//!    that proves the hazard and the abort recovery).
//! 2. `ctl_tx: Sender<()>` — abort wake signals, held by the executor;
//!    capacity P, one try_send per worker on abort, never cloned.
//! 3. Workers hold `Receiver<u32>` / `Receiver<()>` clones only —
//!    receiver clones never affect disconnect semantics.
//!
//! Owner death keeps the frozen PD semantics (SPEC §5.4): worker panics
//! are contained (`catch_unwind`), the defect is latched, the session
//! fails at settlement, `abort` joins everything, and NO terminal,
//! aggregate or receipt is ever forged for the dead owner's items.
//!
//! Business execution is the SHARED mechanism-neutral core
//! ([`crate::session_core::execute_item`], A_NARROW): same wait, same
//! CPU blocks, same content/metadata digests, same contribution, same
//! fault lane as H0. `ref_result` is never called on the execution path
//! (SPEC §2.5(b)).
//!
//! `cfg(test)`-only facilities (compiled out of every non-test build):
//! fence gates (shared machinery, H1-prefixed gate names) for the
//! deterministic interleaving controls, the owner-panic injection
//! delegate, the injected `try_send` failure point (the only way the
//! structurally-unreachable Full arm can be exercised without violating
//! the frozen budgets), and the test-only sender-leak hook.

// Fence macros must precede their first use. In non-test builds every
// call site compiles to nothing.
#[cfg(not(test))]
macro_rules! h1_fence_park {
    ($name:literal, $guard:expr, $lock:expr) => {{
        let _ = &$guard;
        let _ = &$lock;
    }};
}
#[cfg(test)]
macro_rules! h1_fence_park {
    ($name:literal, $guard:expr, $lock:expr) => {
        $guard = $crate::session_core::fences::park($name, $guard, $lock);
    };
}

use std::collections::HashSet;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use crossbeam_channel::{select, Receiver, Sender};

use crate::ledger::TerminalKind;
use crate::observer::ObserverSink;
use crate::session_core::execute_item;
use crate::session_core::{SessionError, SessionStats};
use crate::workload::BatchSpec;

/// Session configuration: P execution parallelism, B admitted-not-terminal
/// budget (= the bounded channel capacity Q), and the optional frozen
/// cancel milestone (k-th admission, group).
#[derive(Clone, Copy, Debug)]
pub struct H1Config {
    pub p: usize,
    pub b: usize,
    /// `(k, group)`: request_cancel(group) fires after the k-th admission.
    pub cancel_milestone: Option<(u32, u32)>,
}

/// H1 lifecycle statistics: the shared mechanism-independent face plus
/// the channel-specific resource accounting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct H1SessionStats {
    /// admitted / B high water / active execution high water / spawn / join.
    pub stats: SessionStats,
    /// CHANNEL_QUEUE_RESIDENT high water — the per-lock-step witness
    /// counter (sends minus recv-observed steps, both under the state
    /// lock). Every counted item is admitted-not-terminal, so
    /// `q_high_water <= b_high_water <= B = Q` holds by construction.
    pub q_high_water: usize,
    /// Resident witness at session end: 0 after a clean drain; > 0 is
    /// honest obligation residue after an abort (messages whose items
    /// were admitted but never received — never silently discarded).
    pub channel_resident_final: usize,
}

struct H1State {
    /// The B accounting set: admitted and not yet terminal-ACKed
    /// (queue resident + running + completed-not-ACK by construction).
    admitted_not_terminal: usize,
    /// Obligation bookkeeping: admitted item ids awaiting their terminal.
    pending: HashSet<u32>,
    cancelled_groups: HashSet<u32>,
    producer_done: bool,
    shutdown_requested: bool,
    /// Workers currently executing item work (<= P by construction).
    active_execution: usize,
    active_high_water: usize,
    b_high_water: usize,
    /// CHANNEL_QUEUE_RESIDENT witness (see [`H1SessionStats::q_high_water`]).
    channel_resident: usize,
    q_high_water: usize,
    admitted_total: usize,
    results: Vec<(u32, u64)>,
    panic_defect: Option<SessionError>,
}

struct H1Shared {
    state: Mutex<H1State>,
    /// The single producer waits here for B permit availability.
    b_cv: Condvar,
    /// Settlement waiters (and the session runner awaiting a defect).
    settle_cv: Condvar,
}

/// The H1 executor. `start` spawns the worker pool and binds the channel;
/// the session owner (control thread) then drives admission, settlement
/// and quiescence.
///
/// Dropping without `abort()` / `await_quiescence()` leaks the parked
/// worker pool — every caller must take the session to a terminal action.
pub struct H1Executor {
    cfg: H1Config,
    spec: BatchSpec,
    sink: ObserverSink,
    shared: Arc<H1Shared>,
    /// The ONLY work sender (see the module-level ownership enumeration).
    work_tx: Option<Sender<u32>>,
    /// Abort wake channel (bounded P; signals, not work).
    ctl_tx: Sender<()>,
    handles: Vec<JoinHandle<()>>,
    /// Test-only witness clone of the control receiver (compiled out of
    /// every non-test build); lets tests observe abort wake signals.
    #[cfg(test)]
    ctl_rx_witness: Receiver<()>,
}

impl H1Executor {
    /// Bind the bounded channel (capacity Q = B) and spawn P long-lived
    /// workers (each recording ExecutorSpawned at the true spawn
    /// boundary). Admission has NOT begun; the session owner drives it
    /// with [`Self::admit_all`].
    pub fn start(cfg: H1Config, spec: BatchSpec, sink: ObserverSink) -> Self {
        assert!(cfg.p >= 1, "P must be >= 1");
        assert!(cfg.b >= 1, "B must be >= 1");
        // Q = B (Owner GO §1.B): the channel capacity IS the cell's B.
        // Q and B remain two separately-accounted faces (the channel's
        // structural capacity vs the admitted-not-terminal budget), never
        // one counter.
        let (work_tx, work_rx) = crossbeam_channel::bounded::<u32>(cfg.b);
        let (ctl_tx, ctl_rx) = crossbeam_channel::bounded::<()>(cfg.p);
        let shared = Arc::new(H1Shared {
            state: Mutex::new(H1State {
                admitted_not_terminal: 0,
                pending: HashSet::new(),
                cancelled_groups: HashSet::new(),
                producer_done: false,
                shutdown_requested: false,
                active_execution: 0,
                active_high_water: 0,
                b_high_water: 0,
                channel_resident: 0,
                q_high_water: 0,
                admitted_total: 0,
                results: Vec::new(),
                panic_defect: None,
            }),
            b_cv: Condvar::new(),
            settle_cv: Condvar::new(),
        });
        let mut handles = Vec::with_capacity(cfg.p);
        for w in 1..=cfg.p as u64 {
            sink.worker_spawned(w);
            let shared = Arc::clone(&shared);
            let sink = sink.clone();
            let spec = spec.clone();
            let work_rx = work_rx.clone();
            let ctl_rx = ctl_rx.clone();
            handles.push(
                std::thread::Builder::new()
                    .name(format!("h1-worker-{w}"))
                    .spawn(move || worker_main(w, spec, sink, shared, work_rx, ctl_rx))
                    .expect("spawn h1 worker"),
            );
        }
        H1Executor {
            cfg,
            spec,
            sink,
            shared,
            work_tx: Some(work_tx),
            ctl_tx,
            handles,
            #[cfg(test)]
            ctl_rx_witness: ctl_rx.clone(),
        }
    }

    /// Cloneable handle for the `request_cancel` role (SPEC §5.5 action 1).
    pub fn cancel_handle(&self) -> CancelHandle {
        CancelHandle {
            shared: Arc::clone(&self.shared),
            sink: self.sink.clone(),
        }
    }

    /// Cloneable handle for the `await_settlement` role (SPEC §5.5 action
    /// 2) — callable from a thread that owns no `&mut` of the executor.
    pub fn settlement_watch(&self) -> SettlementWatch {
        SettlementWatch {
            shared: Arc::clone(&self.shared),
        }
    }

    /// Producer role (session owner thread): admit every offered item
    /// under the frozen ADMISSION_RECIPE, firing the workload-owned
    /// cancel milestone after the k-th admission (SPEC §5.6). Single
    /// producer by construction. Does NOT close the offer stream — call
    /// [`Self::finish_admission`] afterwards (SPEC §5.5's explicit
    /// precondition of await_settlement).
    pub fn admit_all(&mut self) {
        let mut admitted: u32 = 0;
        for item in &self.spec.items {
            let id = item.item_id;
            let group = item.cancellation_group;
            // (A) acquire the B permit: block until admitted-not-terminal
            // < B. The count is decremented only after the terminal ACK,
            // so it covers queue resident + running + completed-not-ACK.
            let mut st = self.shared.state.lock().unwrap();
            while st.admitted_not_terminal >= self.cfg.b {
                // A latched defect (or an abort) has already failed the
                // session: stop admitting instead of blocking on permits
                // that the dead owner will never release —
                // await_settlement reports the defect and abort() joins
                // everything (loud, never wedged).
                if st.panic_defect.is_some() || st.shutdown_requested {
                    return;
                }
                st = self.shared.b_cv.wait(st).unwrap();
            }
            // (C) cancel-order critical section: the linearization point.
            // The B permit is virtual here (the count is not incremented
            // on a cut), so a cut releases it by not consuming it —
            // N1(b) holds structurally and no permit can leak.
            h1_fence_park!("h1_admit_pre_cut", st, &self.shared.state);
            if st.cancelled_groups.contains(&group) {
                // Cut: the item stays Offered with no terminal obligation.
                drop(st);
                continue;
            }
            // Register the obligation BEFORE the mechanism send (rollback-
            // safe order): if the send fails, the not-yet-published
            // registration is undone and NO ledger record, NO obligation
            // and NO permit can leak. The Q=B permit argument makes Full
            // structurally unreachable in a healthy session (see module
            // docs), so this arm only fires on a real invariant break or
            // an injected fault — and must fail closed.
            st.pending.insert(id);
            st.admitted_not_terminal += 1;
            st.admitted_total += 1;
            st.b_high_water = st.b_high_water.max(st.admitted_not_terminal);
            let tx = self
                .work_tx
                .as_ref()
                .expect("the producer sender is held until finish_admission");
            // The cfg(test) injection REPLACES THE SEND (decided before any
            // real send), so a "failed" admission never leaves a phantom
            // message in the channel — the rollback drill stays exact: no
            // message, no obligation, no ledger record, no permit.
            #[cfg(test)]
            let injected_full = test_send_full_item() == id;
            #[cfg(not(test))]
            let injected_full = false;
            let send_result = if injected_full {
                Err(crossbeam_channel::TrySendError::Full(id))
            } else {
                tx.try_send(id)
            };
            if let Err(e) = send_result {
                st.pending.remove(&id);
                st.admitted_not_terminal -= 1;
                st.admitted_total -= 1;
                if st.panic_defect.is_none() {
                    st.panic_defect = Some(SessionError {
                        worker: 0,
                        item: id,
                        message: format!(
                            "H1 channel invariant violation at admission of item {id}: {e:?}"
                        ),
                    });
                }
                drop(st);
                self.shared.settle_cv.notify_all();
                self.shared.b_cv.notify_all();
                return;
            }
            st.channel_resident += 1;
            st.q_high_water = st.q_high_water.max(st.channel_resident);
            // Linearization: the ItemAdmitted ledger record, INSIDE the
            // cancel-order critical section. It is appended while holding
            // the state lock, so any later post-recv state step (which
            // needs the same lock) observes the obligation and the
            // admission before the item's first possible effect.
            self.sink.admit(id, group, 0);
            drop(st);
            // The channel wakes a parked worker directly (the MPMC
            // parker); no manual notify exists or is needed on the
            // dispatch path.
            // (E) workload-owned cancel milestone: admission-count based,
            // executor-speed independent.
            admitted += 1;
            if let Some((k, cancel_group)) = self.cfg.cancel_milestone {
                if admitted == k {
                    self.cancel_handle().request_cancel(cancel_group);
                }
            }
        }
    }

    /// Close the offer stream (the fourth, explicit action of SPEC §5.5):
    /// flip `producer_done` under the lock (notifying settlement waiters —
    /// the lost-wakeup discipline), then drop the LAST actual work
    /// sender. Workers drain the queued items and observe `Disconnected`
    /// only after the drain — `Disconnected` ≠ empty, and neither is
    /// settlement.
    pub fn finish_admission(&mut self) {
        let mut st = self.shared.state.lock().unwrap();
        h1_fence_park!("h1_producer_pre_done", st, &self.shared.state);
        st.producer_done = true;
        let tx = self.work_tx.take();
        drop(st);
        drop(tx);
        self.shared.settle_cv.notify_all();
    }

    /// await_settlement (SPEC §5.5 action 2): returns when every admitted
    /// item has exactly one terminal ACK — or immediately with the defect
    /// if an owner died. Promises nothing about executor shutdown.
    pub fn await_settlement(&self) -> Result<(), SessionError> {
        self.settlement_watch().wait()
    }

    /// await_quiescence (SPEC §5.5 action 3): join every worker (all
    /// exits are recorded by the workers themselves strictly before their
    /// thread returns, and the join happens-before the receipt), publish
    /// the fan-in aggregate and the quiescence receipt as the LAST
    /// record. A caller that skipped settlement — or a worker that died
    /// outside `catch_unwind` — is a loud `Err`, never a forged receipt.
    pub fn await_quiescence(&mut self) -> Result<H1SessionStats, SessionError> {
        // Precondition guard (mirrors H0's Owner-review P2 guard): a
        // worker can only exit via the disconnect funnel
        // (`finish_admission`'s last-sender drop, post-drain) or an
        // abort. With the offer stream still open, workers are parked in
        // the channel select forever, so `join` has no termination
        // guarantee — reject BEFORE joining instead of hanging. The
        // parked pool is recovered with [`Self::abort`].
        if !self.shared.state.lock().unwrap().producer_done {
            return Err(SessionError {
                worker: 0,
                item: u32::MAX,
                message: "await_quiescence before the offer stream is closed \
                          (finish_admission): workers can never disconnect — \
                          recover an un-settled executor with abort()"
                    .into(),
            });
        }
        let stats = self.join_workers()?;
        let mut sorted = {
            let st = self.shared.state.lock().unwrap();
            if !st.pending.is_empty() {
                return Err(SessionError {
                    worker: 0,
                    item: u32::MAX,
                    message: "quiescence requires settlement (pending obligations remain)".into(),
                });
            }
            if let Some(defect) = st.panic_defect.clone() {
                return Err(defect);
            }
            st.results.clone()
        };
        sorted.sort_by_key(|(id, _)| *id);
        let agg = crate::workload::aggregate(&sorted);
        self.sink.aggregate(agg);
        self.sink.quiescence_receipt();
        Ok(stats)
    }

    /// Owner-death abort: request worker shutdown, join everything, and
    /// return the stats WITHOUT publishing any aggregate or receipt.
    /// The obligations left behind by the dead owner are reported, never
    /// silently discharged by the harness: unreceived queue messages via
    /// [`H1SessionStats::channel_resident_final`], and received-but-not-
    /// executed items via the ledger/pending ACK gap (no stats field —
    /// both faces are honest residue, neither is forged). Abort also
    /// invalidates the settlement face: an `await_settlement` on an
    /// aborted executor with an open stream may block forever — the
    /// owner's result is abort's return value, exactly as in H0.
    ///
    /// Linearization: the flag flip under the state lock. Workers observe
    /// it at (a) their top-of-loop check, (b) the post-recv state step —
    /// even a worker that pulled a work item in a race must re-check the
    /// flag before executing, so sustained WORK_READY cannot starve abort.
    /// Every parked worker is woken by one control signal per worker;
    /// settlement watchers (legally concurrent via the cloneable watch)
    /// are notified, and the permit-waiter wakeup covers a caller that
    /// synchronizes admission behind its own lock. `abort` itself needs
    /// `&mut self` like every session action: admission and abort are
    /// sequenced by the session owner, never run concurrently through
    /// this API.
    pub fn abort(&mut self) -> H1SessionStats {
        {
            let mut st = self.shared.state.lock().unwrap();
            st.shutdown_requested = true;
        }
        self.shared.b_cv.notify_all();
        self.shared.settle_cv.notify_all();
        // One wake signal per worker (bounded(P) control channel;
        // try_send never blocks, and Full is impossible: at most P
        // unconsumed signals can exist because only abort sends). A
        // failed wake can only mean P signals are already queued —
        // enough wakeups for every parked worker.
        for _ in 0..self.cfg.p {
            let _ = self.ctl_tx.try_send(());
        }
        self.join_all()
    }

    fn join_workers(&mut self) -> Result<H1SessionStats, SessionError> {
        let stats = self.join_all();
        if stats.stats.workers_joined != self.cfg.p {
            // A worker died outside `catch_unwind` (or was never joined):
            // loud failure, never a receipt over an unjoined executor.
            return Err(SessionError {
                worker: 0,
                item: u32::MAX,
                message: format!(
                    "only {}/{} workers joined cleanly",
                    stats.stats.workers_joined, self.cfg.p
                ),
            });
        }
        Ok(stats)
    }

    /// Join every handle and report the stats whatever happened — abort's
    /// best-effort face (the failure is the caller's to publish).
    fn join_all(&mut self) -> H1SessionStats {
        let mut joined = 0usize;
        for h in self.handles.drain(..) {
            if h.join().is_ok() {
                joined += 1;
            }
        }
        let st = self.shared.state.lock().unwrap();
        H1SessionStats {
            stats: SessionStats {
                admitted_count: st.admitted_total,
                b_high_water: st.b_high_water,
                active_execution_high_water: st.active_high_water,
                workers_spawned: self.cfg.p,
                workers_joined: joined,
            },
            q_high_water: st.q_high_water,
            channel_resident_final: st.channel_resident,
        }
    }
}

/// The `request_cancel` role bound to one session (SPEC §5.5 action 1):
/// records the intent and cuts admission for the group. Promises nothing
/// about in-flight items — they complete or are abandoned by their real
/// owner (SPEC §5.3, observed at the worker's post-recv state step). The
/// cancel record is appended while holding the cancel-order lock, so any
/// later Abandoned ACK for the group sorts after it in ledger order.
#[derive(Clone)]
pub struct CancelHandle {
    shared: Arc<H1Shared>,
    sink: ObserverSink,
}

impl CancelHandle {
    pub fn request_cancel(&self, group: u32) {
        let mut st = self.shared.state.lock().unwrap();
        if st.cancelled_groups.insert(group) {
            self.sink.cancel(group);
        }
        drop(st);
        // No channel/condvar wakeup is needed: cancel creates neither
        // work nor permits. In-flight items are resolved by their real
        // owners at their next state step; the producer re-checks the cut
        // at every admission.
    }
}

/// The `await_settlement` role bound to one session (SPEC §5.5 action 2).
#[derive(Clone)]
pub struct SettlementWatch {
    shared: Arc<H1Shared>,
}

impl SettlementWatch {
    pub fn wait(&self) -> Result<(), SessionError> {
        let mut st = self.shared.state.lock().unwrap();
        loop {
            if st.producer_done && (st.pending.is_empty() || st.panic_defect.is_some()) {
                break;
            }
            h1_fence_park!("h1_settle_pre_wait", st, &self.shared.state);
            // Post-fence predicate re-check (lost-wakeup defense, same
            // discipline as the H0 watch): the fence may have released
            // the lock, so a settle_cv notify may have landed while
            // parked.
            if st.producer_done && (st.pending.is_empty() || st.panic_defect.is_some()) {
                break;
            }
            st = self.shared.settle_cv.wait(st).unwrap();
        }
        match st.panic_defect.clone() {
            Some(defect) => Err(defect),
            None => Ok(()),
        }
    }
}

/// Convenience session runner: admission → offer close → settlement →
/// quiescence; on owner death, abort (join, no receipt) and report the
/// defect. Used by the harness and the 92-cell matrix.
pub fn run_session_h1(
    cfg: H1Config,
    spec: BatchSpec,
    sink: &ObserverSink,
) -> Result<H1SessionStats, SessionError> {
    let mut ex = H1Executor::start(cfg, spec, sink.clone());
    ex.admit_all();
    ex.finish_admission();
    if let Err(defect) = ex.await_settlement() {
        let _ = ex.abort();
        return Err(defect);
    }
    ex.await_quiescence()
}

/// One worker's dispatch loop — the H1 mechanism proper. Business work
/// goes through the SHARED mechanism-neutral core
/// ([`crate::session_core::execute_item`]); everything around it is H1's
/// own channel dispatch, state steps and exit funnel.
fn worker_main(
    worker: u64,
    spec: BatchSpec,
    sink: ObserverSink,
    shared: Arc<H1Shared>,
    work_rx: Receiver<u32>,
    ctl_rx: Receiver<()>,
) {
    loop {
        // (1) Authoritative shutdown check before waiting on anything.
        {
            // The `mut` is consumed only by the cfg(test) fence
            // reassignment; non-test builds never reassign.
            #[cfg_attr(not(test), allow(unused_mut))]
            let mut st = shared.state.lock().unwrap();
            h1_fence_park!("h1_worker_pre_select", st, &shared.state);
            // Post-fence re-check: the fence may have released the lock,
            // so an abort may have landed while parked.
            if st.shutdown_requested {
                drop(st);
                sink.worker_exit(worker);
                return;
            }
        }
        // (2) Wait for work or an abort wake. crossbeam selects randomly
        // among READY operations — this is exactly why the abort flag is
        // authoritative and re-checked in [`handle_item`]: a worker that
        // consumes one more work item after the abort linearization does
        // NOT execute it, so sustained WORK_READY cannot starve abort.
        let received = select! {
            recv(work_rx) -> msg => msg,
            recv(ctl_rx) -> _ => continue,
        };
        let Ok(item) = received else {
            // Disconnected = the last actual sender was dropped (only
            // `finish_admission` does that) AND the queue drained. The
            // single legal normal-exit funnel: record ExecutorExited at
            // the true lifecycle boundary (this thread returns right
            // after). A disconnect with the offer stream still open is
            // an invariant break — latch it loudly, never exit silently.
            {
                let mut st = shared.state.lock().unwrap();
                if !st.producer_done && st.panic_defect.is_none() {
                    st.panic_defect = Some(SessionError {
                        worker,
                        item: u32::MAX,
                        message: "work channel disconnected while the offer stream was open".into(),
                    });
                }
            }
            shared.settle_cv.notify_all();
            shared.b_cv.notify_all();
            sink.worker_exit(worker);
            return;
        };
        handle_item(worker, &spec, &sink, &shared, item);
    }
}

/// The post-recv obligation step + execution + terminal. The state step
/// here is the linearization witness for every ordering claim:
/// - the obligation was registered (admission) BEFORE the send, under
///   the same lock this step acquires, so `pending` provably names the
///   item before any effect of that item;
/// - the channel-resident witness is decremented here — strictly before
///   the item can produce any effect;
/// - the abort flag and the cancellation group are observed HERE, so the
///   dequeue-observation abandon and the abort residue rule are both
///   owner-authoritative.
fn handle_item(
    worker: u64,
    spec: &BatchSpec,
    sink: &ObserverSink,
    shared: &Arc<H1Shared>,
    item: u32,
) {
    // Pure lookup, no protocol state: hoisted out of the lock section so
    // a structurally-impossible index panic could never fire under the
    // lock (review A F5).
    let item_group = spec.items[item as usize].cancellation_group;
    let mut st = shared.state.lock().unwrap();
    st.channel_resident -= 1;
    h1_fence_park!("h1_worker_post_recv", st, &shared.state);
    // Post-fence re-check (the fence may have released the lock): the
    // authoritative state may have moved — abort, cancel, defect.
    if st.shutdown_requested {
        // Abort after recv, before execution: the obligation stays
        // outstanding (admitted-not-terminal; the residue is visible in
        // `pending` and in the ledger's ACK gap — never executed, never
        // forged). Stats carry the unreceived-queue face
        // (`channel_resident_final`).
        drop(st);
        sink.worker_exit(worker);
        return;
    }
    let cancelled = st.cancelled_groups.contains(&item_group);
    st.active_execution += 1;
    st.active_high_water = st.active_high_water.max(st.active_execution);
    drop(st);

    sink.item_start(item);
    if cancelled {
        // The owner observed the cancel while actually holding the item
        // (post-recv observation): legal Abandon (SPEC §5.1).
        finish_terminal(sink, shared, item, TerminalKind::Abandoned, worker, None);
    } else {
        // PD premise: panic-contained so the owner survives; the frozen
        // terminal vocabulary has no panic-terminal, so the
        // undischarged obligation fails the session (never forged).
        let outcome =
            panic::catch_unwind(AssertUnwindSafe(|| execute_item(worker, spec, sink, item)));
        match outcome {
            Ok((term, contribution)) => {
                finish_terminal(sink, shared, item, term, worker, Some(contribution));
            }
            Err(payload) => {
                let message = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "opaque panic payload".to_string());
                let mut st = shared.state.lock().unwrap();
                if st.panic_defect.is_none() {
                    st.panic_defect = Some(SessionError {
                        worker,
                        item,
                        message,
                    });
                }
                drop(st);
                shared.settle_cv.notify_all();
                shared.b_cv.notify_all();
            }
        }
    }
    let mut st = shared.state.lock().unwrap();
    st.active_execution -= 1;
    drop(st);
    shared.b_cv.notify_all();
    shared.settle_cv.notify_all();
}

/// Terminal ACK by the real owner (the completing/abandoning worker),
/// then B-permit release + obligation discharge + fan-in sink update,
/// then wake the backpressure and settlement waiters.
fn finish_terminal(
    sink: &ObserverSink,
    shared: &H1Shared,
    item_id: u32,
    term: TerminalKind,
    owner: u64,
    contribution: Option<u64>,
) {
    sink.terminal_ack(item_id, term, contribution, owner);
    let mut st = shared.state.lock().unwrap();
    if st.pending.remove(&item_id) {
        st.admitted_not_terminal = st.admitted_not_terminal.saturating_sub(1);
    }
    if let Some(c) = contribution {
        st.results.push((item_id, c));
    }
    drop(st);
    shared.b_cv.notify_all();
    shared.settle_cv.notify_all();
}

// ---- cfg(test)-only facilities ----

#[cfg(test)]
static TEST_SEND_FULL_ITEM: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(u32::MAX);

#[cfg(test)]
fn test_send_full_item() -> u32 {
    TEST_SEND_FULL_ITEM.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
impl H1Executor {
    /// Test-only owner-panic injection (shared facility, H1 delegate).
    pub(crate) fn inject_owner_panic_at(item: u32) {
        crate::session_core::set_test_panic_item(item);
    }

    /// Serializes session tests (process-global fence gates and panic
    /// injection; same facility H0 uses).
    pub(crate) fn session_test_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::session_core::session_test_lock()
    }

    /// Test-only injected `try_send` failure for the fail-closed control:
    /// the ONLY way the structurally-unreachable Full arm can run without
    /// violating the frozen budgets (Gate II §15).
    pub(crate) fn inject_send_full_at(item: u32) -> InjectSendFullGuard {
        TEST_SEND_FULL_ITEM.store(item, std::sync::atomic::Ordering::Relaxed);
        InjectSendFullGuard
    }

    /// Test-only sender-leak hook (Gate II §15 class 4): clones the work
    /// sender so the test can hold a second live sender after
    /// `finish_admission` dropped the real one. Production code NEVER
    /// clones this sender — this hook exists to prove the hazard is real
    /// and that abort recovers it in bounded time.
    pub(crate) fn leak_work_sender_clone(&self) -> Sender<u32> {
        self.work_tx.as_ref().expect("sender still held").clone()
    }

    /// Test-only control-plane witness: a `Receiver` clone for the ctl
    /// channel, so a test-side helper can deterministically observe that
    /// abort's wake signals were sent (and only then open a fence), with
    /// no sleeps and no access to executor internals.
    pub(crate) fn leak_ctl_receiver_clone(&self) -> Receiver<()> {
        self.ctl_rx_witness.clone()
    }
}

#[cfg(test)]
pub(crate) struct InjectSendFullGuard;

#[cfg(test)]
impl Drop for InjectSendFullGuard {
    fn drop(&mut self) {
        TEST_SEND_FULL_ITEM.store(u32::MAX, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    //! H1 lifecycle obligations and executable negative controls. The
    //! fence-based controls serialize on the session test lock (fence
    //! gates are process-global, shared with H0's controls under one
    //! mutex); every control runs under a watchdog whose ONLY job is
    //! timeout protection — a hang is a failure, never evidence.

    use super::*;
    use crate::h0::tests::{with_watchdog, SEED};
    use crate::ledger::EffectKind;
    use crate::oracle;
    use crate::scenario::Scenario;
    use crate::session_core::fences;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn h1_cfg(p: usize, b: usize, cancel: Option<(u32, u32)>) -> H1Config {
        H1Config {
            p,
            b,
            cancel_milestone: cancel,
        }
    }

    fn expect_receipt_absent(sink: &ObserverSink) {
        assert!(
            !sink
                .ledger()
                .unwrap()
                .snapshot()
                .iter()
                .any(|r| r.kind == EffectKind::QuiescenceReceipt),
            "no receipt may exist on this failed session"
        );
    }

    // ---- lifecycle GREEN arms ----

    #[test]
    fn h1_on_mode_no_cancel_full_oracle_green() {
        let _g = H1Executor::session_test_lock();
        let spec = BatchSpec::probe_batch();
        let sink = ObserverSink::on(spec.batch_id);
        let stats = run_session_h1(h1_cfg(4, 8, None), spec.clone(), &sink).expect("session");
        assert_eq!(stats.stats.admitted_count, 48);
        assert!(
            stats.stats.b_high_water <= 8,
            "B bound violated: {}",
            stats.stats.b_high_water
        );
        // Q=B: the queue-resident witness never exceeds the capacity,
        // which is the same cell parameter as B.
        assert!(
            stats.q_high_water <= 8,
            "Q bound violated: {}",
            stats.q_high_water
        );
        assert!(stats.stats.active_execution_high_water <= 4);
        assert_eq!(stats.stats.workers_spawned, 4);
        assert_eq!(stats.stats.workers_joined, 4);
        assert_eq!(stats.channel_resident_final, 0, "clean drain");
        let verdict = oracle::adjudicate(&spec, sink.ledger().unwrap());
        assert!(verdict.green(), "oracle RED: {:?}", verdict.failed());
    }

    #[test]
    fn h1_on_mode_cancel_milestone_full_oracle_green() {
        let _g = H1Executor::session_test_lock();
        // cancel after the 24th admission (group 7), the probe milestone.
        let spec = BatchSpec::probe_batch();
        let sink = ObserverSink::on(spec.batch_id);
        let stats =
            run_session_h1(h1_cfg(4, 8, Some((24, 7))), spec.clone(), &sink).expect("session");
        // Admission order is linear: items 0..=23 all admitted; after the
        // milestone only group-1 items (id % 3 != 0) may be admitted.
        let expected_admitted = 24 + (24..48u32).filter(|id| id % 3 != 0).count();
        assert_eq!(stats.stats.admitted_count, expected_admitted);
        assert!(stats.stats.b_high_water <= 8);
        assert!(stats.q_high_water <= 8);
        assert!(stats.stats.active_execution_high_water <= 4);
        let verdict = oracle::adjudicate(&spec, sink.ledger().unwrap());
        assert!(verdict.green(), "oracle RED: {:?}", verdict.failed());
    }

    #[test]
    fn h1_off_mode_receipt_structure() {
        let _g = H1Executor::session_test_lock();
        let spec = BatchSpec::probe_batch();
        let sink = ObserverSink::off(spec.batch_id);
        let stats = run_session_h1(h1_cfg(4, 8, None), spec.clone(), &sink).expect("session");
        assert_eq!(stats.stats.workers_joined, 4);
        let receipt = sink.off_snapshot().unwrap();
        assert_eq!(receipt.items.len(), 48);
        assert!(receipt.quiescence_recorded);
        assert_eq!(receipt.worker_exits.len(), 4);
        assert!(receipt.cancels.is_empty());
        for (id, it) in &receipt.items {
            assert!(
                it.terminal.is_some(),
                "item {id} without terminal in OFF receipt"
            );
            assert!(it.terminal_ordinal.unwrap() > it.admitted_ordinal);
            assert!(it.admitted_ts <= it.terminal_ts.unwrap());
            let expect_wait = spec.items[*id as usize].wait_us;
            assert_eq!(it.wait_done_count, u32::from(expect_wait > 0), "item {id}");
        }
        assert_eq!(receipt.aggregate, Some(0x980a_9dc0_d8d8_cee6));
    }

    #[test]
    fn s5_scenario_cell_cancel_oracle_green() {
        let _g = H1Executor::session_test_lock();
        // Real frozen cell: S5, P=4, B=8 (2P), cancel after the 164th
        // admission, group 7. Non-timed correctness run.
        let cell = Scenario::S5;
        let spec = cell.batch_spec(1, SEED);
        let sink = ObserverSink::on(spec.batch_id);
        let stats = run_session_h1(h1_cfg(4, 8, cell.cancel_milestone()), spec.clone(), &sink)
            .expect("session");
        assert!(stats.stats.b_high_water <= 8);
        assert!(stats.stats.active_execution_high_water <= 4);
        assert!(
            stats.stats.admitted_count < spec.items.len(),
            "the cut must bite"
        );
        let verdict = oracle::adjudicate(&spec, sink.ledger().unwrap());
        assert!(verdict.green(), "oracle RED: {:?}", verdict.failed());
    }

    // ---- B/P/Q budget controls ----

    #[test]
    fn full_b_producer_blocks_and_recovers() {
        let _g = H1Executor::session_test_lock();
        // B=2, Q=2, P=2, 48 items: the producer MUST hit the B wall and
        // only progress after terminal ACKs release permits. b_high_water
        // == B exactly, never above; active execution never above P; the
        // queue-resident witness never above Q = B.
        let spec = BatchSpec::probe_batch();
        let sink = ObserverSink::on(spec.batch_id);
        let stats = run_session_h1(h1_cfg(2, 2, None), spec.clone(), &sink).expect("session");
        assert_eq!(stats.stats.b_high_water, 2);
        assert!(stats.q_high_water <= 2);
        assert!(stats.stats.active_execution_high_water <= 2);
        let verdict = oracle::adjudicate(&spec, sink.ledger().unwrap());
        assert!(verdict.green(), "oracle RED: {:?}", verdict.failed());
    }

    #[test]
    fn q_equals_b_is_a_real_queue_not_a_counter_alias() {
        let _g = H1Executor::session_test_lock();
        // P=1, B=Q=8: the single worker stalls on item 0's 2ms wait while
        // the producer fills the channel toward capacity — the queue
        // genuinely buffers more than P items, and its separate
        // accounting (q_high_water / channel_resident) is distinct from
        // the B budget while still bounded by it.
        let spec = BatchSpec::probe_batch();
        let sink = ObserverSink::on(spec.batch_id);
        let stats = run_session_h1(h1_cfg(1, 8, None), spec.clone(), &sink).expect("session");
        assert!(stats.q_high_water <= 8, "Q bound: {}", stats.q_high_water);
        assert!(
            stats.q_high_water >= 2,
            "queue never buffered: q_high_water = {} (worker stalled on the 2ms head item while the producer kept admitting)",
            stats.q_high_water
        );
        assert_eq!(stats.channel_resident_final, 0);
        assert_eq!(stats.stats.b_high_water, 8);
        let verdict = oracle::adjudicate(&spec, sink.ledger().unwrap());
        assert!(verdict.green(), "oracle RED: {:?}", verdict.failed());
    }

    // ---- owner-death controls ----

    struct H1PanicInjectGuard(u32);
    impl Drop for H1PanicInjectGuard {
        fn drop(&mut self) {
            H1Executor::inject_owner_panic_at(self.0);
        }
    }

    #[test]
    fn owner_panic_fails_session_loudly() {
        let _g = H1Executor::session_test_lock();
        with_watchdog(30, move || {
            let _injection = H1PanicInjectGuard(u32::MAX);
            H1Executor::inject_owner_panic_at(3);
            let spec = BatchSpec::probe_batch();
            let sink = ObserverSink::on(spec.batch_id);
            let mut ex = H1Executor::start(h1_cfg(4, 8, None), spec.clone(), sink.clone());
            ex.admit_all();
            ex.finish_admission();
            let err = ex
                .await_settlement()
                .expect_err("session must fail on owner death");
            assert_eq!(err.item, 3);
            assert!((1..=4).contains(&err.worker));
            let stats = ex.abort();
            assert_eq!(stats.stats.workers_joined, 4);
            expect_receipt_absent(&sink);
        });
    }

    #[test]
    fn owner_panic_with_saturated_b_fails_loudly() {
        let _g = H1Executor::session_test_lock();
        // B=1 (Q=1), P=1: the permit held by the panicking item is the
        // ONLY permit, so a defect path that leaks it would wedge
        // `admit_all` on b_cv forever. The permit-wait predicate must
        // observe the defect latch and bail out.
        with_watchdog(30, move || {
            let _injection = H1PanicInjectGuard(u32::MAX);
            H1Executor::inject_owner_panic_at(0);
            let spec = BatchSpec::probe_batch();
            let sink = ObserverSink::on(spec.batch_id);
            let err = run_session_h1(h1_cfg(1, 1, None), spec.clone(), &sink)
                .expect_err("session must fail on owner death");
            assert_eq!(err.item, 0);
            expect_receipt_absent(&sink);
            let ledger = sink.ledger().unwrap().snapshot();
            assert!(
                !ledger
                    .iter()
                    .any(|r| r.kind == EffectKind::AggregatePublished),
                "no aggregate may follow an owner-death abort"
            );
        });
    }

    // ---- executable interleaving controls (fence-based) ----

    #[test]
    fn workers_drain_queue_after_close_while_parked() {
        // Both workers park BEFORE any item exists; the producer admits
        // four items and closes the offer stream while they are parked.
        // Workers must still drain the whole channel and exit cleanly at
        // the disconnect — producer_done never means "channel is empty",
        // and disconnect only arrives after the drain.
        let _g = H1Executor::session_test_lock();
        with_watchdog(30, move || {
            fences::reset();
            fences::install("h1_worker_pre_select");
            let spec = BatchSpec::new(1, SEED, 4);
            let sink = ObserverSink::on(spec.batch_id);
            let mut ex = H1Executor::start(h1_cfg(2, 4, None), spec.clone(), sink.clone());
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                ex.admit_all();
                ex.finish_admission();
                ex.await_settlement().expect("settlement");
                let stats = ex.await_quiescence().expect("quiescence");
                let green = oracle::adjudicate(&spec, sink.ledger().unwrap()).green();
                tx.send((stats, green)).expect("send");
            });
            fences::wait_arrivals("h1_worker_pre_select", 2);
            // Both workers are parked at the pre-select fence; admission
            // and the close land while they are out of the select.
            fences::open("h1_worker_pre_select");
            let (stats, green) = rx.recv().expect("session completes");
            assert_eq!(stats.stats.admitted_count, 4);
            assert_eq!(stats.stats.workers_joined, 2);
            assert!(green);
        });
    }

    #[test]
    fn cancel_racing_admission_linearization() {
        // Cancel lands while the producer is parked INSIDE the admission
        // critical section (before the cut check, lock released). The
        // re-check must cut the group-7 items: never admitted, no permit
        // consumed, O7 stays green.
        let _g = H1Executor::session_test_lock();
        with_watchdog(30, move || {
            fences::reset();
            fences::install("h1_admit_pre_cut");
            let spec = BatchSpec::new(1, SEED, 4); // items 0,3 in group 7
            let sink = ObserverSink::on(spec.batch_id);
            let mut ex = H1Executor::start(h1_cfg(2, 4, None), spec.clone(), sink.clone());
            let cancel = ex.cancel_handle();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                ex.admit_all();
                ex.finish_admission();
                ex.await_settlement().expect("settlement");
                let stats = ex.await_quiescence().expect("quiescence");
                let verdict = oracle::adjudicate(&spec, sink.ledger().unwrap());
                let admitted: Vec<u32> = sink
                    .ledger()
                    .unwrap()
                    .snapshot()
                    .iter()
                    .filter(|r| r.kind == EffectKind::ItemAdmitted)
                    .filter_map(|r| r.item_id)
                    .collect();
                tx.send((stats, verdict, admitted)).expect("send");
            });
            fences::wait_arrivals("h1_admit_pre_cut", 1);
            cancel.request_cancel(7);
            fences::open("h1_admit_pre_cut");
            let (stats, verdict, admitted) = rx.recv().expect("session completes");
            assert_eq!(admitted, vec![1, 2]);
            assert_eq!(stats.stats.admitted_count, 2);
            assert!(verdict.green(), "oracle RED: {:?}", verdict.failed());
        });
    }

    #[test]
    fn settlement_survives_done_flip_while_waiter_parked() {
        // Forced schedule: every ACK precedes the producer_done flip
        // (B=1, P=1 makes this THE schedule), the settlement waiter is
        // parked INSIDE its predicate loop, and only then does the flip
        // happen. The settle_cv notify in finish_admission is the ONLY
        // wakeup source for the parked waiter — remove it and this
        // control hangs (watchdog RED).
        let _g = H1Executor::session_test_lock();
        with_watchdog(30, move || {
            fences::reset();
            fences::install("h1_producer_pre_done");
            fences::install("h1_settle_pre_wait");
            let spec = BatchSpec::new(1, SEED, 2);
            let sink = ObserverSink::off(spec.batch_id);
            let mut ex = H1Executor::start(h1_cfg(1, 1, None), spec.clone(), sink.clone());
            let watch = ex.settlement_watch();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                ex.admit_all();
                ex.finish_admission();
                let stats = ex.await_quiescence().expect("quiescence");
                tx.send(stats).expect("send");
            });
            let (wtx, wrx) = mpsc::channel();
            std::thread::spawn(move || {
                let r = watch.wait();
                wtx.send(r).expect("send");
            });
            fences::wait_arrivals("h1_settle_pre_wait", 1);
            fences::open("h1_settle_pre_wait");
            fences::wait_arrivals("h1_producer_pre_done", 1);
            fences::open("h1_producer_pre_done");
            let settled = wrx
                .recv_timeout(Duration::from_secs(10))
                .expect("settlement returns after the flip");
            settled.expect("settlement ok");
            let stats = rx.recv().expect("session completes");
            assert_eq!(stats.stats.workers_joined, 1);
            let receipt = sink.off_snapshot().unwrap();
            assert!(receipt.quiescence_recorded);
            assert_eq!(receipt.items.len(), 2);
        });
    }

    #[test]
    fn queue_empty_and_disconnect_are_not_settlement() {
        // The worker holds a received item at its post-recv fence; the
        // producer admits it and CLOSES the stream. At that moment the
        // channel is empty AND disconnected and producer_done is true —
        // yet the item's obligation is unsettled, so a settlement waiter
        // must stay parked until the real owner's ACK. The pending-based
        // predicate (never a queue-based one) is what this control pins.
        let _g = H1Executor::session_test_lock();
        with_watchdog(30, move || {
            fences::reset();
            fences::install("h1_worker_post_recv");
            let spec = BatchSpec::new(1, SEED, 1);
            let sink = ObserverSink::on(spec.batch_id);
            let mut ex = H1Executor::start(h1_cfg(1, 1, None), spec.clone(), sink.clone());
            let watch = ex.settlement_watch();
            let (wtx, wrx) = mpsc::channel();
            std::thread::spawn(move || {
                let r = watch.wait();
                wtx.send(r).expect("send");
            });
            ex.admit_all();
            ex.finish_admission();
            fences::wait_arrivals("h1_worker_post_recv", 1);
            // Channel empty + disconnected + producer_done, obligation
            // still outstanding: early await_settlement must NOT return.
            let deadline = Instant::now() + Duration::from_secs(3);
            let mut early = false;
            while Instant::now() < deadline {
                if wrx.try_recv().is_ok() {
                    early = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(
                !early,
                "settlement returned while an admitted item had no terminal \
                 (queue-emptiness or disconnect was treated as settlement)"
            );
            fences::open("h1_worker_post_recv");
            let settled = wrx
                .recv_timeout(Duration::from_secs(10))
                .expect("settlement returns after the owner ACK");
            settled.expect("settlement ok");
            let stats = ex.await_quiescence().expect("quiescence");
            assert_eq!(stats.stats.workers_joined, 1);
            assert!(oracle::adjudicate(&spec, sink.ledger().unwrap()).green());
        });
    }

    #[test]
    fn quiescence_before_stream_close_fails_loudly_not_hang() {
        // await_quiescence() on an executor whose offer stream is still
        // open would join workers parked in the channel select — an
        // unbounded hang (disconnect requires finish_admission). The
        // precondition guard must reject BEFORE the join; the watchdog is
        // the oracle. The parked pool is then recovered with abort().
        let _g = H1Executor::session_test_lock();
        with_watchdog(6, move || {
            let spec = BatchSpec::new(1, SEED, 4);
            let sink = ObserverSink::off(spec.batch_id);
            let mut ex = H1Executor::start(h1_cfg(2, 4, None), spec, sink.clone());
            let err = ex
                .await_quiescence()
                .expect_err("guard must reject an un-settled executor");
            assert!(
                err.message.contains("offer stream"),
                "unexpected error: {err:?}"
            );
            let stats = ex.abort();
            assert_eq!(stats.stats.workers_spawned, 2);
            assert_eq!(stats.stats.workers_joined, 2);
            assert!(
                !sink.off_snapshot().unwrap().quiescence_recorded,
                "a rejected quiescence must not publish a receipt"
            );
        });
    }

    // ---- abort family ----

    #[test]
    fn abort_wakes_parked_workers_bounded() {
        // All workers parked in the channel select with the stream open;
        // WORK_READY stays false forever. abort must wake them via the
        // control plane and join everything in bounded time — no receipt.
        let _g = H1Executor::session_test_lock();
        with_watchdog(30, move || {
            let spec = BatchSpec::new(1, SEED, 0);
            let sink = ObserverSink::on(spec.batch_id);
            let mut ex = H1Executor::start(h1_cfg(2, 4, None), spec, sink.clone());
            let stats = ex.abort();
            assert_eq!(stats.stats.workers_spawned, 2);
            assert_eq!(stats.stats.workers_joined, 2);
            expect_receipt_absent(&sink);
        });
    }

    #[test]
    fn abort_with_saturated_queue_leaves_obligation_residue() {
        // P=1, B=2 (Q=2): both workers' budget is saturated BEFORE the
        // stream closes — item 0 and item 1 are queued while the worker
        // is parked at its pre-select fence. abort lands in that
        // queue-full state. Required behavior (Gate II §11/§12): the
        // worker re-checks the authoritative flag after the fence and
        // exits WITHOUT receiving or executing anything, nothing is
        // forged, the obligations stay visible as honestly-resident
        // queue messages, and the join is bounded with no receipt.
        let _g = H1Executor::session_test_lock();
        with_watchdog(30, move || {
            fences::reset();
            fences::install("h1_worker_pre_select");
            let spec = BatchSpec::new(1, SEED, 2);
            let sink = ObserverSink::on(spec.batch_id);
            let mut ex = H1Executor::start(h1_cfg(1, 2, None), spec.clone(), sink.clone());
            // The fence opener is driven by abort's OWN control signal:
            // the helper deterministically opens the gate exactly when
            // the wake signal for this worker lands — no sleeps, and the
            // worker can never pass the fence before the flag is set.
            let ctl_rx = ex.leak_ctl_receiver_clone();
            std::thread::spawn(move || {
                let _ = ctl_rx.recv();
                fences::open("h1_worker_pre_select");
            });
            fences::wait_arrivals("h1_worker_pre_select", 1);
            ex.admit_all();
            let stats = ex.abort();
            assert_eq!(stats.stats.workers_joined, 1);
            assert_eq!(stats.stats.admitted_count, 2);
            assert_eq!(
                stats.channel_resident_final, 2,
                "both items stay queued, honestly resident"
            );
            assert!(stats.q_high_water <= 2);
            let ledger = sink.ledger().unwrap().snapshot();
            assert_eq!(
                ledger
                    .iter()
                    .filter(|r| r.kind == EffectKind::ItemAdmitted)
                    .count(),
                2
            );
            assert!(
                !ledger.iter().any(|r| r.terminal.is_some()),
                "no terminal may be forged under abort"
            );
            assert!(
                !ledger.iter().any(|r| r.terminal.is_some()
                    || r.kind == EffectKind::TransformDone
                    || r.kind == EffectKind::IoWaitDone),
                "no item work may run after the abort flag is authoritative"
            );
            expect_receipt_absent(&sink);
        });
    }

    #[test]
    fn abort_mid_flight_bounded_all_outcomes_legal() {
        // Abort at an arbitrary mid-flight moment (the worker is usually
        // inside item 0's 2ms wait when the flag flips; the race is
        // benign because EVERY legal outcome is asserted): the executing
        // owner completes its item honestly; an item only received in
        // the post-recv shutdown re-check window leaves its obligation
        // honestly outstanding; unreceived items stay honestly resident
        // in the channel. Whatever the interleaving: the join is bounded
        // (watchdog), every ACK belongs to an admitted item and is the
        // only one, admitted == ACKed + resident, and no receipt exists.
        let _g = H1Executor::session_test_lock();
        with_watchdog(30, move || {
            let spec = BatchSpec::new(1, SEED, 4);
            let sink = ObserverSink::on(spec.batch_id);
            let mut ex = H1Executor::start(h1_cfg(1, 4, None), spec.clone(), sink.clone());
            ex.admit_all();
            let stats = ex.abort();
            assert_eq!(stats.stats.workers_joined, 1, "bounded join");
            assert_eq!(stats.stats.admitted_count, 4);
            assert!(stats.stats.b_high_water <= 4);
            assert!(stats.q_high_water <= 4);
            let ledger = sink.ledger().unwrap().snapshot();
            let acked: Vec<u32> = ledger
                .iter()
                .filter(|r| r.terminal.is_some())
                .filter_map(|r| r.item_id)
                .collect();
            let mut sorted = acked.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted, acked, "at most one terminal per item");
            assert!(
                acked.iter().all(|id| (*id as usize) < 4),
                "every ACK belongs to an admitted item"
            );
            // Per-item partition of the four admitted items (review A F1,
            // delta-reviewed formulation): every admitted item is either
            // ACKed or effect-less at stats-read time — abort() joins all
            // workers before the stats are read, and an executed item
            // always reaches its ACK, so "effects without ACK" is illegal
            // and "not ACKed" (unreceived OR received-but-dropped at the
            // post-recv recheck, which leaves no per-item record) is
            // exactly the effect-less class. The ledger cannot express the
            // resident-vs-dropped split per item, so the resident witness
            // is bounded aggregate-side instead of being double-counted.
            let execution_effects = |id: u32| {
                ledger.iter().any(|r| {
                    r.item_id == Some(id)
                        && (r.kind == EffectKind::IoWaitDone
                            || r.kind == EffectKind::TransformDone
                            || (r.kind == EffectKind::ResultPublished
                                && r.log == crate::ledger::LogKind::Effect))
                })
            };
            let not_acked: Vec<u32> = (0u32..4).filter(|id| !acked.contains(id)).collect();
            assert!(
                not_acked.iter().all(|id| !execution_effects(*id)),
                "execution effects without a terminal ACK are illegal under abort"
            );
            assert_eq!(
                acked.len() + not_acked.len(),
                4,
                "admitted partitions exactly into ACKed and effect-less"
            );
            assert!(
                stats.channel_resident_final <= not_acked.len(),
                "resident witness {} exceeds the not-ACKed class {}",
                stats.channel_resident_final,
                not_acked.len()
            );
            expect_receipt_absent(&sink);
        });
    }

    // ---- channel-specific failure controls (Gate II §15) ----

    #[test]
    fn sender_leak_stalls_drain_and_abort_recovers() {
        // A leaked work-sender clone keeps the channel connected after
        // finish_admission: items all settle (settlement is pending-
        // based, NOT disconnect-based) but the workers can never observe
        // Disconnected, so the quiescence join would have no termination
        // guarantee. The control pins the hazard and the bounded
        // recovery: abort's control plane (flag + ctl wakes) joins every
        // worker regardless of channel state, with no receipt.
        let _g = H1Executor::session_test_lock();
        with_watchdog(30, move || {
            let spec = BatchSpec::new(1, SEED, 2);
            let sink = ObserverSink::on(spec.batch_id);
            let mut ex = H1Executor::start(h1_cfg(1, 2, None), spec.clone(), sink.clone());
            let leaked = ex.leak_work_sender_clone();
            ex.admit_all();
            ex.finish_admission();
            ex.await_settlement()
                .expect("settlement does not need the disconnect");
            // Bounded observation: with the leaked sender, no worker can
            // exit the drain funnel (no disconnect exists to observe).
            let deadline = Instant::now() + Duration::from_millis(500);
            let mut stalled = true;
            while Instant::now() < deadline {
                let exits = sink
                    .ledger()
                    .unwrap()
                    .snapshot()
                    .iter()
                    .filter(|r| r.kind == EffectKind::ExecutorExited)
                    .count();
                if exits > 0 {
                    stalled = false;
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(
                stalled,
                "a worker exited without a disconnect — the leak-stall hazard \
                 this control documents is not real"
            );
            let stats = ex.abort();
            drop(leaked);
            assert_eq!(stats.stats.workers_joined, 1);
            expect_receipt_absent(&sink);
        });
    }

    #[test]
    fn channel_send_failure_fails_closed() {
        // Injected Full at item 5 (the only way the structurally
        // unreachable Full arm runs without violating the frozen
        // budgets): the admission must roll back BEFORE any ledger
        // record exists (no ghost admission, no leaked permit) and the
        // session must fail loudly — defect reported, everything joined,
        // no aggregate, no receipt.
        let _g = H1Executor::session_test_lock();
        with_watchdog(30, move || {
            let _injection = H1Executor::inject_send_full_at(5);
            let spec = BatchSpec::probe_batch();
            let sink = ObserverSink::on(spec.batch_id);
            let err = run_session_h1(h1_cfg(2, 4, None), spec.clone(), &sink)
                .expect_err("a channel invariant violation must fail the session");
            assert_eq!(err.item, 5);
            assert_eq!(err.worker, 0, "an admission-side failure names no worker");
            let ledger = sink.ledger().unwrap().snapshot();
            let admitted: Vec<u32> = ledger
                .iter()
                .filter(|r| r.kind == EffectKind::ItemAdmitted)
                .filter_map(|r| r.item_id)
                .collect();
            assert_eq!(
                admitted,
                vec![0, 1, 2, 3, 4],
                "the failed admission must leave NO ItemAdmitted record"
            );
            assert!(
                !ledger.iter().any(|r| r.item_id == Some(5)),
                "item 5 must have no records at all (stays Offered)"
            );
            assert!(
                !ledger
                    .iter()
                    .any(|r| r.kind == EffectKind::AggregatePublished),
                "no aggregate may follow a channel-invariant defect"
            );
            expect_receipt_absent(&sink);
        });
    }

    #[test]
    fn cancel_between_admission_and_close_interleave_green() {
        // finish_admission vs cancel interleave: the cancel lands in the
        // window between the last admission and the offer-stream close.
        // Admitted items resolve legally (complete or owner-observed
        // abandon), the session settles, and the frozen oracle stays
        // green — closing the stream is not a cancel and a cancel is not
        // a close.
        let _g = H1Executor::session_test_lock();
        with_watchdog(30, move || {
            fences::reset();
            fences::install("h1_producer_pre_done");
            let spec = BatchSpec::new(1, SEED, 4); // items 0,3 in group 7
            let sink = ObserverSink::on(spec.batch_id);
            let mut ex = H1Executor::start(h1_cfg(2, 4, None), spec.clone(), sink.clone());
            let cancel = ex.cancel_handle();
            let sink_in = sink.clone();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                ex.admit_all();
                ex.finish_admission();
                ex.await_settlement().expect("settlement");
                let stats = ex.await_quiescence().expect("quiescence");
                let verdict = oracle::adjudicate(&spec, sink_in.ledger().unwrap());
                tx.send((stats, verdict)).expect("send");
            });
            fences::wait_arrivals("h1_producer_pre_done", 1);
            cancel.request_cancel(7);
            fences::open("h1_producer_pre_done");
            let (stats, verdict) = rx.recv().expect("session completes");
            assert_eq!(stats.stats.admitted_count, 4);
            let cancels: Vec<u32> = sink
                .ledger()
                .unwrap()
                .snapshot()
                .iter()
                .filter(|r| r.kind == EffectKind::CancelRequested)
                .map(|r| r.payload as u32)
                .collect();
            assert_eq!(cancels, vec![7]);
            assert!(verdict.green(), "oracle RED: {:?}", verdict.failed());
        });
    }
}
