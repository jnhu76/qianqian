//! Navigation-burst campaign instrument I2 (research/navigation-burst-
//! boundary-0): a timing WATERFALL of the D14.6 replacement primitives
//! on the REAL Playback Session (real kernel, real session, real edge,
//! real legs) with mechanism doubles at the ports seams.
//!
//! This decomposes ONE manual `N` transition and ONE natural-EOF
//! transition into their control-plane phases and measures each on this
//! host. It is OBSERVATION ONLY: it pins no timing contract (CI hosts
//! vary); the structural assertions alone must stay true. The device
//! leg is a double — every device-dependent number here is explicitly
//! labeled simulated; the real-host device leg is measured by the
//! campaign's Windows leg and the prior f6-open-smoke evidence.
//!
//! Phases measured (campaign §13):
//!   activate        K0 revise_desired: decode open + edge + render open
//!                   + worker spawn, through established
//!   first_submit    established → first render-leg submission
//!   first_position  established → first Position publication
//!   stop_settle     request_stop → wait_terminal (mid-play, Stopped)
//!   dispose         wait_terminal → authoritative Discharged
//!   drain_complete  natural EOF → wait_terminal (Completed)
//!
//! Two stop contexts bracket RC-C (old episode settlement): an
//! instant-consuming device (consumer parked on an EMPTY edge) and a
//! slow device (producer parked on a FULL edge).

mod common;

use std::time::{Duration, Instant};

use qianqian_app::QianqianApp;
use qianqian_audio_api::ports::PcmDecode;
use qianqian_composition::{DesiredEntry, Revision};
use qianqian_playback::{EpisodeTerminalOutcome, PlaybackSessionHandle, playback_session_spec};

use common::{OutputBehavior, SourceBehavior, TestDecode, TestOutput};

const DUMMY_PATH: &str = "test://navigation-waterfall";

fn desired(id: &str, component: &'static str) -> DesiredEntry {
    DesiredEntry::enabled(id, component, Revision::new(1))
}

struct Episode {
    runtime: QianqianApp,
    handle: PlaybackSessionHandle,
    consumed: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

/// Activate one episode on the real session with the given source and
/// device behavior, returning phase timings. This mirrors the fresh
/// half of `ReferencePlayerApp::replace_episode` (the App layer lives
/// in the apps/headless binary and is measured there by I1).
fn start_episode(
    source: SourceBehavior,
    device: OutputBehavior,
) -> (Episode, Vec<(&'static str, f64)>) {
    let t0 = Instant::now();
    let mut runtime = QianqianApp::new();
    let handle = PlaybackSessionHandle::new();
    runtime
        .register_component({
            qianqian_composition::ComponentSpec::new("test_decode_plugin")
                .provides::<qianqian_audio_api::ports::PcmDecodeCapability>()
                .on_activate(move |ctx| {
                    ctx.provide::<qianqian_audio_api::ports::PcmDecodeCapability>(
                        std::rc::Rc::new(TestDecode::new(source)),
                    )
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("decode provider registers");
    let consumed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    runtime
        .register_component({
            let consumed = consumed.clone();
            qianqian_composition::ComponentSpec::new("test_output_plugin")
                .provides::<qianqian_audio_api::ports::AudioOutputCapability>()
                .on_activate(move |ctx| {
                    let service = TestOutput::observed(device, consumed.clone());
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
            handle.clone(),
        ))
        .expect("session registers");
    let t_registered = t0.elapsed();

    runtime
        .revise_desired(vec![
            desired("decode", "test_decode_plugin"),
            desired("output", "test_output_plugin"),
            desired("session", "playback_session"),
        ])
        .expect("composition is legal");
    let t_activated = t0.elapsed();

    let established = handle.observe().source_format.is_some();
    assert!(
        established,
        "the episode established (the Open commit evidence)"
    );

    // First render-leg submission and first Position publication.
    let deadline = Instant::now() + Duration::from_secs(5);
    while consumed.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        assert!(Instant::now() < deadline, "no first submission");
        std::thread::sleep(Duration::from_micros(200));
    }
    let t_first_submit = t0.elapsed();
    let deadline = Instant::now() + Duration::from_secs(5);
    while handle.observe().position.is_none() {
        assert!(Instant::now() < deadline, "no first position publication");
        std::thread::sleep(Duration::from_micros(200));
    }
    let t_first_position = t0.elapsed();

    let phases = vec![
        ("register", t_registered.as_secs_f64() * 1000.0),
        (
            "activate",
            (t_activated - t_registered).as_secs_f64() * 1000.0,
        ),
        (
            "first_submit",
            (t_first_submit - t_activated).as_secs_f64() * 1000.0,
        ),
        (
            "first_position",
            (t_first_position - t_first_submit).as_secs_f64() * 1000.0,
        ),
    ];
    (
        Episode {
            runtime,
            handle,
            consumed,
        },
        phases,
    )
}

/// `request_stop` → `wait_terminal`, then `dispose`; both timed. The
/// retire half of `ReferencePlayerApp::retire_old_episode`.
fn retire_episode(mut episode: Episode) -> (Vec<(&'static str, f64)>, EpisodeTerminalOutcome) {
    let t0 = Instant::now();
    episode.handle.request_stop();
    let outcome = episode.handle.wait_terminal();
    let t_terminal = t0.elapsed();
    let disposal = episode.runtime.dispose();
    let t_disposed = t0.elapsed();
    assert_eq!(outcome, EpisodeTerminalOutcome::Stopped);
    assert_eq!(
        disposal.verdict,
        qianqian_composition::DisposeVerdict::Discharged
    );
    (
        vec![
            ("stop_settle", t_terminal.as_secs_f64() * 1000.0),
            ("dispose", (t_disposed - t_terminal).as_secs_f64() * 1000.0),
        ],
        outcome,
    )
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    values[values.len() / 2]
}

const RUNS: usize = 15;

#[test]
fn waterfall_manual_stop_two_edge_contexts() {
    let _guard = common::lifecycle_lock();
    for (label, device) in [
        // The consumer sits parked on an EMPTY edge when stop lands.
        ("empty_edge", OutputBehavior::Consume),
        // The producer sits parked on a FULL edge (8192 frames ≈ 185 ms
        // @ 44.1 kHz stereo) when stop lands; the device reads one
        // staging block per 20 ms.
        (
            "full_edge",
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(20),
            },
        ),
    ] {
        let mut activate = Vec::new();
        let mut first_submit = Vec::new();
        let mut first_position = Vec::new();
        let mut stop_settle = Vec::new();
        let mut dispose = Vec::new();
        for _ in 0..RUNS {
            let (episode, phases) = start_episode(
                // Infinite fast source: the worker keeps producing until
                // the edge fills (or forever against an instant reader).
                SourceBehavior::EofAfter(usize::MAX / 2),
                device,
            );
            activate.push(phases[1].1);
            first_submit.push(phases[2].1);
            first_position.push(phases[3].1);
            let (timings, _) = retire_episode(episode);
            stop_settle.push(timings[0].1);
            dispose.push(timings[1].1);
        }
        println!(
            "WATERFALL manual_stop/{label}: activate {:.3} ms | first_submit {:.3} ms | \
             first_position {:.3} ms | stop_settle {:.3} ms | dispose {:.3} ms  (medians, n={RUNS})",
            median(&mut activate),
            median(&mut first_submit),
            median(&mut first_position),
            median(&mut stop_settle),
            median(&mut dispose),
        );
    }
}

#[test]
fn waterfall_natural_eof_drain() {
    let _guard = common::lifecycle_lock();
    let mut activate = Vec::new();
    let mut first_submit = Vec::new();
    let mut produce = Vec::new();
    let mut drain_complete = Vec::new();
    // Two edge-loads of source content; the device consumes one staging
    // block per 5 ms (~4.6× real time for 44.1 kHz): fast enough for a
    // tight test, slow enough that the producer actually fills the edge
    // and the EOF drain spans several device iterations.
    let total_frames = 2 * 8192;
    let device = OutputBehavior::SlowConsume {
        per_read: Duration::from_millis(5),
    };
    for _ in 0..RUNS {
        let t0 = Instant::now();
        let (mut episode, phases) = start_episode(SourceBehavior::EofAfter(total_frames), device);
        activate.push(phases[1].1);
        first_submit.push(phases[2].1);
        // Produce: established → the decode worker hit EOF (edge closed).
        // Witnessed from the read side: every produced frame has been
        // submitted to the (mock) device — `consumed` counts FRAMES.
        let deadline = Instant::now() + Duration::from_secs(5);
        while episode.consumed.load(std::sync::atomic::Ordering::SeqCst) < total_frames {
            assert!(Instant::now() < deadline, "source never fully produced");
            std::thread::sleep(Duration::from_micros(300));
        }
        let t_produced = t0.elapsed();
        // Total elapsed minus the register/activate phases already
        // counted: established → the worker's EOF production is done.
        produce.push(t_produced.as_secs_f64() * 1000.0 - phases[0].1 - phases[1].1);
        // The natural-EOF drain: everything produced must be submitted
        // AND the (simulated) device tail must reach zero before the
        // session commits Completed — `wait_terminal`, never polled by
        // the policy path.
        let t_drain0 = Instant::now();
        let outcome = episode.handle.wait_terminal();
        let t_done = t_drain0.elapsed();
        assert_eq!(outcome, EpisodeTerminalOutcome::Completed);
        drain_complete.push(t_done.as_secs_f64() * 1000.0);
        let disposal = episode.runtime.dispose();
        assert_eq!(
            disposal.verdict,
            qianqian_composition::DisposeVerdict::Discharged
        );
    }
    println!(
        "WATERFALL natural_eof: activate {:.3} ms | first_submit {:.3} ms | \
         produce_to_eof {:.3} ms | drain_to_Completed {:.3} ms  (medians, n={RUNS})",
        median(&mut activate),
        median(&mut first_submit),
        median(&mut produce),
        median(&mut drain_complete),
    );
}

/// The probe (D14.6 step 1) on the real decode provider surface: on
/// this host, against the synthetic provider, it is a pure in-process
/// query. Recorded so the burst ledger shows the probe step was not
/// skipped; the REAL SongCore probe cost on Windows media comes from
/// the campaign's Windows leg and `experiments/f6-source-probe`.
#[test]
fn waterfall_probe_step_on_synth_provider() {
    let decode = TestDecode::new(SourceBehavior::EofAfter(8));
    let runs = 1000;
    let t0 = Instant::now();
    for _ in 0..runs {
        let stream = decode
            .open_media(std::path::Path::new(DUMMY_PATH))
            .expect("synthetic source opens");
        drop(stream);
    }
    println!(
        "WATERFALL probe/open+close (synthetic provider): {:.4} ms/op  (n={runs})",
        t0.elapsed().as_secs_f64() * 1000.0 / runs as f64,
    );
}
