//! Reconcile + revision identity and quiescence oracles (#70 Stages 6–7,
//! oracle families D and F): the P0 D0–D4 desired-revision-identity oracle
//! (design §L.5) and the §L.1 quiescence transition predicate.
//!
//! quiet ≠ healthy; quiet ≠ successful; quiet ≠ confluent.

mod common;

use common::*;
use qianqian_composition::{CompositionKernel, DesiredEntry, Revision};

/// D0: desired Decoder@R1 enabled → mount G1 → activation fails → raise →
/// Unloading (partial unwind) → fully discharged → G1 FAILED, outcome visible.
#[test]
fn d0_fresh_incarnation_fails_into_discharged_failed() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(flaky_provider("decoder", 1, &l))
        .expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled(
        "decoder",
        "decoder",
        Revision::new(1),
    )])
    .expect("legal");
    k.settle();

    let snap = k.snapshot();
    let g1 = snap.fibers.get("decoder").expect("G1 still mounted");
    assert_eq!(g1.state, qianqian_composition::FiberState::Failed);
    assert!(g1.failed_outcome, "FAILED outcome visible (§F.5)");
    assert!(snap.quiet, "D1 precondition: FAILED is quiet-legal");
    assert_eq!(entries(&l), vec!["decoder:attempt0"]);
}

/// D1: reconcile runs again with the UNCHANGED Decoder@R1 entry → diff = same
/// desired incarnation → no orchestration request → G1 remains FAILED; settle
/// returns; silent retry is a defect (B19, R7).
#[test]
fn d1_unchanged_incarnation_never_retries() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(flaky_provider("decoder", 1, &l))
        .expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled(
        "decoder",
        "decoder",
        Revision::new(1),
    )])
    .expect("legal");
    k.settle();
    let before = entries(&l);

    // Reconcile repeatedly with the identical entry.
    for _ in 0..3 {
        k.set_desired(vec![DesiredEntry::enabled(
            "decoder",
            "decoder",
            Revision::new(1),
        )])
        .expect("legal");
        k.settle();
    }

    assert_eq!(
        entries(&l),
        before,
        "an unchanged desired incarnation can never retry FAILED (R1/R7)"
    );
    let snap = k.snapshot();
    assert_eq!(
        snap.fibers.get("decoder").map(|f| f.state),
        Some(qianqian_composition::FiberState::Failed)
    );
    assert!(snap.quiet, "settle() returns with FAILED visible");
}

/// D2: a capability unrelated to Decoder disappears and reappears → G1 stays
/// FAILED; no desired revision identity changed anywhere (R2).
#[test]
fn d2_unrelated_dependency_churn_fabricates_no_retry() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(flaky_provider("decoder", 1, &l))
        .expect("component registered");
    k.register_component(extra_provider("unrelated", "u", &l))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("decoder", "decoder", Revision::new(1)),
        DesiredEntry::enabled("unrelated", "unrelated", Revision::new(1)),
    ])
    .expect("legal");
    k.settle();
    assert_eq!(
        k.snapshot().fibers.get("decoder").map(|f| f.state),
        Some(qianqian_composition::FiberState::Failed)
    );
    let before = entries(&l);

    // Unrelated dependency churns: disappears, reappears (with churn of its
    // own revision — still unrelated to the FAILED entry).
    k.set_desired(vec![DesiredEntry::enabled(
        "decoder",
        "decoder",
        Revision::new(1),
    )])
    .expect("legal");
    k.settle();
    k.set_desired(vec![
        DesiredEntry::enabled("decoder", "decoder", Revision::new(1)),
        DesiredEntry::enabled("unrelated", "unrelated", Revision::new(2)),
    ])
    .expect("legal");
    k.settle();

    let decoder_events_now = entries(&l)
        .iter()
        .filter(|e| e.starts_with("decoder:"))
        .count();
    let decoder_events_before = before.iter().filter(|e| e.starts_with("decoder:")).count();
    assert_eq!(
        decoder_events_now, decoder_events_before,
        "dependency churn must not fabricate a retry (R2)"
    );
    assert_eq!(
        k.snapshot().fibers.get("decoder").map(|f| f.state),
        Some(qianqian_composition::FiberState::Failed)
    );
}

/// D3: the operator presents Decoder@R2 (fresh incarnation; component and
/// configuration otherwise identical) → visible staged retire/remove of G1 →
/// fresh G2 mounts and may activate (R3/R4/R8).
#[test]
fn d3_fresh_incarnation_stages_visible_revision() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(flaky_provider("decoder", 1, &l))
        .expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled(
        "decoder",
        "decoder",
        Revision::new(1),
    )])
    .expect("legal");
    k.settle();
    let before = entries(&l);
    assert_eq!(
        k.snapshot().fibers.get("decoder").map(|f| f.state),
        Some(qianqian_composition::FiberState::Failed)
    );

    k.set_desired(vec![DesiredEntry::enabled(
        "decoder",
        "decoder",
        Revision::new(2),
    )])
    .expect("legal");
    k.settle();

    // The FAILED generation was removed (not resurrected) and a fresh
    // generation mounted and activated: a visible revision (R4/R8).
    let attempts_now = entries(&l)
        .iter()
        .filter(|e| e.starts_with("decoder:attempt"))
        .count();
    let attempts_before = before
        .iter()
        .filter(|e| e.starts_with("decoder:attempt"))
        .count();
    assert_eq!(
        attempts_now,
        attempts_before + 1,
        "exactly one fresh activation episode for the fresh incarnation"
    );
    let snap = k.snapshot();
    assert_eq!(
        snap.fibers.get("decoder").map(|f| f.state),
        Some(qianqian_composition::FiberState::Active),
        "G2 may activate"
    );
    assert!(!snap.fibers.get("decoder").unwrap().failed_outcome);
    assert!(snap.quiet);
}

/// D4: repeated reconcile at Decoder@R2 → no further requests, no G3/G4
/// churn (R7).
#[test]
fn d4_repeated_reconcile_at_new_incarnation_is_idempotent() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(flaky_provider("decoder", 1, &l))
        .expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled(
        "decoder",
        "decoder",
        Revision::new(1),
    )])
    .expect("legal");
    k.settle();
    k.set_desired(vec![DesiredEntry::enabled(
        "decoder",
        "decoder",
        Revision::new(2),
    )])
    .expect("legal");
    k.settle();
    let activations_after_revision = entries(&l)
        .iter()
        .filter(|e| e.starts_with("decoder:attempt"))
        .count();

    for _ in 0..5 {
        k.set_desired(vec![DesiredEntry::enabled(
            "decoder",
            "decoder",
            Revision::new(2),
        )])
        .expect("legal");
        k.settle();
    }
    assert_eq!(
        entries(&l)
            .iter()
            .filter(|e| e.starts_with("decoder:attempt"))
            .count(),
        activations_after_revision,
        "no G3/G4 churn from repeated reconciles (R7)"
    );
    assert!(k.snapshot().quiet);
}

/// A revision requested on a FAILED fiber stages through the visible
/// retire → remove → mount sequence, never in-place (R4/R8). Observed via
/// step boundaries: removal and mount are separate transitions.
#[test]
fn revision_staging_is_visible_in_step_boundaries() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(flaky_provider("decoder", 1, &l))
        .expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled(
        "decoder",
        "decoder",
        Revision::new(1),
    )])
    .expect("legal");
    k.settle();

    k.set_desired(vec![DesiredEntry::enabled(
        "decoder",
        "decoder",
        Revision::new(2),
    )])
    .expect("legal");

    // Step 1: the FAILED fiber is retired (flag only; it stays mounted).
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned);
    let snap = k.snapshot();
    assert_eq!(
        snap.fibers.get("decoder").map(|f| f.state),
        Some(qianqian_composition::FiberState::Failed),
        "retired FAILED fiber stays Failed until removed"
    );
    // Step 2: removal (empty table, Inactive-family state, guard released).
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned);
    assert!(
        !k.snapshot().fibers.contains_key("decoder"),
        "the FAILED generation is removed, not resurrected (R8)"
    );
    // Between removal and remount the registry holds no decoder — the honest
    // staging gap (§E.4).
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned);
    assert_eq!(
        k.snapshot().fibers.get("decoder").map(|f| f.state),
        Some(qianqian_composition::FiberState::Pending),
        "fresh G2 mounts only after the removal"
    );
    k.settle();
    assert!(k.snapshot().quiet);
}

// ---------------------------------------------------------------------------
// Quiescence matrix (§L.1 examples; oracle family F)
// ---------------------------------------------------------------------------

/// Pending settles; FAILED settles (covered above); Activating/Unloading do
/// not settle; latched violation does not settle; a half-finished staged
/// replacement with a still-owed mount does not settle.
#[test]
fn pending_settles() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_consumer("c", &l))
        .expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled("c", "c", Revision::fresh())])
        .expect("legal");
    k.settle();
    assert!(k.snapshot().quiet);
}

#[test]
fn unloading_does_not_settle() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_provider("p", "p1", &l))
        .expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled("p", "p", Revision::fresh())])
        .expect("legal");
    k.settle();

    // Request retirement. Step 1 sets the retire flag (Active, retired);
    // step 2 is the L-Leave divert into Unloading — observable mid-flight.
    k.set_desired(Vec::new()).expect("legal");
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned);
    assert_eq!(
        k.snapshot().fibers.get("p").map(|f| f.state),
        Some(qianqian_composition::FiberState::Active)
    );
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned);
    let snap = k.snapshot();
    assert_eq!(
        snap.fibers.get("p").map(|f| f.state),
        Some(qianqian_composition::FiberState::Unloading)
    );
    assert!(!snap.quiet, "Unloading never settles");
    // Draining completes the transition to removal.
    k.settle();
    assert!(k.snapshot().quiet);
    assert!(k.snapshot().fibers.is_empty());
}

#[test]
fn latched_violation_does_not_settle_and_step_is_blocked() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(violating_unwind_component("v", &l))
        .expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled("v", "v", Revision::fresh())])
        .expect("legal");

    // Mount, then activate: the activation itself succeeds (its effect is
    // legitimately registered); the violated inverse latches at unload.
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned); // mount
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned); // activate
    assert_eq!(
        k.snapshot().fibers.get("v").map(|f| f.state),
        Some(qianqian_composition::FiberState::Active)
    );
    // Retire and drive into the unload whose inverse violates.
    k.set_desired(Vec::new()).expect("legal");
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned); // retire flag
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned); // divert
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned); // violated unload
    let snap = k.snapshot();
    let v = snap.fibers.get("v").expect("fiber present");
    assert_eq!(v.state, qianqian_composition::FiberState::Unloading);
    assert!(v.teardown_violated, "TEARDOWN_VIOLATED latched");
    assert!(!snap.quiet);
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Blocked);
    // settle() must terminate (not hang) on the latch.
    k.settle();
    assert!(!k.snapshot().quiet);
}

/// Half-finished staged replacement: between the old generation's removal
/// and the still-owed new mount the composition is NOT quiet (§L.1 clause 5).
#[test]
fn half_finished_staged_replacement_does_not_settle() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_provider("p", "p1", &l))
        .expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled("p", "p", Revision::new(1))])
        .expect("legal");
    k.settle();

    // Present the fresh incarnation; advance one step at a time.
    k.set_desired(vec![DesiredEntry::enabled("p", "p", Revision::new(2))])
        .expect("legal");
    // §E.4 staging, one observable transition at a time:
    // 1. retire flag (Active, retired — target moved to ⊥).
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned);
    // 2. L-Leave divert into Unloading.
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned);
    assert!(!k.snapshot().quiet);
    // 3. discharged unload (guard released: no dependents).
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned);
    // 4. O-Remove — the staging gap: old gone, new not yet mounted.
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned);
    let snap = k.snapshot();
    assert!(!snap.fibers.contains_key("p"), "old generation removed");
    assert!(snap.provisions.get("Tag").is_none_or(|v| v.is_empty()));
    assert!(
        !snap.quiet,
        "a staged replacement still owing its new mount is not quiescent"
    );
    // 5. The owed mount (then its activation) completes the staging.
    assert_eq!(k.step(), qianqian_composition::StepOutcome::Transitioned);
    k.settle();
    assert!(k.snapshot().quiet);
    assert_eq!(
        k.snapshot().capabilities.get("Tag"),
        Some(&Some("p".to_owned()))
    );
}

/// quiet ≠ healthy: a Pending composition over a missing provider is quiet
/// but not healthy; quiet ≠ successful: a settled FAILED composition is
/// quiet but not successful.
#[test]
fn quiet_is_not_healthy_nor_successful() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_consumer("c", &l))
        .expect("component registered");
    k.register_component(flaky_provider("p", 1, &l))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("c", "c", Revision::fresh()),
        DesiredEntry::enabled("p", "p", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    let snap = k.snapshot();
    assert!(snap.quiet, "both Pending and FAILED may be quiet");
    assert_eq!(
        snap.fibers.get("c").map(|f| f.state),
        Some(qianqian_composition::FiberState::Pending),
        "not healthy: the consumer's dependency is missing"
    );
    assert_eq!(
        snap.fibers.get("p").map(|f| f.state),
        Some(qianqian_composition::FiberState::Failed),
        "not successful: the provider failed"
    );
}
