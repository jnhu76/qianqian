//! F6-S-PROBE harness — the load-bearing question:
//!
//! ```text
//! Can an OLD live episode keep decoding/rendering normally
//! while a SECOND SongCore native handle probes a candidate media source?
//! ```
//!
//! Evidence only (no production feature work): the harness starts ONE
//! live playback episode through the production wiring exactly as
//! `apps/headless/src/main.rs start_episode` builds it, then drives a
//! second SongCore native handle over the raw ABI — open → probe →
//! close, no PCM reads, no render stream, no output, no episode —
//! while sampling the old episode's D14.2 observation seam.
//!
//! The probe mirrors the production open path
//! (`crates/qianqian-decode-songcore/src/lib.rs`,
//! `SongcoreDecodeStream::open`) through `song_probe` and stops there;
//! it is temporary source-mechanism evidence only.
//!
//! Scenarios (one process per invocation):
//! ```text
//! S1  live × valid MP3 probe
//! S2  live × valid lossless (FLAC) probe
//! S3  live × corrupt/invalid candidate
//! S4  live × repeated probe/open/drop cycles
//! S5  paused current episode × candidate probe
//! S6  current episode around seek activity × probe
//! S7  several failed candidate probes → old keeps playing
//! S8  successful probe held, then dropped → old keeps playing
//! S9  repeated alternating valid/invalid probes
//! S10 two/three SongCore handles alive concurrently
//! NEG dead-episode negative control: the same schedule with the old
//!     episode stopped at window start — the advance oracle MUST fire
//!     (proves the harness detects a non-rendering episode)
//! ```
//!
//! Old-episode evidence per scenario: Position progression (D14.8
//! projection samples), terminal truth (D11 via wait_terminal),
//! failure/activation diagnostics, dispose quietness. The audible
//! continuity property stays a human-ear item for review, exactly like
//! the F5 physical smoke; what this harness proves mechanically is
//! that the old render leg keeps consuming (Position advances at the
//! source rate), never settles Failed, and the probe verdicts stay
//! correct — plus process stability (clean exit).
//!
//! Output: one JSON document on stdout; a human summary on stderr.
//! Exit code 0 iff the run matched its expectation (including NEG's
//! expected RED).

use std::io::{Read, Seek, SeekFrom};
use std::os::raw::c_void;
use std::path::{Path, PathBuf};
use std::slice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use qianqian_composition::{DesiredEntry, FiberState, Revision};
use qianqian_playback::PlaybackSessionHandle;
use qianqian_songcore_sys as sys;

// --- scenario schedule ----------------------------------------------------

const SAMPLE_INTERVAL: Duration = Duration::from_millis(200);
const WARMUP: Duration = Duration::from_millis(1200);
const AFTER_WINDOW: Duration = Duration::from_millis(400);

/// Position advance floor over the playing window: measured frames must
/// reach this fraction of `sample_rate × elapsed`. Not a freshness
/// claim — a coarse liveness bound, loose enough for scheduler noise
/// and tight enough that a frozen/withdrawn position (the NEG shape)
/// cannot pass.
const ADVANCE_FRACTION: f64 = 0.7;
/// While the Paused projection is established, the position may still
/// climb by at most this many frames (the pre-engagement tail, bounded
/// by the edge capacity ~185 ms plus margin).
const PAUSE_FREEZE_TOLERANCE_FRAMES: u64 = 16384;
/// A position regression is legal only within this window after a seek
/// command (the rebase to the actual landing).
const SEEK_REGRESSION_WINDOW_MS: u128 = 2500;

#[derive(Clone, Debug)]
enum Step {
    Sleep(Duration),
    /// Probe a candidate; `expect_valid` is the oracle.
    Probe {
        cand: usize,
        expect_valid: bool,
    },
    /// Open → probe → hold → close (one full open/drop cycle with a
    /// held interval).
    ProbeHold {
        cand: usize,
        hold: Duration,
        expect_valid: bool,
    },
    /// Open a handle and keep it alive until [`Step::CloseHeld`].
    OpenHold {
        cand: usize,
        expect_valid: bool,
    },
    /// Probe a second candidate while a held handle is alive.
    ProbeWhileHeld {
        cand: usize,
        expect_valid: bool,
    },
    CloseHeld,
    PauseCmd,
    ResumeCmd,
    StopCmd,
    /// Seek the old episode to `target_s` (source-relative seconds).
    SeekCmd(f64),
}

fn scenario(name: &str) -> Option<(Vec<Step>, bool)> {
    // (steps, expected_overall_green). NEG expects RED: it stops the old
    // episode at window start and keeps probing; the advance oracle must
    // fire for the harness to count as falsifiable.
    let steps = match name {
        "S1" => vec![
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
        ],
        "S2" => vec![
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
        ],
        "S3" => vec![
            Step::Probe {
                cand: 0,
                expect_valid: false,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: false,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: false,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: false,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: false,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: false,
            },
        ],
        "S4" => {
            let mut v = Vec::new();
            for i in 0..12 {
                v.push(Step::Probe {
                    cand: 0,
                    expect_valid: true,
                });
                if i % 3 == 2 {
                    v.push(Step::ProbeHold {
                        cand: 0,
                        hold: Duration::from_millis(250),
                        expect_valid: true,
                    });
                }
                v.push(Step::Sleep(Duration::from_millis(400)));
            }
            v
        }
        "S5" => vec![
            Step::Sleep(Duration::from_millis(400)),
            Step::PauseCmd,
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::ResumeCmd,
            Step::Sleep(Duration::from_millis(1200)),
        ],
        "S6" => vec![
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(600)),
            Step::SeekCmd(1.0),
            Step::Sleep(Duration::from_millis(300)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(600)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(600)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(600)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(600)),
        ],
        "S7" => vec![
            Step::Probe {
                cand: 0,
                expect_valid: false,
            },
            Step::Sleep(Duration::from_millis(500)),
            Step::Probe {
                cand: 1,
                expect_valid: false,
            },
            Step::Sleep(Duration::from_millis(500)),
            Step::Probe {
                cand: 2,
                expect_valid: false,
            },
            Step::Sleep(Duration::from_millis(500)),
            Step::Probe {
                cand: 0,
                expect_valid: false,
            },
            Step::Sleep(Duration::from_millis(500)),
            Step::Probe {
                cand: 1,
                expect_valid: false,
            },
            Step::Sleep(Duration::from_millis(500)),
            Step::Probe {
                cand: 2,
                expect_valid: false,
            },
        ],
        "S8" => vec![
            Step::ProbeHold {
                cand: 0,
                hold: Duration::from_millis(1000),
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(600)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(600)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(600)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
        ],
        "S9" => {
            let mut v = Vec::new();
            for i in 0..10 {
                let (cand, expect_valid) = if i % 2 == 0 {
                    (0usize, true)
                } else {
                    (1usize, false)
                };
                v.push(Step::Probe { cand, expect_valid });
                v.push(Step::Sleep(Duration::from_millis(350)));
            }
            v
        }
        "S10" => vec![
            Step::OpenHold {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(800)),
            Step::ProbeWhileHeld {
                cand: 1,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(800)),
            Step::ProbeWhileHeld {
                cand: 1,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(800)),
            Step::ProbeWhileHeld {
                cand: 1,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(800)),
            Step::CloseHeld,
            Step::Probe {
                cand: 1,
                expect_valid: true,
            },
        ],
        "NEG" => vec![
            Step::StopCmd,
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
            Step::Sleep(Duration::from_millis(700)),
            Step::Probe {
                cand: 0,
                expect_valid: true,
            },
        ],
        _ => return None,
    };
    let expected_green = name != "NEG";
    Some((steps, expected_green))
}

// --- second-handle probe machinery (mirrors production, owns nothing
// --- RT-visible; zero production code touched) ---------------------------

// Host-IO callbacks: harness-local copies of the production guard
// discipline (fail closed on null userdata/destination, negative offset).
unsafe extern "C" fn file_read(ud: *mut c_void, dst: *mut u8, size: usize) -> i64 {
    if ud.is_null() || dst.is_null() {
        return -1;
    }
    let file = unsafe { &mut *(ud as *mut std::fs::File) };
    let buf = unsafe { slice::from_raw_parts_mut(dst, size) };
    match file.read(buf) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

unsafe extern "C" fn file_seek(ud: *mut c_void, offset: i64) -> i64 {
    if ud.is_null() || offset < 0 {
        return -1;
    }
    let file = unsafe { &mut *(ud as *mut std::fs::File) };
    match file.seek(SeekFrom::Start(offset as u64)) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

unsafe extern "C" fn file_size(ud: *mut c_void) -> i64 {
    if ud.is_null() {
        return -1;
    }
    let file = unsafe { &mut *(ud as *mut std::fs::File) };
    match file.metadata() {
        Ok(m) => m.len() as i64,
        Err(_) => -1,
    }
}

/// One temporary native handle + its kept-alive host file. Drop runs
/// `song_close` exactly like the production endpoint. The probe NEVER
/// reads PCM frames.
///
/// Drop-order guarantee (Rust reference): `Drop::drop` runs BEFORE any
/// field is dropped, so `song_close` below always executes while the
/// `file` box is still alive for the callbacks to borrow — the same
/// behavior the production endpoint relies on.
struct ProbeHandle {
    handle: *mut sys::song_handle,
    /// Kept alive for the handle's lifetime; the callbacks borrow it raw
    /// (same ownership shape as the production endpoint). Never read:
    /// its whole job is being alive, then dropped after song_close.
    #[allow(dead_code)]
    file: Box<std::fs::File>,
}

impl Drop for ProbeHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { sys::song_close(self.handle) };
        }
    }
}

#[derive(Clone, serde::Serialize)]
struct ProbeFacts {
    sample_rate: i32,
    channels: i32,
    duration_us: i64,
    bits_per_sample: i32,
}

#[derive(Clone, serde::Serialize)]
struct ProbeRecord {
    step: usize,
    t_ms: u128,
    candidate: String,
    expect_valid: bool,
    /// Whether the observed verdict matched the expectation.
    ok: bool,
    elapsed_us: u128,
    facts: Option<ProbeFacts>,
    error: Option<String>,
}

/// Open + probe one source on a fresh native handle. Mirrors the
/// production open path through `song_probe`; no `song_read_pcm` call
/// exists on this path.
fn probe_open(path: &Path) -> Result<ProbeHandle, String> {
    let file: Box<std::fs::File> = Box::new(
        std::fs::File::open(path).map_err(|e| format!("cannot open '{}': {e}", path.display()))?,
    );
    let io = sys::song_io {
        userdata: Box::into_raw(file) as *mut c_void,
        read: Some(file_read),
        seek: Some(file_seek),
        size: Some(file_size),
    };
    let mut handle: *mut sys::song_handle = std::ptr::null_mut();
    // SAFETY: song_open per the ABI v1 contract; a non-OK status leaves
    // no handle to close and the host file is reclaimed here.
    let status = unsafe { sys::song_open(&io, &mut handle) };
    if status != sys::SONG_OK {
        unsafe { drop(Box::from_raw(io.userdata as *mut std::fs::File)) };
        return Err(format!(
            "song_open refused '{}': status {status}",
            path.display()
        ));
    }
    if handle.is_null() {
        unsafe { drop(Box::from_raw(io.userdata as *mut std::fs::File)) };
        return Err(format!(
            "song_open '{}' returned SONG_OK with null handle",
            path.display()
        ));
    }
    Ok(ProbeHandle {
        handle,
        // SAFETY: io.userdata was created by Box::into_raw above and has
        // not been freed on any path that reaches this line.
        file: unsafe { Box::from_raw(io.userdata as *mut std::fs::File) },
    })
}

fn probe_facts(handle: &ProbeHandle) -> Result<ProbeFacts, String> {
    let mut info: sys::song_info = unsafe { std::mem::zeroed() };
    // SAFETY: handle is a live song_handle; info is a valid zeroed
    // song_info out-parameter per the ABI.
    let status = unsafe { sys::song_probe(handle.handle, &mut info) };
    if status != sys::SONG_OK {
        return Err(format!("song_probe failed: status {status}"));
    }
    Ok(ProbeFacts {
        sample_rate: info.sample_rate,
        channels: info.channels,
        duration_us: info.duration_us,
        bits_per_sample: info.bits_per_sample,
    })
}

/// The whole probe operation: open → probe → close, with the ABI
/// version check the production mechanism performs first.
fn probe_source(path: &Path, step: usize, t_ms: u128, expect_valid: bool) -> ProbeRecord {
    let started = Instant::now();
    let display = path.display().to_string();
    let outcome = (|| -> Result<ProbeFacts, String> {
        let loaded = unsafe { sys::songcore_abi_version() };
        if loaded != sys::SONGCORE_ABI_VERSION {
            return Err(format!("ABI mismatch: v{loaded}"));
        }
        let handle = probe_open(path)?;
        let facts = probe_facts(&handle);
        drop(handle); // song_close runs here — the "drop" of the cycle
        facts
    })();
    let elapsed_us: u128 = started.elapsed().as_micros();
    let (facts, error) = match outcome {
        Ok(facts) => (Some(facts), None),
        Err(message) => (None, Some(message)),
    };
    ProbeRecord {
        step,
        t_ms,
        candidate: display,
        expect_valid,
        ok: facts.is_some() == expect_valid,
        elapsed_us,
        facts,
        error,
    }
}

// --- live episode through the production wiring ---------------------------

struct Episode {
    runtime: qianqian_app::QianqianApp,
    handle: PlaybackSessionHandle,
}

/// Exactly the production wiring of `apps/headless/src/main.rs
/// start_episode`, unmodified: one QianqianApp composition root per
/// episode, decode/output/session components, one desired composition.
fn start_episode(file: &Path) -> Result<Episode, String> {
    let mut runtime = qianqian_app::QianqianApp::new();
    runtime
        .register_component(qianqian_decode_songcore::songcore_decode_plugin())
        .map_err(|e| format!("decode plugin registration failed: {e:?}"))?;
    runtime
        .register_component(qianqian_output_wasapi::output_plugin())
        .map_err(|e| format!("output plugin registration failed: {e:?}"))?;
    let handle = PlaybackSessionHandle::new();
    runtime
        .register_component(qianqian_playback::playback_session_spec(
            file.to_path_buf(),
            handle.clone(),
        ))
        .map_err(|e| format!("session registration failed: {e:?}"))?;
    let desired =
        |id: &str, component: &'static str| DesiredEntry::enabled(id, component, Revision::new(1));
    runtime
        .revise_desired(vec![
            desired("decode", "songcore_decode_plugin"),
            desired("output", "output_plugin"),
            desired("session", "playback_session"),
        ])
        .map_err(|e| format!("composition refused: {e}"))?;
    // Activation diagnostics only (same class as the production
    // wiring's use): decides whether waiting for a terminal Fact is
    // meaningful. Not a correctness input to any oracle below.
    let activated = runtime
        .composition_snapshot()
        .fibers
        .get("session")
        .map(|f| f.state)
        == Some(FiberState::Active);
    if !activated {
        return Err(format!(
            "session never activated: {:?}",
            handle.observe().activation_error
        ));
    }
    Ok(Episode { runtime, handle })
}

// --- sampler ---------------------------------------------------------------

type Samples = Arc<Mutex<Vec<(u128, Option<u64>)>>>;

fn spawn_sampler(handle: PlaybackSessionHandle, stop: Arc<AtomicBool>, samples: Samples) {
    std::thread::Builder::new()
        .name("sprobe-sampler".into())
        .spawn(move || {
            let started = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                let position = handle.observe().position;
                samples
                    .lock()
                    .expect("sampler lock")
                    .push((started.elapsed().as_millis(), position));
                std::thread::sleep(SAMPLE_INTERVAL);
            }
        })
        .expect("sampler spawn");
}

fn now_ms(samples: &Samples) -> u128 {
    samples
        .lock()
        .expect("sampler lock")
        .last()
        .map(|(t, _)| *t)
        .unwrap_or(0)
}

// --- oracle ----------------------------------------------------------------

/// Compute the verdict. Returns `(actual_green, reasons)`.
/// `actual_green == expected_green` decides the exit code.
fn verdict_of(
    samples: &[(u128, Option<u64>)],
    sample_rate: u32,
    window: (u128, u128),
    pause_interval: Option<(u128, u128)>,
    seek_cmd_ms: Option<u128>,
    probes: &[ProbeRecord],
    extra_reasons: &[String],
) -> (bool, Vec<String>) {
    let mut reasons: Vec<String> = extra_reasons.to_vec();

    // Probe verdict oracle: every probe operation must match its
    // expectation.
    for p in probes {
        if !p.ok {
            reasons.push(format!(
                "probe verdict mismatch at {} ms for {} (expect_valid={}): {:?}",
                p.t_ms, p.candidate, p.expect_valid, p.error
            ));
        }
    }

    // Position-advance oracle over consecutive sample pairs inside the
    // playing window (pause interval excluded, freeze-checked below).
    let in_playing = |t: u128| -> bool {
        t >= window.0 && t <= window.1 && !pause_interval.is_some_and(|(a, b)| t >= a && t <= b)
    };
    let mut advance: u64 = 0;
    let mut expected: f64 = 0.0;
    let mut missing_pairs: Vec<u128> = Vec::new();
    let mut saw_some = false;
    for pair in samples.windows(2) {
        let (t0, p0) = (pair[0].0, pair[0].1);
        let (t1, p1) = (pair[1].0, pair[1].1);
        // Only pairs FULLY inside the playing window count: a pair
        // straddling the window start may legitimately carry a None
        // (the pre-first-publication warm-up) and must not read as an
        // in-window absence.
        if !in_playing(t0) || !in_playing(t1) {
            continue;
        }
        match (p0, p1) {
            (Some(a), Some(b)) => {
                saw_some = true;
                expected += sample_rate as f64
                    * (t1 - t0).min(SAMPLE_INTERVAL.as_millis() * 3) as f64
                    / 1000.0;
                if b >= a {
                    advance += b - a;
                } else {
                    // A regression is legal only around a seek command
                    // (the rebase to the actual landing).
                    let near_seek = seek_cmd_ms
                        .is_some_and(|s| t1.saturating_sub(s) <= SEEK_REGRESSION_WINDOW_MS);
                    if !near_seek {
                        reasons.push(format!(
                            "position regressed {} frames at {} ms without a seek",
                            a - b,
                            t1
                        ));
                    }
                }
            }
            _ => missing_pairs.push(t1),
        }
    }
    if !missing_pairs.is_empty() {
        reasons.push(format!(
            "position absent in {} playing-window samples (first at {} ms) — \
             the episode was not live/rendering",
            missing_pairs.len(),
            missing_pairs[0]
        ));
    }
    if !saw_some {
        reasons.push("no in-window position sample pairs at all — no render evidence".into());
    }
    if saw_some && expected > 0.0 && (advance as f64) < ADVANCE_FRACTION * expected {
        reasons.push(format!(
            "playing advance {advance} frames < {ADVANCE_FRACTION:.0} of expected {expected:.0}"
        ));
    }

    // Pause-freeze oracle.
    if let Some((pa, pb)) = pause_interval {
        let positions: Vec<u64> = samples
            .iter()
            .filter(|(t, _)| *t >= pa && *t <= pb)
            .filter_map(|(_, p)| *p)
            .collect();
        if let (Some(min), Some(max)) = (positions.iter().min(), positions.iter().max()) {
            if max - min > PAUSE_FREEZE_TOLERANCE_FRAMES {
                reasons.push(format!(
                    "position moved {} frames while the pause projection was established",
                    max - min
                ));
            }
        }
    }

    let green = reasons.is_empty();
    (green, reasons)
}

// --- main ------------------------------------------------------------------

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut scenario_name: Option<String> = None;
    let mut main_path: Option<PathBuf> = None;
    let mut cands: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--scenario" => {
                i += 1;
                scenario_name = args.get(i).cloned();
            }
            "--main" => {
                i += 1;
                main_path = args.get(i).map(PathBuf::from);
            }
            "--cand" => {
                i += 1;
                if let Some(c) = args.get(i) {
                    cands.push(PathBuf::from(c));
                }
            }
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    let (Some(scenario_name), Some(main_path)) = (scenario_name, main_path) else {
        eprintln!("usage: sprobe --scenario <S1..S10|NEG> --main <path> [--cand <path>]...");
        std::process::exit(2);
    };
    let Some((steps, expected_green)) = scenario(&scenario_name) else {
        eprintln!("unknown scenario {scenario_name}");
        std::process::exit(2);
    };

    let outcome = run_scenario(main_path.as_path(), &cands, &steps);
    let (
        mut reasons,
        old_report,
        samples_json,
        probes,
        window,
        pause_interval,
        seek_cmd_ms,
        sample_rate,
    ) = match outcome {
        Ok(v) => v,
        Err(e) => {
            eprintln!("HARNESS ERROR: {e}");
            println!(
                "{}",
                serde_json::json!({
                    "scenario": scenario_name,
                    "verdict": "HARNESS_ERROR",
                    "error": e,
                })
            );
            std::process::exit(1);
        }
    };

    let samples = samples_json_to_tuples(&samples_json);
    let (green, oracle_reasons) = verdict_of(
        &samples,
        sample_rate,
        window,
        pause_interval,
        seek_cmd_ms,
        &probes,
        &reasons,
    );
    reasons = oracle_reasons;
    let matched = green == expected_green;
    let verdict = if green { "GREEN" } else { "RED" };

    let doc = serde_json::json!({
        "scenario": scenario_name,
        "expected": if expected_green { "GREEN" } else { "RED" },
        "verdict": verdict,
        "matched_expectation": matched,
        "reasons": reasons,
        "sample_rate": sample_rate,
        "window_ms": [window.0, window.1],
        "old_episode": old_report,
        "position_samples": samples_json,
        "probes": probes,
    });
    println!("{}", serde_json::to_string_pretty(&doc).expect("json"));
    eprintln!("{scenario_name}: verdict={verdict} matched={matched}");
    for r in &reasons {
        eprintln!("  reason: {r}");
    }
    std::process::exit(if matched { 0 } else { 1 });
}

fn samples_json_to_tuples(samples: &serde_json::Value) -> Vec<(u128, Option<u64>)> {
    samples
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| {
                    let arr = s.as_array()?;
                    let t = arr.first()?.as_u64()? as u128;
                    let p = arr.get(1)?.as_u64();
                    Some((t, p))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[allow(clippy::type_complexity)]
fn run_scenario(
    main_path: &Path,
    cands: &[PathBuf],
    steps: &[Step],
) -> Result<
    (
        Vec<String>,
        serde_json::Value,
        serde_json::Value,
        Vec<ProbeRecord>,
        (u128, u128),
        Option<(u128, u128)>,
        Option<u128>,
        u32,
    ),
    String,
> {
    let mut reasons: Vec<String> = Vec::new();
    let mut episode = start_episode(main_path)?;
    let observation = episode.handle.observe();
    let sample_rate = observation
        .source_format
        .map(|f| f.sample_rate)
        .unwrap_or(44100);

    // Sampler runs from before the warm-up until after the window.
    let stop_flag = Arc::new(AtomicBool::new(false));
    let samples: Samples = Arc::new(Mutex::new(Vec::new()));
    spawn_sampler(episode.handle.clone(), stop_flag.clone(), samples.clone());

    std::thread::sleep(WARMUP);

    let window_start = now_ms(&samples);
    let mut pause_interval: Option<(u128, u128)> = None;
    let mut seek_cmd_ms: Option<u128> = None;
    let mut probes: Vec<ProbeRecord> = Vec::new();
    let mut held: Option<ProbeHandle> = None;

    for (idx, step) in steps.iter().enumerate() {
        match step {
            Step::Sleep(d) => std::thread::sleep(*d),
            Step::Probe { cand, expect_valid } => {
                let path = cands
                    .get(*cand)
                    .ok_or_else(|| format!("scenario references missing cand {cand}"))?;
                probes.push(probe_source(path, idx, now_ms(&samples), *expect_valid));
            }
            Step::ProbeHold {
                cand,
                hold,
                expect_valid,
            } => {
                let path = cands
                    .get(*cand)
                    .ok_or_else(|| format!("scenario references missing cand {cand}"))?;
                let started = Instant::now();
                let t_ms = now_ms(&samples);
                let record = (|| -> Result<ProbeFacts, String> {
                    let handle = probe_open(path)?;
                    let facts = probe_facts(&handle)?;
                    std::thread::sleep(*hold);
                    drop(handle);
                    Ok(facts)
                })();
                probes.push(ProbeRecord {
                    step: idx,
                    t_ms,
                    candidate: path.display().to_string(),
                    expect_valid: *expect_valid,
                    ok: record.is_ok() == *expect_valid,
                    elapsed_us: started.elapsed().as_micros(),
                    facts: record.clone().ok(),
                    error: record.err(),
                });
            }
            Step::OpenHold { cand, expect_valid } => {
                let path = cands
                    .get(*cand)
                    .ok_or_else(|| format!("scenario references missing cand {cand}"))?;
                match probe_open(path).and_then(|h| probe_facts(&h).map(|f| (h, f))) {
                    Ok((h, _facts)) => {
                        if !*expect_valid {
                            reasons.push("OpenHold expected invalid but opened".into());
                        }
                        held = Some(h);
                    }
                    Err(e) => {
                        if *expect_valid {
                            reasons.push(format!("OpenHold failed: {e}"));
                        }
                    }
                }
            }
            Step::ProbeWhileHeld { cand, expect_valid } => {
                if held.is_none() {
                    reasons.push("ProbeWhileHeld with no held handle".into());
                }
                let path = cands
                    .get(*cand)
                    .ok_or_else(|| format!("scenario references missing cand {cand}"))?;
                probes.push(probe_source(path, idx, now_ms(&samples), *expect_valid));
            }
            Step::CloseHeld => {
                held = None; // Drop runs song_close
            }
            Step::PauseCmd => {
                episode.handle.request_pause();
                // Wait until the D14.7 Paused projection is established
                // (bounded); the freeze window starts at establishment.
                let deadline = Instant::now() + Duration::from_secs(3);
                loop {
                    if episode.handle.observe().paused() {
                        pause_interval = Some((now_ms(&samples), now_ms(&samples)));
                        break;
                    }
                    if Instant::now() > deadline {
                        reasons.push("pause projection never established".into());
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            Step::ResumeCmd => {
                if let Some((pa, _)) = pause_interval {
                    pause_interval = Some((pa, now_ms(&samples)));
                }
                episode.handle.request_resume();
            }
            Step::StopCmd => {
                episode.handle.request_stop();
            }
            Step::SeekCmd(target_s) => {
                seek_cmd_ms = Some(now_ms(&samples));
                episode
                    .handle
                    .request_seek(Duration::from_secs_f64(*target_s));
            }
        }
    }
    let window_end = now_ms(&samples);
    std::thread::sleep(AFTER_WINDOW);
    stop_flag.store(true, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(250));

    // Settle: stop intent iff unsettled (the production tui_transport
    // discipline), wait for the D11 Fact, dispose, report. A watchdog
    // bounds the whole settle: wait_terminal is a pure wait with no
    // timeout, so a wedged episode would otherwise hang the runner
    // instead of surfacing as a failed run (evidence-robustness bound;
    // 30 s is far beyond any observed settle). A fired watchdog exits
    // the whole process with the distinctive code 42; on the normal
    // path the binding is dropped (detaching the thread) and the
    // process exits first.
    let _settle_watchdog = std::thread::Builder::new()
        .name("sprobe-settle-watchdog".into())
        .spawn(|| {
            std::thread::sleep(Duration::from_secs(30));
            eprintln!("SETTLE WATCHDOG FIRED: episode did not settle within 30 s");
            std::process::exit(42);
        })
        .expect("watchdog spawn");
    if episode.handle.observe().terminal_outcome.is_none() {
        episode.handle.request_stop();
    }
    let terminal = episode.handle.wait_terminal();
    let final_observation = episode.handle.observe();
    let snapshot = episode.runtime.dispose();
    if !snapshot.quiet {
        reasons.push("dispose snapshot not quiet (teardown violation?)".into());
    }
    if let Some(d) = &final_observation.failure_diagnostic {
        reasons.push(format!("old episode failure diagnostic: {d}"));
    }
    if let Some(e) = &final_observation.activation_error {
        reasons.push(format!("old episode activation error: {e}"));
    }
    if terminal != qianqian_playback::EpisodeTerminalOutcome::Stopped {
        reasons.push(format!("terminal outcome {terminal:?} != Stopped"));
    }

    let samples_json: Vec<serde_json::Value> = samples
        .lock()
        .expect("sampler lock")
        .iter()
        .map(|(t, p)| serde_json::json!([t, p]))
        .collect();
    let old_report = serde_json::json!({
        "source_format": final_observation
            .source_format
            .map(|f| serde_json::json!({"sample_rate": f.sample_rate, "channels": f.channels})),
        "terminal": format!("{terminal:?}"),
        "failure_diagnostic": final_observation.failure_diagnostic,
        "activation_error": final_observation.activation_error,
        "dispose_quiet": snapshot.quiet,
    });

    Ok((
        reasons,
        old_report,
        serde_json::Value::Array(samples_json),
        probes,
        (window_start, window_end),
        pause_interval,
        seek_cmd_ms,
        sample_rate,
    ))
}
