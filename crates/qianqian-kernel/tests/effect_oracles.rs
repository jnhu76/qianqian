//! Effect / provenance oracles (#70 Stage 5, oracle families B and E):
//! LIFO unwind, same-key contribution independence, owner-local effects
//! without fabricated relations, double-dispose idempotence.
//!
//! Semantic authority: composition-kernel-0-design.md §H (effect model),
//! §D.4 (structural provenance, Corrective-5).
//!
//! Contributions are observed through the kernel's relation diagnostics —
//! the projection of the single Effect authority (§K.4) — never through
//! token values or private identity.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use qianqian_kernel::{
    ComponentSpec, DesiredEntry, Discharge, Kernel, RelationDiagnostic, Revision,
};

// A consumer that contributes a listener AND consumes `Tag`: its
/// contribution cycles when its Tag provider is replaced (B30).
fn listener_consumer_with_tag(name: &'static str, l: &Log) -> ComponentSpec {
    let log = l.clone();
    ComponentSpec::new(name)
        .requires::<Listeners>()
        .requires::<Tag>()
        .on_activate(move |ctx| {
            let binding = ctx
                .resolve::<Listeners>()
                .map_err(|e| qianqian_kernel::ActivationError::new(format!("{e:?}")))?;
            let token = binding.service().register(name);
            ctx.register_relation(&binding, move || {
                token.unregister();
                Discharge::Discharged
            });
            let tag = ctx
                .resolve::<Tag>()
                .map_err(|e| qianqian_kernel::ActivationError::new(format!("{e:?}")))?;
            log.borrow_mut()
                .push(format!("{name}:bound-to-{}", tag.service().tag()));
            Ok(())
        })
}

fn relation(owner: &str) -> RelationDiagnostic {
    RelationDiagnostic {
        owner: owner.to_owned(),
        provider: "registry".to_owned(),
        capability: "Listeners",
    }
}

// ---------------------------------------------------------------------------
// Oracles
// ---------------------------------------------------------------------------

/// Owner-local effects unwind strictly LIFO within the owner's episode.
#[test]
fn owner_local_effects_unwind_lifo() {
    let mut k = Kernel::new();
    let l = log();
    let spec = {
        let lg = l.clone();
        ComponentSpec::new("w").on_activate(move |ctx| {
            for i in 0..3 {
                let l2 = lg.clone();
                lg.borrow_mut().push(format!("w:register{i}"));
                ctx.register_effect(move || {
                    l2.borrow_mut().push(format!("w:dispose{i}"));
                    Discharge::Discharged
                });
            }
            Ok(())
        })
    };
    k.register_component(spec);
    k.set_desired(vec![DesiredEntry::enabled("w", "w", Revision::fresh())])
        .expect("legal");
    k.settle();
    // Withdraw: the accumulator applies in reverse.
    k.set_desired(Vec::new()).expect("legal");
    k.settle();
    assert_eq!(
        entries(&l),
        vec![
            "w:register0",
            "w:register1",
            "w:register2",
            "w:dispose2",
            "w:dispose1",
            "w:dispose0",
        ]
    );
    let snap = k.snapshot();
    assert!(snap.fibers.is_empty(), "removed cleanly");
    assert!(snap.quiet);
}

/// Same-key contribution safety: removing fiber A leaves fiber B's
/// contribution untouched; foreign contributions survive (§H.4, M5).
#[test]
fn same_key_contribution_removal_leaves_foreign_contribution() {
    let mut k = Kernel::new();
    k.register_component(listeners_provider("registry"));
    k.register_component(listener_consumer("a"));
    k.register_component(listener_consumer("b"));
    k.set_desired(vec![
        DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        DesiredEntry::enabled("a", "a", Revision::fresh()),
        DesiredEntry::enabled("b", "b", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();

    let rel = |k: &Kernel| k.snapshot().relations.into_iter().collect::<Vec<_>>();
    assert_eq!(rel(&k), vec![relation("a"), relation("b")]);

    // Remove A: B's contribution must remain, exactly and alone.
    k.set_desired(vec![
        DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        DesiredEntry::enabled("b", "b", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    assert_eq!(rel(&k), vec![relation("b")]);
    assert!(k.snapshot().quiet);
}

/// Owner-local effect exists with NO fabricated capability key, no provider,
/// no peer (§D.4 Corrective-5): registered, unwound, and absent from the
/// relation diagnostics.
#[test]
fn owner_local_effect_carries_no_relation() {
    let mut k = Kernel::new();
    let l = log();
    let spec = {
        let lg = l.clone();
        ComponentSpec::new("local").on_activate(move |ctx| {
            let l2 = lg.clone();
            ctx.register_effect(move || {
                l2.borrow_mut().push("local:disposed".to_owned());
                Discharge::Discharged
            });
            Ok(())
        })
    };
    k.register_component(spec);
    k.set_desired(vec![DesiredEntry::enabled(
        "local",
        "local",
        Revision::fresh(),
    )])
    .expect("legal");
    k.settle();

    // Live effect: no relation row may appear for the owner-local effect.
    let snap = k.snapshot();
    assert!(snap.relations.is_empty());
    assert!(snap.provisions.values().all(|v| v.is_empty()));

    k.set_desired(Vec::new()).expect("legal");
    k.settle();
    assert_eq!(entries(&l), vec!["local:disposed"]);
    assert!(k.snapshot().fibers.is_empty());
}

/// Double dispose is an idempotent no-op (B26), and effects cannot fire
/// after their episode ends.
#[test]
fn double_dispose_is_idempotent() {
    let mut k = Kernel::new();
    let l = log();
    let handle_cell: Rc<RefCell<Option<qianqian_kernel::EffectHandle>>> =
        Rc::new(RefCell::new(None));
    let spec = {
        let lg = l.clone();
        let hc = handle_cell.clone();
        ComponentSpec::new("d").on_activate(move |ctx| {
            let l2 = lg.clone();
            let h = ctx.register_effect(move || {
                l2.borrow_mut().push("d:disposed".to_owned());
                Discharge::Discharged
            });
            *hc.borrow_mut() = Some(h);
            // Explicit dispose now; a second dispose of the same handle is a
            // no-op — the accumulator no longer contains the effect.
            ctx.dispose(h);
            ctx.dispose(h);
            Ok(())
        })
    };
    k.register_component(spec);
    k.set_desired(vec![DesiredEntry::enabled("d", "d", Revision::fresh())])
        .expect("legal");
    k.settle();
    assert_eq!(entries(&l), vec!["d:disposed"], "inverse ran exactly once");

    // Unload: nothing left to unwind — no second discharge.
    k.set_desired(Vec::new()).expect("legal");
    k.settle();
    assert_eq!(entries(&l), vec!["d:disposed"]);
    assert!(k.snapshot().quiet);
}

/// Unrelated Y-side contribution survives X-provider churn (M6 shape): Y
/// diverts and reactivates through X's staged replacement, and at quiescence
/// Y's relation truth equals its pre-churn truth (independence, §H.2).
#[test]
fn y_contribution_survives_x_provider_churn() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_provider("x", "x1", &l));
    k.register_component(listeners_provider("registry"));
    k.register_component(listener_consumer_with_tag("y", &l));
    let desired = |rev: Revision| {
        vec![
            DesiredEntry::enabled("registry", "registry", Revision::fresh()),
            DesiredEntry::enabled("x", "x", rev),
            DesiredEntry::enabled("y", "y", Revision::fresh()),
        ]
    };
    k.set_desired(desired(Revision::new(1))).expect("legal");
    k.settle();
    let before = k.snapshot().relations.clone();

    // X churn: staged replacement x@R1 -> x@R2.
    k.set_desired(desired(Revision::new(2))).expect("legal");
    k.settle();

    let after = k.snapshot().relations.clone();
    assert_eq!(before, after, "Y's relation truth must survive X churn");
    assert!(k.snapshot().quiet);
    // The consumer did cycle through the churn (B30 reactivation evidence).
    assert_eq!(
        entries(&l).iter().filter(|e| *e == "y:bound-to-x1").count(),
        2
    );
}

/// Removing a totally unrelated X fiber leaves Y untouched — Y does not even
/// divert: its contribution is preserved, not torn down and recreated.
#[test]
fn removal_of_unrelated_x_leaves_y_untouched() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_provider("x", "x1", &l));
    k.register_component(listeners_provider("registry"));
    k.register_component(listener_consumer("y"));
    k.set_desired(vec![
        DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        DesiredEntry::enabled("x", "x", Revision::fresh()),
        DesiredEntry::enabled("y", "y", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    let before = k.snapshot().relations.clone();

    k.set_desired(vec![
        DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        DesiredEntry::enabled("y", "y", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    let after = k.snapshot().relations.clone();
    assert_eq!(before, after);
    assert!(k.snapshot().quiet);
}
