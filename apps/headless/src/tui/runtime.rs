//! The terminal session of the reference player: raw mode + alternate
//! screen under a small RAII guard, and a modest event loop.
//!
//! Terminal ownership is lexical: the guard on this function's stack is
//! the ONLY owner of the entered terminal modes, and unwinding (a panic
//! inside the draw/loop code) drops it like any other early return. The
//! module deliberately installs no process-global policy (no panic
//! hook): background threads panicking must not be able to tear down a
//! terminal session they do not own.
//!
//! Deliberately the ONLY place where crossterm I/O happens, and
//! deliberately ordinary: no event framework, no state machine, no
//! background threads. Per refresh the loop takes exactly one pure
//! `observe()` read and one `terminal.draw()`; it never blocks the
//! audio path (the episode's realtime work lives on session-owned
//! threads behind the seam, and this loop only polls terminal input
//! between draws). UI refresh cadence: the loop waits for input up to
//! [`TICK`], so a quiet terminal redraws about every 150 ms and a key
//! press is answered within the same budget.
//!
//! The one deliberate exception to non-blocking key handling is the
//! Open operation (ADR-PBK-002 D14.6): `ReferencePlayerApp::open` runs
//! the whole frozen replacement sequence synchronously on this thread
//! (repeated Open is App-thread-serialized), so the O key's Enter can
//! block for as long as the old episode needs to settle. That stall IS
//! the replacement being honest about its ordering — no async
//! machinery is earned in v1.

use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::player::{EpisodeStart, OpenOutcome, ReferencePlayerApp};

use super::model::{
    Action, GotoConfirm, PlaylistRow, Step, TuiModel, action_for_key, apply_action, row_label,
};
use super::view;

/// UI refresh cadence (~100–250 ms band).
pub const TICK: Duration = Duration::from_millis(150);

/// Run the reference-player shell over the player until the user
/// quits. Restores the terminal on every exit path that unwinds
/// through this frame (normal quit, I/O error, panic unwind) before
/// returning; the caller owns everything else (quit, disposal
/// reporting, exit codes). `initial_status` is presented as the first
/// Open-operation feedback line (e.g. the startup Open's outcome).
pub fn run<S: EpisodeStart>(
    player: &mut ReferencePlayerApp<S>,
    initial_status: Option<String>,
) -> Result<(), String> {
    let mut guard =
        TerminalGuard::acquire().map_err(|error| format!("terminal setup failed: {error}"))?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal =
        Terminal::new(backend).map_err(|error| format!("terminal setup failed: {error}"))?;

    let mut model = TuiModel::new(String::new());
    model.set_status(initial_status);
    refresh(&mut model, player);

    loop {
        terminal
            .draw(|frame| view::draw(frame, &model))
            .map_err(|error| format!("terminal draw failed: {error}"))?;

        if event::poll(TICK).map_err(|error| format!("terminal input failed: {error}"))? {
            match event::read().map_err(|error| format!("terminal input failed: {error}"))? {
                Event::Key(key) => {
                    if handle_key(key, &mut model, player) == Step::Exit {
                        break;
                    }
                }
                // The next draw picks up the new terminal size.
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        // The App's natural-EOF policy (Issue #166 §13): one D11
        // `Completed` Fact may advance the temporary playlist through
        // the SAME Open replacement. It runs AFTER input so a key press
        // and an automatic transition never race for the same refresh,
        // and it blocks exactly as the O key's Enter does (the
        // documented synchronous-Open stall) — no async machinery is
        // earned here either.
        if let Some(outcome) = player.poll_eof_policy() {
            model.set_status(Some(eof_feedback(&outcome, player)));
        }

        refresh(&mut model, player);
    }

    guard.restore();
    Ok(())
}

/// The operation feedback for one automatic EOF transition. Application
/// composition feedback under the operation's own name — never a
/// playback semantic, and never a claim about the sound.
fn eof_feedback<S: EpisodeStart>(outcome: &OpenOutcome, player: &ReferencePlayerApp<S>) -> String {
    match outcome {
        OpenOutcome::Opened => match player.active_source() {
            Some(source) => format!("auto-next: opened {}", source.display()),
            None => "auto-next: opened".to_owned(),
        },
        OpenOutcome::Refused { diagnostic } => format!("auto-next refused: {diagnostic}"),
        OpenOutcome::ActivationFailedClean { diagnostic } => {
            format!("auto-next failed (clean): {diagnostic}")
        }
        OpenOutcome::FailStop { diagnostic } => format!("FAIL-STOP: {diagnostic}"),
    }
}

/// Whether a key press carries no modifier, or only SHIFT (terminals
/// disagree about reporting SHIFT with a character, so both act for
/// every LETTER key).
fn plain(key: crossterm::event::KeyEvent) -> bool {
    key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT
}

/// One key press against the shell state, under the frozen input-mode
/// precedence (Issue #166 §28): Ctrl+C quits from EVERY mode, the Open
/// line captures the editing keys, then the GoTo line, then the help
/// overlay — which owns the keyboard entirely, so no playback key can
/// fire behind it — and only then the normal grammar. No key both edits
/// and executes.
fn handle_key<S: EpisodeStart>(
    key: crossterm::event::KeyEvent,
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
) -> Step {
    use crossterm::event::KeyEventKind;
    if key.kind != KeyEventKind::Press {
        return Step::Continue;
    }
    // The conventional quit is the ONE key that works everywhere,
    // including inside the Open line (that is why it is not part of any
    // modal's vocabulary).
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Step::Exit;
    }
    if model.open_input_active() {
        match key.code {
            KeyCode::Enter => {
                if let Some(candidate) = model.confirm_open_input() {
                    perform_open(model, player, Path::new(&candidate));
                }
            }
            KeyCode::Esc => model.cancel_open_input(),
            KeyCode::Backspace => model.open_input_backspace(),
            // The Q-drive-lesson (Issue #166 §29): inside the Open line
            // every character is a literal path character, `q`/`Q`
            // included — `Q:\Music` must stay typeable. Only Ctrl+C
            // (handled above) quits from here.
            KeyCode::Char(c) if plain(key) => model.open_input_push(c),
            _ => {}
        }
        return Step::Continue;
    }
    if model.goto_input_active() {
        match key.code {
            KeyCode::Enter => match model.confirm_goto_input() {
                GotoConfirm::Seek(target) => match player.active_handle() {
                    Some(handle) => {
                        // The SAME frozen seek command the arrows use
                        // (Issue #166 §27): the episode's own
                        // clamp/refusal contract decides the landing.
                        handle.request_seek(target);
                        model.set_status(Some(format!(
                            "seek requested: {}",
                            crate::status::format_clock(target)
                        )));
                    }
                    // Nothing to seek: the parsed target is dropped
                    // rather than sent into a nonexistent episode.
                    None => model.set_status(Some("seek: no episode".to_owned())),
                },
                GotoConfirm::Cancelled => {}
                GotoConfirm::Unreadable(diagnostic) => {
                    model.set_status(Some(diagnostic.to_owned()))
                }
            },
            KeyCode::Esc => model.cancel_goto_input(),
            KeyCode::Backspace => model.goto_input_backspace(),
            KeyCode::Char(c) if plain(key) => model.goto_input_push(c),
            _ => {}
        }
        return Step::Continue;
    }
    if model.help_visible() {
        // Help owns the keyboard: `?`/Esc close it, Q quits, and every
        // other key is noise. Playback keys therefore cannot fire
        // behind the overlay.
        match key.code {
            KeyCode::Char('?') if plain(key) => model.close_help(),
            KeyCode::Esc => model.close_help(),
            KeyCode::Char('q') | KeyCode::Char('Q') if plain(key) => return Step::Exit,
            _ => {}
        }
        return Step::Continue;
    }
    let Some(action) = action_for_key(key) else {
        return Step::Continue;
    };
    match action {
        Action::Open => {
            model.begin_open_input();
            Step::Continue
        }
        Action::GoTo => {
            model.begin_goto_input();
            Step::Continue
        }
        Action::Help => {
            model.toggle_help();
            Step::Continue
        }
        // Quit is loop control, never an episode command: it exits even
        // with no episode committed — the U1 idle shell must stay
        // quittable (previously unreachable: every session used to
        // START with a committed episode).
        Action::Quit => Step::Exit,
        // Selection is presentation: it moves the `>` cursor and
        // nothing else (Issue #166 §18).
        Action::SelectNext => {
            player.select_next_track();
            Step::Continue
        }
        Action::SelectPrevious => {
            player.select_previous_track();
            Step::Continue
        }
        Action::PlaySelected => {
            perform_play_selected(model, player);
            Step::Continue
        }
        Action::Next => {
            perform_navigation(model, player, Navigation::Next);
            Step::Continue
        }
        Action::Previous => {
            perform_navigation(model, player, Navigation::Previous);
            Step::Continue
        }
        Action::ToggleOrder => {
            let order = player.toggle_order();
            model.set_order(order);
            model.set_status(Some(format!("Order: {}", order.label())));
            Step::Continue
        }
        Action::CycleRepeat => {
            let repeat = player.cycle_repeat();
            model.set_repeat(repeat);
            model.set_status(Some(format!("Repeat: {}", repeat.label())));
            Step::Continue
        }
        Action::VolumeUp => {
            let volume = player.change_volume(VOLUME_STEP);
            model.set_status(Some(format!("volume {volume}/100 (desired)")));
            Step::Continue
        }
        Action::VolumeDown => {
            let volume = player.change_volume(-VOLUME_STEP);
            model.set_status(Some(format!("volume {volume}/100 (desired)")));
            Step::Continue
        }
        // Remaining episode commands route through the player's
        // committed seam; with no episode they are inert (there is
        // nothing to command).
        action => {
            if let Some(handle) = player.active_handle() {
                apply_action(action, handle)
            } else {
                Step::Continue
            }
        }
    }
}

/// Which navigation key was pressed.
enum Navigation {
    Next,
    Previous,
}

impl Navigation {
    /// The direction in the App's traversal order.
    fn forward(&self) -> bool {
        matches!(self, Self::Next)
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Next => "next",
            Self::Previous => "previous",
        }
    }
}

/// The D14.9 volume step: one key press, five points of the desired
/// stream factor. One product decision, one constant.
const VOLUME_STEP: i16 = 5;

/// Perform one MANUAL navigation through the player (Issue #166
/// §34/§35): the SAME Open replacement, with the committed cursor moving
/// only on commit. The inert ends (no wrap outside Repeat All) report
/// honestly; every other outcome is the Open outcome under the
/// operation's name.
fn perform_navigation<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
    navigation: Navigation,
) {
    let name = navigation.name();
    let outcome = if navigation.forward() {
        player.next_track()
    } else {
        player.previous_track()
    };
    let feedback = match outcome {
        None => format!("no {name} track"),
        Some(outcome) => match outcome {
            OpenOutcome::Opened => match player.active_source() {
                Some(source) => format!("{name}: opened {}", source.display()),
                None => format!("{name}: opened"),
            },
            OpenOutcome::Refused { diagnostic } => format!("{name} refused: {diagnostic}"),
            OpenOutcome::ActivationFailedClean { diagnostic } => {
                format!("{name} failed (clean): {diagnostic}")
            }
            OpenOutcome::FailStop { diagnostic } => format!("FAIL-STOP: {diagnostic}"),
        },
    };
    model.set_status(Some(feedback));
}

/// Play the SELECTED playlist row through the same Open replacement
/// (Issue #166 §19). The selection is presentation state the user
/// already moved; this key is the only thing that turns it into
/// playback, and only on commit evidence. One inert rule (field
/// round 3): Enter on the row that IS the unsettled live episode is
/// not a replay request — the frozen replacement would restart the
/// track from its head, so the shell refuses to re-invoke it and says
/// so instead (no probe, no teardown, nothing moves).
fn perform_play_selected<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
) {
    if player.selected_is_live_episode() {
        model.set_status(Some("already playing the selected track".to_owned()));
        return;
    }
    let feedback = match player.play_selected() {
        None => "nothing selected".to_owned(),
        Some(OpenOutcome::Opened) => match player.active_source() {
            Some(source) => format!("play: opened {}", source.display()),
            None => "play: opened".to_owned(),
        },
        Some(OpenOutcome::Refused { diagnostic }) => format!("play refused: {diagnostic}"),
        Some(OpenOutcome::ActivationFailedClean { diagnostic }) => {
            format!("play failed (clean): {diagnostic}")
        }
        Some(OpenOutcome::FailStop { diagnostic }) => format!("FAIL-STOP: {diagnostic}"),
    };
    model.set_status(Some(feedback));
}

/// Perform the Open composition command for one user-supplied path —
/// file OR folder (U1, Issue #166 §10). The input expansion runs
/// BEFORE any destructive step: an empty/unreadable expansion refuses
/// here and the player is not touched at all (no episode destroyed, no
/// navigation state changed, diagnostic shown). A non-empty expansion
/// opens its FIRST candidate through the existing frozen replacement
/// and seeds the accepted list on commit — exactly the startup
/// discipline. The recorded outcome is the status block's feedback —
/// application composition feedback (D14.6), never a playback
/// semantic; a partial traversal reports its bounded scan warnings
/// right under the opened line (U1 corrective REQUIRED-2), so a
/// partially unreadable folder never looks complete.
fn perform_open<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
    candidate: &Path,
) {
    let mut expansion = crate::input::expand_inputs([candidate]);
    let outcome = crate::input::open_expanded(player, &mut expansion);
    let feedback = match outcome {
        None => format!("open refused: {}", expansion.refusal()),
        Some(OpenOutcome::Opened) => expansion.opened_status(),
        Some(OpenOutcome::Refused { diagnostic }) => format!("open refused: {diagnostic}"),
        Some(OpenOutcome::ActivationFailedClean { diagnostic }) => {
            format!("open failed (clean): {diagnostic}")
        }
        Some(OpenOutcome::FailStop { diagnostic }) => format!("FAIL-STOP: {diagnostic}"),
    };
    model.set_status(Some(feedback));
}

/// Follow the player's committed episode and its playlist: swap the
/// source label when the committed episode changed, take the new
/// episode's one pure observation for this refresh, and rebuild the
/// playlist rows only when the App's playlist revision moved (a huge
/// playlist must not cost per-frame work).
fn refresh<S: EpisodeStart>(model: &mut TuiModel, player: &ReferencePlayerApp<S>) {
    model.set_episode(
        player
            .active_source()
            .map(|p| p.to_string_lossy().into_owned()),
    );
    model.set_navigation(player.navigation_position());
    model.set_volume(Some(player.desired_volume()));
    model.set_order(player.playlist_order());
    model.set_repeat(player.playlist_repeat());
    model.set_playlist(player.playlist_revision(), || {
        player
            .playlist_rows()
            .map(|row| PlaylistRow {
                label: row_label(row.path),
                playing: row.playing,
                selected: row.selected,
            })
            .collect()
    });
    if let Some(handle) = player.active_handle() {
        model.update(handle.observe());
    }
}

/// Owns the entered terminal modes until the shell is done. Restore is
/// best-effort (that is all terminal recovery can honestly promise):
/// the flag records whether a full pass succeeded, so a partially
/// failed restore is retried once more through `Drop`.
struct TerminalGuard {
    restored: bool,
}

impl TerminalGuard {
    fn acquire() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            // Do not strand raw mode because the screen enter failed.
            let _ = disable_raw_mode();
            return Err(error);
        }
        if let Err(error) = execute!(io::stdout(), Hide) {
            // A cursor-hide failure must not strand the already-entered
            // alternate screen.
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self { restored: false })
    }

    fn restore(&mut self) {
        if self.restored {
            return;
        }
        let restored = execute!(io::stdout(), LeaveAlternateScreen, Show).is_ok()
            && disable_raw_mode().is_ok()
            && io::stdout().flush().is_ok();
        self.restored = restored;
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

#[cfg(test)]
mod tests {
    //! Key-routing tests for the shell grammar over the SAME fake
    //! episode harness the player's C7 matrix uses (real kernel, real
    //! playback session, fake providers). Unlike the player matrix,
    //! these tests use REAL temporary files and folders for everything
    //! that crosses the U1 input expansion — that seam reads the
    //! filesystem, and faking it here would test nothing. The terminal
    //! itself stays fake-free by construction: `handle_key` is the
    //! loop's whole reaction to a key and needs no terminal.

    use std::fs;
    use std::path::{Path, PathBuf};

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::super::model::{Step, TuiModel};
    use super::*;
    use crate::player::tests::FakeEpisodeSource;
    use crate::playlist::{PlaybackOrder, RepeatMode};

    /// A fresh unique temporary directory for one test.
    struct TempTree(PathBuf);

    impl TempTree {
        fn new(name: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "qianqian-tui-test-{}-{}",
                name,
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&base);
            fs::create_dir_all(&base).expect("temp tree root");
            Self(base)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        /// A regular file the fake probe accepts (it refuses only
        /// paths containing "invalid").
        fn live_file(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("parent dir");
            }
            fs::write(&path, b"not audio; the fake decode reads nothing").expect("temp file");
            path
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_text(
        model: &mut TuiModel,
        player: &mut ReferencePlayerApp<FakeEpisodeSource>,
        text: &str,
    ) {
        for c in text.chars() {
            assert_eq!(
                handle_key(key(KeyCode::Char(c)), model, player),
                Step::Continue
            );
        }
    }

    /// The whole O flow from a model state: begin input, type, Enter.
    fn open_via_keys(
        model: &mut TuiModel,
        player: &mut ReferencePlayerApp<FakeEpisodeSource>,
        target: &Path,
    ) {
        assert_eq!(
            handle_key(key(KeyCode::Char('O')), model, player),
            Step::Continue
        );
        assert!(model.open_input_active());
        type_text(model, player, &target.to_string_lossy());
        assert_eq!(
            handle_key(key(KeyCode::Enter), model, player),
            Step::Continue
        );
    }

    /// U1 §16: the idle no-episode state stays truthful — nothing to
    /// command, nothing fabricated, and Q exits the loop.
    #[test]
    fn q_from_the_idle_state_exits_and_touches_nothing() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        assert_eq!(model.source(), None, "the idle start has no episode");

        assert_eq!(
            handle_key(key(KeyCode::Char('q')), &mut model, &mut player),
            Step::Exit
        );
        assert!(player.active_handle().is_none());
    }

    /// U1 §16: O from the no-episode state, typing a REAL file path,
    /// commits an episode through the same frozen replacement.
    #[test]
    fn o_from_no_episode_opens_a_real_file() {
        let tree = TempTree::new("o-file");
        let file = tree.live_file("song live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());

        open_via_keys(&mut model, &mut player, &file);
        refresh(&mut model, &player);

        assert_eq!(model.source(), Some(file.to_string_lossy().as_ref()));
        assert_eq!(
            model.status(),
            Some(format!("opened {}", file.display()).as_str()),
            "a single accepted file keeps the plain opened feedback"
        );
        let observation = player.active_handle().expect("committed").observe();
        assert_eq!(observation.activation_error, None);
        assert_eq!(observation.terminal_outcome, None);
    }

    /// U1 §16: a folder typed into the O line expands into the seeded
    /// list — first track opens, N reaches the next entry.
    #[test]
    fn o_folder_seeds_the_list_and_navigates() {
        let tree = TempTree::new("o-folder");
        let a = tree.live_file("album live-a.flac");
        let b = tree.live_file("album live-b.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());

        open_via_keys(&mut model, &mut player, tree.path());
        refresh(&mut model, &player);

        assert_eq!(model.source(), Some(a.to_string_lossy().as_ref()));
        assert_eq!(
            model.navigation_position(),
            Some((1, 2)),
            "the accepted folder candidates are the navigation state"
        );

        // N walks the SEEDED list through the same replacement.
        assert_eq!(
            handle_key(key(KeyCode::Char('N')), &mut model, &mut player),
            Step::Continue
        );
        refresh(&mut model, &player);
        assert_eq!(model.source(), Some(b.to_string_lossy().as_ref()));
        assert_eq!(model.navigation_position(), Some((2, 2)));
    }

    /// U1 §16: Esc cancels the Open line and opens nothing — not even
    /// a probe runs.
    #[test]
    fn esc_cancels_the_open_line_without_touching_the_player() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let tree = TempTree::new("esc");
        let file = tree.live_file("song.flac");
        let mut player = ReferencePlayerApp::new(source);
        let mut model = TuiModel::new(String::new());

        assert_eq!(
            handle_key(key(KeyCode::Char('O')), &mut model, &mut player),
            Step::Continue
        );
        type_text(&mut model, &mut player, &file.to_string_lossy());
        assert_eq!(
            handle_key(key(KeyCode::Esc), &mut model, &mut player),
            Step::Continue
        );

        assert!(!model.open_input_active());
        assert_eq!(model.status(), None);
        assert!(player.active_handle().is_none());
        assert!(log.lock().unwrap().is_empty(), "no Open was attempted");
    }

    /// U1 §16: an empty/unreadable folder refuses INSIDE the shell
    /// before any destructive step — the idle state survives intact
    /// (no episode, no playlist, no probe).
    #[test]
    fn a_bad_folder_open_from_idle_preserves_the_idle_state() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let tree = TempTree::new("bad-folder-idle");
        let missing = tree.path().join("does-not-exist");
        let mut player = ReferencePlayerApp::new(source);
        let mut model = TuiModel::new(String::new());

        open_via_keys(&mut model, &mut player, &missing);
        refresh(&mut model, &player);

        let status = model.status().expect("a diagnostic is shown");
        assert!(status.starts_with("open refused: "), "{status}");
        assert!(player.active_handle().is_none(), "still no episode");
        assert_eq!(model.navigation_position(), None, "no list was committed");
        assert!(
            log.lock().unwrap().is_empty(),
            "the refusal preceded the probe"
        );
    }

    /// U1 §16: a bad folder open WHILE PLAYING leaves live playback
    /// untouched — the expansion refusal happens before the frozen
    /// replacement can destroy anything.
    #[test]
    fn a_bad_folder_open_while_playing_leaves_playback_untouched() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let tree = TempTree::new("bad-folder-live");
        let file = tree.live_file("playing.flac");
        let mut player = ReferencePlayerApp::new(source);
        let handle = {
            let outcome = player.open(&file);
            assert_eq!(outcome, crate::player::OpenOutcome::Opened);
            player.active_handle().expect("committed").clone()
        };
        // The setup Open probed once; the refusal below must add
        // nothing to that log.
        let log_len_after_setup = log.lock().unwrap().len();
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        let empty_dir = tree.path().join("empty-班级");
        fs::create_dir_all(&empty_dir).expect("empty dir");
        open_via_keys(&mut model, &mut player, &empty_dir);
        refresh(&mut model, &player);

        let status = model.status().expect("a diagnostic is shown");
        assert!(status.starts_with("open refused: "), "{status}");
        assert_eq!(model.source(), Some(file.to_string_lossy().as_ref()));
        let observation = handle.observe();
        assert!(
            !observation.stop_requested,
            "the episode was never asked to stop"
        );
        assert_eq!(observation.terminal_outcome, None);
        assert_eq!(
            player.navigation_position(),
            Some((1, 1)),
            "the navigation state kept exactly what the committed Open gave it"
        );
        assert_eq!(
            log.lock().unwrap().len(),
            log_len_after_setup,
            "the refusal preceded the probe"
        );
    }

    /// U1 corrective REQUIRED-2: an O-open over a folder that only
    /// PARTIALLY enumerated still opens its first candidate — and the
    /// status block names the warnings instead of looking complete.
    /// (Unix permission simulation; the equivalent partial-failure
    /// presentation is pinned platform-free in the input module.)
    #[cfg(unix)]
    #[test]
    fn an_o_open_over_a_partially_unreadable_folder_reports_the_warnings() {
        use std::os::unix::fs::PermissionsExt;

        let tree = TempTree::new("o-partial");
        let file = tree.live_file("kept.flac");
        let locked = tree.path().join("locked");
        fs::create_dir_all(&locked).expect("locked dir");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("lock");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());

        open_via_keys(&mut model, &mut player, tree.path());
        refresh(&mut model, &player);
        // Restore FIRST so cleanup can remove the tree even on failure.
        let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o755));

        assert_eq!(model.source(), Some(file.to_string_lossy().as_ref()));
        let status = model.status().expect("a partial scan is reported");
        assert!(status.contains("; 1 scan warning"), "{status}");
        assert!(
            status.contains("\nscan warning: cannot read "),
            "the diagnostic itself is shown: {status}"
        );
        assert_eq!(
            player.navigation_position(),
            Some((1, 1)),
            "the commit-riding seed holds the found candidates"
        );
    }

    /// Field round 3, problem 1: Enter on the row that IS the
    /// unsettled live episode is INERT — no probe, no replacement, no
    /// restart from the head of the track, and the shell says so. The
    /// frozen replacement is a restart by construction; the shell must
    /// not re-invoke it for the row already playing.
    #[test]
    fn enter_on_the_live_selected_track_is_inert() {
        let tree = TempTree::new("enter-inert");
        let file = tree.live_file("live.flac");
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        let mut model = TuiModel::new(String::new());
        open_via_keys(&mut model, &mut player, &file);
        refresh(&mut model, &player);
        assert!(player.selected_is_live_episode(), "the committed row is live");
        let activations = || {
            log.lock()
                .expect("fixture log")
                .iter()
                .filter(|event| event.starts_with("activate "))
                .count()
        };
        let before = activations();
        let observation = player.active_handle().expect("committed").observe();

        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        refresh(&mut model, &player);

        assert_eq!(
            model.status(),
            Some("already playing the selected track"),
            "{:?}",
            model.status()
        );
        assert_eq!(
            activations(),
            before,
            "Enter must not re-open the live track"
        );
        assert_eq!(
            player
                .active_handle()
                .expect("still live")
                .observe()
                .terminal_outcome,
            observation.terminal_outcome,
            "the live episode was never disturbed"
        );
    }

    /// The inert rule ends where the episode settles: after the D11
    /// terminal is committed, the same Enter replays the row through
    /// the frozen replacement (a replay of a finished track is what
    /// Enter on it means).
    #[test]
    fn enter_on_a_settled_selected_track_replays() {
        let tree = TempTree::new("enter-replay");
        let file = tree.live_file("finite-00.flac");
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        let mut model = TuiModel::new(String::new());
        open_via_keys(&mut model, &mut player, &file);
        refresh(&mut model, &player);
        let handle = player.active_handle().expect("committed").clone();
        assert_eq!(
            handle.wait_terminal(),
            qianqian_playback::EpisodeTerminalOutcome::Completed
        );
        assert!(
            !player.selected_is_live_episode(),
            "a settled episode is not live: Enter replays it"
        );
        let activations = || {
            log.lock()
                .expect("fixture log")
                .iter()
                .filter(|event| event.starts_with("activate "))
                .count()
        };
        let before = activations();

        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        refresh(&mut model, &player);

        assert!(
            activations() > before,
            "the settled row was re-opened through the frozen replacement"
        );
        assert!(
            model.status().unwrap().starts_with("play: opened "),
            "{:?}",
            model.status()
        );
    }

    /// The playlist keys reach the App through the shell: ↑/↓ move only
    /// the selection, Enter plays the selected row, R and L move only
    /// the policy (Issue #166 §18/§19/§25).
    #[test]
    fn the_playlist_keys_route_to_the_player() {
        let tree = TempTree::new("playlist-keys");
        let files: Vec<PathBuf> = (0..3)
            .map(|n| tree.live_file(&format!("live-{n}.flac")))
            .collect();
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        let mut model = TuiModel::new(String::new());
        open_via_keys(&mut model, &mut player, tree.path());
        refresh(&mut model, &player);
        assert_eq!(model.navigation_position(), Some((1, 3)));
        let events_after_open = log.lock().unwrap().len();

        // ↓ / ↑ move the selection and NOTHING else.
        assert_eq!(
            handle_key(key(KeyCode::Down), &mut model, &mut player),
            Step::Continue
        );
        refresh(&mut model, &player);
        assert_eq!(player.playlist_selected_position(), Some(1));
        assert_eq!(
            model.navigation_position(),
            Some((1, 3)),
            "the committed cursor did not move"
        );
        assert_eq!(
            log.lock().unwrap().len(),
            events_after_open,
            "selection is presentation: no probe, no open"
        );

        // Enter plays the selected row.
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        refresh(&mut model, &player);
        assert_eq!(player.active_source(), Some(files[1].as_path()));
        assert_eq!(model.navigation_position(), Some((2, 3)));
        assert!(
            model.status().unwrap().starts_with("play: opened "),
            "{:?}",
            model.status()
        );

        // R and L move only the policy, and say so.
        assert_eq!(
            handle_key(key(KeyCode::Char('r')), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(player.playlist_order(), PlaybackOrder::Shuffle);
        assert_eq!(model.status(), Some("Order: Shuffle"));
        assert_eq!(model.order_label(), Some("Shuffle"));
        assert_eq!(
            handle_key(key(KeyCode::Char('L')), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(player.playlist_repeat(), RepeatMode::All);
        assert_eq!(model.status(), Some("Repeat: All"));

        // They never touched the episode.
        let handle = player.active_handle().expect("committed").clone();
        assert_eq!(handle.observe().terminal_outcome, None);
        assert_eq!(player.active_source(), Some(files[1].as_path()));
    }

    /// The frozen input-mode precedence (Issue #166 §28): the help
    /// overlay OWNS the keyboard, so no playback key can fire behind it,
    /// and the modal lines capture their editing keys — no key both
    /// edits and executes.
    #[test]
    fn no_key_fires_across_input_modes() {
        let tree = TempTree::new("modes");
        let file = tree.live_file("live-a.flac");
        let other = tree.live_file("live-b.flac");
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        let handle = {
            assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
            player.active_handle().expect("committed").clone()
        };
        // A TWO-entry playlist: with one entry, Enter/N behind the
        // overlay would be inert for reasons that have nothing to do
        // with mode precedence, and the test could not fail.
        player.establish_playlist(vec![file.clone(), other.clone()]);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        let activations = || {
            log.lock()
                .expect("fixture log")
                .iter()
                .filter(|event| event.starts_with("activate "))
                .count()
        };
        let settled_activations = activations();
        assert_eq!(player.playlist_order(), PlaybackOrder::Sequential);
        assert_eq!(player.playlist_repeat(), RepeatMode::Off);

        // Help owns the keyboard: Space, N, S and the arrows are noise.
        handle_key(key(KeyCode::Char('?')), &mut model, &mut player);
        assert!(model.help_visible());
        for code in [
            KeyCode::Char(' '),
            KeyCode::Char('n'),
            KeyCode::Char('s'),
            KeyCode::Char('r'),
            KeyCode::Char('l'),
            KeyCode::Down,
            KeyCode::Up,
            KeyCode::Enter,
            KeyCode::Left,
        ] {
            assert_eq!(
                handle_key(key(code), &mut model, &mut player),
                Step::Continue,
                "{code:?} must not act behind the help overlay"
            );
        }
        let observation = handle.observe();
        assert!(!observation.stop_requested, "S never reached the episode");
        assert!(
            !observation.pause_requested,
            "Space never reached it either"
        );
        assert_eq!(player.playlist_playing_position(), Some(0));
        assert_eq!(
            player.playlist_selected_position(),
            Some(0),
            "the selection did not move behind the overlay"
        );
        assert_eq!(
            player.playlist_order(),
            PlaybackOrder::Sequential,
            "R did not toggle the order behind the overlay"
        );
        assert_eq!(
            player.playlist_repeat(),
            RepeatMode::Off,
            "L did not cycle the repeat mode behind the overlay"
        );
        assert_eq!(
            activations(),
            settled_activations,
            "no episode was composed behind the overlay"
        );
        // `?` and Esc close it; Q quits.
        handle_key(key(KeyCode::Esc), &mut model, &mut player);
        assert!(!model.help_visible());
        handle_key(key(KeyCode::Char('?')), &mut model, &mut player);
        assert!(model.help_visible());
        handle_key(key(KeyCode::Char('?')), &mut model, &mut player);
        assert!(!model.help_visible());
        handle_key(key(KeyCode::Char('?')), &mut model, &mut player);
        assert_eq!(
            handle_key(key(KeyCode::Char('q')), &mut model, &mut player),
            Step::Exit,
            "Q quits from inside help"
        );
        // (The real loop would have exited there; the test closes the
        // overlay and carries on.)
        handle_key(key(KeyCode::Esc), &mut model, &mut player);
        assert!(!model.help_visible());

        // The GoTo line captures its editing keys: `n`, `s` and Space are
        // target characters there, never commands.
        handle_key(key(KeyCode::Char('g')), &mut model, &mut player);
        assert!(model.goto_input_active());
        for (code, expected) in [
            (KeyCode::Char('1'), "1"),
            (KeyCode::Char(':'), "1:"),
            (KeyCode::Char('3'), "1:3"),
            (KeyCode::Char('.'), "1:3."),
            (KeyCode::Backspace, "1:3"),
            (KeyCode::Char('n'), "1:3n"),
            (KeyCode::Char(' '), "1:3n "),
        ] {
            handle_key(key(code), &mut model, &mut player);
            assert_eq!(model.goto_input(), Some(expected), "{code:?}");
        }
        let observation = handle.observe();
        assert!(!observation.stop_requested && !observation.pause_requested);
        // Esc leaves without sending anything.
        handle_key(key(KeyCode::Esc), &mut model, &mut player);
        assert!(!model.goto_input_active());
        assert_eq!(
            handle.observe(),
            observation,
            "a cancelled GoTo sends no command"
        );
    }

    /// The Q-drive lesson survives every modal (Issue #166 §29): inside
    /// the Open line a `Q`/`q` is a literal path character — `Q:\Music`
    /// stays typeable — and only Ctrl+C quits from there.
    #[test]
    fn the_open_line_types_a_drive_letter_and_only_ctrl_c_quits() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        assert_eq!(
            handle_key(key(KeyCode::Char('O')), &mut model, &mut player),
            Step::Continue
        );
        assert!(model.open_input_active());
        for c in r"Q:\Music".chars() {
            assert_eq!(
                handle_key(key(KeyCode::Char(c)), &mut model, &mut player),
                Step::Continue,
                "{c:?} must be typed literally, not acted on"
            );
        }
        assert_eq!(model.open_input(), Some(r"Q:\Music"));
        // The GoTo line gets the same treatment for `q`.
        model.cancel_open_input();
        handle_key(key(KeyCode::Char('g')), &mut model, &mut player);
        handle_key(key(KeyCode::Char('q')), &mut model, &mut player);
        assert_eq!(model.goto_input(), Some("q"));
        assert_eq!(
            handle_key(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &mut model,
                &mut player
            ),
            Step::Exit,
            "Ctrl+C quits from inside a modal line"
        );
    }

    /// The GoTo line sends the parsed target through the SAME seek seam
    /// the arrows use, reports the request honestly, and sends nothing
    /// for an unreadable token (Issue #166 §27).
    #[test]
    fn the_goto_line_requests_a_seek_and_fails_closed() {
        let tree = TempTree::new("goto");
        let file = tree.live_file("live-a.flac");
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        let handle = {
            assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
            player.active_handle().expect("committed").clone()
        };
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        handle_key(key(KeyCode::Char('G')), &mut model, &mut player);
        assert!(model.goto_input_active());
        for c in "1:35".chars() {
            handle_key(key(KeyCode::Char(c)), &mut model, &mut player);
        }
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        assert!(!model.goto_input_active());
        assert_eq!(model.status(), Some("seek requested: 01:35"));
        // The shell's claim is that the parsed target became the SAME
        // D14.5 seek command the arrows issue. That is witnessed at the
        // DECODER (the fixture logs every seek request it receives), not
        // by the status line the same branch just wrote: the F2 read side
        // has no seek-pending / seek-complete state at all, so the shell
        // cannot own seek-completion truth (Issue #166 §0), and the
        // frozen provider verdict for this fixture is `RefusedUnchanged`.
        // The D14.5 seek is worker-owned and asynchronous by design, so
        // the witness is a bounded wait for the request to REACH the
        // decoder — not a sleep, and not the status line.
        let requested = wait_for_seek(&log, "seek 95000ms", Duration::from_secs(5));
        assert!(
            requested,
            "the typed 1:35 must become a 95 s provider seek request: {:?}",
            seek_requests(&log)
        );
        let observation = handle.observe();
        assert_eq!(observation.terminal_outcome, None);
        assert!(!observation.stop_requested && !observation.pause_requested);

        // An unreadable token stays in the line and sends nothing.
        handle_key(key(KeyCode::Char('G')), &mut model, &mut player);
        for c in "abc".chars() {
            handle_key(key(KeyCode::Char(c)), &mut model, &mut player);
        }
        let before = handle.observe();
        assert_eq!(before, observation, "the request left no read-side residue");
        handle_key(key(KeyCode::Enter), &mut model, &mut player);
        assert!(model.goto_input_active(), "the line stays open");
        assert!(model.status().unwrap().starts_with("cannot read that time"));
        assert_eq!(handle.observe(), before, "no malformed seek was sent");
        // …and the unreadable line added nothing on top of the one
        // legitimate request (a fully quiet window for a late arrival).
        assert!(
            !wait_for_seek(&log, "seek 0ms", Duration::from_millis(200)),
            "no empty seek was sent"
        );
        assert_eq!(
            seek_requests(&log),
            vec!["seek 95000ms".to_owned()],
            "the unreadable line added no seek request"
        );
    }

    /// Wait, bounded, for one seek request to reach the decoder.
    fn wait_for_seek(
        log: &std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        expected: &str,
        within: Duration,
    ) -> bool {
        let deadline = std::time::Instant::now() + within;
        loop {
            if seek_requests(log).iter().any(|event| event == expected) {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// The seek requests the episode's decoder actually received.
    fn seek_requests(log: &std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Vec<String> {
        log.lock()
            .expect("fixture log")
            .iter()
            .filter(|event| event.starts_with("seek "))
            .cloned()
            .collect()
    }

    /// An automatic EOF transition is visible to the shell exactly as a
    /// manual one: the model follows the App's committed episode and the
    /// feedback names the automatic step.
    #[test]
    fn an_auto_next_transition_reaches_the_shell() {
        let tree = TempTree::new("auto-next-shell");
        let first = tree.live_file("finite-00.flac");
        let second = tree.live_file("live-01.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&first), crate::player::OpenOutcome::Opened);
        player.establish_playlist(vec![first.clone(), second.clone()]);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        assert_eq!(model.navigation_position(), Some((1, 2)));

        // Drive the real D11 settlement, then the policy the loop runs.
        let handle = player.active_handle().expect("committed").clone();
        assert_eq!(
            handle.wait_terminal(),
            qianqian_playback::EpisodeTerminalOutcome::Completed
        );
        let outcome = player.poll_eof_policy().expect("the policy is due");
        model.set_status(Some(eof_feedback(&outcome, &player)));
        refresh(&mut model, &player);

        assert_eq!(model.source(), Some(second.to_string_lossy().as_ref()));
        assert_eq!(model.navigation_position(), Some((2, 2)));
        assert!(
            model.status().unwrap().starts_with("auto-next: opened "),
            "{:?}",
            model.status()
        );
        // The pane followed: the second row is the committed one.
        let pane = model.playlist();
        assert_eq!(pane.len(), 2);
        assert!(pane[1].playing && pane[1].selected);
        assert!(!pane[0].playing);
    }

    /// The help overlay toggles from the plain grammar and Esc closes
    /// it; neither touches a live episode.
    #[test]
    fn help_toggles_and_esc_closes_without_touching_the_episode() {
        let tree = TempTree::new("help");
        let file = tree.live_file("song.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let handle = {
            assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
            player.active_handle().expect("committed").clone()
        };
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        assert_eq!(
            handle_key(key(KeyCode::Char('?')), &mut model, &mut player),
            Step::Continue
        );
        assert!(model.help_visible());
        assert_eq!(
            handle_key(key(KeyCode::Esc), &mut model, &mut player),
            Step::Continue
        );
        assert!(!model.help_visible());
        // Esc with the overlay closed is noise.
        assert_eq!(
            handle_key(key(KeyCode::Esc), &mut model, &mut player),
            Step::Continue
        );
        assert!(!model.help_visible());

        let observation = handle.observe();
        assert_eq!(observation.terminal_outcome, None);
        assert!(!observation.stop_requested);
    }
}
