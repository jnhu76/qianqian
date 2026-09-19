//! ratatui rendering of one [`TuiModel`]: three panels, only earned
//! information.
//!
//! This function is presentation only. It reads the model and draws;
//! it performs no I/O, resolves no capability, and observes nothing —
//! the runtime owns the single `observe()` read per refresh. Rendering
//! is exercised on ratatui's `TestBackend`, so the layout and the exact
//! vocabulary are pinned without a real terminal.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::model::TuiModel;

/// Shown once the episode's terminal Fact is committed: the shell
/// stays up so the outcome can be inspected; only the quit command
/// remains meaningful.
pub const COMMITTED_HINT: &str = "terminal outcome committed; press Q to quit";

/// The idle page's one honest claim: no episode exists (a fresh
/// interactive launch, or a clean-failed Open), so nothing is playing
/// and nothing is loaded. The panel fabricates no label beyond it.
pub const NO_MUSIC_LINE: &str = "No music loaded.";

/// Render one frame of the reference player.
pub fn draw(frame: &mut Frame, model: &TuiModel) {
    let [main, diagnostics, controls] = Layout::vertical([
        Constraint::Min(6),
        // Two content rows: an episode can carry BOTH an activation
        // diagnostic and a published failure diagnostic, and a truth-
        // class-correct presentation does not clip one behind the other
        // (Stage-C closure audit C13).
        Constraint::Length(4),
        Constraint::Length(7),
    ])
    .areas(frame.area());
    frame.render_widget(main_panel(model), main);
    frame.render_widget(diagnostics_panel(model), diagnostics);
    frame.render_widget(controls_panel(), controls);
    if model.help_visible() {
        let [help] = Layout::vertical([Constraint::Length(12)]).areas(centered_area(frame.area()));
        // Clear wipes what is underneath so the overlay reads as one
        // panel, not as overprinted text.
        frame.render_widget(Clear, help);
        frame.render_widget(help_panel(), help);
    }
}

/// A centered popup area, degraded honestly on tiny terminals.
fn centered_area(area: Rect) -> Rect {
    let width = area.width.min(64).saturating_sub(4).max(10);
    let height = area.height.min(12);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

fn bold(title: &'static str) -> Span<'static> {
    Span::styled(title, Style::default().add_modifier(Modifier::BOLD))
}

/// The Open operation feedback (application composition feedback,
/// never a playback semantic) and the Open input line while it is
/// active. Shared by both panel shapes: the no-episode state is exactly
/// where typing the next Open must be visible.
fn open_lines(model: &TuiModel, lines: &mut Vec<Line<'_>>) {
    if let Some(status) = model.status() {
        lines.push(Line::from(""));
        // A status BLOCK may be multi-line (the bounded scan-warning
        // detail rides under the opened line, U1 corrective
        // REQUIRED-2); each line renders on its own row.
        for line in status.lines() {
            lines.push(Line::from(line.to_owned()));
        }
    }
    if let Some(input) = model.open_input() {
        lines.push(Line::from(format!(
            "Open: {input}▏  (Enter = open, Esc = cancel)"
        )));
    }
}

fn main_panel(model: &TuiModel) -> Paragraph<'_> {
    let Some(source) = model.source() else {
        // The no-episode panel (F6; the U1 idle page): after a
        // clean-failed Open no runtime remains, and on a no-argument
        // launch none was ever started — the honest frame says exactly
        // that instead of fabricating labels for an episode that does
        // not exist. The Open feedback and input line still render —
        // this is the state the next Open starts from.
        let mut lines = vec![
            Line::from(""),
            Line::from(NO_MUSIC_LINE),
            Line::from("Press O to open a file or folder."),
        ];
        if let Some((position, total)) = model.navigation_position() {
            lines.push(Line::from(format!(
                "Track: {position}/{total} (navigation cursor)"
            )));
        }
        if let Some(volume) = model.volume_label() {
            lines.push(Line::from(format!("Volume: {volume} (desired)")));
        }
        open_lines(model, &mut lines);
        return Paragraph::new(lines).block(
            Block::bordered()
                .title(bold(" Qianqian Reference Player "))
                .title_style(Style::default()),
        );
    };
    let mut lines = vec![
        Line::from(""),
        Line::from(format!("Source: {source}")),
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
    if let Some((position, total)) = model.navigation_position() {
        lines.push(Line::from(format!("Track: {position}/{total}")));
    }
    if let Some(volume) = model.volume_label() {
        lines.push(Line::from(format!("Volume: {volume} (desired)")));
    }
    if model.terminal_committed() {
        lines.push(Line::from(""));
        lines.push(Line::from(COMMITTED_HINT));
    }
    open_lines(model, &mut lines);
    Paragraph::new(lines).block(
        Block::bordered()
            .title(bold(" Qianqian Reference Player "))
            .title_style(Style::default()),
    )
}

/// The keyboard-help overlay (the `?` key): the shipped keys and
/// nothing beyond them. Every line here must stay in lockstep with the
/// frozen grammar — no U2/U3 affordance (list-row selection, exact or
/// large seek, auto-next, shuffle) may be advertised before it exists.
fn help_panel() -> Paragraph<'static> {
    Paragraph::new(vec![
        Line::from(" Qianqian keys"),
        Line::from(""),
        Line::from(" Space        pause / resume"),
        Line::from(" ← / →        seek 5 s back / forward"),
        Line::from(" S            stop"),
        Line::from(" N / P        next / previous track"),
        Line::from(" + / -        volume up / down"),
        Line::from(" O            open a file or folder"),
        Line::from(" ?            close this help"),
        Line::from(" Q / Ctrl+C   quit"),
    ])
    .block(Block::bordered().title(bold(" Help ")))
}

fn diagnostics_panel(model: &TuiModel) -> Paragraph<'_> {
    let mut lines: Vec<Line> = model.diagnostics().into_iter().map(Line::from).collect();
    if lines.is_empty() {
        lines.push(Line::from("(none)"));
    }
    Paragraph::new(lines).block(Block::bordered().title(bold(" Diagnostics ")))
}

fn controls_panel() -> Paragraph<'static> {
    Paragraph::new(vec![
        Line::from(" ←/→  Seek ±5s      Space  Pause/Resume"),
        Line::from(" S  Stop            Q  Quit    Ctrl+C  Quit"),
        Line::from(" O  Open file/folder          ?  Help"),
        Line::from(" N  Next            P  Previous"),
        Line::from(" +  Louder          -  Softer"),
    ])
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

    fn stopped() -> PlaybackSessionObservation {
        PlaybackSessionObservation {
            terminal_outcome: Some(EpisodeTerminalOutcome::Stopped),
            ..pending()
        }
    }

    /// Render the model on a fixed-size virtual terminal and return
    /// the rows as plain text. Tall enough for the fully-established
    /// panel (source/format/position/terminal/two command lines/paused,
    /// the committed hint, the status line and the Open input line) to
    /// stay scannable.
    fn rendered(model: &TuiModel) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).expect("virtual terminal");
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
    /// of the two independent evidence sides render as themselves, and
    /// the timeline line itself adds no seek affordance (no gutter, no
    /// cursor — the fixed-step seek keys live in the controls panel).
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

    /// The controls panel documents the F5-earned seek keys — the
    /// fixed ±5 s step and nothing beyond it: no scrub affordance, no
    /// proportional or unbounded seek vocabulary.
    #[test]
    fn the_controls_panel_documents_the_fixed_step_seek_keys() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        let text = rendered(&model);
        for earned in ["←/→", "Seek ±5s"] {
            assert!(text.contains(earned), "{earned:?} missing in:\n{text}");
        }
        for unearned in ["scrub", "Scrub", "%"] {
            assert!(
                !text.contains(unearned),
                "no scrub affordance may appear: {unearned:?} in\n{text}"
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

    /// The no-episode frame (F6; the U1 idle page): a clean-failed Open
    /// leaves no runtime, and a no-argument launch never started one —
    /// the panel says "No music loaded." and fabricates nothing — no
    /// Source/Format/Position/Terminal labels may render for an episode
    /// that does not exist.
    #[test]
    fn a_no_episode_frame_says_so_and_fabricates_nothing() {
        let mut model = TuiModel::new("song.flac");
        model.set_episode(None);
        let text = rendered(&model);
        assert!(text.contains(NO_MUSIC_LINE), "{text}");
        assert!(text.contains("Press O to open a file or folder"), "{text}");
        for fabricated in ["Source:", "Format:", "Position:", "Terminal:", "Paused:"] {
            assert!(
                !text.contains(fabricated),
                "{fabricated} must not render without an episode:\n{text}"
            );
        }
        // U1 corrective REQUIRED-1: the bare launch's frame carries no
        // operation feedback at all — no fabricated refusal for an
        // Open that was never attempted.
        for fabricated_feedback in ["open refused", "no audio candidates"] {
            assert!(
                !text.contains(fabricated_feedback),
                "{fabricated_feedback:?} must not render for an operation never attempted:\n{text}"
            );
        }
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    /// The Open feedback block may be multi-line (U1 corrective
    /// REQUIRED-2): the opened line counts the scan warnings and each
    /// bounded detail line renders on its own row — a partial folder
    /// scan is visible, never silently discarded. (Rendered at a
    /// 26-row virtual terminal: a fully established episode panel plus
    /// a two-line status block needs the extra row; smaller terminals
    /// degrade by clipping, as everywhere else.)
    #[test]
    fn a_multi_line_status_block_renders_every_line() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        model.set_status(Some(
            "opened /media (2 candidates, 1 skipped; 1 scan warning)\n\
             scan warning: cannot read /media/album-c: access denied"
                .to_owned(),
        ));
        let text = rendered_at(&model, 100, 26);
        assert!(
            text.contains("opened /media (2 candidates, 1 skipped; 1 scan warning)"),
            "{text}"
        );
        assert!(
            text.contains("scan warning: cannot read /media/album-c: access denied"),
            "{text}"
        );
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    /// The no-episode panel is exactly where the next Open starts from:
    /// the operation feedback AND the Open input line render there too,
    /// while the episode labels stay suppressed (review round-1
    /// REQUIRED-1).
    #[test]
    fn the_no_episode_panel_still_renders_open_feedback_and_the_input_line() {
        let mut model = TuiModel::new("song.flac");
        model.set_episode(None);
        model.set_status(Some("open failed (clean): decode open failed".to_owned()));
        let text = rendered(&model);
        assert!(text.contains(NO_MUSIC_LINE), "{text}");
        assert!(
            text.contains("open failed (clean): decode open failed"),
            "the startup Open feedback must be visible: {text}"
        );
        for fabricated in ["Source:", "Format:", "Position:", "Terminal:"] {
            assert!(!text.contains(fabricated), "{fabricated} in\n{text}");
        }

        model.begin_open_input();
        for c in "/media/b.flac".chars() {
            model.open_input_push(c);
        }
        let text = rendered(&model);
        assert!(text.contains("Open: /media/b.flac"), "{text}");
        assert!(text.contains("Enter = open"), "{text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    /// The Open input line renders with its affordances while editing,
    /// and nothing unearned appears.
    #[test]
    fn the_open_input_line_renders_while_editing() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        model.begin_open_input();
        for c in "/media/b.flac".chars() {
            model.open_input_push(c);
        }
        let text = rendered(&model);
        assert!(text.contains("Open: /media/b.flac"), "{text}");
        assert!(text.contains("Enter = open"), "{text}");
        assert!(text.contains("Esc = cancel"), "{text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    /// Open operation feedback renders as composition feedback — a
    /// refusal, a clean failure, or the fail-stop banner — and never
    /// as a playback claim.
    #[test]
    fn open_operation_feedback_renders_as_composition_feedback() {
        for status in [
            "opened /media/b.flac",
            "open refused: unsupported container: /media/x.txt",
            "open failed (clean): decode open failed",
            "FAIL-STOP: old episode teardown violated (§G.6 latch; no exit)",
        ] {
            let mut model = TuiModel::new("song.flac");
            model.update(pending());
            model.set_status(Some(status.to_owned()));
            let text = rendered(&model);
            assert!(text.contains(status), "{status:?} missing in:\n{text}");
            assert_eq!(forbidden_status_claim(&text), None, "{text}");
        }
    }

    /// The navigation projection renders as a Track line in the
    /// episode panel (D14.6; presentation of navigation state only).
    #[test]
    fn the_track_line_renders_the_navigation_projection() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        assert!(
            !rendered(&model).contains("Track:"),
            "no playlist, no Track line"
        );
        model.set_navigation(Some((2, 3)));
        let text = rendered(&model);
        assert!(text.contains("Track: 2/3"), "{text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    /// The no-episode panel keeps the honest navigation line when a
    /// cursor survives a clean failure — labeled a cursor, never a
    /// playback claim (the forbidden-vocabulary scan must stay clean).
    #[test]
    fn the_no_episode_navigation_line_says_not_playing() {
        let mut model = TuiModel::new("song.flac");
        model.set_episode(None);
        model.set_navigation(Some((1, 3)));
        let text = rendered(&model);
        assert!(text.contains("Track: 1/3 (navigation cursor)"), "{text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    /// The volume line renders the App's desired stream factor (D14.9
    /// read side: exactly the configured value, never an acoustic or
    /// mechanism claim).
    #[test]
    fn the_volume_line_renders_the_desired_factor() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        assert!(
            !rendered(&model).contains("Volume:"),
            "before the first refresh the model holds no level and renders none"
        );
        model.set_volume(Some(80));
        let text = rendered(&model);
        assert!(text.contains("Volume: 80/100 (desired)"), "{text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    /// The controls panel documents the O key and the help overlay
    /// without growing a row (the 24-row contract of the committed-hint
    /// test stays intact).
    #[test]
    fn the_controls_panel_documents_the_open_key() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        let text = rendered(&model);
        assert!(text.contains("O  Open file/folder"), "{text}");
        assert!(text.contains("?  Help"), "{text}");
        assert!(text.contains("N  Next"), "{text}");
        assert!(text.contains("P  Previous"), "{text}");
        assert!(text.contains("+  Louder"), "{text}");
        assert!(text.contains("-  Softer"), "{text}");
    }

    /// The help overlay documents exactly the shipped keys — and never
    /// an unearned affordance (U2/U3 vocabulary must not be advertised
    /// before it exists) — then disappears when the model closes it.
    #[test]
    fn the_help_overlay_lists_only_shipped_keys_and_closes() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        assert!(
            !rendered(&model).contains("Qianqian keys"),
            "help hidden by default"
        );
        model.toggle_help();
        let text = rendered(&model);
        assert!(text.contains(" Help "), "{text}");
        for earned in [
            "pause / resume",
            "seek 5 s back / forward",
            "stop",
            "next / previous track",
            "volume up / down",
            "open a file or folder",
            "close this help",
            "quit",
        ] {
            assert!(text.contains(earned), "{earned:?} missing in:\n{text}");
        }
        // U2/U3 affordances are not shipped; none may be advertised.
        for unearned in [
            "shuffle",
            "Shuffle",
            "auto-next",
            "Up",
            "Down",
            "Enter",
            "exact seek",
            "+30",
            "select",
        ] {
            assert!(!text.contains(unearned), "{unearned:?} in:\n{text}");
        }
        assert_eq!(forbidden_status_claim(&text), None, "{text}");

        model.toggle_help();
        assert!(
            !rendered(&model).contains("Qianqian keys"),
            "the overlay closes again"
        );
    }

    /// The help overlay renders over a LIVE episode without corrupting
    /// it: underneath the popup the episode panel is unchanged, and at
    /// tiny sizes the overlay degrades without panicking or fabricating
    /// semantics.
    #[test]
    fn the_help_overlay_survives_live_episodes_and_tiny_terminals() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        model.set_navigation(Some((2, 3)));
        model.set_volume(Some(75));
        model.toggle_help();
        let text = rendered(&model);
        assert!(text.contains("Help"), "{text}");
        assert!(text.contains("Source: song.flac"), "{text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
        for (width, height) in [(40u16, 12u16), (20, 8), (10, 6), (4, 3), (1, 1)] {
            let text = rendered_at(&model, width, height);
            assert_eq!(
                forbidden_status_claim(&text),
                None,
                "unearned semantic at {width}x{height}:\n{text}"
            );
        }
    }

    /// Both diagnostic lines render when an episode carries an
    /// activation diagnostic AND a published failure diagnostic (C13):
    /// the panel grew a second content row, and neither truth-class-
    /// correct presentation text may be clipped behind the other.
    #[test]
    fn both_diagnostic_lines_render_together() {
        let mut model = TuiModel::new("song.flac");
        model.update(PlaybackSessionObservation {
            activation_error: Some("render stream open failed: no device".to_owned()),
            failure_diagnostic: Some("decode: corrupt frame".to_owned()),
            ..pending()
        });
        let text = rendered(&model);
        assert!(
            text.contains("activation: render stream open failed: no device"),
            "{text}"
        );
        assert!(text.contains("failure: decode: corrupt frame"), "{text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    /// Render at an exact terminal size and return the rows as text.
    fn rendered_at(model: &TuiModel, width: u16, height: u16) -> String {
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("virtual terminal");
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

    /// A full draw at a series of shrinking sizes — down to a 4×3
    /// window — never panics and never renders an unearned semantic
    /// (C10). Layout degrades by clipping; the truth classes live in
    /// the model, not in the geometry, so nothing fabricated can
    /// appear merely because fields no longer fit.
    #[test]
    fn tiny_terminals_degrade_without_panicking_or_fabricating() {
        let mut model = TuiModel::new("song.flac");
        model.update(PlaybackSessionObservation {
            activation_error: Some("render stream open failed: no device".to_owned()),
            failure_diagnostic: Some("decode: corrupt frame".to_owned()),
            ..pending()
        });
        // A committed terminal outcome must also ride the shrink loop:
        // the committed-hint line is one more content class that may
        // only clip, never fabricate (closure review T8 note).
        model.update(stopped());
        model.set_navigation(Some((1, 3)));
        model.set_volume(Some(70));
        model.set_status(Some("volume 75/100 (desired)".to_owned()));
        model.begin_open_input();
        for c in "synth30.flac".chars() {
            model.open_input_push(c);
        }
        for (width, height) in [(100u16, 24u16), (40, 12), (20, 8), (10, 6), (4, 3), (1, 1)] {
            let text = rendered_at(&model, width, height);
            assert_eq!(
                forbidden_status_claim(&text),
                None,
                "unearned semantic at {width}x{height}:\n{text}"
            );
        }
    }

    /// A large → small → large resize sequence keeps rendering the
    /// whole shell at every step (C9): no panic, the controls panel
    /// stays present once the window is large enough for it again.
    /// (Physical resize evidence rides on the ConPTY harness, which
    /// resizes the pseudoconsole around every key write; the runtime
    /// has no resize-specific code — the next draw picks up the new
    /// size.)
    #[test]
    fn a_resize_sequence_keeps_rendering_the_whole_shell() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        let large = rendered_at(&model, 100, 24);
        assert!(large.contains("Controls"), "{large}");
        let small = rendered_at(&model, 40, 12);
        assert_eq!(forbidden_status_claim(&small), None, "{small}");
        let large_again = rendered_at(&model, 100, 24);
        assert!(
            large_again.contains("←/→  Seek ±5s"),
            "controls readable again after growing back:\n{large_again}"
        );
    }

    /// A very long source path clips at the panel edge without
    /// corrupting the rows below it or the controls panel (C11): the
    /// display truncates; the model keeps the full internal identity
    /// (never truncated to fit).
    #[test]
    fn a_long_source_path_clips_without_corrupting_other_rows() {
        let long_path = format!(
            "C:\\very\\long\\prefix\\{}\\season.takes.flac",
            "directory_component_".repeat(10)
        );
        let mut model = TuiModel::new(long_path.clone());
        model.update(pending());
        assert_eq!(model.source(), Some(long_path.as_str()), "identity intact");
        let text = rendered_at(&model, 100, 24);
        // The clipped Source line still names the beginning of the
        // path, and the panels BELOW it are uncorrupted.
        assert!(text.contains("Source: C:\\very\\long\\prefix\\"), "{text}");
        assert!(text.contains("Format: pending"), "{text}");
        assert!(text.contains("Terminal: pending"), "{text}");
        assert!(text.contains("←/→  Seek ±5s"), "{text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    /// A CJK filename renders and edits structurally (C12): the Source
    /// line shows the characters, the Open input line accepts CJK
    /// characters and backspace pops ONE character (per-char editing),
    /// and the frame stays free of unearned semantics. Typography
    /// perfection is not claimed — structural correctness is.
    #[test]
    fn cjk_filenames_render_and_edit_structurally() {
        let mut model = TuiModel::new("千曲テスト曲.flac");
        model.update(pending());
        let text = rendered(&model);
        // Wide CJK glyphs occupy two cells; the skipped cells surface as
        // blanks in this test's per-cell text reconstruction (a real
        // terminal renders them as one glyph). Structural assertion is
        // on the space-normalized text: the characters are present, in
        // order, on the right row — and the row did not overflow
        // (`.flac` stayed on it), which is the width-calculation
        // property under test.
        let compact = text.replace(' ', "");
        assert!(compact.contains("Source:千曲テスト曲.flac"), "{text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");

        model.begin_open_input();
        for c in "音楽/千曲.flac".chars() {
            model.open_input_push(c);
        }
        model.open_input_backspace();
        let text = rendered(&model);
        let compact = text.replace(' ', "");
        assert!(compact.contains("Open:音楽/千曲"), "{text}");
        assert!(text.contains("Enter = open"), "{text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }
}
