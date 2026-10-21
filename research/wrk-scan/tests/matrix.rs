//! Phase 5 — the frozen 92-cell non-timed correctness matrix
//! (Gate 授权 §2.5: 46 frozen scenario×P×B cells × K0{ON,OFF}).
//!
//! Per cell, BOTH arms run in this one binary over the same
//! WorkloadCore/H0 executor code path:
//! - K0 ON  : the session is composed on the real CompositionKernel
//!   ([`K0ScanSession`]); `DisposeVerdict::Discharged` is required.
//! - K0 OFF : the same executor driven directly ([`run_session`]).
//!
//! Verdict per arm (recorded in `evidence/gate-i-matrix/cells.jsonl`):
//! - no-cancel cells: exact equality with the independently computed REF —
//!   every item Completed{Ok,Err} with its REF contribution, the fan-in
//!   aggregate equal to `workload::aggregate(REF contributions)`, and the
//!   full B/P/quiescence/join discipline.
//! - S5 (cancel) cells: the ON arm is adjudicated by the frozen O1–O10
//!   oracle; both arms additionally get an independent per-item legal-set
//!   audit: Completed items must carry exactly their REF contribution,
//!   Abandoned items must belong to a cancelled group, the published
//!   aggregate must equal the re-fold of the completed set, and the
//!   cancel cut must exist. (ON↔OFF equality is NOT required for S5:
//!   which cancelled items Abandon vs complete is legal interleaving
//!   nondeterminism, and evidence must not pretend otherwise.)
//!
//! This test is harness/oracle-side code: it computes REF via
//! `workload::ref_result`, which executor code must never call
//! (SPEC §2.5(b)).

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use qianqian_composition::DisposeVerdict;

use wrk_scan::h0::{self, H0Config, SessionStats};
use wrk_scan::k0_session::K0ScanSession;
use wrk_scan::ledger::{EffectKind, LedgerRecord, TerminalKind};
use wrk_scan::observer::{ObserverSink, OffReceipt};
use wrk_scan::oracle;
use wrk_scan::scenario::{Cell, Scenario};
use wrk_scan::workload::{self, BatchSpec};

/// The fixed public seed of Gate I (Gate 授权 §2.5).
const SEED: u64 = 0x00C0_FFEE_0304_0001;
/// This host's physical core count (frozen cell expansion input; the
/// expansion self-checks against the 46-cell table below).
const P_PHYS: usize = 10;

// ---------------------------------------------------------------------------
// Oracle-side reference per scenario
// ---------------------------------------------------------------------------

/// The independently computed REF face of one scenario's batch.
struct ScenarioRef {
    /// REF contribution per item id.
    contributions: BTreeMap<u32, u64>,
    /// REF aggregate over ALL items (the exact no-cancel target).
    full_aggregate: u64,
    /// sha256 over item-ordered (LE4 id || LE8 contribution).
    result_digest: String,
    /// sha256 over item-ordered (LE4 id || content_digest[32]).
    input_digest: String,
}

fn build_ref(spec: &BatchSpec) -> ScenarioRef {
    let mut contributions = BTreeMap::new();
    let mut input_hasher = Sha256::new();
    for item in &spec.items {
        let r = workload::ref_result(spec.seed, item);
        input_hasher.update(item.item_id.to_le_bytes());
        input_hasher.update(r.content_digest);
        contributions.insert(item.item_id, r.aggregation_contribution);
    }
    let full: Vec<(u32, u64)> = contributions.iter().map(|(k, v)| (*k, *v)).collect();
    ScenarioRef {
        result_digest: result_digest(&full),
        input_digest: format!("sha256:{}", sha256_hex(input_hasher.finalize())),
        full_aggregate: workload::aggregate(&full),
        contributions,
    }
}

/// Result identity of a completed set: sha256 over item-ordered
/// (LE4 id || LE8 contribution).
fn result_digest(results: &[(u32, u64)]) -> String {
    let mut h = Sha256::new();
    for (id, c) in results {
        h.update(id.to_le_bytes());
        h.update(c.to_le_bytes());
    }
    format!("sha256:{}", sha256_hex(h.finalize()))
}

fn agg_hex(v: u64) -> String {
    format!("0x{v:016x}")
}

fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    let d = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in d {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// ---------------------------------------------------------------------------
// Per-run extraction (mode-independent face of one executed arm)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RunFace {
    admitted: usize,
    completed_ok: usize,
    completed_err: usize,
    /// (item, contribution) of Completed{Ok,Err} items, item-ordered.
    results: Vec<(u32, u64)>,
    /// Item ids abandoned, with their cancellation group.
    abandoned_groups: Vec<(u32, u32)>,
    cancel_groups: Vec<u32>,
    worker_exits: usize,
    aggregate: Option<u64>,
    /// Receipt column value (the implementation's formal record name).
    quiescence: String,
}

impl RunFace {
    fn abandoned(&self) -> usize {
        self.abandoned_groups.len()
    }
}

fn face_from_ledger(records: &[LedgerRecord]) -> RunFace {
    let mut f = RunFace::default();
    let mut groups: BTreeMap<u32, u32> = BTreeMap::new();
    for r in records {
        match r.kind {
            EffectKind::ItemAdmitted => {
                f.admitted += 1;
                if let Some(item) = r.item_id {
                    groups.insert(item, r.payload as u32);
                }
            }
            EffectKind::CancelRequested => f.cancel_groups.push(r.payload as u32),
            EffectKind::ExecutorExited => f.worker_exits += 1,
            EffectKind::AggregatePublished => f.aggregate = Some(r.payload),
            EffectKind::QuiescenceReceipt => {
                f.quiescence = format!("QuiescenceReceipt@seq={}", r.seq)
            }
            _ => {}
        }
        if let (Some(item), Some(term)) = (r.item_id, r.terminal) {
            match term {
                TerminalKind::CompletedOk => {
                    f.completed_ok += 1;
                    f.results.push((item, r.payload));
                }
                TerminalKind::CompletedErr(_) => {
                    f.completed_err += 1;
                    f.results.push((item, r.payload));
                }
                TerminalKind::Abandoned => {
                    let group = groups.get(&item).copied().unwrap_or(u32::MAX);
                    f.abandoned_groups.push((item, group));
                }
            }
        }
    }
    f.results.sort_by_key(|(id, _)| *id);
    f.abandoned_groups.sort_by_key(|(id, _)| *id);
    f
}

fn face_from_receipt(receipt: &OffReceipt) -> RunFace {
    let mut f = RunFace {
        admitted: receipt.items.len(),
        ..RunFace::default()
    };
    for (item, it) in &receipt.items {
        match it.terminal {
            Some(TerminalKind::CompletedOk) => {
                f.completed_ok += 1;
                f.results
                    .push((*item, it.result_contribution.expect("ok has contribution")));
            }
            Some(TerminalKind::CompletedErr(_)) => {
                f.completed_err += 1;
                f.results
                    .push((*item, it.result_contribution.expect("err has contribution")));
            }
            Some(TerminalKind::Abandoned) => f.abandoned_groups.push((*item, it.group)),
            None => {} // a missing terminal is a verdict failure below
        }
    }
    f.results.sort_by_key(|(id, _)| *id);
    f.cancel_groups = receipt.cancels.iter().map(|(g, _)| *g).collect();
    f.worker_exits = receipt.worker_exits.len();
    f.aggregate = receipt.aggregate;
    f.quiescence = if receipt.quiescence_recorded {
        "OffReceipt.quiescence_recorded=true".into()
    } else {
        "ABSENT".into()
    };
    f
}

/// The independent per-item legal-set audit shared by both arms: every
/// terminal item's contribution must equal REF, abandons must belong to a
/// cancelled group, and the aggregate must be the re-fold of the actual
/// completed set. Returns the failed check names (empty = pass).
fn legal_set_audit(face: &RunFace, r: &ScenarioRef) -> Vec<String> {
    let mut bad = Vec::new();
    for (item, c) in &face.results {
        if r.contributions.get(item) != Some(c) {
            bad.push(format!("contribution[{item}] != REF"));
        }
    }
    for (item, group) in &face.abandoned_groups {
        if !face.cancel_groups.contains(group) {
            bad.push(format!("abandoned[{item}] group {group} not cancelled"));
        }
    }
    match face.aggregate {
        Some(a) if a != workload::aggregate(&face.results) => {
            bad.push("aggregate != re-fold(completed set)".into())
        }
        Some(_) => {}
        None => bad.push("no aggregate published".into()),
    }
    bad
}

// ---------------------------------------------------------------------------
// Cell record
// ---------------------------------------------------------------------------

struct CellRecord {
    cell: Cell,
    k0_on: bool,
    batch_id: u64,
    input_digest: String,
    face: RunFace,
    stats: SessionStats,
    dispose_verdict: Option<DisposeVerdict>,
    oracle: Option<oracle::OracleVerdict>,
    failures: Vec<String>,
}

fn write_record(out: &mut impl Write, rec: &CellRecord) {
    let c = rec.cell;
    let f = &rec.face;
    let verdict = if rec.failures.is_empty() {
        "GREEN".to_string()
    } else {
        format!("RED({})", rec.failures.join("; "))
    };
    let _ = writeln!(
        out,
        concat!(
            r#"{{"ARM":"{}","SCENARIO":"{}","P":{},"B":{},"K0_MODE":"{}","#,
            r#""SEED":"0x00c0ffee03040001","BATCH_ID":{},"INPUT_DIGEST":"{}","#,
            r#""ADMITTED_COUNT":{},"TERMINAL_COUNT":{},"COMPLETED_OK":{},"#,
            r#""COMPLETED_ERR":{},"ABANDONED":{},"RESULT_DIGEST":"{}","#,
            r#""AGGREGATE_DIGEST":"{}","MAX_B_IN_FLIGHT":{},"#,
            r#""MAX_ACTIVE_EXECUTION":{},"WORKER_EXIT_COUNT":{},"JOIN_COUNT":{},"#,
            r#""QUISCENCE_RECEIPT":"{}","K0_DISPOSE_VERDICT":{},"ORACLE":{},"#,
            r#""ORACLE_VERDICT":"{}"}}"#
        ),
        c.arm_id(rec.k0_on),
        c.scenario_label(),
        c.p,
        c.b,
        if rec.k0_on { "ON" } else { "OFF" },
        rec.batch_id,
        rec.input_digest,
        rec.stats.admitted_count,
        f.completed_ok + f.completed_err + f.abandoned(),
        f.completed_ok,
        f.completed_err,
        f.abandoned(),
        result_digest(&f.results),
        f.aggregate.map(agg_hex).unwrap_or_else(|| "ABSENT".into()),
        rec.stats.b_high_water,
        rec.stats.active_execution_high_water,
        f.worker_exits,
        rec.stats.workers_joined,
        f.quiescence,
        match rec.dispose_verdict {
            Some(ref v) => format!("\"{v:?}\""),
            None => "null".into(),
        },
        match rec.oracle {
            Some(ref o) => format!("\"{}\"", if o.green() { "GREEN" } else { "RED" }),
            None => "null".into(),
        },
        verdict,
    );
}

// ---------------------------------------------------------------------------
// The matrix
// ---------------------------------------------------------------------------

fn cfg_of(cell: Cell) -> H0Config {
    H0Config {
        p: cell.p,
        b: cell.b,
        cancel_milestone: cell.scenario.cancel_milestone(),
    }
}

/// One K0-ON execution through the real composition kernel.
fn run_on(
    cell: Cell,
    spec: &BatchSpec,
) -> (RunFace, SessionStats, DisposeVerdict, oracle::OracleVerdict) {
    let sink = ObserverSink::on(spec.batch_id);
    let mut session =
        K0ScanSession::compose(cfg_of(cell), spec.clone(), sink.clone()).expect("K0 compose");
    session
        .run_admission_and_settlement()
        .expect("K0-ON settlement");
    let (verdict, stats) = session.dispose();
    let face = face_from_ledger(&sink.ledger().unwrap().snapshot());
    let oracle_v = oracle::adjudicate(spec, sink.ledger().unwrap());
    (
        face,
        stats.expect("teardown records stats"),
        verdict,
        oracle_v,
    )
}

/// One K0-OFF execution of the same executor, driven directly.
fn run_off(cell: Cell, spec: &BatchSpec) -> (RunFace, SessionStats) {
    let sink = ObserverSink::off(spec.batch_id);
    let stats = h0::run_session(cfg_of(cell), spec.clone(), &sink).expect("K0-OFF settlement");
    (face_from_receipt(&sink.off_snapshot().unwrap()), stats)
}

#[test]
fn matrix_92_cells_all_green() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("evidence/gate-i-matrix");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cells.jsonl");
    // Each run rewrites the receipt: one file = one complete matrix run.
    fs::write(&path, b"").unwrap();
    let mut out = OpenOptions::new().append(true).open(&path).unwrap();

    let cells = Scenario::expand_cells(P_PHYS);
    assert_eq!(cells.len(), 46, "frozen cell expansion on this host");

    // Scenario REF faces (oracle-side; independent of every execution).
    let mut refs = HashMap::new();
    for s in [
        Scenario::S0,
        Scenario::S1,
        Scenario::S2,
        Scenario::S3,
        Scenario::S4,
        Scenario::S5,
        Scenario::S6,
    ] {
        let spec = s.batch_spec(0, SEED);
        refs.insert(s, build_ref(&spec));
    }
    assert_eq!(refs.len(), 7);

    let mut red: Vec<String> = Vec::new();
    for (idx, cell) in cells.iter().enumerate() {
        let spec = cell.scenario.batch_spec(1000 + (idx as u64) * 2, SEED);
        let r = &refs[&cell.scenario];
        let cancelled = cell.scenario.cancel_milestone().is_some();
        let n = spec.items.len();

        for k0_on in [true, false] {
            let batch_id = 1000 + (idx as u64) * 2 + k0_on as u64;
            let spec = BatchSpec {
                batch_id,
                ..spec.clone()
            };
            let mut failures = Vec::new();

            let (face, stats, dispose, oracle_v) = if k0_on {
                let (f, s, d, o) = run_on(*cell, &spec);
                if !o.green() {
                    failures.push(format!("oracle RED: {:?}", o.failed()));
                }
                if d != DisposeVerdict::Discharged {
                    failures.push(format!("dispose {d:?} != Discharged"));
                }
                (f, s, Some(d), Some(o))
            } else {
                let (f, s) = run_off(*cell, &spec);
                (f, s, None, None)
            };

            // Shared B/P/join/quiescence discipline (both modes).
            if stats.b_high_water > cell.b {
                failures.push(format!("B violated: {} > {}", stats.b_high_water, cell.b));
            }
            if stats.active_execution_high_water > cell.p {
                failures.push(format!(
                    "P violated: {} > {}",
                    stats.active_execution_high_water, cell.p
                ));
            }
            if stats.workers_spawned != cell.p || stats.workers_joined != cell.p {
                failures.push(format!(
                    "worker pool {} spawned / {} joined != P {}",
                    stats.workers_spawned, stats.workers_joined, cell.p
                ));
            }
            if face.worker_exits != cell.p {
                failures.push(format!("exits {} != P {}", face.worker_exits, cell.p));
            }
            if face.quiescence == "ABSENT" {
                failures.push("no quiescence receipt".into());
            }

            if cancelled {
                // S5: cut must bite, exactly one cancel of group 7, and
                // the independent per-item legal-set audit.
                if face.cancel_groups != vec![7] {
                    failures.push(format!("cancels {:?} != [7]", face.cancel_groups));
                }
                if stats.admitted_count >= n {
                    failures.push(format!(
                        "cut did not bite: admitted {} >= N {n}",
                        stats.admitted_count
                    ));
                }
                if face.admitted != stats.admitted_count {
                    failures.push(format!(
                        "admitted face {} != stats {}",
                        face.admitted, stats.admitted_count
                    ));
                }
                failures.extend(legal_set_audit(&face, r));
            } else {
                // No-cancel: EXACT equality with REF, both modes.
                if stats.admitted_count != n || face.admitted != n {
                    failures.push(format!(
                        "admitted {}/face {} != N {n}",
                        stats.admitted_count, face.admitted
                    ));
                }
                if face.abandoned() != 0 {
                    failures.push(format!("{} abandons in a no-cancel cell", face.abandoned()));
                }
                if face.completed_ok + face.completed_err != n {
                    failures.push(format!(
                        "terminals {} != N {n}",
                        face.completed_ok + face.completed_err
                    ));
                }
                let rd = result_digest(&face.results);
                if rd != r.result_digest {
                    failures.push("RESULT_DIGEST != REF".into());
                }
                if face.aggregate != Some(r.full_aggregate) {
                    failures.push(format!(
                        "aggregate {:?} != REF {}",
                        face.aggregate,
                        agg_hex(r.full_aggregate)
                    ));
                }
            }

            let rec = CellRecord {
                cell: *cell,
                k0_on,
                batch_id,
                input_digest: r.input_digest.clone(),
                face,
                stats,
                dispose_verdict: dispose,
                oracle: oracle_v,
                failures: failures.clone(),
            };
            write_record(&mut out, &rec);
            if !failures.is_empty() {
                red.push(format!("{}: {}", cell.arm_id(k0_on), failures.join("; ")));
            }
        }
    }
    out.flush().unwrap();

    assert!(
        red.is_empty(),
        "RED cells in the 92-cell matrix ({}):\n  {}",
        red.len(),
        red.join("\n  ")
    );
}
