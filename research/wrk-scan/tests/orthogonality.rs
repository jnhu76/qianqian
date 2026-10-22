//! Gate II — K0 × Observer 2×2 orthogonal semantic smoke (issue #308,
//! GII-F; SPEC §4.6/§7.5). NOT a performance experiment: no timing, no
//! ranking, and the smoke runs are never counted into any frozen
//! 92-cell matrix.
//!
//! For EACH mechanism (H0 condvar pool, H1 bounded channel) the four
//! quadrants run over the SAME binary and the SAME business path:
//!
//! ```text
//! K0 OFF / Observer OFF     K0 OFF / Observer ON
//! K0 ON  / Observer OFF     K0 ON  / Observer ON
//! ```
//!
//! On a few pre-declared bounded inputs (the frozen probe batch at
//! P=4,B=8 and an S0 cell at P=1,B=1, both no-cancel), the quadrants
//! must prove:
//! 1. ON/OFF changes NOTHING about completion: identical per-item
//!    result sets, identical fan-in aggregate, identical to the
//!    independently computed REF (SPEC §4.6 rule 5, §7.5).
//! 2. Observer OFF deletes no obligation: the OFF receipt retains the
//!    per-item wait-done counts, the worker exits, the terminal set and
//!    the aggregate — OFF changes what is WRITTEN DOWN, never what is
//!    DONE.
//! 3. K0 ON adds only composition/lifecycle: Discharged teardown, and
//!    the executor surface (admit/settle/quiesce/abort) is the only
//!    thing K0 touches; items never appear in the kernel.
//! 4. The K0 composition owner stays on the calling (control) thread:
//!    the kernel/fiber types are `!Send` (`Rc`-based), so the compiler
//!    forbids moving the composition off this thread; the smoke runs
//!    compose/settle/dispose on the test thread throughout.

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use qianqian_composition::DisposeVerdict;

use wrk_scan::h0::{self, H0Config, H0Executor};
use wrk_scan::h1::{H1Config, H1Executor};
use wrk_scan::k0_session::{K0ScanSession, ScanSessionExecutor, SettlementWatchFace};
use wrk_scan::ledger::TerminalKind;
use wrk_scan::observer::{ObserverSink, OffReceipt};
use wrk_scan::oracle;
use wrk_scan::scenario::Scenario;
use wrk_scan::workload::{self, BatchSpec};

const SEED: u64 = 0x00C0_FFEE_0304_0001;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn result_digest(results: &[(u32, u64)]) -> String {
    let mut h = Sha256::new();
    for (id, c) in results {
        h.update(id.to_le_bytes());
        h.update(c.to_le_bytes());
    }
    format!("sha256:{}", hex(&h.finalize()))
}

/// The completed face of one run: per-item (id, class, contribution) and
/// the aggregate — extracted identically from an ON ledger or an OFF
/// receipt, so the quadrants are comparable without mode-specific code.
struct RunFace {
    results: Vec<(u32, u64)>,
    classes: HashMap<u32, TerminalKind>,
    aggregate: u64,
}

fn face_from_ledger(spec: &BatchSpec, sink: &ObserverSink) -> RunFace {
    let verdict = oracle::adjudicate(spec, sink.ledger().unwrap());
    assert!(verdict.green(), "oracle RED: {:?}", verdict.failed());
    let mut results = Vec::new();
    let mut classes = HashMap::new();
    let mut aggregate = None;
    for r in sink.ledger().unwrap().snapshot() {
        if let (Some(item), Some(term)) = (r.item_id, r.terminal) {
            classes.insert(item, term);
            if term != TerminalKind::Abandoned {
                results.push((item, r.payload));
            }
        }
        if r.kind == wrk_scan::ledger::EffectKind::AggregatePublished {
            aggregate = Some(r.payload);
        }
    }
    results.sort_by_key(|(id, _)| *id);
    RunFace {
        results,
        classes,
        aggregate: aggregate.expect("aggregate published"),
    }
}

fn face_from_receipt(receipt: &OffReceipt) -> RunFace {
    let mut results = Vec::new();
    let mut classes = HashMap::new();
    for (item, it) in &receipt.items {
        let term = it.terminal.expect("every admitted item reaches a terminal");
        classes.insert(*item, term);
        if term != TerminalKind::Abandoned {
            results.push((
                *item,
                it.result_contribution.expect("completed has contribution"),
            ));
        }
    }
    results.sort_by_key(|(id, _)| *id);
    RunFace {
        results,
        classes,
        aggregate: receipt.aggregate.expect("aggregate published"),
    }
}

/// The common face of one quadrant run plus the OFF-side obligation
/// evidence when the observer is OFF.
struct QuadrantRun {
    face: RunFace,
    off: Option<OffReceipt>,
}

/// One no-cancel run in a given quadrant. The K0-OFF arm drives the
/// mechanism through its own `run_session` (`direct`), the K0-ON arm
/// through the real composition kernel — both on this (control) thread.
fn run_quadrant<E: ScanSessionExecutor>(
    k0_on: bool,
    obs_on: bool,
    cfg: E::Config,
    spec: &BatchSpec,
    direct: impl Fn(E::Config, BatchSpec, &ObserverSink) -> Result<(), String>,
) -> QuadrantRun {
    let sink = if obs_on {
        ObserverSink::on(spec.batch_id)
    } else {
        ObserverSink::off(spec.batch_id)
    };
    if k0_on {
        let mut session =
            K0ScanSession::<E>::compose(cfg, spec.clone(), sink.clone()).expect("K0 compose");
        session.run_admission_and_settlement().expect("settlement");
        let (verdict, _stats) = session.dispose();
        assert_eq!(verdict, DisposeVerdict::Discharged, "K0-ON teardown");
    } else {
        direct(cfg, spec.clone(), &sink).expect("direct session");
    }
    let face = if obs_on {
        face_from_ledger(spec, &sink)
    } else {
        face_from_receipt(&sink.off_snapshot().unwrap())
    };
    QuadrantRun {
        face,
        off: sink.off_snapshot(),
    }
}

fn assert_quadrant_equivalence(mech: &str, spec: &BatchSpec, quadrants: [QuadrantRun; 4]) {
    // The independently computed REF face (oracle side only).
    let mut refs = Vec::new();
    for item in &spec.items {
        let r = workload::ref_result(spec.seed, item);
        refs.push((item.item_id, r.aggregation_contribution));
    }
    refs.sort_by_key(|(id, _)| *id);
    let ref_digest = result_digest(&refs);
    let ref_aggregate = workload::aggregate(&refs);

    let labels = ["K0OFF/OBSOFF", "K0OFF/OBSON", "K0ON/OBSOFF", "K0ON/OBSON"];
    for (label, q) in labels.iter().zip(quadrants.iter()) {
        assert_eq!(
            result_digest(&q.face.results),
            ref_digest,
            "{mech} {label}: result set != REF"
        );
        assert_eq!(q.face.aggregate, ref_aggregate, "{mech} {label}: aggregate");
        for (id, term) in &q.face.classes {
            let want = match spec.items[*id as usize].fault {
                Some(code) => TerminalKind::CompletedErr(code),
                None => TerminalKind::CompletedOk,
            };
            assert_eq!(*term, want, "{mech} {label} item {id} class");
        }
    }
    let d = |q: &QuadrantRun| result_digest(&q.face.results);
    assert_eq!(
        d(&quadrants[0]),
        d(&quadrants[1]),
        "{mech}: OBS OFF/ON differ (K0 OFF)"
    );
    assert_eq!(
        d(&quadrants[0]),
        d(&quadrants[2]),
        "{mech}: K0 OFF/ON differ (OBS OFF)"
    );
    assert_eq!(
        d(&quadrants[0]),
        d(&quadrants[3]),
        "{mech}: diagonal differs"
    );

    // Observer OFF must not delete obligations: wait-done counts exactly
    // where the workload demands them, terminals complete, quiescence
    // recorded, worker exits present.
    for (label, q) in [
        ("K0OFF/OBSOFF", &quadrants[0]),
        ("K0ON/OBSOFF", &quadrants[2]),
    ] {
        let receipt = q.off.as_ref().expect("OFF receipt retained");
        assert!(receipt.quiescence_recorded, "{mech} {label}");
        assert_eq!(receipt.items.len(), spec.items.len(), "{mech} {label}");
        for (id, it) in &receipt.items {
            let want_waits = u32::from(spec.items[*id as usize].wait_us > 0);
            assert_eq!(
                it.wait_done_count, want_waits,
                "{mech} {label} item {id} waits"
            );
            assert!(it.terminal.is_some(), "{mech} {label} item {id} terminal");
        }
        assert!(
            !receipt.worker_exits.is_empty(),
            "{mech} {label}: worker exits must be recorded even in OFF"
        );
    }
}

/// A pre-declared bounded input of this smoke: (label, scenario, P, B).
/// The frozen probe batch shape at 4/8 and a saturating S0 cell at 1/1;
/// both no-cancel; never re-counted into any frozen matrix.
fn smoke_inputs() -> Vec<(&'static str, BatchSpec, usize, usize)> {
    vec![
        ("probe-batch P4 B8", BatchSpec::probe_batch(), 4, 8),
        ("S0-cell P1 B1", Scenario::S0.batch_spec(9001, SEED), 1, 1),
    ]
}

#[test]
fn k0_observer_2x2_h0() {
    for (name, spec, p, b) in smoke_inputs() {
        let cfg = H0Config {
            p,
            b,
            cancel_milestone: None,
        };
        let direct = |cfg: H0Config, spec: BatchSpec, sink: &ObserverSink| {
            h0::run_session(cfg, spec, sink)
                .map(|_| ())
                .map_err(|e| format!("{e:?}"))
        };
        let quadrants = [
            run_quadrant::<H0Executor>(false, false, cfg, &spec, direct),
            run_quadrant::<H0Executor>(false, true, cfg, &spec, direct),
            run_quadrant::<H0Executor>(true, false, cfg, &spec, direct),
            run_quadrant::<H0Executor>(true, true, cfg, &spec, direct),
        ];
        assert_quadrant_equivalence(&format!("H0 [{name}]"), &spec, quadrants);
    }
}

#[test]
fn k0_observer_2x2_h1() {
    for (name, spec, p, b) in smoke_inputs() {
        let cfg = H1Config {
            p,
            b,
            cancel_milestone: None,
        };
        let direct = |cfg: H1Config, spec: BatchSpec, sink: &ObserverSink| {
            wrk_scan::h1::run_session_h1(cfg, spec, sink)
                .map(|_| ())
                .map_err(|e| format!("{e:?}"))
        };
        let quadrants = [
            run_quadrant::<H1Executor>(false, false, cfg, &spec, direct),
            run_quadrant::<H1Executor>(false, true, cfg, &spec, direct),
            run_quadrant::<H1Executor>(true, false, cfg, &spec, direct),
            run_quadrant::<H1Executor>(true, true, cfg, &spec, direct),
        ];
        assert_quadrant_equivalence(&format!("H1 [{name}]"), &spec, quadrants);
    }
}

// Keep the trait-surface import honest: the witness asserts the shared
// watch face is what the K0 wrapper consumes for both mechanisms.
#[test]
fn settlement_watch_face_is_the_shared_surface() {
    fn takes_watch<W: SettlementWatchFace>(_w: &W) {}
    let spec = BatchSpec::new(1, SEED, 1);
    let sink = ObserverSink::off(spec.batch_id);
    let mut ex = H1Executor::start(
        H1Config {
            p: 1,
            b: 1,
            cancel_milestone: None,
        },
        spec,
        sink,
    );
    takes_watch(&ex.settlement_watch());
    let _ = ex.abort();
}
