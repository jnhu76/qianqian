//! Qianqian composition root.
//!
//! R0 constructor-only composition is now hosted by the generic Composition
//! Kernel (#70 R0 migration witness): product capability contracts are
//! declared here, providers install them as kernel provisions, and product
//! consumers reach them through kernel-mediated resolution. This is the
//! smallest proof that Qianqian product components can be hosted by the
//! generic kernel — no real FFmpeg/WASAPI/PocketJS integration happens here.
//!
//! Ownership universes (design §J.4): `MusicKernel` stays product/domain
//! state owned by this crate. The kernel controls only reachability,
//! ownership and lifetime of the composition — never payloads.

use std::cell::RefCell;
use std::rc::Rc;

use qianqian_core::music::MusicKernel;
use qianqian_core::ports::AudioOutput;
use qianqian_kernel::{Capability, ComponentSpec, DesiredEntry, Kernel, Revision};

/// The R0 audio-output port hosted as a kernel capability contract.
/// Capability identity is this contract definition site — not any concrete
/// output implementation. Consumers depend on the definition across the
/// plugin seam (architecture: capabilities expose contracts; providers own
/// mechanisms).
pub struct AudioOutputCapability;

impl Capability for AudioOutputCapability {
    const NAME: &'static str = "AudioOutput";
    type Service = dyn AudioOutput;
}

/// The running application assembled through the generic kernel.
pub struct AppRuntime {
    /// Generic composition kernel: reachability, ownership, lifetime.
    composition: Kernel,
    /// Product/domain state: the Music Kernel. Outside the kernel by the
    /// composition/domain firewall (design §J).
    kernel: MusicKernel,
    /// Pre-bound data-plane handle to the resolved audio output service.
    /// A cached projection for ergonomic access — the composition authority
    /// is the kernel's provision effect; this cell holds the service object
    /// for direct payload use after binding (capability plane != data plane).
    audio_output: Rc<RefCell<Option<Rc<dyn AudioOutput>>>>,
}

impl Default for AppRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl AppRuntime {
    /// Empty runtime: the music fiber is desired but sits Pending over its
    /// unsatisfied dependency — the honest degraded state, never a crash
    /// (#53 §K.5).
    pub fn new() -> Self {
        let audio_output: Rc<RefCell<Option<Rc<dyn AudioOutput>>>> = Rc::new(RefCell::new(None));
        let mut composition = Kernel::new();
        composition.register_component(music_component(audio_output.clone()));
        composition
            .set_desired(vec![DesiredEntry::enabled(
                "music",
                "music",
                Revision::new(1),
            )])
            .expect("the R0 desired composition is legal");
        composition.settle();
        Self {
            composition,
            kernel: MusicKernel::new(),
            audio_output,
        }
    }

    /// Static profile composition: present the audio output implementation
    /// as a kernel-hosted provider fiber.
    pub fn with_audio_output(self, audio_output: Box<dyn AudioOutput>) -> Self {
        let service: Rc<dyn AudioOutput> = Rc::from(audio_output);
        let mut composition = self.composition;
        composition.register_component(
            ComponentSpec::new("audio_output")
                .provides::<AudioOutputCapability>()
                .on_activate(move |ctx| {
                    ctx.provide::<AudioOutputCapability>(service.clone())
                        .expect("provides declared");
                    Ok(())
                }),
        );
        composition
            .set_desired(vec![
                DesiredEntry::enabled("audio_output", "audio_output", Revision::new(1)),
                DesiredEntry::enabled("music", "music", Revision::new(1)),
            ])
            .expect("the R0 desired composition is legal");
        composition.settle();
        Self {
            composition,
            ..self
        }
    }

    /// Product/domain semantics: untouched by composition (design §J.3 —
    /// the kernel may never know playback state).
    pub fn kernel(&self) -> &MusicKernel {
        &self.kernel
    }

    /// The resolved audio output service handle, present iff the kernel
    /// mediates an active AudioOutput binding for the music fiber.
    pub fn audio_output(&self) -> Option<Rc<dyn AudioOutput>> {
        self.audio_output.borrow().clone()
    }

    /// Composition truth for diagnostics/tests (closed §I.1 surface).
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

/// The music fiber: requires the audio output capability at activation and
/// owns its binding as an effect whose inverse releases the pre-bound
/// handle on teardown (kernel-mediated reachability, §K).
fn music_component(audio_output: Rc<RefCell<Option<Rc<dyn AudioOutput>>>>) -> ComponentSpec {
    let handle_on_activate = audio_output.clone();
    ComponentSpec::new("music")
        .requires::<AudioOutputCapability>()
        .on_activate(move |ctx| {
            let binding = ctx
                .resolve::<AudioOutputCapability>()
                .map_err(|e| qianqian_kernel::ActivationError::new(format!("{e:?}")))?;
            *handle_on_activate.borrow_mut() = Some(binding.service());
            Ok(())
        })
        .on_teardown(move |_| {
            // The binding's teardown releases the pre-bound handle.
            *audio_output.borrow_mut() = None;
            qianqian_kernel::Discharge::Discharged
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use qianqian_core::music::PlaybackState;
    use qianqian_core::ports::AudioOutput;
    use qianqian_kernel::FiberState;

    struct FakeAudioOutput;

    impl AudioOutput for FakeAudioOutput {}

    #[test]
    fn kernel_hosted_profile_composes_music_with_audio_output() {
        let runtime = AppRuntime::new().with_audio_output(Box::new(FakeAudioOutput));
        assert_eq!(runtime.kernel().state(), PlaybackState::Idle);
        assert!(runtime.audio_output().is_some());
        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("music").map(|f| f.state),
            Some(FiberState::Active),
            "the music fiber is hosted and active"
        );
        assert_eq!(
            snap.capabilities.get("AudioOutput"),
            Some(&Some("audio_output".to_owned()))
        );
        assert!(snap.quiet);
    }

    #[test]
    fn empty_runtime_degrades_to_pending_without_audio_output() {
        let runtime = AppRuntime::new();
        assert_eq!(runtime.kernel().state(), PlaybackState::Idle);
        assert!(runtime.audio_output().is_none());
        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("music").map(|f| f.state),
            Some(FiberState::Pending),
            "unsatisfied dependency => Pending, never a root crash (#53 §K.5)"
        );
        assert!(snap.quiet);
    }

    #[test]
    fn withdrawing_the_output_releases_the_binding_through_the_kernel() {
        let runtime = AppRuntime::new().with_audio_output(Box::new(FakeAudioOutput));
        assert!(runtime.audio_output().is_some());

        let mut runtime = runtime;
        runtime
            .revise_desired(vec![DesiredEntry::enabled(
                "music",
                "music",
                Revision::new(1),
            )])
            .expect("legal");

        assert!(
            runtime.audio_output().is_none(),
            "the music fiber's teardown must release the pre-bound handle"
        );
        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("music").map(|f| f.state),
            Some(FiberState::Pending)
        );
        assert_eq!(snap.capabilities.get("AudioOutput"), Some(&None));
    }
}
