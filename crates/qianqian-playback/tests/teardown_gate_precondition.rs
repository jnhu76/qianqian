//! B-02 regression pin (Stage-B integrated audit, Reviewer B): the
//! `RenderStream::stop_and_join` ownership precondition, made
//! regression-visible.
//!
//! The contract (now stated on the trait): the owning session teardown
//! releases every session-owned render-gate hold BEFORE calling
//! `stop_and_join`; the backend never fabricates pause/seek release
//! intent. Production realizes the release in the session's teardown
//! inverse — `release_pause_gate()` (under the completion lock, both
//! park attributions) strictly before `stop_and_join()`.
//!
//! The fake below witnesses the gate state AT `stop_and_join` entry
//! with a bounded `park_loop_top` probe whose tail observations always
//! fail: any still-routed park shape exits bounded with
//! `ParkOutcome::TailProbeFailed` instead of hanging, so the pin never
//! hangs by construction. `Proceeded` means nothing was routed — the
//! precondition held.
//!
//! The negative detection lives in the dispose-while-parked test: the
//! stop path cannot isolate this precondition (`request_stop` itself
//! releases the gate under the completion lock), so that test withdraws
//! a PARKED episode directly — there, the teardown inverse's own
//! `release_pause_gate()` is the only release. Delete or reorder it and
//! the witness records routed work at teardown (and, if the witness
//! probe is also removed, the join itself wedges — the `within` harness
//! bounds that true-defect signature into a timeout failure, never a
//! silent hang).

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use qianqian_app::QianqianApp;
use qianqian_audio_api::ports::{
    AudioOutput, DrainVerdict, GateSlice, OutputError, ParkOutcome, PcmFormat, PcmPull, RenderGate,
    RenderPcmInput, RenderRequest, RenderStream, TailProbeOutcome,
};
use qianqian_composition::{DesiredEntry, Revision};
use qianqian_playback::{EpisodeTerminalOutcome, PlaybackSessionHandle, playback_session_spec};

use common::{SourceBehavior, TEST_FORMAT, TestDecode, lifecycle_lock, within};

const DUMMY_PATH: &str = "test://teardown-gate-precondition";

/// Witness values written by `stop_and_join`.
const UNWITNESSED: usize = 0;
const GATE_FULLY_RELEASED: usize = 1;
const ROUTED_WORK_REMAINED: usize = 2;

/// The session-owned pause/seek gate, captured at open, is the thing
/// the precondition talks about. The leg itself is a minimal steady
/// loop: one loop-top gate check, then a data-plane read — enough to
/// park genuinely (its tail observations always report quiesced, like
/// a device with nothing pending) without any content witnesses.
struct GateWitnessOutput {
    outcome: Arc<AtomicUsize>,
}

fn leg_loop(input: &Arc<dyn RenderPcmInput>, gate: &RenderGate) -> DrainVerdict {
    let mut dst = vec![0.0f32; 256 * usize::from(TEST_FORMAT.channels)];
    loop {
        if matches!(
            gate.park_loop_top(|slice| match slice {
                GateSlice::TailProbe => TailProbeOutcome::Quiesced,
                GateSlice::SeekRelease(_) => TailProbeOutcome::Pending,
            }),
            ParkOutcome::TailProbeFailed
        ) {
            return DrainVerdict::Aborted;
        }
        match input.read_frames(&mut dst) {
            PcmPull::Frames(_) => {}
            PcmPull::Eof => return DrainVerdict::Drained,
            PcmPull::Stopped => return DrainVerdict::Aborted,
        }
    }
}

struct WitnessStream {
    input: Arc<dyn RenderPcmInput>,
    gate: RenderGate,
    witness: Arc<AtomicUsize>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl RenderStream for WitnessStream {
    fn negotiated_format(&self) -> PcmFormat {
        TEST_FORMAT
    }

    fn stop_and_join(mut self: Box<Self>) {
        // The B-02 witness, first thing on the mechanism side: read the
        // gate's routed state at teardown entry. Bounded by
        // construction — the probe's tail observations always fail, so
        // every park shape exits immediately.
        let outcome = self.gate.park_loop_top(|slice| match slice {
            GateSlice::TailProbe => TailProbeOutcome::Failed,
            GateSlice::SeekRelease(_) => TailProbeOutcome::Pending,
        });
        self.witness.store(
            match outcome {
                ParkOutcome::Proceeded => GATE_FULLY_RELEASED,
                _ => ROUTED_WORK_REMAINED,
            },
            Ordering::SeqCst,
        );
        self.input.stop();
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl AudioOutput for GateWitnessOutput {
    fn open_stream(&self, request: RenderRequest) -> Result<Box<dyn RenderStream>, OutputError> {
        let RenderRequest {
            input,
            drain,
            gate,
            position: _,
            level: _,
            format: _,
        } = request;
        let thread = std::thread::Builder::new()
            .name("qianqian-test-render".into())
            .spawn({
                let input = input.clone();
                let gate = gate.clone();
                move || {
                    let verdict = leg_loop(&input, &gate);
                    drain.complete(verdict);
                }
            })
            .map_err(|e| OutputError {
                message: format!("test render spawn failed: {e}"),
            })?;
        Ok(Box::new(WitnessStream {
            input,
            gate,
            witness: self.outcome.clone(),
            thread: Some(thread),
        }))
    }
}

fn registered_runtime(
    source: SourceBehavior,
    witness: Arc<AtomicUsize>,
    handle: PlaybackSessionHandle,
) -> QianqianApp {
    let mut runtime = QianqianApp::new();

    runtime
        .register_component({
            let behavior = source;
            qianqian_composition::ComponentSpec::new("test_decode_plugin")
                .provides::<qianqian_audio_api::ports::PcmDecodeCapability>()
                .on_activate(move |ctx| {
                    let service = TestDecode::new(behavior);
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
            let witness = witness.clone();
            qianqian_composition::ComponentSpec::new("test_output_plugin")
                .provides::<qianqian_audio_api::ports::AudioOutputCapability>()
                .on_activate(move |ctx| {
                    let service = GateWitnessOutput {
                        outcome: witness.clone(),
                    };
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
            DesiredEntry::enabled("decode", "test_decode_plugin", Revision::new(1)),
            DesiredEntry::enabled("output", "test_output_plugin", Revision::new(1)),
            DesiredEntry::enabled("session", "playback_session", Revision::new(1)),
        ])
        .expect("composition is legal");
}

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

/// Plain positive oracle: a stop on a playing episode tears down with
/// nothing routed, so `stop_and_join` witnesses a fully released gate.
/// (This shape alone cannot detect a reordered release — no hold ever
/// exists — it pins the Green baseline of the witness itself.)
#[test]
fn stop_on_a_playing_episode_witnesses_a_fully_released_gate() {
    let _lifecycle = lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let handle = PlaybackSessionHandle::new();
        let witness = Arc::new(AtomicUsize::new(UNWITNESSED));

        let mut runtime = registered_runtime(
            // A decode side slower than the consumer: the leg genuinely
            // blocks on the data plane mid-playback and EOF can never
            // race the stop.
            SourceBehavior::Paced {
                after: 44_100,
                delay: Duration::from_millis(100),
            },
            witness.clone(),
            handle.clone(),
        );
        activate(&mut runtime);

        handle.request_stop();
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Stopped,
            "the episode settles Stopped on the stop path"
        );

        // The teardown inverse runs at discharge; the witness is
        // written during this call.
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet);

        assert_eq!(
            witness.load(Ordering::SeqCst),
            GATE_FULLY_RELEASED,
            "stop_and_join ran with session-owned render-gate work still \
             routed — the teardown precondition was violated"
        );
    });
}

/// The load-bearing pin: dispose an actually PARKED episode (paused
/// projection established — intent routed, leg engaged, tail quiesced)
/// WITHOUT stopping it first. This isolates the teardown inverse's own
/// `release_pause_gate()`: the stop path (`request_stop`) releases the
/// gate under the completion lock too, so a stop-first scenario cannot
/// detect its deletion — but a paused episode withdrawn by disposal
/// has this one release standing between a parked leg and a wedged
/// join. Deleting or reordering the release makes the witness record
/// `ROUTED_WORK_REMAINED` (bounded) and this test goes RED.
#[test]
fn disposing_a_parked_episode_witnesses_a_fully_released_gate() {
    let _lifecycle = lifecycle_lock();
    within(Duration::from_secs(10), move || {
        let handle = PlaybackSessionHandle::new();
        let witness = Arc::new(AtomicUsize::new(UNWITNESSED));

        let mut runtime = registered_runtime(
            SourceBehavior::Paced {
                after: 44_100,
                delay: Duration::from_millis(100),
            },
            witness.clone(),
            handle.clone(),
        );
        activate(&mut runtime);

        handle.request_pause();
        assert!(
            wait_until(Duration::from_secs(5), || handle.observe().paused()),
            "the Paused projection never established: {:?}",
            handle.observe()
        );

        // Withdraw the paused episode directly: no stop request, so
        // the only owner-side release is the teardown inverse's own.
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet);

        assert_eq!(
            witness.load(Ordering::SeqCst),
            GATE_FULLY_RELEASED,
            "stop_and_join ran while pause intent was still routed — the \
             owner-side gate release did not precede the join"
        );
    });
}
