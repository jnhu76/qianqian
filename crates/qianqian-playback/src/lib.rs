//! Playback Session: one concrete playback episode
//! (first-audible-slice design §2, §6).
//!
//! The session is a kernel component requiring the Decode and Output
//! capabilities. Its activation is the whole control plane:
//! resolve each capability once, open one playback-specific decode
//! endpoint, build the bounded PCM edge, open one render stream, spawn
//! the decode worker — registering every inverse in the order that earns
//! the stop -> join -> release unwind. Its steady state is pure data
//! plane: decode worker -> bounded edge -> render thread, with zero K0
//! work per quantum.
//!
//! The application-facing surface is the episode handle
//! ([`PlaybackSessionHandle`], F2 seam): `request_stop`, `observe`,
//! `wait_terminal`. Terminal truth is session-owned: evidence
//! publication and D11 settlement run only on session-owned paths
//! behind this crate boundary — synchronously, with no resolver thread
//! (D14.3); the completion core is an internal replaceable
//! realization, not K0 truth and not the public API.
//!
//! The public terminal vocabulary is the stable semantic triple
//! ([`EpisodeTerminalOutcome`]); failure diagnostics stay internal
//! realization and are surfaced separately as presentation text
//! (D14.2).

mod completion;
mod edge;
mod handle;
mod session;

pub use edge::{EdgeTerminal, PcmEdge, SharedEdge, WriteOutcome};
pub use handle::{EpisodeTerminalOutcome, PlaybackSessionHandle, PlaybackSessionObservation};
pub use session::playback_session_spec;

// Test doubles shared by the integration tests and the crate-internal
// white-box settlement tests (one copy of the mechanism harness).
#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod test_common;

// Crate-internal white-box tests for the F2 settlement contract and the
// #144 production↔formal decision-table oracle (D14.3: verifier needs
// must not leak mechanism mutators back into the product seam, so these
// live inside the crate boundary).
#[cfg(test)]
mod decision_table_oracle;
#[cfg(test)]
mod settlement_contract_tests;
