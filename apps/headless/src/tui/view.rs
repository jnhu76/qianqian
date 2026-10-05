//! ratatui rendering of the T1B shell: the persistent top navigation,
//! the active route's body, and the bottom status line — plus the
//! frame's hit regions, published from the SAME layout decisions that
//! drew the controls (§14: one geometry calculation, two readers).
//!
//! ```text
//! ┌ [Now Playing] [Playlist] [Audio] [Visualizer] ┐  tabs (1 row)
//! │                                               │
//! │  route body (Now Playing / Playlist /         │  main region
//! │  Audio / Visualizer placeholders)             │
//! │                                               │
//! └───────────────────────────────────────────────┘
//!  status / latest feedback                        bottom line
//!  Tab=focus Enter=activate ?=help Q=quit
//! ```
//!
//! This function is presentation only: it reads the model and draws,
//! performs no I/O, resolves no capability, and observes nothing — the
//! runtime owns the single `observe()` read per refresh. It is also the
//! one place geometry exists: every control is rendered from the same
//! `Rect` that is published as its [`HitRegion`], so a hit test can
//! never disagree with what is on screen. Rendering is exercised on
//! ratatui's `TestBackend`, so the layout and the exact vocabulary are
//! pinned without a real terminal.
//!
//! The pane is WYSIWYG by construction: its rows ARE the App's
//! traversal order, so the row below the committed one is exactly the
//! one `N` plays next, and a reorder shows up as a reorder.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::model::{
    FocusId, HitRegion, HitTarget, Modal, PlaylistRow, PreferenceButton, ResponsiveClass,
    TRANSPORT, TuiModel, TuiRoute, responsive_class,
};

/// Shown once the episode's terminal Fact is committed: the shell
/// stays up so the outcome can be inspected; the player keys stay
/// meaningful (the App's EOF policy may already have moved on to the
/// next entry).
pub const COMMITTED_HINT: &str = "terminal outcome committed";

/// The idle page's one honest claim: no episode exists (a fresh
/// interactive launch, or a clean-failed Open), so nothing is playing
/// and nothing is loaded. The panel fabricates no label beyond it.
pub const NO_MUSIC_LINE: &str = "No music loaded.";

/// The below-minimum page (§28): no interactive layout, no hit
/// regions, no focus — one truthful line, and playback continues under
/// the product's own semantics.
pub const TOO_SMALL_LINE: &str = "Terminal too small.";

/// The play marker: the committed (playing) row. Deliberately a shape,
/// not a colour — the shell must stay readable in a monochrome
/// terminal (Issue #166 §20).
const PLAYING_MARKER: &str = "▶";

/// The selection marker: the UI cursor row. Independent of the play
/// marker; one row may carry both.
const SELECTED_MARKER: &str = ">";

/// How many status lines the bottom bar renders at most (a multi-line
/// status BLOCK clips honestly past this, like every other region).
const MAX_STATUS_ROWS: usize = 3;

/// Render one frame of the shell, and publish the frame's hit regions
/// into the model from the same layout this draw used.
pub fn draw(frame: &mut Frame, model: &mut TuiModel) {
    let area = frame.area();
    let class = responsive_class(area.width, area.height);
    model.set_class(class);
    // The frame starts with no geometry; everything published below is
    // drawn THIS frame (§15: stale coordinates never survive).
    let mut regions: Vec<HitRegion> = Vec::new();
    // Focus is validated against the visible enabled controls of THIS
    // frame's class (§12/§29).
    model.validate_focus();

    if class == ResponsiveClass::Minimum {
        render_too_small(frame, area);
        model.publish_regions(regions);
        return;
    }

    // The modal's regions are PUBLISHED first, so the first-match hit
    // test answers a click inside the popup with the modal's own
    // control and never with the background under it (§25). The modal
    // is still PAINTED last, over everything (§23) — publication order
    // and paint order are deliberately independent decisions.
    if let Some(modal) = model.modal() {
        modal_regions(modal, area, &mut regions);
    }

    let status_rows = status_line_count(model);
    let [tabs, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(status_rows as u16),
    ])
    .areas(area);

    draw_tabs(frame, model, tabs, &mut regions);
    match model.route() {
        TuiRoute::NowPlaying => draw_now_playing(frame, model, body, &mut regions),
        TuiRoute::Playlist => draw_playlist(frame, model, body, &mut regions),
        route => draw_placeholder(frame, route, body),
    }
    draw_status(frame, model, status);

    // The modal paints last, over everything (§23).
    if let Some(modal) = model.modal() {
        draw_modal(frame, modal, area, model);
    }

    model.publish_regions(regions);
}

/// The below-minimum page (§28): truthful, non-interactive, and with
/// NO hit regions published — the shell fabricates neither controls
/// nor silence about playback.
fn render_too_small(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(TOO_SMALL_LINE),
        Line::from("Enlarge the window; playback continues."),
    ];
    let height = 4;
    let width = area.width.min(60);
    let box_area = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height.min(area.height),
    );
    frame.render_widget(Clear, box_area);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(bold(" Qianqian "))),
        box_area,
    );
}

/// The persistent top navigation: the four frozen tabs (§6). The active
/// route is bold, the focused tab is inverted — two shapes a monochrome
/// terminal renders distinctly. Each tab's cell is published as its hit
/// region from the very rect it was drawn into.
fn draw_tabs(frame: &mut Frame, model: &TuiModel, area: Rect, regions: &mut Vec<HitRegion>) {
    let cells: [Rect; 4] = Layout::horizontal([Constraint::Ratio(1, 4); 4]).areas(area);
    let compact = model.class() == ResponsiveClass::Compact;
    for (route, cell) in TuiRoute::ALL.iter().zip(cells.iter()) {
        let active = *route == model.route();
        let focused = model.focus() == Some(FocusId::RouteTab(*route));
        let label = if compact {
            route.compact_label()
        } else {
            route.label()
        };
        let mut style = Style::default();
        if active {
            style = style.add_modifier(Modifier::BOLD);
        }
        if focused {
            style = style.add_modifier(Modifier::REVERSED);
        }
        frame.render_widget(Paragraph::new(Line::styled(label, style)), *cell);
        regions.push(HitRegion {
            area: *cell,
            target: HitTarget::RouteTab(*route),
        });
    }
}

/// The Now Playing route (G1): the episode read-side panel with the
/// desired-DSP summary line, the click-to-position seek bar, the
/// transport row with the visible Open control, and the preference row
/// (volume steppers, order, repeat) — every core player function as a
/// visible control that keyboard focus and the mouse both reach.
fn draw_now_playing(frame: &mut Frame, model: &TuiModel, area: Rect, regions: &mut Vec<HitRegion>) {
    let [content, bar, transport, preferences] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .areas(area);

    let compact = model.class() == ResponsiveClass::Compact;
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
            // The desired-DSP summary is secondary detail (G5 §39): it
            // yields to the core controls under constrained layouts
            // before any of them does.
            if !compact && let Some(dsp) = model.desired_dsp_label() {
                lines.push(Line::from(dsp.to_owned()));
            }
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
            super::model::BAR_WIDTH as u16,
            1,
        ))
    }

    // The transport row (§36, G1): Open joins the four transport
    // buttons — the picker entry is a core player control, not a
    // shortcut reserved for those who read the help.
    let buttons: [Rect; 5] = Layout::horizontal([Constraint::Ratio(1, 5); 5]).areas(transport);
    for (button, cell) in TRANSPORT.iter().zip(buttons.iter()) {
        let focused = model.focus() == Some(FocusId::Transport(*button));
        let label = if compact {
            button.compact_label()
        } else {
            button.label()
        };
        let paragraph = Paragraph::new(Line::from(label).centered());
        frame.render_widget(
            if focused {
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
    let compact = model.class() == ResponsiveClass::Compact;
    let [down, label, up, order, repeat] = Layout::horizontal([
        Constraint::Length(3),
        Constraint::Percentage(22),
        Constraint::Length(3),
        Constraint::Percentage(37),
        Constraint::Percentage(38),
    ])
    .areas(area);

    let steppers: [(Rect, PreferenceButton); 2] = [
        (down, PreferenceButton::VolumeDown),
        (up, PreferenceButton::VolumeUp),
    ];
    for (cell, button) in steppers {
        let focused = model.focus() == Some(FocusId::Preference(button));
        let paragraph = Paragraph::new(
            Line::from(if button == PreferenceButton::VolumeDown {
                " − "
            } else {
                " + "
            })
            .centered(),
        );
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
    if let Some(volume) = model.volume_label() {
        let desired = if compact {
            volume
        } else {
            format!("{volume} (desired)")
        };
        frame.render_widget(Paragraph::new(desired).centered(), label);
    }

    let toggles: [(Rect, PreferenceButton, Option<String>); 2] = [
        (
            order,
            PreferenceButton::Order,
            model.order_label().map(|label| {
                if compact {
                    format!("Ord:{}", order_short(label))
                } else {
                    format!("Order: {label}")
                }
            }),
        ),
        (
            repeat,
            PreferenceButton::Repeat,
            model.repeat_label().map(|label| {
                if compact {
                    format!("Rep:{label}")
                } else {
                    format!("Repeat: {label}")
                }
            }),
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

/// The compact spelling of an order label (same policy, shorter word).
fn order_short(label: &str) -> &'static str {
    match label {
        "Sequential" => "Seq",
        "Shuffle" => "Shuf",
        _ => "?",
    }
}

/// The Playlist route: the same pane as before (Issue #166 §6/§20/§21),
/// proving list focus, the selection action, mouse row hits and wheel
/// scroll without turning T1B into playlist productization (§35).
/// Rows and regions come from ONE offset calculation.
fn draw_playlist(frame: &mut Frame, model: &TuiModel, area: Rect, regions: &mut Vec<HitRegion>) {
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

/// The Audio / Visualizer placeholder routes (§33/§34): honest panels
/// that exist so the route model, the focus model and the tabs are
/// complete — no DSP or visualization semantics are claimed.
fn draw_placeholder(frame: &mut Frame, route: TuiRoute, area: Rect) {
    let (title, line) = match route {
        TuiRoute::Audio => (" Audio ", "Audio route — not started (T4)."),
        TuiRoute::Visualizer => (" Visualizer ", "Visualizer route — not started (T5)."),
        _ => return,
    };
    frame.render_widget(
        Paragraph::new(vec![Line::from(""), Line::from(line)])
            .block(Block::bordered().title(bold(title))),
        area,
    );
}

/// The bottom status/context line (§6): the latest operation feedback
/// (application composition feedback, never a playback semantic) and
/// one fixed keymap hint.
fn draw_status(frame: &mut Frame, model: &TuiModel, area: Rect) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(status) = model.status() {
        for line in status.lines().take(MAX_STATUS_ROWS) {
            lines.push(Line::from(line.to_owned()));
        }
    }
    lines.push(Line::from(HINT_LINE));
    frame.render_widget(Paragraph::new(lines), area);
}

/// The one-line keymap hint under the status feedback (§6's
/// "status / latest feedback" line). The FULL keymap lives in the Help
/// modal; this line only names the foundation keys.
const HINT_LINE: &str = "Tab=focus  Enter=activate  O=open  ?=help  Q=quit";

/// How many rows the bottom bar needs: the status block (capped) plus
/// the hint line — always at least one.
fn status_line_count(model: &TuiModel) -> usize {
    let status_rows = model
        .status()
        .map(|status| status.lines().count())
        .unwrap_or(0)
        .min(MAX_STATUS_ROWS);
    status_rows + 1
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

/// The Open picker's geometry for ONE frame: the popup, its field row,
/// the listing rows area, the (optional) honest error row, the commit
/// buttons and the hint line. The SINGLE layout decision (§14) —
/// [`modal_regions`] publishes from it and [`draw_modal`] paints from
/// it, so a hit test cannot disagree with what is on screen.
struct PickerLayout {
    popup: Rect,
    field: Rect,
    list: Rect,
    error: Option<Rect>,
    buttons: [Rect; 3],
    hint: Rect,
}

/// Compute the picker's layout for one frame. The listing window shows
/// as many entries as the popup can hold (degrading honestly on small
/// terminals), and nothing ever exceeds the terminal area.
fn picker_layout(picker: &super::model::OpenPicker, area: Rect) -> PickerLayout {
    let error_rows = u16::from(picker.error.is_some());
    let width = area.width.min(64).saturating_sub(4).max(16);
    // field + buttons + hint + borders (+ an honest error row).
    let reserved = 3 + 2 + error_rows;
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
    let [list, error, buttons, hint] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(error_rows),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(rest);
    let cells: [Rect; 3] = Layout::horizontal([
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
    ])
    .areas(buttons);
    PickerLayout {
        popup,
        field,
        list,
        error: (error_rows > 0).then_some(error),
        buttons: cells,
        hint,
    }
}

/// The ORDER of the picker's three commit buttons, in render (and Tab)
/// order.
const PICKER_BUTTONS: [super::model::ModalButton; 3] = [
    super::model::ModalButton::Open,
    super::model::ModalButton::Add,
    super::model::ModalButton::Cancel,
];

fn button_label(button: super::model::ModalButton) -> &'static str {
    match button {
        super::model::ModalButton::Open => "[ Open ]",
        super::model::ModalButton::Add => "[ Add to Playlist ]",
        super::model::ModalButton::Cancel => "[ Cancel ]",
    }
}

/// Publish the active modal's hit regions from the SAME layout the
/// modal paints from. Only the Open picker has mouse controls; the
/// GoTo and Help modals stay keyboard-owned and publish none.
fn modal_regions(modal: &Modal, area: Rect, regions: &mut Vec<HitRegion>) {
    let Modal::Open(picker) = modal else {
        return;
    };
    let layout = picker_layout(picker, area);
    regions.push(HitRegion {
        area: layout.field,
        target: HitTarget::ModalField,
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
fn draw_modal(frame: &mut Frame, modal: &Modal, area: Rect, model: &TuiModel) {
    match modal {
        Modal::Open(picker) => {
            let layout = picker_layout(picker, area);
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

            if let (Some(error), Some(slot)) = (&picker.error, layout.error) {
                frame.render_widget(Paragraph::new(error.clone()), slot);
            }

            // The listing window: the same stateless offset rule as the
            // playlist pane, the cursor marked `>`, directories spelled
            // with a trailing `/`.
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
                let paragraph = Paragraph::new(Line::from(button_label(*button)).centered());
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
                Paragraph::new("Enter open · ↑↓ select · Tab cycle · Esc cancel").centered(),
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

fn bold(title: &'static str) -> Span<'static> {
    Span::styled(title, Style::default().add_modifier(Modifier::BOLD))
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
        Line::from("   Tab / Shift+Tab   path field / listing / commit buttons"),
        Line::from("   ↑ / ↓ / wheel     move the listing selection"),
        Line::from("   Enter             open the selection or the typed path"),
        Line::from("   Backspace         on the listing: up to the parent folder"),
        Line::from("   [Open] / [Add]    commit buttons; Add appends to the playlist"),
        Line::from("   Esc               cancel"),
        Line::from(""),
        Line::from(" Playback"),
        Line::from("   Space             pause / resume"),
        Line::from("   ← / →             seek 5 s back / forward"),
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
    //! Rendering pinned on ratatui's `TestBackend` (a virtual terminal,
    //! no ANSI buffers): the shell geometry, the published hit regions,
    //! the truth classes, and the forbidden-semantics negative control
    //! from [`crate::status`].

    use super::*;
    use crate::status::forbidden_status_claim;
    use crate::tui::model::{FocusId, ModalKind, TransportButton, responsive_class};
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
            last_processing_refusal: None,
        }
    }

    fn stopped() -> PlaybackSessionObservation {
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
    fn scan(text: &str) -> Option<&'static str> {
        let mut lines = text.splitn(2, '\n');
        let _tabs_row = lines.next();
        lines.next().and_then(forbidden_status_claim)
    }

    fn established(model: &mut TuiModel) {
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
    fn rendered(model: &mut TuiModel, width: u16, height: u16) -> String {
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

    fn plain_model() -> TuiModel {
        let mut model = TuiModel::new("song.flac");
        established(&mut model);
        model
    }

    // ------------------------------------------------------------------
    // The shell: tabs / route body / status line.
    // ------------------------------------------------------------------

    /// The persistent shell shows the four tabs on every route, and the
    /// route bodies differ (§6).
    #[test]
    fn the_shell_shows_the_four_frozen_tabs() {
        let mut model = plain_model();
        for route in TuiRoute::ALL {
            model.set_route(route);
            let text = rendered(&mut model, 100, 30);
            for tab in TuiRoute::ALL {
                assert!(
                    text.contains(tab.label()),
                    "{:?}: tab {:?} missing in:\n{text}",
                    route.label(),
                    tab.label()
                );
            }
        }
    }

    /// The active tab is bold; a focused tab renders (the REVERSED
    /// style does not change the text). The tab row carries all four
    /// labels in order.
    #[test]
    fn the_tab_row_carries_the_tabs_in_order() {
        let mut model = plain_model();
        model.set_route(TuiRoute::Playlist);
        model.set_focus(Some(FocusId::RouteTab(TuiRoute::Playlist)));
        let text = rendered(&mut model, 100, 30);
        let now = text.find("Now Playing").expect("Now Playing tab");
        let playlist = text.find("Playlist").expect("Playlist tab");
        let audio = text.find("Audio").expect("Audio tab");
        let visualizer = text.find("Visualizer").expect("Visualizer tab");
        assert!(
            now < playlist && playlist < audio && audio < visualizer,
            "{text}"
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
            "◀ Prev",
            "Play/Pause",
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

    /// The placeholder routes render honest not-started panels (§33/§34)
    /// and no transport/list controls.
    #[test]
    fn the_placeholder_routes_render_honest_not_started_panels() {
        let mut model = plain_model();
        model.set_route(TuiRoute::Audio);
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("Audio route — not started (T4)."), "{text}");
        assert!(
            !text.contains("Play/Pause"),
            "no transport on a placeholder: {text}"
        );
        assert_eq!(
            model
                .regions()
                .iter()
                .filter(|region| matches!(region.target, HitTarget::Transport(_)))
                .count(),
            0,
            "no transport regions on a placeholder route"
        );

        model.set_route(TuiRoute::Visualizer);
        let text = rendered(&mut model, 100, 30);
        assert!(
            text.contains("Visualizer route — not started (T5)."),
            "{text}"
        );
        assert_eq!(scan(&text), None, "{text}");
    }

    // ------------------------------------------------------------------
    // The bottom status line.
    // ------------------------------------------------------------------

    /// The status feedback renders under the body, and the hint line is
    /// always present.
    #[test]
    fn the_status_line_renders_feedback_and_the_hint() {
        let mut model = plain_model();
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("Tab=focus"), "{text}");

        model.set_status(Some("opened D:\\media (6 candidates)".to_owned()));
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("opened D:\\media (6 candidates)"), "{text}");
        assert_eq!(scan(&text), None, "{text}");
    }

    // ------------------------------------------------------------------
    // The modal popups.
    // ------------------------------------------------------------------

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
                        assert!(text.contains("Enter open"), "{text}");
                        assert!(text.contains("[ Add to Playlist ]"), "{text}");
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

    #[test]
    fn a_fresh_episode_renders_pending_without_inventing_state() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        let text = rendered(&mut model, 100, 30);
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
        assert_eq!(scan(&text), None, "{text}");
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
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("Pause requested: true"), "{text}");
        assert!(text.contains("Paused: false"), "intent alone: {text}");
        assert_eq!(scan(&text), None, "{text}");

        model.update(PlaybackSessionObservation {
            pause_requested: true,
            pause_engagement: PauseEngagement::TailQuiesced,
            ..pending()
        });
        let text = rendered(&mut model, 100, 30);
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
    fn each_terminal_fact_renders_its_own_label() {
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
            let text = rendered(&mut model, 100, 30);
            assert!(text.contains(label), "{label:?} missing in:\n{text}");
            assert!(
                text.contains(COMMITTED_HINT),
                "committed hint missing in:\n{text}"
            );
            assert_eq!(scan(&text), None, "{text}");
        }
    }

    /// An activation failure and a published failure diagnostic both
    /// render (C13) — as presentation text, never as forged facts.
    #[test]
    fn diagnostics_render_as_diagnostics() {
        let mut model = TuiModel::new("song.flac");
        model.update(PlaybackSessionObservation {
            activation_error: Some("render stream open failed: no device".to_owned()),
            failure_diagnostic: Some("decode: corrupt frame".to_owned()),
            ..pending()
        });
        let text = rendered(&mut model, 100, 30);
        assert!(
            text.contains("activation: render stream open failed: no device"),
            "{text}"
        );
        assert!(text.contains("failure: decode: corrupt frame"), "{text}");
        assert!(
            text.contains("Terminal: pending"),
            "no terminal Fact: {text}"
        );
        assert_eq!(scan(&text), None, "{text}");
    }

    /// The pane shows no playing marker when no episode is live, and
    /// the idle page keeps the honest navigation line (labeled a
    /// cursor, never a playback claim).
    #[test]
    fn the_pane_shows_no_playing_marker_when_no_episode_is_live() {
        let mut model = TuiModel::new("song.flac");
        model.set_playlist(1, || {
            vec![
                PlaylistRow {
                    label: "flac4.flac".to_owned(),
                    playing: true,
                    selected: true,
                },
                PlaylistRow {
                    label: "alac4.m4a".to_owned(),
                    playing: false,
                    selected: false,
                },
            ]
        });
        model.set_navigation(Some((1, 2)));
        // The pane renders on the Playlist route; a live episode: the
        // committed row carries the marker.
        model.set_route(TuiRoute::Playlist);
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("▶ >   1  flac4.flac"), "{text}");

        // The same navigation state with no live episode (a clean-failed
        // Open): the cursor is still shown, the play marker is not.
        model.set_episode(None);
        let text = rendered(&mut model, 100, 30);
        assert!(!text.contains(PLAYING_MARKER), "{text}");
        assert!(text.contains("  >   1  flac4.flac"), "{text}");

        // The idle page on Now Playing keeps the honest navigation line.
        model.set_route(TuiRoute::NowPlaying);
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains(NO_MUSIC_LINE), "{text}");
        assert!(text.contains("Track: 1/2 (navigation cursor)"), "{text}");
        assert_eq!(scan(&text), None, "{text}");
    }

    /// The no-episode frame (F6; the U1 idle page) fabricates nothing —
    /// no Source/Format/Position/Terminal labels, and no operation
    /// feedback for an Open that was never attempted. The visible Open
    /// control is the discoverable entry (G1), so the idle page names
    /// it instead of a shortcut.
    #[test]
    fn a_no_episode_frame_says_so_and_fabricates_nothing() {
        let mut model = TuiModel::new("song.flac");
        model.set_episode(None);
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains(NO_MUSIC_LINE), "{text}");
        assert!(
            text.contains("Open a file or folder with the Open button below"),
            "{text}"
        );
        for fabricated in ["Source:", "Format:", "Position:", "Terminal:", "Paused:"] {
            assert!(
                !text.contains(fabricated),
                "{fabricated} must not render without an episode:\n{text}"
            );
        }
        assert_eq!(scan(&text), None, "{text}");
    }

    /// Open operation feedback renders as composition feedback — a
    /// refusal, a clean failure, or the fail-stop banner — and never
    /// as a playback claim. A multi-line status block renders every
    /// line (up to the bar's cap).
    #[test]
    fn operation_feedback_renders_as_composition_feedback() {
        for status in [
            "opened /media/b.flac".to_owned(),
            "open refused: unsupported container: /media/x.txt".to_owned(),
            "open failed (clean): decode open failed".to_owned(),
            "FAIL-STOP: old episode teardown violated (§G.6 latch; no exit)".to_owned(),
            "opened /media (2 candidates, 1 skipped; 1 scan warning)\n\
             scan warning: cannot read /media/album-c: access denied"
                .to_owned(),
        ] {
            let mut model = TuiModel::new("song.flac");
            model.update(pending());
            model.set_status(Some(status.clone()));
            let text = rendered(&mut model, 100, 32);
            for line in status.lines().take(MAX_STATUS_ROWS) {
                assert!(text.contains(line), "{line:?} missing in:\n{text}");
            }
            assert_eq!(scan(&text), None, "{text}");
        }
    }

    /// The preference row renders the App's desired stream factor
    /// between its steppers (D14.9 read side: exactly the configured
    /// value, never an acoustic or mechanism claim).
    #[test]
    fn the_volume_line_renders_the_desired_factor() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        assert!(
            !rendered(&mut model, 100, 30).contains("(desired)"),
            "before the first refresh the model holds no level and renders none"
        );
        model.set_volume(Some(80));
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("80/100 (desired)"), "{text}");
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

    /// Below the minimum the shell renders the truthful message and
    /// publishes NO interactive geometry — no stale hit regions, no
    /// focus (§28).
    #[test]
    fn a_below_minimum_frame_has_no_interactive_geometry() {
        let mut model = plain_model();
        model.set_focus(Some(FocusId::Transport(TransportButton::PlayPause)));
        let text = rendered(&mut model, 30, 10);
        assert!(text.contains(TOO_SMALL_LINE), "{text}");
        assert!(
            !text.contains("Play/Pause"),
            "no interactive layout below the minimum:\n{text}"
        );
        assert!(
            model.regions().is_empty(),
            "below the minimum no hit regions survive: {:?}",
            model.regions()
        );
        assert_eq!(model.focus(), None, "no invisible focus below the minimum");
        assert_eq!(model.class(), ResponsiveClass::Minimum);
        assert_eq!(responsive_class(30, 10), ResponsiveClass::Minimum);
    }

    /// The compact class shortens the tab labels — the mechanics exist
    /// so controls do not overlap on a narrow terminal (§27).
    #[test]
    fn the_compact_class_shortens_the_tab_labels() {
        let mut model = plain_model();
        let text = rendered(&mut model, 50, 16);
        assert!(text.contains("Now"), "{text}");
        assert!(text.contains("List"), "{text}");
        assert!(text.contains("Viz"), "{text}");
        assert_eq!(model.class(), ResponsiveClass::Compact);
    }

    /// A large → small → large resize sequence keeps the shell coherent
    /// at every step: rendering, fresh geometry, and a validated focus
    /// (§29's draw half; the event half lives in the runtime tests).
    #[test]
    fn a_resize_sequence_keeps_the_shell_coherent() {
        let mut model = plain_model();
        model.set_focus(Some(FocusId::Transport(TransportButton::PlayPause)));
        let large = rendered(&mut model, 100, 30);
        assert!(large.contains("Play/Pause"), "{large}");
        assert_eq!(model.class(), ResponsiveClass::Wide);

        let small = rendered(&mut model, 30, 10);
        assert!(small.contains(TOO_SMALL_LINE), "{small}");
        assert_eq!(model.focus(), None, "below the minimum nothing is focused");

        let large_again = rendered(&mut model, 100, 30);
        assert!(
            large_again.contains("Play/Pause"),
            "the shell is back after growing:\n{large_again}"
        );
        assert_eq!(
            model.focus(),
            Some(FocusId::Transport(TransportButton::Open)),
            "focus revalidates to a visible enabled control (§12: the route's \
             first local control)"
        );
    }

    /// A full draw at a series of shrinking sizes — down to a 4×3
    /// window — never panics and never renders an unearned semantic
    /// (C10). Layout degrades by clipping; the truth classes live in
    /// the model, not in the geometry.
    #[test]
    fn tiny_terminals_degrade_without_panicking_or_fabricating() {
        let mut model = plain_model();
        model.update(stopped());
        model.set_status(Some("volume 75/100 (desired)".to_owned()));
        model.open_modal(ModalKind::Open);
        for c in "synth30.flac".chars() {
            model.modal_push(c);
        }
        for (width, height) in [(100u16, 24u16), (40, 12), (20, 8), (10, 6), (4, 3), (1, 1)] {
            let text = rendered(&mut model, width, height);
            assert_eq!(
                scan(&text),
                None,
                "unearned semantic at {width}x{height}:\n{text}"
            );
        }
    }

    /// A very long source path clips at the panel edge without
    /// corrupting the rows below it (C11): the display truncates; the
    /// model keeps the full internal identity.
    #[test]
    fn a_long_source_path_clips_without_corrupting_other_rows() {
        let long_path = format!(
            "C:\\very\\long\\prefix\\{}\\season.takes.flac",
            "directory_component_".repeat(10)
        );
        let mut model = TuiModel::new(long_path.clone());
        model.update(pending());
        assert_eq!(model.source(), Some(long_path.as_str()), "identity intact");
        let text = rendered(&mut model, 100, 30);
        // The clipped Source line still names the beginning of the
        // path, and the panel below it is uncorrupted.
        assert!(text.contains("Source: C:\\very\\long\\prefix\\"), "{text}");
        assert!(text.contains("Format: pending"), "{text}");
        assert!(text.contains("◀ Prev"), "{text}");
        assert_eq!(scan(&text), None, "{text}");
    }

    /// CJK filenames render structurally (C12): the Source line shows
    /// the characters, the Open modal accepts CJK characters and
    /// backspace pops ONE character, and the frame stays free of
    /// unearned semantics.
    #[test]
    fn cjk_filenames_render_and_edit_structurally() {
        let mut model = TuiModel::new("千曲テスト曲.flac");
        model.update(pending());
        let text = rendered(&mut model, 100, 30);
        // Wide CJK glyphs occupy two cells; the skipped cells surface as
        // blanks in this test's per-cell text reconstruction (a real
        // terminal renders them as one glyph). Structural assertion is
        // on the space-normalized text.
        let compact = text.replace(' ', "");
        assert!(compact.contains("Source:千曲テスト曲.flac"), "{text}");
        assert_eq!(scan(&text), None, "{text}");

        model.open_modal(ModalKind::Open);
        for c in "音楽/千曲.flac".chars() {
            model.modal_push(c);
        }
        model.modal_backspace();
        let text = rendered(&mut model, 100, 30);
        let compact = text.replace(' ', "");
        assert!(
            compact.contains("音楽/千曲.fla▏"),
            "backspace popped exactly one character:\n{text}"
        );
        assert!(text.contains("Enter open"), "{text}");
        assert_eq!(scan(&text), None, "{text}");
    }
    // ------------------------------------------------------------------
    // G1: the seek bar affordance, the DSP summary line, the picker.
    // ------------------------------------------------------------------

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
        model.set_desired_dsp(super::super::model::dsp_summary(
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

    /// The Open picker publishes regions for its field row, its
    /// VISIBLE listing rows and its commit buttons, from the same
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
                })
                .collect()),
        );
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains(" Open "), "{text}");
        assert!(text.contains("[ Add to Playlist ]"), "{text}");

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
            super::super::model::ModalButton::Open,
            super::super::model::ModalButton::Add,
            super::super::model::ModalButton::Cancel,
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
