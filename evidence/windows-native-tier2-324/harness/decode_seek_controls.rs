//! #324 decode-seek × C1–C4 (WINDOWS x86_64).
//!
//! Linux lineage: `evidence/native-tier2-296/harness/decode_seek_controls.rs`.
//! Producer (identical in every cell): a real SongCore artifact seek whose
//! landing frame BODY is corrupt while the frame HEADER is intact. The generic
//! seek lands on the first corrupted frame, the landing decode fails inside the
//! real FFmpeg decoder, and the reliable results are the `MutatedThenFailed`
//! seek outcome ("could not reach a landing point") plus the post-seek native
//! `DecodeError` ("decode error").
//!
//! Windows-specific premises:
//! * C1's blocked sink is the REAL process-wide `std::io::stderr()` lock held by
//!   a helper (verified process-global on Windows, see ENVIRONMENT.md). The
//!   sink it blocks is healthy, so the released drainer must still settle to a
//!   COMPLETE drain — the pipe-backpressure construction used by the other two
//!   arms can never drain and would make that assertion meaningless.
//! * C4's write failure is a real one: the process stderr is swapped with
//!   `SetStdHandle` onto an anonymous pipe whose READ end is closed, and the
//!   real `StderrExporter` performs the failing write (no injected
//!   `ExportError`, no `/dev/full`).
//!
//! ONE CELL PER PROCESS (`Diagnostics::install` is process-global).

// This harness is Windows-only: it links kernel32 and drives Win32
// handles/pipes. A non-Windows host must compile it to zero tests rather than
// fail at link time (the crate is workspace-excluded and not CI-compiled).
#![cfg(windows)]

#[allow(dead_code)]
mod common;

use std::cell::RefCell;
use std::ffi::c_void;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use qianqian_audio_api::ports::{
    DecodeOutcome, DecodedPcmStream, PcmDecode, PcmDecodeCapability, ProviderSeekOutcome,
};
use qianqian_composition::{ComponentSpec, CompositionKernel, DesiredEntry, FiberState, Revision};
use qianqian_diagnostics::{
    AlphaRegistration, Config, Counters, Diagnostics, Disposition, ExportError, Exporter, Level,
    Record, StderrExporter, Text, emit,
};

const SEEK_FAULT_FLIPS: [(usize, u8); 3] = [(73_502, 0x01), (77_042, 0x01), (80_375, 0x01)];
const SEEK_TARGET_US: u64 = 2_000_000;

const NATIVE_INVALIDDATA: i32 = -1_094_995_529; // AVERROR_INVALIDDATA
const SONG_ERR_SEEK_ERROR: i32 = 110;
const DIAG_SITE: &str = "qianqian-decode-songcore";

/// The verified `windows-x86_64` artifact (ARTIFACT.md).
const EXPECTED_ARTIFACT_SHA256: &str = "90bde1ca3f9924842ebb06005219f746d5053499a03fc3afafebccd1d57ba6d2";

#[allow(clippy::explicit_write)]
fn line(s: &str) {
    writeln!(std::io::stdout(), "{s}").expect("stdout receipt");
    std::io::stdout().flush().expect("stdout receipt flush");
}

fn stdout_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "ASSERTION {info}");
        let _ = out.flush();
    }));
}

fn require_fresh_process() {
    assert!(
        !Diagnostics::is_installed(),
        "this cell must run in a fresh process (one facility per process)"
    );
}

fn assert_artifact_identity() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/build/artifacts/libsongcore.a");
    let bytes = std::fs::read(&path).expect("native artifact readable");
    let mut hasher = common::Sha256::new();
    hasher.update(&bytes);
    let actual = hasher.hex();
    line(&format!("ARTIFACT_PATH={}", path.display()));
    line(&format!("ARTIFACT_SHA256={actual}"));
    let embedded = qianqian_songcore_sys::QN_SONGCORE_ARTIFACT_SHA256
        .trim_start_matches('\\')
        .trim();
    line(&format!(
        "ARTIFACT_BUILD_TIME_SHA256_RAW={}",
        qianqian_songcore_sys::QN_SONGCORE_ARTIFACT_SHA256
    ));
    assert_eq!(actual, EXPECTED_ARTIFACT_SHA256, "PREMISE: verified artifact");
    assert_eq!(
        actual, embedded,
        "PREMISE: the binary must be linked against the artifact on disk"
    );
}

// --- Windows sink primitives ---------------------------------------------

mod win {
    use std::ffi::c_void;

    pub type Handle = *mut c_void;
    pub const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4;
    pub const FILE_TYPE_PIPE: u32 = 3;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(id: u32) -> Handle;
        fn GetFileType(h: Handle) -> u32;
        fn CreatePipe(r: *mut Handle, w: *mut Handle, sa: *mut c_void, size: u32) -> i32;
        fn CloseHandle(h: Handle) -> i32;
        fn SetStdHandle(id: u32, h: Handle) -> i32;
        fn GetLastError() -> u32;
    }

    pub fn stderr_handle() -> Handle {
        unsafe { GetStdHandle(STD_ERROR_HANDLE) }
    }

    pub fn stderr_file_type() -> u32 {
        unsafe { GetFileType(stderr_handle()) }
    }

    pub struct BrokenSink {
        pub original: Handle,
        pub write: Handle,
    }

    /// Replace the process stderr with an anonymous pipe whose READ end is
    /// already closed: the real writer fails with a real OS error, and Rust's
    /// std resolves the new handle on every write (`GetStdHandle`, never a
    /// cached handle — std's `sys/stdio/windows.rs`).
    pub fn replace_stderr_broken() -> BrokenSink {
        let original = stderr_handle();
        let mut r: Handle = std::ptr::null_mut();
        let mut w: Handle = std::ptr::null_mut();
        let ok = unsafe { CreatePipe(&mut r, &mut w, std::ptr::null_mut(), 0) };
        assert_eq!(
            ok,
            1,
            "PREMISE: CreatePipe failed (err={})",
            unsafe { GetLastError() }
        );
        unsafe { CloseHandle(r) };
        let ok = unsafe { SetStdHandle(STD_ERROR_HANDLE, w) };
        assert_eq!(ok, 1, "PREMISE: SetStdHandle(STD_ERROR_HANDLE) failed");
        assert_eq!(
            unsafe { GetFileType(stderr_handle()) },
            FILE_TYPE_PIPE,
            "PREMISE: std stderr must resolve to the broken pipe"
        );
        BrokenSink { original, write: w }
    }

    pub fn restore(sink: &BrokenSink) {
        let ok = unsafe { SetStdHandle(STD_ERROR_HANDLE, sink.original) };
        assert_eq!(ok, 1, "TEARDOWN: restore the original stderr handle");
    }

    pub const ERROR_BROKEN_PIPE: i32 = 109;
    pub const ERROR_NO_DATA: i32 = 232;
}

// --- real production path: kernel + plugin + capability consumer -----------

fn activate_decode_plugin_and_capture() -> Rc<dyn PcmDecode> {
    thread_local! {
        static CAPTURED: RefCell<Option<Rc<dyn PcmDecode>>> = const { RefCell::new(None) };
    }
    let mut k = CompositionKernel::new();
    k.register_component(qianqian_decode_songcore::songcore_decode_plugin())
        .expect("decode plugin registers");
    k.register_component(
        ComponentSpec::new("control_consumer")
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
    .expect("control consumer registers");
    k.set_desired(vec![
        DesiredEntry::enabled(
            "songcore_decode_plugin",
            "songcore_decode_plugin",
            Revision::fresh(),
        ),
        DesiredEntry::enabled("control_consumer", "control_consumer", Revision::fresh()),
    ])
    .expect("both fibers desired");
    k.settle();
    let snap = k.snapshot();
    for name in ["songcore_decode_plugin", "control_consumer"] {
        let fiber = snap
            .fibers
            .get(name)
            .unwrap_or_else(|| panic!("{name} fiber present"));
        assert_eq!(
            fiber.state,
            FiberState::Active,
            "{name} must be active for the producer to be the real one"
        );
    }
    CAPTURED
        .with(|c| c.borrow().as_ref().expect("service captured").clone())
}

fn corrupted_copy(name: &str, flips: &[(usize, u8)]) -> PathBuf {
    let (_fx, path) = common::load_fixture("flac-16-44-stereo");
    let mut bytes = std::fs::read(&path).expect("fixture bytes");
    for &(off, xor) in flips {
        bytes[off] ^= xor;
    }
    let out = std::env::temp_dir().join(format!("q324-decode-seek-{}-{name}", std::process::id()));
    std::fs::write(&out, &bytes).expect("corrupted copy written");
    out
}

fn open_and_seek_to_fault(
    decode: &dyn PcmDecode,
    path: &Path,
) -> (Box<dyn DecodedPcmStream>, ProviderSeekOutcome) {
    let mut stream = decode
        .open_media(path)
        .expect("the seek-corrupted file still opens and probes");
    let channels = stream.format().channels as usize;
    let mut buf = vec![0f32; 8192 * channels];
    let mut decoded = 0usize;
    for _ in 0..8 {
        if let Ok(DecodeOutcome::Frames(n)) = stream.read_frames(&mut buf) {
            decoded += n;
        }
        if decoded >= 44_100 {
            break;
        }
    }
    assert!(
        decoded >= 44_100,
        "the pre-seek prefix must decode clean before the corrupted region"
    );
    let outcome = stream.seek(Duration::from_micros(SEEK_TARGET_US));
    (stream, outcome)
}

fn seek_diagnostic(outcome: ProviderSeekOutcome) -> String {
    match outcome {
        ProviderSeekOutcome::MutatedThenFailed { diagnostic } => diagnostic,
        other => panic!("seek must fail mutated-then-failed on the corrupt landing, got {other:?}"),
    }
}

fn post_seek_decode_error(stream: &mut dyn DecodedPcmStream) -> String {
    stream
        .read_frames(&mut vec![0f32; 8192 * 2])
        .expect_err("the stream is dead after the failed seek (real native decode failure)")
        .message
}

fn producer_discipline(diag: &str, post: &str, cell: &str) {
    assert_eq!(
        diag, "could not reach a landing point",
        "{cell}: the reliable seek diagnostic is the native landing failure"
    );
    assert_eq!(
        post, "decode error",
        "{cell}: the post-seek reliable DecodeError is the real landing-frame decode failure"
    );
}

fn counters_text(c: &Counters) -> String {
    format!(
        "beta_queued={} beta_rejected={} beta_consumed={} alpha_rejected={} copied_out={} \
         exported={} export_failed={} outstanding={} self_diagnostics={}",
        c.beta_queued,
        c.beta_rejected,
        c.beta_consumed,
        c.alpha_rejected,
        c.copied_out,
        c.exported,
        c.export_failed,
        c.outstanding,
        c.self_diagnostics
    )
}

fn identity_at_quiescence(c: &Counters, cell: &str) {
    assert_eq!(
        c.copied_out + c.beta_consumed,
        c.exported + c.export_failed,
        "{cell}: ACCOUNTING-ORACLE at quiescence: copied_out + beta_consumed = exported + export_failed ({})",
        counters_text(c)
    );
}

// --- exporters used by the controls ---------------------------------------

/// A real stderr sink behind an observation seam: the seam announces entry to
/// the drainer's export call and records that the call is still in progress;
/// the write itself is `StderrExporter`, i.e. a real `stderr` lock/write.
struct RealStderrSink {
    entered: mpsc::Sender<()>,
    in_export: Arc<AtomicBool>,
    exports: Arc<Mutex<Vec<String>>>,
}

impl Exporter for RealStderrSink {
    fn name(&self) -> &'static str {
        "control-real-stderr"
    }

    fn export(&mut self, record: &Record) -> Result<(), ExportError> {
        let mut rendered = format!("{}|{}|", record.site, record.level.tag());
        rendered.push_str(&record.message.render());
        self.exports
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(rendered);
        self.in_export.store(true, Ordering::SeqCst);
        let _ = self.entered.send(());
        let result = StderrExporter::default().export(record);
        self.in_export.store(false, Ordering::SeqCst);
        result
    }
}

/// The stopped-consumer seam.
struct HeldExporter {
    entered: mpsc::Sender<()>,
    gate: Arc<(Mutex<bool>, Condvar)>,
}

impl Exporter for HeldExporter {
    fn name(&self) -> &'static str {
        "control-held"
    }

    fn export(&mut self, _record: &Record) -> Result<(), ExportError> {
        let _ = self.entered.send(());
        let (lock, cv) = &*self.gate;
        let guard = lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        drop(
            cv.wait_while(guard, |released| !*released)
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
        Ok(())
    }
}

/// C4's recording exporter: it records the delivered line and typed fields and
/// then performs the REAL write with `StderrExporter`. The failure phase is a
/// real OS failure of that write (broken stderr pipe), not an injected error.
struct RecordingStderrExporter {
    lines: Arc<Mutex<Vec<String>>>,
    typed: Arc<Mutex<Vec<(Option<i32>, Option<i32>)>>>,
    inner: StderrExporter,
}

impl Exporter for RecordingStderrExporter {
    fn name(&self) -> &'static str {
        "control-recording-stderr"
    }

    fn export(&mut self, record: &Record) -> Result<(), ExportError> {
        let mut rendered = format!("{}|{}|", record.site, record.level.tag());
        rendered.push_str(&record.message.render());
        self.inner.export(record)?;
        self.lines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(rendered);
        self.typed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((record.status, record.native_code));
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ExportError> {
        self.inner.flush()
    }
}

fn wait_for_counters(
    diagnostics: &Diagnostics,
    predicate: impl Fn(&Counters) -> bool,
    what: &str,
) -> Counters {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let c = diagnostics.counters();
        if predicate(&c) {
            return c;
        }
        assert!(
            Instant::now() < deadline,
            "the counters never reached {what}: {}",
            counters_text(&c)
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

// =========================================================================
// C1 BLOCKED_SINK (real process-wide std stderr lock)
// =========================================================================

#[test]
fn c1_blocked_sink() {
    stdout_panic_hook();
    require_fresh_process();
    unsafe { std::env::set_var("QIANQIAN_FFMPEG_LOG", "1") };

    line("CELL=C1 BLOCKED_SINK");
    assert_artifact_identity();
    line(&format!(
        "C1-ENV=windows-x86_64 ffmpeg=n9.0.1 QIANQIAN_FFMPEG_LOG=1 pid={} stderr_file_type={}",
        std::process::id(),
        win::stderr_file_type()
    ));

    let (entered_tx, entered_rx) = mpsc::channel();
    let in_export = Arc::new(AtomicBool::new(false));
    let exports = Arc::new(Mutex::new(Vec::new()));
    let diagnostics = Diagnostics::install(
        Box::new(RealStderrSink {
            entered: entered_tx,
            in_export: Arc::clone(&in_export),
            exports: Arc::clone(&exports),
        }),
        Config {
            beta_capacity: 4,
            poll_window: Duration::from_millis(1),
        },
    )
    .expect("fresh process installs the one facility");

    // Real process-wide stderr lock held by a helper (verified cross-thread on
    // Windows). It never releases until the producer has finished.
    let released = Arc::new(AtomicBool::new(false));
    let producer_done = Arc::new((Mutex::new(false), Condvar::new()));
    let lock_acquired = Arc::new((Mutex::new(false), Condvar::new()));
    let helper = {
        let released = Arc::clone(&released);
        let producer_done = Arc::clone(&producer_done);
        let lock_acquired = Arc::clone(&lock_acquired);
        std::thread::spawn(move || {
            let guard = std::io::stderr().lock();
            {
                let (lock, cv) = &*lock_acquired;
                *lock.lock().unwrap_or_else(|p| p.into_inner()) = true;
                cv.notify_all();
            }
            let (lock, cv) = &*producer_done;
            let guard_state = lock.lock().unwrap_or_else(|p| p.into_inner());
            let _ = cv
                .wait_timeout_while(guard_state, Duration::from_secs(10), |done| !*done)
                .unwrap_or_else(|p| p.into_inner());
            released.store(true, Ordering::SeqCst);
            drop(guard);
        })
    };
    {
        let (lock, cv) = &*lock_acquired;
        let guard = lock.lock().unwrap_or_else(|p| p.into_inner());
        let (guard, timeout) = cv
            .wait_timeout_while(guard, Duration::from_secs(5), |acquired| !*acquired)
            .unwrap_or_else(|p| p.into_inner());
        assert!(
            !timeout.timed_out() && *guard,
            "PREMISE: the real process-wide stderr lock was acquired"
        );
        line("C1-STDERR-LOCK-HELD");
    }

    // Seed the drainer into the real exporter write.
    assert_eq!(
        emit(
            Record::new("decode-seek-control", Level::Info).with_message(Text::from_static("seed"))
        ),
        Disposition::BetaQueued,
        "PREMISE: the seed rides the β channel into the sole drainer"
    );
    entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("PREMISE: the drainer entered the real export call");
    wait_for_counters(
        &diagnostics,
        |c| c.beta_consumed >= 1,
        "the drainer consumed the seed and is inside the blocked export",
    );
    assert!(
        in_export.load(Ordering::SeqCst),
        "PREMISE: the drainer is still inside the real stderr export"
    );
    line("C1-CONSUMER-BLOCKED");

    let registration = AlphaRegistration::register();
    let decode = activate_decode_plugin_and_capture();
    let file = corrupted_copy("c1.flac", &SEEK_FAULT_FLIPS);
    let seek_started = Instant::now();
    let (mut stream, outcome) = open_and_seek_to_fault(decode.as_ref(), &file);
    let diag = seek_diagnostic(outcome);
    let post = post_seek_decode_error(stream.as_mut());
    let finished_while_held = !released.load(Ordering::SeqCst);
    let sink_still_blocked = in_export.load(Ordering::SeqCst);
    let producer_elapsed = seek_started.elapsed();
    drop(stream);

    {
        let (lock, cv) = &*producer_done;
        *lock.lock().unwrap_or_else(|p| p.into_inner()) = true;
        cv.notify_all();
    }
    helper.join().expect("stderr-lock helper joins");

    let attempted = registration.attempted();
    let accepted = registration.accepted();
    let rejected = registration.rejected();
    let outstanding = registration.outstanding();
    line(&format!(
        "C1-PRODUCER diag={diag:?} post={post:?} finished_while_held={finished_while_held} \
         sink_still_blocked={sink_still_blocked} elapsed_ms={}",
        producer_elapsed.as_millis()
    ));
    line(&format!(
        "C1-ALPHA attempted={attempted} accepted={accepted} rejected={rejected} outstanding={outstanding}"
    ));
    line(&format!(
        "C1-BLOCKED-COUNTERS {}",
        counters_text(&diagnostics.counters())
    ));
    let exported_at_sink = exports
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    line(&format!(
        "C1-EXPORT-ENTRIES={} first={:?}",
        exported_at_sink.len(),
        exported_at_sink.first()
    ));

    producer_discipline(&diag, &post, "C1");
    assert!(
        finished_while_held,
        "C1 ISOLATION: the producer returned only after the blocked sink was released"
    );
    assert!(
        sink_still_blocked,
        "C1 ISOLATION: the real exporter was no longer blocked when the producer returned"
    );
    assert_eq!(
        attempted,
        accepted + rejected,
        "C1 ADMISSION-ORACLE: attempted = accepted + rejected"
    );
    assert!(attempted >= 1, "C1: the real producer emitted at least one record");
    assert!(
        rejected >= 1,
        "C1: the blocked drainer forces bounded α rejection (loss counted)"
    );
    assert!(
        outstanding <= 2,
        "C1: the α pool bound is respected while the drainer is blocked"
    );

    let report = diagnostics.shutdown(Duration::from_secs(5));
    line(&format!(
        "C1-SHUTDOWN drained={} abandoned={} undelivered={} {}",
        report.drained,
        report.abandoned,
        report.undelivered_at_exit,
        counters_text(&report.counters)
    ));
    assert!(
        report.drained && !report.abandoned,
        "C1 TEARDOWN: the released drainer settles to a complete drain: {report:?}"
    );
    assert_eq!(
        report.undelivered_at_exit, 0,
        "C1: a complete drain leaves nothing admitted and undelivered"
    );
    assert!(
        report.counters.exported >= 1,
        "C1: the sink received records after release"
    );
    assert_eq!(
        report.counters.export_failed, 0,
        "C1: the real stderr sink did not fail"
    );
    identity_at_quiescence(&report.counters, "C1");
    line("C1-ACCOUNTING-OK");
    line("C1-PASS");
    let _ = std::fs::remove_file(file);
}

// =========================================================================
// C2 STOPPED_CONSUMER
// =========================================================================

#[test]
fn c2_stopped_consumer() {
    stdout_panic_hook();
    require_fresh_process();
    unsafe { std::env::set_var("QIANQIAN_FFMPEG_LOG", "1") };

    line("CELL=C2 STOPPED_CONSUMER");
    assert_artifact_identity();
    line(&format!(
        "C2-ENV=windows-x86_64 ffmpeg=n9.0.1 QIANQIAN_FFMPEG_LOG=1 pid={}",
        std::process::id()
    ));

    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (entered_tx, entered_rx) = mpsc::channel();
    let diagnostics = Diagnostics::install(
        Box::new(HeldExporter {
            entered: entered_tx,
            gate: Arc::clone(&gate),
        }),
        Config {
            beta_capacity: 4,
            poll_window: Duration::from_millis(1),
        },
    )
    .expect("fresh process installs the one facility");

    assert_eq!(
        emit(
            Record::new("decode-seek-control", Level::Info).with_message(Text::from_static("seed"))
        ),
        Disposition::BetaQueued,
        "PREMISE: the seed rides the β channel into the sole drainer"
    );
    entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("PREMISE: the drainer entered the stopped export call");
    line("C2-CONSUMER-STOPPED");

    let registration = AlphaRegistration::register();
    let decode = activate_decode_plugin_and_capture();
    let file = corrupted_copy("c2.flac", &SEEK_FAULT_FLIPS);
    let before = diagnostics.counters();
    let started = Instant::now();
    let (mut stream, outcome) = open_and_seek_to_fault(decode.as_ref(), &file);
    let diag = seek_diagnostic(outcome);
    let post = post_seek_decode_error(stream.as_mut());
    let elapsed = started.elapsed();
    drop(stream);
    let after_producer = diagnostics.counters();

    let attempted = registration.attempted();
    let accepted = registration.accepted();
    let rejected = registration.rejected();
    let outstanding = registration.outstanding();
    line(&format!(
        "C2-PRODUCER diag={diag:?} post={post:?} elapsed_ms={}",
        elapsed.as_millis()
    ));
    line(&format!(
        "C2-ALPHA attempted={attempted} accepted={accepted} rejected={rejected} outstanding={outstanding}"
    ));
    line(&format!(
        "C2-PRODUCER-COUNTERS {}",
        counters_text(&after_producer)
    ));

    producer_discipline(&diag, &post, "C2");
    assert_eq!(
        after_producer.exported, before.exported,
        "C2 ISOLATION: nothing was delivered while the drainer was stopped"
    );
    assert_eq!(
        after_producer.export_failed, before.export_failed,
        "C2: a stopped consumer is not a failed export"
    );
    assert_eq!(
        attempted,
        accepted + rejected,
        "C2 ADMISSION-ORACLE: attempted = accepted + rejected"
    );
    assert!(rejected >= 1, "C2: the stopped drainer forces bounded α rejection");
    assert!(outstanding <= 2, "C2: the α pool bound holds with no consumer");

    let shutdown_started = Instant::now();
    let report = diagnostics.shutdown(Duration::from_millis(50));
    let shutdown_elapsed = shutdown_started.elapsed();
    line(&format!(
        "C2-SHUTDOWN elapsed_ms={} drained={} abandoned={} undelivered={} {}",
        shutdown_elapsed.as_millis(),
        report.drained,
        report.abandoned,
        report.undelivered_at_exit,
        counters_text(&report.counters)
    ));
    assert!(
        shutdown_elapsed < Duration::from_secs(2),
        "C2 TEARDOWN: shutdown returned inside its bounded window, took {shutdown_elapsed:?}"
    );
    assert!(
        !report.drained,
        "C2 HONEST-ACCOUNTING: a stopped drainer must NEVER be reported drained: {report:?}"
    );
    assert!(
        report.abandoned,
        "C2 HONEST-ACCOUNTING: bounded abandonment must be reported as abandoned: {report:?}"
    );
    assert!(
        report.undelivered_at_exit >= 1,
        "C2 LOSS-ACCOUNTING: records left at the stopped consumer are counted at exit: {report:?}"
    );
    assert_eq!(
        report.counters.exported, before.exported,
        "C2 LOSS-ACCOUNTING: abandoned records are not written as delivered"
    );
    assert_eq!(
        registration.attempted(),
        registration.accepted() + registration.rejected(),
        "C2 ADMISSION-ORACLE after shutdown: attempted = accepted + rejected"
    );
    line("C2-ACCOUNTING-OK");
    line("C2-PASS");
    let _ = std::fs::remove_file(file);
}

// =========================================================================
// C3 CAPACITY_EXHAUSTION
// =========================================================================

#[test]
fn c3_capacity_exhaustion() {
    stdout_panic_hook();
    require_fresh_process();
    unsafe { std::env::set_var("QIANQIAN_FFMPEG_LOG", "1") };

    line("CELL=C3 CAPACITY_EXHAUSTION");
    assert_artifact_identity();
    line(&format!(
        "C3-ENV=windows-x86_64 ffmpeg=n9.0.1 QIANQIAN_FFMPEG_LOG=1 pid={}",
        std::process::id()
    ));

    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (entered_tx, entered_rx) = mpsc::channel();
    let diagnostics = Diagnostics::install(
        Box::new(HeldExporter {
            entered: entered_tx,
            gate: Arc::clone(&gate),
        }),
        Config {
            beta_capacity: 4,
            poll_window: Duration::from_millis(1),
        },
    )
    .expect("fresh process installs the one facility");
    assert_eq!(
        emit(
            Record::new("decode-seek-control", Level::Info).with_message(Text::from_static("seed"))
        ),
        Disposition::BetaQueued,
        "PREMISE: the seed rides the β channel into the sole drainer"
    );
    entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("PREMISE: the drainer entered the stopped export call");
    line("C3-CONSUMER-STOPPED");

    let registration = AlphaRegistration::register();
    let mut pool_full_seen = false;
    let mut fill_accepted = 0u64;
    for _ in 0..3 {
        match emit(
            Record::new("decode-seek-fill", Level::Info).with_message(Text::from_static("capacity")),
        ) {
            Disposition::AlphaRecorded => fill_accepted += 1,
            Disposition::AlphaRejected => {
                pool_full_seen = true;
                break;
            }
            other => panic!("C3 PREMISE: wrong admission path {other:?}"),
        }
    }
    line(&format!(
        "C3-ALPHA-FILL accepted={fill_accepted} pool_full_seen={pool_full_seen}"
    ));

    let rejected_before = registration.rejected();
    let accepted_before = registration.accepted();
    let decode = activate_decode_plugin_and_capture();
    let file = corrupted_copy("c3.flac", &SEEK_FAULT_FLIPS);
    let started = Instant::now();
    let (mut stream, outcome) = open_and_seek_to_fault(decode.as_ref(), &file);
    let diag = seek_diagnostic(outcome);
    let post = post_seek_decode_error(stream.as_mut());
    let elapsed = started.elapsed();
    drop(stream);
    let rejected_after = registration.rejected();
    let accepted_after = registration.accepted();
    let attempted = registration.attempted();
    let outstanding = registration.outstanding();
    line(&format!(
        "C3-PRODUCER diag={diag:?} post={post:?} elapsed_ms={}",
        elapsed.as_millis()
    ));
    line(&format!(
        "C3-ALPHA attempted={attempted} accepted={accepted_after} rejected={rejected_after} \
         outstanding={outstanding} pool_full_seen={pool_full_seen}"
    ));
    line(&format!(
        "C3-COUNTERS {}",
        counters_text(&diagnostics.counters())
    ));

    producer_discipline(&diag, &post, "C3");
    assert!(
        rejected_after > rejected_before,
        "C3 ISOLATION: the real producer's records were rejected at the full α pool \
         (rejected_before={rejected_before} rejected_after={rejected_after})"
    );
    assert_eq!(
        accepted_after, accepted_before,
        "C3: no record was admitted into a full pool (accepted_before={accepted_before} \
         accepted_after={accepted_after})"
    );
    assert!(
        pool_full_seen,
        "C3 PREMISE: the pre-fill observed the two-slot pool's first rejection"
    );
    assert_eq!(
        attempted,
        accepted_after + rejected_after,
        "C3 ADMISSION-ORACLE: attempted = accepted + rejected"
    );
    assert!(
        outstanding <= 2,
        "C3 BOUNDEDNESS: the pool never grew past its two-slot bound (outstanding={outstanding})"
    );

    let shutdown_started = Instant::now();
    let report = diagnostics.shutdown(Duration::from_millis(50));
    line(&format!(
        "C3-SHUTDOWN elapsed_ms={} drained={} abandoned={} undelivered={} {}",
        shutdown_started.elapsed().as_millis(),
        report.drained,
        report.abandoned,
        report.undelivered_at_exit,
        counters_text(&report.counters)
    ));
    assert!(
        report.abandoned && !report.drained,
        "C3 TEARDOWN: the stopped consumer still yields bounded honest abandonment: {report:?}"
    );
    assert!(
        report.undelivered_at_exit >= 1,
        "C3 LOSS-ACCOUNTING: the two admitted records remain outstanding at exit"
    );
    line("C3-ACCOUNTING-OK");
    line("C3-PASS");
    let _ = std::fs::remove_file(file);
}

// =========================================================================
// C4 WRITE_FAILURE (real OS write failure at the real exporter)
// =========================================================================

#[test]
fn c4_write_failure() {
    stdout_panic_hook();
    require_fresh_process();
    unsafe { std::env::set_var("QIANQIAN_FFMPEG_LOG", "1") };

    line("CELL=C4 WRITE_FAILURE");
    assert_artifact_identity();
    line(&format!(
        "C4-ENV=windows-x86_64 ffmpeg=n9.0.1 QIANQIAN_FFMPEG_LOG=1 pid={} stderr_file_type={}",
        std::process::id(),
        win::stderr_file_type()
    ));

    let lines = Arc::new(Mutex::new(Vec::new()));
    let typed = Arc::new(Mutex::new(Vec::new()));
    let diagnostics = Diagnostics::install(
        Box::new(RecordingStderrExporter {
            lines: Arc::clone(&lines),
            typed: Arc::clone(&typed),
            inner: StderrExporter::default(),
        }),
        Config {
            beta_capacity: 64,
            poll_window: Duration::from_millis(1),
        },
    )
    .expect("fresh process installs the one facility");

    let registration = AlphaRegistration::register();
    let decode = activate_decode_plugin_and_capture();

    // Healthy phase: the real write reaches the runner-drained stderr.
    let healthy_file = corrupted_copy("c4-healthy.flac", &SEEK_FAULT_FLIPS);
    let (mut stream, outcome) = open_and_seek_to_fault(decode.as_ref(), &healthy_file);
    let diag_healthy = seek_diagnostic(outcome);
    let post_healthy = post_seek_decode_error(stream.as_mut());
    drop(stream);
    producer_discipline(&diag_healthy, &post_healthy, "C4 healthy");

    // The α fault record is best-effort: a second REAL producer run on an
    // UNREGISTERED thread rides the β channel and therefore gives the
    // delivered record/premise: same diagnostic and the typed
    // (status 110, native AVERROR_INVALIDDATA) receipt.
    let beta_file = corrupted_copy("c4-beta.flac", &SEEK_FAULT_FLIPS);
    let beta_file_for_thread = beta_file.clone();
    let (diag_beta, post_beta) = std::thread::spawn(move || {
        let decode = activate_decode_plugin_and_capture();
        let (mut stream, outcome) = open_and_seek_to_fault(decode.as_ref(), &beta_file_for_thread);
        let diagnostic = seek_diagnostic(outcome);
        let post = post_seek_decode_error(stream.as_mut());
        drop(stream);
        (diagnostic, post)
    })
    .join()
    .expect("the unregistered β producer thread completes");
    let _ = std::fs::remove_file(beta_file);
    assert_eq!(
        diag_beta, diag_healthy,
        "C4 PREMISE: the unregistered-thread producer's reliable seek result is identical"
    );
    assert_eq!(
        post_beta, post_healthy,
        "C4 PREMISE: the unregistered-thread producer's post-seek DecodeError is identical"
    );

    let premise_deadline = Instant::now() + Duration::from_secs(3);
    let mut healthy_typed_seen = false;
    let mut delivered: Vec<String> = Vec::new();
    while Instant::now() < premise_deadline {
        healthy_typed_seen = typed
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .any(|&(s, n)| s == Some(SONG_ERR_SEEK_ERROR) && n == Some(NATIVE_INVALIDDATA));
        delivered = lines.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let site_seen = delivered
            .iter()
            .any(|l| l.starts_with(&format!("{DIAG_SITE}|error|")));
        if healthy_typed_seen && site_seen {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    line(&format!(
        "C4-HEALTHY diag={diag_healthy:?} post={post_healthy:?} beta_diag={diag_beta:?} \
         beta_post={post_beta:?} typed_110={healthy_typed_seen} delivered={}",
        delivered.len()
    ));
    assert!(
        healthy_typed_seen,
        "C4 PREMISE: the healthy sink delivered the real seek-fault record with \
         (status 110, native AVERROR_INVALIDDATA); delivered={:?}",
        delivered
    );
    assert!(
        delivered
            .iter()
            .any(|l| l.starts_with(&format!("{DIAG_SITE}|error|"))),
        "C4 PREMISE: the delivered record is the producer's own decode-fault site; \
         delivered={:?}",
        delivered
    );
    let before = wait_for_counters(
        &diagnostics,
        |c| c.outstanding == 0 && c.copied_out + c.beta_consumed == c.exported + c.export_failed,
        "healthy-phase quiescence before the injected export failure",
    );
    line(&format!("C4-HEALTHY-COUNTERS {}", counters_text(&before)));

    // REAL write failure: the process stderr becomes an anonymous pipe whose
    // read end is closed. The canary proves the write fails on the exact
    // handle the real exporter uses, before the failing phase runs.
    let sink = win::replace_stderr_broken();
    let error = qianqian_diagnostics::write_stderr_line("Q324-C4-WRITE-CANARY")
        .expect_err("C4 PREMISE: the real stderr writer must fail inside this child");
    let code = error.raw_os_error().unwrap_or(-1);
    assert!(
        code == win::ERROR_NO_DATA || code == win::ERROR_BROKEN_PIPE,
        "C4 PREMISE: expected ERROR_NO_DATA (232) or ERROR_BROKEN_PIPE (109), got {code}"
    );
    line(&format!("C4-BROKEN-STDERR-CONFIRMED raw_os_error={code}"));

    let failing_file = corrupted_copy("c4-failing.flac", &SEEK_FAULT_FLIPS);
    let (mut stream, outcome) = open_and_seek_to_fault(decode.as_ref(), &failing_file);
    let diag_failing = seek_diagnostic(outcome);
    let post_failing = post_seek_decode_error(stream.as_mut());
    drop(stream);
    let after_producer = diagnostics.counters();
    let attempted = registration.attempted();
    let accepted = registration.accepted();
    let rejected = registration.rejected();
    line(&format!(
        "C4-FAILING diag={diag_failing:?} post={post_failing:?} \
         alpha_attempted={attempted} alpha_accepted={accepted} alpha_rejected={rejected} counters={}",
        counters_text(&after_producer)
    ));

    assert_eq!(
        attempted,
        accepted + rejected,
        "C4 ADMISSION-ORACLE: attempted = accepted + rejected"
    );
    assert_eq!(
        diag_failing, diag_healthy,
        "C4 ISOLATION: export failure must not change the reliable seek outcome"
    );
    assert_eq!(
        post_failing, post_healthy,
        "C4 ISOLATION: export failure must not change the post-seek DecodeError"
    );
    assert_eq!(
        after_producer.exported, before.exported,
        "C4: nothing was delivered while the exporter was failing"
    );

    wait_for_counters(
        &diagnostics,
        |c| c.self_diagnostics >= 1,
        "the facility's one exporter-disable self-diagnostic",
    );
    win::restore(&sink);
    let report = diagnostics.shutdown(Duration::from_secs(5));
    line(&format!(
        "C4-SHUTDOWN drained={} abandoned={} undelivered={} {}",
        report.drained,
        report.abandoned,
        report.undelivered_at_exit,
        counters_text(&report.counters)
    ));
    assert!(
        report.drained && !report.abandoned,
        "C4 TEARDOWN: a failing exporter is not a stopped consumer: {report:?}"
    );
    assert!(
        report.counters.export_failed > before.export_failed,
        "C4 LOSS-ACCOUNTING: the failed export attempts are counted as export_failed \
         (before={} after={})",
        before.export_failed,
        report.counters.export_failed
    );
    assert_eq!(
        report.counters.exported, before.exported,
        "C4 LOSS-ACCOUNTING: a failed record is never counted as delivered"
    );
    assert_eq!(
        report.counters.self_diagnostics, 1,
        "C4: exactly one exporter-disable self-diagnostic"
    );
    identity_at_quiescence(&report.counters, "C4");
    line("C4-ACCOUNTING-OK");
    line("C4-PASS");
    let _ = std::fs::remove_file(healthy_file);
    let _ = std::fs::remove_file(failing_file);
}
