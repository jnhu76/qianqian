//! Reference-player TUI shell: the presentation/input adapter over the
//! F2 episode seam.
//!
//! The shell is split so every piece stays testable without a real
//! terminal:
//!
//! ```text
//! model    pure projection of one PlaybackSessionObservation plus the
//!          keyboard grammar; no I/O, no terminal, no mechanism types
//! view     ratatui rendering of the model (exercised on TestBackend)
//! runtime  crossterm terminal session: raw mode / alternate screen
//!          under a small RAII guard, a ~150 ms event loop, and the
//!          key → seam wiring
//! ```
//!
//! Boundary discipline (the reason this module is small): the TUI reads
//! the episode only through `PlaybackSessionHandle::observe()` and
//! acts on it only through `request_stop()`. It never sees K0 snapshot
//! types, FiberState, PcmEdge, decode/output mechanisms, or any
//! realtime path, and it renders no playback semantic that F2 has not
//! earned (no Playing/Starting/Paused/Stopping, no position, no
//! volume, no playlist).

pub mod model;
pub mod runtime;
pub mod view;
