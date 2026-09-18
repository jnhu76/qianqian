//! F5 seek seam tests (ADR-PBK-002 D14.5): the REAL Playback Session
//! over mechanism doubles, asserting what a seek command observably
//! does — and observably does NOT do.
//!
//! Truth classes under test: `request_seek` is a Command whose
//! acceptance is not a cutover; the observable consequences are exactly
//! (a) the Position projection rebasing to the decoder's ACTUAL landing
//! at a committed cutover (or withdrawing forever for an unknown
//! landing), (b) the consumed CONTENT sequence showing exactly one
//! discontinuity — the cut — for a commit, and none for a refusal
//! (refusal equals the no-seek control, frame for frame), and (c) for a
//! destructive provider failure, the ordinary terminal `Failed` through
//! the existing D11 path. There is deliberately no public positive seek
//! state to read; these matrices pin the consequences instead.
//!
//! The content oracle: the decode double position-tags every frame
//! (sample value = absolute frame index), and the output double records
//! the channel-0 value of every submitted frame in submission order. A
//! committed cutover MUST appear as exactly one discontinuity `K →
//! landing` with both sides contiguous; the skipped stretch between the
//! old position and the landing is what a seek IS, so the sequence
//! length is a witness only for the refusal (zero loss) and is
//! meaningless for a cut. A refused seek MUST produce the frame-exact
//! sequence of the no-seek control.
//!
//! What the mechanism doubles cannot cover is stated, not implied:
//!
//! ```text
//! covered elsewhere
//!   gate algebra      qianqian-audio-api's render_gate_seek oracles
//!                     (payload-awaits-consumption, one-payload, hold
//!                     resets, closed gate)
//!   position algebra  qianqian-audio-api's position_evidence oracles
//!                     (rebase as the one legal backward step, zero
//!                     landing, withdrawal discipline)
//!   real leg order    the Windows render loop's source-order oracle
//!                     (publish/unconditional-slice rules P1–P10)
//!   protocol latches  the completion's own white-box tests inside the
//!                     crate (slot policy, acceptance conditions, the
//!                     commit conjunction, first-wins latches) —
//!                     deliberately NOT public product surface (D14.5:
//!                     no public positive seek state)
//!
//! blind spots of ANY double-driven test here
//!   failed submission  the mock has no failing ReleaseBuffer; "a failed
//!                      submission is never counted" rests on the order
//!                      oracle, not on this file
//!   frame units        the mock is unit-agnostic (see common/mod.rs)
//! ```

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use qianqian_app::QianqianApp;
use qianqian_audio_api::ports::ProviderSeekOutcome;
use qianqian_composition::{DesiredEntry, Revision};
use qianqian_playback::{EpisodeTerminalOutcome, PlaybackSessionHandle};

use common::{
    DeviceTail, OutputBehavior, SourceBehavior, TailProbe, TestDecode, TestOutput, within,
};

const DUMMY_PATH: &str = "test://seek-seam";

/// An 8-second source: long enough that a seek fired in the first second
/// is decisively mid-stream (the bounded edge is full and the worker is
/// parked inside its interruptible write when the command is planted),
/// short enough that the slow mock consumer still finishes it in
/// seconds.
const SOURCE_FRAMES: usize = 44_100 * 8;
/// 0.5 s, 1 s and 5 s of source, in frames (the episode format's rate).
const HALF_A_SECOND: u64 = 22_050;
const ONE_SECOND: u64 = 44_100;
const FIVE_SECONDS: u64 = 5 * 44_100;

fn desired(id: &str, component: &'static str) -> DesiredEntry {
    DesiredEntry::enabled(id, component, Revision::new(1))
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

/// The witnesses one seek test drives and asserts on; the test owns
/// every one of them, so a test can read them before, while and after
/// the runtime uses them.
struct Witnesses {
    consumed: Arc<AtomicUsize>,
    consumed_values: Arc<Mutex<Vec<f32>>>,
    device_tail: DeviceTail,
    tail_probe: TailProbe,
}

/// Register the standard F5 episode: a seek-scripted decode double, an
/// output double sharing the caller's witnesses, and the real session
/// over `handle`.
fn registered_runtime(
    decode: TestDecode,
    output: OutputBehavior,
    witnesses: &Witnesses,
    handle: PlaybackSessionHandle,
) -> QianqianApp {
    let consumed = witnesses.consumed.clone();
    let consumed_values = witnesses.consumed_values.clone();
    let device_tail = witnesses.device_tail.clone();
    let tail_probe = witnesses.tail_probe.clone();
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
                    let mut service = TestOutput::observed_with_content(
                        output,
                        consumed.clone(),
                        consumed_values.clone(),
                        device_tail.clone(),
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
        .register_component(qianqian_playback::playback_session_spec(
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

/// The standard episode: an 8-second position-tagged source played out
/// by a slow mock consumer (so a seek fired in the first second is
/// decisively mid-stream), with the standard witness set.
fn episode(seeks: Vec<ProviderSeekOutcome>) -> (Witnesses, PlaybackSessionHandle, QianqianApp) {
    let witnesses = Witnesses {
        consumed: Arc::new(AtomicUsize::new(0)),
        consumed_values: Arc::new(Mutex::new(Vec::new())),
        device_tail: DeviceTail::default(),
        tail_probe: TailProbe::default(),
    };
    let handle = PlaybackSessionHandle::new();
    let mut runtime = registered_runtime(
        TestDecode {
            behavior: SourceBehavior::EofAfter(SOURCE_FRAMES),
            duration: Some(Duration::from_secs(8)),
            seeks,
        },
        OutputBehavior::SlowConsume {
            per_read: Duration::from_millis(1),
        },
        &witnesses,
        handle.clone(),
    );
    activate(&mut runtime);
    (witnesses, handle, runtime)
}

/// Block until the projection has climbed past `frames` (the episode is
/// decisively playing) and return the sample seen there.
fn wait_for_position_past(handle: &PlaybackSessionHandle, frames: u64) -> u64 {
    let mut sample = 0u64;
    assert!(
        wait_until(Duration::from_secs(5), || {
            if let Some(position) = handle.observe().position {
                sample = position;
                position >= frames
            } else {
                false
            }
        }),
        "the position never reached {frames}: {sample}"
    );
    sample
}

/// The consumed content, snapped once.
fn content(witnesses: &Witnesses) -> Vec<f32> {
    witnesses.consumed_values.lock().unwrap().clone()
}

/// Indices where the position-tagged sequence does not continue the
/// previous frame index. Frame values stay exact in f32 far beyond this
/// source's length (< 2^24), so the comparison is exact.
fn discontinuities(values: &[f32]) -> Vec<usize> {
    values
        .iter()
        .zip(values.iter().skip(1))
        .enumerate()
        .filter_map(|(i, (a, b))| (*b != *a + 1.0).then_some(i + 1))
        .collect()
}

/// Assert the canonical committed-cut content shape: contiguous from
/// frame 0, exactly ONE discontinuity, landing exactly at `landing`,
/// contiguous after.
fn assert_one_cut_to(values: &[f32], landing: u64) {
    let breaks = discontinuities(values);
    assert_eq!(
        breaks.len(),
        1,
        "a committed cutover must appear as exactly one content \
         discontinuity (got {breaks:?} in {} frames)",
        values.len()
    );
    assert_eq!(
        values[0], 0.0,
        "the first stretch must start at the source's frame 0"
    );
    assert_eq!(
        values[breaks[0]] as u64, landing,
        "the cut must land exactly at the provider's ACTUAL landing"
    );
}

// --- committed cut -------------------------------------------------------

/// The core happy path: a mid-stream seek to 5 s lands the consumed
/// content exactly at the provider's actual landing — contiguous before,
/// one discontinuity, contiguous after — and the projection climbs again
/// from the landing. The episode still runs to its ordinary Completed
/// terminal. This also drives the mid-write cut point: by half a second
/// the bounded edge is full and the worker is parked inside its
/// interruptible write when the command is planted.
#[test]
fn a_committed_forward_seek_jumps_the_content_exactly_at_the_actual_landing() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let (witnesses, handle, mut runtime) = episode(Vec::new());
        wait_for_position_past(&handle, HALF_A_SECOND);
        handle.request_seek(Duration::from_secs(5));
        assert!(
            wait_until(Duration::from_secs(5), || handle
                .observe()
                .position
                .is_some_and(|p| p >= FIVE_SECONDS)),
            "the position never rebased to the landing: {:?}",
            handle.observe()
        );
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "the seek must not disturb the episode's ordinary EOF course"
        );
        assert_one_cut_to(&content(&witnesses), FIVE_SECONDS);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// A backward seek is the same mechanism with the cut pointing earlier:
/// the projection legally steps backward exactly once (the rebase — the
/// one backward step the cell admits), the content shows the one jump
/// back to the landing, and afterwards the sample resumes its
/// never-backward discipline.
#[test]
fn a_committed_backward_seek_rebases_the_position_legally_backward() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let (witnesses, handle, mut runtime) = episode(Vec::new());
        let pre = wait_for_position_past(&handle, ONE_SECOND);
        handle.request_seek(Duration::from_millis(200));
        assert!(
            wait_until(Duration::from_secs(5), || handle
                .observe()
                .position
                .is_some_and(|p| p < pre)),
            "the position must step backward to below {pre} at the rebase: {:?}",
            handle.observe()
        );
        // After the rebase the sample resumes its monotone course: a
        // later read never goes backward again.
        let first = handle.observe().position.expect("published");
        let second = handle.observe().position.expect("published");
        assert!(
            second >= first,
            "after the one legal backward step the sample must never go \
             backward again: {first} -> {second}"
        );
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_one_cut_to(&content(&witnesses), 8_820); // 200 ms at 44.1 kHz
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// A seek to time zero is a real landing: the projection legally steps
/// backward to the landing and KEEPS PUBLISHING from it — a zero landing
/// is a position, and only an unknown landing withdraws (that
/// distinction is pinned at the cell's own algebra in
/// qianqian-audio-api; here it is pinned end-to-end: any published
/// sample below the pre-seek one proves the zero landing published
/// rather than withdrew, and the content restarts from frame 0). The
/// exact `Some(0)` instant is a freshness race for a poller — the
/// sample sits at zero for one publication cadence before it climbs —
/// so it is deliberately not what this test waits for.
#[test]
fn a_zero_landing_publishes_position_zero_not_undefined() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let (witnesses, handle, mut runtime) = episode(Vec::new());
        wait_for_position_past(&handle, HALF_A_SECOND);
        handle.request_seek(Duration::ZERO);
        let mut saw_rebased_sample = false;
        wait_until(Duration::from_secs(5), || {
            let observation = handle.observe();
            if let Some(p) = observation.position
                && p < HALF_A_SECOND
            {
                saw_rebased_sample = true;
                return true;
            }
            // Stop early once the episode settled: past the terminal the
            // projection is withdrawn by design, which would mask the
            // question this poll answers.
            observation.terminal_outcome.is_some()
        });
        assert!(
            saw_rebased_sample,
            "a zero landing is a position, never undefined: the projection \
             must publish a live sample below the pre-seek position at the \
             rebase (last: {:?})",
            handle.observe()
        );
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_one_cut_to(&content(&witnesses), 0);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// Beyond-extent targets pass through untouched: duration evidence is
/// never consulted at acceptance, and the PROVIDER's decision — its own
/// validity rule and its own landing — is authoritative. The scripted
/// landing stands in for that decision (near the extent, deliberately
/// unrelated to the 999 s request), and the cut must land THERE: the
/// request is never the landing.
#[test]
fn a_beyond_duration_target_is_the_providers_decision() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let landing = SOURCE_FRAMES as u64 - 100;
        let (witnesses, handle, mut runtime) = episode(vec![ProviderSeekOutcome::Applied {
            landing: Some(landing),
        }]);
        wait_for_position_past(&handle, HALF_A_SECOND);
        handle.request_seek(Duration::from_secs(999));
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_one_cut_to(&content(&witnesses), landing);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// An `Applied` cutover whose landing the provider could not report:
/// the commit still happens (the content jumps — stale exclusion is
/// independent of landing knowledge), production continues from the
/// provider's own continuation, and the Position projection is
/// withdrawn for the REST OF THE EPISODE — through production, a later
/// committed cutover with a KNOWN landing, EOF, and the terminal Fact.
/// Unknown stays unknown; it never becomes zero and never becomes the
/// requested target.
#[test]
fn an_unknown_landing_withdraws_the_position_for_the_rest_of_the_episode() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let later_landing = 6 * ONE_SECOND;
        let (witnesses, handle, mut runtime) = episode(vec![
            ProviderSeekOutcome::Applied { landing: None },
            ProviderSeekOutcome::Applied {
                landing: Some(later_landing),
            },
        ]);
        wait_for_position_past(&handle, HALF_A_SECOND);
        handle.request_seek(Duration::from_secs(5));
        assert!(
            wait_until(Duration::from_secs(5), || handle
                .observe()
                .position
                .is_none()),
            "the unknown landing withdraws the projection"
        );
        // A second committed cutover WITH a known landing must not
        // resurrect the withdrawn projection: the withdrawal is for the
        // rest of the episode (D14.5 position rebase). The request may
        // race the worker's post-consumption slot free by a poll slice,
        // so it is retried — exactly what a client would do — and the
        // retry stops the moment the second cut is observable, so no
        // extra legal seek fires past it.
        let mut second_cut = false;
        wait_until(Duration::from_secs(5), || {
            second_cut = discontinuities(&content(&witnesses)).len() == 2;
            if !second_cut {
                handle.request_seek(Duration::from_secs(6));
            }
            second_cut
        });
        assert!(
            second_cut,
            "precondition: the second cutover must happen for this oracle"
        );
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert!(
            handle.observe().position.is_none(),
            "the withdrawal survives a later known-landing cutover: {:?}",
            handle.observe()
        );
        // Both cuts are in the content, at the two landings.
        let values = content(&witnesses);
        let breaks = discontinuities(&values);
        assert_eq!(breaks.len(), 2);
        assert_eq!(values[breaks[0]] as u64, FIVE_SECONDS);
        assert_eq!(values[breaks[1]] as u64, later_landing);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- refusal -------------------------------------------------------------

/// A proven pre-mutation refusal loses NOTHING: the preserved remainder
/// of the interrupted staging block is finished frame-for-frame, and the
/// consumed content is INDISTINGUISHABLE from a no-seek control — every
/// frame index in order, zero discontinuities.
#[test]
fn a_refused_seek_finishes_its_own_remainder_with_zero_content_loss() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let (witnesses, handle, mut runtime) = episode(vec![ProviderSeekOutcome::RefusedUnchanged]);
        wait_for_position_past(&handle, HALF_A_SECOND);
        handle.request_seek(Duration::from_secs(5));
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "a refusal is not a failure and not a cut"
        );
        let values = content(&witnesses);
        assert!(
            discontinuities(&values).is_empty(),
            "a refused seek must be content-indistinguishable from no seek"
        );
        let control: Vec<f32> = (0..SOURCE_FRAMES).map(|i| i as f32).collect();
        assert_eq!(values, control, "zero content loss, frame for frame");
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- destructive provider failure ---------------------------------------

/// An unprovable provider failure is destructive by definition: the
/// episode NEVER resumes old-cursor production — it takes the ordinary
/// D11 `Failed` route, production stops for good, and the terminal is
/// `Failed` (not Stopped, not Completed), with the diagnostic as
/// presentation only.
#[test]
fn a_destructive_provider_failure_fails_the_episode_and_never_resumes() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let (witnesses, handle, mut runtime) =
            episode(vec![ProviderSeekOutcome::MutatedThenFailed {
                diagnostic: "mid-stream corruption".to_owned(),
            }]);
        wait_for_position_past(&handle, HALF_A_SECOND);
        handle.request_seek(Duration::from_secs(5));
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Failed,
            "an unprovable seek failure is a decode failure (D11)"
        );
        let observation = handle.observe();
        assert!(
            observation
                .failure_diagnostic
                .as_deref()
                .is_some_and(|d| d.contains("seek failed")),
            "the diagnostic is presentation evidence of the route: {observation:?}"
        );
        // The episode never resumes: production stopped at the cut.
        let stopped_at = witnesses.consumed.load(Ordering::SeqCst);
        assert!(
            stopped_at < SOURCE_FRAMES,
            "the destructive failure must end before the source is exhausted"
        );
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            witnesses.consumed.load(Ordering::SeqCst),
            stopped_at,
            "a failed episode must never resume production"
        );
        assert!(
            handle.observe().position.is_none(),
            "the terminal Fact withdraws the projection"
        );
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- pause x seek --------------------------------------------------------

/// A paused episode is seekable: the cutover commits through the PAUSE
/// engagement (its park is the same physical evidence class), the pause
/// intent and the Paused projection SURVIVE the seek, and on resume the
/// leg consumes the rebase payload at its gate check — before any
/// further submission — so the projection climbs from the landing and
/// the content shows exactly the one cut.
#[test]
fn a_paused_episode_commits_its_seek_and_stays_paused_until_resumed() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let (witnesses, handle, mut runtime) = episode(Vec::new());
        wait_for_position_past(&handle, HALF_A_SECOND);
        handle.request_pause();
        assert!(
            wait_until(Duration::from_secs(5), || handle.observe().paused()),
            "precondition: the episode never established Paused: {:?}",
            handle.observe()
        );
        handle.request_seek(Duration::from_secs(5));
        // The commit must have routed while still paused, and nothing
        // about the seek may have implicit-resumed the episode.
        std::thread::sleep(Duration::from_millis(200));
        let during = handle.observe();
        assert!(
            during.paused(),
            "pause intent survives a seek; the seek never resumes: {during:?}"
        );
        assert_eq!(
            during.terminal_outcome, None,
            "a seek is never a terminal event"
        );
        handle.request_resume();
        assert!(
            wait_until(Duration::from_secs(5), || handle
                .observe()
                .position
                .is_some_and(|p| p >= FIVE_SECONDS)),
            "after resume the projection must climb from the landing: {:?}",
            handle.observe()
        );
        assert!(!handle.observe().paused(), "resumed is not paused");
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_one_cut_to(&content(&witnesses), FIVE_SECONDS);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- one-seek policy -----------------------------------------------------

/// One seek in flight: requests fired while the cut's protocol is still
/// running (held open by the armed tail probe) are INERT — no queueing,
/// no coalescing, no latest-wins. Exactly one cutover happens, at the
/// FIRST seek's landing; a queued or latest-wins policy would cut again.
#[test]
fn a_second_seek_while_one_is_in_flight_is_inert() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let (witnesses, handle, mut runtime) = episode(Vec::new());
        wait_for_position_past(&handle, HALF_A_SECOND);
        // The armed probe holds the leg inside its seek-park tail
        // observation: the park cannot publish its quiescence, so the
        // commit decision cannot route while we fire the extra requests.
        witnesses.tail_probe.arm();
        handle.request_seek(Duration::from_secs(5));
        assert!(
            witnesses
                .tail_probe
                .wait_held_within(Duration::from_secs(5)),
            "precondition: the leg never parked under the cut's hold"
        );
        // Both later requests linearize while the first seek is provably
        // still in flight.
        handle.request_seek(Duration::from_secs(6));
        handle.request_seek(Duration::from_secs(7));
        std::thread::sleep(Duration::from_millis(200));
        witnesses.tail_probe.unhold();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        // Exactly one cut, to the FIRST seek's landing — a queueing or
        // latest-wins policy would show a second cut (to 6 s or 7 s).
        assert_one_cut_to(&content(&witnesses), FIVE_SECONDS);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// The commit does not end a cut: its rebase payload is part of the cut
/// until the leg CONSUMES it. A cut committed while the leg is held by
/// PAUSE leaves the payload awaiting (pause slices stopped observing),
/// and a seek fired in that window must be INERT — a new hold would
/// wipe the awaiting `Committed` and the episode would keep its OLD
/// position basis forever (the rebase lost). After resume the content
/// cuts exactly once, at the FIRST seek's landing.
#[test]
fn a_committed_release_is_never_wiped_by_a_later_seek() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(30), move || {
        let (witnesses, handle, mut runtime) = episode(vec![
            ProviderSeekOutcome::Applied {
                landing: Some(FIVE_SECONDS),
            },
            ProviderSeekOutcome::Applied {
                landing: Some(6 * ONE_SECOND),
            },
        ]);
        wait_for_position_past(&handle, HALF_A_SECOND);
        handle.request_pause();
        assert!(
            wait_until(Duration::from_secs(5), || handle.observe().paused()),
            "precondition: the episode never established Paused"
        );
        handle.request_seek(Duration::from_secs(5));
        // The cut commits through the pause engagement; its payload
        // awaits the parked leg (pause slices are quiesced-and-blind).
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            handle.observe().paused(),
            "precondition: the seek must not have resumed the episode"
        );
        // Fires while the first cut's payload provably still awaits.
        handle.request_seek(Duration::from_secs(6));
        std::thread::sleep(Duration::from_millis(100));
        handle.request_resume();
        assert!(
            wait_until(Duration::from_secs(5), || handle
                .observe()
                .position
                .is_some_and(|p| p >= FIVE_SECONDS)),
            "the projection must rebase to the FIRST seek's landing: {:?}",
            handle.observe()
        );
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        // Exactly one cut, at the first landing: the later seek never
        // cut, and the committed rebase was never lost.
        assert_one_cut_to(&content(&witnesses), FIVE_SECONDS);
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- stop x seek ---------------------------------------------------------

/// Stop intent recorded while a cut's protocol is running WINS: the
/// commit decision sees the stop, routes an abort, and the episode
/// settles `Stopped` — never `Completed` through a post-cut course,
/// never `Failed` through the seek.
#[test]
fn stop_intent_wins_over_an_in_flight_seek() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let (witnesses, handle, mut runtime) = episode(Vec::new());
        wait_for_position_past(&handle, HALF_A_SECOND);
        witnesses.tail_probe.arm();
        handle.request_seek(Duration::from_secs(5));
        assert!(
            witnesses
                .tail_probe
                .wait_held_within(Duration::from_secs(5)),
            "precondition: the leg never parked under the cut's hold"
        );
        handle.request_stop();
        witnesses.tail_probe.unhold();
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Stopped,
            "stop recorded before the commit wins the race"
        );
        assert_eq!(handle.observe().failure_diagnostic, None);
        let stopped_at = witnesses.consumed.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            witnesses.consumed.load(Ordering::SeqCst),
            stopped_at,
            "a stopped episode produces nothing further"
        );
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- inert acceptance windows -------------------------------------------

/// The post-EOF drain window is explicitly NOT seekable, and a terminal
/// Fact freezes the whole command surface: a seek after Completed is
/// inert command history — nothing new plays, nothing changes.
#[test]
fn a_seek_after_the_episode_has_settled_is_inert() {
    let _lifecycle = common::lifecycle_lock();
    within(Duration::from_secs(20), move || {
        let (witnesses, handle, mut runtime) = episode(Vec::new());
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        let values_before = content(&witnesses);
        handle.request_seek(Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(content(&witnesses), values_before, "nothing new plays");
        let settled = handle.observe();
        assert_eq!(
            settled.terminal_outcome,
            Some(EpisodeTerminalOutcome::Completed)
        );
        assert!(
            settled.position.is_none(),
            "the projection stays withdrawn after the terminal Fact"
        );
        let snapshot = runtime.dispose();
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// A seek on a handle bound to no episode (never activated) is a no-op:
/// the command records nothing and cannot fabricate any state.
#[test]
fn a_seek_on_a_never_activated_episode_is_inert() {
    let handle = PlaybackSessionHandle::new();
    handle.request_seek(Duration::from_secs(1));
    let observation = handle.observe();
    assert_eq!(observation.terminal_outcome, None);
    assert_eq!(observation.position, None);
}
