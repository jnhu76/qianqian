//! E1-WORKER — deterministic stop / pending-fatal interleavings on the
//! REAL Session machinery (migration §7.5; reconciliation §2.3.1, as
//! corrected by R1.1/R1.2).
//!
//! What is real here: the production `decode_worker` loop, the production
//! bounded `PcmEdge`, the production `SessionCompletion` (evidence
//! publication, the single-lock settlement step, `request_stop`'s own
//! window) and a real consuming render leg publishing through the real
//! `DrainSignal` + `RenderGate`. What is scripted: the decode endpoint — a
//! test `DecodedPcmStream` whose reads and seek follow a script with
//! rendezvous parks the test controls. Script-protocol validation is thus
//! kept distinct from the real C latch validation (E1-ABI, binding crate):
//! neither harness substitutes for the other.
//!
//! No sleep is an ordering edge anywhere in this file. Every precondition
//! is a rendezvous — script park arrival, the leg's own published park
//! evidence (`leg_parked_evidence`), the worker's own slot/refusal
//! publications — and every wait is a bounded watchdog whose expiry FAILS
//! the trace instead of passing it.
//!
//! The cut construction (W2/W2b/W3/W5a/W5b-R): the leg is parked by the
//! episode's own pause command BEFORE it exists, so its first loop-top gate
//! check parks it and it holds no device buffer and drains nothing for the
//! whole trace. The worker is then allowed to fill the edge exactly and is
//! handed one more block; that write can never complete (a parked leg frees
//! no space), so the write pass is provably mid-flight when the seek is
//! planted and its next observation cuts with a non-empty remainder.
//! W5b-N/W5c omit the extra block, so no write pass ever runs with the
//! command present and the remainder is provably absent.
//!
//! TEST-QUALITY-STANDARD-v1 authoring record entry points (per trace, in
//! its doc comment): property authority, precondition witness, settlement
//! observation, failure signature, negative control, cleanup boundary.

use std::collections::VecDeque;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use qianqian_audio_api::ports::{
    AudioOutput, DecodeError, DecodeOutcome, DecodedPcmStream, DrainVerdict, PcmFormat,
    ProviderSeekOutcome, RenderPcmInput, RenderRequest, RenderStream,
};

use crate::completion::{SessionCompletion, SessionOutcome, WorkerFailureOrigin, test_sync};
use crate::edge::{EdgeTerminal, PcmEdge};
use crate::handle::EpisodeTerminalOutcome;
use crate::observation::ObservationTap;
use crate::session::{EDGE_CAPACITY_FRAMES, ProcessingRuntime, STAGING_FRAMES, decode_worker};
use crate::test_common::{OutputBehavior, TEST_FORMAT, TestOutput, within};

/// The block size every trace uses: the production staging block. The
/// observation tap bounds the staging size to its FFT window, so a block
/// can never exceed the edge — the cut below therefore rests on the parked
/// leg draining nothing, not on block arithmetic.
const BLOCK_FRAMES: usize = STAGING_FRAMES;

/// How many blocks fill the edge exactly (the worker's own production).
const FILL_BLOCKS: usize = EDGE_CAPACITY_FRAMES / BLOCK_FRAMES;

const CHANNELS: usize = TEST_FORMAT.channels as usize;

/// Park ids used by the script rendezvous. `PARK_START` is where every
/// episode's worker waits until the trace publishes its first real read
/// script, so no read ever observes an unscripted state.
const PARK_START: u64 = 0;
const PARK_READ: u64 = 1;
const PARK_READ_2: u64 = 2;
const PARK_SEEK: u64 = 3;

const WATCHDOG: Duration = Duration::from_secs(5);
const TEST_TARGET: Duration = Duration::from_millis(500);

/// One scripted provider read step. Steps are consumed in order; with
/// `repeat_tail` the LAST executed step repeats forever.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    /// One block of up to `n` frames: the worker's write path runs.
    Frames(usize),
    /// `SONG_OK` with zero frames — the loop-top spin arm that runs NO
    /// write (session.rs:831-835).
    Zero,
    /// A read-time failure of the provider's ordinary fault class.
    Fail(&'static str),
    /// Rendezvous: park inside this provider call until the test releases
    /// it, then continue with the NEXT step.
    Park(u64),
}

/// What the scripted provider's `seek` does after its (optional) park.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SeekPlan {
    Applied(Option<u64>),
    Refused,
    Mutated,
}

#[derive(Default)]
struct Inner {
    reads: VecDeque<Step>,
    repeat_tail: bool,
    last_step: Option<Step>,
    park_entered: Vec<u64>,
    park_released: Vec<u64>,
    read_calls: usize,
    /// The steps actually EXECUTED by reads, in order — the provider-side
    /// witness for "no write pass ran with the command present" (W5b-N).
    history: Vec<Step>,
    /// The C-side latched pending fatal, emulated: when set, the NEXT read
    /// surfaces it (the ABI's deferred-reporting row). `Applied` erases it;
    /// a refusal retains it; a destructive outcome never reaches a read.
    latch: Option<&'static str>,
    latch_surfaced_at: Option<usize>,
    seek_calls: usize,
    seek_plan: Option<(Option<u64>, SeekPlan)>,
    seek_entered: bool,
    seek_released: bool,
    frames_handed: u64,
    /// Script-protocol violation (an unplanned seek, a malformed script):
    /// surfaces instead of silently producing a cell.
    violation: Option<&'static str>,
}

/// The test-owned script + its rendezvous primitive. Every mutation and
/// observation is under one mutex; waits use the same condition variable,
/// so a witness never polls a sleep.
struct Script {
    inner: Mutex<Inner>,
    cv: Condvar,
}

impl Script {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            cv: Condvar::new(),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn set_reads(&self, steps: Vec<Step>, repeat_tail: bool) {
        let mut inner = self.lock();
        inner.reads = steps.into();
        inner.repeat_tail = repeat_tail;
        if !inner.repeat_tail {
            inner.last_step = None;
        }
        self.cv.notify_all();
    }

    fn plan_seek(&self, park: Option<u64>, plan: SeekPlan) {
        self.lock().seek_plan = Some((park, plan));
    }

    fn set_latch(&self, message: &'static str) {
        self.lock().latch = Some(message);
    }

    fn release_park(&self, id: u64) {
        let mut inner = self.lock();
        if !inner.park_released.contains(&id) {
            inner.park_released.push(id);
        }
        self.cv.notify_all();
    }

    fn release_seek(&self) {
        let mut inner = self.lock();
        inner.seek_released = true;
        self.cv.notify_all();
    }

    fn release_everything(&self) {
        let mut inner = self.lock();
        for id in inner.park_entered.clone() {
            if !inner.park_released.contains(&id) {
                inner.park_released.push(id);
            }
        }
        inner.seek_released = true;
        self.cv.notify_all();
    }

    /// Bounded watchdog wait over a predicate on the script state.
    fn wait_for(&self, limit: Duration, predicate: impl Fn(&Inner) -> bool) -> bool {
        let deadline = Instant::now() + limit;
        let mut inner = self.lock();
        loop {
            if predicate(&inner) {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let (guard, _timeout) = self
                .cv
                .wait_timeout(inner, deadline.saturating_duration_since(now))
                .unwrap_or_else(|p| p.into_inner());
            inner = guard;
        }
    }

    fn wait_park_entered(&self, id: u64) {
        assert!(
            self.wait_for(WATCHDOG, |inner| inner.park_entered.contains(&id)),
            "the worker never entered the scripted park {id}"
        );
    }

    fn wait_seek_entered(&self) {
        assert!(
            self.wait_for(WATCHDOG, |inner| inner.seek_entered),
            "the worker never called the provider seek"
        );
    }

    fn wait_read_calls(&self, at_least: usize) {
        let saw = self.wait_for(WATCHDOG, |inner| inner.read_calls >= at_least);
        let calls = self.lock().read_calls;
        let history = self.lock().history.clone();
        assert!(
            saw,
            "the worker never issued {at_least} scripted reads (saw {calls}; history {history:?})"
        );
    }

    fn read_calls(&self) -> usize {
        self.lock().read_calls
    }

    fn seek_calls(&self) -> usize {
        self.lock().seek_calls
    }

    fn history(&self) -> Vec<Step> {
        self.lock().history.clone()
    }

    fn latch_surfaced_at(&self) -> Option<usize> {
        self.lock().latch_surfaced_at
    }

    fn violation(&self) -> Option<&'static str> {
        self.lock().violation
    }
}

struct ScriptedStream {
    script: Arc<Script>,
    /// The tag handed to the block most recently produced (the read-call
    /// index). Content is tagged so a trace can witness WHICH blocks
    /// reached the leg.
    tag: f32,
}

impl DecodedPcmStream for ScriptedStream {
    fn format(&self) -> PcmFormat {
        TEST_FORMAT
    }

    fn source_duration(&self) -> Option<Duration> {
        None
    }

    fn read_frames(&mut self, dst: &mut [f32]) -> Result<DecodeOutcome, DecodeError> {
        let mut inner = self.script.lock();
        inner.read_calls += 1;
        let call = inner.read_calls;
        // The latched pending fatal is reported by the next read (the
        // ABI's deferred-reporting row). Identity is the script's own
        // record, never this text.
        if let Some(message) = inner.latch {
            inner.history.push(Step::Fail("latched"));
            inner.latch_surfaced_at.get_or_insert(call);
            return Err(DecodeError {
                message: message.to_owned(),
            });
        }
        loop {
            let step = match inner.reads.pop_front() {
                Some(step) => {
                    if inner.repeat_tail {
                        inner.last_step = Some(step);
                    }
                    step
                }
                None if inner.repeat_tail => match inner.last_step {
                    Some(step) => step,
                    None => {
                        inner.violation = Some("repeat tail without a last step");
                        return Ok(DecodeOutcome::Eof);
                    }
                },
                None => {
                    // An exhausted non-repeating script is a script
                    // boundary, not a fault: the source reports EOF.
                    inner.history.push(Step::Zero);
                    return Ok(DecodeOutcome::Eof);
                }
            };
            match step {
                Step::Park(id) => {
                    if !inner.park_entered.contains(&id) {
                        inner.park_entered.push(id);
                    }
                    self.script.cv.notify_all();
                    while !inner.park_released.contains(&id) {
                        inner = self
                            .script
                            .cv
                            .wait(inner)
                            .unwrap_or_else(|p| p.into_inner());
                    }
                    self.script.cv.notify_all();
                    continue;
                }
                Step::Frames(n) => {
                    let frames = n.min(dst.len() / CHANNELS).max(1);
                    let samples = frames * CHANNELS;
                    // Tag = this read's index: the block's identity is
                    // observable in the leg's content witness.
                    self.tag = call as f32;
                    for sample in dst[..samples].iter_mut() {
                        *sample = self.tag;
                    }
                    inner.frames_handed += frames as u64;
                    inner.history.push(Step::Frames(frames));
                    return Ok(DecodeOutcome::Frames(frames));
                }
                Step::Zero => {
                    inner.history.push(Step::Zero);
                    return Ok(DecodeOutcome::Frames(0));
                }
                Step::Fail(message) => {
                    inner.history.push(Step::Fail(message));
                    return Err(DecodeError {
                        message: message.to_owned(),
                    });
                }
            }
        }
    }

    fn seek(&mut self, _target: Duration) -> ProviderSeekOutcome {
        let mut inner = self.script.lock();
        inner.seek_calls += 1;
        let (park, plan) = match inner.seek_plan {
            Some(plan) => plan,
            None => {
                inner.violation = Some("a seek arrived with no plan");
                return ProviderSeekOutcome::MutatedThenFailed {
                    diagnostic: "script violation: unplanned seek".to_owned(),
                };
            }
        };
        if let Some(_id) = park {
            inner.seek_entered = true;
            self.script.cv.notify_all();
            while !inner.seek_released {
                inner = self
                    .script
                    .cv
                    .wait(inner)
                    .unwrap_or_else(|p| p.into_inner());
            }
            self.script.cv.notify_all();
        }
        match plan {
            SeekPlan::Applied(landing) => {
                // The ABI's Applied row: a successful seek erases the
                // latched fatal (reconciliation §2.3 row 2).
                inner.latch = None;
                ProviderSeekOutcome::Applied { landing }
            }
            SeekPlan::Refused => ProviderSeekOutcome::RefusedUnchanged,
            SeekPlan::Mutated => ProviderSeekOutcome::MutatedThenFailed {
                diagnostic: "scripted destructive seek failure".to_owned(),
            },
        }
    }
}

/// The deliberate no-op processor (the worker's default is the same).
struct NoopProcessing;

impl ProcessingRuntime for NoopProcessing {
    fn stage(&mut self, _block: &mut [f32]) -> Result<(), String> {
        Ok(())
    }

    fn invalidate_signal_history(&mut self) {}
}

/// One E1-WORKER episode: real worker + edge + completion, the shared
/// consuming render-leg double, and the script handle the test drives.
struct Episode {
    completion: SessionCompletion,
    edge: Arc<PcmEdge>,
    script: Arc<Script>,
    /// The leg's channel-0 content witness (one value per delivered frame).
    values: Arc<Mutex<Vec<f32>>>,
    stream: Option<Box<dyn RenderStream>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Episode {
    /// Build the episode. `pause_before_stream` routes pause intent before
    /// the leg exists, so the leg's first loop-top gate check parks it and
    /// it stays frozen for the whole trace (the cut construction's basis).
    fn build(pause_before_stream: bool) -> Episode {
        let script = Script::new();
        // The worker parks here on its first read until the trace supplies
        // its script, so no read can observe an unscripted (EOF) state.
        script.set_reads(vec![Step::Park(PARK_START)], true);
        let completion = SessionCompletion::new();
        let edge = Arc::new(PcmEdge::new(TEST_FORMAT.channels, EDGE_CAPACITY_FRAMES));
        completion.bind_stop_target(Arc::clone(&edge));
        if pause_before_stream {
            completion.request_pause();
        }
        let output = TestOutput::observed(
            OutputBehavior::Consume,
            Arc::new(AtomicUsize::new(0)),
        );
        let values = Arc::clone(&output.consumed_values);
        let stream = output
            .open_stream(RenderRequest {
                format: TEST_FORMAT,
                input: Arc::clone(&edge) as Arc<dyn RenderPcmInput>,
                drain: completion.drain_signal(),
                gate: completion.render_gate(),
                level: completion.output_level(),
                position: completion.position_evidence(),
            })
            .expect("the scripted episode's render leg opens");
        let worker = std::thread::Builder::new()
            .name("qianqian-decode".into())
            .spawn({
                let script = Arc::clone(&script);
                let edge = Arc::clone(&edge);
                let completion = completion.clone();
                move || {
                    decode_worker(
                        Box::new(ScriptedStream { script, tag: 0.0 }),
                        edge,
                        completion,
                        STAGING_FRAMES,
                        NoopProcessing,
                        ObservationTap::new(TEST_FORMAT, STAGING_FRAMES),
                    );
                }
            })
            .expect("the decode worker spawns");
        let episode = Episode {
            completion,
            edge,
            script,
            values,
            stream: Some(stream),
            worker: Some(worker),
        };
        if pause_before_stream {
            episode.await_leg_parked(WATCHDOG);
        }
        episode
    }

    /// The leg's own published park evidence (the D14.5 precondition).
    fn await_leg_parked(&self, limit: Duration) {
        let deadline = Instant::now() + limit;
        while !self.completion.leg_parked_evidence() {
            assert!(
                Instant::now() < deadline,
                "the render leg never published its park"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Publish the trace's first read script and release the worker from
    /// its start rendezvous. Order does not matter: a worker that already
    /// parked at `PARK_START` sees the release immediately, and one that
    /// has not read yet simply finds the new script.
    fn start_reads(&self, steps: Vec<Step>, repeat_tail: bool) {
        self.script.set_reads(steps, repeat_tail);
        self.script.release_park(PARK_START);
    }

    /// The worker's own production fills the edge exactly (the parked leg
    /// drains nothing). Witness: the edge is at capacity and the worker
    /// reached the zero-frame step after the last block.
    fn fill_edge_from_worker(&self) {
        let mut steps = vec![Step::Frames(BLOCK_FRAMES); FILL_BLOCKS];
        steps.push(Step::Zero);
        self.start_reads(steps, true);
        self.script.wait_read_calls(FILL_BLOCKS + 1);
        assert_eq!(
            self.edge.buffered_frames(),
            EDGE_CAPACITY_FRAMES,
            "the parked leg must leave the worker's own production to fill the edge"
        );
    }

    /// The CutPoint construction. The injected block's write can never
    /// complete (the edge is full and the parked leg frees no space), so
    /// the pass is provably still in flight when the seek is planted and
    /// its next observation cuts with a non-empty remainder. Returns the
    /// content tag of the cut block.
    fn plant_seek_and_cut(&self, plan: SeekPlan) -> usize {
        let cut_tag = self.script.read_calls() + 1;
        self.script
            .set_reads(vec![Step::Frames(BLOCK_FRAMES), Step::Zero], true);
        // The injected block was handed out: its write pass is running.
        self.script.wait_read_calls(cut_tag);
        self.script.plan_seek(Some(PARK_SEEK), plan);
        self.completion.request_seek(TEST_TARGET);
        cut_tag
    }

    /// Plant a seek with NO write pass (zero-frame reads only): the
    /// remainder is provably absent.
    fn plant_seek_without_writes(&self, plan: SeekPlan) {
        self.script.set_reads(vec![Step::Zero], true);
        self.script.plan_seek(Some(PARK_SEEK), plan);
        self.completion.request_seek(TEST_TARGET);
    }

    /// Frames the leg has delivered carrying `tag` (the content witness).
    fn delivered_with_tag(&self, tag: usize) -> usize {
        let values = self.values.lock().unwrap_or_else(|p| p.into_inner());
        values
            .chunks(CHANNELS)
            .filter(|frame| frame[0] == tag as f32)
            .count()
    }

    fn outcome(&self) -> Option<EpisodeTerminalOutcome> {
        self.completion.observe_snapshot().terminal_outcome
    }

    /// The internal committed outcome — the cell discriminator, since the
    /// failure ORIGIN is an internal realization the public triple
    /// collapses (D14.2). Never a message-text identity.
    fn committed(&self) -> Option<SessionOutcome> {
        self.completion.committed()
    }

    fn drain_mirror(&self) -> Option<DrainVerdict> {
        self.completion.drain_mirror_verdict()
    }

    fn await_terminal(&self, limit: Duration) -> (SessionOutcome, Option<DrainVerdict>) {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(outcome) = self.committed() {
                return (outcome, self.drain_mirror());
            }
            assert!(
                Instant::now() < deadline,
                "no terminal outcome was committed within {limit:?}"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Bounded watchdog over a predicate on the completion's own state.
    fn await_completion_state(&self, what: &str, predicate: impl Fn() -> bool) {
        let deadline = Instant::now() + WATCHDOG;
        while !predicate() {
            assert!(Instant::now() < deadline, "never observed: {what}");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// A trace may not pass while the script itself reports an unexpected
    /// call shape (an unplanned seek included).
    fn assert_no_violation(&self) {
        assert_eq!(
            self.script.violation(),
            None,
            "the scripted provider reported a protocol violation"
        );
    }

    /// Cleanup boundary (never asserted as the success path): release every
    /// park, stop the episode, join the worker, release the stream.
    fn teardown(mut self) {
        self.script.release_everything();
        self.completion.request_stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Some(stream) = self.stream.take() {
            let _ = stream.stop_and_join();
        }
    }
}

/// Assert the committed class is `Failed` with the DECODE worker origin —
/// the typed internal discriminator (never the diagnostic text).
fn assert_failed_decode(episode: &Episode, context: &str) {
    let outcome = episode.committed().expect("a committed outcome");
    match &outcome {
        SessionOutcome::Failed { stage } => assert!(
            stage.starts_with("decode"),
            "{context}: reached Failed with the {stage} origin"
        ),
        other => panic!("{context}: expected Failed{{decode}}, got {other:?}"),
    }
    assert_eq!(
        episode.completion.worker_failure_origin(),
        Some(WorkerFailureOrigin::Decode),
        "{context}: the typed worker-failure origin"
    );
}

fn assert_stopped(outcome: &SessionOutcome, context: &str) {
    assert_eq!(*outcome, SessionOutcome::Stopped, "{context}");
}

// --- W1: loop-top read failure ------------------------------------------------

/// Oracle: a read-time provider failure at the loop top commits
/// `Failed{decode}` (the read-Err arm, session.rs:889-893).
///
/// Precondition witness: the worker is INSIDE the parked provider call
/// (script park arrival) before the failure is released. Failure
/// signature: any other committed class, or the park never being entered.
#[test]
fn w1_loop_top_read_failure_commits_failed_decode() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(20), || {
        let episode = Episode::build(false);
        episode.start_reads(
            vec![Step::Park(PARK_READ), Step::Fail("scripted read failure")],
            false,
        );
        episode.script.wait_park_entered(PARK_READ);
        episode.script.release_park(PARK_READ);
        episode.await_terminal(WATCHDOG);
        assert_failed_decode(&episode, "a read failure on the worker's own path");
        assert_eq!(episode.script.read_calls(), 1, "one read, one failure");
        episode.assert_no_violation();
        episode.teardown();
    });
}

// --- W2: destructive seek with a retained remainder ---------------------------

/// Oracle: `MutatedThenFailed` at the serialization point takes the
/// destructive arm — `decode_failed` + `edge.fail` + return, with the
/// retained remainder discarded together with the pre-cut world
/// (session.rs:656-668). The old continuation is never resumed: no frame
/// of the preserved remainder reaches the leg.
///
/// Precondition witness: the CutPoint construction (the leg parked for the
/// whole trace, the write pass provably in flight when the command lands),
/// then the worker parked inside the provider seek; the leg is released
/// (resumed) BEFORE the destructive outcome so the witness is not vacuous.
/// Sibling: W2b runs the identical construction with a refusal and shows
/// the same block tagging really is delivered — so "0 delivered frames of
/// the cut block" here means discarded, not invisible.
#[test]
fn w2_destructive_seek_with_retained_remainder_commits_failed_decode() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(20), || {
        let episode = Episode::build(true);
        episode.fill_edge_from_worker();
        let cut_tag = episode.plant_seek_and_cut(SeekPlan::Mutated);
        episode.script.wait_seek_entered();
        let reads_at_seek = episode.script.read_calls();
        episode.completion.request_resume();
        episode.script.release_seek();
        episode.await_terminal(WATCHDOG);
        assert_failed_decode(
            &episode,
            "an unprovable seek outcome must never resume the old continuation",
        );
        assert_eq!(
            episode.script.read_calls(),
            reads_at_seek,
            "the destructive arm returns without another read"
        );
        assert_eq!(
            episode.delivered_with_tag(cut_tag),
            0,
            "the retained remainder must not surface as post-cut production"
        );
        assert_eq!(episode.script.seek_calls(), 1, "exactly one provider seek");
        episode.assert_no_violation();
        episode.teardown();
    });
}

// --- W2b: the refusal path delivers the retained remainder exactly -------------

/// Oracle (the precondition witness for W3/W5b-R): the SAME CutPoint
/// construction with a refusal and NO stop finishes the preserved
/// remainder before any new decode (session.rs:786-804) — the cut block's
/// frames really reach the leg — and only then does the next read surface
/// the retained latch.
///
/// This is what proves the construction really retained a remainder
/// (content witness, not arithmetic). Precondition witnesses: the flush
/// completed (the seek slot is freed only after it, session.rs:803) and
/// the leg received frames carrying the cut block's tag.
#[test]
fn w2b_refused_seek_delivers_the_retained_remainder_then_surfaces_the_latch() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(20), || {
        let episode = Episode::build(true);
        episode.fill_edge_from_worker();
        let cut_tag = episode.plant_seek_and_cut(SeekPlan::Refused);
        episode.script.wait_seek_entered();
        episode.script.release_seek();
        // Let the leg drain so the refusal's remainder flush can finish.
        episode.completion.request_resume();
        episode.await_completion_state("the refusal's remainder flush completed", || {
            !episode.completion.seek_in_flight()
        });
        episode.await_completion_state("the remainder's frames reached the leg", || {
            episode.delivered_with_tag(cut_tag) > 0
        });
        // Only now is the old latch set: the following read surfaces it.
        episode.script.set_latch("pending decode error");
        episode.await_terminal(WATCHDOG);
        assert_failed_decode(
            &episode,
            "the refusal retains the latch; the next read surfaces it",
        );
        assert!(
            episode.script.latch_surfaced_at().is_some(),
            "the retained latch must surface (identity from the script record)"
        );
        assert!(
            episode.delivered_with_tag(cut_tag) > 0,
            "the retained remainder was delivered after the refusal (zero content loss)"
        );
        assert_eq!(episode.script.seek_calls(), 1);
        episode.assert_no_violation();
        episode.teardown();
    });
}

// --- W3: stop during the remainder flush --------------------------------------

/// Oracle: a stop that terminalizes the edge while the preserved remainder
/// is still pending turns the flush into the legal mid-remainder exit
/// (`write_some == 0 ∧ terminal ≠ Open`, session.rs:792-801) and the
/// episode commits `Stopped` — a stop is not a decode failure, and the
/// remainder is abandoned with the pre-cut world.
///
/// Construction note (recorded differential against migration §7.5's
/// sketch): the sketch places the stop in the flush's capacity wait. This
/// trace fixes the SAME cell deterministically by stopping while the
/// worker is parked inside the provider seek (a rendezvous), so the
/// flush's FIRST `write_some` already meets the stopped edge; the exit
/// surface is the same mid-remainder return. Failure signature: any
/// `Failed` class, or the cut block's frames reaching the leg.
#[test]
fn w3_stop_during_remainder_flush_commits_stopped() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(20), || {
        let episode = Episode::build(true);
        episode.fill_edge_from_worker();
        let cut_tag = episode.plant_seek_and_cut(SeekPlan::Refused);
        episode.script.wait_seek_entered();
        let reads_at_seek = episode.script.read_calls();
        episode.completion.request_stop();
        episode.script.release_seek();
        let (outcome, drain) = episode.await_terminal(WATCHDOG);
        assert_stopped(&outcome, "a stop with no new fault is not a decode failure");
        assert_eq!(drain, Some(DrainVerdict::Aborted), "the leg reports the abort");
        assert_eq!(
            episode.script.read_calls(),
            reads_at_seek,
            "the flush ran before any new decode"
        );
        assert_eq!(
            episode.delivered_with_tag(cut_tag),
            0,
            "the interrupted flush must NOT deliver the remainder"
        );
        assert_eq!(episode.script.seek_calls(), 1);
        episode.assert_no_violation();
        episode.teardown();
    });
}

// --- W4: the intent-visible / own-stop-not-yet-run window ---------------------

/// Oracle: inside `request_stop`'s real window — intent recorded, this
/// request's own `edge.stop()` not yet executed — a pending fatal
/// published by the worker still commits `Failed{decode}`, and the stop
/// that completes afterwards cannot relabel it (D11 first-wins).
///
/// Precondition witness: the fenced request is parked at the new arming
/// point (E1 seam) while `stop_requested == true`, no outcome is
/// committed, `worker_gone == false`, and the edge terminal is still
/// `Open`. Failure signature: the edge already Stopped at the park (the
/// fence missed its window), or the outcome flipping to Stopped after the
/// release.
#[test]
fn w4_pending_fatal_wins_inside_the_stop_window_and_a_late_stop_cannot_relabel() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(20), || {
        let episode = Episode::build(false);
        episode.start_reads(
            vec![Step::Park(PARK_READ), Step::Fail("pending decode error")],
            false,
        );
        episode.script.wait_park_entered(PARK_READ);
        // The real request_stop parks between recording intent and running
        // its own edge stop.
        let mut stop = test_sync::FencedRequest::spawn_stop(&episode.completion);
        stop.wait_parked();
        let state = test_sync::second_hold_state(&episode.completion);
        assert!(state.stop_requested, "intent is recorded at the fence");
        assert!(!state.outcome_set, "nothing is committed yet");
        assert!(!state.worker_gone, "the worker is inside its provider call");
        assert_eq!(
            episode.edge.terminal(),
            EdgeTerminal::Open,
            "the request's own edge stop has not run — the fence is inside the window"
        );
        // Release the provider: the pending fatal is published while the
        // stop still sits in its window.
        episode.script.release_park(PARK_READ);
        episode.await_completion_state("the pending fatal commits", || {
            episode.committed().is_some()
        });
        assert_failed_decode(
            &episode,
            "the decode failure is decisive even before the stop's own edge stop",
        );
        // Now let the stop finish: the edge stops, but the Fact cannot move.
        stop.release();
        stop.join();
        assert_eq!(
            episode.edge.terminal(),
            EdgeTerminal::Failed,
            "the failure terminalized the edge first; the stop's own stop is first-wins inert"
        );
        assert_failed_decode(
            &episode,
            "a later stop must not relabel a committed terminal",
        );
        assert_eq!(episode.script.read_calls(), 1);
        episode.assert_no_violation();
        episode.teardown();
    });
}

// --- W5a: Applied after stop erases the old latch ------------------------------

/// Oracle: a stop during the provider seek with an `Applied` outcome
/// ERASES the old latched fatal (the ABI's Applied row, emulated by the
/// script). `Failed{decode}` then has no legal source at all, so the
/// episode must commit `Stopped` (reconciliation §2.3 row 2 plus the
/// cutover decision's stop class).
///
/// Precondition witness: the old latch is set while the worker is parked
/// inside the provider seek (no read can observe it early). Failure
/// signature: any `Failed` class, or the old latch surfacing.
#[test]
fn w5a_applied_seek_after_stop_erases_the_old_latch_and_commits_stopped() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(20), || {
        let episode = Episode::build(true);
        episode.fill_edge_from_worker();
        episode.plant_seek_and_cut(SeekPlan::Applied(Some(1024)));
        episode.script.wait_seek_entered();
        episode.script.set_latch("pending decode error");
        episode.completion.request_stop();
        // Post-cut production: a healthy slice under the SAME applied
        // configuration (never a fault).
        episode
            .script
            .set_reads(vec![Step::Frames(256), Step::Zero], true);
        episode.script.release_seek();
        let (outcome, drain) = episode.await_terminal(WATCHDOG);
        assert_stopped(
            &outcome,
            "with the latch erased by the Applied row, Failed{decode} has no legal source",
        );
        assert_eq!(drain, Some(DrainVerdict::Aborted));
        assert_eq!(
            episode.script.latch_surfaced_at(),
            None,
            "the old latch must never surface after an Applied seek"
        );
        assert_eq!(episode.script.seek_calls(), 1, "the seek really ran");
        episode.assert_no_violation();
        episode.teardown();
    });
}

// --- W5b-N: refusal, no remainder → the old latch surfaces ---------------------

/// Oracle: a refused seek with NO remainder must not swallow the old
/// latched fatal: after the refusal the worker's next read surfaces it and
/// the episode commits `Failed{decode}` (the single legal outcome).
///
/// Precondition witness (the R5 construction): after the command is
/// planted the script hands out ONLY zero-frame reads — no write pass runs
/// with the command present (script history), so no CutPoint exists and
/// the remainder is provably absent. The latch is set while the worker
/// parks inside the provider seek, so no read can observe it early.
/// `Stopped` here would mean the fatal read never ran — a defect, not a
/// widened allowed set.
#[test]
fn w5b_n_refusal_without_remainder_surfaces_the_old_latch() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(20), || {
        let episode = Episode::build(true);
        episode.fill_edge_from_worker();
        episode.plant_seek_without_writes(SeekPlan::Refused);
        episode.script.wait_seek_entered();
        let history = episode.script.history();
        let last_frames = history
            .iter()
            .rposition(|step| matches!(step, Step::Frames(_)))
            .expect("the fill produced blocks");
        assert!(
            history[last_frames + 1..]
                .iter()
                .all(|step| *step == Step::Zero),
            "no write pass may run with the command present (history: {history:?})"
        );
        episode.script.set_latch("pending decode error");
        episode.completion.request_stop();
        episode.script.release_seek();
        episode.await_terminal(WATCHDOG);
        assert_failed_decode(
            &episode,
            "the refusal retains the latch; Stopped here would mean the fatal read never ran",
        );
        assert!(
            episode.script.latch_surfaced_at().is_some(),
            "the OLD latch must surface on the read after the refusal"
        );
        assert_eq!(episode.script.seek_calls(), 1);
        episode.assert_no_violation();
        episode.teardown();
    });
}

// --- W5b-R: refusal WITH a retained remainder → mid-remainder Stopped ----------

/// Oracle: a refused seek WITH a retained remainder never surfaces the old
/// latch: after the refusal the worker flushes the remainder FIRST, and a
/// stop that terminalized the edge turns that flush into the legal
/// mid-remainder exit — `Stopped`, with the old latch dropped together
/// with the pre-cut world.
///
/// This trace kills the over-strong "stop during a provider call ⇒
/// Failed{decode}" oracle; W5b-N is its sibling (same park, no remainder,
/// latch surfaces). Precondition witness: the CutPoint construction, which
/// W2b witnesses by actual remainder delivery; the stop lands while the
/// worker is parked inside the provider seek.
#[test]
fn w5b_r_refusal_with_retained_remainder_commits_stopped() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(20), || {
        let episode = Episode::build(true);
        episode.fill_edge_from_worker();
        let cut_tag = episode.plant_seek_and_cut(SeekPlan::Refused);
        episode.script.wait_seek_entered();
        let reads_at_seek = episode.script.read_calls();
        episode.script.set_latch("old pending decode error");
        episode.completion.request_stop();
        episode.script.release_seek();
        let (outcome, drain) = episode.await_terminal(WATCHDOG);
        assert_stopped(
            &outcome,
            "the retained remainder's interrupted flush is the legal stop exit",
        );
        assert_eq!(drain, Some(DrainVerdict::Aborted));
        assert_eq!(
            episode.script.read_calls(),
            reads_at_seek,
            "the interrupted flush, not a further read, ended the worker"
        );
        assert_eq!(
            episode.script.latch_surfaced_at(),
            None,
            "the old latch is dropped with the pre-cut world, never surfaced"
        );
        assert_eq!(
            episode.delivered_with_tag(cut_tag),
            0,
            "the abandoned remainder never reaches the leg"
        );
        assert_eq!(episode.script.seek_calls(), 1);
        episode.assert_no_violation();
        episode.teardown();
    });
}

// --- W5c: MutatedThenFailed after a stop ---------------------------------------

/// Oracle: a destructive seek outcome takes the immediate failure route
/// even when a stop is already recorded — the decisive evidence lands
/// before any later read, so the commit is `Failed{decode}` (a stop cannot
/// erase evidence that already landed).
///
/// Precondition witness: the seek is entered (parked) and the stop is
/// issued while the worker is inside it; no read happens after the seek.
#[test]
fn w5c_mutated_seek_after_stop_commits_failed_decode() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(20), || {
        let episode = Episode::build(true);
        episode.fill_edge_from_worker();
        episode.plant_seek_without_writes(SeekPlan::Mutated);
        episode.script.wait_seek_entered();
        let reads_at_seek = episode.script.read_calls();
        episode.script.set_latch("old pending decode error");
        episode.completion.request_stop();
        episode.script.release_seek();
        episode.await_terminal(WATCHDOG);
        assert_failed_decode(
            &episode,
            "the destructive seek arm is decisive on its own path",
        );
        assert_eq!(
            episode.script.read_calls(),
            reads_at_seek,
            "the destructive arm returns without another read"
        );
        assert_eq!(episode.script.seek_calls(), 1);
        episode.assert_no_violation();
        episode.teardown();
    });
}

// --- W6: a canceled seek is not a worker exit ----------------------------------

/// Oracle: a seek command released as canceled (stop intent recorded
/// before the worker acts on it) performs NO provider call and is NOT a
/// worker exit: the worker continues to its next boundary, and the final
/// class comes from the actual decisive evidence (reconciliation §2.3.1's
/// "a canceled pending seek is not an exit").
///
/// Construction: the worker is parked inside a provider read (rendezvous),
/// the seek is planted, and the stop is issued through the stop-window
/// fence — so intent is recorded while the edge is still Open and the
/// serialization point takes the `seek_aborted` release branch.
/// Precondition witness: the worker reaches a LATER script park while the
/// canceled seek holds no provider call (`seek_calls == 0`) and the slot
/// is free.
#[test]
fn w6_canceled_seek_is_not_a_worker_exit() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(20), || {
        let episode = Episode::build(false);
        episode.start_reads(
            vec![Step::Frames(256), Step::Park(PARK_READ), Step::Zero],
            true,
        );
        episode.script.wait_park_entered(PARK_READ);
        episode.completion.request_seek(TEST_TARGET);
        let mut stop = test_sync::FencedRequest::spawn_stop(&episode.completion);
        stop.wait_parked();
        assert_eq!(
            episode.edge.terminal(),
            EdgeTerminal::Open,
            "the canceled-seek check runs while the edge is still Open"
        );
        // Let the worker out of its read: it picks the command up, sees the
        // recorded stop intent at the serialization point and releases the
        // seek without any provider call, then runs on to its next boundary.
        episode
            .script
            .set_reads(vec![Step::Zero, Step::Park(PARK_READ_2), Step::Zero], true);
        episode.script.release_park(PARK_READ);
        episode.script.wait_park_entered(PARK_READ_2);
        assert_eq!(
            episode.script.seek_calls(),
            0,
            "a canceled seek must not reach the provider"
        );
        assert!(
            !episode.completion.seek_in_flight(),
            "the canceled seek's slot was released"
        );
        assert_eq!(
            episode.outcome(),
            None,
            "no terminal came from the cancellation itself"
        );
        episode.script.release_park(PARK_READ_2);
        stop.release();
        stop.join();
        let (outcome, _drain) = episode.await_terminal(WATCHDOG);
        assert_stopped(
            &outcome,
            "the final class comes from the post-cancel decisive evidence",
        );
        assert_eq!(
            episode.script.seek_calls(),
            0,
            "still no provider seek for the whole episode"
        );
        episode.assert_no_violation();
        episode.teardown();
    });
}
