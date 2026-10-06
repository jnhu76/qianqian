//! The visible control vocabularies: the transport row, the seek row
//! and the preference row — each a small table the view renders, the
//! focus cycle walks, and the decoders converge on the same
//! [`TuiAction`](super::actions::TuiAction) with.

/// One transport control on the Now Playing route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportButton {
    /// The Open entry control (G1): the picker modal, converged with the
    /// O key on the same [`TuiAction::OpenModal`].
    Open,
    Previous,
    PlayPause,
    Stop,
    Next,
}

/// The transport controls in left-to-right render (and Tab) order.
pub const TRANSPORT: [TransportButton; 5] = [
    TransportButton::Open,
    TransportButton::Previous,
    TransportButton::PlayPause,
    TransportButton::Stop,
    TransportButton::Next,
];

impl TransportButton {
    /// The button's label. The central control's label is CONTEXTUAL
    /// (T0 transport freeze: Pause / Resume / Play) — the view renders
    /// it from [`TuiModel::play_pause_offer`], not from this table.
    pub fn label(self) -> &'static str {
        match self {
            TransportButton::Open => "Open",
            TransportButton::Previous => "◀ Prev",
            TransportButton::PlayPause => "Pause",
            TransportButton::Stop => "■ Stop",
            TransportButton::Next => "Next ▶",
        }
    }

    /// The button's label in the compact shell class. Same control,
    /// shorter spelling — never a different control set. The GLYPHS are
    /// the decoration and drop first (T0 retention priority); the words
    /// stay, because T0's minimum is text controls — a symbol-only
    /// button is an affordance the terminal may not render at all. (The
    /// central control stays context-labeled in every class; the short
    /// words fit the compact cell's six inner columns.)
    pub fn compact_label(self) -> &'static str {
        match self {
            TransportButton::Open => "Open",
            TransportButton::Previous => "Prev",
            TransportButton::PlayPause => "Pause",
            TransportButton::Stop => "Stop",
            TransportButton::Next => "Next",
        }
    }
}

/// One visible seek control on the Now Playing route's seek row (G1
/// F07): the discoverable relative-seek affordance the T0 input-parity
/// table freezes — visible, Tab-reachable, mouse-clickable, converging
/// on the SAME [`TuiAction::SeekRelative`] the arrow accelerators
/// produce. The controls exist only while the episode publishes the
/// evidence a relative seek is computed from (position + sample rate);
/// without evidence they render nothing and arm nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekButton {
    Back,
    Forward,
}

/// The seek controls in left-to-right render (and Tab) order.
pub const SEEK_BUTTONS: [SeekButton; 2] = [SeekButton::Back, SeekButton::Forward];

impl SeekButton {
    /// The button's label (the T0 wireframe's wording).
    pub fn label(self) -> &'static str {
        match self {
            SeekButton::Back => "Back 5s",
            SeekButton::Forward => "Forward 5s",
        }
    }
}

/// One visible preference control on the Now Playing route's preference
/// row (G1): the App-owned policies every core player function must
/// expose without a memorized shortcut. Each converges on the same
/// [`TuiAction`] its accelerator key produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceButton {
    VolumeDown,
    VolumeUp,
    Order,
    Repeat,
}

/// The preference controls in left-to-right render (and Tab) order;
/// the desired-volume label between the two steppers is display, not a
/// control, so it is not in this cycle.
pub const PREFERENCES: [PreferenceButton; 4] = [
    PreferenceButton::VolumeDown,
    PreferenceButton::VolumeUp,
    PreferenceButton::Order,
    PreferenceButton::Repeat,
];

/// One visible toolbar control on the Playlist route (G2, the T0
/// playlist wireframe). Every playlist product function is a button:
/// keyboard focus and the mouse both reach it, and each converges on
/// the same [`TuiAction`](super::actions::TuiAction) the route's other
/// paths produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistButton {
    /// The shared picker over the filesystem, restricted to a FILE
    /// subject (T0: "Playlist Add File uses the same picker with
    /// file-only final selection").
    AddFile,
    /// The shared picker restricted to a FOLDER subject.
    AddFolder,
    /// Play the SELECTED row through the same Open replacement — the
    /// same action Enter on the focused list performs.
    PlaySelected,
    /// Remove the selected row. Removing the CURRENT row asks for the
    /// frozen stop-aware confirmation first (T0 owner decision);
    /// removing a non-current row is a list edit only.
    Remove,
    /// Clear the whole list. Always asks for the frozen stop-aware
    /// confirmation first (T0 owner decision) while an episode is live.
    Clear,
}

/// The playlist toolbar controls in left-to-right render (and Tab)
/// order (the T0 wireframe's row).
pub const PLAYLIST_BUTTONS: [PlaylistButton; 5] = [
    PlaylistButton::AddFile,
    PlaylistButton::AddFolder,
    PlaylistButton::PlaySelected,
    PlaylistButton::Remove,
    PlaylistButton::Clear,
];

impl PlaylistButton {
    /// The button's label (the T0 wireframe's wording).
    pub fn label(self) -> &'static str {
        match self {
            PlaylistButton::AddFile => "[Add File...]",
            PlaylistButton::AddFolder => "[Add Folder...]",
            PlaylistButton::PlaySelected => "[Play selected]",
            PlaylistButton::Remove => "[Remove]",
            PlaylistButton::Clear => "[Clear...]",
        }
    }

    /// The button's label in the compact classes. Same controls,
    /// shorter spellings — the words stay, the decoration drops.
    pub fn compact_label(self) -> &'static str {
        match self {
            PlaylistButton::AddFile => "[+File]",
            PlaylistButton::AddFolder => "[+Folder]",
            PlaylistButton::PlaySelected => "[Play]",
            PlaylistButton::Remove => "[Remove]",
            PlaylistButton::Clear => "[Clear]",
        }
    }
}

/// One visible toolbar control on the Audio route (G3). The route's
/// product functions are buttons, like every route's: keyboard focus
/// and the mouse both reach them, and each converges on the same
/// [`TuiAction`](super::actions::TuiAction).
///
/// T0 freezes editing as PER OPERATION: each button belongs to one of
/// the four admitted operations (enablement, preset, preamp, EQ), and
/// each operation commits through its own App seam — there is no
/// global Apply transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioButton {
    /// Toggle the desired processing enablement (bypass vs on) —
    /// commits through `set_processing_enabled`.
    Enabled,
    /// Step the preamp draft down one visible press (linear gain).
    PreampDown,
    /// Step the preamp draft up one visible press (linear gain).
    PreampUp,
    /// Commit the preamp draft through `set_preamp`.
    PreampCommit,
    /// Discard the preamp draft: restore the latest desired value,
    /// send nothing.
    PreampCancel,
    /// Open the EQ preset picker modal (the whole-configuration
    /// operation).
    Presets,
    /// Commit the EQ draft through exactly one `set_eq_config`.
    EqCommit,
    /// Revert the EQ draft: restore the latest desired stage, send
    /// nothing.
    EqRevert,
}

/// The audio toolbar controls in left-to-right render (and Tab) order:
/// the enablement and the preamp operation on the first row, the EQ
/// operation and the preset picker on the second.
pub const AUDIO_BUTTONS: [AudioButton; 8] = [
    AudioButton::Enabled,
    AudioButton::PreampDown,
    AudioButton::PreampUp,
    AudioButton::PreampCommit,
    AudioButton::PreampCancel,
    AudioButton::Presets,
    AudioButton::EqCommit,
    AudioButton::EqRevert,
];

impl AudioButton {
    /// The button's label. The enablement control's label is
    /// CONTEXTUAL (like the transport's central control): the view
    /// renders it from the draft (or the desired configuration when no
    /// draft is open), showing what activation WOULD commit.
    pub fn label(self, enabled: bool, compact: bool) -> &'static str {
        match self {
            AudioButton::Enabled => match (enabled, compact) {
                (true, false) => "[DSP: on]",
                (false, false) => "[DSP: off]",
                (true, true) => "[DSP on]",
                (false, true) => "[DSP off]",
            },
            AudioButton::PreampDown => {
                if compact {
                    "[Pre−]"
                } else {
                    "[Preamp −]"
                }
            }
            AudioButton::PreampUp => {
                if compact {
                    "[Pre+]"
                } else {
                    "[Preamp +]"
                }
            }
            AudioButton::PreampCommit => {
                if compact {
                    "[Set pre]"
                } else {
                    "[Set preamp]"
                }
            }
            AudioButton::PreampCancel => {
                if compact {
                    "[Cancel]"
                } else {
                    "[Cancel edit]"
                }
            }
            AudioButton::Presets => {
                if compact {
                    "[Presets]"
                } else {
                    "[EQ preset...]"
                }
            }
            AudioButton::EqCommit => {
                if compact {
                    "[Apply]"
                } else {
                    "[Apply EQ]"
                }
            }
            AudioButton::EqRevert => {
                if compact {
                    "[Revert]"
                } else {
                    "[Revert draft]"
                }
            }
        }
    }
}

/// Which way one EQ band's trim adjusts. The band steppers are two
/// buttons per band (− / +), the same affordance grammar as every
/// other stepper in the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqAdjust {
    Cut,
    Boost,
}

/// One persistent application control at the nav bar's right end (G5):
/// Help and Quit live on every route and in every responsive class
/// above the minimum — the two things a first-time user must always be
/// able to find. They are buttons like any other: keyboard focus and
/// the mouse both reach them, and each converges on the same
/// [`TuiAction`](super::actions::TuiAction) its accelerator key
/// produces (`?` and `Q`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavBarButton {
    /// Open the help overlay (the `?` key's action).
    Help,
    /// Quit (the `Q` key's action).
    Quit,
}

/// The nav bar's persistent controls in left-to-right render (and Tab)
/// order, after the route tabs.
pub const NAV_BUTTONS: [NavBarButton; 2] = [NavBarButton::Help, NavBarButton::Quit];

impl NavBarButton {
    /// The button's label; the bracket names the accelerator, the same
    /// affordance grammar as every other bracketed button.
    pub fn label(self) -> &'static str {
        match self {
            NavBarButton::Help => "[?] Help",
            NavBarButton::Quit => "[Q] Quit",
        }
    }

    /// The label in the compact shell class. Same controls, shorter
    /// spellings — the accelerators stay discoverable through the
    /// help overlay itself.
    pub fn compact_label(self) -> &'static str {
        match self {
            NavBarButton::Help => "Help",
            NavBarButton::Quit => "Quit",
        }
    }
}
