//! F1 stop-seam tests: the application-facing stop request
//! (`PlaybackSessionHandle::request_stop` — pre-F2 spelling:
//! `SessionCompletion::request_stop`) drives the REAL Playback Session
//! through its session-owned stop mechanism, on mechanism doubles at the
//! ports seams. Platform-independent: these run wherever the workspace
//! tests run; the Windows real-device stop gate covers the physical
//! path.
//!
//! These drive ONLY the public F2 episode seam. The adversarial-shape
//! witnesses (buffered_frames) and the resolver-level precedence tests
//! moved to the crate-internal settlement contract tests when the
//! evidence mutators became crate-private (D14.3).

mod common;

use std::time::Duration;

use qianqian_app::QianqianApp;
use qianqian_composition::{DesiredEntry, FiberState, Revision};
use qianqian_playback::{EpisodeTerminalOutcome, PlaybackSessionHandle, playback_session_spec};

use common::{OutputBehavior, SourceBehavior, TestDecode, TestOutput, within};

const DUMMY_PATH: &str = "test://stop-seam";

fn desired(id: &str, component: &'static str) -> DesiredEntry {
    DesiredEntry::enabled(id, component, Revision::new(1))
}

#[cfg(target_os = "linux")]
fn assert_no_leg_threads() {
    const LEAK_ORACLE_GRACE: Duration = Duration::from_secs(2);
    assert!(
        common::named_thread_gone_within("qianqian-decode", LEAK_ORACLE_GRACE),
        "decode worker thread leaked"
    );
    assert!(
        common::named_thread_gone_within("qianqian-test-render", LEAK_ORACLE_GRACE),
        "render thread leaked"
    );
    // No settlement/resolver thread may exist at all (D14.3): the
    // publication paths settle synchronously on the legs' own stacks.
    assert!(
        !common::named_thread_alive("qianqian-settle"),
        "a settlement/resolver thread exists (D14.3 forbids one)"
    );
}

#[cfg(not(target_os = "linux"))]
fn assert_no_leg_threads() {}

fn registered_runtime(
    source: SourceBehavior,
    output: OutputBehavior,
    handle: PlaybackSessionHandle,
) -> QianqianApp {
    let mut runtime = QianqianApp::new();

    runtime
        .register_component({
            let behavior = source;
            qianqian_composition::ComponentSpec::new("test_decode_plugin")
                .provides::<qianqian_audio_api::ports::PcmDecodeCapability>()
                .on_activate(move |ctx| {
                    let service = TestDecode { behavior };
                    ctx.provide::<qianqian_audio_api::ports::PcmDecodeCapability>(
                        std::rc::Rc::new(service),
                    )
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("decode provider registers");

    runtime
        .register_component({
            let behavior = output;
            qianqian_composition::ComponentSpec::new("test_output_plugin")
                .provides::<qianqian_audio_api::ports::AudioOutputCapability>()
                .on_activate(move |ctx| {
                    let service = TestOutput::new(behavior);
                    ctx.provide::<qianqian_audio_api::ports::AudioOutputCapability>(
                        std::rc::Rc::new(service),
                    )
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("output provider registers");

    runtime
        .register_component(playback_session_spec(
            std::path::PathBuf::from(DUMMY_PATH),
            handle,
        ))
        .expect("session registers");
    runtime
}

fn activate(runtime: &mut QianqianApp) {
    runtime
        .revise_desired(vec![
            desired("decode", "test_decode_plugin"),
            desired("output", "test_output_plugin"),
            desired("session", "playback_session"),
        ])
        .expect("composition is legal");
}

/// Stop intent recorded BEFORE the episode binds its edge ("stop before
/// the episode fully opened") is applied at bind time: activation still
/// succeeds, but the episode settles Stopped instead of playing.
#[test]
fn stop_before_binding_stops_the_episode_once_bound() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let handle = PlaybackSessionHandle::new();
        handle.request_stop();
        assert!(handle.observe().stop_requested, "intent is recorded");

        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::Consume,
            handle.clone(),
        );
        activate(&mut runtime);

        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Stopped,
            "a stop that arrived before the edge existed must not be lost"
        );

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

/// Stop intent recorded before activation that FAILS (device open
/// failure) neither breaks the failure path nor resurrects the episode:
/// the diagnostic stays a diagnostic, the terminal Fact stays absent.
#[test]
fn stop_before_a_failing_activation_leaves_the_diagnostic_in_charge() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let handle = PlaybackSessionHandle::new();
        handle.request_stop();

        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::FailOpen,
            handle.clone(),
        );
        // Activation raises; the session fiber lands FAILED.
        activate(&mut runtime);
        let snapshot = runtime.composition_snapshot();
        assert_eq!(
            snapshot.fibers.get("session").map(|f| f.state),
            Some(FiberState::Failed)
        );
        let observation = handle.observe();
        assert_eq!(
            observation.terminal_outcome, None,
            "an activation failure never forges a terminal Fact"
        );
        assert!(
            observation.activation_error.is_some(),
            "the activation failure is published"
        );

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

/// A late stop after the episode already settled Completed is a no-op:
/// a committed EOF is not renamed into Stopped (first-wins terminals).
#[test]
fn late_stop_after_completed_changes_nothing() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::Consume,
            handle.clone(),
        );
        activate(&mut runtime);

        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        handle.request_stop();
        handle.request_stop();
        assert_eq!(
            handle.observe().terminal_outcome,
            Some(EpisodeTerminalOutcome::Completed),
            "stop after settlement must not rewrite the outcome"
        );

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

/// Repeated stop requests are idempotent: one Stopped settlement, one
/// quiet disposal, no leaked legs.
#[test]
fn repeated_stop_requests_are_idempotent() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(15), move || {
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            SourceBehavior::Paced {
                after: 0,
                delay: Duration::from_millis(50),
            },
            OutputBehavior::Consume,
            handle.clone(),
        );
        activate(&mut runtime);

        for _ in 0..5 {
            handle.request_stop();
        }
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

/// Without any stop intent, a decoder failure lands Failed{"decode: ..."}
/// through the real session: the failure path is not disturbed by the
/// stop seam's presence.
#[test]
fn a_decode_failure_without_any_stop_lands_failed_decode() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            SourceBehavior::FailAfter(48_000),
            OutputBehavior::Consume,
            handle.clone(),
        );
        activate(&mut runtime);

        let outcome = handle.wait_terminal();
        assert_eq!(
            outcome,
            EpisodeTerminalOutcome::Failed,
            "a decoder failure must land Failed, got {outcome:?}"
        );
        let observation = handle.observe();
        assert!(
            observation
                .failure_diagnostic
                .as_deref()
                .is_some_and(|stage| stage.starts_with("decode")),
            "the decode diagnostic travels separately: {observation:?}"
        );
        assert!(!observation.stop_requested, "nobody requested a stop");

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

/// A render leg that aborts on its own — the real device-failure shape:
/// the render loop exits, stops the data plane, and reports the drain
/// aborted — must settle Failed{"device"}, never Stopped. Regression
/// for the settlement discriminator: the worker's Stopped terminal is
/// identical for a user stop and a device death, so recorded stop intent
/// is the only honest witness (a stop nobody requested is not a stop).
#[test]
fn a_device_abort_without_stop_request_lands_failed_device() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(4_000_000),
            OutputBehavior::AbortMidStream { after_reads: 4 },
            handle.clone(),
        );
        activate(&mut runtime);

        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Failed,
            "an abort nobody requested is a device failure, not a stop"
        );
        let observation = handle.observe();
        assert_eq!(
            observation.failure_diagnostic.as_deref(),
            Some("device"),
            "the device diagnostic travels separately"
        );
        assert!(
            !observation.stop_requested,
            "no stop intent was ever recorded"
        );

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

/// The stop seam does not disturb the normal EOF path when nobody
/// stops: regression guard that binding a stop target alone changes no
/// settlement semantics.
#[test]
fn binding_the_stop_target_alone_preserves_the_eof_path() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::Consume,
            handle.clone(),
        );
        activate(&mut runtime);

        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}
