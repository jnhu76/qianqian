//! Shared test helpers for the view submodules' unit tests.

use std::time::Duration;

use qianqian_audio_api::ports::PcmFormat;
use qianqian_playback::{EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionObservation};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::draw;
use crate::status::forbidden_status_claim;
use crate::tui::model::{PlaylistRow, TuiModel};

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

pub(crate) fn stopped() -> PlaybackSessionObservation {
    PlaybackSessionObservation {
        terminal_outcome: Some(EpisodeTerminalOutcome::Stopped),
        ..pending()
    }
}

/// The frame text with the tabs ROW removed, for the
/// forbidden-vocabulary scan: the tab name "Now Playing" is
/// navigation chrome (the frozen T0 route name), not a playback
/// claim, and the tabs row is the only place that label renders.
/// Everything else in the frame is scanned verbatim.
pub(crate) fn scan(text: &str) -> Option<&'static str> {
    let mut lines = text.splitn(2, '\n');
    let _tabs_row = lines.next();
    lines.next().and_then(forbidden_status_claim)
}

pub(crate) fn established(model: &mut TuiModel) {
    model.set_episode(Some("D:\\media\\song.flac".to_owned()));
    model.update(PlaybackSessionObservation {
        source_format: Some(PcmFormat {
            sample_rate: 44100,
            channels: 2,
            channel_mask: 0x3,
        }),
        position: Some(44_100 * 86),
        source_duration: Some(Duration::from_secs(383)),
        ..pending()
    });
    model.set_navigation(Some((1, 6)));
    model.set_volume(Some(100));
    model.set_order(crate::playlist::PlaybackOrder::Sequential);
    model.set_repeat(crate::playlist::RepeatMode::Off);
    model.set_playlist(1, || {
        (0..6)
            .map(|n| PlaylistRow {
                label: format!("track-{n}.flac"),
                playing: n == 0,
                selected: n == 0,
            })
            .collect()
    });
}

/// Draw the model at a fixed size on a virtual terminal and return
/// the rows as plain text. The model keeps the published regions.
pub(crate) fn rendered(model: &mut TuiModel, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("virtual terminal");
    terminal.draw(|frame| draw(frame, model)).expect("draw");
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

pub(crate) fn plain_model() -> TuiModel {
    let mut model = TuiModel::new("song.flac");
    established(&mut model);
    model
}
