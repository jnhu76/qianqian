//! The ONE active interactive focus target and the Tab move direction.

use super::actions::TuiRoute;
use super::controls::{PlaylistButton, PreferenceButton, SeekButton, TransportButton};
use super::modal::ModalButton;

/// The ONE active interactive focus target (§10). Focus is presentation
/// state only: it selects which control Enter activates and which group
/// owns the contextual arrows, and it must never become product
/// authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusId {
    /// One of the four route tabs.
    RouteTab(TuiRoute),
    /// One of the Now Playing seek buttons (G1 F07), while the
    /// episode's evidence makes a relative seek computable.
    Seek(SeekButton),
    /// One of the Now Playing transport buttons.
    Transport(TransportButton),
    /// One of the Now Playing preference controls.
    Preference(PreferenceButton),
    /// The playlist list (the selection cursor is the focus inside it).
    Playlist,
    /// One of the Playlist route's toolbar controls (G2).
    PlaylistButton(PlaylistButton),
    /// The Open picker's directory listing (the picker cursor is the
    /// focus inside it).
    PickerList,
    /// One of the Open picker's visible buttons.
    PickerButton(ModalButton),
    /// The text field of the active modal.
    ModalField,
}

/// The direction of a Tab / Shift+Tab focus move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMove {
    Next,
    Previous,
}
