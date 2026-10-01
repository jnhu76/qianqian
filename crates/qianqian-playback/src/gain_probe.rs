//! I0 Gain disposable probe (Issue #177 Stage 2 / I0; ADR-PBK-002
//! D14.11). EXPERIMENT-ONLY evidence infrastructure, compiled only in
//! this crate's own test build (`cfg(all(test, not(loom)))`) and never
//! shipped: the whole module — and its one call site in the decode
//! worker's staging loop — is deleted with the I0 evidence.
//!
//! The probe is the smallest realization of the D14.11 processing
//! contract at the frozen decode-worker staging placement:
//!
//! ```text
//! y = x * factor    (in place, one staging block)
//! ```
//!
//! Source-rate/layout/frame-count preserving by construction (it never
//! touches anything but sample values), stateless (no signal-derived
//! history, so no seek invalidation obligations beyond the existing
//! staging/remainder correctness), bounded causal, no pending output at
//! EOF. It owns no configuration authority: the factor is armed by a
//! test, held only for that test's lifetime, and defaults to disarmed
//! (probe_stage is a no-op) so no other test in this binary can observe
//! it.
//!
//! Realtime posture of the exercised path: no per-block K0 work, no
//! capability/context/dispatch, no allocation (measured by
//! `steady_state_processing_allocates_zero` in gain_probe_tests), no
//! filesystem/network round trip — three relaxed atomic loads and an
//! in-place multiply per staging block.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// How many times the factor is applied per staging block. 0 = the probe
/// is disarmed (production-equivalent pass-through); 1 = the honest
/// probe; 2 = the deliberate double-application mutation, armed only by
/// the negative-control oracle.
static PROBE_APPLIES: AtomicU32 = AtomicU32::new(0);
/// The gain factor's f32 bit pattern.
static PROBE_FACTOR_BITS: AtomicU32 = AtomicU32::new(0);
/// The staging-block index at (and after) which the probe reports the
/// synthetic unrecoverable processing failure. u64::MAX = never.
static PROBE_FAIL_AFTER_BLOCK: AtomicU64 = AtomicU64::new(u64::MAX);
/// Staging blocks processed since the probe was armed (failure-injection
/// counter; not telemetry any product path reads).
static PROBE_BLOCK_INDEX: AtomicU64 = AtomicU64::new(0);

/// Disarms the probe on drop — including through a failing assertion —
/// so an armed factor can never leak into another test in this binary.
pub(crate) struct ProbeGuard;

impl Drop for ProbeGuard {
    fn drop(&mut self) {
        disarm();
    }
}

fn disarm() {
    PROBE_APPLIES.store(0, Ordering::Relaxed);
    PROBE_FAIL_AFTER_BLOCK.store(u64::MAX, Ordering::Relaxed);
    PROBE_BLOCK_INDEX.store(0, Ordering::Relaxed);
}

fn arm_mode(factor: f32, applies: u32, fail_after_block: u64) -> ProbeGuard {
    PROBE_FACTOR_BITS.store(factor.to_bits(), Ordering::Relaxed);
    PROBE_APPLIES.store(applies, Ordering::Relaxed);
    PROBE_FAIL_AFTER_BLOCK.store(fail_after_block, Ordering::Relaxed);
    PROBE_BLOCK_INDEX.store(0, Ordering::Relaxed);
    ProbeGuard
}

/// Arm the honest probe: every staging block is scaled by `factor` once.
pub(crate) fn arm(factor: f32) -> ProbeGuard {
    arm_mode(factor, 1, u64::MAX)
}

/// Arm the double-application mutation (negative control only): every
/// staging block is scaled by `factor` twice, so processed content is
/// `x * factor²`. The half-gain oracle MUST reject this.
pub(crate) fn arm_double_apply(factor: f32) -> ProbeGuard {
    arm_mode(factor, 2, u64::MAX)
}

/// Arm the probe so it reports the synthetic unrecoverable processing
/// failure from staging block `fail_after_block` on (blocks before that
/// are processed normally). Oracle injection for the D14.11 failure
/// semantics: the failure must settle through the existing D11 `Failed`
/// class with a diagnostic that stays truthful about its processing
/// origin.
pub(crate) fn arm_failure_after_block(factor: f32, fail_after_block: u64) -> ProbeGuard {
    arm_mode(factor, 1, fail_after_block)
}

/// The probe's entire processing contract, executed at the decode
/// worker's staging point (after `read_frames`, before the edge write).
///
/// `Err` is the synthetic unrecoverable processing failure: the worker
/// routes it through the existing D11 `Failed` publication with the
/// truthful processing-origin diagnostic. Never a bypass, never a
/// partial result (a failed processor's output is not trustworthy).
pub(crate) fn probe_stage(block: &mut [f32]) -> Result<(), String> {
    let applies = PROBE_APPLIES.load(Ordering::Relaxed);
    if applies == 0 {
        return Ok(());
    }
    let index = PROBE_BLOCK_INDEX.fetch_add(1, Ordering::Relaxed);
    if index >= PROBE_FAIL_AFTER_BLOCK.load(Ordering::Relaxed) {
        return Err(format!(
            "synthetic processing failure at staging block {index}"
        ));
    }
    let factor = f32::from_bits(PROBE_FACTOR_BITS.load(Ordering::Relaxed));
    for _ in 0..applies {
        for sample in block.iter_mut() {
            *sample *= factor;
        }
    }
    Ok(())
}
