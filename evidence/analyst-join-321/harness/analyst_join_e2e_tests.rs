//! VERSIONED EVIDENCE HARNESS (#321) — NOT compiled by the product build.
//!
//! `replay.sh` copies this file into a SCRATCH worktree and applies the
//! `overlay/` patches there. The product tree never contains this module or the
//! panic injection.
//!
//! End-to-end causal witness for the finding "observation analyst panic x
//! blocked stderr -> the session exit funnel's `analyst.join()` never returns
//! -> the session teardown's `worker.join()` and the authoritative K0
//! `Discharge` never complete".
//!
//! Every mechanism under test is the production one:
//!   * the production panic-hook installer (`machine_input::install_panic_hook_posture`),
//!   * the REAL Playback Session composition (`playback_session_spec_with_establishment`),
//!   * the REAL decode worker thread (`qianqian-decode`),
//!   * the REAL observation analyst thread (`qianqian-observation`),
//!   * the REAL exit funnel and K0 relation unwind (`QianqianApp::dispose`).
//!
//! Test doubles stand only at the Decode/Output *ports* seams, exactly as
//! `tests/session_activation.rs` and `machine_input::tests` already do.
//!
//! The analyst has no product-reachable panic (no input can make `analyze`
//! panic), so the panic is injected by ONE env-gated line INSIDE the real
//! protected region in `crates/qianqian-playback/src/observation.rs` (see
//! `overlay/000-harness.patch`). That patch is evidence-only; the panic, the
//! hook, the join and the teardown are real. The negative control
//! (`overlay/001-negative-control-registration-after-panic.patch`) moves the
//! class-alpha registration after the injection point, which must restore the
//! wedge.
//!
//! Child scenarios are selected by `QIANQIAN_ANALYST_JOIN_SCENARIO`; the
//! parent re-execs this test binary with `--exact` so the process-global hook
//! posture is clean, following the existing DG-4b/E-8 subprocess pattern.

#![allow(clippy::print_stdout)]

use crate::assembly::establish_specs_for_test;
use crate::assembly::mechanisms::{OutputBehavior, SourceBehavior, TestDecode, TestOutput};
use crate::machine_input::install_panic_hook_posture;
use qianqian_app::{DisposeOutcome, QianqianApp};
use qianqian_audio_api::ports::{AudioOutputCapability, PcmDecodeCapability};
use qianqian_composition::{ComponentSpec, Discharge};
use qianqian_playback::{
    AudioProcessingConfig, EstablishmentResult, PlaybackSessionHandle,
    playback_session_spec_with_establishment,
};
use std::cell::Cell;
use std::io::{Read, Write};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const SCENARIO: &str = "QIANQIAN_ANALYST_JOIN_SCENARIO";
const CHILD_TEST: &str = "analyst_join_e2e_tests::analyst_join_child";

/// Write one marker line to the REAL process stdout, unbuffered and
/// error-ignoring: the blocked-stderr scenarios still need a healthy witness
/// stream, and a failing witness write must never panic in a panic hook.
fn out(line: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(line.as_bytes());
    let _ = stdout.write_all(b"\n");
    let _ = stdout.flush();
}

/// Capture the current (std default) hook and install a transparent marker
/// hook around it. `install_panic_hook_posture` then captures THIS hook as its
/// `previous`, which is exactly the host/tracer shape the posture admits on
/// non-critical threads. The marker proves the previous hook was ENTERED and
/// whether it RETURNED, without changing what the default hook does.
fn install_marker_previous() {
    let real = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let analyst = std::thread::current().name() == Some("qianqian-observation");
        if analyst {
            out("PREVIOUS-HOOK-ENTERED-ANALYST");
        } else {
            out("PREVIOUS-HOOK-ENTERED-OTHER");
        }
        real(info);
        if analyst {
            out("PREVIOUS-HOOK-RETURNED-ANALYST");
        } else {
            out("PREVIOUS-HOOK-RETURNED-OTHER");
        }
    }));
}

/// Close fd 2. Writes then fail fast with EBADF; the std default hook ignores
/// its own write error (probed independently), so the chained hook returns.
fn close_stderr() {
    unsafe extern "C" {
        fn close(fd: i32) -> i32;
    }
    // SAFETY: fd 2 is the process stderr; the scenario deliberately breaks it.
    let _ = unsafe { close(2) };
}

/// Fill the process stderr from a helper thread until it BLOCKS inside a write,
/// returning only once that stall is witnessed. Same technique as
/// `machine_input::tests::block_process_stderr` (DG-4b): a chunk completed, a
/// chunk is in flight, no chunk completed for a whole window, no write failed.
fn block_process_stderr() -> usize {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let completed = std::sync::Arc::new(AtomicUsize::new(0));
    let in_flight = std::sync::Arc::new(AtomicBool::new(false));
    let errored = std::sync::Arc::new(AtomicBool::new(false));
    let (entered, ready) = mpsc::channel();
    let (c, f, e) = (
        std::sync::Arc::clone(&completed),
        std::sync::Arc::clone(&in_flight),
        std::sync::Arc::clone(&errored),
    );
    std::thread::spawn(move || {
        let chunk = vec![b'.'; 64 * 1024];
        entered.send(()).unwrap();
        loop {
            f.store(true, Ordering::SeqCst);
            if std::io::stderr().write_all(&chunk).is_err() {
                e.store(true, Ordering::SeqCst);
                return;
            }
            f.store(false, Ordering::SeqCst);
            c.fetch_add(1, Ordering::SeqCst);
        }
    });
    ready.recv().expect("the filler started");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let before = completed.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(250));
        let after = completed.load(Ordering::SeqCst);
        assert!(
            !errored.load(Ordering::SeqCst),
            "stderr was not writable: nothing to block"
        );
        if before > 0 && before == after && in_flight.load(Ordering::SeqCst) {
            return before;
        }
        assert!(
            Instant::now() < deadline,
            "stderr never filled (chunks completed: {after})"
        );
    }
}

fn build_runtime(
    handle: PlaybackSessionHandle,
) -> (QianqianApp, EstablishmentResult, Rc<Cell<bool>>) {
    let disposed = Rc::new(Cell::new(false));
    let marker = disposed.clone();
    let decode = ComponentSpec::new("decode")
        .provides::<PcmDecodeCapability>()
        .on_activate(|ctx| {
            ctx.provide::<PcmDecodeCapability>(Rc::new(TestDecode::new(
                SourceBehavior::EofAfter(96_000),
            )))
            .unwrap();
            Ok(())
        });
    let output = ComponentSpec::new("output")
        .provides::<AudioOutputCapability>()
        .on_activate(move |ctx| {
            let marker = marker.clone();
            ctx.register_effect(move || {
                marker.set(true);
                Discharge::Discharged
            });
            ctx.provide::<AudioOutputCapability>(Rc::new(TestOutput::new(
                OutputBehavior::Consume,
            )))
            .unwrap();
            Ok(())
        });
    let (session, attempt) = playback_session_spec_with_establishment(
        "test://analyst-join-e2e".into(),
        handle.clone(),
        AudioProcessingConfig::BYPASS,
    );
    let assembled = establish_specs_for_test(decode, output, session, attempt, handle);
    let establishment = assembled.start.establishment;
    let runtime = assembled.start.runtime;
    (runtime, establishment, disposed)
}

/// Child entry: inert without the scenario variable, so a normal test run is
/// untouched. Explicit `process::exit` avoids harness output on the streams the
/// scenarios manipulate.
#[test]
fn analyst_join_child() {
    let Ok(scenario) = std::env::var(SCENARIO) else {
        return;
    };
    match scenario.as_str() {
        "healthy" => {
            install_marker_previous();
            install_panic_hook_posture();
            out("HOOK-INSTALLED healthy");
        }
        "broken-stderr-panic" => {
            close_stderr();
            install_marker_previous();
            install_panic_hook_posture();
            out("HOOK-INSTALLED broken-stderr-panic");
        }
        "blocked-stderr-panic" => {
            let chunks = block_process_stderr();
            install_marker_previous();
            install_panic_hook_posture();
            out(&format!("HOOK-INSTALLED blocked-stderr-panic chunks={chunks}"));
        }
        "blocked-sink-control" => {
            // Same saturated, unread sink as the subject, but NO panic: the
            // sink alone must not wedge anything.
            let chunks = block_process_stderr();
            install_marker_previous();
            install_panic_hook_posture();
            out(&format!("HOOK-INSTALLED blocked-sink-control chunks={chunks}"));
        }
        other => panic!("unknown analyst-join scenario {other:?}"),
    }

    let handle = PlaybackSessionHandle::new();
    let (mut runtime, establishment, disposed) = build_runtime(handle.clone());
    out(&format!("ESTABLISHMENT={establishment:?}"));
    assert_eq!(
        establishment,
        EstablishmentResult::Established,
        "the real session must activate"
    );

    // Independent read-side witness: the terminal Fact is published by
    // `worker_exited` BEFORE the funnel reaches close/join; `is_closed()` true
    // proves the funnel returned from `observation.close()` and is therefore
    // at the analyst join.
    let (terminal_tx, terminal_rx) = mpsc::channel();
    let witness_handle = handle.clone();
    std::thread::spawn(move || {
        let terminal = witness_handle.wait_terminal();
        out(&format!("TERMINAL={terminal:?}"));
        let deadline = Instant::now() + Duration::from_secs(5);
        let closed = loop {
            if witness_handle
                .observation_reader()
                .map(|reader| reader.is_closed())
                .unwrap_or(true)
            {
                break true;
            }
            if Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        out(&format!("IS-CLOSED={closed}"));
        let _ = terminal_tx.send(());
    });

    terminal_rx
        .recv_timeout(Duration::from_secs(20))
        .expect("terminal Fact + close observation within the bound");
    out("DISPOSE-ENTERED");
    let outcome: DisposeOutcome = runtime.dispose();
    out(&format!(
        "DISPOSE-RETURNED verdict={:?} quiet={}",
        outcome.verdict, outcome.snapshot.quiet
    ));
    out(&format!("OUTPUT-EFFECT-DISCHARGED={}", disposed.get()));
    out(&format!(
        "CRITICAL-PANICS={}",
        qianqian_diagnostics::critical_panics()
    ));
    out(&format!(
        "ANALYST-THREAD-GONE={}",
        named_thread_gone_within("qianqian-observ", Duration::from_secs(2))
    ));
    std::process::exit(0);
}

/// Linux `/proc/self/task` diagnostic (name only; no timing contract).
fn named_thread_gone_within(name: &str, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        let mut found = false;
        if let Ok(entries) = std::fs::read_dir("/proc/self/task") {
            for entry in entries.flatten() {
                let comm = entry.path().join("comm");
                if let Ok(text) = std::fs::read_to_string(comm) {
                    // /proc comm is truncated to TASK_COMM_LEN - 1 bytes.
                    if text.trim().starts_with(name) {
                        found = true;
                        break;
                    }
                }
            }
        }
        if !found {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

// --- parent-side observation ------------------------------------------------

struct ChildRun {
    child: std::process::Child,
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
    stderr: Option<std::process::ChildStderr>,
}

impl ChildRun {
    fn spawn(scenario: &str, block_stderr: bool, inject_panic: bool) -> ChildRun {
        let mut command = std::process::Command::new(std::env::current_exe().expect("test binary"));
        command
            .args(["--exact", CHILD_TEST, "--nocapture"])
            .env(SCENARIO, scenario)
            .stdin(std::process::Stdio::null());
        if inject_panic {
            command.env("QIANQIAN_ANALYST_JOIN_INJECT_PANIC", "1");
        }
        command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = command.spawn().expect("spawn the analyst-join child");
        let stdout = child.stdout.take().expect("child stdout");
        let stderr = child.stderr.take().expect("child stderr");
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        if !block_stderr {
            // Drain (never block) the healthy/broken stderr pipe.
            let mut stderr_drain = stderr;
            std::thread::spawn(move || {
                let mut sink = Vec::new();
                let _ = stderr_drain.read_to_end(&mut sink);
            });
            ChildRun {
                child,
                lines,
                seen: Vec::new(),
                stderr: None,
            }
        } else {
            ChildRun {
                child,
                lines,
                seen: Vec::new(),
                stderr: Some(stderr),
            }
        }
    }

    /// Consume lines until one starts with `prefix` (bounded).
    fn wait_for(&mut self, prefix: &str, limit: Duration) -> String {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(line) = self.seen.iter().find(|l| l.starts_with(prefix)) {
                return line.clone();
            }
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(
                !left.is_zero(),
                "child never printed {prefix:?}; seen: {:?}",
                self.seen
            );
            match self.lines.recv_timeout(left) {
                Ok(line) => self.seen.push(line),
                Err(_) => {
                    panic!(
                        "child never printed {prefix:?} within the bound; seen: {:?}",
                        self.seen
                    );
                }
            }
        }
    }

    /// Consume lines until one starts with `prefix` (bounded). Returns None on
    /// timeout instead of panicking: the branch selector must be able to say
    /// "the verdict did not arrive".
    fn try_wait_for(&mut self, prefix: &str, limit: Duration) -> Option<String> {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(line) = self.seen.iter().find(|l| l.starts_with(prefix)) {
                return Some(line.clone());
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            match self.lines.recv_timeout(left) {
                Ok(line) => self.seen.push(line),
                Err(_) => return None,
            }
        }
    }

    /// Drain whatever the child printed for `window`, without waiting for more.
    fn drain_for(&mut self, window: Duration) {
        let deadline = Instant::now() + window;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            match self.lines.recv_timeout(left) {
                Ok(line) => self.seen.push(line),
                Err(_) => return,
            }
        }
    }

    fn saw(&self, prefix: &str) -> bool {
        self.seen.iter().any(|l| l.starts_with(prefix))
    }

    fn kill_and_wait(&mut self) -> std::process::ExitStatus {
        let _ = self.child.kill();
        self.child.wait().expect("reap the child")
    }
}

/// Unread bytes on the parent's pipe read end (FIONREAD), a direct occupancy
/// witness that the child's stderr sink is saturated and unread.
fn unread_bytes(stderr: &std::process::ChildStderr) -> i32 {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn ioctl(fd: i32, request: std::ffi::c_ulong, ...) -> i32;
    }
    const FIONREAD: std::ffi::c_ulong = 0x541B;
    let mut value: i32 = 0;
    // SAFETY: FIONREAD writes one c_int into the provided pointer.
    let rc = unsafe { ioctl(stderr.as_raw_fd(), FIONREAD, &mut value as *mut i32) };
    assert_eq!(rc, 0, "FIONREAD failed");
    value
}

#[derive(Debug)]
struct TaskSnap {
    tid: String,
    comm: String,
    state: String,
    wchan: String,
}

fn task_snapshot(pid: u32) -> Vec<TaskSnap> {
    let mut tasks = Vec::new();
    let Ok(entries) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
        return tasks;
    };
    for entry in entries.flatten() {
        let tid = entry.file_name().to_string_lossy().into_owned();
        let comm = std::fs::read_to_string(entry.path().join("comm"))
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        let stat = std::fs::read_to_string(entry.path().join("stat")).unwrap_or_default();
        // pid (comm) state ... — comm may contain spaces/parens; state is the
        // first field after the final ')'.
        let state = stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .unwrap_or("?")
            .to_owned();
        let wchan = std::fs::read_to_string(entry.path().join("wchan"))
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        tasks.push(TaskSnap {
            tid,
            comm,
            state,
            wchan,
        });
    }
    tasks
}

fn print_tasks(label: &str, pid: u32) {
    for task in task_snapshot(pid) {
        out(&format!(
            "PROC {label} tid={} comm={} state={} wchan={}",
            task.tid, task.comm, task.state, task.wchan
        ));
    }
}

fn analyst_task(pid: u32) -> Option<TaskSnap> {
    task_snapshot(pid)
        .into_iter()
        .find(|t| t.comm.starts_with("qianqian-observ"))
}

fn decode_task(pid: u32) -> Option<TaskSnap> {
    task_snapshot(pid)
        .into_iter()
        .find(|t| t.comm.starts_with("qianqian-decode"))
}

fn assert_sleeping(task: &TaskSnap, what: &str) {
    assert!(
        matches!(task.state.as_str(), "S" | "D"),
        "{what} must be sleeping while wedged: {task:?}"
    );
    assert!(
        !matches!(task.wchan.as_str(), "" | "0"),
        "{what} must be blocked in a kernel wait, not running: {task:?}"
    );
}

// --- scenarios --------------------------------------------------------------

/// Healthy control: the real analyst exits normally (close -> one iteration ->
/// join), the decode worker joins, and the authoritative K0 verdict returns.
#[test]
fn analyst_join_e2e_healthy_control() {
    let _guard = crate::machine_input::test_spawn_lock();
    let mut run = ChildRun::spawn("healthy", false, false);
    let establishment = run.wait_for("ESTABLISHMENT=", Duration::from_secs(30));
    assert!(establishment.contains("Established"), "{establishment}");
    run.wait_for("TERMINAL=", Duration::from_secs(30));
    run.wait_for("DISPOSE-RETURNED", Duration::from_secs(30));
    run.drain_for(Duration::from_secs(2));
    let status = run.kill_and_wait();
    for line in &run.seen {
        out(&format!("CHILD-LINE={line}"));
    }
    assert!(status.success(), "healthy child failed: {status:?}");
    assert!(
        run.saw("DISPOSE-RETURNED verdict=Discharged quiet=true"),
        "healthy disposal must be authoritative and quiet: {:?}",
        run.seen
    );
    assert!(run.saw("IS-CLOSED=true"), "{:?}", run.seen);
    assert!(run.saw("ANALYST-THREAD-GONE=true"), "{:?}", run.seen);
    assert!(!run.saw("PREVIOUS-HOOK-ENTERED"), "{:?}", run.seen);
}

/// Broken stderr (EBADF) + real analyst panic: the chained default hook's write
/// fails fast and is ignored, so the panic unwinds, the analyst's catch
/// completes, the funnel join returns and dispose returns Discharged. This
/// isolates the cause to BLOCKING, not to chaining or to the panic itself.
#[test]
fn analyst_join_e2e_broken_stderr_panic_still_discharges() {
    let _guard = crate::machine_input::test_spawn_lock();
    let isolated = std::env::var("QIANQIAN_ANALYST_JOIN_EXPECT").as_deref() == Ok("complete");
    let mut run = ChildRun::spawn("broken-stderr-panic", false, true);
    run.wait_for("ESTABLISHMENT=", Duration::from_secs(30));
    if isolated {
        // Fix build: the analyst is class α, so the previous hook is never
        // entered at all; the panic is the atomics-only record.
        run.wait_for("TERMINAL=", Duration::from_secs(30));
    } else {
        // Baseline: the chained default hook is entered and returns because
        // the EBADF write fails fast.
        run.wait_for("PREVIOUS-HOOK-ENTERED-ANALYST", Duration::from_secs(30));
        run.wait_for("PREVIOUS-HOOK-RETURNED-ANALYST", Duration::from_secs(30));
    }
    run.wait_for("TERMINAL=", Duration::from_secs(30));
    run.wait_for("DISPOSE-RETURNED", Duration::from_secs(30));
    run.drain_for(Duration::from_secs(2));
    let status = run.kill_and_wait();
    for line in &run.seen {
        out(&format!("CHILD-LINE={line}"));
    }
    assert!(status.success(), "broken-stderr child failed: {status:?}");
    assert!(
        run.saw("DISPOSE-RETURNED verdict=Discharged"),
        "{:?}",
        run.seen
    );
    assert!(run.saw("ANALYST-THREAD-GONE=true"), "{:?}", run.seen);
    if isolated {
        assert!(!run.saw("PREVIOUS-HOOK-ENTERED-ANALYST"), "{:?}", run.seen);
        assert!(run.saw("CRITICAL-PANICS=1"), "{:?}", run.seen);
    }
}

/// Difference control: the same saturated, UNREAD stderr sink with NO analyst
/// panic. A full sink alone must not wedge close/join/dispose — the trigger is
/// panic × not-registered × join, not the sink by itself.
#[test]
fn analyst_join_e2e_blocked_sink_without_panic_discharges() {
    let _guard = crate::machine_input::test_spawn_lock();
    let mut run = ChildRun::spawn("blocked-sink-control", true, false);
    run.wait_for("ESTABLISHMENT=", Duration::from_secs(30));
    run.wait_for("TERMINAL=", Duration::from_secs(30));
    run.wait_for("DISPOSE-RETURNED", Duration::from_secs(30));
    run.drain_for(Duration::from_secs(2));
    let status = run.kill_and_wait();
    for line in &run.seen {
        out(&format!("CHILD-LINE={line}"));
    }
    assert!(status.success(), "blocked-sink control failed: {status:?}");
    assert!(
        run.saw("DISPOSE-RETURNED verdict=Discharged"),
        "{:?}",
        run.seen
    );
    assert!(run.saw("ANALYST-THREAD-GONE=true"), "{:?}", run.seen);
    assert!(!run.saw("PREVIOUS-HOOK-ENTERED"), "{:?}", run.seen);
    assert!(run.saw("CRITICAL-PANICS=0"), "{:?}", run.seen);
}

/// The ONE oracle for the claimed dependency edge, parameterized only by the
/// outcome the current source shape predicts:
///
///   * baseline (`QIANQIAN_ANALYST_JOIN_EXPECT=wedge`, the default): the
///     previous hook is ENTERED on the real analyst thread and never returns
///     (wchan=anon_pipe_write, pipe saturated and unread); the terminal Fact is
///     already published and `close()` returned, but the decode worker is
///     alive in `analyst.join()` and `dispose()` never returns.
///   * falsifier builds (`...EXPECT=complete`): either the analyst registers
///     class α (hook never chains) or the funnel join is removed; the SAME
///     oracle steps must then observe a completed disposal instead.
///
/// The observed discriminator is when the first of
/// `PREVIOUS-HOOK-ENTERED-ANALYST` / `DISPOSE-RETURNED` appears; everything else
/// is asserted per branch. The kill is fail-closed cleanup, never the proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Observed {
    Wedged,
    Completed,
}

fn drive_blocked_oracle() -> Observed {
    let mut run = ChildRun::spawn("blocked-stderr-panic", true, true);
    run.wait_for("ESTABLISHMENT=", Duration::from_secs(30));
    let terminal = run.wait_for("TERMINAL=", Duration::from_secs(30));
    assert!(
        terminal.contains("Completed"),
        "the terminal Fact must be committed regardless of the sink: {terminal}"
    );

    // The verdict under test is whether the AUTHORITATIVE dispose returns. The
    // bounded wait is the branch selector only; the wedge branch's proof is the
    // marker + /proc + pipe evidence below, never this wait.
    let completed = run.try_wait_for("DISPOSE-RETURNED", Duration::from_secs(10));
    if completed.is_none() {
        run.wait_for("PREVIOUS-HOOK-ENTERED-ANALYST", Duration::from_secs(10));
        run.wait_for("IS-CLOSED=true", Duration::from_secs(30));
        run.wait_for("DISPOSE-ENTERED", Duration::from_secs(30));

        // Causal witness, not a timeout: the pipe is saturated and UNREAD, the
        // previous hook was entered on the analyst and has not returned, the
        // analyst task exists, and the decode worker is still a live thread.
        let stderr = run.stderr.as_ref().expect("blocked stderr handle");
        let unread = unread_bytes(stderr);
        assert!(
            unread > 0,
            "the child's stderr pipe must be saturated (unread bytes: {unread})"
        );
        let pid = run.child.id();
        let analyst = analyst_task(pid).expect("the real analyst thread must exist");
        let decode = decode_task(pid).expect("the real decode worker must still exist");
        assert_sleeping(&analyst, "the analyst (inside the chained previous hook)");
        assert_sleeping(&decode, "the decode worker (inside analyst.join)");
        print_tasks("blocked", pid);

        // A further observation window: the funnel is still parked, the hook
        // still has not returned, and no dispose verdict appeared.
        run.drain_for(Duration::from_secs(2));
        assert!(
            !run.saw("PREVIOUS-HOOK-RETURNED-ANALYST"),
            "the previous hook returned on the analyst: {:?}",
            run.seen
        );
        assert!(
            !run.saw("DISPOSE-RETURNED"),
            "dispose returned despite the blocked analyst: {:?}",
            run.seen
        );
        let decode_again = decode_task(pid).expect("decode worker must still exist");
        assert_sleeping(&decode_again, "the decode worker (still in analyst.join)");
        print_tasks("blocked-late", pid);

        // Fail-closed cleanup; NOT the proof.
        let status = run.kill_and_wait();
        for line in &run.seen {
            out(&format!("CHILD-LINE={line}"));
        }
        out(&format!(
            "ORACLE observed=Wedged unread={unread} analyst_tid={} decode_tid={} killed={status:?}",
            analyst.tid, decode.tid
        ));
        Observed::Wedged
    } else {
        // Completed: the sink never decided the join. Require the two exact
        // signatures of the fix candidates, and explicitly the ABSENCE of the
        // chained previous hook (α isolation) or of the join edge.
        run.wait_for("CRITICAL-PANICS=", Duration::from_secs(30));
        run.wait_for("ANALYST-THREAD-GONE=", Duration::from_secs(30));
        let status = run.kill_and_wait();
        assert!(status.success(), "completed child failed: {status:?}");
        assert!(
            run.saw("DISPOSE-RETURNED verdict=Discharged"),
            "{:?}",
            run.seen
        );
        assert!(run.saw("OUTPUT-EFFECT-DISCHARGED=true"), "{:?}", run.seen);
        assert!(!run.saw("PREVIOUS-HOOK-RETURNED-ANALYST"), "{:?}", run.seen);
        for line in &run.seen {
            out(&format!("CHILD-LINE={line}"));
        }
        out(&format!(
            "ORACLE observed=Completed critical_panics={} analyst_gone={}",
            run.seen
                .iter()
                .find(|l| l.starts_with("CRITICAL-PANICS="))
                .cloned()
                .unwrap_or_default(),
            run.seen
                .iter()
                .find(|l| l.starts_with("ANALYST-THREAD-GONE="))
                .cloned()
                .unwrap_or_default()
        ));
        Observed::Completed
    }
}

#[test]
fn analyst_join_e2e_blocked_stderr_oracle() {
    let _guard = crate::machine_input::test_spawn_lock();
    let expected = match std::env::var("QIANQIAN_ANALYST_JOIN_EXPECT")
        .unwrap_or_else(|_| "wedge".to_owned())
        .as_str()
    {
        "wedge" => Observed::Wedged,
        "complete" => Observed::Completed,
        other => panic!("unknown expectation {other:?}"),
    };
    let observed = drive_blocked_oracle();
    out(&format!("ORACLE expected={expected:?} observed={observed:?}"));
    assert_eq!(
        observed, expected,
        "the blocked-stderr x analyst-panic oracle did not match the predicted outcome"
    );
}
