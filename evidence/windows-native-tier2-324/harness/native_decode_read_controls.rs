//! #324 decode-read × C1–C4 native runtime fault cells (WINDOWS x86_64).
//!
//! Linux lineage: `evidence/native-tier2-296/harness/native_decode_read_controls.rs`
//! (#323). This file is the Windows-native port: the producer path is
//! unchanged (real SongCore `song_read_pcm` over a corrupted committed FLAC
//! fixture → real native decode error → real diagnostic record), but every
//! OS-level premise is rebuilt from Windows primitives:
//!
//! * the sink is a real anonymous pipe installed with `SetStdHandle`
//!   (Rust's std resolves `STD_ERROR_HANDLE` per write — it deliberately does
//!   not cache, see std's `sys/stdio/windows.rs`), never `/dev/full`, never
//!   `dup2`/CRT fd 2 (which Rust's std does not consult on Windows);
//! * C1's block premise is real pipe backpressure: the pipe's read end is
//!   held and never read, and one 64 KiB raw `WriteFile` (which cannot
//!   complete into the 4 KiB anonymous-pipe queue) is witnessed as still in
//!   flight. The OS queue occupancy is read with `PeekNamedPipe`, not
//!   inferred from a sleep;
//! * C4's failure is a real `ERROR_NO_DATA`/`ERROR_BROKEN_PIPE` from an
//!   anonymous pipe whose read end is closed — self-witnessed by a canary
//!   write before any producer work, on the very handle the real
//!   `StderrExporter` later writes to.
//!
//! One fresh process per cell (the facility is process-global). The runner
//! selects one `#[ignore]`d test with `--ignored --exact <cell>`.
//!
//! Every seam is in this file; production source is untouched.

// This harness is Windows-only: it links kernel32 and drives Win32
// handles/pipes. A non-Windows host must compile it to zero tests rather than
// fail at link time (the crate is workspace-excluded and not CI-compiled).
#![cfg(windows)]

#[allow(dead_code)]
mod common;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use qianqian_audio_api::ports::{
    DecodeError, DecodeOutcome, DecodedPcmStream, PcmDecode, PcmDecodeCapability,
};
use qianqian_composition::{ComponentSpec, CompositionKernel, DesiredEntry, FiberState, Revision};
use qianqian_diagnostics::{
    AlphaRegistration, Config, Counters, Diagnostics, Disposition, ExportError, Exporter, Level,
    Record, StderrExporter, write_stderr_line,
};

// --- artifact identity ----------------------------------------------------

/// The verified `windows-x86_64` mingw-w64 artifact this campaign built and
/// gated (see evidence/windows-native-tier2-324/ARTIFACT.md). A drift here
/// means the cell is not running against the verified producer path; a
/// rebuild changes the archive hash (absolute build paths are embedded), so a
/// re-run against a rebuilt artifact updates this constant deliberately.
const EXPECTED_ARTIFACT_SHA256: &str = "90bde1ca3f9924842ebb06005219f746d5053499a03fc3afafebccd1d57ba6d2";

/// One bit inside a FLAC frame BODY; headers stay intact so the failure occurs
/// while decoding, not while parsing (same offset the Linux arm pinned).
const READ_FAULT_FLIP: (usize, u8) = (76_000, 0x01);

// --- Windows sink primitives ---------------------------------------------

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

    /// The process's current stderr handle (resolved fresh: std does the same).
    pub fn stderr_handle() -> Handle {
        unsafe { GetStdHandle(STD_ERROR_HANDLE) }
    }

    pub fn stderr_file_type() -> u32 {
        unsafe { GetFileType(stderr_handle()) }
    }

    pub struct Sink {
        pub original: Handle,
        /// Kept open (and never read) for the blocked sink; closed for the
        /// broken sink. Either way this is the OS fact the cell relies on.
        pub read: Handle,
        pub write: Handle,
        #[allow(dead_code)]
        pub broken: bool,
        pub avail_at_install: u32,
    }

    /// Replace the process stderr with a fresh anonymous pipe via
    /// `SetStdHandle`. `broken` closes the read end immediately (a real
    /// failing sink); otherwise the read end is held and never read (a real
    /// backpressured sink).
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
        let mut avail_at_install = 0u32;
        if broken {
            unsafe { CloseHandle(r) };
        } else {
            avail_at_install = peek_handle(r).expect("PREMISE: PeekNamedPipe on the held read end").0;
        }
        Sink {
            original,
            read: r,
            write: w,
            broken,
            avail_at_install,
        }
    }

    pub fn restore(sink: &Sink) {
        let ok = unsafe { SetStdHandle(STD_ERROR_HANDLE, sink.original) };
        assert_eq!(ok, 1, "TEARDOWN: restore the original stderr handle");
    }

    fn peek_handle(read: Handle) -> Option<(u32, u32)> {
        let mut avail = 0u32;
        let mut left = 0u32;
        let ok = unsafe {
            PeekNamedPipe(
                read,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut avail,
                &mut left,
            )
        };
        if ok == 0 { None } else { Some((avail, left)) }
    }

    pub fn peek(sink: &Sink) -> Option<(u32, u32)> {
        peek_handle(sink.read)
    }

    /// One oversized raw `WriteFile` on the pipe's write end (deliberately
    /// NOT `std::io::stderr`, so the std lock is not involved: this is pure
    /// OS pipe backpressure). `issued` is set immediately before the call,
    /// `done` immediately after it returns.
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

// --- evidence channel (stdout, never the controlled stderr) ---------------

fn line(text: &str) {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{text}").expect("stdout receipt");
    out.flush().expect("stdout receipt flush");
}

/// Assertions must be visible even when stderr is blocked or broken.
fn stdout_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "ASSERTION {info}");
        let _ = out.flush();
    }));
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
    let bytes = std::fs::read(&path).expect("artifact bytes");
    let mut hasher = common::Sha256::new();
    hasher.update(&bytes);
    let actual = hasher.hex();
    line(&format!("ARTIFACT_PATH={}", path.display()));
    line(&format!("ARTIFACT_SHA256={actual}"));
    line(&format!(
        "ARTIFACT_BUILD_TIME_SHA256_RAW={}",
        qianqian_songcore_sys::QN_SONGCORE_ARTIFACT_SHA256
    ));
    // Windows build note (reported to #172, NOT fixed by this verification
    // campaign): `build.rs` shells out to `sha256sum`, and GNU coreutils
    // escapes a filename containing backslashes with a leading `\`. The
    // embedded value is the same digest carrying that one escape byte, so it
    // is normalized here to keep the "no stale link" check exact without
    // touching product/build code.
    let embedded = qianqian_songcore_sys::QN_SONGCORE_ARTIFACT_SHA256
        .trim_start_matches('\\')
        .trim();
    line(&format!("ARTIFACT_BUILD_TIME_SHA256={embedded}"));
    assert_eq!(
        actual, EXPECTED_ARTIFACT_SHA256,
        "PREMISE: this cell must run against the verified windows-x86_64 artifact"
    );
    assert_eq!(
        actual, embedded,
        "PREMISE: the test binary must be linked against the artifact on disk (no stale link)"
    );
}

// --- the real production decode path -------------------------------------

fn activate_decode_plugin_and_capture() -> std::rc::Rc<dyn PcmDecode> {
    thread_local! {
        static CAPTURED: std::cell::RefCell<Option<std::rc::Rc<dyn PcmDecode>>> =
            const { std::cell::RefCell::new(None) };
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
        "q324-decode-read-{}-{name}.flac",
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

/// Run the real producer on its own thread (the production decode worker is
/// its own thread). The receipt is sent only after the producer returned and
/// the stream was dropped (`song_close` completed), so receiving it IS the
/// producer-progress witness. The bounded receive in the cell is a
/// fail-closed watchdog, not the property observation.
fn spawn_native_read_fault(name: &'static str, plan: ReadPlan) -> mpsc::Receiver<ProducerReceipt> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name(format!("q324-producer-{name}"))
        .spawn(move || {
            // Production shape: the decode worker is an isolation-obligated
            // class-α producer.
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

// --- exporter seams (test-only) -------------------------------------------
//
// C1/C4 delegate every real write to `StderrExporter` (the process stderr
// handle). C2/C3 deliberately use a test-owned exporter that parks the sole
// drainer inside `export`; those two cells claim a stopped consumer and pool
// accounting, NOT a real sink write.

/// Delegates the actual write to the REAL `StderrExporter`, signalling entry
/// first so the witness proves the drainer reached the real export call.
struct WitnessedStderr {
    entered: Option<mpsc::Sender<()>>,
    inner: StderrExporter,
}

impl Exporter for WitnessedStderr {
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

/// The stopped-consumer seam: the drainer enters the REAL export call and
/// then waits on a test gate that is never released during the property.
struct HeldExporter {
    target: &'static str,
    entered: mpsc::Sender<()>,
    gate: Arc<(Mutex<bool>, Condvar)>,
}

impl Exporter for HeldExporter {
    fn name(&self) -> &'static str {
        "q324-held"
    }

    fn export(&mut self, record: &Record) -> Result<(), ExportError> {
        if record.site == self.target {
            self.entered.send(()).expect("witness receiver alive");
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
        line(&format!("{cell}-PREMISE CONSUMER_HELD=1"));
    }
}

fn install_stopped_consumer(target: &'static str) -> (Diagnostics, InstallWitness) {
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (entered_tx, entered_rx) = mpsc::channel();
    let diagnostics = Diagnostics::install(
        Box::new(HeldExporter {
            target,
            entered: entered_tx,
            gate: Arc::clone(&gate),
        }),
        Config {
            beta_capacity: 4,
            poll_window: Duration::from_millis(1),
        },
    )
    .expect("this cell process installs exactly one facility");
    (diagnostics, InstallWitness { entered_rx, gate })
}

// --- C1: BLOCKED_SINK -----------------------------------------------------

#[test]
#[ignore = "process-global diagnostics facility: run one cell per fresh process with --ignored --exact <cell>"]
fn c1_blocked_sink_native_decode_read() {
    stdout_panic_hook();
    assert_artifact_identity();
    line(&format!("C1-ENV=windows-x86_64 pid={}", std::process::id()));

    // PREMISE: the process stderr is a real anonymous pipe whose read end this
    // process holds and never reads. A 64 KiB raw WriteFile into a no-reader
    // anonymous pipe parks (a 4 KiB write completes) — measured on this host,
    // see ENVIRONMENT.md. The parked write is then corroborated by an
    // INDEPENDENT 1-byte write that also cannot complete: at that point the
    // pipe's write quota is exhausted for every writer, not just for the
    // harness's flood.
    let sink = win::replace_stderr(false);
    line(&format!(
        "C1-PREMISE stderr_file_type={} pipe=1 read_end_held_not_read=1",
        win::stderr_file_type()
    ));
    let (issued, done) = win::spawn_raw_flood(sink.write, 64 * 1024);
    wait_until(
        || issued.load(Ordering::SeqCst),
        "the raw flood write was issued",
    );
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        !done.load(Ordering::SeqCst),
        "C1-PREMISE: the 64 KiB flood completed — the sink is not backpressured"
    );
    let (avail, _) = win::peek(&sink).expect("C1-PREMISE: PeekNamedPipe on the held read end");
    assert!(
        avail > 0,
        "C1-PREMISE: the pipe queue must be non-empty after the parked flood"
    );
    let (probe_issued, probe_done) = win::spawn_raw_flood(sink.write, 1);
    wait_until(
        || probe_issued.load(Ordering::SeqCst),
        "the independent 1-byte probe write was issued",
    );
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        !probe_done.load(Ordering::SeqCst),
        "C1-PREMISE: a fresh 1-byte write completed — the pipe still accepts data, so the sink \
         is not blocked for the drainer either"
    );
    line(&format!(
        "C1-PREMISE SINK_HOLD_ENTERED queued_bytes={avail} flood_bytes=65536 \
         flood_outstanding=1 second_write_outstanding=1"
    ));

    // The opt-in bridge is installed by the real plugin activation, so the
    // native decoder's own av_log errors become class-α records.
    unsafe { std::env::set_var("QIANQIAN_FFMPEG_LOG", "1") };

    let (entered_tx, entered_rx) = mpsc::channel();
    let diagnostics = Diagnostics::install(
        Box::new(WitnessedStderr {
            entered: Some(entered_tx),
            inner: StderrExporter::default(),
        }),
        Config {
            beta_capacity: 4,
            poll_window: Duration::from_millis(1),
        },
    )
    .expect("this cell process installs exactly one facility");

    // Seed one β record: the sole real drainer pulls it and blocks inside the
    // real StderrExporter write on the full pipe.
    assert_eq!(
        qianqian_diagnostics::emit(Record::new("c1-seed", Level::Info)),
        Disposition::BetaQueued,
        "C1-PREMISE: the seed enters the real β channel"
    );
    entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("C1-PREMISE: the drainer reached the real exporter");
    let seeded = wait_for_counters(
        &diagnostics,
        |c| c.beta_consumed >= 1 && c.exported == 0,
        "the drainer inside the blocked real export",
    );
    line(&format!(
        "C1-PREMISE EXPORTER_ENTERED beta_consumed={} exported={}",
        seeded.beta_consumed, seeded.exported
    ));

    // PRODUCER_PROGRESS: real native decode-read fault while the sink blocks.
    let receipt_rx = spawn_native_read_fault("c1", ReadPlan::FaultThenTerminal);
    let receipt = receipt_rx.recv_timeout(Duration::from_secs(10)).expect(
        "C1-PRODUCER-PROGRESS: real decode producer did not return while the real sink was blocked \
         (counterfactual: a synchronous producer sink write never returns)",
    );
    line(&format!("C1-PRODUCER-RETURNED {receipt:?}"));

    // RELIABLE_RESULT / TERMINAL_OUTCOME: native DecodeError, fatal persists.
    assert_native_read_fault(&receipt, "C1");

    // SINK_STILL_BLOCKED: observed AFTER the producer returned. The drainer
    // entered export() (no error code) and its write neither completed nor
    // failed while the producer ran to completion: it is parked in WriteFile,
    // exactly like the two independent raw writers above.
    let (avail_after, _) = win::peek(&sink).expect("C1: peek the held read end");
    let counters_now = diagnostics.counters();
    let still_blocked = !done.load(Ordering::SeqCst)
        && !probe_done.load(Ordering::SeqCst)
        && avail_after > 0
        && counters_now.exported == 0
        && counters_now.export_failed == 0;
    assert!(
        still_blocked,
        "C1-ISOLATION: the last observation must be taken while the sink is still blocked \
         (flood_done={} probe_done={} queued={avail_after} exported={} export_failed={})",
        done.load(Ordering::SeqCst),
        probe_done.load(Ordering::SeqCst),
        counters_now.exported,
        counters_now.export_failed
    );
    line(&format!(
        "C1-PRODUCER_COMPLETED_WHILE_HELD queued_bytes={avail_after} exported={} \
         export_failed={} SINK_STILL_BLOCKED=1",
        counters_now.exported, counters_now.export_failed
    ));

    // LOSS_ACCOUNTING: the two-slot pool accepted the first two presentations;
    // later records were rejected+counted; nothing was copied out.
    assert_eq!(
        receipt.accepted, 2,
        "C1-CAPACITY: the two-slot α pool accepted exactly two records while the sink was blocked"
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
        receipt.rejected >= 1,
        "C1-DROP-ACCOUNTING: later records were dropped+counted while blocked, got {}",
        receipt.rejected
    );
    assert_eq!(
        receipt.attempted,
        receipt.accepted + receipt.rejected,
        "C1-ADMISSION-IDENTITY: attempted = accepted + rejected"
    );
    line(&format!(
        "C1-POOL attempted={} accepted={} rejected={} copied_out={} outstanding={}",
        receipt.attempted, receipt.accepted, receipt.rejected, receipt.copied_out, receipt.outstanding
    ));

    // TEARDOWN_OUTCOME: bounded abandonment, never a fake drained.
    let report = diagnostics.shutdown(Duration::from_millis(50));
    line(&format!("C1-SHUTDOWN {report:?}"));
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
    line("C1-VERDICT=PASS");
}

// --- C2: STOPPED_CONSUMER -------------------------------------------------

#[test]
#[ignore = "process-global diagnostics facility: run one cell per fresh process with --ignored --exact <cell>"]
fn c2_stopped_consumer_native_decode_read() {
    stdout_panic_hook();
    assert_artifact_identity();
    line(&format!("C2-ENV=windows-x86_64 pid={}", std::process::id()));
    unsafe { std::env::set_var("QIANQIAN_FFMPEG_LOG", "1") };

    // PREMISE: the sole real drainer is inside the exporter and never returns.
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
    line(&format!("C2-PRODUCER-RETURNED {receipt:?}"));
    assert_native_read_fault(&receipt, "C2");

    assert_eq!(receipt.copied_out, 0, "C2-NO-CONSUMER");
    assert_eq!(receipt.outstanding, 2, "C2-BOUNDED");
    assert_eq!(receipt.accepted, 2, "C2-ALPHA-POOL-FILLED");
    assert!(
        receipt.rejected >= 1,
        "C2-DROP-ACCOUNTING: fault records dropped+counted, got {}",
        receipt.rejected
    );
    assert_eq!(
        receipt.attempted,
        receipt.accepted + receipt.rejected,
        "C2-ADMISSION-IDENTITY"
    );
    line(&format!(
        "C2-POOL attempted={} accepted={} rejected={} copied_out={} outstanding={}",
        receipt.attempted, receipt.accepted, receipt.rejected, receipt.copied_out, receipt.outstanding
    ));

    // TEARDOWN_OUTCOME: bounded and honest.
    let (report_tx, report_rx) = mpsc::channel();
    let shutdown_worker = std::thread::Builder::new()
        .name("q324-c2-shutdown".into())
        .spawn(move || {
            let report = diagnostics.shutdown(Duration::from_millis(50));
            report_tx.send(report).expect("cell observes the shutdown report");
        })
        .expect("shutdown worker spawns");
    let report = report_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("C2-TEARDOWN: shutdown did not return inside its bounded deadline");
    let _ = shutdown_worker.join();
    line(&format!("C2-SHUTDOWN {report:?}"));

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
    line("C2-VERDICT=PASS");
}

// --- C3: CAPACITY_EXHAUSTION ---------------------------------------------

#[test]
#[ignore = "process-global diagnostics facility: run one cell per fresh process with --ignored --exact <cell>"]
fn c3_capacity_exhaustion_native_decode_read() {
    stdout_panic_hook();
    assert_artifact_identity();
    line(&format!("C3-ENV=windows-x86_64 pid={}", std::process::id()));
    unsafe { std::env::set_var("QIANQIAN_FFMPEG_LOG", "1") };

    // PREMISE: stopped consumer, so no record can leave the two-slot pool.
    let (diagnostics, witness) = install_stopped_consumer("c3-seed");
    assert_eq!(
        qianqian_diagnostics::emit(Record::new("c3-seed", Level::Info)),
        Disposition::BetaQueued
    );
    witness.wait_stopped("C3");

    // PRODUCER: the REAL producer floods the pool: the native av_log records,
    // then one fault record, then three more on the latched fatal. Against a
    // capacity of 2 every presentation past the first two is rejected.
    let receipt_rx = spawn_native_read_fault("c3", ReadPlan::FaultThenRepeats(3));
    let receipt = receipt_rx.recv_timeout(Duration::from_secs(10)).expect(
        "C3-PRODUCER-PROGRESS: producer blocked on a full pool (bounded publish must not wait)",
    );
    line(&format!("C3-PRODUCER-RETURNED {receipt:?}"));
    assert_native_read_fault(&receipt, "C3");

    assert!(
        receipt.attempted >= 4,
        "C3-REAL-PRODUCER: the native fault path must attempt at least the four fault records, got {}",
        receipt.attempted
    );
    assert_eq!(
        receipt.accepted, 2,
        "C3-CAPACITY-ORACLE: the α pool's bound is exactly 2"
    );
    assert_eq!(
        receipt.rejected,
        receipt.attempted - receipt.accepted,
        "C3-DROP-ACCOUNTING: every presentation over capacity is rejected+counted"
    );
    assert!(
        receipt.rejected >= 2,
        "C3-DROP-ACCOUNTING: over-capacity presentations must be rejected, got {}",
        receipt.rejected
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
    line(&format!(
        "C3-POOL attempted={} accepted={} rejected={} copied_out={} outstanding={}",
        receipt.attempted, receipt.accepted, receipt.rejected, receipt.copied_out, receipt.outstanding
    ));

    // TEARDOWN: honest abandoned close (the consumer is still stopped).
    let report = diagnostics.shutdown(Duration::from_millis(50));
    line(&format!("C3-SHUTDOWN {report:?}"));
    assert!(
        report.abandoned && !report.drained,
        "C3-TEARDOWN: bounded abandonment under a stopped consumer"
    );
    assert_eq!(report.undelivered_at_exit, 3, "C3-LOSS-ACCOUNTING");
    assert_eq!(report.counters.exported, 0, "C3-NOTHING-DELIVERED");
    line("C3-VERDICT=PASS");
}

// --- C4: WRITE_FAILURE ----------------------------------------------------

#[test]
#[ignore = "process-global diagnostics facility: run one cell per fresh process with --ignored --exact <cell>"]
fn c4_write_failure_native_decode_read() {
    stdout_panic_hook();
    assert_artifact_identity();
    line(&format!("C4-ENV=windows-x86_64 pid={}", std::process::id()));
    unsafe { std::env::set_var("QIANQIAN_FFMPEG_LOG", "1") };

    // PREMISE: a real failing sink. stderr is an anonymous pipe whose READ end
    // is closed; the canary proves the real writer fails inside this process
    // BEFORE any producer work, and the exporter later writes to the same
    // process stderr handle.
    let sink = win::replace_stderr(true);
    let error = write_stderr_line("Q324-C4-CANARY")
        .expect_err("C4-PREMISE: a real stderr write must fail inside this child");
    let code = error.raw_os_error().unwrap_or(-1);
    assert!(
        code == win::ERROR_NO_DATA || code == win::ERROR_BROKEN_PIPE,
        "C4-PREMISE: expected ERROR_NO_DATA (232) or ERROR_BROKEN_PIPE (109), got {code}"
    );
    line(&format!(
        "C4-PREMISE BROKEN-STDERR-CONFIRMED raw_os_error={code}"
    ));

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
            win::restore(&sink);
            panic!("C4-PRODUCER-PROGRESS: producer did not return after the exporter write failure: {error:?}");
        }
    };
    line(&format!("C4-PRODUCER-RETURNED {receipt:?}"));

    // QUIESCENCE: all admitted records reached the drain boundary.
    let deadline = Instant::now() + Duration::from_secs(5);
    let counters = loop {
        let counters = diagnostics.counters();
        if counters.outstanding == 0 && counters.copied_out + counters.beta_consumed >= 2 {
            break counters;
        }
        if Instant::now() >= deadline {
            win::restore(&sink);
            panic!("C4-QUIESCENCE WATCHDOG (fail-closed): {counters:?}");
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    line(&format!("C4-COUNTERS {counters:?}"));
    // All export attempts are finished (the exporter is disabled); restoring
    // the original handle only affects the oracle's own reporting.
    win::restore(&sink);

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
        counters.export_failed >= 1,
        "C4-LOSS-ACCOUNTING: at least one real write failure is charged, got {}",
        counters.export_failed
    );
    assert_eq!(
        counters.copied_out + counters.beta_consumed,
        counters.exported + counters.export_failed,
        "C4-ACCOUNTING-ORACLE: drain identity at quiescence"
    );
    assert_eq!(
        receipt.attempted,
        receipt.accepted + receipt.rejected,
        "C4-ADMISSION-IDENTITY"
    );

    let report = diagnostics.shutdown(Duration::from_secs(5));
    line(&format!("C4-SHUTDOWN {report:?}"));
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
    line("C4-VERDICT=PASS");
}
