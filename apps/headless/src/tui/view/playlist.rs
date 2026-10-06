//! The Playlist route (G2): the toolbar of visible list-edit controls,
//! the windowed list pane with its independent playing/selection
//! markers, and the summary row — every T0 playlist function as a
//! visible control that keyboard focus and the mouse both reach.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};

use super::{PLAYING_MARKER, SELECTED_MARKER, bold};
use crate::tui::model::{
    FocusId, HitRegion, HitTarget, PLAYLIST_BUTTONS, PlaylistButton, PlaylistRow, PreferenceButton,
    TuiModel,
};

/// The Playlist route (G2): toolbar, list, summary. Rows and regions
/// come from ONE window calculation, and the effective window is
/// reported back to the model so wheel scrolling continues from the
/// rows actually on screen.
pub(super) fn draw_playlist(
    frame: &mut Frame,
    model: &mut TuiModel,
    area: Rect,
    regions: &mut Vec<HitRegion>,
) {
    let compact = model.compact_layout();
    // The toolbar wraps whenever the FULL spellings would not fit one
    // row — the class decides the spelling, the measurement decides
    // the wrap. A clipped active hit target is a T0 violation at any
    // class (this is the compact freeze's own reason, generalized to
    // the narrow end of the Normal band).
    let full_width: u16 = PLAYLIST_BUTTONS
        .iter()
        .map(|button| button.label().chars().count() as u16 + 2)
        .sum();
    let wrap = compact || full_width > area.width;
    let [toolbar, list, summary] = Layout::vertical([
        // T0 Compact freeze: "Playlist toolbar wraps" — two rows where
        // the full spellings would not fit the supported minimum.
        Constraint::Length(if wrap { 2 } else { 1 }),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_playlist_toolbar(frame, model, toolbar, compact, wrap, regions);
    draw_playlist_list(frame, model, list, regions);
    draw_playlist_summary(frame, model, summary, compact, regions);
}

/// The toolbar (the T0 playlist wireframe's control row): Add File,
/// Add Folder, Play selected, Remove, Clear — measured buttons, so no
/// label clips at any supported width.
fn draw_playlist_toolbar(
    frame: &mut Frame,
    model: &mut TuiModel,
    area: Rect,
    compact: bool,
    wrap: bool,
    regions: &mut Vec<HitRegion>,
) {
    if !wrap {
        draw_button_row(frame, model, area, &PLAYLIST_BUTTONS, compact, regions);
        return;
    }
    // T0 Compact freeze: "Playlist toolbar wraps" — three buttons on
    // the first row, two on the second; every spelling stays inside
    // the class minimum (and the Normal band's narrow end keeps its
    // full spellings, wrapped).
    let [first, second] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
    let (head, tail) = PLAYLIST_BUTTONS.split_at(3);
    draw_button_row(frame, model, first, head, compact, regions);
    draw_button_row(frame, model, second, tail, compact, regions);
}

/// One measured row of bordered text buttons (the transport row's
/// idiom, at label-driven widths: an equal share would clip
/// `[Add Folder...]` at the class minimum — a clipped active hit
/// target is a T0 violation).
fn draw_button_row(
    frame: &mut Frame,
    model: &mut TuiModel,
    area: Rect,
    buttons: &[PlaylistButton],
    compact: bool,
    regions: &mut Vec<HitRegion>,
) {
    let widths: Vec<Constraint> = buttons
        .iter()
        .map(|button| {
            let label = if compact {
                button.compact_label()
            } else {
                button.label()
            };
            Constraint::Length(label.chars().count() as u16 + 2)
        })
        .collect();
    let cells = Layout::horizontal(widths).split(area);
    for (button, cell) in buttons.iter().zip(cells.iter()) {
        let label = if compact {
            button.compact_label()
        } else {
            button.label()
        };
        let focused = model.focus() == Some(FocusId::PlaylistButton(*button));
        // A plain text button — the T0 wireframe's `[Add File...]`
        // shape, the same affordance as the picker's buttons. The
        // measured cell leaves one padding column each side, so the
        // label never clips; focus is REVERSED (never colour alone).
        let paragraph = Paragraph::new(Line::from(label).centered());
        frame.render_widget(
            if focused {
                paragraph.style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                paragraph
            },
            *cell,
        );
        regions.push(HitRegion {
            area: *cell,
            target: HitTarget::PlaylistButton(*button),
        });
    }
}

/// The list pane: the model's viewport window over the App's traversal
/// order, the two independent markers, and one hit region per VISIBLE
/// row (T0: only visible rows get hit targets).
fn draw_playlist_list(
    frame: &mut Frame,
    model: &mut TuiModel,
    area: Rect,
    regions: &mut Vec<HitRegion>,
) {
    // Everything the immutable rows borrow feeds is computed first;
    // the window report below needs &mut.
    let (total, selection_ordinal, episode_live) = {
        let rows = model.playlist();
        (
            rows.len(),
            rows.iter().position(|row| row.selected).map(|p| p + 1),
            model.source().is_some(),
        )
    };
    let mut block = Block::bordered().title(bold(" Playlist "));
    if total > 0 {
        // The pane's title is the SELECTION ordinal (the committed
        // position is the `Track: <committed>/<len>` line on the Now
        // Playing panel, so no reader has to guess which cursor an
        // unlabelled number is).
        block = block.title(
            Line::from(format!(
                " sel {}/{} ",
                selection_ordinal.unwrap_or(0),
                total
            ))
            .right_aligned(),
        );
    }
    let inner = block.inner(area);
    let visible = inner.height as usize;
    let (top, visible_indices, lines) = {
        let rows = model.playlist();
        // The ONE window decision (§14): the model's stored top,
        // clamped so a full window fits — the same top draws the lines
        // and indexes the row regions, so a hit test cannot disagree
        // with the screen.
        let top = model
            .playlist_viewport_hint()
            .min(total.saturating_sub(visible.min(total)));
        let indices: Vec<usize> = (0..total).skip(top).take(visible).collect();
        let lines: Vec<Line<'static>> = rows
            .iter()
            .enumerate()
            .skip(top)
            .take(visible)
            .map(|(position, row)| playlist_row_line(position, row, episode_live))
            .collect();
        (top, indices, lines)
    };
    frame.render_widget(Paragraph::new(lines).block(block), area);

    // The view reports the window it actually drew; wheel steps and
    // reveals continue from these rows.
    model.note_playlist_window(top, visible);

    if total > 0 {
        for (visible_row, index) in visible_indices.into_iter().enumerate() {
            regions.push(HitRegion {
                area: Rect::new(inner.x, inner.y + visible_row as u16, inner.width, 1),
                target: HitTarget::PlaylistRow(index),
            });
        }
        regions.push(HitRegion {
            area: inner,
            target: HitTarget::PlaylistPane,
        });
    }
}

/// The summary row (the T0 wireframe's status line): the row/selection
/// counts, with the order/repeat toggles in the spacious classes. In
/// Compact the toggles drop (they remain visible on Now Playing; T0's
/// playlist retention puts the summary last).
fn draw_playlist_summary(
    frame: &mut Frame,
    model: &TuiModel,
    area: Rect,
    compact: bool,
    regions: &mut Vec<HitRegion>,
) {
    let rows = model.playlist();
    let selected = rows.iter().position(|row| row.selected).map(|p| p + 1);
    let counts = format!(
        "{} tracks / selected {}",
        rows.len(),
        selected
            .map(|s| s.to_string())
            .unwrap_or_else(|| "none".into())
    );
    if compact {
        frame.render_widget(Paragraph::new(counts), area);
        return;
    }
    let [text, order, repeat] = Layout::horizontal([
        Constraint::Min(12),
        Constraint::Length(19),
        Constraint::Length(13),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(counts), text);
    let toggles: [(Rect, PreferenceButton, Option<String>); 2] = [
        (
            order,
            PreferenceButton::Order,
            model.order_label().map(|label| format!("Order: {label}")),
        ),
        (
            repeat,
            PreferenceButton::Repeat,
            model.repeat_label().map(|label| format!("Repeat: {label}")),
        ),
    ];
    for (cell, button, label) in toggles {
        let focused = model.focus() == Some(FocusId::Preference(button));
        let paragraph = match label {
            Some(label) => Paragraph::new(Line::from(label).centered()),
            None => Paragraph::new(Line::from("—").centered()),
        };
        frame.render_widget(
            if focused {
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
}

/// One playlist row: the two markers, the traversal position and the
/// file name.
///
/// `episode_live` is what keeps the committed marker HONEST: after a
/// clean-failed Open (and on a launch that never opened anything) the
/// App's cursor still names the last committed ENTRY — that is
/// navigation state the authority sanctions — but no episode exists, so
/// the row must not carry the play marker.
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

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use super::super::testutil::*;

    use crate::tui::model::*;

    /// The Playlist route renders the pane and publishes the pane
    /// region plus one region per VISIBLE row, at the rows the pane
    /// actually drew — including through the windowed scroll.
    #[test]
    fn playlist_row_regions_match_the_visible_window() {
        let mut model = plain_model();
        model.set_route(TuiRoute::Playlist);
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains(" Playlist "), "{text}");
        assert!(text.contains("sel 1/6"), "{text}");
        assert!(text.contains("6 tracks"), "{text}");
        assert_eq!(scan(&text), None, "{text}");

        let mut row_regions: Vec<(u16, usize)> = model
            .regions()
            .iter()
            .filter_map(|region| match region.target {
                HitTarget::PlaylistRow(index) => Some((region.area.y, index)),
                _ => None,
            })
            .collect();
        row_regions.sort_by_key(|(y, _)| *y);
        assert!(!row_regions.is_empty(), "the visible rows have regions");
        for (position, (_, index)) in row_regions.iter().enumerate() {
            assert_eq!(*index, position, "row regions follow the traversal order");
        }
        let pane_region = model
            .regions()
            .iter()
            .find(|region| region.target == HitTarget::PlaylistPane)
            .expect("the pane region exists");
        assert!(
            pane_region.area.height >= row_regions.len() as u16,
            "the pane contains its rows"
        );
    }

    /// The toolbar is a visible control row (G2): every T0 playlist
    /// button renders, carries its own hit region, and the counts line
    /// names the selection.
    #[test]
    fn the_playlist_toolbar_is_visible_and_hittable() {
        let mut model = plain_model();
        model.set_route(TuiRoute::Playlist);
        let text = rendered(&mut model, 100, 30);
        for label in [
            "[Add File...]",
            "[Add Folder...]",
            "[Play selected]",
            "[Remove]",
            "[Clear...]",
        ] {
            assert!(text.contains(label), "{label} must render:\n{text}");
        }
        for button in PLAYLIST_BUTTONS {
            assert!(
                model
                    .regions()
                    .iter()
                    .any(|region| region.target == HitTarget::PlaylistButton(button)),
                "{button:?} has no hit region:\n{text}"
            );
        }
        assert_eq!(scan(&text), None, "{text}");
    }

    /// The pane marks the committed row and the selected row with two
    /// SEPARATE indicators, and one row can carry both (Issue #166 §20).
    #[test]
    fn the_playlist_pane_marks_playing_and_selected_independently() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        let row = |label: &str, playing: bool, selected: bool| PlaylistRow {
            label: label.to_owned(),
            playing,
            selected,
        };
        model.set_playlist(1, || {
            vec![
                row("01 Intro.flac", true, false),
                row("02 Nocturne.flac", false, true),
                row("03 Common.flac", false, false),
                row("04 Both.flac", true, true),
            ]
        });
        model.set_route(TuiRoute::Playlist);
        let text = rendered(&mut model, 100, 30);
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
        assert_eq!(scan(&text), None, "{text}");
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
                .map(|n| PlaylistRow {
                    label: format!("track-{n:04}.flac"),
                    playing: n == 1,
                    selected: n == 4200,
                })
                .collect()
        });
        model.set_route(TuiRoute::Playlist);
        let text = rendered(&mut model, 100, 30);
        assert!(
            text.contains("track-4200.flac"),
            "the selected row is always visible:\n{text}"
        );
        assert!(
            !text.contains("track-0001.flac"),
            "the committed row is allowed to scroll away:\n{text}"
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
        assert_eq!(scan(&text), None, "{text}");
    }

    /// The viewport window follows the selection up and down while the
    /// selection is the thing that moves (the wheel has its own ruling
    /// — see the mouse tests).
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
                super::super::viewport_offset(20, selected, 4),
                expected,
                "selection {selected:?}"
            );
        }
        // A list that fits, an empty list and a zero-height pane all sit
        // at the top.
        assert_eq!(super::super::viewport_offset(3, Some(2), 4), 0);
        assert_eq!(super::super::viewport_offset(0, None, 4), 0);
        assert_eq!(super::super::viewport_offset(20, Some(19), 0), 0);
    }

    // ------------------------------------------------------------------
    // Responsive classes (§27/§28).
    // ------------------------------------------------------------------
}
