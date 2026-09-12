//! Real WASAPI Output capability provider.
//!
//! Mechanism provider only: it owns the WASAPI mechanism code (COM call
//! sequences, format negotiation, the render loop) and publishes the
//! `AudioOutput` capability through a kernel `ComponentSpec`. Per-episode
//! state — one acquired render stream, including its render thread —
//! belongs to the caller (the Playback Session)
//! (first-audible-slice design §2, §5).
//!
//! Platform behavior (design §8): the WASAPI mechanism exists under
//! `cfg(windows)`. On any other platform the plugin compiles, and its real
//! activation reports the platform unsupported — loudly, never a fake
//! success or a silent null output.

use std::rc::Rc;

use qianqian_core::ports::AudioOutputCapability;
use qianqian_kernel::{ActivationError, ComponentSpec};

#[cfg(windows)]
mod wasapi;

/// Build the platform's real output mechanism. On non-Windows this is the
/// honest unsupported-platform report, surfaced as an activation failure.
#[cfg(windows)]
fn platform_provider() -> Result<Rc<dyn qianqian_core::ports::AudioOutput>, String> {
    Ok(Rc::new(wasapi::WasapiOutput::new()?))
}

#[cfg(not(windows))]
fn platform_provider() -> Result<Rc<dyn qianqian_core::ports::AudioOutput>, String> {
    Err("WASAPI output requires Windows; this platform has no real output \
         mechanism and the plugin refuses to fake one"
        .to_owned())
}

/// The Output Plugin component definition: provides the `AudioOutput`
/// capability backed by the real WASAPI mechanism (Windows only).
pub fn wasapi_output_plugin() -> ComponentSpec {
    ComponentSpec::new("wasapi_output_plugin")
        .provides::<AudioOutputCapability>()
        .on_activate(|ctx| match platform_provider() {
            Ok(service) => ctx
                .provide::<AudioOutputCapability>(service)
                .map_err(|e| ActivationError::new(format!("provision refused: {e:?}"))),
            Err(message) => Err(ActivationError::new(message)),
        })
}
