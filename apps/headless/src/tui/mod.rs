//! Reference-player TUI shell: the presentation/input adapter over the
//! F2 episode seam and the F6 Open composition command.
//!
//! The shell is split so every piece stays testable without a real
//! terminal:
//!
//! ```text
//! model    pure projection of the player's committed episode plus the
//!          keyboard grammar; no I/O, no terminal, no mechanism types
//! view     ratatui rendering of the model (exercised on TestBackend)
//! runtime  crossterm terminal session: raw mode / alternate screen
//!          under a small RAII guard, a ~150 ms event loop, the
//!          key → seam wiring, the O key's Open input line (file OR
//!          folder since U1), and the `?` keyboard-help overlay
//! ```
//!
//! The public surface is deliberately ONE item: [`run`], the session
//! the application transport starts. The model/view machinery is an
//! internal test seam (unit tests live inside the module); it is not
//! product API and must not grow into one.
//!
//! Boundary discipline (the reason this module is small): the TUI
//! reads the episode only through `PlaybackSessionHandle::observe()`,
//! acts on it only through the command seams, performs Open only
//! through [`crate::player::ReferencePlayerApp::open`] — with U1 input
//! expansion from [`crate::input`] preparing file/folder candidates
//! before any destructive step — and navigates
//! only through the player's `next_track`/`previous_track` (D14.6
//! playlist closure; the Track line is presentation of the player's
//! own navigation state) — whose outcomes are application composition
//! feedback rendered as text. It never sees
//! K0 snapshot types, FiberState, PcmEdge, decode/output mechanisms, or
//! any realtime path, and it renders no playback semantic that F2/F3/F6
//! have not earned (no Playing/Starting/Stopping — Paused is the one
//! D14.7-earned projection). What it DOES render beyond the raw
//! observation is the player's own application state: the desired volume
//! (D14.9) and the temporary playlist pane with its two navigation
//! cursors (the 2026-09-19 U2 amendment in ADR-PBK-002 D14.6) — labels
//! for decisions the App owns, never playback truth.

mod model;
mod runtime;
mod view;

pub use runtime::run;
