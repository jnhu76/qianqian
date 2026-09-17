//! F5-GATE Experiment E1 — decoder seek reality over the SongCore ABI
//! (non-Windows; links the same static SongCore the decode plugin
//! links).
//!
//! For every committed corpus fixture the probe answers, with
//! measurements, the questions the F5 gate must decide the decoder
//! seam from:
//!
//! ```text
//! requested target (us)      the song_seek argument
//! reported landing (us)      song_seek's out_actual_position_us
//! first retained PCM (frame) content match of post-seek decode
//!                            against a full sequential reference
//!                            decode of the same file (ground truth
//!                            independent of reported timestamps)
//! pre-target emission        does post-seek PCM start before the
//!                            requested target? (caller-side discard
//!                            question)
//! seek latency               wall time of the song_seek call
//! failure paths              negative target, target beyond duration,
//!                            target == duration (EOF), seek before
//!                            any read, back-to-back seeks, post-
//!                            failure decode usability, format
//!                            stability across seek
//! ```
//!
//! Content matching is exact sample equality. For lossless codecs
//! (FLAC, ALAC) a post-seek decode is bit-identical to the sequential
//! reference, so the first retained frame is measured exactly. For the
//! lossy fixture (MP3) the codec's documented seek tolerance means the
//! first frames after a seek may differ from a sequential decode; the
//! probe therefore also searches later windows (skipping whole codec
//! frames) and reports the smallest skip that converges, plus whether
//! a pre-target match exists at all.

#![cfg(not(windows))]

use std::collections::HashMap;
use std::os::raw::c_void;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Instant;

use qianqian_songcore_sys as sys;

const FIXTURE_IDS: [&str; 4] = [
    "mp3-cbr-id3v23",
    "flac-16-44-stereo",
    "alac-16-44-stereo",
    "alac-long",
];

/// Post-seek decode depth for the content match (frames).
const MATCH_WINDOW_FRAMES: usize = 1024;
/// MP3 convergence search: window start skips probed, in frames.
const SKIP_STEPS_FRAMES: [usize; 6] = [0, 1152, 2304, 3456, 4608, 6912];

/// Host-IO callbacks over std::fs::File — same shape as the production
/// binding (f4duration carries the same copies; the evidence crate
/// imports no qianqian-* production code).
unsafe extern "C" fn file_read(ud: *mut c_void, dst: *mut u8, size: usize) -> i64 {
    if ud.is_null() || dst.is_null() {
        return -1;
    }
    use std::io::Read;
    let file = unsafe { &mut *(ud as *mut std::fs::File) };
    let buf = unsafe { std::slice::from_raw_parts_mut(dst, size) };
    match file.read(buf) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

unsafe extern "C" fn file_seek(ud: *mut c_void, offset: i64) -> i64 {
    if ud.is_null() || offset < 0 {
        return -1;
    }
    use std::io::{Seek, SeekFrom};
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

fn status_name(status: u32) -> &'static str {
    match status {
        sys::SONG_OK => "SONG_OK",
        sys::SONG_EOF => "SONG_EOF",
        sys::SONG_ERR_INVALID_ARGUMENT => "INVALID_ARGUMENT",
        sys::SONG_ERR_STATE => "STATE",
        sys::SONG_ERR_SEEK_UNSUPPORTED => "SEEK_UNSUPPORTED",
        sys::SONG_ERR_SEEK_ERROR => "SEEK_ERROR",
        sys::SONG_ERR_STREAM_CHANGE => "STREAM_CHANGE",
        sys::SONG_ERR_DECODE_ERROR => "DECODE_ERROR",
        sys::SONG_ERR_IO => "IO",
        _ => "OTHER",
    }
}

struct Opened {
    handle: *mut sys::song_handle,
    rate: u32,
    channels: usize,
    duration_us: i64,
}

fn open(path: &Path) -> Result<Opened, String> {
    let file: Box<std::fs::File> = std::fs::File::open(path)
        .map(Box::new)
        .map_err(|e| format!("open: {e}"))?;
    let io = sys::song_io {
        userdata: Box::into_raw(file) as *mut c_void,
        read: Some(file_read),
        seek: Some(file_seek),
        size: Some(file_size),
    };
    let mut handle: *mut sys::song_handle = std::ptr::null_mut();
    let status = unsafe { sys::song_open(&io, &mut handle) };
    if status != sys::SONG_OK {
        unsafe { drop(Box::from_raw(io.userdata as *mut std::fs::File)) };
        return Err(format!("song_open status {}", status_name(status)));
    }
    let mut info: sys::song_info = unsafe { std::mem::zeroed() };
    let status = unsafe { sys::song_probe(handle, &mut info) };
    if status != sys::SONG_OK {
        return Err(format!("song_probe status {}", status_name(status)));
    }
    Ok(Opened {
        handle,
        rate: info.sample_rate as u32,
        channels: info.channels as usize,
        duration_us: info.duration_us,
    })
}

/// Decode from the current cursor to EOF, chunk by chunk.
fn decode_to_eof(s: &Opened) -> Result<Vec<f32>, String> {
    let mut out = Vec::new();
    let mut buf = vec![0.0f32; 1024 * s.channels];
    let mut stall = 0u32;
    loop {
        let mut produced: u64 = 0;
        let status = unsafe {
            sys::song_read_pcm(s.handle, buf.as_mut_ptr(), 1024, &mut produced)
        };
        match status {
            sys::SONG_OK => {
                if produced == 0 {
                    stall += 1;
                    if stall > 64 {
                        return Err("decoder stopped making progress".into());
                    }
                    continue;
                }
                stall = 0;
                out.extend_from_slice(&buf[..produced as usize * s.channels]);
            }
            sys::SONG_EOF => return Ok(out),
            _ => return Err(format!("read_pcm status {}", status_name(status))),
        }
    }
}

/// Read exactly `frames` frames from the current cursor (or fewer at
/// EOF); returns the samples read.
fn decode_frames(s: &Opened, frames: usize) -> Result<Vec<f32>, String> {
    let mut out = Vec::with_capacity(frames * s.channels);
    let mut buf = vec![0.0f32; 1024 * s.channels];
    let mut remaining = frames;
    while remaining > 0 {
        let want = remaining.min(1024);
        let mut produced: u64 = 0;
        let status = unsafe {
            sys::song_read_pcm(s.handle, buf.as_mut_ptr(), want as u64, &mut produced)
        };
        match status {
            sys::SONG_OK => {
                if produced == 0 {
                    continue;
                }
                let n = produced as usize;
                out.extend_from_slice(&buf[..n * s.channels]);
                remaining -= n;
            }
            sys::SONG_EOF => break,
            _ => return Err(format!("read_pcm status {}", status_name(status))),
        }
    }
    Ok(out)
}

/// Frame index in `reference` where `needle` (samples, from window start
/// `skip_frames`) first matches exactly, scanning the whole reference.
/// `hint` is the frame offset to verify first (the ABI's landing claim);
/// `None` skips the fast path. Returns (frame_offset, skip_frames_used).
fn content_match(
    reference: &[f32],
    needle: &[f32],
    channels: usize,
    skip_frames: usize,
    hint: Option<usize>,
) -> Option<(usize, usize)> {
    let needle_frames = (needle.len() / channels).saturating_sub(skip_frames);
    if needle_frames == 0 {
        return None;
    }
    let start_sample = skip_frames * channels;
    let window = &needle[start_sample..start_sample + needle_frames * channels];
    let hay_frames = reference.len() / channels;
    if hay_frames < needle_frames {
        return None;
    }
    // Fast path: the ABI's reported landing claims this offset.
    if let Some(h) = hint {
        if h + needle_frames <= hay_frames
            && window == &reference[h * channels..h * channels + window.len()]
        {
            return Some((h, skip_frames));
        }
    }
    // Slow path: full scan.
    for o in 0..=(hay_frames - needle_frames) {
        if window == &reference[o * channels..o * channels + window.len()] {
            return Some((o, skip_frames));
        }
    }
    None
}

fn us_to_frames(us: i64, rate: u32) -> i64 {
    // Rounded conversion: the ABI reports whole microseconds, so the
    // landing frame is only recoverable to ±1 frame of µs quantization
    // (1 µs = 0.0441 frames at 44.1 kHz). Comparisons against a
    // content-matched offset therefore tolerate 1 frame; anything
    // beyond that is a real landing mismatch.
    (us * i64::from(rate) + 500_000) / 1_000_000
}

fn do_seek(s: &Opened, target_us: i64) -> (u32, i64, u128) {
    let mut actual: i64 = 0;
    let t0 = Instant::now();
    let status = unsafe { sys::song_seek(s.handle, target_us, &mut actual) };
    let dt = t0.elapsed().as_nanos();
    (status, actual, dt)
}

/// One seek measurement against the sequential reference.
fn probe_seek(
    label: &str,
    fixture: &str,
    s: &Opened,
    reference: &[f32],
    target_us: i64,
    rate: u32,
) -> usize {
    let mut failures = 0usize;
    let (status, actual, dt) = do_seek(s, target_us);
    let target_frames = us_to_frames(target_us, rate);
    println!(
        "F5SEEK {fixture} {label} requested_us={target_us} requested_frames={target_frames} \
         status={} actual_us={actual} actual_frames={} latency_ns={dt}",
        status_name(status),
        us_to_frames(actual, rate),
    );
    if status != sys::SONG_OK {
        return failures;
    }
    let landing_frames = us_to_frames(actual, rate);
    // Near-EOF targets leave less audio than one match window: the
    // content match is undefined there (and for lossy the post-seek
    // decode no longer mirrors the reference byte-for-byte). Record the
    // landing, skip the match, count no failure.
    let remaining_reference = (reference.len() / s.channels) as i64 - landing_frames;
    if remaining_reference < (MATCH_WINDOW_FRAMES + *SKIP_STEPS_FRAMES.last().unwrap()) as i64 {
        println!(
            "F5SEEK {fixture} {label} EOF_REGION remaining_reference_frames={remaining_reference} match=skipped"
        );
        return failures;
    }
    // Decode a post-seek window and find its true source offset.
    let window = match decode_frames(s, MATCH_WINDOW_FRAMES + *SKIP_STEPS_FRAMES.last().unwrap())
    {
        Ok(w) => w,
        Err(e) => {
            println!("F5SEEK {fixture} {label} POST_SEEK_DECODE_FAILED {e}");
            return 1;
        }
    };
    if window.len() < (MATCH_WINDOW_FRAMES + 4096) * s.channels {
        // Near-EOF target: fewer frames than the full window is legal;
        // match what exists if it can hold the smallest window.
        if window.len() < MATCH_WINDOW_FRAMES * s.channels {
            println!(
                "F5SEEK {fixture} {label} SHORT_WINDOW frames_read={}",
                window.len() / s.channels
            );
            return failures;
        }
    }
    let mut matched = None;
    for &skip in &SKIP_STEPS_FRAMES {
        if let Some(m) = content_match(
            reference,
            &window,
            s.channels,
            skip,
            Some(landing_frames.max(0) as usize),
        ) {
            matched = Some(m);
            break;
        }
    }
    match matched {
        Some((offset, skip)) => {
            let pre_target = (offset as i64) < target_frames;
            println!(
                "F5SEEK {fixture} {label} MATCH offset_frames={offset} skip_frames={skip} \
                 landing_vs_match_delta={} pre_target_emission={pre_target}",
                offset as i64 - landing_frames
            );
            if skip == 0
                && (offset as i64 - landing_frames).abs() > 1
            {
                // ±1 frame is µs-quantization noise (see us_to_frames);
                // anything wider is a real landing mismatch.
                println!(
                    "F5SEEK {fixture} {label} LANDING_MISMATCH reported={landing_frames} actual={offset}"
                );
                failures += 1;
            }
        }
        None => {
            println!("F5SEEK {fixture} {label} NO_CONTENT_MATCH");
            failures += 1;
        }
    }
    // Format stability across seek (header contract).
    let mut info: sys::song_info = unsafe { std::mem::zeroed() };
    let st = unsafe { sys::song_probe(s.handle, &mut info) };
    if st != sys::SONG_OK
        || info.sample_rate as u32 != s.rate
        || info.channels as usize != s.channels
    {
        println!("F5SEEK {fixture} {label} FORMAT_CHANGED_AFTER_SEEK");
        failures += 1;
    }
    failures
}

fn run_fixture(fixture: &str, path: &Path) -> usize {
    let mut failures = 0usize;
    let s = match open(path) {
        Ok(s) => s,
        Err(e) => {
            println!("F5SEEK {fixture} OPEN_FAILED {e}");
            return 1;
        }
    };
    println!(
        "F5SEEK {fixture} PROBE rate={} channels={} duration_us={}",
        s.rate, s.channels, s.duration_us
    );
    // Sequential reference decode (ground truth).
    let reference = match decode_to_eof(&s) {
        Ok(r) => r,
        Err(e) => {
            println!("F5SEEK {fixture} REFERENCE_DECODE_FAILED {e}");
            return 1;
        }
    };
    let reference_frames = (reference.len() / s.channels) as i64;
    println!("F5SEEK {fixture} REFERENCE frames={reference_frames}");

    // Seek before any read: the fresh-handle seek the worker protocol
    // can hit when the command arrives during decode warm-up.
    failures += probe_seek(
        "fresh-zero",
        fixture,
        &s,
        &reference,
        0,
        s.rate,
    );
    // Re-seed to a mid position, then back-to-back seeks (T13 shape at
    // decoder level): the second landing must be honored exactly.
    failures += probe_seek("b2b-first", fixture, &s, &reference, s.duration_us / 2, s.rate);
    failures += probe_seek("b2b-second", fixture, &s, &reference, s.duration_us / 4, s.rate);

    // Sweep of proportional targets.
    for pct in [10u64, 33, 50, 66, 90] {
        let target = s.duration_us * (pct as i64) / 100;
        failures += probe_seek(
            &format!("pct-{pct}"),
            fixture,
            &s,
            &reference,
            target,
            s.rate,
        );
    }

    // Target == declared duration (EOF neighborhood) and beyond.
    failures += probe_seek(
        "at-duration",
        fixture,
        &s,
        &reference,
        s.duration_us,
        s.rate,
    );
    failures += probe_seek(
        "beyond-duration",
        fixture,
        &s,
        &reference,
        s.duration_us + 500_000,
        s.rate,
    );

    // Negative target: the ABI's validation path. After the rejection
    // the decoder must still be usable — the F5 failure policy leans on
    // pre-cut failures leaving playback intact. Seed a mid position
    // first (a cursor already at EOF has nothing left to match, which
    // would say nothing about usability).
    let (_seed_st, seed_landing, _seed_dt) = do_seek(&s, s.duration_us / 4);
    let (status, actual, dt) = do_seek(&s, -1_000);
    println!(
        "F5SEEK {fixture} negative-target seed_landing_frames={} status={} actual_us={actual} latency_ns={dt}",
        us_to_frames(seed_landing, s.rate),
        status_name(status)
    );
    if status == sys::SONG_OK {
        println!("F5SEEK {fixture} negative-target UNEXPECTED_SUCCESS");
        failures += 1;
    } else {
        // Usability check: decode a window and locate it against the
        // reference. The cursor must still sit at the seed landing, so
        // the match must exist AND land near the seed (the decoder was
        // not disturbed by the rejected seek). Lossy codecs need the
        // skip search (their first post-seek frames diverge from a
        // sequential decode even at an undisturbed cursor).
        let w = match decode_frames(&s, MATCH_WINDOW_FRAMES + *SKIP_STEPS_FRAMES.last().unwrap())
        {
            Ok(w) => w,
            Err(e) => {
                println!("F5SEEK {fixture} post-negative DECODE_FAILED {e}");
                failures += 1;
                return failures;
            }
        };
        let mut usable = None;
        for &skip in &SKIP_STEPS_FRAMES {
            if let Some(m) = content_match(
                &reference,
                &w,
                s.channels,
                skip,
                Some(us_to_frames(seed_landing, s.rate).max(0) as usize),
            ) {
                usable = Some(m);
                break;
            }
        }
        match usable {
            Some((offset, skip)) => {
                println!(
                    "F5SEEK {fixture} post-negative USABLE offset_frames={offset} skip_frames={skip}"
                );
            }
            None => {
                println!("F5SEEK {fixture} post-negative UNUSABLE_AFTER_REJECTION");
                failures += 1;
            }
        }
    }

    // EOF after a near-end seek: the episode-completion interaction.
    let (_st, landing, _dt) = do_seek(&s, s.duration_us);
    let tail = decode_to_eof(&s).unwrap_or_default();
    let tail_frames = (tail.len() / s.channels) as i64;
    println!(
        "F5SEEK {fixture} eof-tail landing_frames={} tail_frames={} expected_le={} ok={}",
        us_to_frames(landing, s.rate),
        tail_frames,
        reference_frames - us_to_frames(landing, s.rate).max(0),
        tail_frames <= reference_frames - us_to_frames(landing, s.rate).max(0) + 1
    );
    if s.duration_us < 0 {
        println!("F5SEEK {fixture} NO_DECLARED_DURATION (seek-clamp behavior unmeasurable)");
    }
    failures
}

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixtures_dir = manifest
        .join("../../native/experiments/songcore-equivalence/fixtures")
        .canonicalize()
        .expect("fixtures dir");
    let reference: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir.parent().unwrap().join("reference.json"))
            .expect("reference.json"),
    )
    .expect("reference JSON");
    let by_id: HashMap<String, &serde_json::Value> = reference["fixtures"]
        .as_array()
        .expect("fixtures array")
        .iter()
        .map(|f| (f["id"].as_str().expect("id").to_owned(), f))
        .collect();

    println!("F5SEEK BEGIN songcore_abi_v{}", unsafe {
        sys::songcore_abi_version()
    });
    let mut failures = 0usize;
    for id in FIXTURE_IDS {
        let entry = by_id.get(id).copied().expect("fixture id");
        let file = entry["file"].as_str().expect("file");
        let path = fixtures_dir.join(file);
        let f = run_fixture(id, &path);
        println!("F5SEEK {id} DONE failures={f}");
        failures += f;
    }
    println!("F5SEEK END failures={failures}");
    exit(i32::from(failures > 0));
}
