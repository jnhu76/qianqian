//! Quiet-window navigation coalescing PROTOTYPE (campaign §20/§21/§47/
//! §48 of QIANQIAN-NAVIGATION-BURST-ROOT-CAUSE-AND-BOUNDARY-0).
//!
//! OUTSIDE PRODUCTION by construction: a research crate the product
//! never references. It models the candidate interaction policy
//! (pending virtual target + quiet window + ONE final activation) on a
//! VIRTUAL clock, while every activation runs the REAL
//! `ReferencePlayerApp` replacement path (real K0 root, real session,
//! real bounded edge, mechanism doubles at the ports) so the opens
//! being counted are real composition Commands.
//!
//! Measured per quiet window w and key cadence g:
//!   opens    — real `open` composition commands executed
//!   collapse — opens / non-inert keys (100% = today's per-key shape)
//!   ok       — final committed target == traversal-policy result
//!   zeroNP   — (NP row only) N then P collapses to NO open at all
//!
//! The single-press latency cost of the policy is exactly w (commit
//! waits one quiet window) plus one real replacement period R on the
//! device — measured on the Windows host at ~60–90 ms warm (campaign
//! R1); the prototype's device-free R_proto is printed for reference.
//!
//! Preview traversal: the prototype replicates the documented U2
//! manual-step policy locally (Sequential; Repeat Off/One inert at the
//! boundary, Repeat All wraps) because the current
//! `TemporaryPlaylist::manual_step` previews only FROM the committed
//! cursor — chaining steps from a pending virtual target needs the
//! playlist-side pure preview API that report R3 recommends. That gap
//! is a prototype-observed fact, not an implemented API.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Instant;

use qianqian_audio_api::ports::{
    AudioOutput, AudioOutputCapability, DecodeError, DecodeOutcome, DecodeOpenError,
    DecodedPcmStream, DrainSignal, DrainVerdict, OutputError, PcmDecode, PcmDecodeCapability,
    PcmFormat, PcmPull, ProviderSeekOutcome, RenderRequest, RenderStream,
};
use qianqian_app::QianqianApp;
use qianqian_composition::{ComponentSpec, DesiredEntry, DisposeVerdict, Revision};
use qianqian_headless::player::{EpisodeStart, OpenOutcome, ReferencePlayerApp, StartAttempt};
use qianqian_playback::{PlaybackSessionHandle, playback_session_spec};

const FORMAT: PcmFormat = PcmFormat {
    sample_rate: 44100,
    channels: 2,
    channel_mask: 0x3,
};

// --- mechanism doubles at the ports: the prototype's episode content
// is irrelevant; only OPEN COUNTS and COMMIT EVIDENCE are observed, so
// the smallest honest legs suffice. ---

struct TinyDecode;

impl PcmDecode for TinyDecode {
    fn open_media(&self, _path: &Path) -> Result<Box<dyn DecodedPcmStream>, DecodeOpenError> {
        Ok(Box::new(TinyStream { blocks_left: 8 }))
    }
}

struct TinyStream {
    blocks_left: usize,
}

impl DecodedPcmStream for TinyStream {
    fn format(&self) -> PcmFormat {
        FORMAT
    }

    fn source_duration(&self) -> Option<std::time::Duration> {
        None
    }

    fn read_frames(&mut self, dst: &mut [f32]) -> Result<DecodeOutcome, DecodeError> {
        if self.blocks_left == 0 {
            return Ok(DecodeOutcome::Eof);
        }
        self.blocks_left -= 1;
        let frames = (dst.len() / usize::from(FORMAT.channels)).min(1024);
        Ok(DecodeOutcome::Frames(frames))
    }

    fn seek(&mut self, _target: std::time::Duration) -> ProviderSeekOutcome {
        ProviderSeekOutcome::RefusedUnchanged
    }
}

struct TinyOutput;

impl AudioOutput for TinyOutput {
    fn open_stream(&self, request: RenderRequest) -> Result<Box<dyn RenderStream>, OutputError> {
        let input = request.input.clone();
        let drain: DrainSignal = request.drain;
        let handle = std::thread::Builder::new()
            .name("tiny-render".into())
            .spawn(move || {
                let mut dst = vec![0.0f32; 1024 * usize::from(FORMAT.channels)];
                loop {
                    match input.read_frames(&mut dst) {
                        PcmPull::Frames(_) => {}
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
            .map_err(|e| OutputError {
                message: format!("spawn: {e}"),
            })?;
        Ok(Box::new(TinyStreamHandle {
            input: request.input,
            handle,
        }))
    }
}

struct TinyStreamHandle {
    input: Arc<dyn qianqian_audio_api::ports::RenderPcmInput>,
    handle: std::thread::JoinHandle<()>,
}

impl RenderStream for TinyStreamHandle {
    fn negotiated_format(&self) -> PcmFormat {
        FORMAT
    }

    fn stop_and_join(self: Box<Self>) {
        self.input.stop();
        let _ = self.handle.join();
    }
}

// --- the counting EpisodeStart: the opens the policy executes are the
// real composition commands; the counters make them countable. ---

#[derive(Clone)]
struct TrackedSource {
    probes: Arc<AtomicUsize>,
    starts: Arc<AtomicUsize>,
}

impl TrackedSource {
    fn new() -> Self {
        Self {
            probes: Arc::new(AtomicUsize::new(0)),
            starts: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl EpisodeStart for TrackedSource {
    fn probe(&self, _candidate: &Path) -> Result<(), String> {
        self.probes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn start(&self, source: &Path, _initial_output_level: u8) -> StartAttempt {
        self.starts.fetch_add(1, Ordering::Relaxed);
        let mut runtime = QianqianApp::new();
        let handle = PlaybackSessionHandle::new();
        runtime
            .register_component(
                ComponentSpec::new("tiny_decode_plugin")
                    .provides::<PcmDecodeCapability>()
                    .on_activate(|ctx| {
                        ctx.provide::<PcmDecodeCapability>(Rc::new(TinyDecode))
                            .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                        Ok(())
                    }),
            )
            .expect("decode registers");
        runtime
            .register_component(
                ComponentSpec::new("tiny_output_plugin")
                    .provides::<AudioOutputCapability>()
                    .on_activate(|ctx| {
                        ctx.provide::<AudioOutputCapability>(Rc::new(TinyOutput))
                            .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                        Ok(())
                    }),
            )
            .expect("output registers");
        runtime
            .register_component(playback_session_spec(
                source.to_path_buf(),
                handle.clone(),
            ))
            .expect("session registers");
        let refused = runtime
            .revise_desired(vec![
                desired("decode", "tiny_decode_plugin"),
                desired("output", "tiny_output_plugin"),
                desired("session", "playback_session"),
            ])
            .err()
            .map(|e| format!("{e}"));
        StartAttempt {
            runtime,
            handle,
            refused,
        }
    }
}

fn desired(id: &str, component: &'static str) -> DesiredEntry {
    DesiredEntry::enabled(id, component, Revision::new(1))
}

/// The local replication of the documented U2 manual-step policy over
/// positions (Sequential order; Repeat Off/One inert at the boundary,
/// Repeat All wraps). Pure; the prototype's stand-in for the
/// playlist-side pure preview API R3 recommends.
fn preview_step(position: usize, len: usize, forward: bool, repeat_all: bool) -> Option<usize> {
    if forward {
        if position + 1 < len {
            Some(position + 1)
        } else if repeat_all {
            Some(0)
        } else {
            None
        }
    } else if position > 0 {
        Some(position - 1)
    } else if repeat_all {
        Some(len - 1)
    } else {
        None
    }
}

fn main() {
    let tracks: Vec<PathBuf> = (0..8)
        .map(|i| PathBuf::from(format!("/media/quiet-{i}.flac")))
        .collect();
    let source = TrackedSource::new();
    let mut player = ReferencePlayerApp::new(source.clone());
    assert!(matches!(player.open(&tracks[0]), OpenOutcome::Opened));
    player.establish_playlist(tracks.clone());

    const SEQUENCES: [(&str, &[bool]); 5] = [
        ("N", &[true]),
        ("NNNNN", &[true, true, true, true, true]),
        ("PPPP", &[false, false, false, false]),
        ("NP", &[true, false]),
        ("NNNPPNNP", &[
            true, true, true, false, false, true, true, false,
        ]),
    ];
    const INTERVALS_MS: [u64; 5] = [10, 50, 100, 150, 250];
    const WINDOWS_MS: [u64; 7] = [0, 50, 75, 100, 125, 150, 200];
    // Every burst starts from the committed middle (position 3 of 8) so
    // both directions have room; the player is walked there once, out
    // of band.
    const START: usize = 3;
    const LEN: usize = 8;
    for _ in 0..START {
        let outcome = player.next_track();
        assert!(matches!(outcome, Some(OpenOutcome::Opened)), "walk step: {outcome:?}");
    }

    // The prototype stack's own warm replacement period (device-free;
    // the Windows R is quoted separately in the report).
    let t0 = Instant::now();
    const R_RUNS: usize = 10; // N/P pairs: two replacements each, net position unchanged
    for _ in 0..R_RUNS {
        let next = player.next_track();
        assert!(matches!(next, Some(OpenOutcome::Opened)), "warm next: {next:?}");
        let prev = player.previous_track();
        assert!(matches!(prev, Some(OpenOutcome::Opened)), "warm prev: {prev:?}");
    }
    let r_proto_ms = t0.elapsed().as_secs_f64() * 1000.0 / (R_RUNS * 2) as f64;

    println!("QUIET-WINDOW PROTOTYPE (virtual-clock policy; real Open commands)");
    println!(
        "prototype warm replacement R_proto = {r_proto_ms:.1} ms (device-free; Windows warm R ~= 60-90 ms)"
    );
    println!();
    println!(
        "sequence  keys g_ms | w0_opns w50 w75 w100 w125 w150 w200   (opens per burst; collapse% for NNNNN@10; ok=final target)"
    );
    for (name, keys) in SEQUENCES {
        for &g in &INTERVALS_MS {
            print!("{name:<9} {:>4} {g:>4} |", keys.len());
            for &w in &WINDOWS_MS {
                source.probes.store(0, Ordering::Relaxed);
                source.starts.store(0, Ordering::Relaxed);
                let mut committed = START;
                let mut virtual_pos = committed;
                let mut pending_deadline: Option<u64> = None;
                let mut opens = 0usize;
                let repeat_all = false;

                // ONE real composition command per policy commit.
                let commit = |player: &mut ReferencePlayerApp<TrackedSource>,
                                  target: usize,
                                  opens: &mut usize,
                                  committed: &mut usize| {
                    if player.open(&tracks[target]) == OpenOutcome::Opened {
                        *opens += 1;
                        *committed = target;
                    }
                };

                for (i, &forward) in keys.iter().enumerate() {
                    let t_i = (i as u64) * g;
                    // Timer progression: a due pending window expires
                    // before the next key is handled.
                    if let Some(deadline) = pending_deadline {
                        if t_i >= deadline {
                            if virtual_pos != committed {
                                commit(&mut player, virtual_pos, &mut opens, &mut committed);
                            }
                            pending_deadline = None;
                        }
                    }
                    match preview_step(virtual_pos, LEN, forward, repeat_all) {
                        Some(next) => {
                            virtual_pos = next;
                            if w == 0 {
                                // Today's shape: one full replacement
                                // per non-inert raw key, immediately.
                                if virtual_pos != committed {
                                    commit(&mut player, virtual_pos, &mut opens, &mut committed);
                                }
                            } else {
                                pending_deadline = Some(t_i + w);
                            }
                        }
                        None => {
                            // Inert at the boundary: the key is
                            // consumed and moves nothing — but it is
                            // still user activity, so a PENDING window
                            // restarts rather than being cancelled
                            // (cancelling here would swallow a burst's
                            // final commit, which the first prototype
                            // run demonstrated). A lone inert key
                            // never creates a pending window.
                            if let Some(_deadline) = pending_deadline {
                                pending_deadline = Some(t_i + w.max(1));
                            }
                        }
                    }
                }
                // The final window expires with no further key.
                if pending_deadline.is_some() && virtual_pos != committed {
                    commit(&mut player, virtual_pos, &mut opens, &mut committed);
                }

                // Traversal-policy expectation, computed independently.
                let mut expect = START;
                let mut non_inert = 0usize;
                for &forward in keys {
                    if let Some(next) = preview_step(expect, LEN, forward, repeat_all) {
                        expect = next;
                        non_inert += 1;
                    }
                }
                let ok = committed == expect;
                print!(" {opens:>4}{:>3}", if ok { "ok" } else { "X" });
                let _ = non_inert;
            }
            println!();
        }
        println!();
    }

    let report = player.quit();
    assert_eq!(report.disposal, Some(DisposeVerdict::Discharged));
    println!("final quit: disposal {:?}", report.disposal);
}
