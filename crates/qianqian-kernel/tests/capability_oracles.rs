//! Capability / Context oracles (#70 Stage 3, oracle family A): required-single
//! reachability, access discipline, and the teardown-access window.
//!
//! Semantic authority: composition-kernel-0-design.md §E (Capability Algebra),
//! oracle families per issue #70 §7 and §15.A.

mod common;

use std::rc::Rc;

use common::*;
use qianqian_kernel::{ActivationError, Capability, ComponentSpec, DesiredEntry, Kernel, Revision};

/// A capability the consumer did not declare — the undeclared-access probe.
struct Undeclared;
impl Capability for Undeclared {
    const NAME: &'static str = "Undeclared";
    type Service = dyn TagService;
}

struct UndeclaredProvider;
impl TagService for UndeclaredProvider {
    fn tag(&self) -> &'static str {
        "undeclared"
    }
}

fn undeclared_provider(name: &'static str) -> ComponentSpec {
    ComponentSpec::new(name)
        .provides::<Undeclared>()
        .on_activate(|ctx| {
            ctx.provide::<Undeclared>(Rc::new(UndeclaredProvider))
                .expect("provides declared");
            Ok(())
        })
}

// --- A. Reachability --------------------------------------------------------

/// Consumer mounted before provider: consumer settles Pending (never ACTIVE,
/// never a root crash), then activates when the provider arrives (B24's
/// loader argument: no load order needed).
#[test]
fn consumer_before_provider_is_pending_then_activates() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_consumer("c", &l))
        .expect("component registered");
    k.register_component(tag_provider("p", "p1", &l))
        .expect("component registered");
    // Only the consumer is desired at first.
    k.set_desired(vec![DesiredEntry::enabled("c", "c", Revision::fresh())])
        .expect("legal");
    k.settle();

    let snap = k.snapshot();
    assert_eq!(snap.capabilities.get("Tag"), Some(&None));
    assert_eq!(
        snap.fibers.get("c").map(|f| f.state),
        Some(qianqian_kernel::FiberState::Pending)
    );
    assert!(snap.quiet, "Pending over a missing provider may be quiet");

    // The provider appears; no orchestration order is arranged — the
    // consumer reacts to its target view becoming satisfiable.
    k.set_desired(vec![
        DesiredEntry::enabled("c", "c", Revision::fresh()),
        DesiredEntry::enabled("p", "p", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    let snap = k.snapshot();
    assert_eq!(snap.capabilities.get("Tag"), Some(&Some("p".to_owned())));
    assert_eq!(
        snap.fibers.get("c").map(|f| f.state),
        Some(qianqian_kernel::FiberState::Active)
    );
    assert!(entries(&l).contains(&"c:bound-to-p1".to_owned()));
}

/// Provider before consumer: consumer activates in the same settle pass.
#[test]
fn provider_before_consumer_activates() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_consumer("c", &l))
        .expect("component registered");
    k.register_component(tag_provider("p", "p1", &l))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("p", "p", Revision::fresh()),
        DesiredEntry::enabled("c", "c", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();

    let snap = k.snapshot();
    assert_eq!(snap.capabilities.get("Tag"), Some(&Some("p".to_owned())));
    assert_eq!(
        snap.fibers.get("c").map(|f| f.state),
        Some(qianqian_kernel::FiberState::Active)
    );
    assert!(entries(&l).contains(&"c:bound-to-p1".to_owned()));
}

/// Provider flap N times (M1 shape): consumers end committed to the FINAL
/// generation; no ghost generations survive.
#[test]
fn provider_flap_n_times_settles_on_final_generation() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_consumer("c", &l))
        .expect("component registered");
    k.register_component(tag_provider("p", "final", &l))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("p", "p", Revision::fresh()),
        DesiredEntry::enabled("c", "c", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();

    // Flap: withdraw, re-present, N = 3 cycles, via enabled/disabled flips
    // with fresh incarnations (explicit operator intent each time).
    for _ in 0..3 {
        k.set_desired(vec![
            DesiredEntry::enabled("p", "p", Revision::fresh()),
            DesiredEntry::enabled("c", "c", Revision::fresh()),
        ])
        .expect("legal");
        // Toggle the provider off through a disabled entry, then drain.
        k.set_desired(vec![
            DesiredEntry::disabled("p", "p", Revision::fresh()),
            DesiredEntry::enabled("c", "c", Revision::fresh()),
        ])
        .expect("legal");
        k.settle();
        let snap = k.snapshot();
        assert_eq!(snap.capabilities.get("Tag"), Some(&None));
        assert_eq!(
            snap.fibers.get("c").map(|f| f.state),
            Some(qianqian_kernel::FiberState::Pending)
        );
        // Re-present enabled with a fresh incarnation.
        k.set_desired(vec![
            DesiredEntry::enabled("p", "p", Revision::fresh()),
            DesiredEntry::enabled("c", "c", Revision::fresh()),
        ])
        .expect("legal");
        k.settle();
        let snap = k.snapshot();
        assert_eq!(snap.capabilities.get("Tag"), Some(&Some("p".to_owned())));
        assert_eq!(
            snap.fibers.get("c").map(|f| f.state),
            Some(qianqian_kernel::FiberState::Active)
        );
    }
    assert!(k.snapshot().quiet);
}

/// Missing dependency settles Pending; the root never crashes (A0 §K.5).
#[test]
fn missing_dependency_settles_pending_without_crash() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_consumer("c", &l))
        .expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled("c", "c", Revision::fresh())])
        .expect("legal: a missing provider is a degraded state, not an error");
    k.settle();
    let snap = k.snapshot();
    assert!(snap.quiet);
    assert_eq!(
        snap.fibers.get("c").map(|f| f.state),
        Some(qianqian_kernel::FiberState::Pending)
    );
    assert_eq!(snap.capabilities.get("Tag"), Some(&None));
}

/// Two enabled desired providers of one required-single capability: plan
/// refused, previous composition kept, never a silent pick (§E.2).
#[test]
fn ambiguous_providers_refused_at_plan_time() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_consumer("c", &l))
        .expect("component registered");
    k.register_component(tag_provider("p1", "p1", &l))
        .expect("component registered");
    k.register_component(tag_provider("p2", "p2", &l))
        .expect("component registered");
    let err = k
        .set_desired(vec![
            DesiredEntry::enabled("p1", "p1", Revision::fresh()),
            DesiredEntry::enabled("p2", "p2", Revision::fresh()),
            DesiredEntry::enabled("c", "c", Revision::fresh()),
        ])
        .expect_err("ambiguous desired graph is illegal");
    assert!(matches!(
        err.0.as_slice(),
        [qianqian_kernel::CompositionError::AmbiguousProvider {
            capability: "Tag",
            ..
        }]
    ));
    // Nothing was mounted; the kernel keeps its previous (empty) composition.
    k.settle();
    let snap = k.snapshot();
    assert!(snap.fibers.is_empty());
    assert!(snap.quiet);
}

// --- Access discipline (§E.3) ----------------------------------------------

/// Undeclared access is rejected at the Context door (paper's
/// UNDECLARED_ACCESS, adopted semantically).
#[test]
fn undeclared_access_is_rejected() {
    let mut k = Kernel::new();
    let l = log();
    // Consumer declares only `Tag` but resolves `Undeclared`.
    let probe = {
        let lg = l.clone();
        ComponentSpec::new("probe")
            .requires::<Tag>()
            .on_activate(move |ctx| {
                let outcome = ctx.resolve::<Undeclared>();
                lg.borrow_mut().push(format!("probe:{:?}", outcome.err()));
                Err(ActivationError::new("stop after probe"))
            })
    };
    k.register_component(probe).expect("component registered");
    k.register_component(undeclared_provider("up"))
        .expect("component registered");
    k.register_component(tag_provider("tp", "tp", &l))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("up", "up", Revision::fresh()),
        DesiredEntry::enabled("tp", "tp", Revision::fresh()),
        DesiredEntry::enabled("probe", "probe", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();

    assert!(
        entries(&l).iter().any(|e| e == "probe:Some(Undeclared)"),
        "the resolution must be rejected as Undeclared even though a provider exists"
    );
    assert_eq!(
        k.snapshot().fibers.get("probe").map(|f| f.failed_outcome),
        Some(true)
    );
}

/// Inactive access is structurally unrepresentable: Context handles exist
/// only inside an episode (activation / teardown). The executable evidence
/// is the API shape plus the degraded Pending behavior — a fiber with an
/// unsatisfiable dependency never activates, so it can never hold a Context.
#[test]
fn inactive_access_is_unrepresentable_outside_episodes() {
    // No fiber outside an episode can construct an ActivationCtx/TeardownCtx:
    // both are crate-private constructors handed out only by the lifecycle
    // engine. This test pins the semantic consequence: an always-unsatisfied
    // consumer stays Pending forever and performs zero resolutions.
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_consumer("c", &l))
        .expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled("c", "c", Revision::fresh())])
        .expect("legal");
    k.settle();
    k.settle(); // repeated settles are no-ops; no resolution attempt happens
    assert_eq!(entries(&l), Vec::<String>::new());
    assert!(k.snapshot().quiet);
}

/// Capability identity is not provider identity: across a staged replacement
/// the SAME capability identity resolves against the new provider generation
/// (§E.1, B30 — equal values from different fibers are different
/// resolutions, so the consumer must reactivate).
#[test]
fn capability_identity_survives_provider_replacement() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_consumer("c", &l))
        .expect("component registered");
    k.register_component(tag_provider("p", "tag", &l))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("p", "p", Revision::new(1)),
        DesiredEntry::enabled("c", "c", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    let activations_gen1 = entries(&l).iter().filter(|e| *e == "p:activated").count();
    assert_eq!(activations_gen1, 1);

    // Fresh desired incarnation: same entry id, same component, new revision.
    k.set_desired(vec![
        DesiredEntry::enabled("p", "p", Revision::new(2)),
        DesiredEntry::enabled("c", "c", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    let snap = k.snapshot();
    assert_eq!(snap.capabilities.get("Tag"), Some(&Some("p".to_owned())));
    // The consumer reactivated against the NEW generation (B30): two
    // consumer activations and two provider generations in total.
    let activations = entries(&l).iter().filter(|e| *e == "p:activated").count();
    assert_eq!(activations, 2, "a fresh generation must mount and activate");
    let bindings = entries(&l)
        .iter()
        .filter(|e| *e == "c:bound-to-tag")
        .count();
    assert_eq!(
        bindings, 2,
        "the consumer must reactivate against the new provider identity"
    );
}
