//! Frozen scenario matrix S0–S6 (SPEC §3.1–§3.4) and the target-machine
//! 46-cell expansion.
//!
//! The item_id distribution functions are ARCHIVE_NORMALIZATION (SPEC
//! §3.1): they are pure representational refinements of the reviewed
//! percentages and must never be silently reshaped. P/B candidates are
//! derived by the frozen §3.4 rule on the host's physical core count.

use crate::workload::{BatchSpec, ScanItemSpec, WorkloadClass};

/// Frozen scenario identities (SPEC §3.2 table A row ids).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scenario {
    S0,
    S1,
    S2,
    S3,
    S4,
    S5,
    S6,
}

impl Scenario {
    /// Batch size N (SPEC §3.2 table A).
    pub fn n(self) -> u32 {
        match self {
            Scenario::S0 => 1024,
            Scenario::S1 => 256,
            Scenario::S2 => 128,
            Scenario::S3 => 256,
            Scenario::S4 => 256,
            Scenario::S5 => 256,
            Scenario::S6 => 4096,
        }
    }

    /// Scenario-table concurrency gears, before the frozen §3.4 candidate
    /// derivation (values as printed in SPEC §3.2 `CONCURRENCY_P`).
    fn table_gears(self) -> &'static [usize] {
        match self {
            Scenario::S0 => &[1, 4],
            Scenario::S1 => &[4, 8, 16],
            // S2 table gears are already the P_phys-derived pair.
            Scenario::S2 => &[], // derived directly from p_phys
            Scenario::S3 => &[4],
            Scenario::S4 => &[4],
            Scenario::S5 => &[4],
            Scenario::S6 => &[4, 8],
        }
    }

    /// B-candidate rule as printed in SPEC §3.2 `OUTSTANDING_BOUND_B`:
    /// which multiples of P apply.
    fn b_multiples(self) -> &'static [usize] {
        match self {
            Scenario::S0 => &[1, 2],
            Scenario::S4 | Scenario::S5 => &[2],
            Scenario::S1 | Scenario::S6 => &[1, 2, 4],
            Scenario::S2 => &[1],
            Scenario::S3 => &[2, 4],
        }
    }

    /// Frozen §3.4 derivation:
    /// `P 候选 = (场景表档位 ∩ {1..P_phys}) ∪ {floor(P_phys/2), P_phys}`,
    /// deduplicated ascending. `P_phys` is the OS-reported physical core
    /// count (SMT siblings excluded), capped at 16.
    pub fn p_candidates(self, p_phys: usize) -> Vec<usize> {
        let p_phys = p_phys.min(16);
        let mut set = std::collections::BTreeSet::new();
        for g in self.table_gears() {
            if *g >= 1 && *g <= p_phys {
                set.insert(*g);
            }
        }
        set.insert(p_phys / 2);
        set.insert(p_phys);
        set.remove(&0);
        set.into_iter().collect()
    }

    /// The frozen 46-cell expansion for this host (SPEC §3.4 rule ×
    /// scenario B multiples). Ordered S0→S6, P ascending, B ascending.
    pub fn expand_cells(p_phys: usize) -> Vec<Cell> {
        let mut cells = Vec::new();
        for s in [
            Scenario::S0,
            Scenario::S1,
            Scenario::S2,
            Scenario::S3,
            Scenario::S4,
            Scenario::S5,
            Scenario::S6,
        ] {
            for p in s.p_candidates(p_phys) {
                for m in s.b_multiples() {
                    cells.push(Cell {
                        scenario: s,
                        p,
                        b: p * m,
                    });
                }
            }
        }
        cells
    }

    /// Frozen cancellation milestone (SPEC §3.1 injection rule): only S5
    /// cancels, after the 164th admission, group 7. The milestone is the
    /// workload owner's single-threaded admission count — never a
    /// completion count, never a duration.
    pub fn cancel_milestone(self) -> Option<(u32, u32)> {
        match self {
            Scenario::S5 => Some((164, 7)),
            _ => None,
        }
    }

    /// Deterministic batch for this scenario (SPEC §3.1 ARCHIVE_NORMALIZATION
    /// item_id functions + §3.1 CPU/wait overrides + injection rules).
    pub fn batch_spec(self, batch_id: u64, seed: u64) -> BatchSpec {
        let n = self.n();
        let mut items = Vec::with_capacity(n as usize);
        for item_id in 0..n {
            let (class, cpu_blocks, wait_us) = self.item_shape(item_id);
            let fault = match self {
                Scenario::S3 | Scenario::S5 => (item_id % 16 == 13).then_some(0x31),
                _ => None,
            };
            let group = if item_id.is_multiple_of(3) { 7 } else { 1 };
            items.push(ScanItemSpec {
                item_id,
                input_len: 0, // derived in generate_input; schema completeness
                workload_class: class,
                cpu_blocks,
                wait_us,
                fault,
                cancellation_group: group,
            });
        }
        BatchSpec {
            batch_id,
            seed,
            items,
        }
    }

    /// One item's (class, effective cpu_blocks, effective wait_us) under
    /// the frozen §3.1 distribution and override table.
    fn item_shape(self, item_id: u32) -> (WorkloadClass, u32, u64) {
        match self {
            Scenario::S0 => (WorkloadClass::Trivial, 4, 0),
            Scenario::S1 => {
                if item_id % 5 == 3 {
                    (WorkloadClass::Trivial, 4, 0)
                } else {
                    (WorkloadClass::IoWait, 8, 5_000)
                }
            }
            Scenario::S2 => {
                if item_id % 5 == 3 {
                    (WorkloadClass::Trivial, 4, 0)
                } else {
                    (WorkloadClass::CpuHeavy, 2048, 0)
                }
            }
            Scenario::S3 | Scenario::S5 => {
                // common_base5 rotation; every class waits 2000us.
                let class = common_base5(item_id);
                let cpu = match class {
                    WorkloadClass::IoWait => 8,
                    WorkloadClass::CpuHeavy => 2048,
                    WorkloadClass::Mixed => 128,
                    WorkloadClass::Trivial => 4,
                    WorkloadClass::LongTail => unreachable!("base5 has no LongTail"),
                };
                (class, cpu, 2_000)
            }
            Scenario::S4 => {
                if item_id.is_multiple_of(10) {
                    (WorkloadClass::LongTail, 8192, 2_000)
                } else {
                    (WorkloadClass::Mixed, 128, 0)
                }
            }
            Scenario::S6 => (WorkloadClass::Mixed, 128, 2_000),
        }
    }
}

/// `common_base5(item_id)` (SPEC §3.1): the four-class rotation shared by
/// S3/S5 — no LongTail override.
fn common_base5(item_id: u32) -> WorkloadClass {
    match item_id % 5 {
        0 => WorkloadClass::IoWait,
        1 => WorkloadClass::CpuHeavy,
        2 => WorkloadClass::Mixed,
        3 => WorkloadClass::Trivial,
        _ => WorkloadClass::IoWait,
    }
}

/// One frozen correctness cell: (scenario, P, B). K0 mode and observer
/// mode are orthogonal factors applied at run time, giving 46 × 2 = 92
/// correctness cases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub scenario: Scenario,
    pub p: usize,
    pub b: usize,
}

impl Cell {
    /// Stable arm identity for the frozen per-seed arm-order rule
    /// (SPEC §3.5: `SHA256(LE8(seed) || 0x00 || ASCII(arm_id))`, first 8
    /// bytes LE as u64, ascending). No ranking is performed in Gate I;
    /// the identity only has to exist and be stable for future gates.
    pub fn arm_id(self, k0_on: bool) -> String {
        format!(
            "H0-{}-{}-P{}-B{}",
            if k0_on { "ON" } else { "OFF" },
            self.scenario_label(),
            self.p,
            self.b
        )
    }

    pub fn scenario_label(self) -> &'static str {
        match self.scenario {
            Scenario::S0 => "S0",
            Scenario::S1 => "S1",
            Scenario::S2 => "S2",
            Scenario::S3 => "S3",
            Scenario::S4 => "S4",
            Scenario::S5 => "S5",
            Scenario::S6 => "S6",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_46_cell_expansion_at_p_phys_10() {
        let cells = Scenario::expand_cells(10);
        assert_eq!(cells.len(), 46);
        let by = |s: Scenario| -> Vec<(usize, usize)> {
            cells
                .iter()
                .filter(|c| c.scenario == s)
                .map(|c| (c.p, c.b))
                .collect()
        };
        assert_eq!(by(Scenario::S0).len(), 8);
        assert_eq!(by(Scenario::S1).len(), 12);
        assert_eq!(by(Scenario::S2).len(), 2);
        assert_eq!(by(Scenario::S3).len(), 6);
        assert_eq!(by(Scenario::S4).len(), 3);
        assert_eq!(by(Scenario::S5).len(), 3);
        assert_eq!(by(Scenario::S6).len(), 12);
        // Spot-check the derived candidate sets against the frozen table.
        assert_eq!(Scenario::S0.p_candidates(10), vec![1, 4, 5, 10]);
        assert_eq!(Scenario::S1.p_candidates(10), vec![4, 5, 8, 10]);
        assert_eq!(Scenario::S2.p_candidates(10), vec![5, 10]);
        assert_eq!(Scenario::S3.p_candidates(10), vec![4, 5, 10]);
        assert_eq!(Scenario::S5.p_candidates(10), vec![4, 5, 10]);
        assert_eq!(Scenario::S6.p_candidates(10), vec![4, 5, 8, 10]);
    }

    #[test]
    fn scenario_shapes_match_frozen_overrides() {
        // S1: 4/5 IoWait(8, 5000us) + Trivial residual at %5==3.
        let (c, cpu, w) = Scenario::S1.item_shape(3);
        assert_eq!((c, cpu, w), (WorkloadClass::Trivial, 4, 0));
        let (c, cpu, w) = Scenario::S1.item_shape(0);
        assert_eq!((c, cpu, w), (WorkloadClass::IoWait, 8, 5_000));
        // S3: base5 rotation, every class waits 2000us, no LongTail.
        let (c, cpu, w) = Scenario::S3.item_shape(6);
        assert_eq!((c, cpu, w), (WorkloadClass::CpuHeavy, 2048, 2_000));
        // S4: LongTail at %10==0 (8192, 2000us), Mixed otherwise (128, 0).
        let (c, cpu, w) = Scenario::S4.item_shape(10);
        assert_eq!((c, cpu, w), (WorkloadClass::LongTail, 8192, 2_000));
        let (c, cpu, w) = Scenario::S4.item_shape(1);
        assert_eq!((c, cpu, w), (WorkloadClass::Mixed, 128, 0));
        // S5 cancels at the 164th admission, group 7; nobody else does.
        assert_eq!(Scenario::S5.cancel_milestone(), Some((164, 7)));
        assert_eq!(Scenario::S0.cancel_milestone(), None);
        // Faults only in S3/S5 at %16==13 -> 0x31.
        let s3 = Scenario::S3.batch_spec(1, 0x00C0_FFEE_0304_0001);
        assert!(s3.items[13].fault == Some(0x31));
        assert!(s3.items[12].fault.is_none());
        let s6 = Scenario::S6.batch_spec(1, 0x00C0_FFEE_0304_0001);
        assert!(s6.items.iter().all(|i| i.fault.is_none()));
    }
}
