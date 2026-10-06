//! The ONE dispatch boundary and the command performers: every
//! [`TuiAction`](crate::tui::model::TuiAction) comes through
//! [`dispatch`], and the same action leads to the same product
//! operation regardless of its input source.

use std::time::Duration;

use crate::player::{EpisodeStart, OpenOutcome, ReferencePlayerApp};

use super::drain_busy_interval_input;
use super::picker::{handle_modal_input, navigate_picker_to};
use crate::tui::model::{
    ModalKind, PlaylistCursor, Step, TuiAction, TuiModel, seek_fraction_target, seek_target,
};

/// The operation feedback for one automatic EOF transition. Application
/// composition feedback under the operation's own name — never a
/// playback semantic, and never a claim about the sound.
pub(super) fn eof_feedback<S: EpisodeStart>(
    outcome: &OpenOutcome,
    player: &ReferencePlayerApp<S>,
) -> String {
    match outcome {
        OpenOutcome::Opened => match player.active_source() {
            Some(source) => format!("auto-next: opened {}", source.display()),
            None => "auto-next: opened".to_owned(),
        },
        OpenOutcome::Refused { diagnostic } => format!("auto-next refused: {diagnostic}"),
        OpenOutcome::ActivationFailedClean { diagnostic } => {
            format!("auto-next failed (clean): {diagnostic}")
        }
        OpenOutcome::FailStop { diagnostic } => format!("FAIL-STOP: {diagnostic}"),
    }
}

/// The ONE dispatch boundary (§8): every [`TuiAction`] — decoded from a
/// key, a mouse click, or a focused control's activation — comes here,
/// and the same action leads to the same product operation regardless
/// of its source. One call performs at most one product operation
/// (§31): the `ActivateFocused` resolution happens inside this same
/// call, and a resolved activation is never itself an activation.
pub(super) fn dispatch<S: EpisodeStart>(
    action: TuiAction,
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
) -> Step {
    match action {
        // Quit is loop control, never an episode command: it exits even
        // with no episode committed — the idle shell must stay
        // quittable.
        TuiAction::Quit => Step::Exit,
        // Route changes are presentation-only (§5).
        TuiAction::Navigate(route) => {
            model.set_route(route);
            Step::Continue
        }
        TuiAction::MoveFocus(direction) => {
            model.move_focus(direction);
            Step::Continue
        }
        // Enter on the focused control: resolve to that control's own
        // action and run it — inside this one dispatch, so one physical
        // event still performs at most one product operation (§31).
        TuiAction::ActivateFocused => match model.activation() {
            Some(resolved) => dispatch(resolved, model, player),
            None => Step::Continue,
        },
        TuiAction::OpenModal(kind) => {
            model.open_modal(kind);
            // The picker starts browsing where the shell runs (G1 §8):
            // the runtime performs the one directory read and hands the
            // listing to the model; an unreadable start keeps the
            // picker's honest diagnostic with the field typeable.
            if matches!(
                kind,
                ModalKind::Open | ModalKind::AddFile | ModalKind::AddFolder
            ) {
                let start =
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                navigate_picker_to(model, &start);
            }
            Step::Continue
        }
        TuiAction::ModalInput(input) => {
            handle_modal_input(input, model, player);
            Step::Continue
        }
        // The central transport control (T0 transport freeze, G1 F03).
        // The frozen truth table, executed through EXISTING product
        // authority only — no second start path, no episode mutation:
        //
        //   unsettled current episode -> pause / resume, by the FRESH
        //                                pause-command state
        //   terminal current entry    -> replay through the existing
        //                                replacement (`play_current`)
        //   no episode, a list        -> play the selected entry
        //                                (`play_current`'s own rule)
        //   neither                   -> honest inert feedback
        TuiAction::PlayPause => {
            match player.active_handle() {
                Some(handle) => {
                    // The choice comes from a FRESH authoritative
                    // observation — the shell never keeps a local
                    // `paused` or `terminal` bool.
                    if handle.observe().terminal_outcome.is_some() {
                        perform_play_current(model, player);
                    } else if handle.observe().pause_requested {
                        handle.request_resume();
                    } else {
                        handle.request_pause();
                    }
                }
                // No live episode: `play_current` IS the App's
                // Home-Play seam — with no episode it plays the
                // selected entry, and with neither it reports why not.
                None => perform_play_current(model, player),
            }
            Step::Continue
        }
        TuiAction::Stop => {
            if let Some(handle) = player.active_handle() {
                handle.request_stop();
            }
            Step::Continue
        }
        TuiAction::SeekRelative(seconds) => {
            if let Some(handle) = player.active_handle() {
                // The SAME frozen seek command as before: the target is
                // derived from one fresh coherent observation, and an
                // episode whose position is unknown gets NO command at
                // all (no fabricated zero, no seek to the start).
                if let Some(target) = seek_target(
                    &handle.observe(),
                    Duration::from_secs(seconds.unsigned_abs()),
                    seconds > 0,
                ) {
                    handle.request_seek(target);
                }
            }
            Step::Continue
        }
        // The seek bar's click-to-position (G1 §9): the fraction
        // chooses WHERE on the timeline, the episode's FRESH published
        // duration chooses WHAT target exists. No duration evidence —
        // no command, and the bar published no region to click in the
        // first place.
        TuiAction::SeekPerMille(per_mille) => {
            if let Some(handle) = player.active_handle()
                && let Some(target) = seek_fraction_target(&handle.observe(), per_mille)
            {
                handle.request_seek(target);
                model.set_status(Some(format!(
                    "seek requested: {}",
                    crate::status::format_clock(target)
                )));
            }
            Step::Continue
        }
        TuiAction::Previous => {
            perform_navigation(model, player, Navigation::Previous);
            Step::Continue
        }
        TuiAction::Next => {
            perform_navigation(model, player, Navigation::Next);
            Step::Continue
        }
        // The playlist selection is presentation of the App's own
        // selection cursor: it moves the cursor and nothing else
        // (Issue #166 §18). The viewport reveals the selected row
        // (T0: keyboard navigation selects and reveals).
        TuiAction::PlaylistSelect(cursor) => {
            match cursor {
                PlaylistCursor::Next => player.select_next_track(),
                PlaylistCursor::Previous => player.select_previous_track(),
                PlaylistCursor::Row(position) => {
                    player.select_track(position);
                }
            }
            if let Some(position) = player.playlist_selected_position() {
                model.playlist_reveal(position);
            }
            Step::Continue
        }
        TuiAction::PlaylistPlaySelected => {
            perform_play_selected(model, player);
            Step::Continue
        }
        // The playlist toolbar (G2): Add File / Add Folder open the
        // shared picker in their restricted Add mode; the submission
        // and its feedback live in the picker's commit path.
        TuiAction::PlaylistAddFile => {
            open_picker(model, ModalKind::AddFile);
            Step::Continue
        }
        TuiAction::PlaylistAddFolder => {
            open_picker(model, ModalKind::AddFolder);
            Step::Continue
        }
        // Remove (G2): the CURRENT row needs the frozen stop-aware
        // confirmation first; a non-current row is a direct App list
        // edit. The App owns the retire-then-edit ordering either way.
        TuiAction::PlaylistRemove => {
            if player.selected_is_live_episode() {
                model.open_modal(ModalKind::ConfirmRemoveCurrent);
            } else {
                perform_remove_selected(model, player);
            }
            Step::Continue
        }
        // Clear (G2): the frozen stop-aware confirmation first while an
        // episode is live; without one it is a direct App list edit.
        TuiAction::PlaylistClear => {
            if player.active_handle().is_some() {
                model.open_modal(ModalKind::ConfirmClear);
            } else {
                perform_clear_playlist(model, player);
            }
            Step::Continue
        }
        // Audio route (G3): draft edits are presentation mutations on
        // the model; the explicit [Apply] is the one path that touches
        // the App's desired-DSP seams. Nothing here ever claims an
        // applied-DSP readback.
        TuiAction::DspToggleEnabled => {
            model.audio_toggle_enabled();
            Step::Continue
        }
        TuiAction::DspPreampStep(delta_db) => {
            model.audio_preamp_step(delta_db as f32);
            Step::Continue
        }
        TuiAction::DspEqBandStep(band, delta_db) => {
            model.audio_eq_band_step(band, delta_db as f32);
            Step::Continue
        }
        TuiAction::DspOpenPresets => {
            model.open_modal(ModalKind::Presets);
            Step::Continue
        }
        TuiAction::DspApply => {
            perform_dsp_apply(model, player);
            Step::Continue
        }
        TuiAction::DspCancel => {
            if model.audio_cancel_draft() {
                model.set_status(Some("draft discarded".to_owned()));
            }
            Step::Continue
        }
        TuiAction::ToggleOrder => {
            let order = player.toggle_order();
            model.set_order(order);
            model.set_status(Some(format!("Order: {}", order.label())));
            Step::Continue
        }
        TuiAction::CycleRepeat => {
            let repeat = player.cycle_repeat();
            model.set_repeat(repeat);
            model.set_status(Some(format!("Repeat: {}", repeat.label())));
            Step::Continue
        }
        TuiAction::VolumeUp => {
            let volume = player.change_volume(VOLUME_STEP);
            model.set_status(Some(format!("volume {volume}/100 (desired)")));
            Step::Continue
        }
        TuiAction::VolumeDown => {
            let volume = player.change_volume(-VOLUME_STEP);
            model.set_status(Some(format!("volume {volume}/100 (desired)")));
            Step::Continue
        }
    }
}

/// Open the shared picker in one mode (G2): a plain open, an Add File
/// or an Add Folder. The modal opens first, then the runtime lists the
/// start directory — the same one-read-then-hand-over flow the Open
/// entry uses.
fn open_picker(model: &mut TuiModel, kind: ModalKind) {
    model.open_modal(kind);
    let start = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    navigate_picker_to(model, &start);
}

/// The confirmed Remove of the SELECTED row through the App's own
/// remove seam (G2): the App retires the current episode FIRST when the
/// selection owns it, then mutates the list — the TUI never orders a
/// teardown itself, and a retirement failure leaves the list untouched
/// with the honest diagnostic.
pub(super) fn perform_remove_selected<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
) {
    let label = player.playlist_selected_position().and_then(|position| {
        player
            .playlist_rows()
            .nth(position)
            .map(|row| row.path.display().to_string())
    });
    match player.remove_selected() {
        Ok(true) => {
            model.set_status(Some(match label {
                Some(label) => format!("removed {label}"),
                None => "removed the selected row".to_owned(),
            }));
        }
        Ok(false) => model.set_status(Some("nothing selected".to_owned())),
        Err(refusal) => model.set_status(Some(format!("remove refused: {refusal}"))),
    }
    drain_busy_interval_input();
}

/// The confirmed Clear through the App's own clear seam (G2): the App
/// retires the current episode first (the T0 owner decision), then
/// clears; preferences survive, and a retirement failure leaves the
/// list untouched with the honest diagnostic.
pub(super) fn perform_clear_playlist<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
) {
    match player.clear_playlist() {
        Ok(()) => model.set_status(Some("playlist cleared".to_owned())),
        Err(refusal) => model.set_status(Some(format!("clear refused: {refusal}"))),
    }
    drain_busy_interval_input();
}

/// The confirmed Apply of the Audio route's draft (G3): commit the
/// draft's fields through the App's own desired-DSP seams, in the
/// stage order the configuration documents (enablement, preamp, EQ).
/// Each seam re-validates; the FIRST refusal stops the apply with the
/// honest diagnostic and keeps the draft dirty for correction. Either
/// way the draft's staleness witness re-syncs to the App's current
/// desired configuration — the base change was the shell's OWN apply,
/// never "elsewhere". The seams set the DESIRED state; the shell
/// claims no applied readback (the route's standing line says exactly
/// that).
pub(super) fn perform_dsp_apply<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
) {
    let config = match model.audio_draft() {
        Some(draft) => draft.config(),
        None => return,
    };
    let result = (|| -> Result<(), String> {
        if config.enabled != player.desired_processing().enabled {
            player.set_processing_enabled(config.enabled)?;
        }
        if config.gain != player.desired_processing().gain {
            player.set_preamp(config.gain)?;
        }
        if config.eq != player.desired_processing().eq
            && let Some(eq) = config.eq
        {
            player.set_eq_config(eq)?;
        }
        Ok(())
    })();
    model.audio_note_applied(player.desired_processing());
    match result {
        Ok(()) => model.set_status(Some(
            "desired DSP updated (applied state not reported)".to_owned(),
        )),
        Err(refusal) => model.set_status(Some(format!("apply refused: {refusal}"))),
    }
    drain_busy_interval_input();
}

/// Which navigation step was requested.
enum Navigation {
    Next,
    Previous,
}

impl Navigation {
    /// The direction in the App's traversal order.
    fn forward(&self) -> bool {
        matches!(self, Self::Next)
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Next => "next",
            Self::Previous => "previous",
        }
    }
}

/// The D14.9 volume step: one action, five points of the desired
/// stream factor. One product decision, one constant.
const VOLUME_STEP: i16 = 5;

/// Perform one MANUAL navigation through the player (Issue #166
/// §34/§35): the SAME Open replacement, with the committed cursor moving
/// only on commit. The inert ends (no wrap outside Repeat All) report
/// honestly; every other outcome is the Open outcome under the
/// operation's name.
fn perform_navigation<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
    navigation: Navigation,
) {
    let name = navigation.name();
    let outcome = if navigation.forward() {
        player.next_track()
    } else {
        player.previous_track()
    };
    if matches!(outcome, Some(OpenOutcome::Opened)) {
        model.note_episode_replacement();
    }
    let feedback = match outcome {
        None => format!("no {name} track"),
        Some(outcome) => match outcome {
            OpenOutcome::Opened => match player.active_source() {
                Some(source) => format!("{name}: opened {}", source.display()),
                None => format!("{name}: opened"),
            },
            OpenOutcome::Refused { diagnostic } => format!("{name} refused: {diagnostic}"),
            OpenOutcome::ActivationFailedClean { diagnostic } => {
                format!("{name} failed (clean): {diagnostic}")
            }
            OpenOutcome::FailStop { diagnostic } => format!("FAIL-STOP: {diagnostic}"),
        },
    };
    model.set_status(Some(feedback));
    drain_busy_interval_input();
}
/// Play the SELECTED playlist row through the same Open replacement
/// (Issue #166 §19). The selection is presentation state the user
/// already moved; this action is the only thing that turns it into
/// playback, and only on commit evidence. One inert rule (field
/// round 3): activation on the row that IS the unsettled live episode
/// is not a replay request — the frozen replacement would restart the
/// track from its head, so the shell refuses to re-invoke it and says
/// so instead (no probe, no teardown, nothing moves).
fn perform_play_selected<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
) {
    if player.selected_is_live_episode() {
        model.set_status(Some("the selected track is the live episode".to_owned()));
        return;
    }
    let feedback = match player.play_selected() {
        None => "nothing selected".to_owned(),
        Some(OpenOutcome::Opened) => {
            model.note_episode_replacement();
            match player.active_source() {
                Some(source) => format!("play: opened {}", source.display()),
                None => "play: opened".to_owned(),
            }
        }
        Some(OpenOutcome::Refused { diagnostic }) => format!("play refused: {diagnostic}"),
        Some(OpenOutcome::ActivationFailedClean { diagnostic }) => {
            format!("play failed (clean): {diagnostic}")
        }
        Some(OpenOutcome::FailStop { diagnostic }) => format!("FAIL-STOP: {diagnostic}"),
    };
    model.set_status(Some(feedback));
    drain_busy_interval_input();
}
/// The central transport control's Home-Play commands (T0 transport
/// freeze, G1 F03), for a terminal current entry or a no-episode
/// list: replay/play through the App's EXISTING `play_current`
/// replacement seam — never a local restart, never a mutated episode,
/// never a second start path. The feedback is the operation's own
/// composition feedback, and the inert cases say why.
fn perform_play_current<S: EpisodeStart>(model: &mut TuiModel, player: &mut ReferencePlayerApp<S>) {
    let feedback = match player.play_current() {
        // `play_current` is inert in exactly two ways: a terminal
        // episode with no replayable entry, or no episode and no
        // selection (the frozen "Play is disabled with its reason").
        None => {
            if player.active_handle().is_some() {
                "play: the current episode has no replayable entry".to_owned()
            } else {
                "No music loaded — open something with the Open button".to_owned()
            }
        }
        Some(OpenOutcome::Opened) => {
            model.note_episode_replacement();
            match player.active_source() {
                Some(source) => format!("play: opened {}", source.display()),
                None => "play: opened".to_owned(),
            }
        }
        Some(OpenOutcome::Refused { diagnostic }) => format!("play refused: {diagnostic}"),
        Some(OpenOutcome::ActivationFailedClean { diagnostic }) => {
            format!("play failed (clean): {diagnostic}")
        }
        Some(OpenOutcome::FailStop { diagnostic }) => format!("FAIL-STOP: {diagnostic}"),
    };
    model.set_status(Some(feedback));
    drain_busy_interval_input();
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use std::path::PathBuf;

    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };

    use super::super::refresh;
    use super::super::testutil::*;
    use super::dispatch;
    use crate::player::ReferencePlayerApp;
    use crate::player::tests::FakeEpisodeSource;
    use crate::playlist::{PlaybackOrder, RepeatMode};
    use crate::tui::model::*;
    use qianqian_playback::EqPreset;

    /// The idle no-episode state stays truthful — nothing to command,
    /// nothing fabricated, and Q exits the loop.
    #[test]
    fn q_from_the_idle_state_exits_and_touches_nothing() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        assert_eq!(model.source(), None, "the idle start has no episode");

        assert_eq!(
            handle_key(key(KeyCode::Char('q')), &mut model, &mut player),
            Step::Exit
        );
        assert!(player.active_handle().is_none());
    }

    /// The stop action routes through the EXISTING request_stop seam —
    /// the same frozen right the machine transport uses — quit never
    /// touches the episode, and with no episode the command is inert.
    #[test]
    fn the_stop_action_routes_through_the_request_stop_seam_only() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        // Inert without an episode.
        assert_eq!(
            dispatch(TuiAction::Stop, &mut model, &mut player),
            Step::Continue
        );
        assert!(player.active_handle().is_none());

        // Commit an episode by opening a real file.
        let tree = TempTree::new("stop-seam");
        let file = tree.live_file("live.flac");
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        refresh(&mut model, &player);

        assert!(!player.active_handle().unwrap().observe().stop_requested);
        assert_eq!(
            dispatch(TuiAction::Stop, &mut model, &mut player),
            Step::Continue
        );
        assert!(
            player.active_handle().unwrap().observe().stop_requested,
            "Stop must record stop intent through the seam"
        );
        // Idempotent: dispatching Stop again stays a plain seam call.
        assert_eq!(
            dispatch(TuiAction::Stop, &mut model, &mut player),
            Step::Continue
        );

        // Quit is loop control, not a playback command.
        let before = player.active_handle().unwrap().observe();
        assert_eq!(
            dispatch(TuiAction::Quit, &mut model, &mut player),
            Step::Exit
        );
        assert_eq!(player.active_handle().unwrap().observe(), before);
    }

    /// The pause/resume action never keeps a local paused bool: the
    /// first dispatch records pause intent through the seam, the next
    /// releases it, and the choice between the two commands is read
    /// from a fresh authoritative observation each time.
    #[test]
    fn the_pause_resume_action_routes_through_the_seam_both_ways() {
        let tree = TempTree::new("pause-resume");
        let file = tree.live_file("live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        let handle = player.active_handle().expect("committed").clone();
        assert!(!handle.observe().pause_requested);
        assert!(!handle.observe().paused());

        assert_eq!(
            dispatch(TuiAction::PlayPause, &mut model, &mut player),
            Step::Continue
        );
        assert!(
            handle.observe().pause_requested,
            "the first PlayPause must record pause intent through the seam"
        );
        assert_eq!(
            dispatch(TuiAction::PlayPause, &mut model, &mut player),
            Step::Continue
        );
        assert!(
            !handle.observe().pause_requested,
            "the second PlayPause must release the pause through the seam"
        );
    }

    /// The seek action routes the SAME request_seek seam, and an
    /// episode whose position is unknown gets NO command: the dispatch
    /// changes nothing (and panics on nothing).
    #[test]
    fn the_seek_action_changes_nothing_when_the_position_is_unknown() {
        let tree = TempTree::new("seek-blind");
        let file = tree.live_file("live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        let handle = player.active_handle().expect("committed").clone();

        let before = handle.observe();
        assert_eq!(
            dispatch(TuiAction::SeekRelative(5), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(
            dispatch(TuiAction::SeekRelative(-30), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(
            handle.observe(),
            before,
            "no position evidence: no seek command"
        );
    }

    /// The N/P actions are the App's manual navigation, not episode
    /// commands: the shell reports the outcome, the episode is only
    /// ever touched by the frozen replacement.
    #[test]
    fn n_and_p_route_the_manual_navigation() {
        let tree = TempTree::new("navigation");
        let a = tree.live_file("live-a.flac");
        let b = tree.live_file("live-b.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        assert_eq!(player.open(&a), crate::player::OpenOutcome::Opened);
        player.establish_playlist(vec![a.clone(), b.clone()]);
        refresh(&mut model, &player);

        assert_eq!(
            dispatch(TuiAction::Next, &mut model, &mut player),
            Step::Continue
        );
        refresh(&mut model, &player);
        assert_eq!(model.source(), Some(b.to_string_lossy().as_ref()));
        assert!(
            model.status().unwrap().starts_with("next: opened "),
            "{:?}",
            model.status()
        );

        assert_eq!(
            dispatch(TuiAction::Previous, &mut model, &mut player),
            Step::Continue
        );
        refresh(&mut model, &player);
        assert_eq!(model.source(), Some(a.to_string_lossy().as_ref()));
    }

    /// The policy and volume actions route to the player and say so;
    /// they never touch the episode.
    #[test]
    fn policy_and_volume_route_to_the_player() {
        let tree = TempTree::new("policy");
        let file = tree.live_file("live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let handle = {
            assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
            player.active_handle().expect("committed").clone()
        };
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        assert_eq!(
            dispatch(TuiAction::ToggleOrder, &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(player.playlist_order(), PlaybackOrder::Shuffle);
        assert_eq!(model.status(), Some("Order: Shuffle"));
        assert_eq!(
            dispatch(TuiAction::CycleRepeat, &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(player.playlist_repeat(), RepeatMode::All);
        assert_eq!(
            dispatch(TuiAction::VolumeDown, &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(player.desired_volume(), 95);
        assert_eq!(model.status(), Some("volume 95/100 (desired)"));
        assert_eq!(
            handle.observe().terminal_outcome,
            None,
            "policy and volume never touch the episode"
        );
    }

    // ------------------------------------------------------------------
    // Route switching never touches product state (§5).
    // ------------------------------------------------------------------

    /// Route switching leaves playback, navigation and policy state
    /// untouched, and the SAME TuiAction arrives from the keyboard
    /// (Enter on the focused tab) and the mouse (a tab click) (§30).
    #[test]
    fn route_switching_is_presentation_only_and_parity_holds() {
        let tree = TempTree::new("routes");
        let file = tree.live_file("live.flac");
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        player.establish_playlist(vec![file.clone()]);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        let activations = || {
            log.lock()
                .unwrap()
                .iter()
                .filter(|event| event.starts_with("activate "))
                .count()
        };
        let activations_before = activations();

        // Keyboard: focus the Playlist tab, Enter.
        model.set_focus(Some(FocusId::RouteTab(TuiRoute::Playlist)));
        let keyboard_action = model.activation().expect("the tab activates");
        assert_eq!(keyboard_action, TuiAction::Navigate(TuiRoute::Playlist));
        assert_eq!(
            dispatch(keyboard_action, &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(model.route(), TuiRoute::Playlist);

        // The player never noticed.
        let observation = player.active_handle().unwrap().observe();
        assert_eq!(observation.terminal_outcome, None);
        assert!(!observation.stop_requested);
        assert!(!observation.pause_requested);
        assert_eq!(player.playlist_order(), PlaybackOrder::Sequential);
        assert_eq!(player.playlist_repeat(), RepeatMode::Off);
        assert_eq!(player.navigation_position(), Some((1, 1)));
        assert_eq!(activations(), activations_before, "no probe, no open");

        // Mouse: click the Audio tab. Same action shape, same dispatch.
        let (column, row) = draw_and_locate(
            &mut model,
            100,
            30,
            &crate::tui::model::HitTarget::RouteTab(TuiRoute::Audio),
        );
        let mouse_action = click_at(column, row, &mut model, &mut player);
        assert_eq!(mouse_action, Some(TuiAction::Navigate(TuiRoute::Audio)));
        assert_eq!(model.route(), TuiRoute::Audio);
        let observation = player.active_handle().unwrap().observe();
        assert_eq!(observation.terminal_outcome, None);
        assert!(!observation.stop_requested);
        assert!(!observation.pause_requested);
        assert_eq!(activations(), activations_before, "still no probe, no open");
    }

    // ------------------------------------------------------------------
    // Transport parity: keyboard and mouse converge on one action (§30).
    // ------------------------------------------------------------------

    #[test]
    fn transport_activation_parity_holds() {
        let tree = TempTree::new("transport-parity");
        let file = tree.live_file("live.flac");
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        let activations = || {
            log.lock()
                .unwrap()
                .iter()
                .filter(|event| event.starts_with("activate "))
                .count()
        };
        let activations_before = activations();

        let wanted = crate::tui::model::HitTarget::Transport(TransportButton::PlayPause);
        let (column, row) = draw_and_locate(&mut model, 100, 30, &wanted);
        let mouse_action = click_at(column, row, &mut model, &mut player);

        model.set_focus(Some(FocusId::Transport(TransportButton::PlayPause)));
        let keyboard_action = model.activation();
        assert_eq!(mouse_action, keyboard_action, "§30: one action per control");

        // The dispatched mouse click really paused the episode: the
        // witness is the SEAM's command state, not the status line.
        assert!(
            player.active_handle().unwrap().observe().pause_requested,
            "the clicked Play/Pause recorded pause intent"
        );
        assert_eq!(
            activations(),
            activations_before,
            "the transport click composed no new episode"
        );
    }

    // ------------------------------------------------------------------
    // Playlist route behaviors (§35: focus, selection, row hit, wheel).
    // ------------------------------------------------------------------

    #[test]
    fn the_playlist_route_selects_and_plays() {
        let tree = TempTree::new("playlist-route");
        let files: Vec<PathBuf> = (0..3)
            .map(|n| tree.live_file(&format!("live-{n}.flac")))
            .collect();
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        assert_eq!(player.open(&files[0]), crate::player::OpenOutcome::Opened);
        player.establish_playlist(files.clone());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        // To the Playlist route; focus falls to the route's first local
        // control (§12) — the toolbar's first button (G2: the toolbar
        // always exists) — then Tab walks the toolbar into the list.
        dispatch(
            TuiAction::Navigate(TuiRoute::Playlist),
            &mut model,
            &mut player,
        );
        model.validate_focus();
        assert_eq!(
            model.focus(),
            Some(FocusId::PlaylistButton(PlaylistButton::AddFile))
        );
        for _ in 0..5 {
            handle_key(key(KeyCode::Tab), &mut model, &mut player);
        }
        assert_eq!(model.focus(), Some(FocusId::Playlist));

        // ↓ moves only the selection — no probe, no open.
        let events_before = log.lock().unwrap().len();
        assert_eq!(
            handle_key(key(KeyCode::Down), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(player.playlist_selected_position(), Some(1));
        assert_eq!(player.playlist_playing_position(), Some(0));
        assert_eq!(
            log.lock().unwrap().len(),
            events_before,
            "selection is presentation"
        );

        // A mouse row hit selects that row.
        let wanted = crate::tui::model::HitTarget::PlaylistRow(2);
        let (column, row) = draw_and_locate(&mut model, 100, 30, &wanted);
        let mouse_action = click_at(column, row, &mut model, &mut player);
        assert_eq!(
            mouse_action,
            Some(TuiAction::PlaylistSelect(PlaylistCursor::Row(2)))
        );
        assert_eq!(player.playlist_selected_position(), Some(2));

        // Enter on the focused list plays the selected row.
        model.set_focus(Some(FocusId::Playlist));
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        refresh(&mut model, &player);
        assert_eq!(player.active_source(), Some(files[2].as_path()));
        assert!(
            model.status().unwrap().starts_with("play: opened "),
            "{:?}",
            model.status()
        );
    }

    // ------------------------------------------------------------------
    // Playlist productization (G2): the toolbar's destructive flows and
    // the mode-restricted Add picker.
    // ------------------------------------------------------------------

    /// Remove of the row that OWNS the live episode asks first (T0):
    /// the stop-aware confirmation opens with Cancel focused, the
    /// question touches nothing, and the confirmed choice performs the
    /// App's retire-first removal.
    #[test]
    fn removing_the_live_row_asks_first_then_retires_it() {
        let tree = TempTree::new("remove-current");
        let files: Vec<PathBuf> = (0..3)
            .map(|n| tree.live_file(&format!("live-{n}.flac")))
            .collect();
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&files[0]), crate::player::OpenOutcome::Opened);
        player.establish_playlist(files.clone());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        dispatch(TuiAction::PlaylistRemove, &mut model, &mut player);
        assert_eq!(
            model.modal().map(Modal::kind),
            Some(ModalKind::ConfirmRemoveCurrent),
            "the live row's removal asks first"
        );
        assert_eq!(
            model.focus(),
            Some(FocusId::PickerButton(ModalButton::Cancel)),
            "Cancel starts focused (T0): the dangerous choice is never the default"
        );
        assert!(player.active_handle().is_some(), "asking touches nothing");
        assert_eq!(player.playlist_rows().count(), 3, "asking removes nothing");

        // Tab to the destructive choice, Enter confirms: the App
        // retires the episode FIRST, then the row leaves the list.
        handle_key(key(KeyCode::Tab), &mut model, &mut player);
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        assert!(model.modal().is_none(), "the confirmation closed");
        assert!(
            player.active_handle().is_none(),
            "removing the live row retired the episode first"
        );
        refresh(&mut model, &player);
        assert_eq!(model.playlist().len(), 2);
        assert!(
            model
                .status()
                .is_some_and(|status| status.starts_with("removed ")),
            "{}",
            model.status().unwrap_or_default()
        );
    }

    /// Remove of a row that does NOT own the live episode is a direct
    /// App list edit — no confirmation stands between (the frozen
    /// confirmation exists for the stop consequence only), and the live
    /// episode survives.
    #[test]
    fn removing_a_non_live_row_removes_directly() {
        let tree = TempTree::new("remove-other");
        let files: Vec<PathBuf> = (0..3)
            .map(|n| tree.live_file(&format!("live-{n}.flac")))
            .collect();
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&files[0]), crate::player::OpenOutcome::Opened);
        player.establish_playlist(files.clone());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        dispatch(
            TuiAction::PlaylistSelect(PlaylistCursor::Row(1)),
            &mut model,
            &mut player,
        );
        dispatch(TuiAction::PlaylistRemove, &mut model, &mut player);
        assert!(
            model.modal().is_none(),
            "no confirmation for a non-live row"
        );
        refresh(&mut model, &player);
        assert_eq!(model.playlist().len(), 2);
        assert!(player.active_handle().is_some(), "the episode survived");
        assert_eq!(player.active_source(), Some(files[0].as_path()));
    }

    /// Clear with a live episode asks first (T0); Cancel keeps
    /// everything; the confirmed choice retires the episode first and
    /// empties the list.
    #[test]
    fn clearing_with_a_live_episode_asks_and_cancel_keeps_everything() {
        let tree = TempTree::new("clear-confirm");
        let files: Vec<PathBuf> = (0..2)
            .map(|n| tree.live_file(&format!("live-{n}.flac")))
            .collect();
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&files[0]), crate::player::OpenOutcome::Opened);
        player.establish_playlist(files.clone());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        dispatch(TuiAction::PlaylistClear, &mut model, &mut player);
        assert_eq!(
            model.modal().map(Modal::kind),
            Some(ModalKind::ConfirmClear),
            "clearing a live episode asks first"
        );
        assert_eq!(
            model.focus(),
            Some(FocusId::PickerButton(ModalButton::Cancel)),
            "Cancel starts focused (T0)"
        );

        // Esc cancels: the list and the episode are untouched.
        handle_key(key(KeyCode::Esc), &mut model, &mut player);
        assert!(model.modal().is_none());
        assert!(player.active_handle().is_some());
        refresh(&mut model, &player);
        assert_eq!(model.playlist().len(), 2);

        // Ask again, confirm this time: retire first, then empty.
        dispatch(TuiAction::PlaylistClear, &mut model, &mut player);
        handle_key(key(KeyCode::Tab), &mut model, &mut player);
        handle_key(key(KeyCode::Enter), &mut model, &mut player);
        assert!(
            player.active_handle().is_none(),
            "clear retired the episode first"
        );
        refresh(&mut model, &player);
        assert!(model.playlist().is_empty());
        assert_eq!(model.status(), Some("playlist cleared"));
    }

    /// The toolbar's Add File opens the picker in the restricted
    /// Add-File mode — no [Open], no [Use this folder] — and a
    /// committed file appends WITHOUT starting an episode (T0: Add
    /// never plays; T1A A1/A2).
    #[test]
    fn the_toolbar_add_file_appends_without_starting_playback() {
        let tree = TempTree::new("toolbar-add-file");
        let _existing = tree.live_file("already.flac");
        let addition = tree.live_file("addition.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        dispatch(TuiAction::PlaylistAddFile, &mut model, &mut player);
        assert_eq!(model.picker_mode(), Some(PickerMode::AddFile));
        let cycle = model.focus_cycle();
        assert!(
            !cycle.contains(&FocusId::PickerButton(ModalButton::Open)),
            "no [Open] in Add File mode"
        );
        assert!(
            !cycle.contains(&FocusId::PickerButton(ModalButton::UseFolder)),
            "no [Use this folder] in Add File mode"
        );

        model.set_open_listing(
            tree.path().to_path_buf(),
            Ok(vec![crate::input::DirectoryEntry {
                name: "addition.flac".to_owned(),
                is_dir: false,
                path: addition,
            }]),
        );
        // ↓ ↓ walks the cursor onto the file row (past `..`).
        handle_key(key(KeyCode::Down), &mut model, &mut player);
        handle_key(key(KeyCode::Down), &mut model, &mut player);
        dispatch(
            TuiAction::ModalInput(ModalInput::CommitAdd),
            &mut model,
            &mut player,
        );
        refresh(&mut model, &player);
        assert!(
            player.active_handle().is_none(),
            "Add never starts an episode"
        );
        assert_eq!(model.playlist().len(), 1);
        assert!(
            model
                .status()
                .is_some_and(|status| status.contains("added")),
            "{}",
            model.status().unwrap_or_default()
        );
    }

    /// The Audio route's [Apply] (G3) commits the draft through the
    /// App's own desired-DSP seams. Without a live episode the seams
    /// validate the candidate directly; the draft re-syncs clean and
    /// the status line stays truthful about applied state.
    #[test]
    fn the_audio_route_applies_the_draft_without_a_live_episode() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        dispatch(
            TuiAction::Navigate(TuiRoute::Audio),
            &mut model,
            &mut player,
        );
        dispatch(TuiAction::DspToggleEnabled, &mut model, &mut player);
        dispatch(TuiAction::DspPreampStep(-6), &mut model, &mut player);
        assert!(
            model.audio_draft().expect("draft").dirty(),
            "the edits are pending"
        );

        dispatch(TuiAction::DspApply, &mut model, &mut player);
        let desired = player.desired_processing();
        assert!(desired.enabled, "the enablement committed");
        assert!(
            (desired.gain - 10f32.powf(-6.0 / 20.0)).abs() < 1e-5,
            "the preamp committed: {}",
            desired.gain
        );
        let draft = model.audio_draft().expect("the session stays open");
        assert!(!draft.dirty(), "the draft re-synced clean");
        assert!(
            model
                .status()
                .is_some_and(|status| status.contains("applied state not reported")),
            "{}",
            model.status().unwrap_or_default()
        );
    }

    /// [Apply] with a live episode runs the same seams through the
    /// handle; a preset chosen in the menu commits as the custom
    /// config seam (the summary still recognizes it by its trims).
    #[test]
    fn the_audio_route_applies_a_preset_with_a_live_episode() {
        let tree = TempTree::new("audio-apply-live");
        let file = tree.live_file("live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        dispatch(
            TuiAction::Navigate(TuiRoute::Audio),
            &mut model,
            &mut player,
        );
        // DSP must be ON for the summary to name the EQ stage at all
        // (a bypass configuration's stages are inert by definition).
        dispatch(TuiAction::DspToggleEnabled, &mut model, &mut player);
        dispatch(TuiAction::DspOpenPresets, &mut model, &mut player);
        // ↓ ↓ into the menu, Enter applies the preset to the draft.
        assert_eq!(
            handle_key(key(KeyCode::Down), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(
            handle_key(key(KeyCode::Down), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        assert!(model.modal().is_none(), "the menu closed on activation");
        let draft = model.audio_draft().expect("the menu edited the draft");
        assert_eq!(
            draft.config().eq,
            Some(EqPreset::Jazz.to_config().eq.expect("EQ")),
            "two downs from no cursor: flat, jazz"
        );

        dispatch(TuiAction::DspApply, &mut model, &mut player);
        let desired = player.desired_processing();
        assert_eq!(
            desired.eq,
            Some(EqPreset::Jazz.to_config().eq.expect("EQ")),
            "the preset trims committed"
        );
        refresh(&mut model, &player);
        assert!(
            model
                .desired_dsp_label()
                .is_some_and(|label| label.contains("preset jazz")),
            "the summary recognizes the preset by its trims: {:?}",
            model.desired_dsp_label()
        );
    }

    /// [Cancel] discards a dirty draft with a notice; the App's
    /// desired configuration is untouched.
    #[test]
    fn the_audio_route_cancel_discards_the_draft() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        let before = player.desired_processing();

        dispatch(
            TuiAction::Navigate(TuiRoute::Audio),
            &mut model,
            &mut player,
        );
        dispatch(TuiAction::DspToggleEnabled, &mut model, &mut player);
        dispatch(TuiAction::DspCancel, &mut model, &mut player);
        assert!(model.audio_draft().is_none(), "the draft discarded");
        assert_eq!(
            player.desired_processing(),
            before,
            "cancel never touches the App"
        );
        assert!(
            model
                .status()
                .is_some_and(|status| status.contains("draft discarded")),
            "{}",
            model.status().unwrap_or_default()
        );
    }

    /// Add Folder refuses a FILE subject with the bounded truthful
    /// diagnostic and keeps the picker open for correction (T0 subject
    /// kinds; G1 F11 retention).
    #[test]
    fn add_folder_mode_refuses_a_file_subject() {
        let tree = TempTree::new("toolbar-add-folder");
        let file = tree.live_file("not-a-folder.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        dispatch(TuiAction::PlaylistAddFolder, &mut model, &mut player);
        assert_eq!(model.picker_mode(), Some(PickerMode::AddFolder));
        model.set_open_listing(
            tree.path().to_path_buf(),
            Ok(vec![crate::input::DirectoryEntry {
                name: "not-a-folder.flac".to_owned(),
                is_dir: false,
                path: file,
            }]),
        );
        handle_key(key(KeyCode::Down), &mut model, &mut player);
        handle_key(key(KeyCode::Down), &mut model, &mut player);
        dispatch(
            TuiAction::ModalInput(ModalInput::CommitAdd),
            &mut model,
            &mut player,
        );
        assert!(
            model.modal().is_some(),
            "the picker stays open for correction"
        );
        assert_eq!(model.status(), Some("select a folder to add"));
        refresh(&mut model, &player);
        assert!(model.playlist().is_empty(), "nothing was appended");
    }

    // ------------------------------------------------------------------
    // Modal isolation and the one-event-one-action invariant (§25/§31).
    // ------------------------------------------------------------------

    /// While a modal is open, a click on a background control neither
    /// dispatches nor steals focus — and after the modal closes on ONE
    /// key event, that event has already been consumed: nothing leaks
    /// through to the background.
    #[test]
    fn a_modal_click_never_leaks_and_a_close_consumes_its_event() {
        let tree = TempTree::new("modal-isolation");
        let file = tree.live_file("live.flac");
        let source = FakeEpisodeSource::new();
        let mut player = ReferencePlayerApp::new(source);
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        // Background click behind the help overlay.
        dispatch(
            TuiAction::OpenModal(ModalKind::Help),
            &mut model,
            &mut player,
        );
        let wanted = crate::tui::model::HitTarget::Transport(TransportButton::Stop);
        let (column, row) = draw_and_locate(&mut model, 100, 30, &wanted);
        assert_eq!(click_at(column, row, &mut model, &mut player), None);
        assert!(
            !player.active_handle().unwrap().observe().stop_requested,
            "the background Stop never fired"
        );

        // Esc closes the help overlay and NOTHING else: the same event
        // cannot also act on the background (§31).
        assert_eq!(
            handle_key(key(KeyCode::Esc), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(model.modal(), None);
        assert!(
            !player.active_handle().unwrap().observe().stop_requested,
            "the closing Esc never leaked through"
        );
        assert!(
            !player.active_handle().unwrap().observe().pause_requested,
            "no background command fired behind the modal either"
        );
    }

    /// ONE physical Enter inside the GoTo modal performs at most one
    /// product operation: the seek. The same Enter cannot also activate
    /// a background control (§31).
    #[test]
    fn one_modal_enter_performs_at_most_one_product_operation() {
        let tree = TempTree::new("one-enter");
        let file = tree.live_file("live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        dispatch(
            TuiAction::OpenModal(ModalKind::GoTo),
            &mut model,
            &mut player,
        );
        type_text(&mut model, &mut player, "1:35");
        let event = key(KeyCode::Enter);
        // Exactly one decode out of this physical event.
        let action = decode_key(event, &model);
        assert_eq!(action, Some(TuiAction::ModalInput(ModalInput::Confirm)));
        assert_eq!(
            dispatch(action.expect("one action"), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(model.modal(), None, "the modal closed");
        assert_eq!(
            model.status(),
            Some("seek requested: 01:35"),
            "exactly one product operation was performed"
        );
    }

    // ------------------------------------------------------------------
    // Resize (§29).
    // ------------------------------------------------------------------

    /// A resize event clears the armed click and the old hit regions;
    /// the next draw republishes fresh geometry and the focus stays on
    /// a valid visible control. No product command is generated.
    #[test]
    fn a_resize_invalidates_geometry_and_preserves_a_valid_focus() {
        let tree = TempTree::new("resize");
        let file = tree.live_file("live.flac");
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        let activations = || {
            log.lock()
                .unwrap()
                .iter()
                .filter(|event| event.starts_with("activate "))
                .count()
        };
        let activations_before = activations();

        // Arm a click on a transport button.
        let wanted = crate::tui::model::HitTarget::Transport(TransportButton::PlayPause);
        let (column, row) = draw_and_locate(&mut model, 100, 30, &wanted);
        let down = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        decode_mouse(down, &mut model);
        assert!(model.armed().is_some());

        // The resize event.
        model.invalidate_frame();
        assert_eq!(model.armed(), None, "the resize cleared the armed click");
        let up = MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(
            decode_mouse(up, &mut model),
            None,
            "the Up after a resize is no action"
        );

        // The next draw republishes fresh geometry and the focus (set
        // by the Down before the resize) is still a valid control.
        draw_and_locate(&mut model, 100, 30, &wanted);
        assert!(
            !model.regions().is_empty(),
            "fresh geometry after the resize"
        );
        assert_eq!(
            model.focus(),
            Some(FocusId::Transport(TransportButton::PlayPause)),
            "the focus is still a visible enabled control"
        );
        assert_eq!(
            activations(),
            activations_before,
            "the resize composed nothing"
        );
    }

    // ------------------------------------------------------------------
    // Focus cycle keys route (Tab / Shift+Tab).
    // ------------------------------------------------------------------

    #[test]
    fn tab_moves_focus_through_the_real_cycle() {
        let tree = TempTree::new("tab");
        let file = tree.live_file("live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        // Draw once so the class (Wide at 100x30) is known; the draw's
        // focus validation lands on the route's first local control.
        draw_and_locate(
            &mut model,
            100,
            30,
            &crate::tui::model::HitTarget::RouteTab(TuiRoute::NowPlaying),
        );
        assert_eq!(
            model.focus(),
            Some(FocusId::Transport(TransportButton::Open)),
            "the validated start is the route's first local control"
        );

        for expected in [
            FocusId::Transport(TransportButton::Previous),
            FocusId::Transport(TransportButton::PlayPause),
            FocusId::Transport(TransportButton::Stop),
            FocusId::Transport(TransportButton::Next),
            FocusId::Preference(PreferenceButton::VolumeDown),
            FocusId::Preference(PreferenceButton::VolumeUp),
            FocusId::Preference(PreferenceButton::Order),
            FocusId::Preference(PreferenceButton::Repeat),
            FocusId::RouteTab(TuiRoute::NowPlaying),
            FocusId::RouteTab(TuiRoute::Playlist),
        ] {
            assert_eq!(
                handle_key(key(KeyCode::Tab), &mut model, &mut player),
                Step::Continue
            );
            assert_eq!(model.focus(), Some(expected));
        }
        // Shift+Tab walks back (terminals report BackTab).
        assert_eq!(
            handle_key(
                KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
                &mut model,
                &mut player
            ),
            Step::Continue
        );
        assert_eq!(model.focus(), Some(FocusId::RouteTab(TuiRoute::NowPlaying)));
        assert_eq!(
            dispatch(
                TuiAction::MoveFocus(FocusMove::Previous),
                &mut model,
                &mut player
            ),
            Step::Continue
        );
        assert_eq!(
            model.focus(),
            Some(FocusId::Preference(PreferenceButton::Repeat))
        );
    }

    // ------------------------------------------------------------------
    // Terminal guard: the mouse-capture pairing (§16/§40).
    // ------------------------------------------------------------------

    /// EnableMouseCapture on the enter sequence has its matching
    /// DisableMouseCapture on the same restore lifecycle, ordered
    /// before the alternate-screen leave. (The raw-mode pairing is the
    /// guard's own concern; this pins the NEW capture obligation.)
    ///
    /// The oracle is ANSI byte order, valid only where the capture
    /// commands actually emit bytes: on Windows crossterm routes mouse
    /// capture through the WinAPI console path
    /// (`is_ansi_code_supported` is `false`), so nothing reaches the
    /// writer. The pairing there is crossterm's own mechanism; Windows
    /// keeps the compile gate and device-free tests, and real device
    /// evidence stays out of CI scope.
    #[cfg(not(windows))]
    /// The preference row converges mouse and keyboard: clicking a
    /// visible control performs exactly what its accelerator key does,
    /// through the same TuiAction.
    #[test]
    fn preference_row_clicks_converge_on_the_same_actions_as_their_keys() {
        let tree = TempTree::new("prefs-row");
        let file = tree.live_file("live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        // Volume down via the visible stepper.
        let (column, row) = draw_and_locate(
            &mut model,
            100,
            30,
            &crate::tui::model::HitTarget::Preference(PreferenceButton::VolumeDown),
        );
        click_at(column, row, &mut model, &mut player);
        assert_eq!(player.desired_volume(), 95, "− steps the desired factor");

        // Order toggle via the visible control.
        let (column, row) = draw_and_locate(
            &mut model,
            100,
            30,
            &crate::tui::model::HitTarget::Preference(PreferenceButton::Order),
        );
        click_at(column, row, &mut model, &mut player);
        assert_eq!(
            player.playlist_order(),
            PlaybackOrder::Shuffle,
            "the order control toggles Sequential ↔ Shuffle"
        );

        // Repeat cycle via the visible control.
        let (column, row) = draw_and_locate(
            &mut model,
            100,
            30,
            &crate::tui::model::HitTarget::Preference(PreferenceButton::Repeat),
        );
        click_at(column, row, &mut model, &mut player);
        assert_eq!(player.playlist_repeat(), RepeatMode::All);
    }

    /// §25: while the picker is open a click on the background — even
    /// on a rendered, armed-looking control behind the popup —
    /// dispatches nothing and touches no product state.
    #[test]
    fn a_background_click_while_the_picker_is_open_dispatches_nothing() {
        let tree = TempTree::new("picker-isolation");
        let file = tree.live_file("live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        assert_eq!(
            handle_key(key(KeyCode::Char('O')), &mut model, &mut player),
            Step::Continue
        );
        let (column, row) = draw_and_locate(
            &mut model,
            100,
            30,
            &crate::tui::model::HitTarget::Transport(TransportButton::Previous),
        );
        let action = click_at(column, row, &mut model, &mut player);
        assert!(
            !matches!(action, Some(TuiAction::Previous | TuiAction::OpenModal(_))),
            "a background control must not activate behind the modal: {action:?}"
        );
        assert_eq!(
            model.modal().map(|modal| modal.kind()),
            Some(ModalKind::Open),
            "the modal stays open"
        );
        assert!(
            player.active_source().is_some(),
            "no navigation fired behind the modal"
        );
    }

    /// A seek-bar click arrives as a per-mille and the dispatch is
    /// inert without an episode or duration evidence — never a
    /// fabricated command.
    #[test]
    fn a_seek_per_mille_without_evidence_sends_nothing() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        assert_eq!(
            dispatch(TuiAction::SeekPerMille(500), &mut model, &mut player),
            Step::Continue,
            "no episode: the fraction is dropped, not fabricated into a command"
        );
    }
    /// G1 F03 — the frozen Home-Play truth table, through the existing
    /// product authority only:
    ///
    /// ```text
    /// unsettled episode, pause not requested -> pause (command state)
    /// paused episode (pause requested)       -> resume
    /// terminal current entry                 -> fresh replay via the
    ///                                           existing replacement
    /// no episode, a list                     -> play the selection
    /// neither                                -> honest inert reason
    /// ```
    #[test]
    fn home_play_follows_the_frozen_truth_table() {
        let tree = TempTree::new("home-play");
        let file = tree.live_file("home.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());

        // Neither episode nor list: the honest disabled reason.
        refresh(&mut model, &player);
        assert_eq!(
            dispatch(TuiAction::PlayPause, &mut model, &mut player),
            Step::Continue
        );
        assert!(player.active_handle().is_none(), "nothing started");
        assert_eq!(
            model.status(),
            Some("No music loaded — open something with the Open button"),
            "the disabled Play says why"
        );

        // No episode, a list: play_current plays the SELECTED entry
        // (the App's no-episode Home-Play rule) — no autoplay happened
        // at Add time.
        assert_eq!(player.append_admitted(vec![file.clone()]), Ok(1));
        refresh(&mut model, &player);
        assert!(player.active_handle().is_none());
        assert_eq!(
            dispatch(TuiAction::PlayPause, &mut model, &mut player),
            Step::Continue
        );
        let handle = player
            .active_handle()
            .expect("the selection was played")
            .clone();
        assert_eq!(handle.observe().terminal_outcome, None);

        // Unsettled episode: the fresh pause-command state decides.
        assert_eq!(
            dispatch(TuiAction::PlayPause, &mut model, &mut player),
            Step::Continue
        );
        assert!(
            handle.observe().pause_requested,
            "the first press requested a pause"
        );
        assert_eq!(
            dispatch(TuiAction::PlayPause, &mut model, &mut player),
            Step::Continue
        );
        assert!(
            !handle.observe().pause_requested,
            "the second press resumed"
        );

        // Terminal current entry: the press REPLAYS through the
        // existing replacement — a fresh episode, never a mutated old
        // one.
        handle.request_stop();
        let outcome = handle.wait_terminal();
        assert_eq!(outcome, qianqian_playback::EpisodeTerminalOutcome::Stopped);
        assert_eq!(
            player.active_handle().unwrap().observe().terminal_outcome,
            Some(qianqian_playback::EpisodeTerminalOutcome::Stopped),
            "the terminal Fact is committed and visible"
        );
        assert_eq!(
            dispatch(TuiAction::PlayPause, &mut model, &mut player),
            Step::Continue
        );
        let replayed = player.active_handle().expect("a fresh episode");
        assert_eq!(
            replayed.observe().terminal_outcome,
            None,
            "the fresh episode is unsettled — the terminal old handle was \
             replaced, not mutated back"
        );
        assert!(
            model
                .status()
                .is_some_and(|status| status.starts_with("play: opened ")),
            "{}",
            model.status().unwrap_or_default()
        );
    }

    /// G1 re-review (R1/R2/R3 convergent): a SAME-FILE replay replaces
    /// the episode while the display string is unchanged, and the arm
    /// must die with the old episode — the lossy display string is not
    /// episode identity, so the runtime's replacement notification is
    /// what cancels the arm.
    #[test]
    fn a_same_file_replay_disarms_a_press_at_the_same_cell() {
        let tree = TempTree::new("same-file-replay");
        let file = tree.live_file("song.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        open_via_keys(&mut model, &mut player, &file);
        refresh(&mut model, &player);

        // Arm the central transport button on the live episode.
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|frame| super::super::view::draw(frame, &mut model))
            .expect("draw");
        let (column, row) = region_cell(&model, &HitTarget::Transport(TransportButton::PlayPause));
        decode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut model,
        );
        assert!(model.armed().is_some(), "the press arms");

        // Terminal, then a Home-Play replay through the SAME path: a
        // fresh episode under an unchanged display string.
        let handle = player.active_handle().expect("live").clone();
        handle.request_stop();
        let outcome = handle.wait_terminal();
        assert_eq!(outcome, qianqian_playback::EpisodeTerminalOutcome::Stopped);
        assert_eq!(
            dispatch(TuiAction::PlayPause, &mut model, &mut player),
            Step::Continue
        );
        refresh(&mut model, &player);
        assert_eq!(model.source(), Some(file.to_string_lossy().as_ref()));

        // The same control still sits at the same cell — but the armed
        // press died with the episode it was armed in.
        terminal
            .draw(|frame| super::super::view::draw(frame, &mut model))
            .expect("draw");
        assert_eq!(
            model.hit_test(column, row),
            Some(HitTarget::Transport(TransportButton::PlayPause)),
            "the same control IS rendered at the same cell"
        );
        assert_eq!(model.armed(), None, "the replay disarmed the press");
        assert_eq!(
            decode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), column, row),
                &mut model
            ),
            None,
            "the old interaction cannot cross into the replayed episode"
        );
    }

    /// The inert Enter on the row that IS the live episode (field round
    /// 3): the refusal is the bounded status feedback, named in
    /// vocabulary the forbidden-claim scan accepts — never a playing
    /// claim. This render pins that the scan stays clean (the review
    /// round found the old wording leaked a forbidden word).
    #[test]
    fn enter_on_the_live_row_refuses_in_scan_clean_vocabulary() {
        let tree = TempTree::new("live-row");
        let file = tree.live_file("live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(player.open(&file), crate::player::OpenOutcome::Opened);
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        // The committed episode is also the selection.
        assert!(player.selected_is_live_episode());
        assert_eq!(
            dispatch(TuiAction::PlaylistPlaySelected, &mut model, &mut player),
            Step::Continue
        );
        let status = model.status().expect("the refusal is reported").to_owned();
        assert_eq!(status, "the selected track is the live episode");
        assert_eq!(
            crate::status::forbidden_status_claim(&status),
            None,
            "the refusal must stay inside the earned vocabulary"
        );
        assert!(
            player.active_handle().is_some(),
            "the live episode is untouched"
        );
    }
}
