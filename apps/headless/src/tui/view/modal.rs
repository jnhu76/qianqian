//! The active modal, rendered as the ONE popup over the shell: the
//! Open picker (geometry, regions, painting), the input modals and the
//! help overlay.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::{SELECTED_MARKER, bold, viewport_offset};
use crate::tui::model::{ConfirmKind, FocusId, HitRegion, HitTarget, Modal, ModalButton, TuiModel};
use qianqian_playback::EqPreset;

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
    let picker_button_list = picker_buttons(picker.mode);
    let labels: Vec<&'static str> = picker_button_list
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
/// order, per picker MODE (G2): the Open-any mode shows both commit
/// buttons; the Add File / Add Folder modes show Add only — "their
/// final button is Add, with replacement Open absent" (T0). The
/// [Use this folder] control lives on the popup's nav row, above the
/// listing ([`USE_FOLDER_LABEL`]), and exists only where a folder can
/// be the subject.
fn picker_buttons(mode: crate::tui::model::PickerMode) -> Vec<crate::tui::model::ModalButton> {
    let mut buttons = Vec::with_capacity(4);
    if mode.allows_open() {
        buttons.push(crate::tui::model::ModalButton::Open);
    }
    buttons.push(crate::tui::model::ModalButton::Add);
    buttons.push(crate::tui::model::ModalButton::EnterFolder);
    buttons.push(crate::tui::model::ModalButton::Cancel);
    buttons
}

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
        crate::tui::model::ModalButton::Confirm => "[Stop & remove]",
        crate::tui::model::ModalButton::Cancel => "[Cancel]",
    }
}

/// The button labels in the compact classes: the SAME controls,
/// spellings that fit the supported-minimum popup (the class minimums
/// and the row tests pin the fit — G1 F13). The nav-row [Use this
/// folder] label is shared — see [`USE_FOLDER_LABEL`].
fn compact_button_label(button: crate::tui::model::ModalButton) -> &'static str {
    match button {
        crate::tui::model::ModalButton::Open => "[Open]",
        crate::tui::model::ModalButton::Add => "[Add]",
        crate::tui::model::ModalButton::UseFolder => USE_FOLDER_LABEL,
        crate::tui::model::ModalButton::EnterFolder => "[Enter]",
        crate::tui::model::ModalButton::Confirm => "[Stop & remove]",
        crate::tui::model::ModalButton::Cancel => "[Cancel]",
    }
}

/// The popup title for the picker's mode (G2): the Add modes name
/// their disposition so the user knows which operation the final
/// button will commit.
fn mode_title(mode: crate::tui::model::PickerMode) -> &'static str {
    match mode {
        crate::tui::model::PickerMode::OpenAny => "Open",
        crate::tui::model::PickerMode::AddFile => "Add File",
        crate::tui::model::PickerMode::AddFolder => "Add Folder",
    }
}

/// Publish the active modal's hit regions from the SAME layout the
/// modal paints from. The Open picker's controls follow its mode (G2);
/// the confirm modal publishes its two buttons; the GoTo and Help
/// modals stay keyboard-owned and publish none.
pub(super) fn modal_regions(
    modal: &Modal,
    area: Rect,
    compact: bool,
    regions: &mut Vec<HitRegion>,
) {
    match modal {
        Modal::Open(picker) => {
            let layout = picker_layout(picker, area, compact);
            regions.push(HitRegion {
                area: layout.field,
                target: HitTarget::ModalField,
            });
            if picker.mode.allows_use_folder() {
                regions.push(HitRegion {
                    area: layout.nav,
                    target: HitTarget::ModalButton(ModalButton::UseFolder),
                });
            }
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
            for (button, cell) in picker_buttons(picker.mode).into_iter().zip(layout.buttons) {
                regions.push(HitRegion {
                    area: cell,
                    target: HitTarget::ModalButton(button),
                });
            }
        }
        Modal::Confirm { kind } => confirm_regions(*kind, area, regions),
        Modal::Presets { .. } => {
            let (_, inner, rows) = presets_layout(area);
            for row in 0..rows {
                regions.push(HitRegion {
                    area: Rect::new(inner.x, inner.y + row, inner.width, 1),
                    target: HitTarget::PickerRow(row as usize),
                });
            }
        }
        _ => {}
    }
}

/// The preset menu's geometry (G3): one centered popup listing the
/// eight factory presets, one row each, plus the frozen CONSEQUENCE
/// lines and a hint row. The SINGLE layout decision for
/// [`modal_regions`] and [`draw_modal`].
fn presets_layout(area: Rect) -> (Rect, Rect, u16) {
    let rows = EqPreset::all().len() as u16;
    // 8 presets + 2 consequence lines + the hint + the borders.
    let height = (rows + 5).min(area.height);
    let mut popup = centered_area(area, height);
    popup.width = popup.width.min(48);
    let inner = Rect::new(
        popup.x + 1,
        popup.y + 1,
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    );
    (popup, inner, rows)
}

/// The stop-aware confirmation's geometry (T0): one consequence line
/// and the two buttons — the destructive choice and the Cancel that
/// starts focused. The popup is small and centered; it cannot become
/// an invisible keyboard trap (§28).
fn confirm_layout(kind: ConfirmKind, area: Rect) -> (Rect, Rect, [Rect; 2]) {
    let width = area.width.min(56).saturating_sub(4).max(20);
    let height = 6.min(area.height);
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
    let [_, consequence, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);
    let labels = confirm_button_labels(kind);
    let total: u16 = labels
        .iter()
        .map(|label| label.chars().count() as u16)
        .sum();
    let gap = buttons.width.saturating_sub(total) / 2;
    let mut cells = [buttons; 2];
    let mut x = buttons.x + gap;
    for (cell, label) in cells.iter_mut().zip(labels.iter()) {
        *cell = Rect::new(x, buttons.y, label.chars().count() as u16, 1);
        x += cell.width;
    }
    (popup, consequence, cells)
}

/// The confirmation's two button labels. The destructive choice names
/// its stop consequence in the button itself (T0: "an explicit stop
/// consequence").
fn confirm_button_labels(kind: ConfirmKind) -> [&'static str; 2] {
    match kind {
        ConfirmKind::RemoveCurrent => ["[Stop & remove]", "[Cancel]"],
        ConfirmKind::Clear => ["[Stop & clear]", "[Cancel]"],
    }
}

fn confirm_regions(kind: ConfirmKind, area: Rect, regions: &mut Vec<HitRegion>) {
    let (_, _, cells) = confirm_layout(kind, area);
    regions.push(HitRegion {
        area: cells[0],
        target: HitTarget::ModalButton(ModalButton::Confirm),
    });
    regions.push(HitRegion {
        area: cells[1],
        target: HitTarget::ModalButton(ModalButton::Cancel),
    });
}

/// The active modal, rendered as the ONE popup over the shell (§23).
/// The Open picker paints from the same [`picker_layout`] that
/// [`modal_regions`] published regions from.
pub(super) fn draw_modal(frame: &mut Frame, modal: &Modal, area: Rect, model: &TuiModel) {
    match modal {
        Modal::Open(picker) => {
            let compact = model.compact_layout();
            let layout = picker_layout(picker, area, compact);
            frame.render_widget(Clear, layout.popup);
            // The popup's title names the listed directory — the field
            // stays the user's own typed line, so the picker says
            // where it is instead of editing under the user's hands.
            // A long path clips honestly at the border.
            let title = Span::styled(
                match (&picker.dir, picker.mode) {
                    (Some(dir), crate::tui::model::PickerMode::OpenAny) => {
                        format!(" Open — {} ", dir.display())
                    }
                    (Some(dir), mode) => {
                        format!(" {} — {} ", mode_title(mode), dir.display())
                    }
                    (None, crate::tui::model::PickerMode::OpenAny) => " Open ".to_owned(),
                    (None, mode) => format!(" {} ", mode_title(mode)),
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
            // target without entering it. Absent in the Add File mode,
            // where a folder can never be the subject (G2).
            if picker.mode.allows_use_folder() {
                let nav_focused =
                    model.focus() == Some(FocusId::PickerButton(ModalButton::UseFolder));
                let nav = Paragraph::new(USE_FOLDER_LABEL);
                frame.render_widget(
                    if nav_focused {
                        nav.style(Style::default().add_modifier(Modifier::REVERSED))
                    } else {
                        nav
                    },
                    layout.nav,
                );
            }

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

            for (button, cell) in picker_buttons(picker.mode).into_iter().zip(layout.buttons) {
                let focused = model.focus() == Some(FocusId::PickerButton(button));
                let label = if compact {
                    compact_button_label(button)
                } else {
                    button_label(button)
                };
                let paragraph = Paragraph::new(Line::from(label).centered());
                frame.render_widget(
                    if focused {
                        paragraph.style(Style::default().add_modifier(Modifier::REVERSED))
                    } else {
                        paragraph
                    },
                    cell,
                );
            }
            frame.render_widget(
                Paragraph::new("↑↓ select · Tab cycle · Esc cancel").centered(),
                layout.hint,
            );
        }
        Modal::Confirm { kind } => {
            let (popup, consequence, cells) = confirm_layout(*kind, area);
            frame.render_widget(Clear, popup);
            frame.render_widget(
                Paragraph::new("").block(Block::bordered().title(bold(kind.title()))),
                popup,
            );
            frame.render_widget(
                Paragraph::new(Line::from(kind.consequence()).centered()),
                consequence,
            );
            let labels = confirm_button_labels(*kind);
            for ((cell, label), button) in cells
                .iter()
                .zip(labels.iter())
                .zip([ModalButton::Confirm, ModalButton::Cancel])
            {
                let focused = model.focus() == Some(FocusId::PickerButton(button));
                let paragraph = Paragraph::new(Line::from(*label).centered());
                frame.render_widget(
                    if focused {
                        paragraph.style(Style::default().add_modifier(Modifier::REVERSED))
                    } else {
                        paragraph
                    },
                    *cell,
                );
            }
        }
        Modal::Presets { cursor } => {
            let (popup, inner, rows) = presets_layout(area);
            frame.render_widget(Clear, popup);
            frame.render_widget(
                Paragraph::new("").block(Block::bordered().title(bold(" EQ preset "))),
                popup,
            );
            // One row per preset; the cursor carries `>` and — while
            // the list owns the focus — the REVERSED emphasis.
            let visible = rows.min(inner.height.saturating_sub(3));
            for (row, preset) in EqPreset::all().iter().enumerate().take(visible as usize) {
                let marked = *cursor == Some(row);
                let mark = if marked { SELECTED_MARKER } else { " " };
                let mut line = Line::from(format!("{mark} {}", preset.name()));
                if marked && model.focus() == Some(FocusId::PickerList) {
                    line.style = Style::default().add_modifier(Modifier::REVERSED);
                }
                frame.render_widget(
                    Paragraph::new(line),
                    Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                );
            }
            // The frozen consequence (T0): shown BEFORE the Enter that
            // commits — a preset is the whole-configuration operation,
            // not an EQ-trim copy.
            let consequence_row = inner.y + inner.height.saturating_sub(2);
            frame.render_widget(
                Paragraph::new("Replaces the whole configuration:"),
                Rect::new(inner.x, consequence_row, inner.width, 1),
            );
            frame.render_widget(
                Paragraph::new("on · unity preamp · the preset's trims"),
                Rect::new(inner.x, consequence_row + 1, inner.width, 1),
            );
            let hint_row = inner.y + inner.height.saturating_sub(1);
            frame.render_widget(
                Paragraph::new("↑↓ select · Enter use · Esc").centered(),
                Rect::new(inner.x, hint_row, inner.width, 1),
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
        Modal::Help { scroll } => {
            let lines = help_lines();
            // Content rows + the borders + one always-visible hint
            // row. A terminal too short for all of it scrolls instead
            // of clipping (G5).
            let height = (lines.len() as u16 + 3).min(area.height);
            let popup = centered_area(area, height);
            frame.render_widget(Clear, popup);
            frame.render_widget(
                Paragraph::new("").block(Block::bordered().title(bold(" Help "))),
                popup,
            );
            let inner = Rect::new(
                popup.x + 1,
                popup.y + 1,
                popup.width.saturating_sub(2),
                popup.height.saturating_sub(2),
            );
            let [content, hint_area] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
            // The offset counts content lines from the top; the model
            // keeps the raw count, the view clamps it to what THIS
            // popup can actually show.
            let visible = content.height as usize;
            let max_offset = lines.len().saturating_sub(visible);
            let offset = (*scroll as usize).min(max_offset);
            frame.render_widget(Paragraph::new(lines).scroll((offset as u16, 0)), content);
            let hint = if max_offset == 0 {
                "Esc close"
            } else if offset >= max_offset {
                "↑↓ scroll · end · Esc close"
            } else {
                "↑↓ scroll · Esc close"
            };
            frame.render_widget(Paragraph::new(hint).centered(), hint_area);
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
/// methods, ordered by the T0 user workflows — open and play, steer
/// the playlist, shape the sound, watch — instead of a flat key dump,
/// with the always-true keys last. Every line here must stay in
/// lockstep with the frozen grammar — no affordance may be advertised
/// before it exists, and none may be missing once it does (the
/// QUICKSTART lockstep test pins this from both sides). The popup
/// scrolls (G5), so the terminal's height decides the first page, not
/// the content's importance.
fn help_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(" Qianqian — the short tour"),
        Line::from(""),
        Line::from(" 1 · Open and play"),
        Line::from("   O                 open a file or folder"),
        Line::from("   Tab / Shift+Tab   path field / listing / buttons"),
        Line::from("   ↑ / ↓ / wheel     move the listing selection"),
        Line::from("   Enter             folder row: enter it; file row: select it"),
        Line::from("   Backspace         on the listing: up to the parent folder"),
        Line::from("   [Open]            play the picked subject now"),
        Line::from("   [Add]             append it to the playlist instead"),
        Line::from("   [Use this folder] make the shown folder the subject"),
        Line::from("   Esc               cancel"),
        Line::from(""),
        Line::from(" 2 · Steer what plays"),
        Line::from("   ↑ / ↓             select the previous / next playlist row"),
        Line::from("   Enter             play the selected row"),
        Line::from("   N / P             next / previous track"),
        Line::from("   R                 order: sequential / shuffle"),
        Line::from("   L                 repeat: off / all / one"),
        Line::from("   Space             pause / resume / play"),
        Line::from("   ← / →             seek 5 s back / forward"),
        Line::from("   Shift+← / →       seek 30 s back / forward"),
        Line::from("   G                 go to an exact position"),
        Line::from("   + / -             volume up / down; the wheel steps it too"),
        Line::from("   S                 stop"),
        Line::from("   wheel             over the list: scroll its view"),
        Line::from(""),
        Line::from(" 3 · Shape the sound — the Audio tab"),
        Line::from("   Each setting commits on its own button; nothing is"),
        Line::from("   applied silently, and applied state is not reported."),
        Line::from("   [DSP: on/off]     processing bypass or on (commits)"),
        Line::from("   [Preamp −] / [+]  the preamp draft, 0.1x per press"),
        Line::from("   [Set preamp] / [Cancel edit]  commit / discard it"),
        Line::from("   [−] / [+]         one band's trim, ±18 dB"),
        Line::from("   [Apply EQ] / [Revert draft]   commit / discard them"),
        Line::from("   [EQ preset...]    a preset: on, unity preamp, its trims"),
        Line::from(""),
        Line::from(" 4 · Watch — the Visualizer tab"),
        Line::from("   [Spectrum] [Peak-RMS] [Waveform]  switch the display"),
        Line::from("   Without a live episode the panel says so honestly."),
        Line::from(""),
        Line::from(" 5 · Any time"),
        Line::from("   Tab / Shift+Tab   move keyboard focus"),
        Line::from("   Enter             activate the focused control"),
        Line::from("   Mouse             click any control: tabs, buttons, rows"),
        Line::from("   ?                 close this help"),
        Line::from("   Esc               cancel edits · close this help"),
        Line::from("   Q / Ctrl+C        quit (Ctrl+C works everywhere)"),
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
                        // The tour opens with the first workflow (G5),
                        // whatever the terminal's height.
                        assert!(
                            text.contains("open a file or folder"),
                            "{width}x{height}:\n{text}"
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    /// QUICKSTART ↔ `?` overlay consistency: every key the shipped help
    /// file documents must be advertised by the on-screen overlay, and
    /// nothing beyond the shipped set. The usage-text side of the same
    /// agreement lives in QUICKSTART.md's key table; this test renders
    /// the ACTUAL overlay — scrolling through every page, the way a
    /// short terminal reads it (G5) — and reads the ACTUAL
    /// QUICKSTART.md, so the two surfaces cannot drift apart silently.
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
            ("↑ / ↓", "↑ / ↓"),
            ("Backspace", "Backspace"),
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
        // Page through the whole overlay (one draw per page), the way
        // a terminal too short for the content reads it.
        let mut seen = String::new();
        for _ in 0..10 {
            let text = rendered(&mut model, 100, 30);
            assert_eq!(scan(&text), None, "{text}");
            seen.push_str(&text);
            if text.contains("· end ·") {
                break;
            }
            for _ in 0..HELP_PAGE_LINES {
                model.help_scroll(HelpScroll::PageDown);
            }
        }
        assert!(
            seen.contains("· end ·"),
            "the page walk never reached the overlay's end:\n{seen}"
        );
        for (quickstart_key, overlay_needle) in overlay_needles {
            assert!(
                documented.iter().any(|key| key == quickstart_key),
                "{quickstart_key:?} missing from QUICKSTART's key table"
            );
            assert!(
                seen.contains(overlay_needle),
                "the overlay does not advertise {quickstart_key:?} \
                 (expected {overlay_needle:?}):\n{seen}"
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
            "library",
            "favorites",
        ] {
            assert!(!seen.contains(unearned), "{unearned:?} in:\n{seen}");
        }
    }

    /// The help overlay pages (G5): a short terminal scrolls, says so
    /// in its hint row, and says "end" at the bottom; a terminal tall
    /// enough for the whole tour says only "Esc close".
    #[test]
    fn the_help_overlay_pages_and_tells_the_truth_about_it() {
        // A terminal the tour is taller than.
        let mut model = plain_model();
        model.open_modal(ModalKind::Help);
        let text = rendered(&mut model, 60, 20);
        assert!(text.contains(" Help "), "{text}");
        assert!(text.contains("↑↓ scroll · Esc close"), "{text}");
        assert!(
            !text.contains("move keyboard focus"),
            "the last section must start below the fold:\n{text}"
        );

        // Page to the end: the late sections arrive and the hint
        // says so.
        for _ in 0..6 {
            model.help_scroll(HelpScroll::PageDown);
        }
        let text = rendered(&mut model, 60, 20);
        assert!(text.contains("move keyboard focus"), "{text}");
        assert!(text.contains("· end ·"), "{text}");

        // A tall terminal shows the whole tour at once.
        let mut model = plain_model();
        model.open_modal(ModalKind::Help);
        let text = rendered(&mut model, 100, 60);
        assert!(text.contains("move keyboard focus"), "{text}");
        assert!(text.contains("Esc close"), "{text}");
        assert!(!text.contains("↑↓ scroll"), "nothing to scroll:\n{text}");
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
