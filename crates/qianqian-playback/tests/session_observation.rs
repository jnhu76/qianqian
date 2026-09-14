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
    assert!(obs.activation_error.is_none(), "no activation diagnostic yet");
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
