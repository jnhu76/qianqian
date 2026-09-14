//! Test-local mechanism doubles standing in at the real ports seams.
//!
//! The production Decode/Output providers are real SongCore and real
//! WASAPI; this file contains NO production code. The doubles exist so
//! the REAL Playback Session composition, lifecycle and completion logic
//! can be exercised on any platform and under adversarial timing. The
//! Windows real-sound gate covers the physical path.

// Shared test support: each test binary uses a subset, so per-binary
// dead-code findings on the unused remainder are expected, not defects.
#![allow(dead_code)]

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// Serializes the lifecycle tests in one binary: the named-thread leak
/// oracle is only valid when no OTHER session in this process can have a
/// leg thread alive. (Other test binaries are separate processes.)
pub fn lifecycle_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

use qianqian_audio_api::ports::{
    AudioOutput, DecodeError, DecodeOpenError, DecodeOutcome, DecodedPcmStream, DrainVerdict,
    OutputError, PcmDecode, PcmFormat, PcmPull, RenderPcmInput, RenderRequest, RenderStream,
};

pub const TEST_FORMAT: PcmFormat = PcmFormat {
    sample_rate: 44100,
    channels: 2,
    channel_mask: 0x3,
};

/// How the fake decode source behaves. Which variants are live differs
/// per test binary sharing this mod, so the enum carries a dead-code
/// allowance for the binaries that exercise a subset.
#[derive(Clone, Copy)]
pub enum SourceBehavior {
    /// Produce `n` frames of payload, then clean EOF.
    EofAfter(usize),
    /// Produce `n` frames, then fail.
    FailAfter(usize),
    /// Produce `n` frames fast, then pace one frame per `delay` — models
    /// a decode side that is slower than the consumer, so the render leg
    /// is genuinely blocked mid-playback on an empty edge.
    Paced { after: usize, delay: Duration },
}

pub struct TestDecode {
    pub behavior: SourceBehavior,
}

impl PcmDecode for TestDecode {
    fn open_media(
        &self,
        _path: &std::path::Path,
    ) -> Result<Box<dyn DecodedPcmStream>, DecodeOpenError> {
        let (n, fail, pace) = match self.behavior {
            SourceBehavior::EofAfter(n) => (n, false, None),
            SourceBehavior::FailAfter(n) => (n, true, None),
            SourceBehavior::Paced { after, delay } => (after, false, Some(delay)),
        };
        Ok(Box::new(TestDecodeStream {
            remaining: n,
            fail_after: fail,
            pace_delay: pace,
            format: TEST_FORMAT,
        }))
    }
}

struct TestDecodeStream {
    remaining: usize,
    fail_after: bool,
    pace_delay: Option<Duration>,
    format: PcmFormat,
}

impl DecodedPcmStream for TestDecodeStream {
    fn format(&self) -> PcmFormat {
        self.format
    }

    fn read_frames(&mut self, dst: &mut [f32]) -> Result<DecodeOutcome, DecodeError> {
        if self.remaining == 0 {
            if self.fail_after {
                return Err(DecodeError {
                    message: "test decode failure".to_owned(),
                });
            }
            if let Some(delay) = self.pace_delay {
                std::thread::sleep(delay);
                self.remaining = 1; // one paced frame per call, forever
            } else {
                return Ok(DecodeOutcome::Eof);
            }
        }
        let channels = usize::from(self.format.channels);
        let n = (dst.len() / channels).min(self.remaining).max(1);
        for s in dst[..n * channels].iter_mut() {
            *s = 0.25;
        }
        self.remaining -= n;
        Ok(DecodeOutcome::Frames(n))
    }
}

/// How the test render leg behaves. (Per-binary usage, see above.)
#[derive(Clone, Copy)]
pub enum OutputBehavior {
    /// Consume the edge to EOF, then report Drained.
    Consume,
    /// Consume with a sleep after every read: the producer genuinely
    /// fills the bounded edge and blocks mid-write long before EOF.
    SlowConsume { per_read: Duration },
    /// Fail at open (device open failure).
    FailOpen,
}

pub struct TestOutput {
    pub behavior: OutputBehavior,
}

impl AudioOutput for TestOutput {
    fn open_stream(&self, request: RenderRequest) -> Result<Box<dyn RenderStream>, OutputError> {
        match self.behavior {
            OutputBehavior::FailOpen => Err(OutputError {
                message: "test device open failure".to_owned(),
            }),
            OutputBehavior::Consume | OutputBehavior::SlowConsume { .. } => {
                let pace = match self.behavior {
                    OutputBehavior::SlowConsume { per_read } => Some(per_read),
                    _ => None,
                };
                let RenderRequest {
                    input,
                    drain,
                    format: _,
                } = request;
                let thread = std::thread::Builder::new()
                    .name("qianqian-test-render".into())
                    .spawn({
                        let input = input.clone();
                        move || {
                            let verdict = consume_loop(input.clone(), pace);
                            if verdict == DrainVerdict::Aborted {
                                // Mirror the real mechanism: a dead render
                                // leg stops the data plane.
                                input.stop();
                            }
                            drain.complete(verdict);
                        }
                    })
                    .map_err(|e| OutputError {
                        message: format!("test render spawn failed: {e}"),
                    })?;
                Ok(Box::new(TestStream {
                    input,
                    thread: Some(thread),
                }))
            }
        }
    }
}

fn consume_loop(input: Arc<dyn RenderPcmInput>, pace: Option<Duration>) -> DrainVerdict {
    let mut dst = vec![0.0f32; 256 * usize::from(TEST_FORMAT.channels)];
    loop {
        match input.read_frames(&mut dst) {
            PcmPull::Frames(_) => {
                if let Some(per_read) = pace {
                    std::thread::sleep(per_read);
                }
            }
            PcmPull::Eof => return DrainVerdict::Drained,
            PcmPull::Stopped => return DrainVerdict::Aborted,
        }
    }
}

struct TestStream {
    input: Arc<dyn RenderPcmInput>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl RenderStream for TestStream {
    fn negotiated_format(&self) -> PcmFormat {
        TEST_FORMAT
    }

    fn stop_and_join(mut self: Box<Self>) {
        self.input.stop();
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

/// Linux-only leak oracle: is any OS thread with this name still alive?
/// Deterministic under the parallel test harness (which inflates raw
/// thread counts with other tests' workers). Only the lifecycle test
/// binary calls it.
///
/// The kernel truncates thread comm names to 15 bytes (`PR_SET_NAME`),
/// so the query is truncated the same way; comparing a longer Builder
/// name against comm would make the check vacuously absent.
#[cfg(target_os = "linux")]
pub fn named_thread_alive(name: &str) -> bool {
    let comm_name = name.get(..15).unwrap_or(name);
    let tasks = std::fs::read_dir("/proc/self/task").expect("/proc/self/task available");
    for entry in tasks.flatten() {
        let comm = std::fs::read_to_string(entry.path().join("comm")).unwrap_or_default();
        if comm.trim_end() == comm_name {
            return true;
        }
    }
    false
}

/// Bounded-poll variant of [`named_thread_alive`] for disposal oracles.
///
/// A successful `join()` already means the worker has terminated — that
/// is the Rust/POSIX lifecycle contract, which this oracle does not
/// restate. Whether the thread's entry has vanished from
/// `/proc/self/task` is a separate, external Linux diagnostic
/// observation with no timing contract; empirically the listing can
/// still show the task right after a successful join on a loaded
/// machine (issue #121). Poll until the name disappears or `limit`
/// elapses so the diagnostic does not report false leaks, while a
/// genuinely running worker never leaves the window. The grace is
/// observation tolerance for the diagnostic, not part of the join
/// contract.
#[cfg(target_os = "linux")]
pub fn named_thread_gone_within(name: &str, limit: Duration) -> bool {
    let deadline = std::time::Instant::now() + limit;
    loop {
        if !named_thread_alive(name) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Run `f` on a watchdog thread that fails the test if it exceeds `limit`.
/// A panic inside `f` is re-raised here unchanged: `Disconnected` on the
/// channel means the body died, not that the limit was exceeded (issue
/// #121 — body panics were being misreported as "operation exceeded 10s").
pub fn within<R>(limit: Duration, f: impl FnOnce() -> R + Send + 'static) -> R
where
    R: Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)));
    });
    match rx.recv_timeout(limit) {
        Ok(Ok(value)) => value,
        Ok(Err(payload)) => std::panic::resume_unwind(payload),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            panic!("operation exceeded {limit:?}")
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            panic!("watchdog body vanished without a panic payload")
        }
    }
}
