//! D3 live-transition oracles (campaign #190; ADR-PBK-002 D14.11
//! live-update = OPEN until these probes earn it). Every test here
//! drives the REAL Playback Session composition — the real decode
//! worker loop, staging placement, PcmEdge partial writes, seek/pause
//! protocol and terminal settlement — with the disposable
//! [`crate::live_probe::LiveProbeEngine`] at the processing seam.
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

use crate::live_probe::{LiveProbeControl, ProbeEvent, probe_control};
use crate::presets::EqPreset;
use crate::processing::AudioProcessingConfig;
use crate::processing::EpisodeProcessing;
use crate::processing_support::{DUMMY_PATH, EIGHT_SECONDS, TEST_RATE, Witnesses, wait_until};
use crate::session::{EDGE_CAPACITY_FRAMES, STAGING_FRAMES, playback_session_spec_with_live_probe};
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

/// A live-probe episode: the standard doubles + the real session
/// carrying the [`crate::live_probe::LiveProbeEngine`] at the seam.
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
    LiveProbeControl,
    qianqian_app::QianqianApp,
) {
    let control = probe_control();
    let witnesses = Witnesses::new();
    let handle = crate::handle::PlaybackSessionHandle::new();
    let mut runtime = qianqian_app::QianqianApp::new();

    let decode = TestDecode {
        behavior: SourceBehavior::EofAfter(source_frames),
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
        .register_component(playback_session_spec_with_live_probe(
            std::path::PathBuf::from(DUMMY_PATH),
            handle.clone(),
            initial,
            transition_frames,
            control.clone(),
        ))
        .expect("session registers");

    runtime
        .revise_desired(vec![
            crate::processing_support::desired("decode", "test_decode_plugin"),
            crate::processing_support::desired("output", "test_output_plugin"),
            crate::processing_support::desired("session", "playback_session"),
        ])
        .expect("composition is legal");

    (witnesses, handle, control, runtime)
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
        // The OLD side: warmed from the previous segment boundary by
        // staging the running configuration up to `start`, then
        // continued across the whole tail with the SAME instance. The
        // warm-up writes only the PURE stretch — the previous
        // transition's blend stretch stays what that blend computed.
        let mut old_processor = EpisodeProcessing::new(old, &format()).expect("compiles");
        if start > prev_start {
            let mut warm = source_input(start - prev_start, prev_start);
            old_processor.stage(&mut warm).expect("stages");
            let skip = if prev_start == 0 {
                0
            } else {
                prev_total.min(start - prev_start)
            };
            for (i, frame) in warm.chunks(2).enumerate().skip(skip) {
                expected[(prev_start + i) * 2] = frame[0];
                expected[(prev_start + i) * 2 + 1] = frame[1];
            }
        }
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
fn wait_transition_started(control: &LiveProbeControl, limit: Duration) -> (usize, usize) {
    assert!(
        wait_until(limit, || control
            .events()
            .iter()
            .any(|e| matches!(e, ProbeEvent::TransitionStarted { .. }))),
        "no transition ever started"
    );
    match control
        .events()
        .into_iter()
        .find(|e| matches!(e, ProbeEvent::TransitionStarted { .. }))
        .expect("checked")
    {
        ProbeEvent::TransitionStarted { at_frame, block } => (at_frame, block),
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
                .any(|e| matches!(e, ProbeEvent::TransitionCompleted { .. }))),
            "the first transition never settled"
        );
        control.request_update(initial);
        assert!(
            wait_until(Duration::from_secs(5), || {
                control
                    .events()
                    .iter()
                    .filter(|e| matches!(e, ProbeEvent::TransitionStarted { .. }))
                    .count()
                    >= 2
            }),
            "the second update must start a second transition"
        );
        let start2 = match control
            .events()
            .into_iter()
            .filter(|e| matches!(e, ProbeEvent::TransitionStarted { .. }))
            .nth(1)
            .expect("checked")
        {
            ProbeEvent::TransitionStarted { at_frame, .. } => at_frame,
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
/// the no-seek control, and the post-cut stretch is BIT-EXACTLY a fresh
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

        // The no-seek control: the same live transition.
        let (control_w, control_handle, control_probe, mut control_runtime) = live_episode(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            initial,
            TEST_RATE / 5,
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            control_handle
                .observe()
                .position
                .is_some_and(|p| p >= HALF_A_SECOND / 4)
        })
        .then_some(())
        .expect("control never started");
        control_probe.request_update(desired);
        let _ = wait_transition_started(&control_probe, Duration::from_secs(5));
        assert_eq!(
            control_handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );
        let control_values = control_w.content();

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
        let _ = wait_transition_started(&probe, Duration::from_secs(5));
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
                .any(|e| matches!(e, ProbeEvent::InvalidationDroppedTransition { .. })),
            "the Applied cut must drop the in-flight transition"
        );

        let values = w.content();
        let cut = values
            .iter()
            .zip(control_values.iter())
            .position(|(a, c)| a != c)
            .expect("the applied cut must appear as a divergence from the control");
        assert_eq!(
            &values[..cut],
            &control_values[..cut],
            "the pre-cut stretch must be content-identical to the \
                 no-seek control"
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
        let _ = control_runtime.dispose();
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

/// An invalid desired update is REFUSED, honestly: the old configuration
/// continues bit-exactly, the refusal is reported through the probe's
/// diagnostic sink (never a processing failure, never a terminal).
#[test]
fn an_invalid_update_is_refused_and_the_old_config_continues() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let initial = preset_config(EqPreset::Flat);
        let mut invalid = preset_config(EqPreset::Rock);
        invalid.gain = f32::NAN;

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
        probe.request_update(invalid);
        assert!(
            wait_until(Duration::from_secs(5), || !probe.take_refusals().is_empty()),
            "the invalid update must be refused through the sink"
        );
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );

        // No transition ever started; the content is the pure old
        // configuration, bit-exactly.
        assert!(
            !probe
                .events()
                .iter()
                .any(|e| matches!(e, ProbeEvent::TransitionStarted { .. })),
            "a refused update must not start a transition"
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
/// transition to the LATEST desired configuration.
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
        probe.request_update(rock);
        probe.request_update(jazz);
        assert_eq!(
            probe.pending().as_ref(),
            Some(&jazz),
            "the slot is depth-1, latest wins"
        );
        let (start, _) = wait_transition_started(&probe, Duration::from_secs(5));
        assert_eq!(
            handle.wait_terminal(),
            crate::handle::EpisodeTerminalOutcome::Completed
        );

        let started: Vec<usize> = probe
            .events()
            .into_iter()
            .filter_map(|e| match e {
                ProbeEvent::TransitionStarted { at_frame, .. } => Some(at_frame),
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
        let control = probe_control();
        let fail_after_transition = Arc::new(AtomicUsize::new(0));
        let fail_counter = fail_after_transition.clone();
        let control_for_engine = control.clone();
        let build_engine = move |format: &qianqian_audio_api::ports::PcmFormat| {
            let initial_side = EpisodeProcessing::test_driven(
                Box::new(move |block: &mut [f32]| {
                    if fail_counter.load(Ordering::SeqCst) > 0 {
                        let _ = block;
                        return Err("probe transition-side failure".to_owned());
                    }
                    Ok(())
                }),
                Box::new(|| {}),
            );
            crate::live_probe::LiveProbeEngine::with_initial_processor(
                initial_side,
                preset_config(EqPreset::Flat),
                format,
                TEST_RATE,
                control_for_engine,
            )
        };
        runtime
            .register_component(
                crate::session::playback_session_spec_with_live_probe_engine(
                    std::path::PathBuf::from(DUMMY_PATH),
                    handle.clone(),
                    build_engine,
                ),
            )
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
        control.request_update(preset_config(EqPreset::Rock));

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
            diagnostic.contains("probe transition-side failure"),
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
    use crate::live_probe::mutant_remainder_reprocessed;
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
    use crate::live_probe::mutant_mixed_config;
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
        let _ = wait_transition_started(&probe, Duration::from_secs(5));
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
                .any(|e| matches!(e, ProbeEvent::InvalidationDroppedTransition { .. })),
            "the mutant must exhibit the missing invalidation"
        );

        // The landing oracle: post-cut == fresh accepted config.
        let mut fresh = EpisodeProcessing::new(&desired, &format()).expect("compiles");
        let mut post_input = source_input(landing_frames, five_seconds);
        fresh.stage(&mut post_input).expect("stages");
        let expected: Vec<f32> = post_input.chunks(2).map(|f| f[0]).collect();
        let values = w.content();
        assert_ne!(
            values, expected,
            "the mutant world must observably differ from the fresh \
                 landing"
        );
        assert!(crate::processing_support::rejects(|| {
            assert_eq!(
                values, expected,
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
