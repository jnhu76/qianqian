//! The mock decode worker — the production `decode_worker` shape
//! (product `session.rs`): refill a staging buffer from a source,
//! `write` into the bounded edge, exit on Stopped, commit EOF, or
//! publish a failure. Behavior variants model the source, not the
//! worker.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::edge::{EdgeTerminal, PcmEdge, WriteOutcome};
use crate::events::{Event, Log};

/// How the fake source behaves (test-harness `SourceBehavior` shape).
#[derive(Clone, Copy)]
pub enum Source {
    /// `total_frames` frames of payload, then clean EOF.
    EofAfter(usize),
    /// `total_frames` frames, then fail.
    FailAfter(usize),
    /// `total_frames` frames then EOF, throttled to one chunk per
    /// `delay` once `after` frames are written — a producer slower than
    /// the consumer, so the render leg genuinely blocks mid-playback on
    /// an empty edge.
    Paced {
        total: usize,
        after: usize,
        delay: Duration,
    },
}

/// Exit terminal the worker reported (evidence for the oracle).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerExit {
    Eof,
    Failed,
    Stopped,
}

/// Spawn the producer thread. `write_in_flight` is set around every
/// `edge.write` call so the oracle can observe "producer blocked
/// through bounded backpressure" (in-flight && no progress while the
/// edge is full).
pub fn spawn_worker(
    source: Source,
    edge: Arc<PcmEdge>,
    chunk_frames: usize,
    channels: u16,
    log: Log,
    write_in_flight: Arc<AtomicBool>,
) -> std::thread::JoinHandle<WorkerExit> {
    std::thread::Builder::new()
        .name("sim-decode".into())
        .spawn(move || {
            let channels = usize::from(channels);
            let staging = vec![0.5f32; chunk_frames * channels];
            let mut written = 0usize;
            loop {
                let total = match source {
                    Source::EofAfter(t) | Source::FailAfter(t) => t,
                    Source::Paced { total, .. } => total,
                };
                if written >= total {
                    break match source {
                        Source::FailAfter(_) => {
                            edge.fail();
                            log.push(Event::ProducerFailed);
                            WorkerExit::Failed
                        }
                        _ => {
                            edge.close_eof();
                            log.push(Event::ProducerEof);
                            debug_assert_eq!(edge.terminal(), EdgeTerminal::Eof);
                            WorkerExit::Eof
                        }
                    };
                }
                if let Source::Paced { after, delay, .. } = source {
                    if written >= after {
                        std::thread::sleep(delay);
                    }
                }
                let take = (total - written).min(chunk_frames);
                log.push(Event::ProducerWriteEnter);
                write_in_flight.store(true, Ordering::Release);
                let outcome = edge.write(&staging[..take * channels]);
                write_in_flight.store(false, Ordering::Release);
                log.push(Event::ProducerWriteExit(outcome == WriteOutcome::Written));
                match outcome {
                    WriteOutcome::Written => written += take,
                    WriteOutcome::Stopped => break WorkerExit::Stopped,
                }
            }
        })
        .expect("worker spawn")
}
