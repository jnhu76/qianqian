//! D14.9 volume seam tests: the App's desired output level routes
//! through the episode seam's idempotent command into the session-owned
//! output-level cell, and the mechanism holds that cell and reads it on
//! its own path. The volume command is application configuration in
//! transit — it never establishes or settles terminal truth, and the
//! level is clamped at the seam (`0..=100`).

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use qianqian_audio_api::ports::{
    AudioOutput, AudioOutputCapability, DrainSignal, DrainVerdict, GateSlice, OutputError,
    OutputLevel, ParkOutcome, PcmFormat, PcmPull, RenderRequest, RenderStream, TailProbeOutcome,
};
use qianqian_composition::ComponentSpec;
use qianqian_playback::{EpisodeTerminalOutcome, PlaybackSessionHandle, playback_session_spec};

mod common;

use common::SourceBehavior;

const FORMAT: PcmFormat = PcmFormat {
    sample_rate: 44_100,
    channels: 2,
    channel_mask: 0x3,
};

/// A minimal capture-and-pump output: records the RenderRequest's
/// output-level cell (the SAME cell the real mechanism would read) and
/// drains the edge on a pump thread, exactly the observable shape the
/// real stream has.
struct CapturingOutput {
    captured: Arc<Mutex<Option<OutputLevel>>>,
}

impl AudioOutput for CapturingOutput {
    fn open_stream(&self, request: RenderRequest) -> Result<Box<dyn RenderStream>, OutputError> {
        *self.captured.lock().expect("captured level") = Some(request.level.clone());
        let input = request.input.clone();
        let drain: DrainSignal = request.drain.clone();
        let gate = request.gate;
        let channels = usize::from(request.format.channels);
        let join = std::thread::Builder::new()
            .name("volume-seam-pump".into())
            .spawn(move || {
                let mut dst = vec![0.0f32; 256 * channels];
                loop {
                    if matches!(
                        gate.park_loop_top(|slice| match slice {
                            GateSlice::TailProbe => TailProbeOutcome::Quiesced,
                            GateSlice::SeekRelease(_) => TailProbeOutcome::Quiesced,
                        }),
                        ParkOutcome::TailProbeFailed
                    ) {
                        drain.complete(DrainVerdict::Aborted);
                        return;
                    }
                    match input.read_frames(&mut dst) {
                        PcmPull::Frames(_) => std::thread::sleep(Duration::from_millis(1)),
                        PcmPull::Eof => {
                            drain.complete(DrainVerdict::Drained);
                            return;
                        }
                        PcmPull::Stopped => {
                            drain.complete(DrainVerdict::Aborted);
                            return;
                        }
                    }
                }
            })
            .expect("pump spawn");
        let _ = join;
        Ok(Box::new(CaptureStream))
    }
}

/// The acquired stream: stop → join happens on the session side through
/// `input.stop()`; this handle contributes nothing further.
struct CaptureStream;

impl RenderStream for CaptureStream {
    fn negotiated_format(&self) -> PcmFormat {
        FORMAT
    }
    fn stop_and_join(self: Box<Self>) {}
}

/// A live episode over the real session with the capturing output.
/// Returns (handle, runtime, captured-level slot).
fn live_episode() -> (
    PlaybackSessionHandle,
    qianqian_app::QianqianApp,
    Arc<Mutex<Option<OutputLevel>>>,
) {
    let _lifecycle = common::lifecycle_lock();
    let captured = Arc::new(Mutex::new(None));
    let mut runtime = qianqian_app::QianqianApp::new();
    runtime
        .register_component({
            let behavior = SourceBehavior::EofAfter(400_000_000); // effectively endless at test scale
            ComponentSpec::new("test_decode_plugin")
                .provides::<qianqian_audio_api::ports::PcmDecodeCapability>()
                .on_activate(move |ctx| {
                    ctx.provide::<qianqian_audio_api::ports::PcmDecodeCapability>(
                        std::rc::Rc::new(common::TestDecode::new(behavior)),
                    )
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("decode provider registers");
    let slot = captured.clone();
    runtime
        .register_component(
            ComponentSpec::new("test_output_plugin")
                .provides::<AudioOutputCapability>()
                .on_activate(move |ctx| {
                    let slot = slot.clone();
                    ctx.provide::<AudioOutputCapability>(std::rc::Rc::new(CapturingOutput {
                        captured: slot,
                    }))
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                }),
        )
        .expect("output provider registers");
    let handle = PlaybackSessionHandle::new();
    runtime
        .register_component(playback_session_spec(
            std::path::PathBuf::from("test://volume-seam"),
            handle.clone(),
        ))
        .expect("session registers");
    runtime
        .revise_desired(vec![
            qianqian_composition::DesiredEntry::enabled(
                "decode",
                "test_decode_plugin",
                qianqian_composition::Revision::new(1),
            ),
            qianqian_composition::DesiredEntry::enabled(
                "output",
                "test_output_plugin",
                qianqian_composition::Revision::new(1),
            ),
            qianqian_composition::DesiredEntry::enabled(
                "session",
                "playback_session",
                qianqian_composition::Revision::new(1),
            ),
        ])
        .expect("composition is legal");
    let _ = &_lifecycle;
    (handle, runtime, captured)
}

/// The volume command routes the desired factor into the SAME cell the
/// mechanism received at open, and it NEVER settles terminal truth.
#[test]
fn volume_command_reaches_the_mechanism_cell_and_never_settles() {
    let (handle, mut runtime, captured) = live_episode();
    let observation = handle.observe();
    assert!(
        observation.source_format.is_some(),
        "the episode established"
    );

    handle.request_output_level(40);
    // Give the (none) mechanism a moment; the CELL is the contract and
    // it reads back the routed factor deterministically. The factor is
    // the perceptual taper's realization of 40 (10^(-0.03*60), −36 dB)
    // — the mapping itself is pinned by `desired_level_tests` in the
    // handle module; this test owns the ROUTING.
    let level = captured
        .lock()
        .expect("captured")
        .as_ref()
        .expect("the mechanism received the level cell")
        .load();
    let expect = 10f32.powf(-0.03 * 60.0);
    assert!(
        (level - expect).abs() <= 0.01,
        "routed 40 ⇒ taper factor {expect}, got {level}"
    );

    // Non-terminal by frozen definition: no stop intent, no Fact.
    let observation = handle.observe();
    assert_eq!(observation.terminal_outcome, None);
    assert!(!observation.stop_requested);

    // Idempotent command: routing the same value again changes nothing
    // about truth.
    handle.request_output_level(40);
    assert_eq!(handle.observe().terminal_outcome, None);

    handle.request_stop();
    assert_eq!(
        common::within(Duration::from_secs(10), {
            let handle = handle.clone();
            move || handle.wait_terminal()
        }),
        EpisodeTerminalOutcome::Stopped
    );
    let snapshot = runtime.dispose().snapshot;
    assert!(snapshot.quiet);
}

/// The seam clamps the level to 0..=100 (0 ⇒ silence factor, values
/// above 100 ⇒ unity).
#[test]
fn the_volume_level_clamps_at_the_seam() {
    let (handle, mut runtime, captured) = live_episode();
    assert!(handle.observe().source_format.is_some());
    let cell = captured
        .lock()
        .expect("captured")
        .clone()
        .expect("the mechanism received the level cell");

    handle.request_output_level(0);
    assert!((cell.load() - 0.0).abs() <= 0.001, "0 ⇒ silence");

    handle.request_output_level(255);
    assert!((cell.load() - 1.0).abs() <= 0.001, "255 clamps to unity");

    let snapshot = runtime.dispose().snapshot;
    assert!(snapshot.quiet);
}

/// A volume command after the terminal Fact is inert command history:
/// it settles nothing, forges nothing, and leaves the observation
/// exactly as it was.
#[test]
fn a_volume_command_after_the_terminal_fact_is_inert_history() {
    let (handle, mut runtime, captured) = live_episode();
    let cell = captured
        .lock()
        .expect("captured")
        .clone()
        .expect("the mechanism received the level cell");
    handle.request_output_level(70);

    handle.request_stop();
    assert_eq!(
        common::within(Duration::from_secs(10), {
            let handle = handle.clone();
            move || handle.wait_terminal()
        }),
        EpisodeTerminalOutcome::Stopped
    );
    let settled = handle.observe();
    assert_eq!(
        settled.terminal_outcome,
        Some(EpisodeTerminalOutcome::Stopped)
    );
    assert!(
        (cell.load() - 10f32.powf(-0.9)).abs() <= 0.01,
        "the pre-settle route holds in the cell (70 ⇒ −27 dB taper factor)"
    );

    // Late volume: the cell still routes (inert history the mechanism
    // will never read again), and the settled truth is untouched —
    // the whole observation is identical to the settled one.
    handle.request_output_level(10);
    assert!(
        (cell.load() - 10f32.powf(-2.7)).abs() <= 0.01,
        "the late route lands in the cell (10 ⇒ −81 dB taper factor)"
    );
    let after = handle.observe();
    assert_eq!(after, settled, "a late volume command changes no truth");

    let snapshot = runtime.dispose().snapshot;
    assert!(snapshot.quiet);
}
