//! Qianqian composition root.
//!
//! The root is a thin admission layer over the generic Composition Kernel.
//! It defines no product component and constructs no resource: plugin
//! authors (or the App) own [`ComponentSpec`] construction and own the
//! desired composition ([`DesiredEntry`] entries through
//! [`QianqianApp::revise_desired`]); the root only admits both into the
//! kernel and never inspects plugin semantics.
//!
//! Shutdown is explicit: [`QianqianApp::dispose`] retires the whole
//! composition and returns the AUTHORITATIVE disposal outcome (the
//! kernel's own operation verdict) plus the post-disposal composition
//! snapshot. Dropping an [`QianqianApp`] without calling `dispose` does
//! **not** run teardown inverses — the kernel carries no `Drop` — so a
//! latched violation can only ever be observed through the explicit
//! seam.

use qianqian_composition::{
    ComponentRegistrationError, ComponentSpec, CompositionErrors, CompositionKernel,
    CompositionSnapshot, DesiredEntry, DisposeVerdict,
};

/// The running application assembled through the generic Composition Kernel.
pub struct QianqianApp {
    /// Generic composition truth: reachability, binding ownership and Fiber
    /// lifetime, held entirely by the kernel. The root keeps no parallel
    /// service-handle state.
    composition: CompositionKernel,
}

/// The authoritative root-disposal result: the kernel operation's own
/// verdict (control-legal) plus the post-disposal composition snapshot
/// (read-side projection; never a success certificate).
#[derive(Debug)]
pub struct DisposeOutcome {
    pub verdict: DisposeVerdict,
    pub snapshot: CompositionSnapshot,
}

impl Default for QianqianApp {
    fn default() -> Self {
        Self::new()
    }
}

impl QianqianApp {
    /// An empty runtime over an empty kernel composition. Nothing is
    /// desired, nothing is mounted, and the root is quiet; real components
    /// enter through [`QianqianApp::register_component`] plus
    /// [`QianqianApp::revise_desired`].
    pub fn new() -> Self {
        Self {
            composition: CompositionKernel::new(),
        }
    }

    /// Admit a plugin-authored component definition into the composition.
    ///
    /// Thin passthrough over [`CompositionKernel::register_component`]: the plugin owns
    /// the spec — its declarations, its activation, its teardown — and the
    /// root adds no semantics and constructs nothing on its behalf. A
    /// registered definition is fixed for the kernel's lifetime (no
    /// component hot replacement); mount instances of it by desiring them
    /// through [`QianqianApp::revise_desired`].
    pub fn register_component(
        &mut self,
        spec: ComponentSpec,
    ) -> Result<(), ComponentRegistrationError> {
        self.composition.register_component(spec)
    }

    /// Kernel-derived composition snapshot for diagnostics/tests. The
    /// kernel's internal committed state is the authority; this snapshot is
    /// only its read-side projection.
    pub fn composition_snapshot(&self) -> CompositionSnapshot {
        self.composition.snapshot()
    }

    /// Install a desired composition and drive the control plane to
    /// quiescence (or to a latched, loudly visible blocked state).
    pub fn revise_desired(&mut self, entries: Vec<DesiredEntry>) -> Result<(), CompositionErrors> {
        self.composition.set_desired(entries)?;
        self.composition.settle();
        Ok(())
    }

    /// The authoritative root-disposal result (ADR-PBK-002 D14.6, the
    /// F6-AUTHORITY-PROMOTION-1 amendment). The verdict is the control
    /// plane's own operation outcome — the ONLY control-legal reading of
    /// a disposal; the snapshot rides along as the same read-side
    /// diagnostic projection as ever: an observation, never a success
    /// certificate. A latched teardown-contract violation stays visible
    /// in it ([`qianqian_composition::FiberDiagnostic::teardown_violated`],
    /// `quiet == false`) and is never reported past as a clean completion.
    pub fn dispose(&mut self) -> DisposeOutcome {
        let verdict = self.composition.dispose_root();
        let snapshot = self.composition.snapshot();
        DisposeOutcome { verdict, snapshot }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use qianqian_composition::{ActivationError, Capability, Discharge, FiberState, Revision};

    use super::*;

    /// Test-only capability: identity is this definition site; the service
    /// is a shared probe so tests can observe kernel-mediated binding
    /// without any product semantics.
    struct ProbeCapability;

    impl Capability for ProbeCapability {
        const NAME: &'static str = "Probe";
        type Service = Probe;
    }

    struct Probe;

    fn desired_probe(id: &'static str) -> DesiredEntry {
        DesiredEntry::enabled(id, id, Revision::new(1))
    }

    /// T1 — the production root starts empty: no hardcoded product
    /// component, nothing desired, quiet.
    #[test]
    fn new_root_is_empty_and_quiet() {
        let runtime = QianqianApp::new();
        let snap = runtime.composition_snapshot();
        assert!(
            snap.fibers.is_empty(),
            "no hardcoded product component: the root owns no unearned composition"
        );
        assert!(snap.capabilities.is_empty());
        assert!(snap.quiet);
    }

    /// T2 — a plugin authors its own ComponentSpec (declarations +
    /// activation) and the root merely admits it; the kernel binds
    /// provider → consumer without any root-side handle state.
    #[test]
    fn plugin_authored_components_enter_composition() {
        let mut runtime = QianqianApp::new();
        runtime
            .register_component(
                ComponentSpec::new("probe_provider")
                    .provides::<ProbeCapability>()
                    .on_activate(|ctx| {
                        ctx.provide::<ProbeCapability>(Rc::new(Probe))
                            .expect("provides declared");
                        Ok(())
                    }),
            )
            .expect("first registration is legal");
        runtime
            .register_component(ComponentSpec::new("probe_consumer").requires::<ProbeCapability>())
            .expect("distinct name is legal");
        runtime
            .revise_desired(vec![
                desired_probe("probe_provider"),
                desired_probe("probe_consumer"),
            ])
            .expect("the desired composition is legal");

        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("probe_provider").map(|f| f.state),
            Some(FiberState::Active),
            "the plugin-authored provider is hosted and active"
        );
        assert_eq!(
            snap.fibers.get("probe_consumer").map(|f| f.state),
            Some(FiberState::Active),
            "the consumer activated over its satisfied dependency"
        );
        assert_eq!(
            snap.capabilities.get("Probe"),
            Some(&Some("probe_provider".to_owned())),
            "the capability binding is kernel truth, not root state"
        );
        assert!(snap.quiet);
    }

    /// T3 — a raising activation lands the fiber in FAILED with no ghost
    /// provisions, and FAILED is quiet-legal.
    #[test]
    fn activation_failure_lands_failed_without_ghost_provisions() {
        let mut runtime = QianqianApp::new();
        runtime
            .register_component(
                ComponentSpec::new("raising_provider")
                    .provides::<ProbeCapability>()
                    .on_activate(|_ctx| Err(ActivationError::new("device open failed"))),
            )
            .expect("legal");
        runtime
            .revise_desired(vec![desired_probe("raising_provider")])
            .expect("the desired composition is legal");

        let snap = runtime.composition_snapshot();
        let fiber = snap.fibers.get("raising_provider").expect("installed");
        assert_eq!(fiber.state, FiberState::Failed);
        assert!(fiber.failed_outcome, "the raise is recorded as FAILED");
        assert!(
            snap.provisions
                .get("Probe")
                .is_none_or(|providers| providers.is_empty()),
            "a raised activation publishes no provision"
        );
        assert_eq!(
            snap.capabilities.get("Probe"),
            Some(&None),
            "the declared capability stays unresolvable"
        );
        assert!(snap.quiet, "settled FAILED is quiet-legal");
    }

    /// T4 — explicit disposal runs every registered inverse exactly once.
    /// The counter is test instrumentation; the effect under test is the
    /// kernel's real teardown path.
    #[test]
    fn explicit_disposal_runs_registered_inverses() {
        let inverses_run = Rc::new(Cell::new(0u64));
        let counter_in_activation = inverses_run.clone();
        let mut runtime = QianqianApp::new();
        runtime
            .register_component(ComponentSpec::new("effected").on_activate(move |ctx| {
                let counter_in_inverse = counter_in_activation.clone();
                ctx.register_effect(move || {
                    counter_in_inverse.set(counter_in_inverse.get() + 1);
                    Discharge::Discharged
                });
                Ok(())
            }))
            .expect("legal");
        runtime
            .revise_desired(vec![desired_probe("effected")])
            .expect("legal");
        assert_eq!(inverses_run.get(), 0, "activation alone runs no inverse");

        let outcome = runtime.dispose();
        assert_eq!(
            outcome.verdict,
            qianqian_composition::DisposeVerdict::Discharged,
            "a clean disposal proves discharged"
        );
        let snap = outcome.snapshot;
        assert_eq!(
            inverses_run.get(),
            1,
            "the registered inverse ran exactly once at explicit disposal"
        );
        assert!(snap.fibers.is_empty(), "the composition drained");
        assert!(snap.quiet);
    }

    /// T5 — a latched teardown violation is observable through the explicit
    /// disposal seam; disposal never presents it as a clean completion.
    #[test]
    fn disposal_does_not_claim_success_past_a_latched_violation() {
        let mut runtime = QianqianApp::new();
        runtime
            .register_component(ComponentSpec::new("violating").on_activate(|ctx| {
                ctx.register_effect(|| Discharge::Violated);
                Ok(())
            }))
            .expect("legal");
        runtime
            .revise_desired(vec![desired_probe("violating")])
            .expect("legal");

        let outcome = runtime.dispose();
        assert_eq!(
            outcome.verdict,
            qianqian_composition::DisposeVerdict::TeardownViolated,
            "the authoritative verdict — not the snapshot — reports the violation"
        );
        let snap = outcome.snapshot;
        let fiber = snap
            .fibers
            .get("violating")
            .expect("the latched fiber stays visible in the post-disposal snapshot");
        assert!(fiber.teardown_violated, "the §G.6 latch is observable");
        assert!(
            !snap.quiet,
            "a latched violation is never reported as quiet/clean"
        );
    }

    /// Withdrawing the provider degrades the surviving consumer to Pending
    /// over its vanished dependency — plugin-authored edition of the
    /// kernel-mediated withdrawal regression.
    #[test]
    fn withdrawing_the_provider_degrades_the_consumer_to_pending() {
        let mut runtime = QianqianApp::new();
        runtime
            .register_component(probe_provider("probe_provider"))
            .expect("legal");
        runtime
            .register_component(ComponentSpec::new("probe_consumer").requires::<ProbeCapability>())
            .expect("legal");
        runtime
            .revise_desired(vec![
                desired_probe("probe_provider"),
                desired_probe("probe_consumer"),
            ])
            .expect("legal");

        // Withdraw only the provider; the consumer stays desired.
        runtime
            .revise_desired(vec![desired_probe("probe_consumer")])
            .expect("legal");

        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("probe_consumer").map(|f| f.state),
            Some(FiberState::Pending),
            "the consumer remains desired and degrades over its vanished dependency"
        );
        assert_eq!(snap.capabilities.get("Probe"), Some(&None));
    }

    /// Composition regression at the root: K0 legally instantiates several
    /// desired entries of one plugin-authored component, and withdrawing
    /// one instance must leave the surviving sibling's kernel-mediated
    /// binding untouched.
    #[test]
    fn sibling_consumer_instances_withdraw_independently_in_kernel_truth() {
        let mut runtime = QianqianApp::new();
        runtime
            .register_component(probe_provider("probe_provider"))
            .expect("legal");
        runtime
            .register_component(ComponentSpec::new("probe_consumer").requires::<ProbeCapability>())
            .expect("legal");

        runtime
            .revise_desired(vec![
                desired_probe("probe_provider"),
                desired_probe("probe_consumer"),
                DesiredEntry::enabled("dup", "probe_consumer", Revision::new(1)),
            ])
            .expect("multi-instance desired composition is kernel-legal");
        {
            let snap = runtime.composition_snapshot();
            assert_eq!(
                snap.fibers.get("probe_consumer").map(|f| f.state),
                Some(FiberState::Active)
            );
            assert_eq!(
                snap.fibers.get("dup").map(|f| f.state),
                Some(FiberState::Active)
            );
        }

        // Withdraw only the duplicate instance; the survivor must keep its
        // kernel-mediated binding.
        runtime
            .revise_desired(vec![
                desired_probe("probe_provider"),
                desired_probe("probe_consumer"),
            ])
            .expect("legal");

        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("probe_consumer").map(|f| f.state),
            Some(FiberState::Active),
            "the surviving sibling instance must stay Active"
        );
        assert!(
            !snap.fibers.contains_key("dup"),
            "the withdrawn instance is gone"
        );
        assert_eq!(
            snap.capabilities.get("Probe"),
            Some(&Some("probe_provider".to_owned()))
        );
        assert!(snap.quiet);
    }

    fn probe_provider(name: &'static str) -> ComponentSpec {
        ComponentSpec::new(name)
            .provides::<ProbeCapability>()
            .on_activate(|ctx| {
                ctx.provide::<ProbeCapability>(Rc::new(Probe))
                    .expect("provides declared");
                Ok(())
            })
    }
}
