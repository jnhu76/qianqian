//! Mechanism-neutral session core — the A_NARROW shared business layer
//! (Gate II, issue #308; Owner GO `SHARED_CODE = A_NARROW`, #308 comment
//! 6330781617).
//!
//! This module holds ONLY what H0 and H1 genuinely share because it is
//! mechanism-neutral business semantics:
//! - [`execute_item`]: the frozen item work — synthetic wait, input
//!   generation, CPU transform, content digest, contribution, fault lane,
//!   and the business effect publication. ONE function, called by both
//!   executors' real dispatch paths (H0's condvar worker loop and H1's
//!   channel worker loop), so the H0/H1 comparison stays about the
//!   execution+communication mechanism, never about two business
//!   algorithms.
//! - [`SessionError`] / [`SessionStats`]: the shared defect vocabulary and
//!   the mode-independent lifecycle statistics face.
//! - `cfg(test)`-only facilities shared by BOTH mechanisms' negative
//!   controls: the owner-panic injection point (read inside
//!   `execute_item`, so an injected owner death behaves identically on
//!   either dispatch mechanism) and the fence gates (named interlocks
//!   that force documented interleavings; gate NAMES stay per mechanism).
//!
//! It deliberately does NOT contain any scheduling mechanism state: no
//! queue, no channel, no condvar, no worker loop, no admission logic, no
//! terminal-release logic. Those are owned per mechanism by `h0.rs`
//! (VecDeque + Condvar) and `h1.rs` (bounded MPMC channel). The
//! `ref_result` shortcut is never called here (SPEC §2.5(b)) — REF
//! belongs to the oracle side only.
//!
//! NOT a benchmark harness: no timing anywhere (Gate II is non-timed
//! correctness).

use std::time::Duration;

use crate::ledger::TerminalKind;
use crate::observer::ObserverSink;
use crate::oracle::fold_meta;
use crate::workload::{self, BatchSpec};

/// Why a session failed. Published by real owner-death handling
/// (`worker`/`item` name the dead owner) or — under the
/// `worker: 0 / item: u32::MAX` sentinel — by an executor-level
/// precondition/cleanliness failure (unjoined worker, outstanding
/// settlement obligations, offer stream still open, channel-invariant
/// violation). Never by forging a terminal or a receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionError {
    pub worker: u64,
    pub item: u32,
    pub message: String,
}

/// Mode-independent lifecycle statistics (same values in ON and OFF).
/// Channel-specific accounting (Q high water) is reported by the H1
/// executor's own stats face on top of this shared base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionStats {
    pub admitted_count: usize,
    pub b_high_water: usize,
    pub active_execution_high_water: usize,
    pub workers_spawned: usize,
    pub workers_joined: usize,
}

/// The REAL item work, shared verbatim by H0 and H1: wait + input
/// generation + CPU transform + content digest + contribution. Never
/// `ref_result` (SPEC §2.5(b)); the fault lane performs all common work
/// and then publishes CompletedErr(0x31).
pub(crate) fn execute_item(
    worker: u64,
    spec: &BatchSpec,
    sink: &ObserverSink,
    item_id: u32,
) -> (TerminalKind, u64) {
    #[cfg(test)]
    {
        if test_panic_item() == item_id {
            panic!("session-core test-injected owner panic for item {item_id}");
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

// ---- cfg(test)-only facilities shared by both mechanisms ----

#[cfg(test)]
static TEST_PANIC_ITEM: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);

/// Serializes every session test (h0 + h1 + k0_session): the fence gates
/// and TEST_PANIC_ITEM are process-global, so concurrent sessions would
/// observe each other's injections.
#[cfg(test)]
pub(crate) fn session_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Fixture hygiene: a control that hung inside its watchdog thread
    // (mutation arms) never drops its PanicInjectGuard, so the
    // process-global injection would cascade-fail every later session
    // test. Acquiring the fixture lock resets it.
    set_test_panic_item(u32::MAX);
    guard
}

#[cfg(test)]
pub(crate) fn set_test_panic_item(item: u32) {
    TEST_PANIC_ITEM.store(item, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
fn test_panic_item() -> u32 {
    TEST_PANIC_ITEM.load(std::sync::atomic::Ordering::Relaxed)
}

/// Named fence gates, `cfg(test)` only. A test installs a gate; an
/// executor thread reaching a matching fence point (h0's `h0_fence_park!`
/// or h1's `h1_fence_park!`) registers its arrival and blocks —
/// TEMPORARILY RELEASING the state lock it holds — until the test opens
/// the gate. This forces the documented interleavings deterministically,
/// never with sleeps, and leaves the fenced critical region observable to
/// the other threads exactly as a legal interleaving would. Gates are
/// process-global but session tests serialize on [`session_test_lock`],
/// so H0 and H1 controls never observe each other's gates.
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
