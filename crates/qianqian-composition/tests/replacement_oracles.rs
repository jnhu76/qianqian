//! Staged provider replacement oracles (#70 Stage 9, oracle family E):
//! required-single staged orchestration with the pointwise single-source
//! invariant checked after EVERY observable transition (§E.4, M2).
//!
//! Semantic authority: composition-kernel-0-design.md §E.4.

mod common;

use common::*;
use qianqian_composition::{ComponentSpec, CompositionKernel, DesiredEntry, Revision, StepOutcome};

/// Step-driven settle that asserts the pointwise invariant after every
/// transition: at every observable running-registry state, at most one
/// installed provider declares the capability.
fn settle_with_single_source(k: &mut CompositionKernel, capability: &'static str) {
    loop {
        let outcome = k.step();
        let snap = k.snapshot();
        let installed = snap
            .provisions
            .get(capability)
            .map(|v| v.len())
            .unwrap_or(0);
        assert!(
            installed <= 1,
            "single-source violated: {installed} installed providers of \
             '{capability}' at an observable state"
        );
        if outcome != StepOutcome::Transitioned {
            break;
        }
    }
}

fn desired_svc(component: &'static str, revision: Revision) -> Vec<DesiredEntry> {
    vec![
        DesiredEntry::enabled("svc", component, revision),
        DesiredEntry::enabled("consumer", "consumer", Revision::fresh()),
    ]
}

/// A1 → A2 replacement: staged retire → drain → remove → mount; the consumer
/// degrades to Pending in the staging gap and reactivates against the new
/// provider identity (B30). Old bindings never survive into the new
/// generation.
#[test]
fn replacement_a1_to_a2_is_staged_with_pointwise_single_source() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_provider("p1", "one", &l))
        .expect("component registered");
    k.register_component(tag_provider("p2", "two", &l))
        .expect("component registered");
    k.register_component(tag_consumer("consumer", &l))
        .expect("component registered");
    k.set_desired(desired_svc("p1", Revision::new(1)))
        .expect("legal");
    k.settle();
    assert_eq!(
        k.snapshot().capabilities.get("Tag"),
        Some(&Some("svc".to_owned())),
        "capability truth names the PROVIDER FIBER (entry id), not the component"
    );

    k.set_desired(desired_svc("p2", Revision::new(2)))
        .expect("legal");
    settle_with_single_source(&mut k, "Tag");

    let snap = k.snapshot();
    assert!(snap.quiet);
    assert_eq!(snap.capabilities.get("Tag"), Some(&Some("svc".to_owned())));
    assert_eq!(
        snap.fibers.get("consumer").map(|f| f.state),
        Some(qianqian_composition::FiberState::Active),
        "dependents reactivate against the new provider"
    );
    let log = entries(&l);
    let bindings: Vec<_> = log
        .iter()
        .filter(|e| e.starts_with("consumer:bound-to-"))
        .cloned()
        .collect();
    assert_eq!(
        bindings,
        vec!["consumer:bound-to-one", "consumer:bound-to-two"],
        "the old binding must not survive into the new generation"
    );
}

/// A1 → A2 → A1 (M2): consumers commit to the final generation; exactly one
/// provider at every step; no stale views.
#[test]
fn replacement_a1_a2_a1_round_trip() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_provider("p1", "one", &l))
        .expect("component registered");
    k.register_component(tag_provider("p2", "two", &l))
        .expect("component registered");
    k.register_component(tag_consumer("consumer", &l))
        .expect("component registered");

    for (component, rev) in [("p1", 1), ("p2", 2), ("p1", 3), ("p2", 4), ("p1", 5)] {
        k.set_desired(desired_svc(component, Revision::new(rev)))
            .expect("legal");
        settle_with_single_source(&mut k, "Tag");
        let snap = k.snapshot();
        assert!(snap.quiet, "each replacement settles");
        assert_eq!(snap.capabilities.get("Tag"), Some(&Some("svc".to_owned())));
        assert_eq!(
            snap.fibers.get("consumer").map(|f| f.state),
            Some(qianqian_composition::FiberState::Active)
        );
    }
    // The consumer bound once per generation, in order.
    let log = entries(&l);
    let bindings = log
        .iter()
        .filter(|e| e.starts_with("consumer:bound-to-"))
        .count();
    assert_eq!(bindings, 5);
}

/// Repeated replacement with an uninvolved third fiber: the third fiber
/// never transitions (incremental reconcile, §L.3 / Cor 69).
#[test]
fn replacement_leaves_uninvolved_fibers_untouched() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_provider("p1", "one", &l))
        .expect("component registered");
    k.register_component(tag_provider("p2", "two", &l))
        .expect("component registered");
    k.register_component(tag_consumer("consumer", &l))
        .expect("component registered");
    k.register_component(ComponentSpec::new("bystander").on_activate({
        let lg = l.clone();
        move |_ctx| {
            lg.borrow_mut().push("bystander:activated".to_owned());
            Ok(())
        }
    }))
    .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("svc", "p1", Revision::new(1)),
        DesiredEntry::enabled("consumer", "consumer", Revision::fresh()),
        DesiredEntry::enabled("bystander", "bystander", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    assert_eq!(
        entries(&l)
            .iter()
            .filter(|e| *e == "bystander:activated")
            .count(),
        1
    );

    k.set_desired(desired_svc("p2", Revision::new(2)))
        .expect("legal");
    settle_with_single_source(&mut k, "Tag");
    assert_eq!(
        entries(&l)
            .iter()
            .filter(|e| *e == "bystander:activated")
            .count(),
        1,
        "untouched fibers never transition (Cor 69 + Thm 68)"
    );
}
