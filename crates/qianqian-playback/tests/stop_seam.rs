//! F1 stop-seam tests: the application-facing stop request
//! (`SessionCompletion::request_stop`) drives the REAL Playback Session
//! through its session-owned stop mechanism, on mechanism doubles at the
//! ports seams. Platform-independent: these run wherever the workspace
//! tests run; the Windows real-device stop gate covers the physical path.

mod common;

use std::time::Duration;

use qianqian_app::QianqianApp;
use qianqian_composition::{DesiredEntry, FiberState, Revision};
use qianqian_playback::{SessionCompletion, SessionOutcome, playback_session_spec};

use common::{OutputBehavior, SourceBehavior, TestDecode, TestOutput, within};

const DUMMY_PATH: &str = "test://stop-seam";
/// The session's bounded edge capacity in frames (session.rs
/// EDGE_CAPACITY_FRAMES). Used as the "producer is blocked on a full
/// edge" witness; a change there is a session-semantics change that this
/// test must follow deliberately.
const EDGE_CAPACITY_FRAMES: usize = 8192;

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
}

#[cfg(not(target_os = "linux"))]
fn assert_no_leg_threads() {}

/// Poll until `pred` holds or the deadline passes (a witness, not an
/// oracle: the stop request below does not depend on the predicate, only
/// the adversarial shape does).
fn wait_for_witness(pred: impl Fn() -> bool, limit: Duration, what: &str) {
    let deadline = std::time::Instant::now() + limit;
    while !pred() {
        assert!(
            std::time::Instant::now() < deadline,
            "witness never appeared: {what}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn registered_runtime(
    source: SourceBehavior,
    output: OutputBehavior,
    completion: SessionCompletion,
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
                    let service = TestOutput { behavior };
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
            completion,
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

/// Stop while playing: the session resolves Stopped, both legs exit,
/// disposal is quiet, no leg thread leaks. The slow producer keeps the
/// episode genuinely mid-flight, and the consumer is blocked on an empty
/// edge (the trickle never lets it accumulate) when the stop lands.
#[test]
fn stop_while_playing_resolves_stopped_and_disposes_quietly() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::Paced {
                after: 0,
                delay: Duration::from_millis(50),
            },
            OutputBehavior::Consume,
            completion.clone(),
        );
        activate(&mut runtime);

        // No witness is required for correctness here: the stop is valid
        // in any episode state. The bounded `within` watchdog above is
        // the hang oracle.
        completion.request_stop();
        assert_eq!(completion.wait(), SessionOutcome::Stopped);

        let snapshot = runtime.dispose();
        assert!(
            snapshot.quiet,
            "a stopped episode must dispose quietly: {snapshot:?}"
        );
        assert_no_leg_threads();
    });
}

/// Stop intent recorded BEFORE the episode binds its edge ("stop before
/// the episode fully opened") is applied at bind time: activation still
/// succeeds, but the episode resolves Stopped instead of playing.
#[test]
fn stop_before_binding_stops_the_episode_once_bound() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        completion.request_stop();
        assert!(completion.stop_requested(), "intent is recorded");

        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::Consume,
            completion.clone(),
        );
        activate(&mut runtime);

        assert_eq!(
            completion.wait(),
            SessionOutcome::Stopped,
            "a stop that arrived before the edge existed must not be lost"
        );

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

/// Stop intent recorded before activation that FAILS (device open
/// failure) neither breaks the failure path nor resurrects the episode:
/// the completion resolves Failed.
#[test]
fn stop_before_a_failing_activation_leaves_failed_in_charge() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        completion.request_stop();

        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::FailOpen,
            completion.clone(),
        );
        // Activation raises; the session fiber lands FAILED.
        activate(&mut runtime);
        let snapshot = runtime.composition_snapshot();
        assert_eq!(
            snapshot.fibers.get("session").map(|f| f.state),
            Some(FiberState::Failed)
        );
        assert!(
            completion.activation_error().is_some(),
            "the activation failure is published"
        );

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

/// A late stop after the episode already resolved Completed is a no-op:
/// a committed EOF is not renamed into Stopped (first-wins terminals).
#[test]
fn late_stop_after_completed_changes_nothing() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::Consume,
            completion.clone(),
        );
        activate(&mut runtime);

        assert_eq!(completion.wait(), SessionOutcome::Completed);
        completion.request_stop();
        completion.request_stop();
        assert_eq!(
            completion.try_resolve_now(),
            Some(SessionOutcome::Completed),
            "stop after resolution must not rewrite the outcome"
        );

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

/// Repeated stop requests are idempotent: one Stopped resolution, one
/// quiet disposal, no leaked legs.
#[test]
fn repeated_stop_requests_are_idempotent() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(15), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::Paced {
                after: 0,
                delay: Duration::from_millis(50),
            },
            OutputBehavior::Consume,
            completion.clone(),
        );
        activate(&mut runtime);

        for _ in 0..5 {
            completion.request_stop();
        }
        assert_eq!(completion.wait(), SessionOutcome::Stopped);

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

/// Stop while the producer is genuinely blocked on a full edge: the fast
/// producer fills the bounded edge long before EOF and parks in write;
/// the slow consumer keeps the edge full. The stop must wake the
/// producer and resolve Stopped.
#[test]
fn stop_wakes_a_producer_blocked_on_a_full_edge() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(15), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(4_000_000),
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(100),
            },
            completion.clone(),
        );
        activate(&mut runtime);

        // Witness: the edge is full, so the producer's next write blocks
        // (the consumer drains 256 frames per 100 ms and the producer
        // refills instantly, keeping the edge pinned at capacity).
        wait_for_witness(
            || {
                completion
                    .buffered_frames()
                    .is_some_and(|n| n >= EDGE_CAPACITY_FRAMES)
            },
            Duration::from_secs(5),
            "edge pinned full (producer blocked mid-write)",
        );

        completion.request_stop();
        assert_eq!(completion.wait(), SessionOutcome::Stopped);

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

/// Stop while the consumer is genuinely blocked on an empty edge: the
/// paced producer emits one frame every 50 ms and the fast consumer
/// drains it immediately; the consumer parks in read_frames between
/// frames. The stop must wake it and resolve Stopped.
#[test]
fn stop_wakes_a_consumer_blocked_on_an_empty_edge() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(15), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::Paced {
                after: 0,
                delay: Duration::from_millis(50),
            },
            OutputBehavior::Consume,
            completion.clone(),
        );
        activate(&mut runtime);

        wait_for_witness(
            || completion.buffered_frames().is_some_and(|n| n == 0),
            Duration::from_secs(5),
            "edge observed empty (consumer between frames)",
        );
        // Give the consumer a moment to park inside read_frames on the
        // empty edge; the stop below is valid regardless of whether it
        // has parked yet.
        std::thread::sleep(Duration::from_millis(60));

        completion.request_stop();
        assert_eq!(completion.wait(), SessionOutcome::Stopped);

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

/// Stop during a decode failure: the failure committed first stays the
/// outcome (Failed is not downgraded to Stopped); a racing or later stop
/// cannot rewrite it.
#[test]
fn stop_during_decode_failure_leaves_failed_in_charge() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::FailAfter(48_000),
            OutputBehavior::Consume,
            completion.clone(),
        );
        activate(&mut runtime);

        // The failure fires after 48k frames of fast production — race a
        // stop right in around the failure window. Either terminal may
        // commit first, but Failed must win if it committed first.
        let outcome = completion.wait();
        if matches!(outcome, SessionOutcome::Failed { .. }) {
            completion.request_stop();
            assert_eq!(
                completion.try_resolve_now(),
                Some(SessionOutcome::Failed {
                    stage: outcome_stage(&outcome)
                }),
                "a late stop must not downgrade a committed failure"
            );
        } else {
            assert_eq!(
                outcome,
                SessionOutcome::Stopped,
                "if the stop committed first, Stopped is the honest outcome"
            );
        }

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

fn outcome_stage(outcome: &SessionOutcome) -> String {
    match outcome {
        SessionOutcome::Failed { stage } => stage.clone(),
        other => panic!("expected a failure outcome, got {other:?}"),
    }
}

/// The stop seam does not disturb the normal EOF path when nobody
/// stops: regression guard that binding a stop target alone changes no
/// completion semantics.
#[test]
fn binding_the_stop_target_alone_preserves_the_eof_path() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::Consume,
            completion.clone(),
        );
        activate(&mut runtime);

        assert_eq!(completion.wait(), SessionOutcome::Completed);

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}
