//! The mouse decoder: the frozen armed-click rule (Left Down identifies,
//! focuses and arms; a matching Left Up activates once), the wheel
//! policy, and the seek-bar geometry. Decoding mutates exactly the
//! presentation state a physical event owns — never a product seam.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use super::actions::{PlaylistCursor, TuiAction};
use super::hit::{ArmedClick, HitTarget};
use super::modal::ModalInput;
use super::state::TuiModel;

/// Decode one mouse event into AT MOST ONE [`TuiAction`] (§16–§25).
///
/// The frozen armed-click rule: Left Down identifies the target, focuses
/// it and arms it; Left Up activates ONLY the same valid target. Drag,
/// a moved pointer, stale geometry, a route/modal change or a resize
/// all cancel; an unmatched Up is no action. Right/middle clicks and
/// double clicks carry no product meaning (§19/§20); plain movement is
/// inert (§18); the wheel scrolls only where a control already has
/// clear meaning (§22). While a modal is open the background is inert
/// (§25).
///
/// Decoding mutates exactly the presentation state a physical event
/// owns — the focus and the armed click — and never a product seam.
pub fn decode_mouse(mouse: MouseEvent, model: &mut TuiModel) -> Option<TuiAction> {
    // §24/§25: while a modal is open only the modal's own controls
    // answer; the background (inside or outside the popup) dispatches
    // nothing.
    if model.modal().is_some() {
        return decode_modal_mouse(mouse, model);
    }
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => match model.hit_test(mouse.column, mouse.row) {
            Some(target) => {
                // A seek-bar hit carries no keyboard focus (the bar is
                // a mouse affordance over an already keyboard-complete
                // command); every other control focuses as before.
                if let Some(focus) = TuiModel::focus_of_target(target) {
                    model.focus = Some(focus);
                }
                // Only a control arms; the pane area focuses the list
                // and nothing else, and the seek bar arms for its
                // geometry-resolved action below.
                model.armed = if target == HitTarget::SeekBar {
                    Some(ArmedClick {
                        target,
                        column: mouse.column,
                        row: mouse.row,
                    })
                } else {
                    model.action_of_target(target).map(|_| ArmedClick {
                        target,
                        column: mouse.column,
                        row: mouse.row,
                    })
                };
                None
            }
            None => {
                model.disarm();
                None
            }
        },
        MouseEventKind::Up(MouseButton::Left) => {
            // An unmatched Up is no action.
            let armed = model.armed.take()?;
            // Activate only when the release is the SAME interaction
            // the press armed (§17): the SAME CELL (a press and a
            // release on different cells are different interactions,
            // G1 F15) AND the SAME semantic target still sitting under
            // the pointer. Stale geometry, a moved pointer, a
            // re-render/revision that moved or replaced the control —
            // all cancel.
            if armed.column == mouse.column
                && armed.row == mouse.row
                && model.hit_test(mouse.column, mouse.row) == Some(armed.target)
            {
                match armed.target {
                    HitTarget::SeekBar => {
                        Some(TuiAction::SeekPerMille(seek_bar_per_mille(model, &mouse)))
                    }
                    _ => model.action_of_target(armed.target),
                }
            } else {
                None
            }
        }
        // Drag cancels the armed click (§21); plain movement is inert.
        MouseEventKind::Drag(_) => {
            model.disarm();
            None
        }
        MouseEventKind::Moved => None,
        // §22: the wheel scrolls where a control already has a clear
        // meaning — the playlist list — and is inert everywhere else.
        // Either way the wheel cancels any armed press (§22: "wheel
        // cancels an arm and acts once on the control under the
        // pointer").
        MouseEventKind::ScrollUp => {
            model.disarm();
            wheel(model, &mouse, PlaylistCursor::Previous)
        }
        MouseEventKind::ScrollDown => {
            model.disarm();
            wheel(model, &mouse, PlaylistCursor::Next)
        }
        // Horizontal wheels have no meaning here (§22: no scroll physics).
        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => None,
        // Right/middle buttons carry no product meaning (§19).
        MouseEventKind::Down(_) | MouseEventKind::Up(_) => None,
    }
}

/// The per-mille a click on the seek bar's area requests (G1 §9): the
/// clicked cell's center as a fraction of the bar, clamped into
/// `1..=1000`. The bar region is this frame's own geometry — the same
/// region the hit test just answered — so the fraction and the hit can
/// never disagree.
fn seek_bar_per_mille(model: &TuiModel, mouse: &MouseEvent) -> u16 {
    let Some(region) = model
        .regions
        .iter()
        .find(|region| region.target == HitTarget::SeekBar)
    else {
        return 0;
    };
    let offset = u32::from(mouse.column.saturating_sub(region.area.x));
    let width = u32::from(region.area.width.max(1));
    (((offset * 2 + 1) * 500) / width).min(1000) as u16
}

/// Whether a hit target belongs to the active modal's own surface
/// (§25): everything else is background and stays inert.
fn modal_target(target: HitTarget) -> bool {
    matches!(
        target,
        HitTarget::PickerRow(_) | HitTarget::ModalButton(_) | HitTarget::ModalField
    )
}

/// The armed-click discipline (§16–§22) applied to the Open picker's
/// own controls: rows and buttons arm and activate like every other
/// control, the wheel steps the listing where it has rows, and a
/// click anywhere else — including the background visible around the
/// popup — disarms and dispatches nothing.
fn decode_modal_mouse(mouse: MouseEvent, model: &mut TuiModel) -> Option<TuiAction> {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => match model.hit_test(mouse.column, mouse.row) {
            Some(target) if modal_target(target) => {
                model.focus = TuiModel::focus_of_target(target);
                model.armed = model.action_of_target(target).map(|_| ArmedClick {
                    target,
                    column: mouse.column,
                    row: mouse.row,
                });
                None
            }
            _ => {
                model.disarm();
                None
            }
        },
        MouseEventKind::Up(MouseButton::Left) => {
            // The same one-rule check as the background: same cell,
            // same target still under the pointer, still a modal
            // control (§17; G1 F15).
            let armed = model.armed.take()?;
            if modal_target(armed.target)
                && armed.column == mouse.column
                && armed.row == mouse.row
                && model.hit_test(mouse.column, mouse.row) == Some(armed.target)
            {
                model.action_of_target(armed.target)
            } else {
                None
            }
        }
        MouseEventKind::Drag(_) => {
            model.disarm();
            None
        }
        // The wheel cancels any armed press even inside the modal
        // (§22), and steps the listing where it has rows.
        MouseEventKind::ScrollUp => {
            model.disarm();
            modal_wheel(model, &mouse, PlaylistCursor::Previous)
        }
        MouseEventKind::ScrollDown => {
            model.disarm();
            modal_wheel(model, &mouse, PlaylistCursor::Next)
        }
        _ => None,
    }
}

/// The wheel action over one cell while the picker is open: a listing
/// scroll over the listing rows, otherwise nothing.
fn modal_wheel(model: &TuiModel, mouse: &MouseEvent, cursor: PlaylistCursor) -> Option<TuiAction> {
    match model.hit_test(mouse.column, mouse.row)? {
        HitTarget::PickerRow(_) => Some(TuiAction::ModalInput(ModalInput::ListMove(cursor))),
        _ => None,
    }
}

/// The wheel action over one cell: a list scroll when the cell belongs
/// to the playlist list, otherwise nothing.
fn wheel(model: &TuiModel, mouse: &MouseEvent, cursor: PlaylistCursor) -> Option<TuiAction> {
    match model.hit_test(mouse.column, mouse.row)? {
        HitTarget::PlaylistRow(_) | HitTarget::PlaylistPane => {
            Some(TuiAction::PlaylistSelect(cursor))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use crate::tui::model::testutil::{model_with_regions, mouse, pending};
    use crate::tui::model::*;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    use qianqian_playback::PlaybackSessionObservation;
    use std::time::Duration;

    /// Left Down focuses and arms; Left Up on the same valid target
    /// activates exactly one action; the Down itself dispatches none.
    #[test]
    fn the_armed_click_rule_down_focuses_and_up_activates() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        // Click the Playlist tab.
        let (column, row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::Playlist));
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Down(MouseButton::Left), column, row),
                &mut model
            ),
            None,
            "the Down dispatches nothing"
        );
        assert_eq!(model.focus(), Some(FocusId::RouteTab(TuiRoute::Playlist)));
        assert_eq!(
            model.armed().map(|armed| armed.target),
            Some(HitTarget::RouteTab(TuiRoute::Playlist))
        );
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                &mut model
            ),
            Some(TuiAction::Navigate(TuiRoute::Playlist))
        );
        assert_eq!(model.armed(), None, "the Up consumed the armed target");
    }

    /// A mouse Up without a matching Down dispatches nothing (§17).
    #[test]
    fn an_unmatched_up_is_no_action() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let (column, row) = region_cell(&model, &HitTarget::Transport(TransportButton::PlayPause));
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                &mut model
            ),
            None
        );
    }

    /// Down on one target, Up on another: no action (§17).
    #[test]
    fn down_on_a_up_on_b_is_no_action() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let (down, down_row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::NowPlaying));
        let (up, up_row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::Playlist));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), down, down_row),
            &mut model,
        );
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), up, up_row),
                &mut model
            ),
            None,
            "the pointer changed target between Down and Up"
        );
        assert_eq!(
            model.armed(),
            None,
            "the Up consumed the stale armed target"
        );
    }

    /// Down, then a resize invalidates the frame, then Up at the same
    /// cell: no action (§17/§29). The regions are gone and the armed
    /// target with them, so the Up matches nothing.
    #[test]
    fn down_resize_up_is_no_action() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let (column, row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::NowPlaying));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut model,
        );
        model.invalidate_frame();
        assert_eq!(model.armed(), None);
        assert!(
            model.hit_test(column, row).is_none(),
            "the resize cleared the regions"
        );
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                &mut model
            ),
            None
        );
    }

    /// Down, then a route change, then Up at the same cell: no action
    /// (§17). A route change can also move the control that sits at the
    /// cell — either way nothing dispatches.
    #[test]
    fn down_route_change_up_is_no_action() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let (column, row) = region_cell(&model, &HitTarget::Transport(TransportButton::PlayPause));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut model,
        );
        assert_eq!(
            model.armed().map(|armed| armed.target),
            Some(HitTarget::Transport(TransportButton::PlayPause))
        );
        model.set_route(TuiRoute::Playlist);
        assert_eq!(model.armed(), None, "the route change disarmed the click");
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                &mut model
            ),
            None
        );
    }

    /// G1 F06, the stale-arm lifetime (§17): a press armed on one
    /// episode cannot activate into the next one. The episode is
    /// REPLACED between Down and Up — and the same control reappears at
    /// the SAME cell — but the Up is inert: the arm died with the
    /// replaced episode, not with the geometry.
    #[test]
    fn an_episode_replacement_disarms_a_press_even_at_the_same_cell() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let (column, row) = region_cell(&model, &HitTarget::Transport(TransportButton::PlayPause));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut model,
        );
        assert!(model.armed().is_some(), "the press arms");

        // The episode is replaced by a redraw that publishes THE SAME
        // regions — only the committed source changed.
        model.set_episode(Some("other-song.flac".to_owned()));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|frame| crate::tui::view::draw(frame, &mut model))
            .expect("draw");
        assert_eq!(
            model.hit_test(column, row),
            Some(HitTarget::Transport(TransportButton::PlayPause)),
            "the same control IS rendered at the same cell for the new episode"
        );
        assert_eq!(model.armed(), None, "the replacement disarmed the press");
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                &mut model
            ),
            None,
            "the old interaction cannot cross into the new episode"
        );
    }

    /// §22 (G1 re-review): a wheel step cancels an armed press and acts
    /// once on the control under the pointer — a press-and-hold, a
    /// wheel, then a release on the same cell must NOT activate the
    /// armed target.
    #[test]
    fn a_wheel_step_cancels_an_armed_press() {
        let mut model = model_with_regions(100, 30, TuiRoute::Playlist);
        let (column, row) = region_cell(&model, &HitTarget::PlaylistRow(2));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut model,
        );
        assert!(model.armed().is_some(), "the row press arms");

        // The wheel acts once (the selection moves) and kills the arm.
        assert_eq!(
            decode_mouse(mouse(MouseEventKind::ScrollDown, column, row), &mut model),
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Next)),
            "the wheel acts on the control under the pointer"
        );
        assert_eq!(model.armed(), None, "the wheel cancelled the arm");
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                &mut model
            ),
            None,
            "the release after a wheel is inert"
        );

        // The same rule inside the modal.
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        model.open_modal(ModalKind::Open);
        model.set_open_listing(
            std::path::PathBuf::from("/media"),
            Ok(vec![crate::input::DirectoryEntry {
                name: "b.flac".to_owned(),
                is_dir: false,
                path: std::path::PathBuf::from("/media/b.flac"),
            }]),
        );
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|frame| crate::tui::view::draw(frame, &mut model))
            .expect("draw");
        let (column, row) = region_cell(&model, &HitTarget::PickerRow(0));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut model,
        );
        assert!(model.armed().is_some(), "the picker row press arms");
        assert_eq!(
            decode_mouse(mouse(MouseEventKind::ScrollDown, column, row), &mut model),
            Some(TuiAction::ModalInput(ModalInput::ListMove(
                PlaylistCursor::Next
            ))),
        );
        assert_eq!(model.armed(), None, "the modal wheel cancelled the arm");
    }

    /// §22 (G1 re-review): "the same applies after content changes
    /// that move targets" — a status block that changes SHAPE (its
    /// bounded line count) moves every target below it and cancels the
    /// arm; a same-shape feedback keeps it.
    #[test]
    fn a_status_shape_change_disarms_but_a_same_shape_feedback_keeps_the_arm() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let (column, row) = region_cell(&model, &HitTarget::Transport(TransportButton::Stop));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut model,
        );
        assert!(model.armed().is_some(), "the press arms");

        // A second feedback line grows the status block: the targets
        // below move.
        model.set_status(Some("volume 75/100 (desired)\nsecond line".to_owned()));
        assert_eq!(model.armed(), None, "the shape change cancelled the arm");

        // Re-arm, then a same-shape feedback: the targets stay put.
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|frame| crate::tui::view::draw(frame, &mut model))
            .expect("draw");
        let (column, row) = region_cell(&model, &HitTarget::Transport(TransportButton::Stop));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut model,
        );
        assert!(model.armed().is_some());
        model.set_status(Some(
            "opened /media (2 candidates)\nanother line".to_owned(),
        ));
        assert!(
            model.armed().is_some(),
            "a same-shape feedback keeps the arm"
        );
    }

    /// G1 F06, the list-revision half of the arm lifetime: a press
    /// armed on a playlist row cannot survive a list edit, even when a
    /// row still sits at the same coordinates (§17: a list revision
    /// change cancels the arm).
    #[test]
    fn a_playlist_revision_change_disarms_a_row_press() {
        let mut model = model_with_regions(100, 30, TuiRoute::Playlist);
        let (column, row) = region_cell(&model, &HitTarget::PlaylistRow(2));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut model,
        );
        assert!(model.armed().is_some(), "the row press arms");

        // A list edit moves the revision; a redraw publishes fresh
        // regions where SOME row still occupies the cell.
        model.set_playlist(2, || {
            (0..40)
                .map(|n| PlaylistRow {
                    label: format!("new-track-{n:02}.flac"),
                    playing: false,
                    selected: false,
                })
                .collect()
        });
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|frame| crate::tui::view::draw(frame, &mut model))
            .expect("draw");
        assert_eq!(model.armed(), None, "the revision change disarmed");
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                &mut model
            ),
            None,
            "the old row interaction cannot fire on the edited list"
        );
    }

    /// G1 F15: the arm carries its origin CELL. A Down and an Up on
    /// DIFFERENT cells of the same wide control are different
    /// interactions — the Up is inert, even though both cells hit-test
    /// to the same target (§17: matching Up on the same target/cell).
    #[test]
    fn a_release_on_a_different_cell_of_the_same_control_is_inert() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let region = model
            .regions()
            .iter()
            .find(|region| region.target == HitTarget::Transport(TransportButton::PlayPause))
            .expect("the transport region");
        let down = (region.area.x + 1, region.area.y);
        let up = (region.area.x + region.area.width - 2, region.area.y);
        assert_ne!(down, up, "the two cells differ within the one control");
        assert_eq!(
            model.hit_test(up.0, up.1),
            Some(HitTarget::Transport(TransportButton::PlayPause)),
            "the release cell still hit-tests to the SAME target"
        );
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), down.0, down.1),
            &mut model,
        );
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), up.0, up.1),
                &mut model
            ),
            None,
            "same target, different cell: no activation"
        );
        assert_eq!(model.armed(), None, "the release consumed the stale arm");
    }

    /// Drag cancels the armed click (§21); plain movement is inert (§18).
    #[test]
    fn drag_cances_and_movement_is_inert() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let (column, row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::Audio));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut model,
        );
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Drag(MouseButton::Left), column, row + 1),
                &mut model
            ),
            None
        );
        assert_eq!(model.armed(), None, "the drag cancelled the armed click");
        assert_eq!(
            decode_mouse(mouse(MouseEventKind::Moved, 5, 5), &mut model),
            None,
            "plain movement dispatches nothing (§18)"
        );
    }

    /// Right/middle clicks carry no product meaning (§19) and never
    /// arm; they do not disturb an existing armed click either.
    #[test]
    fn right_click_is_ignored() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        let (column, row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::Playlist));
        model.set_focus(None);
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Down(MouseButton::Right), column, row),
                &mut model
            ),
            None
        );
        assert_eq!(model.armed(), None);
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Down(MouseButton::Middle), column, row),
                &mut model
            ),
            None
        );
        assert_eq!(model.focus(), None, "a non-left Down does not even focus");
    }

    /// Double clicks have no special product meaning (§20): the second
    /// Down/Up pair dispatches exactly what a first pair would — one
    /// row select, never a play.
    #[test]
    fn a_double_click_stays_two_single_clicks() {
        let mut model = model_with_regions(100, 30, TuiRoute::Playlist);
        let (column, row) = region_cell(&model, &HitTarget::PlaylistRow(1));
        for _ in 0..2 {
            decode_mouse(
                mouse(MouseEventKind::Down(MouseButton::Left), column, row),
                &mut model,
            );
            assert_eq!(
                decode_mouse(
                    mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                    &mut model
                ),
                Some(TuiAction::PlaylistSelect(PlaylistCursor::Row(1)))
            );
        }
    }

    /// The wheel scrolls only the playlist list (§22): over a row or
    /// the pane it moves the selection, everywhere else it is inert.
    #[test]
    fn the_wheel_scrolls_only_the_list() {
        let mut model = model_with_regions(100, 30, TuiRoute::Playlist);
        let (column, row) = region_cell(&model, &HitTarget::PlaylistRow(2));
        assert_eq!(
            decode_mouse(mouse(MouseEventKind::ScrollDown, column, row), &mut model),
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Next))
        );
        assert_eq!(
            decode_mouse(mouse(MouseEventKind::ScrollUp, column, row), &mut model),
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Previous))
        );
        // Horizontal wheels have no meaning.
        assert_eq!(
            decode_mouse(mouse(MouseEventKind::ScrollLeft, column, row), &mut model),
            None
        );

        // Inert over a tab on the same frame.
        let (tab, tab_row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::Audio));
        assert_eq!(
            decode_mouse(mouse(MouseEventKind::ScrollDown, tab, tab_row), &mut model),
            None
        );
    }

    /// While a modal is open the background is inert (§25): a click on
    /// a background control neither arms nor dispatches — including the
    /// very cell that would otherwise activate.
    #[test]
    fn a_modal_open_ignores_background_clicks() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        // Locate the background cells BEFORE the modal opens: opening
        // invalidates the frame's regions (§24), which is exactly the
        // behavior under test.
        let (column, row) = region_cell(&model, &HitTarget::Transport(TransportButton::PlayPause));
        let (tab, tab_row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::Playlist));
        model.open_modal(ModalKind::Help);
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Down(MouseButton::Left), column, row),
                &mut model
            ),
            None
        );
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                &mut model
            ),
            None
        );
        assert_eq!(model.armed(), None, "no arming behind a modal");
        assert_eq!(
            model.focus(),
            Some(FocusId::ModalField),
            "the click did not steal focus from the modal"
        );
        // Wheel behind the modal is inert too — the tab cell would be
        // background either way.
        assert_eq!(
            decode_mouse(mouse(MouseEventKind::ScrollDown, tab, tab_row), &mut model),
            None
        );
    }

    /// The mouse activation converges on the same action as the
    /// keyboard activation for the same control (§30).
    #[test]
    fn keyboard_and_mouse_activation_converge_on_one_action() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);

        // Route tab.
        let (tab, tab_row) = region_cell(&model, &HitTarget::RouteTab(TuiRoute::Playlist));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), tab, tab_row),
            &mut model,
        );
        let mouse_action = decode_mouse(
            mouse(MouseEventKind::Up(MouseButton::Left), tab, tab_row),
            &mut model,
        );
        model.focus = Some(FocusId::RouteTab(TuiRoute::Playlist));
        let keyboard_action = model.activation();
        assert_eq!(mouse_action, keyboard_action);

        // Transport button.
        let (button, button_row) =
            region_cell(&model, &HitTarget::Transport(TransportButton::Stop));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), button, button_row),
            &mut model,
        );
        let mouse_action = decode_mouse(
            mouse(MouseEventKind::Up(MouseButton::Left), button, button_row),
            &mut model,
        );
        model.focus = Some(FocusId::Transport(TransportButton::Stop));
        assert_eq!(mouse_action, model.activation());
    }

    /// The first cell (left column, middle row) of the region whose
    /// target matches, for mouse-decoding tests.
    fn region_cell(model: &TuiModel, target: &HitTarget) -> (u16, u16) {
        let region = model
            .regions()
            .iter()
            .find(|region| &region.target == target)
            .unwrap_or_else(|| panic!("no region for {target:?} in the published frame"));
        (region.area.x + 1, region.area.y + region.area.height / 2)
    }

    // ------------------------------------------------------------------
    // Responsive class tests (§27/§28 mechanics only).
    // ------------------------------------------------------------------

    /// A click on the seek bar decodes to the clicked cell's per-mille
    /// of the bar (G1 §9): Down arms, Up activates at the SAME cell,
    /// and the fraction comes from the frame's own published geometry.
    #[test]
    fn a_click_on_the_seek_bar_decodes_to_a_per_mille_of_the_bar() {
        let mut model = TuiModel::new("song.flac");
        model.update(PlaybackSessionObservation {
            source_format: Some(qianqian_audio_api::ports::PcmFormat {
                sample_rate: 44_100,
                channels: 2,
                channel_mask: 0x3,
            }),
            position: Some(44_100),
            source_duration: Some(Duration::from_secs(200)),
            ..pending()
        });
        model.set_class(ResponsiveClass::Wide);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|frame| crate::tui::view::draw(frame, &mut model))
            .expect("draw");
        let region = model
            .regions()
            .iter()
            .find(|region| region.target == HitTarget::SeekBar)
            .expect("the bar publishes a region with duration evidence");
        let column = region.area.x + region.area.width / 2;
        let row = region.area.y;

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
        assert_eq!(decode_mouse(down, &mut model), None, "Down only arms");
        assert_eq!(
            decode_mouse(up, &mut model),
            Some(TuiAction::SeekPerMille(520)),
            "the click's per-mille of the DRAWN glyph (24 cells, click at center)"
        );
    }
}
