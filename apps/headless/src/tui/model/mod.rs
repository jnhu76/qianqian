//! The shell's pure model, split by concern: the truthful projection of
//! the player's committed episode plus the T1B interaction foundation —
//! the routes, the one typed [`TuiAction`] vocabulary, the focus model,
//! the hit regions, the modal state — and the input decoders that turn
//! raw terminal events into actions.
//!
//! Same truth-class discipline as [`crate::status`] (D14.2/D14.7):
//! `pending` states only "no terminal Fact committed yet"; command
//! state keeps its own vocabulary; unknown stays unknown. This module
//! performs no I/O and holds no playback truth of its own.
//!
//! ```text
//! terminal event
//!       ↓ decode_key / decode_mouse      (input / mouse)
//!    TuiAction                            one typed vocabulary
//!       ↓ dispatch                        (the runtime's ONE boundary)
//! presentation mutation
//!   or ReferencePlayerApp operation
//!   or existing playback command
//! ```
//!
//! The submodules:
//!
//! ```text
//! actions     routes, the action vocabulary, cursor/offer/step types
//! controls    the visible button vocabularies (transport, seek, preference)
//! modal       the ONE modal: kinds, the picker draft, editing steps
//! focus       the one active focus target and its moves
//! hit         hit regions, semantic targets, the armed click
//! responsive  the four operable shell classes, the shell fit, and the
//!             supported minimum (§27/§28)
//! projection  read-side labels derived from one coherent observation
//! state       TuiModel: the presentation state and its mutators
//! interaction route/focus/modal/viewport behavior on TuiModel
//! audio       the Audio route's per-operation drafts and read-side
//!             labels (G3)
//! input       the keyboard decoder
//! mouse       the mouse decoder
//! ```

mod actions;
mod audio;
mod controls;
mod focus;
mod hit;
mod input;
mod interaction;
mod modal;
mod mouse;
mod projection;
mod responsive;
mod state;
mod visualizer;

pub use actions::{PlaylistCursor, Step, TuiAction, TuiRoute};
#[cfg(test)]
pub use controls::SeekButton;
pub use controls::{
    AUDIO_BUTTONS, AudioButton, EqAdjust, NAV_BUTTONS, NavBarButton, PLAYLIST_BUTTONS,
    PlaylistButton, PreferenceButton, SEEK_BUTTONS, TRANSPORT, TransportButton,
};
pub use focus::FocusId;
#[cfg(test)]
pub use focus::FocusMove;
pub use hit::{HitRegion, HitTarget};
pub use input::decode_key;
pub use modal::{
    ConfirmKind, Modal, ModalButton, ModalConfirm, ModalInput, ModalKind, OpenPicker, PickerMode,
    PickerSubject,
};
#[cfg(test)]
pub use modal::{HELP_PAGE_LINES, HelpScroll};
pub use mouse::decode_mouse;
#[cfg(test)]
pub use projection::dsp_summary;
pub(crate) use projection::status_shape;
pub use projection::{
    BAR_WIDTH, MAX_STATUS_ROWS, PlaylistRow, row_label, seek_fraction_target, seek_target,
};
#[cfg(test)]
pub use projection::{LARGE_SEEK_STEP_SECS, SEEK_STEP_SECS};
#[cfg(test)]
pub use responsive::{MIN_HEIGHT, MIN_WIDTH, ResponsiveClass};
pub use responsive::{ShellFit, shell_fit};
pub use state::TuiModel;
pub use visualizer::{VISUALIZER_MODES, VisualizerMode};

#[cfg(test)]
pub(crate) mod testutil;
