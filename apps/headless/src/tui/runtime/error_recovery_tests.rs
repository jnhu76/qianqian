//! Issue #23 PR-B2 integration evidence: the TUI's error presentation
//! and safe manual recovery, driven through the REAL dispatch boundary
//! over the SAME fake-episode harness the player matrix uses (real
//! kernel, real playback session, fake providers) with REAL temporary
//! files for everything that crosses the U1 input expansion.
//!
//! The cases follow the shell's four provable error kinds (B1):
//! A OpenRefused · B ActivationFailedClean · C a D11 Failed episode ·
//! D/E manual recovery through the EXISTING Space/N operations · F the
//! §G.6 fail-stop gate. NC-1 is the negative-control oracle the work
//! order owes; NC-2's oracle IS case F's sentinel section — the
//! prefix-gating and gate-bypass mutations are executed for real in
//! the PR evidence (apply → RED capture → restore → GREEN capture),
//! never simulated.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use qianqian_playback::{
    EpisodeTerminalOutcome, PlaybackSessionHandle, PlaybackSessionObservation,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::dispatch::dispatch;
use super::testutil::*;
use super::{refresh, settle_timers};
use crate::error_presentation::{ErrorPresentationKind, fail_stop_presentation};
use crate::player::tests::FakeEpisodeSource;
use crate::player::{NAVIGATION_QUIET_WINDOW, OpenOutcome, ReferencePlayerApp};
use crate::tui::model::*;
use crate::tui::view;

/// Draw the model at a fixed size and return the frame text (the view
/// testutil is private to the view module, so the runtime tests keep
/// this five-line local copy). `frame_lines` is the same text with
/// ASCII spaces removed: the TestBackend stores a double-width CJK
/// glyph's continuation cell as a space cell, so CJK needles compare
/// against the collapsed form.
fn frame_text(model: &mut TuiModel, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("virtual terminal");
    terminal
        .draw(|frame| view::draw(frame, model))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol().to_string()))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn frame_lines(model: &mut TuiModel, width: u16, height: u16) -> String {
    frame_text(model, width, height).replace(' ', "")
}

/// One D11 Failed observation with a chosen diagnostic, for the
/// presentation tests that need no player.
fn failed_observation(diagnostic: &str) -> PlaybackSessionObservation {
    PlaybackSessionObservation {
        terminal_outcome: Some(EpisodeTerminalOutcome::Failed),
        failure_diagnostic: Some(diagnostic.to_owned()),
        stop_requested: false,
        pause_requested: false,
        source_format: None,
        source_duration: None,
        position: None,
        pause_engagement: qianqian_playback::PauseEngagement::Disengaged,
        activation_error: None,
        last_processing_refusal: None,
    }
}

/// Wait (bounded) for the CURRENT episode to settle into the wanted
/// D11 terminal outcome. The corrupt fake errors on its FIRST read, so
/// the real render thread reaches the failure within a few
/// milliseconds even under heavy parallel-test load; the generous
/// bound exists so a wedged harness fails the test instead of hanging
/// it.
fn wait_for_terminal(handle: &PlaybackSessionHandle, wanted: EpisodeTerminalOutcome) -> bool {
    for _ in 0..2000 {
        if handle.observe().terminal_outcome == Some(wanted) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    handle.observe().terminal_outcome == Some(wanted)
}

/// CASE A (OpenRefused): a probe-refused Open through the O flow keeps
/// the picker for correction, reports the B1 copy (never the raw
/// diagnostic) on the status block, and `E` shows the diagnostic in
/// the details overlay — then Esc closes with the focus back.
#[test]
fn case_a_a_refused_open_reports_b1_copy_and_e_shows_the_detail() {
    let tree = TempTree::new("b2-refused");
    let invalid = tree.live_file("invalid.flac");
    let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
    let mut model = TuiModel::new(String::new());

    open_via_keys(&mut model, &mut player, &invalid);
    // The picker stays for correction (G1 F11) — the failure is
    // correctable, not fatal.
    assert!(model.modal().is_some(), "the picker stays on a refusal");
    let viewable = model.viewable_error().expect("a refused Open is viewable");
    assert_eq!(viewable.kind(), ErrorPresentationKind::OpenRefused);
    assert_eq!(
        model.status(),
        Some(crate::error_presentation::operation_error_feedback("open", &viewable).as_str()),
        "the status block carries the B1 user copy, not the raw diagnostic"
    );
    assert!(
        player.active_handle().is_none(),
        "a refusal replaces no episode"
    );

    // Close the picker; E then shows the recorded operation error.
    assert_eq!(
        handle_key(key(KeyCode::Esc), &mut model, &mut player),
        Step::Continue
    );
    assert!(model.modal().is_none());
    let focus_before = model.focus();
    assert_eq!(
        handle_key(key(KeyCode::Char('E')), &mut model, &mut player),
        Step::Continue
    );
    let Some(Modal::ErrorDetails { presentation, .. }) = model.modal() else {
        panic!("E must open the details overlay over a viewable error");
    };
    assert_eq!(presentation.kind(), ErrorPresentationKind::OpenRefused);
    assert!(
        presentation
            .technical_detail()
            .unwrap_or_default()
            .contains("no playable audio files found"),
        "the details keep the REAL refusal verbatim: {:?}",
        presentation.technical_detail()
    );
    // Esc closes and restores the pre-modal focus.
    assert_eq!(
        handle_key(key(KeyCode::Esc), &mut model, &mut player),
        Step::Continue
    );
    assert!(model.modal().is_none());
    assert_eq!(
        model.focus(),
        focus_before,
        "Esc restores the pre-modal focus"
    );
}

/// CASE B (ActivationFailedClean): a passing probe whose start fails
/// clean reports the clean-failure copy and fabricates NO playback —
/// no episode exists, so the shell derives no episode failure and the
/// operation error is what stays viewable.
#[test]
fn case_b_a_clean_activation_failure_forges_no_playback() {
    let tree = TempTree::new("b2-failstart");
    let failstart = tree.live_file("failstart.flac");
    let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
    let mut model = TuiModel::new(String::new());

    open_via_keys(&mut model, &mut player, &failstart);
    assert!(
        model.modal().is_some(),
        "only an Opened commit closes the picker"
    );
    assert_eq!(
        model.viewable_error().map(|e| e.kind()),
        Some(ErrorPresentationKind::ActivationFailedClean)
    );
    assert!(
        player.active_handle().is_none(),
        "a clean failure leaves no episode"
    );
    let presentation = model.viewable_error().expect("viewable");
    assert_eq!(
        presentation.technical_detail(),
        Some("decode open failed: decode open failed after a passing probe"),
        "the diagnostic rides the presentation verbatim"
    );

    // The idle shell derives nothing: no episode failure, and the
    // operation error survives the refresh.
    refresh(&mut model, &player);
    assert_eq!(model.episode_error(), None);
    assert_eq!(
        model.viewable_error().map(|e| e.kind()),
        Some(ErrorPresentationKind::ActivationFailedClean),
        "the operation error stays viewable while no episode stands"
    );
    let text = frame_lines(&mut model, 100, 30);
    assert!(
        !text.contains("这首歌曲播放失败"),
        "a clean start failure must not present as an episode failure:\n{text}"
    );
}

/// CASE C (EpisodeFailed): a corrupt episode commits, fails at its
/// first read, and the D11 Failed fact — not any mechanism evidence —
/// is what the shell presents: the Now Playing failure block, and `E`
/// with the real diagnostic.
///
/// Test-quality record (M1 / high semantic risk):
/// PROPERTY: only a committed D11 Failed fact produces the shell's
/// episode-failure presentation. PRECONDITION: the corrupt fake errors
/// on its first read (deterministic); nothing requests a stop and no
/// EOF exists, so Failed is the only legal terminal. LEGAL OUTCOMES:
/// Failed within the bound (else the harness is wedged — a test
/// failure, not a product verdict). OBSERVATION: `observe()` at the
/// settlement boundary, then the shell's derived presentation after a
/// refresh. FALSIFIER: presenting pending/mechanism state as failure,
/// or deriving the error from anything but the fact, fails the kinds
/// and copy assertions.
#[test]
fn case_c_a_failed_episode_presents_from_the_d11_fact() {
    let tree = TempTree::new("b2-corrupt");
    let corrupt = tree.live_file("corrupt.flac");
    let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
    let mut model = TuiModel::new(String::new());

    assert!(matches!(player.open(&corrupt), OpenOutcome::Opened));
    let handle = player.active_handle().expect("committed").clone();
    assert!(wait_for_terminal(&handle, EpisodeTerminalOutcome::Failed));
    refresh(&mut model, &player);

    let error = model.episode_error().expect("a Failed fact presents");
    assert_eq!(error.kind(), ErrorPresentationKind::EpisodeFailed);
    assert_eq!(model.viewable_error(), Some(error.clone()));
    // The status block itself claims nothing new (the failure writes
    // no status); the PANEL carries the failure.
    let text = frame_lines(&mut model, 100, 30);
    assert!(text.contains("这首歌曲播放失败"), "{text}");
    assert!(
        text.contains("E查看技术详情"),
        "the failure block names the E affordance:\n{text}"
    );
    assert!(
        text.contains("Space重新播放"),
        "the manual recovery hints stay available while the shell is \
         not fail-stopped:\n{text}"
    );

    // E shows the REAL diagnostic, display-safe.
    assert_eq!(
        handle_key(key(KeyCode::Char('E')), &mut model, &mut player),
        Step::Continue
    );
    let Some(Modal::ErrorDetails { presentation, .. }) = model.modal() else {
        panic!("E must open over the episode failure");
    };
    assert_eq!(presentation.kind(), ErrorPresentationKind::EpisodeFailed);
    assert!(
        presentation
            .technical_detail()
            .unwrap_or_default()
            .contains("synthetic mid-stream decode failure"),
        "{:?}",
        presentation.technical_detail()
    );
    assert_eq!(
        handle_key(key(KeyCode::Esc), &mut model, &mut player),
        Step::Continue
    );
}

/// CASE D (recovery by replay): the App's `play_current` replacement
/// over the failed episode's SAME file — the display string is
/// unchanged, so the revocation witness is the RUNTIME's
/// `note_episode_replacement`, not any path comparison. The committed
/// replacement closes an open details panel (the panel showed the
/// RETIRED episode's error) and the next refresh's fresh observation
/// derives nothing. The one-loop-iteration staleness window between
/// the commit and the refresh is the loop's ordinary projection
/// cadence (≤ one TICK), shared with position/format.
#[test]
fn case_d_space_replay_replaces_the_episode_and_revokes_the_error() {
    let tree = TempTree::new("b2-replay");
    // The LATE-corrupt variant: the fresh episode after the replay is
    // 200 paced render reads (the fake leg's own 1 ms/block seam) from
    // its failure, while this oracle observes synchronously right
    // after the commit — the immediate-refresh assertion below is
    // therefore deterministic, not a race with the render thread.
    let corrupt = tree.live_file("latecorrupt.flac");
    let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
    let mut model = TuiModel::new(String::new());

    assert!(matches!(player.open(&corrupt), OpenOutcome::Opened));
    let handle = player.active_handle().expect("committed").clone();
    assert!(wait_for_terminal(&handle, EpisodeTerminalOutcome::Failed));
    refresh(&mut model, &player);
    assert!(model.episode_error().is_some());

    // Open the details, then replay: the replacement closes the panel
    // because the error it showed belongs to the RETIRED episode.
    assert_eq!(
        handle_key(key(KeyCode::Char('E')), &mut model, &mut player),
        Step::Continue
    );
    assert!(matches!(model.modal(), Some(Modal::ErrorDetails { .. })));
    assert_eq!(
        dispatch(TuiAction::PlayPause, &mut model, &mut player),
        Step::Continue,
        "PlayPause on a terminal episode routes to the App's play_current"
    );
    assert!(
        model.modal().is_none(),
        "the committed replacement closes the stale details panel"
    );
    refresh(&mut model, &player);
    assert_eq!(model.source(), Some(corrupt.to_string_lossy().as_ref()));
    assert_eq!(
        model.viewable_error(),
        None,
        "the same-file replay revokes the old episode's error"
    );
}

/// CASE E (recovery by navigation): N after a failure registers ONE
/// App-owned burst intent (the #311 coalescing is intact — the key
/// never opens synchronously), and the deadline's single commit moves
/// to the next entry and revokes the error.
#[test]
fn case_e_n_after_a_failure_moves_through_the_burst_and_revokes_the_error() {
    let tree = TempTree::new("b2-nav");
    let corrupt = tree.live_file("corrupt.flac");
    let finite = tree.live_file("finite.flac");
    let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
    let mut model = TuiModel::new(String::new());

    assert!(matches!(player.open(&corrupt), OpenOutcome::Opened));
    player.establish_playlist(vec![corrupt.clone(), finite.clone()]);
    let handle = player.active_handle().expect("committed").clone();
    assert!(wait_for_terminal(&handle, EpisodeTerminalOutcome::Failed));
    refresh(&mut model, &player);
    assert!(model.episode_error().is_some());

    let now = Instant::now();
    assert_eq!(
        handle_key(key(KeyCode::Char('N')), &mut model, &mut player),
        Step::Continue
    );
    refresh(&mut model, &player);
    assert_eq!(
        model.source(),
        Some(corrupt.to_string_lossy().as_ref()),
        "registration opens nothing"
    );
    // The settle instant is anchored on the App's OWN armed deadline —
    // never a wall-clock guess from before the dispatch (the pattern
    // the sibling `n_and_p_route_the_manual_navigation` test froze
    // after a real CI incident).
    let deadline = player
        .pending_navigation_deadline()
        .expect("the burst is armed");
    assert!(
        deadline >= now,
        "the deadline is armed relative to the dispatch"
    );
    settle_timers(&mut model, &mut player, deadline + NAVIGATION_QUIET_WINDOW);
    refresh(&mut model, &player);
    assert_eq!(model.source(), Some(finite.to_string_lossy().as_ref()));
    assert_eq!(
        model.status(),
        Some(format!("navigation: opened {}", finite.display()).as_str())
    );
    assert_eq!(
        model.episode_error(),
        None,
        "the new episode's fresh observation owes the old failure nothing"
    );
}

/// CASE F + the fail-stop gate: a §G.6 latch presents the B1 copy and
/// the banner, and the shell refuses to SEND any open/play/navigate
/// intent to the App — while details, help and quit stay live.
///
/// Test-quality record (M1 / high semantic risk):
/// PROPERTY: after the latch, the shell's OWN gate blocks the action
/// before the App is consulted. PRECONDITION: the violating-cleanup
/// fake latches deterministically on the old-side disposal of the
/// second open (the player matrix's frozen sequence). LEGAL OUTCOMES:
/// the second open is exactly `FailStop`. OBSERVATION: a sentinel
/// planted in the operation-error slot before each gated key — the
/// slot survives every gated key, which proves no App answer ever
/// REPLACED it (the latched App refuses whatever arrives, so an
/// illegal call would still flip the slot to FailStop). That flip is
/// exactly NC-2's red under the gate-bypass mutation. FALSIFIER:
/// removing the gate lets the App answer and the sentinel dies —
/// recorded as the NC-2 mutation run in the PR evidence.
#[test]
fn case_f_a_fail_stopped_shell_sends_no_open_or_play_intents() {
    let tree = TempTree::new("b2-failstop");
    let a = tree.live_file("album live-a.flac");
    let b = tree.live_file("album live-b.flac");
    let mut source = FakeEpisodeSource::new();
    source.violating_cleanup = true;
    let mut player = ReferencePlayerApp::new(source);
    let mut model = TuiModel::new(String::new());

    assert!(matches!(player.open(&a), OpenOutcome::Opened));
    // The second Open's old-side disposal violates: the outcome is
    // FailStop and the latch stands.
    open_via_keys(&mut model, &mut player, &b);
    assert_eq!(
        model.viewable_error().map(|e| e.kind()),
        Some(ErrorPresentationKind::FailStop)
    );
    assert!(
        model.modal().is_some(),
        "the picker stays; only Opened closes it"
    );
    assert_eq!(
        handle_key(key(KeyCode::Esc), &mut model, &mut player),
        Step::Continue
    );
    refresh(&mut model, &player);
    assert!(model.fail_stopped(), "the refresh mirrors the App's latch");

    // The banner leads the status bar; the copy has no retry advice.
    let text = frame_lines(&mut model, 100, 30);
    assert!(
        text.contains("FAIL-STOP：播放器遇到严重的资源清理问题"),
        "{text}"
    );
    assert!(text.contains("资源清理未能安全完成"), "{text}");
    assert!(
        !text.contains("重新播放"),
        "no retry advice over a latch:\n{text}"
    );

    // While the latch stands, E still opens — with the FAIL-STOP copy
    // and the real latch diagnostic: reading stays allowed.
    assert_eq!(
        handle_key(key(KeyCode::Char('E')), &mut model, &mut player),
        Step::Continue
    );
    let Some(Modal::ErrorDetails { presentation, .. }) = model.modal() else {
        panic!("E must work while fail-stopped");
    };
    assert_eq!(presentation.kind(), ErrorPresentationKind::FailStop);
    assert!(
        presentation
            .technical_detail()
            .unwrap_or_default()
            .contains("old episode teardown violated"),
        "{:?}",
        presentation.technical_detail()
    );
    assert_eq!(
        handle_key(key(KeyCode::Esc), &mut model, &mut player),
        Step::Continue
    );

    // THE GATE: with a sentinel planted in the operation-error slot,
    // every gated key must leave BOTH the slot and the App untouched.
    let gate_line = format!("fail-stop: {}", fail_stop_presentation().explanation());
    let plant_sentinel = |model: &mut TuiModel| {
        model.set_operation_error(crate::error_presentation::presentation_for_open(
            &OpenOutcome::Refused {
                diagnostic: "sentinel refusal".to_owned(),
            },
        ));
    };

    plant_sentinel(&mut model);
    assert_eq!(
        handle_key(key(KeyCode::Char('N')), &mut model, &mut player),
        Step::Continue
    );
    assert_eq!(
        model.status(),
        Some(gate_line.as_str()),
        "N is refused at the boundary"
    );
    assert_eq!(
        model.viewable_error().map(|e| e.kind()),
        Some(ErrorPresentationKind::OpenRefused),
        "no App call happened: the sentinel was never replaced"
    );
    assert!(
        player.pending_navigation_deadline().is_none(),
        "no navigation intent was registered"
    );

    plant_sentinel(&mut model);
    assert_eq!(
        dispatch(TuiAction::PlayPause, &mut model, &mut player),
        Step::Continue
    );
    assert_eq!(
        model.viewable_error().map(|e| e.kind()),
        Some(ErrorPresentationKind::OpenRefused),
        "the replay/play intent never reached the App"
    );

    plant_sentinel(&mut model);
    assert_eq!(
        handle_key(key(KeyCode::Char('O')), &mut model, &mut player),
        Step::Continue
    );
    assert!(
        model.modal().is_none(),
        "the Open composition entry is refused"
    );
    assert_eq!(
        model.viewable_error().map(|e| e.kind()),
        Some(ErrorPresentationKind::OpenRefused)
    );

    // Help and quit stay live over the latch.
    assert_eq!(
        handle_key(key(KeyCode::Char('?')), &mut model, &mut player),
        Step::Continue
    );
    assert!(matches!(model.modal(), Some(Modal::Help { .. })));
    assert_eq!(
        handle_key(key(KeyCode::Esc), &mut model, &mut player),
        Step::Continue
    );
    assert_eq!(
        handle_key(
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            &mut model,
            &mut player
        ),
        Step::Exit,
        "Ctrl+C keeps quitting over the latch"
    );
}

/// E is INERT without a viewable error: no panel, no fabricated copy,
/// no status change.
#[test]
fn e_without_any_error_is_fully_inert() {
    let tree = TempTree::new("b2-inert");
    let file = tree.live_file("plain.flac");
    let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
    let mut model = TuiModel::new(String::new());
    assert!(matches!(player.open(&file), OpenOutcome::Opened));
    refresh(&mut model, &player);
    let before = model.status().map(str::to_owned);

    assert_eq!(
        handle_key(key(KeyCode::Char('E')), &mut model, &mut player),
        Step::Continue
    );
    assert!(model.modal().is_none(), "no error, no panel");
    assert_eq!(
        model.status(),
        before.as_deref(),
        "an inert key says nothing"
    );
}

/// The details overlay's scroll is modal-local and a REOPEN starts a
/// fresh snapshot at the top.
#[test]
fn error_details_scrolls_and_a_reopen_resets_the_snapshot() {
    let tree = TempTree::new("b2-scroll");
    let corrupt = tree.live_file("corrupt.flac");
    let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
    let mut model = TuiModel::new(String::new());
    assert!(matches!(player.open(&corrupt), OpenOutcome::Opened));
    let handle = player.active_handle().expect("committed").clone();
    assert!(wait_for_terminal(&handle, EpisodeTerminalOutcome::Failed));
    refresh(&mut model, &player);

    assert_eq!(
        handle_key(key(KeyCode::Char('E')), &mut model, &mut player),
        Step::Continue
    );
    assert_eq!(
        handle_key(key(KeyCode::PageDown), &mut model, &mut player),
        Step::Continue
    );
    let Some(Modal::ErrorDetails { scroll: paged, .. }) = model.modal() else {
        panic!("details open");
    };
    assert!(*paged > 0, "PageDown moves the details viewport");
    assert_eq!(
        handle_key(key(KeyCode::PageUp), &mut model, &mut player),
        Step::Continue
    );
    assert_eq!(
        handle_key(key(KeyCode::Esc), &mut model, &mut player),
        Step::Continue
    );

    // Reopen: scroll starts at the top again (fresh snapshot).
    assert_eq!(
        handle_key(key(KeyCode::Char('E')), &mut model, &mut player),
        Step::Continue
    );
    let Some(Modal::ErrorDetails { scroll, .. }) = model.modal() else {
        panic!("details reopen");
    };
    assert_eq!(
        *scroll, 0,
        "a reopened overlay shows the fresh snapshot's head"
    );
}

/// Invisible Unicode in a diagnostic reaches the details overlay in
/// its DISPLAY-ESCAPED form only — the raw control never renders,
/// while ordinary CJK survives verbatim.
#[test]
fn the_details_overlay_renders_diagnostics_display_safe() {
    let mut model = TuiModel::new(String::new());
    model.update(failed_observation("decode: a\u{200b}b\u{feff}音乐.flac"));
    model.open_modal(ModalKind::ErrorDetails);
    let text = frame_text(&mut model, 100, 30);
    let collapsed = text.replace(' ', "");
    assert!(
        !text.contains('\u{200b}'),
        "the raw zero-width space must not render:\n{text}"
    );
    assert!(
        !text.contains('\u{feff}'),
        "the raw BOM must not render:\n{text}"
    );
    assert!(
        collapsed.contains("\\u{200b}"),
        "the escape is VISIBLE:\n{text}"
    );
    assert!(
        collapsed.contains("\\u{feff}"),
        "the escape is VISIBLE:\n{text}"
    );
    assert!(
        collapsed.contains("音乐.flac"),
        "ordinary CJK survives verbatim:\n{text}"
    );
}

/// The details overlay owns the keyboard the Help way: no transport,
/// open or character key acts on the background behind it, `q` keeps
/// its conventional quit, and the panel survives everything else.
#[test]
fn the_details_overlay_owns_the_keyboard_and_quits_with_q() {
    let tree = TempTree::new("b2-modalkeys");
    let corrupt = tree.live_file("corrupt.flac");
    let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
    let mut model = TuiModel::new(String::new());
    assert!(matches!(player.open(&corrupt), OpenOutcome::Opened));
    let handle = player.active_handle().expect("committed").clone();
    assert!(wait_for_terminal(&handle, EpisodeTerminalOutcome::Failed));
    refresh(&mut model, &player);
    assert_eq!(
        handle_key(key(KeyCode::Char('E')), &mut model, &mut player),
        Step::Continue
    );
    let status_before = model.status().map(str::to_owned);

    for probe in [
        KeyCode::Char('N'),
        KeyCode::Char(' '),
        KeyCode::Char('O'),
        KeyCode::Char('x'),
    ] {
        assert_eq!(
            handle_key(key(probe), &mut model, &mut player),
            Step::Continue
        );
        assert!(
            matches!(model.modal(), Some(Modal::ErrorDetails { .. })),
            "{probe:?} must not act on the background behind the overlay"
        );
        assert_eq!(
            model.status(),
            status_before.as_deref(),
            "{probe:?} performed nothing"
        );
    }
    assert_eq!(model.source(), Some(corrupt.to_string_lossy().as_ref()));

    // `q` inside the overlay keeps quitting.
    assert_eq!(
        handle_key(key(KeyCode::Char('q')), &mut model, &mut player),
        Step::Exit
    );
}

/// NC-1 (Issue #23 B2): the diagnostic's PREFIX must never select the
/// presentation — `decode:`/`device:`/no-prefix diagnostics of the
/// same fact present IDENTICAL user copy; only the verbatim detail
/// differs. This is the oracle the work order's prefix-gating mutation
/// must turn red (mutation executed and recorded in the PR evidence).
///
/// Test-quality record (M1 / high semantic risk):
/// PROPERTY: B1 projects from the FACT (D14.6 outcome / D11 Failed),
/// never by parsing diagnostics. PRECONDITION: three real inputs that
/// differ ONLY in their diagnostic text. LEGAL OUTCOMES: identical
/// kind/title/explanation across all three. OBSERVATION: the pure
/// projection. FALSIFIER: any prefix-keyed branch in the projection
/// flips one row and fails the equality.
#[test]
fn nc1_diagnostic_prefixes_never_select_the_presentation() {
    for diagnostic in [
        "decode: synthetic mid-stream decode failure",
        "device: the endpoint refused to start",
        "no recognizable prefix at all",
    ] {
        let mut model = TuiModel::new(String::new());
        model.update(failed_observation(diagnostic));
        let error = model
            .episode_error()
            .unwrap_or_else(|| panic!("every Failed fact presents: {diagnostic}"));
        assert_eq!(
            error.kind(),
            ErrorPresentationKind::EpisodeFailed,
            "{diagnostic}"
        );
        assert_eq!(error.title(), "这首歌曲播放失败", "{diagnostic}");
        assert_eq!(
            error.explanation(),
            "播放已失败，暂时无法确定具体原因。",
            "{diagnostic}"
        );
        assert_eq!(error.technical_detail(), Some(diagnostic), "{diagnostic}");

        // The same independence on the Open side: a refused Open
        // presents the refusal copy whatever its diagnostic says.
        let refused = crate::error_presentation::presentation_for_open(&OpenOutcome::Refused {
            diagnostic: diagnostic.to_owned(),
        })
        .expect("Refused presents");
        assert_eq!(refused.kind(), ErrorPresentationKind::OpenRefused);
        assert_eq!(refused.title(), "无法打开所选音频");
    }
}

// ----------------------------------------------------------------------
// Owner review of PR #316 (round 1), P2-01 and P2-02 counterfactuals.
// ----------------------------------------------------------------------

/// P2-01, the exact counterfactual through the REAL dispatch boundary:
/// with A's Failed fact standing, a refused Open of B reports B's
/// refusal on the status block AND `E` opens B's details — while A's
/// failure block stays visible on the Now Playing panel. No error
/// history queue: the rule is only which standing error the shell
/// recorded last.
#[test]
fn a_refused_open_after_a_failed_episode_is_what_e_shows() {
    let tree = TempTree::new("b2-recency");
    let corrupt = tree.live_file("corrupt.flac");
    let invalid = tree.live_file("invalid.flac");
    let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
    let mut model = TuiModel::new(String::new());

    // A commits and fails: the D11 Failed fact stands.
    assert!(matches!(player.open(&corrupt), OpenOutcome::Opened));
    let handle = player.active_handle().expect("committed").clone();
    assert!(wait_for_terminal(&handle, EpisodeTerminalOutcome::Failed));
    refresh(&mut model, &player);
    assert_eq!(
        model.episode_error().map(|e| e.kind()),
        Some(ErrorPresentationKind::EpisodeFailed)
    );

    // Attempt to open B: refused; A's episode and its fact are untouched.
    open_via_keys(&mut model, &mut player, &invalid);
    assert!(model.modal().is_some(), "the picker stays for correction");
    assert!(
        player.active_handle().is_some(),
        "a refusal replaces no episode"
    );
    assert_eq!(
        model.viewable_error().map(|e| e.kind()),
        Some(ErrorPresentationKind::OpenRefused),
        "the refusal is the error the shell recorded LAST — E selects it"
    );
    assert_eq!(
        model.episode_error().map(|e| e.kind()),
        Some(ErrorPresentationKind::EpisodeFailed),
        "A's Failed fact still stands for the panel"
    );
    let viewable = model.viewable_error().expect("viewable");
    assert_eq!(
        model.status(),
        Some(crate::error_presentation::operation_error_feedback("open", &viewable).as_str()),
        "the status block carries B's refusal copy"
    );

    // E opens B's details.
    assert_eq!(
        handle_key(key(KeyCode::Esc), &mut model, &mut player),
        Step::Continue
    );
    assert_eq!(
        handle_key(key(KeyCode::Char('E')), &mut model, &mut player),
        Step::Continue
    );
    let Some(Modal::ErrorDetails { presentation, .. }) = model.modal() else {
        panic!("E must open the newest error's details");
    };
    assert_eq!(presentation.kind(), ErrorPresentationKind::OpenRefused);
    assert_eq!(
        handle_key(key(KeyCode::Esc), &mut model, &mut player),
        Step::Continue
    );

    // A's failure block is STILL on the Now Playing panel.
    dispatch(
        TuiAction::Navigate(TuiRoute::NowPlaying),
        &mut model,
        &mut player,
    );
    let text = frame_lines(&mut model, 100, 30);
    assert!(
        text.contains("这首歌曲播放失败"),
        "A's Failed fact stays visible on the panel:\n{text}"
    );
}

/// P2-02 through the REAL dispatch boundary, on the platform where the
/// failure is legal (Linux filenames may contain newlines; Windows
/// forbids them, so the file cannot even be created there — the
/// composition rule itself is pinned cross-platform by the pure unit
/// test in `input.rs`): a path fragment carrying a raw newline is
/// display-escaped BEFORE it joins the status text, so it can never
/// become an extra status row, and the replay's opened line still
/// renders on exactly one row.
#[cfg(unix)]
#[test]
fn a_newline_in_an_opened_path_cannot_become_a_status_row() {
    let tree = TempTree::new("b2-newline-path");
    let file = tree.live_file("new\nlinecorrupt.flac");
    let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
    let mut model = TuiModel::new(String::new());

    // The episode commits, then fails (the corrupt fake); the replay
    // through the transport seam recomposes the opened line over the
    // same newline-bearing path.
    assert!(matches!(player.open(&file), OpenOutcome::Opened));
    let handle = player.active_handle().expect("committed").clone();
    assert!(wait_for_terminal(&handle, EpisodeTerminalOutcome::Failed));
    refresh(&mut model, &player);
    assert_eq!(
        dispatch(TuiAction::PlayPause, &mut model, &mut player),
        Step::Continue,
        "the terminal episode routes to the App's play_current replay"
    );

    let status = model.status().expect("the replay's opened line");
    assert!(status.starts_with("play: opened "), "{status:?}");
    assert_eq!(
        status.lines().count(),
        1,
        "the path's newline must not become a status row: {status:?}"
    );
    assert!(
        status.contains("new\\nlinecorrupt.flac"),
        "the newline is VISIBLE as its escape: {status:?}"
    );

    // And the drawn frame carries the escaped form in the status bar.
    let text = frame_lines(&mut model, 160, 30);
    assert!(text.contains("new\\nlinecorrupt.flac"), "{text}");
}
