//! #324 ffmpeg-bridge × C1–C4 (WINDOWS x86_64).
//!
//! Linux lineage: `evidence/native-tier2-296/harness/native_ffmpeg_bridge_controls.rs`.
//! Real producer: real FFmpeg `av_log` from a real SongCore decode (pinned
//! n9.0.1, truncated committed FLAC fixture) → the real Rust `av_log` bridge
//! (`QIANQIAN_FFMPEG_LOG=1`, installed by the real decode-plugin activation) →
//! the real `qianqian-diagnostics` admission path.
//!
//! Windows rebuild of the sink premises (no `/dev/full`, no `dup2`/CRT fd):
//! * C1 blocked = anonymous pipe installed with `SetStdHandle`, read end held
//!   and never read, quota exhausted by a parked 64 KiB raw write and
//!   corroborated by an independent 1-byte write that also cannot complete;
//! * C4 failed = anonymous pipe with the read end closed, canary-proven with
//!   the real writer before any producer work.
//!
//! One cell per fresh process (`Q324_CELL=C1|C2|C3|C4`).

// This harness is Windows-only: it links kernel32 and drives Win32
// handles/pipes. A non-Windows host must compile it to zero tests rather than
// fail at link time (the crate is workspace-excluded and not CI-compiled).
#![cfg(windows)]

#[allow(dead_code)]
mod common;

use std::ffi::c_void;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use qianqian_audio_api::ports::{DecodeOutcome, PcmDecode, PcmDecodeCapability};
use qianqian_composition::{ComponentSpec, CompositionKernel, DesiredEntry, FiberState, Revision};
use qianqian_diagnostics::{
    AlphaRegistration, Config, Counters, Diagnostics, ExportError, Exporter, Level, Record,
    StderrExporter, Text, emit, write_stderr_line,
};

/// The native read-fault media identity pinned by closeout2 (first 100,000
/// bytes of the committed fixture).
const TRUNCATED_SHA256: &str = "0aa66168770d5d36072dccf7fec728647717290f96bf13335775144f3438b39f";

/// The verified `windows-x86_64` artifact (ARTIFACT.md).
const EXPECTED_ARTIFACT_SHA256: &str = "90bde1ca3f9924842ebb06005219f746d5053499a03fc3afafebccd1d57ba6d2";

// --- Windows sink primitives (same construction as the decode-read arm) ---

mod win {
    use std::ffi::c_void;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

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
        fn WriteFile(h: Handle, buf: *const u8, len: u32, written: *mut u32, ov: *mut c_void)
        -> i32;
        fn PeekNamedPipe(
            h: Handle,
            buf: *mut u8,
            blen: u32,
            bread: *mut u32,
            avail: *mut u32,
            left: *mut u32,
        ) -> i32;
        fn GetLastError() -> u32;
    }

    pub fn stderr_handle() -> Handle {
        unsafe { GetStdHandle(STD_ERROR_HANDLE) }
    }

    pub fn stderr_file_type() -> u32 {
        unsafe { GetFileType(stderr_handle()) }
    }

    pub struct Sink {
        pub original: Handle,
        pub read: Handle,
        pub write: Handle,
    }

    /// Install a fresh anonymous pipe as stderr. `broken` closes the read end.
    pub fn replace_stderr(broken: bool) -> Sink {
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
        let ok = unsafe { SetStdHandle(STD_ERROR_HANDLE, w) };
        assert_eq!(ok, 1, "PREMISE: SetStdHandle(STD_ERROR_HANDLE) failed");
        assert_eq!(
            unsafe { GetFileType(stderr_handle()) },
            FILE_TYPE_PIPE,
            "PREMISE: std stderr must now resolve to the real pipe"
        );
        if broken {
            unsafe { CloseHandle(r) };
        }
        Sink {
            original,
            read: r,
            write: w,
        }
    }

    pub fn restore(sink: &Sink) {
        let ok = unsafe { SetStdHandle(STD_ERROR_HANDLE, sink.original) };
        assert_eq!(ok, 1, "TEARDOWN: restore the original stderr handle");
    }

    pub fn peek(sink: &Sink) -> Option<(u32, u32)> {
        let mut avail = 0u32;
        let mut left = 0u32;
        let ok = unsafe {
            PeekNamedPipe(
                sink.read,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut avail,
                &mut left,
            )
        };
        if ok == 0 { None } else { Some((avail, left)) }
    }

    /// One raw `WriteFile` on the pipe (never `std::io::stderr`, so the std
    /// lock is not involved: pure OS pipe backpressure).
    pub fn spawn_raw_flood(write: Handle, bytes: usize) -> (Arc<AtomicBool>, Arc<AtomicBool>) {
        let issued = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let h = write as usize;
        let issued_thread = Arc::clone(&issued);
        let done_thread = Arc::clone(&done);
        std::thread::Builder::new()
            .name("q324-raw-flood".into())
            .spawn(move || {
                let buf = vec![b'F'; bytes];
                let mut written = 0u32;
                issued_thread.store(true, Ordering::SeqCst);
                let _ = unsafe {
                    WriteFile(
                        h as Handle,
                        buf.as_ptr(),
                        buf.len() as u32,
                        &mut written,
                        std::ptr::null_mut(),
                    )
                };
                done_thread.store(true, Ordering::SeqCst);
            })
            .expect("raw flood thread spawns");
        (issued, done)
    }

    pub const ERROR_BROKEN_PIPE: i32 = 109;
    pub const ERROR_NO_DATA: i32 = 232;
}

// --- receipts (stdout; the C1/C4 stderr is broken or blocked by design) ---

static RECEIPT_LOCK: Mutex<()> = Mutex::new(());

fn line(text: &str) {
    let _guard = RECEIPT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut out = std::io::stdout().lock();
    writeln!(out, "{text}").expect("stdout receipt");
    out.flush().expect("stdout receipt flush");
}

fn stdout_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let _guard = RECEIPT_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "ASSERTION {info}");
        let _ = out.flush();
    }));
}

fn wait_until(mut predicate: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "WATCHDOG (fail-closed): condition never reached: {what}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

// --- the one facility of this test binary --------------------------------

static FACILITY: OnceLock<Diagnostics> = OnceLock::new();

fn install_facility(exporter: Box<dyn Exporter>) -> &'static Diagnostics {
    FACILITY.get_or_init(|| {
        Diagnostics::install(
            exporter,
            Config {
                beta_capacity: 4,
                poll_window: Duration::from_millis(1),
            },
        )
        .expect("this test binary installs exactly one facility")
    })
}

fn diagnostics_handle() -> &'static Diagnostics {
    FACILITY.get().expect("facility installed")
}

// --- fault sinks ---------------------------------------------------------

/// C2/C3: the consumer is stopped inside a real `Exporter::export` call.
struct SeedGateExporter {
    entered: mpsc::Sender<()>,
    gate: Arc<(Mutex<bool>, Condvar)>,
}

impl Exporter for SeedGateExporter {
    fn name(&self) -> &'static str {
        "q324-seed-gate"
    }

    fn export(&mut self, record: &Record) -> Result<(), ExportError> {
        if record.site == "q324-seed" {
            self.entered.send(()).expect("witness receiver alive");
            let (lock, cv) = &*self.gate;
            let guard = lock
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            drop(
                cv.wait_while(guard, |released| !*released)
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
            );
        }
        Ok(())
    }
}

/// C1/C4: delegate the actual write to the REAL `StderrExporter`, signalling
/// entry first so the witness proves the drainer reached the real export call.
struct WitnessedStderrExporter {
    entered: Option<mpsc::Sender<()>>,
    inner: StderrExporter,
}

impl Exporter for WitnessedStderrExporter {
    fn name(&self) -> &'static str {
        "q324-witnessed-stderr"
    }

    fn export(&mut self, record: &Record) -> Result<(), ExportError> {
        if let Some(tx) = self.entered.take() {
            let _ = tx.send(());
        }
        self.inner.export(record)
    }

    fn flush(&mut self) -> Result<(), ExportError> {
        self.inner.flush()
    }
}

fn stopped_consumer() -> (mpsc::Receiver<()>, Arc<(Mutex<bool>, Condvar)>) {
    let (tx, rx) = mpsc::channel();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let _ = install_facility(Box::new(SeedGateExporter {
        entered: tx,
        gate: Arc::clone(&gate),
    }));
    assert_eq!(
        emit(Record::new("q324-seed", Level::Info)),
        qianqian_diagnostics::Disposition::BetaQueued,
        "PREMISE: the seed must enter the real β channel"
    );
    rx.recv_timeout(Duration::from_secs(5))
        .expect("PREMISE: the real drainer entered export() and waits");
    line("QFB-PREMISE consumer_held_in_export=1");
    (rx, gate)
}

// --- the real producer ---------------------------------------------------

struct ProducerReport {
    abi: u32,
    reads: u32,
    frames_before_fault: usize,
    first_error: String,
    terminal_error: String,
    elapsed_us: u128,
    attempts_after_fill: u64,
    accepted_after_fill: u64,
    rejected_after_fill: u64,
    attempts_after: u64,
    accepted_after: u64,
    rejected_after: u64,
    outstanding_after: u32,
    fill: Option<FillWitness>,
}

struct FillWitness {
    attempts: u32,
    accepted: u32,
    rejected: u32,
    first_reject_attempt: Option<u32>,
}

/// Fill THIS thread's current α pool until its own admission boundary rejects.
fn fill_current_alpha() -> FillWitness {
    assert!(
        qianqian_diagnostics::is_alpha_producer(),
        "fill must run on the α-registered producer thread"
    );
    let mut w = FillWitness {
        attempts: 0,
        accepted: 0,
        rejected: 0,
        first_reject_attempt: None,
    };
    for attempt in 1..=3 {
        w.attempts = attempt;
        match emit(Record::new("qfb-fill", Level::Info).with_message(Text::from_static("capacity")))
        {
            qianqian_diagnostics::Disposition::AlphaRecorded => w.accepted += 1,
            qianqian_diagnostics::Disposition::AlphaRejected => {
                w.rejected += 1;
                w.first_reject_attempt = Some(attempt);
                break;
            }
            other => panic!("CAPACITY-ORACLE: wrong admission path {other:?}"),
        }
    }
    w
}

fn activate_decode_plugin_and_capture() -> std::rc::Rc<dyn PcmDecode> {
    use std::cell::RefCell;
    thread_local! {
        static CAPTURED: RefCell<Option<std::rc::Rc<dyn PcmDecode>>> = const { RefCell::new(None) };
    }
    let mut k = CompositionKernel::new();
    k.register_component(qianqian_decode_songcore::songcore_decode_plugin())
        .expect("decode plugin registers");
    k.register_component(
        ComponentSpec::new("qfb_consumer")
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
        DesiredEntry::enabled("qfb_consumer", "qfb_consumer", Revision::fresh()),
    ])
    .expect("both fibers desired");
    k.settle();
    let snap = k.snapshot();
    for name in ["songcore_decode_plugin", "qfb_consumer"] {
        let fiber = snap
            .fibers
            .get(name)
            .unwrap_or_else(|| panic!("{name} fiber present"));
        assert_eq!(
            fiber.state,
            FiberState::Active,
            "{name} must be active for the witness to mean anything"
        );
    }
    CAPTURED.with(|c| c.borrow().as_ref().expect("service captured").clone())
}

fn truncated_fault_media() -> (PathBuf, String) {
    let (_fx, path) = common::load_fixture("flac-16-44-stereo");
    let bytes = std::fs::read(&path).expect("fixture readable");
    let truncated = &bytes[..100_000];
    let mut hasher = common::Sha256::new();
    hasher.update(truncated);
    let sha = hasher.hex();
    assert_eq!(
        sha, TRUNCATED_SHA256,
        "PREMISE: truncated media identity drifted from closeout2"
    );
    let out = std::env::temp_dir().join(format!("q324-trunc100k-{}.flac", std::process::id()));
    std::fs::write(&out, truncated).expect("truncated media written");
    (out, sha)
}

fn spawn_producer(path: PathBuf, prefill: bool) -> mpsc::Receiver<ProducerReport> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("qfb-native-producer".into())
        .spawn(move || {
            let started = Instant::now();
            let abi = unsafe { qianqian_songcore_sys::songcore_abi_version() };
            let registration = AlphaRegistration::register();
            let decode = activate_decode_plugin_and_capture();
            let mut stream = decode
                .open_media(&path)
                .expect("the truncated source still opens and probes");
            let channels = stream.format().channels as usize;
            let mut buf = vec![0f32; 8192 * channels];

            let fill = if prefill {
                let w = fill_current_alpha();
                line(&format!(
                    "QFB-CAPACITY fill_attempts={} accepted={} rejected={} first_reject_attempt={:?}",
                    w.attempts, w.accepted, w.rejected, w.first_reject_attempt
                ));
                Some(w)
            } else {
                None
            };
            let attempts_after_fill = registration.attempted();
            let accepted_after_fill = registration.accepted();
            let rejected_after_fill = registration.rejected();

            let mut frames = 0usize;
            let mut reads = 0u32;
            let first_error = loop {
                reads += 1;
                assert!(
                    reads < 10_000,
                    "PRODUCER-PROGRESS: the real decode never reached its fault"
                );
                match stream.read_frames(&mut buf) {
                    Ok(DecodeOutcome::Frames(n)) => frames += n,
                    Ok(other) => panic!("unexpected non-error outcome: {other:?}"),
                    Err(e) => break e,
                }
            };
            let terminal = stream
                .read_frames(&mut buf)
                .expect_err("the fatal decode error persists on the terminal call");
            drop(stream); // the mandatory resource release: drop -> song_close

            let report = ProducerReport {
                abi,
                reads,
                frames_before_fault: frames,
                first_error: first_error.message.clone(),
                terminal_error: terminal.message,
                elapsed_us: started.elapsed().as_micros(),
                attempts_after_fill,
                accepted_after_fill,
                rejected_after_fill,
                attempts_after: registration.attempted(),
                accepted_after: registration.accepted(),
                rejected_after: registration.rejected(),
                outstanding_after: registration.outstanding(),
                fill,
            };
            drop(registration);
            tx.send(report).expect("report receiver alive");
        })
        .expect("producer thread spawns");
    rx
}

fn await_producer(rx: &mpsc::Receiver<ProducerReport>, deadline: Duration) -> ProducerReport {
    rx.recv_timeout(deadline).unwrap_or_else(|_| {
        panic!(
            "PRODUCER-NOT-BACKPRESSURED: the real native producer did not complete within {:?} \
             while the diagnostic consumer was held",
            deadline
        )
    })
}

fn print_producer(cell: &str, sha: &str, report: &ProducerReport) {
    line(&format!(
        "QFB-{cell}-PRODUCER artifact_abi={} media_sha256={} reads={} frames_before_fault={} \
         first_error={:?} terminal_error={:?} elapsed_us={}",
        report.abi,
        sha,
        report.reads,
        report.frames_before_fault,
        report.first_error,
        report.terminal_error,
        report.elapsed_us
    ));
    line(&format!(
        "QFB-{cell}-REGISTRATION attempted={} accepted={} rejected={} outstanding={}",
        report.attempts_after,
        report.accepted_after,
        report.rejected_after,
        report.outstanding_after
    ));
}

fn print_counters(cell: &str, c: &Counters) {
    line(&format!(
        "QFB-{cell}-COUNTERS beta_queued={} beta_rejected={} beta_consumed={} \
         alpha_rejected={} copied_out={} exported={} export_failed={} outstanding={} \
         self_diagnostics={} critical_panics={}",
        c.beta_queued,
        c.beta_rejected,
        c.beta_consumed,
        c.alpha_rejected,
        c.copied_out,
        c.exported,
        c.export_failed,
        c.outstanding,
        c.self_diagnostics,
        c.critical_panics
    ));
}

fn artifact_sha256() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/build/artifacts/libsongcore.a");
    let bytes = std::fs::read(&path).expect("native artifact readable");
    let mut hasher = common::Sha256::new();
    hasher.update(&bytes);
    hasher.hex()
}

fn assert_artifact_identity() -> String {
    let actual = artifact_sha256();
    line(&format!("ARTIFACT_SHA256={actual}"));
    line(&format!(
        "ARTIFACT_BUILD_TIME_SHA256_RAW={}",
        qianqian_songcore_sys::QN_SONGCORE_ARTIFACT_SHA256
    ));
    let embedded = qianqian_songcore_sys::QN_SONGCORE_ARTIFACT_SHA256
        .trim_start_matches('\\')
        .trim();
    assert_eq!(
        actual, EXPECTED_ARTIFACT_SHA256,
        "PREMISE: verified windows-x86_64 artifact"
    );
    assert_eq!(
        actual, embedded,
        "PREMISE: the binary must be linked against the artifact on disk"
    );
    actual
}

/// Establish the C1 blocked-sink premise: a real no-reader anonymous pipe
/// whose write quota is exhausted, witnessed by two independent parked writes.
fn block_stderr_pipe() -> (win::Sink, Arc<AtomicBool>, Arc<AtomicBool>, u32) {    let sink = win::replace_stderr(false);
    line(&format!(
        "QFB-PREMISE stderr_file_type={} pipe=1 read_end_held_not_read=1",
        win::stderr_file_type()
    ));
    let (issued, done) = win::spawn_raw_flood(sink.write, 64 * 1024);
    wait_until(|| issued.load(Ordering::SeqCst), "flood write issued");
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        !done.load(Ordering::SeqCst),
        "C1-PREMISE: the 64 KiB flood completed — the sink is not backpressured"
    );
    let (avail, _) = win::peek(&sink).expect("PREMISE: PeekNamedPipe on the held read end");
    assert!(avail > 0, "C1-PREMISE: the pipe queue must be non-empty");
    let (probe_issued, probe_done) = win::spawn_raw_flood(sink.write, 1);
    wait_until(
        || probe_issued.load(Ordering::SeqCst),
        "the independent 1-byte probe write was issued",
    );
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        !probe_done.load(Ordering::SeqCst),
        "C1-PREMISE: a fresh 1-byte write completed — the pipe still accepts data"
    );
    line(&format!(
        "QFB-PREMISE SINK_HOLD_ENTERED queued_bytes={avail} flood_bytes=65536 \
         flood_outstanding=1 second_write_outstanding=1"
    ));
    let _ = issued;
    (sink, done, probe_done, avail)
}

// ---------------------------------------------------------------------------
// C1 BLOCKED_SINK
// ---------------------------------------------------------------------------

fn run_c1() {
    let sha = assert_artifact_identity();
    let (media, media_sha) = truncated_fault_media();
    let (sink, flood_done, probe_done, _avail) = block_stderr_pipe();
    let (entered_tx, entered_rx) = mpsc::channel();
    let diagnostics = install_facility(Box::new(WitnessedStderrExporter {
        entered: Some(entered_tx),
        inner: StderrExporter::default(),
    }));
    let rx = spawn_producer(media.clone(), false);
    let report = await_producer(&rx, Duration::from_secs(10));
    print_producer("C1", &media_sha, &report);
    entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("PREMISE: the drainer reached the real exporter while the sink was blocked");
    let c = diagnostics.counters();
    let sink_blocked = !flood_done.load(Ordering::SeqCst)
        && !probe_done.load(Ordering::SeqCst)
        && c.exported == 0
        && c.export_failed == 0;
    line(&format!(
        "QFB-PREMISE EXPORTER_ENTERED=1 SINK_STILL_BLOCKED={} exported={} export_failed={}",
        if sink_blocked { 1 } else { 0 },
        c.exported,
        c.export_failed
    ));

    print_counters("C1", &c);
    assert_eq!(
        report.frames_before_fault, 115_200,
        "RELIABLE-RESULT: the truncated stream decodes its 25 intact frames (25 x 4608) before the fault"
    );
    assert!(
        report.first_error.contains("pending decode error"),
        "RELIABLE-RESULT: the native read fault surfaced unchanged, got {:?}",
        report.first_error
    );
    assert_eq!(
        report.terminal_error, report.first_error,
        "TERMINAL-OUTCOME: the fatal classification persists unchanged"
    );
    assert_eq!(
        report.attempts_after,
        report.accepted_after + report.rejected_after,
        "LOSS-ACCOUNTING: attempted = accepted + rejected at the α admission boundary"
    );
    assert_eq!(
        report.accepted_after, 2,
        "LOSS-ACCOUNTING: the two-slot pool accepted exactly two records while the sink was blocked"
    );
    assert!(
        report.rejected_after >= 1,
        "LOSS-ACCOUNTING: later bridge/fault records were rejected and counted, got {}",
        report.rejected_after
    );
    assert_eq!(
        report.outstanding_after, 2,
        "LOSS-ACCOUNTING: the two admitted records remain outstanding in the blocked pool"
    );
    assert_eq!(
        c.exported, 0,
        "PREMISE: the blocked real StderrExporter delivered nothing"
    );
    assert_eq!(
        c.self_diagnostics, 0,
        "a blocked sink is not a failed sink: no exporter disable"
    );
    assert!(
        sink_blocked,
        "C1-ISOLATION: the sink was still blocked (both raw writes parked, nothing delivered) when the producer returned"
    );
    line(&format!("ARTIFACT_SHA256_REPORTED={sha}"));

    let report_shutdown = diagnostics.shutdown(Duration::from_millis(50));
    line(&format!(
        "QFB-C1-SHUTDOWN drained={} abandoned={} undelivered_at_exit={}",
        report_shutdown.drained,
        report_shutdown.abandoned,
        report_shutdown.undelivered_at_exit
    ));
    assert!(
        report_shutdown.abandoned && !report_shutdown.drained,
        "TEARDOWN-OUTCOME: a drainer blocked in the sink must be honestly abandoned: {report_shutdown:?}"
    );
    assert!(
        report_shutdown.undelivered_at_exit >= 1,
        "LOSS-ACCOUNTING: abandoned records stay counted as undelivered"
    );
    let _ = std::fs::remove_file(media);
}

// ---------------------------------------------------------------------------
// C2 STOPPED_CONSUMER
// ---------------------------------------------------------------------------

fn run_c2() {
    let _sha = assert_artifact_identity();
    let (media, media_sha) = truncated_fault_media();
    let (_entered, _gate) = stopped_consumer();
    let diagnostics = diagnostics_handle();
    let rx = spawn_producer(media.clone(), false);
    let report = await_producer(&rx, Duration::from_secs(10));
    print_producer("C2", &media_sha, &report);

    let c = diagnostics.counters();
    print_counters("C2", &c);
    assert!(
        report.first_error.contains("pending decode error"),
        "RELIABLE-RESULT: the native read fault surfaced unchanged"
    );
    assert_eq!(
        report.attempts_after,
        report.accepted_after + report.rejected_after,
        "LOSS-ACCOUNTING: attempted = accepted + rejected"
    );
    assert_eq!(
        report.accepted_after, 2,
        "LOSS-ACCOUNTING: exactly the two pool slots were admitted"
    );
    assert!(
        report.rejected_after >= 1,
        "LOSS-ACCOUNTING: later records rejected and counted"
    );
    assert_eq!(c.beta_queued, 1, "the seed entered the real β channel once");
    assert_eq!(c.beta_consumed, 1, "the drainer consumed the seed once");
    assert_eq!(
        c.exported, 0,
        "PREMISE: the stopped consumer is still inside export()"
    );

    let report_shutdown = diagnostics.shutdown(Duration::from_millis(50));
    line(&format!(
        "QFB-C2-SHUTDOWN drained={} abandoned={} undelivered_at_exit={}",
        report_shutdown.drained,
        report_shutdown.abandoned,
        report_shutdown.undelivered_at_exit
    ));
    assert!(
        report_shutdown.abandoned && !report_shutdown.drained,
        "TEARDOWN-OUTCOME: a stopped consumer must yield an honest abandoned shutdown, \
         never a claimed drain: {report_shutdown:?}"
    );
    assert!(
        report_shutdown.undelivered_at_exit >= 1,
        "LOSS-ACCOUNTING: abandoned records must remain visible as undelivered"
    );
    let _ = std::fs::remove_file(media);
}

// ---------------------------------------------------------------------------
// C3 CAPACITY_EXHAUSTION
// ---------------------------------------------------------------------------

fn run_c3() {
    let _sha = assert_artifact_identity();
    let (media, media_sha) = truncated_fault_media();
    let (_entered, _gate) = stopped_consumer();
    let diagnostics = diagnostics_handle();
    let rx = spawn_producer(media.clone(), true);
    let report = await_producer(&rx, Duration::from_secs(10));
    print_producer("C3", &media_sha, &report);

    let fill = report.fill.as_ref().expect("C3 ran with the prefill witness");
    assert!(
        fill.first_reject_attempt.is_some(),
        "CAPACITY-ORACLE: the α pool did not reject within the bounded fill loop"
    );
    assert!(
        fill.rejected >= 1,
        "CAPACITY-ORACLE: the first rejection is the pool-bound witness"
    );
    assert_eq!(
        report.outstanding_after, 2,
        "CAPACITY-ORACLE: the pool's outstanding distance stays at its two-slot bound"
    );

    let target_attempts = report.attempts_after - report.attempts_after_fill;
    let target_rejected = report.rejected_after - report.rejected_after_fill;
    let target_accepted = report.accepted_after - report.accepted_after_fill;
    line(&format!(
        "QFB-C3-TARGET attempts={} accepted={} rejected={}",
        target_attempts, target_accepted, target_rejected
    ));
    assert!(
        target_attempts > 0,
        "PRODUCER-PROGRESS: the real bridge attempted no record while the pool was full"
    );
    assert_eq!(
        target_accepted, 0,
        "CAPACITY-ORACLE: no target record may be accepted into a full pool"
    );
    assert_eq!(
        target_rejected, target_attempts,
        "REJECT-ACCOUNTING: every full-pool attempt must be rejected and counted"
    );
    assert!(
        report.first_error.contains("pending decode error"),
        "RELIABLE-RESULT: the native read fault surfaced unchanged"
    );
    assert_eq!(
        report.terminal_error, report.first_error,
        "TERMINAL-OUTCOME: the fatal classification persists unchanged"
    );

    let c = diagnostics.counters();
    print_counters("C3", &c);
    assert_eq!(
        c.alpha_rejected,
        u64::from(fill.rejected) + target_rejected,
        "LOSS-ACCOUNTING: the global rejection counter equals the fill witness plus the \
         producer's rejected attempts"
    );

    let report_shutdown = diagnostics.shutdown(Duration::from_millis(50));
    line(&format!(
        "QFB-C3-SHUTDOWN drained={} abandoned={} undelivered_at_exit={}",
        report_shutdown.drained,
        report_shutdown.abandoned,
        report_shutdown.undelivered_at_exit
    ));
    assert!(
        report_shutdown.abandoned && !report_shutdown.drained,
        "TEARDOWN-OUTCOME: the held consumer yields an honest abandoned shutdown"
    );
    let _ = std::fs::remove_file(media);
}

// ---------------------------------------------------------------------------
// C4 WRITE_FAILURE
// ---------------------------------------------------------------------------

fn run_c4() {
    let _sha = assert_artifact_identity();
    let (media, media_sha) = truncated_fault_media();
    // PREMISE: a real failing sink — anonymous pipe with the READ end closed.
    // The canary proves the failure inside this child before any producer work,
    // on the very process stderr handle the exporter later writes to.
    let sink = win::replace_stderr(true);
    let error = write_stderr_line("QFB-C4-WRITE-CANARY")
        .expect_err("PREMISE: the real stderr writer must fail inside this child");
    let code = error.raw_os_error().unwrap_or(-1);
    assert!(
        code == win::ERROR_NO_DATA || code == win::ERROR_BROKEN_PIPE,
        "C4-PREMISE: expected ERROR_NO_DATA (232) or ERROR_BROKEN_PIPE (109), got {code}"
    );
    line(&format!("QFB-PREMISE BROKEN-STDERR-CONFIRMED raw_os_error={code}"));

    let (entered_tx, entered_rx) = mpsc::channel();
    let diagnostics = install_facility(Box::new(WitnessedStderrExporter {
        entered: Some(entered_tx),
        inner: StderrExporter::default(),
    }));

    let rx = spawn_producer(media.clone(), false);
    let report = await_producer(&rx, Duration::from_secs(10));
    print_producer("C4", &media_sha, &report);
    entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("PREMISE: the drainer reached the real exporter");
    line("QFB-PREMISE EXPORTER_ENTERED=1 broken_stderr=1");
    assert!(
        report.first_error.contains("pending decode error"),
        "RELIABLE-RESULT: the native read fault surfaced unchanged despite the failing sink"
    );
    assert_eq!(
        report.terminal_error, report.first_error,
        "TERMINAL-OUTCOME: the fatal classification persists unchanged"
    );
    assert!(
        report.attempts_after > 0,
        "PREMISE: the real bridge published records to the facility"
    );

    // Poll the drainer's fault accounting (bounded, drainer cadence).
    let deadline = Instant::now() + Duration::from_secs(5);
    let c = loop {
        let c = diagnostics.counters();
        if c.export_failed >= 1 && c.self_diagnostics == 1 {
            break c;
        }
        assert!(
            Instant::now() < deadline,
            "LOSS-ACCOUNTING: the failing exporter was never accounted: {c:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    win::restore(&sink);
    print_counters("C4", &c);

    assert_eq!(
        c.exported, 0,
        "LOSS-ACCOUNTING: a failed write must never be counted as delivered"
    );
    assert_eq!(
        c.self_diagnostics, 1,
        "LOSS-ACCOUNTING: exactly one exporter-disable self-diagnostic"
    );

    let report_shutdown = diagnostics.shutdown(Duration::from_secs(2));
    line(&format!(
        "QFB-C4-SHUTDOWN drained={} abandoned={} undelivered_at_exit={}",
        report_shutdown.drained,
        report_shutdown.abandoned,
        report_shutdown.undelivered_at_exit
    ));
    assert!(
        report_shutdown.drained && !report_shutdown.abandoned,
        "TEARDOWN-OUTCOME: the drainer exits and the sweep is complete: {report_shutdown:?}"
    );
    let c = report_shutdown.counters;
    print_counters("C4-final", &c);
    assert_eq!(
        c.copied_out + c.beta_consumed,
        c.exported + c.export_failed,
        "LOSS-ACCOUNTING: copied_out + beta_consumed = exported + export_failed at quiescence"
    );
    assert!(
        c.export_failed >= 1,
        "LOSS-ACCOUNTING: the real write failures are charged as export losses"
    );
    assert_eq!(
        c.exported, 0,
        "LOSS-ACCOUNTING: nothing was delivered through the broken sink"
    );
    let _ = std::fs::remove_file(media);
}

// ---------------------------------------------------------------------------
// the single fresh-process witness
// ---------------------------------------------------------------------------

#[test]
fn qfb_native_ffmpeg_bridge_cell() {
    stdout_panic_hook();
    // Unarmed (e.g. a plain `cargo test` of the crate): the control is inert.
    let Ok(cell) = std::env::var("Q324_CELL") else {
        return;
    };
    let log = std::env::var("QIANQIAN_FFMPEG_LOG").expect("QIANQIAN_FFMPEG_LOG must be set");
    assert_eq!(log, "1", "PREMISE: the FFmpeg log bridge opt-in");
    line(&format!(
        "QFB-ENV cell={} qianqian_ffmpeg_log={} pid={}",
        cell,
        log,
        std::process::id()
    ));
    match cell.as_str() {
        "C1" => run_c1(),
        "C2" => run_c2(),
        "C3" => run_c3(),
        "C4" => run_c4(),
        other => panic!("unknown Q324_CELL {other}"),
    }
    line(&format!("QFB-{cell}-CELL-PASS"));
}
