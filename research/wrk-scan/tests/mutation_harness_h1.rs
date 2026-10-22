//! H1 mutation-arm harness (Gate II §14, issue #308): the frozen probe
//! shape — probe batch N=48, P=4, B=Q=8, cancel milestone after the 24th
//! admission (group 7) — executed by the H1 bounded-channel mechanism,
//! with an ABSOLUTE green assertion.
//!
//! The harness source is byte-identical in the clean tree and in every
//! mutant copy; the arm driver (mutations/run-mutation-arms-h1.sh) runs
//! it as a separate OS process per arm:
//!   arm A (clean copy)        → this test must PASS;
//!   arm B (mutant copy M1–M9) → this test must FAIL with the mutant's
//!                               designated oracle check;
//!   arm C (fresh clean copy)  → this test must PASS.
//! The oracle is never modified or inverted by the driver — a mutant
//! "counts" only when the unmodified oracle goes red. M2/M6/M8/M9 are
//! the SHARED session-core mutants (the same patch files the H0
//! campaign applies); M1/M3/M4/M5/M7 are the H1 dispatch-path mutants.

use wrk_scan::h1::{self, H1Config};
use wrk_scan::observer::ObserverSink;
use wrk_scan::oracle;
use wrk_scan::workload::BatchSpec;

#[test]
fn probe_batch_oracle_green_h1() {
    let spec = BatchSpec::probe_batch();
    let sink = ObserverSink::on(spec.batch_id);
    let cfg = H1Config {
        p: 4,
        b: 8,
        cancel_milestone: Some((24, 7)),
    };
    let stats = h1::run_session_h1(cfg, spec.clone(), &sink).expect("settlement");
    println!(
        "ADMITTED={} JOINED={}",
        stats.stats.admitted_count, stats.stats.workers_joined
    );
    let verdict = oracle::adjudicate(&spec, sink.ledger().unwrap());
    for c in &verdict.checks {
        println!("  {:<42} {}", c.check, if c.pass { "PASS" } else { "FAIL" });
    }
    println!(
        "VERDICT = {}",
        if verdict.green() { "GREEN" } else { "RED" }
    );
    assert!(verdict.green(), "ORACLE RED: {:?}", verdict.failed());
}
