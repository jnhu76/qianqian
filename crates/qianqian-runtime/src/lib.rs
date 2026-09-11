//! Qianqian composition root.
//!
//! Product components are hosted by the generic Composition Kernel: product
//! capability contracts are declared here, providers install them as kernel
//! provisions, and consumers reach them through kernel-mediated resolution.

use std::rc::Rc;

use qianqian_core::ports::AudioOutput;
use qianqian_kernel::{Capability, ComponentSpec, DesiredEntry, Kernel, Revision};

/// The current audio-output port hosted as a kernel capability contract.
/// Capability identity is this contract definition site — not any concrete
/// output implementation. Consumers depend on the definition across the
/// plugin seam; providers own mechanisms.
pub struct AudioOutputCapability;

impl Capability for AudioOutputCapability {
    const NAME: &'static str = "AudioOutput";
    type Service = dyn AudioOutput;
}

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
    /// Empty runtime: the audio-output consumer fiber is desired but stays
    /// Pending over its unsatisfied output dependency rather than crashing
    /// the root.
    pub fn new() -> Self {
        let mut composition = Kernel::new();
        composition
            .register_component(audio_output_consumer_component())
            .expect("audio-output consumer component registration is legal");
        composition
            .set_desired(vec![DesiredEntry::enabled(
                "audio_output_consumer",
                "audio_output_consumer",
                Revision::new(1),
            )])
            .expect("the desired composition is legal");
        composition.settle();
        Self { composition }
    }

    /// Static profile composition: present an audio-output implementation as
    /// a kernel-hosted provider Fiber.
    pub fn with_audio_output(self, audio_output: Box<dyn AudioOutput>) -> Self {
        let service: Rc<dyn AudioOutput> = Rc::from(audio_output);
        let mut composition = self.composition;
        composition
            .register_component(
                ComponentSpec::new("audio_output")
                    .provides::<AudioOutputCapability>()
                    .on_activate(move |ctx| {
                        ctx.provide::<AudioOutputCapability>(service.clone())
                            .expect("provides declared");
                        Ok(())
                    }),
            )
            .expect("audio-output component registration is legal");
        composition
            .set_desired(vec![
                DesiredEntry::enabled("audio_output", "audio_output", Revision::new(1)),
                DesiredEntry::enabled(
                    "audio_output_consumer",
                    "audio_output_consumer",
                    Revision::new(1),
                ),
            ])
            .expect("the desired composition is legal");
        composition.settle();
        Self { composition }
    }

    /// Kernel-derived composition snapshot for diagnostics/tests. The
    /// kernel's internal committed state is the authority; this snapshot is
    /// only its read-side projection.
    pub fn composition_snapshot(&self) -> qianqian_kernel::CompositionSnapshot {
        self.composition.snapshot()
    }

    /// Drive the control plane after a desired-composition change.
    pub fn revise_desired(
        &mut self,
        entries: Vec<DesiredEntry>,
    ) -> Result<(), qianqian_kernel::CompositionErrors> {
        self.composition.set_desired(entries)?;
        self.composition.settle();
        Ok(())
    }
}

/// The audio-output consumer witness: a component whose entire earned
/// responsibility is requiring [`AudioOutputCapability`]. The kernel keeps
/// it Pending while no single legal provider exists, activates it when one
/// does, and records the capability binding itself — the root holds no
/// service-handle state beside that. It carries no music/playback
/// semantics; the name records only which capability it consumes.
fn audio_output_consumer_component() -> ComponentSpec {
    ComponentSpec::new("audio_output_consumer").requires::<AudioOutputCapability>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use qianqian_core::ports::AudioOutput;
    use qianqian_kernel::FiberState;

    struct FakeAudioOutput;

    impl AudioOutput for FakeAudioOutput {}

    #[test]
    fn without_provider_the_consumer_stays_pending() {
        let runtime = AppRuntime::new();
        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("audio_output_consumer").map(|f| f.state),
            Some(FiberState::Pending),
            "unsatisfied dependency => Pending, never a root crash"
        );
        assert_eq!(snap.capabilities.get("AudioOutput"), Some(&None));
        assert!(snap.quiet);
    }

    #[test]
    fn with_provider_the_consumer_activates_and_the_kernel_binds() {
        let runtime = AppRuntime::new().with_audio_output(Box::new(FakeAudioOutput));
        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("audio_output_consumer").map(|f| f.state),
            Some(FiberState::Active),
            "the audio-output consumer fiber is hosted and active"
        );
        assert_eq!(
            snap.capabilities.get("AudioOutput"),
            Some(&Some("audio_output".to_owned()))
        );
        assert!(snap.quiet);
    }

    #[test]
    fn withdrawing_the_provider_degrades_the_consumer_to_pending() {
        let runtime = AppRuntime::new().with_audio_output(Box::new(FakeAudioOutput));
        let mut runtime = runtime;
        runtime
            .revise_desired(vec![DesiredEntry::enabled(
                "audio_output_consumer",
                "audio_output_consumer",
                Revision::new(1),
            )])
            .expect("legal");

        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("audio_output_consumer").map(|f| f.state),
            Some(FiberState::Pending),
            "the consumer remains desired and degrades over its vanished dependency"
        );
        assert_eq!(snap.capabilities.get("AudioOutput"), Some(&None));
    }

    /// Composition regression at the root: K0 legally instantiates several
    /// desired entries of one component, and withdrawing one instance must
    /// leave the surviving sibling's kernel-mediated binding untouched.
    #[test]
    fn sibling_consumer_instances_withdraw_independently_in_kernel_truth() {
        let runtime = AppRuntime::new().with_audio_output(Box::new(FakeAudioOutput));
        let mut runtime = runtime;
        runtime
            .revise_desired(vec![
                DesiredEntry::enabled("audio_output", "audio_output", Revision::new(1)),
                DesiredEntry::enabled(
                    "audio_output_consumer",
                    "audio_output_consumer",
                    Revision::new(1),
                ),
                DesiredEntry::enabled("dup", "audio_output_consumer", Revision::new(1)),
            ])
            .expect("multi-instance desired composition is kernel-legal");
        {
            let snap = runtime.composition_snapshot();
            assert_eq!(
                snap.fibers.get("audio_output_consumer").map(|f| f.state),
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
                DesiredEntry::enabled("audio_output", "audio_output", Revision::new(1)),
                DesiredEntry::enabled(
                    "audio_output_consumer",
                    "audio_output_consumer",
                    Revision::new(1),
                ),
            ])
            .expect("legal");

        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("audio_output_consumer").map(|f| f.state),
            Some(FiberState::Active),
            "the surviving sibling instance must stay Active"
        );
        assert!(
            !snap.fibers.contains_key("dup"),
            "the withdrawn instance is gone"
        );
        assert_eq!(
            snap.capabilities.get("AudioOutput"),
            Some(&Some("audio_output".to_owned()))
        );
        assert!(snap.quiet);
    }
}
