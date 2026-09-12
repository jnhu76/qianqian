//! Fiber lifecycle / failure-sanitation oracles (#70 Stage 4, oracle family
//! C): activation-raise state machine, FAILED semantics, TEARDOWN_VIOLATED
//! latch, and no-auto-retry.
//!
//! Semantic authority: composition-kernel-0-design.md §F (lifecycle), §G.6
//! (teardown contract violations), §O (failure semantics).

mod common;

use common::*;
use qianqian_composition::{ComponentSpec, CompositionKernel, DesiredEntry, Discharge, Revision};

fn mount_all(k: &mut CompositionKernel, entries: Vec<DesiredEntry>) {
    k.set_desired(entries).expect("legal desired composition");
    k.settle();
}

/// Activation raising after 0 effects: clean unwind earns FAILED, no ghost.
#[test]
fn raise_with_zero_effects_lands_failed() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(failing_after_effects("f", 0, &l))
        .expect("component registered");
    mount_all(
        &mut k,
        vec![DesiredEntry::enabled("f", "f", Revision::fresh())],
    );
    let snap = k.snapshot();
    let f = snap.fibers.get("f").expect("fiber present");
    assert_eq!(f.state, qianqian_composition::FiberState::Failed);
    assert!(f.failed_outcome, "FAILED records the discharged failure");
    assert!(!f.teardown_violated);
    assert!(snap.quiet, "FAILED is quiet-legal (§L.1 clause 3)");
}

/// Activation raising after 1 and N effects: partial effects unwind LIFO,
/// nothing installs, FAILED is earned only by full discharge (Cor 69).
#[test]
fn raise_after_n_effects_unwinds_lifo_then_fails() {
    for n in [1usize, 3] {
        let mut k = CompositionKernel::new();
        let l = log();
        k.register_component(failing_after_effects("f", n, &l))
            .expect("component registered");
        mount_all(
            &mut k,
            vec![DesiredEntry::enabled("f", "f", Revision::fresh())],
        );
        let seq = entries(&l);
        let disposes: Vec<_> = seq.iter().filter(|e| e.starts_with("f:dispose")).collect();
        let expected: Vec<String> = (0..n).rev().map(|i| format!("f:dispose{i}")).collect();
        let actual: Vec<String> = disposes.iter().map(|s| s.to_string()).collect();
        assert_eq!(actual, expected, "unwind must be strictly LIFO (n={n})");
        let snap = k.snapshot();
        let f = snap.fibers.get("f").expect("fiber present");
        assert_eq!(f.state, qianqian_composition::FiberState::Failed);
        // No ghost: no provisions or relations survive a failed activation.
        assert!(snap.relations.is_empty());
        assert!(snap.provisions.values().all(|v| v.is_empty()));
    }
}

/// A violated unwind during a failed activation stays latched in Unloading:
/// FAILED is unreachable, the run is not quiescent (§G.6 second failure site).
#[test]
fn violated_raise_unwind_latches_and_never_reaches_failed() {
    let mut k = CompositionKernel::new();
    let l = log();
    // The component registers one violating effect and then raises: the
    // raise's partial unwind hits the violated inverse.
    let spec = {
        let lg = l.clone();
        ComponentSpec::new("f").on_activate(move |ctx| {
            lg.borrow_mut().push("f:effect".to_owned());
            ctx.register_effect(|| Discharge::Violated);
            Err(qianqian_composition::ActivationError::new(
                "fixture failure",
            ))
        })
    };
    k.register_component(spec).expect("component registered");
    mount_all(
        &mut k,
        vec![DesiredEntry::enabled("f", "f", Revision::fresh())],
    );
    let snap = k.snapshot();
    let f = snap.fibers.get("f").expect("fiber present");
    assert_eq!(f.state, qianqian_composition::FiberState::Unloading);
    assert!(f.teardown_violated, "TEARDOWN_VIOLATED latches");
    assert!(
        !f.failed_outcome,
        "FAILED is unreachable for a violated unwind"
    );
    assert!(!snap.quiet, "a latched violation can never satisfy quiet");
    // Step is blocked, not silently progressing.
    assert_eq!(
        k.step(),
        qianqian_composition::StepOutcome::Blocked,
        "the latched run must be loudly blocked"
    );
}

/// A violated teardown during ordinary unloading: episode stays open, the
/// provider's final release stays blocked (relied stays true), no quiescence.
#[test]
fn violated_dependent_teardown_blocks_provider_final_release() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_provider("p", "p1", &l))
        .expect("component registered");
    k.register_component(violating_teardown_consumer("c", &l))
        .expect("component registered");
    mount_all(
        &mut k,
        vec![
            DesiredEntry::enabled("p", "p", Revision::fresh()),
            DesiredEntry::enabled("c", "c", Revision::fresh()),
        ],
    );
    assert!(k.snapshot().quiet);

    // Withdraw the provider: the consumer's teardown obligation fails.
    k.set_desired(vec![DesiredEntry::enabled("c", "c", Revision::fresh())])
        .expect("legal");
    k.settle();

    let snap = k.snapshot();
    let c = snap.fibers.get("c").expect("consumer present");
    assert_eq!(c.state, qianqian_composition::FiberState::Unloading);
    assert!(c.teardown_violated);
    assert!(!snap.quiet);
    // The provider must NOT have final-released: it is still installed and
    // still Unloading, latched behind the violated dependent (§G.6).
    let p = snap.fibers.get("p").expect("provider still installed");
    assert_eq!(p.state, qianqian_composition::FiberState::Unloading);
    assert!(
        !entries(&l).contains(&"p:released".to_owned()),
        "provider final release must remain blocked past an undischarged dependent"
    );
    // No new resolution may commit to the withdrawing provider (L-Leave).
    assert_eq!(snap.capabilities.get("Tag"), Some(&None));
    assert_eq!(
        k.step(),
        qianqian_composition::StepOutcome::Blocked,
        "progress is deliberately forfeited on the violated edge"
    );
}

/// The violating-consumer fixture used above: requires Tag, its declared
/// domain teardown obligation fails to discharge.
fn violating_teardown_consumer(name: &'static str, l: &Log) -> ComponentSpec {
    let log = l.clone();
    ComponentSpec::new(name)
        .requires::<Tag>()
        .on_teardown(move |_| {
            log.borrow_mut().push(format!("{name}:teardown-violated"));
            Discharge::Violated
        })
}

/// FAILED never auto-retries: dependency churn and repeated reconciles leave
/// the FAILED generation exactly where it is (B19, R2). (D1/D2 exercise this
/// in depth in the revision oracles; here the sibling-independence facet.)
#[test]
fn failed_fiber_does_not_disturb_siblings() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(failing_after_effects("f", 0, &l))
        .expect("component registered");
    k.register_component(tag_provider("p", "p1", &l))
        .expect("component registered");
    mount_all(
        &mut k,
        vec![
            DesiredEntry::enabled("f", "f", Revision::fresh()),
            DesiredEntry::enabled("p", "p", Revision::fresh()),
        ],
    );
    let snap = k.snapshot();
    assert_eq!(
        snap.fibers.get("f").map(|f| f.state),
        Some(qianqian_composition::FiberState::Failed)
    );
    assert_eq!(
        snap.fibers.get("p").map(|f| f.state),
        Some(qianqian_composition::FiberState::Active),
        "siblings keep running (B19)"
    );
    assert!(snap.quiet);
}
