//! Reference-player TUI shell: the presentation/input adapter over the
//! F2 episode seam and the F6 Open composition command.
//!
//! The shell is split so every piece stays testable without a real
//! terminal:
//!
//! ```text
//! model    pure projection of the player's committed episode plus the
//!          T1B interaction foundation: the four TuiRoutes, the one
//!          typed TuiAction vocabulary, the focus model, the hit
//!          regions, the one-modal state, the armed-click rule, and the
//!          keyboard/mouse decoders that converge both input methods
//!          onto that vocabulary
//! view     ratatui rendering of the shell (top navigation tabs, the
//!          active route's body, the bottom status line, modal popups),
//!          exercised on TestBackend; it publishes each frame's hit
//!          regions from the same layout it drew
//! runtime  crossterm terminal session: raw mode / alternate screen /
//!          mouse capture under a small RAII guard, a ~150 ms event
//!          loop, the ONE dispatch boundary every decoded TuiAction
//!          goes through, and the App/playback command routing
//! ```
//!
//! The public surface is deliberately ONE item: [`run`], the session
//! the application transport starts. The model/view machinery is an
//! internal test seam (unit tests live inside the modules); it is not
//! product API and must not grow into one.
//!
//! Boundary discipline (the reason this module stays small): the TUI
//! reads the episode only through `PlaybackSessionHandle::observe()`,
//! acts on it only through the command seams, performs Open only
//! through [`crate::player::ReferencePlayerApp::open`] — with U1 input
//! expansion from [`crate::input`] preparing file/folder candidates
//! before any destructive step — and navigates only through the
//! player's `next_track`/`previous_track`/`play_selected` (D14.6
//! playlist closure). It never sees K0 snapshot types, FiberState,
//! PcmEdge, decode/output mechanisms, or any realtime path, and it
//! renders no playback semantic that F2/F3/F6 have not earned. The
//! interaction state it DOES own is presentation-only (route, focus,
//! modal, armed click, hit regions, feedback labels) — never playback
//! truth.
//!
//! v2 scope note (Issue #188 QIANQIAN-TUI-V2): all four routes ship —
//! Now Playing and Playlist (T1B/T2), the Audio route's desired-DSP
//! draft workbench (G3), the Visualizer route over the Observation
//! Plane's telemetry (G4) — plus the shared picker, the stop-aware
//! confirmations, the playlist toolbar and viewport, and the nav bar's
//! persistent Help/Quit controls (G5) with the scrollable,
//! workflow-first help overlay.

mod model;
mod runtime;
mod view;

pub use runtime::run;
