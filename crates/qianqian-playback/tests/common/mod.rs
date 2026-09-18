//! Test-local mechanism doubles standing in at the real ports seams.
//!
//! The production Decode/Output providers are real SongCore and real
//! WASAPI; this file contains NO production code. The doubles exist so
//! the REAL Playback Session composition, lifecycle and completion logic
//! can be exercised on any platform and under adversarial timing. The
//! Windows real-sound gate covers the physical path.
//!
//! The mock render leg mirrors the real loop's D14.8 accounting (publish
//! at every tail observation from the pre-submission total, credit only
//! after the mock's submission succeeds) and, since F5, the real loop's
//! seek shape (D14.5): the loop-top pause park followed by the
//! cut-attributed seek park whose released payload rebases the stretch
//! basis on the leg's own path (including the payload-awaits-consumption
//! case of a cut committed while pause-parked) — exactly the
//! steady_loop posture in wasapi.rs. The mock decode source
//! position-tags every frame (sample value = absolute frame index) so
//! tests can assert content continuity across a cutover without knowing
//! where the cut landed.
//!
//! Two things the mock structurally cannot witness, so no test here may
//! claim them: a FAILED device submission (the mock has no failing
//! ReleaseBuffer path — that rests on the order oracle in
//! `qianqian-output-wasapi`) and FRAME UNITS (the mock is unit-agnostic
//! — the real unit rule is a property of the WASAPI stream negotiation).

// Shared test support: each test binary uses a subset, so per-binary
// dead-code findings on the unused remainder are expected, not defects.
#![allow(dead_code)]

use std::collections::VecDeque;
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
    DrainVerdict, OutputError, PcmDecode, PcmFormat, PcmPull, PositionEvidence, ProviderSeekOutcome,
    RenderGate, RenderPcmInput, RenderRequest, RenderStream, SeekParkOutcome, SeekParkRelease,
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

#[derive(Clone)]
pub struct TestDecode {
    pub behavior: SourceBehavior,
    /// Source duration evidence the probe reports (D14.8). `None` is the
    /// provider's unknown path — the mock's default, so an episode built
    /// without saying otherwise observes no duration, exactly like a
    /// container that declares none.
    pub duration: Option<Duration>,
    /// Scripted provider seek outcomes (D14.5), consumed in order by the
    /// endpoint's `seek`; an empty script lands exactly at the requested
    /// target (clamped to the source's frame total). Script entries are
    /// taken verbatim — a scripted `Applied { landing }` need not match
    /// the requested target, which is how tests prove the Position
    /// rebases to the ACTUAL landing, never the request.
    pub seeks: Vec<ProviderSeekOutcome>,
}

impl TestDecode {
    /// A decode double whose probe reports no duration (unknown).
    pub fn new(behavior: SourceBehavior) -> Self {
        Self {
            behavior,
            duration: None,
            seeks: Vec::new(),
        }
    }

    /// A decode double whose probe reports `duration` as source
    /// evidence.
    pub fn with_duration(behavior: SourceBehavior, duration: Duration) -> Self {
        Self {
            behavior,
            duration: Some(duration),
            seeks: Vec::new(),
        }
    }
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
            total: n as u64,
            cursor: 0,
            fail_after: fail,
            pace_delay: pace,
            format: TEST_FORMAT,
            duration: self.duration,
            seek_script: self.seeks.clone().into(),
        }))
    }
}

/// `target` expressed in this source's frame units (the episode format's
/// sample rate) — the mock's stand-in for the provider's own
/// media-time→frame decision.
fn target_frames(target: Duration, sample_rate: u32) -> u64 {
    ((target.as_micros() * u64::from(sample_rate) as u128) / 1_000_000) as u64
}

struct TestDecodeStream {
    /// Total frames of fast (unpaced) source content.
    total: u64,
    /// The next frame this endpoint will produce. Every sample of frame
    /// `i` carries the value `i as f32`, so the consumed-content oracle
    /// can assert continuity across a seek cutover without knowing where
    /// the cut landed.
    cursor: u64,
    fail_after: bool,
    pace_delay: Option<Duration>,
    format: PcmFormat,
    duration: Option<Duration>,
    seek_script: VecDeque<ProviderSeekOutcome>,
}

impl DecodedPcmStream for TestDecodeStream {
    fn format(&self) -> PcmFormat {
        self.format
    }

    fn source_duration(&self) -> Option<Duration> {
        self.duration
    }

    fn read_frames(&mut self, dst: &mut [f32]) -> Result<DecodeOutcome, DecodeError> {
        let avail = self.total.saturating_sub(self.cursor);
        let n = if avail == 0 {
            if self.fail_after {
                return Err(DecodeError {
                    message: "test decode failure".to_owned(),
                });
            }
            if let Some(delay) = self.pace_delay {
                std::thread::sleep(delay);
                1 // one paced frame per call, forever, past the fast total
            } else {
                return Ok(DecodeOutcome::Eof);
            }
        } else {
            let channels = usize::from(self.format.channels);
            (dst.len() / channels).min(avail as usize)
        };
        let channels = usize::from(self.format.channels);
        for f in 0..n {
            let value = (self.cursor + f as u64) as f32;
            for s in dst[f * channels..(f + 1) * channels].iter_mut() {
                *s = value;
            }
        }
        self.cursor += n as u64;
        Ok(DecodeOutcome::Frames(n))
    }

    fn seek(&mut self, target: Duration) -> ProviderSeekOutcome {
        let outcome = match self.seek_script.pop_front() {
            Some(scripted) => scripted,
            // Unscripted default: the well-behaved provider — land
            // exactly at the requested target, clamped to the source.
            None => ProviderSeekOutcome::Applied {
                landing: Some(target_frames(target, self.format.sample_rate).min(self.total)),
            },
        };
        match &outcome {
            ProviderSeekOutcome::Applied { landing } => {
                // The endpoint's cursor moves to the landing the provider
                // REPORTS (an unknown landing produces from around the
                // requested target — the content continues; only the
                // Position projection is withdrawn).
                self.cursor = match landing {
                    Some(landing) => (*landing).min(self.total),
                    None => target_frames(target, self.format.sample_rate).min(self.total),
                };
            }
            // Proven pre-mutation: the cursor is untouched.
            ProviderSeekOutcome::RefusedUnchanged => {}
            // Destructive: the worker never reads again, so the cursor
            // value is irrelevant — leave it untouched.
            ProviderSeekOutcome::MutatedThenFailed { .. } => {}
        }
        outcome
    }
}

/// How the mock output device plays out the tail it holds between two
/// observations of it — the mock's only clock. Frames enter the tail on
/// submission and leave it at this rate, so a leg that submits faster
/// than the rate accumulates a queue, and a leg that stops submitting
/// (a parked pause) watches it drain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Playout {
    /// The device plays out everything it holds before the next
    /// observation: it never accumulates a queue. This is the default —
    /// the `Consume` behavior's "instant consumer", the mock device that
    /// holds nothing.
    Everything,
    /// The device plays out at most this many frames per observation,
    /// and nothing at all for `0` (a device frozen mid-buffer, or one
    /// whose queue outlives the observation window).
    FramesPerObservation(u64),
}

/// The mock device's queued-to-play tail, by the frame — the same
/// physical quantity the real mechanism reads with `GetCurrentPadding`
/// and the leg's position evidence subtracts (D14.8). One model, two
/// uses: the F3 establishment oracle asks whether it is quiesced
/// (`== 0`), and the F4 accounting derives from its value.
#[derive(Clone)]
pub struct DeviceTail {
    queued: Arc<std::sync::atomic::AtomicU64>,
    playout: Arc<std::sync::atomic::AtomicU64>,
}

/// The internal spelling of [`Playout::Everything`]: a rate no queue
/// length can exceed.
const PLAY_OUT_EVERYTHING: u64 = u64::MAX;

impl Default for DeviceTail {
    fn default() -> Self {
        Self {
            queued: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            playout: Arc::new(std::sync::atomic::AtomicU64::new(PLAY_OUT_EVERYTHING)),
        }
    }
}

impl DeviceTail {
    /// Change the mock device's playout rate (see [`Playout`]).
    pub fn set_playout(&self, playout: Playout) {
        let rate = match playout {
            Playout::Everything => PLAY_OUT_EVERYTHING,
            Playout::FramesPerObservation(frames) => frames,
        };
        self.playout
            .store(rate, std::sync::atomic::Ordering::SeqCst);
    }

    /// The frames currently queued to play — a witness for assertions,
    /// never an input to any product path.
    pub fn queued(&self) -> u64 {
        self.queued.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Put `frames` into the queue as if the leg had submitted them.
    /// (The leg's own submissions go through [`DeviceTail::submit`].)
    pub fn seed(&self, frames: u64) {
        self.queued
            .fetch_add(frames, std::sync::atomic::Ordering::SeqCst);
    }

    /// One tail observation: the device plays out its slice, then
    /// reports what is still queued.
    fn observe(&self) -> u64 {
        use std::sync::atomic::Ordering;
        let rate = self.playout.load(Ordering::SeqCst);
        let mut remaining = self.queued.load(Ordering::SeqCst);
        if rate >= remaining {
            remaining = 0;
        } else {
            remaining -= rate;
        }
        self.queued.store(remaining, Ordering::SeqCst);
        remaining
    }

    /// The device accepted `n` submitted frames into its queue (the
    /// mock's `ReleaseBuffer(n)` acceptance).
    fn submit(&self, n: u64) {
        self.queued
            .fetch_add(n, std::sync::atomic::Ordering::SeqCst);
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
    /// The consumed-content witness (F5): the channel-0 sample value of
    /// every frame the mock leg successfully submitted, in submission
    /// order. The position-tagged decode double makes this a frame-index
    /// sequence, so a committed cutover must appear as exactly one
    /// discontinuity `K → landing` and a refusal as none.
    pub consumed_values: Arc<Mutex<Vec<f32>>>,
    /// The mock device's output-tail occupancy (frames already consumed
    /// but still queued to "play"), observed by the render gate's
    /// tail-quiescence check and by the leg's position accounting
    /// (D14.7/D14.8). An instant-consuming mock holds nothing, so the
    /// default is an empty tail that drains everything at each
    /// observation; a test changes the playout rate to model a real
    /// device still playing out already-submitted frames.
    pub device_tail: DeviceTail,
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
        TestOutput::observed_with_tail(behavior, consumed, DeviceTail::default())
    }

    /// [`TestOutput::observed`] with the caller owning the mock device's
    /// output tail, so a pause test can model a real device whose
    /// already-submitted frames are still queued to play, and a
    /// position test can model one that drains at a given rate.
    pub fn observed_with_tail(
        behavior: OutputBehavior,
        consumed: Arc<std::sync::atomic::AtomicUsize>,
        device_tail: DeviceTail,
    ) -> TestOutput {
        TestOutput::observed_with_content(
            behavior,
            consumed,
            Arc::new(Mutex::new(Vec::new())),
            device_tail,
        )
    }

    /// [`TestOutput::observed_with_tail`] plus the caller owning the
    /// consumed-content witness (F5): the seek matrices assert on the
    /// exact submitted sample sequence, so the witness must outlive the
    /// service the runtime captured.
    pub fn observed_with_content(
        behavior: OutputBehavior,
        consumed: Arc<std::sync::atomic::AtomicUsize>,
        consumed_values: Arc<Mutex<Vec<f32>>>,
        device_tail: DeviceTail,
    ) -> TestOutput {
        TestOutput {
            behavior,
            consumed,
            consumed_values,
            device_tail,
            tail_probe: TailProbe::default(),
            open_abort_engaged: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// [`TestOutput::observed_with_tail`] plus the caller-owned
    /// open-abort engagement witness, for
    /// [`OutputBehavior::OpenTimeoutAbort`].
    pub fn open_timeout_abort(
        consumed: Arc<std::sync::atomic::AtomicUsize>,
        device_tail: DeviceTail,
        tail_probe: TailProbe,
        open_abort_engaged: Arc<std::sync::atomic::AtomicBool>,
    ) -> TestOutput {
        TestOutput {
            behavior: OutputBehavior::OpenTimeoutAbort,
            consumed,
            consumed_values: Arc::new(Mutex::new(Vec::new())),
            device_tail,
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
/// run_render_thread), including its F4 frame accounting and position
/// publication.
fn spawn_test_leg(
    input: Arc<dyn RenderPcmInput>,
    gate: qianqian_audio_api::ports::RenderGate,
    drain: DrainSignal,
    position: PositionEvidence,
    pace: Option<Duration>,
    abort_after: Option<usize>,
    output: &TestOutput,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    let consumed = output.consumed.clone();
    let consumed_values = output.consumed_values.clone();
    let device_tail = output.device_tail.clone();
    let tail_probe = output.tail_probe.clone();
    std::thread::Builder::new()
        .name("qianqian-test-render".into())
        .spawn(move || {
            let verdict = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                consume_loop(
                    input.clone(),
                    &gate,
                    &position,
                    pace,
                    abort_after,
                    &MockDevice {
                        tail: &device_tail,
                        tail_probe: &tail_probe,
                        consumed: &consumed,
                        consumed_values: &consumed_values,
                    },
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
                    position,
                    format: _,
                } = request;
                let thread = spawn_test_leg(
                    input.clone(),
                    gate.clone(),
                    drain,
                    position,
                    None,
                    None,
                    self,
                )
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
                    position,
                    format: _,
                } = request;
                let thread = spawn_test_leg(
                    input.clone(),
                    gate,
                    drain,
                    position,
                    pace,
                    abort_after,
                    self,
                )
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

/// The mock device as the render leg sees it: the queue it reads and
/// submits into, the injected hold on that observation, the consumption
/// witnesses the tests assert on, and the content witness.
struct MockDevice<'a> {
    tail: &'a DeviceTail,
    tail_probe: &'a TailProbe,
    consumed: &'a std::sync::atomic::AtomicUsize,
    consumed_values: &'a Mutex<Vec<f32>>,
}

/// The mock stand-in for the leg's `publish_consumed` helper (wasapi.rs):
/// the F5 stretch basis folded into the F4 consumed estimate.
fn publish_consumed(
    position: &PositionEvidence,
    basis: u64,
    handed_off: u64,
    tail: u64,
    publishing: bool,
) {
    if publishing {
        position.publish_consumed(basis + handed_off, tail);
    }
}

fn consume_loop(
    input: Arc<dyn RenderPcmInput>,
    gate: &RenderGate,
    position: &PositionEvidence,
    pace: Option<Duration>,
    abort_after: Option<usize>,
    device: &MockDevice<'_>,
) -> DrainVerdict {
    use std::sync::atomic::Ordering;
    let mut dst = vec![0.0f32; 256 * usize::from(TEST_FORMAT.channels)];
    let mut reads = 0usize;
    // The mock leg's own frame accounting (D14.8 + the F5 stretch basis,
    // D14.5), mirroring the real loop's locals: handed-off counts only
    // the CURRENT stretch, and the basis is rebased by the leg itself at
    // a committed cutover.
    let mut handed_off: u64 = 0;
    let mut basis: u64 = 0;
    let mut publishing: bool = true;
    loop {
        if abort_after.is_some_and(|limit| reads >= limit) {
            // Device died mid-stream: the render loop exits on its own.
            // The caller mirrors the real mechanism by stopping the data
            // plane before completing the drain.
            return DrainVerdict::Aborted;
        }
        // Mirror the real mechanism's loop-top pause gate (D14.7): the
        // gate parks before the read; the mock's tail observation is
        // its own device-tail queue, passable through the probe. The
        // same reading feeds the position accounting (D14.8): submission
        // is frozen while parked, so the park slices are what walk the
        // published sample up to the frozen handed-off total.
        gate.park_while_paused(|| {
            let tail = device.tail.observe();
            publish_consumed(position, basis, handed_off, tail, publishing);
            device.tail_probe.observe(tail == 0)
        });
        // Mirror the real loop's cut-attributed seek park (D14.5): same
        // loop-top park invariant, separate attribution, and the release
        // payload rebases the stretch basis on this path before any
        // further submission.
        if let SeekParkOutcome::Released(release) = gate.park_while_seek_hold(|| {
            let tail = device.tail.observe();
            publish_consumed(position, basis, handed_off, tail, publishing);
            device.tail_probe.observe(tail == 0)
        }) {
            match release {
                SeekParkRelease::Committed { landing } => {
                    handed_off = 0;
                    if landing.is_none() {
                        publishing = false;
                    }
                    if let Some(landing) = landing.filter(|_| publishing) {
                        basis = landing;
                    }
                    // A withdrawal is for the REST of the episode (D14.5
                    // position rebase) — identical to the real leg's
                    // wasapi.rs rebase arm: a later KNOWN landing after
                    // an unknown one neither resurrects publication nor
                    // un-withdraws the cell.
                    position.rebase(if publishing { landing } else { None });
                }
                SeekParkRelease::Aborted => {}
            }
        }
        // Mirror the real loop's per-iteration padding observation:
        // publish the consumed estimate as of THIS instant, from the
        // handed-off total as it stands BEFORE the submission below.
        let tail = device.tail.observe();
        publish_consumed(position, basis, handed_off, tail, publishing);
        match input.read_frames(&mut dst) {
            PcmPull::Frames(n) => {
                device.consumed.fetch_add(n, Ordering::SeqCst);
                reads += 1;
                // The content witness: the channel-0 value of every
                // submitted frame, in order.
                {
                    let mut values = device.consumed_values.lock().unwrap();
                    let channels = usize::from(TEST_FORMAT.channels);
                    values.extend(dst[..n * channels].iter().step_by(channels).copied());
                }
                // The mock's ReleaseBuffer(n): the device took the block,
                // so only now does it earn handed-off accounting.
                device.tail.submit(n as u64);
                handed_off += n as u64;
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
