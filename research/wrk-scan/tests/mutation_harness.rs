//! Mutation-arm harness (Gate 授权 §2.6): the frozen probe shape —
//! probe batch N=48, P=4, B=8, cancel milestone after the 24th admission
//! (group 7) — with an ABSOLUTE green assertion.
//!
//! The harness source is byte-identical in the clean tree and in every
//! mutant copy; the arm driver (mutations/run-mutation-arms.sh) runs it
//! as a separate OS process per arm:
//!   arm A (clean copy)        → this test must PASS;
//!   arm B (mutant copy M1–M9) → this test must FAIL with the mutant's
//!                               designated oracle check;
//!   arm C (fresh clean copy)  → this test must PASS.
//! The oracle is never modified or inverted by the driver — a mutant
//! "counts" only when the unmodified oracle goes red.

use wrk_scan::h0::{self, H0Config};
use wrk_scan::observer::ObserverSink;
use wrk_scan::oracle;
use wrk_scan::workload::BatchSpec;

#[test]
fn probe_batch_oracle_green() {
    let spec = BatchSpec::probe_batch();
    let sink = ObserverSink::on(spec.batch_id);
    let cfg = H0Config {
        p: 4,
        b: 8,
        cancel_milestone: Some((24, 7)),
    };
    let stats = h0::run_session(cfg, spec.clone(), &sink).expect("settlement");
    println!(
        "ADMITTED={} JOINED={}",
        stats.admitted_count, stats.workers_joined
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
