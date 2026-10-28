//! #324 S2 — Windows native Rust adapter smoke.
//!
//! Real path: `CompositionKernel` → decode plugin activation → `PcmDecode`
//! capability → `SongCore` FFI → native FFmpeg decode. This is the same
//! production adapter the twelve fault cells drive; the smoke pins the healthy
//! baseline (full decode to EOF, seek, post-seek read) so the fault cells have
//! a comparable reference.

// This harness is Windows-only: it links kernel32 and drives Win32
// handles/pipes. A non-Windows host must compile it to zero tests rather than
// fail at link time (the crate is workspace-excluded and not CI-compiled).
#![cfg(windows)]

#[allow(dead_code)]
mod common;

use std::cell::RefCell;
use std::io::Write as _;
use std::path::PathBuf;
use std::rc::Rc;

use qianqian_audio_api::ports::{DecodeOutcome, PcmDecode, PcmDecodeCapability};
use qianqian_composition::{ComponentSpec, CompositionKernel, DesiredEntry, FiberState, Revision};

fn line(s: &str) {
    writeln!(std::io::stdout(), "{s}").expect("stdout receipt");
    std::io::stdout().flush().expect("stdout receipt flush");
}

fn activate_decode_plugin_and_capture() -> Rc<dyn PcmDecode> {
    thread_local! {
        static CAPTURED: RefCell<Option<Rc<dyn PcmDecode>>> = const { RefCell::new(None) };
    }
    let mut k = CompositionKernel::new();
    k.register_component(qianqian_decode_songcore::songcore_decode_plugin())
        .expect("decode plugin registers");
    k.register_component(
        ComponentSpec::new("smoke_consumer")
            .requires::<PcmDecodeCapability>()
            .on_activate(|ctx| {
                let svc = ctx
                    .resolve::<PcmDecodeCapability>()
                    .expect("PcmDecode resolvable")
                    .service();
                CAPTURED.with(|c| *c.borrow_mut() = Some(svc));
                Ok(())
            }),
    )
    .expect("consumer component registers");
    k.set_desired(vec![
        DesiredEntry::enabled(
            "songcore_decode_plugin",
            "songcore_decode_plugin",
            Revision::fresh(),
        ),
        DesiredEntry::enabled("smoke_consumer", "smoke_consumer", Revision::fresh()),
    ])
    .expect("both fibers desired");
    k.settle();
    let snap = k.snapshot();
    for name in ["songcore_decode_plugin", "smoke_consumer"] {
        let fiber = snap
            .fibers
            .get(name)
            .unwrap_or_else(|| panic!("{name} fiber present"));
        assert_eq!(
            fiber.state,
            FiberState::Active,
            "{name} must be active"
        );
    }
    CAPTURED.with(|c| c.borrow().as_ref().expect("service captured").clone())
}

fn artifact_sha256() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/build/artifacts/libsongcore.a");
    let bytes = std::fs::read(&path).expect("native artifact readable");
    let mut hasher = common::Sha256::new();
    hasher.update(&bytes);
    hasher.hex()
}

#[test]
fn s2_rust_adapter_smoke() {
    std::panic::set_hook(Box::new(|info| line(&format!("ASSERTION {info}"))));
    let sha = artifact_sha256();
    line(&format!("PLATFORM=windows-x86_64 SMOKE=S2_RUST_ADAPTER"));
    line(&format!("ARTIFACT_SHA256={sha}"));
    line(&format!(
        "ARTIFACT_BUILD_TIME_SHA256_RAW={}",
        qianqian_songcore_sys::QN_SONGCORE_ARTIFACT_SHA256
    ));
    let embedded = qianqian_songcore_sys::QN_SONGCORE_ARTIFACT_SHA256
        .trim_start_matches('\\')
        .trim();
    assert_eq!(sha, embedded, "the binary is linked against the artifact on disk");

    let (_fx, path) = common::load_fixture("flac-16-44-stereo");
    let abi = unsafe { qianqian_songcore_sys::songcore_abi_version() };
    line(&format!("SONGCORE_ABI_VERSION={abi}"));
    assert_eq!(abi, qianqian_songcore_sys::SONGCORE_ABI_VERSION);

    let registration = qianqian_diagnostics::AlphaRegistration::register();
    let decode = activate_decode_plugin_and_capture();
    let mut stream = decode.open_media(&path).expect("clean fixture opens");
    let format = stream.format();
    line(&format!(
        "FORMAT rate={} channels={} mask={}",
        format.sample_rate, format.channels, format.channel_mask
    ));
    assert_eq!(format.sample_rate, 44_100);
    assert_eq!(format.channels, 2);

    let mut buf = vec![0f32; 8192 * format.channels as usize];
    let mut frames = 0usize;
    let mut reads = 0usize;
    loop {
        match stream.read_frames(&mut buf) {
            Ok(DecodeOutcome::Frames(n)) if n > 0 => {
                frames += n;
                reads += 1;
            }
            Ok(DecodeOutcome::Eof) => break,
            Ok(other) => panic!("unexpected outcome: {other:?}"),
            Err(error) => panic!("clean fixture must not fail: {}", error.message),
        }
        assert!(reads < 10_000, "decode must terminate");
    }
    line(&format!("DECODE frames={frames} reads={reads} eof=1"));
    assert_eq!(frames, 176_400, "four seconds at 44.1 kHz");

    let outcome = stream.seek(std::time::Duration::from_micros(1_000_000));
    line(&format!("SEEK outcome={outcome:?}"));
    match outcome {
        qianqian_audio_api::ports::ProviderSeekOutcome::Applied { .. } => {}
        other => panic!("seek must apply on a clean lossless source, got {other:?}"),
    }
    let after = stream
        .read_frames(&mut buf)
        .expect("post-seek read succeeds on a clean source");
    match after {
        DecodeOutcome::Frames(n) => {
            line(&format!("POST_SEEK frames={n}"));
            assert!(n > 0);
        }
        other => panic!("post-seek read must produce frames, got {other:?}"),
    }
    drop(stream);
    drop(registration);
    line("S2_VERDICT=PASS");
}
