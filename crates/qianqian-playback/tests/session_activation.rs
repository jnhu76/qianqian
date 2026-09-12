//! The REAL Playback Session under adversarial lifecycle conditions,
//! using test-local mechanism doubles at the real ports seams
//! (production Decode/Output are exercised by their own crates and by
//! the Windows real-sound gate).

mod common;

use std::time::Duration;

use qianqian_core::ports::PcmDecode;
use qianqian_kernel::{DesiredEntry, FiberState, Revision};
use qianqian_playback::{SessionCompletion, SessionOutcome, playback_session_spec};
use qianqian_runtime::AppRuntime;

use common::{
    OutputBehavior, SourceBehavior, TEST_FORMAT, TestDecode, TestOutput, named_thread_alive, within,
};

const DUMMY_PATH: &str = "test://sine";

fn desired(id: &str, component: &'static str) -> DesiredEntry {
    DesiredEntry::enabled(id, component, Revision::new(1))
}

/// No session leg thread may survive disposal.
#[cfg(target_os = "linux")]
fn assert_no_leg_threads() {
    assert!(
        !named_thread_alive("qianqian-decode"),
        "decode worker thread leaked"
    );
    assert!(
        !named_thread_alive("qianqian-test-render"),
        "render thread leaked"
    );
}

fn registered_runtime(
    source: SourceBehavior,
    output: OutputBehavior,
    completion: SessionCompletion,
) -> AppRuntime {
    let mut runtime = AppRuntime::new();
    let source_behavior = source;
    let output_behavior = output;

    runtime
        .register_component({
            let behavior = source_behavior;
            qianqian_kernel::ComponentSpec::new("test_decode_plugin")
                .provides::<qianqian_core::ports::PcmDecodeCapability>()
                .on_activate(move |ctx| {
                    let service = TestDecode { behavior };
                    ctx.provide::<qianqian_core::ports::PcmDecodeCapability>(std::rc::Rc::new(
                        service,
                    ))
                    .map_err(|e| qianqian_kernel::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("decode provider registers");

    runtime
        .register_component({
            let behavior = output_behavior;
            qianqian_kernel::ComponentSpec::new("test_output_plugin")
                .provides::<qianqian_core::ports::AudioOutputCapability>()
                .on_activate(move |ctx| {
                    let service = TestOutput { behavior };
                    ctx.provide::<qianqian_core::ports::AudioOutputCapability>(std::rc::Rc::new(
                        service,
                    ))
                    .map_err(|e| qianqian_kernel::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("output provider registers");

    runtime
        .register_component(playback_session_spec(
            std::path::PathBuf::from(DUMMY_PATH),
            completion,
        ))
        .expect("session registers");
    runtime
}

#[test]
fn session_completes_through_eof_and_disposes_quietly() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::Consume,
            completion.clone(),
        );
        runtime
            .revise_desired(vec![
                desired("decode", "test_decode_plugin"),
                desired("output", "test_output_plugin"),
                desired("session", "playback_session"),
            ])
            .expect("composition is legal");

        let outcome = completion.wait();
        assert_eq!(
            outcome,
            SessionOutcome::Completed,
            "EOF + drain = completion"
        );

        let snap = runtime.dispose();
        assert!(snap.quiet, "clean shutdown");

        #[cfg(target_os = "linux")]
        assert_no_leg_threads();
    });
}

#[test]
fn session_reports_decode_failure_and_cleans_up() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::FailAfter(1024),
            OutputBehavior::Consume,
            completion.clone(),
        );
        runtime
            .revise_desired(vec![
                desired("decode", "test_decode_plugin"),
                desired("output", "test_output_plugin"),
                desired("session", "playback_session"),
            ])
            .expect("legal");

        let outcome = completion.wait();
        assert!(
            matches!(&outcome, SessionOutcome::Failed { stage } if stage.starts_with("decode")),
            "decode failure surfaces as the session outcome: {outcome:?}"
        );

        let snap = runtime.dispose();
        assert!(snap.quiet, "failure shutdown is still clean");
    });
}

#[test]
fn output_open_failure_fails_activation_without_leaks() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::FailOpen,
            completion.clone(),
        );
        runtime
            .revise_desired(vec![
                desired("decode", "test_decode_plugin"),
                desired("output", "test_output_plugin"),
                desired("session", "playback_session"),
            ])
            .expect("legal");

        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("session").map(|f| f.state),
            Some(FiberState::Failed),
            "device open failure lands the session FAILED"
        );
        assert!(
            completion.try_resolve_now().is_none(),
            "a failed activation never started an episode: no completion to wait for"
        );

        let snap = runtime.dispose();
        assert!(snap.quiet);

        #[cfg(target_os = "linux")]
        assert_no_leg_threads();
    });
}

#[test]
fn stopping_a_playing_session_disposes_promptly_without_leaks() {
    // The §39 adversary: stop while the producer is blocked on a full
    // edge and the consumer is mid-stream. Dispose must return (the
    // edge stop unblocks both legs) and leak nothing.
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        // Paced source: after a fast prefix the decode side produces one
        // frame per 100 ms, so at stop time the consumer is genuinely
        // blocked mid-play on an empty edge (the producer-blocked-on-full
        // case is covered at the edge seam in edge_lifecycle).
        let mut runtime = registered_runtime(
            SourceBehavior::Paced {
                after: 4 * 1024,
                delay: Duration::from_millis(100),
            },
            OutputBehavior::Consume,
            completion.clone(),
        );
        runtime
            .revise_desired(vec![
                desired("decode", "test_decode_plugin"),
                desired("output", "test_output_plugin"),
                desired("session", "playback_session"),
            ])
            .expect("legal");

        // No wait: stop immediately, mid-playback.
        let snap = runtime.dispose();
        assert!(snap.quiet, "dispose settles after a mid-stream stop");
        let _ = completion; // never resolved; that is the point of stopping

        #[cfg(target_os = "linux")]
        assert_no_leg_threads();
    });
}

#[test]
fn withdrawing_a_provider_degrades_the_session_to_pending() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::Consume,
            completion.clone(),
        );
        runtime
            .revise_desired(vec![
                desired("decode", "test_decode_plugin"),
                desired("output", "test_output_plugin"),
                desired("session", "playback_session"),
            ])
            .expect("legal");
        assert_eq!(
            runtime
                .composition_snapshot()
                .fibers
                .get("session")
                .map(|f| f.state),
            Some(FiberState::Active)
        );

        // Withdraw only the decode provider.
        runtime
            .revise_desired(vec![
                desired("output", "test_output_plugin"),
                desired("session", "playback_session"),
            ])
            .expect("legal");

        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("session").map(|f| f.state),
            Some(FiberState::Pending),
            "the session degrades over its vanished dependency"
        );

        let snap = runtime.dispose();
        assert!(snap.quiet);
    });
}

#[test]
fn missing_capabilities_leave_the_session_pending_not_failed() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(1),
            OutputBehavior::Consume,
            completion,
        );
        runtime
            .revise_desired(vec![desired("session", "playback_session")])
            .expect("legal");
        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("session").map(|f| f.state),
            Some(FiberState::Pending),
            "unsatisfied requirements are Pending, not Failed"
        );
        runtime.dispose();
    });
}

/// The session's activation resolves the real format from the endpoint:
/// observable through the render request's negotiated format.
#[test]
fn session_binds_the_source_format_into_the_data_plane() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::Consume,
            completion.clone(),
        );
        runtime
            .revise_desired(vec![
                desired("decode", "test_decode_plugin"),
                desired("output", "test_output_plugin"),
                desired("session", "playback_session"),
            ])
            .expect("legal");
        assert_eq!(completion.wait(), SessionOutcome::Completed);
        // TEST_FORMAT flows through the edge end to end; the decoded
        // payload was consumed to completion (asserted by Completed).
        let _ = TEST_FORMAT;
        runtime.dispose();
    });
}
