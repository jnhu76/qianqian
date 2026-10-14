//! The active modal, rendered as the ONE popup over the shell: the
//! Open picker (geometry, regions, painting), the input modals and the
//! help overlay.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::theme;
use super::{SELECTED_MARKER, viewport_offset};
use crate::tui::model::{ConfirmKind, FocusId, HitRegion, HitTarget, Modal, ModalButton, TuiModel};
use qianqian_playback::EqPreset;

/// A string's terminal CELL width — the measure the terminal renders
/// by (a CJK glyph is two cells), unlike a char count (#235: a modal
/// must lay its text out by the width it actually occupies).
fn cell_width(text: &str) -> u16 {
    use unicode_width::UnicodeWidthStr;
    text.width() as u16
}

/// Greedy word-wrap of one body line to `width` cells. A word longer
/// than the width fills the current line's remaining cells and is
/// hard-broken at the width; a continuation line keeps the original
/// line's leading indentation, so a wrapped help row stays visually
/// inside its own section. Width 0 degrades to the unwrapped text (the
/// layout has already degraded truthfully); the degenerate widths —
/// `indent_width >= width`, or a single glyph wider than the whole
/// width — are below every caller's reachable floor (indents are ≤ 3
/// cells, inner widths ≥ 10) and make no packing promise.
fn wrap_text(text: &str, width: u16) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;
    if width == 0 || cell_width(text) <= width {
        return vec![text.to_owned()];
    }
    let indent: String = text.chars().take_while(|&c| c == ' ').collect();
    let indent_width = cell_width(&indent);
    let mut lines = Vec::new();
    let mut current = indent.clone();
    let mut current_width = indent_width;
    for word in text[indent.len()..].split(' ') {
        if word.is_empty() {
            // A double space is content in these lines; keep it.
            if current_width < width {
                current.push(' ');
                current_width += 1;
            }
            continue;
        }
        let word_width = cell_width(word);
        let separator = u16::from(current_width > indent_width);
        if current_width + separator + word_width <= width {
            if separator == 1 {
                current.push(' ');
                current_width += 1;
            }
            current.push_str(word);
            current_width += word_width;
        } else if word_width > width.saturating_sub(indent_width) {
            // The word cannot fit any line: fill the current line's
            // remaining cells first (no row is wasted on the flush),
            // then hard-break the rest at the width.
            let mut chars = word.chars().peekable();
            if current_width > indent_width {
                if current_width < width {
                    current.push(' ');
                    current_width += 1;
                }
                while let Some(&c) = chars.peek() {
                    let c_width = UnicodeWidthChar::width(c).unwrap_or(0) as u16;
                    if current_width + c_width > width {
                        break;
                    }
                    current.push(c);
                    current_width += c_width;
                    chars.next();
                }
                lines.push(std::mem::replace(&mut current, indent.clone()));
            }
            let mut piece = indent.clone();
            let mut piece_width = indent_width;
            for c in chars {
                let c_width = UnicodeWidthChar::width(c).unwrap_or(0) as u16;
                if piece_width + c_width > width {
                    lines.push(std::mem::replace(&mut piece, indent.clone()));
                    piece_width = indent_width;
                }
                piece.push(c);
                piece_width += c_width;
            }
            current = piece;
            current_width = piece_width;
        } else {
            lines.push(std::mem::replace(&mut current, format!("{indent}{word}")));
            current_width = indent_width + word_width;
        }
    }
    lines.push(current);
    lines
}

/// The END of `text`, cut from the LEFT so it fits `width` cells — an
/// input line shows its tail, like every terminal input field.
fn tail_fit(text: &str, width: u16) -> String {
    use unicode_width::UnicodeWidthChar;
    if cell_width(text) <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut width_used = 0u16;
    for c in text.chars().rev() {
        let c_width = UnicodeWidthChar::width(c).unwrap_or(0) as u16;
        if width_used + c_width > width {
            break;
        }
        out.insert(0, c);
        width_used += c_width;
    }
    out
}

/// The confirmation's consequence, wrapped to the popup's inner width
/// (#235: the Clear spelling is 71 cells — longer than any class's
/// unwrapped inner width; it wraps instead of clipping at the border).
fn wrapped_consequence(kind: ConfirmKind, inner_width: u16) -> Vec<Line<'static>> {
    wrap_text(kind.consequence(), inner_width)
        .into_iter()
        .map(Line::from)
        .collect()
}

/// The preset menu's frozen consequence lines, wrapped the same way.
fn wrapped_presets_consequence(inner_width: u16) -> Vec<Line<'static>> {
    [
        "Replaces the whole configuration:",
        "on · unity preamp · the preset's trims",
    ]
    .into_iter()
    .flat_map(|line| wrap_text(line, inner_width))
    .map(Line::from)
    .collect()
}

/// The note row's wrapped spelling: an honest diagnostic (or the
/// [Use this folder] target display) wraps inside at most two rows,
/// then clips truthfully — a bounded degrade, never a border overrun.
fn wrapped_note(text: &str, inner_width: u16) -> Vec<Line<'static>> {
    wrap_text(text, inner_width)
        .into_iter()
        .take(2)
        .map(Line::from)
        .collect()
}

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
    let width = area.width.min(64).saturating_sub(4).max(16);
    let inner_width = width.saturating_sub(2);
    // The bounded note rows (#235): an error diagnostic or the
    // [Use this folder] target wraps inside at most two rows.
    let note_text = if let Some(error) = &picker.error {
        Some(error.clone())
    } else if picker.folder_target
        && let Some(dir) = &picker.dir
    {
        Some(format!("Target: {} (folder)", dir.display()))
    } else {
        None
    };
    let note_rows = note_text
        .as_deref()
        .map(|text| (wrap_text(text, inner_width).len().min(2)) as u16)
        .unwrap_or(0);
    // field + nav + buttons + hint + borders (+ the bounded note rows).
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
    let total: u16 = labels.iter().map(|label| cell_width(label)).sum();
    let gap = buttons.width.saturating_sub(total) / 2;
    let mut cells = Vec::with_capacity(labels.len());
    let mut x = buttons.x + gap;
    for label in &labels {
        let width = cell_width(label);
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
    // 8 presets + the wrapped consequence lines + the hint + borders
    // (#235: the second frozen line wraps at the supported minimum).
    let popup_width = area.width.min(70).saturating_sub(4).clamp(12, 48);
    let consequence_rows = wrapped_presets_consequence(popup_width.saturating_sub(2)).len() as u16;
    let height = (rows + 3 + consequence_rows).min(area.height);
    let mut popup = centered_area(area, height);
    popup.width = popup_width;
    let inner = Rect::new(
        popup.x + 1,
        popup.y + 1,
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    );
    (popup, inner, rows)
}

/// The stop-aware confirmation's geometry (T0): blank line, the
/// WRAPPED consequence block, and the two buttons — the destructive
/// choice and the Cancel that starts focused. The popup grows with
/// the wrapped rows and cannot become an invisible keyboard trap (§28).
fn confirm_layout(kind: ConfirmKind, area: Rect) -> (Rect, Rect, [Rect; 2]) {
    let width = area.width.min(56).saturating_sub(4).max(20);
    let consequence_rows = wrapped_consequence(kind, width.saturating_sub(2)).len() as u16;
    let height = (consequence_rows + 4).min(area.height);
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
        Constraint::Length(consequence_rows),
        Constraint::Length(1),
    ])
    .areas(inner);
    let labels = confirm_button_labels(kind);
    let total: u16 = labels.iter().map(|label| cell_width(label)).sum();
    let gap = buttons.width.saturating_sub(total) / 2;
    let mut cells = [buttons; 2];
    let mut x = buttons.x + gap;
    for (cell, label) in cells.iter_mut().zip(labels.iter()) {
        *cell = Rect::new(x, buttons.y, cell_width(label), 1);
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
            // The path row: the typed line (a click focuses it). While
            // the field owns the focus it reads at a glance — BOLD plus
            // the accent (#235), the same composition rule as every
            // theme style; the ▏ keeps marking the line's end.
            let field_claimed = model.claims(&HitTarget::ModalField);
            let field = Paragraph::new(format!(" {}▏", picker.input));
            frame.render_widget(
                if field_claimed {
                    field.style(theme::focus_field())
                } else {
                    field
                },
                layout.field,
            );

            // The nav row: the T0 picker's frozen [Use this folder]
            // control — the displayed directory becomes the submission
            // target without entering it. Absent in the Add File mode,
            // where a folder can never be the subject (G2).
            if picker.mode.allows_use_folder() {
                let nav_claimed = model.claims(&HitTarget::ModalButton(ModalButton::UseFolder));
                let nav = Paragraph::new(USE_FOLDER_LABEL);
                frame.render_widget(
                    if nav_claimed {
                        nav.style(theme::focus())
                    } else {
                        nav
                    },
                    layout.nav,
                );
            }

            // The note rows: the honest diagnostic while correction is
            // needed; otherwise the [Use this folder] target display.
            // Wrapped inside the measured inner width, at most two
            // rows, then clipping truthfully (#235).
            if let Some(slot) = layout.note {
                if let Some(error) = &picker.error {
                    let wrapped = wrapped_note(error, slot.width);
                    frame.render_widget(Paragraph::new(wrapped), slot);
                } else if picker.folder_target
                    && let Some(dir) = &picker.dir
                {
                    let wrapped =
                        wrapped_note(&format!("Target: {} (folder)", dir.display()), slot.width);
                    frame.render_widget(Paragraph::new(wrapped), slot);
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
                    let mut line = Line::from(format!("{cursor_mark} {name}"));
                    // The focus composition (#235, review F4): while the
                    // listing owns the keyboard focus its CURSOR row
                    // carries the REVERSED shape with the accent riding
                    // on top — the preset menu's own rule; a held press
                    // renders the same composition transiently (§16).
                    if (model.focus() == Some(FocusId::PickerList) && picker.cursor == Some(index))
                        || model.pressed(&HitTarget::PickerRow(index))
                    {
                        line.style = theme::focus();
                    }
                    line
                })
                .collect();
            frame.render_widget(Paragraph::new(lines), layout.list);

            for (button, cell) in picker_buttons(picker.mode).into_iter().zip(layout.buttons) {
                let claimed = model.claims(&HitTarget::ModalButton(button));
                let label = if compact {
                    compact_button_label(button)
                } else {
                    button_label(button)
                };
                let paragraph = Paragraph::new(Line::from(label).centered());
                frame.render_widget(
                    if claimed {
                        paragraph.style(theme::focus())
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
                Paragraph::new("").block(
                    Block::bordered()
                        .title(theme::title(kind.title()))
                        .border_style(theme::border()),
                ),
                popup,
            );
            // The consequence wraps inside the measured inner width —
            // its every word stays visible in every class (#235).
            let wrapped = wrapped_consequence(*kind, popup.width.saturating_sub(2));
            frame.render_widget(Paragraph::new(wrapped).centered(), consequence);
            let labels = confirm_button_labels(*kind);
            for ((cell, label), button) in cells
                .iter()
                .zip(labels.iter())
                .zip([ModalButton::Confirm, ModalButton::Cancel])
            {
                let claimed = model.claims(&HitTarget::ModalButton(button));
                let paragraph = Paragraph::new(Line::from(*label).centered());
                frame.render_widget(
                    if claimed {
                        paragraph.style(theme::focus())
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
                Paragraph::new("").block(
                    Block::bordered()
                        .title(theme::title(" EQ preset "))
                        .border_style(theme::border()),
                ),
                popup,
            );
            // One row per preset; the cursor carries `>` and — while
            // the list owns the focus, or a press holds the row — the
            // REVERSED emphasis with the accent riding on top (#235).
            let visible = rows.min(inner.height.saturating_sub(3));
            for (row, preset) in EqPreset::all().iter().enumerate().take(visible as usize) {
                let marked = *cursor == Some(row);
                let mark = if marked { SELECTED_MARKER } else { " " };
                let mut line = Line::from(format!("{mark} {}", preset.name()));
                if marked
                    && (model.focus() == Some(FocusId::PickerList)
                        || model.pressed(&HitTarget::PickerRow(row)))
                {
                    line.style = theme::focus();
                }
                frame.render_widget(
                    Paragraph::new(line),
                    Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                );
            }
            // The frozen consequence (T0): shown BEFORE the Enter that
            // commits — a preset is the whole-configuration operation,
            // not an EQ-trim copy. Wrapped to the inner width (#235).
            let consequence_lines = wrapped_presets_consequence(inner.width);
            let consequence_rows = consequence_lines.len() as u16;
            let consequence_top = inner.y + inner.height.saturating_sub(1 + consequence_rows);
            for (offset, line) in consequence_lines.into_iter().enumerate() {
                frame.render_widget(
                    Paragraph::new(line),
                    Rect::new(inner.x, consequence_top + offset as u16, inner.width, 1),
                );
            }
            let hint_row = inner.y + inner.height.saturating_sub(1);
            frame.render_widget(
                Paragraph::new("↑↓ select · Enter use · Esc").centered(),
                Rect::new(inner.x, hint_row, inner.width, 1),
            );
        }
        Modal::GoTo { input } => {
            let popup = input_popup_area(area, 3);
            frame.render_widget(Clear, popup);
            // The typed line yields from its LEFT end when it would
            // overflow; the static grammar degrades in bounded steps so
            // the typed input always keeps cells of its own and nothing
            // clips at the border (review F1: the full suffix alone is
            // 39 cells — wider than the inner width at the supported
            // minimum). The tier is chosen WITH a minimum input window
            // (review round 2, N1): without the floor the longest
            // fitting tier could leave a single cell at exactly 46
            // columns, so the budget dipped non-monotonically and a
            // two-cell glyph could vanish again there.
            let inner_width = popup.width.saturating_sub(2);
            // The longest legal token ("01:35.5") is 7 cells; a floor of
            // 8 keeps the input window usable at every width — the tier
            // switch can still shrink it (inner 46→47: 24→8 cells), but
            // never below the token.
            const MIN_INPUT_CELLS: u16 = 8;
            let (prefix, suffix) = [
                ("Go to: [", "]  (Enter = seek, Esc = cancel)"),
                ("Go to: [", "]  (Enter/Esc)"),
                ("Go to: [", "]"),
            ]
            .into_iter()
            .find(|(prefix, suffix)| {
                cell_width(prefix) + cell_width(suffix) + MIN_INPUT_CELLS <= inner_width
            })
            .expect("the popup floor leaves room for the shortest prompt");
            let shown = tail_fit(
                input,
                inner_width - (cell_width(prefix) + cell_width(suffix)),
            );
            frame.render_widget(
                Paragraph::new(format!("{prefix}{shown}{suffix}")).block(
                    Block::bordered()
                        .title(theme::title(" Go to "))
                        .border_style(theme::border()),
                ),
                popup,
            );
        }
        Modal::Help { scroll } => {
            // The tour's lines, wrapped to THIS popup's inner width
            // (#235): no line clips at the border in any class; a
            // continuation keeps the row's indentation. Content rows +
            // the borders + one always-visible hint row; a terminal
            // too short for all of it scrolls instead of clipping.
            let popup_width = area.width.min(70).saturating_sub(4).max(12);
            let lines: Vec<Line<'static>> = help_lines()
                .into_iter()
                .flat_map(|line| {
                    let text: String = line
                        .spans
                        .iter()
                        .map(|span| span.content.as_ref())
                        .collect();
                    wrap_text(&text, popup_width.saturating_sub(2))
                        .into_iter()
                        .map(Line::from)
                        .collect::<Vec<_>>()
                })
                .collect();
            let height = (lines.len() as u16 + 3).min(area.height);
            let popup = centered_area(area, height);
            frame.render_widget(Clear, popup);
            frame.render_widget(
                Paragraph::new("").block(
                    Block::bordered()
                        .title(theme::title(" Help "))
                        .border_style(theme::border()),
                ),
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
            // The offset counts display lines from the top; the model
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
        Line::from("   [Open]            the transport button — same as O"),
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
            ("[Open]", "[Open]            the transport"),
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

    // ------------------------------------------------------------------
    // Issue #235: modal body text stays whole inside the measured
    // inner geometry, and the focused modal control is unmistakable.
    // ------------------------------------------------------------------

    /// C1: the confirmation's consequence wraps inside the popup
    /// instead of clipping at the border. The Clear spelling is 72
    /// cells — longer than ANY class's unwrapped inner width. The
    /// oracle is word-level: clipping kills the tail words; wrapping
    /// keeps every one of them on some row inside the popup.
    #[test]
    fn the_confirm_consequence_wraps_whole_inside_the_popup() {
        for (width, height) in [
            (100u16, 30u16),
            (crate::tui::model::MIN_WIDTH, crate::tui::model::MIN_HEIGHT),
        ] {
            let mut model = plain_model();
            model.open_modal(ModalKind::ConfirmClear);
            let text = rendered(&mut model, width, height);
            for word in ConfirmKind::Clear.consequence().split_whitespace() {
                assert!(
                    text.contains(word),
                    "the consequence word {word:?} must stay visible at {width}x{height}:\n{text}"
                );
            }
            assert!(
                text.contains("[Stop & clear]"),
                "the buttons stay whole:\n{text}"
            );
            assert!(text.contains("[Cancel]"), "{text}");
            assert_eq!(scan(&text), None, "{text}");
        }
    }

    /// C1: the presets modal's frozen consequence lines wrap whole at
    /// the supported minimum (inner 34 cells vs the 39-cell line).
    #[test]
    fn the_presets_consequence_wraps_whole_at_the_supported_minimum() {
        let mut model = plain_model();
        model.open_modal(ModalKind::Presets);
        let text = rendered(
            &mut model,
            crate::tui::model::MIN_WIDTH,
            crate::tui::model::MIN_HEIGHT,
        );
        for word in "on · unity preamp · the preset's trims".split_whitespace() {
            assert!(
                text.contains(word),
                "the consequence word {word:?} must stay visible:\n{text}"
            );
        }
        assert!(text.contains("Replaces the whole configuration:"), "{text}");
        assert_eq!(scan(&text), None, "{text}");
    }

    /// C1: the GoTo prompt's static grammar suffix never falls off the
    /// right edge — an over-long typed input shows its TAIL instead.
    #[test]
    fn the_goto_prompt_keeps_its_suffix_under_a_long_input() {
        let mut model = plain_model();
        model.open_modal(ModalKind::GoTo);
        for c in "1:23:45-very-long-input-0123456789abcdef".chars() {
            model.modal_push(c);
        }
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("Go to: ["), "{text}");
        assert!(
            text.contains("Enter = seek, Esc = cancel"),
            "the suffix is the grammar; the input's tail must yield instead:\n{text}"
        );
    }

    /// Review F1 (regression guard): at the supported minimum the GoTo
    /// prompt must still SHOW the typed input. Before the fix the
    /// 39-cell static suffix consumed the whole 34-cell inner width:
    /// the input was invisible AND the suffix still clipped — the
    /// grammar now degrades in bounded steps so the input always has
    /// cells of its own.
    #[test]
    fn the_goto_prompt_keeps_its_typed_input_visible_at_the_supported_minimum() {
        let mut model = plain_model();
        model.open_modal(ModalKind::GoTo);
        for c in "1234567890".chars() {
            model.modal_push(c);
        }
        let text = rendered(
            &mut model,
            crate::tui::model::MIN_WIDTH,
            crate::tui::model::MIN_HEIGHT,
        );
        assert!(text.contains("Go to: ["), "{text}");
        assert!(
            text.contains("1234567890"),
            "the typed input must stay visible at the supported minimum:\n{text}"
        );
        assert_eq!(scan(&text), None, "{text}");
    }

    /// Review round 2 (N1): the tier ladder keeps a USABLE input window
    /// at every operable width. Without the minimum-input floor the
    /// longest fitting tier left a single cell at exactly 46 columns,
    /// so the budget dipped non-monotonically and an input whose last
    /// glyph is two cells wide vanished there again. The longest legal
    /// token must stay whole at every width in the Minimum band upward.
    #[test]
    fn the_goto_prompt_keeps_a_usable_input_window_at_every_class_width() {
        let mut model = plain_model();
        model.open_modal(ModalKind::GoTo);
        for c in "01:35.5".chars() {
            model.modal_push(c);
        }
        for width in crate::tui::model::MIN_WIDTH..=130 {
            let text = rendered(&mut model, width, 24);
            assert!(
                text.contains("Go to: [01:35.5]"),
                "the typed token must stay whole at width {width}:\n{text}"
            );
        }
    }

    /// Review F2: an over-long word FILLS the current line's remaining
    /// cells before it hard-breaks — no row is wasted on the flush.
    /// Before the fix, `Target:` was pushed alone onto its own row and
    /// the wrapped tail lost the `(folder)` marker to the bounded cap.
    #[test]
    fn an_overlong_word_fills_the_current_line_before_hard_breaking() {
        let lines = wrap_text(
            "Target: C:\\Users\\fred1\\qianqian-tui-acceptance\\media (folder)",
            34,
        );
        assert!(lines.len() <= 2, "packed into the bounded rows: {lines:?}");
        assert!(
            lines[0].starts_with("Target: C"),
            "the path starts on the label's own row: {lines:?}"
        );
        let joined = lines.join("");
        for part in ["Target:", "qianqian-tui-acceptance", "(folder)"] {
            assert!(joined.contains(part), "{part:?} lost in {lines:?}");
        }
    }

    /// C1: the help overlay wraps its widest lines at the supported
    /// minimum instead of clipping them at the popup border. The tour
    /// scrolls there, so the walk pages through it the way a short
    /// terminal reads it; the widest line's tail must survive wrapping
    /// on whatever page carries it.
    #[test]
    fn the_help_overlay_wraps_its_widest_lines_at_the_supported_minimum() {
        let mut model = plain_model();
        model.open_modal(ModalKind::Help);
        let mut seen = String::new();
        for _ in 0..12 {
            let text = rendered(
                &mut model,
                crate::tui::model::MIN_WIDTH,
                crate::tui::model::MIN_HEIGHT,
            );
            seen.push_str(&text);
            if text.contains("· end ·") {
                break;
            }
            for _ in 0..HELP_PAGE_LINES {
                model.help_scroll(HelpScroll::PageDown);
            }
        }
        assert!(
            seen.contains("buttons, rows"),
            "the Mouse line's tail must survive wrapping at the minimum:\n{seen}"
        );
    }

    /// C1 / review F2: the picker's note row (the honest error
    /// diagnostic) wraps inside its bounded rows instead of clipping
    /// mid-diagnostic at the border. The bound is a deliberate degrade:
    /// at the supported minimum the rows hold the diagnostic's head;
    /// anything beyond the budget is dropped (never a border overrun).
    #[test]
    fn the_picker_note_wraps_its_diagnostic_inside_the_bounded_rows() {
        let mut model = plain_model();
        model.open_modal(ModalKind::Open);
        model.set_open_listing(
            std::path::PathBuf::from("/media"),
            Err("listing failed: the directory could not be read in full detail here".to_owned()),
        );
        let text = rendered(&mut model, 100, 30);
        for word in
            "listing failed: the directory could not be read in full detail here".split_whitespace()
        {
            assert!(
                text.contains(word),
                "the diagnostic word {word:?} must stay visible:\n{text}"
            );
        }
        // The supported minimum: the bounded rows hold the head; the
        // frame stays honest (no border overrun, no fabricated line).
        let text = rendered(
            &mut model,
            crate::tui::model::MIN_WIDTH,
            crate::tui::model::MIN_HEIGHT,
        );
        for word in "listing failed: the directory".split_whitespace() {
            assert!(text.contains(word), "{text}");
        }
        assert_eq!(scan(&text), None, "{text}");
    }

    /// C2: a focused modal control is unmistakable — the keyboard
    /// focus inversion keeps its REVERSED shape carrier and gains the
    /// theme's accent color on top, and the picker's path field (a
    /// focus stop with no button shape) gains a BOLD carrier.
    #[test]
    fn a_focused_modal_control_is_unmistakable() {
        use ratatui::style::{Color, Modifier};

        let mut model = plain_model();
        model.open_modal(ModalKind::AddFile);
        model.set_open_listing(
            std::path::PathBuf::from("/media"),
            Ok(vec![crate::input::DirectoryEntry {
                name: "b.flac".to_owned(),
                is_dir: false,
                path: std::path::PathBuf::from("/media/b.flac"),
            }]),
        );
        // The full Tab cycle, twice around, checking every stop.
        for _ in 0..10 {
            model.move_focus(FocusMove::Next);
            let buffer = rendered_buffer(&mut model, 100, 30);
            match model.focus().expect("a focus stop") {
                FocusId::PickerButton(button) => {
                    let region = model
                        .regions()
                        .iter()
                        .find(|region| region.target == HitTarget::ModalButton(button))
                        .unwrap_or_else(|| panic!("region for {button:?}"));
                    let cell = buffer
                        .cell((region.area.x, region.area.y))
                        .expect("the label's first cell");
                    assert!(
                        cell.modifier.contains(Modifier::REVERSED),
                        "the shape carrier stays on {button:?}"
                    );
                    assert_eq!(
                        cell.fg,
                        Color::Cyan,
                        "the focus accent rides on top for {button:?}"
                    );
                }
                FocusId::ModalField => {
                    let region = model
                        .regions()
                        .iter()
                        .find(|region| region.target == HitTarget::ModalField)
                        .expect("the field region");
                    let cell = buffer
                        .cell((region.area.x + 1, region.area.y))
                        .expect("the field's first character cell");
                    assert!(
                        cell.modifier.contains(Modifier::BOLD),
                        "the focused field carries BOLD"
                    );
                }
                FocusId::PickerList => {
                    // The listing's emphasis shows on its CURSOR row;
                    // give it one (the field's Enter does this natively)
                    // and re-read the frame.
                    model.move_picker_cursor(PlaylistCursor::Row(0));
                    let buffer = rendered_buffer(&mut model, 100, 30);
                    let region = model
                        .regions()
                        .iter()
                        .find(|region| region.target == HitTarget::PickerRow(0))
                        .expect("the cursor row's region");
                    let cell = buffer
                        .cell((region.area.x, region.area.y))
                        .expect("the cursor row's first cell");
                    assert!(
                        cell.modifier.contains(Modifier::REVERSED),
                        "the focused listing marks its cursor row"
                    );
                }
                _ => {}
            }
            // Negative control (review F6): the accent sits ONLY on the
            // focused stop — a bug styling every button would fail here.
            let focused_button = match model.focus() {
                Some(FocusId::PickerButton(button)) => Some(HitTarget::ModalButton(button)),
                _ => None,
            };
            for region in model
                .regions()
                .iter()
                .filter(|region| matches!(region.target, HitTarget::ModalButton(_)))
            {
                if Some(region.target) == focused_button {
                    continue;
                }
                let cell = buffer
                    .cell((region.area.x, region.area.y))
                    .expect("the label's first cell");
                assert_ne!(
                    cell.fg,
                    Color::Cyan,
                    "an unfocused button carries no accent: {:?}",
                    region.target
                );
            }
        }
    }
}
