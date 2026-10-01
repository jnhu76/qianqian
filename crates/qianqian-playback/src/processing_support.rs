//! Shared harness for the in-crate Audio Processing oracles (Issue #177
//! Stage 2; ADR-PBK-002 D14.11). One copy of the episode builders and
//! the exact-content oracles, used by `gain_tests` (I1, config-carrying
//! production Gain) and `stateful_probe_tests` (I2, deliberate stateful
//! test processors through the real composition). Test-only: compiled
//! only in this crate's test build, never shipped.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use qianqian_app::QianqianApp;
use qianqian_audio_api::ports::ProviderSeekOutcome;
use qianqian_composition::{DesiredEntry, Revision};

use crate::handle::PlaybackSessionHandle;
use crate::processing::{AudioProcessingConfig, EpisodeProcessing};
use crate::test_common::{self, OutputBehavior, SourceBehavior, TestDecode, TestOutput};

pub(crate) const DUMMY_PATH: &str = "test://audio-processing";

/// The test format's sample rate (frames of source per second).
pub(crate) const TEST_RATE: usize = 44_100;

/// An 8-second source — the F5 seek-matrix shape: long enough that a
/// seek fired in the first second is decisively mid-stream (the bounded
/// edge is full and the worker sits inside its interruptible write), and
/// the slow mock consumer still finishes it in seconds.
pub(crate) const EIGHT_SECONDS: usize = TEST_RATE * 8;

pub(crate) struct Witnesses {
    consumed: Arc<AtomicUsize>,
    consumed_values: Arc<Mutex<Vec<f32>>>,
    consumed_values_ch1: Arc<Mutex<Vec<f32>>>,
}

impl Witnesses {
    pub(crate) fn new() -> Self {
        Self {
            consumed: Arc::new(AtomicUsize::new(0)),
            consumed_values: Arc::new(Mutex::new(Vec::new())),
            consumed_values_ch1: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The consumed channel-0 content, snapped once: the value of every
    /// frame the render leg submitted, in submission order.
    pub(crate) fn content(&self) -> Vec<f32> {
        self.consumed_values.lock().unwrap().clone()
    }

    /// The consumed channel-1 content (tag + 0.5 per the decode double's
    /// channel-distinct tagging) — the second observable axis (I1 review
    /// P2-1): per-channel processing misapplication fails here even when
    /// channel 0 looks right.
    pub(crate) fn content_ch1(&self) -> Vec<f32> {
        self.consumed_values_ch1.lock().unwrap().clone()
    }

    /// The consumed frame count.
    pub(crate) fn consumed(&self) -> usize {
        self.consumed.load(Ordering::SeqCst)
    }
}

pub(crate) fn desired(id: &str, component: &'static str) -> DesiredEntry {
    DesiredEntry::enabled(id, component, Revision::new(1))
}

/// Register the standard episode with a DESIRED Audio Processing
/// configuration: a decode double, an output double sharing the caller's
/// witnesses, and the real session carrying the config (the product
/// establishment path).
pub(crate) fn registered_runtime(
    decode: TestDecode,
    output: OutputBehavior,
    processing: AudioProcessingConfig,
    witnesses: &Witnesses,
    handle: PlaybackSessionHandle,
) -> QianqianApp {
    registered_runtime_with(
        decode,
        output,
        witnesses,
        handle,
        move |runtime: &mut QianqianApp, handle| {
            runtime.register_component(crate::playback_session_spec_with_processing(
                std::path::PathBuf::from(DUMMY_PATH),
                handle,
                processing,
            ))
        },
    )
}

/// Register the standard episode with a DELIBERATE test-only processing
/// runtime (I2): the same composition, with the white-box
/// `playback_session_spec_with_test_processor` establishment — the real
/// activation path, real legs, real seek/pause protocol, only the
/// processing instance replaced.
pub(crate) fn registered_runtime_with_test_processor(
    decode: TestDecode,
    output: OutputBehavior,
    processing: EpisodeProcessing,
    witnesses: &Witnesses,
    handle: PlaybackSessionHandle,
) -> QianqianApp {
    registered_runtime_with(
        decode,
        output,
        witnesses,
        handle,
        move |runtime: &mut QianqianApp, handle| {
            runtime.register_component(crate::session::playback_session_spec_with_test_processor(
                std::path::PathBuf::from(DUMMY_PATH),
                handle,
                processing,
            ))
        },
    )
}

fn registered_runtime_with(
    decode: TestDecode,
    output: OutputBehavior,
    witnesses: &Witnesses,
    handle: PlaybackSessionHandle,
    register_session: impl FnOnce(
        &mut QianqianApp,
        PlaybackSessionHandle,
    ) -> Result<(), qianqian_composition::ComponentRegistrationError>,
) -> QianqianApp {
    let consumed = witnesses.consumed.clone();
    let consumed_values = witnesses.consumed_values.clone();
    let consumed_values_ch1 = witnesses.consumed_values_ch1.clone();
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
                    ctx.provide::<qianqian_audio_api::ports::AudioOutputCapability>(
                        std::rc::Rc::new(TestOutput::observed_with_stereo_content(
                            output,
                            consumed.clone(),
                            consumed_values.clone(),
                            consumed_values_ch1.clone(),
                            test_common::DeviceTail::default(),
                        )),
                    )
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("output provider registers");

    register_session(&mut runtime, handle.clone()).expect("session registers");
    runtime
}

/// The standard config-carrying episode: a position-tagged source of
/// `source_frames` frames played by the given output double, with the
/// standard witness set and the given desired Audio Processing
/// configuration.
pub(crate) fn episode(
    source_frames: usize,
    output: OutputBehavior,
    processing: AudioProcessingConfig,
    seeks: Vec<ProviderSeekOutcome>,
) -> (Witnesses, PlaybackSessionHandle, QianqianApp) {
    let witnesses = Witnesses::new();
    let handle = PlaybackSessionHandle::new();
    let mut runtime = registered_runtime(
        TestDecode {
            behavior: SourceBehavior::EofAfter(source_frames),
            duration: None,
            seeks,
        },
        output,
        processing,
        &witnesses,
        handle.clone(),
    );
    runtime
        .revise_desired(vec![
            desired("decode", "test_decode_plugin"),
            desired("output", "test_output_plugin"),
            desired("session", "playback_session"),
        ])
        .expect("composition is legal");
    (witnesses, handle, runtime)
}

/// The standard test-processor episode: the same shape, with a
/// deliberate processing runtime in place of the config-compiled one.
pub(crate) fn episode_with_test_processor(
    source_frames: usize,
    output: OutputBehavior,
    processing: EpisodeProcessing,
    seeks: Vec<ProviderSeekOutcome>,
) -> (Witnesses, PlaybackSessionHandle, QianqianApp) {
    let handle = PlaybackSessionHandle::new();
    let (witnesses, runtime) = episode_with_test_processor_and_handle(
        source_frames,
        output,
        processing,
        seeks,
        handle.clone(),
    );
    (witnesses, handle, runtime)
}

/// [`episode_with_test_processor`] with the CALLER owning the handle,
/// so an in-crate oracle can read crate-internal diagnostics through it
/// (`completion.buffered_frames()` — the edge-occupancy witness that
/// pins seek geometry).
pub(crate) fn episode_with_test_processor_and_handle(
    source_frames: usize,
    output: OutputBehavior,
    processing: EpisodeProcessing,
    seeks: Vec<ProviderSeekOutcome>,
    handle: PlaybackSessionHandle,
) -> (Witnesses, QianqianApp) {
    let witnesses = Witnesses::new();
    let mut runtime = registered_runtime_with_test_processor(
        TestDecode {
            behavior: SourceBehavior::EofAfter(source_frames),
            duration: None,
            seeks,
        },
        output,
        processing,
        &witnesses,
        handle.clone(),
    );
    runtime
        .revise_desired(vec![
            desired("decode", "test_decode_plugin"),
            desired("output", "test_output_plugin"),
            desired("session", "playback_session"),
        ])
        .expect("composition is legal");
    (witnesses, runtime)
}

/// Bounded poll for an asynchronously-published observation.
pub(crate) fn wait_until(limit: Duration, mut predicate: impl FnMut() -> bool) -> bool {
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

/// The exact processed-content oracle for one channel: one output frame
/// per source frame (frame conservation — no insertion, no drop, no EOF
/// tail), each exactly `(frame_index + tag_offset) × factor` (see the
/// gain_tests module-doc exactness argument; no float tolerance is
/// needed or allowed there). Channel 0's tag offset is 0; channel 1's
/// is 0.5. Stateful oracles pass their own expectations instead.
pub(crate) fn assert_processed_exactly_at(
    values: &[f32],
    tag_offset: f32,
    factor: f32,
    source_frames: usize,
    channel: &'static str,
) {
    assert_eq!(
        values.len(),
        source_frames,
        "frame conservation ({channel}): N decoded frames in, N consumed \
         frames out (an EOF tail or a dropped/duplicated frame breaks this)"
    );
    for (i, value) in values.iter().enumerate() {
        assert_eq!(
            *value,
            (i as f32 + tag_offset) * factor,
            "frame {i} of {channel} must be exactly (frame index + \
             {tag_offset}) × {factor}"
        );
    }
}

/// The stereo scalar-gain oracle: both witnessed channels must carry the
/// exact processed content. A seam that applied processing to only one
/// channel of each frame fails here.
pub(crate) fn assert_stereo_processed_exactly(
    ch0: &[f32],
    ch1: &[f32],
    factor: f32,
    source_frames: usize,
) {
    assert_processed_exactly_at(ch0, 0.0, factor, source_frames, "channel 0");
    assert_processed_exactly_at(ch1, 0.5, factor, source_frames, "channel 1");
}

/// Indices where the scaled position-tagged sequence does not continue
/// the previous frame index (`b != a + factor`; scaled frame indices
/// stay far below the f32-exact range). Works for either channel: the
/// tag offset shifts both neighbors equally.
pub(crate) fn discontinuities(values: &[f32], factor: f32) -> Vec<usize> {
    values
        .iter()
        .zip(values.iter().skip(1))
        .enumerate()
        .filter_map(|(i, (a, b))| (*b != *a + factor).then_some(i + 1))
        .collect()
}

/// Run `f` and report whether it panicked — the negative-control shape:
/// an oracle MUST be able to fail, and these controls prove it does on
/// the exact defect signature.
pub(crate) fn rejects(f: impl FnOnce()) -> bool {
    catch_unwind(AssertUnwindSafe(f)).is_err()
}
