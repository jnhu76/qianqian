//! The independent oracle (SPEC §6.2 O1–O10, APP-B `oracle.rs`, file
//! SHA256 3cfe25b2…).
//!
//! Consumes ONLY: (a) the frozen BatchSpec (offered items + injected
//! faults/groups), (b) the dual-ledger snapshot. It never reads
//! executor-internal bookkeeping: the admitted set is reconstructed from
//! ItemAdmitted records, expected content from REF. All ordering judgements
//! use ledger sequence numbers (the serialization boundary), never
//! wall-clock, never queue-emptiness, never timeouts.
//!
//! PROVENANCE (FREEZE-AUTHORITY-MAP §7.1): reused from the frozen APP-B
//! source. The single adaptation is that the O2 expected-wait value reads
//! the item's *effective* `wait_us` (SPEC §3.1 scenario override) instead
//! of the class default — identical behavior for the probe batch.

use crate::ledger::{EffectKind, Ledger, LedgerRecord, LogKind, TerminalKind};
use crate::workload::{aggregate, ref_result, BatchSpec, ResultClass};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckResult {
    pub check: &'static str,
    pub pass: bool,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct OracleVerdict {
    pub checks: Vec<CheckResult>,
}

impl OracleVerdict {
    pub fn green(&self) -> bool {
        self.checks.iter().all(|c| c.pass)
    }

    pub fn failed(&self) -> Vec<&CheckResult> {
        self.checks.iter().filter(|c| !c.pass).collect()
    }
}

fn fail(check: &'static str, detail: String) -> CheckResult {
    CheckResult {
        check,
        pass: false,
        detail,
    }
}

fn ok(check: &'static str, detail: String) -> CheckResult {
    CheckResult {
        check,
        pass: true,
        detail,
    }
}

/// Adjudicate one batch run. `records` must be the full ledger snapshot.
pub fn adjudicate(spec: &BatchSpec, ledger: &Ledger) -> OracleVerdict {
    let records = ledger.snapshot();
    let mut checks = Vec::new();

    // ---- Reconstruct per-item views from the ledger ----
    struct ItemView {
        admitted_seq: u64,
        admit_count: usize,
        wait_done: Vec<(u64, u64)>,               // (seq, payload=wait_us)
        transform_done: Vec<(u64, u64)>,          // (seq, payload)
        result_published: Vec<(u64, u64, u64)>,   // (seq, payload, owner)
        acks: Vec<(u64, TerminalKind, u64, u64)>, // (seq, kind, digest, owner)
    }
    impl ItemView {
        fn new() -> Self {
            ItemView {
                admitted_seq: 0,
                admit_count: 0,
                wait_done: Vec::new(),
                transform_done: Vec::new(),
                result_published: Vec::new(),
                acks: Vec::new(),
            }
        }
    }
    let mut items: HashMap<u32, ItemView> = HashMap::new();
    let mut cancel_group_seq: HashMap<u32, u64> = HashMap::new(); // group -> first cancel seq
    let mut spawned: Vec<(u64, u64)> = Vec::new(); // (seq, executor id)
    let mut exited: HashMap<u64, u64> = HashMap::new(); // executor id -> seq
    let mut quiescence: Option<u64> = None;
    let mut batch_ids: HashSet<u64> = HashSet::new();

    for r in &records {
        batch_ids.insert(r.batch_id);
        match (r.log, r.kind) {
            (_, EffectKind::ItemAdmitted) => {
                let it = r.item_id.expect("admission must name an item");
                let v = items.entry(it).or_insert_with(ItemView::new);
                v.admit_count += 1;
                // keep the FIRST admission seq; duplicates flagged in O2
                if v.admitted_seq == 0 {
                    v.admitted_seq = r.seq;
                }
            }
            (_, EffectKind::IoWaitDone) => items
                .entry(r.item_id.expect("wait must name an item"))
                .or_insert_with(ItemView::new)
                .wait_done
                .push((r.seq, r.payload)),
            (_, EffectKind::TransformDone) => items
                .entry(r.item_id.expect("transform must name an item"))
                .or_insert_with(ItemView::new)
                .transform_done
                .push((r.seq, r.payload)),
            (LogKind::Effect, EffectKind::ResultPublished) => items
                .entry(r.item_id.expect("result must name an item"))
                .or_insert_with(ItemView::new)
                .result_published
                .push((r.seq, r.payload, r.owner)),
            (LogKind::OwnerAck, _) => items
                .entry(r.item_id.expect("ack must name an item"))
                .or_insert_with(ItemView::new)
                .acks
                .push((
                    r.seq,
                    r.terminal.expect("ack carries terminal"),
                    r.payload,
                    r.owner,
                )),
            (_, EffectKind::CancelRequested) => {
                cancel_group_seq.entry(r.payload as u32).or_insert(r.seq);
            }
            (_, EffectKind::ExecutorSpawned) => spawned.push((r.seq, r.payload)),
            (_, EffectKind::ExecutorExited) => {
                exited.entry(r.payload).or_insert(r.seq);
            }
            (_, EffectKind::QuiescenceReceipt) => quiescence = Some(r.seq),
            (_, EffectKind::AggregatePublished) => { /* consumed below */ }
        }
    }

    let offered: HashSet<u32> = spec.items.iter().map(|i| i.item_id).collect();

    // O1 — admission/terminal coverage: every admitted item exactly one
    // terminal ACK; no ACK for a non-admitted or non-offered item.
    {
        let mut missing: Vec<u32> = Vec::new();
        let mut dup: Vec<u32> = Vec::new();
        let mut unadmitted: Vec<u32> = Vec::new();
        for (id, v) in &items {
            match v.acks.len() {
                0 => {
                    if v.admitted_seq != 0 {
                        missing.push(*id)
                    }
                }
                1 => {
                    if v.admitted_seq == 0 {
                        unadmitted.push(*id)
                    }
                }
                _ => dup.push(*id),
            }
        }
        let ghost: Vec<u32> = items
            .keys()
            .filter(|id| !offered.contains(*id))
            .copied()
            .collect();
        if missing.is_empty() && dup.is_empty() && unadmitted.is_empty() && ghost.is_empty() {
            checks.push(ok(
                "O1_terminal_coverage",
                format!("{} admitted items, each exactly one ACK", items.len()),
            ));
        } else {
            let mut d = Vec::new();
            if !missing.is_empty() {
                d.push(format!("admitted-but-no-terminal: {:?}", missing));
            }
            if !dup.is_empty() {
                d.push(format!("duplicate-terminal: {:?}", dup));
            }
            if !unadmitted.is_empty() {
                d.push(format!("terminal-without-admission: {:?}", unadmitted));
            }
            if !ghost.is_empty() {
                d.push(format!("ghost-items-not-offered: {:?}", ghost));
            }
            checks.push(fail("O1_terminal_coverage", d.join("; ")));
        }
    }

    // O2 — exactly-once protected effects, INCLUDING the wait dimension:
    // every non-abandoned item must show exactly (wait_us > 0 ? 1 : 0)
    // IoWaitDone effects with payload == wait_us (a mechanism that skips
    // the wait entirely or shortens it is detected here); items with any
    // protected effect must be admitted (ghost-effect check); admission
    // itself at most once.
    {
        let mut bad: Vec<String> = Vec::new();
        let spec_of: HashMap<u32, _> = spec.items.iter().map(|s| (s.item_id, s)).collect();
        for (id, v) in &items {
            if v.admit_count > 1 {
                bad.push(format!("item {id}: admitted {} times", v.admit_count));
            }
            // Ghost effects: protected work on an item with no admission.
            if v.admitted_seq == 0
                && (!v.wait_done.is_empty()
                    || !v.transform_done.is_empty()
                    || !v.result_published.is_empty())
            {
                bad.push(format!(
                    "item {id}: protected effects without admission (ghost work)"
                ));
            }
            let t = match v.acks.first() {
                Some((_, t, _, _)) => *t,
                None => continue, // already O1-red
            };
            let expect_wait = spec_of.get(id).map(|s| s.wait_us).unwrap_or(0);
            if t != TerminalKind::Abandoned {
                let expect_waits = usize::from(expect_wait > 0);
                if v.wait_done.len() != expect_waits {
                    bad.push(format!(
                        "item {id}: {} IoWaitDone effects (expect {expect_waits} for wait_us={expect_wait})",
                        v.wait_done.len()
                    ));
                } else if expect_wait > 0 && v.wait_done[0].1 != expect_wait {
                    bad.push(format!(
                        "item {id}: IoWaitDone payload {} != wait_us {expect_wait}",
                        v.wait_done[0].1
                    ));
                }
                if v.transform_done.len() != 1 {
                    bad.push(format!(
                        "item {id}: {} TransformDone effects (expect 1)",
                        v.transform_done.len()
                    ));
                }
                if v.result_published.len() != 1 {
                    bad.push(format!(
                        "item {id}: {} ResultPublished effects (expect 1)",
                        v.result_published.len()
                    ));
                }
            } else if v.wait_done.len() > 1 {
                bad.push(format!(
                    "item {id}: {} IoWaitDone effects on abandoned item",
                    v.wait_done.len()
                ));
            }
        }
        if bad.is_empty() {
            checks.push(ok(
                "O2_exactly_once_effects",
                "wait/transform/result exactly as specified per item".into(),
            ));
        } else {
            checks.push(fail("O2_exactly_once_effects", bad.join("; ")));
        }
    }

    // O3 — content correctness vs REF + aggregate determinism.
    {
        let mut bad: Vec<String> = Vec::new();
        let refs: HashMap<u32, _> = spec
            .items
            .iter()
            .map(|s| (s.item_id, ref_result(spec.seed, s)))
            .collect();
        for s in &spec.items {
            let id = s.item_id;
            let r = &refs[&id];
            let v = match items.get(&id) {
                Some(v) if v.acks.len() == 1 => v,
                _ => continue, // O1 territory
            };
            let (_, term, _, ack_owner) = &v.acks[0];
            // Owner-authority consistency: the terminal ACK for a
            // Completed* item must be written by the same owner that
            // published its result effect — otherwise "terminal by the
            // real owner" is an unverified honor system.
            if term != &TerminalKind::Abandoned {
                if let Some((_, _, publisher)) = v.result_published.first() {
                    if publisher != ack_owner {
                        bad.push(format!(
                            "item {id}: terminal ACK owner {ack_owner} != ResultPublished owner {publisher}"
                        ));
                    }
                }
            }
            match term {
                TerminalKind::CompletedOk => {
                    if r.result_class != ResultClass::CompletedOk {
                        bad.push(format!(
                            "item {id}: CompletedOk but REF says {:?}",
                            r.result_class
                        ));
                    } else {
                        match v.result_published.first() {
                            Some((_, payload, _)) if *payload == r.aggregation_contribution => {}
                            _ => bad.push(format!(
                                "item {id}: ResultPublished payload != REF contribution"
                            )),
                        }
                        let expect_meta = crate::oracle::fold_meta(&r.metadata_digest);
                        match v.transform_done.first() {
                            Some((_, payload)) if *payload == expect_meta => {}
                            _ => bad.push(format!(
                                "item {id}: TransformDone payload != REF metadata fold"
                            )),
                        }
                    }
                }
                TerminalKind::CompletedErr(code) => {
                    if r.result_class != ResultClass::CompletedErr(*code) {
                        bad.push(format!(
                            "item {id}: CompletedErr({code:#x}) mismatches REF {:?}",
                            r.result_class
                        ));
                    } else {
                        // Errored items still do the full shared work
                        // (wait + transform) before failing logically:
                        // payloads REF-pinned exactly like Ok items — a
                        // fabricated error contribution cannot hide in
                        // the error lane.
                        match v.result_published.first() {
                            Some((_, payload, _)) if *payload == r.aggregation_contribution => {}
                            _ => bad.push(format!(
                                "item {id}: Err ResultPublished payload != REF contribution"
                            )),
                        }
                        let expect_meta = crate::oracle::fold_meta(&r.metadata_digest);
                        match v.transform_done.first() {
                            Some((_, payload)) if *payload == expect_meta => {}
                            _ => bad.push(format!(
                                "item {id}: Err TransformDone payload != REF metadata fold"
                            )),
                        }
                    }
                }
                TerminalKind::Abandoned => {
                    if !v.result_published.is_empty() {
                        bad.push(format!("item {id}: Abandoned but ResultPublished present"));
                    }
                    // abandoned items contribute nothing to the published
                    // aggregate (membership is schedule-dependent; the
                    // cancel-arm aggregate is consistency-checked below)
                }
            }
        }
        // Aggregate check. Frozen aggregate policy (v0):
        //  - No cancellation in the batch: the aggregate MUST equal the
        //    REF fold over ALL offered items (fully deterministic).
        //  - Cancellation occurred: the completed set is schedule-dependent
        //    by design (in-flight items may legally complete or abandon);
        //    the oracle recomputes the fold from the ledger's own
        //    ResultPublished payloads in item_id order and requires the
        //    published aggregate to equal that fold (loss/dup/order errors
        //    still detected; each member value is still REF-pinned).
        let cancel_happened = !cancel_group_seq.is_empty();
        let expect_agg = if cancel_happened {
            let pairs: Vec<(u32, u64)> = items
                .iter()
                .filter(|(_, v)| {
                    v.acks.len() == 1
                        && v.acks[0].1 != TerminalKind::Abandoned
                        && v.result_published.len() == 1
                })
                .map(|(id, v)| (*id, v.result_published[0].1))
                .collect();
            aggregate(&pairs)
        } else {
            aggregate(
                &spec
                    .items
                    .iter()
                    .map(|s| (s.item_id, refs[&s.item_id].aggregation_contribution))
                    .collect::<Vec<_>>(),
            )
        };
        let agg_records: Vec<&LedgerRecord> = records
            .iter()
            .filter(|r| r.kind == EffectKind::AggregatePublished && r.batch_id == spec.batch_id)
            .collect();
        if agg_records.is_empty() {
            bad.push("no AggregatePublished for batch".into());
        }
        for a in &agg_records {
            if a.payload != expect_agg {
                bad.push(format!(
                    "aggregate digest {} != expected {}",
                    a.payload, expect_agg
                ));
            }
        }
        if bad.is_empty() {
            checks.push(ok(
                "O3_content_vs_ref",
                "per-item classes/digests and aggregate match REF".into(),
            ));
        } else {
            checks.push(fail("O3_content_vs_ref", bad.join("; ")));
        }
    }

    // O4 — late effect per item: no effect for item X after X's terminal
    // ACK (checks the INDIVIDUAL item boundary, not just global quiet).
    {
        let mut bad: Vec<String> = Vec::new();
        for (id, v) in &items {
            if let Some((ack_seq, _, _, _)) = v.acks.first() {
                let late: Vec<u64> = v
                    .wait_done
                    .iter()
                    .map(|(s, _)| s)
                    .chain(v.transform_done.iter().map(|(s, _)| s))
                    .chain(v.result_published.iter().map(|(s, _, _)| s))
                    .filter(|s| **s > *ack_seq)
                    .copied()
                    .collect();
                if !late.is_empty() {
                    bad.push(format!(
                        "item {id}: effects at seq {late:?} after terminal ack at {ack_seq}"
                    ));
                }
            }
        }
        if bad.is_empty() {
            checks.push(ok(
                "O4_no_effect_after_item_terminal",
                "no per-item late effects".into(),
            ));
        } else {
            checks.push(fail("O4_no_effect_after_item_terminal", bad.join("; ")));
        }
    }

    // O5 — Abandoned legitimacy: an Abandoned ACK is legal only if a
    // CancelRequested for that item's group precedes the ACK in ledger
    // order, and the item produced no result; anything else is premature
    // or forged abandonment. Harness never writes terminals itself.
    {
        let mut bad: Vec<String> = Vec::new();
        for s in &spec.items {
            let id = s.item_id;
            let v = match items.get(&id) {
                Some(v) if v.acks.len() == 1 => v,
                _ => continue,
            };
            if let (ack_seq, TerminalKind::Abandoned, _, owner) = &v.acks[0] {
                let cancel_first = cancel_group_seq.get(&s.cancellation_group);
                match cancel_first {
                    Some(cs) if *cs < *ack_seq => { /* legal */ }
                    Some(_) => bad.push(format!("item {id}: Abandoned ack seq {ack_seq} but cancel for group {} recorded later", s.cancellation_group)),
                    None => bad.push(format!("item {id}: Abandoned (owner {owner}) with NO cancel recorded for group {}", s.cancellation_group)),
                }
                if !v.result_published.is_empty() {
                    bad.push(format!("item {id}: Abandoned after ResultPublished"));
                }
            }
        }
        if bad.is_empty() {
            checks.push(ok(
                "O5_abandon_legitimacy",
                "every Abandoned preceded by group cancel, no results".into(),
            ));
        } else {
            checks.push(fail("O5_abandon_legitimacy", bad.join("; ")));
        }
    }

    // O6 — executor join/quiescence: every spawned executor has an
    // ExecutorExited BEFORE the quiescence receipt; a receipt must exist
    // and be the LAST record in the ledger.
    {
        let mut bad: Vec<String> = Vec::new();
        match quiescence {
            None => bad.push("no QuiescenceReceipt".into()),
            Some(qs) => {
                for (spawn_seq, exec) in &spawned {
                    match exited.get(exec) {
                        Some(x) if *x < qs => {}
                        Some(x) => bad.push(format!("executor {exec} exited at {x} AFTER receipt {qs}")),
                        None => bad.push(format!("executor {exec} spawned at {spawn_seq} never recorded exit before receipt {qs}")),
                    }
                }
                if let Some(last) = records.last() {
                    if last.kind != EffectKind::QuiescenceReceipt {
                        bad.push(format!(
                            "receipt at {qs} is not the last record (last: {:?} at {})",
                            last.kind, last.seq
                        ));
                    }
                }
            }
        }
        if bad.is_empty() {
            checks.push(ok(
                "O6_executors_joined_before_quiescence",
                format!("{} executors exited before receipt", spawned.len()),
            ));
        } else {
            checks.push(fail(
                "O6_executors_joined_before_quiescence",
                bad.join("; "),
            ));
        }
    }

    // O7 — post-cancel admission: no ItemAdmitted for a group's item
    // after that group's CancelRequested record.
    {
        let mut bad: Vec<String> = Vec::new();
        for s in &spec.items {
            let v = match items.get(&s.item_id) {
                Some(v) if v.admitted_seq != 0 => v,
                _ => continue,
            };
            if let Some(cs) = cancel_group_seq.get(&s.cancellation_group) {
                if v.admitted_seq > *cs {
                    bad.push(format!(
                        "item {} (group {}) admitted at {} after cancel at {}",
                        s.item_id, s.cancellation_group, v.admitted_seq, cs
                    ));
                }
            }
        }
        if bad.is_empty() {
            checks.push(ok(
                "O7_no_admission_after_cancel",
                "no post-cancel admissions".into(),
            ));
        } else {
            checks.push(fail("O7_no_admission_after_cancel", bad.join("; ")));
        }
    }

    // O8 — batch isolation: every record carries the batch id; records
    // from other batches in this ledger are pollution.
    {
        let foreign: Vec<u64> = batch_ids
            .iter()
            .copied()
            .filter(|b| *b != spec.batch_id)
            .collect();
        if foreign.is_empty() {
            checks.push(ok(
                "O8_batch_isolation",
                "all records belong to the batch".into(),
            ));
        } else {
            checks.push(fail(
                "O8_batch_isolation",
                format!("foreign batch ids: {foreign:?}"),
            ));
        }
    }

    // O9 — fan-in completeness: AggregatePublished only after ALL
    // ADMITTED items of the batch have their (single) terminal ACK.
    // (Cut — never-admitted — items carry no terminal obligation.)
    {
        let mut bad: Vec<String> = Vec::new();
        for a in records
            .iter()
            .filter(|r| r.kind == EffectKind::AggregatePublished && r.batch_id == spec.batch_id)
        {
            for (id, v) in &items {
                if v.admitted_seq == 0 {
                    continue;
                }
                let ok_terminal = v.acks.len() == 1 && v.acks[0].0 < a.seq;
                if !ok_terminal {
                    bad.push(format!("aggregate at {} before item {id} terminal", a.seq));
                }
            }
        }
        if bad.is_empty() {
            checks.push(ok(
                "O9_fanin_after_all_terminals",
                "aggregate published only after full settlement".into(),
            ));
        } else {
            checks.push(fail("O9_fanin_after_all_terminals", bad.join("; ")));
        }
    }

    // O10 — global late effects: nothing at all after the quiescence
    // receipt (distinct from O4's per-item boundary).
    {
        match quiescence {
            None => checks.push(fail("O10_no_effect_after_quiescence", "no receipt".into())),
            Some(qs) => {
                let late: Vec<&LedgerRecord> = records.iter().filter(|r| r.seq > qs).collect();
                if late.is_empty() {
                    checks.push(ok(
                        "O10_no_effect_after_quiescence",
                        "ledger ends at receipt".into(),
                    ));
                } else {
                    let d = late
                        .iter()
                        .map(|r| format!("{:?}@{}", r.kind, r.seq))
                        .collect::<Vec<_>>()
                        .join(",");
                    checks.push(fail(
                        "O10_no_effect_after_quiescence",
                        format!("records after receipt: {d}"),
                    ));
                }
            }
        }
    }

    OracleVerdict { checks }
}

/// Fold a 32-byte REF metadata digest into the u64 payload that the
/// executor's TransformDone record must carry (APP-B, frozen).
pub fn fold_meta(meta: &[u8; 32]) -> u64 {
    let mut x = u64::from_le_bytes(meta[0..8].try_into().unwrap());
    x ^= u64::from_le_bytes(meta[8..16].try_into().unwrap()).rotate_left(17);
    x ^= u64::from_le_bytes(meta[16..24].try_into().unwrap()).rotate_left(43);
    x ^ u64::from_le_bytes(meta[24..32].try_into().unwrap()).rotate_left(59)
}
