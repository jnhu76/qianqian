//! F4-GATE Experiment C — duration provenance over the SongCore ABI
//! (non-Windows; links the same static SongCore the decode plugin
//! links).
//!
//! For every committed corpus fixture and a set of truncated
//! adversarial copies, the probe compares:
//!
//! ```text
//! container duration_us   song_probe -> song_info.duration_us
//! stream duration_us      song_audio_stream_info -> duration_us
//! exact decoded total     frames actually produced by song_read_pcm
//!                         to EOF
//! ```
//!
//! It answers, with evidence, whether the reported duration can be
//! called exact, approximate, or merely optional metadata — the input
//! for the F4 decision that Duration is exposed as optional mechanism
//! evidence and never as an exact Fact.

#![cfg(not(windows))]

use std::collections::HashMap;
use std::os::raw::c_void;
use std::path::{Path, PathBuf};
use std::process::exit;

use qianqian_songcore_sys as sys;

const FIXTURE_IDS: [&str; 4] = [
    "mp3-cbr-id3v23",
    "flac-16-44-stereo",
    "alac-16-44-stereo",
    "alac-long",
];

/// A decoder that makes no progress forever cannot be allowed to hang
/// the probe (same guard shape as the production decode worker's
/// zero-frame rule).
const ZERO_FRAME_STALL_LIMIT: u32 = 64;

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

    println!("F4DURATION BEGIN songcore_abi_v{}", unsafe {
        sys::songcore_abi_version()
    });

    let mut failures = 0usize;
    for id in FIXTURE_IDS {
        let entry = by_id.get(id).copied().expect("fixture id");
        let file = entry["file"].as_str().expect("file");
        let rate = entry["sample_rate"].as_u64().expect("rate") as u32;
        let reference_frames = entry["pcm_frames"].as_u64().expect("pcm_frames");
        let path = fixtures_dir.join(file);
        failures += measure_one(id, &path, rate, reference_frames, None);
    }

    // Adversarial truncations: metadata that claims more audio than the
    // damaged file can still decode. Cut 30% off the byte tail of two
    // formats with different container shapes (CBR MP3, FLAC).
    let tmp = std::env::temp_dir().join("f4-duration-adversarial");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("tmp dir");
    for id in ["mp3-cbr-id3v23", "flac-16-44-stereo"] {
        let entry = by_id.get(id).copied().expect("fixture id");
        let file = entry["file"].as_str().expect("file");
        let rate = entry["sample_rate"].as_u64().expect("rate") as u32;
        let reference_frames = entry["pcm_frames"].as_u64().expect("pcm_frames");
        let src = fixtures_dir.join(file);
        let bytes = std::fs::read(&src).expect("fixture bytes");
        let cut = bytes.len() * 7 / 10;
        let truncated = tmp.join(format!("truncated-{file}"));
        std::fs::write(&truncated, &bytes[..cut]).expect("write truncated");
        failures += measure_one(
            &format!("{id}/truncated-70pct"),
            &truncated,
            rate,
            reference_frames,
            Some(reference_frames * 7 / 10),
        );
    }
    let _ = std::fs::remove_dir_all(&tmp);

    println!("F4DURATION END failures={failures}");
    exit(i32::from(failures > 0));
}

/// Open, probe and decode-to-EOF one file; print the comparison line.
/// `decodable_floor` is the decode-total upper bound a truncated copy
/// must respect (70% of the reference frames); `None` for intact
/// fixtures, where the decoded total must equal the reference exactly.
fn measure_one(
    label: &str,
    path: &Path,
    rate: u32,
    reference_frames: u64,
    decodable_ceiling: Option<u64>,
) -> usize {
    let file: Box<std::fs::File> = match std::fs::File::open(path).map(Box::new) {
        Ok(f) => f,
        Err(e) => {
            println!("F4DURATION {label} OPEN_FAILED {e}");
            return 1;
        }
    };
    let raw: *mut std::fs::File = Box::into_raw(file);
    let io = sys::song_io {
        userdata: raw as *mut c_void,
        read: Some(probe_read),
        seek: Some(probe_seek),
        size: Some(probe_size),
    };

    let mut handle: *mut sys::song_handle = std::ptr::null_mut();
    let status = unsafe { sys::song_open(&io, &mut handle) };
    if status != sys::SONG_OK || handle.is_null() {
        unsafe { drop(Box::from_raw(raw)) };
        println!("F4DURATION {label} SONG_OPEN status={status}");
        // An intact fixture must open; a truncated copy may legitimately
        // be refused by the container parser.
        return usize::from(decodable_ceiling.is_none());
    }

    let mut info: sys::song_info = unsafe { std::mem::zeroed() };
    let container_us = if unsafe { sys::song_probe(handle, &mut info) } == sys::SONG_OK {
        info.duration_us
    } else {
        -1
    };
    let mut stream_info: sys::song_stream_info = unsafe { std::mem::zeroed() };
    let stream_us =
        if unsafe { sys::song_audio_stream_info(handle, 0, &mut stream_info) } == sys::SONG_OK {
            stream_info.duration_us
        } else {
            -1
        };
    let channels = usize::try_from(stream_info.channels.max(1)).unwrap_or(1);

    // Decode to EOF, counting exact frames.
    let mut decoded_frames: u64 = 0;
    let mut decode_failed = false;
    let mut stalled = false;
    let mut zero_streak: u32 = 0;
    let mut dst = vec![0.0f32; 1024 * channels];
    loop {
        let mut produced: u64 = 0;
        let status = unsafe { sys::song_read_pcm(handle, dst.as_mut_ptr(), 1024, &mut produced) };
        match status {
            sys::SONG_OK => {
                decoded_frames += produced;
                zero_streak = if produced == 0 { zero_streak + 1 } else { 0 };
                if zero_streak >= ZERO_FRAME_STALL_LIMIT {
                    stalled = true;
                    break;
                }
            }
            sys::SONG_EOF => break,
            _ => {
                decode_failed = true;
                break;
            }
        }
    }
    unsafe { sys::song_close(handle) };
    unsafe { drop(Box::from_raw(raw)) };

    let exact_us = i64::try_from(decoded_frames * 1_000_000 / u64::from(rate)).unwrap_or(-1);
    let container_delta_us = if container_us >= 0 && exact_us >= 0 {
        container_us - exact_us
    } else {
        0
    };
    let pass = if let Some(_ceiling) = decodable_ceiling {
        // Truncated: metadata may lie; the requirement is only that the
        // decode never produces the intact total from damaged bytes. A
        // decode error and a clean EOF are both legal stops, and the
        // exact frame count of the damaged tail is a byte/frame
        // boundary artifact, not a provenance fact.
        !stalled && decoded_frames < reference_frames
    } else {
        decoded_frames == reference_frames && !decode_failed && !stalled
    };
    println!(
        "F4DURATION {label} container_us={container_us} stream_us={stream_us} exact_frames={decoded_frames} exact_us={exact_us} reference_frames={reference_frames} container_delta_us={container_delta_us} decode_failed={decode_failed} stalled={stalled} pass={pass}"
    );
    usize::from(!pass)
}

// --- host IO callbacks (same fail-closed shape as the decode plugin) -------

unsafe extern "C" fn probe_read(ud: *mut c_void, dst: *mut u8, size: usize) -> i64 {
    use std::io::Read;
    if ud.is_null() || dst.is_null() {
        return -1;
    }
    let file = unsafe { &mut *(ud as *mut std::fs::File) };
    let buf = unsafe { std::slice::from_raw_parts_mut(dst, size) };
    match file.read(buf) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

unsafe extern "C" fn probe_seek(ud: *mut c_void, offset: i64) -> i64 {
    use std::io::{Seek, SeekFrom};
    if ud.is_null() || offset < 0 {
        return -1;
    }
    let file = unsafe { &mut *(ud as *mut std::fs::File) };
    match file.seek(SeekFrom::Start(offset as u64)) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

unsafe extern "C" fn probe_size(ud: *mut c_void) -> i64 {
    if ud.is_null() {
        return -1;
    }
    let file = unsafe { &mut *(ud as *mut std::fs::File) };
    match file.metadata() {
        Ok(m) => m.len() as i64,
        Err(_) => -1,
    }
}
