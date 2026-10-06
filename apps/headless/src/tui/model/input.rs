//! The keyboard decoder: one key press becomes AT MOST ONE
//! [`TuiAction`](super::actions::TuiAction). Priority: modal input >
//! focused control > global accelerators; release events are
//! presentation noise.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::actions::{PlaylistCursor, TuiAction};
use super::focus::{FocusId, FocusMove};
use super::modal::{Modal, ModalButton, ModalInput, ModalKind};
use super::projection::{LARGE_SEEK_STEP_SECS, SEEK_STEP_SECS};
use super::responsive::ResponsiveClass;
use super::state::TuiModel;

/// Whether a key press carries no modifier, or only SHIFT (terminals
/// disagree about reporting SHIFT with a character, so both act for
/// every LETTER key).
fn plain(key: KeyEvent) -> bool {
    key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT
}

/// Decode one key event into AT MOST ONE [`TuiAction`] (§9/§31).
///
/// Priority: modal input > focused control (Enter) and its contextual
/// arrows > global accelerators. Release events are presentation noise.
///
/// The global accelerator table is the SHRUNK v2 table: the playlist
/// arrows became contextual to the list focus, and Enter became the
/// focused-control activation — the transport keys, seek keys, policy
/// keys, Open/GoTo/Help and quit keep their existing meanings.
pub fn decode_key(key: KeyEvent, model: &TuiModel) -> Option<TuiAction> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    // The conventional quit is the ONE key that works everywhere,
    // including inside a modal (that is why it is not part of any
    // modal's vocabulary).
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(TuiAction::Quit);
    }
    if model.modal().is_some() {
        return decode_modal_key(key, model);
    }
    match key.code {
        KeyCode::Tab => Some(TuiAction::MoveFocus(FocusMove::Next)),
        KeyCode::BackTab => Some(TuiAction::MoveFocus(FocusMove::Previous)),
        KeyCode::Enter if plain(key) => Some(TuiAction::ActivateFocused),
        // Arrow behavior is contextual to the focused group (§10): the
        // playlist list owns Up/Down, and they never move focus. Left/
        // Right keep their global seek meaning — the two axes never
        // share a key.
        KeyCode::Up if plain(key) && model.focus() == Some(FocusId::Playlist) => {
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Previous))
        }
        KeyCode::Down if plain(key) && model.focus() == Some(FocusId::Playlist) => {
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Next))
        }
        KeyCode::Left if key.modifiers.is_empty() => Some(TuiAction::SeekRelative(-SEEK_STEP_SECS)),
        KeyCode::Right if key.modifiers.is_empty() => Some(TuiAction::SeekRelative(SEEK_STEP_SECS)),
        KeyCode::Left if key.modifiers == KeyModifiers::SHIFT => {
            Some(TuiAction::SeekRelative(-LARGE_SEEK_STEP_SECS))
        }
        KeyCode::Right if key.modifiers == KeyModifiers::SHIFT => {
            Some(TuiAction::SeekRelative(LARGE_SEEK_STEP_SECS))
        }
        // Space (§25, T0 focus rule): a focused control consumes it
        // exactly like Enter; the global pause/resume/play accelerator
        // acts only when no focused control exists to consume it.
        KeyCode::Char(' ') if plain(key) => {
            if model.focus().is_some() {
                Some(TuiAction::ActivateFocused)
            } else {
                Some(TuiAction::PlayPause)
            }
        }
        KeyCode::Char('s') | KeyCode::Char('S') if plain(key) => Some(TuiAction::Stop),
        KeyCode::Char('o') | KeyCode::Char('O') if plain(key) => {
            Some(TuiAction::OpenModal(ModalKind::Open))
        }
        KeyCode::Char('g') | KeyCode::Char('G') if plain(key) => {
            Some(TuiAction::OpenModal(ModalKind::GoTo))
        }
        KeyCode::Char('?') if plain(key) => Some(TuiAction::OpenModal(ModalKind::Help)),
        KeyCode::Char('n') | KeyCode::Char('N') if plain(key) => Some(TuiAction::Next),
        KeyCode::Char('p') | KeyCode::Char('P') if plain(key) => Some(TuiAction::Previous),
        KeyCode::Char('r') | KeyCode::Char('R') if plain(key) => Some(TuiAction::ToggleOrder),
        KeyCode::Char('l') | KeyCode::Char('L') if plain(key) => Some(TuiAction::CycleRepeat),
        KeyCode::Char('+') | KeyCode::Char('=') if plain(key) => Some(TuiAction::VolumeUp),
        KeyCode::Char('-') | KeyCode::Char('_') if plain(key) => Some(TuiAction::VolumeDown),
        KeyCode::Char('q') | KeyCode::Char('Q') if plain(key) => Some(TuiAction::Quit),
        _ => None,
    }
}

/// The modal owns the keyboard while it is open (§24). No key both
/// edits and executes: inside a text modal every plain character is a
/// literal character — `q`/`Q` included, so `Q:\Music` stays typeable
/// (Issue #166 §29), and the same rule makes the picker's path line an
/// ordinary text field, so it has NO letter accelerators — and the
/// help overlay lets only its close keys through, so no playback key
/// can fire behind it.
///
/// The Open picker's grammar (G1 §8): Tab cycles field → list →
/// commit buttons; ↑/↓ move the listing cursor; Enter acts on the
/// focused picker control (the field confirms its typed subject, a
/// listing row descends into a directory or opens a file, a commit
/// button commits); Backspace on the listing steps up to the parent
/// directory; Esc cancels.
fn decode_modal_key(key: KeyEvent, model: &TuiModel) -> Option<TuiAction> {
    let modal_kind = model.modal().map(Modal::kind)?;
    // Below the minimum size the shell renders no interactive layout
    // and paints no popup (§28): the modal's keys go inert except the
    // cancel — a blind Enter behind an invisible popup must never
    // commit a real Open. Ctrl+C keeps its conventional meaning (it is
    // decoded before this function).
    if model.class() == ResponsiveClass::Minimum {
        return match key.code {
            KeyCode::Esc => Some(TuiAction::ModalInput(ModalInput::Cancel)),
            _ => None,
        };
    }
    match modal_kind {
        ModalKind::Help => match key.code {
            KeyCode::Esc => Some(TuiAction::ModalInput(ModalInput::Cancel)),
            KeyCode::Char('?') if plain(key) => Some(TuiAction::ModalInput(ModalInput::Cancel)),
            KeyCode::Char('q') | KeyCode::Char('Q') if plain(key) => Some(TuiAction::Quit),
            _ => None,
        },
        ModalKind::GoTo => match key.code {
            KeyCode::Esc => Some(TuiAction::ModalInput(ModalInput::Cancel)),
            KeyCode::Enter if plain(key) => Some(TuiAction::ModalInput(ModalInput::Confirm)),
            KeyCode::Backspace if plain(key) => Some(TuiAction::ModalInput(ModalInput::Backspace)),
            KeyCode::Char(c) if plain(key) => Some(TuiAction::ModalInput(ModalInput::Char(c))),
            _ => None,
        },
        ModalKind::Open | ModalKind::AddFile | ModalKind::AddFolder => match key.code {
            KeyCode::Esc => Some(TuiAction::ModalInput(ModalInput::Cancel)),
            KeyCode::Tab => Some(TuiAction::MoveFocus(FocusMove::Next)),
            KeyCode::BackTab => Some(TuiAction::MoveFocus(FocusMove::Previous)),
            KeyCode::Enter if plain(key) => Some(TuiAction::ModalInput(match model.focus() {
                Some(FocusId::PickerList) => ModalInput::ListActivate,
                Some(FocusId::PickerButton(ModalButton::Open)) => ModalInput::CommitOpen,
                Some(FocusId::PickerButton(ModalButton::Add)) => ModalInput::CommitAdd,
                Some(FocusId::PickerButton(ModalButton::UseFolder)) => ModalInput::UseFolder,
                Some(FocusId::PickerButton(ModalButton::EnterFolder)) => ModalInput::ListActivate,
                Some(FocusId::PickerButton(ModalButton::Cancel)) => ModalInput::Cancel,
                _ => ModalInput::Confirm,
            })),
            KeyCode::Up if plain(key) => Some(TuiAction::ModalInput(ModalInput::ListMove(
                PlaylistCursor::Previous,
            ))),
            KeyCode::Down if plain(key) => Some(TuiAction::ModalInput(ModalInput::ListMove(
                PlaylistCursor::Next,
            ))),
            KeyCode::Backspace if plain(key) => {
                match model.focus() {
                    // On the listing, Backspace is the parent step; on
                    // the field it edits the line.
                    Some(FocusId::PickerList) => {
                        Some(TuiAction::ModalInput(ModalInput::ListParent))
                    }
                    _ => Some(TuiAction::ModalInput(ModalInput::Backspace)),
                }
            }
            KeyCode::Char(c) if plain(key) => Some(TuiAction::ModalInput(ModalInput::Char(c))),
            _ => None,
        },
        // The stop-aware confirmation (T0): Tab between the two
        // buttons, Enter activates the FOCUSED one (Cancel starts
        // focused), Esc cancels. No field exists, so characters are
        // inert — a stray keystroke can never confirm the edit.
        ModalKind::ConfirmRemoveCurrent | ModalKind::ConfirmClear => match key.code {
            KeyCode::Esc => Some(TuiAction::ModalInput(ModalInput::Cancel)),
            KeyCode::Tab => Some(TuiAction::MoveFocus(FocusMove::Next)),
            KeyCode::BackTab => Some(TuiAction::MoveFocus(FocusMove::Previous)),
            KeyCode::Enter if plain(key) => Some(TuiAction::ActivateFocused),
            _ => None,
        },
        // The preset menu (G3): ↑/↓ move the cursor, Enter activates it
        // (fills the draft's EQ stage), Esc cancels. No field exists,
        // so characters are inert.
        ModalKind::Presets => match key.code {
            KeyCode::Esc => Some(TuiAction::ModalInput(ModalInput::Cancel)),
            KeyCode::Up => Some(TuiAction::ModalInput(ModalInput::ListMove(
                PlaylistCursor::Previous,
            ))),
            KeyCode::Down => Some(TuiAction::ModalInput(ModalInput::ListMove(
                PlaylistCursor::Next,
            ))),
            KeyCode::Enter if plain(key) => Some(TuiAction::ActivateFocused),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use crate::tui::model::testutil::{key, model_with_regions};
    use crate::tui::model::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    #[test]
    fn s_maps_to_stop_and_q_maps_to_quit() {
        let model = TuiModel::new("song.flac");
        for key_char in ['s', 'S'] {
            assert_eq!(
                decode_key(
                    KeyEvent::new(KeyCode::Char(key_char), KeyModifiers::NONE),
                    &model
                ),
                Some(TuiAction::Stop),
                "{key_char} must request stop"
            );
        }
        for key_char in ['q', 'Q'] {
            assert_eq!(
                decode_key(
                    KeyEvent::new(KeyCode::Char(key_char), KeyModifiers::NONE),
                    &model
                ),
                Some(TuiAction::Quit),
                "{key_char} must quit"
            );
        }
        // Terminals disagree about reporting SHIFT with a letter.
        assert_eq!(
            decode_key(
                KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT),
                &model
            ),
            Some(TuiAction::Stop)
        );
    }

    #[test]
    fn ctrl_c_keeps_its_conventional_quit_meaning_everywhere() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(
            decode_key(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &model
            ),
            Some(TuiAction::Quit)
        );
        model.open_modal(ModalKind::Open);
        assert_eq!(
            decode_key(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &model
            ),
            Some(TuiAction::Quit),
            "Ctrl+C quits from inside a modal too"
        );
    }

    /// Tab/Shift+Tab/Enter decode to focus moves and focused-control
    /// activation; Enter's effect is whatever the FOCUSED control
    /// activates (§10).
    #[test]
    fn tab_and_enter_decode_to_the_focus_vocabulary() {
        let mut model = TuiModel::new("song.flac");
        model.set_class(ResponsiveClass::Normal);
        assert_eq!(
            decode_key(key(KeyCode::Tab), &model),
            Some(TuiAction::MoveFocus(FocusMove::Next))
        );
        assert_eq!(
            decode_key(key(KeyCode::BackTab), &model),
            Some(TuiAction::MoveFocus(FocusMove::Previous))
        );
        assert_eq!(
            decode_key(key(KeyCode::Enter), &model),
            Some(TuiAction::ActivateFocused)
        );
        // With nothing focused, activation resolves to nothing.
        assert_eq!(model.activation(), None);

        model.focus = Some(FocusId::RouteTab(TuiRoute::Playlist));
        assert_eq!(
            model.activation(),
            Some(TuiAction::Navigate(TuiRoute::Playlist))
        );
        model.focus = Some(FocusId::Transport(TransportButton::PlayPause));
        assert_eq!(model.activation(), Some(TuiAction::PlayPause));
        model.focus = Some(FocusId::Playlist);
        assert_eq!(model.activation(), Some(TuiAction::PlaylistPlaySelected));
    }

    /// The contextual arrows (§10): the focused list owns Up/Down; with
    /// any other focus they are noise, and they never also seek.
    #[test]
    fn up_down_are_contextual_to_the_focused_list() {
        let mut model = TuiModel::new("song.flac");
        model.set_class(ResponsiveClass::Normal);
        model.focus = Some(FocusId::Transport(TransportButton::PlayPause));
        assert_eq!(decode_key(key(KeyCode::Up), &model), None, "not the list");
        assert_eq!(decode_key(key(KeyCode::Down), &model), None);
        model.focus = Some(FocusId::Playlist);
        assert_eq!(
            decode_key(key(KeyCode::Up), &model),
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Previous))
        );
        assert_eq!(
            decode_key(key(KeyCode::Down), &model),
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Next))
        );
        // Left/Right keep the seek meaning next to a focused list — the
        // two axes never share a key.
        assert_eq!(
            decode_key(key(KeyCode::Left), &model),
            Some(TuiAction::SeekRelative(-5))
        );
        assert_eq!(
            decode_key(key(KeyCode::Right), &model),
            Some(TuiAction::SeekRelative(5))
        );
    }

    /// The seek keys decode to the same signed-relative action at the
    /// frozen 5 s and 30 s steps; release events never act.
    #[test]
    fn arrows_map_to_the_small_and_large_seek_actions() {
        let model = TuiModel::new("song.flac");
        for (key_code, small, large) in [
            (KeyCode::Left, -SEEK_STEP_SECS, -LARGE_SEEK_STEP_SECS),
            (KeyCode::Right, SEEK_STEP_SECS, LARGE_SEEK_STEP_SECS),
        ] {
            assert_eq!(
                decode_key(KeyEvent::new(key_code, KeyModifiers::NONE), &model),
                Some(TuiAction::SeekRelative(small))
            );
            assert_eq!(
                decode_key(KeyEvent::new(key_code, KeyModifiers::SHIFT), &model),
                Some(TuiAction::SeekRelative(large)),
                "the shift-keyed arrow is the LARGE step"
            );
            assert_eq!(
                decode_key(
                    KeyEvent::new_with_kind(key_code, KeyModifiers::NONE, KeyEventKind::Release),
                    &model
                ),
                None,
                "release events never act"
            );
            for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
                assert_eq!(
                    decode_key(KeyEvent::new(key_code, modifiers), &model),
                    None,
                    "chorded arrows stay noise"
                );
            }
        }
    }

    #[test]
    fn any_other_key_is_presentation_noise() {
        let model = TuiModel::new("song.flac");
        for event in [
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            // Chords stay noise (except Ctrl+C) so e.g. Ctrl+S/Ctrl+Q
            // never act by accident.
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::ALT),
            KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
        ] {
            assert_eq!(decode_key(event, &model), None, "{event:?} must be ignored");
        }
        // Key-release events (Windows terminals emit them) never act.
        assert_eq!(
            decode_key(
                KeyEvent::new_with_kind(
                    KeyCode::Char('s'),
                    KeyModifiers::NONE,
                    KeyEventKind::Release
                ),
                &model
            ),
            None
        );
    }

    /// Space stays the pause/resume accelerator; Open/GoTo/Help decode
    /// to the modal actions.
    #[test]
    fn the_application_keys_map_to_their_actions() {
        let model = TuiModel::new("song.flac");
        assert_eq!(
            decode_key(key(KeyCode::Char(' ')), &model),
            Some(TuiAction::PlayPause)
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('o')), &model),
            Some(TuiAction::OpenModal(ModalKind::Open))
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('G')), &model),
            Some(TuiAction::OpenModal(ModalKind::GoTo))
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('?')), &model),
            Some(TuiAction::OpenModal(ModalKind::Help))
        );
        for (key_char, action) in [
            ('n', TuiAction::Next),
            ('p', TuiAction::Previous),
            ('r', TuiAction::ToggleOrder),
            ('l', TuiAction::CycleRepeat),
        ] {
            assert_eq!(
                decode_key(key(KeyCode::Char(key_char)), &model),
                Some(action)
            );
        }
        assert_eq!(
            decode_key(key(KeyCode::Char('+')), &model),
            Some(TuiAction::VolumeUp)
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('=')), &model),
            Some(TuiAction::VolumeUp)
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('-')), &model),
            Some(TuiAction::VolumeDown)
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('_')), &model),
            Some(TuiAction::VolumeDown)
        );
    }

    /// Inside a text modal every plain character is a literal character
    /// (the Q-drive lesson, Issue #166 §29): `Q:\Music` stays typeable,
    /// Enter confirms, Esc cancels, Backspace edits — and no other key
    /// acts.
    #[test]
    fn a_text_modal_captures_its_editing_keys() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::Open);
        for c in r"Q:\Music".chars() {
            assert_eq!(
                decode_key(key(KeyCode::Char(c)), &model),
                Some(TuiAction::ModalInput(ModalInput::Char(c))),
                "{c:?} must be typed literally, not acted on"
            );
        }
        assert_eq!(
            decode_key(key(KeyCode::Enter), &model),
            Some(TuiAction::ModalInput(ModalInput::Confirm))
        );
        assert_eq!(
            decode_key(key(KeyCode::Esc), &model),
            Some(TuiAction::ModalInput(ModalInput::Cancel))
        );
        assert_eq!(
            decode_key(key(KeyCode::Backspace), &model),
            Some(TuiAction::ModalInput(ModalInput::Backspace))
        );
        // Non-plain characters stay noise.
        assert_eq!(
            decode_key(
                KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
                &model
            ),
            None
        );
        // The GoTo line gets the same capture.
        model.open_modal(ModalKind::GoTo);
        assert_eq!(
            decode_key(key(KeyCode::Char('q')), &model),
            Some(TuiAction::ModalInput(ModalInput::Char('q')))
        );
    }

    /// The help overlay owns the keyboard: `?`/Esc close it, Q quits,
    /// everything else is noise — no playback key can fire behind it.
    #[test]
    fn the_help_modal_lets_only_its_close_keys_through() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::Help);
        for code in [
            KeyCode::Char(' '),
            KeyCode::Char('s'),
            KeyCode::Char('n'),
            KeyCode::Char('r'),
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Left,
        ] {
            assert_eq!(
                decode_key(key(code), &model),
                None,
                "{code:?} must not act behind the help overlay"
            );
        }
        assert_eq!(
            decode_key(key(KeyCode::Char('?')), &model),
            Some(TuiAction::ModalInput(ModalInput::Cancel))
        );
        assert_eq!(
            decode_key(key(KeyCode::Esc), &model),
            Some(TuiAction::ModalInput(ModalInput::Cancel))
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('q')), &model),
            Some(TuiAction::Quit)
        );
    }

    /// The confirmation modal keeps only its own keys (G2): Esc
    /// cancels, Enter activates the focused button, Tab moves focus,
    /// and no other key — not even text — reaches it or the
    /// background. A destructive dialog answers to nothing else.
    #[test]
    fn the_confirm_modal_lets_only_its_keys_through() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::ConfirmClear);
        assert_eq!(
            decode_key(key(KeyCode::Esc), &model),
            Some(TuiAction::ModalInput(ModalInput::Cancel))
        );
        assert_eq!(
            decode_key(key(KeyCode::Enter), &model),
            Some(TuiAction::ActivateFocused)
        );
        assert_eq!(
            decode_key(key(KeyCode::Tab), &model),
            Some(TuiAction::MoveFocus(FocusMove::Next))
        );
        assert_eq!(
            decode_key(key(KeyCode::Char('y')), &model),
            None,
            "no text reaches the confirmation"
        );
    }

    // ------------------------------------------------------------------
    // Modal lifecycle tests (the old Open/GoTo/Help behaviors migrated).
    // ------------------------------------------------------------------

    /// §25 (T0 focus rule, G1 re-review): a focused control consumes
    /// Space exactly like Enter; the global pause/play accelerator
    /// acts only when nothing is focused.
    #[test]
    fn space_activates_the_focused_control_before_the_global_accelerator() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        model.set_focus(Some(FocusId::Transport(TransportButton::Stop)));
        assert_eq!(
            decode_key(key(KeyCode::Char(' ')), &model),
            Some(TuiAction::ActivateFocused),
            "the focused control consumes Space"
        );

        model.set_focus(None);
        assert_eq!(
            decode_key(key(KeyCode::Char(' ')), &model),
            Some(TuiAction::PlayPause),
            "with no focus, Space is the global accelerator"
        );
    }
}
