//! The Now Playing route: the episode read-side panel with the
//! desired-DSP summary line, the click-to-position seek bar, the
//! discoverable relative-seek buttons, the transport row with the
//! visible Open control, and the preference row.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};

use super::{COMMITTED_HINT, NO_MUSIC_LINE, bold};
use crate::playlist::{PlaybackOrder, RepeatMode};
use crate::tui::model::{
    HitRegion, HitTarget, PreferenceButton, SEEK_BUTTONS, TRANSPORT, TransportButton, TuiModel,
};

/// The Now Playing route (G1): the episode read-side panel with the
/// desired-DSP summary line, the click-to-position seek bar, the
/// discoverable relative-seek buttons, the transport row with the
/// visible Open control, and the preference row (volume steppers,
/// order, repeat) — every core player function as a visible control
/// that keyboard focus and the mouse both reach.
pub(super) fn draw_now_playing(
    frame: &mut Frame,
    model: &TuiModel,
    area: Rect,
    regions: &mut Vec<HitRegion>,
) {
    let [content, bar, seek, transport, preferences] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .areas(area);

    let compact = model.compact_layout();
    let mut lines: Vec<Line<'static>> = Vec::new();
    match model.source() {
        // The no-episode panel (F6; the U1 idle page): after a
        // clean-failed Open no runtime remains, and on a no-argument
        // launch none was ever started — the honest frame says exactly
        // that instead of fabricating labels for an episode that does
        // not exist. The Open control below is always live.
        None => {
            lines.push(Line::from(""));
            lines.push(Line::from(NO_MUSIC_LINE));
            lines.push(Line::from(
                "Open a file or folder with the Open button below.",
            ));
            if let Some((position, total)) = model.navigation_position() {
                lines.push(Line::from(format!(
                    "Track: {position}/{total} (navigation cursor)"
                )));
            }
        }
        Some(source) => {
            lines.push(Line::from(""));
            lines.push(Line::from(format!("Source: {source}")));
            lines.push(Line::from(format!("Format: {}", model.format_label())));
            // Read-side presentation only (D14.8). The bar below is
            // the same projection with a click-to-position affordance
            // wherever duration evidence exists.
            lines.push(Line::from(format!("Position: {}", model.timeline_label())));
            let observation = model.observation();
            lines.push(Line::from(format!(
                "Terminal: {}   Stop requested: {}   Pause requested: {}   Paused: {}",
                model.terminal_label(),
                observation.stop_requested,
                observation.pause_requested,
                model.paused(),
            )));
            if let Some((position, total)) = model.navigation_position() {
                lines.push(Line::from(format!("Track: {position}/{total}")));
            }
            if model.terminal_committed() {
                lines.push(Line::from(COMMITTED_HINT));
            }
            // Diagnostics ride the same panel: an episode can carry BOTH
            // an activation diagnostic and a published failure
            // diagnostic, and neither may clip behind the other (C13).
            for diagnostic in model.diagnostics() {
                lines.push(Line::from(diagnostic));
            }
        }
    }
    // The desired-DSP summary is the App's DESIRED configuration line —
    // its product lifetime is independent of any active episode (T0:
    // a validated desired change with no episode configures the next
    // one), so it renders on the idle page too (G1 F14). It stays
    // secondary detail (G5 §39): it yields to the core controls under
    // constrained layouts before any of them does.
    if !compact && let Some(dsp) = model.desired_dsp_label() {
        lines.push(Line::from(dsp.to_owned()));
    }
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(bold(" Qianqian Reference Player "))
                .title_style(Style::default()),
        ),
        content,
    );

    // The seek bar row (G1 §9): a click-to-position affordance over
    // the D14.8 display. It publishes a region ONLY while duration
    // evidence exists — an unknown timeline is never clickable and
    // never fabricates a target. The region is exactly the drawn
    // bar GLYPH cells (not the whole row): the click fraction and the
    // painted marker come from the same geometry, so a click where the
    // bar looks full is a click near the end. The row itself always
    // occupies its line, so the layout does not flex as evidence
    // arrives.
    if let Some(bar_label) = model.position_bar_label() {
        frame.render_widget(Paragraph::new(bar_label.clone()).centered(), bar);
        if model
            .observation()
            .source_duration
            .is_some_and(|duration| !duration.is_zero())
            && let Some(glyph) = seek_bar_glyph_area(&bar_label, bar)
        {
            regions.push(HitRegion {
                area: glyph,
                target: HitTarget::SeekBar,
            });
        }
    }

    /// The drawn bar glyph's cells inside the seek-bar row, from the SAME
    /// centering the painter used: the label (`mm:ss ━━━╸─── mm:ss`) is
    /// single-width characters, so its character count is its cell width,
    /// and the glyph is the [`BAR_WIDTH`]-cell run starting at the first
    /// box-drawing character. `None` when the row cannot show the whole
    /// label — a clipped bar is a display, not a scrub strip.
    fn seek_bar_glyph_area(bar_label: &str, bar: Rect) -> Option<Rect> {
        let label_cells = bar_label.chars().count();
        if label_cells >= bar.width as usize {
            return None;
        }
        let glyph_offset = bar_label
            .char_indices()
            .find(|(_, character)| matches!(character, '━' | '╸' | '─'))?
            .0;
        Some(Rect::new(
            // The painter's EXACT centering (ratatui's Paragraph): the
            // click mapping and the painted cells must never disagree
            // by a cell at odd label lengths.
            bar.x + (bar.width / 2).saturating_sub(label_cells as u16 / 2) + glyph_offset as u16,
            bar.y,
            crate::tui::model::BAR_WIDTH as u16,
            1,
        ))
    }

    // The seek row (G1 F07, the T0 input-parity freeze): the visible,
    // focusable, clickable relative-seek controls. They render ONLY
    // while the episode publishes the evidence a relative seek is
    // computed from (position + sample rate) — without evidence the
    // row stays empty, publishes no regions, and holds no focus stop;
    // an invisible control is never an interactive one. The row's slot
    // is always reserved so the layout does not flex as evidence
    // arrives.
    if model.relative_seek_available() {
        let cells: [Rect; 2] = Layout::horizontal([Constraint::Ratio(1, 2); 2]).areas(seek);
        for (button, cell) in SEEK_BUTTONS.iter().zip(cells.iter()) {
            let claimed = model.claims(&HitTarget::Seek(*button));
            let paragraph =
                Paragraph::new(Line::from(format!("[ {} ]", button.label())).centered());
            frame.render_widget(
                if claimed {
                    paragraph.style(Style::default().add_modifier(Modifier::REVERSED))
                } else {
                    paragraph
                },
                *cell,
            );
            regions.push(HitRegion {
                area: *cell,
                target: HitTarget::Seek(*button),
            });
        }
    }

    // The transport row (§36, G1): Open joins the four transport
    // buttons — the picker entry is a core player control, not a
    // shortcut reserved for those who read the help. The central
    // control is CONTEXT-LABELED from the model's fresh offer (T0
    // transport freeze): Pause / Resume / Play.
    let buttons: [Rect; 5] = Layout::horizontal([Constraint::Ratio(1, 5); 5]).areas(transport);
    let play_pause_label = model.play_pause_offer().label();
    for (button, cell) in TRANSPORT.iter().zip(buttons.iter()) {
        let claimed = model.claims(&HitTarget::Transport(*button));
        let label = if *button == TransportButton::PlayPause {
            play_pause_label.to_owned()
        } else if compact {
            button.compact_label().to_owned()
        } else {
            button.label().to_owned()
        };
        let paragraph = Paragraph::new(Line::from(label).centered());
        frame.render_widget(
            if claimed {
                paragraph.style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                paragraph
            }
            .block(Block::bordered()),
            *cell,
        );
        regions.push(HitRegion {
            area: *cell,
            target: HitTarget::Transport(*button),
        });
    }

    draw_preference_row(frame, model, preferences, regions);
}

/// The preference row (G1): the App-owned policies as visible controls
/// — volume steppers around the desired-factor display, then the order
/// and repeat toggles. Each converges on exactly the [`TuiAction`] its
/// accelerator key produces.
fn draw_preference_row(
    frame: &mut Frame,
    model: &TuiModel,
    area: Rect,
    regions: &mut Vec<HitRegion>,
) {
    let compact = model.compact_layout();
    // The toggle cells are FIXED, measured for the WIDEST spelling the
    // class can ever render (owner ruling 2026-10-10: a mode change
    // must never re-lay the row out — the row holds still). The volume
    // VALUE is the flexible display text inside its fixed cell.
    let order_text = model.order_label().map(|label| {
        if compact {
            format!("Ord:{}", order_short(label))
        } else {
            format!("Order: {label}")
        }
    });
    let repeat_text = model.repeat_label().map(|label| {
        if compact {
            format!("Rep:{label}")
        } else {
            format!("Repeat: {label}")
        }
    });
    let value_width = if compact { 7 } else { 18 };
    let [down, label, up, order, repeat, rest] = Layout::horizontal([
        Constraint::Length(3),
        Constraint::Length(value_width),
        Constraint::Length(3),
        Constraint::Length(order_toggle_width(compact)),
        Constraint::Length(repeat_toggle_width(compact)),
        Constraint::Min(0),
    ])
    .areas(area);
    let _ = rest;

    let steppers: [(Rect, PreferenceButton); 2] = [
        (down, PreferenceButton::VolumeDown),
        (up, PreferenceButton::VolumeUp),
    ];
    for (cell, button) in steppers {
        let claimed = model.claims(&HitTarget::Preference(button));
        // The stepper glyph is ONE plain ASCII character (G1 F02): the
        // cell is one column wide inside its borders, so any longer —
        // or any glyph the terminal's font may lack — renders as a
        // blank, and an invisible control is a live target with no
        // visible affordance.
        let paragraph = Paragraph::new(
            Line::from(if button == PreferenceButton::VolumeDown {
                "-"
            } else {
                "+"
            })
            .centered(),
        );
        frame.render_widget(
            if claimed {
                paragraph.style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                paragraph
            }
            .block(Block::bordered()),
            cell,
        );
        regions.push(HitRegion {
            area: cell,
            target: HitTarget::Preference(button),
        });
    }
    if let Some(volume) = model.volume_label() {
        let desired = if compact {
            volume
        } else {
            format!("{volume} (desired)")
        };
        frame.render_widget(Paragraph::new(desired).centered(), label);
    }

    let toggles: [(Rect, PreferenceButton, Option<String>, bool); 2] = [
        (
            order,
            PreferenceButton::Order,
            order_text,
            model.order() == Some(PlaybackOrder::Shuffle),
        ),
        (
            repeat,
            PreferenceButton::Repeat,
            repeat_text,
            !matches!(model.repeat(), None | Some(RepeatMode::Off)),
        ),
    ];
    for (cell, button, label, engaged) in toggles {
        let claimed = model.claims(&HitTarget::Preference(button));
        // Two shapes, the tabs' vocabulary: the persistent shape of an
        // ENGAGED mode is BOLD; the TRANSIENT inversion — the keyboard
        // focus or a held press (§16: the release bounces it back) —
        // is REVERSED. A monochrome terminal can tell both apart.
        let mut style = Style::default();
        if engaged {
            style = style.add_modifier(Modifier::BOLD);
        }
        if claimed {
            style = style.add_modifier(Modifier::REVERSED);
        }
        let paragraph = match label {
            Some(label) => Paragraph::new(Line::from(label).centered()),
            None => Paragraph::new(Line::from("—").centered()),
        };
        frame.render_widget(paragraph.style(style).block(Block::bordered()), cell);
        regions.push(HitRegion {
            area: cell,
            target: HitTarget::Preference(button),
        });
    }
}

/// The compact spelling of an order label (same policy, shorter word).
fn order_short(label: &str) -> &'static str {
    match label {
        "Sequential" => "Seq",
        "Shuffle" => "Shuf",
        _ => "?",
    }
}

/// The FIXED Order toggle cell width: measured for the WIDEST spelling
/// the class can ever render, so a mode change — or the preference
/// arriving at all — never re-lays the row out (owner ruling
/// 2026-10-10: the row holds still).
pub(super) fn order_toggle_width(compact: bool) -> u16 {
    [PlaybackOrder::Sequential, PlaybackOrder::Shuffle]
        .iter()
        .map(|mode| {
            let text = if compact {
                format!("Ord:{}", order_short(mode.label()))
            } else {
                format!("Order: {}", mode.label())
            };
            text.chars().count() as u16 + 2
        })
        .max()
        .expect("the order modes are known")
}

/// The FIXED Repeat toggle cell width, the same rule.
pub(super) fn repeat_toggle_width(compact: bool) -> u16 {
    [RepeatMode::Off, RepeatMode::All, RepeatMode::One]
        .iter()
        .map(|mode| {
            let text = if compact {
                format!("Rep:{}", mode.label())
            } else {
                format!("Repeat: {}", mode.label())
            };
            text.chars().count() as u16 + 2
        })
        .max()
        .expect("the repeat modes are known")
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use super::super::testutil::*;
    use super::{order_toggle_width, repeat_toggle_width};
    use crate::playlist::{PlaybackOrder, RepeatMode};
    use crate::tui::model::*;
    use qianqian_playback::PlaybackSessionObservation;

    /// The preference row's toggles are measured cells (the toolbar
    /// idiom, everywhere): the full "Order: Sequential" spelling
    /// renders WHOLE at the 60-column Compact boundary, where a fixed
    /// share used to starve it into "Order: Sequentia".
    #[test]
    fn the_preference_toggles_render_whole_at_the_compact_boundary() {
        let mut model = plain_model();
        model.set_order(PlaybackOrder::Sequential);
        model.set_repeat(RepeatMode::Off);
        let text = rendered(&mut model, 60, 18);
        assert!(text.contains("Order: Sequential"), "{text}");
        assert!(text.contains("Repeat: Off"), "{text}");
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("Order: Sequential"), "{text}");
    }

    /// The owner's stability ruling (2026-10-10): a mode change — or
    /// the preference arriving at all — never re-lays the preference
    /// row out. Every control's cell holds still; only the label text
    /// inside its fixed cell changes (突然变尺寸挺吓人的).
    #[test]
    fn a_mode_change_never_moves_the_preference_row() {
        // Start from NO preference state at all: the fixed cells exist
        // (the `—` fallback) before the state lands.
        let mut model = TuiModel::new(String::new());
        model.set_class(ResponsiveClass::Normal);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        let row_areas = |model: &TuiModel| {
            [
                PreferenceButton::VolumeDown,
                PreferenceButton::VolumeUp,
                PreferenceButton::Order,
                PreferenceButton::Repeat,
            ]
            .map(|button| {
                model
                    .regions()
                    .iter()
                    .find(|region| region.target == HitTarget::Preference(button))
                    .expect("the control's region")
                    .area
            })
        };

        // Draw the quiet row: the cells predate the state.
        terminal
            .draw(|frame| crate::tui::view::draw(frame, &mut model))
            .expect("draw");
        let unknown = row_areas(&model);

        // The preferences arrive on a quiet row — and move nothing.
        model.set_order(PlaybackOrder::Sequential);
        model.set_repeat(RepeatMode::Off);
        terminal
            .draw(|frame| crate::tui::view::draw(frame, &mut model))
            .expect("draw");
        assert_eq!(
            row_areas(&model),
            unknown,
            "the state arriving moves nothing"
        );
        let settled = row_areas(&model);

        // A mode change re-renders the labels and moves NOTHING.
        model.set_order(PlaybackOrder::Shuffle);
        model.set_repeat(RepeatMode::All);
        terminal
            .draw(|frame| crate::tui::view::draw(frame, &mut model))
            .expect("draw");
        assert_eq!(row_areas(&model), settled, "a mode change moves nothing");

        // The fixed widths themselves: the widest spelling per class,
        // plus the toolbar's two padding columns.
        assert_eq!(order_toggle_width(false), 19, "Order: Sequential + 2");
        assert_eq!(repeat_toggle_width(false), 13, "Repeat: Off + 2");
        assert_eq!(order_toggle_width(true), 10, "Ord:Shuf + 2");
        assert_eq!(repeat_toggle_width(true), 9, "Rep:One + 2");
    }

    /// An ENGAGED preference toggle (Shuffle / a repeat mode that is
    /// not Off) carries the tabs' persistent shape (BOLD) — distinct
    /// from the transient focus inversion, which follows the click and
    /// lingers on it — so the mode stays readable after the focus
    /// moves on.
    #[test]
    fn an_engaged_preference_toggle_is_bold() {
        let mut model = plain_model();
        model.set_order(PlaybackOrder::Shuffle);
        model.set_repeat(RepeatMode::All);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|frame| crate::tui::view::draw(frame, &mut model))
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        let label_cell = |button| {
            let area = model
                .regions()
                .iter()
                .find(|region| region.target == HitTarget::Preference(button))
                .expect("the toggle's region")
                .area;
            // The bordered toggle draws its label on the cell's middle
            // row.
            (area.x + area.width / 2, area.y + 1)
        };
        assert!(
            buffer
                .cell(label_cell(PreferenceButton::Order))
                .expect("label cell")
                .style()
                .add_modifier
                .contains(ratatui::style::Modifier::BOLD),
            "Shuffle is engaged"
        );
        assert!(
            buffer
                .cell(label_cell(PreferenceButton::Repeat))
                .expect("label cell")
                .style()
                .add_modifier
                .contains(ratatui::style::Modifier::BOLD),
            "Repeat All is engaged"
        );
    }

    /// The Now Playing route renders the episode read side, the
    /// diagnostics, the transport row with its Open control, and the
    /// preference row — every core player function visible (G1) — and
    /// nothing unearned.
    #[test]
    fn the_now_playing_route_renders_the_episode_and_transport() {
        let mut model = plain_model();
        let text = rendered(&mut model, 100, 30);
        for earned in [
            "Source: D:\\media\\song.flac",
            "Format: 44100 Hz, 2 channels, mask 0x3",
            "Position:",
            "Track: 1/6",
            "100/100 (desired)",
            "Order: Sequential",
            "Repeat: Off",
            "Open",
            "[ Back 5s ]",
            "[ Forward 5s ]",
            "◀ Prev",
            "Pause",
            "■ Stop",
            "Next ▶",
            "Tab=focus",
        ] {
            assert!(text.contains(earned), "{earned:?} missing in:\n{text}");
        }
        assert_eq!(scan(&text), None, "{text}");
    }

    /// The transport row publishes one region per button, and the
    /// region's rect contains the button's own label — render and hit
    /// geometry come from the same layout decision (§14).
    #[test]
    fn transport_regions_carry_their_own_labels() {
        let mut model = plain_model();
        let _text = rendered(&mut model, 100, 30);
        for button in TRANSPORT {
            let target = HitTarget::Transport(button);
            let region = model
                .regions()
                .iter()
                .find(|region| region.target == target)
                .unwrap_or_else(|| panic!("no region for {button:?}"));
            assert!(
                region.area.width >= button.label().chars().count() as u16,
                "{button:?} region too narrow: {:?}",
                region.area
            );
        }
        // Exactly five transport regions exist (Open joined the row).
        assert_eq!(
            model
                .regions()
                .iter()
                .filter(|region| matches!(region.target, HitTarget::Transport(_)))
                .count(),
            5
        );
    }

    /// The seek bar publishes a click region ONLY while duration
    /// evidence exists (G1 §9): an unknown timeline renders at most a
    /// display bar and is never clickable into a fabricated target.
    #[test]
    fn the_seek_bar_publishes_a_region_only_with_duration_evidence() {
        // With duration evidence: the bar row is a seek affordance.
        let mut model = plain_model();
        let _text = rendered(&mut model, 100, 30);
        assert!(
            model
                .regions()
                .iter()
                .any(|region| region.target == HitTarget::SeekBar),
            "a known duration makes the bar clickable"
        );

        // The same frame without duration evidence: no region at all.
        let mut model = plain_model();
        model.update(PlaybackSessionObservation {
            source_duration: None,
            ..pending()
        });
        let text = rendered(&mut model, 100, 30);
        assert!(
            !model
                .regions()
                .iter()
                .any(|region| region.target == HitTarget::SeekBar),
            "an unknown duration is never clickable:\n{text}"
        );
        assert_eq!(scan(&text), None, "{text}");
    }

    /// The desired-DSP summary renders at generous sizes and yields
    /// first under constrained layouts (G5 §39: secondary detail hides
    /// before any core control does). The line is always a DESIRED
    /// statement, never an applied claim.
    #[test]
    fn the_dsp_line_renders_desired_and_hides_in_compact() {
        let mut model = plain_model();
        model.set_desired_dsp(crate::tui::model::dsp_summary(
            &qianqian_playback::EqPreset::Rock.to_config(),
        ));
        let wide = rendered(&mut model, 100, 30);
        assert!(wide.contains("DSP (desired): on — preset rock"), "{wide}");
        assert_eq!(scan(&wide), None, "{wide}");

        // The compact class hides the summary and keeps every control.
        let compact = rendered(&mut model, 50, 16);
        assert!(!compact.contains("DSP (desired)"), "{compact}");
        for control in ["Open", "Ord:Seq", "Rep:Off", "100/100"] {
            assert!(compact.contains(control), "{control:?} missing:\n{compact}");
        }
    }

    /// G1 F14: the DESIRED DSP configuration has a product lifetime
    /// independent of any active episode, so its summary renders on the
    /// idle page too — not only inside an active-source panel. Still a
    /// desired statement, never an applied claim.
    #[test]
    fn the_desired_dsp_summary_renders_while_idle() {
        let mut model = TuiModel::new(String::new());
        model.set_episode(None);
        model.update(pending());
        assert_eq!(model.source(), None, "the idle page has no episode");
        model.set_desired_dsp(crate::tui::model::dsp_summary(
            &qianqian_playback::EqPreset::Rock.to_config(),
        ));
        let text = rendered(&mut model, 100, 30);
        assert!(
            text.contains("No music loaded."),
            "the honest idle page: {text}"
        );
        assert!(
            text.contains("DSP (desired): on — preset rock"),
            "the desired summary is visible while idle:\n{text}"
        );
        assert_eq!(scan(&text), None, "{text}");
    }

    /// G1 F02: a live control's visible affordance exists INSIDE its
    /// rendered geometry. The volume steppers are one-cell-wide inner
    /// areas; the glyph drawn there must be the plain ASCII `-` / `+`
    /// — rendered, at every supported class — and the regions must
    /// still answer at that exact cell.
    #[test]
    fn the_volume_steppers_render_visible_glyphs_in_their_own_geometry() {
        for (width, height) in [(100u16, 30u16), (50, 16)] {
            let mut model = plain_model();
            let _text = rendered(&mut model, width, height);
            for button in [PreferenceButton::VolumeDown, PreferenceButton::VolumeUp] {
                let region = model
                    .regions()
                    .iter()
                    .find(|region| region.target == HitTarget::Preference(button))
                    .unwrap_or_else(|| panic!("no {button:?} region at {width}x{height}"));
                // The bordered stepper is 3 cells: border, glyph, border.
                assert!(
                    region.area.width >= 3,
                    "{button:?} cell too narrow at {width}x{height}: {region:?}"
                );
            }
        }
        // Glyph-presence check on the actual buffer rows: the cell
        // inside each stepper's border holds exactly the glyph.
        let mut model = plain_model();
        let text = rendered(&mut model, 100, 30);
        let rows: Vec<&str> = text.lines().collect();
        for (button, glyph) in [
            (PreferenceButton::VolumeDown, "-"),
            (PreferenceButton::VolumeUp, "+"),
        ] {
            let region = model
                .regions()
                .iter()
                .find(|region| region.target == HitTarget::Preference(button))
                .expect("the stepper region");
            let row = rows
                .get(region.area.y as usize + 1)
                .unwrap_or_else(|| panic!("row {} exists", region.area.y + 1));
            let inner = row
                .chars()
                .nth(region.area.x as usize + 1)
                .map(|character| character.to_string())
                .unwrap_or_default();
            assert_eq!(
                inner,
                glyph,
                "{button:?}: the glyph inside its own geometry at x={} is {inner:?}",
                region.area.x + 1
            );
        }
    }

    /// G1 F07: the seek buttons are rendered affordances with regions
    /// while the evidence exists — and they vanish (no regions, no
    /// painted labels) without it.
    #[test]
    fn the_seek_buttons_render_with_evidence_and_vanish_without_it() {
        // Evidence (position + rate): two rendered, hit-tested buttons.
        let mut model = plain_model();
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("[ Back 5s ]"), "{text}");
        assert!(text.contains("[ Forward 5s ]"), "{text}");
        for button in crate::tui::model::SEEK_BUTTONS {
            assert!(
                model
                    .regions()
                    .iter()
                    .any(|region| region.target == HitTarget::Seek(button)),
                "no region for {button:?}"
            );
        }

        // No position evidence: nothing rendered, nothing published.
        let mut model = plain_model();
        model.update(PlaybackSessionObservation {
            position: None,
            ..pending()
        });
        let text = rendered(&mut model, 100, 30);
        assert!(!text.contains("[ Back 5s ]"), "{text}");
        assert!(
            !model
                .regions()
                .iter()
                .any(|region| matches!(region.target, HitTarget::Seek(_))),
            "no seek regions without evidence"
        );
    }
}
