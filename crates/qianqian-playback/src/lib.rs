//! Playback Session: one concrete playback episode
//! (first-audible-slice design §2, §6).
//!
//! The session is a kernel component requiring the Decode and Output
//! capabilities. Its activation is the whole control plane:
//! resolve each capability once, open one playback-specific decode
//! endpoint, build the bounded PCM edge, open one render stream, spawn the
//! decode worker — registering every inverse in the order that earns the
//! stop -> join -> release unwind. Its steady state is pure data plane:
//! decode worker -> bounded edge -> render thread, with zero K0 work per
//! quantum.
//!
//! The completion signal is session truth, not K0 truth: the App waits on
//! it and then drives disposal explicitly.

mod completion;
mod edge;
mod session;

pub use completion::{SessionCompletion, SessionOutcome};
pub use edge::{EdgeTerminal, PcmEdge, SharedEdge, WriteOutcome};
pub use session::playback_session_spec;

// Re-export core port types for convenient use across the workspace.
pub use qianqian_audio_api::ports::{
    AudioOutput, DecodeError, DecodeOpenError, DecodeOutcome, DecodedPcmStream, DrainSignal,
    DrainVerdict, PcmDecode, PcmFormat, RenderPcmInput, RenderRequest, RenderStream,
};
