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
fn wait_for_witness(mut pred: impl FnMut() -> bool, limit: Duration, what: &str) {
    let deadline = std::time::Instant::now() + limit;
    while !pred() {
        assert!(
            std::time::Instant::now() < deadline,
            "witness never appeared: {what}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Witness the "consumer parked on an empty edge" shape: the render leg
/// has consumed at least one frame — exact mechanism evidence, since a
/// fast consumer keeps the bounded edge empty and the edge alone cannot
/// show production — and the edge has then stayed empty across a window
/// long enough that a fast producer would have refilled it. The paced
/// producer's next frame is a full pacing gap away, so when this returns
/// the consumer is parked inside `read_frames` on an empty edge.
fn wait_for_a_parked_consumer(
    completion: &SessionCompletion,
    consumed: &std::sync::atomic::AtomicUsize,
    limit: Duration,
) {
    use std::sync::atomic::Ordering;
    const QUIET: Duration = Duration::from_millis(80);
    let deadline = std::time::Instant::now() + limit;
    let mut empty_since: Option<std::time::Instant> = None;
    loop {
        let produced = consumed.load(Ordering::SeqCst) > 0;
        match (produced, completion.buffered_frames()) {
            (true, Some(0)) => match empty_since {
                None => empty_since = Some(std::time::Instant::now()),
                Some(since) if since.elapsed() >= QUIET => return,
                Some(_) => {}
            },
            _ => empty_since = None,
        }
        assert!(
            std::time::Instant::now() < deadline,
            "witness never appeared: consumer parked on an empty edge"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn registered_runtime(
    source: SourceBehavior,
    output: OutputBehavior,
    completion: SessionCompletion,
) -> QianqianApp {
    registered_runtime_observed(source, output, completion).0
}

/// The same runtime, with the render double's consumption counter
/// surfaced: tests that must witness "the episode really produced audio"
/// read it instead of guessing from an edge a fast consumer keeps empty.
fn registered_runtime_observed(
    source: SourceBehavior,
    output: OutputBehavior,
    completion: SessionCompletion,
) -> (QianqianApp, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
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

    let consumed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    runtime
        .register_component({
            let behavior = output;
            let consumed = consumed.clone();
            qianqian_composition::ComponentSpec::new("test_output_plugin")
                .provides::<qianqian_audio_api::ports::AudioOutputCapability>()
                .on_activate(move |ctx| {
                    let service = TestOutput::observed(behavior, consumed.clone());
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
    (runtime, consumed)
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
/// disposal is quiet, no leg thread leaks. The episode is genuinely
/// mid-flight — a large burst has been produced and consumed, and the
/// paced tail leaves the consumer parked on the empty edge — when the
/// stop lands.
#[test]
fn stop_while_playing_resolves_stopped_and_disposes_quietly() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let (mut runtime, consumed) = registered_runtime_observed(
            SourceBehavior::Paced {
                after: 100_000,
                delay: Duration::from_millis(250),
            },
            OutputBehavior::Consume,
            completion.clone(),
        );
        activate(&mut runtime);

        wait_for_a_parked_consumer(&completion, &consumed, Duration::from_secs(5));
        // The witness guarantees the empty window is fresh; the next
        // paced frame is still ~170 ms away, so the stop lands with the
        // consumer parked.
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

/// Stop while the consumer is genuinely blocked on an empty edge: after
/// a produced burst, the paced producer's next frame is a full pacing
/// gap away, so the consumer is parked inside `read_frames` on the empty
/// edge when the stop lands. The stop must wake it and resolve Stopped.
#[test]
fn stop_wakes_a_consumer_blocked_on_an_empty_edge() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(15), move || {
        let completion = SessionCompletion::new();
        let (mut runtime, consumed) = registered_runtime_observed(
            SourceBehavior::Paced {
                after: 100_000,
                delay: Duration::from_millis(250),
            },
            OutputBehavior::Consume,
            completion.clone(),
        );
        activate(&mut runtime);

        wait_for_a_parked_consumer(&completion, &consumed, Duration::from_secs(5));

        completion.request_stop();
        assert_eq!(completion.wait(), SessionOutcome::Stopped);

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

/// Decode failure dominates stop intent, deterministically: at the seam,
/// a published decode failure resolves Failed{"decode: ..."} whether the
/// stop was requested before or after it. (At the session level the two
/// can only coexist if the failure wins the race, because a stop applied
/// at bind time ends the episode at the producer's first write — which
/// is why this precedence rule is pinned here, directly on the resolver's
/// inputs, rather than through a racy episode.)
#[test]
fn a_stop_cannot_downgrade_a_decode_failure() {
    // Stop first, failure second.
    let completion = SessionCompletion::new();
    completion.request_stop();
    completion.decode_failed("test decode failure");
    let outcome = completion.try_resolve_now();
    assert_eq!(
        outcome,
        Some(SessionOutcome::Failed {
            stage: "decode: test decode failure".to_owned()
        }),
        "a committed decode failure is not relabelled by stop intent"
    );
    assert!(
        completion.stop_requested(),
        "the command was recorded; it simply did not win"
    );
    assert_eq!(
        completion.try_resolve_now(),
        outcome,
        "resolution is stable across reads"
    );

    // Failure first, stop second: same answer.
    let completion = SessionCompletion::new();
    completion.decode_failed("test decode failure");
    completion.request_stop();
    assert_eq!(
        completion.try_resolve_now(),
        Some(SessionOutcome::Failed {
            stage: "decode: test decode failure".to_owned()
        })
    );
}

/// Without any stop intent, a decoder failure lands Failed{"decode: ..."}
/// through the real session: the failure path is not disturbed by the
/// stop seam's presence.
#[test]
fn a_decode_failure_without_any_stop_lands_failed_decode() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::FailAfter(48_000),
            OutputBehavior::Consume,
            completion.clone(),
        );
        activate(&mut runtime);

        let outcome = completion.wait();
        assert!(
            matches!(&outcome, SessionOutcome::Failed { stage } if stage.starts_with("decode")),
            "a decoder failure must land Failed{{decode}}, got {outcome:?}"
        );
        assert!(!completion.stop_requested(), "nobody requested a stop");

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

/// A render leg that aborts on its own — the real device-failure shape:
/// the render loop exits, stops the data plane, and reports the drain
/// aborted — must resolve Failed{"device"}, never Stopped. Regression
/// for the resolve() discriminator: the worker's Stopped terminal is
/// identical for a user stop and a device death, so recorded stop intent
/// is the only honest witness (a stop nobody requested is not a stop).
#[test]
fn a_device_abort_without_stop_request_lands_failed_device() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(4_000_000),
            OutputBehavior::AbortMidStream { after_reads: 4 },
            completion.clone(),
        );
        activate(&mut runtime);

        assert_eq!(
            completion.wait(),
            SessionOutcome::Failed {
                stage: "device".to_owned()
            },
            "an abort nobody requested is a device failure, not a stop"
        );
        assert!(
            !completion.stop_requested(),
            "no stop intent was ever recorded"
        );

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
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
