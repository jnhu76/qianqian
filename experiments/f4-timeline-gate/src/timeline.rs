//! The position-evidence shape and its deterministic oracles.
//!
//! Nothing here touches a thread, a device or a decoder. This module
//! pins the accounting the F4 gate proposes for a product Position, so
//! the physical probe (f4probe) only has to confirm that the real WASAPI
//! mechanism can publish the same single monotone cell:
//!
//! ```text
//! writer (render leg — one execution path owns both inputs)
//!   hand_off(n)          n source frames were submitted into the
//!                        device buffer; mechanism-local accounting
//!   publish_consumed(p)  the mechanism observed p frames still queued
//!                        to play:
//!                          estimate  = handed_off - min(p, handed_off)
//!                          published = max(published, estimate)
//!                        (one relaxed monotone update of one cell)
//!
//! reader (D14.2 observation seam)
//!   position()           ONE relaxed load; None while the cell is
//!                        undefined (unknown is never collapsed to 0)
//! ```
//!
//! The reader keeps no state: monotonicity is owned by the publication.
//! A reader-side clamp over two separately published cells was this
//! gate's first draft and is REJECTED — it cannot live inside a pure
//! read (`observe()` is "one coherent pure read"; repeating it changes
//! nothing, settles nothing). The rejected shape is kept below as an
//! executable negative control, never as a product rule.

use std::sync::atomic::{AtomicU64, Ordering};

/// The published cell encodes position `v` as `v + 1`, leaving the
/// zero-initialized value as "undefined". One encoding serves both
/// requirements: undefined needs no second load, and a monotone
/// `fetch_max` can start from the initial value.
const UNDEFINED: u64 = 0;

/// Session-owned position-evidence cell in the shape the gate proposes
/// for production (relaxed atomics, written once per mechanism
/// observation, read without locks by the observation path).
pub struct PositionEvidence {
    /// Writer-local handed-off accounting. Kept as an atomic only so the
    /// physical probe and these oracles can report it; no product reader
    /// derives anything from it — composing it with `tail` in a reader
    /// IS the rejected shape (see the negative-control oracle below).
    handed_off: AtomicU64,
    /// Writer-local last tail reading; diagnostics only, same reason.
    tail: AtomicU64,
    /// Writer-local last raw estimate; diagnostics only, same reason.
    estimate: AtomicU64,
    /// The one published cell (encoded; [`UNDEFINED`] until the first
    /// publication).
    published: AtomicU64,
    /// Publication count — mechanism cadence diagnostic.
    publications: AtomicU64,
}

impl PositionEvidence {
    pub fn new() -> Self {
        Self {
            handed_off: AtomicU64::new(0),
            tail: AtomicU64::new(0),
            estimate: AtomicU64::new(0),
            published: AtomicU64::new(UNDEFINED),
            publications: AtomicU64::new(0),
        }
    }

    /// Writer: `n` source frames were submitted into the device buffer
    /// (`read_frames` → `ReleaseBuffer(n)` in the render leg). Returns
    /// the writer's own running total.
    pub fn hand_off(&self, n: u64) -> u64 {
        self.handed_off.fetch_add(n, Ordering::Relaxed) + n
    }

    /// Writer: the render mechanism observed `p` frames still queued to
    /// play, derived the consumed estimate from its own handed-off total
    /// and published it monotonically. Returns the published value.
    pub fn publish_consumed(&self, p: u64) -> u64 {
        let handed_off = self.handed_off.load(Ordering::Relaxed);
        let estimate = handed_off.saturating_sub(p.min(handed_off));
        self.tail.store(p, Ordering::Relaxed);
        self.estimate.store(estimate, Ordering::Relaxed);
        self.publications.fetch_add(1, Ordering::Relaxed);
        // fetch_max returns the PREVIOUS stored value; what the reader
        // sees afterwards is the max of that and this estimate.
        let previous = self.published.fetch_max(estimate + 1, Ordering::Relaxed);
        previous.max(estimate + 1) - 1
    }

    /// Reader: one pure load. `None` while the cell is undefined —
    /// before the mechanism's first publication, and (at the observation
    /// layer) once the terminal Fact withdraws the projection.
    pub fn position(&self) -> Option<u64> {
        match self.published.load(Ordering::Relaxed) {
            UNDEFINED => None,
            encoded => Some(encoded - 1),
        }
    }
}

/// Diagnostics for the physical probe and these oracles (never product
/// surfaces, never a derivation input).
impl PositionEvidence {
    pub fn diag_handed_off(&self) -> u64 {
        self.handed_off.load(Ordering::Relaxed)
    }
    pub fn diag_tail(&self) -> u64 {
        self.tail.load(Ordering::Relaxed)
    }
    pub fn diag_estimate(&self) -> Option<u64> {
        if self.published.load(Ordering::Relaxed) == UNDEFINED {
            None
        } else {
            Some(self.estimate.load(Ordering::Relaxed))
        }
    }
    pub fn diag_publications(&self) -> u64 {
        self.publications.load(Ordering::Relaxed)
    }
}

impl Default for PositionEvidence {
    fn default() -> Self {
        Self::new()
    }
}

/// One-block backward step of the REJECTED two-cell reader shape (1024
/// source frames is the production staging block; the probe uses the
/// same size). It is the negative control's diagnostic — NOT a contract
/// on the selected shape, which has no cross-cell read at all.
pub const REJECTED_PAIR_BACKWARD_STEP: u64 = 1024;

#[cfg(test)]
mod oracles {
    use super::*;

    /// Unknown stays None and never collapses into zero: handed-off
    /// accounting alone publishes nothing.
    #[test]
    fn undefined_is_none_not_zero_until_the_first_publication() {
        let ev = PositionEvidence::new();
        assert_eq!(ev.position(), None);
        ev.hand_off(4096);
        assert_eq!(ev.position(), None);
        ev.publish_consumed(0);
        assert_eq!(ev.position(), Some(4096));
    }

    /// Each published sample is exact for the instant the writer read the
    /// tail: the two inputs belong to one execution path.
    #[test]
    fn published_value_is_exact_for_the_writers_instant() {
        let ev = PositionEvidence::new();
        ev.publish_consumed(0); // stream start: nothing handed off yet
        assert_eq!(ev.position(), Some(0));
        for _ in 0..8 {
            ev.hand_off(1024);
        }
        ev.publish_consumed(4096); // queue holds half -> half consumed
        assert_eq!(ev.position(), Some(4096));
        ev.publish_consumed(0); // device drained everything submitted
        assert_eq!(ev.position(), Some(8192));
    }

    /// A tail reading above the handed-off total (a legal transient) is
    /// capped at it: never wrapped, never trusted.
    #[test]
    fn tail_above_the_handed_off_total_is_capped_not_wrapped() {
        let ev = PositionEvidence::new();
        ev.hand_off(1024);
        ev.publish_consumed(0);
        assert_eq!(ev.position(), Some(1024));
        ev.publish_consumed(9999);
        assert_eq!(ev.diag_estimate(), Some(0));
        assert_eq!(ev.position(), Some(1024));
    }

    /// The tail reading is not monotone, so the raw estimate can regress;
    /// the published sample cannot. Monotonicity is the publication rule,
    /// not a caller-supplied clamp.
    #[test]
    fn publication_never_goes_backward_when_the_queue_grows_again() {
        let ev = PositionEvidence::new();
        ev.publish_consumed(0);
        ev.hand_off(1024);
        ev.publish_consumed(0);
        assert_eq!(ev.position(), Some(1024));
        ev.publish_consumed(1024); // queue grew: the raw estimate drops
        assert_eq!(ev.diag_estimate(), Some(0));
        assert_eq!(ev.position(), Some(1024));
        ev.hand_off(1024); // submission resumes after quiescence
        ev.publish_consumed(0);
        assert_eq!(ev.position(), Some(2048));
    }

    /// The published sample is always ≤ the mechanism's own accounting.
    #[test]
    fn consumed_never_exceeds_the_handed_off_accounting() {
        let ev = PositionEvidence::new();
        ev.publish_consumed(0);
        let mut total = 0u64;
        for block in 1..=64u64 {
            let n = block * 16;
            total += n;
            ev.hand_off(n);
            let published = ev.publish_consumed(total.min(2048));
            assert!(published <= ev.diag_handed_off());
        }
    }

    /// Pause: after engagement the writer hands off nothing further, the
    /// park slices keep publishing the draining tail, and the published
    /// sample stops moving at tail quiescence — the same evidence that
    /// establishes D14.7 Paused.
    #[test]
    fn pause_freezes_the_published_sample_at_tail_quiescence() {
        let ev = PositionEvidence::new();
        ev.publish_consumed(0);
        for _ in 0..10 {
            ev.hand_off(1024);
        }
        ev.publish_consumed(4096); // 6 of 10 blocks consumed
        assert_eq!(ev.position(), Some(6144));

        // Pause engaged: no further hand_off from here.
        ev.publish_consumed(2048);
        assert_eq!(ev.position(), Some(8192));
        ev.publish_consumed(0); // tail quiescence = Paused establishment
        assert_eq!(ev.position(), Some(10240));

        // Still parked, still frozen: further park slices do not move it.
        for _ in 0..5 {
            ev.publish_consumed(0);
            assert_eq!(ev.position(), Some(10240));
        }
    }

    /// EOF: submission reaches the exact decoded total, the device
    /// drains, the published sample rises to that total — not to the
    /// metadata duration, which is separate evidence and may disagree.
    #[test]
    fn eof_rises_to_the_exact_decoded_total() {
        let ev = PositionEvidence::new();
        let total: u64 = 176_400; // the committed 4 s / 44.1 kHz corpus total
        ev.publish_consumed(0);
        let mut done = 0u64;
        while done < total {
            let n = (total - done).min(1024);
            ev.hand_off(n);
            done += n;
            ev.publish_consumed(done.min(2048));
        }
        assert!(ev.position().unwrap() < total); // tail still queued
        ev.publish_consumed(0);
        assert_eq!(ev.position(), Some(total));
    }

    /// Terminal withdrawal is an observation-layer guard: the cell may
    /// still hold evidence after settlement, and the observation simply
    /// stops deriving the projection from it (no final-position latch
    /// storage, no writer, no race).
    #[test]
    fn terminal_withdrawal_is_observation_gating_not_a_cell_write() {
        let ev = PositionEvidence::new();
        ev.publish_consumed(0);
        ev.hand_off(4096);
        ev.publish_consumed(1024);
        assert_eq!(ev.position(), Some(3072));

        let terminal_committed = true;
        let observed = if terminal_committed {
            None
        } else {
            ev.position()
        };
        assert_eq!(observed, None);
        assert_eq!(ev.position(), Some(3072));
    }

    /// Negative control: the REJECTED shape, executed. With two
    /// separately published cells composed by the reader, one hand-off
    /// between the two loads tears the sample backward by one block —
    /// the defect that forced the collapse to a single published cell.
    /// The selected shape is immune to the same interleaving.
    #[test]
    fn rejected_two_cell_reader_pair_tears_backward_by_one_block() {
        let ev = PositionEvidence::new();
        ev.publish_consumed(0);
        for _ in 0..8 {
            ev.hand_off(1024);
            ev.publish_consumed(0);
        }
        assert_eq!(ev.position(), Some(8192));

        // The rejected reader, load by load:
        let handed_off_seen = ev.diag_handed_off(); // load #1
        ev.hand_off(1024); // leg: one more block handed off
        ev.publish_consumed(1024); // leg: device queued it, consumed none
        let tail_seen = ev.diag_tail(); // load #2
        let torn = handed_off_seen.saturating_sub(tail_seen.min(handed_off_seen));

        assert_eq!(handed_off_seen, 8192);
        assert_eq!(tail_seen, 1024);
        assert_eq!(torn, 7168);
        assert_eq!(
            8192 - torn,
            REJECTED_PAIR_BACKWARD_STEP,
            "the rejected pair steps backward by one block"
        );
        assert_eq!(
            ev.position(),
            Some(8192),
            "the selected single-cell publication is unaffected by the same interleaving"
        );
    }

    /// Legal interleaving fuzz over a scripted schedule (xorshift, no
    /// threads — the physical probe covers the real scheduler): the
    /// published sample is exactly the running maximum of the writer's
    /// estimates and never regresses, while the estimates themselves do.
    #[test]
    fn scripted_interleavings_keep_the_published_sample_monotone() {
        let mut state = 0x9E3779B97F4A7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        for _round in 0..5 {
            let ev = PositionEvidence::new();
            ev.publish_consumed(0);
            let mut last_published = ev.position();
            let mut max_estimate = 0u64;
            let mut regressions = 0usize;

            for _step in 0..400 {
                match next() % 5 {
                    // Hand off a block-sized chunk.
                    0..=1 => {
                        ev.hand_off((next() % 4 + 1) * 256);
                    }
                    // The device consumed part of the queued tail.
                    2 => {
                        let tail = ev.diag_tail().saturating_sub((next() % 3 + 1) * 512);
                        ev.publish_consumed(tail);
                    }
                    // The queue grows again (not monotone).
                    3 => {
                        let tail = ev.diag_tail() + (next() % 3) * 1024;
                        ev.publish_consumed(tail);
                    }
                    // Park slice / drain observation.
                    _ => {
                        ev.publish_consumed(0);
                    }
                }

                let published = ev.position().expect("published after stream start");
                if let Some(prev) = last_published {
                    assert!(
                        published >= prev,
                        "published went backward: {prev} -> {published}"
                    );
                }
                let estimate = ev.diag_estimate().expect("estimate known");
                if estimate < max_estimate {
                    regressions += 1;
                }
                max_estimate = max_estimate.max(estimate);
                assert_eq!(
                    published, max_estimate,
                    "the published sample is the running max of the estimates"
                );
                assert!(published <= ev.diag_handed_off());
                last_published = Some(published);
            }

            assert!(regressions > 0, "schedule never regressed an estimate");
        }
    }
}
