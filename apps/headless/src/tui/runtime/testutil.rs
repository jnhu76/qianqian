//! Shared test helpers for the runtime submodules' unit tests.
//!
//! Action-routing tests for the shell over the SAME fake episode
//! harness the player's C7 matrix uses (real kernel, real playback
//! session, fake providers). Unlike the player matrix, these tests
//! use REAL temporary files and folders for everything that crosses
//! the U1 input expansion — that seam reads the filesystem, and
//! faking it here would test nothing. The terminal itself stays
//! fake-free by construction: decode + dispatch is the loop's whole
//! reaction to an input event and needs no terminal.

use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::dispatch::dispatch;
use crate::player::{EpisodeStart, ReferencePlayerApp};
use crate::tui::model::{HitTarget, Step, TuiAction, TuiModel, decode_key, decode_mouse};
use crate::tui::view;

/// A fresh unique temporary directory for one test.
pub(crate) struct TempTree(PathBuf);

impl TempTree {
    pub(crate) fn new(name: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("qianqian-tui-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("temp tree root");
        Self(base)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    /// A regular file the fake probe accepts (it refuses only
    /// paths containing "invalid").
    pub(crate) fn live_file(&self, relative: &str) -> PathBuf {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent dir");
        }
        fs::write(&path, b"not audio; the fake decode reads nothing").expect("temp file");
        path
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// One synthetic mouse event at a cell (the model tests' shape).
pub(crate) fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// The center cell of one published hit region.
pub(crate) fn region_cell(model: &TuiModel, target: &HitTarget) -> (u16, u16) {
    let region = model
        .regions()
        .iter()
        .find(|region| &region.target == target)
        .unwrap_or_else(|| panic!("no region for {target:?} in the published frame"));
    (region.area.x + 1, region.area.y + region.area.height / 2)
}

/// The loop's whole reaction to one key press: decode, then
/// dispatch. Needs no terminal.
pub(crate) fn handle_key<S: EpisodeStart>(
    event: KeyEvent,
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
) -> Step {
    match decode_key(event, model) {
        Some(action) => dispatch(action, model, player),
        None => Step::Continue,
    }
}

/// The loop's whole reaction to one left click at a terminal cell:
/// a Down then an Up at the same cell, each decoded and dispatched.
/// Needs no terminal beyond the published regions the model
/// already holds.
pub(crate) fn click_at<S: EpisodeStart>(
    column: u16,
    row: u16,
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
) -> Option<TuiAction> {
    let down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    let up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    if let Some(action) = decode_mouse(down, model) {
        let _ = dispatch(action, model, player);
    }
    match decode_mouse(up, model) {
        Some(action) => {
            let _ = dispatch(action, model, player);
            Some(action)
        }
        None => None,
    }
}

/// Draw the model at a fixed size so the published regions are the
/// real frame geometry, then return the first cell of the region
/// with the wanted target.
pub(crate) fn draw_and_locate(
    model: &mut TuiModel,
    width: u16,
    height: u16,
    target: &HitTarget,
) -> (u16, u16) {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
        .expect("virtual terminal");
    terminal
        .draw(|frame| view::draw(frame, model))
        .expect("draw");
    let region = model
        .regions()
        .iter()
        .find(|region| &region.target == target)
        .unwrap_or_else(|| panic!("no region for {target:?}"));
    (region.area.x + 1, region.area.y + region.area.height / 2)
}

pub(crate) fn type_text<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
    text: &str,
) {
    for c in text.chars() {
        assert_eq!(
            handle_key(key(KeyCode::Char(c)), model, player),
            Step::Continue
        );
    }
}

/// The whole O flow from a model state: open the modal, type, Enter.
/// The typed-path flow under the frozen picker grammar (T0): the
/// typed line + Enter NAVIGATES a directory or SELECTS a named
/// file — it never starts playback — and the explicit [Open]
/// button (Tab to it, Enter on it) commits. Keyboard-only, no
/// mouse, no hidden shortcuts.
pub(crate) fn open_via_keys<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
    target: &Path,
) {
    assert_eq!(
        handle_key(key(KeyCode::Char('O')), model, player),
        Step::Continue
    );
    assert!(matches!(
        model.modal(),
        Some(crate::tui::model::Modal::Open { .. })
    ));
    type_text(model, player, &target.to_string_lossy());
    assert_eq!(
        handle_key(key(KeyCode::Enter), model, player),
        Step::Continue,
        "the field's Enter navigates or selects; it never commits"
    );
    assert!(
        model.modal().is_some(),
        "the picker stays open after the field's Enter"
    );
    // Tab to the [Open] button and activate it.
    for _ in 0..8 {
        assert_eq!(handle_key(key(KeyCode::Tab), model, player), Step::Continue);
        if model.focus()
            == Some(crate::tui::model::FocusId::PickerButton(
                crate::tui::model::ModalButton::Open,
            ))
        {
            break;
        }
    }
    assert_eq!(
        model.focus(),
        Some(crate::tui::model::FocusId::PickerButton(
            crate::tui::model::ModalButton::Open
        )),
        "Tab reaches the [Open] button from the field"
    );
    assert_eq!(
        handle_key(key(KeyCode::Enter), model, player),
        Step::Continue
    );
}
