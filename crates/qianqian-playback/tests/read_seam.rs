//! F2 public read-seam tests: the application-facing episode surface is
//! exactly `PlaybackSessionHandle` (`request_stop` / `observe` /
//! `wait_terminal`) over `PlaybackSessionObservation` /
//! `EpisodeTerminalOutcome`.
//! These tests consume ONLY that public surface — the same rights a
//! future UI adapter will have — and pin its truth-class contract:
//! pending is the absence of a Fact, `stop_requested` is Command state,
//! `source_format` is mechanism evidence, `activation_error` is a
//! diagnostic, and no observation ever creates truth.
//!
//! The authority-side properties (settlement autonomy, wait purity,
//! coherence races, the decision-boundary witnesses and the
//! no-settlement-thread rule) need the crate-internal seam and live in
//! `src/settlement_contract_tests.rs` (D14.3).

use std::time::Duration;

use qianqian_playback::{EpisodeTerminalOutcome, PlaybackSessionHandle, playback_session_spec};

mod common;

use common::{OutputBehavior, SourceBehavior, TestDecode, TestOutput};

#[test]
fn a_fresh_episode_observes_pending_with_no_side_evidence() {
    let handle = PlaybackSessionHandle::new();
    let observation = handle.observe();
    assert_eq!(observation.terminal_outcome, None);
    assert!(!observation.stop_requested);
    assert_eq!(observation.source_format, None);
    assert_eq!(observation.activation_error, None);
    // Clones reference the same episode: the same coherent truth.
    assert_eq!(handle.clone().observe(), observation);
}

#[test]
fn stop_request_is_command_state_not_a_terminal_fact() {
    let handle = PlaybackSessionHandle::new();
    handle.request_stop();
    let observation = handle.observe();
    assert!(observation.stop_requested, "intent recorded");
    assert_eq!(
        observation.terminal_outcome, None,
        "nothing but settlement commits a terminal Fact"
    );
    // Idempotent, monotone: repeated requests change nothing.
    handle.request_stop();
    assert!(handle.observe().stop_requested);
    assert_eq!(handle.observe().terminal_outcome, None);
}

#[test]
fn repeated_observation_is_a_pure_stable_read() {
    let handle = PlaybackSessionHandle::new();
    handle.request_stop();
    let first = handle.observe();
    for _ in 0..1000 {
        assert_eq!(handle.observe(), first);
    }
    assert_eq!(first.terminal_outcome, None);
}

#[test]
fn a_stopped_episode_observes_its_fact_with_the_command_recorded() {
    // Full real composition on mechanism doubles: the stop command flows
    // through the seam, the authority settles on its own, and the
    // observation then shows the committed Fact together with the
    // recorded intent — never one without the other.
    let _lifecycle = common::lifecycle_lock();
    let handle = PlaybackSessionHandle::new();
    let mut runtime = qianqian_app::QianqianApp::new();
    runtime
        .register_component({
            let behavior = SourceBehavior::EofAfter(4_000_000);
            qianqian_composition::ComponentSpec::new("test_decode_plugin")
                .provides::<qianqian_audio_api::ports::PcmDecodeCapability>()
                .on_activate(move |ctx| {
                    ctx.provide::<qianqian_audio_api::ports::PcmDecodeCapability>(
                        std::rc::Rc::new(TestDecode { behavior }),
                    )
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("decode provider registers");
    runtime
        .register_component({
            let behavior = OutputBehavior::Consume;
            qianqian_composition::ComponentSpec::new("test_output_plugin")
                .provides::<qianqian_audio_api::ports::AudioOutputCapability>()
                .on_activate(move |ctx| {
                    ctx.provide::<qianqian_audio_api::ports::AudioOutputCapability>(
                        std::rc::Rc::new(TestOutput::new(behavior)),
                    )
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("output provider registers");
    runtime
        .register_component(playback_session_spec(
            std::path::PathBuf::from("test://read-seam"),
            handle.clone(),
        ))
        .expect("session registers");
    runtime
        .revise_desired(vec![
            qianqian_composition::DesiredEntry::enabled(
                "decode",
                "test_decode_plugin",
                qianqian_composition::Revision::new(1),
            ),
            qianqian_composition::DesiredEntry::enabled(
                "output",
                "test_output_plugin",
                qianqian_composition::Revision::new(1),
            ),
            qianqian_composition::DesiredEntry::enabled(
                "session",
                "playback_session",
                qianqian_composition::Revision::new(1),
            ),
        ])
        .expect("composition is legal");

    let _ = &_lifecycle;
    handle.request_stop();
    assert_eq!(
        common::within(Duration::from_secs(10), {
            let handle = handle.clone();
            move || handle.wait_terminal()
        }),
        EpisodeTerminalOutcome::Stopped
    );
    let observation = handle.observe();
    assert_eq!(
        observation.terminal_outcome,
        Some(EpisodeTerminalOutcome::Stopped)
    );
    assert!(observation.stop_requested);
    assert!(
        observation.source_format.is_some(),
        "a settled real episode published its mechanism-evidence format"
    );
    assert_eq!(observation.activation_error, None);
    let snapshot = runtime.dispose();
    assert!(snapshot.quiet);
}
