//! Real decode endpoint correctness against the frozen reference corpus.
//!
//! Expected values come from the independent reference manifest
//! (`native/experiments/songcore-equivalence/reference.json` — formats,
//! frame counts, full-drain PCM SHA-256), never from re-deriving what the
//! endpoint produces.

use crate::test_common as common;

use std::cell::RefCell;

use std::path::Path;
use std::rc::Rc;

use crate::{SeekClass, SongcoreDecode, classify_seek_status, songcore_decode_plugin};
use qianqian_app::QianqianApp;
use qianqian_audio_api::ports::{DecodeOutcome, PcmDecode, PcmDecodeCapability};
use qianqian_composition::{ComponentSpec, DesiredEntry, Revision};
use qianqian_songcore_sys as sys;

fn service() -> SongcoreDecode {
    SongcoreDecode::new().expect("SongCore mechanism binds fail-closed")
}

#[test]
fn opens_real_files_with_reference_format() {
    for id in ["mp3-cbr-id3v23", "flac-16-44-stereo"] {
        let (fx, path) = common::load_fixture(id);
        let decode_stream = service().open_media(&path).unwrap_or_else(|e| {
            panic!("{id}: real file must open: {}", e.message);
        });
        let fmt = decode_stream.format();
        assert_eq!(
            fmt.sample_rate, fx.sample_rate,
            "{id}: reference sample rate"
        );
        assert_eq!(
            fmt.channels as u32, fx.channels,
            "{id}: reference channel count"
        );
    }
}

#[test]
fn drains_exact_reference_frame_count_then_stable_eof() {
    let (fx, path) = common::load_fixture("mp3-cbr-id3v23");
    let mut decode_stream = service().open_media(&path).expect("opens");

    let channels = decode_stream.format().channels as usize;
    let mut block = vec![0.0f32; 1024 * channels];
    let mut total = 0usize;
    loop {
        match decode_stream
            .read_frames(&mut block)
            .expect("read succeeds")
        {
            DecodeOutcome::Frames(n) => total += n,
            DecodeOutcome::Eof => break,
        }
    }
    assert_eq!(total, fx.pcm_frames, "reference frame count");
    // EOF is terminal and stable, not one-shot.
    assert_eq!(
        decode_stream
            .read_frames(&mut block)
            .expect("read after EOF"),
        DecodeOutcome::Eof,
        "EOF stays terminal"
    );
}

#[test]
fn full_drain_pcm_matches_reference_sha256() {
    for id in ["mp3-cbr-id3v23", "flac-16-44-stereo", "alac-16-44-stereo"] {
        let (fx, path) = common::load_fixture(id);
        let mut decode_stream = service().open_media(&path).expect("opens");

        let channels = decode_stream.format().channels as usize;
        let mut hasher = common::Sha256::new();
        let mut block = vec![0.0f32; 1024 * channels];
        let mut total = 0usize;
        loop {
            match decode_stream
                .read_frames(&mut block)
                .expect("read succeeds")
            {
                DecodeOutcome::Frames(n) => {
                    let samples = n * channels;
                    let bytes: Vec<u8> = block[..samples]
                        .iter()
                        .flat_map(|f| f.to_le_bytes())
                        .collect();
                    hasher.update(&bytes);
                    total += n;
                }
                DecodeOutcome::Eof => break,
            }
        }
        assert_eq!(total, fx.pcm_frames, "{id}: frame count");
        assert_eq!(hasher.hex(), fx.pcm_sha256, "{id}: full-drain PCM identity");
    }
}

#[test]
fn missing_file_is_an_open_error() {
    let err = service()
        .open_media(Path::new("/nonexistent/qianqian-test media.mp3"))
        .err()
        .expect("missing file must not open");
    assert!(!err.message.is_empty());
}

#[test]
fn undecodable_file_is_an_open_error() {
    let path = std::env::temp_dir().join("qianqian-decode-test-garbage.bin");
    std::fs::write(&path, b"this is not a media container at all").expect("temp file");
    let result = service().open_media(&path);
    let _ = std::fs::remove_file(&path);
    assert!(result.is_err(), "garbage bytes must not open as media");
}

/// NATIVE-BOUNDARY-AUDIT-0 adversarial coverage（round record：Git 历史 / PR #134）: a zero-length source is
/// not a media file and must fail at open, not produce a zombie endpoint.
#[test]
fn zero_length_file_is_an_open_error() {
    let path = std::env::temp_dir().join("qianqian-decode-test-empty.mp3");
    std::fs::write(&path, b"").expect("temp file");
    let result = service().open_media(&path);
    let _ = std::fs::remove_file(&path);
    assert!(result.is_err(), "empty file must not open as media");
}

/// NATIVE-BOUNDARY-AUDIT-0 adversarial coverage（round record：Git 历史 / PR #134）: a truncated container
/// must end deterministically — a clean EOF or a typed decode error —
/// never a crash or a hang.
#[test]
fn truncated_file_drains_to_eof_or_typed_error() {
    let (fx, real) = common::load_fixture("mp3-cbr-id3v23");
    let bytes = std::fs::read(&real).expect("fixture readable");
    let truncated_path = std::env::temp_dir().join("qianqian-decode-test-truncated.mp3");
    std::fs::write(&truncated_path, &bytes[..bytes.len() / 3]).expect("temp file");
    let opened = service().open_media(&truncated_path);
    match opened {
        Err(_) => {} // refusing to open a broken container is honest
        Ok(mut decode_stream) => {
            let channels = decode_stream.format().channels as usize;
            let mut block = vec![0.0f32; 1024 * channels];
            let mut total = 0usize;
            // A typed decode error is an accepted terminal here: the
            // assertion is that the drain *ends* — clean EOF or typed
            // error, never a crash or a hang.
            while let Ok(outcome) = decode_stream.read_frames(&mut block) {
                match outcome {
                    DecodeOutcome::Frames(n) => total += n,
                    DecodeOutcome::Eof => break,
                }
            }
            assert!(total <= fx.pcm_frames, "truncation cannot add frames");
        }
    }
    let _ = std::fs::remove_file(&truncated_path);
}

/// NATIVE-BOUNDARY-AUDIT-0 adversarial coverage（round record：Git 历史 / PR #134）: dropping an endpoint
/// mid-stream releases the native handle immediately; the same file must
/// reopen cleanly afterwards (drop-before-EOF and repeated open/close).
#[test]
fn drop_before_eof_then_repeated_reopens_are_stable() {
    let (_, path) = common::load_fixture("flac-16-44-stereo");
    {
        let mut decode_stream = service().open_media(&path).expect("opens");
        let channels = decode_stream.format().channels as usize;
        let mut block = vec![0.0f32; 1024 * channels];
        assert!(matches!(
            decode_stream.read_frames(&mut block).expect("first read"),
            DecodeOutcome::Frames(_)
        ));
        // Drop mid-stream: the endpoint's Drop releases the native handle.
    }
    for cycle in 0..16 {
        let mut decode_stream = service()
            .open_media(&path)
            .unwrap_or_else(|e| panic!("cycle {cycle}: reopen must succeed: {}", e.message));
        let channels = decode_stream.format().channels as usize;
        let mut block = vec![0.0f32; 1024 * channels];
        assert!(
            matches!(
                decode_stream.read_frames(&mut block).expect("reads"),
                DecodeOutcome::Frames(_)
            ),
            "cycle {cycle}: reopened endpoint produces PCM"
        );
    }
}

#[test]
fn two_endpoints_from_one_service_are_independent() {
    let (_, mp3) = common::load_fixture("mp3-cbr-id3v23");
    let (_, flac) = common::load_fixture("flac-16-44-stereo");
    let svc = service();
    let mut a = svc.open_media(&mp3).expect("opens");
    let mut b = svc.open_media(&flac).expect("opens");
    let mut buf = vec![0.0f32; 1024 * 2];
    assert!(matches!(
        a.read_frames(&mut buf).expect("a reads"),
        DecodeOutcome::Frames(_)
    ));
    assert!(matches!(
        b.read_frames(&mut buf).expect("b reads"),
        DecodeOutcome::Frames(_)
    ));
}

/// The kernel-mediated seam the Playback Session will use: the plugin
/// publishes the capability, a consumer resolves it once and opens real
/// media through the resolved service.
#[test]
fn plugin_publishes_capability_that_opens_real_media_through_the_kernel() {
    let (fx, path) = common::load_fixture("mp3-cbr-id3v23");
    let observed: Rc<RefCell<Option<(u32, usize)>>> = Rc::new(RefCell::new(None));
    let observed_in_activate = observed.clone();
    let open_path = path.clone();

    let consumer = ComponentSpec::new("decode_probe_consumer")
        .requires::<PcmDecodeCapability>()
        .on_activate(move |ctx| {
            let binding = ctx
                .resolve::<PcmDecodeCapability>()
                .expect("decode resolves");
            let mut decode_stream = binding
                .service()
                .open_media(&open_path)
                .expect("real file opens via resolved capability");
            let rate = decode_stream.format().sample_rate;
            let mut buf = vec![0.0f32; 1024 * 2];
            let n = match decode_stream.read_frames(&mut buf).expect("reads") {
                DecodeOutcome::Frames(n) => n,
                DecodeOutcome::Eof => 0,
            };
            *observed_in_activate.borrow_mut() = Some((rate, n));
            Ok(())
        });

    let mut runtime = QianqianApp::new();
    runtime
        .register_component(songcore_decode_plugin())
        .expect("legal");
    runtime.register_component(consumer).expect("legal");
    runtime
        .revise_desired(vec![
            DesiredEntry::enabled("decode", "songcore_decode_plugin", Revision::new(1)),
            DesiredEntry::enabled("probe", "decode_probe_consumer", Revision::new(1)),
        ])
        .expect("composition is legal");

    let snap = runtime.composition_snapshot();
    assert_eq!(
        snap.capabilities.get("PcmDecode").map(|p| p.as_deref()),
        Some(Some("decode")),
        "the real decode capability binding is kernel truth"
    );
    let (rate, first_frames) = observed
        .borrow()
        .expect("consumer probed the real endpoint");
    assert_eq!(rate, fx.sample_rate);
    assert!(
        first_frames > 0,
        "real PCM flowed through the resolved capability"
    );
}

// --- refusal-class boundary probes (F5-SEEK-IMPLEMENTATION-CORRECTIVE-2) ---
//
// The session may resume old-cursor production ONLY on a provider
// `RefusedUnchanged`, so which raw statuses earn that class is
// load-bearing. Both refusal-path statuses are defensive at the endpoint
// (a constructed stream is always probed, and the µs target is clamped
// non-negative), so these probes drive the RAW ABI to pin the two facts
// the classification rests on: that `SONG_ERR_NOT_OPEN` is live and
// certifies no continuation, and that `SONG_ERR_INVALID_ARGUMENT` leaves
// the continuation bit-identical (the E1 usability claim, re-earned as an
// executable oracle rather than restated).

/// One raw ABI handle over a real fixture, with the same host-IO
/// callbacks the endpoint uses. `probe` selects the ABI state under test:
/// `false` leaves the handle opened but NOT probed, the only state that
/// makes `song_seek` answer `SONG_ERR_NOT_OPEN`.
struct RawProbe {
    handle: *mut sys::song_handle,
    /// Owns the host file across the handle's lifetime; reclaimed on drop
    /// after song_close (the callbacks borrow it raw).
    file: *mut std::fs::File,
}

impl RawProbe {
    fn open(path: &Path, probe: bool) -> Self {
        let file = Box::into_raw(Box::new(std::fs::File::open(path).expect("fixture opens")));
        let io = sys::song_io {
            userdata: file as *mut _,
            read: Some(raw_read),
            seek: Some(raw_seek),
            size: Some(raw_size),
        };
        let mut handle = std::ptr::null_mut();
        let status = unsafe { sys::song_open(&io, &mut handle) };
        assert_eq!(status, sys::SONG_OK, "raw open failed: {status}");
        if probe {
            let mut info: sys::song_info = unsafe { std::mem::zeroed() };
            let status = unsafe { sys::song_probe(handle, &mut info) };
            assert_eq!(status, sys::SONG_OK, "raw probe failed: {status}");
        }
        Self { handle, file }
    }

    /// Read `blocks` blocks of `frames` and concatenate the produced
    /// samples.
    fn read_blocks(&mut self, channels: usize, frames: usize, blocks: usize) -> Vec<f32> {
        let mut out = Vec::new();
        for _ in 0..blocks {
            let mut buf = vec![0.0f32; frames * channels];
            let mut produced = 0u64;
            let status = unsafe {
                sys::song_read_pcm(self.handle, buf.as_mut_ptr(), frames as u64, &mut produced)
            };
            assert_eq!(status, sys::SONG_OK, "raw read failed: {status}");
            buf.truncate(produced as usize * channels);
            out.extend_from_slice(&buf);
        }
        out
    }
}

impl Drop for RawProbe {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                sys::song_close(self.handle);
            }
            if !self.file.is_null() {
                drop(Box::from_raw(self.file));
            }
        }
    }
}

unsafe extern "C" fn raw_read(ud: *mut std::ffi::c_void, dst: *mut u8, size: usize) -> i64 {
    if ud.is_null() || dst.is_null() {
        return -1;
    }
    use std::io::Read;
    let f = unsafe { &mut *(ud as *mut std::fs::File) };
    let buf = unsafe { std::slice::from_raw_parts_mut(dst, size) };
    f.read(buf).map_or(-1, |n| n as i64)
}

unsafe extern "C" fn raw_seek(ud: *mut std::ffi::c_void, off: i64) -> i64 {
    if ud.is_null() || off < 0 {
        return -1;
    }
    use std::io::{Seek, SeekFrom};
    let f = unsafe { &mut *(ud as *mut std::fs::File) };
    f.seek(SeekFrom::Start(off as u64)).map_or(-1, |n| n as i64)
}

unsafe extern "C" fn raw_size(ud: *mut std::ffi::c_void) -> i64 {
    if ud.is_null() {
        return -1;
    }
    let f = unsafe { &mut *(ud as *mut std::fs::File) };
    f.metadata().map_or(-1, |m| m.len() as i64)
}

/// `SONG_ERR_NOT_OPEN` is a live ABI status, and it must never be a
/// refusal: the handle it reports on is not in an opened/probed state, so
/// there is no proven-usable old cursor to resume.
#[test]
fn an_unprobed_handle_reports_not_open_and_is_never_a_refusal() {
    let (_, path) = common::load_fixture("flac-16-44-stereo");
    let raw = RawProbe::open(&path, false);
    let mut landing = 0i64;
    let status = unsafe { sys::song_seek(raw.handle, 0, &mut landing) };
    assert_eq!(
        status,
        sys::SONG_ERR_NOT_OPEN,
        "the ABI's not-opened status"
    );
    assert_eq!(
        classify_seek_status(status),
        SeekClass::MutatedThenFailed,
        "a not-opened handle certifies no continuation and must fail closed"
    );
}

/// The usability claim the `INVALID_ARGUMENT` refusal class rests on,
/// re-earned at the ABI: after the rejection, the decode continuation is
/// bit-identical to a control handle that never seeked. A check that ran
/// after (or a flush that ran before) the rejection would diverge here.
#[test]
fn an_invalid_argument_refusal_leaves_the_decode_continuation_intact() {
    let (_, path) = common::load_fixture("flac-16-44-stereo");
    const CHANNELS: usize = 2;
    const FRAMES: usize = 1024;

    let mut control = RawProbe::open(&path, true);
    let mut probed = RawProbe::open(&path, true);

    let prefix = probed.read_blocks(CHANNELS, FRAMES, 2);
    assert!(!prefix.is_empty(), "the probe decoded a prefix");
    assert_eq!(
        prefix,
        control.read_blocks(CHANNELS, FRAMES, 2),
        "both handles decode the same prefix before the seek"
    );

    let mut landing = 0i64;
    let status = unsafe { sys::song_seek(probed.handle, -1, &mut landing) };
    assert_eq!(status, sys::SONG_ERR_INVALID_ARGUMENT);
    assert_eq!(
        classify_seek_status(status),
        SeekClass::RefusedUnchanged,
        "only this status earns the resume-old-cursor class"
    );

    assert_eq!(
        probed.read_blocks(CHANNELS, FRAMES, 4),
        control.read_blocks(CHANNELS, FRAMES, 4),
        "a proven pre-mutation refusal must not disturb the continuation"
    );
}
