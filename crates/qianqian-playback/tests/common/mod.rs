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

use std::sync::{Arc, Condvar, Mutex, OnceLock};
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
    AudioOutput, DecodeError, DecodeOpenError, DecodeOutcome, DecodedPcmStream, DrainSignal,
    DrainVerdict, OutputError, PcmDecode, PcmFormat, PcmPull, RenderPcmInput, RenderRequest,
    RenderStream,
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
    /// Consume `after_reads` blocks, then abort mid-stream — models a
    /// real device failure/invalidation where the render loop exits on
    /// its own, stops the data plane, and reports the drain aborted. No
    /// stop was requested; this is how a genuine device abort reaches
    /// the completion resolver.
    AbortMidStream { after_reads: usize },
    /// Open never succeeds: the render leg is spawned, parks at the
    /// pause gate (routed pause intent must already sit there), is
    /// PROVEN parked through the armed tail probe, then the provider
    /// aborts it through the open-abort protocol — permanent gate
    /// close, data-plane stop, join — and returns the open failure.
    /// Mirrors the real WASAPI open-timeout abort
    /// (`open_abort::abort_render_thread`): the stream never becomes a
    /// session episode, its gate can never park again, and no decode
    /// worker exists yet (activation never got that far).
    OpenTimeoutAbort,
}

pub struct TestOutput {
    pub behavior: OutputBehavior,
    /// Test-local mechanism evidence: how many reads returned frames.
    /// A fast consumer keeps the bounded edge empty almost always, so
    /// `buffered_frames == 0` alone cannot witness that the episode
    /// really produced audio; this counter can.
    pub consumed: Arc<std::sync::atomic::AtomicUsize>,
    /// The mock device's output-tail occupancy (frames already consumed
    /// but still queued to "play"), observed by the render gate's
    /// tail-quiescence check. An instant-consuming mock holds nothing,
    /// so the default is quiesced (false = empty tail); a test flips it
    /// to true to model a real device still playing out already-
    /// submitted frames.
    pub device_tail_padding: Arc<std::sync::atomic::AtomicBool>,
    /// Controllable hold on the render leg's tail observation; see
    /// [`TailProbe`]. Unarmed by default, so it costs nothing.
    pub tail_probe: TailProbe,
    /// Open-abort witness (`OpenTimeoutAbort` only): set after the
    /// spawned leg was proven parked at the pause gate (its armed tail
    /// observation is held — the gate only calls it from inside a park)
    /// and before the open-abort closes, releases and joins the leg.
    /// The caller owns the flag so the test can pin the engagement
    /// precondition of the never-activated oracle.
    pub open_abort_engaged: Arc<std::sync::atomic::AtomicBool>,
}

impl TestOutput {
    pub fn new(behavior: OutputBehavior) -> TestOutput {
        Self::observed(behavior, Arc::new(std::sync::atomic::AtomicUsize::new(0)))
    }

    /// The caller keeps the consumption counter for witness purposes.
    pub fn observed(
        behavior: OutputBehavior,
        consumed: Arc<std::sync::atomic::AtomicUsize>,
    ) -> TestOutput {
        TestOutput::observed_with_tail(
            behavior,
            consumed,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    }

    /// [`TestOutput::observed`] with the caller owning the mock device's
    /// output-tail flag, so a pause test can model a real device whose
    /// already-submitted frames are still queued to play.
    pub fn observed_with_tail(
        behavior: OutputBehavior,
        consumed: Arc<std::sync::atomic::AtomicUsize>,
        device_tail_padding: Arc<std::sync::atomic::AtomicBool>,
    ) -> TestOutput {
        TestOutput {
            behavior,
            consumed,
            device_tail_padding,
            tail_probe: TailProbe::default(),
            open_abort_engaged: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// [`TestOutput::observed_with_tail`] plus the caller-owned
    /// open-abort engagement witness, for
    /// [`OutputBehavior::OpenTimeoutAbort`].
    pub fn open_timeout_abort(
        consumed: Arc<std::sync::atomic::AtomicUsize>,
        device_tail_padding: Arc<std::sync::atomic::AtomicBool>,
        tail_probe: TailProbe,
        open_abort_engaged: Arc<std::sync::atomic::AtomicBool>,
    ) -> TestOutput {
        TestOutput {
            behavior: OutputBehavior::OpenTimeoutAbort,
            consumed,
            device_tail_padding,
            tail_probe,
            open_abort_engaged,
        }
    }
}

/// Controllable hold on the render leg's tail observation. A slow — but
/// finite — tail observation is a legal mechanism execution (the real
/// observation is a `GetCurrentPadding` system call with no contractual
/// duration bound), so holding the leg inside its park loop's
/// observation makes "the leg cannot reach its released check" a stable
/// state instead of a race. Unarmed probes pass through.
///
/// (The gate calls the observation only until quiescence is published
/// for the engagement, so the deterministic hold is the engagement's
/// FIRST armed observation — before its quiescence, if any.)
#[derive(Clone, Default)]
pub struct TailProbe {
    armed: Arc<std::sync::atomic::AtomicBool>,
    held_entered: Arc<(Mutex<bool>, Condvar)>,
    hold_released: Arc<(Mutex<bool>, Condvar)>,
}

impl TailProbe {
    /// Hold every armed observation (the first one signals and blocks).
    pub fn arm(&self) {
        self.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Block until an observation is provably held inside the gate.
    pub fn wait_held(&self) {
        let mut held = self.held_entered.0.lock().unwrap();
        while !*held {
            held = self.held_entered.1.wait(held).unwrap();
        }
    }

    /// Bounded [`TailProbe::wait_held`]: false if no observation was
    /// held within `limit`.
    pub fn wait_held_within(&self, limit: Duration) -> bool {
        let deadline = std::time::Instant::now() + limit;
        let mut held = self.held_entered.0.lock().unwrap();
        loop {
            if *held {
                return true;
            }
            let now = std::time::Instant::now();
            if now >= deadline {
                return false;
            }
            let (guard, _) = self
                .held_entered
                .1
                .wait_timeout(held, deadline - now)
                .unwrap();
            held = guard;
        }
    }

    /// Release the held observation.
    pub fn unhold(&self) {
        let mut released = self.hold_released.0.lock().unwrap();
        *released = true;
        self.hold_released.1.notify_all();
    }

    fn observe(&self, quiescent: bool) -> bool {
        if self.armed.load(std::sync::atomic::Ordering::SeqCst) {
            {
                // Signal and drop before blocking: the held mutex must
                // never be held across the hold wait, or the waiter in
                // `wait_held` cannot re-acquire it after its wake.
                let mut held = self.held_entered.0.lock().unwrap();
                *held = true;
                self.held_entered.1.notify_all();
            }
            let mut released = self.hold_released.0.lock().unwrap();
            while !*released {
                released = self.hold_released.1.wait(released).unwrap();
            }
        }
        quiescent
    }
}

/// Spawn the mock render leg shared by the streaming behaviors: the
/// loop-top pause gate before every read, a panic still publishing a
/// verdict and stopping the data plane, and the drain verdict published
/// on exit — mirroring the real mechanism's leg posture (wasapi.rs
/// run_render_thread).
fn spawn_test_leg(
    input: Arc<dyn RenderPcmInput>,
    gate: qianqian_audio_api::ports::RenderGate,
    drain: DrainSignal,
    pace: Option<Duration>,
    abort_after: Option<usize>,
    output: &TestOutput,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    let consumed = output.consumed.clone();
    let device_tail_padding = output.device_tail_padding.clone();
    let tail_probe = output.tail_probe.clone();
    std::thread::Builder::new()
        .name("qianqian-test-render".into())
        .spawn(move || {
            let verdict = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                consume_loop(
                    input.clone(),
                    &gate,
                    pace,
                    abort_after,
                    &consumed,
                    &device_tail_padding,
                    &tail_probe,
                )
            }))
            .unwrap_or(DrainVerdict::Aborted);
            if verdict == DrainVerdict::Aborted {
                // Mirror the real mechanism: a dead render leg stops
                // the data plane.
                input.stop();
            }
            drain.complete(verdict);
        })
}

impl AudioOutput for TestOutput {
    fn open_stream(&self, request: RenderRequest) -> Result<Box<dyn RenderStream>, OutputError> {
        match self.behavior {
            OutputBehavior::FailOpen => Err(OutputError {
                message: "test device open failure".to_owned(),
            }),
            OutputBehavior::OpenTimeoutAbort => {
                let RenderRequest {
                    input,
                    drain,
                    gate,
                    format: _,
                } = request;
                let thread = spawn_test_leg(input.clone(), gate.clone(), drain, None, None, self)
                    .map_err(|e| OutputError {
                    message: format!("test render spawn failed: {e}"),
                })?;
                // The pre-activation pause intent must already sit at
                // the gate: the leg parks at its first loop-top check,
                // and its armed tail observation held inside the park
                // loop is the deterministic park proof (the gate only
                // calls the observation from inside a park, after the
                // engagement ack).
                assert!(
                    self.tail_probe.wait_held_within(Duration::from_secs(5)),
                    "the OpenTimeoutAbort leg never parked at the gate: \
                     route pause intent before activating"
                );
                self.open_abort_engaged
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                // The open has now "timed out": abort through the
                // open-abort protocol — permanent gate close first, then
                // the data-plane stop, then the join (the
                // `abort_render_thread` order). The stream never becomes
                // a session episode: no decode worker exists yet, so the
                // aborted drain verdict alone can never settle a
                // terminal Fact (D11 activation firewall).
                gate.close_and_release();
                self.tail_probe.unhold();
                input.stop();
                let _ = thread.join();
                Err(OutputError {
                    message: "test open timeout abort".to_owned(),
                })
            }
            OutputBehavior::Consume
            | OutputBehavior::SlowConsume { .. }
            | OutputBehavior::AbortMidStream { .. } => {
                let (pace, abort_after) = match self.behavior {
                    OutputBehavior::SlowConsume { per_read } => (Some(per_read), None),
                    OutputBehavior::AbortMidStream { after_reads } => (None, Some(after_reads)),
                    _ => (None, None),
                };
                let RenderRequest {
                    input,
                    drain,
                    gate,
                    format: _,
                } = request;
                let thread = spawn_test_leg(input.clone(), gate, drain, pace, abort_after, self)
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

fn consume_loop(
    input: Arc<dyn RenderPcmInput>,
    gate: &qianqian_audio_api::ports::RenderGate,
    pace: Option<Duration>,
    abort_after: Option<usize>,
    consumed: &std::sync::atomic::AtomicUsize,
    device_tail_padding: &std::sync::atomic::AtomicBool,
    tail_probe: &TailProbe,
) -> DrainVerdict {
    use std::sync::atomic::Ordering;
    let mut dst = vec![0.0f32; 256 * usize::from(TEST_FORMAT.channels)];
    let mut reads = 0usize;
    loop {
        if abort_after.is_some_and(|limit| reads >= limit) {
            // Device died mid-stream: the render loop exits on its own.
            // The caller mirrors the real mechanism by stopping the data
            // plane before completing the drain.
            return DrainVerdict::Aborted;
        }
        // Mirror the real mechanism's loop-top pause gate (D14.7): the
        // gate parks before the read; the mock's tail observation is
        // its own device-tail counter, passable through the probe.
        gate.park_while_paused(|| tail_probe.observe(!device_tail_padding.load(Ordering::SeqCst)));
        match input.read_frames(&mut dst) {
            PcmPull::Frames(n) => {
                consumed.fetch_add(n, Ordering::SeqCst);
                reads += 1;
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
