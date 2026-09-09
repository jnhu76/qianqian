//! Qianqian composition root.
//!
//! Product components are hosted by the generic Composition Kernel: product
//! capability contracts are declared here, providers install them as kernel
//! provisions, and consumers reach them through kernel-mediated resolution.
//!
//! The composition kernel controls reachability/lifetime only. The playback
//! types hosted here (`MusicKernel`, `TransportKernel`) are **experimental
//! Playback evidence** from an earlier architecture experiment; their presence
//! does not establish current architecture authority or compatibility
//! requirements. Neither is the generic Composition Kernel, and neither makes
//! PCM a Context payload.

use std::cell::RefCell;
use std::rc::Rc;

use qianqian_core::music::MusicKernel;
use qianqian_core::ports::AudioOutput;
use qianqian_core::transport::TransportKernel;
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
    /// lifetime. It never owns playback cursor/window/product state.
    composition: Kernel,
    /// Music-domain/product semantic authority.
    music_kernel: MusicKernel,
    /// Playback-temporal semantic authority.
    transport_kernel: TransportKernel,
    /// Pre-bound data-plane handle to the resolved audio output service.
    /// The composition authority is still the kernel's binding; this cached
    /// service is for direct payload/mechanism use after binding.
    audio_output: Rc<RefCell<Option<Rc<dyn AudioOutput>>>>,
}

impl Default for AppRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl AppRuntime {
    /// Empty runtime: the music Fiber is desired but stays Pending over its
    /// unsatisfied output dependency rather than crashing the root.
    pub fn new() -> Self {
        let audio_output: Rc<RefCell<Option<Rc<dyn AudioOutput>>>> = Rc::new(RefCell::new(None));
        let mut composition = Kernel::new();
        composition
            .register_component(music_component(audio_output.clone()))
            .expect("music component registration is legal");
        composition
            .set_desired(vec![DesiredEntry::enabled(
                "music",
                "music",
                Revision::new(1),
            )])
            .expect("the desired composition is legal");
        composition.settle();
        Self {
            composition,
            music_kernel: MusicKernel::new(),
            transport_kernel: TransportKernel::new(),
            audio_output,
        }
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
                DesiredEntry::enabled("music", "music", Revision::new(1)),
            ])
            .expect("the desired composition is legal");
        composition.settle();
        Self {
            composition,
            ..self
        }
    }

    /// Music/product semantic authority.
    pub fn music_kernel(&self) -> &MusicKernel {
        &self.music_kernel
    }

    /// Playback-temporal semantic authority.
    ///
    /// This shell does not yet expose Window/Generation/Fence APIs; it only
    /// makes the frozen authority split explicit in production Rust topology.
    pub fn transport_kernel(&self) -> &TransportKernel {
        &self.transport_kernel
    }

    /// The resolved audio output service handle, present iff the Composition
    /// Kernel mediates an active AudioOutput binding for the music Fiber.
    pub fn audio_output(&self) -> Option<Rc<dyn AudioOutput>> {
        self.audio_output.borrow().clone()
    }

    /// Composition truth for diagnostics/tests.
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

/// The Music Fiber requires the audio-output capability at activation. On
/// activation the fiber resolves the service and stores it outside kernel
/// storage as a pre-bound data edge; on teardown the `on_teardown` closure —
/// a domain teardown obligation whose verdict (`Discharge::Discharged` /
/// `Discharge::Violated`) is all the kernel observes, not a kernel Effect
/// inverse — releases that handle. Domain/temporal state remains outside
/// generic kernel storage.
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
        assert_eq!(runtime.music_kernel().state(), PlaybackState::Idle);
        let _transport = runtime.transport_kernel();
        assert!(runtime.audio_output().is_some());
        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("music").map(|f| f.state),
            Some(FiberState::Active),
            "the music Fiber is hosted and active"
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
        assert_eq!(runtime.music_kernel().state(), PlaybackState::Idle);
        let _transport = runtime.transport_kernel();
        assert!(runtime.audio_output().is_none());
        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("music").map(|f| f.state),
            Some(FiberState::Pending),
            "unsatisfied dependency => Pending, never a root crash"
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
            "the music Fiber's teardown must release the pre-bound handle"
        );
        let snap = runtime.composition_snapshot();
        assert_eq!(
            snap.fibers.get("music").map(|f| f.state),
            Some(FiberState::Pending)
        );
        assert_eq!(snap.capabilities.get("AudioOutput"), Some(&None));
    }
}
