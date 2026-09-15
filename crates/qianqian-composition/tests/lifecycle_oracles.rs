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

/// Which §G.6 failure locus the fixture violates. Both loci latch
/// TEARDOWN_VIOLATED from inside `unload_fiber`, but they differ in
/// representation: the effect-bearing locus stops the unwind with a
/// non-empty accumulator (provenance tombstone), while the teardown
/// closure locus latches on a fully discharged (empty) accumulator with
/// no tombstone record at all.
#[derive(Clone, Copy, Debug)]
enum ViolationLocus {
    /// Shape A — violated owned-effect inverse during the unload unwind.
    EffectInverse,
    /// Shape B — component teardown closure returns Violated with an
    /// empty effect accumulator.
    TeardownClosure,
}

fn violating_consumer(locus: ViolationLocus, name: &'static str, l: &Log) -> ComponentSpec {
    match locus {
        ViolationLocus::EffectInverse => {
            let log = l.clone();
            ComponentSpec::new(name)
                .requires::<Tag>()
                .on_activate(move |ctx| {
                    let lg = log.clone();
                    ctx.register_effect(move || {
                        lg.borrow_mut().push(format!("{name}:inverse-violated"));
                        Discharge::Violated
                    });
                    Ok(())
                })
        }
        ViolationLocus::TeardownClosure => violating_teardown_consumer(name, l),
    }
}

/// §G.6 violation-latch semantic family — cross-locus refinement oracle.
///
/// The TLA model (`specs/composition-kernel-0`) directly explores only the
/// effect-bearing witness: its `TombstoneRetained` invariant assumes every
/// violation latch keeps the accumulator non-empty. The second production
/// failure locus — a component teardown closure returning `Violated` on an
/// EMPTY accumulator (kernel.rs `unload_fiber`, after a fully discharged
/// unwind) — is deliberately NOT separately modeled. This oracle carries
/// the production refinement coverage: it drives both loci through the
/// identical scenario (withdraw provider → latch → attempt same-capability
/// replacement → dispose_root) and asserts the same K0 semantic
/// consequences for each:
///
/// ```text
/// violation latched and loud      fiber installed, Unloading, FAILED
///                                 unreachable, diagnostics flag set
/// episode stays open              the committed binding remains projected
/// provider guard conservative     provider never final-releases
/// removal blocked                 dispose_root + settle remove nothing
/// replacement cannot overlap      a second Tag provider is never mounted
/// settle ends Blocked             step() reports Blocked, quiet stays false
/// ```
///
/// Representation differences (tombstone / accumulator depth) are
/// deliberately NOT asserted identical — only the semantic family is.
/// FORMALIZATION_NOT_EARNED record: specs/composition-kernel-0/RESULTS.md
/// §4 (no independent interleaving collision was found for the teardown
/// closure locus, so no TLA extension was added).
#[test]
fn violation_latch_semantic_family_is_locus_invariant() {
    for locus in [
        ViolationLocus::EffectInverse,
        ViolationLocus::TeardownClosure,
    ] {
        let mut k = CompositionKernel::new();
        let l = log();
        k.register_component(tag_provider("p", "p1", &l))
            .expect("component registered");
        k.register_component(violating_consumer(locus, "c", &l))
            .expect("component registered");
        // A same-capability replacement provider, registered but not
        // desired until after the latch is held.
        k.register_component(tag_provider("p2", "p2", &l))
            .expect("component registered");
        mount_all(
            &mut k,
            vec![
                DesiredEntry::enabled("p", "p", Revision::fresh()),
                DesiredEntry::enabled("c", "c", Revision::fresh()),
            ],
        );
        assert!(k.snapshot().quiet, "clean mount baseline must be quiet");

        // Withdraw the provider: the consumer's teardown contract fails
        // (locus-specific), latching §G.6 while its episode stays open.
        k.set_desired(vec![DesiredEntry::enabled("c", "c", Revision::fresh())])
            .expect("legal desired composition");
        k.settle();

        // Attempt a same-capability replacement while the latch is held.
        k.set_desired(vec![
            DesiredEntry::enabled("c", "c", Revision::fresh()),
            DesiredEntry::enabled("p2", "p2", Revision::fresh()),
        ])
        .expect("legal desired composition");
        k.settle();

        // dispose_root: even full withdrawal must not remove a latched fiber.
        k.set_desired(Vec::new())
            .expect("legal desired composition");
        k.settle();

        let snap = k.snapshot();
        // Locus evidence: each scenario must have exercised its own named
        // failure path (guards the oracle against silent vacuity).
        match locus {
            ViolationLocus::EffectInverse => {
                assert!(entries(&l).contains(&"c:inverse-violated".to_owned()));
                assert!(!entries(&l).contains(&"c:teardown-violated".to_owned()));
            }
            ViolationLocus::TeardownClosure => {
                assert!(entries(&l).contains(&"c:teardown-violated".to_owned()));
                assert!(!entries(&l).contains(&"c:inverse-violated".to_owned()));
            }
        }
        let c = snap
            .fibers
            .get("c")
            .expect("a §G.6-latched fiber stays installed");
        assert_eq!(c.state, qianqian_composition::FiberState::Unloading);
        assert!(c.teardown_violated, "TEARDOWN_VIOLATED latches loudly");
        assert!(
            !c.failed_outcome,
            "FAILED is unreachable from either violation locus"
        );
        // The episode stays open: the committed binding remains projected
        // (§I.1 surface 3 — a latched episode must not pretend it closed).
        assert_eq!(
            snap.committed.get("c").and_then(|b| b.get("Tag")),
            Some(&"p".to_owned()),
            "the open committed view must stay visible for locus {locus:?}"
        );
        // The provider guard stays conservative: final release blocked.
        let p = snap
            .fibers
            .get("p")
            .expect("the guarded provider stays installed");
        assert_eq!(p.state, qianqian_composition::FiberState::Unloading);
        assert!(
            !entries(&l).contains(&"p:released".to_owned()),
            "provider final release must stay blocked for locus {locus:?}"
        );
        // Replacement cannot overlap the violated edge: the mount of the
        // second Tag provider stays withheld (§E.4/§L.2).
        assert!(
            !snap.fibers.contains_key("p2"),
            "a replacement provider must not mount over a violated edge"
        );
        assert_eq!(
            snap.capabilities.get("Tag"),
            Some(&None),
            "no resolvable Tag source while the violated edge is frozen"
        );
        // Settle ends Blocked rather than silently progressing.
        assert!(!snap.quiet, "a latched violation can never satisfy quiet");
        assert_eq!(
            k.step(),
            qianqian_composition::StepOutcome::Blocked,
            "progress is deliberately forfeited on the violated edge"
        );
    }
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
