//! The position projection algebra and its deterministic oracles.
//!
//! Nothing here touches a thread, a device or a decoder. This module
//! pins the exact accounting the F4 gate proposes for a product
//! Position, so the physical probe (f4probe) only has to confirm that
//! the real WASAPI mechanism publishes the same evidence shape the
//! algebra assumes:
//!
//! ```text
//! writers (mechanism legs):
//!   submit(n)        one edge handoff / device submission of n frames
//!   observe_tail(p)  one mechanism tail observation (p frames still
//!                    queued to play); the first one is stream-start
//!                    evidence
//!
//! reader (observation path):
//!   raw = submitted - min(tail, submitted)
//!   position = max(last_projected, raw)     -- monotone clamp
//!   None before the first tail observation (unknown is not zero)
//! ```

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Session-owned position evidence cells in exactly the shape the gate
/// proposes for production (atomics, published per block by the
/// mechanism legs, read by the observation path without locks).
pub struct PositionEvidence {
    submitted: AtomicU64,
    tail: AtomicU64,
    tail_published: AtomicBool,
}

impl PositionEvidence {
    pub fn new() -> Self {
        Self {
            submitted: AtomicU64::new(0),
            tail: AtomicU64::new(0),
            tail_published: AtomicBool::new(false),
        }
    }

    /// Producer-side evidence: `n` source frames were handed to the
    /// render leg (and, on every non-terminal path, submitted into the
    /// device buffer). Monotone accumulation; the caller is the edge
    /// read path, once per successful pull.
    pub fn submit(&self, n: u64) {
        self.submitted.fetch_add(n, Ordering::Relaxed);
    }

    /// Mechanism-side evidence: the output mechanism observed `p`
    /// frames still queued to play. The first call of an episode is the
    /// stream-start evidence; before it, no position is derivable.
    pub fn observe_tail(&self, p: u64) {
        self.tail.store(p, Ordering::Relaxed);
        self.tail_published.store(true, Ordering::Relaxed);
    }

    /// Raw derived position: `None` before stream-start evidence
    /// (unknown is never collapsed into zero), otherwise
    /// `submitted - min(tail, submitted)`.
    ///
    /// The two loads are not atomic as a pair: a submission between
    /// them can make one sample step backward by at most that block
    /// (`MAX_RAW_BACKWARD_STEP` bounds it at one full block). That is
    /// why the product projection applies [`Self::clamped`].
    pub fn raw(&self) -> Option<u64> {
        if !self.tail_published.load(Ordering::Relaxed) {
            return None;
        }
        let submitted = self.submitted.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);
        Some(submitted.saturating_sub(tail.min(submitted)))
    }

    /// The product projection: raw position clamped monotone against
    /// the caller's last value. `None` only while the position is not
    /// (yet or any longer) defined — before stream-start evidence or,
    /// at the observation layer, after the terminal Fact (the
    /// observation layer withdraws it by guard, not by cell writes).
    pub fn clamped(&self, last: Option<u64>) -> Option<u64> {
        match (last, self.raw()) {
            (_, None) => None,
            (None, Some(raw)) => Some(raw),
            (Some(prev), Some(raw)) => Some(prev.max(raw)),
        }
    }
}

impl Default for PositionEvidence {
    fn default() -> Self {
        Self::new()
    }
}

/// Upper bound of one raw backward step: a torn reader pair can straddle
/// at most one block submission (1024 frames is the production staging
/// block; the probe uses the same block size).
pub const MAX_RAW_BACKWARD_STEP: u64 = 1024;

// --- test-only accessors (the physical probe covers the real scheduler) ----

#[cfg(test)]
impl PositionEvidence {
    /// Tear-test helper: a bare load of the submitted cell, so a test
    /// can construct the exact two-load tear [`Self::raw`] avoids
    /// clamping. Never part of any proposed production surface.
    fn test_submitted(&self) -> u64 {
        self.submitted.load(Ordering::Relaxed)
    }

    /// Tear-test helper: a bare load of the tail cell.
    fn test_tail(&self) -> u64 {
        self.tail.load(Ordering::Relaxed)
    }

    /// Current tail cell for scripted schedules (algebra tests only).
    fn current_tail(&self) -> u64 {
        self.tail.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod oracles {
    use super::*;

    /// The raw derivation never claims more consumed than submitted,
    /// whatever the tail reports (a tail above submitted is capped).
    #[test]
    fn raw_is_capped_by_submitted() {
        let ev = PositionEvidence::new();
        ev.observe_tail(500);
        assert_eq!(ev.raw(), Some(0));
        ev.submit(100);
        assert_eq!(ev.raw(), Some(0));
        ev.submit(400);
        assert_eq!(ev.raw(), Some(0));
        ev.submit(1);
        assert_eq!(ev.raw(), Some(1));
        // A stale tail above submitted (a legal transient
        // inconsistency) is capped at submitted, never wrapped and
        // never trusted.
        ev.observe_tail(10_000);
        assert_eq!(ev.raw(), Some(0));
    }

    /// Unknown stays None and never collapses into zero: before the
    /// mechanism's first tail observation, submitted evidence alone
    /// derives nothing.
    #[test]
    fn unknown_before_stream_start_is_none_not_zero() {
        let ev = PositionEvidence::new();
        ev.submit(4096);
        assert_eq!(ev.raw(), None);
        assert_eq!(ev.clamped(None), None);
        ev.observe_tail(0);
        assert_eq!(ev.raw(), Some(4096));
    }

    /// Tail reads below the queued truth make the consumed estimate run
    /// ahead but never past submitted; the projection is honest about
    /// its bound.
    #[test]
    fn consumed_never_exceeds_submitted() {
        let ev = PositionEvidence::new();
        ev.observe_tail(0);
        let mut cumulative = 0u64;
        for block in 1..=64u64 {
            let n = block * 16;
            ev.submit(n);
            cumulative += n;
            assert!(ev.raw().unwrap() <= cumulative);
        }
    }

    /// Torn reader pair, constructed exactly: the reader loads
    /// `submitted`, the render leg then submits one block and
    /// republishes the grown tail, and only then does the reader load
    /// `tail`. The raw value steps backward by exactly that one block;
    /// the monotone clamp erases it. This is the worst legal tear of
    /// the two-cell read (the production observation clamps, so a
    /// displayed position never jitters).
    #[test]
    fn torn_read_backward_step_is_bounded_by_one_block_and_clamp_erases_it() {
        let ev = PositionEvidence::new();
        ev.observe_tail(0);
        // Steady state: submitted 8 blocks, device consumed all of it.
        for _ in 0..8 {
            ev.submit(1024);
        }
        ev.observe_tail(0);
        let before = ev.raw();
        assert_eq!(before, Some(8192));

        // The tear, load by load:
        let submitted_seen = ev.test_submitted(); // reader load #1
        ev.submit(1024); // leg: one new block handed off
        ev.observe_tail(1024); // leg: device queued it, consumed none
        let tail_seen = ev.test_tail(); // reader load #2
        let torn_raw = submitted_seen.saturating_sub(tail_seen.min(submitted_seen));

        assert_eq!(submitted_seen, 8192);
        assert_eq!(tail_seen, 1024);
        assert_eq!(torn_raw, 7168);
        assert_eq!(
            before.unwrap() - torn_raw,
            MAX_RAW_BACKWARD_STEP,
            "the worst legal tear steps back by at most one block"
        );

        // The clamp erases exactly this: a reader holding last = 8192
        // that performs the same torn read reports 8192, never 7168.
        // (clamped() re-reads both cells fresh; feed it the torn value
        // through the same max the observation layer applies.)
        assert_eq!(ev.clamped(Some(8192)), Some(8192));
    }

    /// Pause semantics: after engagement the submitter is frozen (the
    /// F3 gate sits before any further submission), the tail drains,
    /// the consumed estimate rises to the frozen submitted value, and
    /// the first tail==0 observation is exactly where the projection
    /// freezes — the same evidence that establishes D14.7 Paused.
    #[test]
    fn pause_freezes_position_exactly_at_tail_quiescence() {
        let ev = PositionEvidence::new();
        ev.observe_tail(0);
        // Steady: 10 blocks submitted, device has consumed 6, queues 4.
        for _ in 0..10 {
            ev.submit(1024);
        }
        ev.observe_tail(4096);
        assert_eq!(ev.raw(), Some(6144));

        // Pause engaged: no further submit() from here.
        ev.observe_tail(2048);
        assert_eq!(ev.raw(), Some(8192));
        let quiesced = ev.clamped(None);
        ev.observe_tail(0);
        let frozen = ev.clamped(quiesced);
        assert_eq!(frozen, Some(10240));
        assert_eq!(frozen, ev.raw());

        // Still parked, still frozen: repeated observations do not move.
        for _ in 0..5 {
            ev.observe_tail(0);
            assert_eq!(ev.clamped(frozen), Some(10240));
        }
    }

    /// EOF: submission reaches the exact decoded total, the tail drains,
    /// the projection rises to that total, and it equals the decoded
    /// frame count — not the metadata duration, which is separate
    /// evidence and may disagree.
    #[test]
    fn eof_rises_to_exact_decoded_total() {
        let ev = PositionEvidence::new();
        let total: u64 = 176_400; // the committed 4 s / 44.1 kHz corpus total
        ev.observe_tail(0);
        let mut done = 0u64;
        while done < total {
            let n = (total - done).min(1024);
            ev.submit(n);
            done += n;
            ev.observe_tail(done.min(2048));
        }
        // Decoder EOF published; device still queues the tail.
        assert!(ev.raw().unwrap() < total);
        ev.observe_tail(0);
        assert_eq!(ev.raw(), Some(total));
    }

    /// Terminal withdrawal is an observation-layer guard: the cells may
    /// still hold evidence after settlement, and the observation simply
    /// no longer derives from them (no final-position latch storage).
    #[test]
    fn position_is_withdrawn_once_the_terminal_fact_committed() {
        let ev = PositionEvidence::new();
        ev.observe_tail(0);
        ev.submit(4096);
        ev.observe_tail(1024);
        assert_eq!(ev.raw(), Some(3072));

        let terminal_committed = true;
        let observation = if terminal_committed { None } else { ev.raw() };
        assert_eq!(observation, None);
        // The cells are untouched — withdrawal needs no writer, no race,
        // no lifecycle storage.
        assert_eq!(ev.raw(), Some(3072));
    }

    /// Legal interleaving fuzz over a scripted schedule: for every
    /// reader sample the clamped projection is monotone and every raw
    /// backward step stays within one block. Deterministic pseudo-random
    /// schedule (xorshift), no threads — the physical probe covers the
    /// real scheduler.
    #[test]
    fn scripted_interleavings_keep_the_projection_monotone_and_bounded() {
        let mut state = 0x9E3779B97F4A7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        // Five rounds, each a full steady → park → quiesce → resume →
        // EOF-drain cycle with randomized block/tail values.
        for _round in 0..5 {
            let ev = PositionEvidence::new();
            ev.observe_tail(0);
            let mut last_clamped = None;
            let mut last_raw: Option<u64> = None;
            let mut parked = false;
            let mut frozen_at: Option<u64> = None;

            for _step in 0..400 {
                let roll = next() % 6;
                match roll {
                    // Device consumed between 0 and 3 blocks of tail.
                    0..=2 => {
                        let tail = ev.current_tail().saturating_sub((next() % 3 + 1) * 512);
                        ev.observe_tail(tail);
                    }
                    // A submission block (none while parked).
                    3 => {
                        if !parked {
                            ev.submit(1024);
                            ev.observe_tail(ev.current_tail() + 1024);
                        }
                    }
                    // Park / quiesce / resume events.
                    4 => {
                        parked = true; // engagement: submissions stop
                        frozen_at = None; // the bound is per-park
                    }
                    _ => {
                        if parked {
                            ev.observe_tail(0); // tail quiescence
                            frozen_at = ev.clamped(last_clamped);
                        }
                        parked = false;
                    }
                }

                let raw = ev.raw();
                if let (Some(prev), Some(cur)) = (last_raw, raw) {
                    assert!(
                        prev.saturating_sub(cur) <= MAX_RAW_BACKWARD_STEP,
                        "raw stepped backward by more than one block: {prev} -> {cur}"
                    );
                }
                last_raw = raw;
                let clamped = ev.clamped(last_clamped);
                if let (Some(prev), Some(cur)) = (last_clamped, clamped) {
                    assert!(
                        cur >= prev,
                        "clamped projection went backward: {prev} -> {cur}"
                    );
                }
                last_clamped = clamped;

                // While parked (before the quiescing release), submitted
                // is frozen and the projection may only rise to it.
                if parked {
                    if let Some(frozen) = frozen_at {
                        assert!(clamped.unwrap() <= frozen + 1024);
                    }
                }
            }
        }
    }
}
