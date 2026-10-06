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
