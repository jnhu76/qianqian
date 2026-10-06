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
//! responsive  the responsive classes and the supported minimum
//! projection  read-side labels derived from one coherent observation
//! state       TuiModel: the presentation state and its mutators
//! interaction route/focus/modal/viewport behavior on TuiModel
//! input       the keyboard decoder
//! mouse       the mouse decoder
//! ```

mod actions;
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

pub use actions::{PlaylistCursor, Step, TuiAction, TuiRoute};
#[cfg(test)]
pub use controls::SeekButton;
pub use controls::{
    PLAYLIST_BUTTONS, PlaylistButton, PreferenceButton, SEEK_BUTTONS, TRANSPORT, TransportButton,
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
pub use mouse::decode_mouse;
pub(crate) use projection::status_shape;
pub use projection::{
    BAR_WIDTH, MAX_STATUS_ROWS, PlaylistRow, dsp_summary, row_label, seek_fraction_target,
    seek_target,
};
#[cfg(test)]
pub use projection::{LARGE_SEEK_STEP_SECS, SEEK_STEP_SECS};
#[cfg(test)]
pub use responsive::{MIN_HEIGHT, MIN_WIDTH};
pub use responsive::{ResponsiveClass, responsive_class};
pub use state::TuiModel;

#[cfg(test)]
pub(crate) mod testutil;
