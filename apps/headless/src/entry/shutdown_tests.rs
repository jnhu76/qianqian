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
