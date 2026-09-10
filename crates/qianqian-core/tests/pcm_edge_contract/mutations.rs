//! Adversarial twins of the candidate transfers.
//!
//! Every twin below is a deliberately wrong implementation of one bug class,
//! executed for real. Each `#[test]` proves two things:
//!
//! 1. the honest baseline passes the named oracle, and
//! 2. the twin with the bug active is *killed* by that oracle — the failure
//!    comes from the twin's real execution state, not from an error the test
//!    constructed by hand.
//!
//! The legacy provenance labels for these mutations and the full kill matrix
//! live in `docs/architecture/pcm-contract-a0.md`; the code uses semantic
//! names only.

use super::candidates::BorrowedReadOnlyFlow;
use super::harness::{
    PcmFormat, PcmShapeError, PcmView, Sample, SyntheticConsumer, SyntheticProducer, TransferError,
    deterministic_sample,
};

fn stereo() -> PcmFormat {
    PcmFormat::new(48_000, 2).expect("stereo is a valid format")
}

/// Frame-conservation oracle: whatever delivery semantics an edge uses, the
/// frames the producer generated and the frames the consumer verified must
/// reconcile exactly.
fn assert_conserved(produced: usize, consumed: usize) -> Result<(), TransferError> {
    if produced == consumed {
        Ok(())
    } else {
        Err(TransferError::FramesLost {
            offered: produced,
            delivered: consumed,
        })
    }
}

// ---------------------------------------------------------------------------
// Bug class: frame count confused with scalar count
// ---------------------------------------------------------------------------

/// Baseline: a stereo buffer sized for 4 frames (8 scalars) serves 4 frames.
#[test]
fn frame_capacity_baseline_requests_whole_frames() {
    let mut producer = SyntheticProducer::new(stereo());
    let mut consumer = SyntheticConsumer::new(stereo());
    let mut storage = vec![0.0; stereo().scalar_count(4)];
    let view = producer
        .lend_read_only(&mut storage, 4)
        .expect("4 frames fit a 4-frame buffer");
    consumer
        .verify_view(&view)
        .expect("baseline stream is valid");
    assert_eq!(consumer.frames_consumed(), 4);
}

/// Twin: treats the scalar count (8) as a frame count, requesting 8 frames
/// out of a buffer that holds 4. The capacity oracle must reject it before
/// any out-of-bounds write.
#[test]
fn frame_scalar_confusion_is_killed_by_capacity_oracle() {
    let mut producer = SyntheticProducer::new(stereo());
    let mut storage = vec![0.0; stereo().scalar_count(4)];
    let confused_frames = storage.len(); // bug: 8 scalars misread as 8 frames
    let err = producer
        .lend_read_only(&mut storage, confused_frames)
        .expect_err("8 frames cannot fit 8 scalars of stereo storage");
    assert_eq!(
        err,
        TransferError::DestinationTooSmall {
            required_scalars: 16,
            available_scalars: 8,
        }
    );
}

// ---------------------------------------------------------------------------
// Bug class: malformed payload (trailing partial frame, zero channels)
// ---------------------------------------------------------------------------

/// Baseline: whole-frame payloads construct fine.
#[test]
fn whole_frame_payloads_are_accepted() {
    let data = vec![0.0; stereo().scalar_count(4)];
    let view = PcmView::new(stereo(), &data).expect("8 stereo scalars are 4 whole frames");
    assert_eq!(view.frames(), 4);
}

/// Twin: 5 stereo scalars are 2 frames plus a trailing scalar. Constructing
/// a view must reject the payload — the alternative (silently reporting 2
/// frames) discards a sample that a caller believes it delivered.
#[test]
fn trailing_partial_frame_is_rejected_not_truncated() {
    let data = vec![0.0; 5];
    let err = PcmView::new(stereo(), &data).expect_err("5 stereo scalars are not whole frames");
    assert_eq!(
        err,
        PcmShapeError::TrailingScalar {
            scalars: 5,
            channel_count: 2,
        }
    );
}

/// Twin: a zero-channel format. Scalar-to-frame grouping is undefined, so
/// the format must be unconstructable rather than reporting `frames == 0`
/// for every payload.
#[test]
fn zero_channel_format_is_rejected_at_construction() {
    let err = PcmFormat::new(48_000, 0).expect_err("zero channels cannot group scalars");
    assert_eq!(err, PcmShapeError::ZeroChannelCount);
}

// ---------------------------------------------------------------------------
// Bug class: out-of-range channel/frame access aliasing another sample
// ---------------------------------------------------------------------------

/// Checked access contract: valid last frame and last channel resolve;
/// out-of-range frame or channel return `None`. The critical property is
/// that stereo channel 2 (one past the end) never resolves to
/// `frame 1, channel 0`'s sample.
#[test]
fn out_of_range_access_returns_none_instead_of_aliasing() {
    let data: Vec<Sample> = (0..8).map(|i| i as f32).collect();
    let view = PcmView::new(stereo(), &data).expect("8 stereo scalars are 4 frames");

    // Valid corners resolve, including last frame / last channel.
    assert_eq!(view.sample(0, 0), Some(0.0));
    assert_eq!(view.sample(0, 1), Some(1.0));
    assert_eq!(view.sample(3, 1), Some(7.0));

    // Out-of-range channel must NOT return data[2] (frame 1, channel 0).
    assert_eq!(view.sample(0, 2), None, "channel 2 must not alias frame 1");
    assert_eq!(
        view.sample(1, stereo().channel_count() as usize),
        None,
        "channel == channel_count must not resolve"
    );
    // Out-of-range frame must not alias anything.
    assert_eq!(view.sample(4, 0), None);
    assert_eq!(view.sample(usize::MAX, 0), None);
    assert_eq!(view.sample(0, usize::MAX), None);
}

/// Sensitivity precondition: a consumer that reads channel 0 but credits the
/// value to channel 1 can be caught at all, because the deterministic
/// generator produces different values per channel.
#[test]
fn value_oracle_distinguishes_channels() {
    let mut producer = SyntheticProducer::new(stereo());
    let mut storage = vec![0.0; stereo().scalar_count(4)];
    let view = producer
        .lend_read_only(&mut storage, 4)
        .expect("baseline lend is valid");

    let mislabeled = view.sample(0, 0).expect("in-range access resolves");
    let channel_1_oracle = deterministic_sample(0, 1);
    assert_ne!(
        mislabeled, channel_1_oracle,
        "the oracle must distinguish channels"
    );
}

/// Twin: the channel order inside the payload is corrupted in transit —
/// channels 0 and 1 are physically swapped in every frame. The executed
/// per-(frame, channel) value oracle fires at the first swapped sample; the
/// corruption is never silently accepted.
#[test]
fn channel_order_corruption_is_killed_by_value_oracle() {
    let format = stereo();
    let mut producer = SyntheticProducer::new(format);
    let mut consumer = SyntheticConsumer::new(format);
    let mut storage = vec![0.0; format.scalar_count(2)];

    let mut span = producer
        .lend_in_place(&mut storage, 2)
        .expect("baseline lend is valid");
    // The bug: swap channels 0 and 1 in every frame.
    for f in 0..span.frames() {
        let a = *span.sample_mut(f, 0).expect("in-range sample resolves");
        let b = *span.sample_mut(f, 1).expect("in-range sample resolves");
        *span.sample_mut(f, 0).expect("in-range sample resolves") = b;
        *span.sample_mut(f, 1).expect("in-range sample resolves") = a;
    }
    let view = span.freeze();
    let err = consumer
        .verify_view(&view)
        .expect_err("swapped channel order must fail the value oracle");
    assert!(
        matches!(
            err,
            TransferError::SampleMismatch {
                frame: 0,
                channel: 0,
                ..
            }
        ),
        "expected the first swapped sample to mismatch, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Bug class: in-place processing that never touches the handed-out storage
// ---------------------------------------------------------------------------

/// Baseline: a consumer writes the oracle value back through checked mutable
/// access; the verifier over the frozen span of the same storage still
/// passes (the write path is real but value-preserving).
#[test]
fn in_place_write_through_span_is_visible_in_same_storage() {
    let format = stereo();
    let mut producer = SyntheticProducer::new(format);
    let mut consumer = SyntheticConsumer::new(format);
    let mut storage = vec![0.0; format.scalar_count(4)];

    let mut span = producer
        .lend_in_place(&mut storage, 4)
        .expect("baseline lend is valid");
    let channels = format.channel_count() as usize;
    for f in 0..span.frames() {
        for ch in 0..channels {
            *span
                .sample_mut(f, ch)
                .expect("validated span resolves in-range samples") = deterministic_sample(f, ch);
        }
    }
    let view = span.freeze();
    consumer
        .verify_view(&view)
        .expect("idempotent write verifies");
}

/// Twin: "processing" that only touches a private copy and leaves the
/// handed-out storage unmodified would be invisible — prove the opposite
/// direction too: a real write through the span must be observed by the
/// verifier reading the same storage, at the exact sample it changed.
#[test]
fn in_place_modification_is_observed_by_verifier_at_exact_sample() {
    let format = stereo();
    let mut producer = SyntheticProducer::new(format);
    let mut consumer = SyntheticConsumer::new(format);
    let mut storage = vec![0.0; format.scalar_count(4)];

    let mut span = producer
        .lend_in_place(&mut storage, 4)
        .expect("baseline lend is valid");
    // Processing applies a change at frame 2, channel 1.
    *span.sample_mut(2, 1).expect("in-range sample resolves") += 0.5;
    let view = span.freeze();
    let err = consumer
        .verify_view(&view)
        .expect_err("the modification must be observed through the same storage");
    match err {
        TransferError::SampleMismatch {
            frame,
            channel,
            expected,
            actual,
        } => {
            assert_eq!((frame, channel), (2, 1));
            assert!((actual - expected).abs() - 0.5 < 1e-6);
        }
        other => panic!("expected a sample mismatch at (2, 1), got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Bug class: hidden intermediate storage (copy) on a borrowed path
// ---------------------------------------------------------------------------

/// Baseline: the borrowed push flow hands the consumer the producer's own
/// storage — consumer-observed address equals the producer storage address,
/// so no intermediate storage buffer was inserted. (Storage identity only;
/// this observes nothing about CPU/cache-level data movement.)
#[test]
fn borrowed_baseline_observes_producer_storage_identity() {
    let mut flow = BorrowedReadOnlyFlow::new(stereo(), 4);
    flow.run(16).expect("baseline flow is valid");
    let report = flow.report();
    assert_eq!(
        report.producer_storage_addr, report.consumer_observed_storage_addr,
        "reference path must not insert intermediate storage"
    );
}

/// Twin: an adapter that silently copies the payload into a fresh buffer
/// before handing it on. The *value* oracle still passes (the copy is
/// faithful) — only the pointer-identity oracle observes the inserted
/// storage, which is exactly why pointer identity is part of the evidence.
/// The per-block `to_vec` also makes the copy visible to the counting
/// allocator.
fn borrowed_flow_with_hidden_copy(
    format: PcmFormat,
    total_frames: usize,
    block_frames: usize,
) -> (usize, usize) {
    let mut producer = SyntheticProducer::new(format);
    let mut consumer = SyntheticConsumer::new(format);
    let mut storage = vec![0.0; format.scalar_count(block_frames)];
    let producer_addr = storage.as_ptr() as usize;
    let mut delivered = 0;
    while delivered < total_frames {
        let this_block = block_frames.min(total_frames - delivered);
        let view = producer
            .lend_read_only(&mut storage, this_block)
            .expect("twin lend is valid");
        // The bug: a full-payload copy the caller never asked for.
        let copied: Vec<Sample> = view.payload().to_vec();
        let copied_view = PcmView::new(view.format(), &copied).expect("copy is whole-frame");
        consumer
            .verify_view(&copied_view)
            .expect("values survive the copy — the value oracle alone cannot see it");
        delivered += this_block;
    }
    (
        producer_addr,
        consumer.observed_storage_addr().expect("observed"),
    )
}

#[test]
fn hidden_copy_is_killed_by_pointer_identity() {
    let (producer_addr, consumer_observed) = borrowed_flow_with_hidden_copy(stereo(), 16, 4);
    assert_ne!(
        producer_addr, consumer_observed,
        "storage identity must expose the inserted copy buffer"
    );
}

#[test]
fn hidden_copy_is_killed_by_allocation_measurement() {
    let (_, allocated) =
        crate::run_counting_allocations(|| borrowed_flow_with_hidden_copy(stereo(), 16, 4));
    assert!(
        allocated > 0,
        "the counting allocator must observe the hidden per-block copy \
         (observed {allocated} allocations for 4 blocks)"
    );
}

// ---------------------------------------------------------------------------
// Bug class: per-transfer allocation on a reused-buffer path
// ---------------------------------------------------------------------------

/// Baseline: the honest borrowed flow performs zero heap allocations in its
/// steady-state loop (measured by the counting allocator, not self-reported).
#[test]
fn borrowed_steady_state_is_allocation_free_measured() {
    let mut flow = BorrowedReadOnlyFlow::new(stereo(), 8);
    // Warmup block outside the measurement window.
    flow.run(8).expect("warmup block is valid");
    let (result, allocated) = crate::run_counting_allocations(|| flow.run(64));
    result.expect("steady-state flow is valid");
    assert_eq!(
        allocated, 0,
        "reused-buffer steady state must not heap-allocate (measured {allocated})"
    );
}

/// Twin: the owned shape allocates one block per transfer by design; the
/// allocator must *measure* that cost rather than take it on faith. This is
/// also the sensitivity check proving the allocation measurement reacts to
/// real allocations.
#[test]
fn owned_transfer_cost_is_measured_by_allocator() {
    use super::candidates::OwnedTransferFlow;
    let mut flow = OwnedTransferFlow::new(stereo(), 8);
    flow.run(8).expect("warmup block is valid");
    let (result, allocated) = crate::run_counting_allocations(|| flow.run(64));
    result.expect("owned flow is valid");
    // 8 blocks of 8 frames: at least one allocation per owned block.
    assert!(
        allocated >= 8,
        "owned transfer must show its per-block allocation cost (measured {allocated} \
         allocations for 8 blocks)"
    );
}

// ---------------------------------------------------------------------------
// Bug class: silent format change mid-edge
// ---------------------------------------------------------------------------

/// Baseline: a format change is legal as an explicit new edge — a fresh
/// producer/consumer pair with the new format context.
#[test]
fn explicit_format_boundary_via_new_edge_is_legal() {
    let format_a = PcmFormat::new(44_100, 2).expect("valid");
    let format_b = PcmFormat::new(48_000, 2).expect("valid");
    let mut flow_a = BorrowedReadOnlyFlow::new(format_a, 4);
    flow_a.run(8).expect("edge A is valid");
    let mut flow_b = BorrowedReadOnlyFlow::new(format_b, 4);
    flow_b.run(8).expect("edge B is valid");
}

/// Twin: the producer silently switches format mid-stream while the consumer
/// keeps the old format context. The format-identity oracle rejects the
/// block; the alternative (continuing) would reinterpret samples under a
/// stale format.
#[test]
fn silent_format_change_is_killed_by_format_identity_oracle() {
    let format_a = PcmFormat::new(44_100, 2).expect("valid");
    let format_b = PcmFormat::new(48_000, 2).expect("valid");
    // The formats differ only in sample rate: rate is part of format
    // identity, so a rate change is just as much a format change as a
    // channel-count change.
    assert_ne!(format_a.sample_rate(), format_b.sample_rate());
    assert_eq!(format_a.channel_count(), format_b.channel_count());
    let mut producer = SyntheticProducer::new(format_a);
    let mut consumer = SyntheticConsumer::new(format_a);
    let mut storage = vec![0.0; format_a.scalar_count(4)];

    let view_a = producer
        .lend_read_only(&mut storage, 4)
        .expect("first block is valid");
    consumer.verify_view(&view_a).expect("first block verifies");

    // The bug: same edge, different format, no boundary.
    let mut switched_producer = SyntheticProducer::new(format_b);
    let view_b = switched_producer
        .lend_read_only(&mut storage, 4)
        .expect("lend itself is valid");
    let err = consumer
        .verify_view(&view_b)
        .expect_err("a block with a foreign format must not verify");
    assert!(
        matches!(err, TransferError::FormatMismatch { .. }),
        "got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Bug class: zero-frame payload overloaded as a terminal signal
// ---------------------------------------------------------------------------

/// A stream item: a payload of whole frames, an empty payload, or an
/// explicit out-of-band terminal marker.
enum StreamItem {
    Frames(usize),
    Empty,
    Terminal,
}

/// Honest driver: empty payloads pass through as ordinary data; only the
/// explicit terminal marker ends the stream.
fn run_stream_ignoring_empty(
    plan: &[StreamItem],
    producer: &mut SyntheticProducer,
    consumer: &mut SyntheticConsumer,
    storage: &mut [Sample],
) -> Result<usize, TransferError> {
    let mut delivered = 0;
    for item in plan {
        match item {
            StreamItem::Frames(frames) => {
                let view = producer.lend_read_only(storage, *frames)?;
                consumer.verify_view(&view)?;
                delivered += *frames;
            }
            StreamItem::Empty => {
                let view = producer.lend_read_only(storage, 0)?;
                consumer.verify_view(&view)?;
            }
            StreamItem::Terminal => break,
        }
    }
    Ok(delivered)
}

/// Twin: a driver that treats the first empty payload as end-of-stream.
fn run_stream_stopping_at_empty(
    plan: &[StreamItem],
    producer: &mut SyntheticProducer,
    consumer: &mut SyntheticConsumer,
    storage: &mut [Sample],
) -> Result<usize, TransferError> {
    let mut delivered = 0;
    for item in plan {
        match item {
            StreamItem::Frames(frames) => {
                let view = producer.lend_read_only(storage, *frames)?;
                consumer.verify_view(&view)?;
                delivered += *frames;
            }
            StreamItem::Empty => break, // the bug: empty payload read as EOF
            StreamItem::Terminal => break,
        }
    }
    Ok(delivered)
}

/// Baseline: an empty payload mid-stream is ordinary data; the stream
/// continues and the explicit terminal is a distinct out-of-band marker.
#[test]
fn zero_frame_payload_passes_through_and_terminal_is_out_of_band() {
    let planned = 8usize;
    let plan = [
        StreamItem::Frames(4),
        StreamItem::Empty,
        StreamItem::Frames(4),
        StreamItem::Terminal,
    ];
    let mut producer = SyntheticProducer::new(stereo());
    let mut consumer = SyntheticConsumer::new(stereo());
    let mut storage = vec![0.0; stereo().scalar_count(4)];

    let delivered = run_stream_ignoring_empty(&plan, &mut producer, &mut consumer, &mut storage)
        .expect("honest driver delivers the planned stream");
    assert_eq!(delivered, planned);
    assert_conserved(producer.next_frame(), consumer.frames_consumed())
        .expect("honest driver conserves frames");
}

/// Twin: stopping at the empty payload loses the remainder of the stream —
/// the conservation oracle fires on the twin's real delivered count.
#[test]
fn zero_frame_as_eof_is_killed_by_conservation_oracle() {
    let planned = 8usize;
    let plan = [
        StreamItem::Frames(4),
        StreamItem::Empty,
        StreamItem::Frames(4),
        StreamItem::Terminal,
    ];
    let mut producer = SyntheticProducer::new(stereo());
    let mut consumer = SyntheticConsumer::new(stereo());
    let mut storage = vec![0.0; stereo().scalar_count(4)];

    let delivered = run_stream_stopping_at_empty(&plan, &mut producer, &mut consumer, &mut storage)
        .expect("the twin driver itself does not error");
    let err = assert_conserved(planned, delivered)
        .expect_err("early termination must violate frame conservation");
    assert_eq!(
        err,
        TransferError::FramesLost {
            offered: 8,
            delivered: 4,
        }
    );
}

// ---------------------------------------------------------------------------
// Bug class: partial acceptance that silently drops the remainder
// ---------------------------------------------------------------------------

/// Baseline A: partial acceptance is legal when the accepted/remaining split
/// is explicit — the remainder is re-offered and the stream conserves frames.
#[test]
fn partial_acceptance_with_explicit_remainder_conserves_frames() {
    let mut producer = SyntheticProducer::new(stereo());
    let mut consumer = SyntheticConsumer::new(stereo());
    let mut storage = vec![0.0; stereo().scalar_count(5)];

    let offered = 5usize;
    let capacity = 3usize;
    let accepted = capacity.min(offered);
    let remaining = offered - accepted;

    let view = producer
        .lend_read_only(&mut storage, accepted)
        .expect("accepted block is valid");
    consumer
        .verify_view(&view)
        .expect("accepted block verifies");

    let view = producer
        .lend_read_only(&mut storage, remaining)
        .expect("remainder is re-offered");
    consumer.verify_view(&view).expect("remainder verifies");

    assert_eq!(consumer.frames_consumed(), offered, "frames must conserve");
    assert_conserved(producer.next_frame(), consumer.frames_consumed())
        .expect("partial acceptance with explicit remainder conserves frames");
}

/// Baseline B: all-or-nothing acceptance is equally legal — the producer
/// retries with a smaller block instead of dropping anything.
#[test]
fn all_or_nothing_acceptance_with_retry_conserves_frames() {
    let mut producer = SyntheticProducer::new(stereo());
    let mut consumer = SyntheticConsumer::new(stereo());

    let offered = 5usize;
    let capacity = 3usize;
    let mut delivered = 0;
    while delivered < offered {
        let this_block = capacity.min(offered - delivered);
        let mut storage = vec![0.0; stereo().scalar_count(this_block)];
        let view = producer
            .lend_read_only(&mut storage, this_block)
            .expect("retry block is valid");
        consumer.verify_view(&view).expect("retry block verifies");
        delivered += this_block;
    }
    assert_eq!(consumer.frames_consumed(), offered);
    assert_conserved(producer.next_frame(), consumer.frames_consumed())
        .expect("all-or-nothing retry conserves frames");
}

/// Twin: the producer generates the full offer, delivers only what fits the
/// consumer's capacity, and discards the remainder without any report. The
/// conservation oracle compares the producer's real generation count against
/// the consumer's real verified count and fires.
fn partial_acceptance_that_drops_remainder(
    format: PcmFormat,
    offered: usize,
    capacity: usize,
) -> (usize, usize) {
    let mut producer = SyntheticProducer::new(format);
    let mut consumer = SyntheticConsumer::new(format);
    let mut storage = vec![0.0; format.scalar_count(offered)];

    // The consumer takes only `capacity` frames.
    let view = producer
        .lend_read_only(&mut storage, capacity)
        .expect("accepted block is valid");
    consumer
        .verify_view(&view)
        .expect("accepted block verifies");

    // The bug: the producer's remainder is generated and then silently
    // dropped — never delivered, never reported.
    let remainder_view = producer
        .lend_read_only(&mut storage, offered - capacity)
        .expect("remainder is generated");
    let _ = remainder_view; // discarded without verification: silent loss

    (producer.next_frame(), consumer.frames_consumed())
}

#[test]
fn silent_remainder_drop_is_killed_by_conservation_oracle() {
    let (produced, consumed) = partial_acceptance_that_drops_remainder(stereo(), 5, 3);
    let err = assert_conserved(produced, consumed)
        .expect_err("dropping the remainder must violate conservation");
    assert_eq!(
        err,
        TransferError::FramesLost {
            offered: 5,
            delivered: 3,
        }
    );
}
