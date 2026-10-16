//! Real-sink delivery failure: a CLOSED stderr descriptor (a pipe whose reader
//! was dropped) must be accounted as a FAILED export, never as a delivered
//! record. The `export_failed` counter exists precisely so a failed descriptor
//! cannot be reported as successful delivery, and this witnesses it on the REAL
//! `StderrExporter`, in a child process whose stderr pipe has no reader.
//!
//! A custom failing exporter (see `exporter_failure.rs`) cannot witness this:
//! the property is about what the built-in sink does with an operating-system
//! write error.

use std::io::Read;
use std::process::Stdio;
use std::time::Duration;

const DELIVERY_SCENARIO: &str = "QIANQIAN_STDERR_DELIVERY_SCENARIO";
const MARKERS: i64 = 5;

/// Child entry: the real sink, the real facility, a stderr pipe with no reader.
///
/// The child does NOTHING until the parent releases it on stdin: the broken
/// read end must exist before the first write, and "the parent dropped it
/// before the child got that far" is a schedule, not a guarantee. The gate is
/// the synchronization seam that makes the premise true by construction.
#[test]
fn broken_stderr_child_witness() {
    let Ok(scenario) = std::env::var(DELIVERY_SCENARIO) else {
        return;
    };
    assert_eq!(scenario, "broken-stderr");
    {
        use std::io::BufRead;
        let mut go = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut go)
            .expect("the parent releases the child after the read end is gone");
        assert_eq!(go.trim(), "GO", "the parent's gate");
    }
    use qianqian_diagnostics::{Config, Diagnostics, Level, Record, StderrExporter, Text};
    let diagnostics = Diagnostics::install(Box::new(StderrExporter::default()), Config::default())
        .expect("the child owns exactly one facility");
    for marker in 0..MARKERS {
        let mut message = Text::from_static("delivery-failure=");
        message.push_i64(marker);
        let _ = qianqian_diagnostics::emit(
            Record::new("stderr-delivery", Level::Error).with_message(message),
        );
    }
    // The bounded shutdown's final sweep delivers everything still queued, so
    // the accounting below is complete when it returns.
    let report = diagnostics.shutdown(Duration::from_secs(10));
    let counters = report.counters;
    println!(
        "DELIVERY exported={} export_failed={} self_diagnostics={} copied_out={} beta_consumed={}",
        counters.exported,
        counters.export_failed,
        counters.self_diagnostics,
        counters.copied_out,
        counters.beta_consumed,
    );
    std::process::exit(0);
}

/// Oracle: every record the drainer handed to the built-in stderr sink on a
/// broken descriptor is counted as an export LOSS — `exported` stays 0, the
/// drain-boundary identity still holds, and the disable is reported exactly
/// once as a self-diagnostic.
#[test]
fn a_broken_stderr_descriptor_is_counted_as_a_failed_delivery() {
    let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", "broken_stderr_child_witness", "--nocapture"])
        .env(DELIVERY_SCENARIO, "broken-stderr")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the broken-stderr child");
    let mut stdin = child.stdin.take().expect("stdin pipe");
    // Drop the stderr read end NOW, and only then release the child: the child
    // is gated on stdin precisely so this order cannot race. Its writes now
    // fail with EPIPE instead of blocking (this is the broken-descriptor case,
    // not the blocked one).
    drop(child.stderr.take());
    {
        use std::io::Write;
        writeln!(stdin, "GO").expect("release the child");
        stdin.flush().expect("flush the gate");
    }
    let mut stdout = child.stdout.take().expect("stdout pipe");
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let text = loop {
        match child.try_wait().expect("wait on the child") {
            Some(status) => {
                let mut text = String::new();
                let _ = stdout.read_to_string(&mut text);
                assert!(status.success(), "the child failed: {text}");
                break text;
            }
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the broken-stderr child never exited");
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    println!("broken-stderr child: {text:?}");
    let find = |key: &str| -> u64 {
        text.lines()
            .find_map(|line| {
                line.split_whitespace()
                    .find_map(|token| token.strip_prefix(key))
                    .and_then(|value| value.parse::<u64>().ok())
            })
            .unwrap_or_else(|| panic!("no {key} in the child report: {text:?}"))
    };
    let exported = find("exported=");
    let export_failed = find("export_failed=");
    let self_diagnostics = find("self_diagnostics=");
    let copied_out = find("copied_out=");
    let beta_consumed = find("beta_consumed=");
    assert_eq!(
        exported, 0,
        "a failed write was accounted as a delivered record: {text:?}"
    );
    assert_eq!(
        export_failed, MARKERS as u64,
        "every record handed to the broken sink is an export loss: {text:?}"
    );
    assert_eq!(
        copied_out + beta_consumed,
        exported + export_failed,
        "the drain-boundary identity still holds: {text:?}"
    );
    assert_eq!(
        self_diagnostics, 1,
        "the disable of a failing exporter is reported exactly once: {text:?}"
    );
}
