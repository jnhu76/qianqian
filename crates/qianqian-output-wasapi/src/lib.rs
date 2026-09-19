//! The stable Output Plugin (ADR-PBK-003).
//!
//! Three identities, kept distinct (PBK-003 §2):
//!
//! ```text
//! Output Plugin            this ComponentSpec: the stable K0
//!                          composition identity ("output_plugin");
//!                          it provides the AudioOutputCapability
//!                          Host Render Backend          the owned
//!                          mechanism it constructs on activation —
//!                          the concrete platform implementation,
//!                          never itself a Plugin (PBK-003 §4/D13)
//!                          AudioOutput contract         the
//!                          backend-neutral seam between them
//!                          (qianqian-audio-api ports)
//! ```
//!
//! A desired composition identifies the stable output role; no playback
//! semantic may depend on which backend realizes it (PBK-003 §3/§10).
//! Backend selection is host-assembly configuration (PBK-003 §9): this
//! assembly's Windows selection is the owned WASAPI backend under
//! `cfg(windows)`. Per-episode state — one acquired render stream,
//! including its render thread — belongs to the caller (the Playback
//! Session).
//!
//! On a platform with no selected backend the plugin compiles, and its
//! real activation reports the platform unsupported — loudly, never a
//! fake success or a silent null output.

use std::rc::Rc;

use qianqian_audio_api::ports::AudioOutputCapability;
use qianqian_composition::{ActivationError, ComponentSpec};

#[cfg(windows)]
mod wasapi;

// Platform-independent open-abort protocol (D14.7): aborting a gated
// render thread releases the routed pause intent before the join. See
// the module doc; tested on every platform.
mod open_abort;

// Source-order oracle for the Windows-only render loop's F4 frame
// accounting (D14.8): it reads `wasapi.rs` as text, so it runs on every
// platform even though the mechanism it pins does not.
#[cfg(test)]
mod render_order_oracle;

// White-box mechanism test (event-handle lifetime oracle,
// NATIVE-BOUNDARY-AUDIT-0 A3.3 corrective). It moved inside the crate
// boundary in the plugin-boundary hardening (H1): the concrete
// mechanism is no longer exported for tests. Windows-only by nature;
// its leak loop needs a real render endpoint, so it belongs to the
// Windows reality gate, not the platform-independent suites.
#[cfg(all(test, windows))]
mod event_handle_lifetime_tests;

/// The Host Render Backend this host assembly selects (PBK-003 §9):
/// the owned Windows backend mechanism. Selection is configuration of
/// the assembly, not composition truth; a future platform backend
/// replaces this factory body without touching the plugin identity.
#[cfg(windows)]
fn selected_backend() -> Result<Rc<dyn qianqian_audio_api::ports::AudioOutput>, String> {
    Ok(Rc::new(wasapi::WasapiOutput::new()?))
}

#[cfg(not(windows))]
fn selected_backend() -> Result<Rc<dyn qianqian_audio_api::ports::AudioOutput>, String> {
    Err(
        "no Host Render Backend is selected for this platform; the Output \
         Plugin refuses to fake one"
            .to_owned(),
    )
}

/// The stable Output Plugin component definition: provides the
/// `AudioOutput` capability backed by the Host Render Backend this
/// assembly selects (PBK-003 §2/§9). The identity composition observers
/// see is this stable role — never a backend brand.
pub fn output_plugin() -> ComponentSpec {
    ComponentSpec::new("output_plugin")
        .provides::<AudioOutputCapability>()
        .on_activate(|ctx| match selected_backend() {
            Ok(service) => ctx
                .provide::<AudioOutputCapability>(service)
                .map_err(|e| ActivationError::new(format!("provision refused: {e:?}"))),
            Err(message) => Err(ActivationError::new(message)),
        })
}
