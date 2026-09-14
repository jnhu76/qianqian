//! F2 truthful read-side tests: [`SessionCompletion::observation`] is
//! the session's application-facing read-side projection — the seam a
//! headless `status` (and a future UI) consumes. These tests pin what a
//! consumer may truthfully learn and what it must not:
//!
//! - the observation is one coherent, read-only copy of committed
//!   session truth (single lock, no interleaved writes visible);
//! - it never resolves or commits: pending means only "no terminal
//!   outcome committed yet", never Playing/Starting;
//! - mechanism evidence, activation diagnostics and command state stay
//!   labeled for what they are — none of them is an outcome.

mod common;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use qianqian_app::QianqianApp;
use qianqian_composition::{DesiredEntry, Revision};
use qianqian_playback::{SessionCompletion, SessionOutcome, playback_session_spec};

use common::{OutputBehavior, SourceBehavior};

const DUMMY_PATH: &str = "test://observation";

fn desired(id: &str, component: &'static str) -> DesiredEntry {
    DesiredEntry::enabled(id, component, Revision::new(1))
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
                    let service = common::TestDecode { behavior };
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
                    let service = common::TestOutput::new(behavior);
                    ctx.provide::<qianqian_audio_api::ports::AudioOutputCapability>(
                        std::rc::Rc::new(service),
                    )
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("output provider registers");

    runtime
        .register_component(playback_session_spec(PathBuf::from(DUMMY_PATH), completion))
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

/// A session that has committed nothing observes pending with no
/// evidence attached (T1 seed: fresh session, no episode truth).
#[test]
fn a_fresh_session_observes_pending_with_no_evidence() {
    let completion = SessionCompletion::new();
    let obs = completion.observation();
    assert!(obs.outcome.is_none(), "no terminal outcome committed yet");
    assert!(obs.source_format.is_none(), "no activation readback yet");
    assert!(
        obs.activation_error.is_none(),
        "no activation diagnostic yet"
    );
    assert!(!obs.stop_requested, "no stop intent recorded yet");
}

/// The observation is a pure read: even with evidence published that
/// would let the resolver decide, reading never commits the outcome
/// (authority separation, T8 side-effect freedom).
#[test]
fn observation_is_a_pure_read_it_never_commits_a_decidable_outcome() {
    let completion = SessionCompletion::new();
    completion.decode_failed("test decode failure");

    let obs = completion.observation();
    assert!(
        obs.outcome.is_none(),
        "observation must not resolve: the authority has not committed"
    );

    // The authority query still decides afterwards, and only then does
    // the same read-side report the committed outcome.
    let resolved = completion
        .try_resolve_now()
        .expect("published decode failure is decidable by the authority");
    assert_eq!(
        resolved,
        SessionOutcome::Failed {
            stage: "decode: test decode failure".to_owned()
        }
    );
    assert_eq!(completion.observation().outcome, Some(resolved));
}

/// A running episode with no committed terminal observes pending plus
/// the activation-published source format (T1 + T2). Pending is not
/// Playing: the observation only says "nothing committed yet"; after a
/// stop-driven resolution the same read-side reports the committed
/// Stopped outcome.
#[test]
fn a_running_episode_observes_pending_then_the_committed_stopped_outcome() {
    let _lifecycle = common::lifecycle_lock();
    let completion = SessionCompletion::new();
    let mut runtime = registered_runtime(
        // A paced tail keeps the episode genuinely mid-flight until the
        // stop lands, so the pending observation below is taken during
        // a live episode, not after it silently finished.
        SourceBehavior::Paced {
            after: 4,
            delay: Duration::from_millis(10),
        },
        OutputBehavior::Consume,
        completion.clone(),
    );
    activate(&mut runtime);

    let obs = completion.observation();
    assert!(
        obs.outcome.is_none(),
        "episode still running: nothing committed"
    );
    assert_eq!(
        obs.source_format,
        Some(common::TEST_FORMAT),
        "activation published the endpoint's PCM format"
    );
    assert!(obs.activation_error.is_none());
    assert!(!obs.stop_requested);

    completion.request_stop();
    assert!(
        completion.observation().stop_requested,
        "recorded stop intent is visible as command state"
    );
    let outcome = completion.wait();
    assert_eq!(outcome, SessionOutcome::Stopped);
    assert_eq!(
        completion.observation().outcome,
        Some(SessionOutcome::Stopped),
    );

    let snapshot = runtime.dispose();
    assert!(snapshot.quiet, "clean disposal after the stopped episode");
}

/// T4: after the authority commits Completed, the observation reports
/// exactly that — and a late stop cannot relabel it (the stop intent
/// stays visible as command state, never as a new outcome).
#[test]
fn observation_reports_committed_completed_and_a_late_stop_cannot_relabel_it() {
    let _lifecycle = common::lifecycle_lock();
    common::within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::Consume,
            completion.clone(),
        );
        activate(&mut runtime);

        assert_eq!(completion.wait(), SessionOutcome::Completed);
        assert_eq!(
            completion.observation().outcome,
            Some(SessionOutcome::Completed)
        );

        completion.request_stop();
        let obs = completion.observation();
        assert_eq!(
            obs.outcome,
            Some(SessionOutcome::Completed),
            "a committed terminal outcome is immutable"
        );
        assert!(obs.stop_requested, "late stop intent is command state only");

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
    });
}

/// T6: a decode-failure episode commits Failed and the observation
/// carries the class diagnostic. The format readback remains mechanism
/// evidence: the endpoint opened before the decode failed.
#[test]
fn observation_reports_the_committed_failure_with_its_class_diagnostic() {
    let _lifecycle = common::lifecycle_lock();
    common::within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::FailAfter(2),
            OutputBehavior::Consume,
            completion.clone(),
        );
        activate(&mut runtime);

        let expected = SessionOutcome::Failed {
            stage: "decode: test decode failure".to_owned(),
        };
        assert_eq!(completion.wait(), expected);
        let obs = completion.observation();
        assert_eq!(obs.outcome, Some(expected));
        assert!(
            obs.source_format.is_some(),
            "the endpoint opened: its format readback remains mechanism evidence"
        );

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
    });
}

/// T7: an activation failure fabricates nothing. No episode terminal
/// outcome exists (pending stays pending); the activation diagnostic is
/// exposed as exactly that, and the format readback that was published
/// before the render leg raised remains write-once mechanism evidence —
/// not an activation-success proof.
#[test]
fn an_activation_failure_stays_pending_and_reports_only_the_activation_diagnostic() {
    let _lifecycle = common::lifecycle_lock();
    common::within(Duration::from_secs(10), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(48_000),
            OutputBehavior::FailOpen,
            completion.clone(),
        );
        activate(&mut runtime);

        let obs = completion.observation();
        assert_eq!(
            obs.outcome, None,
            "activation failed before an episode existed: no terminal outcome is fabricated"
        );
        assert_eq!(
            obs.activation_error.as_deref(),
            Some("render stream open failed: test device open failure"),
            "the activation attempt's diagnostic is visible as exactly that"
        );
        assert!(
            obs.source_format.is_some(),
            "write-once mechanism evidence may exist even when a later activation step failed"
        );
        assert!(!obs.stop_requested);

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);

        #[cfg(target_os = "linux")]
        {
            assert!(
                common::named_thread_gone_within("qianqian-decode", Duration::from_secs(2)),
                "decode worker thread leaked"
            );
            assert!(
                common::named_thread_gone_within("qianqian-test-render", Duration::from_secs(2)),
                "render thread leaked"
            );
        }
    });
}

/// T8: reading is idempotent and side-effect free — repeated reads
/// during a live episode never resolve it, and repeated reads after the
/// terminal are stable.
#[test]
fn repeated_observations_are_side_effect_free_and_stable() {
    let _lifecycle = common::lifecycle_lock();
    common::within(Duration::from_secs(15), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::Paced {
                after: 4,
                delay: Duration::from_millis(10),
            },
            OutputBehavior::Consume,
            completion.clone(),
        );
        activate(&mut runtime);

        for _ in 0..64 {
            let obs = completion.observation();
            assert_eq!(
                obs.outcome, None,
                "reads do not resolve the episode (pure read)"
            );
        }

        completion.request_stop();
        assert_eq!(completion.wait(), SessionOutcome::Stopped);
        let terminal = completion.observation();
        for _ in 0..64 {
            assert_eq!(
                completion.observation(),
                terminal,
                "post-terminal reads are stable and side-effect free"
            );
        }

        let snapshot = runtime.dispose();
        assert!(
            snapshot.quiet,
            "the episode still disposed cleanly after 128+ reads"
        );
    });
}

/// T9: status × stop × resolution race — a reader hammering the
/// observation seam while the episode is stopped and resolved never
/// deadlocks, never panics, and never observes a withdrawn or
/// contradictory outcome: reads are pending until the commit, then
/// monotonically the committed Stopped.
#[test]
fn observation_racing_stop_and_resolution_never_deadlocks_or_lies() {
    let _lifecycle = common::lifecycle_lock();
    common::within(Duration::from_secs(15), move || {
        let completion = SessionCompletion::new();
        let mut runtime = registered_runtime(
            SourceBehavior::Paced {
                after: 4,
                delay: Duration::from_millis(5),
            },
            OutputBehavior::Consume,
            completion.clone(),
        );
        activate(&mut runtime);

        let reader_done = Arc::new(AtomicBool::new(false));
        let transitions = Arc::new(Mutex::new(Vec::<Option<SessionOutcome>>::new()));
        let reader = {
            let completion = completion.clone();
            let reader_done = reader_done.clone();
            let transitions = transitions.clone();
            std::thread::Builder::new()
                .name("qianqian-status-reader".into())
                .spawn(move || {
                    let mut last: Option<Option<SessionOutcome>> = None;
                    loop {
                        let current = completion.observation().outcome;
                        let committed = current.is_some();
                        if Some(&current) != last.as_ref() {
                            transitions
                                .lock()
                                .expect("transitions lock")
                                .push(current.clone());
                            last = Some(current);
                        }
                        // Exit only once the main side is done AND the
                        // committed terminal has been witnessed, so the
                        // witness assertion below holds under every
                        // schedule.
                        if reader_done.load(Ordering::SeqCst) && committed {
                            break;
                        }
                        std::thread::yield_now();
                    }
                })
                .expect("status reader spawns")
        };

        completion.request_stop();
        assert_eq!(completion.wait(), SessionOutcome::Stopped);

        // Keep reading across the terminal boundary: a late status must
        // be as safe as an early one.
        for _ in 0..64 {
            let _ = completion.observation();
        }
        reader_done.store(true, Ordering::SeqCst);
        reader
            .join()
            .expect("status reader joins cleanly (no deadlock, no panic)");

        let history = transitions.lock().expect("transitions lock").clone();
        // Invariants that hold under EVERY schedule (the reader may lose
        // the race entirely and only see the committed terminal — that is
        // truthful too):
        // - every observed value is pending or the committed Stopped;
        // - the commit is monotone: no pending after a committed read;
        // - the reader (which runs until after the terminal) witnesses
        //   the committed outcome.
        let first_commit = history.iter().position(|o| o.is_some());
        if let Some(idx) = first_commit {
            assert!(
                history[..idx].iter().all(|o| o.is_none()),
                "nothing but pending may precede the commit: {history:?}"
            );
            assert!(
                history[idx..].iter().all(|o| o.is_some()),
                "a committed outcome is never withdrawn: {history:?}"
            );
        }
        assert!(
            history.iter().any(|o| o.is_some()),
            "the reader witnessed the committed terminal"
        );
        assert!(
            history
                .iter()
                .all(|o| o.is_none() || *o == Some(SessionOutcome::Stopped)),
            "no contradictory outcome was ever observed: {history:?}"
        );

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet);
    });
}
