//! The active modal, rendered as the ONE popup over the shell: the
//! Open picker (geometry, regions, painting), the input modals and the
//! help overlay.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::{SELECTED_MARKER, bold, viewport_offset};
use crate::tui::model::{
    FocusId, HitRegion, HitTarget, Modal, ModalButton, ResponsiveClass, TuiModel,
};

/// The Open picker's geometry for ONE frame: the popup, its field row,
/// the listing rows area, the (optional) honest error row, the visible
/// buttons and the hint line. The SINGLE layout decision (§14) —
/// [`modal_regions`] publishes from it and [`draw_modal`] paints from
/// it, so a hit test cannot disagree with what is on screen.
struct PickerLayout {
    popup: Rect,
    field: Rect,
    /// The navigation row between the field and the listing: the T0
    /// picker's frozen [Use this folder] control.
    nav: Rect,
    list: Rect,
    /// The bounded note row: an honest failure diagnostic when one
    /// stands, else the [Use this folder] target display. Absent when
    /// there is nothing to say.
    note: Option<Rect>,
    /// One rect per button, in [`PICKER_BUTTONS`] order, MEASURED from
    /// the labels this frame actually renders (G1 F13: at the supported
    /// minimum no control clips into its neighbor, and the hit geometry
    /// is the final rendered geometry).
    buttons: Vec<Rect>,
    hint: Rect,
}

/// Compute the picker's layout for one frame. The listing window shows
/// as many entries as the popup can hold (degrading honestly on small
/// terminals), and nothing ever exceeds the terminal area.
fn picker_layout(
    picker: &crate::tui::model::OpenPicker,
    area: Rect,
    compact: bool,
) -> PickerLayout {
    let note_rows = u16::from(picker.error.is_some() || picker.folder_target);
    let width = area.width.min(64).saturating_sub(4).max(16);
    // field + nav + buttons + hint + borders (+ an honest note row).
    let reserved = 4 + 2 + note_rows;
    let list_rows = picker
        .entries
        .len()
        .min(area.height.saturating_sub(reserved).max(1) as usize) as u16;
    let height = (list_rows + reserved).min(area.height);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let inner = Rect::new(
        popup.x + 1,
        popup.y + 1,
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    );
    let [field, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    let [nav, list, note, buttons, hint] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(note_rows),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(rest);
    // The button run is MEASURED, then centered: each cell is exactly
    // its rendered label wide, so cells cannot overlap and labels
    // cannot clip. The compact classes render the short spellings (the
    // class minimums guarantee the run fits; the row-tests pin it at
    // the exact supported minimum).
    let labels: Vec<&'static str> = PICKER_BUTTONS
        .iter()
        .map(|button| {
            if compact {
                compact_button_label(*button)
            } else {
                button_label(*button)
            }
        })
        .collect();
    let total: u16 = labels
        .iter()
        .map(|label| label.chars().count() as u16)
        .sum();
    let gap = buttons.width.saturating_sub(total) / 2;
    let mut cells = Vec::with_capacity(labels.len());
    let mut x = buttons.x + gap;
    for label in &labels {
        let width = label.chars().count() as u16;
        cells.push(Rect::new(x, buttons.y, width, 1));
        x += width;
    }
    PickerLayout {
        popup,
        field,
        nav,
        list,
        note: (note_rows > 0).then_some(note),
        buttons: cells,
        hint,
    }
}

/// The ORDER of the picker's visible buttons, in render (and Tab)
/// order: the two commit buttons, the mouse's enter-folder step, and
/// the cancel. The [Use this folder] control lives on the popup's nav
/// row, above the listing ([`USE_FOLDER_LABEL`]).
const PICKER_BUTTONS: [crate::tui::model::ModalButton; 4] = [
    crate::tui::model::ModalButton::Open,
    crate::tui::model::ModalButton::Add,
    crate::tui::model::ModalButton::EnterFolder,
    crate::tui::model::ModalButton::Cancel,
];

/// The nav-row control's label (T0 picker freeze: "Use this folder
/// selects the displayed directory as the target"). One spelling for
/// every class: it fits the supported-minimum popup with room to
/// spare (17 ≤ 34 inner cells).
const USE_FOLDER_LABEL: &str = "[Use this folder]";

fn button_label(button: crate::tui::model::ModalButton) -> &'static str {
    match button {
        crate::tui::model::ModalButton::Open => "[Open]",
        crate::tui::model::ModalButton::Add => "[Add to Playlist]",
        crate::tui::model::ModalButton::UseFolder => USE_FOLDER_LABEL,
        crate::tui::model::ModalButton::EnterFolder => "[Enter folder]",
        crate::tui::model::ModalButton::Cancel => "[Cancel]",
    }
}

/// The button labels in the compact classes: the SAME four controls,
/// spellings that fit the supported-minimum popup (the class minimums
/// and the row tests pin the fit — G1 F13). The nav-row [Use this
/// folder] label is shared — see [`USE_FOLDER_LABEL`].
fn compact_button_label(button: crate::tui::model::ModalButton) -> &'static str {
    match button {
        crate::tui::model::ModalButton::Open => "[Open]",
        crate::tui::model::ModalButton::Add => "[Add]",
        crate::tui::model::ModalButton::UseFolder => USE_FOLDER_LABEL,
        crate::tui::model::ModalButton::EnterFolder => "[Enter]",
        crate::tui::model::ModalButton::Cancel => "[Cancel]",
    }
}

/// Publish the active modal's hit regions from the SAME layout the
/// modal paints from. Only the Open picker has mouse controls; the
/// GoTo and Help modals stay keyboard-owned and publish none.
pub(super) fn modal_regions(
    modal: &Modal,
    area: Rect,
    compact: bool,
    regions: &mut Vec<HitRegion>,
) {
    let Modal::Open(picker) = modal else {
        return;
    };
    let layout = picker_layout(picker, area, compact);
    regions.push(HitRegion {
        area: layout.field,
        target: HitTarget::ModalField,
    });
    regions.push(HitRegion {
        area: layout.nav,
        target: HitTarget::ModalButton(crate::tui::model::ModalButton::UseFolder),
    });
    // One region per VISIBLE listing row, from the same offset the
    // painter uses (the playlist pane's rule: the row geometry and the
    // drawn rows come from one calculation).
    let offset = viewport_offset(
        picker.entries.len(),
        picker.cursor,
        layout.list.height as usize,
    );
    for (visible_row, index) in (offset..picker.entries.len())
        .take(layout.list.height as usize)
        .enumerate()
    {
        regions.push(HitRegion {
            area: Rect::new(
                layout.list.x,
                layout.list.y + visible_row as u16,
                layout.list.width,
                1,
            ),
            target: HitTarget::PickerRow(index),
        });
    }
    for (button, cell) in PICKER_BUTTONS.iter().zip(layout.buttons.iter()) {
        regions.push(HitRegion {
            area: *cell,
            target: HitTarget::ModalButton(*button),
        });
    }
}

/// The active modal, rendered as the ONE popup over the shell (§23).
/// The Open picker paints from the same [`picker_layout`] that
/// [`modal_regions`] published regions from.
pub(super) fn draw_modal(frame: &mut Frame, modal: &Modal, area: Rect, model: &TuiModel) {
    match modal {
        Modal::Open(picker) => {
            let compact = model.class() == ResponsiveClass::Compact;
            let layout = picker_layout(picker, area, compact);
            frame.render_widget(Clear, layout.popup);
            // The popup's title names the listed directory — the field
            // stays the user's own typed line, so the picker says
            // where it is instead of editing under the user's hands.
            // A long path clips honestly at the border.
            let title = Span::styled(
                match &picker.dir {
                    Some(dir) => format!(" Open — {} ", dir.display()),
                    None => " Open ".to_owned(),
                },
                Style::default().add_modifier(Modifier::BOLD),
            );
            frame.render_widget(
                Paragraph::new("").block(Block::bordered().title(Line::from(title))),
                layout.popup,
            );
            // The path row: the typed line (a click focuses it).
            frame.render_widget(Paragraph::new(format!(" {}▏", picker.input)), layout.field);

            // The nav row: the T0 picker's frozen [Use this folder]
            // control — the displayed directory becomes the submission
            // target without entering it.
            let nav_focused = model.focus() == Some(FocusId::PickerButton(ModalButton::UseFolder));
            let nav = Paragraph::new(USE_FOLDER_LABEL);
            frame.render_widget(
                if nav_focused {
                    nav.style(Style::default().add_modifier(Modifier::REVERSED))
                } else {
                    nav
                },
                layout.nav,
            );

            // The note row: the honest diagnostic while correction is
            // needed; otherwise the [Use this folder] target display.
            if let Some(slot) = layout.note {
                if let Some(error) = &picker.error {
                    frame.render_widget(Paragraph::new(error.clone()), slot);
                } else if picker.folder_target
                    && let Some(dir) = &picker.dir
                {
                    frame.render_widget(
                        Paragraph::new(format!("Target: {} (folder)", dir.display())),
                        slot,
                    );
                }
            }

            // The listing window: the same stateless offset rule as the
            // playlist pane, the cursor marked `>`, directories spelled
            // with a trailing `/`. The synthesized `..` row is
            // navigation chrome and renders as its own name.
            let offset = viewport_offset(
                picker.entries.len(),
                picker.cursor,
                layout.list.height as usize,
            );
            let lines: Vec<Line<'static>> = (offset..picker.entries.len())
                .take(layout.list.height as usize)
                .map(|index| {
                    let entry = &picker.entries[index];
                    let cursor_mark = if picker.cursor == Some(index) {
                        SELECTED_MARKER
                    } else {
                        " "
                    };
                    let name = if entry.is_parent {
                        entry.name.clone()
                    } else if entry.is_dir {
                        format!("{}/", entry.name)
                    } else {
                        entry.name.clone()
                    };
                    Line::from(format!("{cursor_mark} {name}"))
                })
                .collect();
            frame.render_widget(Paragraph::new(lines), layout.list);

            for (button, cell) in PICKER_BUTTONS.iter().zip(layout.buttons.iter()) {
                let focused = model.focus() == Some(FocusId::PickerButton(*button));
                let label = if compact {
                    compact_button_label(*button)
                } else {
                    button_label(*button)
                };
                let paragraph = Paragraph::new(Line::from(label).centered());
                frame.render_widget(
                    if focused {
                        paragraph.style(Style::default().add_modifier(Modifier::REVERSED))
                    } else {
                        paragraph
                    },
                    *cell,
                );
            }
            frame.render_widget(
                Paragraph::new("↑↓ select · Tab cycle · Esc cancel").centered(),
                layout.hint,
            );
        }
        Modal::GoTo { input } => {
            let popup = input_popup_area(area, 3);
            frame.render_widget(Clear, popup);
            frame.render_widget(
                Paragraph::new(format!("Go to: [{input}]  (Enter = seek, Esc = cancel)"))
                    .block(Block::bordered().title(bold(" Go to "))),
                popup,
            );
        }
        Modal::Help => {
            let lines = help_lines();
            let height = (lines.len() as u16 + 2).min(area.height);
            let popup = centered_area(area, height);
            frame.render_widget(Clear, popup);
            frame.render_widget(
                Paragraph::new(lines).block(Block::bordered().title(bold(" Help "))),
                popup,
            );
        }
    }
}

/// A centered popup area for the two input modals, degraded honestly on
/// narrow terminals.
fn input_popup_area(area: Rect, height: u16) -> Rect {
    let width = area.width.min(64).saturating_sub(4).max(12);
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        y,
        width,
        height.min(area.height),
    )
}

/// A centered popup area of `height` rows, degraded honestly on tiny
/// terminals.
fn centered_area(area: Rect, height: u16) -> Rect {
    let width = area.width.min(70).saturating_sub(4).max(12);
    let height = height.min(area.height);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}
/// The help modal's content (the `?` key): the shipped keys and input
/// methods, and nothing beyond them. Every line here must stay in
/// lockstep with the frozen grammar — no affordance may be advertised
/// before it exists, and none may be missing once it does (the
/// QUICKSTART lockstep test pins this from both sides).
fn help_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(" Qianqian keys"),
        Line::from(""),
        Line::from(" Focus"),
        Line::from("   Tab / Shift+Tab   move keyboard focus"),
        Line::from("   Enter             activate the focused control"),
        Line::from("   Mouse             click a tab, button or playlist row"),
        Line::from(""),
        Line::from(" Playlist"),
        Line::from("   ↑ / ↓ / wheel     select the previous / next row (list focused)"),
        Line::from("   Enter             play the selected row"),
        Line::from("   N / P             next / previous track"),
        Line::from("   R                 order: sequential / shuffle"),
        Line::from("   L                 repeat: off / all / one"),
        Line::from(""),
        Line::from(" Open picker"),
        Line::from("   Tab / Shift+Tab   path field / listing / buttons"),
        Line::from("   ↑ / ↓ / wheel     move the listing selection"),
        Line::from("   Enter             field: navigate a folder or select a file"),
        Line::from("   Enter             folder row: enter it; file row: select it"),
        Line::from("   Backspace         on the listing: up to the parent folder"),
        Line::from("   [Open] / [Add]    commit the selection or the typed path"),
        Line::from("   [Enter folder]    descend into the selected folder"),
        Line::from("   Esc               cancel"),
        Line::from(""),
        Line::from(" Playback"),
        Line::from("   Space             pause / resume / play"),
        Line::from("   ← / →             seek 5 s back / forward"),
        Line::from("   [Back 5s]         the same 5 s seek, as a visible button"),
        Line::from("   [Forward 5s]      the same 5 s seek, as a visible button"),
        Line::from("   Shift+← / →       seek 30 s back / forward"),
        Line::from("   G                 go to an exact position"),
        Line::from("   + / -             volume up / down"),
        Line::from("   S                 stop"),
        Line::from(""),
        Line::from(" Application"),
        Line::from("   O                 open a file or folder"),
        Line::from("   ?                 close this help"),
        Line::from("   Q / Ctrl+C        quit"),
        Line::from("   Esc               cancel / close help"),
    ]
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use super::super::testutil::*;

    use super::*;
    use crate::tui::model::*;

    /// The Open/GoTo/Help modals render as popups over the shell; the
    /// input lines are visible even on a short terminal (the field
    /// round-3 lesson, now for the popup shape).
    #[test]
    fn the_modals_render_as_visible_popups() {
        for kind in [ModalKind::Open, ModalKind::GoTo, ModalKind::Help] {
            let mut model = plain_model();
            model.open_modal(kind);
            if matches!(model.modal(), Some(Modal::Open { .. })) {
                for c in "/media/b.flac".chars() {
                    model.modal_push(c);
                }
            }
            if matches!(model.modal(), Some(Modal::GoTo { .. })) {
                for c in "1:35".chars() {
                    model.modal_push(c);
                }
            }
            for (width, height) in [(100u16, 30u16), (70, 18)] {
                let text = rendered(&mut model, width, height);
                match kind {
                    ModalKind::Open => {
                        assert!(text.contains("/media/b.flac▏"), "{width}x{height}:\n{text}");
                        assert!(text.contains("↑↓ select"), "{text}");
                        assert!(text.contains("[Add to Playlist]"), "{text}");
                    }
                    ModalKind::GoTo => {
                        assert!(text.contains("Go to: [1:35]"), "{width}x{height}:\n{text}");
                        assert!(text.contains("Enter = seek"), "{text}");
                    }
                    ModalKind::Help => {
                        assert!(text.contains(" Help "), "{width}x{height}:\n{text}");
                        assert!(
                            text.contains("move keyboard focus"),
                            "{width}x{height}:\n{text}"
                        );
                    }
                }
            }
        }
    }

    /// QUICKSTART ↔ `?` overlay consistency: every key the shipped help
    /// file documents must be advertised by the on-screen overlay, and
    /// nothing beyond the shipped set. The usage-text side of the same
    /// agreement lives in QUICKSTART.md's key table; this test renders
    /// the ACTUAL overlay and reads the ACTUAL QUICKSTART.md, so the
    /// two surfaces cannot drift apart silently.
    #[test]
    fn the_help_overlay_advertises_every_quickstart_key() {
        let quickstart_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../QUICKSTART.md");
        let quickstart = std::fs::read_to_string(&quickstart_path)
            .unwrap_or_else(|e| panic!("QUICKSTART.md readable: {e}"));
        // The overlay's spelling of each QUICKSTART key row.
        let overlay_needles = [
            ("Tab / Shift+Tab", "Tab / Shift+Tab"),
            ("Enter", "Enter"),
            ("Mouse", "Mouse"),
            ("↑ / ↓", "↑ / ↓ / wheel"),
            ("N / P", "N / P"),
            ("R", "R                 order"),
            ("L", "L                 repeat"),
            ("Space", "Space"),
            ("← / →", "← / →"),
            ("Shift+← / Shift+→", "Shift+← / →"),
            ("G", "G                 go to"),
            ("+ / -", "+ / -"),
            ("S", "S                 stop"),
            ("O", "O                 open"),
            ("?", "?                 close"),
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

        let mut model = plain_model();
        model.open_modal(ModalKind::Help);
        let text = rendered(&mut model, 100, 40);
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
        // Not shipped: none of it may be advertised.
        for unearned in [
            "M3U",
            "drag",
            "Scrub",
            "scrub",
            "lyrics",
            "album art",
            "equalizer",
            "library",
            "favorites",
        ] {
            assert!(!text.contains(unearned), "{unearned:?} in:\n{text}");
        }
        assert_eq!(scan(&text), None, "{text}");
    }

    // ------------------------------------------------------------------
    // Truth classes (unchanged through the shell rewrite).
    // ------------------------------------------------------------------

    /// G1 F13: at the SUPPORTED MINIMUM terminal (40x14, the compact
    /// floor) the picker's four buttons all render their compact
    /// spellings whole — no clipped label, no overlapping cells — and
    /// the hit geometry is the final rendered geometry. One cell
    /// larger, normal size, below the minimum, and restore are all
    /// pinned.
    #[test]
    fn the_picker_buttons_fit_at_the_supported_minimum_and_degrade_truthfully() {
        let build = || {
            let mut model = plain_model();
            model.open_modal(ModalKind::Open);
            model.set_open_listing(
                std::path::PathBuf::from("/media"),
                Ok(vec![crate::input::DirectoryEntry {
                    name: "b.flac".to_owned(),
                    is_dir: false,
                    path: std::path::PathBuf::from("/media/b.flac"),
                }]),
            );
            model
        };

        // Exactly the supported minimum: every compact label whole.
        let mut model = build();
        let text = rendered(
            &mut model,
            crate::tui::model::MIN_WIDTH,
            crate::tui::model::MIN_HEIGHT,
        );
        for label in [
            "[Open]",
            "[Add]",
            "[Enter]",
            "[Use this folder]",
            "[Cancel]",
        ] {
            assert!(
                text.contains(label),
                "{label:?} is clipped at the supported minimum:\n{text}"
            );
        }
        // The nav-row control is its own region; the four button-run
        // regions are disjoint, in order, and each one contains its own
        // label's cells.
        let nav_region = model
            .regions()
            .iter()
            .find(|region| {
                region.target == HitTarget::ModalButton(crate::tui::model::ModalButton::UseFolder)
            })
            .expect("the [Use this folder] nav region exists");
        assert!(
            nav_region.area.width as usize >= USE_FOLDER_LABEL.len(),
            "the nav row holds the whole label"
        );
        let mut button_rects: Vec<(u16, crate::tui::model::ModalButton)> = model
            .regions()
            .iter()
            .filter_map(|region| match region.target {
                HitTarget::ModalButton(button)
                    if button != crate::tui::model::ModalButton::UseFolder =>
                {
                    Some((region.area.x, button))
                }
                _ => None,
            })
            .collect();
        button_rects.sort_by_key(|(x, _)| *x);
        assert_eq!(button_rects.len(), 4, "one region per visible button");
        for pair in button_rects.windows(2) {
            let (_, left) = (&pair[0].0, &pair[0].1);
            let (right_x, _) = pair[1];
            let left_region = model
                .regions()
                .iter()
                .find(|region| region.target == HitTarget::ModalButton(*left))
                .expect("left region");
            assert!(
                left_region.area.x + left_region.area.width <= right_x,
                "button cells overlap at the minimum"
            );
        }

        // One larger: still whole.
        let mut model = build();
        let text = rendered(
            &mut model,
            crate::tui::model::MIN_WIDTH + 1,
            crate::tui::model::MIN_HEIGHT,
        );
        assert!(text.contains("[Enter]"), "{text}");

        // Normal size: the full spellings.
        let mut model = build();
        let text = rendered(&mut model, 100, 30);
        for label in ["[Open]", "[Add to Playlist]", "[Enter folder]", "[Cancel]"] {
            assert!(text.contains(label), "{label:?} missing:\n{text}");
        }

        // Below the minimum: no interactive layout at all, then restore.
        let mut model = build();
        let _text = rendered(
            &mut model,
            crate::tui::model::MIN_WIDTH - 1,
            crate::tui::model::MIN_HEIGHT,
        );
        assert!(model.regions().is_empty(), "below minimum: no regions");
        let text = rendered(
            &mut model,
            crate::tui::model::MIN_WIDTH,
            crate::tui::model::MIN_HEIGHT,
        );
        assert!(
            text.contains("[Cancel]"),
            "restored at the minimum:\n{text}"
        );
        assert!(!model.regions().is_empty(), "restored: regions republished");
    }

    /// The Open picker publishes regions for its field row, its
    /// VISIBLE listing rows and its visible buttons, from the same
    /// layout that painted them (§14) — and the row regions sit at the
    /// rows the popup actually drew.
    #[test]
    fn the_open_picker_publishes_regions_for_its_rows_and_buttons() {
        let mut model = plain_model();
        model.open_modal(ModalKind::Open);
        model.set_open_listing(
            std::path::PathBuf::from("/media"),
            Ok((0..30)
                .map(|n| crate::input::DirectoryEntry {
                    name: format!("track-{n:02}.flac"),
                    is_dir: false,
                    path: std::path::PathBuf::from(format!("/media/track-{n:02}.flac")),
                })
                .collect()),
        );
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains(" Open "), "{text}");
        assert!(text.contains("[Add to Playlist]"), "{text}");
        assert!(text.contains("[Enter folder]"), "{text}");

        let rows: Vec<(u16, usize)> = model
            .regions()
            .iter()
            .filter_map(|region| match region.target {
                HitTarget::PickerRow(index) => Some((region.area.y, index)),
                _ => None,
            })
            .collect();
        assert!(!rows.is_empty(), "the visible rows have regions");
        // The visible indices are contiguous from the top row.
        for (position, (_, index)) in rows.iter().enumerate() {
            assert_eq!(*index, position, "row regions follow the listing order");
        }
        for button in [
            crate::tui::model::ModalButton::Open,
            crate::tui::model::ModalButton::Add,
            crate::tui::model::ModalButton::EnterFolder,
            crate::tui::model::ModalButton::Cancel,
        ] {
            assert!(
                model
                    .regions()
                    .iter()
                    .any(|region| region.target == HitTarget::ModalButton(button)),
                "no region for {button:?}"
            );
        }
        assert_eq!(scan(&text), None, "{text}");
    }
}
