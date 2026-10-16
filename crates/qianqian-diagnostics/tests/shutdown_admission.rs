//! Class-α admission after shutdown: the facility's terminal state is checked
//! FIRST, for both producer classes. `Diagnostics::shutdown` stops admitting
//! records — the drainer's final sweep is the last consumer there will be — so
//! a presentation that arrives after the flag is observed must be dropped and
//! COUNTED (the presenting pool's admission counters for class α, `beta_rejected`
//! for class β), never published into a pool nothing will ever drain again.
//!
//! Producer quiescence is not required by `shutdown` and cannot be: the host's
//! stdin reader is a detached thread. The residual that leaves is declared on
//! `Diagnostics::shutdown` — a publication that races the final sweep of its own
//! pool may still be stranded (neither delivered nor counted). This witness
//! covers the deterministic half: presentation AFTER `shutdown` returned.

use std::io::Read;
use std::process::Stdio;
use std::time::{Duration, Instant};

const ADMISSION_SCENARIO: &str = "QIANQIAN_DIAG_ADMISSION_SCENARIO";

/// Child entry: install, register a class-α thread, deliver one record, shut
/// down, then present one more record on the SAME registered thread.
#[test]
fn shutdown_admission_child_witness() {
    let Ok(scenario) = std::env::var(ADMISSION_SCENARIO) else {
        return;
    };
    assert_eq!(scenario, "post-shutdown");
    use qianqian_diagnostics::{
        AlphaRegistration, Config, Diagnostics, Level, Record, StderrExporter, emit,
    };
    let diagnostics = Diagnostics::install(Box::new(StderrExporter::default()), Config::default())
        .expect("the child owns exactly one facility");
    let registration = AlphaRegistration::register();
    let first = emit(Record::new("shutdown-admission", Level::Info));
    let report = diagnostics.shutdown(Duration::from_secs(10));
    let after = emit(Record::new("shutdown-admission", Level::Info));
    let counters = Diagnostics::global_counters().expect("the child owns the facility");
    println!(
        "ADMISSION first={first:?} after={after:?} accepted={} rejected={} attempted={} \
         outstanding={} alpha_rejected={} beta_rejected={} drained={}",
        registration.accepted(),
        registration.rejected(),
        registration.attempted(),
        registration.outstanding(),
        counters.alpha_rejected,
        counters.beta_rejected,
        report.drained,
    );
    std::process::exit(0);
}

/// Oracle: after shutdown, a class-α presentation is dropped + counted, and
/// nothing is left stranded in the pool. RED under the defect: `emit` consults
/// the thread-local pool BEFORE the facility's shutdown state, so the record is
/// "accepted" into a pool whose drainer has already exited — an outstanding
/// record that can never be delivered, accounted as an admission success.
#[test]
fn a_class_alpha_presentation_after_shutdown_is_dropped_and_counted() {
    let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", "shutdown_admission_child_witness", "--nocapture"])
        .env(ADMISSION_SCENARIO, "post-shutdown")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        // The pre-shutdown record's line is not this witness's subject; the
        // null sink keeps the export healthy so nothing inflates the counters.
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the admission child");
    let mut stdout = child.stdout.take().expect("stdout pipe");
    let deadline = Instant::now() + Duration::from_secs(60);
    let text = loop {
        match child.try_wait().expect("wait on the child") {
            Some(status) => {
                let mut text = String::new();
                let _ = stdout.read_to_string(&mut text);
                assert!(status.success(), "the child failed: {text}");
                break text;
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the post-shutdown admission child never exited");
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    println!("post-shutdown admission child: {text:?}");
    let find = |key: &str| -> u64 {
        text.lines()
            .find_map(|line| {
                line.split_whitespace()
                    .find_map(|token| token.strip_prefix(key))
                    .and_then(|value| value.parse::<u64>().ok())
            })
            .unwrap_or_else(|| panic!("no {key} in the child report: {text:?}"))
    };
    assert!(
        text.contains("first=AlphaRecorded"),
        "the pre-shutdown record was admitted: {text:?}"
    );
    assert!(
        text.contains("after=Shutdown"),
        "a post-shutdown class-α presentation must be rejected, not accepted into \
         a pool nothing will drain: {text:?}"
    );
    assert!(
        text.contains("drained=true"),
        "the premise is a drained shutdown"
    );
    assert_eq!(
        find("accepted="),
        1,
        "only the admitted record was recorded"
    );
    assert_eq!(find("rejected="), 1, "the late record is a counted loss");
    assert_eq!(
        find("attempted="),
        2,
        "the late presentation still crosses the admission boundary: {text:?}"
    );
    assert_eq!(
        find("outstanding="),
        0,
        "nothing may be left stranded in the pool: {text:?}"
    );
    assert!(
        find("alpha_rejected=") >= 1,
        "the loss reaches the facility totals: {text:?}"
    );
    assert_eq!(
        find("beta_rejected="),
        0,
        "a class-α thread's loss is not charged to the β channel: {text:?}"
    );
}
