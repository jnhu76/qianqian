//! Pure TUI model: the truthful projection of one episode observation
//! into display labels, plus the keyboard grammar and its wiring to the
//! episode seam.
//!
//! Same truth-class discipline as [`crate::status`] (D14.2/D14.7):
//! `pending` states only "no terminal Fact committed yet" — never
//! Playing/Starting/Stopping; `stop_requested`/`pause_requested` are
//! Command state; `source_format` is mechanism evidence; the `Paused`
//! projection is derived by the seam itself from the frozen D14.7
//! establishment conjunction; the diagnostics are presentation text.
//! This module performs no I/O and holds no truth of its own; every
//! label is derived from the last observation handed to
//! [`TuiModel::update`].
//!
//! The playlist pane is the same shape of thing (Issue #166 §6): a
//! presentation PROJECTION of the App's own navigation state — the rows
//! it is handed are a snapshot of the temporary playlist's traversal
//! order, and the two markers are the App's committed/selected cursors.
//! No row, marker or count here is playback truth, and the shell never
//! derives one from an episode observation.
//!
//! # Input mode precedence (Issue #166 §28)
//!
//! ```text
//! Open input active   keys edit the Open line; Enter opens, Esc cancels
//! GoTo input active   keys edit the seek target; Enter seeks, Esc cancels
//! Help visible        ? / Esc close it; Q quits; everything else is noise
//! Normal              the whole player grammar below
//! ```
//!
//! The modes are a short precedence list rather than a state-machine
//! framework, and NO key both edits and executes: a playback key can
//! never fire while a modal or the help overlay owns the keyboard.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::path::Path;
use std::time::Duration;

use crate::playlist::{PlaybackOrder, RepeatMode};
use qianqian_playback::{
    EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionHandle, PlaybackSessionObservation,
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

/// The result of confirming the GoTo line (Issue #166 §27).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GotoConfirm {
    /// The line read as a seek target; the line is closed.
    Seek(Duration),
    /// The line was empty: closed, and no command is sent.
    Cancelled,
    /// The line is not a readable time. The line STAYS OPEN for
    /// correction and the shell shows the bounded diagnostic — a
    /// malformed seek intent is never sent.
    Unreadable(&'static str),
}

/// One frame's worth of presentation state: the episode the player has
/// committed (source path + latest coherent observation), the playlist
/// rows, the Open/GoTo input lines while they are active, and the last
/// operation's feedback. All of it is presentation: the shell keeps no
/// playback truth of its own.
pub struct TuiModel {
    /// The committed episode's source path. `None` is a real state
    /// (F6): no episode is live — a clean-failed Open leaves no
    /// runtime, and the honest panel says so instead of fabricating
    /// labels.
    source: Option<String>,
    observation: PlaybackSessionObservation,
    /// The Open input line (D14.6): shell representation of the Open
    /// input UX, which the ADR leaves open. `None` = not in input mode.
    open_input: Option<String>,
    /// The GoTo input line (Issue #166 §27): the exact-seek adapter.
    /// `None` = not in input mode.
    goto_input: Option<String>,
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
    /// Whether the keyboard-help overlay is shown. Presentation-local
    /// state (Issue #166 §0 admits exactly this class); it carries no
    /// playback truth and survives nothing.
    help_visible: bool,
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
            },
            open_input: None,
            goto_input: None,
            status: None,
            navigation_position: None,
            playlist: Vec::new(),
            playlist_revision: None,
            order: None,
            repeat: None,
            volume: None,
            help_visible: false,
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
            };
        }
    }

    /// The committed episode's source path, if one is live.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Record one Open operation's feedback line (composition
    /// feedback, never a playback semantic).
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

    /// Enter Open input mode (the O key). While the line is active the
    /// runtime routes keys INTO it (a second O types `o`); this method
    /// is only reachable from the plain grammar.
    pub fn begin_open_input(&mut self) {
        self.open_input = Some(String::new());
    }

    /// Whether the Open input line is active.
    pub fn open_input_active(&self) -> bool {
        self.open_input.is_some()
    }

    /// The line's current content, while editing.
    pub fn open_input(&self) -> Option<&str> {
        self.open_input.as_deref()
    }

    pub fn open_input_push(&mut self, c: char) {
        if let Some(line) = &mut self.open_input {
            line.push(c);
        }
    }

    pub fn open_input_backspace(&mut self) {
        if let Some(line) = &mut self.open_input {
            line.pop();
        }
    }

    /// Confirm the line: returns the candidate path and closes input
    /// mode. An empty line is a cancel (`None`), never an Open of "".
    pub fn confirm_open_input(&mut self) -> Option<String> {
        let line = self.open_input.take()?;
        if line.is_empty() {
            return None;
        }
        Some(line)
    }

    /// Leave Open input mode without opening anything.
    pub fn cancel_open_input(&mut self) {
        self.open_input = None;
    }

    /// Enter GoTo input mode (the G key, Issue #166 §27).
    pub fn begin_goto_input(&mut self) {
        self.goto_input = Some(String::new());
    }

    /// Whether the GoTo input line is active.
    pub fn goto_input_active(&self) -> bool {
        self.goto_input.is_some()
    }

    /// The line's current content, while editing.
    pub fn goto_input(&self) -> Option<&str> {
        self.goto_input.as_deref()
    }

    pub fn goto_input_push(&mut self, c: char) {
        if let Some(line) = &mut self.goto_input {
            line.push(c);
        }
    }

    pub fn goto_input_backspace(&mut self) {
        if let Some(line) = &mut self.goto_input {
            line.pop();
        }
    }

    /// Confirm the line. The token is read by the EXISTING
    /// [`crate::cli::parse_seek_time`] reader — the shell's one time
    /// grammar, shared with the scriptable transport, so no second seek
    /// syntax exists. An empty line cancels; an unreadable token leaves
    /// the line OPEN and reports a bounded diagnostic instead of sending
    /// anything.
    pub fn confirm_goto_input(&mut self) -> GotoConfirm {
        let Some(line) = self.goto_input.clone() else {
            return GotoConfirm::Cancelled;
        };
        if line.is_empty() {
            self.goto_input = None;
            return GotoConfirm::Cancelled;
        }
        match crate::cli::parse_seek_time(&line) {
            Some(target) => {
                self.goto_input = None;
                GotoConfirm::Seek(target)
            }
            None => GotoConfirm::Unreadable("cannot read that time (try 95, 1:35 or 01:35.5)"),
        }
    }

    /// Leave GoTo input mode without seeking.
    pub fn cancel_goto_input(&mut self) {
        self.goto_input = None;
    }

    /// Toggle the keyboard-help overlay (the `?` key). Pure
    /// presentation state: open over anything, closed again by the
    /// same key or Esc.
    pub fn toggle_help(&mut self) {
        self.help_visible = !self.help_visible;
    }

    /// Close the help overlay if it is open (the Esc key). Idempotent.
    pub fn close_help(&mut self) {
        self.help_visible = false;
    }

    /// Whether the keyboard-help overlay is shown.
    pub fn help_visible(&self) -> bool {
        self.help_visible
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
    pub fn timeline_label(&self) -> String {
        crate::status::format_timeline(&self.observation)
    }

    /// The read-only progress bar (Issue #166 §33):
    /// `00:42 ━━━━━╸────────── 05:47`. `None` unless BOTH sides have
    /// evidence: an unknown duration has no percentage to draw and an
    /// unknown position is not a zero, so the bar simply does not
    /// appear — it is never fabricated, and it is never an input
    /// affordance (seeking stays keyboard-only).
    ///
    /// The fill is the position's fraction of the reported duration,
    /// clamped into the bar. The duration is mechanism evidence and the
    /// position an independent projection, so a position beyond the
    /// reported duration is representable; it clamps to a full bar
    /// rather than overflowing, which is the honest degradation of a
    /// display that cannot show "more than all of it".
    pub fn position_bar_label(&self) -> Option<String> {
        let rate = u64::from(self.observation.source_format?.sample_rate);
        if rate == 0 {
            return None;
        }
        let position_frames = self.observation.position?;
        let duration = self.observation.source_duration?;
        let duration_secs = duration.as_secs();
        let position_secs = position_frames / rate;

        let filled = if duration_secs == 0 {
            0
        } else {
            let width = BAR_WIDTH as u128;
            let filled = u128::from(position_secs) * width / u128::from(duration_secs);
            usize::try_from(filled.min(width)).unwrap_or(BAR_WIDTH)
        };
        let mut bar = String::with_capacity(BAR_WIDTH);
        for cell in 0..BAR_WIDTH {
            bar.push(match cell.cmp(&filled) {
                std::cmp::Ordering::Less => '━',
                std::cmp::Ordering::Equal => '╸',
                std::cmp::Ordering::Greater => '─',
            });
        }
        Some(format!(
            "{} {bar} {}",
            crate::status::format_clock(Duration::from_secs(position_secs)),
            crate::status::format_clock(Duration::from_secs(duration_secs))
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
}

/// What one key press means to the shell. `Stop`, `PauseResume` and the
/// seek keys are episode commands; `Open`, `GoTo`, `Help`, the playlist
/// keys (selection / order / repeat / play-selected) and `Quit` are
/// shell actions, not playback semantics — they are owned by the
/// runtime (they need the player and the input-line state;
/// [`apply_action`] routes EPISODE commands only), `Quit` is loop
/// control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Stop,
    /// Space: pause when pause intent is not recorded, resume when it
    /// is. One key, two commands — never a local `paused` bool.
    PauseResume,
    /// Left arrow: seek one [`SEEK_STEP`] earlier (D14.5).
    SeekBackward,
    /// Right arrow: seek one [`SEEK_STEP`] later (D14.5).
    SeekForward,
    /// Shift+Left: seek one [`LARGE_SEEK_STEP`] earlier (Issue #166
    /// §26). The same command, a larger step — no new seek semantics.
    SeekBackwardLarge,
    /// Shift+Right: seek one [`LARGE_SEEK_STEP`] later.
    SeekForwardLarge,
    /// O: begin the Open input line (D14.6). A shell action; the
    /// runtime performs the Open through the player.
    Open,
    /// G: begin the exact-seek input line (Issue #166 §27). A shell
    /// action; the runtime requests the parsed target through the SAME
    /// frozen seek command the arrows use.
    GoTo,
    /// `?`: toggle the keyboard-help overlay. A shell action; the
    /// runtime routes it to the model's presentation state.
    Help,
    /// Down arrow: move the UI selection one row later. Presentation
    /// only — it never plays (Issue #166 §18).
    SelectNext,
    /// Up arrow: move the UI selection one row earlier.
    SelectPrevious,
    /// Enter: play the SELECTED row through the same Open replacement
    /// (Issue #166 §19).
    PlaySelected,
    /// N: the manual Next traversal step (Issue #166 §35). A shell
    /// action like [`Action::Open`].
    Next,
    /// P: the manual Previous traversal step (Issue #166 §34).
    Previous,
    /// R: toggle Sequential ↔ Shuffle (Issue #166 §25).
    ToggleOrder,
    /// L: cycle Repeat Off → All → One → Off (Issue #166 §12).
    CycleRepeat,
    /// '+'/'=': raise the App's desired stream factor by one step
    /// (D14.9: step 5). A shell action like [`Action::Open`].
    VolumeUp,
    /// '-': lower the App's desired stream factor by one step.
    VolumeDown,
    Quit,
}

/// The fixed seek step the plain arrow keys request (D14.5). One product
/// decision, one constant — deliberately not a configuration surface.
pub const SEEK_STEP: Duration = Duration::from_secs(5);

/// The large seek step Shift+arrow requests (Issue #166 §26). The same
/// product decision at a larger scale: it routes the SAME seek command,
/// and the episode's own clamp/refusal contract decides the landing.
pub const LARGE_SEEK_STEP: Duration = Duration::from_secs(30);

/// The read-only progress bar's width in cells (Issue #166 §33).
pub const BAR_WIDTH: usize = 24;

/// The seek target one arrow key requests, derived from ONE coherent
/// observation of the episode: the position Projection (D14.8, source
/// PCM frames) converted with that same observation's published sample
/// rate (the stream runs at the source format, so frames and rate are
/// one unit world — the F4 negotiation rule). `None` means the arrow is
/// inert for this episode: with no position sample (or no rate to
/// convert it) there is no target to compute, and a seek with no
/// computable target is never SENT — no fabricated zero, no seek to the
/// episode start, no command at all.
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

/// One event-loop step after a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Continue,
    Exit,
}

/// The shell's whole keyboard grammar (Issue #166 §30, frozen).
///
/// ```text
/// Playlist     ↑ ↓ select   Enter play selected   N/P next/previous
///              R order      L repeat
/// Playback     Space pause/resume   ← → seek 5 s   Shift+← → seek 30 s
///              G exact seek   + - volume   S stop
/// Application  O open   ? help   Q / Ctrl+C quit   Esc cancel
/// ```
///
/// Anything else is presentation noise (including key-release events,
/// which Windows terminals emit). The plain and shift-keyed forms of a
/// LETTER both act (terminals disagree about reporting SHIFT); the
/// ARROWS are the one place where the two forms differ by design, so
/// they are matched on their exact modifier set — a chorded arrow stays
/// noise.
pub fn action_for_key(key: KeyEvent) -> Option<Action> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    let plain = key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT;
    match key.code {
        KeyCode::Char(' ') if plain => Some(Action::PauseResume),
        KeyCode::Char('s') | KeyCode::Char('S') if plain => Some(Action::Stop),
        KeyCode::Char('o') | KeyCode::Char('O') if plain => Some(Action::Open),
        KeyCode::Char('g') | KeyCode::Char('G') if plain => Some(Action::GoTo),
        KeyCode::Char('?') if plain => Some(Action::Help),
        KeyCode::Char('n') | KeyCode::Char('N') if plain => Some(Action::Next),
        KeyCode::Char('p') | KeyCode::Char('P') if plain => Some(Action::Previous),
        KeyCode::Char('r') | KeyCode::Char('R') if plain => Some(Action::ToggleOrder),
        KeyCode::Char('l') | KeyCode::Char('L') if plain => Some(Action::CycleRepeat),
        KeyCode::Char('+') | KeyCode::Char('=') if plain => Some(Action::VolumeUp),
        KeyCode::Char('-') | KeyCode::Char('_') if plain => Some(Action::VolumeDown),
        KeyCode::Char('q') | KeyCode::Char('Q') if plain => Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Up if plain => Some(Action::SelectPrevious),
        KeyCode::Down if plain => Some(Action::SelectNext),
        KeyCode::Enter if plain => Some(Action::PlaySelected),
        KeyCode::Left if key.modifiers.is_empty() => Some(Action::SeekBackward),
        KeyCode::Right if key.modifiers.is_empty() => Some(Action::SeekForward),
        KeyCode::Left if key.modifiers == KeyModifiers::SHIFT => Some(Action::SeekBackwardLarge),
        KeyCode::Right if key.modifiers == KeyModifiers::SHIFT => Some(Action::SeekForwardLarge),
        _ => None,
    }
}

/// The event loop's entire reaction to one EPISODE-COMMAND key press,
/// factored out of [`super::runtime::run`] so the key → seam wiring is
/// testable without a terminal. Space routes to the pause/resume seams:
/// which of the two commands is sent comes from a FRESH authoritative
/// observation of the episode's pause-intent command state — the shell
/// never keeps a local `paused` bool. The four seek actions route a
/// fixed-step seek (D14.5 + Issue #166 §26: the same command at 5 s and
/// 30 s): the target is derived from one fresh coherent observation, and
/// an episode whose position is unknown gets NO command at all. S routes
/// to `request_stop`; all of these are idempotent, valid before and
/// after the terminal Fact. Q exits the loop without touching the
/// episode. The shell actions are routed by the runtime itself (input
/// lines, player navigation/policy, overlay state) and must not arrive
/// here.
pub fn apply_action(action: Action, handle: &PlaybackSessionHandle) -> Step {
    match action {
        Action::Stop => {
            handle.request_stop();
            Step::Continue
        }
        Action::PauseResume => {
            if handle.observe().pause_requested {
                handle.request_resume();
            } else {
                handle.request_pause();
            }
            Step::Continue
        }
        Action::SeekBackward | Action::SeekForward => {
            let step = SEEK_STEP;
            if let Some(target) =
                seek_target(&handle.observe(), step, action == Action::SeekForward)
            {
                handle.request_seek(target);
            }
            Step::Continue
        }
        Action::SeekBackwardLarge | Action::SeekForwardLarge => {
            let step = LARGE_SEEK_STEP;
            if let Some(target) =
                seek_target(&handle.observe(), step, action == Action::SeekForwardLarge)
            {
                handle.request_seek(target);
            }
            Step::Continue
        }
        // The shell actions never reach this wiring: the runtime routes
        // Open/GoTo to the input lines, Help to the overlay state,
        // PlaySelected/Next/Previous to the player's navigation, the
        // selection keys to the playlist's presentation cursor, the
        // order/repeat keys to the playlist's policy, and the volume
        // keys to the player's desired level before any episode command
        // is considered. These arms exist so the match stays exhaustive;
        // they must not touch the episode.
        Action::Open
        | Action::GoTo
        | Action::Help
        | Action::SelectNext
        | Action::SelectPrevious
        | Action::PlaySelected
        | Action::Next
        | Action::Previous
        | Action::ToggleOrder
        | Action::CycleRepeat => Step::Continue,
        Action::VolumeUp | Action::VolumeDown => Step::Continue,
        Action::Quit => Step::Exit,
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
        }
    }

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

    #[test]
    fn s_maps_to_stop_and_q_maps_to_quit() {
        for key in ['s', 'S'] {
            assert_eq!(
                action_for_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)),
                Some(Action::Stop),
                "{key} must request stop"
            );
        }
        for key in ['q', 'Q'] {
            assert_eq!(
                action_for_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)),
                Some(Action::Quit),
                "{key} must quit"
            );
        }
        // Terminals disagree about reporting SHIFT with a letter.
        assert_eq!(
            action_for_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT)),
            Some(Action::Stop)
        );
    }

    #[test]
    fn ctrl_c_keeps_its_conventional_quit_meaning() {
        assert_eq!(
            action_for_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Action::Quit)
        );
    }

    #[test]
    fn any_other_key_is_presentation_noise() {
        for key in [
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            // Chords stay noise (except Ctrl+C) so e.g. Ctrl+S/Ctrl+Q
            // never act by accident.
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::ALT),
            KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        ] {
            assert_eq!(action_for_key(key), None, "{key:?} must be ignored");
        }
        // Key-release events (Windows terminals emit them) never act.
        assert_eq!(
            action_for_key(KeyEvent::new_with_kind(
                KeyCode::Char('s'),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            )),
            None
        );
    }

    /// The arrow grammar (F5 + Issue #166 §26): Left/Right seek one
    /// small step, Shift+Left/Shift+Right one large step. This is the
    /// ONE place where the shift-keyed form deliberately differs, so the
    /// match is on the exact modifier set — a chorded arrow stays noise,
    /// and a release event never acts.
    #[test]
    fn arrows_map_to_the_small_and_large_seek_actions() {
        for (key, small, large) in [
            (
                KeyCode::Left,
                Action::SeekBackward,
                Action::SeekBackwardLarge,
            ),
            (
                KeyCode::Right,
                Action::SeekForward,
                Action::SeekForwardLarge,
            ),
        ] {
            assert_eq!(
                action_for_key(KeyEvent::new(key, KeyModifiers::NONE)),
                Some(small)
            );
            assert_eq!(
                action_for_key(KeyEvent::new(key, KeyModifiers::SHIFT)),
                Some(large),
                "the shift-keyed arrow is the LARGE step"
            );
            assert_eq!(
                action_for_key(KeyEvent::new_with_kind(
                    key,
                    KeyModifiers::NONE,
                    KeyEventKind::Release
                )),
                None,
                "release events never act"
            );
            for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
                assert_eq!(
                    action_for_key(KeyEvent::new(key, modifiers)),
                    None,
                    "chorded arrows stay noise"
                );
            }
        }
    }

    /// The playlist keys (Issue #166 §30): ↑/↓ move the selection,
    /// Enter plays it, N/P navigate, R toggles the order, L cycles the
    /// repeat mode — each mapped once, on press, in its plain and
    /// shift-keyed letter form.
    #[test]
    fn the_playlist_keys_map_to_their_actions() {
        for (key, action) in [
            (KeyCode::Up, Action::SelectPrevious),
            (KeyCode::Down, Action::SelectNext),
            (KeyCode::Enter, Action::PlaySelected),
        ] {
            assert_eq!(
                action_for_key(KeyEvent::new(key, KeyModifiers::NONE)),
                Some(action)
            );
            assert_eq!(
                action_for_key(KeyEvent::new(key, KeyModifiers::SHIFT)),
                Some(action),
                "a shift-keyed form of the same key acts too"
            );
            assert_eq!(
                action_for_key(KeyEvent::new_with_kind(
                    key,
                    KeyModifiers::NONE,
                    KeyEventKind::Release
                )),
                None,
                "release events never act"
            );
        }
        for (key, action) in [
            ('r', Action::ToggleOrder),
            ('l', Action::CycleRepeat),
            ('g', Action::GoTo),
        ] {
            for code in [KeyCode::Char(key), KeyCode::Char(key.to_ascii_uppercase())] {
                assert_eq!(
                    action_for_key(KeyEvent::new(code, KeyModifiers::NONE)),
                    Some(action),
                    "{code:?}"
                );
            }
            assert_eq!(
                action_for_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::SHIFT)),
                Some(action)
            );
        }
    }

    /// The three seek actions never touch the episode when its position
    /// (or rate) is unknown, and the large step really is 30 s of media
    /// time around the same observed position.
    #[test]
    fn the_large_seek_step_is_thirty_seconds_of_the_same_observation() {
        let observation =
            |position: Option<u64>, sample_rate: Option<u32>| PlaybackSessionObservation {
                position,
                source_format: sample_rate.map(|sample_rate| PcmFormat {
                    sample_rate,
                    channels: 2,
                    channel_mask: 0x3,
                }),
                ..pending()
            };
        let at_42s = observation(Some(44_100 * 42), Some(44_100));
        assert_eq!(
            seek_target(&at_42s, LARGE_SEEK_STEP, true),
            Some(Duration::from_secs(72))
        );
        assert_eq!(
            seek_target(&at_42s, LARGE_SEEK_STEP, false),
            Some(Duration::from_secs(12))
        );
        // Both steps are inert without evidence: no fabricated target.
        let blind = observation(None, Some(44_100));
        assert_eq!(seek_target(&blind, LARGE_SEEK_STEP, true), None);
        assert_eq!(seek_target(&blind, SEEK_STEP, true), None);
        // …and less than a large step from the start saturates at zero.
        let at_5s = observation(Some(44_100 * 5), Some(44_100));
        assert_eq!(
            seek_target(&at_5s, LARGE_SEEK_STEP, false),
            Some(Duration::ZERO)
        );
    }

    /// Every large-seek key routes through the SAME request_seek seam as
    /// the small ones — one seek command, two step sizes, and no
    /// command at all without a computable target.
    #[test]
    fn the_large_seek_actions_route_through_the_same_request_seek_seam() {
        let handle = PlaybackSessionHandle::new();
        let before = handle.observe();
        for action in [Action::SeekBackwardLarge, Action::SeekForwardLarge] {
            assert_eq!(apply_action(action, &handle), Step::Continue);
        }
        assert_eq!(
            handle.observe(),
            before,
            "no position evidence: no seek command is sent"
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
        assert_eq!(seek_target(&no_position, SEEK_STEP, true), None);
        assert_eq!(seek_target(&no_position, SEEK_STEP, false), None);
        // No published rate: no unit to convert with, inert.
        let no_format = observation_with(Some(100), None);
        assert_eq!(seek_target(&no_format, SEEK_STEP, true), None);

        // 42 s at 44.1 kHz: the step is exactly five seconds of media
        // time, and the backward step saturates at zero (Duration is
        // non-negative by type).
        let at_42s = observation_with(Some(44_100 * 42), Some(44_100));
        assert_eq!(
            seek_target(&at_42s, SEEK_STEP, true),
            Some(Duration::from_secs(47))
        );
        assert_eq!(
            seek_target(&at_42s, SEEK_STEP, false),
            Some(Duration::from_secs(37))
        );
        let at_2s = observation_with(Some(2 * 44_100), Some(44_100));
        assert_eq!(
            seek_target(&at_2s, SEEK_STEP, false),
            Some(Duration::from_secs(0)),
            "before zero the step saturates at the episode start"
        );

        // The conversion uses the observation's own rate: 42 s at
        // 48 kHz is the same media time from different frames.
        let at_48k = observation_with(Some(48_000 * 42), Some(48_000));
        assert_eq!(
            seek_target(&at_48k, SEEK_STEP, true),
            Some(Duration::from_secs(47))
        );
    }

    /// An episode whose position is unknown gets NO seek command: the
    /// arrows change nothing (and panic on nothing).
    #[test]
    fn the_seek_actions_change_nothing_when_the_position_is_unknown() {
        let handle = PlaybackSessionHandle::new();
        let before = handle.observe();
        assert_eq!(apply_action(Action::SeekForward, &handle), Step::Continue);
        assert_eq!(apply_action(Action::SeekBackward, &handle), Step::Continue);
        assert_eq!(handle.observe(), before);
    }

    /// The stop key maps to the EXISTING request_stop seam — the same
    /// frozen right the machine transport uses — and quit never
    /// touches the episode.
    #[test]
    fn the_stop_action_routes_through_the_request_stop_seam_only() {
        let handle = PlaybackSessionHandle::new();
        assert!(!handle.observe().stop_requested);

        assert_eq!(apply_action(Action::Stop, &handle), Step::Continue);
        assert!(
            handle.observe().stop_requested,
            "S must record stop intent through the seam"
        );
        // Idempotent: pressing S again stays a plain seam call.
        assert_eq!(apply_action(Action::Stop, &handle), Step::Continue);
        assert!(handle.observe().stop_requested);

        // Quit is loop control, not a playback command: no new state.
        let before = handle.observe();
        assert_eq!(apply_action(Action::Quit, &handle), Step::Exit);
        assert_eq!(handle.observe(), before);
    }

    #[test]
    fn space_maps_to_the_pause_resume_toggle_and_no_other_key_does() {
        // Same posture as the letters: terminals disagree about
        // reporting SHIFT, so a shift-keyed space acts too.
        for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
            assert_eq!(
                action_for_key(KeyEvent::new(KeyCode::Char(' '), modifiers)),
                Some(Action::PauseResume)
            );
        }
        assert_eq!(
            action_for_key(KeyEvent::new_with_kind(
                KeyCode::Char(' '),
                KeyModifiers::NONE,
                KeyEventKind::Release
            )),
            None
        );
    }

    /// Space never keeps a local paused bool: the first press records
    /// pause intent through the seam, the next press releases it, and
    /// the choice between the two commands is read from a fresh
    /// authoritative observation each time.
    #[test]
    fn the_pause_resume_action_routes_through_the_seam_both_ways() {
        let handle = PlaybackSessionHandle::new();
        assert!(!handle.observe().pause_requested);
        assert!(!handle.observe().paused());

        assert_eq!(apply_action(Action::PauseResume, &handle), Step::Continue);
        assert!(
            handle.observe().pause_requested,
            "first Space must record pause intent through the seam"
        );
        // Idempotent command state: repeated presses while paused stay
        // recorded intent, and the second press resumes.
        assert_eq!(apply_action(Action::PauseResume, &handle), Step::Continue);
        assert!(
            !handle.observe().pause_requested,
            "second Space must release the pause through the seam"
        );
        // Resuming without a prior pause still goes through the seam
        // (inert intent history): the observation derives everything.
        assert_eq!(apply_action(Action::PauseResume, &handle), Step::Continue);
        assert!(handle.observe().pause_requested);
    }

    /// The Open key maps to the shell action (plain and shift-keyed),
    /// and apply_action must never let it touch the episode: the
    /// runtime owns it.
    #[test]
    fn o_maps_to_the_shell_open_action_and_apply_action_never_touches_the_episode() {
        for key in ['o', 'O'] {
            assert_eq!(
                action_for_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)),
                Some(Action::Open),
                "{key} must begin the Open input line"
            );
        }
        assert_eq!(
            action_for_key(KeyEvent::new(KeyCode::Char('O'), KeyModifiers::SHIFT)),
            Some(Action::Open)
        );

        let handle = PlaybackSessionHandle::new();
        let before = handle.observe();
        assert_eq!(apply_action(Action::Open, &handle), Step::Continue);
        assert_eq!(handle.observe(), before, "Open is not an episode command");
    }

    /// The N/P keys map to the two navigation actions (plain and
    /// shift-keyed), and apply_action must never let them touch the
    /// episode: like Open, they are shell actions the runtime routes.
    #[test]
    fn n_and_p_map_to_the_navigation_actions_and_apply_action_never_touches_the_episode() {
        for (key, action) in [('n', Action::Next), ('p', Action::Previous)] {
            assert_eq!(
                action_for_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)),
                Some(action)
            );
            assert_eq!(
                action_for_key(KeyEvent::new(
                    KeyCode::Char(key.to_ascii_uppercase()),
                    KeyModifiers::SHIFT
                )),
                Some(action)
            );
        }

        let handle = PlaybackSessionHandle::new();
        let before = handle.observe();
        assert_eq!(apply_action(Action::Next, &handle), Step::Continue);
        assert_eq!(apply_action(Action::Previous, &handle), Step::Continue);
        assert_eq!(
            handle.observe(),
            before,
            "navigation is not an episode command"
        );
    }

    /// The '+'/'=' and '-'/'_' keys map to the two volume actions
    /// (D14.9: step 5 at the shell), and apply_action must never let
    /// them touch the episode.
    #[test]
    fn volume_keys_map_to_the_volume_actions_and_apply_action_never_touches_the_episode() {
        assert_eq!(
            action_for_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE)),
            Some(Action::VolumeUp)
        );
        assert_eq!(
            action_for_key(KeyEvent::new(KeyCode::Char('='), KeyModifiers::NONE)),
            Some(Action::VolumeUp)
        );
        assert_eq!(
            action_for_key(KeyEvent::new(KeyCode::Char('-'), KeyModifiers::NONE)),
            Some(Action::VolumeDown)
        );
        assert_eq!(
            action_for_key(KeyEvent::new(KeyCode::Char('_'), KeyModifiers::NONE)),
            Some(Action::VolumeDown)
        );

        let handle = PlaybackSessionHandle::new();
        let before = handle.observe();
        assert_eq!(apply_action(Action::VolumeUp, &handle), Step::Continue);
        assert_eq!(apply_action(Action::VolumeDown, &handle), Step::Continue);
        assert_eq!(handle.observe(), before, "volume is not an episode command");
    }

    /// The `?` key maps to the help action (terminals deliver `?` with
    /// SHIFT on most layouts; both postures act), apply_action never
    /// lets it touch the episode, and the overlay state toggles and
    /// closes idempotently.
    #[test]
    fn question_mark_maps_to_help_and_the_overlay_toggles() {
        for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
            assert_eq!(
                action_for_key(KeyEvent::new(KeyCode::Char('?'), modifiers)),
                Some(Action::Help)
            );
        }
        // Release events never act, like every other key.
        assert_eq!(
            action_for_key(KeyEvent::new_with_kind(
                KeyCode::Char('?'),
                KeyModifiers::NONE,
                KeyEventKind::Release
            )),
            None
        );

        let handle = PlaybackSessionHandle::new();
        let before = handle.observe();
        assert_eq!(apply_action(Action::Help, &handle), Step::Continue);
        assert_eq!(handle.observe(), before, "help is not an episode command");

        let mut model = TuiModel::new("song.flac");
        assert!(!model.help_visible());
        model.toggle_help();
        assert!(model.help_visible());
        model.toggle_help();
        assert!(!model.help_visible());
        // Esc-close is idempotent and independent of the input line.
        model.close_help();
        assert!(!model.help_visible());
        model.toggle_help();
        model.close_help();
        assert!(!model.help_visible());
    }

    /// The GoTo line (Issue #166 §27): begin → edit → Enter returns the
    /// parsed target and closes the line. The token reader is the
    /// EXISTING shared `parse_seek_time`, so every spelling it accepts
    /// is accepted here and nothing else is.
    #[test]
    fn the_goto_line_parses_with_the_shared_reader_and_closes() {
        let mut model = TuiModel::new("song.flac");
        assert!(!model.goto_input_active());
        assert_eq!(model.goto_input(), None);

        model.begin_goto_input();
        assert!(model.goto_input_active());
        for c in "01:35.5".chars() {
            model.goto_input_push(c);
        }
        assert_eq!(model.goto_input(), Some("01:35.5"));
        model.goto_input_backspace();
        assert_eq!(model.goto_input(), Some("01:35."));
        model.goto_input_push('5');
        assert_eq!(
            model.confirm_goto_input(),
            GotoConfirm::Seek(Duration::from_millis(95_500)),
            "the shared reader owns the time grammar"
        );
        assert!(!model.goto_input_active());

        // Plain seconds and mm:ss read the same way.
        for (text, expected) in [
            ("95", Duration::from_secs(95)),
            ("1:35", Duration::from_secs(95)),
        ] {
            let mut model = TuiModel::new("song.flac");
            model.begin_goto_input();
            for c in text.chars() {
                model.goto_input_push(c);
            }
            assert_eq!(model.confirm_goto_input(), GotoConfirm::Seek(expected));
        }

        // An empty line is a cancel, and Esc leaves without a command.
        let mut model = TuiModel::new("song.flac");
        model.begin_goto_input();
        assert_eq!(model.confirm_goto_input(), GotoConfirm::Cancelled);
        assert!(!model.goto_input_active());
        model.begin_goto_input();
        model.goto_input_push('9');
        model.cancel_goto_input();
        assert_eq!(model.goto_input(), None);
    }

    /// An unreadable GoTo token never becomes a command: the line stays
    /// OPEN for correction and the shell reports a bounded diagnostic
    /// (Issue #166 §27).
    #[test]
    fn an_unreadable_goto_token_stays_open_and_sends_nothing() {
        for bad in ["abc", "1:99", "-30", "nan", "inf", "1e400"] {
            let mut model = TuiModel::new("song.flac");
            model.begin_goto_input();
            for c in bad.chars() {
                model.goto_input_push(c);
            }
            let GotoConfirm::Unreadable(diagnostic) = model.confirm_goto_input() else {
                panic!("{bad:?} must not parse into a seek target");
            };
            assert!(!diagnostic.is_empty());
            assert!(
                model.goto_input_active(),
                "{bad:?}: the line stays open for correction"
            );
            // The shell may then cancel it, and no target was produced.
            model.cancel_goto_input();
            assert!(!model.goto_input_active());
        }
    }

    /// The read-only progress bar (Issue #166 §33) appears only when
    /// BOTH sides have evidence, and never fabricates a percentage for
    /// an unknown duration or a zero for an unknown position.
    #[test]
    fn the_progress_bar_needs_both_sides_and_never_fabricates_one() {
        let model_with = |position: Option<u64>, duration: Option<u64>| {
            let mut model = TuiModel::new("song.flac");
            model.update(PlaybackSessionObservation {
                source_format: Some(PcmFormat {
                    sample_rate: 44_100,
                    channels: 2,
                    channel_mask: 0x3,
                }),
                position,
                source_duration: duration.map(Duration::from_secs),
                ..pending()
            });
            model
        };

        // Unknown duration, and unknown position: no bar at all — the
        // Position line keeps reporting the honest `--:--` side.
        assert_eq!(
            model_with(Some(44_100 * 42), None).position_bar_label(),
            None
        );
        assert_eq!(model_with(None, Some(238)).position_bar_label(), None);
        assert_eq!(model_with(None, None).position_bar_label(), None);
        assert_eq!(
            model_with(Some(44_100 * 42), Some(238)).timeline_label(),
            "00:42 / 03:58"
        );

        // Both known: the bar carries both times and the fill is the
        // position's fraction of the reported duration.
        let bar = model_with(Some(44_100 * 42), Some(238))
            .position_bar_label()
            .expect("both sides known");
        assert!(bar.starts_with("00:42 "), "{bar:?}");
        assert!(bar.ends_with(" 03:58"), "{bar:?}");
        assert_eq!(
            bar.chars().filter(|c| *c == '━').count(),
            (42 * BAR_WIDTH) / 238,
            "the filled cells are the position's fraction: {bar:?}"
        );
        assert!(bar.contains('╸'), "a head marks the current position");

        // At the start the bar is empty but still honest about both
        // times (a real zero position, unlike an unknown one).
        let start = model_with(Some(0), Some(238)).position_bar_label().unwrap();
        assert!(start.starts_with("00:00 "), "{start:?}");
        assert_eq!(start.chars().filter(|c| *c == '━').count(), 0);

        // A position beyond the reported duration is representable (the
        // two sides are independent evidence): it clamps to a full bar
        // rather than overflowing or panicking.
        let past_end = model_with(Some(44_100 * 999), Some(238))
            .position_bar_label()
            .unwrap();
        assert_eq!(past_end.chars().filter(|c| *c == '━').count(), BAR_WIDTH);
        assert!(!past_end.contains('─'));

        // A zero reported duration cannot divide: the bar degrades to
        // an empty one instead of panicking.
        let zero = model_with(Some(44_100 * 3), Some(0))
            .position_bar_label()
            .unwrap();
        assert_eq!(zero.chars().filter(|c| *c == '━').count(), 0);

        // No published rate: no unit to convert the frames with, so the
        // bar is absent exactly as the seek target is.
        let mut model = TuiModel::new("song.flac");
        model.update(PlaybackSessionObservation {
            position: Some(44_100 * 42),
            source_duration: Some(Duration::from_secs(238)),
            ..pending()
        });
        assert_eq!(model.position_bar_label(), None);
    }

    /// The pane's row labels: the file name when the path has one (CJK
    /// and spaces kept verbatim), the whole path otherwise (Issue #166
    /// §22). No metadata is read — the filename IS the title.
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

    /// The Windows product paths are the ones the physical gate uses:
    /// on the Windows build a drive path's label is its file name (the
    /// separator is platform-owned, so this is pinned where it is true).
    #[cfg(windows)]
    #[test]
    fn windows_drive_paths_label_by_their_file_name() {
        for (path, expected) in [
            (r"D:\Music\夜曲.flac", "夜曲.flac"),
            (r"D:\Music\Album 2\01 Intro.flac", "01 Intro.flac"),
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
        assert_eq!(model.playlist_revision, None);
        assert!(model.playlist().is_empty());

        model.set_playlist(7, || {
            vec![PlaylistRow {
                label: "first.flac".to_owned(),
                playing: true,
                selected: true,
            }]
        });
        assert_eq!(model.playlist_revision, Some(7));
        assert_eq!(model.playlist().len(), 1);

        // The same revision: the closure must not even run.
        model.set_playlist(7, || {
            panic!("an unchanged revision must not rebuild the rows")
        });

        model.set_playlist(8, Vec::new);
        assert_eq!(model.playlist_revision, Some(8));
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
        model.set_order(PlaybackOrder::Sequential);
        model.set_repeat(RepeatMode::Off);
        assert_eq!(model.order_label(), Some("Sequential"));
        assert_eq!(model.repeat_label(), Some("Off"));
    }

    /// The Open input line lifecycle: begin → edit → confirm returns
    /// the path and closes the line; an empty line and Esc are cancels;
    /// a second O restarts the line.
    #[test]
    fn the_open_input_line_edits_confirms_and_cancels() {
        let mut model = TuiModel::new("song.flac");
        assert!(!model.open_input_active());
        assert_eq!(model.open_input(), None);

        model.begin_open_input();
        assert!(model.open_input_active());
        assert_eq!(model.open_input(), Some(""));
        for c in "/media/b.flac".chars() {
            model.open_input_push(c);
        }
        assert_eq!(model.open_input(), Some("/media/b.flac"));
        model.open_input_backspace();
        assert_eq!(model.open_input(), Some("/media/b.fla"));

        assert_eq!(
            model.confirm_open_input(),
            Some("/media/b.fla".to_owned()),
            "confirm returns the candidate and closes the line"
        );
        assert!(!model.open_input_active());

        // An empty line confirms nothing — it is a cancel, never an
        // Open of "".
        model.begin_open_input();
        assert_eq!(model.confirm_open_input(), None);
        assert!(!model.open_input_active());

        // Esc cancels a non-empty line.
        model.begin_open_input();
        model.open_input_push('x');
        model.cancel_open_input();
        assert_eq!(model.open_input(), None);
    }

    /// The committed episode follows the player: a clean-failed Open
    /// leaves `None` (an honest no-episode state), a committed
    /// replacement moves the path.
    #[test]
    fn the_model_follows_the_player_committed_episode() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(model.source(), Some("song.flac"));
        model.set_episode(Some("/media/b.flac".to_owned()));
        assert_eq!(model.source(), Some("/media/b.flac"));
        model.set_episode(None);
        assert_eq!(model.source(), None, "no episode is a real F6 state");
    }

    /// A model that loses its episode drops the last observation with
    /// it: the retired episode's diagnostics must not leak into the
    /// frame as if they described anything current (review round-1
    /// MINOR-3).
    #[test]
    fn a_no_episode_model_drops_the_retired_episode_diagnostics() {
        let mut model = TuiModel::new("song.flac");
        model.update(PlaybackSessionObservation {
            failure_diagnostic: Some("decode: corrupt frame".to_owned()),
            ..pending()
        });
        assert!(!model.diagnostics().is_empty());
        model.set_episode(None);
        assert!(
            model.diagnostics().is_empty(),
            "no episode, no episode diagnostics: {:?}",
            model.diagnostics()
        );
    }

    /// The status line is plain presentation: recorded, read, replaced.
    #[test]
    fn the_status_line_records_open_operation_feedback() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(model.status(), None);
        model.set_status(Some("open refused: unsupported container".to_owned()));
        assert_eq!(model.status(), Some("open refused: unsupported container"));
        model.set_status(None);
        assert_eq!(model.status(), None);
    }

    /// The displayed Paused projection comes from the seam's frozen
    /// establishment conjunction, never from command state alone: an
    /// episode with recorded pause intent but no engaged+quiesced
    /// render evidence must not display Paused.
    #[test]
    fn the_paused_label_follows_the_establishment_conjunction_only() {
        let mut model = TuiModel::new("song.flac");
        assert!(!model.paused(), "fresh episode is not Paused");

        // Pause intent recorded, but no render engagement yet.
        model.update(PlaybackSessionObservation {
            pause_requested: true,
            ..pending()
        });
        assert!(!model.paused(), "intent alone is not Paused");

        // Engaged, but the output tail has not been observed quiesced —
        // including after a prior cycle's release was observed (the
        // D14.7 corrective negative oracle: stale cross-cycle evidence
        // satisfies nothing).
        model.update(PlaybackSessionObservation {
            pause_requested: true,
            pause_engagement: PauseEngagement::Engaged,
            ..pending()
        });
        assert!(
            !model.paused(),
            "engagement without CURRENT quiescence is not Paused"
        );

        // Full establishment.
        model.update(PlaybackSessionObservation {
            pause_requested: true,
            pause_engagement: PauseEngagement::TailQuiesced,
            ..pending()
        });
        assert!(model.paused(), "intent + engagement + quiescence is Paused");

        // A committed terminal outcome breaks establishment even with
        // the mechanism evidence still latched.
        model.update(PlaybackSessionObservation {
            terminal_outcome: Some(EpisodeTerminalOutcome::Stopped),
            pause_requested: true,
            pause_engagement: PauseEngagement::TailQuiesced,
            ..pending()
        });
        assert!(!model.paused(), "a settled episode is never Paused");
    }
}
