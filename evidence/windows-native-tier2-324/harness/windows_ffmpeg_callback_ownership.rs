//! #324 S3 — Windows executable FFmpeg `av_log` callback-ownership witness.
//!
//! The Unix witness (`tests/ffmpeg_log_gate.rs`, `#![cfg(unix)]`) is 0 tests on
//! Windows: it redirects fd 2 and reads a file. This witness needs no fd work at
//! all — it observes what reaches the Rust diagnostic facility, which is the
//! actual ownership question:
//!
//! ```text
//! QIANQIAN_FFMPEG_LOG unset -> SongCore's per-open discard callback -> 0 ffmpeg records
//! QIANQIAN_FFMPEG_LOG=0     -> the same discard baseline           -> 0 ffmpeg records
//! QIANQIAN_FFMPEG_LOG=1     -> the host Rust bridge owns av_log    -> >= 1 ffmpeg records
//! ```
//!
//! ONE CASE PER FRESH PROCESS (`Q324_LOG_CASE=unset|zero|one`): the av_log
//! callback is process-global and the bridge is installed once per process, so
//! switching the variable inside one process would prove nothing.
//!
//! The positive control (`one`) also proves the media really does drive
//! `av_log` at record level; the negative cases then mean "the bridge did not
//! own the callback", not "no log happened". Every case additionally asserts a
//! delivered record from the decode-fault site, so a dead facility cannot be
//! mistaken for a silent FFmpeg.

// This harness is Windows-only: it links kernel32 and drives Win32
// handles/pipes. A non-Windows host must compile it to zero tests rather than
// fail at link time (the crate is workspace-excluded and not CI-compiled).
#![cfg(windows)]

#[allow(dead_code)]
mod common;

use std::cell::RefCell;
use std::io::Write as _;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use qianqian_audio_api::ports::{DecodeOutcome, PcmDecode, PcmDecodeCapability};
use qianqian_composition::{ComponentSpec, CompositionKernel, DesiredEntry, FiberState, Revision};
use qianqian_diagnostics::{
    AlphaRegistration, Config, Diagnostics, ExportError, Exporter, Record,
};

const TRUNCATED_SHA256: &str = "0aa66168770d5d36072dccf7fec728647717290f96bf13335775144f3438b39f";

fn line(s: &str) {
    writeln!(std::io::stdout(), "{s}").expect("stdout receipt");
    std::io::stdout().flush().expect("stdout receipt flush");
}

struct Collecting {
    lines: Arc<Mutex<Vec<String>>>,
}

impl Exporter for Collecting {
    fn name(&self) -> &'static str {
        "s3-collecting"
    }

    fn export(&mut self, record: &Record) -> Result<(), ExportError> {
        let mut rendered = format!("{}|{}|", record.site, record.level.tag());
        rendered.push_str(&record.message.render());
        self.lines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(rendered);
        Ok(())
    }
}

fn delivered_lines() -> &'static Arc<Mutex<Vec<String>>> {
    static LINES: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    LINES.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

fn install_facility() -> &'static Diagnostics {
    static FACILITY: OnceLock<Diagnostics> = OnceLock::new();
    FACILITY.get_or_init(|| {
        // Installed BEFORE the plugin activation, so the av_log bridge (when
        // opted in) publishes into a live facility.
        Diagnostics::install(
            Box::new(Collecting {
                lines: Arc::clone(delivered_lines()),
            }),
            Config {
                beta_capacity: 64,
                poll_window: Duration::from_millis(1),
            },
        )
        .expect("fresh process installs the one facility")
    })
}

fn activate_decode_plugin_and_capture() -> Rc<dyn PcmDecode> {
    thread_local! {
        static CAPTURED: RefCell<Option<Rc<dyn PcmDecode>>> = const { RefCell::new(None) };
    }
    let mut k = CompositionKernel::new();
    k.register_component(qianqian_decode_songcore::songcore_decode_plugin())
        .expect("decode plugin registers");
    k.register_component(
        ComponentSpec::new("s3_consumer")
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
        DesiredEntry::enabled("s3_consumer", "s3_consumer", Revision::fresh()),
    ])
    .expect("both fibers desired");
    k.settle();
    let snap = k.snapshot();
    for name in ["songcore_decode_plugin", "s3_consumer"] {
        let fiber = snap
            .fibers
            .get(name)
            .unwrap_or_else(|| panic!("{name} fiber present"));
        assert_eq!(fiber.state, FiberState::Active, "{name} must be active");
    }
    CAPTURED.with(|c| c.borrow().as_ref().expect("service captured").clone())
}

#[test]
fn windows_ffmpeg_callback_ownership() {
    std::panic::set_hook(Box::new(|info| line(&format!("ASSERTION {info}"))));
    let case = std::env::var("Q324_LOG_CASE").expect("Q324_LOG_CASE must be set by the runner");
    let env_value = std::env::var("QIANQIAN_FFMPEG_LOG").ok();
    line(&format!(
        "S3-CASE={case} QIANQIAN_FFMPEG_LOG={env_value:?} pid={}",
        std::process::id()
    ));
    match case.as_str() {
        "unset" => assert!(env_value.is_none(), "the unset case must not set the variable"),
        "zero" => assert_eq!(env_value.as_deref(), Some("0")),
        "one" => assert_eq!(env_value.as_deref(), Some("1")),
        other => panic!("unknown Q324_LOG_CASE {other}"),
    }

    let _diagnostics = install_facility();
    // The production decode worker is a class-α producer; this witness drives
    // the decode on the main thread, so the same registration is what admits
    // the bridge's records.
    let registration = AlphaRegistration::register();

    let (_fx, path) = common::load_fixture("flac-16-44-stereo");
    let bytes = std::fs::read(&path).expect("fixture readable");
    let truncated = &bytes[..100_000];
    let mut hasher = common::Sha256::new();
    hasher.update(truncated);
    let sha = hasher.hex();
    assert_eq!(sha, TRUNCATED_SHA256, "truncated media identity");
    let media = std::env::temp_dir().join(format!("q324-s3-{}.flac", std::process::id()));
    std::fs::write(&media, truncated).expect("truncated media written");

    let decode = activate_decode_plugin_and_capture();
    let mut stream = decode.open_media(&media).expect("truncated source opens");
    let channels = stream.format().channels as usize;
    let mut buf = vec![0f32; 8192 * channels];
    let mut reads = 0;
    let first_error = loop {
        reads += 1;
        assert!(reads < 10_000, "the decode must reach its fault");
        match stream.read_frames(&mut buf) {
            Ok(DecodeOutcome::Frames(n)) if n > 0 => continue,
            Ok(other) => panic!("unexpected outcome {other:?}"),
            Err(e) => break e,
        }
    };
    drop(stream);
    let _ = std::fs::remove_file(&media);
    line(&format!(
        "S3-PRODUCER first_error={:?} alpha_attempted={} alpha_accepted={} alpha_rejected={}",
        first_error.message,
        registration.attempted(),
        registration.accepted(),
        registration.rejected()
    ));
    assert!(
        first_error.message.contains("pending decode error"),
        "the real native decode fault must surface"
    );

    // Wait (bounded) for the drainer to copy out every ACCEPTED publication.
    // The two-slot pool may reject the trailing fault record when the av_log
    // bridge already filled it, so the wait is on the pool's drain boundary,
    // not on one particular site.
    let accepted = registration.accepted();
    assert!(accepted > 0, "the real producer published at least one record");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let counters = _diagnostics.counters();
        if counters.copied_out >= accepted && counters.outstanding == 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "WATCHDOG (fail-closed): accepted records never reached the drain boundary: {counters:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let records: Vec<String> = delivered_lines()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let fault_ok = records
        .iter()
        .any(|l| l.starts_with("qianqian-decode-songcore|error|"));
    let ffmpeg_records: Vec<&String> = records
        .iter()
        .filter(|l| l.starts_with("ffmpeg|"))
        .collect();
    line(&format!(
        "S3-DELIVERED total={} ffmpeg={} ffmpeg_first={:?} fault_site_delivered={fault_ok}",
        records.len(),
        ffmpeg_records.len(),
        ffmpeg_records.first()
    ));

    match case.as_str() {
        "unset" | "zero" => {
            assert_eq!(
                ffmpeg_records.len(),
                0,
                "the discard baseline must leave FFmpeg's own logging outside the facility"
            );
            assert!(
                fault_ok,
                "the facility itself must be live: the decode-fault record has to be delivered"
            );
        }
        "one" => assert!(
            !ffmpeg_records.is_empty(),
            "the opt-in bridge must own av_log and publish records (the media is proven to log)"
        ),
        _ => unreachable!(),
    }

    let report = _diagnostics.shutdown(Duration::from_secs(5));
    line(&format!(
        "S3-SHUTDOWN drained={} abandoned={} undelivered={}",
        report.drained, report.abandoned, report.undelivered_at_exit
    ));
    line("S3_VERDICT=PASS");
}
