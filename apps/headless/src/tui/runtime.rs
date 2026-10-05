//! The terminal session of the reference player: raw mode + alternate
//! screen + mouse capture under a small RAII guard, and a modest event
//! loop.
//!
//! Terminal ownership is lexical: the guard on this function's stack is
//! the ONLY owner of the entered terminal modes, and unwinding (a panic
//! inside the draw/loop code) drops it like any other early return. The
//! module deliberately installs no process-global policy (no panic
//! hook): background threads panicking must not be able to tear down a
//! terminal session they do not own. Mouse capture is entered and left
//! by the SAME guard lifecycle (§16/§40) — the enter/restore command
//! sequences are factored over any writer so the Enable/Disable pairing
//! is pinned by a test without a real terminal.
//!
//! Deliberately the ONLY place where crossterm I/O happens, and
//! deliberately ordinary: no event framework, no state machine, no
//! background threads. Per refresh the loop takes exactly one pure
//! `observe()` read and one `terminal.draw()`; it never blocks the
//! audio path (the episode's realtime work lives on session-owned
//! threads behind the seam, and this loop only polls terminal input
//! between draws). UI refresh cadence: the loop waits for input up to
//! [`TICK`], so a quiet terminal redraws about every 150 ms and an
//! input event is answered within the same budget.
//!
//! The one deliberate exception to non-blocking input handling is the
//! Open operation (ADR-PBK-002 D14.6): `ReferencePlayerApp::open` runs
//! the whole frozen replacement sequence synchronously on this thread
//! (repeated Open is App-thread-serialized), so the Open modal's Enter
//! can block for as long as the old episode needs to settle. That stall
//! IS the replacement being honest about its ordering — no async
//! machinery is earned in v1.
//!
//! Every input event flows through the same pipe (§8's one dispatch
//! boundary): decode (model) → at most one [`TuiAction`] →
//! [`dispatch`] — which performs at most one product operation
//! (§31). Resize invalidates the frame's geometry and lets the next
//! draw republish it (§29).

use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::player::{EpisodeStart, OpenOutcome, ReferencePlayerApp};

use super::model::{
    ModalConfirm, ModalInput, PlaylistCursor, Step, TuiAction, TuiModel, decode_key, decode_mouse,
    seek_target,
};
use super::view;

/// UI refresh cadence (~100–250 ms band).
pub const TICK: Duration = Duration::from_millis(150);

/// Run the reference-player shell over the player until the user
/// quits. Restores the terminal on every exit path that unwinds
/// through this frame (normal quit, I/O error, panic unwind) before
/// returning; the caller owns everything else (quit, disposal
/// reporting, exit codes). `initial_status` is presented as the first
/// operation feedback line (e.g. the startup Open's outcome).
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
            .draw(|frame| view::draw(frame, &mut model))
            .map_err(|error| format!("terminal draw failed: {error}"))?;

        if event::poll(TICK).map_err(|error| format!("terminal input failed: {error}"))? {
            let step =
                match event::read().map_err(|error| format!("terminal input failed: {error}"))? {
                    // One physical event decodes to at most one action, and
                    // one action dispatches to at most one product
                    // operation (§31) — key and mouse share the boundary.
                    Event::Key(key) => decode_key(key, &model).map_or(Step::Continue, |action| {
                        dispatch(action, &mut model, player)
                    }),
                    Event::Mouse(mouse) => decode_mouse(mouse, &mut model)
                        .map_or(Step::Continue, |action| {
                            dispatch(action, &mut model, player)
                        }),
                    // §29: clear the armed click, invalidate the old hit
                    // regions; the draw at the top of the next iteration
                    // recomputes the layout, revalidates the focus and
                    // publishes fresh geometry. No product command is
                    // generated by a resize.
                    Event::Resize(_, _) => {
                        model.invalidate_frame();
                        Step::Continue
                    }
                    _ => Step::Continue,
                };
            if step == Step::Exit {
                break;
            }
        }

        // The App's natural-EOF policy (Issue #166 §13): one D11
        // `Completed` Fact may advance the temporary playlist through
        // the SAME Open replacement. It runs AFTER input so a key press
        // and an automatic transition never race for the same refresh,
        // and it blocks exactly as the Open modal's Enter does (the
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

/// The ONE dispatch boundary (§8): every [`TuiAction`] — decoded from a
/// key, a mouse click, or a focused control's activation — comes here,
/// and the same action leads to the same product operation regardless
/// of its source. One call performs at most one product operation
/// (§31): the `ActivateFocused` resolution happens inside this same
/// call, and a resolved activation is never itself an activation.
fn dispatch<S: EpisodeStart>(
    action: TuiAction,
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
) -> Step {
    match action {
        // Quit is loop control, never an episode command: it exits even
        // with no episode committed — the idle shell must stay
        // quittable.
        TuiAction::Quit => Step::Exit,
        // Route changes are presentation-only (§5).
        TuiAction::Navigate(route) => {
            model.set_route(route);
            Step::Continue
        }
        TuiAction::MoveFocus(direction) => {
            model.move_focus(direction);
            Step::Continue
        }
        // Enter on the focused control: resolve to that control's own
        // action and run it — inside this one dispatch, so one physical
        // event still performs at most one product operation (§31).
        TuiAction::ActivateFocused => match model.activation() {
            Some(resolved) => dispatch(resolved, model, player),
            None => Step::Continue,
        },
        TuiAction::OpenModal(kind) => {
            model.open_modal(kind);
            Step::Continue
        }
        TuiAction::ModalInput(input) => {
            handle_modal_input(input, model, player);
            Step::Continue
        }
        // The transport commands: with no episode they are inert (there
        // is nothing to command).
        TuiAction::PlayPause => {
            if let Some(handle) = player.active_handle() {
                // The choice between the two commands comes from a FRESH
                // authoritative observation of the episode's pause-intent
                // command state — the shell never keeps a local `paused`
                // bool.
                if handle.observe().pause_requested {
                    handle.request_resume();
                } else {
                    handle.request_pause();
                }
            }
            Step::Continue
        }
        TuiAction::Stop => {
            if let Some(handle) = player.active_handle() {
                handle.request_stop();
            }
            Step::Continue
        }
        TuiAction::SeekRelative(seconds) => {
            if let Some(handle) = player.active_handle() {
                // The SAME frozen seek command as before: the target is
                // derived from one fresh coherent observation, and an
                // episode whose position is unknown gets NO command at
                // all (no fabricated zero, no seek to the start).
                if let Some(target) = seek_target(
                    &handle.observe(),
                    Duration::from_secs(seconds.unsigned_abs()),
                    seconds > 0,
                ) {
                    handle.request_seek(target);
                }
            }
            Step::Continue
        }
        TuiAction::Previous => {
            perform_navigation(model, player, Navigation::Previous);
            Step::Continue
        }
        TuiAction::Next => {
            perform_navigation(model, player, Navigation::Next);
            Step::Continue
        }
        // The playlist selection is presentation of the App's own
        // selection cursor: it moves the cursor and nothing else
        // (Issue #166 §18).
        TuiAction::PlaylistSelect(cursor) => {
            match cursor {
                PlaylistCursor::Next => player.select_next_track(),
                PlaylistCursor::Previous => player.select_previous_track(),
                PlaylistCursor::Row(position) => {
                    player.select_track(position);
                }
            }
            Step::Continue
        }
        TuiAction::PlaylistPlaySelected => {
            perform_play_selected(model, player);
            Step::Continue
        }
        TuiAction::ToggleOrder => {
            let order = player.toggle_order();
            model.set_order(order);
            model.set_status(Some(format!("Order: {}", order.label())));
            Step::Continue
        }
        TuiAction::CycleRepeat => {
            let repeat = player.cycle_repeat();
            model.set_repeat(repeat);
            model.set_status(Some(format!("Repeat: {}", repeat.label())));
            Step::Continue
        }
        TuiAction::VolumeUp => {
            let volume = player.change_volume(VOLUME_STEP);
            model.set_status(Some(format!("volume {volume}/100 (desired)")));
            Step::Continue
        }
        TuiAction::VolumeDown => {
            let volume = player.change_volume(-VOLUME_STEP);
            model.set_status(Some(format!("volume {volume}/100 (desired)")));
            Step::Continue
        }
    }
}

/// One editing step inside the active modal (§24/§26/§31). The modal
/// owns its keys, so nothing here can also reach a background control:
/// a cancel closes and restores a valid route focus, and a confirm
/// closes and performs THIS modal's operation — never both a modal
/// action and a background action in one event.
fn handle_modal_input<S: EpisodeStart>(
    input: ModalInput,
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
) {
    match input {
        ModalInput::Char(c) => model.modal_push(c),
        ModalInput::Backspace => model.modal_backspace(),
        ModalInput::Cancel => model.close_modal(),
        ModalInput::Confirm => match model.confirm_modal() {
            ModalConfirm::Nothing => {}
            ModalConfirm::Open(candidate) => perform_open(model, player, Path::new(&candidate)),
            ModalConfirm::Seek(target) => match player.active_handle() {
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
                // Nothing to seek: the parsed target is dropped rather
                // than sent into a nonexistent episode.
                None => model.set_status(Some("seek: no episode".to_owned())),
            },
            ModalConfirm::Unreadable(diagnostic) => model.set_status(Some(diagnostic.to_owned())),
        },
    }
}

/// Which navigation step was requested.
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

/// The D14.9 volume step: one action, five points of the desired
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
/// already moved; this action is the only thing that turns it into
/// playback, and only on commit evidence. One inert rule (field
/// round 3): activation on the row that IS the unsettled live episode
/// is not a replay request — the frozen replacement would restart the
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
            .map(|row| super::model::PlaylistRow {
                label: super::model::row_label(row.path),
                playing: row.playing,
                selected: row.selected,
            })
            .collect()
    });
    if let Some(handle) = player.active_handle() {
        model.update(handle.observe());
    }
}

/// The terminal enter/restore command sequences, factored over any
/// writer. The enable/disable MOUSE-CAPTURE pairing lives on this same
/// lifecycle (§16/§40), and the factoring exists so a test can pin it
/// without a real terminal.
fn enter_terminal<W: Write>(writer: &mut W) -> io::Result<()> {
    execute!(writer, EnterAlternateScreen, Hide, EnableMouseCapture)
}

fn restore_terminal<W: Write>(writer: &mut W) -> io::Result<()> {
    execute!(writer, DisableMouseCapture, LeaveAlternateScreen, Show)?;
    writer.flush()
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
        let mut stdout = io::stdout();
        if let Err(error) = enter_terminal(&mut stdout) {
            // Unwind the already-entered modes: the screen leave and the
            // capture disable ride the same restore sequence raw mode is
            // dropped under.
            let _ = restore_terminal(&mut stdout);
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self { restored: false })
    }

    fn restore(&mut self) {
        if self.restored {
            return;
        }
        let mut stdout = io::stdout();
        let restored = restore_terminal(&mut stdout).is_ok() && disable_raw_mode().is_ok();
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
    //! Action-routing tests for the shell over the SAME fake episode
    //! harness the player's C7 matrix uses (real kernel, real playback
    //! session, fake providers). Unlike the player matrix, these tests
    //! use REAL temporary files and folders for everything that crosses
    //! the U1 input expansion — that seam reads the filesystem, and
    //! faking it here would test nothing. The terminal itself stays
    //! fake-free by construction: decode + dispatch is the loop's whole
    //! reaction to an input event and needs no terminal.

    use std::fs;
    use std::path::{Path, PathBuf};

    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };

    use super::super::model::{
        FocusId, FocusMove, ModalKind, TransportButton, TuiRoute, responsive_class,
    };
    use super::super::view;
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

    /// The loop's whole reaction to one key press: decode, then
    /// dispatch. Needs no terminal.
    fn handle_key<S: EpisodeStart>(
        event: KeyEvent,
        model: &mut TuiModel,
        player: &mut ReferencePlayerApp<S>,
    ) -> Step {
        match decode_key(event, model) {
            Some(action) => dispatch(action, model, player),
            None => Step::Continue,
        }
    }

    /// The loop's whole reaction to one left click at a terminal cell:
    /// a Down then an Up at the same cell, each decoded and dispatched.
    /// Needs no terminal beyond the published regions the model
    /// already holds.
    fn click_at<S: EpisodeStart>(
        column: u16,
        row: u16,
        model: &mut TuiModel,
        player: &mut ReferencePlayerApp<S>,
    ) -> Option<TuiAction> {
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
        if let Some(action) = decode_mouse(down, model) {
            let _ = dispatch(action, model, player);
        }
        match decode_mouse(up, model) {
            Some(action) => {
                let _ = dispatch(action, model, player);
                Some(action)
            }
            None => None,
        }
    }

    /// Draw the model at a fixed size so the published regions are the
    /// real frame geometry, then return the first cell of the region
    /// with the wanted target.
    fn draw_and_locate(
        model: &mut TuiModel,
        width: u16,
        height: u16,
        target: &super::super::model::HitTarget,
    ) -> (u16, u16) {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
                .expect("virtual terminal");
        terminal
            .draw(|frame| view::draw(frame, model))
            .expect("draw");
        let region = model
            .regions()
            .iter()
            .find(|region| &region.target == target)
            .unwrap_or_else(|| panic!("no region for {target:?}"));
        (region.area.x + 1, region.area.y + region.area.height / 2)
    }

    fn type_text<S: EpisodeStart>(
        model: &mut TuiModel,
        player: &mut ReferencePlayerApp<S>,
        text: &str,
    ) {
        for c in text.chars() {
            assert_eq!(
                handle_key(key(KeyCode::Char(c)), model, player),
                Step::Continue
            );
        }
    }

    /// The whole O flow from a model state: open the modal, type, Enter.
    fn open_via_keys<S: EpisodeStart>(
        model: &mut TuiModel,
        player: &mut ReferencePlayerApp<S>,
        target: &Path,
    ) {
        assert_eq!(
            handle_key(key(KeyCode::Char('O')), model, player),
            Step::Continue
        );
        assert!(matches!(
            model.modal(),
            Some(super::super::model::Modal::Open { .. })
        ));
        type_text(model, player, &target.to_string_lossy());
        assert_eq!(
            handle_key(key(KeyCode::Enter), model, player),
            Step::Continue
        );
    }

    // ------------------------------------------------------------------
    // Idle shell / Open flow (U1 §16 behaviors preserved).
    // ------------------------------------------------------------------

    /// The idle no-episode state stays truthful — nothing to command,
    /// nothing fabricated, and Q exits the loop.
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

    /// O from the no-episode state, typing a REAL file path, commits an
    /// episode through the same frozen replacement.
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

    /// A folder typed into the O modal expands into the seeded list —
    /// first track opens, N reaches the next entry.
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

    /// Esc cancels the Open modal and opens nothing — not even a probe
    /// runs.
    #[test]
    fn esc_cancels_the_open_modal_without_touching_the_player() {
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

        assert_eq!(model.modal(), None);
        assert_eq!(model.status(), None);
        assert!(player.active_handle().is_none());
        assert!(log.lock().unwrap().is_empty(), "no Open was attempted");
    }

    /// An empty/unreadable folder refuses INSIDE the shell before any
    /// destructive step — the idle state survives intact (no episode,
    /// no playlist, no probe).
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

    /// A bad folder open WHILE PLAYING leaves live playback untouched —
    /// the expansion refusal happens before the frozen replacement can
    /// destroy anything.
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

    // ------------------------------------------------------------------
    // The one-dispatch invariant and the transport commands.
    // ------------------------------------------------------------------

    /// The stop action routes through the EXISTING request_stop seam —
    /// the same frozen right the machine transport uses — quit never
    /// touches the episode, and with no episode the command is inert.
    #[test]
    fn the_stop_action_routes_through_the_request_stop_seam_only() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        // Inert without an episode.
        assert_eq!(
            dispatch(TuiAction::Stop, &mut model, &mut player),
            Step::Continue
        );
        assert!(player.active_handle().is_none());

        // Commit an episode by opening a real file.
        let tree = TempTree::new("stop-seam");
        let file = tree.live_file("live.flac");
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        refresh(&mut model, &player);

        assert!(!player.active_handle().unwrap().observe().stop_requested);
        assert_eq!(
            dispatch(TuiAction::Stop, &mut model, &mut player),
            Step::Continue
        );
        assert!(
            player.active_handle().unwrap().observe().stop_requested,
            "Stop must record stop intent through the seam"
        );
        // Idempotent: dispatching Stop again stays a plain seam call.
        assert_eq!(
            dispatch(TuiAction::Stop, &mut model, &mut player),
            Step::Continue
        );

        // Quit is loop control, not a playback command.
        let before = player.active_handle().unwrap().observe();
        assert_eq!(
            dispatch(TuiAction::Quit, &mut model, &mut player),
            Step::Exit
        );
        assert_eq!(player.active_handle().unwrap().observe(), before);
    }

    /// The pause/resume action never keeps a local paused bool: the
    /// first dispatch records pause intent through the seam, the next
    /// releases it, and the choice between the two commands is read
    /// from a fresh authoritative observation each time.
    #[test]
    fn the_pause_resume_action_routes_through_the_seam_both_ways() {
        let tree = TempTree::new("pause-resume");
        let file = tree.live_file("live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        let handle = player.active_handle().expect("committed").clone();
        assert!(!handle.observe().pause_requested);
        assert!(!handle.observe().paused());

        assert_eq!(
            dispatch(TuiAction::PlayPause, &mut model, &mut player),
            Step::Continue
        );
        assert!(
            handle.observe().pause_requested,
            "the first PlayPause must record pause intent through the seam"
        );
        assert_eq!(
            dispatch(TuiAction::PlayPause, &mut model, &mut player),
            Step::Continue
        );
        assert!(
            !handle.observe().pause_requested,
            "the second PlayPause must release the pause through the seam"
        );
    }

    /// The seek action routes the SAME request_seek seam, and an
    /// episode whose position is unknown gets NO command: the dispatch
    /// changes nothing (and panics on nothing).
    #[test]
    fn the_seek_action_changes_nothing_when_the_position_is_unknown() {
        let tree = TempTree::new("seek-blind");
        let file = tree.live_file("live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        let handle = player.active_handle().expect("committed").clone();

        let before = handle.observe();
        assert_eq!(
            dispatch(TuiAction::SeekRelative(5), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(
            dispatch(TuiAction::SeekRelative(-30), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(
            handle.observe(),
            before,
            "no position evidence: no seek command"
        );
    }

    /// The N/P actions are the App's manual navigation, not episode
    /// commands: the shell reports the outcome, the episode is only
    /// ever touched by the frozen replacement.
    #[test]
    fn n_and_p_route_the_manual_navigation() {
        let tree = TempTree::new("navigation");
        let a = tree.live_file("live-a.flac");
        let b = tree.live_file("live-b.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        assert_eq!(player.open(&a), crate::player::OpenOutcome::Opened);
        player.establish_playlist(vec![a.clone(), b.clone()]);
        refresh(&mut model, &player);

        assert_eq!(
            dispatch(TuiAction::Next, &mut model, &mut player),
            Step::Continue
        );
        refresh(&mut model, &player);
        assert_eq!(model.source(), Some(b.to_string_lossy().as_ref()));
        assert!(
            model.status().unwrap().starts_with("next: opened "),
            "{:?}",
            model.status()
        );

        assert_eq!(
            dispatch(TuiAction::Previous, &mut model, &mut player),
            Step::Continue
        );
        refresh(&mut model, &player);
        assert_eq!(model.source(), Some(a.to_string_lossy().as_ref()));
    }

    /// The policy and volume actions route to the player and say so;
    /// they never touch the episode.
    #[test]
    fn policy_and_volume_route_to_the_player() {
        let tree = TempTree::new("policy");
        let file = tree.live_file("live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let handle = {
            assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
            player.active_handle().expect("committed").clone()
        };
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        assert_eq!(
            dispatch(TuiAction::ToggleOrder, &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(player.playlist_order(), PlaybackOrder::Shuffle);
        assert_eq!(model.status(), Some("Order: Shuffle"));
        assert_eq!(
            dispatch(TuiAction::CycleRepeat, &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(player.playlist_repeat(), RepeatMode::All);
        assert_eq!(
            dispatch(TuiAction::VolumeDown, &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(player.desired_volume(), 95);
        assert_eq!(model.status(), Some("volume 95/100 (desired)"));
        assert_eq!(
            handle.observe().terminal_outcome,
            None,
            "policy and volume never touch the episode"
        );
    }

    // ------------------------------------------------------------------
    // Route switching never touches product state (§5).
    // ------------------------------------------------------------------

    /// Route switching leaves playback, navigation and policy state
    /// untouched, and the SAME TuiAction arrives from the keyboard
    /// (Enter on the focused tab) and the mouse (a tab click) (§30).
    #[test]
    fn route_switching_is_presentation_only_and_parity_holds() {
        let tree = TempTree::new("routes");
        let file = tree.live_file("live.flac");
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        player.establish_playlist(vec![file.clone()]);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        let activations = || {
            log.lock()
                .unwrap()
                .iter()
                .filter(|event| event.starts_with("activate "))
                .count()
        };
        let activations_before = activations();

        // Keyboard: focus the Playlist tab, Enter.
        model.set_focus(Some(FocusId::RouteTab(TuiRoute::Playlist)));
        let keyboard_action = model.activation().expect("the tab activates");
        assert_eq!(keyboard_action, TuiAction::Navigate(TuiRoute::Playlist));
        assert_eq!(
            dispatch(keyboard_action, &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(model.route(), TuiRoute::Playlist);

        // The player never noticed.
        let observation = player.active_handle().unwrap().observe();
        assert_eq!(observation.terminal_outcome, None);
        assert!(!observation.stop_requested);
        assert!(!observation.pause_requested);
        assert_eq!(player.playlist_order(), PlaybackOrder::Sequential);
        assert_eq!(player.playlist_repeat(), RepeatMode::Off);
        assert_eq!(player.navigation_position(), Some((1, 1)));
        assert_eq!(activations(), activations_before, "no probe, no open");

        // Mouse: click the Audio tab. Same action shape, same dispatch.
        let (column, row) = draw_and_locate(
            &mut model,
            100,
            30,
            &super::super::model::HitTarget::RouteTab(TuiRoute::Audio),
        );
        let mouse_action = click_at(column, row, &mut model, &mut player);
        assert_eq!(mouse_action, Some(TuiAction::Navigate(TuiRoute::Audio)));
        assert_eq!(model.route(), TuiRoute::Audio);
        let observation = player.active_handle().unwrap().observe();
        assert_eq!(observation.terminal_outcome, None);
        assert!(!observation.stop_requested);
        assert!(!observation.pause_requested);
        assert_eq!(activations(), activations_before, "still no probe, no open");
    }

    // ------------------------------------------------------------------
    // Transport parity: keyboard and mouse converge on one action (§30).
    // ------------------------------------------------------------------

    #[test]
    fn transport_activation_parity_holds() {
        let tree = TempTree::new("transport-parity");
        let file = tree.live_file("live.flac");
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        let activations = || {
            log.lock()
                .unwrap()
                .iter()
                .filter(|event| event.starts_with("activate "))
                .count()
        };
        let activations_before = activations();

        let wanted = super::super::model::HitTarget::Transport(TransportButton::PlayPause);
        let (column, row) = draw_and_locate(&mut model, 100, 30, &wanted);
        let mouse_action = click_at(column, row, &mut model, &mut player);

        model.set_focus(Some(FocusId::Transport(TransportButton::PlayPause)));
        let keyboard_action = model.activation();
        assert_eq!(mouse_action, keyboard_action, "§30: one action per control");

        // The dispatched mouse click really paused the episode: the
        // witness is the SEAM's command state, not the status line.
        assert!(
            player.active_handle().unwrap().observe().pause_requested,
            "the clicked Play/Pause recorded pause intent"
        );
        assert_eq!(
            activations(),
            activations_before,
            "the transport click composed no new episode"
        );
    }

    // ------------------------------------------------------------------
    // Playlist route behaviors (§35: focus, selection, row hit, wheel).
    // ------------------------------------------------------------------

    #[test]
    fn the_playlist_route_selects_and_plays() {
        let tree = TempTree::new("playlist-route");
        let files: Vec<PathBuf> = (0..3)
            .map(|n| tree.live_file(&format!("live-{n}.flac")))
            .collect();
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        assert_eq!(player.open(&files[0]), crate::player::OpenOutcome::Opened);
        player.establish_playlist(files.clone());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        // To the Playlist route; focus falls to the list (the route's
        // first local control, §12).
        dispatch(
            TuiAction::Navigate(TuiRoute::Playlist),
            &mut model,
            &mut player,
        );
        model.validate_focus();
        assert_eq!(model.focus(), Some(FocusId::Playlist));

        // ↓ moves only the selection — no probe, no open.
        let events_before = log.lock().unwrap().len();
        assert_eq!(
            handle_key(key(KeyCode::Down), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(player.playlist_selected_position(), Some(1));
        assert_eq!(player.playlist_playing_position(), Some(0));
        assert_eq!(
            log.lock().unwrap().len(),
            events_before,
            "selection is presentation"
        );

        // A mouse row hit selects that row.
        let wanted = super::super::model::HitTarget::PlaylistRow(2);
        let (column, row) = draw_and_locate(&mut model, 100, 30, &wanted);
        let mouse_action = click_at(column, row, &mut model, &mut player);
        assert_eq!(
            mouse_action,
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Row(2)))
        );
        assert_eq!(player.playlist_selected_position(), Some(2));

        // Enter on the focused list plays the selected row.
        model.set_focus(Some(FocusId::Playlist));
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        refresh(&mut model, &player);
        assert_eq!(player.active_source(), Some(files[2].as_path()));
        assert!(
            model.status().unwrap().starts_with("play: opened "),
            "{:?}",
            model.status()
        );
    }

    // ------------------------------------------------------------------
    // Modal isolation and the one-event-one-action invariant (§25/§31).
    // ------------------------------------------------------------------

    /// While a modal is open, a click on a background control neither
    /// dispatches nor steals focus — and after the modal closes on ONE
    /// key event, that event has already been consumed: nothing leaks
    /// through to the background.
    #[test]
    fn a_modal_click_never_leaks_and_a_close_consumes_its_event() {
        let tree = TempTree::new("modal-isolation");
        let file = tree.live_file("live.flac");
        let source = FakeEpisodeSource::new();
        let mut player = ReferencePlayerApp::new(source);
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        // Background click behind the help overlay.
        dispatch(
            TuiAction::OpenModal(ModalKind::Help),
            &mut model,
            &mut player,
        );
        let wanted = super::super::model::HitTarget::Transport(TransportButton::Stop);
        let (column, row) = draw_and_locate(&mut model, 100, 30, &wanted);
        assert_eq!(click_at(column, row, &mut model, &mut player), None);
        assert!(
            !player.active_handle().unwrap().observe().stop_requested,
            "the background Stop never fired"
        );

        // Esc closes the help overlay and NOTHING else: the same event
        // cannot also act on the background (§31).
        assert_eq!(
            handle_key(key(KeyCode::Esc), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(model.modal(), None);
        assert!(
            !player.active_handle().unwrap().observe().stop_requested,
            "the closing Esc never leaked through"
        );
        assert!(
            !player.active_handle().unwrap().observe().pause_requested,
            "no background command fired behind the modal either"
        );
    }

    /// ONE physical Enter inside the GoTo modal performs at most one
    /// product operation: the seek. The same Enter cannot also activate
    /// a background control (§31).
    #[test]
    fn one_modal_enter_performs_at_most_one_product_operation() {
        let tree = TempTree::new("one-enter");
        let file = tree.live_file("live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        dispatch(
            TuiAction::OpenModal(ModalKind::GoTo),
            &mut model,
            &mut player,
        );
        type_text(&mut model, &mut player, "1:35");
        let event = key(KeyCode::Enter);
        // Exactly one decode out of this physical event.
        let action = decode_key(event, &model);
        assert_eq!(action, Some(TuiAction::ModalInput(ModalInput::Confirm)));
        assert_eq!(
            dispatch(action.expect("one action"), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(model.modal(), None, "the modal closed");
        assert_eq!(
            model.status(),
            Some("seek requested: 01:35"),
            "exactly one product operation was performed"
        );
    }

    // ------------------------------------------------------------------
    // Resize (§29).
    // ------------------------------------------------------------------

    /// A resize event clears the armed click and the old hit regions;
    /// the next draw republishes fresh geometry and the focus stays on
    /// a valid visible control. No product command is generated.
    #[test]
    fn a_resize_invalidates_geometry_and_preserves_a_valid_focus() {
        let tree = TempTree::new("resize");
        let file = tree.live_file("live.flac");
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        let activations = || {
            log.lock()
                .unwrap()
                .iter()
                .filter(|event| event.starts_with("activate "))
                .count()
        };
        let activations_before = activations();

        // Arm a click on a transport button.
        let wanted = super::super::model::HitTarget::Transport(TransportButton::PlayPause);
        let (column, row) = draw_and_locate(&mut model, 100, 30, &wanted);
        let down = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        decode_mouse(down, &mut model);
        assert!(model.armed().is_some());

        // The resize event.
        model.invalidate_frame();
        assert_eq!(model.armed(), None, "the resize cleared the armed click");
        let up = MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(
            decode_mouse(up, &mut model),
            None,
            "the Up after a resize is no action"
        );

        // The next draw republishes fresh geometry and the focus (set
        // by the Down before the resize) is still a valid control.
        draw_and_locate(&mut model, 100, 30, &wanted);
        assert!(
            !model.regions().is_empty(),
            "fresh geometry after the resize"
        );
        assert_eq!(
            model.focus(),
            Some(FocusId::Transport(TransportButton::PlayPause)),
            "the focus is still a visible enabled control"
        );
        assert_eq!(
            activations(),
            activations_before,
            "the resize composed nothing"
        );
    }

    // ------------------------------------------------------------------
    // Focus cycle keys route (Tab / Shift+Tab).
    // ------------------------------------------------------------------

    #[test]
    fn tab_moves_focus_through_the_real_cycle() {
        let tree = TempTree::new("tab");
        let file = tree.live_file("live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        // Draw once so the class (Wide at 100x30) is known; the draw's
        // focus validation lands on the route's first local control.
        draw_and_locate(
            &mut model,
            100,
            30,
            &super::super::model::HitTarget::RouteTab(TuiRoute::NowPlaying),
        );
        assert_eq!(
            model.focus(),
            Some(FocusId::Transport(TransportButton::Previous)),
            "the validated start is the route's first local control"
        );

        for expected in [
            FocusId::Transport(TransportButton::PlayPause),
            FocusId::Transport(TransportButton::Stop),
            FocusId::Transport(TransportButton::Next),
            FocusId::RouteTab(TuiRoute::NowPlaying),
            FocusId::RouteTab(TuiRoute::Playlist),
        ] {
            assert_eq!(
                handle_key(key(KeyCode::Tab), &mut model, &mut player),
                Step::Continue
            );
            assert_eq!(model.focus(), Some(expected));
        }
        // Shift+Tab walks back (terminals report BackTab).
        assert_eq!(
            handle_key(
                KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
                &mut model,
                &mut player
            ),
            Step::Continue
        );
        assert_eq!(model.focus(), Some(FocusId::RouteTab(TuiRoute::NowPlaying)));
        assert_eq!(
            dispatch(
                TuiAction::MoveFocus(FocusMove::Previous),
                &mut model,
                &mut player
            ),
            Step::Continue
        );
        assert_eq!(
            model.focus(),
            Some(FocusId::Transport(TransportButton::Next))
        );
    }

    // ------------------------------------------------------------------
    // Terminal guard: the mouse-capture pairing (§16/§40).
    // ------------------------------------------------------------------

    /// EnableMouseCapture on the enter sequence has its matching
    /// DisableMouseCapture on the same restore lifecycle, ordered
    /// before the alternate-screen leave. (The raw-mode pairing is the
    /// guard's own concern; this pins the NEW capture obligation.)
    ///
    /// The oracle is ANSI byte order, valid only where the capture
    /// commands actually emit bytes: on Windows crossterm routes mouse
    /// capture through the WinAPI console path
    /// (`is_ansi_code_supported` is `false`), so nothing reaches the
    /// writer. The pairing there is crossterm's own mechanism; Windows
    /// keeps the compile gate and device-free tests, and real device
    /// evidence stays out of CI scope.
    #[cfg(not(windows))]
    #[test]
    fn mouse_capture_is_disabled_on_the_same_lifecycle_that_enables_it() {
        let mut enter_buffer = Vec::new();
        enter_terminal(&mut enter_buffer).expect("enter sequence");
        let enter = String::from_utf8(enter_buffer).expect("ansi");
        assert!(
            enter.contains("\x1B[?1000h"),
            "the enter sequence must enable mouse capture: {enter:?}"
        );

        let mut restore_buffer = Vec::new();
        restore_terminal(&mut restore_buffer).expect("restore sequence");
        let restore = String::from_utf8(restore_buffer).expect("ansi");
        assert!(
            restore.contains("\x1B[?1000l"),
            "the restore sequence must disable mouse capture: {restore:?}"
        );
        // Capture is disabled BEFORE the alternate screen is left, so a
        // partially failed restore cannot strand the capture either.
        let disable = restore.find("\x1B[?1000l").expect("disable marker");
        let leave = restore.find("\x1B[?1049l").expect("leave marker");
        assert!(
            disable < leave,
            "disable capture before leaving the screen: {restore:?}"
        );
    }

    /// The guard's acquire-failure path unwinds the modes it already
    /// entered — exercised on the writer seam: a failing writer makes
    /// `enter_terminal` fail, and the caller-side cleanup order is what
    /// the guard performs (restore sequence first, then raw mode).
    #[test]
    fn the_guard_unwinds_entered_modes_when_a_later_step_fails() {
        // A writer that accepts the first command write and then fails
        // exercises the mid-sequence failure branch of `acquire`.
        struct FailsOnSecondWrite {
            writes: usize,
        }
        impl Write for FailsOnSecondWrite {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.writes += 1;
                if self.writes > 1 {
                    Err(io::Error::other("boom"))
                } else {
                    Ok(buf.len())
                }
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut writer = FailsOnSecondWrite { writes: 0 };
        assert!(
            enter_terminal(&mut writer).is_err(),
            "the enter sequence failed mid-way"
        );
        // The recovery is exactly what TerminalGuard::acquire performs:
        // a best-effort restore (which may also fail) and no panic.
        let _ = restore_terminal(&mut writer);
    }

    /// The responsive class helper is wired into the shell through the
    /// draw (the model tests pin the derivation; this pins the wiring
    /// at the runtime's shell sizes).
    #[test]
    fn the_shell_classes_are_the_ones_the_draw_publishes() {
        let mut model = TuiModel::new(String::new());
        refresh(
            &mut model,
            &ReferencePlayerApp::new(FakeEpisodeSource::new()),
        );
        draw_and_locate(
            &mut model,
            100,
            30,
            &super::super::model::HitTarget::RouteTab(TuiRoute::NowPlaying),
        );
        assert_eq!(model.class(), responsive_class(100, 30));
    }
}
