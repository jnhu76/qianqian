//! WRK-SCAN-v0 deterministic core — the frozen workload semantics
//! (SPEC §2, APP-A `workload.rs`, file SHA256 558a27dc…).
//!
//! Everything here is a pure function of (seed, item_id): the same batch
//! always yields the same inputs, the same REF results and the same
//! aggregate, independent of which executor mechanism runs it.
//!
//! NOT a benchmark harness: no timing is used anywhere in this module.
//! CPU work is operation-count based (block iterations), not time based.
//!
//! PROVENANCE (FREEZE-AUTHORITY-MAP §7.1): the functions below are reused
//! from the frozen APP-A source with semantics-identical rustfmt/clippy
//! normalization (golden-vector tests guard byte-level behavioral
//! identity, including the frozen `AGG_VALUE` receipt). The single
//! structural extension is that [`ScanItemSpec`] carries the *effective*
//! `(cpu_blocks, wait_us)` explicitly, because SPEC §3.1 scenario tables
//! override the per-class defaults. `BatchSpec::new` (the frozen
//! probe-batch rule) still fills the class defaults, so probe-batch
//! behavior is byte-identical to the frozen source.

use sha2::{Digest, Sha256};

/// Deterministic PRNG (SplitMix64) — public-domain reference algorithm,
/// chosen so input generation is reproducible on every platform without
/// external dependencies.
#[derive(Clone, Copy)]
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Workload class of one scan item. Determines the amount of synthetic
/// I/O wait and CPU transformation, independently of the executor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkloadClass {
    /// S0-shaped: minimal work, measures fixed mechanism overhead.
    Trivial,
    /// S1-shaped: wait dominates (logical I/O wait), little CPU.
    IoWait,
    /// S2-shaped: CPU dominates, no wait.
    CpuHeavy,
    /// S3-shaped: mixed wait + CPU (pipeline shape).
    Mixed,
    /// S4-shaped: long-tail item (skewed work).
    LongTail,
}

impl WorkloadClass {
    /// Frozen per-class default CPU blocks (SPEC §2.3 class table). The
    /// scenario tables in SPEC §3.1 override these when a scenario covers
    /// the class — the override is carried in `ScanItemSpec::cpu_blocks`.
    pub fn cpu_blocks(self) -> u32 {
        match self {
            WorkloadClass::Trivial => 4,
            WorkloadClass::IoWait => 8,
            WorkloadClass::CpuHeavy => 1024,
            WorkloadClass::Mixed => 128,
            WorkloadClass::LongTail => 4096,
        }
    }

    /// Frozen per-class default synthetic logical I/O wait in microseconds.
    /// A *workload parameter* (simulated pacing), never a synchronization
    /// primitive and never evidence of any ordering. Scenario overrides are
    /// carried in `ScanItemSpec::wait_us`.
    pub fn wait_us(self) -> u64 {
        match self {
            WorkloadClass::Trivial => 0,
            WorkloadClass::IoWait => 2_000,
            WorkloadClass::CpuHeavy => 0,
            WorkloadClass::Mixed => 400,
            WorkloadClass::LongTail => 200,
        }
    }
}

/// One scan item's specification, derived deterministically from
/// (batch seed, item_id). `fault` is the injected logical failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanItemSpec {
    pub item_id: u32,
    pub input_len: usize,
    pub workload_class: WorkloadClass,
    /// Effective CPU block count for this item (SPEC §3.1 scenario
    /// override, or the class default for the probe batch rule).
    pub cpu_blocks: u32,
    /// Effective synthetic wait for this item (same override rule).
    pub wait_us: u64,
    /// Deterministic logical failure injection: None, or an error code
    /// the mechanism MUST propagate as CompletedErr (never as success,
    /// never silently dropped).
    pub fault: Option<u32>,
    /// Cancellation group. request_cancel(group) cuts admission for the
    /// group and turns not-yet-terminal admitted items of that group
    /// into Abandoned (by their real owner, not by the harness).
    pub cancellation_group: u32,
}

impl ScanItemSpec {
    /// Frozen probe-batch construction: effective cpu/wait are the class
    /// defaults (byte-identical to the frozen APP-A behavior).
    pub fn with_class_defaults(
        item_id: u32,
        workload_class: WorkloadClass,
        fault: Option<u32>,
        cancellation_group: u32,
    ) -> Self {
        ScanItemSpec {
            item_id,
            input_len: 0, // derived in generate_input; schema completeness
            workload_class,
            cpu_blocks: workload_class.cpu_blocks(),
            wait_us: workload_class.wait_us(),
            fault,
            cancellation_group,
        }
    }
}

/// Result class — the terminal vocabulary of one item (SPEC §5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultClass {
    CompletedOk,
    CompletedErr(u32),
}

/// The independently computable reference result for one item.
/// REF(item_id) is computable without any executor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefScanResult {
    pub item_id: u32,
    pub content_digest: [u8; 32],
    pub metadata_digest: [u8; 32],
    pub result_class: ResultClass,
    /// Deterministic u64 contribution to the fan-in aggregate.
    pub aggregation_contribution: u64,
}

/// Deterministic input bytes for (seed, item_id). Media-scan stand-in:
/// a pseudo-media payload whose "hashing/metadata extraction" is the
/// CPU transformation below. Synthetic-input limits are declared in the
/// protocol: it validates logic/oracle/cost-shape, NOT native disk-I/O
/// performance (that is the separate NATIVE_IO layer).
pub fn generate_input(seed: u64, item_id: u32) -> Vec<u8> {
    let mut rng = SplitMix64(seed ^ (item_id as u64).wrapping_mul(0x9E37_79B9));
    // v0 input sizes: 4 KiB .. 64 KiB, deterministic per item.
    let len = 4096 + (rng.next_u64() % 61) as usize * 1024;
    let mut out = Vec::with_capacity(len);
    let mut block = [0u8; 8];
    while out.len() < len {
        let v = rng.next_u64().to_le_bytes();
        block.copy_from_slice(&v);
        out.extend_from_slice(&block);
    }
    out.truncate(len);
    out
}

/// The shared content transformation f(item_id, bytes, cpu_blocks) that
/// EVERY mechanism must run identically. CPU work = cpu_blocks iterations
/// of a block-mixing pass over the input (operation-count based; identical
/// work regardless of executor; cannot be skipped without changing the
/// digest).
///
/// Returns the "metadata extraction" output: a 32-byte digest plus a u64
/// synthetic metadata field (peak-mix), both order-independent.
pub fn transform(item_id: u32, bytes: &[u8], cpu_blocks: u32) -> ([u8; 32], u64) {
    let mut hasher = Sha256::new();
    hasher.update(item_id.to_le_bytes());
    hasher.update((bytes.len() as u64).to_le_bytes());
    // Block-mixing pass: each iteration re-mixes a 64-byte window chosen
    // deterministically by the previous state — a fixed amount of real
    // CPU work whose result depends on every input byte.
    let mut state: u64 = item_id as u64 ^ 0x5DEE_CE66;
    let mut peak: u64 = 0;
    let block_len = 64;
    let n_blocks = bytes.len().div_ceil(block_len);
    for iter in 0..cpu_blocks {
        let idx = (state as usize % n_blocks) * block_len;
        let end = (idx + block_len).min(bytes.len());
        let window = &bytes[idx..end];
        let mut mixer = Sha256::new();
        mixer.update(iter.to_le_bytes());
        mixer.update(window);
        let d = mixer.finalize();
        state ^=
            u64::from_le_bytes(d[0..8].try_into().unwrap()).wrapping_mul(0x2545_F491_4F6C_DD1D);
        peak = peak.max(state);
    }
    hasher.update(state.to_le_bytes());
    hasher.update(peak.to_le_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    (digest, peak)
}

/// Content digest of an item's input bytes (identical function used by
/// REF and by any executor "reading" the same bytes).
pub fn content_digest(item_id: u32, bytes: &[u8]) -> [u8; 32] {
    let mut content = Sha256::new();
    content.update(item_id.to_le_bytes());
    content.update(bytes);
    content.finalize().into()
}

/// Deterministic u64 contribution of one item to the fan-in aggregate,
/// derived from the item's content and metadata only. Both REF and the
/// executor use THIS function — same inputs, same output.
pub fn contribution(content_digest: &[u8; 32], metadata_digest: &[u8; 32], peak: u64) -> u64 {
    let mut agg = Sha256::new();
    agg.update(content_digest);
    agg.update(metadata_digest);
    agg.update(peak.to_le_bytes());
    let a: [u8; 32] = agg.finalize().into();
    u64::from_le_bytes(a[0..8].try_into().unwrap())
}

/// REF(item_id): the authoritative expected result, computable in one
/// process without any executor mechanism. Faulty items complete with
/// CompletedErr(code); cancellation never changes REF's per-item class —
/// Abandoned is an *obligation* outcome owned by the mechanism's item
/// owner, not part of the content function.
///
/// Executor code MUST NOT call this function (SPEC §2.5(b)): REF belongs
/// to the oracle/reference side only.
pub fn ref_result(seed: u64, spec: &ScanItemSpec) -> RefScanResult {
    let bytes = generate_input(seed, spec.item_id);
    let (metadata_digest, peak) = transform(spec.item_id, &bytes, spec.cpu_blocks);
    let content_digest = content_digest(spec.item_id, &bytes);
    let result_class = match spec.fault {
        Some(code) => ResultClass::CompletedErr(code),
        None => ResultClass::CompletedOk,
    };
    // Aggregate contribution: deterministic function of content+metadata,
    // independent of completion order.
    let contribution = contribution(&content_digest, &metadata_digest, peak);
    RefScanResult {
        item_id: spec.item_id,
        content_digest,
        metadata_digest,
        result_class,
        aggregation_contribution: contribution,
    }
}

/// Deterministic fan-in aggregate over items ordered by item_id. The
/// aggregate must be identical no matter in which order items complete:
/// fold contribution digests in item_id order.
pub fn aggregate(contributions_by_item: &[(u32, u64)]) -> u64 {
    let mut sorted = contributions_by_item.to_vec();
    sorted.sort_by_key(|(item_id, _)| *item_id);
    let mut acc: u64 = 0x5DEE_CE66_D1CE_B0BA;
    for (item_id, contribution) in sorted {
        acc ^= contribution
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .rotate_left(item_id % 64);
        acc = acc.wrapping_add(contribution);
    }
    acc
}

/// BatchSpec: the whole frozen batch for one run.
#[derive(Clone, Debug)]
pub struct BatchSpec {
    pub batch_id: u64,
    pub seed: u64,
    pub items: Vec<ScanItemSpec>,
}

impl BatchSpec {
    /// The frozen probe batch (feasibility size — NOT the R2 performance
    /// size). Deterministic: fixed item count, fixed fault set, fixed
    /// cancel group, fixed long-tail indices.
    pub fn probe_batch() -> Self {
        Self::new(1, 0x00C0_FFEE_0304_0001, 48)
    }

    /// The frozen probe-batch construction rule for any N (SPEC §2.3):
    /// - fault items: item_id % 16 == 13  -> CompletedErr(0x31)
    /// - cancellation group 7: item_id % 3 == 0
    /// - long tail: item_id % 11 == 5 (applies after class assignment)
    /// - class assignment: deterministic from item_id
    pub fn new(batch_id: u64, seed: u64, n: u32) -> Self {
        let mut items = Vec::with_capacity(n as usize);
        for item_id in 0..n {
            let class = match item_id % 5 {
                0 => WorkloadClass::IoWait,
                1 => WorkloadClass::CpuHeavy,
                2 => WorkloadClass::Mixed,
                3 => WorkloadClass::Trivial,
                _ => WorkloadClass::IoWait,
            };
            let class = if item_id % 11 == 5 {
                WorkloadClass::LongTail
            } else {
                class
            };
            items.push(ScanItemSpec::with_class_defaults(
                item_id,
                class,
                (item_id % 16 == 13).then_some(0x31),
                if item_id % 3 == 0 { 7 } else { 1 },
            ));
        }
        BatchSpec {
            batch_id,
            seed,
            items,
        }
    }
}
