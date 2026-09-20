//! ratatui rendering of one [`TuiModel`]: four regions, only earned
//! information.
//!
//! ```text
//! playlist      the traversal order with its two independent markers,
//!               windowed so a huge list costs only the visible rows
//! now playing   the committed episode's D14.2/D14.8 read side, the
//!               read-only progress bar, the player's preferences and
//!               the operation feedback / input lines
//! diagnostics   the published activation and failure texts
//! controls      the frozen keymap (Issue #166 §30)
//! ```
//!
//! This function is presentation only. It reads the model and draws;
//! it performs no I/O, resolves no capability, and observes nothing —
//! the runtime owns the single `observe()` read per refresh. Rendering
//! is exercised on ratatui's `TestBackend`, so the layout and the exact
//! vocabulary are pinned without a real terminal.
//!
//! The pane is WYSIWYG by construction: its rows ARE the App's
//! traversal order, so the row below the committed one is exactly the
//! one `N` plays next, and a reorder shows up as a reorder.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::model::{PlaylistRow, TuiModel};

/// Shown once the episode's terminal Fact is committed: the shell
/// stays up so the outcome can be inspected; the player keys stay
/// meaningful (the App's EOF policy may already have moved on to the
/// next entry).
pub const COMMITTED_HINT: &str = "terminal outcome committed";

/// The idle page's one honest claim: no episode exists (a fresh
/// interactive launch, or a clean-failed Open), so nothing is playing
/// and nothing is loaded. The panel fabricates no label beyond it.
pub const NO_MUSIC_LINE: &str = "No music loaded.";

/// The play marker: the committed (playing) row. Deliberately a shape,
/// not a colour — the shell must stay readable in a monochrome
/// terminal (Issue #166 §20).
const PLAYING_MARKER: &str = "▶";

/// The selection marker: the UI cursor row. Independent of the play
/// marker; one row may carry both.
const SELECTED_MARKER: &str = ">";

/// Render one frame of the reference player.
pub fn draw(frame: &mut Frame, model: &TuiModel) {
    // The now-playing budget is its own content (see
    // [`now_playing_lines`]): two border rows on top of every line the
    // panel may show this frame, so the operation feedback and the
    // active input lines are never clipped below the fold on a short
    // terminal. The playlist pane absorbs the rest and degrades by
    // clipping rows — its documented degradation, and the honest trade:
    // a hidden modal line is a keyboard black hole, a scrolled playlist
    // is still a playlist.
    let now_playing_min = now_playing_lines(model).len() + 2;
    let [playlist, now_playing, diagnostics, controls] = Layout::vertical([
        // The pane grows with the terminal; below its floor it degrades
        // by clipping rows (never by drawing the whole list).
        Constraint::Min(3),
        Constraint::Min(now_playing_min as u16),
        // Two content rows: an episode can carry BOTH an activation
        // diagnostic and a published failure diagnostic, and a truth-
        // class-correct presentation does not clip one behind the other
        // (Stage-C closure audit C13).
        Constraint::Length(4),
        // Five content rows: the four keymap rows plus the cancel key
        // (U2 review: `Esc` is part of the frozen keymap and the panel
        // advertised the other four rows).
        Constraint::Length(7),
    ])
    .areas(frame.area());
    frame.render_widget(playlist_panel(model, playlist), playlist);
    frame.render_widget(now_playing_panel(model), now_playing);
    frame.render_widget(diagnostics_panel(model), diagnostics);
    frame.render_widget(controls_panel(), controls);
    if model.help_visible() {
        let popup = centered_area(frame.area());
        let [help] = Layout::vertical([Constraint::Length(popup.height)]).areas(popup);
        // Clear wipes what is underneath so the overlay reads as one
        // panel, not as overprinted text.
        frame.render_widget(Clear, help);
        frame.render_widget(help_panel(), help);
    }
}

/// A centered popup area, degraded honestly on tiny terminals.
fn centered_area(area: Rect) -> Rect {
    let width = area.width.min(60).saturating_sub(4).max(10);
    // The full keymap needs 24 rows; a shorter terminal clips it, which
    // is the same degradation every other region has.
    let height = area.height.min(24);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

fn bold(title: &'static str) -> Span<'static> {
    Span::styled(title, Style::default().add_modifier(Modifier::BOLD))
}

/// The first visible row of the playlist pane: a STATELESS scroll that
/// keeps the selected row inside the window (Issue #166 §21). No scroll
/// offset is kept anywhere — the rule is a pure function of the list
/// length, the selection and the pane height, so no presentation state
/// can drift out of sync with what is on screen, and the committed row
/// is free to scroll away while the user browses.
fn viewport_offset(len: usize, selected: Option<usize>, height: usize) -> usize {
    if height == 0 || len <= height {
        return 0;
    }
    let Some(selected) = selected else {
        return 0;
    };
    if selected < height {
        0
    } else {
        (selected + 1 - height).min(len - height)
    }
}

/// One playlist row: the two markers, the traversal position and the
/// file name.
///
/// `episode_live` is what keeps the committed marker HONEST: after a
/// clean-failed Open (and on a launch that never opened anything) the
/// App's cursor still names the last committed ENTRY — that is
/// navigation state the authority sanctions — but no episode exists, so
/// the row must not carry the play marker. The cursor is still visible
/// as the `Track: n/N (navigation cursor)` line, which says what it is.
fn playlist_row_line(position: usize, row: &PlaylistRow, episode_live: bool) -> Line<'static> {
    let playing = if row.playing && episode_live {
        PLAYING_MARKER
    } else {
        " "
    };
    let selected = if row.selected { SELECTED_MARKER } else { " " };
    Line::from(format!(
        "{playing} {selected} {:>3}  {}",
        position + 1,
        row.label
    ))
}

fn playlist_panel(model: &TuiModel, area: Rect) -> Paragraph<'static> {
    let rows = model.playlist();
    let episode_live = model.source().is_some();
    let height = usize::from(area.height.saturating_sub(2));
    let selected = rows.iter().position(|row| row.selected);
    let offset = viewport_offset(rows.len(), selected, height);
    let lines: Vec<Line<'static>> = rows
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(position, row)| playlist_row_line(position, row, episode_live))
        .collect();
    let mut block = Block::bordered().title(bold(" Playlist "));
    if !rows.is_empty() {
        // The pane's title is the SELECTION ordinal (the committed
        // position is the `Track: <committed>/<len>` line below it, so
        // no reader has to guess which cursor an unlabelled number is).
        let cursor = selected.map(|position| position + 1).unwrap_or(0);
        block = block.title(Line::from(format!(" sel {cursor}/{} ", rows.len())).right_aligned());
    }
    Paragraph::new(lines).block(block)
}

/// The operation feedback (application composition feedback, never a
/// playback semantic) and the Open/GoTo input lines while they are
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
    if let Some(input) = model.goto_input() {
        lines.push(Line::from(format!(
            "Go to: [{input}]  (Enter = seek, Esc = cancel)"
        )));
    }
}

/// The player's preferences, as one line of independent facts: each
/// appears only once the shell has been told it (so an unrefreshed
/// model fabricates none of them).
fn preference_line(model: &TuiModel) -> Option<String> {
    let mut facts = Vec::new();
    if let Some((position, total)) = model.navigation_position() {
        facts.push(format!("Track: {position}/{total}"));
    }
    if let Some(volume) = model.volume_label() {
        facts.push(format!("Volume: {volume} (desired)"));
    }
    if let Some(order) = model.order_label() {
        facts.push(format!("Order: {order}"));
    }
    if let Some(repeat) = model.repeat_label() {
        facts.push(format!("Repeat: {repeat}"));
    }
    if facts.is_empty() {
        return None;
    }
    Some(facts.join("   "))
}

/// Every line the now-playing panel can show, including the operation
/// feedback and input lines. The layout budget in [`draw`] is derived
/// from this exact builder (field round 3: the panel used to be pinned
/// at `Min(10)`, so on a ~30-row terminal a live episode filled the
/// panel completely and the status block and the ACTIVE Open/GoTo input
/// lines rendered below the fold — an invisible modal that swallowed
/// every subsequent keypress). One builder, two readers: the panel
/// cannot claim fewer rows than its own content.
fn now_playing_lines(model: &TuiModel) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    match model.source() {
        // The no-episode panel (F6; the U1 idle page): after a
        // clean-failed Open no runtime remains, and on a no-argument
        // launch none was ever started — the honest frame says exactly
        // that instead of fabricating labels for an episode that does
        // not exist. The operation feedback and input lines still
        // render: this is the state the next Open starts from.
        None => {
            lines.push(Line::from(""));
            lines.push(Line::from(NO_MUSIC_LINE));
            lines.push(Line::from("Press O to open a file or folder."));
            if let Some((position, total)) = model.navigation_position() {
                lines.push(Line::from(format!(
                    "Track: {position}/{total} (navigation cursor)"
                )));
            }
            if let Some(volume) = model.volume_label() {
                lines.push(Line::from(format!("Volume: {volume} (desired)")));
            }
        }
        Some(source) => {
            lines.push(Line::from(""));
            lines.push(Line::from(format!("Source: {source}")));
            lines.push(Line::from(format!("Format: {}", model.format_label())));
            // Read-side presentation only (D14.8): the position
            // Projection over the source duration evidence. No seek
            // affordance, no gutter, no interaction — the shell
            // displays what the seam observed and owns no position of
            // its own.
            lines.push(Line::from(format!("Position: {}", model.timeline_label())));
            if let Some(bar) = model.position_bar_label() {
                lines.push(Line::from(bar));
            }
            let observation = model.observation();
            lines.push(Line::from(format!(
                "Terminal: {}   Stop requested: {}   Pause requested: {}   Paused: {}",
                model.terminal_label(),
                observation.stop_requested,
                observation.pause_requested,
                model.paused(),
            )));
            if let Some(preferences) = preference_line(model) {
                lines.push(Line::from(preferences));
            }
            if model.terminal_committed() {
                lines.push(Line::from(COMMITTED_HINT));
            }
        }
    }
    open_lines(model, &mut lines);
    lines
}

fn now_playing_panel(model: &TuiModel) -> Paragraph<'static> {
    Paragraph::new(now_playing_lines(model)).block(
        Block::bordered()
            .title(bold(" Qianqian Reference Player "))
            .title_style(Style::default()),
    )
}

/// The keyboard-help overlay (the `?` key): the shipped keys and
/// nothing beyond them (Issue #166 §31). Every line here must stay in
/// lockstep with the frozen grammar — no affordance may be advertised
/// before it exists, and none may be missing once it does.
fn help_panel() -> Paragraph<'static> {
    Paragraph::new(vec![
        Line::from(" Qianqian keys"),
        Line::from(""),
        Line::from(" Playlist"),
        Line::from("   ↑ / ↓          select the previous / next row"),
        Line::from("   Enter          play the selected row"),
        Line::from("   N / P          next / previous track"),
        Line::from("   R              order: sequential / shuffle"),
        Line::from("   L              repeat: off / all / one"),
        Line::from(""),
        Line::from(" Playback"),
        Line::from("   Space          pause / resume"),
        Line::from("   ← / →          seek 5 s back / forward"),
        Line::from("   Shift+← / →    seek 30 s back / forward"),
        Line::from("   G              go to an exact position"),
        Line::from("   + / -          volume up / down"),
        Line::from("   S              stop"),
        Line::from(""),
        Line::from(" Application"),
        Line::from("   O              open a file or folder"),
        Line::from("   ?              close this help"),
        Line::from("   Q / Ctrl+C     quit"),
        Line::from("   Esc            cancel / close help"),
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

/// The frozen keymap, on screen (Issue #166 §30). The help overlay
/// carries the same set with the longer descriptions.
fn controls_panel() -> Paragraph<'static> {
    Paragraph::new(vec![
        Line::from(" ↑/↓  Select      Enter  Play       Space  Pause/Resume"),
        Line::from(" ←/→  Seek ±5s    Shift+←/→  Seek ±30s      G  Go to"),
        Line::from(" N/P  Next/Prev   R  Order      L  Repeat    +/-  Volume"),
        Line::from(" O  Open          ?  Help       S  Stop    Q/Ctrl+C  Quit"),
        Line::from(" Esc  Cancel"),
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
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("virtual terminal");
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

    /// Field round 3, problem 2: the operation feedback block and the
    /// ACTIVE input lines render inside the panel on a short terminal.
    /// The panel used to be pinned at `Min(10)`, so with a live episode
    /// on a ~30-row terminal it was exactly full and an activated
    /// Open/GoTo line rendered below the fold — an invisible modal that
    /// swallowed every subsequent keypress (the field "keyboard is
    /// dead" report). Pinned at the dogfood size AND at the field
    /// size, over the FULL live panel (preference line included).
    #[test]
    fn the_active_input_lines_and_status_stay_visible_on_a_short_terminal() {
        for (cols, rows) in [(120u16, 30u16), (120, 40)] {
            for (modal, expected) in [
                ("goto", "Go to: ["),
                ("open", "Open: "),
            ] {
                let mut model = TuiModel::new("song.flac");
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
                    vec![PlaylistRow {
                        label: "song.flac".to_owned(),
                        playing: true,
                        selected: true,
                    }]
                });
                model.set_status(Some("opened D:\\media (6 candidates…)".to_owned()));
                match modal {
                    "goto" => model.begin_goto_input(),
                    "open" => model.begin_open_input(),
                    other => unreachable!("{other}"),
                }
                let mut terminal =
                    Terminal::new(TestBackend::new(cols, rows)).expect("virtual terminal");
                terminal.draw(|frame| draw(frame, &model)).expect("draw");
                let buffer = terminal.backend().buffer().clone();
                let text = (0..buffer.area.height)
                    .map(|y| {
                        (0..buffer.area.width)
                            .filter_map(|x| {
                                buffer.cell((x, y)).map(|c| c.symbol().to_string())
                            })
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    text.contains(expected),
                    "{modal} line clipped at {cols}x{rows}:\n{text}"
                );
                assert!(
                    text.contains("opened D:\\media (6 candidates…)"),
                    "status block clipped at {cols}x{rows}:\n{text}"
                );
                assert!(
                    text.contains("Track: 1/6"),
                    "preference line lost at {cols}x{rows}:\n{text}"
                );
            }
        }
    }

    /// Field round 5, the layout invariant under the pending-start
    /// policy: the now-playing panel renders the SAME number of rows
    /// with the position sample pending as it does mid-play. A panel
    /// that shrinks during the switch transient lets the layout grow
    /// the playlist pane by a row — the field saw that as a blank line
    /// appearing after the last track on every `N` press.
    #[test]
    fn the_panel_row_count_is_stable_across_the_switch_transient() {
        let base = |position: Option<u64>| {
            let mut model = TuiModel::new("song.flac");
            model.set_episode(Some("D:\\media\\song.flac".to_owned()));
            model.update(PlaybackSessionObservation {
                source_format: Some(PcmFormat {
                    sample_rate: 44100,
                    channels: 2,
                    channel_mask: 0x3,
                }),
                position,
                source_duration: Some(Duration::from_secs(238)),
                ..pending()
            });
            model.set_navigation(Some((2, 6)));
            model.set_volume(Some(100));
            model.set_order(crate::playlist::PlaybackOrder::Sequential);
            model.set_repeat(crate::playlist::RepeatMode::Off);
            model.set_status(Some("next: opened D:\\media\\song.flac".to_owned()));
            model
        };
        assert_eq!(
            now_playing_lines(&base(Some(44_100 * 42))).len(),
            now_playing_lines(&base(None)).len(),
            "the pending-start frame must render exactly as many panel rows as a mid-play frame"
        );
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
                "Position: 00:00 / 03:58",
            ),
            (None, None, "Position: 00:00 / --:--"),
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

    /// The pane's committed marker is a PLAY claim, so it may only
    /// appear while an episode is live. After a clean-failed Open the
    /// App's cursor still names the last committed entry (navigation
    /// state, which the D14.6 authority sanctions and the panel labels
    /// as such) — but the row must not carry `▶`, because no episode
    /// exists for it to be playing.
    #[test]
    fn the_pane_shows_no_playing_marker_when_no_episode_is_live() {
        let mut model = TuiModel::new("song.flac");
        model.set_playlist(1, || {
            vec![
                row("flac4.flac", true, true),
                row("alac4.m4a", false, false),
            ]
        });
        model.set_navigation(Some((1, 2)));
        // A live episode: the committed row carries the marker.
        let text = rendered(&model);
        assert!(text.contains("▶ >   1  flac4.flac"), "{text}");

        // The same navigation state with no live episode (a clean-failed
        // Open): the cursor is still shown, the play marker is not.
        model.set_episode(None);
        let text = rendered(&model);
        assert!(!text.contains(PLAYING_MARKER), "{text}");
        assert!(text.contains("  >   1  flac4.flac"), "{text}");
        assert!(text.contains("Track: 1/2 (navigation cursor)"), "{text}");
        assert!(text.contains(NO_MUSIC_LINE), "{text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
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
        let text = rendered_at(&model, 100, 32);
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

    /// The controls panel documents the whole frozen keymap (Issue #166
    /// §30) — every key the shell acts on, and no other affordance.
    #[test]
    fn the_controls_panel_documents_the_frozen_keymap() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        let text = rendered(&model);
        for earned in [
            "↑/↓  Select",
            "Enter  Play",
            "Space  Pause/Resume",
            "Seek ±5s",
            "Seek ±30s",
            "G  Go to",
            "N/P  Next/Prev",
            "R  Order",
            "L  Repeat",
            "+/-  Volume",
            "O  Open",
            "?  Help",
            "S  Stop",
            "Q/Ctrl+C  Quit",
            // The frozen keymap's cancellation key (U2 review: the
            // panel advertised every other key of the frozen set).
            "Esc  Cancel",
        ] {
            assert!(text.contains(earned), "{earned:?} missing in:\n{text}");
        }
        for unearned in ["mouse", "Mouse", "drag", "M3U"] {
            assert!(!text.contains(unearned), "{unearned:?} in:\n{text}");
        }
    }

    /// The help overlay documents EXACTLY the shipped keys (Issue #166
    /// §31) — nothing missing, nothing unearned — and closes again.
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
            "select the previous / next row",
            "play the selected row",
            "next / previous track",
            "order: sequential / shuffle",
            "repeat: off / all / one",
            "pause / resume",
            "seek 5 s back / forward",
            "seek 30 s back / forward",
            "go to an exact position",
            "volume up / down",
            "stop",
            "open a file or folder",
            "close this help",
            "quit",
            "cancel / close help",
        ] {
            assert!(text.contains(earned), "{earned:?} missing in:\n{text}");
        }
        // Not shipped: none of it may be advertised.
        for unearned in [
            "M3U",
            "mouse",
            "Mouse",
            "drag",
            "Scrub",
            "scrub",
            "lyrics",
            "album art",
            "visualizer",
            "equalizer",
            "library",
            "favorites",
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

    /// QUICKSTART ↔ `?` overlay consistency (Listening Release): every
    /// key the shipped help file documents must be advertised by the
    /// on-screen overlay, and nothing beyond the shipped set. The
    /// usage-text side of the same agreement lives in
    /// `tests/quickstart_usage.rs`; this test renders the ACTUAL
    /// overlay and reads the ACTUAL QUICKSTART.md, so the two surfaces
    /// cannot drift apart silently.
    #[test]
    fn the_help_overlay_advertises_every_quickstart_key() {
        let quickstart_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../QUICKSTART.md");
        let quickstart = std::fs::read_to_string(&quickstart_path)
            .unwrap_or_else(|e| panic!("QUICKSTART.md readable: {e}"));
        // The overlay's spelling of each QUICKSTART key row.
        let overlay_needles = [
            ("↑ / ↓", "↑ / ↓"),
            ("Enter", "Enter"),
            ("N / P", "N / P"),
            ("R", "R              order"),
            ("L", "L              repeat"),
            ("Space", "Space"),
            ("← / →", "← / →"),
            ("Shift+← / Shift+→", "Shift+← / →"),
            ("G", "G              go to"),
            ("+ / -", "+ / -"),
            ("S", "S              stop"),
            ("O", "O              open"),
            ("?", "?              close"),
            ("Esc", "Esc"),
            ("Q", "Q / Ctrl+C"),
            ("Ctrl+C", "Ctrl+C"),
        ];
        let mut in_key_section = false;
        let mut documented = Vec::new();
        for line in quickstart.lines() {
            if line.starts_with("## ") {
                in_key_section = line.trim() == "## Keys";
                continue;
            }
            if !in_key_section || !line.starts_with('|') || line.contains("----") {
                continue;
            }
            let first = line.split('|').nth(1).unwrap_or("").trim();
            if first.is_empty() || first.starts_with("Key") {
                continue;
            }
            documented.push(first.trim_matches('`').to_owned());
        }
        assert_eq!(
            documented.len(),
            overlay_needles.len(),
            "QUICKSTART key table has {documented:?}; keep it in lockstep with \
             the overlay and this test"
        );

        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        model.toggle_help();
        let text = rendered(&model);
        for (quickstart_key, overlay_needle) in overlay_needles {
            assert!(
                documented.iter().any(|key| key == quickstart_key),
                "{quickstart_key:?} missing from QUICKSTART's key table"
            );
            assert!(
                text.contains(overlay_needle),
                "the overlay does not advertise {quickstart_key:?} \
                 (expected {overlay_needle:?}):\n{text}"
            );
        }
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    fn row(label: &str, playing: bool, selected: bool) -> PlaylistRow {
        PlaylistRow {
            label: label.to_owned(),
            playing,
            selected,
        }
    }

    /// The pane marks the committed row and the selected row with two
    /// SEPARATE indicators, and one row can carry both (Issue #166 §20).
    /// Monochrome-readable by construction: the markers are shapes.
    #[test]
    fn the_playlist_pane_marks_playing_and_selected_independently() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        model.set_playlist(1, || {
            vec![
                row("01 Intro.flac", true, false),
                row("02 Nocturne.flac", false, true),
                row("03 Common.flac", false, false),
                row("04 Both.flac", true, true),
            ]
        });
        let text = rendered(&model);
        let rows: Vec<&str> = text
            .lines()
            .filter(|line| {
                ["Intro.flac", "Nocturne.flac", "Common.flac", "Both.flac"]
                    .iter()
                    .any(|label| line.contains(label))
            })
            .collect();
        assert_eq!(rows.len(), 4, "every row renders once:\n{text}");
        assert!(
            rows[0].contains("▶") && rows[0].contains("  1"),
            "the committed row carries the play marker: {:?}",
            rows[0]
        );
        assert!(
            !rows[0].contains("> ") && !rows[0].contains(" >"),
            "and not the selection marker: {:?}",
            rows[0]
        );
        assert!(
            rows[1].contains(">") && !rows[1].contains("▶"),
            "the selected row carries the selection marker only: {:?}",
            rows[1]
        );
        assert!(
            !rows[2].contains("▶") && !rows[2].contains(">"),
            "an unmarked row carries neither: {:?}",
            rows[2]
        );
        assert!(
            rows[3].contains("▶") && rows[3].contains(">"),
            "a row can be both committed and selected: {:?}",
            rows[3]
        );
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    /// The pane windows a large playlist (Issue #166 §21): only the
    /// visible rows are drawn, the SELECTED row is always inside the
    /// window, and the committed row may scroll away while the user
    /// browses elsewhere.
    #[test]
    fn the_playlist_pane_windows_a_large_list_around_the_selection() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        model.set_navigation(Some((1, 5000)));
        model.set_playlist(1, || {
            (1..=5000)
                .map(|n| row(&format!("track-{n:04}.flac"), n == 1, n == 4200))
                .collect()
        });
        let text = rendered_at(&model, 100, 30);
        assert!(
            text.contains("track-4200.flac"),
            "the selected row is always visible:\n{text}"
        );
        assert!(
            !text.contains("track-0001.flac"),
            "the committed row is allowed to scroll away:\n{text}"
        );
        assert!(
            !text.contains("track-4999.flac"),
            "rows outside the window are not drawn:\n{text}"
        );
        let drawn = text.lines().filter(|line| line.contains("track-")).count();
        assert!(
            drawn <= 30,
            "the pane draws at most its own height in rows, got {drawn}:\n{text}"
        );
        assert!(drawn > 1, "it still draws a window:\n{text}");
        assert!(
            text.contains("4200/5000"),
            "the pane's own cursor position is visible:\n{text}"
        );
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    /// Selection scrolling follows the cursor and returns to the top with
    /// it — the offset is a pure function of the selection, so nothing
    /// can drift out of sync (Issue #166 §21).
    #[test]
    fn the_pane_viewport_follows_the_selection_up_and_down() {
        for (selected, expected) in [
            (None, 0usize),
            (Some(0), 0),
            (Some(3), 0),
            (Some(4), 1),
            (Some(9), 6),
            (Some(19), 16),
        ] {
            assert_eq!(
                viewport_offset(20, selected, 4),
                expected,
                "selection {selected:?}"
            );
        }
        // A list that fits, an empty list and a zero-height pane all sit
        // at the top.
        assert_eq!(viewport_offset(3, Some(2), 4), 0);
        assert_eq!(viewport_offset(0, None, 4), 0);
        assert_eq!(viewport_offset(20, Some(19), 0), 0);
    }

    /// CJK and very long file names render structurally in the pane: the
    /// characters survive, the row does not corrupt the rows around it,
    /// and a long name clips at the pane edge (the model keeps the full
    /// identity).
    #[test]
    fn the_playlist_pane_renders_cjk_and_long_names_structurally() {
        let long = format!("{}.flac", "long_name_".repeat(20));
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        model.set_playlist(1, || {
            vec![
                row("夜曲.flac", true, true),
                row(&long, false, false),
                row("plain.mp3", false, false),
            ]
        });
        let text = rendered_at(&model, 100, 30);
        // Wide glyphs leave a skip cell in this test's per-cell
        // reconstruction; the assertion is on the space-normalized text.
        let compact = text.replace(' ', "");
        assert!(compact.contains("夜曲.flac"), "{text}");
        assert!(compact.contains("plain.mp3"), "{text}");
        // The long row clips at the pane edge and never eats a later row.
        assert!(text.contains("long_name_long_name_"), "{text}");
        let rows: Vec<&str> = text.lines().filter(|line| line.contains("  3  ")).collect();
        assert_eq!(rows.len(), 1, "the third row rendered once:\n{text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    /// The pane is empty and honest before any playlist exists, and the
    /// frame renders the idle page in the panel below it (Issue #166
    /// §37: the empty state stays valid).
    #[test]
    fn an_empty_playlist_renders_no_rows_and_no_fabricated_position() {
        let mut model = TuiModel::new("song.flac");
        model.set_episode(None);
        let text = rendered(&model);
        assert!(text.contains(" Playlist "), "{text}");
        assert!(text.contains(NO_MUSIC_LINE), "{text}");
        assert!(!text.contains("0/0"), "no fabricated position:\n{text}");
        assert_eq!(forbidden_status_claim(&text), None, "{text}");
    }

    /// The live pre-first-sample window renders at the START (field
    /// round 5, the switch-transient layout fix); the negative control
    /// is the SETTLED episode, which keeps the honest dashes — a dead
    /// timeline has no start, and nothing is fabricated there.
    #[test]
    fn the_timeline_keeps_the_dashes_once_settled_without_evidence() {
        let mut model = TuiModel::new("song.flac");
        model.update(PlaybackSessionObservation {
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            source_duration: Some(Duration::from_secs(238)),
            terminal_outcome: Some(EpisodeTerminalOutcome::Stopped),
            ..pending()
        });
        let text = rendered(&model);
        assert!(
            text.contains("Position: --:-- / 03:58"),
            "a settled episode fabricates no start:\n{text}"
        );
        assert!(
            !text.contains('━') && !text.contains('╸'),
            "a settled episode renders no bar:\n{text}"
        );
    }

    /// The read-only progress bar renders as its own line exactly when
    /// both sides have evidence, and the Position line keeps reporting
    /// the honest `--:--` side otherwise (Issue #166 §33).
    #[test]
    fn the_progress_bar_line_appears_only_with_both_sides() {
        let mut model = TuiModel::new("song.flac");
        model.update(PlaybackSessionObservation {
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            position: Some(44_100 * 42),
            source_duration: Some(Duration::from_secs(238)),
            ..pending()
        });
        let text = rendered(&model);
        assert!(
            text.contains("Position: 00:42 / 03:58"),
            "the D14.8 projection stays: {text}"
        );
        assert!(
            text.contains("00:42 ") && text.contains(" 03:58"),
            "the bar carries both times: {text}"
        );
        assert!(text.contains('━'), "{text}");

        // Unknown duration: the Position line says so and NO bar is
        // fabricated.
        model.update(PlaybackSessionObservation {
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            position: Some(44_100 * 42),
            ..pending()
        });
        let text = rendered(&model);
        assert!(text.contains("Position: 00:42 / --:--"), "{text}");
        assert!(
            !text.contains('━') && !text.contains('╸'),
            "no bar without a known duration: {text}"
        );

        // Unknown position on a LIVE episode: the pre-first-sample
        // window (field round 5) — the bar renders at its START, empty
        // but present, so the row never vanishes mid-playback.
        model.update(PlaybackSessionObservation {
            source_format: Some(PcmFormat {
                sample_rate: 44100,
                channels: 2,
                channel_mask: 0x3,
            }),
            source_duration: Some(Duration::from_secs(238)),
            ..pending()
        });
        let text = rendered(&model);
        assert!(text.contains("Position: 00:00 / 03:58"), "{text}");
        assert!(
            !text.contains('━'),
            "the start bar carries no filled cells: {text}"
        );
        assert!(text.contains('╸'), "the bar frame is present: {text}");
    }

    /// The order and repeat labels render exactly the frozen vocabulary
    /// (Issue #166 §12/§25) — "Sequential"/"Shuffle", never "Random" —
    /// and each appears only once the shell has been told it.
    #[test]
    fn the_order_and_repeat_labels_render_the_frozen_vocabulary() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        let text = rendered(&model);
        assert!(!text.contains("Order:"), "{text}");
        assert!(!text.contains("Repeat:"), "{text}");

        for (order, label) in [
            (
                crate::playlist::PlaybackOrder::Sequential,
                "Order: Sequential",
            ),
            (crate::playlist::PlaybackOrder::Shuffle, "Order: Shuffle"),
        ] {
            model.set_order(order);
            let text = rendered(&model);
            assert!(text.contains(label), "{label:?} missing in:\n{text}");
            assert!(!text.contains("Random"), "never Random:\n{text}");
        }
        for (repeat, label) in [
            (crate::playlist::RepeatMode::Off, "Repeat: Off"),
            (crate::playlist::RepeatMode::All, "Repeat: All"),
            (crate::playlist::RepeatMode::One, "Repeat: One"),
        ] {
            model.set_repeat(repeat);
            let text = rendered(&model);
            assert!(text.contains(label), "{label:?} missing in:\n{text}");
        }
        assert_eq!(forbidden_status_claim(&rendered(&model)), None, "clean");
    }

    /// The GoTo modal renders as the exact-seek adapter it is: the typed
    /// token and its two affordances, with no unearned seek vocabulary
    /// around it (Issue #166 §27).
    #[test]
    fn the_goto_modal_renders_its_line_and_affordances() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        model.begin_goto_input();
        for c in "01:35".chars() {
            model.goto_input_push(c);
        }
        let text = rendered(&model);
        assert!(text.contains("Go to: [01:35]"), "{text}");
        assert!(text.contains("Enter = seek"), "{text}");
        assert!(text.contains("Esc = cancel"), "{text}");
        for unearned in ["Seeking", "seeking", "Seek complete", "scrub"] {
            assert!(!text.contains(unearned), "{unearned:?} in:\n{text}");
        }
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
