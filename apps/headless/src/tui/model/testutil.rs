//! Shared test helpers for the model submodules' unit tests.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use qianqian_playback::{PauseEngagement, PlaybackSessionObservation};

use super::{PlaylistRow, TuiModel, TuiRoute};

pub(crate) fn pending() -> PlaybackSessionObservation {
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

pub(crate) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

pub(crate) fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
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
pub(crate) fn model_with_regions(width: u16, height: u16, route: TuiRoute) -> TuiModel {
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
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
        .expect("virtual terminal");
    terminal
        .draw(|frame| crate::tui::view::draw(frame, &mut model))
        .expect("draw");
    model
}
