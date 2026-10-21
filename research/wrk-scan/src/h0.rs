//! H0 — the clean native-threads baseline executor (Gate 授权 §2.3,
//! SPEC §4.2/§4.5).
//!
//! H0 = `Native Threads + Mutex/Condvar + Shared FIFO`, std-only:
//! - P long-lived worker threads spawned once per session (never
//!   per-item); each worker executes at most one item at a time, so
//!   `active_item_execution <= P` holds structurally and the observed
//!   high-water is recorded.
//! - B is the frozen admitted-not-terminal budget (queue resident +
//!   running + completed-not-ACK), enforced by the producer blocking on a
//!   predicate condvar — NOT a queue capacity.
//! - Admission follows the frozen ADMISSION_RECIPE (SPEC §4.2): the B
//!   permit wait, the cut check and the enqueue + obligation bookkeeping
//!   happen in ONE critical section keyed on the cancel-order mutex; the
//!   B permit is released only after the authoritative terminal ACK.
//! - The cancel milestone is the workload owner's single-threaded
//!   admission count (SPEC §5.6) — never a completion count or duration.
//! - Every condvar wait is a predicate loop (`while !pred { wait }`);
//!   there is no polling and no timed wait anywhere in the executor.
//! - Worker panics are contained (`catch_unwind`); the session fails
//!   loudly instead of forging quiescence (SPEC §5.4 PD premise: the
//!   frozen terminal vocabulary has no panic-terminal, so an item whose
//!   owner panicked cannot be honestly terminalized — the SESSION fails
//!   with an explicit error and no receipt is published).
//!
//! Session roles are split by handle so the frozen three-action contract
//! (request_cancel / await_settlement / await_quiescence, SPEC §5.5) is
//! composable from distinct threads without sharing `&mut`:
//! [`H0Executor`] (admission + quiescence owner), [`CancelHandle`],
//! [`SettlementWatch`]. This is what the K0-ON teardown needs.
//!
//! This is an independent implementation: the G2 probe's scheduling
//! logic (its 50ms `wait_timeout` diagnostic polling, its mutant fields)
//! is deliberately NOT inherited (SPEC §4.5). The `ref_result` shortcut
//! is never called on the execution path — workers run the real content
//! primitives (SPEC §2.5(b)).
//!
//! `cfg(test)`-only facilities (compiled out of every non-test build):
//! `fences` — named gates that force documented interleavings for the
//! negative controls, never sleep-based; `TEST_PANIC_ITEM` — owner-panic
//! injection for the owner-death control.

// Fence macros must precede their first use. In non-test builds every
// call site compiles to nothing.
#[cfg(not(test))]
macro_rules! h0_fence_park {
    ($name:literal, $guard:expr, $lock:expr) => {{
        let _ = &$guard;
        let _ = &$lock;
    }};
}
#[cfg(test)]
macro_rules! h0_fence_park {
    ($name:literal, $guard:expr, $lock:expr) => {
        $guard = $crate::h0::fences::park($name, $guard, $lock);
    };
}

use std::collections::{HashSet, VecDeque};
use std::panic::{self, AssertUnwindSafe};
#[cfg(test)]
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::ledger::TerminalKind;
use crate::observer::ObserverSink;
use crate::oracle::fold_meta;
use crate::workload::{self, BatchSpec};

/// Session configuration: P execution parallelism, B admitted-not-terminal
/// budget, and the optional frozen cancel milestone (k-th admission, group).
#[derive(Clone, Copy, Debug)]
pub struct H0Config {
    pub p: usize,
    pub b: usize,
    /// `(k, group)`: request_cancel(group) fires after the k-th admission.
    pub cancel_milestone: Option<(u32, u32)>,
}

/// Why a session failed. Published only by real owner-death handling —
/// never by the harness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionError {
    pub worker: u64,
    pub item: u32,
    pub message: String,
}

/// Mode-independent lifecycle statistics (same values in ON and OFF).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionStats {
    pub admitted_count: usize,
    pub b_high_water: usize,
    pub active_execution_high_water: usize,
    pub workers_spawned: usize,
    pub workers_joined: usize,
}

struct H0State {
    queue: VecDeque<u32>,
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
    b_high_water: usize,
    active_high_water: usize,
    admitted_total: usize,
    results: Vec<(u32, u64)>,
    panic_defect: Option<SessionError>,
}

struct Shared {
    state: Mutex<H0State>,
    /// Workers wait here for queue items / producer_done / shutdown.
    work_cv: Condvar,
    /// Producer waits here for B permit availability.
    slot_cv: Condvar,
    /// Settlement waiters (and the session runner awaiting a defect).
    settle_cv: Condvar,
}

/// The H0 executor. `start` spawns the worker pool; the session owner
/// (control thread) then drives admission, settlement and quiescence.
pub struct H0Executor {
    cfg: H0Config,
    spec: BatchSpec,
    sink: ObserverSink,
    shared: Arc<Shared>,
    handles: Vec<JoinHandle<()>>,
}

impl H0Executor {
    /// Spawn P long-lived workers (each recording ExecutorSpawned at the
    /// true spawn boundary) and return the session handle. Admission has
    /// NOT begun; the session owner drives it with [`Self::admit_all`].
    pub fn start(cfg: H0Config, spec: BatchSpec, sink: ObserverSink) -> Self {
        assert!(cfg.p >= 1, "P must be >= 1");
        assert!(cfg.b >= 1, "B must be >= 1");
        let shared = Arc::new(Shared {
            state: Mutex::new(H0State {
                queue: VecDeque::new(),
                admitted_not_terminal: 0,
                pending: HashSet::new(),
                cancelled_groups: HashSet::new(),
                producer_done: false,
                shutdown_requested: false,
                active_execution: 0,
                b_high_water: 0,
                active_high_water: 0,
                admitted_total: 0,
                results: Vec::new(),
                panic_defect: None,
            }),
            work_cv: Condvar::new(),
            slot_cv: Condvar::new(),
            settle_cv: Condvar::new(),
        });
        let mut handles = Vec::with_capacity(cfg.p);
        for w in 1..=cfg.p as u64 {
            sink.worker_spawned(w);
            let shared = Arc::clone(&shared);
            let sink = sink.clone();
            let spec = spec.clone();
            handles.push(
                std::thread::Builder::new()
                    .name(format!("h0-worker-{w}"))
                    .spawn(move || worker_main(w, spec, sink, shared))
                    .expect("spawn h0 worker"),
            );
        }
        H0Executor {
            cfg,
            spec,
            sink,
            shared,
            handles,
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
                // A latched defect has already failed the session: stop
                // admitting instead of blocking on permits that the dead
                // owner will never release — await_settlement reports the
                // defect and abort() joins everything (loud, never wedged).
                if st.panic_defect.is_some() {
                    return;
                }
                st = self.shared.slot_cv.wait(st).unwrap();
            }
            // (C) cancel-order critical section: the linearization point.
            // The B permit is virtual here (the count is not incremented
            // yet), so a cut releases it by not consuming it — N1(b)
            // holds structurally and no permit can leak. The fence point
            // (test-only) parks WITH the lock released, letting a cancel
            // land in the window before the cut check; the re-check below
            // is what makes the linearization observable.
            h0_fence_park!("admit_pre_cut", st, &self.shared.state);
            if st.cancelled_groups.contains(&group) {
                // Cut: the item stays Offered with no terminal obligation.
                drop(st);
                continue;
            }
            // Admit: record, enqueue, take the obligation — all under the
            // same cancel-order critical section.
            self.sink.admit(id, group, 0);
            st.queue.push_back(id);
            st.pending.insert(id);
            st.admitted_not_terminal += 1;
            st.admitted_total += 1;
            st.b_high_water = st.b_high_water.max(st.admitted_not_terminal);
            drop(st);
            self.shared.work_cv.notify_one();
            // (E) workload-owned cancel milestone: admission-count based,
            // executor-speed independent.
            admitted += 1;
            if let Some((k, group)) = self.cfg.cancel_milestone {
                if admitted == k {
                    self.cancel_handle().request_cancel(group);
                }
            }
        }
    }

    /// Close the offer stream (the fourth, explicit action of SPEC §5.5):
    /// flip producer_done under the lock, then wake BOTH waiter families
    /// — workers may now drain-exit, and the settlement predicate
    /// (producer_done && pending empty) may have just become true with no
    /// further notification source. Dropping the settle_cv notify is the
    /// classic lost wakeup (negative control).
    pub fn finish_admission(&mut self) {
        let mut st = self.shared.state.lock().unwrap();
        h0_fence_park!("producer_pre_done", st, &self.shared.state);
        st.producer_done = true;
        drop(st);
        self.shared.work_cv.notify_all();
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
    pub fn await_quiescence(&mut self) -> Result<SessionStats, SessionError> {
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
        let agg = workload::aggregate(&sorted);
        self.sink.aggregate(agg);
        self.sink.quiescence_receipt();
        Ok(stats)
    }

    /// Owner-death abort: request worker shutdown, join everything, and
    /// return the stats WITHOUT publishing any aggregate or receipt.
    /// The obligation left behind by the dead owner is reported, never
    /// silently discharged by the harness.
    pub fn abort(&mut self) -> SessionStats {
        {
            let mut st = self.shared.state.lock().unwrap();
            st.shutdown_requested = true;
        }
        self.shared.work_cv.notify_all();
        self.join_all()
    }

    fn join_workers(&mut self) -> Result<SessionStats, SessionError> {
        let stats = self.join_all();
        if stats.workers_joined != self.cfg.p {
            // A worker died outside `catch_unwind` (or was never joined):
            // loud failure, never a receipt over an unjoined executor.
            return Err(SessionError {
                worker: 0,
                item: u32::MAX,
                message: format!(
                    "only {}/{} workers joined cleanly",
                    stats.workers_joined, self.cfg.p
                ),
            });
        }
        Ok(stats)
    }

    /// Join every handle and report the stats whatever happened — abort's
    /// best-effort face (the failure is the caller's to publish).
    fn join_all(&mut self) -> SessionStats {
        let mut joined = 0usize;
        for h in self.handles.drain(..) {
            if h.join().is_ok() {
                joined += 1;
            }
        }
        let st = self.shared.state.lock().unwrap();
        SessionStats {
            admitted_count: st.admitted_total,
            b_high_water: st.b_high_water,
            active_execution_high_water: st.active_high_water,
            workers_spawned: self.cfg.p,
            workers_joined: joined,
        }
    }
}

/// The `request_cancel` role bound to one session (SPEC §5.5 action 1):
/// records the intent and cuts admission for the group. Promises nothing
/// about in-flight items — they complete or are abandoned by their real
/// owner (SPEC §5.3). The cancel record is appended while holding the
/// cancel-order lock, so any later Abandoned ACK for the group sorts
/// after it in ledger order.
#[derive(Clone)]
pub struct CancelHandle {
    shared: Arc<Shared>,
    sink: ObserverSink,
}

impl CancelHandle {
    pub fn request_cancel(&self, group: u32) {
        let mut st = self.shared.state.lock().unwrap();
        if st.cancelled_groups.insert(group) {
            self.sink.cancel(group);
        }
        drop(st);
        self.shared.work_cv.notify_all();
        self.shared.slot_cv.notify_all();
    }
}

/// The `await_settlement` role bound to one session (SPEC §5.5 action 2).
#[derive(Clone)]
pub struct SettlementWatch {
    shared: Arc<Shared>,
}

impl SettlementWatch {
    pub fn wait(&self) -> Result<(), SessionError> {
        let mut st = self.shared.state.lock().unwrap();
        loop {
            if st.producer_done && (st.pending.is_empty() || st.panic_defect.is_some()) {
                break;
            }
            h0_fence_park!("settle_pre_wait", st, &self.shared.state);
            // Post-fence predicate re-check (lost-wakeup defense, same
            // discipline as the worker loop): the fence may have released
            // the lock, so a settle_cv notify may have landed while parked.
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
pub fn run_session(
    cfg: H0Config,
    spec: BatchSpec,
    sink: &ObserverSink,
) -> Result<SessionStats, SessionError> {
    let mut ex = H0Executor::start(cfg, spec, sink.clone());
    ex.admit_all();
    ex.finish_admission();
    if let Err(defect) = ex.await_settlement() {
        let _ = ex.abort();
        return Err(defect);
    }
    ex.await_quiescence()
}

fn worker_main(worker: u64, spec: BatchSpec, sink: ObserverSink, shared: Arc<Shared>) {
    loop {
        let mut st = shared.state.lock().unwrap();
        while st.queue.is_empty() && !st.producer_done && !st.shutdown_requested {
            h0_fence_park!("worker_pre_wait", st, &shared.state);
            // Re-evaluate the predicate AFTER the fence: the fence may
            // have released the state lock, so a notify may have landed
            // while this thread was parked — entering condvar.wait now
            // would lose it. This re-check is the structural lost-wakeup
            // defense the fence-based negative controls exercise.
            if !(st.queue.is_empty() && !st.producer_done && !st.shutdown_requested) {
                break;
            }
            st = shared.work_cv.wait(st).unwrap();
        }
        if st.shutdown_requested || (st.queue.is_empty() && st.producer_done) {
            // The single legal exit funnel: record ExecutorExited at the
            // true lifecycle boundary (this thread returns right after)
            // and wake the pool.
            drop(st);
            sink.worker_exit(worker);
            shared.work_cv.notify_all();
            return;
        }
        let item = st.queue.pop_front().expect("non-empty queue under lock");
        let cancelled = st
            .cancelled_groups
            .contains(&spec.items[item as usize].cancellation_group);
        st.active_execution += 1;
        st.active_high_water = st.active_high_water.max(st.active_execution);
        drop(st);

        sink.item_start(item);
        if cancelled {
            // The owner observed the cancel while actually holding the
            // item (dequeue observation): legal Abandon (SPEC §5.1).
            finish_terminal(&sink, &shared, item, TerminalKind::Abandoned, worker, None);
        } else {
            // PD premise: panic-contained so the owner survives; the
            // frozen terminal vocabulary has no panic-terminal, so the
            // undischarged obligation fails the session (never forged).
            let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
                execute_item(worker, &spec, &sink, item)
            }));
            match outcome {
                Ok((term, contribution)) => {
                    finish_terminal(&sink, &shared, item, term, worker, Some(contribution));
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
                    shared.slot_cv.notify_all();
                }
            }
        }
        let mut st = shared.state.lock().unwrap();
        st.active_execution -= 1;
        drop(st);
        shared.slot_cv.notify_all();
    }
}

/// The REAL item work: wait + input generation + CPU transform + content
/// digest + contribution. Never `ref_result` (SPEC §2.5(b)); the fault
/// lane performs all common work and then publishes CompletedErr(0x31).
fn execute_item(
    worker: u64,
    spec: &BatchSpec,
    sink: &ObserverSink,
    item_id: u32,
) -> (TerminalKind, u64) {
    #[cfg(test)]
    {
        if test_panic_item() == item_id {
            panic!("h0 test-injected owner panic for item {item_id}");
        }
    }
    let s = &spec.items[item_id as usize];
    if s.wait_us > 0 {
        // Synthetic logical I/O wait — a workload parameter (simulated
        // pacing), never a synchronization premise (SPEC §3.2).
        std::thread::sleep(Duration::from_micros(s.wait_us));
        sink.wait_done(item_id, s.wait_us, worker);
    }
    let bytes = workload::generate_input(spec.seed, item_id);
    let (meta, peak) = workload::transform(item_id, &bytes, s.cpu_blocks);
    let content = workload::content_digest(item_id, &bytes);
    let contribution = workload::contribution(&content, &meta, peak);
    sink.transform_done(item_id, fold_meta(&meta), worker);
    sink.result_published(item_id, contribution, worker);
    let term = match s.fault {
        Some(code) => TerminalKind::CompletedErr(code),
        None => TerminalKind::CompletedOk,
    };
    (term, contribution)
}

/// Terminal ACK by the real owner (the completing/abandoning worker),
/// then B-permit release + obligation discharge + fan-in sink update,
/// then wake the backpressure and settlement waiters.
fn finish_terminal(
    sink: &ObserverSink,
    shared: &Shared,
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
    shared.slot_cv.notify_all();
    shared.settle_cv.notify_all();
}

// ---- cfg(test)-only facilities ----

#[cfg(test)]
static TEST_PANIC_ITEM: AtomicU32 = AtomicU32::new(u32::MAX);

/// Serializes every session test (h0 + k0_session): the fence gates and
/// TEST_PANIC_ITEM are process-global, so concurrent sessions would
/// observe each other's injections.
#[cfg(test)]
pub(crate) fn session_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
fn test_panic_item() -> u32 {
    TEST_PANIC_ITEM.load(Ordering::Relaxed)
}

#[cfg(test)]
impl H0Executor {
    /// Test-only owner-panic injection for the negative control.
    pub(crate) fn inject_owner_panic_at(item: u32) {
        TEST_PANIC_ITEM.store(item, Ordering::Relaxed);
    }
}

/// Named fence gates, `cfg(test)` only. A test installs a gate; an
/// executor thread reaching the matching `h0_fence_park!` point
/// registers its arrival and blocks — TEMPORARILY RELEASING the state
/// lock it holds — until the test opens the gate. This forces the
/// documented interleavings deterministically, never with sleeps, and
/// leaves the fenced critical region observable to the other threads
/// exactly as a legal interleaving would.
#[cfg(test)]
pub(crate) mod fences {
    use std::collections::HashMap;
    use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};

    struct Gate {
        arrivals: Mutex<usize>,
        arrived_cv: Condvar,
        open: Mutex<bool>,
        open_cv: Condvar,
    }

    fn gates() -> &'static Mutex<HashMap<&'static str, Arc<Gate>>> {
        static GATES: OnceLock<Mutex<HashMap<&'static str, Arc<Gate>>>> = OnceLock::new();
        GATES.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn gate(name: &str) -> Option<Arc<Gate>> {
        gates().lock().unwrap().get(name).cloned()
    }

    /// Drop every gate. Called at test start: gates persist across
    /// tests (process-global), and stale gates would trip install's
    /// double-install guard and leak open/closed state.
    pub fn reset() {
        gates().lock().unwrap().clear();
    }

    /// Install a closed gate. Panics if already installed (test bug).
    pub fn install(name: &'static str) {
        let g = Arc::new(Gate {
            arrivals: Mutex::new(0),
            arrived_cv: Condvar::new(),
            open: Mutex::new(false),
            open_cv: Condvar::new(),
        });
        assert!(
            gates().lock().unwrap().insert(name, g).is_none(),
            "fence gate {name} installed twice"
        );
    }

    /// Block (test side) until `expected` threads reached the gate.
    /// The arrivals guard is released before parking on the condvar, so a
    /// parked executor thread can never block the test's view of arrivals.
    pub fn wait_arrivals(name: &str, expected: usize) {
        let g = gate(name).expect("gate installed");
        let mut n = g.arrivals.lock().unwrap();
        while *n < expected {
            n = g.arrived_cv.wait(n).unwrap();
        }
    }

    /// Open the gate, releasing every parked thread.
    pub fn open(name: &str) {
        let g = gate(name).expect("gate installed");
        *g.open.lock().unwrap() = true;
        g.open_cv.notify_all();
    }

    /// Executor-side fence point: if the named gate is installed and
    /// closed, register arrival, release the lock while parked, then
    /// re-acquire and return. The caller's loop/critical-section
    /// re-check after the fence is exactly what the controls exercise.
    pub fn park<'a, T>(
        name: &str,
        guard: MutexGuard<'a, T>,
        lock: &'a Mutex<T>,
    ) -> MutexGuard<'a, T> {
        let g = match gate(name) {
            Some(g) => g,
            None => {
                return guard;
            }
        };
        if *g.open.lock().unwrap() {
            return guard;
        }
        {
            // Register the arrival and release the arrivals mutex BEFORE
            // parking: holding it across the park would deadlock any test
            // that waits to observe the arrival before opening the gate.
            {
                let mut a = g.arrivals.lock().unwrap();
                *a += 1;
                g.arrived_cv.notify_all();
            }
            let mut o = g.open.lock().unwrap();
            drop(guard);
            while !*o {
                o = g.open_cv.wait(o).unwrap();
            }
        }
        let g2 = lock.lock().unwrap();
        g2
    }
}

#[cfg(test)]
pub(crate) mod tests {
    //! H0 lifecycle obligations and executable negative controls. The
    //! fence-based controls serialize on one mutex (fence gates are
    //! process-global); every control runs under a watchdog whose ONLY
    //! job is timeout protection — a hang is a failure, never evidence.

    use super::*;
    use crate::oracle;
    use std::sync::mpsc;
    use std::time::Instant;

    pub(crate) const SEED: u64 = 0x00C0_FFEE_0304_0001;

    pub(crate) fn with_watchdog<T: Send + 'static>(
        secs: u64,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let v = f();
            let _ = tx.send(v);
        });
        let deadline = Instant::now() + Duration::from_secs(secs);
        loop {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(v) => return v,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if Instant::now() > deadline {
                        panic!("watchdog: control did not settle within {secs}s");
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("watchdog: control thread panicked");
                }
            }
        }
    }

    pub(crate) fn probe_cfg(p: usize, b: usize, cancel: Option<(u32, u32)>) -> H0Config {
        H0Config {
            p,
            b,
            cancel_milestone: cancel,
        }
    }

    // ---- lifecycle GREEN arms ----

    #[test]
    fn h0_on_mode_no_cancel_full_oracle_green() {
        let _g = super::session_test_lock();
        let spec = BatchSpec::probe_batch();
        let sink = ObserverSink::on(spec.batch_id);
        let stats = run_session(probe_cfg(4, 8, None), spec.clone(), &sink).expect("session");
        assert_eq!(stats.admitted_count, 48);
        assert!(
            stats.b_high_water <= 8,
            "B bound violated: {}",
            stats.b_high_water
        );
        assert!(stats.active_execution_high_water <= 4);
        assert_eq!(stats.workers_spawned, 4);
        assert_eq!(stats.workers_joined, 4);
        let verdict = oracle::adjudicate(&spec, sink.ledger().unwrap());
        assert!(verdict.green(), "oracle RED: {:?}", verdict.failed());
    }

    #[test]
    fn h0_on_mode_cancel_milestone_full_oracle_green() {
        let _g = super::session_test_lock();
        // cancel after the 24th admission (group 7), the probe milestone.
        let spec = BatchSpec::probe_batch();
        let sink = ObserverSink::on(spec.batch_id);
        let stats =
            run_session(probe_cfg(4, 8, Some((24, 7))), spec.clone(), &sink).expect("session");
        // Admission order is linear: items 0..=23 all admitted; after the
        // milestone only group-1 items (id % 3 != 0) may be admitted.
        let expected_admitted = 24 + (24..48u32).filter(|id| id % 3 != 0).count();
        assert_eq!(stats.admitted_count, expected_admitted);
        assert!(stats.b_high_water <= 8);
        assert!(stats.active_execution_high_water <= 4);
        let verdict = oracle::adjudicate(&spec, sink.ledger().unwrap());
        assert!(verdict.green(), "oracle RED: {:?}", verdict.failed());
    }

    #[test]
    fn h0_off_mode_receipt_structure() {
        let _g = super::session_test_lock();
        let spec = BatchSpec::probe_batch();
        let sink = ObserverSink::off(spec.batch_id);
        let stats = run_session(probe_cfg(4, 8, None), spec.clone(), &sink).expect("session");
        assert_eq!(stats.workers_joined, 4);
        let receipt = sink.off_snapshot().unwrap();
        assert_eq!(receipt.items.len(), 48);
        assert!(receipt.quiescence_recorded);
        assert_eq!(receipt.worker_exits.len(), 4);
        assert!(receipt.cancels.is_empty());
        // Terminal coverage: every admitted item exactly one terminal,
        // ordinals/timestamps ordered, wait obligation retained per item.
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
        // The OFF aggregate equals the frozen probe receipt.
        assert_eq!(receipt.aggregate, Some(0x980a_9dc0_d8d8_cee6));
    }

    #[test]
    fn s5_scenario_cell_cancel_oracle_green() {
        let _g = super::session_test_lock();
        // Real frozen cell: S5, P=4, B=8 (2P), cancel after the 164th
        // admission, group 7. Non-timed correctness run.
        let cell = crate::scenario::Scenario::S5;
        let spec = cell.batch_spec(1, SEED);
        let sink = ObserverSink::on(spec.batch_id);
        let stats = run_session(
            probe_cfg(4, 8, cell.cancel_milestone()),
            spec.clone(),
            &sink,
        )
        .expect("session");
        assert!(stats.b_high_water <= 8);
        assert!(stats.active_execution_high_water <= 4);
        assert!(stats.admitted_count < spec.items.len(), "the cut must bite");
        let verdict = oracle::adjudicate(&spec, sink.ledger().unwrap());
        assert!(verdict.green(), "oracle RED: {:?}", verdict.failed());
    }

    // ---- B/P budget controls ----

    #[test]
    fn full_b_producer_blocks_and_recovers() {
        let _g = super::session_test_lock();
        // B=2, P=2, 48 items: the producer MUST hit the B wall and only
        // progress after terminal ACKs release permits. b_high_water == B
        // exactly, never above; active execution never above P.
        let spec = BatchSpec::probe_batch();
        let sink = ObserverSink::on(spec.batch_id);
        let stats = run_session(probe_cfg(2, 2, None), spec.clone(), &sink).expect("session");
        assert_eq!(stats.b_high_water, 2);
        assert!(stats.active_execution_high_water <= 2);
        let verdict = oracle::adjudicate(&spec, sink.ledger().unwrap());
        assert!(verdict.green(), "oracle RED: {:?}", verdict.failed());
    }

    // ---- owner-death control ----

    struct PanicInjectGuard(u32);
    impl Drop for PanicInjectGuard {
        fn drop(&mut self) {
            H0Executor::inject_owner_panic_at(self.0);
        }
    }

    #[test]
    fn owner_panic_fails_session_loudly() {
        let _g = super::session_test_lock();
        with_watchdog(30, move || {
            let _injection = PanicInjectGuard(u32::MAX);
            H0Executor::inject_owner_panic_at(3);
            let spec = BatchSpec::probe_batch();
            let sink = ObserverSink::on(spec.batch_id);
            let mut ex = H0Executor::start(probe_cfg(4, 8, None), spec.clone(), sink.clone());
            ex.admit_all();
            ex.finish_admission();
            let err = ex
                .await_settlement()
                .expect_err("session must fail on owner death");
            assert_eq!(err.item, 3);
            assert!((1..=4).contains(&err.worker));
            // Explicit failure: abort joins every worker, publishes NO
            // aggregate and NO receipt.
            let stats = ex.abort();
            assert_eq!(stats.workers_joined, 4);
            let ledger = sink.ledger().unwrap();
            assert!(
                !ledger
                    .snapshot()
                    .iter()
                    .any(|r| r.kind == crate::ledger::EffectKind::QuiescenceReceipt),
                "no receipt may follow an owner-death abort"
            );
        });
    }

    /// The B-saturated defect schedule: with B=1 the permit held by the
    /// panicking item is the ONLY permit, so a defect path that leaks it
    /// would wedge `admit_all` on `slot_cv` forever — a hang, not the
    /// mandated loud failure. The slot predicate must observe the defect
    /// latch and bail out (review P2: empirically reproduced before the
    /// fix; this control pins it).
    #[test]
    fn owner_panic_with_saturated_b_fails_loudly() {
        let _g = super::session_test_lock();
        with_watchdog(30, move || {
            let _injection = PanicInjectGuard(u32::MAX);
            H0Executor::inject_owner_panic_at(0);
            let spec = BatchSpec::probe_batch();
            let sink = ObserverSink::on(spec.batch_id);
            let err = run_session(probe_cfg(1, 1, None), spec.clone(), &sink)
                .expect_err("session must fail on owner death");
            assert_eq!(err.item, 0);
            // Abort already ran inside run_session: everything joined, no
            // aggregate, no receipt.
            let ledger = sink.ledger().unwrap();
            let snap = ledger.snapshot();
            assert!(
                !snap
                    .iter()
                    .any(|r| r.kind == crate::ledger::EffectKind::QuiescenceReceipt),
                "no receipt may follow an owner-death abort"
            );
            assert!(
                !snap
                    .iter()
                    .any(|r| r.kind == crate::ledger::EffectKind::AggregatePublished),
                "no aggregate may follow an owner-death abort"
            );
        });
    }

    // ---- executable interleaving controls (fence-based) ----

    #[test]
    fn worker_wakeup_survives_notify_before_wait() {
        // The dangerous interleaving: the notification lands while the
        // worker is parked at the fence INSIDE its `while !pred` body —
        // i.e. strictly before the condvar wait. The loop re-check after
        // the fence must observe the item; a single-check (`if`) shape
        // would sleep forever here.
        let _g = super::session_test_lock();
        with_watchdog(30, move || {
            fences::reset();
            fences::install("worker_pre_wait");
            fences::install("admit_pre_cut");
            let spec = BatchSpec::new(1, SEED, 1);
            let sink = ObserverSink::on(spec.batch_id);
            let mut ex = H0Executor::start(probe_cfg(1, 1, None), spec.clone(), sink.clone());
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                ex.admit_all();
                ex.finish_admission();
                ex.await_settlement().expect("settlement");
                let stats = ex.await_quiescence().expect("quiescence");
                let green = oracle::adjudicate(&spec, sink.ledger().unwrap()).green();
                tx.send((stats, green)).expect("send");
            });
            // Deterministic setup: the worker parks BEFORE admission even
            // starts (admission is held at the admit fence), then the
            // producer's enqueue + notify_one land while the worker is
            // parked inside its `while !pred` body — the loop re-check
            // after the fence must observe the item.
            fences::wait_arrivals("worker_pre_wait", 1);
            fences::wait_arrivals("admit_pre_cut", 1);
            fences::open("admit_pre_cut");
            fences::open("worker_pre_wait");
            let (stats, green) = rx.recv().expect("session completes");
            assert_eq!(stats.workers_joined, 1);
            assert!(green);
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
        let _g = super::session_test_lock();
        with_watchdog(30, move || {
            fences::reset();
            fences::install("producer_pre_done");
            fences::install("settle_pre_wait");
            let spec = BatchSpec::new(1, SEED, 2);
            let sink = ObserverSink::off(spec.batch_id);
            let mut ex = H0Executor::start(probe_cfg(1, 1, None), spec.clone(), sink.clone());
            let watch = ex.settlement_watch();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                ex.admit_all();
                ex.finish_admission();
                let stats = ex.await_quiescence().expect("quiescence");
                tx.send(stats).expect("send");
            });
            // The settlement waiter parks while the producer is still
            // holding at the pre-done fence.
            let (wtx, wrx) = mpsc::channel();
            std::thread::spawn(move || {
                let r = watch.wait();
                wtx.send(r).expect("send");
            });
            fences::wait_arrivals("settle_pre_wait", 1);
            // Release the waiter's fence so it proceeds to the REAL
            // condvar wait (predicate still false — the producer has not
            // flipped yet) — then hold the producer at its pre-done fence.
            fences::open("settle_pre_wait");
            fences::wait_arrivals("producer_pre_done", 1);
            // All ACKs are already in (B=1, P=1 forces ack-before-flip);
            // the waiter is now parked INSIDE settle_cv.wait. The flip's
            // settle_cv notify is the only possible wakeup.
            fences::open("producer_pre_done");
            let settled = wrx
                .recv_timeout(Duration::from_secs(10))
                .expect("settlement returns after the flip");
            settled.expect("settlement ok");
            let stats = rx.recv().expect("session completes");
            assert_eq!(stats.workers_joined, 1);
            let receipt = sink.off_snapshot().unwrap();
            assert!(receipt.quiescence_recorded);
            assert_eq!(receipt.items.len(), 2);
        });
    }

    #[test]
    fn cancel_racing_admission_linearization() {
        // Cancel lands while the producer is parked INSIDE the admission
        // critical section (before the cut check, lock released). The
        // re-check must cut the group-7 item: never admitted, no permit
        // consumed, O7 stays green.
        let _g = super::session_test_lock();
        with_watchdog(30, move || {
            fences::reset();
            fences::install("admit_pre_cut");
            let spec = BatchSpec::new(1, SEED, 4); // items 0,3 in group 7
            let sink = ObserverSink::on(spec.batch_id);
            let mut ex = H0Executor::start(probe_cfg(2, 4, None), spec.clone(), sink.clone());
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
                    .filter(|r| r.kind == crate::ledger::EffectKind::ItemAdmitted)
                    .filter_map(|r| r.item_id)
                    .collect();
                tx.send((stats, verdict, admitted)).expect("send");
            });
            // Producer is parked inside the critical section for item 0.
            fences::wait_arrivals("admit_pre_cut", 1);
            // The cancel legally lands in the released-lock window.
            cancel.request_cancel(7);
            fences::open("admit_pre_cut");
            let (stats, verdict, admitted) = rx.recv().expect("session completes");
            // Items 0 and 3 (group 7) were cut at the linearization
            // check: never admitted, no obligations.
            assert_eq!(admitted, vec![1, 2]);
            assert_eq!(stats.admitted_count, 2);
            assert!(verdict.green(), "oracle RED: {:?}", verdict.failed());
        });
    }

    #[test]
    fn producer_done_with_queue_nonempty_still_drains() {
        // Both workers park BEFORE any item exists; the producer admits
        // four items and closes the offer stream while they are parked.
        // Workers must still drain the whole queue and exit cleanly —
        // producer_done never means "queue is empty".
        let _g = super::session_test_lock();
        with_watchdog(6, move || {
            fences::reset();
            fences::install("worker_pre_wait");
            fences::install("admit_pre_cut");
            let spec = BatchSpec::new(1, SEED, 4);
            let sink = ObserverSink::on(spec.batch_id);
            let mut ex = H0Executor::start(probe_cfg(2, 4, None), spec.clone(), sink.clone());
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                ex.admit_all();
                ex.finish_admission();
                ex.await_settlement().expect("settlement");
                let stats = ex.await_quiescence().expect("quiescence");
                let green = oracle::adjudicate(&spec, sink.ledger().unwrap()).green();
                tx.send((stats, green)).expect("send");
            });
            fences::wait_arrivals("worker_pre_wait", 2);
            fences::wait_arrivals("admit_pre_cut", 1);
            // Admission proceeds while both workers are parked; the
            // notifications land before their condvar waits — the loop
            // re-checks must observe the queue and drain it fully.
            fences::open("admit_pre_cut");
            fences::open("worker_pre_wait");
            let (stats, green) = rx.recv().expect("session completes");
            assert_eq!(stats.admitted_count, 4);
            assert_eq!(stats.workers_joined, 2);
            assert!(green);
        });
    }
}
