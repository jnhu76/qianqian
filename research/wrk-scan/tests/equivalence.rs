//! Phase 7 — ON/OFF equivalence and the independent OFF shadow audit
//! (Gate 授权 §2: same binary, same business semantics; SPEC §7.5).
//!
//! 1. `no_cancel_cells_on_off_equivalent`: for two representative
//!    no-cancel cells the K0-ON ledger face and the K0-OFF receipt face
//!    must be EXACTLY equal (per-item terminal classes, result digest,
//!    fan-in aggregate) — and equal to the independently computed REF.
//! 2. `off_receipt_survives_shadow_audit`: the OFF receipt is audited
//!    per item against REF without ever consulting the ON ledger: every
//!    admitted item reached exactly one terminal, every Completed item
//!    carries its REF contribution, the wait obligation is present
//!    exactly where the workload demands it (wait_done_count — the
//!    OFF-retained anti-optimization field), ordinals are monotone, and
//!    the aggregate re-folds. S5's OFF receipt additionally proves the
//!    legal-set discipline (abandons only inside the cancelled group).

use std::collections::{HashMap, HashSet};

use sha2::{Digest, Sha256};

use wrk_scan::h0::{self, H0Config};
use wrk_scan::k0_session::K0ScanSession;
use wrk_scan::ledger::TerminalKind;
use wrk_scan::observer::ObserverSink;
use wrk_scan::scenario::Scenario;
use wrk_scan::workload::{self, BatchSpec};

const SEED: u64 = 0x00C0_FFEE_0304_0001;

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn result_digest(results: &[(u32, u64)]) -> String {
    let mut h = Sha256::new();
    for (id, c) in results {
        h.update(id.to_le_bytes());
        h.update(c.to_le_bytes());
    }
    format!("sha256:{}", hex(&h.finalize()))
}

/// REF contributions per item (oracle side — never called by executor
/// code, SPEC §2.5(b)).
fn ref_contributions(spec: &BatchSpec) -> HashMap<u32, u64> {
    spec.items
        .iter()
        .map(|i| {
            (
                i.item_id,
                workload::ref_result(spec.seed, i).aggregation_contribution,
            )
        })
        .collect()
}

fn off_face(
    spec: &BatchSpec,
    cell: (usize, usize),
) -> (Vec<(u32, u64)>, u64, HashMap<u32, TerminalKind>) {
    let sink = ObserverSink::off(spec.batch_id);
    let stats = h0::run_session(
        H0Config {
            p: cell.0,
            b: cell.1,
            cancel_milestone: None,
        },
        spec.clone(),
        &sink,
    )
    .expect("OFF settlement");
    let receipt = sink.off_snapshot().unwrap();
    let mut results = Vec::new();
    let mut classes = HashMap::new();
    for (id, it) in &receipt.items {
        let term = it.terminal.expect("every admitted item reaches a terminal");
        classes.insert(*id, term);
        if term != TerminalKind::Abandoned {
            results.push((
                *id,
                it.result_contribution.expect("completed has contribution"),
            ));
        }
    }
    results.sort_by_key(|(id, _)| *id);
    let agg = receipt.aggregate.expect("aggregate published");
    assert_eq!(stats.workers_joined, cell.0);
    (results, agg, classes)
}

fn on_face(
    spec: &BatchSpec,
    cell: (usize, usize),
) -> (Vec<(u32, u64)>, u64, HashMap<u32, TerminalKind>) {
    use qianqian_composition::DisposeVerdict;
    use wrk_scan::ledger::LogKind;

    let sink = ObserverSink::on(spec.batch_id);
    let mut session = K0ScanSession::compose(
        H0Config {
            p: cell.0,
            b: cell.1,
            cancel_milestone: None,
        },
        spec.clone(),
        sink.clone(),
    )
    .expect("K0 compose");
    session
        .run_admission_and_settlement()
        .expect("ON settlement");
    let (verdict, stats) = session.dispose();
    assert_eq!(verdict, DisposeVerdict::Discharged);
    assert_eq!(stats.expect("stats").workers_joined, cell.0);

    let mut results = Vec::new();
    let mut classes = HashMap::new();
    for r in sink.ledger().unwrap().snapshot() {
        let Some(item) = r.item_id else { continue };
        let Some(term) = r.terminal else { continue };
        if r.log != LogKind::OwnerAck {
            continue;
        }
        classes.insert(item, term);
        if term != TerminalKind::Abandoned {
            results.push((item, r.payload));
        }
    }
    results.sort_by_key(|(id, _)| *id);
    let records = sink.ledger().unwrap().snapshot();
    let agg = records
        .iter()
        .find(|r| r.kind == wrk_scan::ledger::EffectKind::AggregatePublished)
        .map(|r| r.payload)
        .expect("aggregate published");
    (results, agg, classes)
}

#[test]
fn no_cancel_cells_on_off_equivalent() {
    for (scenario, cell) in [(Scenario::S3, (4usize, 8usize)), (Scenario::S0, (1, 1))] {
        let spec = scenario.batch_spec(7000 + cell.0 as u64, SEED);
        let refs = ref_contributions(&spec);
        let full: Vec<(u32, u64)> = {
            let mut v: Vec<(u32, u64)> = refs.iter().map(|(k, c)| (*k, *c)).collect();
            v.sort_by_key(|(id, _)| *id);
            v
        };

        let (on_results, on_agg, on_classes) = on_face(&spec, cell);
        let (off_results, off_agg, off_classes) = off_face(&spec, cell);

        // ON/OFF EXACT equality (same binary, same business semantics).
        assert_eq!(on_results, off_results, "{scenario:?}: result sets differ");
        assert_eq!(on_agg, off_agg, "{scenario:?}: aggregates differ");
        assert_eq!(
            on_classes, off_classes,
            "{scenario:?}: terminal classes differ"
        );

        // Both equal the independent REF.
        assert_eq!(result_digest(&on_results), result_digest(&full));
        assert_eq!(on_agg, workload::aggregate(&full));
        // Fault items complete Err, everything else Ok — no abandons.
        for (id, term) in &on_classes {
            let want = if spec.items[*id as usize].fault.is_some() {
                TerminalKind::CompletedErr(spec.items[*id as usize].fault.unwrap())
            } else {
                TerminalKind::CompletedOk
            };
            assert_eq!(*term, want, "{scenario:?} item {id} class");
        }
    }
}

#[test]
fn off_receipt_survives_shadow_audit() {
    // S1: waits on 4/5 of the batch — the wait-obligation audit bites.
    let scenario = Scenario::S1;
    let spec = scenario.batch_spec(7100, SEED);
    let refs = ref_contributions(&spec);
    let sink = ObserverSink::off(spec.batch_id);
    let stats = h0::run_session(
        H0Config {
            p: 4,
            b: 8,
            cancel_milestone: None,
        },
        spec.clone(),
        &sink,
    )
    .expect("settlement");
    let receipt = sink.off_snapshot().unwrap();

    assert_eq!(
        receipt.items.len(),
        spec.items.len(),
        "no-cancel: all N admitted"
    );
    assert!(receipt.cancels.is_empty());
    assert_eq!(receipt.workers_spawned, 4);
    let exits: HashSet<u64> = receipt.worker_exits.iter().copied().collect();
    assert_eq!(exits, HashSet::from([1, 2, 3, 4]));
    assert!(receipt.quiescence_recorded, "quiescence recorded");

    let mut admitted_ordinals = Vec::new();
    let mut terminal_ordinals = Vec::new();
    let mut results = Vec::new();
    for (id, item) in &receipt.items {
        let s = &spec.items[*id as usize];
        let term = item
            .terminal
            .unwrap_or_else(|| panic!("item {id} never terminal"));
        // Contribution pinning vs REF (per item, not just digest-level).
        assert_eq!(
            item.result_contribution,
            Some(refs[id]),
            "item {id} contribution != REF"
        );
        // Terminal class matches the spec fault lane.
        let want = match s.fault {
            Some(code) => TerminalKind::CompletedErr(code),
            None => TerminalKind::CompletedOk,
        };
        assert_eq!(term, want, "item {id} class");
        // Wait obligation: present exactly where the workload demands it.
        let want_waits = u32::from(s.wait_us > 0);
        assert_eq!(
            item.wait_done_count, want_waits,
            "item {id} wait obligation"
        );
        // Timestamp + ordinal discipline.
        assert!(item.start_ts.is_some(), "item {id} never started");
        assert!(
            item.terminal_ordinal.unwrap() > item.admitted_ordinal,
            "item {id} terminal before admission"
        );
        assert!(
            (1..=4).contains(&item.owner),
            "item {id} owner {}",
            item.owner
        );
        admitted_ordinals.push(item.admitted_ordinal);
        terminal_ordinals.push(item.terminal_ordinal.unwrap());
        results.push((*id, item.result_contribution.unwrap()));
    }
    // Ordinals are unique and admission/terminal sequences are monotone
    // in themselves (assigned under the receipt lock).
    admitted_ordinals.sort_unstable();
    terminal_ordinals.sort_unstable();
    let ord_sorted = |v: &[u64]| v.windows(2).all(|w| w[0] < w[1]);
    assert!(
        ord_sorted(&admitted_ordinals),
        "admission ordinals not monotone"
    );
    assert!(
        ord_sorted(&terminal_ordinals),
        "terminal ordinals not monotone"
    );

    results.sort_by_key(|(id, _)| *id);
    assert_eq!(receipt.aggregate, Some(workload::aggregate(&results)));
    assert_eq!(stats.admitted_count, spec.items.len());

    // S5 OFF: legal-set discipline on the minimal receipt.
    let s5 = Scenario::S5;
    let spec5 = s5.batch_spec(7101, SEED);
    let refs5 = ref_contributions(&spec5);
    let sink5 = ObserverSink::off(spec5.batch_id);
    h0::run_session(
        H0Config {
            p: 4,
            b: 8,
            cancel_milestone: s5.cancel_milestone(),
        },
        spec5.clone(),
        &sink5,
    )
    .expect("S5 settlement");
    let receipt5 = sink5.off_snapshot().unwrap();
    assert_eq!(receipt5.cancels.len(), 1);
    assert_eq!(receipt5.cancels[0].0, 7);
    assert!(
        receipt5.items.len() < spec5.items.len(),
        "the cut must bite"
    );
    let mut results5 = Vec::new();
    for (id, item) in &receipt5.items {
        match item.terminal.expect("admitted item terminal") {
            TerminalKind::Abandoned => {
                assert_eq!(item.group, 7, "item {id} abandoned outside group 7");
                assert_eq!(item.result_contribution, None);
                assert_eq!(item.wait_done_count, 0, "abandoned item never executed");
            }
            term => {
                assert_eq!(
                    item.result_contribution,
                    Some(refs5[id]),
                    "item {id} vs REF"
                );
                assert_eq!(item.wait_done_count, 1, "item {id} skipped its wait");
                assert_eq!(
                    term,
                    match spec5.items[*id as usize].fault {
                        Some(c) => TerminalKind::CompletedErr(c),
                        None => TerminalKind::CompletedOk,
                    }
                );
                results5.push((*id, item.result_contribution.unwrap()));
            }
        }
    }
    results5.sort_by_key(|(id, _)| *id);
    assert_eq!(receipt5.aggregate, Some(workload::aggregate(&results5)));
    assert!(receipt5.quiescence_recorded);
}
