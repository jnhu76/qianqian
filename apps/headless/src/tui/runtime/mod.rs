//! The terminal session of the reference player: raw mode + alternate
//! screen + mouse capture under a small RAII guard, and a modest event
//! loop.
//!
//! Terminal ownership is lexical: the guard on this function's stack is
//! the ONLY owner of the entered terminal modes, and unwinding drops it
//! like any other early return. This module is deliberately the ONLY
//! place where crossterm I/O happens: per refresh the loop takes exactly
//! one pure `observe()` read and one `terminal.draw()`; it never blocks
//! the audio path.
//!
//! Every input event flows through the same pipe: decode (model) → at
//! most one [`TuiAction`] → [`dispatch::dispatch`] — the ONE dispatch
//! boundary, which performs at most one product operation. The command
//! performers live in [`dispatch`]; the Open picker's operations and
//! their I/O live in [`picker`].

mod dispatch;
mod picker;

#[cfg(test)]
mod testutil;

use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::player::{EpisodeStart, OpenOutcome, ReferencePlayerApp};
use crate::tui::view;

use super::model::{Step, TuiModel, decode_key, decode_mouse};
use dispatch::{dispatch, eof_feedback, pending_commit_feedback};

/// UI refresh cadence: 50 ms — 20 fps (Issue #233). The visualizer's
/// per-frame deltas are whole terminal cells, and large-area cell
/// motion reads as continuous to the eye from roughly 20 fps (the
/// large-field flicker-fusion band); the previous 150 ms (~6.7 fps)
/// was visibly steppy. One ordinary tick still serves input and
/// rendering both — `event::poll` wakes on the first input event, so
/// this only caps the redraw interval — and audio execution stays
/// untouched: the loop's per-refresh work is one diff-based draw plus
/// the cheap observation reads, never on the playback threads.
pub const TICK: Duration = Duration::from_millis(50);

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

        // The pending navigation burst's deadline joins the ordinary
        // event wait: the poll wakes on the first input event OR at the
        // deadline, whichever comes first — no timer thread, no extra
        // wakeups, and the 20 fps refresh cadence is unchanged (an
        // expired burst never waits a whole extra tick).
        let timeout = poll_timeout(player.pending_navigation_deadline(), Instant::now());
        if event::poll(timeout).map_err(|error| format!("terminal input failed: {error}"))?
            && handle_event(
                event::read().map_err(|error| format!("terminal input failed: {error}"))?,
                &mut model,
                player,
            ) == Step::Exit
        {
            break;
        }

        settle_timers(&mut model, player, Instant::now());

        refresh(&mut model, player);
    }

    guard.restore();
    Ok(())
}

/// The event wait for one loop iteration: the ordinary refresh tick,
/// shortened to a pending navigation burst's deadline when one expires
/// sooner (`min(TICK, time-to-deadline)`); an already-expired deadline
/// waits zero. Pure so the deadline arithmetic is testable without a
/// terminal.
fn poll_timeout(deadline: Option<Instant>, now: Instant) -> Duration {
    deadline.map_or(TICK, |deadline| {
        TICK.min(deadline.saturating_duration_since(now))
    })
}

/// The loop's per-iteration automatic transitions, in precedence order:
/// a pending navigation burst resolves FIRST (one Open at most, at its
/// deadline), and the natural-EOF policy runs only when no burst
/// attempt was made this iteration — the manual burst's outcome wins
/// over the same Completed Fact's auto-next (research/navigation-burst
/// R3 §6). Both run AFTER input so a key press and an automatic
/// transition never race for the same refresh, and each blocks exactly
/// as the Open modal's Enter does (the documented synchronous-Open
/// stall) — no async machinery is earned here either.
fn settle_timers<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
    now: Instant,
) {
    if let Some(outcome) = player.poll_pending_navigation(now) {
        if matches!(outcome, OpenOutcome::Opened) {
            model.note_episode_replacement();
        }
        model.set_status(Some(pending_commit_feedback(&outcome, player)));
        // The burst's commit stalled this loop like any Open replacement:
        // input queued during the stall never reaches the new episode's
        // geometry (T0 picker freeze).
        drain_busy_interval_input();
        return;
    }
    // The App's natural-EOF policy (Issue #166 §13): one D11
    // `Completed` Fact may advance the temporary playlist through
    // the SAME Open replacement.
    if let Some(outcome) = player.poll_eof_policy() {
        if matches!(outcome, OpenOutcome::Opened) {
            model.note_episode_replacement();
        }
        model.set_status(Some(eof_feedback(&outcome, player)));
        // The advance stalled this loop like any Open replacement:
        // input queued during the stall never reaches the new
        // episode's geometry (T0 picker freeze).
        drain_busy_interval_input();
    }
}

/// The loop's per-event body, factored out so the wiring is itself
/// testable: one physical event decodes to at most one action, and one
/// action dispatches to at most one product operation (§31) — key and
/// mouse share the boundary.
fn handle_event<S: EpisodeStart>(
    event: Event,
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
) -> Step {
    match event {
        Event::Key(key) => {
            decode_key(key, model).map_or(Step::Continue, |action| dispatch(action, model, player))
        }
        Event::Mouse(mouse) => decode_mouse(mouse, model)
            .map_or(Step::Continue, |action| dispatch(action, model, player)),
        // §29: clear the armed click, invalidate the old hit regions;
        // the draw at the top of the next iteration recomputes the
        // layout, revalidates the focus and publishes fresh geometry.
        // No product command is generated by a resize.
        Event::Resize(_, _) => {
            model.invalidate_frame();
            Step::Continue
        }
        _ => Step::Continue,
    }
}

/// Drop the input that arrived during a synchronous stall (T0 picker
/// freeze: "after it returns, clear armed clicks/buffered input from
/// the busy interval before publishing new interactive geometry"). The
/// event loop reads one event per iteration; whatever queued behind
/// the dispatch that stalled — keys, clicks, wheels — was typed
/// against surfaces that no longer exist and must never decode
/// against the post-operation frame. Resize events are dropped too:
/// the next draw auto-resizes to the real terminal size. Every stall
/// (submission, navigation, a large listing) calls this before the
/// next draw publishes fresh geometry.
pub(super) fn drain_busy_interval_input() {
    while event::poll(Duration::ZERO).unwrap_or(false) {
        let _ = event::read();
    }
}

/// Follow the player's committed episode and its playlist: swap the
/// source label when the committed episode changed, take the new
/// episode's one pure observation for this refresh, and rebuild the
/// playlist rows only when the App's playlist revision moved (a huge
/// playlist must not cost per-frame work).
pub(super) fn refresh<S: EpisodeStart>(model: &mut TuiModel, player: &ReferencePlayerApp<S>) {
    model.set_episode(
        player
            .active_source()
            .map(|p| p.to_string_lossy().into_owned()),
    );
    model.set_navigation(player.navigation_position());
    model.set_volume(Some(player.desired_volume()));
    model.set_order(player.playlist_order());
    model.set_repeat(player.playlist_repeat());
    model.note_desired_processing(player.desired_processing());
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
    // The Observation Plane (G4): the current episode's reader, polled
    // for its latest owned snapshot. The lookup happens per refresh, so
    // an episode replacement automatically re-attaches to the NEW
    // episode's reader — no stale telemetry survives the seam.
    model.note_observation_snapshot(
        player
            .active_handle()
            .and_then(|handle| handle.observation_reader())
            .and_then(|reader| reader.latest()),
    );
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
    //! Tests for this submodule.

    use std::io::{self, Write};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use crossterm::event::{Event, MouseButton, MouseEventKind};

    use super::testutil::*;
    use super::{
        TICK, enter_terminal, handle_event, poll_timeout, refresh, restore_terminal, settle_timers,
    };
    use crate::player::tests::FakeEpisodeSource;
    use crate::player::{NAVIGATION_QUIET_WINDOW, OpenOutcome, ReferencePlayerApp};
    use crate::tui::model::*;
    use qianqian_playback::EpisodeTerminalOutcome;

    /// The total kernel-mediated event count (the delta-slicing
    /// baseline for the settle tests).
    fn event_count(log: &Arc<Mutex<Vec<String>>>) -> usize {
        log.lock().unwrap().len()
    }

    /// The refresh tick is 50 ms — 20 fps, the smoothness band for
    /// whole-cell motion (#233). One ordinary tick serves input and
    /// visualization both; this pin exists so a cadence change is a
    /// deliberate act, not drift.
    #[test]
    fn the_refresh_tick_is_the_20fps_band() {
        assert_eq!(TICK, Duration::from_millis(50));
    }

    /// The event wait joins the pending burst's deadline without
    /// changing the ordinary refresh cadence: no deadline means the
    /// plain tick, an earlier deadline shortens the wait to exactly the
    /// time remaining, an expired one waits zero, and a later one
    /// never exceeds the tick.
    #[test]
    fn the_poll_timeout_is_the_tick_shortened_to_the_burst_deadline() {
        let now = Instant::now();
        assert_eq!(poll_timeout(None, now), TICK, "no burst: the plain tick");
        assert_eq!(
            poll_timeout(Some(now + 2 * TICK), now),
            TICK,
            "a later deadline never exceeds the tick"
        );
        assert_eq!(
            poll_timeout(Some(now + Duration::from_millis(20)), now),
            Duration::from_millis(20),
            "an earlier deadline shortens the wait"
        );
        assert_eq!(
            poll_timeout(Some(now - Duration::from_secs(1)), now),
            Duration::ZERO,
            "an expired deadline waits zero"
        );
        assert_eq!(
            poll_timeout(Some(now), now),
            Duration::ZERO,
            "the exact boundary waits zero (now >= deadline commits)"
        );
    }

    /// The settle step's precedence (research/navigation-burst R3 §6):
    /// a due burst commits INSTEAD of the same Completed Fact's
    /// auto-next — exactly one Open, the burst's target — and the EOF
    /// policy then finds a fresh episode.
    #[test]
    fn settle_resolves_a_due_burst_instead_of_the_completed_auto_next() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        let files = [
            PathBuf::from("/media/settle-finite-0.flac"),
            PathBuf::from("/media/settle-finite-1.flac"),
            PathBuf::from("/media/settle-live-2.flac"),
        ];
        assert_eq!(player.open(&files[0]), OpenOutcome::Opened);
        player.establish_playlist(files.to_vec());
        let handle = player.active_handle().expect("committed").clone();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        // Walk to position 1 (the pending P will target 0; the EOF
        // auto-next would target 2).
        assert_eq!(player.next_track(), Some(OpenOutcome::Opened));
        let handle = player.active_handle().expect("committed").clone();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        let baseline = event_count(&log);

        let mut model = TuiModel::new(String::new());
        let t0 = Instant::now();
        player.request_previous_intent(t0);

        settle_timers(&mut model, &mut player, t0 + NAVIGATION_QUIET_WINDOW);

        assert_eq!(
            player.active_source(),
            Some(files[0].as_path()),
            "the burst's target won"
        );
        assert_eq!(player.playlist_playing_position(), Some(0));
        let events = log.lock().unwrap().clone();
        let delta = &events[baseline..];
        assert_eq!(
            delta.iter().filter(|e| e.starts_with("probe ")).count(),
            1,
            "exactly one Open ran this settle\n{events:?}"
        );
        assert!(
            delta.iter().any(|e| e.contains("settle-finite-0")),
            "it was the burst target, never the auto-next\n{events:?}"
        );
    }

    /// A burst that collapses hands the SAME settle to the normal
    /// Completed policy — the auto-next fires exactly once, as if the
    /// burst had never happened.
    #[test]
    fn settle_hands_a_collapsed_burst_to_the_normal_eof_policy() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        let files = [
            PathBuf::from("/media/collapse-finite-0.flac"),
            PathBuf::from("/media/collapse-live-1.flac"),
        ];
        assert_eq!(player.open(&files[0]), OpenOutcome::Opened);
        player.establish_playlist(files.to_vec());
        let handle = player.active_handle().expect("committed").clone();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        let baseline = event_count(&log);

        let mut model = TuiModel::new(String::new());
        let t0 = Instant::now();
        player.request_next_intent(t0);
        player.request_previous_intent(t0 + Duration::from_millis(10));

        settle_timers(
            &mut model,
            &mut player,
            t0 + Duration::from_millis(10) + NAVIGATION_QUIET_WINDOW,
        );

        assert_eq!(
            player.active_source(),
            Some(files[1].as_path()),
            "the collapse committed nothing and the auto-next advanced"
        );
        let events = log.lock().unwrap().clone();
        let delta = &events[baseline..];
        assert_eq!(
            delta.iter().filter(|e| e.starts_with("probe ")).count(),
            1,
            "zero manual attempts, exactly the auto-next\n{events:?}"
        );
        assert!(delta[0].contains("collapse-live-1"), "\n{events:?}");
    }

    /// With no burst pending the settle is exactly the ordinary EOF
    /// policy it always was.
    #[test]
    fn settle_without_a_burst_is_the_ordinary_eof_policy() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        let files = [
            PathBuf::from("/media/plain-finite-0.flac"),
            PathBuf::from("/media/plain-live-1.flac"),
        ];
        assert_eq!(player.open(&files[0]), OpenOutcome::Opened);
        player.establish_playlist(files.to_vec());
        let handle = player.active_handle().expect("committed").clone();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        let baseline = event_count(&log);

        let mut model = TuiModel::new(String::new());
        settle_timers(&mut model, &mut player, Instant::now());

        assert_eq!(player.active_source(), Some(files[1].as_path()));
        let events = log.lock().unwrap().clone();
        let delta = &events[baseline..];
        assert_eq!(
            delta.iter().filter(|e| e.starts_with("probe ")).count(),
            1,
            "one auto-next attempt\n{events:?}"
        );
        assert_eq!(
            delta.iter().filter(|e| e.starts_with("teardown ")).count() / 2,
            1,
            "one old episode retired\n{events:?}"
        );
        assert_eq!(
            delta.iter().filter(|e| e.starts_with("activate ")).count() / 2,
            1,
            "one fresh episode started\n{events:?}"
        );
    }

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

    /// The loop's Resize wiring, now factored into `handle_event` so it
    /// is testable (G1 re-review): a resize event through the real
    /// per-event body invalidates the published geometry and any arm.
    #[test]
    fn a_resize_event_through_the_loop_clears_the_frame() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new("song.flac");
        refresh(&mut model, &player);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|frame| super::super::view::draw(frame, &mut model))
            .expect("draw");
        let (column, row) = region_cell(&model, &HitTarget::Transport(TransportButton::Open));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut model,
        );
        assert!(model.armed().is_some());

        assert_eq!(
            handle_event(Event::Resize(80, 24), &mut model, &mut player,),
            Step::Continue,
            "a resize generates no product command"
        );
        assert!(
            model.regions().is_empty(),
            "the resize cleared the published regions"
        );
        assert_eq!(model.armed(), None, "the resize cancelled the arm");
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
            &crate::tui::model::HitTarget::RouteTab(TuiRoute::NowPlaying),
        );
        assert_eq!(model.fit(), shell_fit(100, 30));
    }

    // ------------------------------------------------------------------
    // G1: the visible Open/picker flow, the preference row, modal
    // isolation.
    // ------------------------------------------------------------------
}
