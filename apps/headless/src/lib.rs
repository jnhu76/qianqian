//! Headless shell vocabulary for Architecture v2.
//!
//! The library target owns the product-facing CLI grammar and the
//! truth-class text projections so both stay separable from the
//! playback mechanism code the binary wires up. Since F1 the binary
//! wires `stop`; since F2 it also wires `status`, rendering the
//! playback seam's coherent observation truthfully. Recognizing the
//! other intents is still not control; they wait for their Phase-F
//! slice.
//!
//! Since the reference-player slice, `play` opens the terminal UI
//! shell ([`tui`]) over the same episode seam, and the scriptable
//! stdin/stdout transport stays available behind `--machine play`.
//! Both adapters render; neither owns playback truth. What a script
//! can observe from the `--machine` transport — report lines, streams,
//! exit codes — is pinned in [`machine`], which the binary renders
//! through so behavior and contract cannot drift.

pub mod cli;
pub mod machine;
pub mod status;
pub mod tui;
