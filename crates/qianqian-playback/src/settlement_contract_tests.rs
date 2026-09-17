//! Crate-internal white-box tests for the F2 truthful read seam and the
//! D14.3 authority-owned terminal settlement.
//!
//! These live inside the crate boundary because the properties they pin
//! are exactly the ones that must NOT be reachable from the product
//! seam: evidence mutators, the settlement step, the drain verdict
//! mirroring and the `buffered_frames` diagnostic (D14.3 — verifier
//! needs must not leak mechanism fields back into the product seam).
//! The application-visible half of the behavior is pinned separately by
//! the public integration tests (`tests/read_seam.rs`,
//! `tests/stop_seam.rs`).
//!
//! Test-matrix references (QIANQIAN-F2-TRUTHFUL-READ-SIDE-IMPLEMENTATION-2
//! §15, plus CORRECTIVE-1): T1 fresh observation, T2 source format, T3
//! stop command state, T4 Completed, T5 Stopped, T6 Failed, T7
//! activation failure, T8 observe purity, T9 wait purity, T10
//! consumer-free settlement (worker-last + drain-last), T11 late-stop
//! stability, T12 coherent observation race, T15 teardown leaves no
//! decisive evidence uncommitted, T16 (F4-IMPLEMENTATION-CORRECTIVE-1)
//! activation failure withdraws a position the render mechanism already
//! published. M4-RUST-A/B and W4-RUST are the
//! dynamic decision-boundary witnesses corresponding to the formal
//! model's M4/W4: the decisive publication paths commit synchronously
//! (before returning), and a late stop cannot relabel the classification.

use std::sync::Arc;
use std::time::Duration;

use qianqian_audio_api::ports::DrainVerdict;

use crate::completion::{SessionCompletion, SessionOutcome};
use crate::edge::EdgeTerminal;
use crate::handle::{EpisodeTerminalOutcome, PlaybackSessionHandle};
use crate::test_common::{OutputBehavior, SourceBehavior, TestDecode, TestOutput, within};

const DUMMY_PATH: &str = "test://settlement-contract";
/// The session's bounded edge capacity in frames (session.rs
/// EDGE_CAPACITY_FRAMES), used as the "producer blocked on a full edge"
/// witness.
const EDGE_CAPACITY_FRAMES: usize = 8192;

/// D14.3: no resolver/settlement thread may exist — ever. The
/// publication paths settle synchronously on the legs' own call stacks;
/// there is nothing for a dedicated thread to do.
#[cfg(target_os = "linux")]
fn assert_no_settlement_thread() {
    assert!(
        !crate::test_common::named_thread_alive("qianqian-settle"),
        "a settlement/resolver thread exists (D14.3 forbids one)"
    );
}

#[cfg(not(target_os = "linux"))]
fn assert_no_settlement_thread() {}

#[cfg(target_os = "linux")]
fn assert_no_leg_threads() {
    const LEAK_ORACLE_GRACE: Duration = Duration::from_secs(2);
    assert!(
        crate::test_common::named_thread_gone_within("qianqian-decode", LEAK_ORACLE_GRACE),
        "decode worker thread leaked"
    );
    assert!(
        crate::test_common::named_thread_gone_within("qianqian-test-render", LEAK_ORACLE_GRACE),
        "render thread leaked"
    );
    assert_no_settlement_thread();
}

#[cfg(not(target_os = "linux"))]
fn assert_no_leg_threads() {}

fn desired(id: &str, component: &'static str) -> qianqian_composition::DesiredEntry {
    qianqian_composition::DesiredEntry::enabled(
        id,
        component,
        qianqian_composition::Revision::new(1),
    )
}

/// Register the test decode/output doubles plus one real Playback
/// Session over the given handle, activate, and return the runtime with
/// the render double's consumption counter (the "episode really
/// produced audio" witness).
fn live_runtime(
    source: SourceBehavior,
    output: OutputBehavior,
    handle: PlaybackSessionHandle,
) -> (
    qianqian_app::QianqianApp,
    Arc<std::sync::atomic::AtomicUsize>,
) {
    let mut runtime = qianqian_app::QianqianApp::new();
    runtime
        .register_component({
            let behavior = source;
            qianqian_composition::ComponentSpec::new("test_decode_plugin")
                .provides::<qianqian_audio_api::ports::PcmDecodeCapability>()
                .on_activate(move |ctx| {
                    ctx.provide::<qianqian_audio_api::ports::PcmDecodeCapability>(
                        std::rc::Rc::new(TestDecode::new(behavior)),
                    )
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("decode provider registers");
    let consumed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
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
        .register_component(crate::playback_session_spec(
            std::path::PathBuf::from(DUMMY_PATH),
            handle,
        ))
        .expect("session registers");
    runtime
        .revise_desired(vec![
            desired("decode", "test_decode_plugin"),
            desired("output", "test_output_plugin"),
            desired("session", "playback_session"),
        ])
        .expect("composition is legal");
    (runtime, consumed)
}

/// Poll the PURE read until a terminal outcome is committed (bounded),
/// then return the full observation. Polling is legitimate here:
/// `observe` settles nothing, so the loop cannot be what produced the
/// outcome — the autonomy claims below are about wait/consumer calls
/// never being required, and observe is not a consumer trigger (T8 pins
/// that separately).
fn await_terminal(
    handle: &PlaybackSessionHandle,
    limit: Duration,
) -> crate::handle::PlaybackSessionObservation {
    let deadline = std::time::Instant::now() + limit;
    loop {
        let observation = handle.observe();
        if observation.terminal_outcome.is_some() {
            return observation;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "terminal outcome was never committed without a consumer call"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

// --- T1: fresh observation ----------------------------------------------------

#[test]
fn fresh_observation_is_pending_with_no_side_evidence() {
    let handle = PlaybackSessionHandle::new();
    let observation = handle.observe();
    assert_eq!(observation.terminal_outcome, None, "pending, not a state");
    assert_eq!(observation.failure_diagnostic, None);
    assert!(!observation.stop_requested);
    assert_eq!(observation.source_format, None);
    assert_eq!(observation.activation_error, None);
    // Pending is the absence of a terminal Fact: nothing in the
    // observation vocabulary spells Playing/Starting (T13's projection
    // oracle pins the text side; structurally there is no such field).
}

// --- T2: source format ---------------------------------------------------------

#[test]
fn source_format_publishes_without_any_terminal_claim() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(10), || {
        let handle = PlaybackSessionHandle::new();
        let (mut runtime, _consumed) = live_runtime(
            SourceBehavior::Paced {
                after: 4 * 1024,
                delay: Duration::from_millis(200),
            },
            OutputBehavior::Consume,
            handle.clone(),
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let observation = handle.observe();
            if observation.source_format.is_some() {
                assert_eq!(
                    observation.terminal_outcome, None,
                    "a format publication is not a terminal claim"
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "format never published"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

// --- T3: stop command state ----------------------------------------------------

#[test]
fn stop_request_records_command_state_not_a_stopped_fact() {
    let handle = PlaybackSessionHandle::new();
    handle.request_stop();
    let observation = handle.observe();
    assert!(
        observation.stop_requested,
        "command state is recorded immediately"
    );
    assert_eq!(
        observation.terminal_outcome, None,
        "stop_requested is not Stopped; only settlement commits a Fact"
    );
}

// --- T4/T5/T6: consumer-free settlement through the real composition ------------

#[test]
fn t4_completed_is_committed_without_observe_or_wait() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(10), || {
        let handle = PlaybackSessionHandle::new();
        let (mut runtime, _consumed) = live_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::Consume,
            handle.clone(),
        );
        // Natural EOF is the drain-last shape: no observe, no wait, no
        // headless polling — the authority must commit on its own.
        let observation = await_terminal(&handle, Duration::from_secs(5));
        assert_eq!(
            observation.terminal_outcome,
            Some(EpisodeTerminalOutcome::Completed)
        );
        assert_eq!(
            observation.failure_diagnostic, None,
            "a completed episode never carries a diagnostic"
        );
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

#[test]
fn t5_stopped_is_committed_without_wait_resolving() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(10), || {
        let handle = PlaybackSessionHandle::new();
        let (mut runtime, _consumed) = live_runtime(
            SourceBehavior::EofAfter(4_000_000),
            OutputBehavior::Consume,
            handle.clone(),
        );
        handle.request_stop();
        let observation = await_terminal(&handle, Duration::from_secs(5));
        assert_eq!(
            observation.terminal_outcome,
            Some(EpisodeTerminalOutcome::Stopped)
        );
        assert!(
            observation.stop_requested,
            "a committed Stopped always observed recorded intent"
        );
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

#[test]
fn t6_decode_failure_is_committed_without_wait() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(10), || {
        let handle = PlaybackSessionHandle::new();
        let (mut runtime, _consumed) = live_runtime(
            SourceBehavior::FailAfter(48_000),
            OutputBehavior::Consume,
            handle.clone(),
        );
        let observation = await_terminal(&handle, Duration::from_secs(5));
        assert_eq!(
            observation.terminal_outcome,
            Some(EpisodeTerminalOutcome::Failed),
            "D11 precedence preserved"
        );
        assert!(
            observation
                .failure_diagnostic
                .as_deref()
                .is_some_and(|stage| stage.starts_with("decode")),
            "the decode diagnostic travels separately: {observation:?}"
        );
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

// --- T7: activation failure firewall --------------------------------------------

#[test]
fn t7_activation_failure_is_diagnostic_not_a_forged_failed_fact() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(10), || {
        let handle = PlaybackSessionHandle::new();
        let (mut runtime, _consumed) = live_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::FailOpen,
            handle.clone(),
        );
        // revise_desired settled before returning: the failed activation
        // is already visible.
        let snapshot = runtime.composition_snapshot();
        assert_eq!(
            snapshot.fibers.get("session").map(|f| f.state),
            Some(qianqian_composition::FiberState::Failed)
        );
        let observation = handle.observe();
        assert_eq!(
            observation.terminal_outcome, None,
            "an episode that never started has no terminal Fact"
        );
        assert!(
            observation.activation_error.is_some(),
            "the activation diagnostic is published"
        );
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
        assert_no_settlement_thread();
    });
}

// --- T8: observe purity -----------------------------------------------------------

#[test]
fn t8_observe_neither_settles_nor_mutates() {
    // Publish non-decisive evidence (worker leg only, no verdict):
    // repeated pure reads must not complete the evidence set or commit
    // anything.
    let completion = SessionCompletion::new();
    completion.worker_exited(EdgeTerminal::Stopped);
    let handle = PlaybackSessionHandle {
        completion: completion.clone(),
    };
    for _ in 0..1000 {
        let observation = handle.observe();
        assert_eq!(observation.terminal_outcome, None, "observe cannot commit");
        assert!(!observation.stop_requested);
    }
    assert_eq!(completion.committed(), None);
    // The pre-corrective "decisive-but-unsettled" construction no longer
    // exists anywhere — that absence IS the decision-boundary fix, and
    // M4-RUST-A/B below prove the commit happens before every decisive
    // publication returns.
    // On a settled core, repeated observation is byte-stable (no
    // lifecycle action, no data-plane touch).
    completion.decode_failed("settling evidence");
    let first = handle.observe();
    for _ in 0..1000 {
        assert_eq!(handle.observe(), first);
    }
}

// --- T9: wait purity ---------------------------------------------------------------

#[test]
fn t9_wait_terminal_is_not_the_settlement_trigger() {
    let completion = SessionCompletion::new();
    // Worker evidence only: published, not decisive. No publication path
    // is left to run, so nothing may commit — including any wait call.
    completion.worker_exited(EdgeTerminal::Stopped);
    assert_eq!(completion.committed(), None);
    let handle = PlaybackSessionHandle {
        completion: completion.clone(),
    };
    let waiter = {
        let handle = handle.clone();
        std::thread::spawn(move || handle.wait_terminal())
    };
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        !waiter.is_finished(),
        "wait_terminal must block instead of resolving"
    );
    assert_eq!(
        completion.committed(),
        None,
        "waiting settled nothing: pure condvar wait"
    );
    // The drain publication path is what completes the evidence set —
    // and it commits synchronously, waking the waiter.
    completion.drain_signal().complete(DrainVerdict::Aborted);
    assert_eq!(
        waiter.join().expect("waiter exits"),
        EpisodeTerminalOutcome::Failed,
        "no stop intent was recorded, so the abort is a device failure"
    );
}

// --- T10: consumer-free settlement, both evidence orders -------------------------

#[test]
fn t10_worker_last_evidence_settles_autonomously() {
    // Drain verdict published first (undecided: the worker has not
    // exited), worker evidence last: the worker publication path itself
    // must commit before it returns.
    let completion = SessionCompletion::new();
    completion.drain_signal().complete(DrainVerdict::Drained);
    assert_eq!(completion.committed(), None);
    completion.worker_exited(EdgeTerminal::Eof);
    assert_eq!(
        completion.committed(),
        Some(SessionOutcome::Completed),
        "the worker evidence path settles synchronously on its own call stack"
    );
}

#[test]
fn t10_drain_last_evidence_settles_synchronously_without_a_watcher() {
    // Natural-EOF shape, worker evidence already in: the drain verdict
    // is the last decisive publication; it must commit inside the
    // complete() call — no watcher, no consumer call.
    let completion = SessionCompletion::new();
    completion.worker_exited(EdgeTerminal::Eof);
    assert_eq!(completion.committed(), None);
    completion.drain_signal().complete(DrainVerdict::Drained);
    assert_eq!(
        completion.committed(),
        Some(SessionOutcome::Completed),
        "the drain publication path settles synchronously on the publishing leg's call stack"
    );
}

// --- M4/W4: dynamic decision-boundary witnesses (formal M4/W4 twins) --------------

/// M4-RUST-A — drain-last then late stop. Proven RED on the reviewed
/// HEAD (daf8117): the old watcher-owned settlement let `complete()`
/// return with the decisive evidence uncommitted, so a later stop was
/// visible to the resolver and relabelled the outcome.
#[test]
fn m4_drain_last_decisive_evidence_commits_before_complete_returns() {
    let completion = SessionCompletion::new();
    completion.worker_exited(EdgeTerminal::Stopped);
    assert_eq!(
        completion.committed(),
        None,
        "worker evidence alone is not decisive"
    );
    completion.drain_signal().complete(DrainVerdict::Aborted);
    // The decision boundary: once complete() returns, the decisive
    // classification must already be committed and immutable.
    assert_eq!(
        completion.committed(),
        Some(SessionOutcome::Failed {
            stage: "device".to_owned()
        }),
        "drain-last decisive evidence must commit synchronously"
    );
    completion.request_stop();
    assert_eq!(
        completion.committed(),
        Some(SessionOutcome::Failed {
            stage: "device".to_owned()
        }),
        "a late stop after the decisive boundary cannot relabel"
    );
}

/// M4-RUST-B — worker-last then late stop. The mirror image of M4-A:
/// the worker publication is the decisive one and must commit before
/// `worker_exited` returns; a later stop cannot relabel.
#[test]
fn m4_worker_last_decisive_evidence_commits_before_worker_exited_returns() {
    let completion = SessionCompletion::new();
    completion.drain_signal().complete(DrainVerdict::Aborted);
    assert_eq!(
        completion.committed(),
        None,
        "an aborted drain alone is not decisive (the worker may still exit)"
    );
    completion.worker_exited(EdgeTerminal::Stopped);
    assert_eq!(
        completion.committed(),
        Some(SessionOutcome::Failed {
            stage: "device".to_owned()
        }),
        "worker-last decisive evidence must commit synchronously"
    );
    completion.request_stop();
    assert_eq!(
        completion.committed(),
        Some(SessionOutcome::Failed {
            stage: "device".to_owned()
        }),
        "a late stop after the decisive boundary cannot relabel"
    );
}

/// W4-RUST — stop really before the boundary: the point is not "Failed
/// always wins" but that the boundary ordering determines truth and late
/// commands do not reinterpret it. Intent linearized BEFORE the decisive
/// publication legitimately yields Stopped.
#[test]
fn w4_stop_before_the_boundary_decides_stopped() {
    let completion = SessionCompletion::new();
    completion.request_stop();
    completion.worker_exited(EdgeTerminal::Stopped);
    assert_eq!(completion.committed(), None, "no verdict yet: undecided");
    completion.drain_signal().complete(DrainVerdict::Aborted);
    assert_eq!(
        completion.committed(),
        Some(SessionOutcome::Stopped),
        "intent recorded before the decisive boundary makes it Stopped"
    );
}

// --- T11: late command stability ---------------------------------------------------

#[test]
fn t11_late_stop_cannot_relabel_a_decisive_failed() {
    let completion = SessionCompletion::new();
    completion.decode_failed("bitstream error");
    assert!(matches!(
        completion.committed(),
        Some(SessionOutcome::Failed { .. })
    ));
    completion.request_stop();
    assert!(
        matches!(completion.committed(), Some(SessionOutcome::Failed { .. })),
        "a late stop is command history, not a relabel"
    );
    assert!(
        completion.observe_snapshot().stop_requested,
        "the command was recorded; it simply did not win"
    );
}

#[test]
fn t11_late_stop_cannot_relabel_a_committed_completed() {
    let completion = SessionCompletion::new();
    completion.worker_exited(EdgeTerminal::Eof);
    completion.drain_signal().complete(DrainVerdict::Drained);
    assert_eq!(completion.committed(), Some(SessionOutcome::Completed));
    completion.request_stop();
    completion.request_stop();
    assert_eq!(
        completion.committed(),
        Some(SessionOutcome::Completed),
        "a committed EOF is not renamed into Stopped"
    );
}

// --- T12: coherent observation under a real race -------------------------------------

#[test]
fn t12_every_observation_is_a_coherent_instant() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(15), || {
        let handle = PlaybackSessionHandle::new();
        let (mut runtime, _consumed) = live_runtime(
            SourceBehavior::Paced {
                after: 8 * 1024,
                delay: Duration::from_millis(2),
            },
            OutputBehavior::Consume,
            handle.clone(),
        );

        // Observers hammer observe() across the whole window that
        // contains the stop request and the settlement itself; every
        // returned observation must be a state that really existed at
        // one instant.
        const OBSERVERS: usize = 4;
        let mut observers = Vec::new();
        for _ in 0..OBSERVERS {
            let handle = handle.clone();
            observers.push(std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                let mut seen: Option<EpisodeTerminalOutcome> = None;
                let mut stop_seen = false;
                loop {
                    let observation = handle.observe();
                    // Command state is monotone.
                    assert!(
                        !stop_seen || observation.stop_requested,
                        "stop_requested flipped back to false"
                    );
                    stop_seen = observation.stop_requested;
                    if let Some(previous) = seen {
                        assert_eq!(
                            observation.terminal_outcome,
                            Some(previous),
                            "the terminal Fact is immutable across observations"
                        );
                    }
                    if observation.terminal_outcome.is_some() && seen.is_none() {
                        seen = observation.terminal_outcome;
                    }
                    if observation.terminal_outcome == Some(EpisodeTerminalOutcome::Stopped) {
                        // The impossible combination under attack: a
                        // Stopped Fact commits only with recorded intent,
                        // and intent is monotone, so no coherent instant
                        // ever shows Stopped with stop_requested == false.
                        assert!(
                            observation.stop_requested,
                            "impossible combination: Stopped with stop_requested=false"
                        );
                    }
                    if observation.terminal_outcome.is_some() {
                        // A settled real episode always published its
                        // format and carries no activation diagnostic.
                        assert!(observation.source_format.is_some());
                        assert_eq!(observation.activation_error, None);
                    }
                    if seen.is_some() || std::time::Instant::now() >= deadline {
                        return seen;
                    }
                }
            }));
        }

        std::thread::sleep(Duration::from_millis(50));
        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let outcomes: Vec<_> = observers
            .into_iter()
            .map(|o| o.join().expect("observer exits"))
            .collect();
        for outcome in &outcomes {
            assert_eq!(
                outcome,
                &Some(EpisodeTerminalOutcome::Stopped),
                "every observer that saw a Fact saw the same committed Fact"
            );
        }
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

// --- T15: teardown leaves no decisive evidence uncommitted ---------------------------

#[test]
fn t15_teardown_leaves_no_decisive_evidence_uncommitted() {
    // Mid-play dispose, no stop, no wait: teardown must stop the legs in
    // order; the render leg's stop_and_join makes it publish its verdict,
    // and that publication settles synchronously before complete()
    // returns — so when dispose returns, the authority has settled
    // (D11/D14.3: teardown must not report quiesced while decisive
    // evidence sits uncommitted). No watcher, no consumer call.
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(10), || {
        let handle = PlaybackSessionHandle::new();
        let (mut runtime, _consumed) = live_runtime(
            SourceBehavior::Paced {
                after: 4 * 1024,
                delay: Duration::from_millis(100),
            },
            OutputBehavior::Consume,
            handle.clone(),
        );
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
        // An aborted episode with no recorded stop intent classifies as
        // a device failure under the current contract.
        let observation = handle.observe();
        assert_eq!(
            observation.terminal_outcome,
            Some(EpisodeTerminalOutcome::Failed)
        );
        assert_eq!(observation.failure_diagnostic.as_deref(), Some("device"));
    });
}

// --- no resolver thread, ever ---------------------------------------------------------

#[test]
fn no_settlement_thread_exists_while_an_episode_is_live() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(10), || {
        let handle = PlaybackSessionHandle::new();
        let (mut runtime, _consumed) = live_runtime(
            SourceBehavior::Paced {
                after: 4 * 1024,
                delay: Duration::from_millis(50),
            },
            OutputBehavior::Consume,
            handle.clone(),
        );
        // Sample across a real slice of the live episode: no settlement
        // thread may appear at any point.
        for _ in 0..25 {
            assert_no_settlement_thread();
            assert_eq!(handle.observe().terminal_outcome, None);
            std::thread::sleep(Duration::from_millis(10));
        }
        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

// --- migrated F1 stop-seam witnesses (buffered_frames is crate-private now) ---------

/// Witness the "consumer parked on an empty edge" shape: the render leg
/// has consumed at least one frame and the edge has then stayed empty
/// across a window long enough that a fast producer would have refilled
/// it.
fn wait_for_a_parked_consumer(
    handle: &PlaybackSessionHandle,
    consumed: &std::sync::atomic::AtomicUsize,
    limit: Duration,
) {
    const QUIET: Duration = Duration::from_millis(80);
    let deadline = std::time::Instant::now() + limit;
    let mut empty_since: Option<std::time::Instant> = None;
    loop {
        let produced = consumed.load(std::sync::atomic::Ordering::SeqCst) > 0;
        match (produced, handle.completion.buffered_frames()) {
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

/// Poll until `pred` holds or the deadline passes (a witness, not an
/// oracle: the stop request below does not depend on the predicate).
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

#[test]
fn stop_while_playing_resolves_stopped_and_disposes_quietly() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(10), || {
        let handle = PlaybackSessionHandle::new();
        let (mut runtime, consumed) = live_runtime(
            SourceBehavior::Paced {
                after: 100_000,
                delay: Duration::from_millis(250),
            },
            OutputBehavior::Consume,
            handle.clone(),
        );
        wait_for_a_parked_consumer(&handle, &consumed, Duration::from_secs(5));
        // The witness guarantees the empty window is fresh; the next
        // paced frame is still ~170 ms away, so the stop lands with the
        // consumer parked.
        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let snapshot = runtime.dispose();
        assert!(
            snapshot.quiet,
            "a stopped episode must dispose quietly: {snapshot:?}"
        );
        assert_no_leg_threads();
    });
}

#[test]
fn stop_wakes_a_producer_blocked_on_a_full_edge() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(15), || {
        let handle = PlaybackSessionHandle::new();
        let (mut runtime, _consumed) = live_runtime(
            SourceBehavior::EofAfter(4_000_000),
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(100),
            },
            handle.clone(),
        );
        wait_for_witness(
            || {
                handle
                    .completion
                    .buffered_frames()
                    .is_some_and(|n| n >= EDGE_CAPACITY_FRAMES)
            },
            Duration::from_secs(5),
            "edge pinned full (producer blocked mid-write)",
        );
        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

#[test]
fn stop_wakes_a_consumer_blocked_on_an_empty_edge() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    within(Duration::from_secs(15), || {
        let handle = PlaybackSessionHandle::new();
        let (mut runtime, consumed) = live_runtime(
            SourceBehavior::Paced {
                after: 100_000,
                delay: Duration::from_millis(250),
            },
            OutputBehavior::Consume,
            handle.clone(),
        );
        wait_for_a_parked_consumer(&handle, &consumed, Duration::from_secs(5));
        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
        assert_no_leg_threads();
    });
}

// --- migrated resolver-level contract tests (were tests/edge_lifecycle.rs) -----------

#[test]
fn completion_resolves_completed_only_from_eof_plus_drained() {
    let completion = SessionCompletion::new();
    // Neither leg has reported: no outcome, and the fresh core stays
    // unsettled.
    assert_eq!(completion.committed(), None);

    completion.worker_exited(EdgeTerminal::Eof);
    assert_eq!(
        completion.committed(),
        None,
        "EOF without drain is not completion"
    );

    // The drain publication commits synchronously, inside complete().
    completion.drain_signal().complete(DrainVerdict::Drained);
    assert_eq!(completion.committed(), Some(SessionOutcome::Completed));
}

#[test]
fn completion_reports_decode_failure_before_any_drain() {
    let completion = SessionCompletion::new();
    completion.decode_failed("corrupt stream");
    assert_eq!(
        completion.committed(),
        Some(SessionOutcome::Failed {
            stage: "decode: corrupt stream".to_owned()
        })
    );
}

#[test]
fn completion_reports_device_abort_as_failure() {
    let completion = SessionCompletion::new();
    completion.worker_exited(EdgeTerminal::Eof);
    completion.drain_signal().complete(DrainVerdict::Aborted);
    assert_eq!(
        completion.committed(),
        Some(SessionOutcome::Failed {
            stage: "device".to_owned()
        }),
        "an abort before drain is a device failure, never a fake completion"
    );
}

#[test]
fn wait_terminal_blocks_until_a_leg_publishes() {
    let handle = PlaybackSessionHandle::new();
    let waiter = {
        let handle = handle.clone();
        std::thread::spawn(move || handle.wait_terminal())
    };
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        !waiter.is_finished(),
        "wait_terminal blocks until the session settles"
    );
    // decode evidence settles inline on the worker call stack and the
    // commit wakes the waiter.
    handle.completion.decode_failed("test failure");
    assert_eq!(
        waiter.join().expect("waiter exits"),
        EpisodeTerminalOutcome::Failed
    );
}

// --- migrated resolver-level stop-precedence test (was tests/stop_seam.rs) ----------

/// Decode failure dominates stop intent, deterministically, whichever
/// order they arrive in (the session-level coexistence is racy; the
/// resolver contract is not).
#[test]
fn a_stop_cannot_downgrade_a_decode_failure() {
    // Stop first, failure second: the failure evidence settles inline.
    let completion = SessionCompletion::new();
    completion.request_stop();
    completion.decode_failed("test decode failure");
    assert_eq!(
        completion.committed(),
        Some(SessionOutcome::Failed {
            stage: "decode: test decode failure".to_owned()
        }),
        "a committed decode failure is not relabelled by stop intent"
    );
    assert!(
        completion.observe_snapshot().stop_requested,
        "the command was recorded; it simply did not win"
    );

    // Failure first, stop second: same answer.
    let completion = SessionCompletion::new();
    completion.decode_failed("test decode failure");
    completion.request_stop();
    assert_eq!(
        completion.committed(),
        Some(SessionOutcome::Failed {
            stage: "decode: test decode failure".to_owned()
        })
    );
}

// --- T16: activation failure withdraws a published position ------------------------

/// F4-IMPLEMENTATION-CORRECTIVE-1. Activation's last fallible step (the
/// decode-worker spawn) runs AFTER the render mechanism is open and
/// publishing, and the open-abort protocol makes the same state
/// end-to-end reachable — a leg that parks at the gate publishes from its
/// park slice on the way to a failed open. So `terminal_outcome == None`,
/// `activation_error == Some(_)` and a NON-EMPTY position cell really
/// coexist in the product, and D14.8 forbids deriving a projection from
/// them.
///
/// This pins the gate itself, including the interleaving no end-to-end
/// test can produce deterministically (a spawn failure on a live,
/// still-publishing leg) and the no-resurrection rule for a sample that
/// lands after the failure was recorded. The reachable counterpart is
/// `activation_failure_withdraws_an_already_published_position` in
/// `tests/position_seam.rs`.
#[test]
fn t16_activation_failure_withdraws_the_projection_without_touching_the_cell() {
    let completion = SessionCompletion::new();
    let handle = PlaybackSessionHandle {
        completion: completion.clone(),
    };
    let cell = completion.position_evidence();
    // The leg published before activation raised — the value the
    // reachable open-abort form really ends up holding.
    cell.publish_consumed(0, 0);
    assert_eq!(cell.published(), Some(0), "precondition: a sample exists");
    completion.set_source_duration(Duration::from_secs(4));

    completion.activation_failed("decode worker spawn failed: test");

    for _ in 0..8 {
        let observation = handle.observe();
        assert_eq!(
            observation.position, None,
            "an episode that never played has no position, however many \
             samples its dying mechanism published: {observation:?}"
        );
        assert_eq!(
            observation.terminal_outcome, None,
            "the failure stays a diagnostic (D11 activation firewall)"
        );
        assert!(observation.activation_error.is_some());
    }
    assert_eq!(
        cell.published(),
        Some(0),
        "withdrawal is the observation gate, not a cell write"
    );

    // A sample published AFTER the failure was recorded — the dying
    // leg's last iterating slice — must not resurrect the projection.
    cell.publish_consumed(4096, 0);
    let observation = handle.observe();
    assert_eq!(
        observation.position, None,
        "a late publication cannot resurrect a withdrawn projection: {observation:?}"
    );
    assert_eq!(
        observation.source_duration,
        Some(Duration::from_secs(4)),
        "duration is source-scoped evidence and is not withdrawn"
    );
    assert_eq!(
        completion.committed(),
        None,
        "an activation failure never settles anything"
    );
}
