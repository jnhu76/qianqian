//! The interaction behavior on [`TuiModel`]: routes, the focus cycle,
//! the modal lifecycle, the Open picker's draft operations and the hit
//! test — presentation state only, never product authority.

use ratatui::layout::Position;

use super::actions::{PlaylistCursor, TuiAction, TuiRoute};
use super::controls::{
    AUDIO_BUTTONS, AudioButton, EqAdjust, NAV_BUTTONS, NavBarButton, PLAYLIST_BUTTONS, PREFERENCES,
    PlaylistButton, PreferenceButton, SEEK_BUTTONS, SeekButton, TRANSPORT, TransportButton,
};
use super::focus::{FocusId, FocusMove};
#[cfg(test)]
use super::hit::ArmedClick;
use super::hit::{HitRegion, HitTarget};
use super::modal::{
    ConfirmKind, HELP_PAGE_LINES, HelpScroll, Modal, ModalButton, ModalConfirm, ModalInput,
    ModalKind, OpenPicker, PickerEntry, PickerMode, PickerSubject,
};
use super::projection::SEEK_STEP_SECS;
use super::responsive::ResponsiveClass;
use super::state::TuiModel;
use super::visualizer::VISUALIZER_MODES;
use qianqian_playback::{EQ_BAND_FREQUENCY_HZ, EqPreset};

impl TuiModel {
    // The T1B interaction state (route / focus / modal / hit regions).
    // All of it is presentation state; none of it is product authority.
    // ------------------------------------------------------------------

    /// The active route.
    pub fn route(&self) -> TuiRoute {
        self.route
    }

    /// The one active focus target, if any.
    pub fn focus(&self) -> Option<FocusId> {
        self.focus
    }

    /// The active modal, if any.
    pub fn modal(&self) -> Option<&Modal> {
        self.modal.as_ref()
    }

    /// The armed-click interaction between Left Down and Left Up, if any.
    #[cfg(test)]
    pub fn armed(&self) -> Option<ArmedClick> {
        self.armed
    }

    /// The current responsive class (set by every draw).
    pub fn class(&self) -> ResponsiveClass {
        self.class
    }

    /// The current frame's hit regions.
    #[cfg(test)]
    pub fn regions(&self) -> &[HitRegion] {
        &self.regions
    }

    /// Hit-test one terminal cell against the current frame's regions
    /// (§13): only a rendered, currently valid control answers.
    pub fn hit_test(&self, column: u16, row: u16) -> Option<HitTarget> {
        let position = Position::new(column, row);
        self.regions
            .iter()
            .find(|region| region.area.contains(position))
            .map(|region| region.target)
    }

    /// Record the responsive class for this frame (§27) — the draw
    /// calls this before rendering, from the real terminal size.
    pub fn set_class(&mut self, class: ResponsiveClass) {
        self.class = class;
    }

    /// Publish the current frame's hit regions (§14). The view calls
    /// this once per draw, from the SAME layout decision it rendered
    /// from — there is no second geometry calculation to drift.
    pub fn publish_regions(&mut self, regions: Vec<HitRegion>) {
        self.regions = regions;
    }

    /// Presentation-only route change (§5): moves the active route,
    /// disarms any armed click (§17), and revalidates focus. It never
    /// touches the player, playback, DSP or Observation state.
    pub fn set_route(&mut self, route: TuiRoute) {
        if self.route == route {
            return;
        }
        self.route = route;
        self.invalidate_frame();
        self.validate_focus();
    }

    /// Invalidate the current frame's interactive geometry (§15/§29):
    /// drop the armed click and the hit regions. The next draw
    /// recomputes layout, revalidates focus and publishes fresh
    /// regions. Called on resize, route change and modal change.
    pub fn invalidate_frame(&mut self) {
        self.armed = None;
        self.regions.clear();
    }

    /// Drop the armed click (§17).
    pub fn disarm(&mut self) {
        self.armed = None;
    }

    /// The visible enabled focus targets in Tab order (§11): the four
    /// route tabs, then the active route's local controls. While a
    /// modal is open the cycle is exactly the modal's controls (§24) —
    /// the picker's field and listing; a listing with no rows leaves
    /// only the field. Below the minimum size there is nothing to
    /// focus (§28).
    pub fn focus_cycle(&self) -> Vec<FocusId> {
        if self.class == ResponsiveClass::Minimum {
            return Vec::new();
        }
        if let Some(modal) = self.modal() {
            return match modal {
                Modal::Open(picker) => {
                    let mut cycle = vec![FocusId::ModalField];
                    // The navigation button sits between the field and
                    // the listing (the popup's nav row); it is a stop
                    // even with no listing rows yet — in the modes
                    // where a folder can be the subject (G2: Add File
                    // has no [Use this folder]).
                    if picker.mode.allows_use_folder() {
                        cycle.push(FocusId::PickerButton(ModalButton::UseFolder));
                    }
                    if !picker.entries.is_empty() {
                        cycle.push(FocusId::PickerList);
                    }
                    if picker.mode.allows_open() {
                        cycle.push(FocusId::PickerButton(ModalButton::Open));
                    }
                    cycle.extend([
                        FocusId::PickerButton(ModalButton::Add),
                        FocusId::PickerButton(ModalButton::EnterFolder),
                        FocusId::PickerButton(ModalButton::Cancel),
                    ]);
                    cycle
                }
                // The confirmation's two buttons: the destructive
                // choice and the Cancel that starts focused (T0).
                Modal::Confirm { .. } => vec![
                    FocusId::PickerButton(ModalButton::Confirm),
                    FocusId::PickerButton(ModalButton::Cancel),
                ],
                // The preset menu (G3): one list, the cursor is the
                // focus inside it.
                Modal::Presets { .. } => vec![FocusId::PickerList],
                // The help overlay owns the keyboard and draws no
                // control: nothing is focused while it is open (its
                // keys are modal-scoped and never consult focus).
                Modal::Help { .. } => Vec::new(),
                Modal::GoTo { .. } => vec![FocusId::ModalField],
            };
        }
        let mut cycle: Vec<FocusId> = TuiRoute::ALL
            .iter()
            .map(|route| FocusId::RouteTab(*route))
            .collect();
        match self.route {
            TuiRoute::NowPlaying => {
                // The seek buttons exist in the cycle only while their
                // evidence exists — an inert control is never a focus
                // stop (§11).
                if self.relative_seek_available() {
                    cycle.extend(SEEK_BUTTONS.iter().map(|button| FocusId::Seek(*button)));
                }
                cycle.extend(TRANSPORT.iter().map(|button| FocusId::Transport(*button)));
                cycle.extend(
                    PREFERENCES
                        .iter()
                        .map(|button| FocusId::Preference(*button)),
                );
            }
            // The playlist toolbar exists whether or not the list has
            // rows (Add works on an empty list); the list itself is
            // focusable only while it has rows (§11). The summary
            // row's Order/Repeat toggles are drawn (and clickable) in
            // the non-compact classes, so they are focus stops there
            // too — a drawn control is always a keyboard-reachable
            // control.
            TuiRoute::Playlist => {
                cycle.extend(
                    PLAYLIST_BUTTONS
                        .iter()
                        .map(|button| FocusId::PlaylistButton(*button)),
                );
                if !self.playlist.is_empty() {
                    cycle.push(FocusId::Playlist);
                }
                if self.class != ResponsiveClass::Compact {
                    cycle.push(FocusId::Preference(PreferenceButton::Order));
                    cycle.push(FocusId::Preference(PreferenceButton::Repeat));
                }
            }
            // The Audio route (G3): the toolbar (the enablement label
            // is contextual but the CONTROL always exists), then the
            // ten band steppers in band order (− before +). The steppers
            // are route-local controls, always present — they edit the
            // draft, seeding it from the desired configuration on first
            // use.
            TuiRoute::Audio => {
                cycle.extend(
                    AUDIO_BUTTONS
                        .iter()
                        .map(|button| FocusId::AudioButton(*button)),
                );
                for band in 0..EQ_BAND_FREQUENCY_HZ.len() {
                    cycle.push(FocusId::EqBand {
                        band,
                        adjust: EqAdjust::Cut,
                    });
                    cycle.push(FocusId::EqBand {
                        band,
                        adjust: EqAdjust::Boost,
                    });
                }
            }
            // The Visualizer route (G4): three mode buttons. The
            // visualization itself arms nothing — it is a display of
            // Observation Plane telemetry, not a control.
            TuiRoute::Visualizer => {
                cycle.extend(
                    VISUALIZER_MODES
                        .iter()
                        .map(|mode| FocusId::VisualizerMode(*mode)),
                );
            }
        }
        // The nav bar's persistent application controls (G5) close the
        // cycle on every route: Help and Quit are always reachable,
        // after the route's own content.
        cycle.extend(NAV_BUTTONS.iter().map(|button| FocusId::NavBar(*button)));
        cycle
    }

    /// Tab / Shift+Tab: move focus to the next/previous visible enabled
    /// control, wrapping. Focus outside the cycle (or none) falls in at
    /// the cycle's edge.
    pub fn move_focus(&mut self, direction: FocusMove) {
        let cycle = self.focus_cycle();
        if cycle.is_empty() {
            self.focus = None;
            return;
        }
        let current = cycle.iter().position(|id| Some(*id) == self.focus);
        self.focus = Some(match current {
            Some(index) => match direction {
                FocusMove::Next => cycle[(index + 1) % cycle.len()],
                FocusMove::Previous => cycle[(index + cycle.len() - 1) % cycle.len()],
            },
            None => match direction {
                FocusMove::Next => cycle[0],
                FocusMove::Previous => cycle[cycle.len() - 1],
            },
        });
    }

    /// Validate focus against the visible enabled controls (§12): a
    /// focus that no longer exists falls back to the route's first
    /// meaningful local control, else the first tab. Invisible or
    /// off-screen focus is never retained.
    pub fn validate_focus(&mut self) {
        let cycle = self.focus_cycle();
        if cycle.iter().any(|id| Some(*id) == self.focus) {
            return;
        }
        let fallback = |id: &FocusId| {
            matches!(
                id,
                FocusId::Seek(_)
                    | FocusId::Transport(_)
                    | FocusId::Preference(_)
                    | FocusId::Playlist
                    | FocusId::PlaylistButton(_)
                    | FocusId::AudioButton(_)
                    | FocusId::EqBand { .. }
                    | FocusId::VisualizerMode(_)
                    | FocusId::ModalField
                    | FocusId::PickerList
                    | FocusId::PickerButton(_)
            )
        };
        self.focus = cycle
            .iter()
            .find(|id| fallback(id))
            .or_else(|| cycle.first())
            .copied();
    }

    /// The focus target a mouse hit on `target` selects (§17: Left
    /// Down focuses the target). `None` for targets that carry no
    /// keyboard focus of their own (the seek bar — a mouse affordance
    /// over an already keyboard-complete command).
    pub fn focus_of_target(target: HitTarget) -> Option<FocusId> {
        match target {
            HitTarget::RouteTab(route) => Some(FocusId::RouteTab(route)),
            HitTarget::Seek(button) => Some(FocusId::Seek(button)),
            HitTarget::Transport(button) => Some(FocusId::Transport(button)),
            HitTarget::Preference(button) => Some(FocusId::Preference(button)),
            HitTarget::PlaylistRow(_) | HitTarget::PlaylistPane => Some(FocusId::Playlist),
            HitTarget::PlaylistButton(button) => Some(FocusId::PlaylistButton(button)),
            HitTarget::AudioButton(button) => Some(FocusId::AudioButton(button)),
            HitTarget::EqBand { band, adjust } => Some(FocusId::EqBand { band, adjust }),
            HitTarget::VisualizerMode(mode) => Some(FocusId::VisualizerMode(mode)),
            HitTarget::NavBar(button) => Some(FocusId::NavBar(button)),
            HitTarget::PickerRow(_) => Some(FocusId::PickerList),
            HitTarget::ModalButton(button) => Some(FocusId::PickerButton(button)),
            HitTarget::ModalField => Some(FocusId::ModalField),
            HitTarget::SeekBar => None,
        }
    }

    /// The action activating `target` performs (§17: Left Up activates
    /// the armed target). `None` for targets that arm nothing: the
    /// playlist pane area, the path-field row, and the seek bar (its
    /// action is the click position, computed from the frame geometry
    /// at decode time).
    ///
    /// This is an INSTANCE method for one reason: the picker's `..`
    /// row is navigation chrome, not a target — activating it is the
    /// parent step, while every other listing row's activation is a
    /// cursor move. The answer needs the listing this frame shows.
    pub fn action_of_target(&self, target: HitTarget) -> Option<TuiAction> {
        match target {
            HitTarget::RouteTab(route) => Some(TuiAction::Navigate(route)),
            HitTarget::Seek(button) => Some(TuiAction::SeekRelative(match button {
                SeekButton::Back => -SEEK_STEP_SECS,
                SeekButton::Forward => SEEK_STEP_SECS,
            })),
            HitTarget::Transport(button) => Some(match button {
                TransportButton::Open => TuiAction::OpenModal(ModalKind::Open),
                TransportButton::Previous => TuiAction::Previous,
                TransportButton::PlayPause => TuiAction::PlayPause,
                TransportButton::Stop => TuiAction::Stop,
                TransportButton::Next => TuiAction::Next,
            }),
            HitTarget::Preference(button) => Some(match button {
                PreferenceButton::VolumeDown => TuiAction::VolumeDown,
                PreferenceButton::VolumeUp => TuiAction::VolumeUp,
                PreferenceButton::Order => TuiAction::ToggleOrder,
                PreferenceButton::Repeat => TuiAction::CycleRepeat,
            }),
            HitTarget::PlaylistRow(index) => {
                Some(TuiAction::PlaylistSelect(PlaylistCursor::Row(index)))
            }
            HitTarget::PlaylistButton(button) => Some(match button {
                PlaylistButton::AddFile => TuiAction::PlaylistAddFile,
                PlaylistButton::AddFolder => TuiAction::PlaylistAddFolder,
                PlaylistButton::PlaySelected => TuiAction::PlaylistPlaySelected,
                PlaylistButton::Remove => TuiAction::PlaylistRemove,
                PlaylistButton::Clear => TuiAction::PlaylistClear,
            }),
            HitTarget::AudioButton(button) => Some(match button {
                AudioButton::Enabled => TuiAction::DspToggleEnabled,
                AudioButton::PreampDown => TuiAction::DspPreampStep(-1),
                AudioButton::PreampUp => TuiAction::DspPreampStep(1),
                AudioButton::Presets => TuiAction::DspOpenPresets,
                AudioButton::Apply => TuiAction::DspApply,
                AudioButton::Cancel => TuiAction::DspCancel,
            }),
            HitTarget::EqBand { band, adjust } => Some(TuiAction::DspEqBandStep(
                band,
                match adjust {
                    EqAdjust::Cut => -1,
                    EqAdjust::Boost => 1,
                },
            )),
            HitTarget::VisualizerMode(mode) => Some(TuiAction::SetVisualizerMode(mode)),
            HitTarget::NavBar(button) => Some(match button {
                NavBarButton::Help => TuiAction::OpenModal(ModalKind::Help),
                // The same loop control the Q key produces (§7: keyboard
                // and mouse converge on one action vocabulary).
                NavBarButton::Quit => TuiAction::Quit,
            }),
            HitTarget::PickerRow(index) => Some(TuiAction::ModalInput(
                match self.picker_entries().get(index) {
                    // The synthesized `..` row activates the parent
                    // step — the same action as keyboard Enter/Backspace
                    // on it (G1 F04: the mouse converges on the same
                    // semantic action, never a separate machine).
                    Some(entry) if entry.is_parent => ModalInput::ListParent,
                    _ => ModalInput::ListMove(PlaylistCursor::Row(index)),
                },
            )),
            HitTarget::ModalButton(button) => Some(TuiAction::ModalInput(match button {
                ModalButton::Open => ModalInput::CommitOpen,
                ModalButton::Add => ModalInput::CommitAdd,
                ModalButton::UseFolder => ModalInput::UseFolder,
                ModalButton::EnterFolder => ModalInput::ListActivate,
                ModalButton::Confirm => ModalInput::CommitConfirm,
                ModalButton::Cancel => ModalInput::Cancel,
            })),
            // The pane area focuses the list but is not itself a
            // control.
            HitTarget::PlaylistPane => None,
            // The path-field row focuses the field but arms nothing.
            HitTarget::ModalField => None,
            // The seek bar arms like a control, but its action is the
            // click position — resolved by the mouse decoder, not by
            // the target alone.
            HitTarget::SeekBar => None,
        }
    }

    /// The action the currently focused control performs on Enter
    /// (§10). `None` when nothing is focused or the focus has no
    /// activation.
    pub fn activation(&self) -> Option<TuiAction> {
        match self.focus? {
            FocusId::RouteTab(route) => Some(TuiAction::Navigate(route)),
            FocusId::Seek(button) => self.action_of_target(HitTarget::Seek(button)),
            FocusId::Transport(button) => self.action_of_target(HitTarget::Transport(button)),
            FocusId::Preference(button) => self.action_of_target(HitTarget::Preference(button)),
            FocusId::PlaylistButton(button) => {
                self.action_of_target(HitTarget::PlaylistButton(button))
            }
            FocusId::AudioButton(button) => self.action_of_target(HitTarget::AudioButton(button)),
            FocusId::EqBand { band, adjust } => {
                self.action_of_target(HitTarget::EqBand { band, adjust })
            }
            FocusId::VisualizerMode(mode) => self.action_of_target(HitTarget::VisualizerMode(mode)),
            FocusId::NavBar(button) => self.action_of_target(HitTarget::NavBar(button)),
            FocusId::Playlist => Some(TuiAction::PlaylistPlaySelected),
            FocusId::PickerList => Some(TuiAction::ModalInput(ModalInput::ListActivate)),
            FocusId::PickerButton(button) => self.action_of_target(HitTarget::ModalButton(button)),
            FocusId::ModalField => Some(TuiAction::ModalInput(ModalInput::Confirm)),
        }
    }

    /// Open the one modal of `kind` (§24): captures input (the modal
    /// field becomes the focus), clears the armed mouse target, and
    /// remembers the route-local focus so a later close can restore it
    /// (§24). Opening while one is open replaces it — there is no
    /// stack. The Open modal starts as a fresh picker draft; the
    /// runtime supplies its first listing right after (the model
    /// performs no I/O).
    pub fn open_modal(&mut self, kind: ModalKind) {
        self.focus_before_modal = self.focus;
        self.modal = Some(match kind {
            ModalKind::Open | ModalKind::AddFile | ModalKind::AddFolder => {
                // One picker, three modes (T0 Add File/Folder policies):
                // navigation identical, commit buttons and subject kind
                // restricted per mode.
                let mode = match kind {
                    ModalKind::AddFile => PickerMode::AddFile,
                    ModalKind::AddFolder => PickerMode::AddFolder,
                    _ => PickerMode::OpenAny,
                };
                Modal::Open(OpenPicker {
                    mode,
                    input: String::new(),
                    dir: None,
                    entries: Vec::new(),
                    cursor: None,
                    error: None,
                    folder_target: false,
                })
            }
            ModalKind::GoTo => Modal::GoTo {
                input: String::new(),
            },
            ModalKind::Help => Modal::Help { scroll: 0 },
            // The preset menu (G3) starts its cursor on the preset the
            // draft (or, with no draft, the desired configuration)
            // currently matches — the menu answers "which preset am I
            // on" as honestly as "pick one".
            ModalKind::Presets => {
                let eq = match self.audio_draft.as_ref() {
                    Some(draft) => draft.config().eq,
                    None => self.desired_processing.as_ref().and_then(|c| c.eq),
                };
                let cursor = eq.and_then(|eq| {
                    EqPreset::all()
                        .iter()
                        .position(|preset| preset.to_config().eq == Some(eq))
                });
                self.focus = Some(FocusId::PickerList);
                Modal::Presets { cursor }
            }
            // T0 modal table: the destructive confirmation starts with
            // CANCEL focused — the dangerous choice is never the
            // default.
            ModalKind::ConfirmRemoveCurrent | ModalKind::ConfirmClear => {
                let confirm = match kind {
                    ModalKind::ConfirmRemoveCurrent => ConfirmKind::RemoveCurrent,
                    _ => ConfirmKind::Clear,
                };
                self.focus = Some(FocusId::PickerButton(ModalButton::Cancel));
                Modal::Confirm { kind: confirm }
            }
        });
        self.invalidate_frame();
        self.validate_focus();
    }

    /// Close the modal and restore a valid route focus (§24): the
    /// pre-modal focus when it still exists in the cycle, else the §12
    /// fallback (the route's first meaningful local control, else the
    /// first tab).
    pub fn close_modal(&mut self) {
        self.modal = None;
        self.invalidate_frame();
        if let Some(focus) = self.focus_before_modal.take() {
            self.focus = Some(focus);
        }
        self.validate_focus();
    }

    /// Apply one editing step to the active modal (a no-op when no
    /// text modal is open — Help has no field to edit). In the picker,
    /// editing the typed line is an edit of the TARGET: it clears the
    /// listing selection, so the submission subject can never be a row
    /// the user stopped looking at while the field shows something
    /// else (G1 F08 — one deterministic subject, the one on display).
    pub fn modal_edit(&mut self, input: ModalInput) {
        let Some(modal) = self.modal.as_mut() else {
            return;
        };
        let text = match modal {
            Modal::Open(picker) => {
                if matches!(input, ModalInput::Char(_) | ModalInput::Backspace) {
                    picker.cursor = None;
                    picker.folder_target = false;
                }
                &mut picker.input
            }
            Modal::GoTo { input } => input,
            // Help and the confirm modal have no text field to edit.
            Modal::Help { .. } | Modal::Confirm { .. } | Modal::Presets { .. } => return,
        };
        match input {
            ModalInput::Char(c) => text.push(c),
            ModalInput::Backspace => {
                text.pop();
            }
            ModalInput::Confirm
            | ModalInput::Cancel
            | ModalInput::ListActivate
            | ModalInput::ListMove(_)
            | ModalInput::ListParent
            | ModalInput::CommitOpen
            | ModalInput::CommitAdd
            | ModalInput::CommitConfirm
            | ModalInput::UseFolder
            | ModalInput::HelpScroll(_) => {}
        }
    }

    /// Replace the Open picker's listing with one runtime-supplied
    /// directory read (the model performs no I/O of its own). A
    /// readable directory becomes `..` (when one exists) plus the
    /// classified entries, each carrying its NATIVE path; an unreadable
    /// one keeps an honest diagnostic instead of a fabricated empty
    /// list. Either way the cursor starts unselected and the armed
    /// click dies with the old geometry (§17: a re-listed pane is new
    /// geometry).
    pub fn set_open_listing(
        &mut self,
        dir: std::path::PathBuf,
        listing: Result<Vec<crate::input::DirectoryEntry>, String>,
    ) {
        let Some(Modal::Open(picker)) = self.modal.as_mut() else {
            return;
        };
        picker.dir = Some(dir);
        picker.error = None;
        picker.cursor = None;
        picker.folder_target = false;
        picker.entries = match listing {
            Ok(entries) => {
                let mut rows: Vec<PickerEntry> = Vec::with_capacity(entries.len() + 1);
                if let Some(parent) = picker
                    .dir
                    .as_ref()
                    .and_then(|dir| dir.parent().map(|parent| parent.to_path_buf()))
                {
                    rows.push(PickerEntry {
                        name: "..".to_owned(),
                        is_dir: true,
                        is_parent: true,
                        path: parent,
                    });
                }
                rows.extend(entries.into_iter().map(|entry| PickerEntry {
                    name: entry.name,
                    is_dir: entry.is_dir,
                    is_parent: false,
                    path: entry.path,
                }));
                rows
            }
            Err(diagnostic) => {
                picker.error = Some(diagnostic);
                Vec::new()
            }
        };
        self.invalidate_frame();
        self.validate_focus();
    }

    /// The Open picker's listed entries, while one is shown.
    pub fn picker_entries(&self) -> &[PickerEntry] {
        match self.modal.as_ref() {
            Some(Modal::Open(picker)) => &picker.entries,
            _ => &[],
        }
    }

    /// Which final subject the active picker accepts (G2). `None` when
    /// no Open-picker modal is active.
    pub fn picker_mode(&self) -> Option<PickerMode> {
        match self.modal.as_ref() {
            Some(Modal::Open(picker)) => Some(picker.mode),
            _ => None,
        }
    }

    /// The Open picker's listed directory, while one is shown.
    pub fn open_picker_dir(&self) -> Option<&std::path::Path> {
        match self.modal.as_ref() {
            Some(Modal::Open(picker)) => picker.dir.as_deref(),
            _ => None,
        }
    }

    /// Record the picker's bounded diagnostic WITHOUT touching any
    /// other picker state: the typed line, the displayed directory, the
    /// listing and the selection all survive — the context a failed
    /// submission needs for correction/retry (G1 F11, T0: failure
    /// keeps the picker).
    pub fn set_picker_error(&mut self, message: String) {
        if let Some(Modal::Open(picker)) = self.modal.as_mut() {
            picker.error = Some(message);
        }
    }

    /// The [Use this folder] button (T0 picker freeze): the DISPLAYED
    /// directory becomes the submission subject, so a folder can be
    /// opened or added without entering it. The target note is what
    /// the popup shows while it stands; any navigation, listing
    /// refresh, field edit or row selection replaces the subject with
    /// a fresh one (the mutators above clear the flag).
    pub fn use_picker_folder(&mut self) {
        let Some(Modal::Open(picker)) = self.modal.as_mut() else {
            return;
        };
        if picker.dir.is_none() {
            return;
        }
        picker.folder_target = true;
        picker.error = None;
        // The popup's note row and the (now released) list geometry
        // change with the target: the next frame republishes.
        self.invalidate_frame();
    }

    /// Move the Open picker's listing cursor (§24 presentation). The
    /// first move from the unselected state enters at the list's edge
    /// in the pressed direction; a row click selects that row. A row
    /// selection replaces the [Use this folder] target — the subject
    /// is the row the user just chose.
    pub fn move_picker_cursor(&mut self, cursor: PlaylistCursor) {
        let Some(Modal::Open(picker)) = self.modal.as_mut() else {
            return;
        };
        if picker.entries.is_empty() {
            return;
        }
        picker.folder_target = false;
        picker.cursor = Some(match (picker.cursor, cursor) {
            (Some(current), PlaylistCursor::Previous) => current.saturating_sub(1),
            (Some(current), PlaylistCursor::Next) => (current + 1).min(picker.entries.len() - 1),
            (None, PlaylistCursor::Previous) => picker.entries.len() - 1,
            (None, PlaylistCursor::Next) | (None, PlaylistCursor::Row(0)) => 0,
            (None, PlaylistCursor::Row(index)) => index.min(picker.entries.len() - 1),
            (Some(_), PlaylistCursor::Row(index)) => index.min(picker.entries.len() - 1),
        });
        self.focus = Some(FocusId::PickerList);
    }

    /// The listing entry under the picker's cursor, with whether
    /// activating it is a directory step: a real directory (or the
    /// `..` row) navigates, a file is selected. The path is the entry's
    /// NATIVE identity (G1 F10) — for the `..` row, the parent
    /// directory.
    pub fn picker_cursor_entry(&self) -> Option<(&PickerEntry, std::path::PathBuf, bool)> {
        let Some(Modal::Open(picker)) = self.modal.as_ref() else {
            return None;
        };
        let index = picker.cursor?;
        let entry = picker.entries.get(index)?;
        Some((entry, entry.path.clone(), entry.is_parent || entry.is_dir))
    }

    /// The Open picker's ONE deterministic submission subject (G1 §8):
    /// the selected listing entry — with its native path — while one
    /// is selected; otherwise the typed path line, as typed (the
    /// runtime normalizes it against the displayed directory at commit
    /// time). Editing the field clears the selection, so the subject is
    /// always exactly what the surface shows (G1 F08). The [Use this
    /// folder] target outranks both while it stands: the displayed
    /// directory is the subject the button visibly chose, until a
    /// navigation, field edit or row selection replaces it.
    pub fn picker_subject(&self) -> PickerSubject {
        if let Some(Modal::Open(picker)) = self.modal.as_ref()
            && picker.folder_target
            && let Some(dir) = &picker.dir
        {
            return PickerSubject::Listing {
                path: dir.clone(),
                is_dir: true,
            };
        }
        if let Some((_entry, path, is_dir)) = self.picker_cursor_entry() {
            return PickerSubject::Listing { path, is_dir };
        }
        let Some(Modal::Open(picker)) = self.modal.as_ref() else {
            return PickerSubject::None;
        };
        if picker.input.is_empty() {
            return PickerSubject::None;
        }
        // The typed subject's kind is decided by the runtime's one
        // filesystem question at commit time, not guessed here.
        PickerSubject::Typed(picker.input.clone())
    }

    /// Confirm the active modal (§26): decides what the confirmation
    /// means and closes the modal — EXCEPT an unreadable GoTo token,
    /// which keeps the line open for correction and sends nothing.
    ///
    /// The Open picker's field is NOT confirmed here: its Enter is a
    /// navigate-or-select step (T0 picker freeze — the field line never
    /// starts playback), and the step's filesystem questions and its
    /// failure disposition live in the runtime's one dispatch boundary,
    /// where the modal closes only on success.
    pub fn confirm_modal(&mut self) -> ModalConfirm {
        match self.modal.as_mut() {
            Some(Modal::Help { .. }) => {
                self.close_modal();
                ModalConfirm::Nothing
            }
            Some(Modal::GoTo { input }) => {
                let line = std::mem::take(input);
                if line.is_empty() {
                    self.close_modal();
                    return ModalConfirm::Nothing;
                }
                match crate::cli::parse_seek_time(&line) {
                    Some(target) => {
                        self.close_modal();
                        ModalConfirm::Seek(target)
                    }
                    None => {
                        // The line stays OPEN for correction.
                        self.modal = Some(Modal::GoTo { input: line });
                        ModalConfirm::Unreadable("cannot read that time (try 95, 1:35 or 01:35.5)")
                    }
                }
            }
            Some(Modal::Confirm { kind }) => {
                // The destructive choice was activated (Enter on the
                // focused [Confirm] button). Close first — the modal
                // never stays over the operation it authorized — and
                // let the dispatch perform the frozen edit.
                let kind = *kind;
                self.close_modal();
                ModalConfirm::Confirm(kind)
            }
            Some(Modal::Open(_)) | Some(Modal::Presets { .. }) | None => ModalConfirm::Nothing,
        }
    }

    /// Type one character into the active text modal. Typing always
    /// edits the picker's path line, from wherever inside the modal the
    /// focus currently sits (type-through), and returns the focus to
    /// the field.
    pub fn modal_push(&mut self, c: char) {
        self.modal_edit(ModalInput::Char(c));
        if self.modal.is_some() {
            self.focus = Some(FocusId::ModalField);
        }
    }

    /// Backspace one character out of the active text modal.
    pub fn modal_backspace(&mut self) {
        self.modal_edit(ModalInput::Backspace);
    }

    /// Scroll the help overlay's content (G5). The offset counts lines
    /// from the top and saturates at zero; the view clamps the top end
    /// to what the popup actually shows. Opening help always starts at
    /// the top — the tour begins at the beginning.
    pub fn help_scroll(&mut self, scroll: HelpScroll) {
        let Some(Modal::Help { scroll: offset }) = self.modal.as_mut() else {
            return;
        };
        match scroll {
            HelpScroll::Up => *offset = offset.saturating_sub(1),
            HelpScroll::Down => *offset = offset.saturating_add(1),
            HelpScroll::PageUp => *offset = offset.saturating_sub(HELP_PAGE_LINES),
            HelpScroll::PageDown => *offset = offset.saturating_add(HELP_PAGE_LINES),
        }
    }

    /// The active modal's text content, while editing.
    #[cfg(test)]
    pub fn modal_line(&self) -> Option<&str> {
        self.modal.as_ref().and_then(Modal::input)
    }

    /// Test seam: place the keyboard focus directly. The real input
    /// paths set it through move_focus, validate_focus and the mouse
    /// decoder; some routing tests need a specific starting target.
    #[cfg(test)]
    pub(crate) fn set_focus(&mut self, focus: Option<FocusId>) {
        self.focus = focus;
    }
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use crate::tui::model::testutil::{key, model_with_regions, pending};
    use crate::tui::model::*;
    use crossterm::event::KeyCode;

    use crate::playlist::PlaybackOrder;
    use qianqian_playback::PlaybackSessionObservation;
    use std::time::Duration;

    /// The default route is Now Playing, and a route change moves only
    /// the route, the focus and the armed click — never any product
    /// state this model holds.
    #[test]
    fn route_change_moves_only_presentation_state() {
        let mut model = TuiModel::new("song.flac");
        model.set_order(PlaybackOrder::Shuffle);
        model.set_status(Some("feedback".to_owned()));
        model.set_playlist(3, || {
            vec![PlaylistRow {
                label: "a.flac".to_owned(),
                playing: true,
                selected: true,
            }]
        });
        let before = (
            model.order_label(),
            model.status().map(str::to_owned),
            model.playlist().len(),
        );

        assert_eq!(model.route(), TuiRoute::NowPlaying);
        model.set_route(TuiRoute::Playlist);
        assert_eq!(model.route(), TuiRoute::Playlist);

        assert_eq!(
            (
                model.order_label(),
                model.status().map(str::to_owned),
                model.playlist().len()
            ),
            before,
            "a route change is presentation-only"
        );
    }

    /// The focus cycle: tabs first, then the route's local controls; the
    /// modal collapses the cycle to its own field; below the minimum
    /// there is nothing to focus.
    #[test]
    fn the_focus_cycle_lists_tabs_then_route_local_controls() {
        let mut model = TuiModel::new("song.flac");
        model.set_class(ResponsiveClass::Normal);
        model.set_playlist(1, || {
            vec![PlaylistRow {
                label: "a.flac".to_owned(),
                playing: false,
                selected: false,
            }]
        });

        // Now Playing: four tabs, then the five transport buttons
        // (Open included), then the preference row, then the nav bar's
        // persistent Help/Quit (G5).
        model.set_route(TuiRoute::NowPlaying);
        assert_eq!(
            model.focus_cycle(),
            vec![
                FocusId::RouteTab(TuiRoute::NowPlaying),
                FocusId::RouteTab(TuiRoute::Playlist),
                FocusId::RouteTab(TuiRoute::Audio),
                FocusId::RouteTab(TuiRoute::Visualizer),
                FocusId::Transport(TransportButton::Open),
                FocusId::Transport(TransportButton::Previous),
                FocusId::Transport(TransportButton::PlayPause),
                FocusId::Transport(TransportButton::Stop),
                FocusId::Transport(TransportButton::Next),
                FocusId::Preference(PreferenceButton::VolumeDown),
                FocusId::Preference(PreferenceButton::VolumeUp),
                FocusId::Preference(PreferenceButton::Order),
                FocusId::Preference(PreferenceButton::Repeat),
                FocusId::NavBar(NavBarButton::Help),
                FocusId::NavBar(NavBarButton::Quit),
            ]
        );

        // Playlist: the toolbar first, then the list (it has rows),
        // then the summary row's Order/Repeat toggles (drawn in this
        // class), then the same persistent pair.
        model.set_route(TuiRoute::Playlist);
        assert_eq!(
            model.focus_cycle(),
            vec![
                FocusId::RouteTab(TuiRoute::NowPlaying),
                FocusId::RouteTab(TuiRoute::Playlist),
                FocusId::RouteTab(TuiRoute::Audio),
                FocusId::RouteTab(TuiRoute::Visualizer),
                FocusId::PlaylistButton(PlaylistButton::AddFile),
                FocusId::PlaylistButton(PlaylistButton::AddFolder),
                FocusId::PlaylistButton(PlaylistButton::PlaySelected),
                FocusId::PlaylistButton(PlaylistButton::Remove),
                FocusId::PlaylistButton(PlaylistButton::Clear),
                FocusId::Playlist,
                FocusId::Preference(PreferenceButton::Order),
                FocusId::Preference(PreferenceButton::Repeat),
                FocusId::NavBar(NavBarButton::Help),
                FocusId::NavBar(NavBarButton::Quit),
            ]
        );

        // Audio (G3): tabs + the six toolbar controls + the ten band
        // steppers (− and + per band) + the persistent pair.
        model.set_route(TuiRoute::Audio);
        assert_eq!(model.focus_cycle().len(), 32);
        model.set_route(TuiRoute::Visualizer);
        assert_eq!(
            model.focus_cycle().len(),
            9,
            "tabs + three mode buttons + the persistent pair"
        );

        // A modal collapses the cycle to its field (§24).
        model.open_modal(ModalKind::GoTo);
        assert_eq!(model.focus_cycle(), vec![FocusId::ModalField]);
        model.close_modal();

        // Below the minimum: nothing is focusable (§28).
        model.set_class(ResponsiveClass::Minimum);
        assert!(model.focus_cycle().is_empty());
        model.validate_focus();
        assert_eq!(model.focus(), None, "no invisible focus below minimum");
    }

    /// The nav bar's persistent pair (G5): Help and Quit close the
    /// focus cycle on every route and activate into the SAME actions
    /// their keys produce — and the §12 fallback never lands on them,
    /// because they are application chrome, not a route's first local
    /// control.
    #[test]
    fn the_nav_bar_buttons_focus_activate_and_never_catch_the_fallback() {
        let mut model = TuiModel::new("song.flac");
        model.set_class(ResponsiveClass::Normal);

        for route in TuiRoute::ALL {
            model.set_route(route);
            let cycle = model.focus_cycle();
            assert_eq!(
                cycle[cycle.len() - 2],
                FocusId::NavBar(NavBarButton::Help),
                "{route:?}"
            );
            assert_eq!(
                cycle[cycle.len() - 1],
                FocusId::NavBar(NavBarButton::Quit),
                "{route:?}"
            );
        }

        // Enter on the focused Help opens the overlay; Enter on Quit
        // is the quit action.
        model.set_route(TuiRoute::Visualizer);
        model.set_focus(Some(FocusId::NavBar(NavBarButton::Help)));
        assert_eq!(
            model.activation(),
            Some(TuiAction::OpenModal(ModalKind::Help))
        );
        model.set_focus(Some(FocusId::NavBar(NavBarButton::Quit)));
        assert_eq!(model.activation(), Some(TuiAction::Quit));

        // A focus that left the cycle falls to the route's first local
        // control, never to the persistent chrome.
        model.set_route(TuiRoute::NowPlaying);
        model.set_focus(Some(FocusId::Playlist));
        model.validate_focus();
        assert_eq!(
            model.focus(),
            Some(FocusId::Transport(TransportButton::Open)),
            "the fallback is the route's first local control"
        );
    }

    /// The help overlay's scroll (G5): it starts at the top, moves one
    /// line per step and one page per page step, saturates at the top,
    /// and reopening starts over. The text-edit path cannot move it —
    /// the overlay has no text field.
    #[test]
    fn the_help_overlay_scrolls_and_reopens_at_the_top() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::Help);
        let scroll = |model: &TuiModel| match model.modal() {
            Some(Modal::Help { scroll }) => *scroll,
            other => panic!("help modal expected, got {other:?}"),
        };
        assert_eq!(scroll(&model), 0, "the tour starts at the top");
        for _ in 0..3 {
            model.help_scroll(HelpScroll::Down);
        }
        assert_eq!(scroll(&model), 3);
        model.help_scroll(HelpScroll::Up);
        assert_eq!(scroll(&model), 2);
        for _ in 0..5 {
            model.help_scroll(HelpScroll::Up);
        }
        assert_eq!(scroll(&model), 0, "the top saturates");
        model.help_scroll(HelpScroll::PageDown);
        assert_eq!(scroll(&model), HELP_PAGE_LINES);
        model.help_scroll(HelpScroll::PageUp);
        assert_eq!(scroll(&model), 0);
        // The editing step is not a scroll step.
        model.modal_edit(ModalInput::HelpScroll(HelpScroll::PageDown));
        assert_eq!(scroll(&model), 0);
        model.close_modal();
        model.open_modal(ModalKind::Help);
        assert_eq!(scroll(&model), 0, "reopening starts over");
    }

    /// Tab/Shift+Tab walk the visible enabled controls with wraparound,
    /// and an empty list drops the list out of the cycle (§11).
    #[test]
    fn tab_moves_through_visible_enabled_controls_only() {
        let mut model = TuiModel::new("song.flac");
        model.set_class(ResponsiveClass::Normal);
        // An EMPTY playlist: the list is not focusable.
        model.set_route(TuiRoute::Playlist);
        model.set_playlist(1, Vec::new);
        model.validate_focus();

        model.validate_focus();
        assert_eq!(
            model.focus(),
            Some(FocusId::PlaylistButton(PlaylistButton::AddFile)),
            "fallback with an empty list is the toolbar's first button"
        );
        model.move_focus(FocusMove::Previous);
        assert_eq!(
            model.focus(),
            Some(FocusId::RouteTab(TuiRoute::Visualizer)),
            "Shift+Tab from the toolbar lands on the last tab"
        );
        // The cycle's other edge: Shift+Tab from the FIRST tab wraps
        // past the route content to the persistent Quit (G5).
        for _ in 0..3 {
            model.move_focus(FocusMove::Previous);
        }
        assert_eq!(model.focus(), Some(FocusId::RouteTab(TuiRoute::NowPlaying)));
        model.move_focus(FocusMove::Previous);
        assert_eq!(
            model.focus(),
            Some(FocusId::NavBar(NavBarButton::Quit)),
            "Shift+Tab from the first tab wraps to the nav bar's Quit"
        );

        // One row appears: the list joins the cycle. The focus on the
        // Quit button is still valid, so validation keeps it.
        model.set_playlist(2, || {
            vec![PlaylistRow {
                label: "a.flac".to_owned(),
                playing: false,
                selected: false,
            }]
        });
        model.validate_focus();
        assert_eq!(
            model.focus(),
            Some(FocusId::NavBar(NavBarButton::Quit)),
            "a still-valid focus is never moved by validation"
        );
        model.move_focus(FocusMove::Next);
        assert_eq!(
            model.focus(),
            Some(FocusId::RouteTab(TuiRoute::NowPlaying)),
            "Tab from the cycle's last stop wraps to the first tab"
        );
        for _ in 0..3 {
            model.move_focus(FocusMove::Next);
        }
        assert_eq!(model.focus(), Some(FocusId::RouteTab(TuiRoute::Visualizer)));
        model.move_focus(FocusMove::Next);
        assert_eq!(
            model.focus(),
            Some(FocusId::PlaylistButton(PlaylistButton::AddFile)),
            "the toolbar joined the cycle between the tabs and the list"
        );
        for _ in 0..5 {
            model.move_focus(FocusMove::Next);
        }
        assert_eq!(
            model.focus(),
            Some(FocusId::Playlist),
            "the list joined the cycle"
        );
        // The summary row's Order/Repeat toggles (drawn in this class)
        // and then the persistent pair (G5) sit between the route
        // content and the wrap.
        model.move_focus(FocusMove::Next);
        assert_eq!(
            model.focus(),
            Some(FocusId::Preference(PreferenceButton::Order))
        );
        model.move_focus(FocusMove::Next);
        assert_eq!(
            model.focus(),
            Some(FocusId::Preference(PreferenceButton::Repeat))
        );
        model.move_focus(FocusMove::Next);
        assert_eq!(model.focus(), Some(FocusId::NavBar(NavBarButton::Help)));
        model.move_focus(FocusMove::Next);
        assert_eq!(model.focus(), Some(FocusId::NavBar(NavBarButton::Quit)));
        model.move_focus(FocusMove::Next);
        assert_eq!(
            model.focus(),
            Some(FocusId::RouteTab(TuiRoute::NowPlaying)),
            "Tab wraps from the last control to the first"
        );

        // The §12 fallback: a focus that left the cycle (the list, after
        // it emptied again) lands on the first remaining local control —
        // the toolbar always exists, so that is its first button.
        model.set_focus(Some(FocusId::Playlist));
        model.set_playlist(3, Vec::new);
        model.validate_focus();
        assert_eq!(
            model.focus(),
            Some(FocusId::PlaylistButton(PlaylistButton::AddFile)),
            "an empty list drops the list from the cycle"
        );
        // On Now Playing the same fallback lands on the transport row,
        // the route's first local control.
        model.set_focus(Some(FocusId::Playlist));
        model.set_route(TuiRoute::NowPlaying);
        assert_eq!(
            model.focus(),
            Some(FocusId::Transport(TransportButton::Open)),
            "an out-of-cycle focus falls back to the route's first local control"
        );
    }

    /// The confirmation modal (G2, T0): Cancel starts focused, the
    /// cycle is exactly the two buttons, and the destructive choice is
    /// one explicit step away.
    #[test]
    fn the_confirm_modal_starts_cancel_focused() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::ConfirmRemoveCurrent);
        assert_eq!(
            model.focus(),
            Some(FocusId::PickerButton(ModalButton::Cancel)),
            "the dangerous choice is never the default (T0)"
        );
        assert_eq!(
            model.focus_cycle(),
            vec![
                FocusId::PickerButton(ModalButton::Confirm),
                FocusId::PickerButton(ModalButton::Cancel),
            ]
        );
        // Tab reaches the destructive choice; its activation is the
        // ONE confirm step the dispatch performs the edit through.
        model.move_focus(FocusMove::Next);
        assert_eq!(
            model.activation(),
            Some(TuiAction::ModalInput(ModalInput::CommitConfirm))
        );
        model.move_focus(FocusMove::Next);
        assert_eq!(
            model.focus(),
            Some(FocusId::PickerButton(ModalButton::Cancel)),
            "the two-button cycle wraps"
        );
    }

    // ------------------------------------------------------------------
    // Keyboard decoding tests.
    // ------------------------------------------------------------------

    /// At most one modal exists (§24): opening while one is open
    /// replaces it; closing restores a valid route focus.
    #[test]
    fn at_most_one_modal_exists_and_closing_restores_a_valid_focus() {
        let mut model = TuiModel::new("song.flac");
        model.set_class(ResponsiveClass::Normal);
        model.open_modal(ModalKind::Open);
        assert!(matches!(model.modal(), Some(Modal::Open { .. })));
        model.open_modal(ModalKind::Help);
        assert_eq!(
            model.modal().map(Modal::kind),
            Some(ModalKind::Help),
            "no modal stack"
        );
        // The help overlay owns the keyboard and draws no control:
        // nothing holds focus while it is open.
        assert_eq!(model.focus(), None);
        assert_eq!(model.armed(), None, "opening clears the armed click (§24)");

        model.close_modal();
        assert_eq!(model.modal(), None);
        assert_eq!(
            model.focus(),
            Some(FocusId::Transport(TransportButton::Open)),
            "closing restores the route's first meaningful local focus (§24)"
        );
    }

    /// The Open picker's field is a text line that never COMMITS (T0
    /// picker freeze, G1 F05): its Enter is the runtime's
    /// navigate-or-select step, so the model's confirm answers Nothing
    /// for the picker and the modal stays open — only Esc (or the
    /// runtime's success path) closes it.
    #[test]
    fn the_open_picker_field_edits_and_never_commits_from_the_model() {
        let mut model = TuiModel::new("song.flac");
        assert_eq!(model.modal(), None);

        model.open_modal(ModalKind::Open);
        assert_eq!(model.modal_line(), Some(""));
        for c in "/media/b.flac".chars() {
            model.modal_push(c);
        }
        assert_eq!(model.modal_line(), Some("/media/b.flac"));
        model.modal_backspace();
        assert_eq!(model.modal_line(), Some("/media/b.fla"));

        // The model's confirm is a no-op for the picker: the field
        // never starts playback, and the modal stays for the runtime's
        // navigate-or-select handling.
        assert_eq!(model.confirm_modal(), ModalConfirm::Nothing);
        assert!(model.modal().is_some(), "the picker stays open");
        assert_eq!(model.modal_line(), Some("/media/b.fla"), "the line stays");

        // Editing primitives are inert without an open text modal.
        model.close_modal();
        model.modal_push('x');
        assert_eq!(model.modal(), None);
    }

    /// The GoTo modal parses with the EXISTING shared reader, closes on
    /// a readable token, and an unreadable token keeps the line OPEN
    /// and sends nothing (Issue #166 §27).
    #[test]
    fn the_goto_modal_parses_with_the_shared_reader_and_fails_open_for_correction() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::GoTo);
        for c in "01:35.5".chars() {
            model.modal_push(c);
        }
        assert_eq!(
            model.confirm_modal(),
            ModalConfirm::Seek(Duration::from_millis(95_500)),
            "the shared reader owns the time grammar"
        );
        assert_eq!(model.modal(), None);

        // Plain seconds and mm:ss read the same way.
        for (text, expected) in [
            ("95", Duration::from_secs(95)),
            ("1:35", Duration::from_secs(95)),
        ] {
            let mut model = TuiModel::new("song.flac");
            model.open_modal(ModalKind::GoTo);
            for c in text.chars() {
                model.modal_push(c);
            }
            assert_eq!(model.confirm_modal(), ModalConfirm::Seek(expected));
        }

        for bad in ["abc", "1:99", "-30", "nan", "inf", "1e400"] {
            let mut model = TuiModel::new("song.flac");
            model.open_modal(ModalKind::GoTo);
            for c in bad.chars() {
                model.modal_push(c);
            }
            let ModalConfirm::Unreadable(diagnostic) = model.confirm_modal() else {
                panic!("{bad:?} must not parse into a seek target");
            };
            assert!(!diagnostic.is_empty());
            assert!(
                model.modal().is_some(),
                "{bad:?}: the line stays open for correction"
            );
            model.close_modal();
            assert_eq!(model.modal(), None);
        }

        // An empty line is a cancel.
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::GoTo);
        assert_eq!(model.confirm_modal(), ModalConfirm::Nothing);
        assert_eq!(model.modal(), None);
    }

    // ------------------------------------------------------------------
    // Mouse decoding tests (armed-click rule, wheel policy, §16–§25).
    // These run against REAL published geometry from the view draw, so
    // a layout regression breaks here first.
    // ------------------------------------------------------------------

    /// G1 F16 (§24): a modal close restores the route-local focus the
    /// modal interrupted — when that focus still exists; an obsolete
    /// one falls back to §12.
    #[test]
    fn a_modal_close_restores_the_prior_route_focus() {
        let mut model = model_with_regions(100, 30, TuiRoute::NowPlaying);
        model.set_focus(Some(FocusId::Preference(PreferenceButton::Repeat)));
        model.open_modal(ModalKind::Open);
        assert_eq!(model.focus(), Some(FocusId::ModalField));
        model.close_modal();
        assert_eq!(
            model.focus(),
            Some(FocusId::Preference(PreferenceButton::Repeat)),
            "close restores the interrupted route focus (§24)"
        );

        // A focus that no longer exists after the close falls back.
        model.open_modal(ModalKind::GoTo);
        model.set_route(TuiRoute::Audio);
        model.close_modal();
        assert_eq!(
            model.focus(),
            Some(FocusId::AudioButton(
                crate::tui::model::AudioButton::Enabled
            )),
            "an obsolete restored focus falls back to §12: the route's
             first local control (G3 gave the Audio route a toolbar)"
        );
    }

    /// G1 F07 (model side): the seek buttons join the focus cycle only
    /// while the episode publishes position + rate — the evidence a
    /// relative seek is computed from. Duration has no say here; the
    /// click-to-position bar has its own, separate gate.
    #[test]
    fn the_seek_buttons_hold_focus_stops_only_with_position_and_rate() {
        let mut model = TuiModel::new("song.flac");
        model.update(pending());
        model.set_class(ResponsiveClass::Normal);
        let cycle = model.focus_cycle();
        assert!(
            !cycle.iter().any(|id| matches!(id, FocusId::Seek(_))),
            "no seek stops without evidence"
        );

        // Position + rate: the two stops exist, before the transport.
        model.update(PlaybackSessionObservation {
            source_format: Some(qianqian_audio_api::ports::PcmFormat {
                sample_rate: 44_100,
                channels: 2,
                channel_mask: 0x3,
            }),
            position: Some(44_100),
            ..pending()
        });
        let cycle = model.focus_cycle();
        let seek_positions: Vec<usize> = cycle
            .iter()
            .enumerate()
            .filter_map(|(index, id)| matches!(id, FocusId::Seek(_)).then_some(index))
            .collect();
        assert_eq!(seek_positions, vec![4, 5], "two stops after the four tabs");
        model.focus = Some(FocusId::Seek(SeekButton::Forward));
        assert_eq!(
            model.activation(),
            Some(TuiAction::SeekRelative(SEEK_STEP_SECS)),
            "activation is the SAME frozen relative seek"
        );

        // The rate disappears: the stops go with it.
        model.update(PlaybackSessionObservation {
            source_format: None,
            position: Some(44_100),
            ..pending()
        });
        assert!(!model.relative_seek_available());
    }

    /// The T0 picker freeze's [Use this folder]: the DISPLAYED
    /// directory becomes the submission subject, outranking any typed
    /// line or row selection; a field edit, a row selection or a
    /// navigation replaces it.
    #[test]
    fn use_this_folder_makes_the_displayed_directory_the_subject() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::Open);
        model.set_open_listing(
            std::path::PathBuf::from("/media/albums"),
            Ok(vec![crate::input::DirectoryEntry {
                name: "b.flac".to_owned(),
                is_dir: false,
                path: std::path::PathBuf::from("/media/albums/b.flac"),
            }]),
        );

        // Select a row AND type a line: the folder target still wins.
        model.move_picker_cursor(PlaylistCursor::Next);
        model.modal_push('x');
        model.use_picker_folder();
        assert_eq!(
            model.picker_subject(),
            PickerSubject::Listing {
                path: std::path::PathBuf::from("/media/albums"),
                is_dir: true,
            },
            "the displayed directory is the subject"
        );

        // A field edit replaces it (the typed line is the subject).
        model.modal_push('y');
        assert_eq!(
            model.picker_subject(),
            PickerSubject::Typed("xy".to_owned()),
            "editing the field replaces the folder target"
        );

        // So does a row selection; and a navigation resets everything.
        model.use_picker_folder();
        model.move_picker_cursor(PlaylistCursor::Row(1));
        assert_eq!(
            model.picker_subject(),
            PickerSubject::Listing {
                path: std::path::PathBuf::from("/media/albums/b.flac"),
                is_dir: false,
            },
            "a row selection replaces the folder target"
        );
        model.use_picker_folder();
        model.set_open_listing(
            std::path::PathBuf::from("/media"),
            Ok(vec![crate::input::DirectoryEntry {
                name: "c.flac".to_owned(),
                is_dir: false,
                path: std::path::PathBuf::from("/media/c.flac"),
            }]),
        );
        assert_eq!(
            model.picker_subject(),
            PickerSubject::Typed("xy".to_owned()),
            "a navigation cleared the (stale) folder target; the user's \
             typed line — what the field shows — is the subject again"
        );
    }

    /// The picker's keyboard grammar (G1 §8): Tab cycles field → list
    /// → commit buttons; ↑/↓ move the listing cursor and hand focus to
    /// the list; typing always edits the field and returns focus to it;
    /// Backspace edits on the field and steps to the parent on the
    /// list; Enter acts on the focused control.
    #[test]
    fn the_open_picker_keyboard_grammar_walks_field_list_and_buttons() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::Open);
        model.set_open_listing(
            std::path::PathBuf::from("/media"),
            Ok(vec![
                crate::input::DirectoryEntry {
                    name: "album".to_owned(),
                    is_dir: true,
                    path: std::path::PathBuf::from("/media/album"),
                },
                crate::input::DirectoryEntry {
                    name: "b.flac".to_owned(),
                    is_dir: false,
                    path: std::path::PathBuf::from("/media/b.flac"),
                },
            ]),
        );
        // The `..` parent row is synthesized in front of the listing.
        assert_eq!(model.modal().map(Modal::kind), Some(ModalKind::Open));
        assert_eq!(model.focus(), Some(FocusId::ModalField));

        // Down: into the list, first row (the parent row).
        assert_eq!(
            decode_key(key(KeyCode::Down), &model),
            Some(TuiAction::ModalInput(ModalInput::ListMove(
                PlaylistCursor::Next
            )))
        );
        model.move_picker_cursor(PlaylistCursor::Next);
        assert_eq!(model.focus(), Some(FocusId::PickerList));

        // Typing from the list is type-through: the field edits and
        // regains focus.
        assert_eq!(
            decode_key(key(KeyCode::Char('x')), &model),
            Some(TuiAction::ModalInput(ModalInput::Char('x')))
        );
        model.modal_push('x');
        assert_eq!(model.modal_line(), Some("x"));
        assert_eq!(model.focus(), Some(FocusId::ModalField));

        // Tab walks field → [Use this folder] → list → Open → Add →
        // Enter folder → Cancel → field.
        for expected in [
            FocusId::PickerButton(ModalButton::UseFolder),
            FocusId::PickerList,
            FocusId::PickerButton(ModalButton::Open),
            FocusId::PickerButton(ModalButton::Add),
            FocusId::PickerButton(ModalButton::EnterFolder),
            FocusId::PickerButton(ModalButton::Cancel),
            FocusId::ModalField,
        ] {
            model.move_focus(FocusMove::Next);
            assert_eq!(model.focus(), Some(expected));
        }

        // Backspace on the LIST is the parent step, on the FIELD an
        // edit. Shift+Tab walks back: field → Cancel → Enter folder →
        // Add → Open → list.
        model.move_focus(FocusMove::Previous);
        assert_eq!(
            model.focus(),
            Some(FocusId::PickerButton(ModalButton::Cancel))
        );
        model.move_focus(FocusMove::Previous);
        assert_eq!(
            model.focus(),
            Some(FocusId::PickerButton(ModalButton::EnterFolder))
        );
        model.move_focus(FocusMove::Previous);
        assert_eq!(model.focus(), Some(FocusId::PickerButton(ModalButton::Add)));
        model.move_focus(FocusMove::Previous);
        assert_eq!(
            model.focus(),
            Some(FocusId::PickerButton(ModalButton::Open))
        );
        model.move_focus(FocusMove::Previous);
        assert_eq!(model.focus(), Some(FocusId::PickerList));
        assert_eq!(
            decode_key(key(KeyCode::Backspace), &model),
            Some(TuiAction::ModalInput(ModalInput::ListParent))
        );
        model.set_focus(Some(FocusId::ModalField));
        assert_eq!(
            decode_key(key(KeyCode::Backspace), &model),
            Some(TuiAction::ModalInput(ModalInput::Backspace))
        );

        // Enter follows the focus: list activates, a button commits or
        // navigates, the field confirms (navigate-or-select).
        assert_eq!(
            decode_key(key(KeyCode::Enter), &model),
            Some(TuiAction::ModalInput(ModalInput::Confirm))
        );
        model.set_focus(Some(FocusId::PickerButton(ModalButton::Add)));
        assert_eq!(
            decode_key(key(KeyCode::Enter), &model),
            Some(TuiAction::ModalInput(ModalInput::CommitAdd))
        );
        model.set_focus(Some(FocusId::PickerList));
        assert_eq!(
            decode_key(key(KeyCode::Enter), &model),
            Some(TuiAction::ModalInput(ModalInput::ListActivate))
        );
        model.set_focus(Some(FocusId::PickerButton(ModalButton::EnterFolder)));
        assert_eq!(
            decode_key(key(KeyCode::Enter), &model),
            Some(TuiAction::ModalInput(ModalInput::ListActivate)),
            "the mouse's [Enter folder] is the list's Enter: one action"
        );
    }

    /// The cursor movement rule: from unselected, ↓ enters at the top
    /// and ↑ at the bottom; a row click selects that row; the cursor
    /// clamps at the edges. The `..` row's NATIVE path is the parent
    /// directory (G1 F10).
    #[test]
    fn the_picker_cursor_moves_and_clamps() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::Open);
        let feed = Ok(vec![
            crate::input::DirectoryEntry {
                name: "d1".to_owned(),
                is_dir: true,
                path: std::path::PathBuf::from("/media/d1"),
            },
            crate::input::DirectoryEntry {
                name: "f1.flac".to_owned(),
                is_dir: false,
                path: std::path::PathBuf::from("/media/f1.flac"),
            },
        ]);
        model.set_open_listing(std::path::PathBuf::from("/media"), feed);
        // Entries: [.., d1, f1.flac].
        model.move_picker_cursor(PlaylistCursor::Next);
        assert_eq!(
            model.picker_cursor_entry().map(|(_entry, path, _)| path),
            Some(std::path::PathBuf::from("/")),
            "the first ↓ lands on the `..` row, which resolves to the PARENT"
        );
        model.move_picker_cursor(PlaylistCursor::Next);
        model.move_picker_cursor(PlaylistCursor::Next);
        assert_eq!(
            model.picker_cursor_entry().map(|(_entry, path, _)| path),
            Some(std::path::PathBuf::from("/media/f1.flac")),
        );
        model.move_picker_cursor(PlaylistCursor::Next);
        assert_eq!(
            model.picker_cursor_entry().map(|(_entry, path, _)| path),
            Some(std::path::PathBuf::from("/media/f1.flac")),
            "the cursor clamps at the last row"
        );
        model.move_picker_cursor(PlaylistCursor::Row(1));
        assert_eq!(
            model.picker_cursor_entry().map(|(_entry, path, _)| path),
            Some(std::path::PathBuf::from("/media/d1")),
            "a row click selects that row"
        );
        model.move_picker_cursor(PlaylistCursor::Previous);
        assert_eq!(
            model.picker_cursor_entry().map(|(_entry, path, _)| path),
            Some(std::path::PathBuf::from("/")),
            "activating `..` ascends"
        );
    }

    /// The ONE deterministic submission subject (G1 §8/F08): the
    /// selected row while one is selected; editing the field CLEARS
    /// the selection, so the subject becomes the typed line — never a
    /// stale row behind a freshly edited target.
    #[test]
    fn the_picker_subject_prefers_the_selection_and_an_edit_clears_it() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::Open);
        assert_eq!(model.picker_subject(), PickerSubject::None);
        model.modal_push('/');

        // No selection: the typed line is the subject, as typed (the
        // runtime normalizes it against the displayed directory).
        assert_eq!(model.picker_subject(), PickerSubject::Typed("/".to_owned()),);

        // The listing is rebuilt (which resets the cursor); selecting
        // a row makes the SELECTION the subject, with its native path.
        model.set_open_listing(
            std::path::PathBuf::from("/media"),
            Ok(vec![crate::input::DirectoryEntry {
                name: "b.flac".to_owned(),
                is_dir: false,
                path: std::path::PathBuf::from("/media/b.flac"),
            }]),
        );
        model.move_picker_cursor(PlaylistCursor::Row(1));
        assert_eq!(
            model.picker_subject(),
            PickerSubject::Listing {
                path: std::path::PathBuf::from("/media/b.flac"),
                is_dir: false,
            },
            "the selected row — its native identity — is the subject"
        );

        // Editing the field invalidates the stale row selection: the
        // subject becomes exactly what the field now shows.
        model.modal_push('x');
        assert_eq!(
            model.picker_subject(),
            PickerSubject::Typed("/x".to_owned()),
            "an edited field is the subject; the old row cannot hide behind it"
        );
        assert!(
            model.picker_cursor_entry().is_none(),
            "the cleared selection is visible: no row is marked"
        );
    }

    /// Below the minimum size the shell paints no popup (§28), so the
    /// modal's keys go inert except the cancel: a blind Enter behind
    /// an invisible picker must never commit a real Open.
    #[test]
    fn a_modal_below_the_minimum_only_cancels() {
        let mut model = TuiModel::new("song.flac");
        model.open_modal(ModalKind::Open);
        model.modal_push('x');
        model.set_class(ResponsiveClass::Minimum);
        assert_eq!(
            decode_key(key(KeyCode::Enter), &model),
            None,
            "no blind commit behind an invisible popup"
        );
        assert_eq!(decode_key(key(KeyCode::Char('a')), &model), None);
        assert_eq!(
            decode_key(key(KeyCode::Esc), &model),
            Some(TuiAction::ModalInput(ModalInput::Cancel)),
            "Esc stays the honest way out"
        );
    }
}
