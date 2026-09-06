//! Qianqian composition root.
//!
//! Where "Profile chooses implementation" becomes ordinary Rust
//! composition of `qianqian-core` semantics and capability ports.
//!
//! Composition is ordinary construction: no registry, service
//! locator, dynamic plugin loading, or dependency solver.

use qianqian_core::music::MusicKernel;
use qianqian_core::ports::AudioOutput;

/// The running application assembled by a profile.
#[derive(Default)]
pub struct AppRuntime {
    kernel: MusicKernel,
    audio_output: Option<Box<dyn AudioOutput>>,
}

impl AppRuntime {
    /// Empty runtime: no capability implementation is selected yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Static profile composition: select the audio output implementation.
    pub fn with_audio_output(mut self, audio_output: Box<dyn AudioOutput>) -> Self {
        self.audio_output = Some(audio_output);
        self
    }

    pub fn kernel(&self) -> &MusicKernel {
        &self.kernel
    }

    pub fn audio_output(&self) -> Option<&dyn AudioOutput> {
        self.audio_output.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qianqian_core::music::PlaybackState;
    use qianqian_core::ports::AudioOutput;

    struct FakeAudioOutput;

    impl AudioOutput for FakeAudioOutput {}

    #[test]
    fn static_profile_composes_kernel_with_fake_audio_output() {
        let runtime = AppRuntime::new().with_audio_output(Box::new(FakeAudioOutput));
        assert_eq!(runtime.kernel().state(), PlaybackState::Idle);
        assert!(runtime.audio_output().is_some());
    }

    #[test]
    fn empty_runtime_starts_without_audio_output() {
        let runtime = AppRuntime::new();
        assert_eq!(runtime.kernel().state(), PlaybackState::Idle);
        assert!(runtime.audio_output().is_none());
    }
}
