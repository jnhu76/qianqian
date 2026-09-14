//! Real decode endpoint correctness against the frozen reference corpus.
//!
//! Expected values come from the independent reference manifest
//! (`native/experiments/songcore-equivalence/reference.json` — formats,
//! frame counts, full-drain PCM SHA-256), never from re-deriving what the
//! endpoint produces.

mod common;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use qianqian_app::QianqianApp;
use qianqian_audio_api::ports::{DecodeOutcome, PcmDecode, PcmDecodeCapability};
use qianqian_composition::{ComponentSpec, CompositionKernel, DesiredEntry, Revision};
use qianqian_decode_songcore::{SongcoreDecode, songcore_decode_plugin};

use common::ReferenceFixture;

fn fixtures_dir() -> PathBuf {
    common::fixtures_dir()
}

fn service() -> SongcoreDecode {
    SongcoreDecode::new().expect("SongCore mechanism binds fail-closed")
}

#[test]
fn opens_real_files_with_reference_format() {
    for id in ["mp3-cbr-id3v23", "flac-16-44-stereo"] {
        let (fx, path) = common::load_fixture(id);
        let mut decode_stream = service().open_media(&path).unwrap_or_else(|e| {
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

/// NATIVE-BOUNDARY-AUDIT-0 adversarial coverage: a zero-length source is
/// not a media file and must fail at open, not produce a zombie endpoint.
#[test]
fn zero_length_file_is_an_open_error() {
    let path = std::env::temp_dir().join("qianqian-decode-test-empty.mp3");
    std::fs::write(&path, b"").expect("temp file");
    let result = service().open_media(&path);
    let _ = std::fs::remove_file(&path);
    assert!(result.is_err(), "empty file must not open as media");
}

/// NATIVE-BOUNDARY-AUDIT-0 adversarial coverage: a truncated container
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

/// NATIVE-BOUNDARY-AUDIT-0 adversarial coverage: dropping an endpoint
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
