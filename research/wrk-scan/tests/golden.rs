//! Golden-vector evidence for the single WorkloadCore (Gate I Phase 1,
//! FREEZE-AUTHORITY-MAP §4). Every check here is anchored OUTSIDE the new
//! crate: python-independent SPEC-pseudocode vectors, the frozen
//! EXECUTED `AGG_VALUE` receipt, and a byte-exact frozen-module
//! comparison — never a self-comparison against this crate's own output.

use wrk_scan::oracle::fold_meta;
use wrk_scan::workload::{
    aggregate, content_digest, contribution, generate_input, ref_result, transform, BatchSpec,
    ResultClass, SplitMix64,
};

/// The frozen probe batch (SPEC §2.3): batch_id 1, seed 0x00C0FFEE03040001,
/// N = 48.
const SEED: u64 = 0x00C0_FFEE_0304_0001;

/// Byte-exact frozen APP-A workload.rs, compiled as an independent
/// reference (sha256 f1960872… over the extracted text). The fixture is
/// NEVER lint-fixed: its value is being byte-identical to the frozen
/// source, so the clippy lints the frozen code trips are allowed here.
mod frozen {
    #![allow(dead_code)]
    #![allow(clippy::unnecessary_sort_by, clippy::unnecessary_cast)]
    include!("fixtures/frozen_workload.rs");
}

/// GV-1 — SplitMix64 vectors computed by an independent python
/// implementation of the SPEC §2.2 pseudocode (not by any Rust code).
#[test]
fn gv1_splitmix64_independent_vectors() {
    let mut rng = SplitMix64(0);
    assert_eq!(rng.next_u64(), 0xe220_a839_7b1d_cdaf);
    assert_eq!(rng.next_u64(), 0x6e78_9e6a_a1b9_65f4);
    assert_eq!(rng.next_u64(), 0x06c4_5d18_8009_454f);
    let mut rng = SplitMix64(SEED);
    assert_eq!(rng.next_u64(), 0xfc27_64f7_362d_7ff5);
}

/// GV-2 — the probe batch aggregate equals the frozen EXECUTED receipt
/// `AGG_VALUE = 0x980a9dc0d8d8cee6` (SPEC §2.3, 2026-10-21 WSL2), and the
/// independent python reimplementation produced the identical value
/// (evidence/golden/python-independent-vectors.json).
#[test]
fn gv2_probe_batch_aggregate_frozen_receipt() {
    let spec = BatchSpec::probe_batch();
    let pairs: Vec<(u32, u64)> = spec
        .items
        .iter()
        .map(|s| (s.item_id, ref_result(SEED, s).aggregation_contribution))
        .collect();
    assert_eq!(aggregate(&pairs), 0x980a_9dc0_d8d8_cee6);
}

/// GV-3 — per-item REF identity against the frozen module: for every
/// probe-batch item the new WorkloadCore yields the byte-identical
/// metadata_digest and aggregation_contribution as the frozen APP-A code.
#[test]
fn gv3_per_item_ref_matches_frozen_module() {
    let spec = BatchSpec::probe_batch();
    for s in &spec.items {
        let new = ref_result(SEED, s);
        let frozen_spec = frozen::ScanItemSpec {
            item_id: s.item_id,
            input_len: 0,
            workload_class: match s.workload_class {
                wrk_scan::workload::WorkloadClass::Trivial => frozen::WorkloadClass::Trivial,
                wrk_scan::workload::WorkloadClass::IoWait => frozen::WorkloadClass::IoWait,
                wrk_scan::workload::WorkloadClass::CpuHeavy => frozen::WorkloadClass::CpuHeavy,
                wrk_scan::workload::WorkloadClass::Mixed => frozen::WorkloadClass::Mixed,
                wrk_scan::workload::WorkloadClass::LongTail => frozen::WorkloadClass::LongTail,
            },
            fault: s.fault,
            cancellation_group: s.cancellation_group,
        };
        let old = frozen::ref_result(SEED, &frozen_spec);
        assert_eq!(
            new.metadata_digest, old.metadata_digest,
            "item {}",
            s.item_id
        );
        assert_eq!(
            new.aggregation_contribution, old.aggregation_contribution,
            "item {}",
            s.item_id
        );
        assert_eq!(
            format!("{:?}", new.result_class),
            format!("{:?}", old.result_class),
            "item {}",
            s.item_id
        );
    }
}

/// GV-4 — fixed-seed input identity: python-independent lengths, content
/// digests, and stream head/tail bytes for representative items (SPEC
/// §2.1 input_digest definition = SHA256(LE4(item_id) || input bytes)).
/// All expected values come from the python reimplementation of the SPEC
/// pseudocode (evidence/golden/python-independent-vectors.json), never
/// from any Rust execution.
#[test]
fn gv4_input_identity_python_vectors() {
    let cases: &[(u32, usize, &str, &str, &str)] = &[
        (
            0,
            22528,
            "877635ed1c10b244d758256e93eb2cbbabd48a317a9571c117600657ef15738d",
            "eaea1f12683b5e77",
            "30059e2cb93af598",
        ),
        (
            13,
            27648,
            "c6956e2d58639c657caa4336dc919d9dbfd81b1bd58d9c58bde9792a6f665300",
            "a9e59e775ad4f283",
            "1601b8dd46110093",
        ),
        (
            25,
            60416,
            "b8a0ae198cb356ab81dfedd51db1a8ffefe2d5caa8f262c56a06a97e9df790e3",
            "ff6891953133c4c1",
            "a6511efb18b3a2a5",
        ),
    ];
    for (id, expect_len, cd_hex, head_hex, tail_hex) in cases {
        let bytes = generate_input(SEED, *id);
        assert_eq!(bytes.len(), *expect_len, "item {id} length");
        assert!((4096..=65536).contains(&bytes.len()));
        assert_eq!(bytes.len() % 1024, 0);
        let cd = content_digest(*id, &bytes);
        assert_eq!(hex(cd), *cd_hex, "item {id} content digest");
        let head: Vec<u8> = (0..8)
            .map(|i| u8::from_str_radix(&head_hex[i * 2..i * 2 + 2], 16).unwrap())
            .collect();
        let tail: Vec<u8> = (0..8)
            .map(|i| u8::from_str_radix(&tail_hex[i * 2..i * 2 + 2], 16).unwrap())
            .collect();
        assert_eq!(&bytes[..8], head.as_slice(), "item {id} head");
        assert_eq!(&bytes[bytes.len() - 8..], tail.as_slice(), "item {id} tail");
    }
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// GV-5 — Err lane content consistency: the fault item (probe batch item
/// 13, %16==13) completes ALL common work and publishes the same
/// contribution the Ok lane would compute; only the class differs.
#[test]
fn gv5_err_lane_content_pinned() {
    let spec = BatchSpec::probe_batch();
    let s = &spec.items[13];
    assert_eq!(s.fault, Some(0x31));
    let r = ref_result(SEED, s);
    assert_eq!(r.result_class, ResultClass::CompletedErr(0x31));
    // The contribution is a function of content+metadata only — identical
    // to a hypothetical non-fault twin of the same item.
    let mut twin = s.clone();
    twin.fault = None;
    let twin_ref = ref_result(SEED, &twin);
    assert_eq!(
        r.aggregation_contribution,
        twin_ref.aggregation_contribution
    );
    assert_eq!(r.metadata_digest, twin_ref.metadata_digest);
}

/// GV-6 — fan-in aggregate is invariant under input permutation
/// (REFCHECK semantics, deterministic rotation instead of RNG).
#[test]
fn gv6_aggregate_order_invariance() {
    let spec = BatchSpec::probe_batch();
    let mut pairs: Vec<(u32, u64)> = spec
        .items
        .iter()
        .map(|s| (s.item_id, ref_result(SEED, s).aggregation_contribution))
        .collect();
    let a = aggregate(&pairs);
    pairs.rotate_left(7);
    let b = aggregate(&pairs);
    assert_eq!(a, b);
    assert_eq!(a, 0x980a_9dc0_d8d8_cee6);
}

/// GV-7 — fold_meta identity on a representative metadata digest
/// (python-independent value for probe item 0, class IoWait).
#[test]
fn gv7_fold_meta_python_vector() {
    let spec = BatchSpec::probe_batch();
    let r = ref_result(SEED, &spec.items[0]);
    assert_eq!(fold_meta(&r.metadata_digest), 0xac94_6e16_3021_2ef3);
}

/// GV-8 — transform/content primitives agreement: new crate's transform
/// output on an explicit input matches the frozen module byte-for-byte.
#[test]
fn gv8_transform_matches_frozen_module() {
    let bytes = generate_input(SEED, 6);
    for cpu in [1u32, 4, 128] {
        let (new_meta, new_peak) = transform(6, &bytes, cpu);
        let (old_meta, old_peak) = frozen::transform(6, &bytes, cpu);
        assert_eq!(new_meta, old_meta);
        assert_eq!(new_peak, old_peak);
        let cd_new = content_digest(6, &bytes);
        let cd_old = frozen::content_digest(6, &bytes);
        assert_eq!(cd_new, cd_old);
        assert_eq!(
            contribution(&cd_new, &new_meta, new_peak),
            frozen::contribution(&cd_old, &old_meta, old_peak)
        );
    }
}
