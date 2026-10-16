//! DG-1 class α ownership-handoff pool — design-candidate protocol
//! verification (PR #284, R7 review directive: ONE complete small
//! verification covering initialization, concurrent handoff, full pool,
//! cross-wrap, slot exclusivity and accounting, BEFORE the doc sync).
//!
//! TEST-ONLY executable evidence for
//! `docs/architecture/error-system-target-contract.md` §DG-1: the pool
//! type exists only under cfg(test) and is NOT production code — the
//! production landing keeps its own gates.
//!
//! Protocol under verification (R7 arithmetic; the R6 skip-0 cycle is
//! withdrawn — see `r6_skip0_rule_withdrawn_counterexample`):
//!   - generations: plain u32 counters wrapping NATURALLY through 0;
//!     empty is counter equality (pub == cons), no sentinel value;
//!   - slot = generation % 2 (parity alternates unbroken through the
//!     wrap: …, u32::MAX-1 even, u32::MAX odd, 0 even, 1 odd, …);
//!   - admission: publish accepted iff `pub.wrapping_sub(cons) <= 1`;
//!     `== 2` is the ONLY loss site (drop + count, before any write);
//!   - both handoff directions are Release/Acquire pairs (publish:
//!     payload -> `pub_gen.store(n, Release)` -> drainer
//!     `pub_gen.load(Acquire)` -> copy; recycle: copy ->
//!     `cons_gen.store(c, Release)` -> producer `cons_gen.load(Acquire)`
//!     -> overwrite);
//!   - consumption walks `last_consumed+1 ..= snapshot(pub)` in order,
//!     no skips.
//!
//! Authoring record (TEST-QUALITY-STANDARD-v1 six questions):
//!   1. Authority-owned property: target-contract §DG-1 acceptance gates
//!      1-3 (no data race, snapshot integrity, loss accounting); gate 4
//!      (producer cost) is code inspection of `publish`.
//!   2. Mechanism x risk: new concurrency protocol design (high) ->
//!      layered evidence: deterministic arithmetic property tests
//!      (native + loom partitions), loom schedule exploration (loom
//!      partition, picked up by the playback-concurrency lane), and a
//!      real-thread Miri race check (local evidence; not CI-wired).
//!   3. Precondition seams: arithmetic tests construct exact counter
//!      values including the u32::MAX boundary (`with_counters`);
//!      loom owns every schedule of the two threads.
//!   4. Oracle per test: named in each test's doc comment.
//!   5. Settlement boundary: arithmetic asserts at call return; loom
//!      and real-thread suites assert after both threads join.
//!   6. Defects that must turn an oracle red: the R6 skip-0 rule
//!      (same-slot overwrite + distance-3 admission miss at the wrap),
//!      a Relaxed recycle load (Miri race — negative control run in an
//!      isolated worktree), out-of-order/skip/duplicate consumption,
//!      accounting drift.

#[cfg(loom)]
use loom::sync::atomic::{AtomicU32, Ordering};
#[cfg(not(loom))]
use std::sync::atomic::{AtomicU32, Ordering};

use std::cell::UnsafeCell;

/// One fixed-capacity record: plain data, owned by exactly one party at a
/// time under the protocol (the safety claim under verification).
pub(crate) struct Record {
    pub generation: u32,
    pub payload: [u8; 8],
}

/// The two-record ownership-handoff pool (design candidate, test-only).
pub(crate) struct ErrorPool {
    pub_gen: AtomicU32,
    cons_gen: AtomicU32,
    records: UnsafeCell<[Record; 2]>,
}

// SAFETY: the safety argument IS the ownership protocol under
// verification — a record slot is written only while generation-
// unreachable by the drainer (admission + recycle Acquire), and read
// only while published. Miri (evidence X15) checks race-freedom; loom
// checks the schedule-level protocol; neither is assumed by the other.
unsafe impl Sync for ErrorPool {}

impl ErrorPool {
    pub(crate) fn new() -> Self {
        Self::with_counters(0, 0)
    }

    /// Test constructor for exact-counter arithmetic (cross-wrap cases).
    #[cfg_attr(loom, allow(dead_code))]
    pub(crate) fn with_counters(pub_gen: u32, cons_gen: u32) -> Self {
        Self {
            pub_gen: AtomicU32::new(pub_gen),
            cons_gen: AtomicU32::new(cons_gen),
            records: UnsafeCell::new([
                Record {
                    generation: 0,
                    payload: [0; 8],
                },
                Record {
                    generation: 0,
                    payload: [0; 8],
                },
            ]),
        }
    }

    /// Fault-path publish: bounded, single attempt — no loop, no CAS, no
    /// lock, no allocation. Returns `false` on a full pool (the caller
    /// drops + counts; this is the protocol's only loss site).
    pub(crate) fn publish(&self, payload: u64) -> bool {
        let p = self.pub_gen.load(Ordering::Acquire);
        if p.wrapping_sub(self.cons_gen.load(Ordering::Acquire)) == 2 {
            return false;
        }
        let n = p.wrapping_add(1);
        // SAFETY: admission above guarantees the cyclic distance
        // cons_gen >= n-2, i.e. the drainer has Release-published its
        // finished copy of generation n-2 (same slot) and this Acquire
        // load synchronized with it — the slot is producer-owned here.
        unsafe {
            let slot = &mut (*self.records.get())[(n % 2) as usize];
            slot.generation = n;
            slot.payload = payload.to_le_bytes();
        }
        self.pub_gen.store(n, Ordering::Release);
        true
    }

    /// Drainer walk: copies every generation in `(last, snapshot]` in
    /// order (no skips), then releases them all with ONE recycle store.
    pub(crate) fn drain<F: FnMut(u32, u64)>(&self, last: &mut u32, sink: &mut F) {
        let g = self.pub_gen.load(Ordering::Acquire);
        // Admission oracle: the true distance is capped at 2 by the
        // producer's own check; anything larger is itself a violation.
        assert!(
            g.wrapping_sub(*last) <= 2,
            "admission invariant violated: distance {}",
            g.wrapping_sub(*last)
        );
        let mut c = *last;
        while c != g {
            c = c.wrapping_add(1);
            // SAFETY: a published record; the producer cannot touch slot
            // c%2 again until it Acquire-reads cons_gen >= c, which this
            // walk stores only after the copy below completes.
            let rec = unsafe { &(*self.records.get())[(c % 2) as usize] };
            sink(rec.generation, u64::from_le_bytes(rec.payload));
        }
        if c != *last {
            *last = c;
            self.cons_gen.store(c, Ordering::Release);
        }
    }

    /// Outstanding (published-unconsumed + drainer-held) distance.
    pub(crate) fn outstanding(&self) -> u32 {
        self.pub_gen
            .load(Ordering::SeqCst)
            .wrapping_sub(self.cons_gen.load(Ordering::SeqCst))
    }
}

/// Oracle: empty-pool acceptance (the R5 failure), slot alternation via
/// in-order markers, full-pool rejection as the only loss site,
/// rejection leaving pub_gen unchanged, and reuse after consumption.
#[cfg(not(loom))]
#[test]
fn arithmetic_first_publish_two_consecutive_full_rejection_reuse() {
    let pool = ErrorPool::new();
    assert!(
        pool.publish(1),
        "first fault on an empty pool must be accepted"
    );
    assert_eq!(pool.outstanding(), 1);
    assert!(pool.publish(2), "second publish (d=1) accepted, other slot");
    assert_eq!(pool.outstanding(), 2);
    assert!(
        !pool.publish(3),
        "third publish (d=2) rejected — the only loss site"
    );
    assert_eq!(pool.outstanding(), 2, "rejection must not advance pub_gen");
    let mut last = 0u32;
    let mut copied = Vec::new();
    pool.drain(&mut last, &mut |generation, payload| {
        copied.push((generation, payload))
    });
    assert_eq!(last, 2);
    assert_eq!(
        copied,
        vec![(1, 1), (2, 2)],
        "in-order, both, markers intact"
    );
    assert_eq!(pool.outstanding(), 0);
    assert!(pool.publish(3), "reuse after consumption (d=0)");
}

/// Oracle: the R7 reviewer counterexample territory — publishing through
/// the u32::MAX -> 0 wrap must alternate slots and keep admission exact;
/// the walk crosses the wrap in order with markers intact.
#[cfg(not(loom))]
#[test]
fn arithmetic_cross_wrap_slot_alternation_and_admission() {
    let m = u32::MAX;
    let pool = ErrorPool::with_counters(m - 1, m - 1); // drainer paused
    assert_eq!(m % 2, 1, "u32::MAX is odd (slot 1)");
    assert!(pool.publish(1), "d=0 at the boundary accepts");
    // slots used: u32::MAX (odd -> 1), then 0 (even -> 0): alternation.
    assert!(pool.publish(2), "d=1 accepts");
    assert!(!pool.publish(3), "d=2 at the boundary rejects");
    let mut last = m - 1;
    let mut copied = Vec::new();
    pool.drain(&mut last, &mut |generation, payload| {
        copied.push((generation, payload))
    });
    assert_eq!(
        copied,
        vec![(m, 1), (0, 2)],
        "wrap-crossing walk in order, markers intact"
    );
    assert_eq!(last, 0);
    assert_eq!(pool.outstanding(), 0);
}

/// The R6 skip-0 cycle, executed with real u32 values at the reviewer's
/// counterexample: it maps u32::MAX and 1 to the SAME slot (both odd) and
/// lets distance 3 past the `== 2` full check. Kept as the killed-rule
/// record; the R7 natural-wrap rule replaces it.
#[cfg(not(loom))]
#[test]
fn r6_skip0_rule_withdrawn_counterexample() {
    let m = u32::MAX;
    let r6_next = |p: u32| if p == m { 1 } else { p + 1 };
    assert_eq!(
        r6_next(m) % 2,
        m % 2,
        "R6 maps consecutive generations to one slot (same-slot overwrite)"
    );
    let pub_after_two = 1u32; // generations u32::MAX and 1 published, cons = m-1
    assert_eq!(
        pub_after_two.wrapping_sub(m - 1),
        3,
        "distance 3 — the == 2 full check is blind on the R6 cycle"
    );
}

/// Oracle: pool-boundary accounting — `attempted = accepted + rejected`
/// and `accepted = copied_out + outstanding` (the R6 identity
/// `written = delivered + drops` is withdrawn: a fully drained pool with
/// 2 written+consumed and 1 rejected would read `2 = 2 + 1`).
#[cfg(not(loom))]
#[test]
fn arithmetic_pool_boundary_accounting() {
    let pool = ErrorPool::new();
    let (mut attempted, mut accepted, mut rejected) = (0u32, 0u32, 0u32);
    for i in 1..=3u64 {
        attempted += 1;
        if pool.publish(i) {
            accepted += 1;
        } else {
            rejected += 1;
        }
    }
    assert_eq!(attempted, accepted + rejected, "admission boundary");
    let mut last = 0u32;
    let mut copied = 0u32;
    pool.drain(&mut last, &mut |generation, payload| {
        assert_eq!(payload, u64::from(generation), "marker");
        copied += 1;
    });
    let outstanding = pool.outstanding();
    assert!(outstanding <= 2);
    assert_eq!(accepted, copied + outstanding, "drain boundary");
}

#[cfg(loom)]
mod loom_suite {
    use super::*;
    use loom::sync::Arc;
    use loom::thread;

    /// Oracle: under every explored schedule — exclusivity (each copied
    /// record whole: payload == generation marker), order (copied
    /// generations are 1..=len — no skip, no duplicate), admission
    /// (outstanding <= 2), and both accounting identities at
    /// quiescence. A SCHEDULE-CLEAN run is within these 2-thread /
    /// 3-attempt bounds — not a general proof.
    #[test]
    fn handoff_exclusive_in_order_accounted_under_all_schedules() {
        loom::model(|| {
            let pool = Arc::new(ErrorPool::new());
            let producer_pool = Arc::clone(&pool);
            let producer = thread::spawn(move || {
                let (mut attempted, mut accepted, mut rejected) = (0u32, 0u32, 0u32);
                for i in 1..=3u64 {
                    attempted += 1;
                    if producer_pool.publish(i) {
                        accepted += 1;
                    } else {
                        rejected += 1;
                    }
                }
                (attempted, accepted, rejected)
            });
            let drainer_pool = Arc::clone(&pool);
            let drainer = thread::spawn(move || {
                let mut last = 0u32;
                let mut copied: Vec<(u32, u64)> = Vec::new();
                for _ in 0..2 {
                    drainer_pool.drain(&mut last, &mut |generation, payload| {
                        copied.push((generation, payload))
                    });
                }
                copied
            });
            let (attempted, accepted, rejected) = producer.join().unwrap();
            let copied = drainer.join().unwrap();
            assert_eq!(attempted, accepted + rejected, "admission boundary");
            for (idx, (generation, payload)) in copied.iter().enumerate() {
                assert_eq!(*generation, idx as u32 + 1, "skip/dup/out-of-order");
                assert_eq!(*payload, u64::from(*generation), "torn or mixed record");
            }
            let outstanding = pool.outstanding();
            assert!(outstanding <= 2);
            assert_eq!(
                accepted,
                copied.len() as u32 + outstanding,
                "drain boundary"
            );
        });
    }
}

/// Real-thread exercise (not loom): under Miri this checks the
/// two-direction handoff for actual data races on the `UnsafeCell`
/// payload (Miri flags conflicting accesses with no happens-before even
/// when they do not overlap in wall-clock time, so the Relaxed-recycle
/// defect is caught deterministically — negative control X15.b). Under
/// native cargo test it is a smoke pass: the assertions must hold under
/// ANY schedule, but only one is executed.
#[cfg(not(loom))]
#[test]
fn real_thread_handoff_race_check() {
    use std::sync::Arc;
    let pool = Arc::new(ErrorPool::new());
    let producer_pool = Arc::clone(&pool);
    // yield-driven interleaving: the producer RETRIES past rejections (a
    // rejected attempt is re-attempted after a yield once the drainer
    // advances cons_gen), so recycle-direction pairs (drainer read slot s
    // for generation k, producer later writes slot s for generation k+2)
    // actually occur — the window a Relaxed recycle load leaves racy
    // (negative control X15.b needs many-seed Miri to hit it).
    let producer = std::thread::spawn(move || {
        let (mut attempted, mut accepted, mut rejected) = (0u32, 0u32, 0u32);
        let mut next = 1u64;
        while accepted < 6 {
            attempted += 1;
            if producer_pool.publish(next) {
                accepted += 1;
                next += 1;
            } else {
                rejected += 1;
            }
            std::thread::yield_now();
        }
        (attempted, accepted, rejected)
    });
    let drainer_pool = Arc::clone(&pool);
    let drainer = std::thread::spawn(move || {
        let mut last = 0u32;
        let mut copied: Vec<(u32, u64)> = Vec::new();
        while last < 6 {
            drainer_pool.drain(&mut last, &mut |generation, payload| {
                copied.push((generation, payload))
            });
            std::thread::yield_now();
        }
        copied
    });
    let (attempted, accepted, rejected) = producer.join().unwrap();
    let copied = drainer.join().unwrap();
    assert_eq!(attempted, accepted + rejected);
    for (idx, (generation, payload)) in copied.iter().enumerate() {
        assert_eq!(*generation, idx as u32 + 1);
        assert_eq!(*payload, u64::from(*generation));
    }
    assert!(pool.outstanding() <= 2);
    assert_eq!(accepted, copied.len() as u32 + pool.outstanding());
}
