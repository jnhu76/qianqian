//! Provider withdrawal oracles (#70 Stage 8, P0): the frozen §G sequence,
//! the teardown-access window (Thm 70 / B14), and the mandatory adversarial
//! dependent-teardown-needs-provider case with no use-after-provider-destroy.
//!
//! Semantic authority: composition-kernel-0-design.md §G.

mod common;

use common::*;
use qianqian_kernel::{ComponentSpec, DesiredEntry, Discharge, Kernel, Revision, StepOutcome};

/// A consumer whose declared domain teardown obligation requires its
/// provider: it closes a provider-backed resource through the committed
/// view inside the window (§G.3 t4b).
fn teardown_access_consumer(name: &'static str, l: &Log) -> ComponentSpec {
    let log = l.clone();
    let log_teardown = l.clone();
    ComponentSpec::new(name)
        .requires::<Tag>()
        .on_activate(move |ctx| {
            let binding = ctx
                .resolve::<Tag>()
                .map_err(|e| qianqian_kernel::ActivationError::new(format!("{e:?}")))?;
            log.borrow_mut().push(format!(
                "{name}:opened-handle-on-{}",
                binding.service().tag()
            ));
            Ok(())
        })
        .on_teardown(move |ctx| {
            // Teardown access: the committed view stays readable (B14).
            match ctx.resolve_committed::<Tag>() {
                Ok(svc) => log_teardown
                    .borrow_mut()
                    .push(format!("{name}:closed-handle-on-{}", svc.tag())),
                Err(e) => log_teardown
                    .borrow_mut()
                    .push(format!("{name}:teardown-access-failed:{e:?}")),
            }
            Discharge::Discharged
        })
}

/// The full §G sequence is observable, in order, with each staging boundary
/// visible between steps:
///
/// provider begins withdrawal → provider removed from NEW resolution →
/// dependents invalidated → dependents enter teardown → dependent-owned
/// effects unwind → dependent episode closes → provider guard releases →
/// provider final unload → removal.
#[test]
fn withdrawal_sequence_is_ordered_and_observable() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_provider("p", "p1", &l))
        .expect("component registered");
    k.register_component(teardown_access_consumer("c", &l))
        .expect("component registered");
    let r_c = Revision::fresh();
    k.set_desired(vec![
        DesiredEntry::enabled("p", "p", Revision::fresh()),
        DesiredEntry::enabled("c", "c", r_c),
    ])
    .expect("legal");
    k.settle();
    assert!(entries(&l).contains(&"c:opened-handle-on-p1".to_owned()));

    // 1. Provider begins withdrawal. The consumer keeps its desired
    // incarnation — only the provider's entry is withdrawn.
    k.set_desired(vec![DesiredEntry::enabled("c", "c", r_c)])
        .expect("legal");
    assert_eq!(k.step(), StepOutcome::Transitioned); // retire flag on p

    // 2. L-Leave: p stops satisfying NEW resolution while still installed.
    assert_eq!(k.step(), StepOutcome::Transitioned); // p diverts to Unloading
    let snap = k.snapshot();
    assert_eq!(
        snap.capabilities.get("Tag"),
        Some(&None),
        "a withdrawing provider must disappear from new resolution"
    );
    assert_eq!(
        snap.provisions.get("Tag").map(|v| v.len()),
        Some(1),
        "but its provision is still installed (final release has not run)"
    );

    // 3. Dependents invalidated.
    assert_eq!(k.step(), StepOutcome::Transitioned); // c diverts to Unloading
    assert_eq!(
        k.snapshot().fibers.get("c").map(|f| f.state),
        Some(qianqian_kernel::FiberState::Unloading)
    );

    // 4–6. Dependents unwind + discharge inside the window; the dependent
    // episode closes; the provider's guard releases.
    assert_eq!(k.step(), StepOutcome::Transitioned); // c unloads (with access)
    assert!(
        entries(&l).iter().any(|e| e == "c:closed-handle-on-p1"),
        "the dependent's teardown access must succeed inside the window"
    );
    assert_eq!(
        k.snapshot().fibers.get("p").map(|f| f.state),
        Some(qianqian_kernel::FiberState::Unloading),
        "the provider is still not final-released before the dependent closed"
    );

    // 7. Provider guard released → final unload.
    assert_eq!(k.step(), StepOutcome::Transitioned); // p unloads
    assert!(entries(&l).contains(&"p:released".to_owned()));
    // 8. Removal.
    assert_eq!(k.step(), StepOutcome::Transitioned); // p removed
    k.settle();
    let snap = k.snapshot();
    assert!(snap.quiet);
    assert!(!snap.fibers.contains_key("p"));

    // Order invariant: the dependent's teardown completed strictly before
    // the provider's final release (Thm 70(2)).
    let log = entries(&l);
    let dependent_closed = log
        .iter()
        .position(|e| e == "c:closed-handle-on-p1")
        .expect("dependent teardown logged");
    let provider_released = log
        .iter()
        .position(|e| e == "p:released")
        .expect("provider release logged");
    assert!(dependent_closed < provider_released);
}

/// Mandatory adversarial case: A provides X; B requires X and holds a
/// resource that needs A during teardown; withdraw A. B must still finish
/// teardown before A final-destroys — no use-after-provider-destroy window
/// (§G.6 scenario A's ordering premise, discharged variant).
#[test]
fn dependent_teardown_needing_provider_completes_before_final_release() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_provider("a", "A", &l))
        .expect("component registered");
    k.register_component(teardown_access_consumer("b", &l))
        .expect("component registered");
    // A deeper chain: c requires b's capability too — invalidation cascades
    // transitively (§G.2 chain row).
    k.register_component(tag_consumer("grand", &l))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("a", "a", Revision::fresh()),
        DesiredEntry::enabled("b", "b", Revision::fresh()),
        // grand also depends on a directly: two dependents of the provider.
        DesiredEntry::enabled("grand", "grand", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    assert!(k.snapshot().quiet);

    k.set_desired(Vec::new()).expect("legal");
    k.settle();

    let log = entries(&l);
    // Both dependents discharged inside the window.
    let b_closed = log
        .iter()
        .position(|e| e == "b:closed-handle-on-A")
        .expect("b discharged with teardown access");
    let a_released = log
        .iter()
        .position(|e| e == "a:released")
        .expect("a finally released");
    assert!(
        b_closed < a_released,
        "no dependent discharge may follow the provider's final release"
    );
    // Every dependent closed before release; no teardown-access failures.
    assert!(
        !log.iter().any(|e| e.contains("teardown-access-failed")),
        "the teardown window must make provider-backed teardown possible"
    );
    assert!(k.snapshot().quiet);
    assert!(k.snapshot().fibers.is_empty());
}

/// A provider with a real service value: the value stays readable through
/// the dependent's whole teardown and dies only with the provider's own
/// unload (the §G.2 'what remains reachable' answer).
#[test]
fn withdrawing_provider_service_readable_until_dependents_close() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_provider("p", "val", &l))
        .expect("component registered");
    k.register_component(teardown_access_consumer("c", &l))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("p", "p", Revision::fresh()),
        DesiredEntry::enabled("c", "c", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();

    // Withdraw the CONSUMER first: its window is its own; the provider
    // simply stays ACTIVE.
    k.set_desired(vec![DesiredEntry::enabled("p", "p", Revision::fresh())])
        .expect("legal");
    k.settle();
    let snap = k.snapshot();
    assert_eq!(
        snap.fibers.get("p").map(|f| f.state),
        Some(qianqian_kernel::FiberState::Active),
        "a pure-consumer withdrawal must not disturb the provider"
    );
    assert_eq!(snap.capabilities.get("Tag"), Some(&Some("p".to_owned())));
}
