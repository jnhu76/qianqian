//! I0 Gain probe oracles (Issue #177 Stage 2 / I0; ADR-PBK-002 D14.11).
//! These tests ground the frozen processing seam — decode-worker
//! staging, between `read_frames` and the PcmEdge write — with the
//! disposable scalar-Gain probe of [`crate::gain_probe`], through the
//! REAL Playback Session composition over the mechanism doubles.
//!
//! What these tests prove is the seam, not a DSP framework: the probe is
//! experiment-only evidence (deleted with I0), and every oracle here is
//! a D14.11 semantic requirement — unity/half/zero exactness, frame and
//! format conservation, EOF tail-freedom, partial edge acceptance of
//! processed blocks, seek RefusedUnchanged/Applied obligations on
//! processed staging and processed remainder, pause continuity, and the
//! D11 failure route with a truthful processing-origin diagnostic.
//!
//! The exactness argument for `==` oracles (no float tolerance): the
//! decode double tags frame `i` with `i as f32` — exact for
//! `i < 2^24`, and every source here is far below that — and for the
//! required factors IEEE-754 f32 multiplication is exact: ×1.0 is the
//! identity, ×0.5 is an exponent decrement (no mantissa change for
//! normal numbers), ×0.0 of a finite non-negative value is exactly +0.0.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use qianqian_app::QianqianApp;
use qianqian_audio_api::ports::ProviderSeekOutcome;
use qianqian_composition::{DesiredEntry, Revision};

use crate::gain_probe;
use crate::handle::{EpisodeTerminalOutcome, PlaybackSessionHandle};
use crate::test_common::{self, OutputBehavior, SourceBehavior, TestDecode, TestOutput, within};

const DUMMY_PATH: &str = "test://gain-probe";

/// The test format's sample rate (frames of source per second).
const TEST_RATE: usize = 44_100;

/// An 8-second source — the F5 seek-matrix shape: long enough that a
/// seek fired in the first second is decisively mid-stream (the bounded
/// edge is full and the worker sits inside its interruptible write), and
/// the slow mock consumer still finishes it in seconds.
const EIGHT_SECONDS: usize = TEST_RATE * 8;

struct Witnesses {
    consumed: Arc<AtomicUsize>,
    consumed_values: Arc<Mutex<Vec<f32>>>,
}

impl Witnesses {
    fn new() -> Self {
        Self {
            consumed: Arc::new(AtomicUsize::new(0)),
            consumed_values: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The consumed content, snapped once: the channel-0 value of every
    /// frame the render leg submitted, in submission order.
    fn content(&self) -> Vec<f32> {
        self.consumed_values.lock().unwrap().clone()
    }
}

fn desired(id: &str, component: &'static str) -> DesiredEntry {
    DesiredEntry::enabled(id, component, Revision::new(1))
}

/// Register the standard probe episode: a decode double, an output
/// double sharing the caller's witnesses, and the real session.
fn registered_runtime(
    decode: TestDecode,
    output: OutputBehavior,
    witnesses: &Witnesses,
    handle: PlaybackSessionHandle,
) -> QianqianApp {
    let consumed = witnesses.consumed.clone();
    let consumed_values = witnesses.consumed_values.clone();
    let mut runtime = QianqianApp::new();

    runtime
        .register_component({
            qianqian_composition::ComponentSpec::new("test_decode_plugin")
                .provides::<qianqian_audio_api::ports::PcmDecodeCapability>()
                .on_activate(move |ctx| {
                    ctx.provide::<qianqian_audio_api::ports::PcmDecodeCapability>(
                        std::rc::Rc::new(decode.clone()),
                    )
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("decode provider registers");

    runtime
        .register_component({
            qianqian_composition::ComponentSpec::new("test_output_plugin")
                .provides::<qianqian_audio_api::ports::AudioOutputCapability>()
                .on_activate(move |ctx| {
                    ctx.provide::<qianqian_audio_api::ports::AudioOutputCapability>(
                        std::rc::Rc::new(TestOutput::observed_with_content(
                            output,
                            consumed.clone(),
                            consumed_values.clone(),
                            test_common::DeviceTail::default(),
                        )),
                    )
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("output provider registers");

    runtime
        .register_component(crate::playback_session_spec(
            std::path::PathBuf::from(DUMMY_PATH),
            handle,
        ))
        .expect("session registers");
    runtime
}

/// The standard episode: a position-tagged source of `source_frames`
/// frames played by the given output double, with the standard witness
/// set. Arm the probe BEFORE calling this (the worker starts on
/// activation).
fn episode(
    source_frames: usize,
    output: OutputBehavior,
    seeks: Vec<ProviderSeekOutcome>,
) -> (Witnesses, PlaybackSessionHandle, QianqianApp) {
    let witnesses = Witnesses::new();
    let handle = PlaybackSessionHandle::new();
    let mut runtime = registered_runtime(
        TestDecode {
            behavior: SourceBehavior::EofAfter(source_frames),
            duration: None,
            seeks,
        },
        output,
        &witnesses,
        handle.clone(),
    );
    runtime
        .revise_desired(vec![
            desired("decode", "test_decode_plugin"),
            desired("output", "test_output_plugin"),
            desired("session", "playback_session"),
        ])
        .expect("composition is legal");
    (witnesses, handle, runtime)
}

/// Bounded poll for an asynchronously-published observation.
fn wait_until(limit: Duration, mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if predicate() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The exact processed-content oracle: one output frame per source frame
/// (frame conservation — no insertion, no drop, no EOF tail), each
/// exactly `frame_index × factor` (see the module-doc exactness
/// argument; no float tolerance is needed or allowed here).
fn assert_processed_exactly(values: &[f32], factor: f32, source_frames: usize) {
    assert_eq!(
        values.len(),
        source_frames,
        "frame conservation: N decoded frames in, N consumed frames out \
         (an EOF tail or a dropped/duplicated frame breaks this)"
    );
    for (i, value) in values.iter().enumerate() {
        assert_eq!(
            *value,
            i as f32 * factor,
            "frame {i} must be exactly (frame index) × {factor}"
        );
    }
}

/// Indices where the scaled position-tagged sequence does not continue
/// the previous frame index (`b != a + factor`; scaled frame indices
/// stay far below the f32-exact range).
fn discontinuities(values: &[f32], factor: f32) -> Vec<usize> {
    values
        .iter()
        .zip(values.iter().skip(1))
        .enumerate()
        .filter_map(|(i, (a, b))| (*b != *a + factor).then_some(i + 1))
        .collect()
}

// --- seek / discontinuity oracles (D14.5 extended to processed PCM) ------

/// RefusedUnchanged with processed PCM in flight: the preserved remainder
/// of the interrupted staging block is already PROCESSED (the probe runs
/// before the edge write), and the refusal must finish it exactly once —
/// the consumed stream is indistinguishable from the no-seek control,
/// frame for frame. A reprocessed remainder would show a ×0.25 stretch
/// (an extra discontinuity and wrong values); a dropped one would shift
/// every later frame; both fail the exact oracle.
#[test]
fn a_refused_seek_finishes_its_processed_remainder_exactly_once() {
    let _lifecycle = test_common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let _probe = gain_probe::arm(0.5);
        let (witnesses, handle, mut runtime) = episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            vec![ProviderSeekOutcome::RefusedUnchanged],
        );
        wait_until(Duration::from_secs(5), || {
            handle.observe().position.is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        handle.request_seek(Duration::from_secs(5));
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "a refusal is not a failure and not a cut"
        );
        let values = witnesses.content();
        let breaks = discontinuities(&values, 0.5);
        assert!(
            breaks.is_empty(),
            "a refused seek must be content-indistinguishable from no seek \
             (first breaks: {:?} in {} frames)",
            &breaks[..breaks.len().min(8)],
            values.len()
        );
        assert_processed_exactly(&values, 0.5, EIGHT_SECONDS);
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// Applied with processed PCM in flight: the cut discards the pre-cut
/// processed remainder with the staging, purges the edge under the
/// existing D14.5 mechanism, and post-cut production continues from the
/// provider's landing — still processed. The consumed stream shows
/// exactly ONE discontinuity, landing exactly where the provider
/// reported, with both stretches exactly `frame_index × 0.5`.
#[test]
fn an_applied_seek_discards_the_processed_remainder_and_cuts_cleanly() {
    let _lifecycle = test_common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let _probe = gain_probe::arm(0.5);
        let five_seconds = 5 * TEST_RATE;
        let (witnesses, handle, mut runtime) = episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            handle.observe().position.is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        handle.request_seek(Duration::from_secs(5));
        assert!(
            wait_until(Duration::from_secs(5), || handle
                .observe()
                .position
                .is_some_and(|p| p >= five_seconds as u64)),
            "the position never rebased to the landing: {:?}",
            handle.observe()
        );
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "the seek must not disturb the episode's ordinary EOF course"
        );
        let values = witnesses.content();
        let breaks = discontinuities(&values, 0.5);
        assert_eq!(
            breaks.len(),
            1,
            "a committed cutover must appear as exactly one content \
             discontinuity (first breaks: {:?} in {} frames)",
            &breaks[..breaks.len().min(8)],
            values.len()
        );
        assert_eq!(
            values[breaks[0]],
            five_seconds as f32 * 0.5,
            "the cut must land exactly at the provider's ACTUAL landing \
             (the landing's first processed sample)"
        );
        // Both stretches are exactly the source scaled by the probe: the
        // pre-cut stretch from source frame 0, the post-cut stretch from
        // the landing (the skipped stretch between them is what a seek
        // IS).
        assert_processed_exactly(&values[..breaks[0]], 0.5, breaks[0]);
        let after = &values[breaks[0]..];
        for (offset, value) in after.iter().enumerate() {
            assert_eq!(
                *value,
                (five_seconds + offset) as f32 * 0.5,
                "post-cut frame {offset} must be exactly its source index × 0.5"
            );
        }
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- pause continuity (D14.11: pause never invalidates processing) ------

/// Pause/resume through the real gate with processed PCM in flight: the
/// Paused projection establishes from the current engagement's tail
/// quiescence, resume releases it, and the consumed stream after the
/// cycle is still exactly the source scaled by the probe — no reset, no
/// duplicated stretch, no gap. (The probe is stateless, so the oracle
/// pins the pipeline continuity pause must not disturb.)
#[test]
fn a_pause_resume_cycle_preserves_the_processed_stream() {
    let _lifecycle = test_common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let _probe = gain_probe::arm(0.5);
        let source_frames = TEST_RATE * 4;
        let (witnesses, handle, mut runtime) = episode(
            source_frames,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            handle.observe().position.is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        handle.request_pause();
        assert!(
            wait_until(Duration::from_secs(5), || handle.observe().paused()),
            "the Paused projection never established: {:?}",
            handle.observe()
        );
        handle.request_resume();
        assert!(
            wait_until(Duration::from_secs(5), || {
                !handle.observe().pause_requested
            }),
            "resume never released the episode: {:?}",
            handle.observe()
        );
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "the pause cycle must not disturb the episode's ordinary course"
        );
        assert_processed_exactly(&witnesses.content(), 0.5, source_frames);
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- failure semantics (D14.11: D11 Failed, truthful origin) ------------

/// The synthetic unrecoverable processing failure settles through the
/// EXISTING D11 `Failed` terminal class — no new public variant — with a
/// diagnostic that stays truthful about its processing origin: it names
/// the processing stage and never masquerades as a decode failure merely
/// because both run on the decode worker. The episode stops producing
/// for good and the terminal withdraws the position projection.
#[test]
fn a_processing_failure_takes_the_d11_failed_route_with_a_truthful_diagnostic() {
    let _lifecycle = test_common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let _probe = gain_probe::arm_failure_after_block(0.5, 3);
        let (witnesses, handle, mut runtime) =
            episode(EIGHT_SECONDS, OutputBehavior::Consume, Vec::new());
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Failed,
            "an unrecoverable processing failure is the existing D11 Failed"
        );
        let observation = handle.observe();
        let diagnostic = observation
            .failure_diagnostic
            .as_deref()
            .expect("a failed episode carries its presentation diagnostic");
        assert!(
            diagnostic.starts_with("processing: "),
            "the diagnostic's stage must name the processing origin: {diagnostic}"
        );
        assert!(
            !diagnostic.contains("decode"),
            "a processing failure must not masquerade as a decode failure: {diagnostic}"
        );
        // The failed episode never resumes production.
        let stopped_at = witnesses.consumed.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            witnesses.consumed.load(Ordering::SeqCst),
            stopped_at,
            "a failed episode must never resume production"
        );
        assert!(
            handle.observe().position.is_none(),
            "the terminal Fact withdraws the projection"
        );
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- negative controls (the oracles must be able to FAIL) ---------------

/// Negative control, omission class: with the seam producing unprocessed
/// audio (probe disarmed — the same shape as "skip Gain entirely"), the
/// half-gain oracle MUST reject the stream. A green suite whose oracle
/// cannot fail here would prove nothing about the seam.
#[test]
fn negative_control_the_half_gain_oracle_rejects_a_skipped_seam() {
    let _lifecycle = test_common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        // No arm(): the staging blocks reach the edge unprocessed.
        let source_frames = 20_000;
        let (witnesses, handle, mut runtime) =
            episode(source_frames, OutputBehavior::Consume, Vec::new());
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "precondition: the unprocessed episode itself completes"
        );
        let values = witnesses.content();
        let rejected = catch_unwind(AssertUnwindSafe(|| {
            assert_processed_exactly(&values, 0.5, source_frames);
        }))
        .is_err();
        assert!(
            rejected,
            "the half-gain oracle must REJECT unprocessed audio — \
             a seam detector that passes here proves nothing"
        );
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// Negative control, double-application class: a seam that scales every
/// staging block twice (content ×0.25) MUST be rejected by the same
/// half-gain oracle. Processing must happen exactly once per block.
#[test]
fn negative_control_the_half_gain_oracle_rejects_double_applied_gain() {
    let _lifecycle = test_common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let _probe = gain_probe::arm_double_apply(0.5);
        let source_frames = 20_000;
        let (witnesses, handle, mut runtime) =
            episode(source_frames, OutputBehavior::Consume, Vec::new());
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "precondition: the double-applied episode itself completes"
        );
        let values = witnesses.content();
        let rejected = catch_unwind(AssertUnwindSafe(|| {
            assert_processed_exactly(&values, 0.5, source_frames);
        }))
        .is_err();
        assert!(
            rejected,
            "the half-gain oracle must REJECT double-applied processing — \
             exactly-once is part of the seam contract"
        );
        // The witness of HOW it failed: the content is ×0.25, i.e. the
        // mutation really ran (not an accidental pass through unprocessed
        // audio, which the omission control already covers).
        assert_eq!(values[1], 0.25, "precondition: content is ×0.25");
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- realtime posture of the exercised path -----------------------------

/// I0.7: the exercised Gain path performs zero steady-state allocation —
/// measured, not claimed, with the crate's counting allocator over the
/// staging-sized block the worker feeds (the in-place multiply touches
/// only the caller's buffer and the probe's atomics). The other firewall
/// rows are structural, not measurable: no capability/context resolve,
/// no dispatch, no K0 work, no I/O exist anywhere in `probe_stage`.
#[test]
fn steady_state_processing_allocates_zero() {
    // The probe state is process-global: every armed window — including
    // this direct measurement, which never spawns an episode — holds the
    // lifecycle lock so no other armed test's worker can observe it.
    let _lifecycle = test_common::lifecycle_lock();
    let _probe = gain_probe::arm(0.5);
    let mut block = vec![0.25f32; 1024 * 2];
    // Warm the path once outside the window (first-touch page faults and
    // any one-time lazy state are not steady-state processing).
    gain_probe::probe_stage(&mut block).expect("warm-up stage succeeds");
    let (_, allocations) =
        crate::edge_lifecycle_tests::counting_allocator::run_counting_allocations(|| {
            for _ in 0..10_000 {
                gain_probe::probe_stage(&mut block).expect("stage succeeds");
            }
        });
    assert_eq!(
        allocations, 0,
        "steady-state processing must allocate nothing per block"
    );
}

// --- conservation oracles ------------------------------------------------

/// Unity is bit-exact: `×1.0` is the IEEE-754 identity, so the consumed
/// stream must equal the raw position-tagged source frame for frame.
#[test]
fn unity_gain_preserves_samples_bit_exact() {
    let _lifecycle = test_common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let _probe = gain_probe::arm(1.0);
        let source_frames = 20_000;
        let (witnesses, handle, mut runtime) =
            episode(source_frames, OutputBehavior::Consume, Vec::new());
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_processed_exactly(&witnesses.content(), 1.0, source_frames);
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// Zero gain is true silence that conserves the stream's shape: every
/// frame arrives exactly +0.0, with the frame count untouched (gain
/// mutes samples, never frames).
#[test]
fn zero_gain_is_silence_with_frame_conservation() {
    let _lifecycle = test_common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let _probe = gain_probe::arm(0.0);
        let source_frames = 20_000;
        let (witnesses, handle, mut runtime) =
            episode(source_frames, OutputBehavior::Consume, Vec::new());
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_processed_exactly(&witnesses.content(), 0.0, source_frames);
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- the seam tracer -----------------------------------------------------

/// The I0 tracer bullet: half gain through the REAL decode-worker
/// staging seam. Two seconds of source — far past the 8192-frame edge
/// capacity — with a slow consumer, so the producer runs its bounded-
/// slice write loop against a full edge: partial edge acceptance of
/// already-processed blocks is structural here, not incidental. The
/// exact oracle leaves no room for a wrong seam: any skipped, doubled,
/// dropped or reprocessed staging content fails the frame-index check.
#[test]
fn half_gain_processes_the_real_staging_seam() {
    let _lifecycle = test_common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let _probe = gain_probe::arm(0.5);
        let source_frames = TEST_RATE * 2;
        let (witnesses, handle, mut runtime) = episode(
            source_frames,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            Vec::new(),
        );
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "gain must not disturb the episode's ordinary EOF course"
        );
        assert_processed_exactly(&witnesses.content(), 0.5, source_frames);
        let observation = handle.observe();
        assert_eq!(
            observation.source_format,
            Some(test_common::TEST_FORMAT),
            "format conservation: processing must not disturb the source format"
        );
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}
