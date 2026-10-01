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
mod processing;
mod session;

// The episode mechanism internals (PcmEdge / EdgeTerminal) are
// crate-private: they are session-owned runtime resources, not
// product API. The application-facing surface is exactly the public
// seam below; composition roots reach the episode only through
// `playback_session_spec` (+ its processing-configuration variant)
// and `PlaybackSessionHandle`.
pub use handle::{
    EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionHandle, PlaybackSessionObservation,
};
pub use processing::AudioProcessingConfig;
pub use session::{playback_session_spec, playback_session_spec_with_processing};

// Test doubles shared by the integration tests and the crate-internal
// white-box settlement tests (one copy of the mechanism harness).
// Thread-spawning suites are excluded from loom builds: loom model
// checks must explore only the in-model edge tests, never real OS
// threads (FV-CONC-0 partition, specs/playback-concurrency/check.sh).
#[cfg(all(test, not(loom)))]
#[path = "../tests/common/mod.rs"]
mod test_common;

// Crate-internal white-box tests for the F2 settlement contract and the
// #144 production↔formal decision-table oracle (D14.3: verifier needs
// must not leak mechanism mutators back into the product seam, so these
// live inside the crate boundary).
#[cfg(all(test, not(loom)))]
mod decision_table_oracle;
#[cfg(all(test, not(loom)))]
mod settlement_contract_tests;

// Episode Audio Processing (ADR-PBK-002 D14.11; Issue #177 Stage 2):
// the production processing module and its in-crate oracles — the
// production Gain slice (I1) with the D14.11 seam/discontinuity/failure
// obligations pinned through the real session composition, and the I2
// StatefulProbe: deliberate stateful test processors proving the same
// seam carries DSP history through fragmentation, partial writes, seek
// discontinuities, pause and episode replacement.
#[cfg(all(test, not(loom)))]
mod gain_tests;
#[cfg(all(test, not(loom)))]
mod stateful_probe_tests;
// The I3 production EQ oracles: stage-level DSP mathematics (independent
// f64 recipe re-derivation, analytic frequency responses, stability
// grid) and the composition-level stateful lifecycle matrix.
#[cfg(all(test, not(loom)))]
mod eq_tests;

// Shared harness for the processing oracles (episode builders over the
// mechanism doubles + the exact-content oracles), one copy for the
// gain and stateful-probe suites.
#[cfg(all(test, not(loom)))]
mod processing_support;

// Edge mechanism tests: they exercise PcmEdge directly, so they moved
// inside the crate boundary rather than keeping the mechanism `pub`
// just for tests. loom_edge explores the REAL edge synchronization
// under loom's drop-in primitives; edge_lifecycle is an ordinary
// thread-based suite and stays out of loom builds.
#[cfg(all(test, not(loom)))]
mod edge_lifecycle_tests;

#[cfg(all(test, loom))]
mod loom_edge_tests;
