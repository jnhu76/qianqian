//! F4 position/duration seam tests (ADR-PBK-002 D14.8): the REAL
//! Playback Session over mechanism doubles, asserting what an
//! application can truthfully read about where playback is.
//!
//! Truth classes under test: `position` is the episode's Projection —
//! one pure load of the cell the render leg publishes into, absent
//! (never zero) before the first publication and after the episode
//! stops being a live one (a terminal Fact, or an activation failure —
//! the mechanism publishes before activation's last fallible step);
//! `source_duration` is optional source-scoped Mechanism Evidence that
//! is NOT exact and NOT withdrawn by settlement; neither is a Fact, and
//! neither feeds settlement or control.
//!
//! What the mechanism doubles cannot cover is covered elsewhere, and what
//! they structurally cannot see is stated rather than implied:
//!
//! ```text
//! covered elsewhere
//!   algebra      qianqian-audio-api's `position_evidence` oracles
//!   real leg     the Windows render loop's call order, by that crate's
//!                source-order oracle (`render_order_oracle.rs`): publish
//!                from the pre-submission total, credit only after a
//!                successful submission, publish in the park slices and
//!                on the drain path
//!
//! blind spots of ANY double-driven test here
//!   failed submission   the mock has no failing ReleaseBuffer, so "a
//!                       failed submission is never counted" rests on the
//!                       order oracle (P3) and on reading the real loop,
//!                       not on this file
//!   frame units         the mock is unit-agnostic: it cannot witness
//!                       that GetCurrentPadding and the submitted counts
//!                       are the same frame unit. That is a property of
//!                       the real negotiation path (the stream is
//!                       initialized at the source format, no
//!                       conversion), pinned there, not here
//!   purity proof        the purity test below is behavioural; the
//!                       structural fact that makes it airtight is that
//!                       `published()` is a plain load and
//!                       `observe_snapshot` writes nothing, so no
//!                       reader-side state can exist at all
//! ```
//!
//! This file adds the end-to-end behavior: the same protocol driven
//! through a real session, a real gate, a real completion and the real
//! observation seam.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use qianqian_app::QianqianApp;
use qianqian_composition::{DesiredEntry, Revision};
use qianqian_playback::{
    EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionHandle, PlaybackSessionObservation,
    playback_session_spec,
};

use common::{
    DeviceTail, OutputBehavior, Playout, SourceBehavior, TailProbe, TestDecode, TestOutput, within,
};

const DUMMY_PATH: &str = "test://position-seam";

/// Frames the mock render leg reads per iteration (mirrors the real
/// loop's staging block, and therefore the size of one submission).
const BLOCK: u64 = 256;
/// A source long enough that no test reaches EOF by accident.
const LONG_SOURCE: usize = 44_100 * 30;

fn desired(id: &str, component: &'static str) -> DesiredEntry {
    DesiredEntry::enabled(id, component, Revision::new(1))
}

/// Register the standard F4 episode: a decode double (whose probe may
/// report a duration), an output double sharing the caller's
/// consumption witness and device tail, and the real session over
/// `handle`.
fn registered_runtime(
    decode: TestDecode,
    output: OutputBehavior,
    consumed: Arc<AtomicUsize>,
    device_tail: DeviceTail,
    handle: PlaybackSessionHandle,
) -> QianqianApp {
    let mut runtime = QianqianApp::new();

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

    runtime
        .register_component({
            qianqian_composition::ComponentSpec::new("test_output_plugin")
                .provides::<qianqian_audio_api::ports::AudioOutputCapability>()
                .on_activate(move |ctx| {
                    let mut service = TestOutput::observed_with_tail(
                        output,
                        consumed.clone(),
                        device_tail.clone(),
                    );
                    service.tail_probe = TailProbe::default();
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

/// The open-abort episode (`OutputBehavior::OpenTimeoutAbort`). Same
/// episode as `registered_runtime`, except that the output double must
/// own the caller's tail probe and engagement witness — they ARE the park
/// proof and the pre-abort publication witness — so it is built with the
/// open-abort constructor rather than the plain one.
fn open_abort_runtime(
    decode: TestDecode,
    consumed: Arc<AtomicUsize>,
    device_tail: DeviceTail,
    tail_probe: TailProbe,
    open_abort_engaged: Arc<AtomicBool>,
    handle: PlaybackSessionHandle,
) -> QianqianApp {
    let mut runtime = QianqianApp::new();

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

    runtime
        .register_component({
            qianqian_composition::ComponentSpec::new("test_output_plugin")
                .provides::<qianqian_audio_api::ports::AudioOutputCapability>()
                .on_activate(move |ctx| {
                    let service = TestOutput::open_timeout_abort(
                        consumed.clone(),
                        device_tail.clone(),
                        tail_probe.clone(),
                        open_abort_engaged.clone(),
                    );
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

/// A running episode: the first frames are submitted, so the projection
/// exists. Returns the observation at that moment.
fn wait_for_a_position(handle: &PlaybackSessionHandle) -> PlaybackSessionObservation {
    assert!(
        wait_until(Duration::from_secs(5), || handle
            .observe()
            .position
            .is_some()),
        "no position sample was ever published: {:?}",
        handle.observe()
    );
    handle.observe()
}

// --- unknown is never zero ----------------------------------------------

/// Before the render mechanism publishes anything there is no position —
/// and a never-started episode never gets one, however long it is
/// observed. Position zero must never be forged for it.
#[test]
fn a_never_activated_episode_never_fabricates_a_position() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            // The decode probe DOES report a duration: it is established
            // before the render open fails, so it stays observable while
            // the position never exists (D14.8 lifetime rule).
            TestDecode::with_duration(SourceBehavior::EofAfter(64), Duration::from_secs(4)),
            OutputBehavior::FailOpen,
            consumed.clone(),
            DeviceTail::default(),
            handle.clone(),
        );
        activate(&mut runtime);

        assert!(
            handle.observe().activation_error.is_some(),
            "precondition: activation failed"
        );
        for _ in 0..8 {
            let observation = handle.observe();
            assert_eq!(
                observation.position, None,
                "no position may be fabricated for an episode that never started"
            );
            assert_eq!(
                observation.terminal_outcome, None,
                "and none of it may forge a terminal Fact (D11 firewall)"
            );
        }
        assert_eq!(
            handle.observe().source_duration,
            Some(Duration::from_secs(4)),
            "the duration evidence was established before the failure and stays"
        );
        assert_eq!(consumed.load(Ordering::SeqCst), 0, "nothing ever played");

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// The never-activated rule has a second, reachable face — the one
/// F4-IMPLEMENTATION-CORRECTIVE-1 was raised on. Activation's last
/// fallible step is the decode-worker spawn, and it runs AFTER the render
/// mechanism is already open and publishing; the open-abort protocol is
/// the same class made deterministic: its leg parks at the gate on the
/// way to a failed open, and the park slice publishes from the padding
/// reading it takes there. So a cell that holds a sample coexists with an
/// episode that never played, and the projection must withdraw it.
///
/// The witness chain, in order: the mock's park slice publishes and THEN
/// calls the armed tail observation that raises the hold (so the hold
/// proves the publication ran), and `open_abort_engaged` is set only
/// after that hold was observed — hence the flag is the test's evidence
/// that the cell was non-empty before the abort. Reverting the gate makes
/// this test RED, which is what keeps the chain non-vacuous.
#[test]
fn activation_failure_withdraws_an_already_published_position() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail = DeviceTail::default();
        let tail_probe = TailProbe::default();
        tail_probe.arm();
        let open_abort_engaged = Arc::new(AtomicBool::new(false));
        let handle = PlaybackSessionHandle::new();
        // Pre-activation pause intent: it is what makes the aborted leg
        // park (and therefore publish) before its open fails.
        handle.request_pause();

        let mut runtime = open_abort_runtime(
            // The decode probe reports a duration before the render open
            // fails, so the duration is established while the position
            // never becomes observable.
            TestDecode::with_duration(SourceBehavior::EofAfter(64), Duration::from_secs(4)),
            consumed.clone(),
            device_tail.clone(),
            tail_probe.clone(),
            open_abort_engaged.clone(),
            handle.clone(),
        );
        activate(&mut runtime);

        assert!(
            open_abort_engaged.load(Ordering::SeqCst),
            "precondition: the leg never parked before the abort, so no \
             publication can have happened"
        );
        let observation = handle.observe();
        assert_eq!(
            observation.position, None,
            "an activation failure must withdraw the sample the dying leg \
             already published: {observation:?}"
        );
        assert_eq!(
            observation.terminal_outcome, None,
            "and the activation failure is still not a terminal Fact (D11 \
             firewall): {observation:?}"
        );
        assert!(
            observation
                .activation_error
                .as_deref()
                .is_some_and(|m| m.contains("render stream open failed")),
            "the diagnostic travelled: {observation:?}"
        );
        assert_eq!(
            observation.source_duration,
            Some(Duration::from_secs(4)),
            "duration is source evidence, not playback state: it stays \
             observable after the failure"
        );
        assert_eq!(consumed.load(Ordering::SeqCst), 0, "nothing ever played");

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// A fresh episode with a running render leg but no published sample yet
/// must still report `None`, not zero — and the first sample it does
/// publish is a real one.
#[test]
fn unknown_position_stays_none_until_the_mechanism_publishes() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            TestDecode::new(SourceBehavior::EofAfter(LONG_SOURCE)),
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(2),
            },
            consumed.clone(),
            DeviceTail::default(),
            handle.clone(),
        );

        // Before activation the episode has no evidence at all.
        let cold = handle.observe();
        assert_eq!(cold.position, None);
        assert_eq!(cold.source_format, None);
        activate(&mut runtime);

        // While running, every observed sample is a real published one:
        // it is never zero just because the leg has not published yet
        // (zero would be a claim about consumption; `None` is the honest
        // "no sample").
        let first = wait_for_a_position(&handle);
        assert!(
            first.position.is_some(),
            "the first observed position is a published sample"
        );
        assert!(!first.paused(), "a fresh episode is not Paused");
        assert_eq!(first.pause_engagement, PauseEngagement::Disengaged);

        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- the writer's promises ----------------------------------------------

/// The projection never goes backward and never exceeds the frames this
/// episode has actually submitted — observed end-to-end while the
/// episode plays, with the tail deliberately lagging (the device plays
/// out less than the leg submits).
#[test]
fn the_published_sample_is_monotone_and_never_exceeds_the_submitted_total() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail = DeviceTail::default();
        // The device plays out 100 frames per observation while the leg
        // submits 256: a queue builds, so the sample trails the total.
        device_tail.set_playout(Playout::FramesPerObservation(100));
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            TestDecode::new(SourceBehavior::EofAfter(LONG_SOURCE)),
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(2),
            },
            consumed.clone(),
            device_tail.clone(),
            handle.clone(),
        );
        activate(&mut runtime);
        wait_for_a_position(&handle);

        let mut last = 0u64;
        let mut saw_lag = false;
        assert!(
            wait_until(Duration::from_secs(10), || {
                let observation = handle.observe();
                let submitted = consumed.load(Ordering::SeqCst) as u64;
                let position = observation.position.expect("published while running");
                assert!(
                    position >= last,
                    "the published sample went backward: {last} -> {position}"
                );
                assert!(
                    position <= submitted,
                    "the published sample {position} exceeded the submitted total {submitted}"
                );
                last = position;
                if position < submitted {
                    saw_lag = true;
                }
                saw_lag
            }),
            "the device never lagged behind the submissions, so the \
             never-exceeds-submitted check was never exercised: last={last}"
        );

        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- pause --------------------------------------------------------------

/// The D14.8 pause rule, end to end: the command alone does not freeze
/// the sample; it keeps advancing while the device drains the already
/// submitted frames; it stops exactly where the D14.7 Paused projection
/// establishes (tail quiescence); it stays constant while parked; and it
/// advances again after resume.
#[test]
fn pause_freezes_the_sample_at_tail_quiescence_not_at_the_command() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail = DeviceTail::default();
        // The device plays out exactly one submission per observation and
        // starts out holding a standoff of already-queued frames, so the
        // queue is stable while the leg plays and drains one step per
        // park slice once it is parked: a deterministic drain instead of
        // a race against how much the leg happens to have queued.
        const STANDOFF: u64 = 32 * BLOCK;
        device_tail.set_playout(Playout::FramesPerObservation(BLOCK));
        device_tail.seed(STANDOFF);
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            TestDecode::new(SourceBehavior::EofAfter(LONG_SOURCE)),
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(2),
            },
            consumed.clone(),
            device_tail.clone(),
            handle.clone(),
        );
        activate(&mut runtime);
        wait_for_a_position(&handle);
        // The sample can only rise above the standoff once the episode has
        // actually submitted past it (the tail reading is capped at the
        // handed-off total), so let it play well past that before pausing.
        assert!(
            wait_until(Duration::from_secs(10), || {
                consumed.load(Ordering::SeqCst) as u64 > STANDOFF + 8 * BLOCK
            }),
            "the episode never submitted past the standoff"
        );

        handle.request_pause();
        let at_command = handle.observe();
        let at_command_position = at_command.position.expect("published");
        assert!(
            !at_command.paused(),
            "the command alone establishes nothing (D14.7): {at_command:?}"
        );
        assert!(
            device_tail.queued() > 0,
            "precondition: the device is still holding queued frames"
        );

        // The tail drains: the projection truthfully keeps advancing —
        // and it advances WHILE the device still holds queued frames, not
        // only once the queue happens to reach zero.
        assert!(
            wait_until(Duration::from_secs(10), || {
                handle.observe().position.expect("published") > at_command_position
                    && device_tail.queued() > 0
            }),
            "the sample was frozen at command time instead of draining \
             toward the handed-off total: at_command={at_command_position}, \
             now={:?}",
            handle.observe()
        );

        // Establishment: Paused is true exactly when the tail is
        // quiesced, and at that instant the sample equals the frozen
        // handed-off total (nothing more is submitted while parked).
        assert!(
            wait_until(Duration::from_secs(10), || handle.observe().paused()),
            "Paused never established: {:?}",
            handle.observe()
        );
        let established = handle.observe();
        let frozen_total = consumed.load(Ordering::SeqCst) as u64;
        assert_eq!(
            established.position,
            Some(frozen_total),
            "at tail quiescence the sample has reached the handed-off total"
        );
        assert_eq!(
            device_tail.queued(),
            0,
            "precondition: the mock device drained its queue"
        );

        // Parked and quiescent: the sample does not move.
        std::thread::sleep(Duration::from_millis(120));
        let parked_later = handle.observe();
        assert_eq!(
            parked_later.position, established.position,
            "a parked, quiesced episode's sample must stay constant"
        );
        assert_eq!(
            consumed.load(Ordering::SeqCst) as u64,
            frozen_total,
            "a parked leg submits nothing"
        );

        // Resume: the device plays out again and the sample advances.
        handle.request_resume();
        assert!(
            wait_until(Duration::from_secs(10), || {
                handle.observe().position.expect("published") > frozen_total
            }),
            "the sample never advanced after resume: {:?}",
            handle.observe()
        );

        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// A pause that never establishes keeps the projection truthful: while
/// the device still holds its queue and refuses to play it out, the
/// sample stays where it is and the episode is not Paused.
#[test]
fn a_pause_that_never_quiesces_leaves_the_sample_where_it_was() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail = DeviceTail::default();
        // A device that plays nothing out: the queued frames stay queued.
        device_tail.set_playout(Playout::FramesPerObservation(0));
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            TestDecode::new(SourceBehavior::EofAfter(LONG_SOURCE)),
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(2),
            },
            consumed.clone(),
            device_tail.clone(),
            handle.clone(),
        );
        activate(&mut runtime);
        wait_for_a_position(&handle);
        handle.request_pause();

        assert!(
            wait_until(Duration::from_secs(10), || handle
                .observe()
                .pause_engagement
                == PauseEngagement::Engaged),
            "the leg never engaged: {:?}",
            handle.observe()
        );
        let snapshot = handle.observe();
        assert!(
            !snapshot.paused(),
            "a device that never quiesces cannot establish Paused: {snapshot:?}"
        );
        // The sample is whatever the last observation published; it must
        // not have been "completed" to the handed-off total by the
        // engagement (the tail is still queued).
        let submitted = consumed.load(Ordering::SeqCst) as u64;
        let position = snapshot.position.expect("published");
        assert!(
            position <= submitted,
            "the engagement must not invent consumption: {position} > {submitted}"
        );
        assert!(
            position < submitted,
            "the queued frames are still queued, so the sample trails the total"
        );

        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- terminal withdrawal ------------------------------------------------

/// Stop from an ESTABLISHED pause: the release ends the park and lets the
/// writer proceed once more, so the sample may advance by at most the one
/// in-flight block the frozen rule already measures at pause-command
/// time — and then the terminal Fact withdraws the projection with no
/// final-position latch anywhere. This is the interaction the pause and
/// terminal rules meet on, so it is pinned rather than assumed.
#[test]
fn stop_from_an_established_pause_withdraws_without_latching_a_final_sample() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail = DeviceTail::default();
        device_tail.set_playout(Playout::FramesPerObservation(100));
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            TestDecode::new(SourceBehavior::EofAfter(LONG_SOURCE)),
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(2),
            },
            consumed.clone(),
            device_tail,
            handle.clone(),
        );
        activate(&mut runtime);
        wait_for_a_position(&handle);

        handle.request_pause();
        assert!(
            wait_until(Duration::from_secs(10), || handle.observe().paused()),
            "Paused never established: {:?}",
            handle.observe()
        );
        let parked = handle.observe();
        let frozen = parked.position.expect("published");
        let submitted_at_park = consumed.load(Ordering::SeqCst) as u64;
        assert_eq!(
            frozen, submitted_at_park,
            "at quiescence the sample equals the frozen handed-off total"
        );

        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let settled = handle.observe();
        assert_eq!(settled.position, None, "withdrawn, not latched");
        assert!(
            consumed.load(Ordering::SeqCst) as u64 >= frozen,
            "the accounting never goes backward across the release"
        );
        assert!(!settled.paused(), "a settled episode is never Paused");

        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// Every terminal Fact withdraws the position projection (no final
/// position is stored anywhere), while the source duration evidence
/// stays — it is source evidence, not a playback-state projection.
///
/// Each case is built so the episode provably publishes a sample and
/// keeps running long enough to observe it BEFORE it ends: a source the
/// render leg chews through at ~256 frames per paced read, so the
/// terminal cannot land before the first observation. The pre-terminal
/// sample is what makes the withdrawal assertion mean something (an
/// episode that never published has `None` on both sides of the Fact).
#[test]
fn each_terminal_fact_withdraws_the_position_and_keeps_the_duration() {
    /// Frames the paced sources carry before ending (~256 ms at the
    /// per-read pace below).
    const PACED_SOURCE: usize = 64 * 1024;
    const PACE: Duration = Duration::from_millis(1);

    for (name, decode, output, expected) in [
        (
            "stopped",
            TestDecode::with_duration(
                SourceBehavior::EofAfter(LONG_SOURCE),
                Duration::from_secs(30),
            ),
            OutputBehavior::SlowConsume { per_read: PACE },
            EpisodeTerminalOutcome::Stopped,
        ),
        (
            "completed",
            // The declared duration deliberately disagrees with what this
            // source actually decodes to (~1.5 s): it is reported
            // evidence, and the Completed path must neither rewrite it
            // nor force the position onto it.
            TestDecode::with_duration(
                SourceBehavior::EofAfter(PACED_SOURCE),
                Duration::from_secs(4),
            ),
            OutputBehavior::SlowConsume { per_read: PACE },
            EpisodeTerminalOutcome::Completed,
        ),
        (
            "failed",
            TestDecode::with_duration(
                SourceBehavior::FailAfter(PACED_SOURCE),
                Duration::from_secs(30),
            ),
            OutputBehavior::SlowConsume { per_read: PACE },
            EpisodeTerminalOutcome::Failed,
        ),
    ] {
        let _lifecycle = common::lifecycle_lock();
        within(Duration::from_secs(30), move || {
            let consumed = Arc::new(AtomicUsize::new(0));
            let handle = PlaybackSessionHandle::new();
            let mut runtime = registered_runtime(
                decode.clone(),
                output,
                consumed.clone(),
                DeviceTail::default(),
                handle.clone(),
            );
            activate(&mut runtime);

            // The episode is still running (paced), so a published
            // sample is observable before the terminal Fact lands.
            let before_terminal = wait_for_a_position(&handle);
            assert_eq!(
                before_terminal.terminal_outcome, None,
                "case {name}: the episode must still be unsettled here"
            );
            let declared = before_terminal.source_duration;

            if name == "stopped" {
                handle.request_stop();
            }
            assert_eq!(handle.wait_terminal(), expected, "case {name}");
            let settled = handle.observe();
            assert_eq!(settled.terminal_outcome, Some(expected));
            assert_eq!(
                settled.position, None,
                "case {name}: the position projection is withdrawn at the terminal Fact"
            );
            assert_eq!(
                settled.source_duration, declared,
                "case {name}: the duration evidence is source-scoped and stays exactly as reported"
            );
            assert!(
                settled.source_duration.is_some(),
                "case {name}: the probe reported a duration, so it is observable"
            );
            assert!(
                !settled.paused(),
                "case {name}: a settled episode is never Paused"
            );

            let snapshot = runtime.dispose();
            assert!(
                snapshot.quiet,
                "case {name}: teardown must stay quiet: {snapshot:?}"
            );
        });
    }
}

/// A duration the probe never reported stays unknown — never zero, and
/// never estimated to fill the display.
#[test]
fn an_unreported_duration_stays_unknown() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            TestDecode::new(SourceBehavior::EofAfter(4 * BLOCK as usize)),
            OutputBehavior::Consume,
            consumed.clone(),
            DeviceTail::default(),
            handle.clone(),
        );
        activate(&mut runtime);

        assert_eq!(
            handle.observe().source_duration,
            None,
            "the probe reported none, so the observation says unknown"
        );
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_eq!(
            handle.observe().source_duration,
            None,
            "unknown stays unknown after settlement too — never zero"
        );
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// A legitimately zero-length declaration is evidence, not "unknown":
/// the two are distinguishable at the product surface.
#[test]
fn a_zero_length_duration_is_distinguishable_from_unknown() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            TestDecode::with_duration(SourceBehavior::EofAfter(LONG_SOURCE), Duration::ZERO),
            // Paced, so the episode cannot reach EOF before the stop
            // below: this test is about the duration spelling, not about
            // racing completion.
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(2),
            },
            consumed.clone(),
            DeviceTail::default(),
            handle.clone(),
        );
        activate(&mut runtime);
        assert_eq!(handle.observe().source_duration, Some(Duration::ZERO));
        assert_ne!(
            handle.observe().source_duration,
            None,
            "a reported zero length is not the absence of evidence"
        );
        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- purity -------------------------------------------------------------

/// `observe()` is one pure read (D14.2): repeating it changes nothing —
/// not the position, not the projection, not the terminal state. The
/// sample is whatever the mechanism published, and reading it never
/// advances, clamps, repairs or settles anything.
///
/// Behavioural oracle, and one with a stated limit: an idempotent
/// reader-side clamp over an already-monotone cell would be invisible to
/// it (and to any observation-level test). What rules that shape out is
/// structural, not behavioural — nothing in the read path can hold state
/// or write: `PositionEvidence::published` is a single load and
/// `observe_snapshot` only reads. This test pins the observable
/// consequence (quiescent state in, identical state out, nothing
/// consumed); the absence of reader state is pinned by the code shape and
/// by the observation-surface allowlist.
#[test]
fn repeating_an_observation_changes_nothing() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let consumed = Arc::new(AtomicUsize::new(0));
        let device_tail = DeviceTail::default();
        // A draining device, paused to quiescence: the leg is parked and
        // submits nothing, so the mechanism itself is quiescent and any
        // change between two observations could only come from the
        // observation.
        device_tail.set_playout(Playout::FramesPerObservation(100));
        let handle = PlaybackSessionHandle::new();
        let mut runtime = registered_runtime(
            TestDecode::new(SourceBehavior::EofAfter(LONG_SOURCE)),
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(2),
            },
            consumed.clone(),
            device_tail,
            handle.clone(),
        );
        activate(&mut runtime);
        wait_for_a_position(&handle);
        handle.request_pause();
        assert!(
            wait_until(Duration::from_secs(10), || handle.observe().paused()),
            "Paused never established: {:?}",
            handle.observe()
        );

        let baseline = handle.observe();
        assert!(baseline.position.is_some(), "a sample exists to be read");
        for _ in 0..64 {
            let repeated = handle.observe();
            assert_eq!(
                repeated, baseline,
                "a repeated observation changed the episode's projection"
            );
            // Also assert the individual truth classes explicitly, so a
            // regression that changed one field's derivation is named.
            assert_eq!(repeated.position, baseline.position);
            assert_eq!(repeated.terminal_outcome, baseline.terminal_outcome);
            assert_eq!(repeated.source_duration, baseline.source_duration);
            assert_eq!(repeated.pause_engagement, baseline.pause_engagement);
        }
        // The submissions did not resume because we read the episode.
        assert_eq!(
            consumed.load(Ordering::SeqCst) as u64,
            baseline.position.expect("published"),
            "observing must not consume or submit anything"
        );

        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}
