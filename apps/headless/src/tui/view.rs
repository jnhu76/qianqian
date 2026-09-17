//! ratatui rendering of one [`TuiModel`]: three panels, only earned
//! information.
//!
//! This function is presentation only. It reads the model and draws;
//! it performs no I/O, resolves no capability, and observes nothing —
//! the runtime owns the single `observe()` read per refresh. Rendering
//! is exercised on ratatui's `TestBackend`, so the layout and the exact
//! vocabulary are pinned without a real terminal.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::model::TuiModel;

/// Shown once the episode's terminal Fact is committed: the shell
/// stays up so the outcome can be inspected; only the quit command
/// remains meaningful.
pub const COMMITTED_HINT: &str = "terminal outcome committed; press Q to quit";

/// Render one frame of the reference player.
pub fn draw(frame: &mut Frame, model: &TuiModel) {
    let [main, diagnostics, controls] = Layout::vertical([
        Constraint::Min(6),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .areas(frame.area());
    frame.render_widget(main_panel(model), main);
    frame.render_widget(diagnostics_panel(model), diagnostics);
    frame.render_widget(controls_panel(), controls);
}

fn bold(title: &'static str) -> Span<'static> {
    Span::styled(title, Style::default().add_modifier(Modifier::BOLD))
}

fn main_panel(model: &TuiModel) -> Paragraph<'_> {
    let mut lines = vec![
        Line::from(""),
        Line::from(format!("Source: {}", model.source())),
        Line::from(format!("Format: {}", model.format_label())),
        // Read-side presentation only (D14.8): the position Projection
        // over the source duration evidence. No seek affordance, no
        // gutter, no interaction — the shell displays what the seam
        // observed and owns no position of its own.
        Line::from(format!("Position: {}", model.timeline_label())),
        Line::from(""),
        Line::from(format!("Terminal: {}", model.terminal_label())),
        Line::from(format!(
            "Stop requested: {}",
            model.observation().stop_requested
        )),
        Line::from(format!(
            "Pause requested: {}",
            model.observation().pause_requested
        )),
        Line::from(format!("Paused: {}", model.paused())),
    ];
    if model.terminal_committed() {
        lines.push(Line::from(""));
        lines.push(Line::from(COMMITTED_HINT));
    }
    Paragraph::new(lines).block(
        Block::bordered()
            .title(bold(" Qianqian Reference Player "))
            .title_style(Style::default()),
    )
}

fn diagnostics_panel(model: &TuiModel) -> Paragraph<'_> {
    let mut lines: Vec<Line> = model.diagnostics().into_iter().map(Line::from).collect();
    if lines.is_empty() {
        lines.push(Line::from("(none)"));
    }
    Paragraph::new(lines).block(Block::bordered().title(bold(" Diagnostics ")))
}

fn controls_panel() -> Paragraph<'static> {
    Paragraph::new(vec![Line::from(
        " Space  Pause/Resume    S  Stop    Q  Quit    Ctrl+C  Quit",
    )])
    .block(Block::bordered().title(bold(" Controls ")))
}

#[cfg(test)]
mod tests {
    //! Rendering pinned on ratatui's `TestBackend` (a virtual terminal,
    //! no ANSI buffers): the exact vocabulary, the truth classes, and
    //! the forbidden-semantics negative control from [`crate::status`].

    use super::*;
    use crate::status::forbidden_status_claim;
    use qianqian_audio_api::ports::PcmFormat;
    use qianqian_playback::{EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionObservation};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::time::Duration;

    fn pending() -> PlaybackSessionObservation {
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
        }
    }

    /// Render the model on a fixed-size virtual terminal and return
    /// the rows as plain text. Tall enough for the fully-established
    /// panel (source/format/position/terminal/two command lines/paused
    /// and the committed hint) to stay scannable.
    fn rendered(model: &TuiModel) -> String {
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).expect("virtual terminal");
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

    #[test]
    fn a_fresh_episode_renders_pending_without_inventing_state() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        let text = rendered(&model);
        assert!(text.contains("Source: song.flac"), "{text}");
        assert!(text.contains("Format: pending"), "{text}");
        assert!(
            text.contains("Position: --:-- / --:--"),
            "no timeline evidence yet — and no fabricated zero:\n{text}"
        );
        assert!(text.contains("Terminal: pending"), "{text}");
        assert!(text.contains("Stop requested: false"), "{text}");
        assert!(text.contains("Paused: false"), "{text}");
        // No unearned playback semantic may appear anywhere in the frame
        // (`Paused` is earned since D14.7 and pinned to the frozen
        // establishment conjunction by the model tests).
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    /// The F4 timeline is read-side output only: the four combinations
    /// of the two independent evidence sides render as themselves, with
    /// `--:--` for what does not exist, and the frame adds no seek
    /// affordance (no gutter, no cursor, no arrow-key hint).
    #[test]
    fn the_timeline_line_renders_each_evidential_combination() {
        for (position, duration, expected) in [
            (
                Some(44_100 * 42),
                Some(Duration::from_secs(238)),
                "Position: 00:42 / 03:58",
            ),
            (Some(44_100 * 42), None, "Position: 00:42 / --:--"),
            (
                None,
                Some(Duration::from_secs(238)),
                "Position: --:-- / 03:58",
            ),
            (None, None, "Position: --:-- / --:--"),
        ] {
            let mut model = TuiModel::new("song.flac");
            model.update(PlaybackSessionObservation {
                source_format: Some(PcmFormat {
                    sample_rate: 44100,
                    channels: 2,
                    channel_mask: 0x3,
                }),
                position,
                source_duration: duration,
                ..pending()
            });
            let text = rendered(&model);
            assert!(text.contains(expected), "{expected:?} missing in:\n{text}");
            assert_eq!(forbidden_status_claim(&text), None, "{text}");
        }
    }

    /// The controls panel stays exactly the F3 grammar: no seek key, no
    /// scrub affordance — F4 is read-side.
    #[test]
    fn the_controls_panel_offers_no_seek_affordance() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        let text = rendered(&model);
        for unearned in ["Seek", "seek", "←", "→", "Left", "Right"] {
            assert!(
                !text.contains(unearned),
                "no seek affordance may appear in F4: {unearned:?} in\n{text}"
            );
        }
    }

    /// The Paused line follows the seam's derived projection: recorded
    /// intent without the mechanism evidence must not display Paused;
    /// full establishment must.
    #[test]
    fn the_paused_line_follows_the_frozen_establishment_conjunction() {
        let mut model = TuiModel::new("song.flac");
        model.update(PlaybackSessionObservation {
            pause_requested: true,
            ..pending()
        });
        let text = rendered(&model);
        assert!(text.contains("Pause requested: true"), "{text}");
        assert!(text.contains("Paused: false"), "intent alone: {text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");

        model.update(PlaybackSessionObservation {
            pause_requested: true,
            pause_engagement: PauseEngagement::TailQuiesced,
            ..pending()
        });
        let text = rendered(&model);
        assert!(
            text.contains("Paused: true"),
            "intent + engagement + tail quiescence: {text}"
        );
        assert!(
            !text.contains("Terminal: Paused"),
            "the projection is never a fourth terminal state: {text}"
        );
    }

    #[test]
    fn each_terminal_fact_renders_its_own_label_and_stays_scannable() {
        for (outcome, label) in [
            (EpisodeTerminalOutcome::Completed, "Terminal: Completed"),
            (EpisodeTerminalOutcome::Stopped, "Terminal: Stopped"),
            (EpisodeTerminalOutcome::Failed, "Terminal: Failed"),
        ] {
            let mut model = TuiModel::new("song.flac");
            model.update(PlaybackSessionObservation {
                terminal_outcome: Some(outcome),
                source_format: Some(PcmFormat {
                    sample_rate: 44100,
                    channels: 2,
                    channel_mask: 0x3,
                }),
                ..pending()
            });
            let text = rendered(&model);
            assert!(text.contains(label), "{label:?} missing in:\n{text}");
            assert!(
                text.contains("Format: 44100 Hz, 2 channels, mask 0x3"),
                "{text}"
            );
            // A committed outcome keeps the shell up with a quit hint.
            assert!(
                text.contains(COMMITTED_HINT),
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
            ..pending()
        });
        let text = rendered(&model);
        assert!(text.contains("Stop requested: true"), "{text}");
        assert!(text.contains("Terminal: pending"), "still no Fact: {text}");
        assert_eq!(forbidden_status_claim(&text), None, "no 'stopping' claim");
    }

    #[test]
    fn an_activation_failure_renders_as_a_diagnostic_not_a_forged_fact() {
        let mut model = TuiModel::new("song.flac");
        model.update(PlaybackSessionObservation {
            activation_error: Some("render stream open failed: no device".to_owned()),
            ..pending()
        });
        let text = rendered(&model);
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
            ..pending()
        });
        let text = rendered(&model);
        assert!(text.contains("Terminal: Failed"), "{text}");
        assert!(text.contains("failure: decode: corrupt frame"), "{text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    #[test]
    fn a_diagnostics_free_episode_renders_the_placeholder() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        let text = rendered(&model);
        assert!(text.contains("(none)"), "{text}");
    }
}
