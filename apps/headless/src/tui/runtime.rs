//! The terminal session of the reference player: raw mode + alternate
//! screen under a small RAII guard, and a modest event loop.
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

use std::io::{self, Write};
use std::time::Duration;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use qianqian_playback::PlaybackSessionHandle;

use super::model::{Step, TuiModel, action_for_key, apply_action};
use super::view;

/// UI refresh cadence (~100–250 ms band).
pub const TICK: Duration = Duration::from_millis(150);

/// Run the reference-player shell over one episode handle until the
/// user quits. Restores the terminal on every exit path (normal quit,
/// I/O error, panic) before returning; the caller owns everything
/// episode-lifecycle related (stop request, terminal wait, dispose).
pub fn run(handle: &PlaybackSessionHandle, source: &str) -> Result<(), String> {
    let mut guard =
        TerminalGuard::acquire().map_err(|error| format!("terminal setup failed: {error}"))?;
    install_panic_restore_hook();

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal =
        Terminal::new(backend).map_err(|error| format!("terminal setup failed: {error}"))?;

    let mut model = TuiModel::new(source);
    model.update(handle.observe());

    loop {
        terminal
            .draw(|frame| view::draw(frame, &model))
            .map_err(|error| format!("terminal draw failed: {error}"))?;

        if event::poll(TICK).map_err(|error| format!("terminal input failed: {error}"))? {
            match event::read().map_err(|error| format!("terminal input failed: {error}"))? {
                Event::Key(key) => {
                    if let Some(action) = action_for_key(key)
                        && matches!(apply_action(action, handle), Step::Exit)
                    {
                        break;
                    }
                }
                // The next draw picks up the new terminal size.
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        model.update(handle.observe());
    }

    guard.restore();
    Ok(())
}

/// Owns the entered terminal modes until the shell is done. Restore is
/// idempotent and runs through `Drop`, so every early return and the
/// unwinding of a panic inside the loop land back on the user's real
/// screen.
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
        self.restored = true;
        let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
        let _ = disable_raw_mode();
        let _ = io::stdout().flush();
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

/// A panic inside the draw/loop code must not strand raw mode or the
/// alternate screen: restore best-effort first, then let the default
/// hook report. Idempotent with [`TerminalGuard::restore`].
fn install_panic_restore_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
        default_hook(info);
    }));
}
