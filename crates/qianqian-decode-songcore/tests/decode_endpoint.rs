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

use qianqian_core::ports::{DecodeOutcome, PcmDecode, PcmDecodeCapability};
use qianqian_decode_songcore::{SongcoreDecode, songcore_decode_plugin};
use qianqian_kernel::{ComponentSpec, DesiredEntry, Kernel, Revision};
use qianqian_runtime::AppRuntime;

use common::ReferenceFixture;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/experiments/songcore-equivalence/fixtures")
}

fn load_fixture(id: &str) -> (ReferenceFixture, PathBuf) {
    let fx = ReferenceFixture::load(
        &fixtures_dir().parent().unwrap().join("reference.json"),
        id,
    );
    let path = fixtures_dir().join(&fx.file);
    fx.verify_file_identity(&path);
    (fx, path)
}

fn service() -> SongcoreDecode {
    SongcoreDecode::new().expect("SongCore mechanism binds fail-closed")
}

#[test]
fn opens_real_files_with_reference_format() {
    for id in ["mp3-cbr-id3v23", "flac-16-44-stereo"] {
        let (fx, path) = load_fixture(id);
        let mut src = service().open_source(&path).unwrap_or_else(|e| {
            panic!("{id}: real file must open: {}", e.message);
        });
        let fmt = src.format();
        assert_eq!(fmt.sample_rate, fx.sample_rate, "{id}: reference sample rate");
        assert_eq!(fmt.channels as u32, fx.channels, "{id}: reference channel count");
    }
}

#[test]
fn drains_exact_reference_frame_count_then_stable_eof() {
    let (fx, path) = load_fixture("mp3-cbr-id3v23");
    let mut src = service().open_source(&path).expect("opens");

    let channels = src.format().channels as usize;
    let mut block = vec![0.0f32; 1024 * channels];
    let mut total = 0usize;
    loop {
        match src.read_frames(&mut block).expect("read succeeds") {
            DecodeOutcome::Frames(n) => total += n,
            DecodeOutcome::Eof => break,
        }
    }
    assert_eq!(total, fx.pcm_frames, "reference frame count");
    // EOF is terminal and stable, not one-shot.
    assert_eq!(
        src.read_frames(&mut block).expect("read after EOF"),
        DecodeOutcome::Eof,
        "EOF stays terminal"
    );
}

#[test]
fn full_drain_pcm_matches_reference_sha256() {
    for id in ["mp3-cbr-id3v23", "flac-16-44-stereo", "alac-16-44-stereo"] {
        let (fx, path) = load_fixture(id);
        let mut src = service().open_source(&path).expect("opens");

        let channels = src.format().channels as usize;
        let mut hasher = common::Sha256::new();
        let mut block = vec![0.0f32; 1024 * channels];
        let mut total = 0usize;
        loop {
            match src.read_frames(&mut block).expect("read succeeds") {
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
        .open_source(Path::new("/nonexistent/qianqian-test media.mp3"))
        .err()
        .expect("missing file must not open");
    assert!(!err.message.is_empty());
}

#[test]
fn undecodable_file_is_an_open_error() {
    let path = std::env::temp_dir().join("qianqian-decode-test-garbage.bin");
    std::fs::write(&path, b"this is not a media container at all").expect("temp file");
    let result = service().open_source(&path);
    let _ = std::fs::remove_file(&path);
    assert!(result.is_err(), "garbage bytes must not open as media");
}

#[test]
fn two_endpoints_from_one_service_are_independent() {
    let (_, mp3) = load_fixture("mp3-cbr-id3v23");
    let (_, flac) = load_fixture("flac-16-44-stereo");
    let svc = service();
    let mut a = svc.open_source(&mp3).expect("opens");
    let mut b = svc.open_source(&flac).expect("opens");
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
    let (fx, path) = load_fixture("mp3-cbr-id3v23");
    let observed: Rc<RefCell<Option<(u32, usize)>>> = Rc::new(RefCell::new(None));
    let observed_in_activate = observed.clone();
    let open_path = path.clone();

    let consumer = ComponentSpec::new("decode_probe_consumer")
        .requires::<PcmDecodeCapability>()
        .on_activate(move |ctx| {
            let binding = ctx.resolve::<PcmDecodeCapability>().expect("decode resolves");
            let mut src = binding
                .service()
                .open_source(&open_path)
                .expect("real file opens via resolved capability");
            let rate = src.format().sample_rate;
            let mut buf = vec![0.0f32; 1024 * 2];
            let n = match src.read_frames(&mut buf).expect("reads") {
                DecodeOutcome::Frames(n) => n,
                DecodeOutcome::Eof => 0,
            };
            *observed_in_activate.borrow_mut() = Some((rate, n));
            Ok(())
        });

    let mut runtime = AppRuntime::new();
    runtime.register_component(songcore_decode_plugin()).expect("legal");
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
    let (rate, first_frames) = observed.borrow().expect("consumer probed the real endpoint");
    assert_eq!(rate, fx.sample_rate);
    assert!(first_frames > 0, "real PCM flowed through the resolved capability");
}
