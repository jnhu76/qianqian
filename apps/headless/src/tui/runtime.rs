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
/// to the input line and every episode command routed through the
/// player's committed seam.
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
        return Step::Continue;
    };
    match action {
        Action::Open => {
            model.begin_open_input();
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
        // Episode commands route through the player's committed seam;
        // with no episode they are inert (there is nothing to command).
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

/// Perform the Open composition command through the player and record
/// the outcome as the status line's feedback — application composition
/// feedback (D14.6), never a playback semantic.
fn perform_open<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
    candidate: &Path,
) {
    let feedback = match player.open(candidate) {
        OpenOutcome::Opened => format!("opened {}", candidate.display()),
        OpenOutcome::Refused { diagnostic } => format!("open refused: {diagnostic}"),
        OpenOutcome::ActivationFailedClean { diagnostic } => {
            format!("open failed (clean): {diagnostic}")
        }
        OpenOutcome::FailStop { diagnostic } => format!("FAIL-STOP: {diagnostic}"),
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
