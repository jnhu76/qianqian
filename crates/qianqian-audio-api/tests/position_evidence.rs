//! Position-evidence cell oracles (F4, ADR-PBK-002 D14.8): the frozen
//! writer-side publication rule and the pure-read contract of the one
//! session-owned cell the render leg publishes into.
//!
//! Deterministic and thread-free: this file pins the accounting algebra
//! the real render loop feeds (the production loop's call ORDER around
//! this algebra is pinned by the output mechanism's own source-order
//! oracle, `crates/qianqian-output-wasapi/src/render_order_oracle.rs`).
//!
//! Truth classes under test: the published sample is Mechanism Evidence
//! — never a Fact — and the reader is one pure load: it holds no state,
//! applies no clamp, and cannot reconstruct anything from a second cell
//! (the cell exposes no handed-off and no tail reader at all).

use qianqian_audio_api::ports::PositionEvidence;

/// The 1024-frame block the production staging path uses; one block is
/// the unit the rejected reader shape tears by.
const BLOCK: u64 = 1024;

#[test]
fn undefined_is_none_not_zero_until_the_first_publication() {
    let cell = PositionEvidence::new();
    assert_eq!(
        cell.published(),
        None,
        "a fresh episode has no position — not position zero"
    );

    // A publication that truthfully carries zero consumed frames is NOT
    // undefined: the stream-start observation (nothing submitted yet,
    // nothing queued) publishes a real 0.
    cell.publish_consumed(0, 0);
    assert_eq!(cell.published(), Some(0));

    // ...and it stays distinguishable from undefined afterwards.
    let fresh = PositionEvidence::new();
    assert_eq!(fresh.published(), None);
    assert_ne!(fresh.published(), Some(0));
}

#[test]
fn publication_is_monotone_even_when_the_raw_estimate_regresses() {
    let cell = PositionEvidence::new();
    cell.publish_consumed(0, 0);
    assert_eq!(cell.published(), Some(0));

    // Eight blocks submitted, none consumed yet.
    cell.publish_consumed(8 * BLOCK, 8 * BLOCK);
    assert_eq!(cell.published(), Some(0));

    // The device drained everything: the estimate jumps to the total.
    cell.publish_consumed(8 * BLOCK, 0);
    assert_eq!(cell.published(), Some(8 * BLOCK));

    // The queue grows again (the tail reading is not monotone): the raw
    // estimate would fall back to zero, the published sample must not.
    cell.publish_consumed(8 * BLOCK, 8 * BLOCK);
    assert_eq!(
        cell.published(),
        Some(8 * BLOCK),
        "monotonicity is owned by the publication, not by the reader"
    );

    // Submission resumes and the device keeps up: it advances again.
    cell.publish_consumed(9 * BLOCK, 0);
    assert_eq!(cell.published(), Some(9 * BLOCK));
}

#[test]
fn the_published_sample_never_exceeds_the_writers_handed_off_accounting() {
    let cell = PositionEvidence::new();
    let mut handed_off = 0u64;

    for block in 1..=64u64 {
        let n = block * 16;
        handed_off += n;
        // A tail reading anywhere in [0, handed_off] — including the
        // pathological "the whole queue is still pending".
        for tail in [0, handed_off, handed_off / 2, handed_off.saturating_sub(1)] {
            cell.publish_consumed(handed_off, tail);
            let published = cell.published().expect("published after the start");
            assert!(
                published <= handed_off,
                "published {published} exceeded handed-off {handed_off} (tail {tail})"
            );
        }
    }
}

#[test]
fn a_tail_reading_above_the_handed_off_total_is_capped_not_wrapped() {
    let cell = PositionEvidence::new();
    cell.publish_consumed(BLOCK, 0);
    assert_eq!(cell.published(), Some(BLOCK));

    // A legal transient: the device reports more queued frames than this
    // leg can account for. The estimate floors at zero; nothing wraps.
    cell.publish_consumed(BLOCK, 9_999);
    assert_eq!(cell.published(), Some(BLOCK), "never backward, never huge");

    // Recovery: the reading comes back into range and advances again.
    cell.publish_consumed(2 * BLOCK, 0);
    assert_eq!(cell.published(), Some(2 * BLOCK));
}

#[test]
fn pause_drain_advances_to_the_handed_off_total_and_then_holds() {
    // Ten blocks in flight, the device has consumed four of them; the
    // pause gate then parks the leg, so no further block is ever handed
    // off. The park slices keep observing the draining tail.
    let cell = PositionEvidence::new();
    let handed_off = 10 * BLOCK;
    cell.publish_consumed(handed_off, 6 * BLOCK);
    assert_eq!(cell.published(), Some(4 * BLOCK));

    cell.publish_consumed(handed_off, 3 * BLOCK);
    assert_eq!(cell.published(), Some(7 * BLOCK));

    // Tail quiescence: the published sample reaches the frozen total —
    // the same instant D14.7's Paused establishment observes tail == 0.
    cell.publish_consumed(handed_off, 0);
    assert_eq!(cell.published(), Some(handed_off));

    // Still parked: further slices move nothing.
    for _ in 0..8 {
        cell.publish_consumed(handed_off, 0);
        assert_eq!(cell.published(), Some(handed_off));
    }
}

#[test]
fn the_eof_drain_rises_to_the_exact_handed_off_total() {
    // The clean Completed path: the producer blocks rather than
    // dropping, so every produced frame is handed off, and the drain
    // observation at padded == 0 publishes the exact total. That total
    // is consumption truth; it is not a claim about the metadata
    // duration (see the duration oracles).
    let cell = PositionEvidence::new();
    let total = 176_400u64; // 4 s at 44.1 kHz
    let mut handed_off = 0u64;
    while handed_off < total {
        let n = BLOCK.min(total - handed_off);
        handed_off += n;
        cell.publish_consumed(handed_off, 2 * BLOCK);
    }
    let before_drain = cell.published().expect("published during the episode");
    assert!(
        before_drain <= total,
        "the sample never exceeds the handed-off total: {before_drain}"
    );
    cell.publish_consumed(total, 0);
    assert_eq!(cell.published(), Some(total));
}

#[test]
fn the_encoding_saturates_instead_of_wrapping_at_its_domain_boundary() {
    let cell = PositionEvidence::new();
    cell.publish_consumed(PositionEvidence::MAX_POSITION, 0);
    assert_eq!(cell.published(), Some(PositionEvidence::MAX_POSITION));

    // One frame past the domain: the sample saturates and stays a
    // sample. It must not wrap into the undefined sentinel (which would
    // read as "unknown") and must not panic or go backward.
    cell.publish_consumed(u64::MAX, 0);
    assert_eq!(cell.published(), Some(PositionEvidence::MAX_POSITION));

    // A fresh cell is still distinguishable: the sentinel is the
    // zero-initialized value only.
    assert_eq!(PositionEvidence::new().published(), None);
}

#[test]
fn repeated_reads_are_pure_loads() {
    let cell = PositionEvidence::new();

    // Reading before any publication neither creates a sample...
    for _ in 0..4 {
        assert_eq!(cell.published(), None);
    }

    cell.publish_consumed(3 * BLOCK, BLOCK);
    let sample = cell.published();
    assert_eq!(sample, Some(2 * BLOCK));

    // ...nor changes one: repeating the read changes nothing, and
    // nothing about the cell's value depends on how often it is read.
    //
    // Stated limit: this is the OBSERVABLE consequence, not the purity
    // proof. An idempotent reader-side clamp would be invisible here
    // because the cell is already monotone; what rules that shape out is
    // that `published()` is a single load with no writable state
    // anywhere in the read path (and the observation surface admits no
    // second cell to clamp against).
    for _ in 0..64 {
        assert_eq!(cell.published(), sample);
    }

    // A reader cannot reconstruct device state from this cell: the type
    // exposes no handed-off total, no tail, and no raw estimate — the
    // rejected two-cell composition has no API here (see the negative
    // control below).
    cell.publish_consumed(3 * BLOCK, 0);
    assert_eq!(cell.published(), Some(3 * BLOCK));
}

/// Negative control for the shape this cell replaced: composing a
/// position in the READER from two separately published values tears
/// backward by one block when one hand-off lands between the two loads
/// (the counterexample that forced the collapse to a single published
/// cell, kept executable by the F4-GATE evidence crate:
/// `experiments/f4-timeline-gate/src/timeline.rs`,
/// `rejected_two_cell_reader_pair_tears_backward_by_one_block`).
///
/// The two loads here are the rejected shape's, not the product's: the
/// selected cell publishes the maximum on the writer side, so the same
/// interleaving cannot reach a reader at all.
#[test]
fn the_rejected_two_cell_reader_pair_tears_backward_by_one_block() {
    let handed_off_cell = std::sync::atomic::AtomicU64::new(8 * BLOCK);
    let tail_cell = std::sync::atomic::AtomicU64::new(0);

    let handed_off_seen = handed_off_cell.load(std::sync::atomic::Ordering::Relaxed); // load #1
    handed_off_cell.fetch_add(BLOCK, std::sync::atomic::Ordering::Relaxed); // leg: one more block
    tail_cell.store(BLOCK, std::sync::atomic::Ordering::Relaxed); // leg: device queued it, consumed none
    let tail_seen = tail_cell.load(std::sync::atomic::Ordering::Relaxed); // load #2

    let torn = handed_off_seen - tail_seen.min(handed_off_seen);
    assert_eq!(handed_off_seen, 8 * BLOCK);
    assert_eq!(tail_seen, BLOCK);
    assert_eq!(torn, 7 * BLOCK);
    assert_eq!(
        8 * BLOCK - torn,
        BLOCK,
        "the rejected reader pair steps backward by one in-flight block"
    );
}

/// F5 (ADR-PBK-002 D14.5): a committed cutover rebases the SAME cell on
/// the writer's side — the one legal backward step. The stretch basis
/// after the rebase is the decoder's reported ACTUAL landing, and
/// monotone publication holds again within the new stretch.
#[test]
fn a_committed_rebase_is_the_one_legal_backward_step() {
    let cell = PositionEvidence::new();
    // Pre-cut stretch: the episode played into its 8th block.
    cell.publish_consumed(8 * BLOCK, 0);
    assert_eq!(cell.published(), Some(8 * BLOCK));

    // The cutover commits: the decoder reported landing at 2 blocks.
    // (Deliberately NOT the requested target — see the next test.)
    cell.rebase(Some(2 * BLOCK));
    assert_eq!(
        cell.published(),
        Some(2 * BLOCK),
        "the rebase is the one legal backward step"
    );

    // Post-cut publications are monotone WITHIN the new stretch: the
    // same-cell rule keeps pre-cut and post-cut accounting from mixing.
    // (The real writer passes `basis + handed_off` — the stretch basis
    // folded into the handed-off total, exactly as the render loop's
    // helper does; the raw calls here mirror that argument shape.)
    cell.publish_consumed(2 * BLOCK, 0);
    assert_eq!(cell.published(), Some(2 * BLOCK));
    cell.publish_consumed(2 * BLOCK + 3 * BLOCK, 0);
    assert_eq!(cell.published(), Some(5 * BLOCK));

    // A regressing tail reading still cannot pull the sample backward
    // across the stretch boundary (the max is the stretch invariant).
    cell.publish_consumed(2 * BLOCK + 3 * BLOCK, 3 * BLOCK);
    assert_eq!(cell.published(), Some(5 * BLOCK));
}

/// Landing at frame zero is a REAL position, not undefined: the +1
/// encoding keeps `Some(0)` distinct from the never-published sentinel
/// (a backward cutover to the very start of the source must not read as
/// "no sample").
#[test]
fn a_zero_landing_is_a_position_not_undefined() {
    let cell = PositionEvidence::new();
    cell.publish_consumed(4 * BLOCK, 0);
    cell.rebase(Some(0));
    assert_eq!(cell.published(), Some(0));
    assert_ne!(cell.published(), None, "zero is not the undefined sentinel");
}

/// Unknown landing withdraws the projection: the cell returns to the
/// undefined sentinel and the sample is gone. Permanence is the WRITER's
/// obligation (the leg's publishing flag stops all further publication),
/// not a cell-side enforcement — the cell is one atomic sample with no
/// authority to judge its writer, and this test pins that boundary
/// honestly rather than pretending the cell could enforce it.
#[test]
fn an_unknown_landing_withdraws_the_sample_forever_on_the_writers_discipline() {
    let cell = PositionEvidence::new();
    cell.publish_consumed(4 * BLOCK, 0);
    assert!(cell.published().is_some());
    cell.rebase(None);
    assert_eq!(
        cell.published(),
        None,
        "unknown is not zero, not the target"
    );

    // The writer's obligation: after an unknown-landing cutover the leg
    // publishes NOTHING more. The cell itself does not (and must not)
    // police a rogue write — a publication that nevertheless happens
    // would raise the sample again, which is exactly why the discipline
    // lives in the leg (publishing = false) and its test in the seek
    // matrices, not here.
    cell.publish_consumed(BLOCK, 0);
    assert_eq!(
        cell.published(),
        Some(BLOCK),
        "the cell enforces nothing against a rogue writer; withdrawal is the writer's discipline"
    );
}

/// The rebase is a writer-side store: readers keep their one-pure-load
/// contract across it (no clamp, no state, no second cell).
#[test]
fn reads_after_a_rebase_stay_pure_and_stable() {
    let cell = PositionEvidence::new();
    cell.publish_consumed(4 * BLOCK, 0);
    cell.rebase(Some(BLOCK));
    assert_eq!(cell.published(), Some(BLOCK));
    assert_eq!(
        cell.published(),
        Some(BLOCK),
        "repeating a read changes nothing"
    );
    cell.rebase(Some(3 * BLOCK));
    assert_eq!(cell.published(), Some(3 * BLOCK));
    assert_eq!(cell.published(), Some(3 * BLOCK));
}
