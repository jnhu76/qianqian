//! The bounded PCM edge: lifecycle specification for the one data plane
//! the Playback Session owns. Bounded, preallocated in steady state,
//! terminals always unblock both endpoints (first-audible-slice design §4).

#[path = "common/counting_allocator.rs"]
mod counting_allocator;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use qianqian_core::ports::{PcmFrameSource, PcmPull};
use qianqian_playback::{PcmEdge, SessionCompletion, SessionOutcome};

const CHANNELS: u16 = 2;
const CAPACITY_FRAMES: usize = 64;

fn frame(channels: u16, value: f32) -> Vec<f32> {
    vec![value; usize::from(channels)]
}

#[test]
fn write_then_read_roundtrips_frames() {
    let edge = PcmEdge::new(CHANNELS, CAPACITY_FRAMES);
    let mut a = vec![0.25f32; 16 * usize::from(CHANNELS)];
    assert_eq!(edge.write(&mut a), qianqian_playback::WriteOutcome::Written);

    let mut dst = vec![0.0f32; 16 * usize::from(CHANNELS)];
    assert_eq!(edge.read_frames(&mut dst), PcmPull::Frames(16));
    assert!(dst.iter().all(|s| *s == 0.25), "payload survives the edge");
}

#[test]
fn reader_sees_partial_frames_as_whole_frames_only() {
    let edge = PcmEdge::new(CHANNELS, CAPACITY_FRAMES);
    // One and a half frames buffered: only the whole frame is readable.
    let mut samples = frame(CHANNELS, 0.5);
    samples.push(0.5);
    edge.write(&mut samples);
    let mut dst = vec![0.0f32; 8 * usize::from(CHANNELS)];
    assert_eq!(edge.read_frames(&mut dst), PcmPull::Frames(1));
}

#[test]
fn producer_blocks_when_full_and_unblocks_on_consume() {
    let edge = Arc::new(PcmEdge::new(CHANNELS, 4));
    let producer = {
        let edge = edge.clone();
        thread::spawn(move || {
            let big = vec![0.0f32; 10 * usize::from(CHANNELS)];
            edge.write(&big)
        })
    };
    // The producer must be blocked (10 frames into a 4-frame edge).
    thread::sleep(Duration::from_millis(50));
    assert!(!producer.is_finished(), "producer must block on a full edge");

    // The read returns what the edge holds per call; accumulate all 10.
    let mut dst = vec![0.0f32; 16 * usize::from(CHANNELS)];
    let mut total = 0;
    while total < 10 {
        match edge.read_frames(&mut dst) {
            PcmPull::Frames(n) => total += n,
            other => panic!("unexpected pull while draining: {other:?}"),
        }
    }
    assert_eq!(total, 10);
    assert_eq!(producer.join().expect("producer exits"), qianqian_playback::WriteOutcome::Written);
}

#[test]
fn eof_drains_before_terminating_and_stays_terminal() {
    let edge = PcmEdge::new(CHANNELS, CAPACITY_FRAMES);
    edge.write(&mut frame(CHANNELS, 0.1));
    edge.close_eof();

    let mut dst = vec![0.0f32; 8 * usize::from(CHANNELS)];
    assert_eq!(edge.read_frames(&mut dst), PcmPull::Frames(1), "EOF drains buffered frames first");
    assert_eq!(edge.read_frames(&mut dst), PcmPull::Eof, "then reports EOF");
    assert_eq!(edge.read_frames(&mut dst), PcmPull::Eof, "EOF is stable");
}

#[test]
fn torn_remainder_at_eof_terminates_instead_of_wedging() {
    // Regression (adversarial review): a torn frame buffered at close_eof
    // used to block the reader forever behind data it can never consume.
    let edge = PcmEdge::new(CHANNELS, CAPACITY_FRAMES);
    let mut torn = frame(CHANNELS, 0.5);
    torn.push(0.5); // one and a half frames
    edge.write(&mut torn);
    edge.close_eof();

    let mut dst = vec![0.0f32; 8 * usize::from(CHANNELS)];
    assert_eq!(edge.read_frames(&mut dst), PcmPull::Frames(1));
    assert_eq!(
        edge.read_frames(&mut dst),
        PcmPull::Eof,
        "the torn remainder is dropped, never awaited"
    );
}

#[test]
fn stop_unblocks_a_reader_blocked_on_an_empty_edge() {
    let edge = Arc::new(PcmEdge::new(CHANNELS, CAPACITY_FRAMES));
    let reader = {
        let edge = edge.clone();
        thread::spawn(move || {
            let mut dst = vec![0.0f32; 8 * usize::from(CHANNELS)];
            edge.read_frames(&mut dst)
        })
    };
    thread::sleep(Duration::from_millis(50));
    assert!(!reader.is_finished(), "reader must block on an empty edge");
    edge.stop();
    assert_eq!(
        reader.join().expect("reader exits"),
        PcmPull::Stopped,
        "stop unblocks the consumer"
    );
}

#[test]
fn stop_unblocks_a_producer_blocked_on_a_full_edge() {
    let edge = Arc::new(PcmEdge::new(CHANNELS, 4));
    let producer = {
        let edge = edge.clone();
        thread::spawn(move || {
            let big = vec![0.0f32; 10 * usize::from(CHANNELS)];
            edge.write(&big)
        })
    };
    thread::sleep(Duration::from_millis(50));
    assert!(!producer.is_finished());
    edge.stop();
    assert_eq!(
        producer.join().expect("producer exits"),
        qianqian_playback::WriteOutcome::Stopped,
        "stop unblocks the producer"
    );
}

#[test]
fn fail_terminal_stops_the_consumer() {
    let edge = PcmEdge::new(CHANNELS, CAPACITY_FRAMES);
    edge.write(&mut frame(CHANNELS, 0.2));
    edge.fail();
    let mut dst = vec![0.0f32; 8 * usize::from(CHANNELS)];
    assert_eq!(
        edge.read_frames(&mut dst),
        PcmPull::Stopped,
        "a failed producer stops the consumer instead of feeding it"
    );
}

#[test]
fn ring_wraps_without_losing_frames() {
    let edge = Arc::new(PcmEdge::new(CHANNELS, 8));
    // Push a non-power-of-two number of frames through a ring smaller
    // than the stream, with a concurrent producer, and verify order by
    // payload value.
    let producer = {
        let edge = edge.clone();
        thread::spawn(move || {
            for i in 0..17u32 {
                edge.write(&frame(CHANNELS, i as f32));
            }
            edge.close_eof();
        })
    };
    for i in 0..17u32 {
        let mut dst = vec![0.0f32; usize::from(CHANNELS)];
        assert_eq!(edge.read_frames(&mut dst), PcmPull::Frames(1));
        assert!(dst.iter().all(|s| *s == i as f32), "frame {i} came back in order");
    }
    producer.join().expect("producer exits");
    let mut dst = vec![0.0f32; usize::from(CHANNELS)];
    assert_eq!(edge.read_frames(&mut dst), PcmPull::Eof);
}

#[test]
fn steady_state_read_write_performs_zero_allocations() {
    let edge = PcmEdge::new(CHANNELS, CAPACITY_FRAMES);
    // Warm the edge: one full write, one full read.
    let warm = vec![0.0f32; CAPACITY_FRAMES * usize::from(CHANNELS)];
    edge.write(&warm);
    let mut dst = vec![0.0f32; usize::from(CHANNELS)];
    let mut src = vec![0.0f32; usize::from(CHANNELS)];
    edge.read_frames(&mut dst);

    let (_, allocations) = counting_allocator::run_counting_allocations(|| {
        for i in 0..100u32 {
            // Preallocated producer buffer, refilled in place.
            for s in src.iter_mut() {
                *s = i as f32;
            }
            assert_eq!(edge.write(&src), qianqian_playback::WriteOutcome::Written);
            assert_eq!(edge.read_frames(&mut dst), PcmPull::Frames(1));
        }
    });
    assert_eq!(allocations, 0, "the steady-state edge performs no allocation");
}

/// Completion resolution truth: the session outcome combines the worker
/// terminal, the decode-failure record and the render drain verdict.
#[test]
fn completion_resolves_completed_only_from_eof_plus_drained() {
    let completion = SessionCompletion::new();
    // Neither leg has reported: no outcome.
    assert_eq!(completion.try_resolve_now(), None);

    completion.worker_exited(qianqian_playback::EdgeTerminal::Eof);
    assert_eq!(completion.try_resolve_now(), None, "EOF without drain is not completion");

    completion.drain_signal().complete(qianqian_core::ports::DrainVerdict::Drained);
    assert_eq!(completion.try_resolve_now(), Some(SessionOutcome::Completed));
}

#[test]
fn completion_reports_decode_failure_before_any_drain() {
    let completion = SessionCompletion::new();
    completion.decode_failed("corrupt stream");
    assert_eq!(
        completion.try_resolve_now(),
        Some(SessionOutcome::Failed {
            stage: "decode: corrupt stream".to_owned()
        })
    );
}

#[test]
fn completion_reports_device_abort_as_failure() {
    let completion = SessionCompletion::new();
    completion.worker_exited(qianqian_playback::EdgeTerminal::Eof);
    completion.drain_signal().complete(qianqian_core::ports::DrainVerdict::Aborted);
    assert_eq!(
        completion.try_resolve_now(),
        Some(SessionOutcome::Failed { stage: "device".to_owned() }),
        "an abort before drain is a device failure, never a fake completion"
    );
}

#[test]
fn wait_blocks_until_a_leg_publishes() {
    let completion = Arc::new(SessionCompletion::new());
    let waiter = {
        let completion = completion.clone();
        thread::spawn(move || completion.wait())
    };
    thread::sleep(Duration::from_millis(50));
    assert!(!waiter.is_finished(), "wait blocks until the session resolves");
    completion.decode_failed("test failure");
    assert!(matches!(
        waiter.join().expect("waiter exits"),
        SessionOutcome::Failed { .. }
    ));
}
