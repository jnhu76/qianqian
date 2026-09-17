//! F4-GATE mechanism evidence: position/duration propositions and the
//! accounting shape a future product Position is read from.
//!
//! This crate is evidence, not production architecture. It proposes (and
//! executable oracles here pin) the shape the F4 gate documents: the
//! render leg derives from its own two mechanism-local values and
//! publishes ONE monotone source-frame sample into a session-owned cell,
//! which the observation reads with one pure load.
//!
//! ```text
//! handed_off  source PCM frames this episode submitted into the device
//!             buffer (read_frames -> ReleaseBuffer(n)). Mechanism-local
//!             accounting only: never published, never read by the
//!             observation. Monotone within one seek epoch.
//! tail        the render leg's own GetCurrentPadding reading of its
//!             queued-to-play tail, in source frames. NOT monotone.
//!             Taken once per loop iteration / park slice / drain check.
//!
//! writer      estimate = handed_off - min(tail, handed_off)
//!             published = max(published, estimate)   (monotone update)
//! reader      position = ONE pure load; undefined until the first
//!             publication (unknown is never collapsed to zero)
//! ```
//!
//! Monotonicity belongs to the publication, not to the reader: a
//! reader-side clamp over two separately published cells cannot live
//! inside the D14.2 pure-read seam, and is kept below only as an
//! executable negative control.
//!
//! Modules:
//!
//! ```text
//! src/timeline.rs        the shape + deterministic interleaving
//!                        oracles (all platforms), incl. the rejected
//!                        reader-side pair as a negative control
//! src/bin/f4probe.rs     physical WASAPI probe (Windows only):
//!                        Experiment A (handed-off vs padding vs
//!                        published sample across pause/EOF) and
//!                        Experiment B (IAudioClock comparison)
//! src/bin/f4duration.rs  duration provenance over the SongCore ABI
//!                        (non-Windows): Experiment C
//! RESULTS.md             the evidence record
//! ```

pub mod timeline;
