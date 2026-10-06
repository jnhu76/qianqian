//! The ONE active interactive focus target and the Tab move direction.

use super::actions::TuiRoute;
use super::controls::{
    AudioButton, EqAdjust, NavBarButton, PlaylistButton, PreferenceButton, SeekButton,
    TransportButton,
};
use super::modal::ModalButton;
use super::visualizer::VisualizerMode;

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
    /// One of the Audio route's toolbar controls (G3).
    AudioButton(AudioButton),
    /// One of the Audio route's EQ band steppers (G3): band index and
    /// which way. The band table is a fixed 10-row grid, so the pair
    /// IS the control identity.
    EqBand { band: usize, adjust: EqAdjust },
    /// One of the Visualizer route's mode buttons (G4).
    VisualizerMode(VisualizerMode),
    /// One of the nav bar's persistent application controls (G5):
    /// Help and Quit, present on every route.
    NavBar(NavBarButton),
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
