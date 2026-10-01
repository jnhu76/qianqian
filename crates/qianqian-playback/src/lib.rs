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

// The episode mechanism internals (PcmEdge / EdgeTerminal) are
// crate-private: they are session-owned runtime resources, not
// product API. The application-facing surface is exactly the public
// seam below; composition roots reach the episode only through
// `playback_session_spec` + `PlaybackSessionHandle`.
pub use handle::{
    EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionHandle, PlaybackSessionObservation,
};
pub use session::playback_session_spec;

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

// I0 Gain disposable probe (Issue #177 Stage 2 / I0; ADR-PBK-002
// D14.11): experiment-only evidence seam for the decode-worker staging
// processing placement, compiled only in this crate's own test build.
// Never shipped; the module and its one worker call site are deleted
// with the I0 evidence.
#[cfg(all(test, not(loom)))]
mod gain_probe;
#[cfg(all(test, not(loom)))]
mod gain_probe_tests;

// Edge mechanism tests: they exercise PcmEdge directly, so they moved
// inside the crate boundary rather than keeping the mechanism `pub`
// just for tests. loom_edge explores the REAL edge synchronization
// under loom's drop-in primitives; edge_lifecycle is an ordinary
// thread-based suite and stays out of loom builds.
#[cfg(all(test, not(loom)))]
mod edge_lifecycle_tests;

#[cfg(all(test, loom))]
mod loom_edge_tests;
