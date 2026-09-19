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

use super::model::{Action, Step, TuiModel, action_for_key, apply_action};
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

        refresh(&mut model, player);
    }

    guard.restore();
    Ok(())
}

/// One key press against the shell state. While the Open input line is
/// active it captures the editing keys (Enter performs the Open
/// through the player; Esc cancels; an empty line confirms nothing);
/// otherwise the frozen grammar applies, with the Open action routed
/// to the input line, the help action to the overlay state, and every
/// episode command routed through the player's committed seam.
fn handle_key<S: EpisodeStart>(
    key: crossterm::event::KeyEvent,
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
) -> Step {
    if key.kind != crossterm::event::KeyEventKind::Press {
        return Step::Continue;
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
            // The conventional quit keeps working from inside the line.
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Step::Exit;
            }
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                model.open_input_push(c)
            }
            _ => {}
        }
        return Step::Continue;
    }
    let Some(action) = action_for_key(key) else {
        // Esc outside the input line closes the help overlay when it is
        // open (the overlay's cancel affordance) and is noise otherwise.
        if key.code == KeyCode::Esc {
            model.close_help();
        }
        return Step::Continue;
    };
    match action {
        Action::Open => {
            model.begin_open_input();
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
        Action::Next => {
            perform_navigation(model, player, Navigation::Next);
            Step::Continue
        }
        Action::Previous => {
            perform_navigation(model, player, Navigation::Previous);
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

/// The D14.9 volume step: one key press, five points of the desired
/// stream factor. One product decision, one constant.
const VOLUME_STEP: i16 = 5;

/// Perform one navigation selection through the player (D14.6): the
/// SAME Open replacement, with the cursor moving only on commit. The
/// inert ends (no wrap) report honestly; every other outcome is the
/// Open outcome under the operation's name.
fn perform_navigation<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
    navigation: Navigation,
) {
    let (name, outcome) = match navigation {
        Navigation::Next => ("next", player.next_track()),
        Navigation::Previous => ("previous", player.previous_track()),
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

/// Perform the Open composition command for one user-supplied path —
/// file OR folder (U1, Issue #166 §10). The input expansion runs
/// BEFORE any destructive step: an empty/unreadable expansion refuses
/// here and the player is not touched at all (no episode destroyed, no
/// navigation state changed, diagnostic shown). A non-empty expansion
/// opens its FIRST candidate through the existing frozen replacement
/// and seeds the accepted list on commit — exactly the startup
/// discipline. The recorded outcome is the status line's feedback —
/// application composition feedback (D14.6), never a playback
/// semantic.
fn perform_open<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
    candidate: &Path,
) {
    let expansion = crate::input::expand_inputs([candidate]);
    let outcome = crate::input::open_expanded(player, &expansion);
    let extra = expansion.accepted.len() > 1 || expansion.skipped > 0;
    let feedback = match outcome {
        None => format!("open refused: {}", expansion.refusal()),
        Some(outcome) => match outcome {
            OpenOutcome::Opened => {
                let opened = expansion
                    .accepted
                    .first()
                    .map(|path| format!("opened {}", path.display()))
                    .unwrap_or_else(|| "opened".to_owned());
                if extra {
                    format!("{opened} ({})", expansion.summary())
                } else {
                    opened
                }
            }
            OpenOutcome::Refused { diagnostic } => format!("open refused: {diagnostic}"),
            OpenOutcome::ActivationFailedClean { diagnostic } => {
                format!("open failed (clean): {diagnostic}")
            }
            OpenOutcome::FailStop { diagnostic } => format!("FAIL-STOP: {diagnostic}"),
        },
    };
    model.set_status(Some(feedback));
}

/// Follow the player's committed episode: swap the source label when
/// the committed episode changed, and take the new episode's one pure
/// observation for this refresh.
fn refresh<S: EpisodeStart>(model: &mut TuiModel, player: &ReferencePlayerApp<S>) {
    model.set_episode(
        player
            .active_source()
            .map(|p| p.to_string_lossy().into_owned()),
    );
    model.set_navigation(player.navigation_position());
    model.set_volume(Some(player.desired_volume()));
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
