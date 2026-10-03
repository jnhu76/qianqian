//! Headless shell vocabulary for Architecture v2.
//!
//! The library target owns the product-facing CLI grammar and the
//! truth-class text projections so both stay separable from the
//! playback mechanism code the binary wires up. Since F1 the binary
//! wires `stop`; since F2 it also wires `status`, rendering the
//! playback seam's coherent observation truthfully; since F3 `pause`
//! and `resume`, and since F5 `seek` — the machine transport's
//! `seek <time>` token plus the terminal shell's fixed-step keys, both
//! routed through the frozen D14.5 seat and neither inventing a target.
//! Recognizing the other intents is still not control; they wait for
//! their Phase-F slice.
//!
//! Since the reference-player slice, `play` opens the terminal UI
//! shell ([`tui`]) over the same episode seam, and the scriptable
//! stdin/stdout transport stays available behind `--machine play`.
//! Both adapters render; neither owns playback truth. What a script
//! can observe from the `--machine` transport — report lines, streams,
//! exit codes — is pinned in [`machine`], which the binary renders
//! through so behavior and contract cannot drift.
//!
//! Since F6, the reference player's episode lifetime is owned by
//! [`player`]: the application composition owner of Open/replacement
//! (ADR-PBK-002 D14.6) — one process-level host sequentially owning
//! non-overlapping composition roots, one per episode.
//!
//! Since U1 (Issue #166), [`input`] is the product launch's host input
//! preparation: user-supplied files/folders expanded into ordered
//! playable candidates that feed the SAME Open path. It is an
//! application function, not a Plugin, and it witnesses nothing about
//! playability — the decode preflight stays the witness. [`entry`] is
//! the transports' wiring both binary targets execute: `qianqian`, the
//! canonical product binary, and `qianqian-headless`, the historical
//! regression target.

#[cfg(any(feature = "playback", test))]
mod assembly;
pub mod cli;
pub mod entry;
pub mod input;
pub mod machine;
#[cfg(any(feature = "playback", test))]
mod machine_input;
pub mod player;
pub mod playlist;
pub mod status;
pub mod tui;
