//! The host exit boundary, witnessed on the real path.
//!
//! The facility's own abandonment test ends at `Diagnostics::shutdown`; the
//! hang the review named lives in the HOST's report *after* the abandoned
//! shutdown: the report used to be written synchronously, so the exit path
//! re-entered the very sink the abandonment rule had just declared stuck — for
//! stderr that means waiting on the process stderr lock held by the blocked
//! drainer. This module drives the real host wiring
//! ([`super::install_host_diagnostics`] + [`super::shutdown_host_diagnostics`])
//! in a child whose stderr is a pipe the parent never reads, and requires the
//! child to exit on its own.

use std::io::Read;
use std::time::{Duration, Instant};

/// Set in the child's environment; unset in the parent's, so the test function
/// below is inert for the ordinary run.
const EXIT_SCENARIO: &str = "QIANQIAN_DIAG_EXIT_SCENARIO";

/// The drained-report scenario's own env var: that child builds the opposite
/// state (a COMPLETED drain with a sink saturated to its last byte) and must
/// not be confused with the abandoned-sink one.
const DRAINED_REPORT_SCENARIO: &str = "QIANQIAN_DIAG_DRAINED_REPORT_SCENARIO";

/// The bulk-record site. Records are emitted unstamped, so they render as
/// `[site] info ts_us=0\n` — one fixed-length line per record.
const DRAINED_LINE_SITE: &str = "drained-report";
/// The record run: this many bulk records, plus the panic record.
const DRAINED_RECORDS: u64 = 8;

/// Child entry: fill the REAL stderr sink through the facility, witness the
/// drainer's stall, then run the production shutdown path and report.
#[test]
fn host_shutdown_report_child_witness() {
    let Ok(scenario) = std::env::var(EXIT_SCENARIO) else {
        return;
    };
    assert_eq!(scenario, "blocked-sink");
    let diagnostics = super::install_host_diagnostics().expect("the child owns the outlet");
    // Fill the sink THROUGH the facility: the drainer blocks inside a write and
    // holds the process stderr lock — the production failure state, not a mock.
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut previous_consumed = 0u64;
    let (consumed, backlog) = loop {
        for marker in 0..256 {
            let mut message = qianqian_diagnostics::Text::from_static("blocked-sink-fill=");
            message.push_i64(marker);
            let _ = qianqian_diagnostics::emit(
                qianqian_diagnostics::Record::new(
                    "exit-boundary",
                    qianqian_diagnostics::Level::Error,
                )
                .with_message(message),
            );
        }
        std::thread::sleep(Duration::from_millis(250));
        let counters = qianqian_diagnostics::Diagnostics::global_counters().expect("installed");
        let backlog = counters.beta_queued.saturating_sub(counters.beta_consumed);
        // Stall witness: work remains queued and the drainer stopped pulling —
        // it is inside a blocking write, holding the stderr lock.
        if backlog > 0 && counters.beta_consumed == previous_consumed && previous_consumed > 0 {
            break (counters.beta_consumed, backlog);
        }
        previous_consumed = counters.beta_consumed;
        assert!(
            Instant::now() < deadline,
            "the sink never blocked (consumed {previous_consumed})"
        );
    };
    println!("SINK-BLOCKED consumed={consumed} backlog={backlog}");
    // The host's own shutdown + report path, unchanged from production.
    super::shutdown_host_diagnostics(Some(diagnostics));
    println!("SHUTDOWN-RETURNED");
    // Explicit exit: the harness must not print to the blocked stderr, and this
    // is the same boundary the real transport crosses (`run_playback` returns,
    // then the runtime exits).
    std::process::exit(0);
}

/// Oracle: a drainer abandoned inside the real stderr sink must not be able to
/// make the host's exit path wait on that sink. RED without the fix: the child
/// never returns from `shutdown_host_diagnostics` (the synchronous report write
/// waits on the stderr lock the stuck drainer holds) and the deadline below
/// kills it.
#[test]
fn an_abandoned_drainer_never_makes_the_host_exit_path_wait_on_its_sink() {
    // Forkling-window exclusion (see machine_input::tests::test_spawn_lock):
    // this process's spawn windows must not overlap another test's pipe close.
    let _spawn_guard = crate::machine_input::test_spawn_lock();
    let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "entry::shutdown_tests::host_shutdown_report_child_witness",
            "--nocapture",
        ])
        .env(EXIT_SCENARIO, "blocked-sink")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        // Kept OPEN and never read: that IS the blocked sink.
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn the blocked-sink child");
    let mut stdout = child.stdout.take().expect("stdout pipe");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match child.try_wait().expect("wait on the child") {
            Some(status) => {
                let mut text = String::new();
                let _ = stdout.read_to_string(&mut text);
                println!("blocked-sink child: status={status:?} stdout={text:?}");
                assert!(status.success(), "the child failed: {text}");
                assert!(
                    text.contains("SINK-BLOCKED"),
                    "the drainer's stall was not witnessed: {text}"
                );
                assert!(
                    text.contains("SHUTDOWN-RETURNED"),
                    "the host shutdown path never returned: {text}"
                );
                return;
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "the host exit path waited on the blocked sink: the child never \
                     returned from shutdown_host_diagnostics"
                );
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Child entry (drained report): the real outlet, the real panic posture, a
/// registered producer, a registered thread that really panics (so the
/// shutdown report is MANDATORY), a bounded drain that really drains — and then
/// an independent writer that fills the sink to its last byte before the
/// production exit path runs its report against it.
///
/// The order is the point. `drained` is established FIRST, while the sink still
/// has room for the record run; only then is the remaining capacity consumed.
/// This is exactly the boundary the review named: a completed drain proves the
/// drainer's own writes finished, NOT that the descriptor still has room for
/// the report the host owes.
#[test]
fn drained_report_child_witness() {
    let Ok(scenario) = std::env::var(DRAINED_REPORT_SCENARIO) else {
        return;
    };
    assert_eq!(scenario, "drained-report");
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    let diagnostics = super::install_host_diagnostics().expect("the child owns the outlet");
    crate::machine_input::install_panic_hook_posture();
    let registration = qianqian_diagnostics::AlphaRegistration::register();
    let deadline = Instant::now() + Duration::from_secs(60);

    // 1. A REAL panic on a registered thread makes the report mandatory
    //    (`critical_panics > 0`); the posture keeps the default print off the
    //    pipe — the sink's every byte is accounted below.
    let panicked = std::thread::Builder::new()
        .name("qianqian-drained-report-panic".into())
        .spawn(|| {
            let _registration = qianqian_diagnostics::AlphaRegistration::register();
            panic!("drained-report witness panic");
        })
        .expect("spawn the panicking thread")
        .join();
    assert!(panicked.is_err(), "the witness thread really panicked");
    wait_for_exported(&diagnostics, 1, deadline);
    assert!(qianqian_diagnostics::critical_panics() >= 1);

    // 2. The record run, delivered through the real facility.
    for expected in 2..=DRAINED_RECORDS + 1 {
        assert_eq!(
            emit_bulk_record(),
            qianqian_diagnostics::Disposition::AlphaRecorded,
            "the pool must have room: each publish waits for its delivery"
        );
        wait_for_exported(&diagnostics, expected, deadline);
    }
    println!("BULK-EXPORTED {}", DRAINED_RECORDS + 1);

    // 3. The bounded drain, and the premise is verified rather than assumed:
    //    drained, with every admitted record delivered.
    let report = diagnostics.shutdown(super::HOST_DIAGNOSTIC_EXIT_DEADLINE);
    assert!(
        report.drained && !report.abandoned,
        "the premise is a completed drain: {report:?}"
    );
    assert_eq!(report.undelivered_at_exit, 0, "{report:?}");
    println!("DRAINED undelivered={}", report.undelivered_at_exit);

    // 4. Saturate the sink: an independent 1-byte writer consumes the remaining
    //    capacity and parks inside a write, holding the process stderr lock.
    //    The sink is now full, and nothing will ever drain it — the parent
    //    keeps the read end open and unread. The filler is deliberately left
    //    parked: this child ends through `exit`, and the parked writer is
    //    exactly the state under test.
    let written = Arc::new(AtomicU64::new(0));
    {
        let written = Arc::clone(&written);
        std::thread::Builder::new()
            .name("qianqian-drained-report-filler".into())
            .spawn(move || {
                use std::io::Write;
                loop {
                    let mut stderr = std::io::stderr().lock();
                    if stderr.write_all(b"x").is_err() {
                        return;
                    }
                    drop(stderr);
                    written.fetch_add(1, Ordering::Relaxed);
                }
            })
            .expect("spawn the filler");
    }
    wait_for_stall(&written, deadline, "the filler never saturated the sink");
    println!("SATURATED bytes={}", written.load(Ordering::Relaxed));

    // 5. The production exit path on the drained facility: the report is
    //    mandatory (critical_panics > 0) and now faces a full sink.
    super::shutdown_host_diagnostics(Some(diagnostics));
    let counters =
        qianqian_diagnostics::Diagnostics::global_counters().expect("the child owns the facility");
    println!(
        "SHUTDOWN-RETURNED exported={} accepted={} rejected={} critical_panics={}",
        counters.exported,
        registration.accepted(),
        registration.rejected(),
        counters.critical_panics,
    );
    std::process::exit(0);
}

fn emit_bulk_record() -> qianqian_diagnostics::Disposition {
    qianqian_diagnostics::emit(qianqian_diagnostics::Record::new(
        DRAINED_LINE_SITE,
        qianqian_diagnostics::Level::Info,
    ))
}

fn wait_for_exported(
    diagnostics: &qianqian_diagnostics::Diagnostics,
    target: u64,
    deadline: Instant,
) {
    while diagnostics.counters().exported < target {
        assert!(
            Instant::now() < deadline,
            "the drainer delivered {} of {target} records before the deadline",
            diagnostics.counters().exported
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Wait until the filler's write counter stops advancing across two check
/// windows: the sample that means its write is parked (a full sink, or a lock
/// a stuck writer holds) — either way, a later write must not be attempted on
/// the exit path.
fn wait_for_stall(written: &std::sync::atomic::AtomicU64, deadline: Instant, what: &str) {
    use std::sync::atomic::Ordering;
    let mut last = written.load(Ordering::Relaxed);
    let mut stable = 0;
    loop {
        std::thread::sleep(Duration::from_millis(250));
        let now = written.load(Ordering::Relaxed);
        if now == last {
            stable += 1;
            if stable >= 2 {
                return;
            }
        } else {
            stable = 0;
        }
        last = now;
        assert!(Instant::now() < deadline, "{what} (wrote {now} bytes)");
    }
}

/// Oracle: a DRAINED drain whose sink has been filled to its last byte by
/// another writer must not hand the host's exit back to that sink. The child
/// verifies `drained` first, then lets an independent writer consume the
/// remaining capacity, and only then runs the production exit path with a
/// MANDATORY report (a counted critical panic). RED against a synchronous
/// report write: the child never returns from `shutdown_host_diagnostics` and
/// the deadline below kills it.
#[test]
fn a_drained_report_never_makes_the_host_exit_path_wait_on_its_sink() {
    // Forkling-window exclusion (see machine_input::tests::test_spawn_lock):
    // this process's spawn windows must not overlap another test's pipe close.
    let _spawn_guard = crate::machine_input::test_spawn_lock();
    let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "entry::shutdown_tests::drained_report_child_witness",
            "--nocapture",
        ])
        .env(DRAINED_REPORT_SCENARIO, "drained-report")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        // Kept OPEN and never read: that IS the saturated sink.
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn the drained-report child");
    let mut stdout = child.stdout.take().expect("stdout pipe");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match child.try_wait().expect("wait on the child") {
            Some(status) => {
                let mut text = String::new();
                let _ = stdout.read_to_string(&mut text);
                println!("drained-report child: status={status:?} stdout={text:?}");
                assert!(status.success(), "the child failed: {text}");
                // The sequence is the premise: the drain completed before the
                // sink was saturated, and the report ran after both.
                let mut previous = 0usize;
                for prefix in ["BULK-EXPORTED", "DRAINED", "SATURATED", "SHUTDOWN-RETURNED"] {
                    let at = text
                        .find(prefix)
                        .unwrap_or_else(|| panic!("no {prefix} in the child report: {text:?}"));
                    assert!(at >= previous, "{prefix} out of order: {text:?}");
                    previous = at;
                }
                assert!(
                    text.contains(&format!("BULK-EXPORTED {}", DRAINED_RECORDS + 1)),
                    "the record run was not delivered: {text}"
                );
                let marker = text
                    .lines()
                    .find(|line| line.contains("SHUTDOWN-RETURNED"))
                    .expect("the returned marker");
                assert!(
                    marker.contains(&format!("exported={}", DRAINED_RECORDS + 1)),
                    "every record must have reached the sink before the saturation: {marker}"
                );
                assert!(
                    marker.contains(&format!("accepted={DRAINED_RECORDS}"))
                        && marker.contains("rejected=0"),
                    "the record run was admitted without a single pool rejection: {marker}"
                );
                assert!(
                    marker.contains("critical_panics=1"),
                    "the report was mandatory (a counted critical panic): {marker}"
                );
                return;
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "the drained drain's report handed the exit path back to the sink: \
                     the child never returned from shutdown_host_diagnostics"
                );
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}
