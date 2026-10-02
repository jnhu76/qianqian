//! Live Audio Processing oracles (campaign #190 D3 + D4). Every test
//! here drives the REAL Playback Session composition — the real decode
//! worker loop, staging placement, PcmEdge partial writes, seek/pause
//! protocol and terminal settlement — with the PRODUCTION live
//! mechanism ([`crate::live::LiveProcessing`] + [`ProcessingControl`])
//! at the processing seam. The D3 oracles were retargeted from the
//! disposable probe to this mechanism (the probe is deleted); the D4
//! oracles add the typed application API, the production-path
//! collisions and the performance/allocation evidence.
//!
//! Oracle style: the consumed content is compared against references
//! computed from the position-tagged source (sample = frame index), so
//! the pre-transition stretch must be BIT-EXACTLY the old
//! configuration's continuation, the transition stretch must be the
//! frame-indexed linear blend `w·old + (1-w)·new` of two REAL
//! reference processors, and the post-settle stretch must be BIT-EXACTLY
//! a fresh instance of the accepted configuration. Frame conservation
//! is asserted everywhere by the reference length itself.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use qianqian_audio_api::ports::ProviderSeekOutcome;

use crate::live::ProcessingControl;
use crate::live::tap::{LiveEvent, LiveTap};
use crate::presets::EqPreset;
use crate::processing::AudioProcessingConfig;
use crate::processing::EpisodeProcessing;
use crate::processing_support::{DUMMY_PATH, EIGHT_SECONDS, TEST_RATE, Witnesses, wait_until};
use crate::session::{EDGE_CAPACITY_FRAMES, STAGING_FRAMES, playback_session_spec_with_live_tap};
use crate::test_common::{self, OutputBehavior, SourceBehavior, TestDecode, TestOutput};

fn preset_config(preset: EqPreset) -> AudioProcessingConfig {
    preset.to_config()
}

fn bypass_config() -> AudioProcessingConfig {
    AudioProcessingConfig::BYPASS
}

fn format() -> qianqian_audio_api::ports::PcmFormat {
    test_common::TEST_FORMAT
}

/// The oracle harness over the production mechanism: the shared
/// product-control cell (the same one the handle's typed `set_*`
/// commands route into) plus the engine's instrumentation tap. The
/// whole-config `request_update` below goes through the SAME command
/// boundary the typed commands use — one intrinsic-validation point,
/// one latest-wins pending slot, one engine.
struct LiveHarness {
    control: Arc<ProcessingControl>,
    tap: LiveTap,
}

impl LiveHarness {
    /// Route a whole desired configuration exactly as a typed `set_*`
    /// command does (intrinsic validation, desired+pending commit
    /// included).
    fn request_update(&self, desired: AudioProcessingConfig) {
        self.control
            .route_whole(desired)
            .expect("the oracle's update is intrinsically valid");
    }

    fn pending(&self) -> Option<AudioProcessingConfig> {
        self.control.pending()
    }

    fn processed_frames(&self) -> usize {
        self.tap.processed_frames()
    }

    fn events(&self) -> Vec<LiveEvent> {
        self.tap.events()
    }

    /// The D3/D4 deliberate-defect knobs; the two newer D4 mutants are
    /// armed explicitly by their own oracles.
    fn arm_mutations(&self, keep_transition_on_invalidate: bool, wallclock_ramp: bool) {
        self.tap
            .arm(keep_transition_on_invalidate, wallclock_ramp, false);
    }
}

/// A live episode: the standard doubles + the real session carrying the
/// PRODUCTION live mechanism at the seam, with the oracle tap attached.
#[allow(clippy::too_many_arguments)]
fn live_episode(
    source_frames: usize,
    output: OutputBehavior,
    initial: AudioProcessingConfig,
    transition_frames: usize,
    seeks: Vec<ProviderSeekOutcome>,
) -> (
    Witnesses,
    crate::handle::PlaybackSessionHandle,
    LiveHarness,
    qianqian_app::QianqianApp,
) {
    live_episode_source(
        SourceBehavior::EofAfter(source_frames),
        output,
        initial,
        transition_frames,
        seeks,
    )
}

/// [`live_episode`] with the caller choosing the decode source behavior
/// (the pacing/fragmentation oracles).
#[allow(clippy::too_many_arguments)]
fn live_episode_source(
    source: SourceBehavior,
    output: OutputBehavior,
    initial: AudioProcessingConfig,
    transition_frames: usize,
    seeks: Vec<ProviderSeekOutcome>,
) -> (
    Witnesses,
    crate::handle::PlaybackSessionHandle,
    LiveHarness,
    qianqian_app::QianqianApp,
) {
    let (witnesses, handle, harness, mut runtime) =
        live_episode_parts(source, output, initial, transition_frames, seeks);
    revise_composition(&mut runtime);
    (witnesses, handle, harness, runtime)
}

/// [`live_episode_source`] WITHOUT the activation: every component is
/// registered but the composition is not yet revised, so an oracle can
/// slip a typed command into the establishment→activation window — the
/// product constructor's exact shape (establishment at construction,
/// activation when the composition runs).
#[allow(clippy::too_many_arguments)]
fn live_episode_parts(
    source: SourceBehavior,
    output: OutputBehavior,
    initial: AudioProcessingConfig,
    transition_frames: usize,
    seeks: Vec<ProviderSeekOutcome>,
) -> (
    Witnesses,
    crate::handle::PlaybackSessionHandle,
    LiveHarness,
    qianqian_app::QianqianApp,
) {
    let tap = LiveTap::disarmed();
    let witnesses = Witnesses::new();
    let handle = crate::handle::PlaybackSessionHandle::new();
    let mut runtime = qianqian_app::QianqianApp::new();

    let decode = TestDecode {
        behavior: source,
        duration: None,
        seeks,
    };
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

    let consumed = witnesses.consumed_arc();
    let consumed_values = witnesses.values_arc();
    let consumed_values_ch1 = witnesses.values_ch1_arc();
    let output_device = output;
    runtime
        .register_component({
            qianqian_composition::ComponentSpec::new("test_output_plugin")
                .provides::<qianqian_audio_api::ports::AudioOutputCapability>()
                .on_activate(move |ctx| {
                    ctx.provide::<qianqian_audio_api::ports::AudioOutputCapability>(
                        std::rc::Rc::new(TestOutput::observed_with_stereo_content(
                            output_device,
                            consumed.clone(),
                            consumed_values.clone(),
                            consumed_values_ch1.clone(),
                            test_common::DeviceTail::default(),
                        )),
                    )
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("output provider registers");

    runtime
        .register_component(playback_session_spec_with_live_tap(
            std::path::PathBuf::from(DUMMY_PATH),
            handle.clone(),
            initial,
            transition_frames,
            tap.clone(),
        ))
        .expect("session registers");

    let harness = LiveHarness {
        control: handle.completion.processing(),
        tap,
    };
    (witnesses, handle, harness, runtime)
}

/// Revise (activate) a [`live_episode_parts`] composition — the step an
/// oracle deliberately DEFERS when it needs the establishment→activation
/// window.
fn revise_composition(runtime: &mut qianqian_app::QianqianApp) {
    runtime
        .revise_desired(vec![
            crate::processing_support::desired("decode", "test_decode_plugin"),
            crate::processing_support::desired("output", "test_output_plugin"),
            crate::processing_support::desired("session", "playback_session"),
        ])
        .expect("composition is legal");
}

/// The source input stream, interleaved stereo, position-tagged exactly
/// as the mock decoder tags it (sample = frame index; channel 1 adds
/// 0.5).
fn source_input(frames: usize, from: usize) -> Vec<f32> {
    (from..from + frames)
        .flat_map(|i| [i as f32, i as f32 + 0.5])
        .collect()
}

/// The linear blend weight the engine applies at consumed frame `i` of
/// a transition that started at `start` and runs `total` frames.
fn blend_weight(i: usize, start: usize, total: usize) -> f32 {
    let local = i - start;
    if local >= total {
        0.0
    } else {
        (1.0 - (local as f64 / total as f64)) as f32 + 0.0
    }
}

/// The full expected channel-0 content of a live episode. Reference
/// construction mirrors the engine exactly:
///
/// - the OLD side of each transition is the LIVE continuation: the old
///   configuration's processor warmed from the previous segment
///   boundary (for stateful EQ this is NOT the same as a fresh compile
///   at the transition start), staged across the transition and beyond;
/// - the NEW side is a fresh instance of the accepted configuration
///   started exactly at the transition start (which is what the engine
///   runs — so the settled stretch is bit-exactly that stream);
/// - the transition stretch is the frame-indexed linear blend
///   `w·old + (1-w)·new` with the engine's exact formula.
fn expected_content(
    source_frames: usize,
    transition_frames: usize,
    initial: &AudioProcessingConfig,
    transitions: &[(usize, AudioProcessingConfig, AudioProcessingConfig)],
) -> Vec<f32> {
    expected_interleaved(source_frames, transition_frames, initial, transitions)
        .iter()
        .step_by(2)
        .copied()
        .collect()
}

/// The channel-1 view of the same reference (the +0.5-tagged stream) —
/// per-channel processing misapplication through a live transition fails
/// here even when channel 0 looks right.
fn expected_content_ch1(
    source_frames: usize,
    transition_frames: usize,
    initial: &AudioProcessingConfig,
    transitions: &[(usize, AudioProcessingConfig, AudioProcessingConfig)],
) -> Vec<f32> {
    expected_interleaved(source_frames, transition_frames, initial, transitions)
        .into_iter()
        .skip(1)
        .step_by(2)
        .collect()
}

/// The full expected INTERLEAVED content of a live episode, both
/// channels independently carried through the same blend law.
fn expected_interleaved(
    source_frames: usize,
    transition_frames: usize,
    initial: &AudioProcessingConfig,
    transitions: &[(usize, AudioProcessingConfig, AudioProcessingConfig)],
) -> Vec<f32> {
    let mut expected = vec![0.0f32; source_frames * 2];
    let mut prev_start = 0usize;
    let mut prev_total = 0usize;
    for &(start, ref old, ref new) in transitions.iter() {
        // A transition start is a PROCESSED-frame witness; the reference
        // models CONSUMED audio only (a stopped episode can end while the
        // edge still holds processed-but-unconsumed frames), so a start
        // at or beyond the consumed length produced no audible
        // transition at all.
        let audible_start = start.min(source_frames);
        // The OLD side: warmed from the previous segment boundary by
        // staging the running configuration up to the audible start,
        // then continued across the whole tail with the SAME instance.
        // The warm-up writes only the PURE stretch — the previous
        // transition's blend stretch stays what that blend computed.
        let mut old_processor = EpisodeProcessing::new(old, &format()).expect("compiles");
        if audible_start > prev_start {
            let mut warm = source_input(audible_start - prev_start, prev_start);
            old_processor.stage(&mut warm).expect("stages");
            let skip = prev_total.min(audible_start - prev_start);
            for (i, frame) in warm.chunks(2).enumerate().skip(skip) {
                expected[(prev_start + i) * 2] = frame[0];
                expected[(prev_start + i) * 2 + 1] = frame[1];
            }
        }
        if start < source_frames {
            let mut old_tail = source_input(source_frames - start, start);
            old_processor.stage(&mut old_tail).expect("stages");

            // The NEW side: a fresh instance at the transition start.
            let mut new_processor = EpisodeProcessing::new(new, &format()).expect("compiles");
            let mut new_tail = source_input(source_frames - start, start);
            new_processor.stage(&mut new_tail).expect("stages");

            // The blend stretch overwrites the old side's tail prefix.
            let total = transition_frames;
            for i in start..(start + total).min(source_frames) {
                let w = blend_weight(i, start, total);
                for c in 0..2 {
                    let a = old_tail[(i - start) * 2 + c];
                    let b = new_tail[(i - start) * 2 + c];
                    expected[i * 2 + c] = w.mul_add(a, (1.0 - w).mul_add(b, 0.0));
                }
            }
            // Beyond the blend the new side IS the continuation.
            for i in (start + total).min(source_frames)..source_frames {
                expected[i * 2] = new_tail[(i - start) * 2];
                expected[i * 2 + 1] = new_tail[(i - start) * 2 + 1];
            }
        }
        prev_start = start;
        prev_total = transition_frames;
    }
    // A transition-free prefix (no transitions at all): the initial
    // configuration renders everything.
    if transitions.is_empty() {
        let mut processor = EpisodeProcessing::new(initial, &format()).expect("compiles");
        let mut input = source_input(source_frames, 0);
        processor.stage(&mut input).expect("stages");
        for (i, frame) in input.chunks(2).enumerate() {
            expected[i * 2] = frame[0];
            expected[i * 2 + 1] = frame[1];
        }
    }
    expected
}

const HALF_A_SECOND: u64 = 22_050;

/// Wait until the probe reports its first transition started, and
/// return the (start_frame, block) it recorded.
fn wait_transition_started(control: &LiveHarness, limit: Duration) -> (usize, usize) {
    assert!(
        wait_until(limit, || control
            .events()
            .iter()
            .any(|e| matches!(e, LiveEvent::TransitionStarted { .. }))),
        "no transition ever started"
    );
    match control
        .events()
        .into_iter()
        .find(|e| matches!(e, LiveEvent::TransitionStarted { .. }))
        .expect("checked")
    {
        LiveEvent::TransitionStarted { at_frame, block } => (at_frame, block),
        _ => unreachable!("filtered"),
    }
}

// --- the core coherence + apply-boundary oracle ---------------------------

/// A live preset switch: the consumed stream is EXACTLY the old
/// configuration up to the recorded transition frame (bit-exact — the
/// whole-staging-block apply boundary), exactly the frame-indexed
/// linear blend of two REAL reference processors through the transition,
/// and EXACTLY a fresh instance of the accepted configuration after it.
/// Frame count is conserved end to end. This is the master oracle the
/// negative controls are built against.
#[test]
fn a_live_preset_switch_is_coherent_whole_block_and_bounded() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let initial = preset_config(EqPreset::Flat);
        let desired = preset_config(EqPreset::Rock);
        let (witnesses, handle, control, mut runtime) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            initial,
            TEST_RATE / 10,
            Vec::new(),
        );

        // Fire the update early in the episode.
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("the episode never started consuming");
        control.request_update(desired);
        let (start, block) = wait_transition_started(&control, Duration::from_secs(5));

        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );
        let values = witnesses.content();

        // Whole-block apply boundary: the transition starts at a
        // staging-block boundary (the pickup runs between blocks).
        assert_eq!(
            start % STAGING_FRAMES,
            0,
            "the apply boundary must be a whole staging block"
        );
        assert_eq!(block, start / STAGING_FRAMES, "block bookkeeping agrees");
        assert!(
            start > HALF_A_SECOND as usize / 8,
            "the update must have been picked up after the request"
        );

        let expected = expected_content(
            EIGHT_SECONDS,
            TEST_RATE / 10,
            &initial,
            &[(start, initial, desired)],
        );
        assert_eq!(
            values.len(),
            EIGHT_SECONDS,
            "frame conservation across the live transition"
        );
        // Pre-transition: bit-exact old continuation (the strongest
        // form of "never half-old/half-new, never touched
        // retroactively").
        assert_eq!(
            &values[..start],
            &expected[..start],
            "pre-transition content must be bit-exactly the old \
                 configuration's continuation"
        );
        // Transition stretch: the exact blend reference (f32 rounding
        // tolerance only — the blend arithmetic itself is the engine's).
        let total = TEST_RATE / 10;
        let blend_end = (start + total).min(EIGHT_SECONDS);
        for (i, (got, want)) in values[start..blend_end]
            .iter()
            .zip(expected[start..blend_end].iter())
            .enumerate()
        {
            let tolerance = 1e-4 * want.abs().max(1.0);
            assert!(
                (got - want).abs() <= tolerance,
                "frame {} (transition+{i}): {got} vs expected blend {want}",
                i + start
            );
        }
        // Settled stretch: BIT-EXACTLY the fresh accepted configuration
        // (the to-side kept at settle is exactly that instance —
        // tolerance here could mask a weight or coefficient defect).
        assert_eq!(
            &values[blend_end..],
            &expected[blend_end..],
            "post-settle content must be bit-exactly the accepted \
             configuration's continuation"
        );
        // Channel 1 rode the same law: compare its full content against
        // the channel-1 reference view.
        let expected_ch1 = expected_content_ch1(
            EIGHT_SECONDS,
            TEST_RATE / 10,
            &initial,
            &[(start, initial, desired)],
        );
        assert_eq!(
            witnesses.content_ch1(),
            expected_ch1,
            "channel 1 must carry the identical transition law"
        );
        let _ = runtime.dispose();
    });
}

/// The gain transition: unity → attenuation → unity rides the same
/// frame-indexed blend law (a scalar gain crossfade is exactly the
/// interpolated gain), both channels independently, with a mid-episode
/// return to unity.
#[test]
fn a_live_gain_transition_blends_exactly_on_both_channels() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let initial = AudioProcessingConfig::gain(1.0);
        let down = AudioProcessingConfig::gain(0.25);
        let (witnesses, handle, control, mut runtime) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            initial,
            TEST_RATE / 10,
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        control.request_update(down);
        let (start1, _) = wait_transition_started(&control, Duration::from_secs(5));

        // Wait for settle, then go back to unity.
        assert!(
            wait_until(Duration::from_secs(5), || control
                .events()
                .iter()
                .any(|e| matches!(e, LiveEvent::TransitionCompleted { .. }))),
            "the first transition never settled"
        );
        control.request_update(initial);
        assert!(
            wait_until(Duration::from_secs(5), || {
                control
                    .events()
                    .iter()
                    .filter(|e| matches!(e, LiveEvent::TransitionStarted { .. }))
                    .count()
                    >= 2
            }),
            "the second update must start a second transition"
        );
        let start2 = match control
            .events()
            .into_iter()
            .filter(|e| matches!(e, LiveEvent::TransitionStarted { .. }))
            .nth(1)
            .expect("checked")
        {
            LiveEvent::TransitionStarted { at_frame, .. } => at_frame,
            _ => unreachable!(),
        };
        let _ = start1;

        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );
        let values = witnesses.content();
        let expected = expected_content(
            EIGHT_SECONDS,
            TEST_RATE / 10,
            &initial,
            &[(start1, initial, down), (start2, down, initial)],
        );
        assert_eq!(values.len(), EIGHT_SECONDS, "frame conservation");
        for (i, (got, want)) in values.iter().zip(expected.iter()).enumerate() {
            let tolerance = 1e-4 * want.abs().max(1.0);
            assert!(
                (got - want).abs() <= tolerance,
                "frame {i}: {got} vs expected {want}"
            );
        }
        // Channel 1: the same interpolated-gain law on the +0.5-tagged
        // stream, independently.
        let expected_ch1 = expected_content_ch1(
            EIGHT_SECONDS,
            TEST_RATE / 10,
            &initial,
            &[(start1, initial, down), (start2, down, initial)],
        );
        assert_eq!(
            witnesses.content_ch1(),
            expected_ch1,
            "channel 1 must carry the identical interpolated-gain law"
        );
        let _ = runtime.dispose();
    });
}

/// Bypass is a live operation too, and it is NOT `gain = 0`: the
/// transition blends between the processed stream and the untouched dry
/// stream, both REAL references.
#[test]
fn a_live_bypass_toggle_blends_between_processed_and_dry() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let initial = preset_config(EqPreset::Bass);
        let off = bypass_config();
        let (witnesses, handle, control, mut runtime) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            initial,
            TEST_RATE / 10,
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        control.request_update(off);
        let (start, _) = wait_transition_started(&control, Duration::from_secs(5));
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );

        let values = witnesses.content();
        let expected = expected_content(
            EIGHT_SECONDS,
            TEST_RATE / 10,
            &initial,
            &[(start, initial, bypass_config())],
        );
        for (i, (got, want)) in values.iter().zip(expected.iter()).enumerate() {
            let tolerance = 1e-4 * want.abs().max(1.0);
            assert!(
                (got - want).abs() <= tolerance,
                "frame {i}: {got} vs expected {want}"
            );
        }
        // Post-settle: bypass is bit-exact dry (the untouched source).
        let settle = start + TEST_RATE / 10;
        for (i, got) in values[settle..].iter().enumerate() {
            let dry = ((settle + i) as f32) * 1.0;
            assert_eq!(*got, dry, "post-settle bypass must be bit-exact dry");
        }
        let _ = runtime.dispose();
    });
}

// --- the protocol collision oracles ---------------------------------------

/// RefusedUnchanged during an in-flight transition: the continuation is
/// exactly the run's own no-seek path — the consumed content equals the
/// deterministic reference (old config up to the transition, the exact
/// blend through it, the accepted config after it) BIT-EXACTLY, which is
/// the zero-content-loss invariant extended to transition state. (The
/// reference, not a second run: a transition's start frame is
/// timing-dependent across runs, so cross-run content comparison would
/// be unsound.)
#[test]
fn a_refused_seek_during_a_transition_preserves_the_no_seek_continuation() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(90), move || {
        let initial = preset_config(EqPreset::Flat);
        let desired = preset_config(EqPreset::Rock);

        let (seeked_w, seeked_handle, seeked_probe, mut seeked_runtime) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            initial,
            TEST_RATE / 5,
            vec![ProviderSeekOutcome::RefusedUnchanged],
        );
        wait_until(Duration::from_secs(5), || {
            seeked_handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        seeked_probe.request_update(desired);
        let (start, _) = wait_transition_started(&seeked_probe, Duration::from_secs(5));
        seeked_handle.request_seek(Duration::from_secs(5));
        assert_eq!(
            seeked_handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed,
            "a refusal is not a failure and not a cut"
        );

        let values = seeked_w.content();
        let expected = expected_content(
            EIGHT_SECONDS,
            TEST_RATE / 5,
            &initial,
            &[(start, initial, desired)],
        );
        assert_eq!(
            values, expected,
            "the refused seek must be content-indistinguishable from the \
             run's own no-seek continuation, transition included"
        );
        let _ = seeked_runtime.dispose();
    });
}

/// Applied seek during an in-flight transition: the cut discards the
/// transition entirely, the pre-cut stretch stays content-identical to
/// the run's own deterministic no-seek continuation (reference-built
/// from the run's own transition start, so the check does not depend on
/// timing across runs), and the post-cut stretch is BIT-EXACTLY a fresh
/// instance of the ACCEPTED configuration fed the exact post-landing
/// tags — no pre-cut contribution, no old-transition audio.
#[test]
fn an_applied_seek_during_a_transition_lands_fresh_under_the_accepted_config() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(90), move || {
        let initial = preset_config(EqPreset::Flat);
        let desired = preset_config(EqPreset::Rock);
        let five_seconds = 5 * TEST_RATE;
        let landing_frames = EIGHT_SECONDS - five_seconds;

        // The seeked run.
        let (w, handle, probe, mut runtime) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            initial,
            TEST_RATE / 5,
            vec![ProviderSeekOutcome::Applied {
                landing: Some((5 * TEST_RATE) as u64),
            }],
        );
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        probe.request_update(desired);
        let (start, _) = wait_transition_started(&probe, Duration::from_secs(5));
        handle.request_seek(Duration::from_secs(5));
        assert!(
            wait_until(Duration::from_secs(5), || handle
                .observe()
                .position
                .is_some_and(|p| p >= five_seconds as u64)),
            "the position never rebased to the landing"
        );
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );

        // The invalidation dropped the transition.
        assert!(
            probe
                .events()
                .iter()
                .any(|e| matches!(e, LiveEvent::InvalidationDroppedTransition)),
            "the Applied cut must drop the in-flight transition"
        );

        let values = w.content();
        // The run's own no-seek continuation (built from ITS transition
        // start): the pre-cut stretch must be bit-identical to it, and
        // the cut is exactly where the landing diverges.
        let reference = expected_content(
            EIGHT_SECONDS,
            TEST_RATE / 5,
            &initial,
            &[(start, initial, desired)],
        );
        let cut = values
            .iter()
            .zip(reference.iter())
            .position(|(a, c)| a != c)
            .expect("the applied cut must appear as a divergence from the run's own continuation");
        assert_eq!(
            &values[..cut],
            &reference[..cut],
            "the pre-cut stretch must be content-identical to the \
             run's own no-seek continuation, transition included"
        );
        assert_eq!(
            values.len(),
            cut + landing_frames,
            "the post-cut stretch conserves its frame count exactly"
        );
        // Fresh accepted-configuration stage over the exact
        // post-landing tags.
        let mut fresh = EpisodeProcessing::new(&desired, &format()).expect("compiles");
        let mut post_input = source_input(landing_frames, five_seconds);
        fresh.stage(&mut post_input).expect("stages");
        let expected: Vec<f32> = post_input.chunks(2).map(|f| f[0]).collect();
        assert_eq!(
            &values[cut..],
            expected.as_slice(),
            "post-cut content must equal a fresh instance of the \
                 ACCEPTED configuration, bit-exactly"
        );
        let _ = runtime.dispose();
    });
}

/// Pause mid-transition: bounded prefetch continues, the ramp advances
/// only on processed samples, and the consumed stream equals the run's
/// own deterministic no-pause continuation bit-exactly.
#[test]
fn a_pause_during_a_transition_advances_only_on_processed_samples() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(90), move || {
        let initial = preset_config(EqPreset::Flat);
        let desired = preset_config(EqPreset::Rock);

        let (paused_w, paused_handle, paused_probe, mut paused_runtime) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(2),
            },
            initial,
            TEST_RATE,
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            paused_handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        paused_probe.request_update(desired);
        let (start, _) = wait_transition_started(&paused_probe, Duration::from_secs(5));
        paused_handle.request_pause();
        assert!(
            wait_until(Duration::from_secs(5), || paused_handle.observe().paused()),
            "the Paused projection never established"
        );
        assert!(
            wait_until(Duration::from_secs(5), || paused_handle
                .completion
                .buffered_frames()
                == Some(EDGE_CAPACITY_FRAMES)),
            "the edge never filled while paused"
        );
        // Bounded prefetch: with the edge full and the leg parked, the
        // worker stops staging — processed frames hold steady.
        let at_pause = paused_probe.processed_frames();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            at_pause,
            paused_probe.processed_frames(),
            "no PCM is processed while the edge is full and the leg is \
             parked (bounded prefetch)"
        );
        // D14.8: the Position does not advance merely because DSP
        // processed future PCM.
        let position_at_pause = paused_handle.observe().position;
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            paused_handle.observe().position,
            position_at_pause,
            "Position must not advance while paused"
        );

        paused_handle.request_resume();
        assert_eq!(
            paused_handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );
        let values = paused_w.content();
        let expected = expected_content(
            EIGHT_SECONDS,
            TEST_RATE,
            &initial,
            &[(start, initial, desired)],
        );
        assert_eq!(
            values, expected,
            "pause must leave the transition content-indistinguishable \
             from the run's own no-pause continuation — the ramp advanced \
             only on processed samples"
        );
        let _ = paused_runtime.dispose();
    });
}

// --- refusals, rapid updates, replacement, failure ------------------------

/// An invalid desired update is REFUSED, honestly, at the command
/// boundary (D4: coherent acceptance validates the WHOLE candidate
/// before anything moves): the typed setter reports the refusal, the
/// observation carries the diagnostic as mechanism evidence, and the old
/// configuration continues bit-exactly — never a processing failure,
/// never a terminal, never a partial config.
#[test]
fn an_invalid_update_is_refused_and_the_old_config_continues() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let initial = preset_config(EqPreset::Flat);

        let (witnesses, handle, probe, mut runtime) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            initial,
            TEST_RATE / 10,
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");

        // The typed command path: a NaN preamp is refused at the
        // command boundary with an honest diagnostic.
        let refusal = handle
            .set_preamp(f32::NAN)
            .expect_err("a NaN preamp must be refused");
        assert!(
            refusal.contains("finite"),
            "the refusal names the invalidity: {refusal}"
        );
        // And an out-of-band EQ trim is refused the same way.
        let mut wild = crate::processing::EqConfig::FLAT.band_gain_db;
        wild[3] = 99.0;
        let refusal = handle
            .set_eq_config(crate::processing::EqConfig::new(wild, 1.0))
            .expect_err("an out-of-band trim must be refused");
        assert!(
            refusal.contains("product bound"),
            "the refusal names the invalidity: {refusal}"
        );
        // The observation carries the last refusal as mechanism
        // evidence, for a client that did not capture the return value.
        assert!(
            wait_until(Duration::from_secs(5), || handle
                .observe()
                .last_processing_refusal
                .is_some()),
            "the refusal must be observable through the episode seam"
        );
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed,
            "a refusal is not a failure and not a terminal"
        );

        // No transition ever started; the content is the pure old
        // configuration, bit-exactly.
        assert!(
            !probe
                .events()
                .iter()
                .any(|e| matches!(e, LiveEvent::TransitionStarted { .. })),
            "a refused update must not start a transition"
        );
        assert!(
            probe.pending().is_none(),
            "a refused update must not occupy the pending slot"
        );
        let mut old_ref = EpisodeProcessing::new(&initial, &format()).expect("compiles");
        let mut input = source_input(EIGHT_SECONDS, 0);
        old_ref.stage(&mut input).expect("stages");
        let expected: Vec<f32> = input.chunks(2).map(|f| f[0]).collect();
        assert_eq!(
            witnesses.content(),
            expected,
            "the old config continues bit-exactly"
        );
        let _ = runtime.dispose();
    });
}

/// Rapid successive updates follow the deterministic policy:
/// complete-in-flight, latest-wins pending (depth-1 slot). Two requests
/// landing before the first pickup coalesce into ONE accepted
/// transition to the LATEST desired configuration. The precondition
/// ("before the first pickup") is established deterministically: with
/// the edge full and the leg parked, the worker cannot reach the
/// fresh-block pickup, so both commands land in the slot before any
/// pickup runs — the race is scripted out, not probabilistic.
#[test]
fn rapid_updates_coalesce_to_the_latest_desired() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let initial = preset_config(EqPreset::Flat);
        let rock = preset_config(EqPreset::Rock);
        let jazz = preset_config(EqPreset::Jazz);

        let (witnesses, handle, probe, mut runtime) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            initial,
            TEST_RATE / 10,
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        handle.request_pause();
        assert!(
            wait_until(Duration::from_secs(5), || handle.observe().paused()),
            "the Paused projection never established"
        );
        assert!(
            wait_until(Duration::from_secs(5), || handle
                .completion
                .buffered_frames()
                == Some(EDGE_CAPACITY_FRAMES)),
            "the edge never filled while paused"
        );
        let parked_at = probe.processed_frames();
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(
            parked_at,
            probe.processed_frames(),
            "parked: the worker stages nothing, so no pickup can run"
        );
        probe.request_update(rock);
        probe.request_update(jazz);
        assert_eq!(
            probe.pending().as_ref(),
            Some(&jazz),
            "the slot is depth-1, latest wins"
        );
        handle.request_resume();
        let (start, _) = wait_transition_started(&probe, Duration::from_secs(5));
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );

        let started: Vec<usize> = probe
            .events()
            .into_iter()
            .filter_map(|e| match e {
                LiveEvent::TransitionStarted { at_frame, .. } => Some(at_frame),
                _ => None,
            })
            .collect();
        assert_eq!(
            started,
            vec![start],
            "both requests must coalesce into exactly ONE accepted \
                 transition (complete-in-flight, latest-wins)"
        );
        let expected = expected_content(
            EIGHT_SECONDS,
            TEST_RATE / 10,
            &initial,
            &[(start, initial, jazz)],
        );
        let values = witnesses.content();
        for (i, (got, want)) in values.iter().zip(expected.iter()).enumerate() {
            let tolerance = 1e-4 * want.abs().max(1.0);
            assert!(
                (got - want).abs() <= tolerance,
                "frame {i}: {got} vs expected {want}"
            );
        }
        let _ = runtime.dispose();
    });
}

/// An accepted update applied in one episode does not leak into the
/// next: the replacement episode compiles fresh under its own snapshot
/// (the D14.11 Open/replacement obligation, with live state in the old
/// episode).
#[test]
fn a_new_episode_starts_fresh_after_a_live_updated_episode() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(90), move || {
        let initial = preset_config(EqPreset::Flat);
        let rock = preset_config(EqPreset::Rock);

        // Episode one: live switch to Rock, completes.
        let (_, handle, probe, mut runtime_one) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            initial,
            TEST_RATE / 10,
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        probe.request_update(rock);
        let _ = wait_transition_started(&probe, Duration::from_secs(5));
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );
        let _ = runtime_one.dispose();

        // Episode two: established with Rock as its INITIAL config —
        // it must render exactly a fresh Rock stream, with nothing
        // carried from the previous episode's transition.
        let (w, handle_two, _, mut runtime_two) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            rock,
            TEST_RATE / 10,
            Vec::new(),
        );
        assert_eq!(
            handle_two.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );
        let mut fresh = EpisodeProcessing::new(&rock, &format()).expect("compiles");
        let mut input = source_input(EIGHT_SECONDS, 0);
        fresh.stage(&mut input).expect("stages");
        let expected: Vec<f32> = input.chunks(2).map(|f| f[0]).collect();
        assert_eq!(w.content(), expected, "the new episode renders fresh Rock");
        let _ = runtime_two.dispose();
    });
}

/// A processing failure DURING an accepted transition (the failure is
/// injected into the old side, so it fires after acceptance while the
/// crossfade is running) settles through the ordinary D11 `Failed`
/// route with a truthful processing-origin diagnostic — never a decode
/// label, never a resurrected continuation.
#[test]
fn a_failure_during_a_transition_settles_failed_through_d11() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(90), move || {
        let witnesses = Witnesses::new();
        let handle = crate::handle::PlaybackSessionHandle::new();
        let mut runtime = qianqian_app::QianqianApp::new();

        let decode = TestDecode {
            behavior: SourceBehavior::EofAfter(EIGHT_SECONDS),
            duration: None,
            seeks: Vec::new(),
        };
        runtime
            .register_component({
                qianqian_composition::ComponentSpec::new("test_decode_plugin")
                    .provides::<qianqian_audio_api::ports::PcmDecodeCapability>()
                    .on_activate(move |ctx| {
                        ctx.provide::<qianqian_audio_api::ports::PcmDecodeCapability>(
                            std::rc::Rc::new(decode.clone()),
                        )
                        .map_err(|e| {
                            qianqian_composition::ActivationError::new(format!("{e:?}"))
                        })?;
                        Ok(())
                    })
            })
            .expect("decode registers");
        let consumed = witnesses.consumed_arc();
        let values = witnesses.values_arc();
        let values_ch1 = witnesses.values_ch1_arc();
        let device = OutputBehavior::Consume;
        runtime
            .register_component({
                qianqian_composition::ComponentSpec::new("test_output_plugin")
                    .provides::<qianqian_audio_api::ports::AudioOutputCapability>()
                    .on_activate(move |ctx| {
                        ctx.provide::<qianqian_audio_api::ports::AudioOutputCapability>(
                            std::rc::Rc::new(TestOutput::observed_with_stereo_content(
                                device,
                                consumed.clone(),
                                values.clone(),
                                values_ch1.clone(),
                                test_common::DeviceTail::default(),
                            )),
                        )
                        .map_err(|e| {
                            qianqian_composition::ActivationError::new(format!("{e:?}"))
                        })?;
                        Ok(())
                    })
            })
            .expect("output registers");

        // The engine's INITIAL side is a deliberate processor that
        // fails once the transition begins (armed by the flag below).
        let tap = LiveTap::disarmed();
        let control = handle.completion.processing();
        let fail_after_transition = Arc::new(AtomicUsize::new(0));
        let fail_counter = fail_after_transition.clone();
        let control_for_engine = control.clone();
        let tap_for_engine = tap.clone();
        let build_engine = move |format: &qianqian_audio_api::ports::PcmFormat| {
            let initial_side = EpisodeProcessing::test_driven(
                Box::new(move |block: &mut [f32]| {
                    if fail_counter.load(Ordering::SeqCst) > 0 {
                        let _ = block;
                        return Err("live transition-side failure".to_owned());
                    }
                    Ok(())
                }),
                Box::new(|| {}),
            );
            crate::live::LiveProcessing::with_initial_processor(
                initial_side,
                preset_config(EqPreset::Flat),
                format,
                TEST_RATE,
                control_for_engine,
                tap_for_engine,
            )
        };
        runtime
            .register_component(crate::session::playback_session_spec_with_live_engine(
                std::path::PathBuf::from(DUMMY_PATH),
                handle.clone(),
                build_engine,
            ))
            .expect("session registers");
        runtime
            .revise_desired(vec![
                crate::processing_support::desired("decode", "test_decode_plugin"),
                crate::processing_support::desired("output", "test_output_plugin"),
                crate::processing_support::desired("session", "playback_session"),
            ])
            .expect("composition is legal");

        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        // Arm the failure and request the update: the acceptance
        // succeeds (the update is valid), the transition starts, and
        // the OLD side then fails inside its crossfade stage.
        fail_after_transition.store(1, Ordering::SeqCst);
        let harness = LiveHarness { control, tap };
        harness.request_update(preset_config(EqPreset::Rock));

        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Failed,
            "an unrecoverable transition-side processing failure \
                 settles through D11 Failed"
        );
        let diagnostic = handle
            .observe()
            .failure_diagnostic
            .expect("a failed episode carries its diagnostic");
        assert!(
            diagnostic.contains("live transition-side failure"),
            "the diagnostic stays truthful about the processing origin: \
                 {diagnostic}"
        );
        let _ = runtime.dispose();
    });
}

// --- the D3.7 negative controls -------------------------------------------

/// N1 — an update that REPROCESSES the preserved remainder violates the
/// remainder rule. The real worker flushes the already-processed
/// remainder untouched; this control builds the defect's output with
/// real machinery (the remainder staged under the NEW configuration on
/// top of the old side's processing) and proves the remainder oracle
/// rejects it.
#[test]
fn n1_the_remainder_oracle_catches_a_reprocessed_remainder() {
    use crate::live::mutant_remainder_reprocessed;
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        // A real remainder world: process a staging block under the
        // old config, keep its unwritten tail as the preserved
        // remainder (already PROCESSED PCM).
        let old = preset_config(EqPreset::Bass);
        let new = preset_config(EqPreset::Rock);
        let mut block = source_input(STAGING_FRAMES, 0);
        let mut old_side = EpisodeProcessing::new(&old, &format()).expect("compiles");
        old_side.stage(&mut block).expect("stages");
        let split = (STAGING_FRAMES - 313) * 2; // a mid-block cut
        let honest_remainder = block[split..].to_vec();

        // The mutant: the remainder staged under the NEW config —
        // double-processing the tail.
        let mutant_remainder = mutant_remainder_reprocessed(&honest_remainder, &new, &format());

        // The defect must be visible on this input...
        assert_ne!(
            honest_remainder, mutant_remainder,
            "Bass-then-Rock reprocessing must be observably different                  from the honest continuation"
        );
        // ...and the remainder oracle must reject the mutant world.
        assert!(crate::processing_support::rejects(|| {
            assert_eq!(
                honest_remainder, mutant_remainder,
                "the preserved remainder must be written exactly as \
                     processed — reprocessing it under the new config \
                     fails here"
            );
        }));
    });
}

/// N2 — a half-published configuration (part old bands, part new bands)
/// is caught: the realize-the-desired oracle rejects the mixed world's
/// output, which is a coherent render of the WRONG configuration.
#[test]
fn n2_the_coherence_oracle_catches_a_half_published_config() {
    use crate::live::mutant_mixed_config;
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let initial = preset_config(EqPreset::Flat);
        let desired = preset_config(EqPreset::Rock);
        let mixed = mutant_mixed_config(&initial, &desired);

        // Settled outputs of both worlds over the same input.
        let mut input = source_input(4096, 0);
        let mut desired_side = EpisodeProcessing::new(&desired, &format()).expect("compiles");
        desired_side.stage(&mut input).expect("stages");
        let desired_out: Vec<f32> = input.chunks(2).map(|f| f[0]).collect();

        let mut input = source_input(4096, 0);
        let mut mixed_side = EpisodeProcessing::new(&mixed, &format()).expect("compiles");
        mixed_side.stage(&mut input).expect("stages");
        let mixed_out: Vec<f32> = input.chunks(2).map(|f| f[0]).collect();

        assert_ne!(
            desired_out, mixed_out,
            "the mixed config must be observably different from the \
                 desired one for Rock-vs-Flat bands"
        );
        assert!(crate::processing_support::rejects(|| {
            assert_eq!(
                mixed_out, desired_out,
                "the settled output must be the DESIRED configuration's \
                     render — a half-published config fails here"
            );
        }));
    });
}

/// N3 — an Applied cut that fails to drop the in-flight transition
/// (pre-cut contribution leaking into post-cut presentation) is caught
/// by the fresh-landing oracle.
#[test]
fn n3_the_landing_oracle_catches_a_transition_that_survives_the_cut() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(90), move || {
        let initial = preset_config(EqPreset::Flat);
        let desired = preset_config(EqPreset::Rock);
        let five_seconds = 5 * TEST_RATE;
        let landing_frames = EIGHT_SECONDS - five_seconds;

        let (w, handle, probe, mut runtime) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            initial,
            TEST_RATE / 5,
            vec![ProviderSeekOutcome::Applied {
                landing: Some((5 * TEST_RATE) as u64),
            }],
        );
        // Arm the defect: the invalidation keeps the transition.
        probe.arm_mutations(true, false);
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        probe.request_update(desired);
        let (start, _) = wait_transition_started(&probe, Duration::from_secs(5));
        handle.request_seek(Duration::from_secs(5));
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );

        // The defect's world: no InvalidationDroppedTransition event.
        assert!(
            !probe
                .events()
                .iter()
                .any(|e| matches!(e, LiveEvent::InvalidationDroppedTransition)),
            "the mutant must exhibit the missing invalidation"
        );

        // The landing oracle: post-cut == fresh accepted config. The
        // cut is located the same way the production oracle locates it
        // (divergence from the run's own no-seek continuation), so the
        // mutant is judged by the shipped predicate, not a weaker one.
        let mut fresh = EpisodeProcessing::new(&desired, &format()).expect("compiles");
        let mut post_input = source_input(landing_frames, five_seconds);
        fresh.stage(&mut post_input).expect("stages");
        let expected: Vec<f32> = post_input.chunks(2).map(|f| f[0]).collect();
        let values = w.content();
        let reference = expected_content(
            EIGHT_SECONDS,
            TEST_RATE / 5,
            &initial,
            &[(start, initial, desired)],
        );
        let cut = values
            .iter()
            .zip(reference.iter())
            .position(|(a, c)| a != c)
            .expect("the Applied cut must still land as a divergence");
        assert_ne!(
            &values[cut..],
            expected.as_slice(),
            "the mutant world must observably differ from the fresh \
                 landing"
        );
        assert!(crate::processing_support::rejects(|| {
            assert_eq!(
                &values[cut..],
                expected.as_slice(),
                "post-cut content must equal a fresh instance of the \
                     ACCEPTED configuration — old transition audio after \
                     the cut fails here"
            );
        }));
        let _ = runtime.dispose();
    });
}

/// N4 — a ramp advanced by WALL CLOCK (progress no processed PCM
/// justifies) is caught: the paused-then-resumed run stops matching its
/// own deterministic continuation, because the mutant's ramp ran ahead
/// during the pause. (The reference, not a second run: transition start
/// frames are timing-dependent across runs.)
#[test]
fn n4_the_pause_oracle_catches_a_wallclock_ramp() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(120), move || {
        let initial = preset_config(EqPreset::Flat);
        let desired = preset_config(EqPreset::Rock);

        let (w, handle, probe, mut runtime) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            initial,
            TEST_RATE * 2,
            Vec::new(),
        );
        probe.arm_mutations(false, true);
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("mutant never started");
        probe.request_update(desired);
        let (start, _) = wait_transition_started(&probe, Duration::from_secs(5));
        handle.request_pause();
        assert!(wait_until(Duration::from_secs(5), || handle
            .observe()
            .paused()));
        assert!(wait_until(Duration::from_secs(5), || handle
            .completion
            .buffered_frames()
            == Some(EDGE_CAPACITY_FRAMES)));
        std::thread::sleep(Duration::from_millis(400));
        handle.request_resume();
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );

        let values = w.content();
        let expected = expected_content(
            EIGHT_SECONDS,
            TEST_RATE * 2,
            &initial,
            &[(start, initial, desired)],
        );
        // The defect must be visible...
        assert_ne!(
            values, expected,
            "the wall-clock ramp must observably diverge from the \
             sample-driven continuation"
        );
        // ...and the pause oracle must reject the mutant world.
        assert!(crate::processing_support::rejects(|| {
            assert_eq!(
                values, expected,
                "the ramp must advance only on processed samples — a \
                 wall-clock jump during the pause fails here"
            );
        }));
        let _ = runtime.dispose();
    });
}

/// N5 — the deliberately unsafe transition method (an INSTANT switch,
/// no crossfade) violates the bounded-continuity oracle the chosen
/// model passes: a preset jump produces a per-sample click far beyond
/// the crossfade's weight-slope bound.
#[test]
fn n5_the_continuity_oracle_rejects_an_instant_preset_switch() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let old = preset_config(EqPreset::Flat);
        let new = preset_config(EqPreset::Rock);
        let frames = 4096;

        // Both reference streams over the same input.
        let mut input = source_input(frames, 0);
        let mut old_side = EpisodeProcessing::new(&old, &format()).expect("compiles");
        old_side.stage(&mut input).expect("stages");
        let old_out: Vec<f32> = input.chunks(2).map(|f| f[0]).collect();
        let mut input = source_input(frames, 0);
        let mut new_side = EpisodeProcessing::new(&new, &format()).expect("compiles");
        new_side.stage(&mut input).expect("stages");
        let new_out: Vec<f32> = input.chunks(2).map(|f| f[0]).collect();

        // The max old/new divergence — the click magnitude an
        // instant switch produces at the switch sample.
        let max_divergence = old_out
            .iter()
            .zip(new_out.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(
            max_divergence > 0.1,
            "Flat-vs-Rock on this input must present a visible \
                 discontinuity risk (max divergence {max_divergence})"
        );

        // The chosen model's bound: with a linear crossfade over T
        // frames, the per-sample click beyond the references' own
        // continuity is bounded by M/T (the weight slope).
        let total = TEST_RATE / 10;
        let bound = max_divergence / total as f32;

        // The INSTANT switch (T = 1): the click equals the full
        // divergence — orders of magnitude beyond the bound.
        let instant_click = max_divergence;
        assert!(
            instant_click > bound * 1000.0,
            "the instant switch must violate the crossfade bound by \
                 orders of magnitude (click {instant_click} vs bound {bound})"
        );
        assert!(crate::processing_support::rejects(|| {
            let observed_click = instant_click;
            assert!(
                observed_click <= bound + 1e-6,
                "transition continuity must stay within the chosen \
                     model's weight-slope bound"
            );
        }));
    });
}

// --- D4: the product-control boundary (ProcessingControl) -----------------

mod control_boundary {
    use super::*;

    /// Establishment binds the desired configuration and leaves the
    /// pending slot EMPTY: the slot exists for live updates only, and a
    /// leftover pending update from an earlier episode on this handle
    /// must never survive into a new episode as a phantom first
    /// transition (Open/replacement, §7.3).
    #[test]
    fn establishment_binds_desired_and_leaves_no_pending_update() {
        let control = ProcessingControl::new(bypass_config());
        control.establish(preset_config(EqPreset::Rock));
        assert_eq!(control.desired(), preset_config(EqPreset::Rock));
        assert!(
            control.pending().is_none(),
            "establishment must clear the pending slot"
        );
        // Even a stale pending planted by a defective caller cannot
        // survive a re-establishment.
        control.plant_pending(preset_config(EqPreset::Bass));
        control.establish(preset_config(EqPreset::Jazz));
        assert!(
            control.pending().is_none(),
            "re-establishment must clear a stale pending update"
        );
        assert_eq!(control.desired(), preset_config(EqPreset::Jazz));
    }

    /// A valid update moves desired AND pending as ONE whole
    /// configuration; a refused update moves NOTHING (desired, pending)
    /// and records its honest diagnostic.
    #[test]
    fn a_valid_update_moves_desired_and_pending_a_refusal_moves_nothing() {
        let control = ProcessingControl::new(preset_config(EqPreset::Flat));

        control
            .route_whole(preset_config(EqPreset::Rock))
            .expect("a preset is valid");
        assert_eq!(control.pending(), Some(preset_config(EqPreset::Rock)));
        assert_eq!(control.last_refusal(), None);

        // A refusal leaves desired and pending exactly where they were.
        let mut invalid = preset_config(EqPreset::Jazz);
        invalid.gain = -1.0;
        let diagnostic = control
            .route_whole(invalid)
            .expect_err("a negative gain must be refused");
        assert!(
            diagnostic.contains("non-negative"),
            "the refusal names the invalidity: {diagnostic}"
        );
        assert_eq!(control.desired(), preset_config(EqPreset::Rock));
        assert_eq!(control.pending(), Some(preset_config(EqPreset::Rock)));
        assert!(
            control.last_refusal().is_some(),
            "the refusal diagnostic is recorded as mechanism evidence"
        );

        // The next accepted update clears it.
        control
            .route_whole(preset_config(EqPreset::Jazz))
            .expect("valid");
        assert_eq!(control.last_refusal(), None);
    }

    /// The typed setters compose the WHOLE desired configuration
    /// field-wise: a field operation never silently changes an unrelated
    /// field (bypass keeps gain/EQ; an EQ edit keeps the preamp), while
    /// a preset switch installs the preset's whole recorded
    /// configuration.
    #[test]
    fn the_typed_setters_compose_the_whole_desired_configuration() {
        let control = ProcessingControl::new(preset_config(EqPreset::Rock));

        control.set_preamp(0.5).expect("finite factor");
        let desired = control.desired();
        assert_eq!(desired.gain, 0.5, "the preamp moved");
        assert_eq!(
            desired.eq,
            preset_config(EqPreset::Rock).eq,
            "the EQ field did not move"
        );
        assert!(desired.enabled, "enabled did not move");

        control.set_enabled(false).expect("toggle");
        let desired = control.desired();
        assert!(!desired.enabled, "bypass is a configuration flag");
        assert_eq!(desired.gain, 0.5, "bypass keeps the gain field");
        assert_eq!(
            desired.eq,
            preset_config(EqPreset::Rock).eq,
            "bypass keeps the EQ field"
        );

        let mut custom = crate::processing::EqConfig::FLAT.band_gain_db;
        custom[0] = 6.0;
        control
            .set_eq_config(crate::processing::EqConfig::new(custom, 1.5))
            .expect("valid trim");
        let desired = control.desired();
        assert_eq!(desired.eq.unwrap().band_gain_db[0], 6.0);
        assert_eq!(desired.eq.unwrap().q, 1.5);
        assert_eq!(desired.gain, 0.5, "an EQ edit keeps the preamp");

        control.set_eq_preset(EqPreset::Bass).expect("valid");
        assert_eq!(
            control.desired(),
            preset_config(EqPreset::Bass),
            "a preset installs its WHOLE recorded configuration (unity \
             preamp included)"
        );
    }

    /// Two typed field commands on DIFFERENT fields, issued
    /// concurrently, must compose: whatever the interleaving, the final
    /// desired configuration carries the LAST command's gain AND the
    /// LAST command's EQ. A read-modify-write split across two lock
    /// acquisitions cannot guarantee this (MUTANT N9: the second command
    /// composes from a stale snapshot and silently rolls the first
    /// command's field back); the production path composes and commits
    /// under ONE lock hold, so the invariant holds for every schedule.
    #[test]
    fn concurrent_field_commands_on_different_fields_compose_without_losing_updates() {
        let custom_bands = |front: f32| {
            let mut bands = crate::processing::EqConfig::FLAT.band_gain_db;
            bands[0] = front;
            bands
        };
        let control = Arc::new(ProcessingControl::new(preset_config(EqPreset::Flat)));
        for i in 0..64 {
            let gain = if i % 2 == 0 { 0.5 } else { 0.25 };
            let front = if i % 2 == 0 { 1.0 } else { 2.0 };

            let gain_control = Arc::clone(&control);
            let eq_control = Arc::clone(&control);
            let gain_thread =
                std::thread::spawn(move || gain_control.set_preamp(gain).expect("valid"));
            let eq_thread = std::thread::spawn(move || {
                eq_control
                    .set_eq_config(crate::processing::EqConfig::new(custom_bands(front), 1.0))
                    .expect("valid")
            });
            gain_thread.join().expect("gain thread");
            eq_thread.join().expect("eq thread");

            let desired = control.desired();
            assert_eq!(
                desired.gain, gain,
                "round {i}: the last gain command survives"
            );
            assert_eq!(
                desired.eq.map(|eq| eq.band_gain_db[0]),
                Some(front),
                "round {i}: the last EQ command survives"
            );
            assert_eq!(
                control.pending().as_ref(),
                Some(&desired),
                "round {i}: the committed whole update occupies the slot"
            );
        }
    }

    /// N9 — the split-lock read-modify-write the typed commands must
    /// never express: composing from a STALE desired snapshot and
    /// committing the whole after an unrelated field command loses that
    /// command's field change. The atomic world (the production
    /// `update_desired` path) composes from the CURRENT desired state in
    /// the same lock hold it commits in, so both fields survive there;
    /// the scripted mutant world demonstrably rolls the gain back, and
    /// the oracle catches it.
    #[test]
    fn n9_the_lost_update_oracle_catches_the_split_lock_rmw() {
        let custom = || {
            let mut bands = crate::processing::EqConfig::FLAT.band_gain_db;
            bands[0] = 6.0;
            crate::processing::EqConfig::new(bands, 1.0)
        };

        // The atomic world: an EQ command composed AFTER a committed
        // gain command keeps the gain.
        let atomic = ProcessingControl::new(preset_config(EqPreset::Rock));
        atomic.set_preamp(0.5).expect("valid");
        atomic
            .update_desired(|candidate| candidate.eq = Some(custom()))
            .expect("valid");
        let desired = atomic.desired();
        assert_eq!(desired.gain, 0.5, "the earlier field command survives");
        assert_eq!(
            desired.eq,
            Some(custom()),
            "the later field command applied"
        );

        // The mutant world: the stale-snapshot interleaving, scripted
        // deterministically (the "concurrent" command runs between the
        // read lock and the commit lock).
        let split = Arc::new(ProcessingControl::new(preset_config(EqPreset::Rock)));
        let in_world = Arc::clone(&split);
        let between = Arc::clone(&split);
        let caught = crate::processing_support::rejects(move || {
            crate::live::mutant_stale_snapshot_rmw(
                &in_world,
                || {
                    between.set_preamp(0.5).expect("valid");
                },
                |stale| stale.eq = Some(custom()),
            )
            .expect("the mutant's commit is intrinsically valid");
            let desired = in_world.desired();
            assert_eq!(
                desired.gain, 0.5,
                "a field command must survive a concurrent unrelated command"
            );
            assert_eq!(desired.eq, Some(custom()));
        });
        assert!(
            caught,
            "the split-lock world must lose the update — the oracle must catch N9"
        );
        // And the loss is visible in the mutant world's final state.
        let rolled_back = split.desired();
        assert_eq!(
            rolled_back.gain, 1.0,
            "the mutant world demonstrably rolled the gain back to the stale snapshot"
        );
        assert_eq!(rolled_back.eq, Some(custom()));
    }
}

// --- D4: the product path (typed commands through the REAL seam) ----------

/// The production transition length at the test rate — the geometry the
/// product-path oracles build their references from.
fn production_transition_frames() -> usize {
    crate::live::live_transition_frames(TEST_RATE as u32)
}

/// The first index where `values` leaves `reference` (bit-exact), i.e.
/// the OBSERVED first divergence from the old continuation.
fn first_divergence(values: &[f32], reference: &[f32]) -> usize {
    values
        .iter()
        .zip(reference.iter())
        .position(|(a, b)| a != b)
        .unwrap_or(values.len())
}

/// The apply boundary inferred from an observed divergence: the
/// staging-block boundary at or before it. The blend's earliest frames
/// (w → 1) can be numerically indistinguishable from the old
/// continuation, so the OBSERVED divergence may lag the true boundary by
/// a few frames — never past the boundary block (by the block's end the
/// new side contributes ~46%, far above an ulp). Callers assert the lag.
fn boundary_at_or_before(observed: usize) -> usize {
    observed - (observed % STAGING_FRAMES)
}

/// A typed command issued in the establishment→activation window folds
/// into the INITIAL applied configuration: the activation bind consumes
/// the pending slot in the same lock hold it reads the desired state, so
/// the episode starts directly under the commanded configuration —
/// bit-exactly a fresh instance of it from frame 0 — and NO phantom
/// initial→same transition ever starts. (Clearing the slot only at
/// establishment left exactly that phantom: a pre-activation command
/// planted a pending update equal to the configuration activation was
/// about to compile.)
#[test]
fn a_command_before_activation_folds_into_the_initial_configuration() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let initial = preset_config(EqPreset::Flat);
        let (witnesses, handle, probe, mut runtime) = live_episode_parts(
            SourceBehavior::EofAfter(EIGHT_SECONDS),
            OutputBehavior::Consume,
            initial,
            production_transition_frames(),
            Vec::new(),
        );
        // The window: establishment bound (the constructor ran),
        // activation has not.
        handle.set_eq_preset(EqPreset::Rock).expect("valid command");
        assert_eq!(
            probe.pending().as_ref(),
            Some(&preset_config(EqPreset::Rock)),
            "the command is recorded as a pending desired update"
        );
        revise_composition(&mut runtime);
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );

        assert!(
            !probe
                .events()
                .iter()
                .any(|e| matches!(e, LiveEvent::TransitionStarted { .. })),
            "a command that folded into the initial configuration must not \
             start a phantom initial→same live transition"
        );
        // The whole stream is a fresh instance of the COMMANDED
        // configuration from frame 0, bit-exactly (a phantom blend is
        // not bit-exact).
        let mut reference =
            EpisodeProcessing::new(&preset_config(EqPreset::Rock), &format()).expect("compiles");
        let mut input = source_input(EIGHT_SECONDS, 0);
        reference.stage(&mut input).expect("stages");
        let expected: Vec<f32> = input.chunks(2).map(|f| f[0]).collect();
        assert_eq!(
            witnesses.content(),
            expected,
            "the episode runs the commanded configuration from frame 0"
        );
        let _ = runtime.dispose();
    });
}

/// The pair: a typed command issued AFTER the activation bind takes the
/// ordinary §7.3 live path — exactly ONE live transition to the
/// commanded configuration, and the settled stretch bit-exactly a fresh
/// instance of it.
#[test]
fn a_command_after_activation_takes_the_ordinary_live_transition() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let initial = preset_config(EqPreset::Flat);
        let total = production_transition_frames();
        let (witnesses, handle, probe, mut runtime) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            initial,
            total,
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        handle.set_eq_preset(EqPreset::Rock).expect("valid command");
        let (start, _) = wait_transition_started(&probe, Duration::from_secs(5));
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );

        let started: Vec<usize> = probe
            .events()
            .into_iter()
            .filter_map(|e| match e {
                LiveEvent::TransitionStarted { at_frame, .. } => Some(at_frame),
                _ => None,
            })
            .collect();
        assert_eq!(
            started,
            vec![start],
            "a post-activation command must start exactly ONE live transition"
        );
        let expected = expected_content(
            EIGHT_SECONDS,
            total,
            &initial,
            &[(start, initial, preset_config(EqPreset::Rock))],
        );
        let values = witnesses.content();
        for (i, (got, want)) in values.iter().zip(expected.iter()).enumerate() {
            let tolerance = 1e-4 * want.abs().max(1.0);
            assert!(
                (got - want).abs() <= tolerance,
                "frame {i}: {got} vs expected blend {want}"
            );
        }
        let _ = runtime.dispose();
    });
}

/// A typed preset switch on the PRODUCT path (no tap, production
/// transition geometry): the apply boundary is a whole staging block
/// after the command, the pre-boundary stretch is bit-exactly the old
/// continuation, the transition stretch is the exact blend, the settled
/// stretch is bit-exactly a fresh instance of the accepted preset, no
/// refusal was recorded, and the frame count is conserved.
#[test]
fn a_typed_preset_switch_on_the_product_path_blends_exactly_and_settles_fresh() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let initial = preset_config(EqPreset::Flat);
        let total = production_transition_frames();
        let (witnesses, handle, mut runtime) = crate::processing_support::episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            initial,
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        let requested_at = handle.observe().position.expect("live");
        handle
            .set_eq_preset(EqPreset::Rock)
            .expect("a preset update is valid");

        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );

        let values = witnesses.content();
        // Flat is the bit-exact identity, so the old continuation is the
        // raw source; the first divergence locates the apply boundary.
        let raw: Vec<f32> = (0..EIGHT_SECONDS).map(|i| i as f32).collect();
        let observed = first_divergence(&values, &raw);
        assert!(
            observed < values.len(),
            "an accepted update must become audible"
        );
        assert!(
            observed > requested_at as usize,
            "the update applies after the command"
        );
        let start = boundary_at_or_before(observed);
        assert!(
            observed - start < STAGING_FRAMES,
            "the observed divergence must lie inside the boundary block"
        );
        assert!(
            handle.observe().last_processing_refusal.is_none(),
            "an accepted update records no refusal"
        );

        let desired = preset_config(EqPreset::Rock);
        let expected =
            expected_content(EIGHT_SECONDS, total, &initial, &[(start, initial, desired)]);
        assert_eq!(
            values.len(),
            EIGHT_SECONDS,
            "frame conservation across the live transition"
        );
        assert_eq!(
            &values[..start],
            &raw[..start],
            "pre-boundary: old continuation"
        );
        for (i, (got, want)) in values[start..]
            .iter()
            .zip(expected[start..].iter())
            .enumerate()
        {
            let tolerance = 1e-4 * want.abs().max(1.0);
            assert!(
                (got - want).abs() <= tolerance,
                "frame {} (transition+{}): {got} vs expected blend {want}",
                i + start,
                i
            );
        }
        let _ = runtime.dispose();
    });
}

/// The typed gain endpoints ride the same blend law, bit-exactly where
/// the factors are exact in f32: unity → quarter, quarter → silence,
/// silence → unity, both channels, with the settled stretches exactly
/// the scaled continuations and the silence stretch exactly +0.0.
#[test]
fn typed_gain_endpoints_ride_the_same_blend_law() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(120), move || {
        let total = production_transition_frames();
        let (witnesses, handle, mut runtime) = crate::processing_support::episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            AudioProcessingConfig::gain(1.0),
            Vec::new(),
        );
        // One command per second of source time; each settled stretch is
        // longer than the transition, so the boundaries are separable.
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= TEST_RATE as u64)
        })
        .then_some(())
        .expect("never reached 1s");
        handle.set_preamp(0.25).expect("valid");
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= 2 * TEST_RATE as u64)
        })
        .then_some(())
        .expect("never reached 2s");
        handle.set_preamp(0.0).expect("valid");
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= 4 * TEST_RATE as u64)
        })
        .then_some(())
        .expect("never reached 4s");
        handle.set_preamp(1.0).expect("valid");
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );

        let values = witnesses.content();
        assert_eq!(values.len(), EIGHT_SECONDS, "frame conservation");
        let raw: Vec<f32> = (0..EIGHT_SECONDS).map(|i| i as f32).collect();

        // Locate each apply boundary by leaving the previous settled
        // continuation (all references here are exact in f32); the
        // boundary is the block boundary at or before the observed
        // divergence (the earliest blend frames can round back onto the
        // continuation).
        let quarter = |i: usize| 0.25 * raw[i];
        let observed1 = (0..2 * TEST_RATE)
            .find(|&i| values[i] != raw[i])
            .expect("the unity->quarter transition must appear");
        let start1 = boundary_at_or_before(observed1);
        let observed2 = (start1 + total..4 * TEST_RATE)
            .find(|&i| values[i] != quarter(i))
            .expect("the quarter->silence transition must appear");
        let start2 = boundary_at_or_before(observed2);
        let observed3 = (start2 + total..6 * TEST_RATE)
            .find(|&i| values[i] != 0.0)
            .expect("the silence->unity transition must appear");
        let start3 = boundary_at_or_before(observed3);
        assert!(observed1 - start1 < STAGING_FRAMES);
        assert!(observed2 - start2 < STAGING_FRAMES);
        assert!(observed3 - start3 < STAGING_FRAMES);

        // Settled stretches, exactly.
        for (i, &got) in values[start1 + total..start2].iter().enumerate() {
            assert_eq!(
                got,
                quarter(start1 + total + i),
                "the settled quarter stretch is exactly 0.25x at {}",
                start1 + total + i
            );
        }
        for (i, &got) in values[start2 + total..start3].iter().enumerate() {
            assert_eq!(
                got,
                0.0,
                "the settled silence stretch is +0.0 at {}",
                start2 + total + i
            );
        }
        for (i, &got) in values[start3 + total..].iter().enumerate() {
            assert_eq!(
                got,
                raw[start3 + total + i],
                "the settled unity stretch is bit-exactly the source at {}",
                start3 + total + i
            );
        }

        // Each transition stretch is the interpolated gain on the same
        // input (the U2 degenerate form of the same crossfade).
        let transitions = [
            (start1, 1.0f32, 0.25f32),
            (start2, 0.25, 0.0),
            (start3, 0.0, 1.0),
        ];
        for (start, from, to) in transitions {
            for (k, i) in (start..start + total).enumerate() {
                let w = (1.0 - (k as f64 / total as f64)) as f32 + 0.0;
                let x = raw[i];
                let want = w.mul_add(from * x, (1.0 - w).mul_add(to * x, 0.0));
                assert!(
                    (values[i] - want).abs() <= 1e-4 * want.abs().max(1.0),
                    "frame {i} of the {from}->{to} transition: {} vs {want}",
                    values[i]
                );
            }
        }
        let _ = runtime.dispose();
    });
}

/// Custom EQ and the Flat preset on the product path: a custom
/// configuration's settled stretch is bit-exactly a fresh instance of
/// itself from its transition start, and the Flat preset settles
/// bit-exactly back to the untouched source.
#[test]
fn custom_eq_and_flat_settle_bit_exact() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(90), move || {
        let total = production_transition_frames();
        let mut custom_bands = crate::processing::EqConfig::FLAT.band_gain_db;
        custom_bands[0] = 7.0;
        custom_bands[1] = -3.0;
        let custom = crate::processing::EqConfig::new(custom_bands, 1.2);

        let (witnesses, handle, mut runtime) = crate::processing_support::episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            preset_config(EqPreset::Flat),
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= TEST_RATE as u64)
        })
        .then_some(())
        .expect("never reached 1s");
        handle.set_eq_config(custom).expect("valid");
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= 3 * TEST_RATE as u64)
        })
        .then_some(())
        .expect("never reached 3s");
        handle.set_eq_preset(EqPreset::Flat).expect("valid");
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );

        let values = witnesses.content();
        assert_eq!(values.len(), EIGHT_SECONDS, "frame conservation");
        let raw: Vec<f32> = (0..EIGHT_SECONDS).map(|i| i as f32).collect();

        // Transition 1: Flat → custom. Boundary = the block boundary at
        // or before the first divergence from the raw source.
        let observed1 = (TEST_RATE..3 * TEST_RATE)
            .find(|&i| values[i] != raw[i])
            .expect("the custom EQ must diverge from Flat");
        let start1 = boundary_at_or_before(observed1);
        assert!(observed1 - start1 < STAGING_FRAMES);
        // Settled custom stretch: bit-exactly a fresh custom instance
        // started at the transition start.
        let settled1_end = 3 * TEST_RATE;
        let settled1_start = start1 + total;
        assert!(settled1_start <= settled1_end);
        // The engine's new side is fed frames from the TRANSITION START,
        // so the fresh reference instance starts there too (a stateful
        // EQ's output depends on its whole fed history).
        let mut fresh_custom =
            EpisodeProcessing::new(&AudioProcessingConfig::eq(custom), &format())
                .expect("compiles");
        let mut input = source_input(EIGHT_SECONDS - start1, start1);
        fresh_custom.stage(&mut input).expect("stages");
        let expected_custom: Vec<f32> = input.chunks(2).map(|f| f[0]).collect();
        assert_eq!(
            &values[settled1_start..settled1_end],
            &expected_custom[settled1_start - start1..settled1_end - start1],
            "the settled custom stretch is a fresh custom continuation"
        );

        // Transition 2: custom → Flat. Boundary = the block boundary at
        // or before the first divergence from the custom continuation.
        let observed2 = (settled1_end..EIGHT_SECONDS)
            .find(|&i| values[i] != expected_custom[i - start1])
            .expect("the Flat switch must diverge from the custom continuation");
        let start2 = boundary_at_or_before(observed2);
        assert!(observed2 - start2 < STAGING_FRAMES);
        for (i, &got) in values[start2 + total..].iter().enumerate() {
            let at = start2 + total + i;
            assert_eq!(
                got, raw[at],
                "the settled Flat stretch is bit-exactly the source at {at}"
            );
        }
        let _ = runtime.dispose();
    });
}

/// An update requested near completion is either applied (its truncated
/// transition follows the same blend law) or inert history — in BOTH
/// worlds the frame count is conserved, the episode completes, and the
/// content is exactly one of the two legal references. Nothing may
/// half-apply.
#[test]
fn an_update_near_completion_is_never_half_applied() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(90), move || {
        let initial = preset_config(EqPreset::Flat);
        let total = production_transition_frames();
        let (witnesses, handle, mut runtime) = crate::processing_support::episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            initial,
            Vec::new(),
        );
        // Request while the transition can still be truncated by EOF.
        let late = (EIGHT_SECONDS - total - 4 * STAGING_FRAMES) as u64;
        wait_until(Duration::from_secs(5), || {
            handle.observe().position.is_some_and(|p| p >= late)
        })
        .then_some(())
        .expect("never reached the late window");
        handle
            .set_eq_preset(EqPreset::Rock)
            .expect("a preset update is valid");
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed,
            "an update near completion is not a failure"
        );
        assert!(
            handle.observe().last_processing_refusal.is_none(),
            "a valid update records no refusal even near EOF"
        );

        let values = witnesses.content();
        assert_eq!(values.len(), EIGHT_SECONDS, "frame conservation");
        let raw: Vec<f32> = (0..EIGHT_SECONDS).map(|i| i as f32).collect();
        let desired = preset_config(EqPreset::Rock);
        let observed = first_divergence(&values, &raw);
        if observed == values.len() {
            // Legal world: the update never reached an apply boundary
            // before EOF — inert command history.
            assert_eq!(values, raw, "the inert world is the pure continuation");
        } else {
            // Legal world: the truncated transition follows the law.
            let start = boundary_at_or_before(observed);
            assert!(observed - start < STAGING_FRAMES);
            let expected =
                expected_content(EIGHT_SECONDS, total, &initial, &[(start, initial, desired)]);
            assert_eq!(&values[..start], &raw[..start]);
            for (i, (got, want)) in values[start..]
                .iter()
                .zip(expected[start..].iter())
                .enumerate()
            {
                let tolerance = 1e-4 * want.abs().max(1.0);
                assert!(
                    (got - want).abs() <= tolerance,
                    "frame {}: {got} vs {want}",
                    i + start
                );
            }
        }
        let _ = runtime.dispose();
    });
}

/// The observed-seek × accepted-update interleaving (carried D3 debt):
/// a seek command that is observed but NOT yet actionable does not block
/// the fresh-block pickup — the accepted update starts its transition
/// while the seek pends.
///
/// The seek's later career is deliberately NOT pinned here. Whether it
/// stays unactionable to the end, or a starved render leg's parked
/// evidence makes it actionable (the frozen D14.5 cut), is
/// schedule-dependent under load — and every legal ending is already
/// pinned by its own deterministic oracle (the applied cut lands fresh
/// under the accepted configuration; a refusal preserves the
/// continuation bit-exactly). What THIS oracle owns is the
/// pickup-availability contract: the transition starts, after the
/// commands, while the seek is still pending; the consumed prefix up to
/// the transition is the pure old configuration in EVERY legal ending
/// (in the cut world the pickup precedes the first post-cut staging, so
/// the transition start coincides with the landing); the update was
/// accepted, never refused; the episode still ends by stop.
#[test]
fn a_merely_observed_seek_does_not_block_the_update_pickup() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(90), move || {
        let initial = preset_config(EqPreset::Flat);
        let desired = preset_config(EqPreset::Rock);

        // A slow paced source: after the fast prefix the decode produces
        // one frame per 5 ms (200 frames/s — far below any consumption
        // rate), so the edge stays empty and the seek pends for the
        // whole observation window under normal scheduling. The
        // transition is short (64 frames) so it completes quickly at the
        // producer's pace.
        let (w, handle, probe, mut runtime) = live_episode_source(
            SourceBehavior::Paced {
                after: 4200,
                delay: Duration::from_millis(5),
            },
            OutputBehavior::Consume,
            initial,
            64,
            vec![ProviderSeekOutcome::Applied {
                landing: Some(TEST_RATE as u64),
            }],
        );
        wait_until(Duration::from_secs(30), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        // Observed seek FIRST (pends), accepted update SECOND: the
        // pickup must proceed — THE contract under test.
        handle.request_seek(Duration::from_secs(1));
        probe.request_update(desired);
        let (start, _) = wait_transition_started(&probe, Duration::from_secs(30));
        assert!(
            start > HALF_A_SECOND as usize / 4,
            "the transition started after the commands, not before them"
        );
        handle.request_stop();
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Stopped,
            "the episode ends by stop in every legal ending of the pended \
                 seek"
        );
        assert!(
            handle.observe().last_processing_refusal.is_none(),
            "the accepted update is not a refusal"
        );
        // The consumed stream decides which legal ending occurred, and
        // each is verified against its own exact reference.
        let values = w.content();
        let raw: Vec<f32> = (0..values.len()).map(|i| i as f32).collect();
        // A post-landing frame carries the landing tag (1 s); every
        // pre-cut frame carries a tag below the consumed length.
        match values.iter().position(|&v| v >= TEST_RATE as f32) {
            None => {
                // No cut became audible. The consumed prefix up to the
                // transition start is the pure old continuation in every
                // no-cut ending; what lies beyond it is, at most, the
                // w→1 head of the blend (bit-equal to the continuation —
                // the blend law itself is pinned by the slow-consume
                // oracles).
                let prefix = start.min(values.len());
                assert_eq!(
                    &values[..prefix],
                    &raw[..prefix],
                    "the consumed prefix before the transition is the pure \
                         old continuation"
                );
            }
            Some(j) => {
                let reference =
                    expected_content(values.len(), 64, &initial, &[(start, initial, desired)]);
                // World B first: the transition rode pre-cut and the
                // actionable cut dropped it (the frozen D14.5 order), so
                // the landing must be a FRESH instance of the ACCEPTED
                // configuration fed the exact post-landing tags,
                // bit-exactly.
                let mut fresh = EpisodeProcessing::new(&desired, &format()).expect("compiles");
                let mut post = source_input(values.len() - j, TEST_RATE);
                fresh.stage(&mut post).expect("stages");
                let fresh_landing: Vec<f32> = post.chunks(2).map(|f| f[0]).collect();
                if values[j..] == fresh_landing.as_slice()[..] {
                    assert_eq!(
                        &values[..j],
                        &reference[..j],
                        "pre-cut: the run's own no-cut continuation, \
                             transition included"
                    );
                } else {
                    // World C: the cut preceded the pickup, so the update
                    // applied AT the landing — the post-landing stretch
                    // is the Model C blend (the fresh-applied old side —
                    // the cut invalidated pre-cut history — against the
                    // new configuration from rest) settling into the
                    // accepted configuration.
                    assert_eq!(&values[..j], &raw[..j], "pre-cut: pure old continuation");
                    let tail = values.len() - j;
                    let mut old_side =
                        EpisodeProcessing::new(&initial, &format()).expect("compiles");
                    let mut old_tail = source_input(tail, TEST_RATE);
                    old_side.stage(&mut old_tail).expect("stages");
                    let mut new_side =
                        EpisodeProcessing::new(&desired, &format()).expect("compiles");
                    let mut new_tail = source_input(tail, TEST_RATE);
                    new_side.stage(&mut new_tail).expect("stages");
                    for (k, got) in values[j..].iter().enumerate() {
                        let want = if k < 64 {
                            let wgt = blend_weight(k + j, j, 64);
                            wgt.mul_add(old_tail[k * 2], (1.0 - wgt).mul_add(new_tail[k * 2], 0.0))
                        } else {
                            new_tail[k * 2]
                        };
                        let tolerance = 1e-4 * want.abs().max(1.0);
                        assert!(
                            (got - want).abs() <= tolerance,
                            "frame {}: {got} vs {want}",
                            j + k
                        );
                    }
                }
            }
        }
        let _ = runtime.dispose();
    });
}

// --- D4 negative controls (the new mutants) -------------------------------

/// N6 — a refused seek that RESETS the processing history (instead of
/// preserving it) is caught by the refusal oracle: with a STATEFUL old
/// configuration, the run's own warmed continuation differs from any
/// reset world, bit-exactly.
#[test]
fn n6_the_refusal_oracle_catches_a_history_reset_on_refused_seek() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(90), move || {
        use crate::live::mutant_reset_history;
        let initial = preset_config(EqPreset::Bass);
        let desired = preset_config(EqPreset::Treble);

        let (w, handle, probe, mut runtime) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            initial,
            TEST_RATE / 5,
            vec![ProviderSeekOutcome::RefusedUnchanged],
        );
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        probe.request_update(desired);
        let (start, _) = wait_transition_started(&probe, Duration::from_secs(5));
        handle.request_seek(Duration::from_secs(5));
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );

        let values = w.content();
        let reference = expected_content(
            EIGHT_SECONDS,
            TEST_RATE / 5,
            &initial,
            &[(start, initial, desired)],
        );
        // Positive: the honest run equals its warmed continuation.
        assert_eq!(
            values, reference,
            "history is preserved through the refusal"
        );

        // Negative: a world where the history was reset at ANY point r
        // after the transition start must be rejected by the SAME
        // predicate. r is chosen mid-stretch; the predicate's bit-exact
        // equality is insensitive to which r.
        let r = start + TEST_RATE / 10;
        let mut reset_world = reference[..r].to_vec();
        reset_world.extend(mutant_reset_history(
            &source_input(EIGHT_SECONDS - r, r),
            &initial,
            &format(),
        ));
        assert_ne!(values, reset_world, "a reset is observably different");
        assert!(crate::processing_support::rejects(|| {
            assert_eq!(values, reset_world, "the shipped refusal predicate");
        }));
        let _ = runtime.dispose();
    });
}

/// N7 — a bypass transition realized as an INSTANT dry switch (the
/// authorized crossfade's frames dropped) is caught by the bypass
/// oracle's blend law.
#[test]
fn n7_the_bypass_oracle_catches_an_instant_dry_switch() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(90), move || {
        let initial = preset_config(EqPreset::Rock);

        let (w, handle, probe, mut runtime) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            initial,
            TEST_RATE / 5,
            Vec::new(),
        );
        probe.tap.arm(false, false, true);
        wait_until(Duration::from_secs(5), || {
            handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        handle.set_processing_enabled(false).expect("valid");
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );

        let values = w.content();
        let raw: Vec<f32> = (0..EIGHT_SECONDS).map(|i| i as f32).collect();
        let start = first_divergence(&values, &raw);
        assert!(
            start < EIGHT_SECONDS,
            "the processed→dry switch must be observable"
        );
        // The shipped bypass oracle — the transition stretch must be the
        // bounded blend of processed and dry — must REJECT the mutant's
        // instant-dry world (its blend frames were dropped).
        let blend_reference = expected_content(
            EIGHT_SECONDS,
            TEST_RATE / 5,
            &initial,
            &[(
                start,
                initial,
                crate::processing::AudioProcessingConfig::BYPASS,
            )],
        );
        assert_ne!(
            values, blend_reference,
            "the instant-dry world must be observably different from the \
                 honest blend"
        );
        assert!(crate::processing_support::rejects(|| {
            assert_eq!(values, blend_reference, "the bypass blend law");
        }));
        let _ = runtime.dispose();
    });
}

/// N8 — a stale pending update from a previous episode leaking into a
/// replacement episode is caught: establishment clears the slot (pinned
/// directly), and the replacement freshness predicate distinguishes the
/// fresh world from the stale-config world.
#[test]
fn n8_the_replacement_oracle_catches_a_stale_pending_leak() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(120), move || {
        let stale = preset_config(EqPreset::Bass);
        let own = preset_config(EqPreset::Rock);

        // Episode one: live-update, complete.
        let (_, handle_one, probe_one, mut runtime_one) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            preset_config(EqPreset::Flat),
            TEST_RATE / 10,
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            handle_one
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("never started");
        probe_one.request_update(stale);
        assert_eq!(
            handle_one.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );
        let _ = runtime_one.dispose();

        // The replacement's control: plant a STALE pending update after
        // establishment — exactly what a leaky establishment would leave
        // behind. The honest mechanism clears it at establishment, and
        // re-establishment clears anything planted before it.
        let handle_two = crate::handle::PlaybackSessionHandle::new();
        handle_two.completion.processing().establish(own);
        handle_two.completion.processing().plant_pending(stale);
        // The honest re-establishment (the spec constructor's act):
        handle_two.completion.processing().establish(own);
        assert!(
            handle_two.completion.processing().pending().is_none(),
            "establishment must clear a stale pending update"
        );

        // Sensitivity: the freshness predicate distinguishes the
        // episode's OWN world from the stale world.
        let mut fresh_own = EpisodeProcessing::new(&own, &format()).expect("compiles");
        let mut own_input = source_input(1024, 0);
        fresh_own.stage(&mut own_input).expect("stages");
        let mut fresh_stale = EpisodeProcessing::new(&stale, &format()).expect("compiles");
        let mut stale_input = source_input(1024, 0);
        fresh_stale.stage(&mut stale_input).expect("stages");
        assert_ne!(
            own_input, stale_input,
            "the worlds are observably different"
        );
        assert!(crate::processing_support::rejects(|| {
            assert_eq!(own_input, stale_input, "the replacement freshness oracle");
        }));
    });
}

// --- D4 performance and allocation evidence -------------------------------

/// The live mechanism's costs, measured at the engine level at both
/// release rates (44.1 kHz / 48 kHz stereo, 1024-frame blocks):
///
/// - steady state allocates NOTHING per block (the counting allocator
///   witnesses the honest engine, not a self-report);
/// - a transition's allocation is bounded (one scratch growth, once);
/// - the transition multiplier over the steady path is recorded and
///   bounded far below the decode-worker block budget (a 1024-frame
///   block is ~23 ms of source time; the whole measurement runs
///   thousands of blocks in well under a second).
#[test]
fn the_live_mechanism_costs_are_bounded_and_recorded() {
    use crate::edge_lifecycle_tests::counting_allocator::run_counting_allocations;
    use crate::session::ProcessingRuntime;

    let _lifecycle = test_common::lifecycle_lock();
    for rate in [44_100u32, 48_000] {
        let mut format = format();
        format.sample_rate = rate;
        let control = Arc::new(ProcessingControl::new(preset_config(EqPreset::Bass)));
        let mut engine = crate::live::LiveProcessing::new(
            preset_config(EqPreset::Bass),
            &format,
            control.clone(),
        )
        .expect("compiles");
        let mut block = vec![0.25f32; STAGING_FRAMES * usize::from(format.channels)];

        // Scratch allocated ONCE outside every counted window, so the
        // allocation counts measure the engine only.
        let mut samples: Vec<f64> = Vec::with_capacity(1024);
        let time_blocks = |engine: &mut crate::live::LiveProcessing,
                           block: &mut [f32],
                           samples: &mut Vec<f64>,
                           n: usize| {
            samples.clear();
            for _ in 0..n {
                let t0 = std::time::Instant::now();
                engine.stage(block).expect("stages");
                samples.push(t0.elapsed().as_secs_f64());
            }
            samples.sort_by(|a, b| a.total_cmp(b));
            (samples[n / 2], samples[(n * 99) / 100])
        };

        // Warm-up (scratch/allocator steady state), then steady evidence.
        for _ in 0..64 {
            engine.stage(&mut block).expect("stages");
        }
        let n = 400;
        let ((steady_median, steady_p99), steady_allocs) =
            run_counting_allocations(|| time_blocks(&mut engine, &mut block, &mut samples, n));
        assert_eq!(
            steady_allocs, 0,
            "the no-update steady path must allocate nothing per block"
        );

        // Start a transition (the sharpest authorized jump: Bass →
        // Treble) and measure its whole length.
        control.set_eq_preset(EqPreset::Treble).expect("valid");
        assert!(
            engine.poll_update().is_some(),
            "the pickup accepts the pending update"
        );
        let transition_blocks =
            crate::live::live_transition_frames(rate).div_ceil(STAGING_FRAMES) + 2;
        let ((trans_median, trans_p99), trans_allocs) = run_counting_allocations(|| {
            time_blocks(&mut engine, &mut block, &mut samples, transition_blocks)
        });
        assert!(
            trans_allocs <= 1,
            "a transition allocates at most its one scratch growth, got {trans_allocs}"
        );

        // Back to steady state: settled, still allocation-free.
        let ((post_median, _), post_allocs) =
            run_counting_allocations(|| time_blocks(&mut engine, &mut block, &mut samples, n));
        assert_eq!(post_allocs, 0, "settled state allocates nothing");

        let multiplier = trans_median / steady_median;
        assert!(
            multiplier < 8.0,
            "the transition multiplier {multiplier:.2}x must stay far \
                 below the block budget"
        );
        println!(
            "live-mechanism costs @ {rate} Hz stereo, {}-frame blocks: \
             steady median {:.2?} (p99 {:.2?}), transition median {:.2?} \
             (p99 {:.2?}), multiplier {multiplier:.2}x, settled median \
             {:.2?}; allocations: steady {steady_allocs}, transition \
             {trans_allocs}, settled {post_allocs}",
            STAGING_FRAMES,
            Duration::from_secs_f64(steady_median),
            Duration::from_secs_f64(steady_p99),
            Duration::from_secs_f64(trans_median),
            Duration::from_secs_f64(trans_p99),
            Duration::from_secs_f64(post_median),
        );
    }
}
