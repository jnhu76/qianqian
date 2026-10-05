//! Pure TUI model: the truthful projection of one episode observation
//! into display labels, plus the T1B interaction foundation — the
//! routes, the one typed action vocabulary, the focus model, the hit
//! regions and the modal state — and the input decoders that turn raw
//! terminal events into [`TuiAction`]s.
//!
//! Same truth-class discipline as [`crate::status`] (D14.2/D14.7):
//! `pending` states only "no terminal Fact committed yet" — never
//! Playing/Starting/Stopping; `stop_requested`/`pause_requested` are
//! Command state; `source_format` is mechanism evidence; the `Paused`
//! projection is derived by the seam itself from the frozen D14.7
//! establishment conjunction; the diagnostics are presentation text.
//! This module performs no I/O and holds no playback truth of its own;
//! every label is derived from the last observation handed to
//! [`TuiModel::update`].
//!
//! The playlist pane is the same shape of thing (Issue #166 §6): a
//! presentation PROJECTION of the App's own navigation state. No row,
//! marker or count here is playback truth.
//!
//! # The interaction foundation (T1B, Issue #188 QIANQIAN-TUI-V2)
//!
//! ```text
//! terminal event
//!       ↓ decode_key / decode_mouse      (this module)
//!    TuiAction                            one typed vocabulary
//!       ↓ dispatch                        (the runtime's ONE boundary)
//! presentation mutation
//!   or ReferencePlayerApp operation
//!   or existing playback command
//! ```
//!
//! Keyboard and mouse are input METHODS; [`TuiAction`] is the semantic
//! presentation action. The decoders below converge both into the same
//! vocabulary, so one physical event decodes to at most one action and
//! the same action means the same operation regardless of source.
//!
//! The TUI owns only presentation state (route, focus, modal, armed
//! click, this frame's hit regions, operation feedback) — never a
//! playing/paused/position truth; those are re-read from the seams.

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use std::path::Path;
use std::time::Duration;

use crate::playlist::{PlaybackOrder, RepeatMode};
use qianqian_playback::{
    AudioProcessingConfig, EpisodeTerminalOutcome, EqPreset, PauseEngagement,
    PlaybackSessionObservation,
};

/// One playlist row as the shell presents it: the display label and the
/// two INDEPENDENT markers. A projection of the App's navigation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistRow {
    pub label: String,
    /// This row is the committed (playing) position.
    pub playing: bool,
    /// This row is the UI selection.
    pub selected: bool,
}

/// The row label for one source (Issue #166 §22): the file name when the
/// path has one, the whole path otherwise. The filename IS the title for
/// this campaign — no metadata is read, and no path is invented.
pub fn row_label(path: &Path) -> String {
    match path.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => path.to_string_lossy().into_owned(),
    }
}

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
    /// The button's label.
    pub fn label(self) -> &'static str {
        match self {
            TransportButton::Open => "Open",
            TransportButton::Previous => "◀ Prev",
            TransportButton::PlayPause => "Play/Pause",
            TransportButton::Stop => "■ Stop",
            TransportButton::Next => "Next ▶",
        }
    }

    /// The button's label in the compact shell class. Same control,
    /// shorter spelling — never a different control set.
    pub fn compact_label(self) -> &'static str {
        match self {
            TransportButton::Open => "Open",
            TransportButton::Previous => "◀",
            TransportButton::PlayPause => "P/P",
            TransportButton::Stop => "■",
            TransportButton::Next => "▶",
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

/// The ONE active interactive focus target (§10). Focus is presentation
/// state only: it selects which control Enter activates and which group
/// owns the contextual arrows, and it must never become product
/// authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusId {
    /// One of the four route tabs.
    RouteTab(TuiRoute),
    /// One of the Now Playing transport buttons.
    Transport(TransportButton),
    /// One of the Now Playing preference controls.
    Preference(PreferenceButton),
    /// The playlist list (the selection cursor is the focus inside it).
    Playlist,
    /// The Open picker's directory listing (the picker cursor is the
    /// focus inside it).
    PickerList,
    /// One of the Open picker's commit buttons.
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

/// The modal kind to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalKind {
    /// The Open input line (D14.6): a literal file-or-folder path.
    Open,
    /// The GoTo exact-seek line (Issue #166 §27).
    GoTo,
    /// The keyboard/mouse help overlay.
    Help,
}

/// One entry of the Open picker's listing: the display name and the
/// kind the runtime's `list_directory` classified it as. A
/// presentation draft inside the modal — no admission happened by
/// listing anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerEntry {
    pub name: String,
    pub is_dir: bool,
    /// The synthesized `..` row: descending goes to the parent
    /// directory (G1 §8 parent navigation).
    pub is_parent: bool,
}

/// The Open modal as a terminal-native picker (G1 §8): an editable
/// path line over a one-level listing of the directory it names, with
/// the commit buttons and their accelerators. The listing is a
/// presentation draft the runtime refreshes (the model performs no
/// I/O); admission still happens only at commit, through the same
/// shared input expansion as before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPicker {
    /// The path line the user can type into (the pre-picker editing
    /// semantics, unchanged).
    pub input: String,
    /// The directory the listing shows, when one has been listed.
    pub dir: Option<std::path::PathBuf>,
    /// The listed entries, runtime-supplied (directories first, then
    /// audio-candidate files).
    pub entries: Vec<PickerEntry>,
    /// The listing cursor (the selection), when the listing has rows.
    pub cursor: Option<usize>,
    /// The listing's honest failure diagnostic (an unreadable
    /// directory), shown inside the modal instead of a fabricated
    /// empty list.
    pub error: Option<String>,
}

/// The ONE active modal (§23/§24), replacing the old collection of
/// modal booleans. At most one exists; there is no modal stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    Open(OpenPicker),
    GoTo { input: String },
    Help,
}

impl Modal {
    /// Which kind this modal is.
    pub fn kind(&self) -> ModalKind {
        match self {
            Modal::Open(_) => ModalKind::Open,
            Modal::GoTo { .. } => ModalKind::GoTo,
            Modal::Help => ModalKind::Help,
        }
    }

    /// The modal's text content, for the two text modals.
    #[cfg(test)]
    pub fn input(&self) -> Option<&str> {
        match self {
            Modal::Open(picker) => Some(picker.input.as_str()),
            Modal::GoTo { input } => Some(input),
            Modal::Help => None,
        }
    }
}

/// One editing step inside a text modal. The modal's editing keys are
/// actions like any other, so a modal key press converges on the same
/// single dispatch boundary as everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalInput {
    Char(char),
    Backspace,
    /// Enter on the path field: confirm the typed subject.
    Confirm,
    /// Enter on the picker listing: a directory descends into view, a
    /// file commits the Open.
    ListActivate,
    /// The picker listing's cursor moves (arrows, wheel, a row click).
    ListMove(PlaylistCursor),
    /// The picker steps up to the parent directory (Backspace on the
    /// list, a click on the `..` row).
    ListParent,
    /// The [Open] button (or its accelerator): commit the picked
    /// subject through the Open composition.
    CommitOpen,
    /// The [Add to Playlist] button (or its accelerator): append the
    /// picked subject through the same shared input expansion.
    CommitAdd,
    /// Esc (or `?` for Help): close the modal. The closing event is
    /// consumed by the modal — it never also acts on the background.
    Cancel,
}

/// One of the Open picker's three visible commit buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalButton {
    Open,
    Add,
    Cancel,
}

/// What confirming the active modal decided. The GoTo reader is the
/// EXISTING [`crate::cli::parse_seek_time`] — the shell's one time
/// grammar, shared with the scriptable transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalConfirm {
    /// Nothing to do (a Help confirm, or an empty line: a cancel).
    Nothing,
    /// The confirmed Open candidate; the modal is already closed.
    Open(String),
    /// The confirmed seek target; the modal is already closed.
    Seek(Duration),
    /// The token is not a readable time. The GoTo modal STAYS OPEN for
    /// correction and the shell shows the bounded diagnostic — a
    /// malformed seek intent is never sent.
    Unreadable(&'static str),
}

/// The semantic target of one hit region: a rendered, currently valid
/// control (§13). No widget tree, no DOM, no retained component graph —
/// only what THIS frame actually drew.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    RouteTab(TuiRoute),
    Transport(TransportButton),
    /// One of the preference-row controls.
    Preference(PreferenceButton),
    /// A playlist row, by absolute traversal index.
    PlaylistRow(usize),
    /// The playlist list's content area (the wheel-scroll target).
    PlaylistPane,
    /// The position bar WITH duration evidence: a click-to-position
    /// seek affordance (G1 §9). Without duration evidence the bar is a
    /// display and publishes no region at all.
    SeekBar,
    /// One visible row of the Open picker's listing, by list index
    /// (index 0 is the synthesized `..` parent row when shown).
    PickerRow(usize),
    /// The Open picker's path-field row (a click focuses the field).
    ModalField,
    /// One of the Open picker's commit buttons.
    ModalButton(ModalButton),
}

/// One frame's hit region: a rectangle plus the semantic target drawn
/// into it (§13). Regions belong to ONE rendered presentation state and
/// are republished (or dropped) by every draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitRegion {
    pub area: Rect,
    pub target: HitTarget,
}

/// The responsive shell class (§27). Exact thresholds are presentation
/// tuning, not authority; they exist so controls do not overlap, focus
/// targets stay visible, and resize produces fresh geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsiveClass {
    Wide,
    Normal,
    Compact,
    /// Below the implementation's minimum: no interactive layout, no
    /// hit regions, no focus — one truthful message instead (§28).
    Minimum,
}

/// Below this width (or [`MIN_HEIGHT`] height) the shell refuses to
/// render the interactive layout (§28). Playback continues under the
/// product's own semantics; resizing back restores the UI.
pub const MIN_WIDTH: u16 = 40;
/// See [`MIN_WIDTH`].
pub const MIN_HEIGHT: u16 = 14;

/// Derive the responsive class for one terminal size (§27).
pub fn responsive_class(width: u16, height: u16) -> ResponsiveClass {
    if width < MIN_WIDTH || height < MIN_HEIGHT {
        ResponsiveClass::Minimum
    } else if width < 60 || height < 18 {
        ResponsiveClass::Compact
    } else if width < 100 {
        ResponsiveClass::Normal
    } else {
        ResponsiveClass::Wide
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
    /// Move the selection (↑/↓, a mouse row hit, or a wheel step over
    /// the list). Presentation of the App's selection — it never plays.
    PlaylistSelect(PlaylistCursor),
    /// Enter on the list: play the SELECTED row through the same Open
    /// replacement (Issue #166 §19).
    PlaylistPlaySelected,

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

/// The small seek step the plain arrow keys request (D14.5): five
/// seconds of media time.
pub const SEEK_STEP_SECS: i64 = 5;
/// The large seek step Shift+arrow requests (Issue #166 §26): thirty
/// seconds of the same media time.
pub const LARGE_SEEK_STEP_SECS: i64 = 30;

/// The read-only progress bar's width in cells (Issue #166 §33).
pub const BAR_WIDTH: usize = 24;

/// The seek target one seek action requests, derived from ONE coherent
/// observation of the episode: the position Projection (D14.8, source
/// PCM frames) converted with that same observation's published sample
/// rate. `None` means the seek is inert for this episode: with no
/// position sample (or no rate to convert it) there is no target to
/// compute, and a seek with no computable target is never SENT — no
/// fabricated zero, no seek to the episode start, no command at all.
pub fn seek_target(
    observation: &PlaybackSessionObservation,
    step: Duration,
    forward: bool,
) -> Option<Duration> {
    let rate = u64::from(observation.source_format?.sample_rate);
    if rate == 0 {
        return None;
    }
    let position = observation.position?;
    let current = Duration::from_micros(position * 1_000_000 / rate);
    Some(if forward {
        current.saturating_add(step)
    } else {
        current.saturating_sub(step)
    })
}

/// The click-to-position seek target (G1 §9): `per_mille` of the
/// episode's PUBLISHED duration evidence, clamped into it. Like
/// [`seek_target`], it is `None` — no command at all — whenever the
/// duration evidence does not exist: an unknown timeline is never
/// seekable by fraction, and nothing is fabricated in its place.
pub fn seek_fraction_target(
    observation: &PlaybackSessionObservation,
    per_mille: u16,
) -> Option<Duration> {
    let duration = observation.source_duration?;
    if duration.is_zero() {
        return None;
    }
    let per_mille = u128::from(per_mille.min(1000));
    let micros = duration.as_micros() * per_mille / 1000;
    Some(Duration::from_micros(u64::try_from(micros).ok()?))
}

/// The one-line summary of the App's DESIRED DSP configuration (G1:
/// the Now Playing route names it; the Audio route will edit it). A
/// desired-state statement only — the word is part of the line — and
/// the parts come from the T1A product data: bypass vs enabled, the
/// EQ stage as a matched factory preset or `custom EQ`, and the
/// preamp in dB. Nothing here is an applied-DSP claim.
pub fn dsp_summary(config: &AudioProcessingConfig) -> String {
    if !config.enabled {
        return "DSP (desired): off (bypass)".to_owned();
    }
    let mut parts = Vec::new();
    if let Some(eq) = &config.eq {
        let preset = EqPreset::all()
            .into_iter()
            .find(|preset| preset.to_config().eq == Some(*eq));
        parts.push(match preset {
            Some(preset) => format!("preset {}", preset.name()),
            None => "custom EQ".to_owned(),
        });
    }
    let preamp_db = if config.gain > 0.0 {
        format!("{:+.1} dB", 20.0 * config.gain.log10())
    } else {
        "-inf dB".to_owned()
    };
    parts.push(format!("preamp {preamp_db}"));
    format!("DSP (desired): on — {}", parts.join(", "))
}

/// One event-loop step after a dispatched action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Continue,
    Exit,
}

/// One frame's worth of presentation state: the episode the player has
/// committed (source path + latest coherent observation), the playlist
/// rows, and the T1B interaction state (route, focus, modal, armed
/// click, this frame's hit regions, responsive class). All of it is
/// presentation: the shell keeps no playback truth of its own.
pub struct TuiModel {
    /// The committed episode's source path. `None` is a real state
    /// (F6): no episode is live — a clean-failed Open leaves no
    /// runtime, and the honest panel says so instead of fabricating
    /// labels.
    source: Option<String>,
    observation: PlaybackSessionObservation,
    /// The last operation's feedback — application composition feedback
    /// (D14.6), never a playback semantic.
    status: Option<String>,
    /// The player's navigation projection (1-based cursor, playlist
    /// length), refreshed with the episode.
    navigation_position: Option<(usize, usize)>,
    /// The playlist pane's rows, rebuilt only when the App's playlist
    /// revision moves (so a 5 000-row list costs nothing per frame).
    playlist: Vec<PlaylistRow>,
    playlist_revision: Option<u64>,
    /// The App's traversal order / repeat preferences (labels only).
    order: Option<PlaybackOrder>,
    repeat: Option<RepeatMode>,
    /// The App's desired stream factor (D14.9 read side: exactly the
    /// configured value — never an acoustic level or mechanism
    /// readback).
    volume: Option<u8>,
    /// The App's desired DSP configuration, as the one-line summary
    /// label built by [`dsp_summary`] (T1A seam read side). Always a
    /// DESIRED-state statement: nothing here is an applied-DSP claim
    /// (G3 §20 discipline applies to the summary line too).
    desired_dsp: Option<String>,

    /// The active route (§5). Presentation-only: default Now Playing.
    route: TuiRoute,
    /// The one active focus target, if any (§10).
    focus: Option<FocusId>,
    /// The one active modal, if any (§23).
    modal: Option<Modal>,
    /// The armed-click target between Left Down and Left Up (§17).
    armed: Option<HitTarget>,
    /// The CURRENT frame's hit regions (§13/§15). Empty before the
    /// first draw and after every invalidation.
    regions: Vec<HitRegion>,
    /// The current responsive class, set by every draw (§27).
    class: ResponsiveClass,
}

impl TuiModel {
    /// A model for an episode whose observation has not been read yet:
    /// every label starts at the honest "unknown/pending" projection.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: Some(source.into()),
            observation: PlaybackSessionObservation {
                terminal_outcome: None,
                failure_diagnostic: None,
                stop_requested: false,
                pause_requested: false,
                source_format: None,
                source_duration: None,
                position: None,
                pause_engagement: PauseEngagement::Disengaged,
                activation_error: None,
                last_processing_refusal: None,
            },
            status: None,
            navigation_position: None,
            playlist: Vec::new(),
            playlist_revision: None,
            order: None,
            repeat: None,
            volume: None,
            desired_dsp: None,
            route: TuiRoute::NowPlaying,
            focus: None,
            modal: None,
            armed: None,
            regions: Vec::new(),
            class: responsive_class(u16::MAX, u16::MAX),
        }
    }

    /// Replace the projected truth with one fresh coherent read. Pure:
    /// the caller got the observation from the episode seam.
    pub fn update(&mut self, observation: PlaybackSessionObservation) {
        self.observation = observation;
    }

    /// Record the player's navigation projection (D14.6).
    pub fn set_navigation(&mut self, position: Option<(usize, usize)>) {
        self.navigation_position = position;
    }

    /// Record the playlist pane's rows, keyed by the App's playlist
    /// revision. A revision the model already holds does not even build
    /// the rows: the caller may call this on every refresh, and an
    /// unchanged (or huge) playlist costs nothing per frame.
    pub fn set_playlist(&mut self, revision: u64, rows: impl FnOnce() -> Vec<PlaylistRow>) {
        if self.playlist_revision == Some(revision) {
            return;
        }
        self.playlist_revision = Some(revision);
        self.playlist = rows();
    }

    /// The playlist pane's rows, in the App's traversal order.
    pub fn playlist(&self) -> &[PlaylistRow] {
        &self.playlist
    }

    /// Record the App's traversal order preference.
    pub fn set_order(&mut self, order: PlaybackOrder) {
        self.order = Some(order);
    }

    /// Record the App's repeat preference.
    pub fn set_repeat(&mut self, repeat: RepeatMode) {
        self.repeat = Some(repeat);
    }

    /// The order label (`Sequential` / `Shuffle`), once known.
    pub fn order_label(&self) -> Option<&'static str> {
        self.order.map(PlaybackOrder::label)
    }

    /// The repeat label (`Off` / `All` / `One`), once known.
    pub fn repeat_label(&self) -> Option<&'static str> {
        self.repeat.map(RepeatMode::label)
    }

    /// Record the player's desired stream factor (D14.9 read side).
    pub fn set_volume(&mut self, volume: Option<u8>) {
        self.volume = volume;
    }

    /// The desired stream factor label: the App's configured value.
    pub fn volume_label(&self) -> Option<String> {
        self.volume.map(|v| format!("{v}/100"))
    }

    /// Record the App's desired DSP summary line (T1A seam read side).
    pub fn set_desired_dsp(&mut self, summary: String) {
        self.desired_dsp = Some(summary);
    }

    /// The desired DSP summary line, once refreshed from the App.
    pub fn desired_dsp_label(&self) -> Option<&str> {
        self.desired_dsp.as_deref()
    }

    /// Follow the player's committed episode: `Some(path)` after a
    /// committed replacement, `None` after a clean-failed one. `None`
    /// also drops the last observation — the model holds no episode
    /// truth at all then, so the retired episode's diagnostics must
    /// not leak into the frame as if they described anything current.
    pub fn set_episode(&mut self, source: Option<String>) {
        let episode_gone = source.is_none();
        self.source = source;
        if episode_gone {
            self.observation = PlaybackSessionObservation {
                terminal_outcome: None,
                failure_diagnostic: None,
                stop_requested: false,
                pause_requested: false,
                source_format: None,
                source_duration: None,
                position: None,
                pause_engagement: PauseEngagement::Disengaged,
                activation_error: None,
                last_processing_refusal: None,
            };
        }
    }

    /// The committed episode's source path, if one is live.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Record one operation's feedback line (composition feedback,
    /// never a playback semantic).
    pub fn set_status(&mut self, status: Option<String>) {
        self.status = status;
    }

    /// The navigation projection (D14.6): the 1-based cursor position
    /// and the playlist length, straight from the player's committed
    /// navigation state — presentation of application navigation state,
    /// never playback truth.
    pub fn navigation_position(&self) -> Option<(usize, usize)> {
        self.navigation_position
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn observation(&self) -> &PlaybackSessionObservation {
        &self.observation
    }

    /// The terminal-outcome label: `pending` while no terminal Fact is
    /// committed, otherwise exactly the committed D11 outcome. No
    /// fourth state exists here.
    pub fn terminal_label(&self) -> &'static str {
        match self.observation.terminal_outcome {
            None => "pending",
            Some(EpisodeTerminalOutcome::Completed) => "Completed",
            Some(EpisodeTerminalOutcome::Stopped) => "Stopped",
            Some(EpisodeTerminalOutcome::Failed) => "Failed",
        }
    }

    /// The Paused projection (D14.7), derived by the seam itself from
    /// the frozen establishment conjunction. Display only.
    pub fn paused(&self) -> bool {
        self.observation.paused()
    }

    /// The source-format label: the published PCM format once
    /// activation reported it (mechanism evidence), `pending` before.
    pub fn format_label(&self) -> String {
        match &self.observation.source_format {
            Some(format) => format!(
                "{} Hz, {} channels, mask {:#x}",
                format.sample_rate, format.channels, format.channel_mask
            ),
            None => "pending".to_owned(),
        }
    }

    /// The F4 timeline line (D14.8): `00:42 / 03:58`, with `--:--` for a
    /// side whose evidence does not exist yet (or is unknown). Rendered
    /// by the same projection helper the scriptable status text uses, so
    /// the two read-side surfaces cannot disagree; the model keeps no
    /// position of its own (no `last_position`, no local playback truth).
    ///
    /// One presentation policy on top (field round 5): while the episode
    /// is LIVE and its position evidence simply has not arrived yet —
    /// the first sampling window after every Open replacement — the
    /// timeline renders at the START (`00:00 / …`) instead of
    /// collapsing to `--:--`. The episode has consumed nothing (the
    /// render leg publishes from zero), and a collapsing timeline
    /// flexed the panel height on every track switch. A SETTLED
    /// episode keeps the dashes: a dead timeline has no start, and the
    /// no-evidence state must not grow a fabricated zero.
    pub fn timeline_label(&self) -> String {
        if self.pending_start_window() {
            let mut pending = self.observation.clone();
            pending.position = Some(0);
            return crate::status::format_timeline(&pending);
        }
        crate::status::format_timeline(&self.observation)
    }

    /// Whether this frame is in the live pre-first-sample window: an
    /// unsettled episode with no activation failure whose position
    /// projection has not published yet.
    fn pending_start_window(&self) -> bool {
        self.observation.position.is_none()
            && self.observation.terminal_outcome.is_none()
            && self.observation.activation_error.is_none()
    }

    /// The progress bar (Issue #166 §33):
    /// `00:42 ━━━━━╸────────── 05:47`. `None` unless BOTH sides have
    /// evidence: an unknown duration has no percentage to draw and an
    /// unknown position is not a zero, so the bar simply does not
    /// appear — it is never fabricated. Since G1 the drawn glyph is
    /// ALSO a click-to-position affordance: a seek-bar hit region over
    /// exactly its cells, and only while duration evidence exists (see
    /// the view's `seek_bar_glyph_area`).
    ///
    /// The one exception is the same live pre-first-sample window as
    /// [`Self::timeline_label`] (field round 5): a LIVE episode with no
    /// position sample yet renders the bar at its START (empty fill),
    /// with `--:--` as the total if the duration is not known either —
    /// the row stays put instead of vanishing and flexing the layout
    /// for the first second of every track. The fill is the position's
    /// fraction of the reported duration, clamped into the bar. The
    /// duration is mechanism evidence and the position an independent
    /// projection, so a position beyond the reported duration is
    /// representable; it clamps to a full bar rather than overflowing,
    /// which is the honest degradation of a display that cannot show
    /// "more than all of it".
    pub fn position_bar_label(&self) -> Option<String> {
        let rate = u64::from(self.observation.source_format?.sample_rate);
        if rate == 0 {
            return None;
        }
        let live_pending = self.pending_start_window();
        let position_frames = match self.observation.position {
            Some(frames) => frames,
            None if live_pending => 0,
            None => return None,
        };
        let position_secs = position_frames / rate;
        let duration_secs = match self.observation.source_duration {
            Some(duration) => Some(duration.as_secs()),
            None if live_pending => None,
            None => return None,
        };

        let filled = match duration_secs {
            Some(d) if d > 0 => {
                let width = BAR_WIDTH as u128;
                let filled = u128::from(position_secs) * width / u128::from(d);
                usize::try_from(filled.min(width)).unwrap_or(BAR_WIDTH)
            }
            _ => 0,
        };
        let mut bar = String::with_capacity(BAR_WIDTH);
        for cell in 0..BAR_WIDTH {
            bar.push(match cell.cmp(&filled) {
                std::cmp::Ordering::Less => '━',
                std::cmp::Ordering::Equal => '╸',
                std::cmp::Ordering::Greater => '─',
            });
        }
        let total = duration_secs
            .map(|d| crate::status::format_clock(Duration::from_secs(d)))
            .unwrap_or_else(|| "--:--".to_owned());
        Some(format!(
            "{} {bar} {}",
            crate::status::format_clock(Duration::from_secs(position_secs)),
            total
        ))
    }

    /// Diagnostics worth showing, in stable order. Both are
    /// presentation text supplied by the seam; their presence or
    /// wording is never part of the semantic outcome.
    pub fn diagnostics(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(error) = &self.observation.activation_error {
            lines.push(format!("activation: {error}"));
        }
        if let Some(failure) = &self.observation.failure_diagnostic {
            lines.push(format!("failure: {failure}"));
        }
        lines
    }

    /// Whether the episode's terminal Fact is already committed. The
    /// shell keeps rendering it (a committed outcome is not erased by
    /// the UI); S stays a no-op of the idempotent seam, Q still quits.
    pub fn terminal_committed(&self) -> bool {
        self.observation.terminal_outcome.is_some()
    }

    // ------------------------------------------------------------------
    // The T1B interaction state (route / focus / modal / hit regions).
    // All of it is presentation state; none of it is product authority.
    // ------------------------------------------------------------------

    /// The active route.
    pub fn route(&self) -> TuiRoute {
        self.route
    }

    /// The one active focus target, if any.
    pub fn focus(&self) -> Option<FocusId> {
        self.focus
    }

    /// The active modal, if any.
    pub fn modal(&self) -> Option<&Modal> {
        self.modal.as_ref()
    }

    /// The armed-click target between Left Down and Left Up, if any.
    #[cfg(test)]
    pub fn armed(&self) -> Option<HitTarget> {
        self.armed
    }

    /// The current responsive class (set by every draw).
    pub fn class(&self) -> ResponsiveClass {
        self.class
    }

    /// The current frame's hit regions.
    #[cfg(test)]
    pub fn regions(&self) -> &[HitRegion] {
        &self.regions
    }

    /// Hit-test one terminal cell against the current frame's regions
    /// (§13): only a rendered, currently valid control answers.
    pub fn hit_test(&self, column: u16, row: u16) -> Option<HitTarget> {
        let position = Position::new(column, row);
        self.regions
            .iter()
            .find(|region| region.area.contains(position))
            .map(|region| region.target)
    }

    /// Record the responsive class for this frame (§27) — the draw
    /// calls this before rendering, from the real terminal size.
    pub fn set_class(&mut self, class: ResponsiveClass) {
        self.class = class;
    }

    /// Publish the current frame's hit regions (§14). The view calls
    /// this once per draw, from the SAME layout decision it rendered
    /// from — there is no second geometry calculation to drift.
    pub fn publish_regions(&mut self, regions: Vec<HitRegion>) {
        self.regions = regions;
    }

    /// Presentation-only route change (§5): moves the active route,
    /// disarms any armed click (§17), and revalidates focus. It never
    /// touches the player, playback, DSP or Observation state.
    pub fn set_route(&mut self, route: TuiRoute) {
        if self.route == route {
            return;
        }
        self.route = route;
        self.invalidate_frame();
        self.validate_focus();
    }

    /// Invalidate the current frame's interactive geometry (§15/§29):
    /// drop the armed click and the hit regions. The next draw
    /// recomputes layout, revalidates focus and publishes fresh
    /// regions. Called on resize, route change and modal change.
    pub fn invalidate_frame(&mut self) {
        self.armed = None;
        self.regions.clear();
    }

    /// Drop the armed click (§17).
    pub fn disarm(&mut self) {
        self.armed = None;
    }

    /// The visible enabled focus targets in Tab order (§11): the four
    /// route tabs, then the active route's local controls. While a
    /// modal is open the cycle is exactly the modal's controls (§24) —
    /// the picker's field and listing; a listing with no rows leaves
    /// only the field. Below the minimum size there is nothing to
    /// focus (§28).
    pub fn focus_cycle(&self) -> Vec<FocusId> {
        if self.class == ResponsiveClass::Minimum {
            return Vec::new();
        }
        if let Some(modal) = self.modal() {
            return match modal {
                Modal::Open(picker) => {
                    let mut cycle = vec![FocusId::ModalField];
                    if !picker.entries.is_empty() {
                        cycle.push(FocusId::PickerList);
                    }
                    cycle.extend([
                        FocusId::PickerButton(ModalButton::Open),
                        FocusId::PickerButton(ModalButton::Add),
                        FocusId::PickerButton(ModalButton::Cancel),
                    ]);
                    cycle
                }
                _ => vec![FocusId::ModalField],
            };
        }
        let mut cycle: Vec<FocusId> = TuiRoute::ALL
            .iter()
            .map(|route| FocusId::RouteTab(*route))
            .collect();
        match self.route {
            TuiRoute::NowPlaying => {
                cycle.extend(TRANSPORT.iter().map(|button| FocusId::Transport(*button)));
                cycle.extend(
                    PREFERENCES
                        .iter()
                        .map(|button| FocusId::Preference(*button)),
                );
            }
            // The list is focusable only while it has rows: an empty
            // pane has no enabled control inside it (§11).
            TuiRoute::Playlist if !self.playlist.is_empty() => {
                cycle.push(FocusId::Playlist);
            }
            _ => {}
        }
        cycle
    }

    /// Tab / Shift+Tab: move focus to the next/previous visible enabled
    /// control, wrapping. Focus outside the cycle (or none) falls in at
    /// the cycle's edge.
    pub fn move_focus(&mut self, direction: FocusMove) {
        let cycle = self.focus_cycle();
        if cycle.is_empty() {
            self.focus = None;
            return;
        }
        let current = cycle.iter().position(|id| Some(*id) == self.focus);
        self.focus = Some(match current {
            Some(index) => match direction {
                FocusMove::Next => cycle[(index + 1) % cycle.len()],
                FocusMove::Previous => cycle[(index + cycle.len() - 1) % cycle.len()],
            },
            None => match direction {
                FocusMove::Next => cycle[0],
                FocusMove::Previous => cycle[cycle.len() - 1],
            },
        });
    }

    /// Validate focus against the visible enabled controls (§12): a
    /// focus that no longer exists falls back to the route's first
    /// meaningful local control, else the first tab. Invisible or
    /// off-screen focus is never retained.
    pub fn validate_focus(&mut self) {
        let cycle = self.focus_cycle();
        if cycle.iter().any(|id| Some(*id) == self.focus) {
            return;
        }
        let fallback = |id: &FocusId| {
            matches!(
                id,
                FocusId::Transport(_)
                    | FocusId::Preference(_)
                    | FocusId::Playlist
                    | FocusId::ModalField
                    | FocusId::PickerList
                    | FocusId::PickerButton(_)
            )
        };
        self.focus = cycle
            .iter()
            .find(|id| fallback(id))
            .or_else(|| cycle.first())
            .copied();
    }

    /// The focus target a mouse hit on `target` selects (§17: Left
    /// Down focuses the target). `None` for targets that carry no
    /// keyboard focus of their own (the seek bar — a mouse affordance
    /// over an already keyboard-complete command).
    pub fn focus_of_target(target: HitTarget) -> Option<FocusId> {
        match target {
            HitTarget::RouteTab(route) => Some(FocusId::RouteTab(route)),
            HitTarget::Transport(button) => Some(FocusId::Transport(button)),
            HitTarget::Preference(button) => Some(FocusId::Preference(button)),
            HitTarget::PlaylistRow(_) | HitTarget::PlaylistPane => Some(FocusId::Playlist),
            HitTarget::PickerRow(_) => Some(FocusId::PickerList),
            HitTarget::ModalButton(button) => Some(FocusId::PickerButton(button)),
            HitTarget::ModalField => Some(FocusId::ModalField),
            HitTarget::SeekBar => None,
        }
    }

    /// The action activating `target` performs (§17: Left Up activates
    /// the armed target). `None` for targets that arm nothing: the
    /// playlist pane area, the path-field row, and the seek bar (its
    /// action is the click position, computed from the frame geometry
    /// at decode time).
    pub fn action_of_target(target: HitTarget) -> Option<TuiAction> {
        match target {
            HitTarget::RouteTab(route) => Some(TuiAction::Navigate(route)),
            HitTarget::Transport(button) => Some(match button {
                TransportButton::Open => TuiAction::OpenModal(ModalKind::Open),
                TransportButton::Previous => TuiAction::Previous,
                TransportButton::PlayPause => TuiAction::PlayPause,
                TransportButton::Stop => TuiAction::Stop,
                TransportButton::Next => TuiAction::Next,
            }),
            HitTarget::Preference(button) => Some(match button {
                PreferenceButton::VolumeDown => TuiAction::VolumeDown,
                PreferenceButton::VolumeUp => TuiAction::VolumeUp,
                PreferenceButton::Order => TuiAction::ToggleOrder,
                PreferenceButton::Repeat => TuiAction::CycleRepeat,
            }),
            HitTarget::PlaylistRow(index) => {
                Some(TuiAction::PlaylistSelect(PlaylistCursor::Row(index)))
            }
            HitTarget::PickerRow(index) => Some(TuiAction::ModalInput(ModalInput::ListMove(
                PlaylistCursor::Row(index),
            ))),
            HitTarget::ModalButton(button) => Some(TuiAction::ModalInput(match button {
                ModalButton::Open => ModalInput::CommitOpen,
                ModalButton::Add => ModalInput::CommitAdd,
                ModalButton::Cancel => ModalInput::Cancel,
            })),
            // The pane area focuses the list but is not itself a
            // control.
            HitTarget::PlaylistPane => None,
            // The path-field row focuses the field but arms nothing.
            HitTarget::ModalField => None,
            // The seek bar arms like a control, but its action is the
            // click position — resolved by the mouse decoder, not by
            // the target alone.
            HitTarget::SeekBar => None,
        }
    }

    /// The action the currently focused control performs on Enter
    /// (§10). `None` when nothing is focused or the focus has no
    /// activation.
    pub fn activation(&self) -> Option<TuiAction> {
        match self.focus? {
            FocusId::RouteTab(route) => Some(TuiAction::Navigate(route)),
            FocusId::Transport(button) => Self::action_of_target(HitTarget::Transport(button)),
            FocusId::Preference(button) => Self::action_of_target(HitTarget::Preference(button)),
            FocusId::Playlist => Some(TuiAction::PlaylistPlaySelected),
            FocusId::PickerList => Some(TuiAction::ModalInput(ModalInput::ListActivate)),
            FocusId::PickerButton(button) => Self::action_of_target(HitTarget::ModalButton(button)),
            FocusId::ModalField => Some(TuiAction::ModalInput(ModalInput::Confirm)),
        }
    }

    /// Open the one modal of `kind` (§24): captures input (the modal
    /// field becomes the focus), clears the armed mouse target. Opening
    /// while one is open replaces it — there is no stack. The Open
    /// modal starts as a fresh picker draft; the runtime supplies its
    /// first listing right after (the model performs no I/O).
    pub fn open_modal(&mut self, kind: ModalKind) {
        self.modal = Some(match kind {
            ModalKind::Open => Modal::Open(OpenPicker {
                input: String::new(),
                dir: None,
                entries: Vec::new(),
                cursor: None,
                error: None,
            }),
            ModalKind::GoTo => Modal::GoTo {
                input: String::new(),
            },
            ModalKind::Help => Modal::Help,
        });
        self.invalidate_frame();
        self.validate_focus();
    }

    /// Close the modal and restore a valid route focus (§24). The
    /// pre-modal focus was the modal's own field, so the restore is the
    /// §12 fallback: the route's first meaningful local control, else
    /// the first tab.
    pub fn close_modal(&mut self) {
        self.modal = None;
        self.invalidate_frame();
        self.validate_focus();
    }

    /// Apply one editing step to the active modal (a no-op when no
    /// text modal is open — Help has no field to edit).
    pub fn modal_edit(&mut self, input: ModalInput) {
        let Some(modal) = self.modal.as_mut() else {
            return;
        };
        let text = match modal {
            Modal::Open(picker) => &mut picker.input,
            Modal::GoTo { input } => input,
            Modal::Help => return,
        };
        match input {
            ModalInput::Char(c) => text.push(c),
            ModalInput::Backspace => {
                text.pop();
            }
            ModalInput::Confirm
            | ModalInput::Cancel
            | ModalInput::ListActivate
            | ModalInput::ListMove(_)
            | ModalInput::ListParent
            | ModalInput::CommitOpen
            | ModalInput::CommitAdd => {}
        }
    }

    /// Replace the Open picker's listing with one runtime-supplied
    /// directory read (the model performs no I/O of its own). A
    /// readable directory becomes `..` (when one exists) plus the
    /// classified entries; an unreadable one keeps an honest
    /// diagnostic instead of a fabricated empty list. Either way the
    /// cursor starts unselected and the armed click dies with the old
    /// geometry (§17: a re-listed pane is new geometry).
    pub fn set_open_listing(
        &mut self,
        dir: std::path::PathBuf,
        listing: Result<Vec<crate::input::DirectoryEntry>, String>,
    ) {
        let Some(Modal::Open(picker)) = self.modal.as_mut() else {
            return;
        };
        picker.dir = Some(dir);
        picker.error = None;
        picker.cursor = None;
        picker.entries = match listing {
            Ok(entries) => {
                let mut rows: Vec<PickerEntry> = Vec::with_capacity(entries.len() + 1);
                if picker
                    .dir
                    .as_ref()
                    .is_some_and(|dir| dir.parent().is_some())
                {
                    rows.push(PickerEntry {
                        name: "..".to_owned(),
                        is_dir: true,
                        is_parent: true,
                    });
                }
                rows.extend(entries.into_iter().map(|entry| PickerEntry {
                    name: entry.name,
                    is_dir: entry.is_dir,
                    is_parent: false,
                }));
                rows
            }
            Err(diagnostic) => {
                picker.error = Some(diagnostic);
                Vec::new()
            }
        };
        self.invalidate_frame();
        self.validate_focus();
    }

    /// The Open picker's listed directory, while one is shown.
    pub fn open_picker_dir(&self) -> Option<&std::path::Path> {
        match self.modal.as_ref() {
            Some(Modal::Open(picker)) => picker.dir.as_deref(),
            _ => None,
        }
    }

    /// Move the Open picker's listing cursor (§24 presentation). The
    /// first move from the unselected state enters at the list's edge
    /// in the pressed direction; a row click selects that row.
    pub fn move_picker_cursor(&mut self, cursor: PlaylistCursor) {
        let Some(Modal::Open(picker)) = self.modal.as_mut() else {
            return;
        };
        if picker.entries.is_empty() {
            return;
        }
        picker.cursor = Some(match (picker.cursor, cursor) {
            (Some(current), PlaylistCursor::Previous) => current.saturating_sub(1),
            (Some(current), PlaylistCursor::Next) => (current + 1).min(picker.entries.len() - 1),
            (None, PlaylistCursor::Previous) => picker.entries.len() - 1,
            (None, PlaylistCursor::Next) | (None, PlaylistCursor::Row(0)) => 0,
            (None, PlaylistCursor::Row(index)) => index.min(picker.entries.len() - 1),
            (Some(_), PlaylistCursor::Row(index)) => index.min(picker.entries.len() - 1),
        });
        self.focus = Some(FocusId::PickerList);
    }

    /// The full path of the picker listing's current cursor entry, if
    /// one is selected, plus whether descending into it stays a
    /// directory step. The `..` row resolves to the PARENT directory —
    /// activating it ascends (G1 §8 parent navigation).
    pub fn picker_cursor_entry(&self) -> Option<(std::path::PathBuf, bool)> {
        let Some(Modal::Open(picker)) = self.modal.as_ref() else {
            return None;
        };
        let dir = picker.dir.as_ref()?;
        let index = picker.cursor?;
        let entry = picker.entries.get(index)?;
        let path = if entry.is_parent {
            dir.parent()?.to_path_buf()
        } else {
            dir.join(&entry.name)
        };
        Some((path, entry.is_parent || entry.is_dir))
    }

    /// The Open picker's commit SUBJECT (G1 §8): the selected listing
    /// entry when one is selected, otherwise the typed path line.
    /// `None` = nothing to commit (no selection, an empty line).
    pub fn picker_commit_subject(&self) -> Option<(std::path::PathBuf, bool)> {
        if let Some(entry) = self.picker_cursor_entry() {
            return Some(entry);
        }
        let Some(Modal::Open(picker)) = self.modal.as_ref() else {
            return None;
        };
        if picker.input.is_empty() {
            return None;
        }
        // The typed subject's kind is decided by the runtime's one
        // filesystem question at commit time, not guessed here.
        Some((std::path::PathBuf::from(&picker.input), false))
    }

    /// Confirm the active modal (§26): decides what the confirmation
    /// means and closes the modal — EXCEPT an unreadable GoTo token,
    /// which keeps the line open for correction and sends nothing.
    pub fn confirm_modal(&mut self) -> ModalConfirm {
        match self.modal.as_mut() {
            Some(Modal::Help) => {
                self.close_modal();
                ModalConfirm::Nothing
            }
            Some(Modal::Open(picker)) => {
                let line = std::mem::take(&mut picker.input);
                self.close_modal();
                if line.is_empty() {
                    // An empty line is a cancel, never an Open of "".
                    ModalConfirm::Nothing
                } else {
                    // The frozen U1 field semantic: the typed line —
                    // file or folder — is an Open subject.
                    ModalConfirm::Open(line)
                }
            }
            Some(Modal::GoTo { input }) => {
                let line = std::mem::take(input);
                if line.is_empty() {
                    self.close_modal();
                    return ModalConfirm::Nothing;
                }
                match crate::cli::parse_seek_time(&line) {
                    Some(target) => {
                        self.close_modal();
                        ModalConfirm::Seek(target)
                    }
                    None => {
                        // The line stays OPEN for correction.
                        self.modal = Some(Modal::GoTo { input: line });
                        ModalConfirm::Unreadable("cannot read that time (try 95, 1:35 or 01:35.5)")
                    }
                }
            }
            None => ModalConfirm::Nothing,
        }
    }

    /// Type one character into the active text modal. Typing always
    /// edits the picker's path line, from wherever inside the modal the
    /// focus currently sits (type-through), and returns the focus to
    /// the field.
    pub fn modal_push(&mut self, c: char) {
        self.modal_edit(ModalInput::Char(c));
        if self.modal.is_some() {
            self.focus = Some(FocusId::ModalField);
        }
    }

    /// Backspace one character out of the active text modal.
    pub fn modal_backspace(&mut self) {
        self.modal_edit(ModalInput::Backspace);
    }

    /// The active modal's text content, while editing.
    #[cfg(test)]
    pub fn modal_line(&self) -> Option<&str> {
        self.modal.as_ref().and_then(Modal::input)
    }

    /// Test seam: place the keyboard focus directly. The real input
    /// paths set it through move_focus, validate_focus and the mouse
    /// decoder; some routing tests need a specific starting target.
    #[cfg(test)]
    pub(crate) fn set_focus(&mut self, focus: Option<FocusId>) {
        self.focus = focus;
    }
}

/// Whether a key press carries no modifier, or only SHIFT (terminals
/// disagree about reporting SHIFT with a character, so both act for
/// every LETTER key).
fn plain(key: KeyEvent) -> bool {
    key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT
}

/// Decode one key event into AT MOST ONE [`TuiAction`] (§9/§31).
///
/// Priority: modal input > focused control (Enter) and its contextual
/// arrows > global accelerators. Release events are presentation noise.
///
/// The global accelerator table is the SHRUNK v2 table: the playlist
/// arrows became contextual to the list focus, and Enter became the
/// focused-control activation — the transport keys, seek keys, policy
/// keys, Open/GoTo/Help and quit keep their existing meanings.
pub fn decode_key(key: KeyEvent, model: &TuiModel) -> Option<TuiAction> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    // The conventional quit is the ONE key that works everywhere,
    // including inside a modal (that is why it is not part of any
    // modal's vocabulary).
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(TuiAction::Quit);
    }
    if model.modal().is_some() {
        return decode_modal_key(key, model);
    }
    match key.code {
        KeyCode::Tab => Some(TuiAction::MoveFocus(FocusMove::Next)),
        KeyCode::BackTab => Some(TuiAction::MoveFocus(FocusMove::Previous)),
        KeyCode::Enter if plain(key) => Some(TuiAction::ActivateFocused),
        // Arrow behavior is contextual to the focused group (§10): the
        // playlist list owns Up/Down, and they never move focus. Left/
        // Right keep their global seek meaning — the two axes never
        // share a key.
        KeyCode::Up if plain(key) && model.focus() == Some(FocusId::Playlist) => {
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Previous))
        }
        KeyCode::Down if plain(key) && model.focus() == Some(FocusId::Playlist) => {
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Next))
        }
        KeyCode::Left if key.modifiers.is_empty() => Some(TuiAction::SeekRelative(-SEEK_STEP_SECS)),
        KeyCode::Right if key.modifiers.is_empty() => Some(TuiAction::SeekRelative(SEEK_STEP_SECS)),
        KeyCode::Left if key.modifiers == KeyModifiers::SHIFT => {
            Some(TuiAction::SeekRelative(-LARGE_SEEK_STEP_SECS))
        }
        KeyCode::Right if key.modifiers == KeyModifiers::SHIFT => {
            Some(TuiAction::SeekRelative(LARGE_SEEK_STEP_SECS))
        }
        KeyCode::Char(' ') if plain(key) => Some(TuiAction::PlayPause),
        KeyCode::Char('s') | KeyCode::Char('S') if plain(key) => Some(TuiAction::Stop),
        KeyCode::Char('o') | KeyCode::Char('O') if plain(key) => {
            Some(TuiAction::OpenModal(ModalKind::Open))
        }
        KeyCode::Char('g') | KeyCode::Char('G') if plain(key) => {
            Some(TuiAction::OpenModal(ModalKind::GoTo))
        }
        KeyCode::Char('?') if plain(key) => Some(TuiAction::OpenModal(ModalKind::Help)),
        KeyCode::Char('n') | KeyCode::Char('N') if plain(key) => Some(TuiAction::Next),
        KeyCode::Char('p') | KeyCode::Char('P') if plain(key) => Some(TuiAction::Previous),
        KeyCode::Char('r') | KeyCode::Char('R') if plain(key) => Some(TuiAction::ToggleOrder),
        KeyCode::Char('l') | KeyCode::Char('L') if plain(key) => Some(TuiAction::CycleRepeat),
        KeyCode::Char('+') | KeyCode::Char('=') if plain(key) => Some(TuiAction::VolumeUp),
        KeyCode::Char('-') | KeyCode::Char('_') if plain(key) => Some(TuiAction::VolumeDown),
        KeyCode::Char('q') | KeyCode::Char('Q') if plain(key) => Some(TuiAction::Quit),
        _ => None,
    }
}

/// The modal owns the keyboard while it is open (§24). No key both
/// edits and executes: inside a text modal every plain character is a
/// literal character — `q`/`Q` included, so `Q:\Music` stays typeable
/// (Issue #166 §29), and the same rule makes the picker's path line an
/// ordinary text field, so it has NO letter accelerators — and the
/// help overlay lets only its close keys through, so no playback key
/// can fire behind it.
///
/// The Open picker's grammar (G1 §8): Tab cycles field → list →
/// commit buttons; ↑/↓ move the listing cursor; Enter acts on the
/// focused picker control (the field confirms its typed subject, a
/// listing row descends into a directory or opens a file, a commit
/// button commits); Backspace on the listing steps up to the parent
/// directory; Esc cancels.
fn decode_modal_key(key: KeyEvent, model: &TuiModel) -> Option<TuiAction> {
    let modal_kind = model.modal().map(Modal::kind)?;
    // Below the minimum size the shell renders no interactive layout
    // and paints no popup (§28): the modal's keys go inert except the
    // cancel — a blind Enter behind an invisible popup must never
    // commit a real Open. Ctrl+C keeps its conventional meaning (it is
    // decoded before this function).
    if model.class() == ResponsiveClass::Minimum {
        return match key.code {
            KeyCode::Esc => Some(TuiAction::ModalInput(ModalInput::Cancel)),
            _ => None,
        };
    }
    match modal_kind {
        ModalKind::Help => match key.code {
            KeyCode::Esc => Some(TuiAction::ModalInput(ModalInput::Cancel)),
            KeyCode::Char('?') if plain(key) => Some(TuiAction::ModalInput(ModalInput::Cancel)),
            KeyCode::Char('q') | KeyCode::Char('Q') if plain(key) => Some(TuiAction::Quit),
            _ => None,
        },
        ModalKind::GoTo => match key.code {
            KeyCode::Esc => Some(TuiAction::ModalInput(ModalInput::Cancel)),
            KeyCode::Enter if plain(key) => Some(TuiAction::ModalInput(ModalInput::Confirm)),
            KeyCode::Backspace if plain(key) => Some(TuiAction::ModalInput(ModalInput::Backspace)),
            KeyCode::Char(c) if plain(key) => Some(TuiAction::ModalInput(ModalInput::Char(c))),
            _ => None,
        },
        ModalKind::Open => match key.code {
            KeyCode::Esc => Some(TuiAction::ModalInput(ModalInput::Cancel)),
            KeyCode::Tab => Some(TuiAction::MoveFocus(FocusMove::Next)),
            KeyCode::BackTab => Some(TuiAction::MoveFocus(FocusMove::Previous)),
            KeyCode::Enter if plain(key) => Some(TuiAction::ModalInput(match model.focus() {
                Some(FocusId::PickerList) => ModalInput::ListActivate,
                Some(FocusId::PickerButton(ModalButton::Open)) => ModalInput::CommitOpen,
                Some(FocusId::PickerButton(ModalButton::Add)) => ModalInput::CommitAdd,
                Some(FocusId::PickerButton(ModalButton::Cancel)) => ModalInput::Cancel,
                _ => ModalInput::Confirm,
            })),
            KeyCode::Up if plain(key) => Some(TuiAction::ModalInput(ModalInput::ListMove(
                PlaylistCursor::Previous,
            ))),
            KeyCode::Down if plain(key) => Some(TuiAction::ModalInput(ModalInput::ListMove(
                PlaylistCursor::Next,
            ))),
            KeyCode::Backspace if plain(key) => {
                match model.focus() {
                    // On the listing, Backspace is the parent step; on
                    // the field it edits the line.
                    Some(FocusId::PickerList) => {
                        Some(TuiAction::ModalInput(ModalInput::ListParent))
                    }
                    _ => Some(TuiAction::ModalInput(ModalInput::Backspace)),
                }
            }
            KeyCode::Char(c) if plain(key) => Some(TuiAction::ModalInput(ModalInput::Char(c))),
            _ => None,
        },
    }
}

/// Decode one mouse event into AT MOST ONE [`TuiAction`] (§16–§25).
///
/// The frozen armed-click rule: Left Down identifies the target, focuses
/// it and arms it; Left Up activates ONLY the same valid target. Drag,
/// a moved pointer, stale geometry, a route/modal change or a resize
/// all cancel; an unmatched Up is no action. Right/middle clicks and
/// double clicks carry no product meaning (§19/§20); plain movement is
/// inert (§18); the wheel scrolls only where a control already has
/// clear meaning (§22). While a modal is open the background is inert
/// (§25).
///
/// Decoding mutates exactly the presentation state a physical event
/// owns — the focus and the armed click — and never a product seam.
pub fn decode_mouse(mouse: MouseEvent, model: &mut TuiModel) -> Option<TuiAction> {
    // §24/§25: while a modal is open only the modal's own controls
    // answer; the background (inside or outside the popup) dispatches
    // nothing.
    if model.modal().is_some() {
        return decode_modal_mouse(mouse, model);
    }
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => match model.hit_test(mouse.column, mouse.row) {
            Some(target) => {
                // A seek-bar hit carries no keyboard focus (the bar is
                // a mouse affordance over an already keyboard-complete
                // command); every other control focuses as before.
                if let Some(focus) = TuiModel::focus_of_target(target) {
                    model.focus = Some(focus);
                }
                // Only a control arms; the pane area focuses the list
                // and nothing else, and the seek bar arms for its
                // geometry-resolved action below.
                model.armed = if target == HitTarget::SeekBar {
                    Some(target)
                } else {
                    TuiModel::action_of_target(target).map(|_| target)
                };
                None
            }
            None => {
                model.disarm();
                None
            }
        },
        MouseEventKind::Up(MouseButton::Left) => {
            // An unmatched Up is no action.
            let armed = model.armed.take()?;
            // Activate only if the SAME valid target still sits under
            // the pointer: stale geometry, a moved pointer, or a
            // re-render/revision that moved or replaced the control
            // all cancel (§17).
            if model.hit_test(mouse.column, mouse.row) == Some(armed) {
                match armed {
                    HitTarget::SeekBar => {
                        Some(TuiAction::SeekPerMille(seek_bar_per_mille(model, &mouse)))
                    }
                    _ => TuiModel::action_of_target(armed),
                }
            } else {
                None
            }
        }
        // Drag cancels the armed click (§21); plain movement is inert.
        MouseEventKind::Drag(_) => {
            model.disarm();
            None
        }
        MouseEventKind::Moved => None,
        // §22: the wheel scrolls where a control already has a clear
        // meaning — the playlist list — and is inert everywhere else.
        MouseEventKind::ScrollUp => wheel(model, &mouse, PlaylistCursor::Previous),
        MouseEventKind::ScrollDown => wheel(model, &mouse, PlaylistCursor::Next),
        // Horizontal wheels have no meaning here (§22: no scroll physics).
        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => None,
        // Right/middle buttons carry no product meaning (§19).
        MouseEventKind::Down(_) | MouseEventKind::Up(_) => None,
    }
}

/// The per-mille a click on the seek bar's area requests (G1 §9): the
/// clicked cell's center as a fraction of the bar, clamped into
/// `1..=1000`. The bar region is this frame's own geometry — the same
/// region the hit test just answered — so the fraction and the hit can
/// never disagree.
fn seek_bar_per_mille(model: &TuiModel, mouse: &MouseEvent) -> u16 {
    let Some(region) = model
        .regions
        .iter()
        .find(|region| region.target == HitTarget::SeekBar)
    else {
        return 0;
    };
    let offset = u32::from(mouse.column.saturating_sub(region.area.x));
    let width = u32::from(region.area.width.max(1));
    (((offset * 2 + 1) * 500) / width).min(1000) as u16
}

/// Whether a hit target belongs to the active modal's own surface
/// (§25): everything else is background and stays inert.
fn modal_target(target: HitTarget) -> bool {
    matches!(
        target,
        HitTarget::PickerRow(_) | HitTarget::ModalButton(_) | HitTarget::ModalField
    )
}

/// The armed-click discipline (§16–§22) applied to the Open picker's
/// own controls: rows and buttons arm and activate like every other
/// control, the wheel steps the listing where it has rows, and a
/// click anywhere else — including the background visible around the
/// popup — disarms and dispatches nothing.
fn decode_modal_mouse(mouse: MouseEvent, model: &mut TuiModel) -> Option<TuiAction> {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => match model.hit_test(mouse.column, mouse.row) {
            Some(target) if modal_target(target) => {
                model.focus = TuiModel::focus_of_target(target);
                model.armed = TuiModel::action_of_target(target).map(|_| target);
                None
            }
            _ => {
                model.disarm();
                None
            }
        },
        MouseEventKind::Up(MouseButton::Left) => {
            let armed = model.armed.take()?;
            if modal_target(armed) && model.hit_test(mouse.column, mouse.row) == Some(armed) {
                TuiModel::action_of_target(armed)
            } else {
                None
            }
        }
        MouseEventKind::Drag(_) => {
            model.disarm();
            None
        }
        MouseEventKind::ScrollUp => modal_wheel(model, &mouse, PlaylistCursor::Previous),
        MouseEventKind::ScrollDown => modal_wheel(model, &mouse, PlaylistCursor::Next),
        _ => None,
    }
}

/// The wheel action over one cell while the picker is open: a listing
/// scroll over the listing rows, otherwise nothing.
fn modal_wheel(model: &TuiModel, mouse: &MouseEvent, cursor: PlaylistCursor) -> Option<TuiAction> {
    match model.hit_test(mouse.column, mouse.row)? {
        HitTarget::PickerRow(_) => Some(TuiAction::ModalInput(ModalInput::ListMove(cursor))),
        _ => None,
    }
}

/// The wheel action over one cell: a list scroll when the cell belongs
/// to the playlist list, otherwise nothing.
fn wheel(model: &TuiModel, mouse: &MouseEvent, cursor: PlaylistCursor) -> Option<TuiAction> {
    match model.hit_test(mouse.column, mouse.row)? {
        HitTarget::PlaylistRow(_) | HitTarget::PlaylistPane => {
            Some(TuiAction::PlaylistSelect(cursor))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qianqian_audio_api::ports::PcmFormat;
    use std::time::Duration;

    fn pending() -> PlaybackSessionObservation {
        PlaybackSessionObservation {
            terminal_outcome: None,
            failure_diagnostic: None,
            stop_requested: false,
            pause_requested: false,
            source_format: None,
            source_duration: None,
            position: None,
            pause_engagement: PauseEngagement::Disengaged,
            activation_error: None,
            last_processing_refusal: None,
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// A model with the current frame's regions drawn at a fixed size
    /// on the given route, so the mouse decoders hit-test against real
    /// published geometry.
    fn model_with_regions(width: u16, height: u16, route: TuiRoute) -> TuiModel {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        model.set_route(route);
        model.set_playlist(1, || {
            (0..40)
                .map(|n| PlaylistRow {
                    label: format!("track-{n:02}.flac"),
                    playing: n == 0,
                    selected: n == 0,
                })
                .collect()
        });
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
                .expect("virtual terminal");
        terminal
            .draw(|frame| super::super::view::draw(frame, &mut model))
            .expect("draw");
        model
    }

    // ------------------------------------------------------------------
    // Model label tests (the read-side projections, unchanged classes).
    // ------------------------------------------------------------------

    #[test]
    fn the_terminal_label_names_only_pending_and_the_three_facts() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(model.terminal_label(), "pending");
        assert!(!model.terminal_committed());
        for (outcome, label) in [
            (EpisodeTerminalOutcome::Completed, "Completed"),
            (EpisodeTerminalOutcome::Stopped, "Stopped"),
            (EpisodeTerminalOutcome::Failed, "Failed"),
        ] {
            model.update(PlaybackSessionObservation {
                terminal_outcome: Some(outcome),
                ..pending()
            });
            assert_eq!(model.terminal_label(), label);
            assert!(model.terminal_committed());
        }
    }

    #[test]
    fn the_format_stays_pending_until_activation_publishes_one() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(model.format_label(), "pending");
        model.update(PlaybackSessionObservation {
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            ..pending()
        });
        assert_eq!(model.format_label(), "44100 Hz, 2 channels, mask 0x3");
    }

    /// The timeline line is the D14.8 projection rendered by the shared
    /// read-side helper, and the model keeps no position of its own: the
    /// label is exactly what the current observation says, including
    /// `--:--` for a side whose evidence is absent.
    #[test]
    fn the_timeline_label_follows_the_observation_only() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(
            model.timeline_label(),
            "--:-- / --:--",
            "no evidence yet, and never a fabricated zero"
        );

        model.update(PlaybackSessionObservation {
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            position: Some(44_100 * 42),
            ..pending()
        });
        assert_eq!(
            model.timeline_label(),
            "00:42 / --:--",
            "an unknown duration must not hide a known position"
        );

        model.update(PlaybackSessionObservation {
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            position: Some(44_100 * 42),
            source_duration: Some(Duration::from_secs(238)),
            ..pending()
        });
        assert_eq!(model.timeline_label(), "00:42 / 03:58");

        // A settled episode (the seam withdraws the position) keeps the
        // duration evidence and shows no position — no final-position
        // latch is invented here either.
        model.update(PlaybackSessionObservation {
            terminal_outcome: Some(EpisodeTerminalOutcome::Completed),
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            source_duration: Some(Duration::from_secs(238)),
            ..pending()
        });
        assert_eq!(model.timeline_label(), "--:-- / 03:58");
    }

    #[test]
    fn diagnostics_stay_ordered_and_absent_without_content() {
        let mut model = TuiModel::new("song.flac");
        assert!(model.diagnostics().is_empty());
        model.update(PlaybackSessionObservation {
            activation_error: Some("no device".to_owned()),
            failure_diagnostic: Some("corrupt frame".to_owned()),
            ..pending()
        });
        assert_eq!(
            model.diagnostics(),
            vec![
                "activation: no device".to_owned(),
                "failure: corrupt frame".to_owned(),
            ]
        );
    }

    /// The seek target is a fixed step around the observed position,
    /// converted with the SAME observation's published rate, saturating
    /// at zero on the backward side — and it is `None` (no command at
    /// all) whenever either side of the evidence is missing: no
    /// fabricated zero, no seek to the episode start.
    #[test]
    fn the_seek_target_is_a_fixed_step_of_the_coherent_observation_or_inert() {
        let observation_with =
            |position: Option<u64>, sample_rate: Option<u32>| PlaybackSessionObservation {
                position,
                source_format: sample_rate.map(|sample_rate| PcmFormat {
                    sample_rate,
                    channels: 2,
                    channel_mask: 0x3,
                }),
                ..pending()
            };
        // Unknown position: inert in both directions.
        let no_position = observation_with(None, Some(44_100));
        assert_eq!(
            seek_target(&no_position, Duration::from_secs(5), true),
            None
        );
        assert_eq!(
            seek_target(&no_position, Duration::from_secs(5), false),
            None
        );
        // No published rate: no unit to convert with, inert.
        let no_format = observation_with(Some(100), None);
        assert_eq!(seek_target(&no_format, Duration::from_secs(5), true), None);

        // 42 s at 44.1 kHz: the step is exactly five seconds of media
        // time, and the backward step saturates at zero (Duration is
        // non-negative by type).
        let at_42s = observation_with(Some(44_100 * 42), Some(44_100));
        assert_eq!(
            seek_target(&at_42s, Duration::from_secs(5), true),
            Some(Duration::from_secs(47))
        );
        assert_eq!(
            seek_target(&at_42s, Duration::from_secs(5), false),
            Some(Duration::from_secs(37))
        );
        let at_2s = observation_with(Some(2 * 44_100), Some(44_100));
        assert_eq!(
            seek_target(&at_2s, Duration::from_secs(5), false),
            Some(Duration::from_secs(0)),
            "before zero the step saturates at the episode start"
        );

        // The conversion uses the observation's own rate: 42 s at
        // 48 kHz is the same media time from different frames.
        let at_48k = observation_with(Some(48_000 * 42), Some(48_000));
        assert_eq!(
            seek_target(&at_48k, Duration::from_secs(5), true),
            Some(Duration::from_secs(47))
        );
    }

    #[test]
    fn row_labels_use_the_file_name_and_keep_unicode() {
        for (path, expected) in [
            ("/media/01 Intro.flac", "01 Intro.flac"),
            ("/media/夜曲 七里香.flac", "夜曲 七里香.flac"),
            ("/media/a/b/c.mp3", "c.mp3"),
            ("/", "/"),
            ("..", ".."),
        ] {
            assert_eq!(row_label(Path::new(path)), expected, "{path}");
        }
    }

    /// The playlist rows are revision-gated: a revision the model
    /// already holds does not even build them, which is what keeps a
    /// huge playlist off the per-frame path (Issue #166 §21).
    #[test]
    fn the_playlist_rows_rebuild_only_when_the_revision_moves() {
        let mut model = TuiModel::new("song.flac");
        assert!(model.playlist().is_empty());

        model.set_playlist(7, || {
            vec![PlaylistRow {
                label: "first.flac".to_owned(),
                playing: true,
                selected: true,
            }]
        });
        assert_eq!(model.playlist().len(), 1);

        // The same revision: the closure must not even run.
        model.set_playlist(7, || {
            panic!("an unchanged revision must not rebuild the rows")
        });

        model.set_playlist(8, Vec::new);
        assert!(model.playlist().is_empty());
    }

    /// The order/repeat labels come from the App's own policy state and
    /// are absent until the shell has been told them.
    #[test]
    fn the_order_and_repeat_labels_follow_the_app_state() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(model.order_label(), None);
        assert_eq!(model.repeat_label(), None);
        model.set_order(PlaybackOrder::Shuffle);
        model.set_repeat(RepeatMode::One);
        assert_eq!(model.order_label(), Some("Shuffle"));
        assert_eq!(model.repeat_label(), Some("One"));
    }

    /// The status line is plain presentation: recorded, read, replaced.
    #[test]
    fn the_status_line_records_operation_feedback() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(model.status(), None);
        model.set_status(Some("open refused: unsupported container".to_owned()));
        assert_eq!(model.status(), Some("open refused: unsupported container"));
        model.set_status(None);
        assert_eq!(model.status(), None);
    }

    /// The model follows the player's committed episode; a no-episode
    /// model drops the retired episode's diagnostics with it.
    #[test]
    fn the_model_follows_the_player_committed_episode() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(model.source(), Some("song.flac"));
        model.set_episode(Some("/media/b.flac".to_owned()));
        assert_eq!(model.source(), Some("/media/b.flac"));
        model.update(PlaybackSessionObservation {
            failure_diagnostic: Some("decode: corrupt frame".to_owned()),
            ..pending()
        });
        model.set_episode(None);
        assert_eq!(model.source(), None, "no episode is a real F6 state");
        assert!(
            model.diagnostics().is_empty(),
            "no episode, no episode diagnostics"
        );
    }

    /// The displayed Paused projection comes from the seam's frozen
    /// establishment conjunction, never from command state alone.
    #[test]
    fn the_paused_label_follows_the_establishment_conjunction_only() {
        let mut model = TuiModel::new("song.flac");
        assert!(!model.paused(), "fresh episode is not Paused");
        model.update(PlaybackSessionObservation {
            pause_requested: true,
            ..pending()
        });
        assert!(!model.paused(), "intent alone is not Paused");
        model.update(PlaybackSessionObservation {
            pause_requested: true,
            pause_engagement: PauseEngagement::Engaged,
            ..pending()
        });
        assert!(
            !model.paused(),
            "engagement without quiescence is not Paused"
        );
        model.update(PlaybackSessionObservation {
            pause_requested: true,
            pause_engagement: PauseEngagement::TailQuiesced,
            ..pending()
        });
        assert!(model.paused(), "intent + engagement + quiescence is Paused");
        model.update(PlaybackSessionObservation {
            terminal_outcome: Some(EpisodeTerminalOutcome::Stopped),
            pause_requested: true,
            pause_engagement: PauseEngagement::TailQuiesced,
            ..pending()
        });
        assert!(!model.paused(), "a settled episode is never Paused");
    }

    // ------------------------------------------------------------------
    // Route / focus / modal model tests.
    // ------------------------------------------------------------------

    /// The default route is Now Playing, and a route change moves only
    /// the route, the focus and the armed click — never any product
    /// state this model holds.
    #[test]
    fn route_change_moves_only_presentation_state() {
        let mut model = TuiModel::new("song.flac");
        model.set_order(PlaybackOrder::Shuffle);
        model.set_status(Some("feedback".to_owned()));
        model.set_playlist(3, || {
            vec![PlaylistRow {
                label: "a.flac".to_owned(),
                playing: true,
                selected: true,
            }]
        });
        let before = (
            model.order_label(),
            model.status().map(str::to_owned),
            model.playlist().len(),
        );

        assert_eq!(model.route(), TuiRoute::NowPlaying);
        model.set_route(TuiRoute::Playlist);
        assert_eq!(model.route(), TuiRoute::Playlist);

        assert_eq!(
            (
                model.order_label(),
                model.status().map(str::to_owned),
                model.playlist().len()
            ),
            before,
            "a route change is presentation-only"
        );
    }

    /// The focus cycle: tabs first, then the route's local controls; the
    /// modal collapses the cycle to its own field; below the minimum
    /// there is nothing to focus.
    #[test]
    fn the_focus_cycle_lists_tabs_then_route_local_controls() {
        let mut model = TuiModel::new("song.flac");
        model.set_class(ResponsiveClass::Normal);
        model.set_playlist(1, || {
            vec![PlaylistRow {
                label: "a.flac".to_owned(),
                playing: false,
                selected: false,
            }]
        });

        // Now Playing: four tabs, then the five transport buttons
        // (Open included), then the preference row.
        model.set_route(TuiRoute::NowPlaying);
        assert_eq!(
            model.focus_cycle(),
            vec![
                FocusId::RouteTab(TuiRoute::NowPlaying),
                FocusId::RouteTab(TuiRoute::Playlist),
                FocusId::RouteTab(TuiRoute::Audio),
                FocusId::RouteTab(TuiRoute::Visualizer),
                FocusId::Transport(TransportButton::Open),
                FocusId::Transport(TransportButton::Previous),
                FocusId::Transport(TransportButton::PlayPause),
                FocusId::Transport(TransportButton::Stop),
                FocusId::Transport(TransportButton::Next),
                FocusId::Preference(PreferenceButton::VolumeDown),
                FocusId::Preference(PreferenceButton::VolumeUp),
                FocusId::Preference(PreferenceButton::Order),
                FocusId::Preference(PreferenceButton::Repeat),
            ]
        );

        // Playlist: the list (it has rows).
        model.set_route(TuiRoute::Playlist);
        assert_eq!(
            model.focus_cycle(),
            vec![
                FocusId::RouteTab(TuiRoute::NowPlaying),
                FocusId::RouteTab(TuiRoute::Playlist),
                FocusId::RouteTab(TuiRoute::Audio),
                FocusId::RouteTab(TuiRoute::Visualizer),
                FocusId::Playlist,
            ]
        );

        // Audio/Visualizer: tabs only (placeholder routes, §34/§33).
        model.set_route(TuiRoute::Audio);
        assert_eq!(model.focus_cycle().len(), 4);

        // A modal collapses the cycle to its field (§24).
        model.open_modal(ModalKind::GoTo);
        assert_eq!(model.focus_cycle(), vec![FocusId::ModalField]);
        model.close_modal();

        // Below the minimum: nothing is focusable (§28).
        model.set_class(ResponsiveClass::Minimum);
        assert!(model.focus_cycle().is_empty());
        model.validate_focus();
        assert_eq!(model.focus(), None, "no invisible focus below minimum");
    }

    /// Tab/Shift+Tab walk the visible enabled controls with wraparound,
    /// and an empty list drops the list out of the cycle (§11).
    #[test]
    fn tab_moves_through_visible_enabled_controls_only() {
        let mut model = TuiModel::new("song.flac");
        model.set_class(ResponsiveClass::Normal);
        // An EMPTY playlist: the list is not focusable.
        model.set_route(TuiRoute::Playlist);
        model.set_playlist(1, Vec::new);
        model.validate_focus();

        model.validate_focus();
        assert_eq!(
            model.focus(),
            Some(FocusId::RouteTab(TuiRoute::NowPlaying)),
            "fallback with an empty list is the first tab"
        );
        model.move_focus(FocusMove::Previous);
        assert_eq!(
            model.focus(),
            Some(FocusId::RouteTab(TuiRoute::Visualizer)),
            "Shift+Tab from the cycle edge wraps to the last control"
        );

        // One row appears: the list joins the cycle. The focus on the
        // Visualizer tab is still valid, so validation keeps it.
        model.set_playlist(2, || {
            vec![PlaylistRow {
                label: "a.flac".to_owned(),
                playing: false,
                selected: false,
            }]
        });
        model.validate_focus();
        assert_eq!(
            model.focus(),
            Some(FocusId::RouteTab(TuiRoute::Visualizer)),
            "a still-valid focus is never moved by validation"
        );
        model.move_focus(FocusMove::Next);
        assert_eq!(
            model.focus(),
            Some(FocusId::Playlist),
            "the list joined the cycle"
        );
        model.move_focus(FocusMove::Next);
        assert_eq!(
            model.focus(),
            Some(FocusId::RouteTab(TuiRoute::NowPlaying)),
            "Tab wraps from the last control to the first"
        );

        // The §12 fallback: a focus that left the cycle (the list, after
        // it emptied again) lands on the first remaining control.
        model.set_playlist(3, Vec::new);
        model.validate_focus();
        assert_eq!(
            model.focus(),
            Some(FocusId::RouteTab(TuiRoute::NowPlaying)),
            "an empty list drops the local control from the cycle"
        );
        // On Now Playing the same fallback lands on the transport row,
        // the route's first local control.
        model.set_focus(Some(FocusId::Playlist));
        model.set_route(TuiRoute::NowPlaying);
        assert_eq!(
            model.focus(),
            Some(FocusId::Transport(TransportButton::Open)),
            "an out-of-cycle focus falls back to the route's first local control"
        );
    }

    // ------------------------------------------------------------------
    // Keyboard decoding tests.
    // ------------------------------------------------------------------

    #[test]
    fn s_maps_to_stop_and_q_maps_to_quit() {
        let model = TuiModel::new("song.flac");
        for key_char in ['s', 'S'] {
            assert_eq!(
                decode_key(
                    KeyEvent::new(KeyCode::Char(key_char), KeyModifiers::NONE),
                    &model
                ),
                Some(TuiAction::Stop),
                "{key_char} must request stop"
            );
        }
        for key_char in ['q', 'Q'] {
            assert_eq!(
                decode_key(
                    KeyEvent::new(KeyCode::Char(key_char), KeyModifiers::NONE),
                    &model
                ),
                Some(TuiAction::Quit),
                "{key_char} must quit"
            );
        }
        // Terminals disagree about reporting SHIFT with a letter.
        assert_eq!(
            decode_key(
                KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT),
                &model
            ),
            Some(TuiAction::Stop)
        );
    }

    #[test]
    fn ctrl_c_keeps_its_conventional_quit_meaning_everywhere() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(
            decode_key(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &model
            ),
            Some(TuiAction::Quit)
        );
        model.open_modal(ModalKind::Open);
        assert_eq!(
            decode_key(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &model
            ),
            Some(TuiAction::Quit),
            "Ctrl+C quits from inside a modal too"
        );
    }

    /// Tab/Shift+Tab/Enter decode to focus moves and focused-control
    /// activation; Enter's effect is whatever the FOCUSED control
    /// activates (§10).
    #[test]
    fn tab_and_enter_decode_to_the_focus_vocabulary() {
        let mut model = TuiModel::new("song.flac");
        model.set_class(ResponsiveClass::Normal);
        assert_eq!(
            decode_key(key(KeyCode::Tab), &model),
            Some(TuiAction::MoveFocus(FocusMove::Next))
        );
        assert_eq!(
            decode_key(key(KeyCode::BackTab), &model),
            Some(TuiAction::MoveFocus(FocusMove::Previous))
        );
        assert_eq!(
            decode_key(key(KeyCode::Enter), &model),
            Some(TuiAction::ActivateFocused)
        );
        // With nothing focused, activation resolves to nothing.
        assert_eq!(model.activation(), None);

        model.focus = Some(FocusId::RouteTab(TuiRoute::Playlist));
        assert_eq!(
            model.activation(),
            Some(TuiAction::Navigate(TuiRoute::Playlist))
        );
        model.focus = Some(FocusId::Transport(TransportButton::PlayPause));
        assert_eq!(model.activation(), Some(TuiAction::PlayPause));
        model.focus = Some(FocusId::Playlist);
        assert_eq!(model.activation(), Some(TuiAction::PlaylistPlaySelected));
    }

    /// The contextual arrows (§10): the focused list owns Up/Down; with
    /// any other focus they are noise, and they never also seek.
    #[test]
    fn up_down_are_contextual_to_the_focused_list() {
        let mut model = TuiModel::new("song.flac");
        model.set_class(ResponsiveClass::Normal);
        model.focus = Some(FocusId::Transport(TransportButton::PlayPause));
        assert_eq!(decode_key(key(KeyCode::Up), &model), None, "not the list");
        assert_eq!(decode_key(key(KeyCode::Down), &model), None);
        model.focus = Some(FocusId::Playlist);
        assert_eq!(
            decode_key(key(KeyCode::Up), &model),
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Previous))
        );
        assert_eq!(
            decode_key(key(KeyCode::Down), &model),
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Next))
        );
        // Left/Right keep the seek meaning next to a focused list — the
        // two axes never share a key.
        assert_eq!(
            decode_key(key(KeyCode::Left), &model),
            Some(TuiAction::SeekRelative(-5))
        );
        assert_eq!(
            decode_key(key(KeyCode::Right), &model),
            Some(TuiAction::SeekRelative(5))
        );
    }

    /// The seek keys decode to the same signed-relative action at the
    /// frozen 5 s and 30 s steps; release events never act.
    #[test]
    fn arrows_map_to_the_small_and_large_seek_actions() {
        let model = TuiModel::new("song.flac");
        for (key_code, small, large) in [
            (KeyCode::Left, -SEEK_STEP_SECS, -LARGE_SEEK_STEP_SECS),
            (KeyCode::Right, SEEK_STEP_SECS, LARGE_SEEK_STEP_SECS),
        ] {
            assert_eq!(
                decode_key(KeyEvent::new(key_code, KeyModifiers::NONE), &model),
                Some(TuiAction::SeekRelative(small))
            );
            assert_eq!(
                decode_key(KeyEvent::new(key_code, KeyModifiers::SHIFT), &model),
                Some(TuiAction::SeekRelative(large)),
                "the shift-keyed arrow is the LARGE step"
            );
            assert_eq!(
                decode_key(
                    KeyEvent::new_with_kind(key_code, KeyModifiers::NONE, KeyEventKind::Release),
                    &model
                ),
                None,
                "release events never act"
            );
            for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
                assert_eq!(
                    decode_key(KeyEvent::new(key_code, modifiers), &model),
                    None,
                    "chorded arrows stay noise"
                );
            }
        }
    }

    #[test]
    fn any_other_key_is_presentation_noise() {
        let model = TuiModel::new("song.flac");
        for event in [
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            // Chords stay noise (except Ctrl+C) so e.g. Ctrl+S/Ctrl+Q
            // never act by accident.
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::ALT),
            KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
        ] {
            assert_eq!(decode_key(event, &model), None, "{event:?} must be ignored");
        }
        // Key-release events (Windows terminals emit them) never act.
        assert_eq!(
            decode_key(
                KeyEvent::new_with_kind(
                    KeyCode::Char('s'),
                    KeyModifiers::NONE,
                    KeyEventKind::Release
                ),
                &model
            ),
            None
        );
    }

    /// Space stays the pause/resume accelerator; Open/GoTo/Help decode
    /// to the modal actions.
    #[test]
    fn the_application_keys_map_to_their_actions() {
        let model = TuiModel::new("song.flac");
        assert_eq!(
            decode_key(key(KeyCode::Char(' ')), &model),
            Some(TuiAction::PlayPause)
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('o')), &model),
            Some(TuiAction::OpenModal(ModalKind::Open))
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('G')), &model),
            Some(TuiAction::OpenModal(ModalKind::GoTo))
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('?')), &model),
            Some(TuiAction::OpenModal(ModalKind::Help))
        );
        for (key_char, action) in [
            ('n', TuiAction::Next),
            ('p', TuiAction::Previous),
            ('r', TuiAction::ToggleOrder),
            ('l', TuiAction::CycleRepeat),
        ] {
            assert_eq!(
                decode_key(key(KeyCode::Char(key_char)), &model),
                Some(action)
            );
        }
        assert_eq!(
            decode_key(key(KeyCode::Char('+')), &model),
            Some(TuiAction::VolumeUp)
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('=')), &model),
            Some(TuiAction::VolumeUp)
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('-')), &model),
            Some(TuiAction::VolumeDown)
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('_')), &model),
            Some(TuiAction::VolumeDown)
        );
    }

    /// Inside a text modal every plain character is a literal character
    /// (the Q-drive lesson, Issue #166 §29): `Q:\Music` stays typeable,
    /// Enter confirms, Esc cancels, Backspace edits — and no other key
    /// acts.
    #[test]
    fn a_text_modal_captures_its_editing_keys() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::Open);
        for c in r"Q:\Music".chars() {
            assert_eq!(
                decode_key(key(KeyCode::Char(c)), &model),
                Some(TuiAction::ModalInput(ModalInput::Char(c))),
                "{c:?} must be typed literally, not acted on"
            );
        }
        assert_eq!(
            decode_key(key(KeyCode::Enter), &model),
            Some(TuiAction::ModalInput(ModalInput::Confirm))
        );
        assert_eq!(
            decode_key(key(KeyCode::Esc), &model),
            Some(TuiAction::ModalInput(ModalInput::Cancel))
        );
        assert_eq!(
            decode_key(key(KeyCode::Backspace), &model),
            Some(TuiAction::ModalInput(ModalInput::Backspace))
        );
        // Non-plain characters stay noise.
        assert_eq!(
            decode_key(
                KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
                &model
            ),
            None
        );
        // The GoTo line gets the same capture.
        model.open_modal(ModalKind::GoTo);
        assert_eq!(
            decode_key(key(KeyCode::Char('q')), &model),
            Some(TuiAction::ModalInput(ModalInput::Char('q')))
        );
    }

    /// The help overlay owns the keyboard: `?`/Esc close it, Q quits,
    /// everything else is noise — no playback key can fire behind it.
    #[test]
    fn the_help_modal_lets_only_its_close_keys_through() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::Help);
        for code in [
            KeyCode::Char(' '),
            KeyCode::Char('s'),
            KeyCode::Char('n'),
            KeyCode::Char('r'),
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Left,
        ] {
            assert_eq!(
                decode_key(key(code), &model),
                None,
                "{code:?} must not act behind the help overlay"
            );
        }
        assert_eq!(
            decode_key(key(KeyCode::Char('?')), &model),
            Some(TuiAction::ModalInput(ModalInput::Cancel))
        );
        assert_eq!(
            decode_key(key(KeyCode::Esc), &model),
            Some(TuiAction::ModalInput(ModalInput::Cancel))
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('q')), &model),
            Some(TuiAction::Quit)
        );
    }

    // ------------------------------------------------------------------
    // Modal lifecycle tests (the old Open/GoTo/Help behaviors migrated).
    // ------------------------------------------------------------------

    /// At most one modal exists (§24): opening while one is open
    /// replaces it; closing restores a valid route focus.
    #[test]
    fn at_most_one_modal_exists_and_closing_restores_a_valid_focus() {
        let mut model = TuiModel::new("song.flac");
        model.set_class(ResponsiveClass::Normal);
        model.open_modal(ModalKind::Open);
        assert!(matches!(model.modal(), Some(Modal::Open { .. })));
        model.open_modal(ModalKind::Help);
        assert_eq!(
            model.modal().map(Modal::kind),
            Some(ModalKind::Help),
            "no modal stack"
        );
        assert_eq!(model.focus(), Some(FocusId::ModalField));
        assert_eq!(model.armed(), None, "opening clears the armed click (§24)");

        model.close_modal();
        assert_eq!(model.modal(), None);
        assert_eq!(
            model.focus(),
            Some(FocusId::Transport(TransportButton::Open)),
            "closing restores the route's first meaningful local focus (§24)"
        );
    }

    /// The Open modal lifecycle: begin → edit → confirm returns the
    /// candidate and closes; an empty line and Esc are cancels; a
    /// second O restarts the line.
    #[test]
    fn the_open_modal_edits_confirms_and_cancels() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(model.modal(), None);

        model.open_modal(ModalKind::Open);
        assert_eq!(model.modal_line(), Some(""));
        for c in "/media/b.flac".chars() {
            model.modal_push(c);
        }
        assert_eq!(model.modal_line(), Some("/media/b.flac"));
        model.modal_backspace();
        assert_eq!(model.modal_line(), Some("/media/b.fla"));

        assert_eq!(
            model.confirm_modal(),
            ModalConfirm::Open("/media/b.fla".to_owned()),
            "confirm returns the candidate and closes the modal"
        );
        assert_eq!(model.modal(), None);

        // An empty line confirms nothing — it is a cancel, never an
        // Open of "".
        model.open_modal(ModalKind::Open);
        assert_eq!(model.confirm_modal(), ModalConfirm::Nothing);
        assert_eq!(model.modal(), None);

        // Editing primitives are inert without an open text modal.
        model.modal_push('x');
        assert_eq!(model.modal(), None);
    }

    /// The GoTo modal parses with the EXISTING shared reader, closes on
    /// a readable token, and an unreadable token keeps the line OPEN
    /// and sends nothing (Issue #166 §27).
    #[test]
    fn the_goto_modal_parses_with_the_shared_reader_and_fails_open_for_correction() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::GoTo);
        for c in "01:35.5".chars() {
            model.modal_push(c);
        }
        assert_eq!(
            model.confirm_modal(),
            ModalConfirm::Seek(Duration::from_millis(95_500)),
            "the shared reader owns the time grammar"
        );
        assert_eq!(model.modal(), None);

        // Plain seconds and mm:ss read the same way.
        for (text, expected) in [
            ("95", Duration::from_secs(95)),
            ("1:35", Duration::from_secs(95)),
        ] {
            let mut model = TuiModel::new("song.flac");
            model.open_modal(ModalKind::GoTo);
            for c in text.chars() {
                model.modal_push(c);
            }
            assert_eq!(model.confirm_modal(), ModalConfirm::Seek(expected));
        }

        for bad in ["abc", "1:99", "-30", "nan", "inf", "1e400"] {
            let mut model = TuiModel::new("song.flac");
            model.open_modal(ModalKind::GoTo);
            for c in bad.chars() {
                model.modal_push(c);
            }
            let ModalConfirm::Unreadable(diagnostic) = model.confirm_modal() else {
                panic!("{bad:?} must not parse into a seek target");
            };
            assert!(!diagnostic.is_empty());
            assert!(
                model.modal().is_some(),
                "{bad:?}: the line stays open for correction"
            );
            model.close_modal();
            assert_eq!(model.modal(), None);
        }

        // An empty line is a cancel.
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::GoTo);
        assert_eq!(model.confirm_modal(), ModalConfirm::Nothing);
        assert_eq!(model.modal(), None);
    }

    // ------------------------------------------------------------------
    // Mouse decoding tests (armed-click rule, wheel policy, §16–§25).
    // These run against REAL published geometry from the view draw, so
    // a layout regression breaks here first.
    // ------------------------------------------------------------------

    /// Left Down focuses and arms; Left Up on the same valid target
    /// activates exactly one action; the Down itself dispatches none.
    #[test]
    fn the_armed_click_rule_down_focuses_and_up_activates() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        // Click the Playlist tab.
        let (column, row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::Playlist));
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Down(MouseButton::Left), column, row),
                &mut model
            ),
            None,
            "the Down dispatches nothing"
        );
        assert_eq!(model.focus(), Some(FocusId::RouteTab(TuiRoute::Playlist)));
        assert_eq!(model.armed(), Some(HitTarget::RouteTab(TuiRoute::Playlist)));
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                &mut model
            ),
            Some(TuiAction::Navigate(TuiRoute::Playlist))
        );
        assert_eq!(model.armed(), None, "the Up consumed the armed target");
    }

    /// A mouse Up without a matching Down dispatches nothing (§17).
    #[test]
    fn an_unmatched_up_is_no_action() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let (column, row) = region_cell(&model, &HitTarget::Transport(TransportButton::PlayPause));
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                &mut model
            ),
            None
        );
    }

    /// Down on one target, Up on another: no action (§17).
    #[test]
    fn down_on_a_up_on_b_is_no_action() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let (down, down_row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::NowPlaying));
        let (up, up_row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::Playlist));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), down, down_row),
            &mut model,
        );
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), up, up_row),
                &mut model
            ),
            None,
            "the pointer changed target between Down and Up"
        );
        assert_eq!(
            model.armed(),
            None,
            "the Up consumed the stale armed target"
        );
    }

    /// Down, then a resize invalidates the frame, then Up at the same
    /// cell: no action (§17/§29). The regions are gone and the armed
    /// target with them, so the Up matches nothing.
    #[test]
    fn down_resize_up_is_no_action() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let (column, row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::NowPlaying));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut model,
        );
        model.invalidate_frame();
        assert_eq!(model.armed(), None);
        assert!(
            model.hit_test(column, row).is_none(),
            "the resize cleared the regions"
        );
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                &mut model
            ),
            None
        );
    }

    /// Down, then a route change, then Up at the same cell: no action
    /// (§17). A route change can also move the control that sits at the
    /// cell — either way nothing dispatches.
    #[test]
    fn down_route_change_up_is_no_action() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let (column, row) = region_cell(&model, &HitTarget::Transport(TransportButton::PlayPause));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut model,
        );
        assert_eq!(
            model.armed(),
            Some(HitTarget::Transport(TransportButton::PlayPause))
        );
        model.set_route(TuiRoute::Playlist);
        assert_eq!(model.armed(), None, "the route change disarmed the click");
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                &mut model
            ),
            None
        );
    }

    /// Drag cancels the armed click (§21); plain movement is inert (§18).
    #[test]
    fn drag_cances_and_movement_is_inert() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let (column, row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::Audio));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut model,
        );
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Drag(MouseButton::Left), column, row + 1),
                &mut model
            ),
            None
        );
        assert_eq!(model.armed(), None, "the drag cancelled the armed click");
        assert_eq!(
            decode_mouse(mouse(MouseEventKind::Moved, 5, 5), &mut model),
            None,
            "plain movement dispatches nothing (§18)"
        );
    }

    /// Right/middle clicks carry no product meaning (§19) and never
    /// arm; they do not disturb an existing armed click either.
    #[test]
    fn right_click_is_ignored() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let (column, row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::Playlist));
        model.set_focus(None);
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Down(MouseButton::Right), column, row),
                &mut model
            ),
            None
        );
        assert_eq!(model.armed(), None);
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Down(MouseButton::Middle), column, row),
                &mut model
            ),
            None
        );
        assert_eq!(model.focus(), None, "a non-left Down does not even focus");
    }

    /// Double clicks have no special product meaning (§20): the second
    /// Down/Up pair dispatches exactly what a first pair would — one
    /// row select, never a play.
    #[test]
    fn a_double_click_stays_two_single_clicks() {
        let mut model = model_with_regions(100, 30, TuiRoute::Playlist);
        let (column, row) = region_cell(&model, &HitTarget::PlaylistRow(1));
        for _ in 0..2 {
            decode_mouse(
                mouse(MouseEventKind::Down(MouseButton::Left), column, row),
                &mut model,
            );
            assert_eq!(
                decode_mouse(
                    mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                    &mut model
                ),
                Some(TuiAction::PlaylistSelect(PlaylistCursor::Row(1)))
            );
        }
    }

    /// The wheel scrolls only the playlist list (§22): over a row or
    /// the pane it moves the selection, everywhere else it is inert.
    #[test]
    fn the_wheel_scrolls_only_the_list() {
        let mut model = model_with_regions(100, 30, TuiRoute::Playlist);
        let (column, row) = region_cell(&model, &HitTarget::PlaylistRow(2));
        assert_eq!(
            decode_mouse(mouse(MouseEventKind::ScrollDown, column, row), &mut model),
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Next))
        );
        assert_eq!(
            decode_mouse(mouse(MouseEventKind::ScrollUp, column, row), &mut model),
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Previous))
        );
        // Horizontal wheels have no meaning.
        assert_eq!(
            decode_mouse(mouse(MouseEventKind::ScrollLeft, column, row), &mut model),
            None
        );

        // Inert over a tab on the same frame.
        let (tab, tab_row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::Audio));
        assert_eq!(
            decode_mouse(mouse(MouseEventKind::ScrollDown, tab, tab_row), &mut model),
            None
        );
    }

    /// While a modal is open the background is inert (§25): a click on
    /// a background control neither arms nor dispatches — including the
    /// very cell that would otherwise activate.
    #[test]
    fn a_modal_open_ignores_background_clicks() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        // Locate the background cells BEFORE the modal opens: opening
        // invalidates the frame's regions (§24), which is exactly the
        // behavior under test.
        let (column, row) = region_cell(&model, &HitTarget::Transport(TransportButton::PlayPause));
        let (tab, tab_row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::Playlist));
        model.open_modal(ModalKind::Help);
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Down(MouseButton::Left), column, row),
                &mut model
            ),
            None
        );
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                &mut model
            ),
            None
        );
        assert_eq!(model.armed(), None, "no arming behind a modal");
        assert_eq!(
            model.focus(),
            Some(FocusId::ModalField),
            "the click did not steal focus from the modal"
        );
        // Wheel behind the modal is inert too — the tab cell would be
        // background either way.
        assert_eq!(
            decode_mouse(mouse(MouseEventKind::ScrollDown, tab, tab_row), &mut model),
            None
        );
    }

    /// The mouse activation converges on the same action as the
    /// keyboard activation for the same control (§30).
    #[test]
    fn keyboard_and_mouse_activation_converge_on_one_action() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);

        // Route tab.
        let (tab, tab_row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::Playlist));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), tab, tab_row),
            &mut model,
        );
        let mouse_action = decode_mouse(
            mouse(MouseEventKind::Up(MouseButton::Left), tab, tab_row),
            &mut model,
        );
        model.focus = Some(FocusId::RouteTab(TuiRoute::Playlist));
        let keyboard_action = model.activation();
        assert_eq!(mouse_action, keyboard_action);

        // Transport button.
        let (button, button_row) =
            region_cell(&model, &HitTarget::Transport(TransportButton::Stop));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), button, button_row),
            &mut model,
        );
        let mouse_action = decode_mouse(
            mouse(MouseEventKind::Up(MouseButton::Left), button, button_row),
            &mut model,
        );
        model.focus = Some(FocusId::Transport(TransportButton::Stop));
        assert_eq!(mouse_action, model.activation());
    }

    /// The first cell (left column, middle row) of the region whose
    /// target matches, for mouse-decoding tests.
    fn region_cell(model: &TuiModel, target: &HitTarget) -> (u16, u16) {
        let region = model
            .regions()
            .iter()
            .find(|region| &region.target == target)
            .unwrap_or_else(|| panic!("no region for {target:?} in the published frame"));
        (region.area.x + 1, region.area.y + region.area.height / 2)
    }

    // ------------------------------------------------------------------
    // Responsive class tests (§27/§28 mechanics only).
    // ------------------------------------------------------------------

    #[test]
    fn the_responsive_class_follows_the_terminal_size() {
        assert_eq!(responsive_class(20, 8), ResponsiveClass::Minimum);
        assert_eq!(
            responsive_class(MIN_WIDTH - 1, 30),
            ResponsiveClass::Minimum
        );
        assert_eq!(
            responsive_class(100, MIN_HEIGHT - 1),
            ResponsiveClass::Minimum
        );
        assert_eq!(responsive_class(50, 16), ResponsiveClass::Compact);
        assert_eq!(responsive_class(80, 24), ResponsiveClass::Normal);
        assert_eq!(responsive_class(120, 40), ResponsiveClass::Wide);
    }

    // ------------------------------------------------------------------
    // G1: the Open picker grammar, the seek bar, the DSP summary.
    // ------------------------------------------------------------------

    /// The picker's keyboard grammar (G1 §8): Tab cycles field → list
    /// → commit buttons; ↑/↓ move the listing cursor and hand focus to
    /// the list; typing always edits the field and returns focus to it;
    /// Backspace edits on the field and steps to the parent on the
    /// list; Enter acts on the focused control.
    #[test]
    fn the_open_picker_keyboard_grammar_walks_field_list_and_buttons() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::Open);
        model.set_open_listing(
            std::path::PathBuf::from("/media"),
            Ok(vec![
                crate::input::DirectoryEntry {
                    name: "album".to_owned(),
                    is_dir: true,
                },
                crate::input::DirectoryEntry {
                    name: "b.flac".to_owned(),
                    is_dir: false,
                },
            ]),
        );
        // The `..` parent row is synthesized in front of the listing.
        assert_eq!(model.modal().map(Modal::kind), Some(ModalKind::Open));
        assert_eq!(model.focus(), Some(FocusId::ModalField));

        // Down: into the list, first row (the parent row).
        assert_eq!(
            decode_key(key(KeyCode::Down), &model),
            Some(TuiAction::ModalInput(ModalInput::ListMove(
                PlaylistCursor::Next
            )))
        );
        model.move_picker_cursor(PlaylistCursor::Next);
        assert_eq!(model.focus(), Some(FocusId::PickerList));

        // Typing from the list is type-through: the field edits and
        // regains focus.
        assert_eq!(
            decode_key(key(KeyCode::Char('x')), &model),
            Some(TuiAction::ModalInput(ModalInput::Char('x')))
        );
        model.modal_push('x');
        assert_eq!(model.modal_line(), Some("x"));
        assert_eq!(model.focus(), Some(FocusId::ModalField));

        // Tab walks field → list → Open → Add → Cancel → field.
        for expected in [
            FocusId::PickerList,
            FocusId::PickerButton(ModalButton::Open),
            FocusId::PickerButton(ModalButton::Add),
            FocusId::PickerButton(ModalButton::Cancel),
            FocusId::ModalField,
        ] {
            model.move_focus(FocusMove::Next);
            assert_eq!(model.focus(), Some(expected));
        }

        // Backspace on the LIST is the parent step, on the FIELD an
        // edit. Shift+Tab walks back: field → Cancel → Add → Open →
        // list.
        model.move_focus(FocusMove::Previous);
        assert_eq!(
            model.focus(),
            Some(FocusId::PickerButton(ModalButton::Cancel))
        );
        model.move_focus(FocusMove::Previous);
        assert_eq!(model.focus(), Some(FocusId::PickerButton(ModalButton::Add)));
        model.move_focus(FocusMove::Previous);
        assert_eq!(
            model.focus(),
            Some(FocusId::PickerButton(ModalButton::Open))
        );
        model.move_focus(FocusMove::Previous);
        assert_eq!(model.focus(), Some(FocusId::PickerList));
        assert_eq!(
            decode_key(key(KeyCode::Backspace), &model),
            Some(TuiAction::ModalInput(ModalInput::ListParent))
        );
        model.set_focus(Some(FocusId::ModalField));
        assert_eq!(
            decode_key(key(KeyCode::Backspace), &model),
            Some(TuiAction::ModalInput(ModalInput::Backspace))
        );

        // Enter follows the focus: list activates, a button commits,
        // the field confirms.
        assert_eq!(
            decode_key(key(KeyCode::Enter), &model),
            Some(TuiAction::ModalInput(ModalInput::Confirm))
        );
        model.set_focus(Some(FocusId::PickerButton(ModalButton::Add)));
        assert_eq!(
            decode_key(key(KeyCode::Enter), &model),
            Some(TuiAction::ModalInput(ModalInput::CommitAdd))
        );
        model.set_focus(Some(FocusId::PickerList));
        assert_eq!(
            decode_key(key(KeyCode::Enter), &model),
            Some(TuiAction::ModalInput(ModalInput::ListActivate))
        );
    }

    /// The cursor movement rule: from unselected, ↓ enters at the top
    /// and ↑ at the bottom; a row click selects that row; the cursor
    /// clamps at the edges.
    #[test]
    fn the_picker_cursor_moves_and_clamps() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::Open);
        let feed = Ok(vec![
            crate::input::DirectoryEntry {
                name: "d1".to_owned(),
                is_dir: true,
            },
            crate::input::DirectoryEntry {
                name: "f1.flac".to_owned(),
                is_dir: false,
            },
        ]);
        model.set_open_listing(std::path::PathBuf::from("/media"), feed);
        // Entries: [.., d1, f1.flac].
        model.move_picker_cursor(PlaylistCursor::Next);
        assert_eq!(
            model.picker_cursor_entry(),
            Some((std::path::PathBuf::from("/",), true)),
            "the first ↓ lands on the `..` row, which resolves to the PARENT"
        );
        model.move_picker_cursor(PlaylistCursor::Next);
        model.move_picker_cursor(PlaylistCursor::Next);
        assert_eq!(
            model.picker_cursor_entry(),
            Some((std::path::PathBuf::from("/media/f1.flac",), false)),
        );
        model.move_picker_cursor(PlaylistCursor::Next);
        assert_eq!(
            model.picker_cursor_entry().map(|(path, _)| path),
            Some(std::path::PathBuf::from("/media/f1.flac")),
            "the cursor clamps at the last row"
        );
        model.move_picker_cursor(PlaylistCursor::Row(1));
        assert_eq!(
            model.picker_cursor_entry().map(|(path, _)| path),
            Some(std::path::PathBuf::from("/media/d1")),
            "a row click selects that row"
        );
        model.move_picker_cursor(PlaylistCursor::Previous);
        assert_eq!(
            model.picker_cursor_entry().map(|(path, _)| path),
            Some(std::path::PathBuf::from("/",)),
            "activating `..` ascends"
        );
    }

    /// The commit subject (G1 §8): the selected row when one is
    /// selected, else the typed line; nothing selected and an empty
    /// line is no subject at all.
    #[test]
    fn the_picker_commit_subject_prefers_the_selection_then_the_field() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::Open);
        assert_eq!(model.picker_commit_subject(), None, "nothing picked yet");
        model.modal_push('/');

        // No selection: the typed line is the subject, kind unknown.
        let (path, is_dir) = model.picker_commit_subject().expect("typed subject");
        assert_eq!(path, std::path::PathBuf::from("/"));
        assert!(!is_dir, "the typed kind is the runtime's question");

        // A selection wins over the field.
        model.set_open_listing(
            std::path::PathBuf::from("/media"),
            Ok(vec![crate::input::DirectoryEntry {
                name: "b.flac".to_owned(),
                is_dir: false,
            }]),
        );
        model.move_picker_cursor(PlaylistCursor::Row(1));
        assert_eq!(
            model.picker_commit_subject().map(|(path, _)| path),
            Some(std::path::PathBuf::from("/media/b.flac")),
        );
    }

    /// A click on the seek bar decodes to the clicked cell's per-mille
    /// of the bar (G1 §9): Down arms, Up activates at the SAME cell,
    /// and the fraction comes from the frame's own published geometry.
    #[test]
    fn a_click_on_the_seek_bar_decodes_to_a_per_mille_of_the_bar() {
        let mut model = TuiModel::new("song.flac");
        model.update(PlaybackSessionObservation {
            source_format: Some(qianqian_audio_api::ports::PcmFormat {
                sample_rate: 44_100,
                channels: 2,
                channel_mask: 0x3,
            }),
            position: Some(44_100),
            source_duration: Some(Duration::from_secs(200)),
            ..pending()
        });
        model.set_class(ResponsiveClass::Wide);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|frame| super::super::view::draw(frame, &mut model))
            .expect("draw");
        let region = model
            .regions()
            .iter()
            .find(|region| region.target == HitTarget::SeekBar)
            .expect("the bar publishes a region with duration evidence");
        let column = region.area.x + region.area.width / 2;
        let row = region.area.y;

        let down = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        let up = MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(decode_mouse(down, &mut model), None, "Down only arms");
        assert_eq!(
            decode_mouse(up, &mut model),
            Some(TuiAction::SeekPerMille(520)),
            "the click's per-mille of the DRAWN glyph (24 cells, click at center)"
        );
    }

    /// The fraction seek target is a per-mille of the PUBLISHED
    /// duration — and `None` (no command at all) without duration
    /// evidence: an unknown timeline is never clickable into a
    /// fabricated target.
    #[test]
    fn the_seek_fraction_target_needs_published_duration_evidence() {
        let with_duration = PlaybackSessionObservation {
            source_duration: Some(Duration::from_secs(200)),
            ..pending()
        };
        assert_eq!(
            seek_fraction_target(&with_duration, 500),
            Some(Duration::from_secs(100))
        );
        assert_eq!(
            seek_fraction_target(&with_duration, 1000),
            Some(Duration::from_secs(200))
        );
        assert_eq!(
            seek_fraction_target(&with_duration, 1500),
            Some(Duration::from_secs(200)),
            "the fraction clamps into the duration"
        );
        let without_duration = PlaybackSessionObservation {
            source_duration: None,
            ..pending()
        };
        assert_eq!(seek_fraction_target(&without_duration, 500), None);
        let zero_duration = PlaybackSessionObservation {
            source_duration: Some(Duration::ZERO),
            ..pending()
        };
        assert_eq!(seek_fraction_target(&zero_duration, 500), None);
    }

    /// The DSP summary line (G1): a DESIRED-state statement only —
    /// bypass says off, a preset configuration names the preset, a
    /// custom EQ says custom, the preamp renders in dB — and nothing
    /// here claims an applied state.
    #[test]
    fn the_dsp_summary_names_the_desired_configuration_only() {
        assert_eq!(
            dsp_summary(&AudioProcessingConfig::BYPASS),
            "DSP (desired): off (bypass)"
        );
        assert_eq!(
            dsp_summary(&EqPreset::Rock.to_config()),
            "DSP (desired): on — preset rock, preamp +0.0 dB"
        );
        let mut custom = EqPreset::Bass.to_config();
        custom.eq = Some(qianqian_playback::EqConfig::new(
            qianqian_playback::EqConfig::FLAT.band_gain_db,
            1.7,
        ));
        assert_eq!(
            dsp_summary(&custom),
            "DSP (desired): on — custom EQ, preamp +0.0 dB"
        );
        assert_eq!(
            dsp_summary(&AudioProcessingConfig::gain(2.0)),
            "DSP (desired): on — preamp +6.0 dB"
        );
    }
    /// Below the minimum size the shell paints no popup (§28), so the
    /// modal's keys go inert except the cancel: a blind Enter behind
    /// an invisible picker must never commit a real Open.
    #[test]
    fn a_modal_below_the_minimum_only_cancels() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::Open);
        model.modal_push('x');
        model.set_class(ResponsiveClass::Minimum);
        assert_eq!(
            decode_key(key(KeyCode::Enter), &model),
            None,
            "no blind commit behind an invisible popup"
        );
        assert_eq!(decode_key(key(KeyCode::Char('a')), &model), None);
        assert_eq!(
            decode_key(key(KeyCode::Esc), &model),
            Some(TuiAction::ModalInput(ModalInput::Cancel)),
            "Esc stays the honest way out"
        );
    }
}
