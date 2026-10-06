//! The presentation routes and the ONE typed action vocabulary: every
//! keyboard and mouse path converges here before anything touches a
//! product seam.

use super::focus::FocusMove;
use super::modal::{ModalInput, ModalKind};

/// The frozen four routes (T0 product/design record, Issue #188).
/// Changing route is presentation-only: it alters the active route, the
/// focus and route-local presentation state, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiRoute {
    NowPlaying,
    Playlist,
    Audio,
    Visualizer,
}

impl TuiRoute {
    /// The route cycle in tab order (left to right).
    pub const ALL: [TuiRoute; 4] = [
        TuiRoute::NowPlaying,
        TuiRoute::Playlist,
        TuiRoute::Audio,
        TuiRoute::Visualizer,
    ];

    /// The tab label at full width.
    pub fn label(self) -> &'static str {
        match self {
            TuiRoute::NowPlaying => "Now Playing",
            TuiRoute::Playlist => "Playlist",
            TuiRoute::Audio => "Audio",
            TuiRoute::Visualizer => "Visualizer",
        }
    }

    /// The tab label in the compact shell class. Same route, shorter
    /// spelling — never a different route set.
    pub fn compact_label(self) -> &'static str {
        match self {
            TuiRoute::NowPlaying => "Now",
            TuiRoute::Playlist => "List",
            TuiRoute::Audio => "Audio",
            TuiRoute::Visualizer => "Viz",
        }
    }
}
/// Where a playlist selection moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistCursor {
    Previous,
    Next,
    /// An absolute traversal index (a mouse row hit).
    Row(usize),
}

/// The ONE typed action vocabulary (§7). Keyboard and mouse converge
/// here before anything touches a product seam; there are no separate
/// product semantics per input method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiAction {
    /// Switch the active route. Presentation-only (§5).
    Navigate(TuiRoute),
    /// Tab / Shift+Tab: the next/previous visible enabled control.
    MoveFocus(FocusMove),
    /// Enter on the focused control.
    ActivateFocused,
    /// Open the one modal of this kind (closes any previous one —
    /// there is no stack, §24).
    OpenModal(ModalKind),
    /// One editing step inside the active modal.
    ModalInput(ModalInput),

    // Transport (episode commands through the existing seams).
    /// Space: pause when pause intent is not recorded, resume when it
    /// is. One key, two commands — never a local `paused` bool.
    PlayPause,
    Stop,
    /// The manual Previous traversal step (Issue #166 §34).
    Previous,
    /// The manual Next traversal step (Issue #166 §35).
    Next,
    /// Seek by a signed number of seconds (±5 arrows, ±30 shifted).
    /// The SAME frozen seek command as before; a target with no
    /// computable position is never sent.
    SeekRelative(i64),
    /// A click-to-position seek on the visible progress bar (G1 §9):
    /// the wanted position as per-mille of the episode's duration.
    /// The runtime derives the actual target from a FRESH coherent
    /// observation — the fraction chooses WHERE, the episode's own
    /// evidence chooses WHAT — and a duration without evidence gets no
    /// command at all.
    SeekPerMille(u16),

    // Playlist route.
    /// Move the selection (↑/↓, or a mouse row hit). Presentation of
    /// the App's selection — it never plays. The viewport reveals the
    /// selected row (T0: keyboard navigation selects and reveals).
    PlaylistSelect(PlaylistCursor),
    /// Enter on the list, or the [Play selected] toolbar button: play
    /// the SELECTED row through the same Open replacement (Issue #166
    /// §19).
    PlaylistPlaySelected,
    /// The [Add File...] toolbar button (G2): the shared picker in its
    /// file-only Add mode (T0 Add File policy). The picker commits
    /// through the same shared input expansion; append never plays.
    PlaylistAddFile,
    /// The [Add Folder...] toolbar button: the shared picker in its
    /// folder-only Add mode (T0 Add Folder policy).
    PlaylistAddFolder,
    /// The [Remove] toolbar button (G2): remove the SELECTED row.
    /// Removing the CURRENT row opens the frozen stop-aware
    /// confirmation first; removing a non-current row is a list edit
    /// only. The App owns the retire-then-edit ordering.
    PlaylistRemove,
    /// The [Clear...] toolbar button: clear the whole list, with the
    /// frozen stop-aware confirmation first while an episode is live.
    PlaylistClear,

    // Audio route (G3, the T0 per-operation contract): each operation
    // edits its own local draft and commits through its OWN App seam —
    // there is no global Apply transaction. Nothing here claims
    // applied state.
    /// Toggle the desired processing enablement (bypass vs on) —
    /// commits through `set_processing_enabled`.
    DspToggleEnabled,
    /// Step the preamp draft one visible press (signed; linear gain).
    /// An integer press count keeps the typed vocabulary `Eq`; the
    /// step size is the model's presentation tuning.
    DspPreampStep(i32),
    /// Step one EQ band's trim: band index, signed decibels.
    DspEqBandStep(usize, i32),
    /// Open the EQ preset menu (the whole-configuration operation).
    DspOpenPresets,
    /// Commit the preamp draft through `set_preamp` ([Set preamp]).
    DspPreampCommit,
    /// Discard the preamp draft: restore the latest desired value,
    /// send nothing ([Cancel edit]).
    DspPreampCancel,
    /// Commit the EQ draft through exactly one `set_eq_config`
    /// ([Apply EQ]); preserves enabled and preamp.
    DspApplyEq,
    /// Discard the EQ draft: restore the latest desired stage, send
    /// nothing ([Revert draft]).
    DspRevertEq,

    // Visualizer route (G4).
    /// Switch the active visualization mode (presentation only).
    SetVisualizerMode(super::visualizer::VisualizerMode),

    // Application policy.
    /// R: toggle Sequential ↔ Shuffle (Issue #166 §25).
    ToggleOrder,
    /// L: cycle Repeat Off → All → One → Off (Issue #166 §12).
    CycleRepeat,
    /// '+'/'=': raise the App's desired stream factor by one step
    /// (D14.9: step 5).
    VolumeUp,
    /// '-': lower the App's desired stream factor by one step.
    VolumeDown,
    Quit,
}
/// What the central transport control currently offers (its context
/// label and dispatch): see [`TuiModel::play_pause_offer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayPauseOffer {
    Pause,
    Resume,
    Play,
}

impl PlayPauseOffer {
    /// The label the central control renders for this offer, in every
    /// responsive class (the words fit the smallest transport cell).
    pub fn label(self) -> &'static str {
        match self {
            PlayPauseOffer::Pause => "Pause",
            PlayPauseOffer::Resume => "Resume",
            PlayPauseOffer::Play => "Play",
        }
    }
}

/// One event-loop step after a dispatched action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Continue,
    Exit,
}
