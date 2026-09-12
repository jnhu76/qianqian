//! Qianqian composition root.
//!
//! The root is a thin admission layer over the generic Composition Kernel.
//! It defines no product component and constructs no resource: plugin
//! authors (or the Host) own [`ComponentSpec`] construction and own the
//! desired composition ([`DesiredEntry`] entries through
//! [`AppRuntime::revise_desired`]); the root only admits both into the
//! kernel and never inspects plugin semantics.
//!
//! Shutdown is explicit: [`AppRuntime::dispose`] retires the whole
//! composition and returns the post-disposal composition snapshot. Dropping an
//! [`AppRuntime`] without calling `dispose` does **not** run teardown
//! inverses — the kernel carries no `Drop` — so a latched violation can
//! only ever be observed through the explicit seam.

use qianqian_kernel::{
    ComponentRegistrationError, ComponentSpec, CompositionErrors, CompositionSnapshot,
    DesiredEntry, Kernel,
};

/// The running application assembled through the generic Composition Kernel.
pub struct AppRuntime {
    /// Generic composition truth: reachability, binding ownership and Fiber
    /// lifetime, held entirely by the kernel. The root keeps no parallel
    /// service-handle state.
    composition: Kernel,
}

impl Default for AppRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl AppRuntime {
    /// An empty runtime over an empty kernel composition. Nothing is
    /// desired, nothing is mounted, and the root is quiet; real components
    /// enter through [`AppRuntime::register_component`] plus
    /// [`AppRuntime::revise_desired`].
    pub fn new() -> Self {
        Self {
            composition: Kernel::new(),
        }
    }

    /// Admit a plugin-authored component definition into the composition.
    ///
    /// Thin passthrough over [`Kernel::register_component`]: the plugin owns
    /// the spec — its declarations, its activation, its teardown — and the
    /// root adds no semantics and constructs nothing on its behalf. A
    /// registered definition is fixed for the kernel's lifetime (no
    /// component hot replacement); mount instances of it by desiring them
    /// through [`AppRuntime::revise_desired`].
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

    /// Explicit root disposal: retire every fiber, drain, and return the
    /// post-disposal composition snapshot.
    ///
    /// The snapshot is an observation, not a success certificate: a latched
    /// teardown-contract violation stays visible in it
    /// ([`qianqian_kernel::FiberDiagnostic::teardown_violated`],
    /// `quiet == false`) and is never reported past as a clean completion.
    pub fn dispose(&mut self) -> CompositionSnapshot {
        self.composition.dispose_root();
        self.composition.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use qianqian_kernel::{ActivationError, Capability, Discharge, FiberState, Revision};

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
        let runtime = AppRuntime::new();
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
        let mut runtime = AppRuntime::new();
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
        let mut runtime = AppRuntime::new();
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
        let mut runtime = AppRuntime::new();
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

        let snap = runtime.dispose();
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
        let mut runtime = AppRuntime::new();
        runtime
            .register_component(ComponentSpec::new("violating").on_activate(|ctx| {
                ctx.register_effect(|| Discharge::Violated);
                Ok(())
            }))
            .expect("legal");
        runtime
            .revise_desired(vec![desired_probe("violating")])
            .expect("legal");

        let snap = runtime.dispose();
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
        let mut runtime = AppRuntime::new();
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
        let mut runtime = AppRuntime::new();
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
