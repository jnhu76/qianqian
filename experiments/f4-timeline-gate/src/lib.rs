//! F4-GATE mechanism evidence: position/duration propositions and the
//! projection algebra a future product Position would be derived from.
//!
//! This crate is evidence, not production architecture. It proposes (and
//! executable oracles here pin) the accounting shape the F4 gate
//! documents: two session-owned mechanism-evidence cells plus one
//! derived, clamped, source-frame projection.
//!
//! ```text
//! submitted   source PCM frames handed to the render leg (== frames
//!             submitted into the device buffer on all non-terminal
//!             paths). Monotone within one seek epoch. Writer: the
//!             edge read path.
//! tail        the output mechanism's latest observation of its
//!             queued-to-play tail (GetCurrentPadding), in source
//!             frames. NOT monotone. Writer: the render loop / drain
//!             loop, once per observation.
//! published   stream-start evidence: nothing is derivable before the
//!             mechanism published its first tail observation.
//!
//! raw position    = submitted - min(tail, submitted)
//! position        = max(last_projected, raw)      (monotone clamp)
//! ```
//!
//! Modules:
//!
//! ```text
//! src/timeline.rs        the algebra + deterministic interleaving
//!                        oracles (all platforms)
//! src/bin/f4probe.rs     physical WASAPI probe (Windows only):
//!                        Experiment A (submitted vs padding vs
//!                        consumed across pause/EOF) and Experiment
//!                        B (IAudioClock comparison)
//! src/bin/f4duration.rs  duration provenance over the SongCore ABI
//!                        (non-Windows): Experiment C
//! RESULTS.md             the evidence record
//! ```

pub mod timeline;
