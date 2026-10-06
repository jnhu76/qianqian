//! The Playlist route: the list pane, its window over the App's
//! traversal order, and the independent playing/selection markers.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};

use super::{PLAYING_MARKER, SELECTED_MARKER, bold, viewport_offset};
use crate::tui::model::{HitRegion, HitTarget, PlaylistRow, TuiModel};

/// The Playlist route: the same pane as before (Issue #166 §6/§20/§21),
/// proving list focus, the selection action, mouse row hits and wheel
/// scroll without turning T1B into playlist productization (§35).
/// Rows and regions come from ONE offset calculation.
pub(super) fn draw_playlist(
    frame: &mut Frame,
    model: &TuiModel,
    area: Rect,
    regions: &mut Vec<HitRegion>,
) {
    let rows = model.playlist();
    let episode_live = model.source().is_some();
    let mut block = Block::bordered().title(bold(" Playlist "));
    if !rows.is_empty() {
        // The pane's title is the SELECTION ordinal (the committed
        // position is the `Track: <committed>/<len>` line on the Now
        // Playing panel, so no reader has to guess which cursor an
        // unlabelled number is).
        let cursor = rows
            .iter()
            .position(|row| row.selected)
            .map(|position| position + 1)
            .unwrap_or(0);
        block = block.title(Line::from(format!(" sel {cursor}/{} ", rows.len())).right_aligned());
    }
    let inner = block.inner(area);
    let height = inner.height as usize;
    let selected = rows.iter().position(|row| row.selected);
    // The ONE layout decision (§14): the same offset draws the lines
    // and indexes the row regions, so a hit test cannot disagree with
    // what is on screen.
    let offset = viewport_offset(rows.len(), selected, height);
    let lines: Vec<Line<'static>> = rows
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(position, row)| playlist_row_line(position, row, episode_live))
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), area);

    // Publish the list's geometry: one region per VISIBLE row first,
    // then the pane content area (the wheel target). Order matters:
    // hit_test answers the FIRST containing region, so the specific
    // rows must sit in front of the whole-pane region.
    if !rows.is_empty() {
        for (visible, (index, _)) in rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(height)
            .enumerate()
        {
            regions.push(HitRegion {
                area: Rect::new(inner.x, inner.y + visible as u16, inner.width, 1),
                target: HitTarget::PlaylistRow(index),
            });
        }
        regions.push(HitRegion {
            area: inner,
            target: HitTarget::PlaylistPane,
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

    use super::*;
    use crate::tui::model::*;

    /// The Playlist route renders the pane and publishes the pane
    /// region plus one region per VISIBLE row, at the rows the pane
    /// actually drew — including through the stateless scroll window.
    #[test]
    fn playlist_row_regions_match_the_visible_window() {
        let mut model = plain_model();
        model.set_route(TuiRoute::Playlist);
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains(" Playlist "), "{text}");
        assert!(text.contains("sel 1/6"), "{text}");
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
        // The visible indices are contiguous and start at 0.
        for (position, (_, index)) in row_regions.iter().enumerate() {
            assert_eq!(*index, position, "row regions follow the traversal order");
        }
        // Each row region's cell holds the marker column of that row.
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

    /// Selection scrolling follows the cursor and returns to the top
    /// with it — the offset is a pure function of the selection, so
    /// nothing can drift out of sync (Issue #166 §21).
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

    // ------------------------------------------------------------------
    // Responsive classes (§27/§28).
    // ------------------------------------------------------------------
}
