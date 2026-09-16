//! Reference-player TUI tests: the projection may only render what the
//! F2 seam observes, and the only episode right the keyboard earns is
//! `request_stop`.
//!
//! Rendering runs on ratatui's `TestBackend`, so the exact vocabulary
//! and layout are pinned without a real terminal; the truth-class
//! negative control reuses the status projection's forbidden-vocabulary
//! oracle. Playback episodes themselves are not exercised here — the
//! terminal-Fact contract belongs to the qianqian-playback suite.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use qianqian_audio_api::ports::PcmFormat;
use qianqian_headless::status::forbidden_status_claim;
use qianqian_headless::tui::model::{Action, Step, TuiModel, action_for_key, apply_action};
use qianqian_headless::tui::view;
use qianqian_playback::{
    EpisodeTerminalOutcome, PlaybackSessionHandle, PlaybackSessionObservation,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn pending_observation() -> PlaybackSessionObservation {
    PlaybackSessionObservation {
        terminal_outcome: None,
        failure_diagnostic: None,
        stop_requested: false,
        source_format: None,
        activation_error: None,
    }
}

fn known_format_observation(outcome: Option<EpisodeTerminalOutcome>) -> PlaybackSessionObservation {
    PlaybackSessionObservation {
        terminal_outcome: outcome,
        source_format: Some(PcmFormat {
            sample_rate: 44100,
            channels: 2,
            channel_mask: 0x3,
        }),
        ..pending_observation()
    }
}

/// Render the model on a fixed-size virtual terminal and return the
/// rows as plain text (no ANSI buffers).
fn rendered(model: &TuiModel) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(60, 16)).expect("virtual terminal");
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
        .collect()
}

fn rendered_text(model: &TuiModel) -> String {
    rendered(model).join("\n")
}

// --- the projection renders only earned information ----------------------

#[test]
fn a_fresh_episode_renders_pending_without_inventing_state() {
    let mut model = TuiModel::new("song.flac");
    model.update(pending_observation());
    let text = rendered_text(&model);
    assert!(text.contains("Source: song.flac"), "{text}");
    assert!(text.contains("Format: pending"), "{text}");
    assert!(text.contains("Terminal: pending"), "{text}");
    assert!(text.contains("Stop requested: false"), "{text}");
    // No unearned playback semantic may appear anywhere in the frame.
    assert_eq!(forbidden_status_claim(&text), None, "{text}");
}

#[test]
fn each_terminal_fact_renders_its_own_label_and_stays_scannable() {
    for (outcome, label) in [
        (EpisodeTerminalOutcome::Completed, "Terminal: Completed"),
        (EpisodeTerminalOutcome::Stopped, "Terminal: Stopped"),
        (EpisodeTerminalOutcome::Failed, "Terminal: Failed"),
    ] {
        let mut model = TuiModel::new("song.flac");
        model.update(known_format_observation(Some(outcome)));
        let text = rendered_text(&model);
        assert!(text.contains(label), "{label:?} missing in:\n{text}");
        assert!(
            text.contains("Format: 44100 Hz, 2 channels, mask 0x3"),
            "{text}"
        );
        // A committed outcome keeps the shell up with a quit hint.
        assert!(
            text.contains("terminal outcome committed"),
            "committed hint missing in:\n{text}"
        );
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }
}

#[test]
fn stop_intent_renders_as_command_state_not_as_a_playback_fact() {
    let mut model = TuiModel::new("song.flac");
    model.update(PlaybackSessionObservation {
        stop_requested: true,
        ..pending_observation()
    });
    let text = rendered_text(&model);
    assert!(text.contains("Stop requested: true"), "{text}");
    assert!(text.contains("Terminal: pending"), "still no Fact: {text}");
    assert_eq!(forbidden_status_claim(&text), None, "no 'stopping' claim");
}

#[test]
fn an_activation_failure_renders_as_a_diagnostic_not_a_forged_fact() {
    let mut model = TuiModel::new("song.flac");
    model.update(PlaybackSessionObservation {
        activation_error: Some("render stream open failed: no device".to_owned()),
        ..pending_observation()
    });
    let text = rendered_text(&model);
    assert!(
        text.contains("activation: render stream open failed: no device"),
        "{text}"
    );
    assert!(
        text.contains("Terminal: pending"),
        "no terminal Fact: {text}"
    );
    assert_eq!(forbidden_status_claim(&text), None, "{text}");
}

#[test]
fn a_failed_fact_renders_its_published_diagnostic_separately() {
    let mut model = TuiModel::new("song.flac");
    model.update(PlaybackSessionObservation {
        terminal_outcome: Some(EpisodeTerminalOutcome::Failed),
        failure_diagnostic: Some("decode: corrupt frame".to_owned()),
        ..pending_observation()
    });
    let text = rendered_text(&model);
    assert!(text.contains("Terminal: Failed"), "{text}");
    assert!(text.contains("failure: decode: corrupt frame"), "{text}");
    assert_eq!(forbidden_status_claim(&text), None, "{text}");
}

#[test]
fn a_diagnostics_free_episode_renders_the_placeholder() {
    let mut model = TuiModel::new("song.flac");
    model.update(pending_observation());
    let text = rendered_text(&model);
    assert!(text.contains("(none)"), "{text}");
}

#[test]
fn the_format_stays_pending_until_activation_publishes_one() {
    let mut model = TuiModel::new("song.flac");
    assert_eq!(model.format_label(), "pending");
    model.update(known_format_observation(None));
    assert_eq!(model.format_label(), "44100 Hz, 2 channels, mask 0x3");
    let published = model
        .observation()
        .source_format
        .as_ref()
        .expect("activation published a format");
    assert_eq!(
        model.format_label(),
        format!(
            "{} Hz, {} channels, mask {:#x}",
            published.sample_rate, published.channels, published.channel_mask
        )
    );
}

// --- the keyboard grammar and its single episode right -------------------

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
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
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

/// The stop key maps to the EXISTING request_stop seam — the same
/// frozen right the machine transport uses — and quit never touches
/// the episode.
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
