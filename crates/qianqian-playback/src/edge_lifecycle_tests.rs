//! The bounded PCM edge: lifecycle specification for the one data plane
//! the Playback Session owns. Bounded, preallocated in steady state,
//! terminals always unblock both endpoints (first-audible-slice design §4).

#[path = "counting_allocator.rs"]
pub(crate) mod counting_allocator;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use qianqian_audio_api::ports::{PcmPull, RenderPcmInput};

// White-box: included into the crate by src/lib.rs, so the mechanism
// under test is reached through the crate path, not a public export.
use crate::edge::{EdgeTerminal, PcmEdge};

const CHANNELS: u16 = 2;
const CAPACITY_FRAMES: usize = 64;

/// The production write shape (the decode worker's interruptible loop in
/// session.rs, without the seek observation): bounded `write_some` slices
/// until the edge has taken everything, or `false` when a terminal ended
/// the write. F5 removed the edge's blocking whole-slice write — the
/// steady write must stay interruptible — so this suite exercises exactly
/// the primitives production uses.
fn write_all(edge: &PcmEdge, src: &[f32]) -> bool {
    let mut off = 0usize;
    while off < src.len() {
        let wrote = edge.write_some(&src[off..]);
        off += wrote;
        if wrote == 0 {
            if edge.terminal() != EdgeTerminal::Open {
                return false;
            }
            edge.wait_for_space(Duration::from_millis(1));
        }
    }
    true
}

fn frame(channels: u16, value: f32) -> Vec<f32> {
    vec![value; usize::from(channels)]
}

#[test]
fn write_then_read_roundtrips_frames() {
    let edge = PcmEdge::new(CHANNELS, CAPACITY_FRAMES);
    let a = vec![0.25f32; 16 * usize::from(CHANNELS)];
    assert!(write_all(&edge, &a), "the whole slice fits");

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
    assert!(write_all(&edge, &samples));
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
            write_all(&edge, &big)
        })
    };
    // The producer must be blocked (10 frames into a 4-frame edge).
    thread::sleep(Duration::from_millis(50));
    assert!(
        !producer.is_finished(),
        "producer must block on a full edge"
    );

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
    assert!(
        producer.join().expect("producer exits"),
        "the whole slice is eventually accepted"
    );
}

#[test]
fn eof_drains_before_terminating_and_stays_terminal() {
    let edge = PcmEdge::new(CHANNELS, CAPACITY_FRAMES);
    assert!(write_all(&edge, &frame(CHANNELS, 0.1)));
    edge.close_eof();

    let mut dst = vec![0.0f32; 8 * usize::from(CHANNELS)];
    assert_eq!(
        edge.read_frames(&mut dst),
        PcmPull::Frames(1),
        "EOF drains buffered frames first"
    );
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
    assert!(write_all(&edge, &torn));
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
            write_all(&edge, &big)
        })
    };
    thread::sleep(Duration::from_millis(50));
    assert!(!producer.is_finished());
    edge.stop();
    assert!(
        !producer.join().expect("producer exits"),
        "stop unblocks the producer"
    );
}

#[test]
fn fail_terminal_stops_the_consumer() {
    let edge = PcmEdge::new(CHANNELS, CAPACITY_FRAMES);
    assert!(write_all(&edge, &frame(CHANNELS, 0.2)));
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
                assert!(write_all(&edge, &frame(CHANNELS, i as f32)));
            }
            edge.close_eof();
        })
    };
    for i in 0..17u32 {
        let mut dst = vec![0.0f32; usize::from(CHANNELS)];
        assert_eq!(edge.read_frames(&mut dst), PcmPull::Frames(1));
        assert!(
            dst.iter().all(|s| *s == i as f32),
            "frame {i} came back in order"
        );
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
    assert!(write_all(&edge, &warm));
    let mut dst = vec![0.0f32; usize::from(CHANNELS)];
    let mut src = vec![0.0f32; usize::from(CHANNELS)];
    edge.read_frames(&mut dst);

    let (_, allocations) = counting_allocator::run_counting_allocations(|| {
        for i in 0..100u32 {
            // Preallocated producer buffer, refilled in place.
            for s in src.iter_mut() {
                *s = i as f32;
            }
            assert_eq!(edge.write_some(&src), usize::from(CHANNELS));
            assert_eq!(edge.read_frames(&mut dst), PcmPull::Frames(1));
        }
    });
    assert_eq!(
        allocations, 0,
        "the steady-state edge performs no allocation"
    );
}
