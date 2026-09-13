//! Staged-mount single-source oracles (FV-TEMP-0 corrective).
//!
//! Semantic authority: composition-kernel-0-design.md §E.4 (pointwise
//! single-source: "An Unloading old fiber is still installed, so inserting
//! an overlapping new provider before the old is removed would violate the
//! registry invariant outright") and §L.2 (a latched teardown violation
//! means reconcile "issues no further requests through the affected edge").
//!
//! The TLA+ counterexample (specs/composition-kernel-0/evidence/
//! m5-counterexample-trace.md): a §G.6-latched fiber can never reach removal,
//! so the step-priority staging alone cannot uphold pointwise single-source —
//! `mount_candidate` must additionally withhold any mount whose declared
//! provisions overlap an installed fiber's.

mod common;

use std::rc::Rc;

use common::*;
use qianqian_composition::{
    ComponentSpec, CompositionKernel, DesiredEntry, Discharge, FiberState, Revision,
};

fn compose(k: &mut CompositionKernel, entries: Vec<DesiredEntry>) {
    k.set_desired(entries).expect("legal desired composition");
    k.settle();
}

/// Variant 1 (provider latched): the provider's own activation raises after
/// installing its provision and registering a violating effect, so the
/// partial unwind latches §G.6 with the provision tombstone retained (§K.4).
/// The desired swap to a replacement provider of the same capability must
/// NOT mount the replacement while the latched fiber is still installed.
#[test]
fn replacement_provider_mount_stays_withheld_behind_latched_provider() {
    let mut k = CompositionKernel::new();
    let l = log();
    let p1 = ComponentSpec::new("p1")
        .provides::<Tag>()
        .on_activate(|ctx| {
            ctx.provide::<Tag>(Rc::new(FixedTag("one")))
                .expect("provides declared");
            ctx.register_effect(|| Discharge::Violated);
            Err(qianqian_composition::ActivationError::new(
                "fixture failure",
            ))
        });
    k.register_component(p1).expect("component registered");
    k.register_component(tag_provider("p2", "two", &l))
        .expect("component registered");

    compose(
        &mut k,
        vec![DesiredEntry::enabled("p1", "p1", Revision::fresh())],
    );
    let snap = k.snapshot();
    let p1f = snap.fibers.get("p1").expect("p1 present");
    assert_eq!(p1f.state, FiberState::Unloading);
    assert!(p1f.teardown_violated, "§G.6 latched by the violated unwind");
    assert!(
        snap.provisions.get("Tag").is_some_and(|p| p.contains("p1")),
        "provision tombstone stays observable (§K.4)"
    );

    // Staged replacement of the same capability while p1 is latched.
    compose(
        &mut k,
        vec![DesiredEntry::enabled("p2", "p2", Revision::fresh())],
    );
    let snap = k.snapshot();
    assert!(
        !snap.fibers.contains_key("p2"),
        "p2 must not mount while the latched p1 still declares Tag: at every \
         point of any legal K0 history at most one installed fiber declares \
         provision for a capability (§E.4), and the affected edge is frozen \
         while the violation is latched (§L.2)"
    );
    let providers = snap.provisions.get("Tag").expect("Tag known");
    assert_eq!(
        providers.len(),
        1,
        "pointwise single-source holds by inspection, not only at quiescence"
    );
    assert!(providers.contains("p1"));
}

/// Variant 2 (consumer latched, §G.6 Scenario A): a clean provider + a
/// consumer whose teardown obligation violates. After the desired swap, the
/// provider withdraws and drains its dependents, but the consumer's open
/// committed view keeps the provider's `¬relied` guard latched forever — the
/// provider can never reach removal, so the replacement must not mount.
#[test]
fn replacement_provider_mount_stays_withheld_behind_violation_latched_guard() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_provider("p1", "one", &l))
        .expect("component registered");
    k.register_component(tag_provider("p2", "two", &l))
        .expect("component registered");
    let violating_consumer = ComponentSpec::new("c")
        .requires::<Tag>()
        .on_teardown(|_ctx| Discharge::Violated);
    k.register_component(violating_consumer)
        .expect("component registered");

    compose(
        &mut k,
        vec![
            DesiredEntry::enabled("p1", "p1", Revision::fresh()),
            DesiredEntry::enabled("c", "c", Revision::fresh()),
        ],
    );
    let snap = k.snapshot();
    let c = snap.fibers.get("c").expect("c present");
    assert_eq!(c.state, FiberState::Active);

    compose(
        &mut k,
        vec![DesiredEntry::enabled("p2", "p2", Revision::fresh())],
    );
    let snap = k.snapshot();
    let cf = snap.fibers.get("c").expect("c still present");
    assert_eq!(cf.state, FiberState::Unloading);
    assert!(
        cf.teardown_violated,
        "consumer's teardown violates (§G.6-A)"
    );
    let p1f = snap.fibers.get("p1").expect("p1 still present");
    assert_eq!(
        p1f.state,
        FiberState::Unloading,
        "relied guard latched: the \
        open committed view keeps p1 in the teardown window (§G.6)"
    );
    assert!(
        !snap.fibers.contains_key("p2"),
        "p2 must not mount while the violation-latched guard keeps p1 \
         installed (§E.4 pointwise single-source; §L.2)"
    );
}
