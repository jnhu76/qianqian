//! Observation tap oracles (#187 O1). The tap is a bounded, lossy,
//! non-authoritative read-only branch off the decode worker's staging
//! seam; these oracles pin the load-bearing mechanism invariants only:
//! bounded/latest-wins delivery, producer contention dropping instead
//! of waiting, the Applied-cut reset at the seam, off-path worker
//! lifecycle, and teardown that cannot require consumer progress. They
//! are not playback-semantics claims; the existing seek/DSP/edge
//! suites own those.

use crate::observation::ObservationTap;
use crate::test_common::{TEST_FORMAT, within};
use std::time::{Duration, Instant};

#[test]
fn offered_block_is_delivered_once_as_latest_truth() {
    let tap = ObservationTap::new(TEST_FORMAT, 1024);
    // One whole staging block: 1024 frames × 2 channels.
    let block: Vec<f32> = (0..2048).map(|i| i as f32 * 0.5).collect();
    tap.offer(&block);

    let mut dst = Vec::new();
    assert_eq!(tap.take_into(&mut dst), Some((1024, false)));
    assert_eq!(dst, block, "the delivered block is the exact offered PCM");
    assert_eq!(
        tap.take_into(&mut dst),
        None,
        "delivery is once, not a queue"
    );
}

#[test]
fn slow_consumer_loses_intermediate_blocks_and_storage_stays_fixed() {
    let tap = ObservationTap::new(TEST_FORMAT, 1024);
    // A "slow" consumer (none at all): every offer still succeeds
    // immediately — overwrite has no full state — and only the latest
    // block survives. Storage is the one fixed slot; 10_000 offers do
    // not grow it.
    for n in 0..10_000u32 {
        tap.offer(&[n as f32; 2048]);
    }
    let mut dst = Vec::new();
    assert_eq!(tap.take_into(&mut dst), Some((1024, false)));
    assert_eq!(dst[0], 9999.0, "latest wins");
    assert!(dst.iter().all(|&s| s == 9999.0), "no torn block");
    // The slot keeps serving after the overwrite storm.
    tap.offer(&[7.0; 2048]);
    assert_eq!(tap.take_into(&mut dst), Some((1024, false)));
    assert_eq!(dst[0], 7.0);
}

#[test]
fn offer_contended_by_the_analyst_drops_instead_of_waiting() {
    let tap = ObservationTap::new(TEST_FORMAT, 1024);
    let block = vec![0.25f32; 2048];
    // Hold the slot exactly like the analyst's mid-copy-out critical
    // section: a blocking offer would self-deadlock on this guard and
    // hang the run; the try_lock policy drops the block and returns
    // while the hold is still active.
    tap.run_with_slot_locked(|| tap.offer(&block));
    let mut dst = Vec::new();
    assert_eq!(
        tap.take_into(&mut dst),
        None,
        "a contended offer drops the block instead of queueing it"
    );
    assert_eq!(tap.latest().delivered_blocks, 0);
}

#[test]
fn applied_cut_drops_pending_precut_material_and_flags_the_next_block() {
    let tap = ObservationTap::new(TEST_FORMAT, 1024);
    let mut dst = Vec::new();

    // Pre-cut block pending delivery when the Applied cut lands.
    tap.offer(&[1.0; 2048]);
    tap.invalidate();
    assert_eq!(
        tap.take_into(&mut dst),
        None,
        "pre-cut pending material cannot survive the cut"
    );

    // The boundary rides exactly the first post-cut block.
    tap.offer(&[2.0; 2048]);
    assert_eq!(
        tap.take_into(&mut dst),
        Some((1024, true)),
        "the cut boundary is visible to the consumer"
    );
    tap.offer(&[3.0; 2048]);
    assert_eq!(
        tap.take_into(&mut dst),
        Some((1024, false)),
        "the bit is consumed by one delivery: continuation is not a cut"
    );

    // Invalidate with nothing pending still arms the boundary: the
    // reset is not coupled to drop timing.
    tap.invalidate();
    tap.offer(&[4.0; 2048]);
    assert_eq!(tap.take_into(&mut dst), Some((1024, true)));
}

#[test]
fn analyst_consumes_off_path_and_publishes_the_probe_record() {
    let tap = ObservationTap::new(TEST_FORMAT, 1024);
    // The first block lands before the analyst exists, so at least one
    // delivery is deterministic under the drop-on-contention policy;
    // the second may contend and drop (loss is normal).
    tap.offer(&[1.0f32; 2048]);
    let worker = tap
        .spawn_worker()
        .expect("the off-path analyst spawns with the tap");
    tap.offer(&[2.0f32; 2048]);

    // Bounded poll: delivery happens on the analyst's own path.
    let deadline = Instant::now() + Duration::from_secs(5);
    let latest = loop {
        let latest = tap.latest();
        if latest.delivered_blocks >= 1 {
            break latest;
        }
        assert!(Instant::now() < deadline, "the analyst never consumed");
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(latest.sample_rate, TEST_FORMAT.sample_rate);
    assert_eq!(latest.channels, TEST_FORMAT.channels);
    assert_eq!(latest.cuts, 0, "no cut signal, no cut evidence");
    // One or two deliveries are both legal (loss is normal); the
    // accounting must be self-consistent.
    assert!(
        (1..=2).contains(&latest.delivered_blocks),
        "delivered {latest:?}"
    );
    assert_eq!(latest.delivered_frames, 1024 * latest.delivered_blocks);

    tap.close();
    within(Duration::from_secs(5), move || {
        worker.join().expect("the analyst exits cleanly");
    });
    assert!(tap.latest().worker_closed);
}

#[test]
fn teardown_never_requires_consumer_progress_or_production() {
    // A block pending at close: the join completes without further
    // production, and the pending block is delivered, not silently lost.
    // The offer precedes the analyst so the pending state cannot race
    // the analyst's startup window under the drop-on-contention policy.
    let tap = ObservationTap::new(TEST_FORMAT, 1024);
    tap.offer(&[1.0f32; 2048]);
    let worker = tap.spawn_worker().expect("analyst spawn");
    tap.close();
    within(Duration::from_secs(5), move || {
        worker.join().expect("the analyst exits on close");
    });
    let latest = tap.latest();
    assert!(latest.worker_closed);
    assert_eq!(
        latest.delivered_blocks, 1,
        "a pending block is delivered before exit"
    );

    // Nothing pending: close alone ends the analyst.
    let tap = ObservationTap::new(TEST_FORMAT, 1024);
    let worker = tap.spawn_worker().expect("analyst spawn");
    tap.close();
    within(Duration::from_secs(5), move || {
        worker.join().expect("the analyst exits on close");
    });
    assert!(tap.latest().worker_closed);
}
