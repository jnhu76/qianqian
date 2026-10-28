//! #296 decode-read × C1–C4 native runtime fault cells (Linux x86_64).
//!
//! Real producer definition (task authority): a genuine native decode READ
//! fault — a corrupted/truncated FLAC fixture drives the real SongCore
//! `song_read_pcm` C path to an FFmpeg `AVERROR_*`, surfaced as
//! `DecodeError` plus the diagnostic record. No upper-layer `DecodeError` is
//! fabricated: the bytes are corrupted on disk and the error comes back
//! through the real statically-linked SongCore artifact (identity asserted
//! below) and the real decode plugin path.
//!
//! Controls (fresh process per cell — the facility is process-global and the
//! `av_log` callback is process-global):
//! * C1 BLOCKED_SINK: the real `StderrExporter` blocks on the real std stderr
//!   lock held by a helper. The real producer must return, publish, and the
//!   pool must account drops; shutdown must abandon honestly.
//! * C2 STOPPED_CONSUMER: the sole real drainer is held inside a test
//!   exporter (entered witness) and never released. Trigger close; shutdown
//!   must return bounded, report `abandoned`, and never claim `drained`.
//! * C3 CAPACITY_EXHAUSTION: same stopped drainer; the real producer floods
//!   the two-slot α pool (two native `av_log` records + repeated fault
//!   records). Every over-capacity presentation is rejected+counted; the
//!   pool stays bounded; the producer never blocks.
//! * C4 WRITE_FAILURE: fd 2 is redirected to a pipe whose read end is closed
//!   (real OS write failure; canary self-witnessed inside the process). The
//!   real `StderrExporter` returns `ExportError`, the drainer disables it,
//!   and the producer result is unchanged while loss accounting stays exact.
//!
//! Execution discipline:
//! ```text
//! QIANQIAN_NATIVE_DIR=<verified artifact tree> \
//!   cargo test --manifest-path crates/qianqian-decode-songcore/Cargo.toml \
//!     --test native_decode_read_controls -- --exact <cell> --nocapture --test-threads=1
//! ```
//! Each cell runs as its own process (`--exact` selects one test); the
//! facility is installed exactly once per process.

#[allow(dead_code)]
mod common;

use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use qianqian_audio_api::ports::{
    DecodeError, DecodeOutcome, DecodedPcmStream, PcmDecode, PcmDecodeCapability,
};
use qianqian_composition::{ComponentSpec, CompositionKernel, DesiredEntry, FiberState, Revision};
use qianqian_diagnostics::{
    AlphaRegistration, Config, Counters, Diagnostics, Disposition, ExportError, Exporter, Level,
    Record, StderrExporter, write_stderr_line,
};

// --- artifact identity (task starting point) ------------------------------

/// The verified Linux x86_64 artifact from closeout2. The build script embeds
/// the archive's sha256 as `QN_SONGCORE_ARTIFACT_SHA256`; a drift here means
/// the cell is not running against the verified producer path.
const EXPECTED_ARTIFACT_SHA256: &str =
    "a0a3baf8eb61be8e991517d9653156a3efe0f67c9ba09fff1deb30187e5fe056";

// Empirically pinned corruption (closeout2 native-decode-faults): one bit
// inside the 19th FLAC frame body; the frame headers stay intact so the
// failure occurs while decoding, not while parsing.
const READ_FAULT_FLIP: (usize, u8) = (76_000, 0x01);

// --- evidence channel (stdout, never the controlled stderr) ---------------

#[allow(clippy::explicit_write)]
fn evidence(line: &str) {
    writeln!(std::io::stdout(), "{line}").unwrap();
    std::io::stdout().flush().unwrap();
}

fn native_artifact_path() -> PathBuf {
    if let Ok(dir) = std::env::var("QIANQIAN_NATIVE_DIR") {
        return PathBuf::from(dir).join("build/artifacts/libsongcore.a");
    }
    let mut dir = Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    while let Some(d) = dir {
        let candidate = d.join("native/build/artifacts/libsongcore.a");
        if candidate.exists() {
            return candidate;
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    panic!("PREMISE: verified SongCore artifact not found; set QIANQIAN_NATIVE_DIR");
}

fn assert_artifact_identity() {
    let path = native_artifact_path();
    let mut hasher = common::Sha256::new();
    hasher.update(&std::fs::read(&path).expect("artifact bytes"));
    let actual = hasher.hex();
    evidence(&format!("ARTIFACT_PATH={}", path.display()));
    evidence(&format!("ARTIFACT_SHA256={actual}"));
    assert_eq!(
        actual, EXPECTED_ARTIFACT_SHA256,
        "PREMISE: this cell must run against the verified closeout2 artifact"
    );
}

// --- the real production decode path --------------------------------------

fn activate_decode_plugin_and_capture() -> Rc<dyn PcmDecode> {
    thread_local! {
        static CAPTURED: RefCell<Option<Rc<dyn PcmDecode>>> = const { RefCell::new(None) };
    }
    let mut k = CompositionKernel::new();
    k.register_component(qianqian_decode_songcore::songcore_decode_plugin())
        .expect("decode plugin registers");
    k.register_component(
        ComponentSpec::new("fault_consumer")
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
    .expect("fault consumer registers");
    k.set_desired(vec![
        DesiredEntry::enabled(
            "songcore_decode_plugin",
            "songcore_decode_plugin",
            Revision::fresh(),
        ),
        DesiredEntry::enabled("fault_consumer", "fault_consumer", Revision::fresh()),
    ])
    .expect("both fibers desired");
    k.settle();
    let snap = k.snapshot();
    for name in ["songcore_decode_plugin", "fault_consumer"] {
        let fiber = snap
            .fibers
            .get(name)
            .unwrap_or_else(|| panic!("{name} fiber present"));
        assert_eq!(
            fiber.state,
            FiberState::Active,
            "{name} must be active for this cell to mean anything"
        );
    }
    CAPTURED.with(|c| c.borrow().as_ref().expect("service captured").clone())
}

fn corrupted_read_fixture(name: &str) -> PathBuf {
    let (_fx, path) = common::load_fixture("flac-16-44-stereo");
    let mut bytes = std::fs::read(&path).expect("fixture bytes");
    bytes[READ_FAULT_FLIP.0] ^= READ_FAULT_FLIP.1;
    let out = std::env::temp_dir().join(format!(
        "q296-decode-read-{}-{name}.flac",
        std::process::id()
    ));
    std::fs::write(&out, &bytes).expect("corrupted copy written");
    out
}

fn open_and_read_to_fault(
    decode: &dyn PcmDecode,
    path: &Path,
) -> (Box<dyn DecodedPcmStream>, DecodeError) {
    let mut stream = decode
        .open_media(path)
        .expect("the corrupted file still opens and probes (corruption is mid-stream)");
    let channels = stream.format().channels as usize;
    let mut buf = vec![0f32; 8192 * channels];
    let first = loop {
        match stream.read_frames(&mut buf) {
            Ok(DecodeOutcome::Frames(n)) if n > 0 => continue,
            Ok(other) => panic!("unexpected non-error outcome before the fault: {other:?}"),
            Err(e) => break e,
        }
    };
    (stream, first)
}

// --- producer receipt and worker -----------------------------------------

#[derive(Debug)]
struct ProducerReceipt {
    first_message: String,
    terminal_messages: Vec<String>,
    attempted: u64,
    accepted: u64,
    rejected: u64,
    copied_out: u64,
    outstanding: u32,
}

#[derive(Clone, Copy)]
enum ReadPlan {
    FaultOnly,
    FaultThenTerminal,
    FaultThenRepeats(usize),
}

/// Run the real producer on a dedicated thread (the production decode worker
/// is its own thread). The receipt is sent only after the producer returned
/// and the stream was dropped (`song_close` completed), so receiving it IS
/// the producer-progress witness. The bounded receive in the cell is a
/// fail-closed watchdog, not the property observation.
fn spawn_native_read_fault(name: &'static str, plan: ReadPlan) -> mpsc::Receiver<ProducerReceipt> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name(format!("q296-producer-{name}"))
        .spawn(move || {
            // Production shape: the decode worker is an isolation-obligated
            // class-α producer (crates/qianqian-playback/src/session.rs:473).
            let registration = AlphaRegistration::register();
            let decode = activate_decode_plugin_and_capture();
            let file = corrupted_read_fixture(name);
            let (mut stream, first) = open_and_read_to_fault(decode.as_ref(), &file);
            let mut terminal_messages = Vec::new();
            match plan {
                ReadPlan::FaultOnly => {}
                ReadPlan::FaultThenTerminal => {
                    let e = stream
                        .read_frames(&mut vec![0f32; 16_384])
                        .expect_err("fatal decode error persists");
                    terminal_messages.push(e.message);
                }
                ReadPlan::FaultThenRepeats(n) => {
                    for _ in 0..n {
                        let e = stream
                            .read_frames(&mut vec![0f32; 16_384])
                            .expect_err("fatal decode error persists");
                        terminal_messages.push(e.message);
                    }
                }
            }
            // Mandatory release on the producer thread; completes even while
            // the drainer is held (C1/C2/C3) or the sink is broken (C4).
            drop(stream);
            let receipt = ProducerReceipt {
                first_message: first.message,
                terminal_messages,
                attempted: registration.attempted(),
                accepted: registration.accepted(),
                rejected: registration.rejected(),
                copied_out: registration.copied_out(),
                outstanding: registration.outstanding(),
            };
            let _ = std::fs::remove_file(&file);
            tx.send(receipt).expect("cell observes producer progress");
            // Registration drops here, on the same thread, after publish.
        })
        .expect("producer thread spawns");
    rx
}

fn wait_for_counters(
    diagnostics: &Diagnostics,
    predicate: impl Fn(&Counters) -> bool,
    what: &str,
) -> Counters {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let counters = diagnostics.counters();
        if predicate(&counters) {
            return counters;
        }
        assert!(
            Instant::now() < deadline,
            "WATCHDOG (fail-closed): counters never reached {what}: {counters:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn assert_native_read_fault(receipt: &ProducerReceipt, cell: &str) {
    assert!(
        receipt.first_message.contains("pending decode error"),
        "{cell}-RELIABLE-RESULT: the native adapter's own diagnostic must surface, got {:?}",
        receipt.first_message
    );
    for terminal in &receipt.terminal_messages {
        assert_eq!(
            terminal, &receipt.first_message,
            "{cell}-TERMINAL: the latched fatal must persist unchanged"
        );
    }
}

// --- C1: BLOCKED_SINK -----------------------------------------------------

#[test]
#[ignore = "process-global diagnostics facility/av_log callback: run one cell per fresh process with --ignored --exact <cell>"]
fn c1_blocked_sink_native_decode_read() {
    assert_artifact_identity();

    // PREMISE: a helper holds the REAL process-global std stderr lock and
    // releases it to no one. Acquisition is observed before the producer runs.
    let (acquired_tx, acquired_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("q296-c1-stderr-holder".into())
        .spawn(move || {
            let _held = std::io::stderr().lock();
            acquired_tx.send(()).unwrap();
            std::thread::park(); // never released
        })
        .expect("stderr holder spawns");
    acquired_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("C1-PREMISE: helper acquired the real stderr lock");
    evidence("C1-PREMISE: REAL-STDERR-LOCK-HELD");

    // The opt-in bridge is installed by the real plugin activation, so the
    // native decoder's own av_log errors become class-α records.
    unsafe { std::env::set_var("QIANQIAN_FFMPEG_LOG", "1") };
    evidence(&format!(
        "FFMPEG_LOG_ENV={:?}",
        std::env::var("QIANQIAN_FFMPEG_LOG")
    ));

    let diagnostics = Diagnostics::install(
        Box::new(StderrExporter::default()),
        Config {
            beta_capacity: 4,
            poll_window: Duration::from_millis(1),
        },
    )
    .expect("this cell process installs exactly one facility");

    // Seed one β record: the sole real drainer pulls it and blocks inside the
    // real StderrExporter on the held lock. `beta_consumed` without `exported`
    // is the synchronization witness that the consumer is inside the write.
    assert_eq!(
        qianqian_diagnostics::emit(Record::new("c1-seed", Level::Info)),
        Disposition::BetaQueued,
        "C1-PREMISE: the seed enters the real β channel"
    );
    let seeded = wait_for_counters(
        &diagnostics,
        |c| c.beta_consumed >= 1 && c.exported == 0,
        "the drainer inside the blocked real export",
    );
    evidence(&format!(
        "C1-PREMISE: DRAINER-BLOCKED-IN-REAL-EXPORTER counters={seeded:?}"
    ));

    // PRODUCER_PROGRESS: real native decode-read fault while the sink blocks.
    let receipt_rx = spawn_native_read_fault("c1", ReadPlan::FaultThenTerminal);
    let receipt = receipt_rx.recv_timeout(Duration::from_secs(10)).expect(
        "C1-PRODUCER-PROGRESS: real decode producer did not return while the real sink was blocked \
         (counterfactual: a synchronous producer sink write never returns)",
    );
    evidence(&format!("C1-PRODUCER-RETURNED {receipt:?}"));

    // RELIABLE_RESULT / TERMINAL_OUTCOME: native DecodeError, fatal persists.
    assert_native_read_fault(&receipt, "C1");

    // LOSS_ACCOUNTING: two native av_log records filled the two-slot pool;
    // the fault records were rejected+counted; nothing was copied out.
    assert_eq!(
        receipt.accepted, 2,
        "C1-CAPACITY: the real bridge's two native log records fill the α pool"
    );
    assert_eq!(
        receipt.copied_out, 0,
        "C1-NO-CONSUMER: the blocked drainer copied nothing out"
    );
    assert_eq!(
        receipt.outstanding, 2,
        "C1-BOUNDED: outstanding stays at the pool bound"
    );
    assert!(
        receipt.rejected >= 2,
        "C1-DROP-ACCOUNTING: fault records were dropped+counted while blocked, got {}",
        receipt.rejected
    );
    assert_eq!(
        receipt.attempted,
        receipt.accepted + receipt.rejected,
        "C1-ADMISSION-IDENTITY: attempted = accepted + rejected"
    );
    evidence(&format!(
        "C1-POOL attempted={} accepted={} rejected={} copied_out={} outstanding={}",
        receipt.attempted, receipt.accepted, receipt.rejected, receipt.copied_out, receipt.outstanding
    ));

    // TEARDOWN_OUTCOME: bounded abandonment, never a fake drained.
    let report = diagnostics.shutdown(Duration::from_millis(50));
    evidence(&format!("C1-SHUTDOWN {report:?}"));
    assert!(
        report.abandoned && !report.drained,
        "C1-TEARDOWN: a stuck sink is abandoned, never reported as drained"
    );
    assert_eq!(
        report.undelivered_at_exit, 3,
        "C1-LOSS-ACCOUNTING: 2 pool records + 1 drainer-held seed are honestly undelivered"
    );
    assert_eq!(
        report.counters.exported, 0,
        "C1-LOSS-ACCOUNTING: the blocked sink delivered nothing"
    );
    assert_eq!(
        report.counters.beta_consumed, 1,
        "C1-PREMISE: the seed is the drainer-held in-flight record"
    );
    assert_eq!(
        report.counters.alpha_rejected, receipt.rejected,
        "C1-LOSS-ACCOUNTING: every α rejection is counted"
    );
    evidence("C1-VERDICT=PASS");
}

// --- C2: STOPPED_CONSUMER -------------------------------------------------

struct HeldExporter {
    target: &'static str,
    entered: mpsc::Sender<()>,
    gate: Arc<(Mutex<bool>, Condvar)>,
}

impl Exporter for HeldExporter {
    fn name(&self) -> &'static str {
        "held"
    }

    fn export(&mut self, record: &Record) -> Result<(), ExportError> {
        if record.site == self.target {
            self.entered.send(()).unwrap();
            let (lock, cv) = &*self.gate;
            let guard = lock
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            // Released by no one: the consumer is STOPPED for the property.
            drop(
                cv.wait_while(guard, |released| !*released)
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
            );
        }
        Ok(())
    }
}

fn install_stopped_consumer(target: &'static str) -> (Diagnostics, InstallWitness) {
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (entered_tx, entered_rx) = mpsc::channel();
    let diagnostics = Diagnostics::install(
        Box::new(HeldExporter {
            target,
            entered: entered_tx,
            gate: gate.clone(),
        }),
        Config {
            beta_capacity: 4,
            poll_window: Duration::from_millis(1),
        },
    )
    .expect("this cell process installs exactly one facility");
    (diagnostics, InstallWitness { entered_rx, gate })
}

struct InstallWitness {
    entered_rx: mpsc::Receiver<()>,
    #[allow(dead_code)]
    gate: Arc<(Mutex<bool>, Condvar)>,
}

impl InstallWitness {
    fn wait_stopped(&self, cell: &str) {
        self.entered_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|_| {
                panic!("{cell}-PREMISE: the sole real drainer entered the exporter")
            });
        evidence(&format!("{cell}-PREMISE: CONSUMER-STOPPED-IN-EXPORT"));
    }
}

#[test]
#[ignore = "process-global diagnostics facility/av_log callback: run one cell per fresh process with --ignored --exact <cell>"]
fn c2_stopped_consumer_native_decode_read() {
    assert_artifact_identity();
    unsafe { std::env::set_var("QIANQIAN_FFMPEG_LOG", "1") };
    evidence(&format!(
        "FFMPEG_LOG_ENV={:?}",
        std::env::var("QIANQIAN_FFMPEG_LOG")
    ));

    // PREMISE: the sole real drainer is inside the exporter and never returns
    // (entered.send -> arm.recv is the synchronization witness).
    let (diagnostics, witness) = install_stopped_consumer("c2-seed");
    assert_eq!(
        qianqian_diagnostics::emit(Record::new("c2-seed", Level::Info)),
        Disposition::BetaQueued,
        "C2-PREMISE: the seed reaches the real drainer"
    );
    witness.wait_stopped("C2");

    // PRODUCER_PROGRESS: real native decode-read fault with no consumer.
    let receipt_rx = spawn_native_read_fault("c2", ReadPlan::FaultThenTerminal);
    let receipt = receipt_rx.recv_timeout(Duration::from_secs(10)).expect(
        "C2-PRODUCER-PROGRESS: real decode producer did not return with the consumer stopped",
    );
    evidence(&format!("C2-PRODUCER-RETURNED {receipt:?}"));
    assert_native_read_fault(&receipt, "C2");

    // The producer's stream drop (song_close) already completed inside the
    // worker before the receipt was sent — mandatory release does not wait on
    // the stopped consumer.
    assert_eq!(receipt.copied_out, 0, "C2-NO-CONSUMER");
    assert_eq!(receipt.outstanding, 2, "C2-BOUNDED");
    assert_eq!(receipt.accepted, 2, "C2-ALPHA-POOL-FILLED");
    assert!(
        receipt.rejected >= 2,
        "C2-DROP-ACCOUNTING: fault records dropped+counted, got {}",
        receipt.rejected
    );
    assert_eq!(
        receipt.attempted,
        receipt.accepted + receipt.rejected,
        "C2-ADMISSION-IDENTITY"
    );
    evidence(&format!(
        "C2-POOL attempted={} accepted={} rejected={} copied_out={} outstanding={}",
        receipt.attempted, receipt.accepted, receipt.rejected, receipt.copied_out, receipt.outstanding
    ));

    // TEARDOWN_OUTCOME: close (shutdown) must be bounded and honest. The
    // bounded receive is fail-closed: on the counterfactual (unbounded wait)
    // the assertion fails instead of the process being killed by a watchdog.
    let (report_tx, report_rx) = mpsc::channel();
    let shutdown_worker = std::thread::Builder::new()
        .name("q296-c2-shutdown".into())
        .spawn(move || {
            let report = diagnostics.shutdown(Duration::from_millis(50));
            report_tx.send(report).unwrap();
        })
        .expect("shutdown worker spawns");
    let report = report_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("C2-TEARDOWN: shutdown did not return inside its bounded deadline");
    let _ = shutdown_worker.join();
    evidence(&format!("C2-SHUTDOWN {report:?}"));

    assert!(
        report.abandoned && !report.drained,
        "C2-HONEST-ACCOUNTING: a stopped consumer is abandoned, never dressed as drained"
    );
    assert!(
        !(report.drained && report.abandoned),
        "C2-HONEST-ACCOUNTING: drained and abandoned are mutually exclusive"
    );
    assert_eq!(
        report.undelivered_at_exit, 3,
        "C2-LOSS-ACCOUNTING: 2 pool records + 1 drainer-held seed"
    );
    assert_eq!(report.counters.exported, 0, "C2-NOTHING-DELIVERED");
    assert_eq!(report.counters.beta_consumed, 1, "C2-DRAINER-HELD-SEED");
    assert_eq!(report.counters.copied_out, 0, "C2-NO-POOL-DRAIN");
    assert_eq!(
        report.counters.alpha_rejected, receipt.rejected,
        "C2-DROP-ACCOUNTING: every α rejection is counted"
    );
    evidence("C2-VERDICT=PASS");
}

// --- C3: CAPACITY_EXHAUSTION ---------------------------------------------

#[test]
#[ignore = "process-global diagnostics facility/av_log callback: run one cell per fresh process with --ignored --exact <cell>"]
fn c3_capacity_exhaustion_native_decode_read() {
    assert_artifact_identity();
    unsafe { std::env::set_var("QIANQIAN_FFMPEG_LOG", "1") };
    evidence(&format!(
        "FFMPEG_LOG_ENV={:?}",
        std::env::var("QIANQIAN_FFMPEG_LOG")
    ));

    // PREMISE: stopped consumer, so no record can leave the two-slot pool.
    let (diagnostics, witness) = install_stopped_consumer("c3-seed");
    assert_eq!(
        qianqian_diagnostics::emit(Record::new("c3-seed", Level::Info)),
        Disposition::BetaQueued
    );
    witness.wait_stopped("C3");

    // PRODUCER: the REAL producer floods the pool: the first fault emits the
    // decoder's two native av_log records, then one fault record; three more
    // fault records follow on the latched fatal. That is 6 real presentations
    // against a capacity of 2.
    let receipt_rx = spawn_native_read_fault("c3", ReadPlan::FaultThenRepeats(3));
    let receipt = receipt_rx.recv_timeout(Duration::from_secs(10)).expect(
        "C3-PRODUCER-PROGRESS: producer blocked on a full pool (bounded publish must not wait)",
    );
    evidence(&format!("C3-PRODUCER-RETURNED {receipt:?}"));
    assert_native_read_fault(&receipt, "C3");

    assert_eq!(
        receipt.attempted, 6,
        "C3-REAL-PRODUCER: 2 native av_log records + 4 native fault records"
    );
    assert_eq!(
        receipt.accepted, 2,
        "C3-CAPACITY-ORACLE: the α pool's bound is exactly 2"
    );
    assert_eq!(
        receipt.rejected, 4,
        "C3-DROP-ACCOUNTING: every presentation over capacity is rejected+counted"
    );
    assert_eq!(
        receipt.outstanding, 2,
        "C3-BOUNDED: outstanding cannot exceed capacity (no unbounded growth)"
    );
    assert_eq!(receipt.copied_out, 0, "C3-NO-CONSUMER");
    assert_eq!(
        receipt.attempted,
        receipt.accepted + receipt.rejected,
        "C3-ADMISSION-IDENTITY: attempted = accepted + rejected"
    );
    evidence(&format!(
        "C3-POOL attempted={} accepted={} rejected={} copied_out={} outstanding={}",
        receipt.attempted, receipt.accepted, receipt.rejected, receipt.copied_out, receipt.outstanding
    ));

    // TEARDOWN: honest abandoned close (the consumer is still stopped).
    let report = diagnostics.shutdown(Duration::from_millis(50));
    evidence(&format!("C3-SHUTDOWN {report:?}"));
    assert!(
        report.abandoned && !report.drained,
        "C3-TEARDOWN: bounded abandonment under a stopped consumer"
    );
    assert_eq!(report.undelivered_at_exit, 3, "C3-LOSS-ACCOUNTING");
    assert_eq!(report.counters.exported, 0, "C3-NOTHING-DELIVERED");
    evidence("C3-VERDICT=PASS");
}

// --- C4: WRITE_FAILURE ----------------------------------------------------

unsafe extern "C" {
    fn dup(fd: i32) -> i32;
    fn dup2(oldfd: i32, newfd: i32) -> i32;
    fn pipe(fds: *mut i32) -> i32;
    fn close(fd: i32) -> i32;
}

/// Redirect fd 2 to the write end of a pipe whose read end is closed: a real
/// OS write failure (`EPIPE`) inside this process. Returns the saved original
/// descriptor. The canary is asserted by the caller before any work starts.
fn break_stderr() -> i32 {
    let saved = unsafe { dup(2) };
    assert!(saved >= 0, "C4-PREMISE: could not save fd 2");
    let mut fds = [0i32; 2];
    assert_eq!(unsafe { pipe(fds.as_mut_ptr()) }, 0, "C4-PREMISE: pipe");
    assert_eq!(
        unsafe { dup2(fds[1], 2) },
        2,
        "C4-PREMISE: redirect fd 2 onto the pipe write end"
    );
    unsafe {
        close(fds[1]);
        close(fds[0]); // all readers gone: writes must now fail with EPIPE
    }
    saved
}

fn restore_stderr(saved: i32) {
    assert_eq!(
        unsafe { dup2(saved, 2) },
        2,
        "C4-TEARDOWN: restore fd 2"
    );
    unsafe { close(saved) };
}

#[test]
#[ignore = "process-global diagnostics facility/av_log callback: run one cell per fresh process with --ignored --exact <cell>"]
fn c4_write_failure_native_decode_read() {
    assert_artifact_identity();
    unsafe { std::env::set_var("QIANQIAN_FFMPEG_LOG", "1") };
    evidence(&format!(
        "FFMPEG_LOG_ENV={:?}",
        std::env::var("QIANQIAN_FFMPEG_LOG")
    ));

    // PREMISE: real export-write failure, self-witnessed inside the process
    // BEFORE any producer work (the closeout2 Linux portability lesson: close
    // the READ end, never just an arbitrary fd).
    let saved = break_stderr();
    let canary = write_stderr_line("Q296-C4-CANARY");
    if canary.is_ok() {
        // Restore first so this failure (and the panic message) is visible.
        restore_stderr(saved);
        panic!("C4-PREMISE: a real stderr write must fail inside this process, got {canary:?}");
    }
    evidence("C4-PREMISE: BROKEN-STDERR-CONFIRMED (real write Err)");

    let diagnostics = Diagnostics::install(
        Box::new(StderrExporter::default()),
        Config {
            beta_capacity: 4,
            poll_window: Duration::from_millis(1),
        },
    )
    .expect("this cell process installs exactly one facility");

    // PRODUCER_PROGRESS: real native decode-read fault; the drainer's export
    // returns Err, disables the exporter, and the producer is untouched.
    let receipt_rx = spawn_native_read_fault("c4", ReadPlan::FaultOnly);
    let receipt = match receipt_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(receipt) => receipt,
        Err(error) => {
            restore_stderr(saved);
            panic!("C4-PRODUCER-PROGRESS: producer did not return after the exporter write failure: {error:?}");
        }
    };
    evidence(&format!("C4-PRODUCER-RETURNED {receipt:?}"));

    // QUIESCENCE: all admitted records reached the drain boundary. The only
    // wait is this fail-closed bounded loop; it restores fd 2 before failing
    // so the oracle message is visible.
    let deadline = Instant::now() + Duration::from_secs(5);
    let counters = loop {
        let counters = diagnostics.counters();
        if counters.outstanding == 0 && counters.copied_out + counters.beta_consumed >= 2 {
            break counters;
        }
        if Instant::now() >= deadline {
            restore_stderr(saved);
            panic!("C4-QUIESCENCE WATCHDOG (fail-closed): {counters:?}");
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    evidence(&format!("C4-COUNTERS {counters:?}"));
    // All export attempts are finished; restoring fd 2 now only affects the
    // oracle's own reporting (the exporter is already disabled).
    restore_stderr(saved);

    assert_native_read_fault(&receipt, "C4");
    assert_eq!(
        counters.exported, 0,
        "C4-WRITE-FAILURE: a failed export is never counted as delivered"
    );
    assert_eq!(
        counters.self_diagnostics, 1,
        "C4-ONE-DISABLE: exactly one exporter-disable self-diagnostic"
    );
    assert_eq!(
        counters.export_failed,
        counters.copied_out + counters.beta_consumed,
        "C4-LOSS-ACCOUNTING: every admitted record is charged as an export loss"
    );
    assert!(
        counters.export_failed >= 2,
        "C4-LOSS-ACCOUNTING: at least the two native av_log records are charged as losses, got {}",
        counters.export_failed
    );
    assert_eq!(
        counters.copied_out + counters.beta_consumed,
        counters.exported + counters.export_failed,
        "C4-ACCOUNTING-ORACLE: drain identity at quiescence"
    );
    // The native fault record may have been rejected by the two-slot pool if
    // the drainer had not freed a slot; either way the admission boundary is
    // exact and the producer result is the same.
    assert_eq!(
        receipt.attempted,
        receipt.accepted + receipt.rejected,
        "C4-ADMISSION-IDENTITY"
    );

    let report = diagnostics.shutdown(Duration::from_secs(5));
    evidence(&format!("C4-SHUTDOWN {report:?}"));
    assert!(
        report.drained && !report.abandoned,
        "C4-TEARDOWN: failed writes return immediately, so the final sweep completes"
    );
    assert_eq!(report.undelivered_at_exit, 0, "C4-LOSS-ACCOUNTING");
    assert_eq!(
        report.counters.copied_out + report.counters.beta_consumed,
        report.counters.exported + report.counters.export_failed,
        "C4-ACCOUNTING-ORACLE: drain identity in the shutdown report"
    );

    assert!(
        write_stderr_line("Q296-C4-STDERR-RESTORED").is_ok(),
        "C4-TEARDOWN: the real stderr descriptor is restored"
    );
    evidence("C4-VERDICT=PASS");
}
