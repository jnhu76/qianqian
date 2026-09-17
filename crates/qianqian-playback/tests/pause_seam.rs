//! F3 pause/resume-seam tests (ADR-PBK-002 D14.7): the application-facing
//! pause/resume commands drive the REAL Playback Session through the
//! mechanism-A render-loop gate, on mechanism doubles at the ports seams.
//! Platform-independent: these run wherever the workspace tests run; the
//! physical audibility/latency claims live in the F3-GATE evidence
//! (`experiments/f3-pause-mechanism`), not here.
//!
//! Truth classes under test: pause/resume are commands (inert after
//! settlement); engagement and output-tail quiescence are mechanism
//! evidence belonging to the CURRENT engagement only; Paused/Resumed are
//! the derived projections of the frozen establishment conjunction; D11
//! terminal outcomes are untouched by any of it.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use qianqian_app::QianqianApp;
use qianqian_composition::{DesiredEntry, Revision};
use qianqian_playback::{
    EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionHandle, playback_session_spec,
};

use common::{OutputBehavior, SourceBehavior, TailProbe, TestDecode, TestOutput, within};

const DUMMY_PATH: &str = "test://pause-seam";

fn desired(id: &str, component: &'static str) -> DesiredEntry {
    DesiredEntry::enabled(id, component, Revision::new(1))
}

/// A decode source producing ~`seconds` of frames at the test format
/// before clean EOF.
fn seconds_of_audio(seconds: u64) -> SourceBehavior {
    SourceBehavior::EofAfter(44_100 * seconds as usize)
}

/// A decode source producing ~`seconds` of frames fast, then one frame
/// per 50ms forever — never EOF. The multi-cycle pause tests observe
/// the Resumed projection, which the unsettled guard truthfully
/// retracts the moment the episode settles; a finite source can drain
/// to EOF during a test's release window and end the episode before
/// the projection is polled, so these tests keep the episode alive.
fn endless_audio(seconds: u64) -> SourceBehavior {
    SourceBehavior::Paced {
        after: 44_100 * seconds as usize,
        delay: Duration::from_millis(50),
    }
}

/// Register the standard F3 episode: a decode source, an output double
/// sharing the caller's consumption counter, mock device-tail flag and
/// tail probe, and the real session over `handle`.
fn registered_runtime(
    source: SourceBehavior,
    output: OutputBehavior,
    consumed: Arc<AtomicUsize>,
    device_tail_padding: Arc<AtomicBool>,
    tail_probe: TailProbe,
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
            qianqian_composition::ComponentSpec::new("test_output_plugin")
                .provides::<qianqian_audio_api::ports::AudioOutputCapability>()
                .on_activate(move |ctx| {
                    let mut service = TestOutput::observed_with_tail(
                        output,
                        consumed.clone(),
                        device_tail_padding.clone(),
                    );
                    service.tail_probe = tail_probe.clone();
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

/// Bounded poll for an asynchronously-published observation.
fn wait_until(limit: Duration, mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if predicate() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The full establishment wait: intent is not enough — the projection
/// may only go true through engagement AND the current engagement's
/// tail quiescence (the mock device tail is empty, so quiescence
/// follows the first bounded park slice).
fn wait_established(handle: &PlaybackSessionHandle) {
    assert!(
        wait_until(Duration::from_secs(5), || handle.observe().paused()),
        "the Paused projection never established: {:?}",
        handle.observe()
    );
}

/// The D14.7-corrective negative oracle in isolation: with the mock
/// device still holding queued frames, an engagement must NOT
/// establish Paused.
fn assert_engaged_but_not_paused(handle: &PlaybackSessionHandle) {
    assert!(
        wait_until(Duration::from_secs(5), || handle.observe().pause_engagement
            == PauseEngagement::Engaged),
        "the render leg never engaged: {:?}",
        handle.observe()
    );
    let snapshot = handle.observe();
    assert!(
        snapshot.pause_requested && snapshot.pause_engagement == PauseEngagement::Engaged,
        "expected engaged pause intent: {snapshot:?}"
    );
    assert!(
        !snapshot.paused(),
        "stale/absent tail quiescence must not establish Paused: {snapshot:?}"
    );
}

/// A pause that establishes, then a resume that releases it, repeated
/// cycles staying truthful — including the D14.7-corrective negative
/// oracle: a NEW pause while the mock device still plays its tail must
/// NOT project Paused, and only the NEW engagement's quiescence may.
#[test]
fn pause_establishes_resume_releases_and_cycles_stay_truthful() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail_padding = Arc::new(AtomicBool::new(false));
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            endless_audio(30),
            OutputBehavior::Consume,
            consumed.clone(),
            device_tail_padding.clone(),
            TailProbe::default(),
            handle.clone(),
        );
        activate(&mut runtime);

        // Playing before anything is paused: consumption progresses.
        assert!(
            wait_until(Duration::from_secs(5), || consumed.load(Ordering::SeqCst)
                > 0),
            "the episode never produced audio"
        );
        let observation = handle.observe();
        assert!(!observation.paused(), "fresh episode: {observation:?}");
        assert!(
            !observation.resumed(),
            "nothing was ever paused: {observation:?}"
        );
        assert!(
            observation.pause_engagement == PauseEngagement::Disengaged,
            "no engagement yet: {observation:?}"
        );

        // Cycle 1: pause -> full establishment -> resume -> released.
        handle.request_pause();
        assert!(handle.observe().pause_requested, "intent is recorded");
        wait_established(&handle);
        handle.request_resume();
        assert!(
            wait_until(Duration::from_secs(5), || {
                let observation = handle.observe();
                !observation.pause_requested && observation.resumed()
            }),
            "resume never released the projection: {:?}",
            handle.observe()
        );

        // Cycle 2 — the stale-quiescence negative oracle: the mock
        // device still holds queued frames while the leg re-engages.
        device_tail_padding.store(true, Ordering::SeqCst);
        handle.request_pause();
        assert_engaged_but_not_paused(&handle);

        // Only the CURRENT engagement's tail quiescence establishes.
        device_tail_padding.store(false, Ordering::SeqCst);
        wait_established(&handle);

        handle.request_resume();
        assert!(
            wait_until(Duration::from_secs(5), || handle.observe().resumed()),
            "second resume never released: {:?}",
            handle.observe()
        );

        // Commands stay commands: stop settles the episode normally.
        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// The Resumed projection is CURRENT-CYCLE evidence, symmetric to the
/// stale-quiescence oracle above: after a first full pause/resume cycle,
/// a second cycle's resume must NOT project Resumed from the FIRST
/// cycle's disengagement latch — only this cycle's own Disengaged may
/// establish it. The render leg is held inside a slow (legal) tail
/// observation — an engagement whose quiescence has not been observed
/// yet, because the gate stops calling the observation once quiescence
/// is published — so the window between the resume command and this
/// cycle's own disengagement is a stable state, not a race. In that
/// window the render mechanism is provably still parked: Resumed
/// (whose claim is "render submission is re-enabled") MUST be false.
#[test]
fn resumed_requires_the_current_pause_cycles_disengagement() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail_padding = Arc::new(AtomicBool::new(false));
        let tail_probe = TailProbe::default();
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            endless_audio(30),
            OutputBehavior::Consume,
            consumed.clone(),
            device_tail_padding.clone(),
            tail_probe.clone(),
            handle.clone(),
        );
        activate(&mut runtime);

        assert!(
            wait_until(Duration::from_secs(5), || consumed.load(Ordering::SeqCst)
                > 0),
            "the episode never produced audio"
        );

        // Cycle 1 completes in full: pause -> established -> resume ->
        // this cycle's own disengagement evidence.
        handle.request_pause();
        wait_established(&handle);
        handle.request_resume();
        assert!(
            wait_until(Duration::from_secs(5), || handle.observe().resumed()),
            "cycle 1 never released: {:?}",
            handle.observe()
        );

        // Cycle 2: re-pause and hold the leg inside its (first, legal
        // but slow) tail observation, so it cannot reach its released
        // check and cannot publish this cycle's disengagement.
        tail_probe.arm();
        handle.request_pause();
        assert!(
            wait_until(Duration::from_secs(5), || handle.observe().pause_engagement
                == PauseEngagement::Engaged),
            "the render leg never re-engaged: {:?}",
            handle.observe()
        );
        tail_probe.wait_held();

        handle.request_resume();
        // The resume command is routed, but this cycle's disengagement
        // has NOT been observed — the leg is provably still held at the
        // gate. Cycle 1's latch must not establish Resumed here.
        let snapshot = handle.observe();
        assert!(
            !snapshot.resumed(),
            "a previous cycle's disengagement established Resumed: \
             {snapshot:?}"
        );

        // This cycle's own Disengaged does establish it.
        tail_probe.unhold();
        assert!(
            wait_until(Duration::from_secs(5), || handle.observe().resumed()),
            "the current cycle's disengagement never established Resumed: \
             {:?}",
            handle.observe()
        );

        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// Stop from an established pause mid-play: the gate release lets the
/// leg reach the stopped edge and the episode settles the ordinary
/// `Stopped` fact (D14.7 terminal interactions; D11 monotonicity —
/// late pause/resume history never relabels it).
#[test]
fn stop_from_an_established_pause_settles_stopped_and_late_commands_stay_inert() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail_padding = Arc::new(AtomicBool::new(false));
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            seconds_of_audio(30),
            OutputBehavior::Consume,
            consumed,
            device_tail_padding,
            TailProbe::default(),
            handle.clone(),
        );
        activate(&mut runtime);

        handle.request_pause();
        wait_established(&handle);

        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let observation = handle.observe();
        assert_eq!(
            observation.terminal_outcome,
            Some(EpisodeTerminalOutcome::Stopped)
        );
        // The settled episode is never Paused/Resumed, whatever the
        // mechanism evidence still latches (D14.7 unsettled guard).
        assert!(!observation.paused());
        assert!(!observation.resumed());
        assert!(
            observation.pause_requested,
            "pause intent stays recorded history"
        );

        // Late pause/resume after settlement: inert command history.
        handle.request_pause();
        handle.request_resume();
        handle.request_pause();
        let observation = handle.observe();
        assert_eq!(
            observation.terminal_outcome,
            Some(EpisodeTerminalOutcome::Stopped),
            "D11 monotonicity: settlement is never relabelled"
        );
        assert!(!observation.paused());
        assert!(!observation.resumed());

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// Pause intent routed before activation: the render leg's FIRST loop
/// iteration parks at the gate (start-paused), and EOF while parked
/// leaves the episode unsettled until it is released and drained.
#[test]
fn eof_while_parked_leaves_the_episode_unsettled_until_resumed() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail_padding = Arc::new(AtomicBool::new(false));
        let handle = PlaybackSessionHandle::new();

        // Record pause intent BEFORE the episode exists: it is applied
        // the moment the render leg reaches its gate.
        handle.request_pause();
        assert!(handle.observe().pause_requested);

        // A tiny source: the worker produces, EOFs and exits while the
        // render leg stays parked at the gate.
        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(64),
            OutputBehavior::Consume,
            consumed.clone(),
            device_tail_padding,
            TailProbe::default(),
            handle.clone(),
        );
        activate(&mut runtime);

        // Start-paused: the parked leg establishes the projection
        // without ever having submitted a frame.
        wait_established(&handle);
        assert_eq!(
            consumed.load(Ordering::SeqCst),
            0,
            "a parked leg must not consume"
        );

        // EOF while parked: the worker has long exited (tiny source),
        // yet no terminal Fact may commit while the leg is parked.
        assert!(
            !wait_until(Duration::from_millis(500), || handle
                .observe()
                .terminal_outcome
                .is_some()),
            "the episode settled while parked at the gate (D14.7: EOF \
             while parked stays unsettled until released)"
        );

        // Resume-and-drain completes it.
        handle.request_resume();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// Stop from a parked post-EOF episode: the gate must NOT force-abort —
/// the tail plays out and drains, settling the SAME Completed fact as
/// today's stop-after-EOF (no Failed{device} fabrication).
#[test]
fn stop_from_parked_after_eof_still_completes() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail_padding = Arc::new(AtomicBool::new(false));
        let handle = PlaybackSessionHandle::new();
        handle.request_pause();

        let mut runtime = registered_runtime(
            SourceBehavior::EofAfter(64),
            OutputBehavior::Consume,
            consumed,
            device_tail_padding,
            TailProbe::default(),
            handle.clone(),
        );
        activate(&mut runtime);
        wait_established(&handle);

        handle.request_stop();
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "stop-from-paused-after-EOF must play out and drain, not fail"
        );
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// Failure while parked settles immediately by the existing precedence,
/// and the teardown releases the parked gate (dispose must terminate).
#[test]
fn failure_while_parked_settles_failed_without_wedging_teardown() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail_padding = Arc::new(AtomicBool::new(false));
        let handle = PlaybackSessionHandle::new();
        handle.request_pause();

        let mut runtime = registered_runtime(
            SourceBehavior::FailAfter(64),
            OutputBehavior::Consume,
            consumed,
            device_tail_padding,
            TailProbe::default(),
            handle.clone(),
        );
        activate(&mut runtime);

        // The leg parks (never established against a failed decode —
        // and once settled, never at all).
        assert!(
            wait_until(Duration::from_secs(5), || handle.observe().terminal_outcome
                == Some(EpisodeTerminalOutcome::Failed)),
            "failure while parked never settled: {:?}",
            handle.observe()
        );
        assert!(
            !handle.observe().paused(),
            "a settled episode is never Paused"
        );

        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Failed);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
        assert_no_leg_threads();
    });
}

/// Stop while the producer is wedged through pause backpressure: the
/// edge is full and the decode worker is blocked mid-write; the stop
/// wakes BOTH participants (worker via the data-plane stop, render leg
/// via the gate release) and no thread leaks.
#[test]
fn stop_wakes_a_producer_blocked_through_pause_backpressure() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail_padding = Arc::new(AtomicBool::new(false));
        let handle = PlaybackSessionHandle::new();

        // The consumer is slower than the producer: the bounded edge
        // fills long before EOF and the worker blocks mid-write.
        let mut runtime = registered_runtime(
            seconds_of_audio(30),
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(50),
            },
            consumed.clone(),
            device_tail_padding,
            TailProbe::default(),
            handle.clone(),
        );
        activate(&mut runtime);

        // Playback progressing, then pause: consumption halts entirely
        // (the parked leg submits nothing), the edge fills, and the
        // producer blocks behind it.
        assert!(
            wait_until(Duration::from_secs(5), || consumed.load(Ordering::SeqCst)
                > 0),
            "the episode never produced audio"
        );
        handle.request_pause();
        wait_established(&handle);
        let at_pause = consumed.load(Ordering::SeqCst);
        assert!(
            !wait_until(Duration::from_millis(400), || consumed
                .load(Ordering::SeqCst)
                != at_pause),
            "a parked leg must not consume (pause backpressure)"
        );

        // Stop through the backpressure: both legs must wake.
        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
        assert_no_leg_threads();
    });
}

/// A pause command that linearizes AFTER stop intent — the deterministic
/// shape of the stop→settlement window race — is recorded as inert
/// command history but must NOT re-park the released episode: the gate
/// stays released, the leg reaches the stopped edge, and the episode
/// settles `Stopped` with bounded latency (D14.7: stop wakes every
/// parked participant; the stop-from-paused→Stopped interaction).
#[test]
fn pause_routed_after_stop_cannot_repark_the_released_episode() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail_padding = Arc::new(AtomicBool::new(false));
        let handle = PlaybackSessionHandle::new();

        // Stop first, THEN pause: both before activation. Without
        // linearized routing the second command would park the gate the
        // stop just released, and this episode would never settle.
        handle.request_stop();
        handle.request_pause();
        let observation = handle.observe();
        assert!(observation.stop_requested, "stop intent recorded");
        assert!(
            observation.pause_requested,
            "pause stays recorded command history"
        );

        let mut runtime = registered_runtime(
            seconds_of_audio(30),
            OutputBehavior::Consume,
            consumed,
            device_tail_padding,
            TailProbe::default(),
            handle.clone(),
        );
        activate(&mut runtime);

        // Bounded settlement: the leg must reach the stopped edge, not
        // sit parked against released intent.
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        assert!(
            !handle.observe().paused(),
            "a settled episode is never Paused"
        );
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
        assert_no_leg_threads();
    });
}

/// A pause that linearizes AFTER the teardown release must not re-park
/// the leg: the stream teardown releases the gate and then joins it, so
/// a pause landing between the two would wedge `stop_and_join` forever
/// (the teardown-side twin of the stop-linearization oracle). The leg
/// is held inside a slow (legal) tail observation, so it cannot observe
/// the release until the hostile pause has routed and the probe lets
/// go — the ordering is decided by the routing linearization, not by a
/// race. If extreme scheduling ever delayed dispose past the hostile
/// pause, the test would degrade to the safe interleaving (still
/// passing, no longer exercising the suppressor); the mutation check
/// is what proves the oracle's bite.
#[test]
fn pause_routed_after_teardown_release_cannot_wedge_the_join() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail_padding = Arc::new(AtomicBool::new(false));
        let tail_probe = TailProbe::default();
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            endless_audio(30),
            OutputBehavior::Consume,
            consumed.clone(),
            device_tail_padding.clone(),
            tail_probe.clone(),
            handle.clone(),
        );
        activate(&mut runtime);

        assert!(
            wait_until(Duration::from_secs(5), || consumed.load(Ordering::SeqCst)
                > 0),
            "the episode never produced audio"
        );

        // Park the leg and hold it inside its first (slow, legal) tail
        // observation: from here it cannot observe any release until the
        // probe lets go.
        tail_probe.arm();
        handle.request_pause();
        assert!(
            wait_until(Duration::from_secs(5), || handle.observe().pause_engagement
                == PauseEngagement::Engaged),
            "the render leg never engaged: {:?}",
            handle.observe()
        );
        tail_probe.wait_held();

        // Fire the hostile pause from another thread well after dispose
        // began (dispose is already inside the teardown join by then:
        // the release runs within microseconds of dispose on this
        // thread, five orders of magnitude before the pause fires), and
        // only then let the leg re-check. With linearized routing the
        // post-release pause is inert and the leg exits; without it, it
        // re-parks and the join below never returns.
        let hostile = {
            let handle = handle.clone();
            let tail_probe = tail_probe.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                handle.request_pause();
                tail_probe.unhold();
            })
        };
        let snapshot = runtime.dispose();
        hostile.join().expect("the hostile pauser joins");
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
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
