//! The class-α ownership-handoff pool: the production realization of the
//! protocol verified by `error-system-target-contract.md` §DG-1 and by the
//! `qianqian-playback` design-candidate model (PR #284).
//!
//! The protocol assumes exactly one producer and exactly one consumer per pool,
//! and this module's types — not documentation — keep that assumption true:
//!
//! * The core is **crate-private** and its methods are crate-private. No code
//!   outside this crate can name it, hold it, or call `publish`/`drain`.
//! * ONE producer: the only producer entry point is the registering thread's
//!   thread-local pointer, owned by [`crate::AlphaRegistration`] — a
//!   thread-bound, non-shareable, non-clonable handle. There is no way to build
//!   a second producer handle for a pool from safe code, and no way to send a
//!   producer handle to another thread. (The `unsafe impl Sync` below exists
//!   for the drainer's cross-thread read; it is not a public contract, because
//!   the type it applies to is not public.)
//! * ONE consumer: the drainer's walk is reached only from the facility's
//!   single drainer thread, and the walk position lives in the core rather than
//!   in a caller-supplied cursor, so a second consumer would have to be a
//!   second drainer — which the facility does not have.
//!
//! One pool per REGISTERED class-α producer (per critical thread or critical
//! site). Registration happens on the cold path; a fault-path publish never
//! locks, never allocates, never loops and never CAS-es:
//!
//! ```text
//! pub_gen : AtomicU32   // last PUBLISHED generation; empty is pub == cons
//! cons_gen: AtomicU32   // last generation the drainer FINISHED copying
//! records[2]: Record    // plain data, owned by exactly one party at a time
//! ```
//!
//! Generation numbering is the R7 natural-wrap rule: plain `u32` counters
//! wrapping through 0, empty is COUNTER EQUALITY, slot = generation % 2, and
//! admission accepts only `pub.wrapping_sub(cons) <= 1`, so every modular
//! distance the protocol uses is bounded by 2 and no ABA is possible. Both
//! handoff directions are Release/Acquire pairs — publish (`payload →
//! pub_gen.store(Release) →` drainer load Acquire → copy) and recycle
//! (`copy → cons_gen.store(Release) →` producer load Acquire → overwrite).
//! A Relaxed load on either consumer side removes the happens-before edge and
//! is a data race (negative control below).
//!
//! Pool states (the unit the acceptance gates verify): `d = pub - cons`
//!
//! | d | state | publish | consume |
//! |---|---|---|---|
//! | 0 | empty (nothing in flight) | accepted — the first fault on an empty pool IS accepted | nothing new |
//! | 1 | one published-unconsumed | accepted, writes the OTHER slot | copies 1 generation, releases it |
//! | 2 | full: one published-unconsumed + one drainer-held | DROPPED + counted (the only loss site) | copies 2 generations in order, releases both |
//!
//! Loss accounting at the pool boundary:
//!
//! ```text
//! attempted = accepted + rejected        (admission boundary)
//! accepted  = copied_out + outstanding   (drain boundary)
//! ```
//!
//! "Copied out of the pool" and "delivered to a sink" are distinct events and
//! are accounted separately by the drainer (`exported` + `export_failed` +
//! `abandoned_at_exit`).
//!
//! Retirement: a registration's `Drop` closes the thread's fast path and then
//! Release-publishes `retired`. The core stays alive (the registry holds a
//! reference) until the drainer has walked the residual records, folded the
//! counters into the facility totals and released it — the producer-exit half
//! of the ownership handoff.

#[cfg(loom)]
use loom::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
#[cfg(not(loom))]
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use std::cell::UnsafeCell;

use crate::Record;

/// The two-record ownership-handoff pool.
///
/// Crate-private on purpose: see the module docs. The producer reaches it
/// through [`crate::AlphaRegistration`]'s thread-local pointer, the consumer
/// through the facility's drainer loop.
pub(crate) struct PoolCore {
    /// Last published generation. Meaningless to read alone — always
    /// compared against `cons_gen`.
    pub_gen: AtomicU32,
    cons_gen: AtomicU32,
    records: UnsafeCell<[Record; 2]>,
    /// Admission-boundary counters (loss accounting, gate 3).
    attempted: AtomicU64,
    accepted: AtomicU64,
    rejected: AtomicU64,
    /// Drain-boundary counter: generations copied out of the pool.
    copied_out: AtomicU64,
    /// The DG-4b one-shot latch (see `publish_panic_once`). Deliberately
    /// OUTSIDE the protocol above: it neither reads nor advances a generation,
    /// so it cannot affect the admission/accounting arithmetic.
    panic_published: AtomicBool,
    /// The drainer's walk position. Owned by the facility's single consumer:
    /// only the drainer thread loads or stores it, so a plain `Relaxed`
    /// ordering is all it needs (the Release that frees a slot for the
    /// producer is the `cons_gen` store below).
    drain_cursor: AtomicU32,
    /// Set (Release) by the producer registration's `Drop` after the thread's
    /// fast path is closed; read (Acquire) by the drainer, which then walks
    /// the residual records before releasing the core. Never read by the
    /// producer.
    retired: AtomicBool,
}

// SAFETY: the safety argument IS the ownership protocol, established by the
// acceptance gates rather than asserted here: a record slot is written only
// while it is generation-unreachable by the drainer (admission check +
// Acquire recycle load), and read only while published (Release publish +
// Acquire snapshot). Gate 1 (no data race) is verified with Loom over the
// interleaving model and with Miri over the real-thread reuse window;
// gates 2-4 cover snapshot integrity, loss accounting and producer cost.
// The Sync-ness is granted to the crate's own two roles (one producer, one
// drainer) — the core itself stays private, so it is not a public promise.
unsafe impl Sync for PoolCore {}

impl Default for PoolCore {
    fn default() -> Self {
        Self::new()
    }
}

impl PoolCore {
    pub(crate) fn new() -> Self {
        Self::with_counters(0, 0)
    }

    /// Exact-counter constructor (cross-wrap arithmetic and tests).
    pub(crate) fn with_counters(pub_gen: u32, cons_gen: u32) -> Self {
        const EMPTY: Record = Record::empty();
        Self {
            pub_gen: AtomicU32::new(pub_gen),
            cons_gen: AtomicU32::new(cons_gen),
            records: UnsafeCell::new([EMPTY, EMPTY]),
            attempted: AtomicU64::new(0),
            accepted: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            copied_out: AtomicU64::new(0),
            panic_published: AtomicBool::new(false),
            drain_cursor: AtomicU32::new(cons_gen),
            retired: AtomicBool::new(false),
        }
    }

    /// Fault-path publish: bounded single attempt — no loop, no CAS, no lock,
    /// no allocation. Returns `false` on a full pool; the caller drops and
    /// counts (this is the protocol's ONLY loss site, and it happens BEFORE
    /// any write, so a rejected attempt never enters `accepted`).
    pub(crate) fn publish(&self, record: Record) -> bool {
        self.attempted.fetch_add(1, Ordering::Relaxed);
        let p = self.pub_gen.load(Ordering::Acquire);
        if p.wrapping_sub(self.cons_gen.load(Ordering::Acquire)) == 2 {
            self.rejected.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let n = p.wrapping_add(1);
        // SAFETY: the admission check above guarantees the cyclic distance
        // `cons_gen >= n - 2`: the drainer has Release-published its finished
        // copy of generation `n - 2` (same slot) and this Acquire load
        // synchronized with it, so the slot is producer-owned here.
        unsafe {
            let slot = &mut (*self.records.get())[(n % 2) as usize];
            *slot = record;
        }
        self.pub_gen.store(n, Ordering::Release);
        self.accepted.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// One-shot publish for the DG-4b panic path. A panic hook may fire more
    /// than once on the same thread (a caught panic, then another); the FIRST
    /// fire publishes, later ones return `false` and are counted by the hook
    /// instead of occupying the second slot. Atomics only — one bounded RMW,
    /// no loop, no lock, no allocation.
    ///
    /// A one-shot suppression is NOT a pool loss: it never touches `attempted`,
    /// `accepted` or `rejected`, so the admission and drain identities are
    /// unaffected by repeated panics. It also does not enter `accepted`, so a
    /// suppressed record cannot appear in `copied_out + outstanding`.
    pub(crate) fn publish_panic_once(&self, record: Record) -> bool {
        if self.panic_published.swap(true, Ordering::AcqRel) {
            return false;
        }
        self.publish(record)
    }

    /// The drainer's walk: copies every generation in `(cursor, snapshot(pub)]`
    /// in order with no skips (at most 2), then releases them all with ONE
    /// recycle store. Single-consumer by construction: the cursor lives here
    /// and only the facility's drainer thread ever calls this.
    pub(crate) fn drain(&self, sink: &mut impl FnMut(&Record)) {
        let g = self.pub_gen.load(Ordering::Acquire);
        let mut last = self.drain_cursor.load(Ordering::Relaxed);
        // Admission oracle: the true distance is capped at 2 by the
        // producer's own check, so a larger distance is itself a violation.
        debug_assert!(
            g.wrapping_sub(last) <= 2,
            "admission invariant violated: distance {}",
            g.wrapping_sub(last)
        );
        let mut c = last;
        while c != g {
            c = c.wrapping_add(1);
            // SAFETY: a published record. The producer cannot touch slot
            // `c % 2` again until it Acquire-reads `cons_gen >= c`, which
            // this walk stores only after every copy below has completed.
            let record = unsafe { &(*self.records.get())[(c % 2) as usize] };
            sink(record);
        }
        if c != last {
            let released = c.wrapping_sub(last);
            last = c;
            self.drain_cursor.store(last, Ordering::Relaxed);
            self.copied_out
                .fetch_add(u64::from(released), Ordering::Relaxed);
            self.cons_gen.store(c, Ordering::Release);
        }
    }

    /// Published-unconsumed + drainer-held distance (0..=2).
    pub(crate) fn outstanding(&self) -> u32 {
        self.pub_gen
            .load(Ordering::SeqCst)
            .wrapping_sub(self.cons_gen.load(Ordering::SeqCst))
    }

    /// The producer's registration left. Release: every publish that happened
    /// before this store is visible to the drainer that Acquire-loads `true`.
    pub(crate) fn retire(&self) {
        self.retired.store(true, Ordering::Release);
    }

    /// Whether the producer's registration already left. Acquire: pairs with
    /// [`Self::retire`], so a `true` read also means every publish before the
    /// retirement is now visible (the residual walk therefore cannot race with
    /// a producer — there are none left — and cannot miss a record).
    pub(crate) fn is_retired(&self) -> bool {
        self.retired.load(Ordering::Acquire)
    }

    pub(crate) fn attempted(&self) -> u64 {
        self.attempted.load(Ordering::Relaxed)
    }

    pub(crate) fn accepted(&self) -> u64 {
        self.accepted.load(Ordering::Relaxed)
    }

    pub(crate) fn rejected(&self) -> u64 {
        self.rejected.load(Ordering::Relaxed)
    }

    pub(crate) fn copied_out(&self) -> u64 {
        self.copied_out.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Level, Text};

    fn record(marker: u64) -> Record {
        let mut message = Text::empty();
        message.push_i64(marker as i64);
        Record {
            site: "pool-test",
            level: Level::Error,
            message,
            status: Some(marker as i32),
            native_code: None,
            ts_micros: marker,
        }
    }

    fn marker(record: &Record) -> u64 {
        record.message.render().parse::<u64>().expect("marker")
    }

    /// Oracle (gate 3 + state machine): empty-pool acceptance, slot
    /// alternation witnessed by in-order markers, full-pool rejection as the
    /// ONLY loss site, rejection leaving `pub_gen` unchanged, and reuse after
    /// consumption.
    #[cfg(not(loom))]
    #[test]
    fn arithmetic_first_publish_two_consecutive_full_rejection_reuse() {
        let pool = PoolCore::new();
        assert!(pool.publish(record(1)), "first fault on an empty pool");
        assert_eq!(pool.outstanding(), 1);
        assert!(pool.publish(record(2)), "second publish (d=1), other slot");
        assert_eq!(pool.outstanding(), 2);
        assert!(
            !pool.publish(record(3)),
            "third publish (d=2) is the loss site"
        );
        assert_eq!(pool.outstanding(), 2, "rejection must not advance pub_gen");
        let mut copied = Vec::new();
        pool.drain(&mut |record| copied.push(marker(record)));
        assert_eq!(copied, vec![1, 2], "in order, both, markers intact");
        assert_eq!(pool.outstanding(), 0);
        assert!(pool.publish(record(3)), "reuse after consumption (d=0)");
        assert_eq!(pool.attempted(), 4);
        assert_eq!(pool.accepted(), 3);
        assert_eq!(pool.rejected(), 1);
        assert_eq!(pool.accepted() + pool.rejected(), pool.attempted());
    }

    /// Oracle: publishing through the `u32::MAX → 0` wrap alternates slots and
    /// keeps admission exact; the walk crosses the wrap in order. This is the
    /// territory of the R6 skip-0 defect (withdrawn rule).
    #[cfg(not(loom))]
    #[test]
    fn arithmetic_cross_wrap_slot_alternation_and_admission() {
        let m = u32::MAX;
        let pool = PoolCore::with_counters(m - 1, m - 1);
        assert_eq!(m % 2, 1, "u32::MAX is odd (slot 1)");
        assert!(pool.publish(record(1)), "d=0 at the boundary accepts");
        assert!(pool.publish(record(2)), "d=1 accepts");
        assert!(!pool.publish(record(3)), "d=2 at the boundary rejects");
        let mut copied = Vec::new();
        pool.drain(&mut |record| copied.push(marker(record)));
        assert_eq!(copied, vec![1, 2], "wrap-crossing walk in order");
        assert_eq!(pool.outstanding(), 0);
    }

    /// The R6 skip-0 cycle, executed with the reviewer's counterexample
    /// values: it maps `u32::MAX` and `1` to the SAME slot and lets distance
    /// 3 past the `== 2` check. Kept as the killed-rule record.
    #[cfg(not(loom))]
    #[test]
    fn r6_skip0_rule_withdrawn_counterexample() {
        let m = u32::MAX;
        let r6_next = |p: u32| if p == m { 1 } else { p + 1 };
        assert_eq!(
            r6_next(m) % 2,
            m % 2,
            "same slot for consecutive generations"
        );
        let pub_after_two = 1u32;
        assert_eq!(
            pub_after_two.wrapping_sub(m - 1),
            3,
            "distance 3 escapes the check"
        );
    }

    /// Oracle (gate 3): consuming walks `cursor+1 ..= pub` in order with no
    /// skips, and the pool-boundary identities hold at quiescence.
    #[cfg(not(loom))]
    #[test]
    fn arithmetic_pool_boundary_accounting_and_no_skip_consumption() {
        let pool = PoolCore::new();
        assert!(pool.publish(record(7)));
        assert!(pool.publish(record(8)));
        let mut copied = Vec::new();
        pool.drain(&mut |record| copied.push(marker(record)));
        assert!(pool.publish(record(9)));
        pool.drain(&mut |record| copied.push(marker(record)));
        assert_eq!(
            copied,
            vec![7, 8, 9],
            "no skip, no duplicate, no reorder across walks"
        );
        assert_eq!(
            pool.accepted(),
            pool.copied_out() + u64::from(pool.outstanding())
        );
        assert_eq!(pool.accepted() + pool.rejected(), pool.attempted());
    }

    /// Retirement: the producer's exit is published with Release and the
    /// residual records stay walkable afterwards — the producer-exit half of
    /// the ownership handoff, which the registry's reaping relies on.
    #[cfg(not(loom))]
    #[test]
    fn a_retired_pool_still_yields_its_residual_records() {
        let pool = PoolCore::new();
        assert!(!pool.is_retired(), "a live producer is not retired");
        assert!(pool.publish(record(1)));
        assert!(pool.publish(record(2)));
        pool.retire();
        assert!(pool.is_retired());
        let mut copied = Vec::new();
        pool.drain(&mut |record| copied.push(marker(record)));
        assert_eq!(copied, vec![1, 2], "the residual walk loses nothing");
        assert_eq!(pool.outstanding(), 0);
        assert_eq!(pool.copied_out(), 2);
    }

    /// Real-thread exercise of the two-direction handoff. Under native
    /// `cargo test` this is a single-schedule smoke pass; under Miri it checks
    /// the `UnsafeCell` payload for actual data races on the recycle window
    /// (a Relaxed recycle load is caught here — negative control).
    #[cfg(not(loom))]
    #[test]
    fn real_thread_handoff_recycle_window_is_race_free() {
        use std::sync::Arc;
        let pool = Arc::new(PoolCore::new());
        let producer_pool = Arc::clone(&pool);
        let producer = std::thread::spawn(move || {
            let mut next = 1u64;
            let mut accepted = 0u64;
            while accepted < 6 {
                if producer_pool.publish(record(next)) {
                    accepted += 1;
                    next += 1;
                }
                std::thread::yield_now();
            }
            accepted
        });
        let drainer_pool = Arc::clone(&pool);
        let drainer = std::thread::spawn(move || {
            let mut copied = Vec::new();
            while copied.len() < 6 {
                drainer_pool.drain(&mut |record| copied.push(marker(record)));
                std::thread::yield_now();
            }
            copied
        });
        let accepted = producer.join().unwrap();
        let copied = drainer.join().unwrap();
        assert_eq!(accepted, 6);
        assert_eq!(copied, (1..=6).collect::<Vec<_>>());
        assert!(pool.outstanding() <= 2);
        assert_eq!(
            pool.accepted(),
            pool.copied_out() + u64::from(pool.outstanding())
        );
    }

    /// Loom (gate 1): every explored schedule of one producer and one drainer
    /// must preserve exclusivity (each copied record whole), order (no skip,
    /// no duplicate), the admission bound and both accounting identities.
    /// SCHEDULE-CLEAN within these 2-thread / 3-attempt bounds — not a general
    /// proof.
    #[cfg(loom)]
    #[test]
    fn all_schedules_preserve_exclusivity_order_and_accounting() {
        loom::model(|| {
            use loom::sync::Arc;
            use loom::thread;
            let pool = Arc::new(PoolCore::new());
            let producer_pool = Arc::clone(&pool);
            let producer = thread::spawn(move || {
                let mut accepted = 0u32;
                for i in 1..=3u64 {
                    if producer_pool.publish(record(i)) {
                        accepted += 1;
                    }
                }
                accepted
            });
            let drainer_pool = Arc::clone(&pool);
            let drainer = thread::spawn(move || {
                let mut copied: Vec<(u64, u32)> = Vec::new();
                for _ in 0..2 {
                    drainer_pool.drain(&mut |record| {
                        copied.push((marker(record), record.status.unwrap_or(0) as u32))
                    });
                }
                copied
            });
            let accepted = producer.join().unwrap();
            let copied = drainer.join().unwrap();
            for (index, (marker, status)) in copied.iter().enumerate() {
                assert_eq!(*marker, index as u64 + 1, "skip/duplicate/out-of-order");
                assert_eq!(*status, *marker as u32, "torn or mixed record");
            }
            let outstanding = pool.outstanding();
            assert!(outstanding <= 2, "admission bound");
            assert_eq!(
                u64::from(accepted),
                pool.copied_out() + u64::from(outstanding),
                "drain boundary"
            );
            assert_eq!(pool.accepted() + pool.rejected(), pool.attempted());
        });
    }

    /// Loom over the WRAP: the same protocol with counters at the natural-wrap
    /// boundary (`u32::MAX - 1`), so slot parity alternation and the modular
    /// admission distance are exercised inside the schedule exploration
    /// (gate 1's cross-wrap state).
    #[cfg(loom)]
    #[test]
    fn all_schedules_preserve_the_cross_wrap_state() {
        loom::model(|| {
            use loom::sync::Arc;
            use loom::thread;
            let m = u32::MAX;
            let pool = Arc::new(PoolCore::with_counters(m - 1, m - 1));
            let producer_pool = Arc::clone(&pool);
            let producer = thread::spawn(move || {
                let mut accepted = 0u32;
                for i in 1..=2u64 {
                    if producer_pool.publish(record(i)) {
                        accepted += 1;
                    }
                }
                accepted
            });
            let drainer_pool = Arc::clone(&pool);
            let drainer = thread::spawn(move || {
                let mut copied = Vec::new();
                for _ in 0..2 {
                    drainer_pool.drain(&mut |record| copied.push(marker(record)));
                }
                copied
            });
            let accepted = producer.join().unwrap();
            let copied = drainer.join().unwrap();
            for (index, marker) in copied.iter().enumerate() {
                assert_eq!(*marker, index as u64 + 1, "wrap walk out of order");
            }
            assert!(pool.outstanding() <= 2);
            assert_eq!(
                u64::from(accepted),
                pool.copied_out() + u64::from(pool.outstanding())
            );
        });
    }
}
