//! The Open picker's operations: the modal's editing steps and the
//! filesystem questions they need — navigation, normalization, the
//! commit dispositions and their busy-interval drains. I/O lives here
//! in the runtime; the model stays pure.

use std::path::Path;

use crate::player::{EpisodeStart, OpenOutcome, ReferencePlayerApp};

use super::dispatch::{perform_clear_playlist, perform_remove_selected};
use super::drain_busy_interval_input;
use crate::tui::model::{
    ConfirmKind, Modal, ModalConfirm, ModalInput, ModalKind, PickerSubject, PlaylistCursor,
    TuiModel, TuiRoute,
};

/// One editing step inside the active modal (§24/§26/§31). The modal
/// owns its keys, so nothing here can also reach a background control:
/// a cancel closes and restores a valid route focus, and a commit
/// performs THIS modal's operation — never both a modal action and a
/// background action in one event.
///
/// The picker's submission disposition (G1 F11/F12, T0 freeze): the
/// modal closes ONLY on a successful commit — an Open that started, an
/// Add that appended. A refused/unreadable subject keeps the picker
/// (the typed line, the displayed directory, the listing and the
/// selection all survive) with a bounded diagnostic, so the failed
/// attempt is correctable; FailStop rides the same retention (the
/// latched state refuses again, honestly). Success routes exactly as
/// T0 freezes it: Open-file -> Now Playing, Open-folder -> Playlist,
/// Add -> Playlist.
pub(super) fn handle_modal_input<S: EpisodeStart>(
    input: ModalInput,
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
) {
    match input {
        ModalInput::Char(c) => model.modal_push(c),
        ModalInput::Backspace => model.modal_backspace(),
        ModalInput::Cancel => model.close_modal(),
        // The help overlay's content scroll (G5): presentation inside
        // the modal, nothing behind it.
        ModalInput::HelpScroll(scroll) => model.help_scroll(scroll),
        // The [Use this folder] button (T0 picker freeze): the
        // displayed directory becomes the subject — pure presentation,
        // nothing is committed until an explicit commit button.
        ModalInput::UseFolder => model.use_picker_folder(),
        // The picker listing's cursor: presentation only, like the
        // playlist pane's selection.
        // The preset menu's list (G3) moves its own cursor; the
        // filesystem picker's grammar is unchanged in its own modes.
        ModalInput::ListMove(cursor) => {
            if matches!(
                model.modal(),
                Some(crate::tui::model::Modal::Presets { .. })
            ) {
                model.move_presets_cursor(cursor);
            } else {
                model.move_picker_cursor(cursor);
            }
        }
        // Enter on the listing (T0 picker freeze): a directory — or the
        // `..` row — navigates; a FILE is SELECTED, never committed.
        // Final Open/Add requires activating an explicit button, so no
        // row gesture can start playback by accident (G1 F05).
        // Activating the preset menu's cursor is the WHOLE-CONFIGURATION
        // preset operation (T0): the App's own preset seam replaces the
        // desired configuration — processing on, unity preamp, the
        // preset's Q/trims. The popup announced that consequence before
        // this Enter.
        ModalInput::ListActivate
            if matches!(
                model.modal(),
                Some(crate::tui::model::Modal::Presets { .. })
            ) =>
        {
            if let Some(preset) = model.activate_preset_selection() {
                match player.set_eq_preset(preset) {
                    Ok(()) => {
                        let discarded = model.audio_note_preset_committed();
                        model.note_desired_processing(player.desired_processing());
                        model.set_status(Some(format!(
                            "preset {} recorded: processing on, unity preamp, {} trims \
                             (applied state not reported){}",
                            preset.name(),
                            preset.name(),
                            if discarded {
                                "; open drafts discarded"
                            } else {
                                ""
                            }
                        )));
                    }
                    Err(refusal) => {
                        model.set_status(Some(format!("preset refused: {refusal}")));
                    }
                }
            }
        }
        ModalInput::ListActivate => match model.picker_cursor_entry() {
            Some((_entry, path, true)) => navigate_picker_to(model, &path),
            Some((_entry, _path, false)) => {
                // Selection only: the cursor is already on this row;
                // the row itself asserts nothing further. The commit
                // buttons are the only way forward.
            }
            None => {}
        },
        // Backspace on the listing: up to the parent directory. From
        // a root there is no parent and the step is inert.
        ModalInput::ListParent => {
            if let Some(parent) = model
                .open_picker_dir()
                .and_then(|dir| dir.parent().map(|parent| parent.to_path_buf()))
            {
                navigate_picker_to(model, &parent);
            }
        }
        // The [Open] button: the ONE subject (selection, else typed
        // line) committed as a folder or file Open. A directory subject
        // commits as a folder Open (the shared expansion seeds the
        // list) — it does not navigate; row gestures navigate, the
        // button commits.
        ModalInput::CommitOpen => {
            commit_picker_subject(model, player, PickerDisposition::Open);
        }
        // The [Add] button: the same subject, appended through the same
        // shared expansion/probe admission — no candidate is activated
        // (T1A A1/A2 semantics). In the Add File / Add Folder modes the
        // subject kind is restricted (G2, T0 policies).
        ModalInput::CommitAdd => {
            commit_picker_subject(model, player, PickerDisposition::Add);
        }
        // The confirmation's destructive choice (G2, T0 owner
        // decision): the modal already closed; perform the frozen edit
        // — retire first, then mutate the list.
        ModalInput::CommitConfirm => match model.confirm_modal() {
            ModalConfirm::Confirm(ConfirmKind::RemoveCurrent) => {
                perform_remove_selected(model, player);
            }
            ModalConfirm::Confirm(ConfirmKind::Clear) => {
                perform_clear_playlist(model, player);
            }
            _ => {}
        },
        // Enter on the field (T0 picker freeze): navigate a directory,
        // select a named file, or stay with a bounded diagnostic. It
        // NEVER starts playback, and an unreadable path keeps the
        // picker for correction (G1 F05/F09/F11).
        ModalInput::Confirm => match model.modal().map(Modal::kind) {
            Some(ModalKind::Open | ModalKind::AddFile | ModalKind::AddFolder) => {
                confirm_picker_field(model)
            }
            _ => match model.confirm_modal() {
                ModalConfirm::Nothing | ModalConfirm::Confirm(_) => {}
                ModalConfirm::Seek(target) => match player.active_handle() {
                    Some(handle) => {
                        // The SAME frozen seek command the arrows use
                        // (Issue #166 §27): the episode's own
                        // clamp/refusal contract decides the landing.
                        handle.request_seek(target);
                        model.set_status(Some(format!(
                            "seek requested: {}",
                            crate::status::format_clock(target)
                        )));
                    }
                    // Nothing to seek: the parsed target is dropped
                    // rather than sent into a nonexistent episode.
                    None => model.set_status(Some("seek: no episode".to_owned())),
                },
                ModalConfirm::Unreadable(diagnostic) => {
                    model.set_status(Some(diagnostic.to_owned()))
                }
            },
        },
    }
}

/// What committing the picker's subject was asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerDisposition {
    Open,
    Add,
}

/// Resolve and commit the picker's ONE subject for one disposition.
/// The subject resolution (G1 F08/F09/F10): a listing selection
/// commits its NATIVE path unchanged; the typed line is normalized
/// against the DISPLAYED directory (relative base, leading `~`), never
/// against the process working directory of the moment. A subject that
/// cannot be normalized keeps the picker with its diagnostic; a
/// well-formed one runs the shared expansion/admission path, and the
/// modal closes only when the operation actually succeeded.
fn commit_picker_subject<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
    disposition: PickerDisposition,
) {
    let nothing_message = match disposition {
        PickerDisposition::Open => "nothing picked to open",
        PickerDisposition::Add => "nothing picked to add",
    };
    // The one filesystem question the typed subject needs (its
    // directory-or-file kind for the success routing); the LISTING
    // subject already knows its kind.
    let resolved = match model.picker_subject() {
        PickerSubject::None => {
            model.set_status(Some(nothing_message.to_owned()));
            return;
        }
        PickerSubject::Listing { path, is_dir } => Some((path, is_dir)),
        PickerSubject::Typed(line) => match picker_typed_path(model, &line) {
            Ok(path) => {
                let is_dir = std::fs::symlink_metadata(&path)
                    .map(|metadata| metadata.is_dir())
                    .unwrap_or(false);
                Some((path, is_dir))
            }
            Err(diagnostic) => {
                // Unresolvable typed line: nothing was touched, the
                // picker stays for correction (G1 F11).
                model.set_picker_error(diagnostic);
                return;
            }
        },
    };
    let Some((path, is_dir)) = resolved else {
        return;
    };
    // The Add File / Add Folder modes restrict the final subject kind
    // (G2, the T0 Add policies): a wrong-kind subject keeps the picker
    // with its bounded refusal — nothing was touched.
    if disposition == PickerDisposition::Add
        && let Some(mode) = model.picker_mode()
        && !mode.accepts(is_dir)
    {
        model.set_status(Some(mode.subject_refusal().to_owned()));
        return;
    }
    // The synchronous expansion/admission can stall this thread for as
    // long as the operation takes; the busy interval publishes no
    // interactive geometry — stale regions and any armed press die
    // here, and the input that arrived during the stall is dropped, so
    // nothing queued in the busy interval can decode against the
    // freshly published geometry after the return (T0 picker freeze).
    model.invalidate_frame();
    match disposition {
        PickerDisposition::Open => {
            let outcome = perform_open(model, player, &path);
            if matches!(outcome, Some(OpenOutcome::Opened)) {
                model.note_episode_replacement();
                model.close_modal();
                // The frozen success routing (T0): a file goes to Now
                // Playing; a folder goes to Playlist with its first
                // candidate already started.
                model.set_route(if is_dir {
                    TuiRoute::Playlist
                } else {
                    TuiRoute::NowPlaying
                });
            }
            // Any other outcome: the picker stays, the diagnostic is
            // on the status block, the context is intact for retry.
        }
        PickerDisposition::Add => {
            let added = perform_add(model, player, &path);
            if let Ok(_count) = added {
                model.close_modal();
                model.set_route(TuiRoute::Playlist);
            }
        }
    }
    drain_busy_interval_input();
}
/// Enter on the picker's path field (T0): navigate a directory or
/// select a named file — never a commit. The typed line is normalized
/// against the displayed directory (G1 F09); a file inside the listing
/// is selected by moving the cursor onto its row (visible, and it
/// becomes the subject); a file outside it is revealed by listing its
/// parent directory with the file row selected. An unreadable path
/// keeps the picker with a bounded diagnostic (G1 F11).
fn confirm_picker_field(model: &mut TuiModel) {
    let line = match model.modal() {
        Some(Modal::Open(picker)) if !picker.input.is_empty() => picker.input.clone(),
        _ => return,
    };
    let path = match picker_typed_path(model, &line) {
        Ok(path) => path,
        Err(diagnostic) => {
            model.set_picker_error(diagnostic);
            return;
        }
    };
    let metadata = std::fs::symlink_metadata(&path);
    match metadata {
        // A directory navigates the listing into view.
        Ok(metadata) if metadata.is_dir() => navigate_picker_to(model, &path),
        // A file is SELECTED: list its parent (when not already shown)
        // and put the cursor on its row. Selection is visible; the
        // commit stays on the explicit buttons.
        Ok(_) => {
            let parent = path.parent().map(|parent| parent.to_path_buf());
            let file_name = path.file_name().map(|name| name.to_os_string());
            if let (Some(parent), Some(file_name)) = (parent, file_name) {
                if model.open_picker_dir() != Some(parent.as_path()) {
                    navigate_picker_to(model, &parent);
                }
                let row = model
                    .picker_entries()
                    .iter()
                    .position(|entry| entry.path.file_name() == Some(file_name.as_os_str()));
                if let Some(row) = row {
                    model.move_picker_cursor(PlaylistCursor::Row(row));
                } else {
                    model.set_picker_error(format!(
                        "cannot select {}: not in the listing",
                        path.display()
                    ));
                }
            }
        }
        // Unreadable: nothing moved, the picker stays for correction.
        Err(error) => model.set_picker_error(format!("cannot read {}: {error}", path.display())),
    }
}

/// Normalize the picker's typed line the ONE frozen way (T0): a
/// leading `~` expands to the home directory, a relative line
/// resolves against the DISPLAYED directory — never the process working
/// directory of the moment (G1 F09). No shell, variables or globs.
fn picker_typed_path(model: &TuiModel, line: &str) -> Result<std::path::PathBuf, String> {
    let base = model
        .open_picker_dir()
        .map(|dir| dir.to_path_buf())
        .unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        });
    crate::input::normalize_path(Path::new(line), &base)
}

/// List one directory into the Open picker (G1 §8 navigation). I/O
/// lives here in the runtime — the model stays pure — and the picker
/// browses canonical paths, so `..` and entry joins stay honest. The
/// typed field is NEVER auto-written: it is the user's path line, and
/// the listing (plus the popup's title) shows where the picker is.
pub(super) fn navigate_picker_to(model: &mut TuiModel, dir: &Path) {
    let listing = crate::input::list_directory(dir);
    let target =
        simplify_verbatim(&std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()));
    model.set_open_listing(target, listing);
    // A large directory read stalls the loop like a submission does;
    // its busy interval clears the same way (T0 picker freeze).
    drain_busy_interval_input();
}

/// Drop the Windows verbatim prefix that [`std::fs::canonicalize`]
/// adds (`\\?\C:\...`): navigation keeps canonicalization's value,
/// while display and commit paths stay in the vocabulary the user
/// typed. UNC targets (`\\?\UNC\server\share`) map to their plain
/// `\\server\share` spelling; on non-Windows targets this is the
/// identity. The transform is LOSSLESS (G1 F10): a path whose text
/// cannot be rendered without loss keeps its canonical native form
/// untouched — a display convenience must never corrupt filesystem
/// identity.
fn simplify_verbatim(path: &Path) -> std::path::PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    let Some(stripped) = text.strip_prefix(r"\\?\") else {
        return path.to_path_buf();
    };
    if let Some(unc) = stripped.strip_prefix("UNC\\") {
        return std::path::PathBuf::from(format!(r"\\{unc}"));
    }
    std::path::PathBuf::from(stripped)
}

/// Append one user-picked subject — file OR folder — to the temporary
/// playlist through the SAME shared input expansion the startup and
/// Open use (T1A A1/A2). Admission runs before anything is appended;
/// the bounded scan detail rides the status block. The operation is
/// list-only: no episode is started, replaced or retired. Returns the
/// appended count so the caller owns the submission's disposition
/// (the picker closes only on success — G1 F11).
fn perform_add<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
    candidate: &Path,
) -> Result<usize, String> {
    let mut expansion = crate::input::expand_inputs([candidate]);
    let result = crate::input::append_expanded(player, &mut expansion);
    let feedback = match &result {
        Ok(count) => {
            // Walkthrough C freezes "report added/skipped/warnings":
            // the bounded scan summary rides the added line, the
            // warnings under it.
            let worth_summarizing = expansion.skipped > 0
                || expansion.duplicates > 0
                || expansion.rejected > 0
                || !expansion.diagnostics.is_empty();
            let mut feedback = if worth_summarizing {
                format!("added {count} to the playlist ({})", expansion.summary())
            } else {
                format!("added {count} to the playlist")
            };
            let warnings = expansion.scan_warnings();
            for line in warnings {
                feedback.push('\n');
                feedback.push_str(&line);
            }
            feedback
        }
        Err(refusal) => format!("add refused: {refusal}"),
    };
    model.set_status(Some(feedback));
    result
}
/// Perform the Open composition command for one user-supplied path —
/// file OR folder (U1, Issue #166 §10). The input expansion runs
/// BEFORE any destructive step: an empty/unreadable expansion refuses
/// here and the player is not touched at all (no episode destroyed, no
/// navigation state changed, diagnostic shown). A non-empty expansion
/// opens its FIRST candidate through the existing frozen replacement
/// and seeds the accepted list on commit — exactly the startup
/// discipline. The recorded outcome is the status block's feedback —
/// application composition feedback (D14.6), never a playback
/// semantic; a partial traversal reports its bounded scan warnings
/// right under the opened line (U1 corrective REQUIRED-2), so a
/// partially unreadable folder never looks complete. Returns the
/// outcome so the caller owns the submission's disposition (the picker
/// closes only on an actual Open — G1 F11).
fn perform_open<S: EpisodeStart>(
    model: &mut TuiModel,
    player: &mut ReferencePlayerApp<S>,
    candidate: &Path,
) -> Option<OpenOutcome> {
    let mut expansion = crate::input::expand_inputs([candidate]);
    let outcome = crate::input::open_expanded(player, &mut expansion);
    let feedback = match &outcome {
        None => format!("open refused: {}", expansion.refusal()),
        Some(OpenOutcome::Opened) => expansion.opened_status(),
        Some(OpenOutcome::Refused { diagnostic }) => format!("open refused: {diagnostic}"),
        Some(OpenOutcome::ActivationFailedClean { diagnostic }) => {
            format!("open failed (clean): {diagnostic}")
        }
        Some(OpenOutcome::FailStop { diagnostic }) => format!("FAIL-STOP: {diagnostic}"),
    };
    model.set_status(Some(feedback));
    outcome
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use std::fs;

    use crossterm::event::KeyCode;

    use std::path::Path;

    use super::super::refresh;
    use super::super::testutil::*;
    use super::navigate_picker_to;
    use super::perform_add;
    use super::simplify_verbatim;
    use crate::player::ReferencePlayerApp;
    use crate::player::tests::FakeEpisodeSource;
    use crate::tui::model::*;

    /// O from the no-episode state, typing a REAL file path, commits an
    /// episode through the same frozen replacement.
    #[test]
    fn o_from_no_episode_opens_a_real_file() {
        let tree = TempTree::new("o-file");
        let file = tree.live_file("song live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());

        open_via_keys(&mut model, &mut player, &file);
        refresh(&mut model, &player);

        assert_eq!(model.source(), Some(file.to_string_lossy().as_ref()));
        assert_eq!(
            model.status(),
            Some(format!("opened {}", file.display()).as_str()),
            "a single accepted file keeps the plain opened feedback"
        );
        let observation = player.active_handle().expect("committed").observe();
        assert_eq!(observation.activation_error, None);
        assert_eq!(observation.terminal_outcome, None);
    }

    /// A folder typed into the O modal expands into the seeded list —
    /// first track opens, N reaches the next entry.
    #[test]
    fn o_folder_seeds_the_list_and_navigates() {
        let tree = TempTree::new("o-folder");
        let a = tree.live_file("album live-a.flac");
        let b = tree.live_file("album live-b.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());

        open_via_keys(&mut model, &mut player, tree.path());
        refresh(&mut model, &player);

        assert_eq!(model.source(), Some(a.to_string_lossy().as_ref()));
        assert_eq!(
            model.navigation_position(),
            Some((1, 2)),
            "the accepted folder candidates are the navigation state"
        );

        // N walks the SEEDED list through the same replacement.
        assert_eq!(
            handle_key(key(KeyCode::Char('N')), &mut model, &mut player),
            Step::Continue
        );
        refresh(&mut model, &player);
        assert_eq!(model.source(), Some(b.to_string_lossy().as_ref()));
        assert_eq!(model.navigation_position(), Some((2, 2)));
    }

    /// Esc cancels the Open modal and opens nothing — not even a probe
    /// runs.
    #[test]
    fn esc_cancels_the_open_modal_without_touching_the_player() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let tree = TempTree::new("esc");
        let file = tree.live_file("song.flac");
        let mut player = ReferencePlayerApp::new(source);
        let mut model = TuiModel::new(String::new());

        assert_eq!(
            handle_key(key(KeyCode::Char('O')), &mut model, &mut player),
            Step::Continue
        );
        type_text(&mut model, &mut player, &file.to_string_lossy());
        assert_eq!(
            handle_key(key(KeyCode::Esc), &mut model, &mut player),
            Step::Continue
        );

        assert_eq!(model.modal(), None);
        assert_eq!(model.status(), None);
        assert!(player.active_handle().is_none());
        assert!(log.lock().unwrap().is_empty(), "no Open was attempted");
    }

    /// An empty/unreadable folder refuses INSIDE the shell before any
    /// destructive step — the idle state survives intact (no episode,
    /// no playlist, no probe).
    #[test]
    fn a_bad_folder_open_from_idle_preserves_the_idle_state() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let tree = TempTree::new("bad-folder-idle");
        let missing = tree.path().join("does-not-exist");
        let mut player = ReferencePlayerApp::new(source);
        let mut model = TuiModel::new(String::new());

        open_via_keys(&mut model, &mut player, &missing);
        refresh(&mut model, &player);

        let status = model.status().expect("a diagnostic is shown");
        assert!(status.starts_with("open refused: "), "{status}");
        assert!(player.active_handle().is_none(), "still no episode");
        assert_eq!(model.navigation_position(), None, "no list was committed");
        assert!(
            log.lock().unwrap().is_empty(),
            "the refusal preceded the probe"
        );
    }

    /// A bad folder open WHILE PLAYING leaves live playback untouched —
    /// the expansion refusal happens before the frozen replacement can
    /// destroy anything.
    #[test]
    fn a_bad_folder_open_while_playing_leaves_playback_untouched() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let tree = TempTree::new("bad-folder-live");
        let file = tree.live_file("playing.flac");
        let mut player = ReferencePlayerApp::new(source);
        let handle = {
            let outcome = player.open(&file);
            assert_eq!(outcome, crate::player::OpenOutcome::Opened);
            player.active_handle().expect("committed").clone()
        };
        // The setup Open probed once; the refusal below must add
        // nothing to that log.
        let log_len_after_setup = log.lock().unwrap().len();
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        let empty_dir = tree.path().join("empty-班级");
        fs::create_dir_all(&empty_dir).expect("empty dir");
        open_via_keys(&mut model, &mut player, &empty_dir);
        refresh(&mut model, &player);

        let status = model.status().expect("a diagnostic is shown");
        assert!(status.starts_with("open refused: "), "{status}");
        assert_eq!(model.source(), Some(file.to_string_lossy().as_ref()));
        let observation = handle.observe();
        assert!(
            !observation.stop_requested,
            "the episode was never asked to stop"
        );
        assert_eq!(observation.terminal_outcome, None);
        assert_eq!(
            player.navigation_position(),
            Some((1, 1)),
            "the navigation state kept exactly what the committed Open gave it"
        );
        assert_eq!(
            log.lock().unwrap().len(),
            log_len_after_setup,
            "the refusal preceded the probe"
        );
    }

    /// U1 corrective REQUIRED-2: an O-open over a folder that only
    /// PARTIALLY enumerated still opens its first candidate — and the
    /// status block names the warnings instead of looking complete.
    /// (Unix permission simulation; the equivalent partial-failure
    /// presentation is pinned platform-free in the input module.)
    #[cfg(unix)]
    #[test]
    fn an_o_open_over_a_partially_unreadable_folder_reports_the_warnings() {
        use std::os::unix::fs::PermissionsExt;

        let tree = TempTree::new("o-partial");
        let file = tree.live_file("kept.flac");
        let locked = tree.path().join("locked");
        fs::create_dir_all(&locked).expect("locked dir");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("lock");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());

        open_via_keys(&mut model, &mut player, tree.path());
        refresh(&mut model, &player);
        // Restore FIRST so cleanup can remove the tree even on failure.
        let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o755));

        assert_eq!(model.source(), Some(file.to_string_lossy().as_ref()));
        let status = model.status().expect("a partial scan is reported");
        assert!(status.contains("; 1 scan warning"), "{status}");
        assert!(
            status.contains("\nscan warning: cannot read "),
            "the diagnostic itself is shown: {status}"
        );
        assert_eq!(
            player.navigation_position(),
            Some((1, 1)),
            "the commit-riding seed holds the found candidates"
        );
    }

    // ------------------------------------------------------------------
    // The one-dispatch invariant and the transport commands.
    // ------------------------------------------------------------------

    /// The visible Open control (G1): a click on the transport row's
    /// Open button opens the picker over the running directory — the
    /// listing is already populated when the popup first shows.
    #[test]
    fn clicking_the_open_button_opens_the_picker_over_the_running_directory() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);
        let (column, row) = draw_and_locate(
            &mut model,
            100,
            30,
            &crate::tui::model::HitTarget::Transport(TransportButton::Open),
        );
        assert!(
            click_at(column, row, &mut model, &mut player).is_some(),
            "the armed click activates the Open button"
        );
        assert_eq!(
            model.modal().map(|modal| modal.kind()),
            Some(ModalKind::Open)
        );
        let dir = model
            .open_picker_dir()
            .expect("the picker lists a starting directory")
            .to_path_buf();
        let entries = match &model.modal() {
            Some(crate::tui::model::Modal::Open(picker)) => picker.entries.len(),
            _ => 0,
        };
        assert!(entries > 0, "the starting directory listing is populated");
        assert_eq!(
            dir.file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            std::env::current_dir()
                .expect("cwd")
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            "the picker starts where the shell runs"
        );
    }

    /// The picker rows navigate (a directory row descends, `..`
    /// ascends) and Enter on a file row SELECTS ONLY (the frozen T0
    /// rule): no row gesture commits — the explicit [Open] button does,
    /// consuming the selection's native path.
    #[test]
    fn the_picker_rows_navigate_and_a_file_row_only_selects() {
        let tree = TempTree::new("picker-rows");
        let file = tree.live_file("picked live.flac");
        let subdir = tree.path().join("album");
        fs::create_dir_all(&subdir).expect("subdir");
        // navigate_picker_to browses CANONICAL paths, and on Windows
        // runners `std::env::temp_dir()` spells the user profile in the
        // 8.3 short form (RUNNER~1) that canonicalize resolves away.
        // The expectations therefore go through the SAME
        // canonicalize + simplify transform the runtime applies.
        let canonical = |path: &Path| {
            simplify_verbatim(&std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
        };
        let subdir = canonical(&subdir);
        let root = canonical(tree.path());
        let file = canonical(&file);

        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        // Open the picker through the real dispatch, then hand it a
        // DETERMINISTIC listing of the temp tree (the model stays a
        // pure projection of what the runtime read).
        assert_eq!(
            handle_key(key(KeyCode::Char('O')), &mut model, &mut player),
            Step::Continue
        );
        model.set_open_listing(
            tree.path().to_path_buf(),
            Ok(vec![
                crate::input::DirectoryEntry {
                    name: "album".to_owned(),
                    is_dir: true,
                    path: tree.path().join("album"),
                },
                crate::input::DirectoryEntry {
                    name: "picked live.flac".to_owned(),
                    is_dir: false,
                    path: tree.path().join("picked live.flac"),
                },
            ]),
        );
        // Listing rows: [.., album, picked live.flac]. ↓ selects `..`
        // — and the arrows drive the list, so the listing holds the
        // focus and Enter activates its cursor.
        assert_eq!(
            handle_key(key(KeyCode::Down), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(
            model.focus(),
            Some(FocusId::PickerList),
            "the arrows drive the listing"
        );
        // Click the album row: it SELECTS it (the armed-click rule) —
        // the click itself moves no focus; Enter then activates the
        // clicked selection through the listing focus the arrows left.
        let (column, row) = draw_and_locate(
            &mut model,
            100,
            30,
            &crate::tui::model::HitTarget::PickerRow(1),
        );
        click_at(column, row, &mut model, &mut player);
        assert_eq!(
            model
                .picker_cursor_entry()
                .map(|(entry, _path, _is_dir_step)| entry.name.as_str()),
            Some("album"),
            "the row click moved the selection"
        );
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(
            model.open_picker_dir(),
            Some(subdir.as_path()),
            "a directory row click + Enter descends into it"
        );

        // Back up with the parent step (Backspace on the listing),
        // then walk ↓ to the file row. Enter on a FILE row selects it
        // — the modal stays open and nothing starts.
        assert_eq!(
            handle_key(key(KeyCode::Backspace), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(
            model.open_picker_dir(),
            Some(root.as_path()),
            "Backspace on the listing steps up to the parent"
        );
        for _ in 0..3 {
            assert_eq!(
                handle_key(key(KeyCode::Down), &mut model, &mut player),
                Step::Continue
            );
        }
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        assert!(
            model.modal().is_some(),
            "Enter on a file row never commits: the picker stays"
        );
        assert!(
            player.active_handle().is_none(),
            "no episode started from a row gesture"
        );
        assert_eq!(
            model.picker_subject(),
            crate::tui::model::PickerSubject::Listing {
                path: file.clone(),
                is_dir: false,
            },
            "the file row IS the selection, its native path the subject"
        );

        // The explicit [Open] button commits the selection through the
        // frozen replacement, and success routes to Now Playing.
        let (column, row) = draw_and_locate(
            &mut model,
            100,
            30,
            &crate::tui::model::HitTarget::ModalButton(crate::tui::model::ModalButton::Open),
        );
        click_at(column, row, &mut model, &mut player);
        refresh(&mut model, &player);
        assert_eq!(
            model.source(),
            Some(file.to_string_lossy().as_ref()),
            "the [Open] button opened the selection"
        );
        assert_eq!(model.modal(), None, "success closed the picker");
        assert_eq!(
            model.route(),
            TuiRoute::NowPlaying,
            "an Open-file success lands on Now Playing"
        );
    }

    /// Mouse-only picker navigation (G1 F04, T0: mouse-only navigation
    /// needs no typed path): clicking `..` ascends, clicking a
    /// directory row selects it, and the visible [Enter folder] button
    /// descends — the mouse converging on the same semantic actions as
    /// the keyboard, with no hidden keyboard assistance.
    #[test]
    fn the_picker_navigates_by_mouse_alone() {
        let tree = TempTree::new("picker-mouse");
        let subdir = tree.path().join("album");
        fs::create_dir_all(&subdir).expect("subdir");
        let canonical = |path: &Path| {
            simplify_verbatim(&std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
        };
        let root = canonical(tree.path());
        let subdir = canonical(&subdir);
        let root_parent = canonical(tree.path().parent().unwrap_or(Path::new("/")));

        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        assert_eq!(
            handle_key(key(KeyCode::Char('O')), &mut model, &mut player),
            Step::Continue
        );
        model.set_open_listing(
            tree.path().to_path_buf(),
            Ok(vec![crate::input::DirectoryEntry {
                name: "album".to_owned(),
                is_dir: true,
                path: tree.path().join("album"),
            }]),
        );

        // Click the `..` row: navigation chrome — the click IS the
        // parent step, ascending one level.
        let (column, row) = draw_and_locate(
            &mut model,
            100,
            30,
            &crate::tui::model::HitTarget::PickerRow(0),
        );
        click_at(column, row, &mut model, &mut player);
        assert_eq!(
            model.open_picker_dir(),
            Some(root_parent.as_path()),
            "a click on `..` ascends to the parent"
        );

        // Back into the temp tree via the real listing, click the
        // directory row (a selection, not a descent)...
        navigate_picker_to(&mut model, &root);
        let (column, row) = draw_and_locate(
            &mut model,
            100,
            30,
            &crate::tui::model::HitTarget::PickerRow(1),
        );
        click_at(column, row, &mut model, &mut player);
        assert_eq!(
            model.open_picker_dir(),
            Some(root.as_path()),
            "a click on a directory row selects; it does not descend"
        );

        // ...and the visible [Enter folder] button descends.
        let (column, row) = draw_and_locate(
            &mut model,
            100,
            30,
            &crate::tui::model::HitTarget::ModalButton(crate::tui::model::ModalButton::EnterFolder),
        );
        click_at(column, row, &mut model, &mut player);
        assert_eq!(
            model.open_picker_dir(),
            Some(subdir.as_path()),
            "[Enter folder] descends into the selected directory"
        );
    }

    /// The [Add to Playlist] button appends the picked subject through
    /// the shared expansion — list-only: no episode is started,
    /// replaced or retired (T1A A1/A2).
    #[test]
    fn the_picker_add_button_appends_without_touching_the_episode() {
        let tree = TempTree::new("picker-add");
        let _file = tree.live_file("addition live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        assert_eq!(
            handle_key(key(KeyCode::Char('O')), &mut model, &mut player),
            Step::Continue
        );
        model.set_open_listing(
            tree.path().to_path_buf(),
            Ok(vec![crate::input::DirectoryEntry {
                name: "addition live.flac".to_owned(),
                is_dir: false,
                path: tree.path().join("addition live.flac"),
            }]),
        );
        // Select the file row, then click the visible Add button.
        assert_eq!(
            handle_key(key(KeyCode::Down), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(
            handle_key(key(KeyCode::Down), &mut model, &mut player),
            Step::Continue
        );
        let (column, row) = draw_and_locate(
            &mut model,
            100,
            30,
            &crate::tui::model::HitTarget::ModalButton(crate::tui::model::ModalButton::Add),
        );
        click_at(column, row, &mut model, &mut player);
        refresh(&mut model, &player);

        assert_eq!(model.playlist().len(), 1, "the file was appended");
        assert!(
            player.active_handle().is_none(),
            "Add never starts an episode"
        );
        assert!(
            model
                .status()
                .is_some_and(|status| status.contains("added 1 to the playlist")),
            "{}",
            model.status().unwrap_or_default()
        );
    }

    /// G1 §8 parent navigation through the VISIBLE affordance: Enter
    /// (or [Open]-button-less activation) on the `..` row ascends to
    /// the parent directory — the review-found P1 where it re-listed
    /// the same directory.
    #[test]
    fn enter_on_the_parent_row_ascends() {
        let tree = TempTree::new("picker-parent");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        assert_eq!(
            handle_key(key(KeyCode::Char('O')), &mut model, &mut player),
            Step::Continue
        );
        model.set_open_listing(
            tree.path().to_path_buf(),
            Ok(vec![crate::input::DirectoryEntry {
                name: "album".to_owned(),
                is_dir: true,
                path: tree.path().join("album"),
            }]),
        );
        // Listing rows: [.., album]. ↓ selects `..`; Enter ascends.
        assert_eq!(
            handle_key(key(KeyCode::Down), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(
            model.open_picker_dir().and_then(|dir| dir.file_name()),
            tree.path().parent().and_then(|dir| dir.file_name()),
            "Enter on `..` lists the parent directory"
        );
    }
    /// G1 F09: a RELATIVE typed line resolves against the DISPLAYED
    /// directory (the picker's own base), never against the process
    /// working directory of the moment. The test's CWD (the crate dir)
    /// differs from the displayed temp directory, so only the frozen
    /// base can resolve the name.
    #[test]
    fn a_relative_typed_line_resolves_against_the_displayed_directory() {
        let tree = TempTree::new("relative-base");
        let file = tree.live_file("relative live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        assert_eq!(
            handle_key(key(KeyCode::Char('O')), &mut model, &mut player),
            Step::Continue
        );
        // The picker now displays the temp tree (a deterministic feed
        // standing in for the runtime's listing of it).
        model.set_open_listing(
            tree.path().to_path_buf(),
            Ok(vec![crate::input::DirectoryEntry {
                name: "relative live.flac".to_owned(),
                is_dir: false,
                path: tree.path().join("relative live.flac"),
            }]),
        );
        // Type the BARE NAME — relative text — and confirm the field.
        for c in "relative live.flac".chars() {
            model.modal_push(c);
        }
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        // The file was SELECTED (it is in the listing): the cursor sits
        // on its row and the subject is its native path.
        assert_eq!(
            model.picker_subject(),
            PickerSubject::Listing {
                path: file.clone(),
                is_dir: false,
            },
            "the relative line resolved against the DISPLAYED directory"
        );
        assert!(
            model.modal().is_some(),
            "the field's Enter selected; it did not start playback"
        );

        // The explicit [Open] commits the resolved subject.
        let (column, row) = draw_and_locate(
            &mut model,
            100,
            30,
            &crate::tui::model::HitTarget::ModalButton(crate::tui::model::ModalButton::Open),
        );
        click_at(column, row, &mut model, &mut player);
        refresh(&mut model, &player);
        assert_eq!(
            model.source(),
            Some(file.to_string_lossy().as_ref()),
            "the resolved native path is what was opened"
        );
    }

    /// G1 F11: a FAILED submission keeps the picker and its whole
    /// retry context — the typed line, the displayed directory and the
    /// selection all survive — with a bounded diagnostic; correcting
    /// the line then commits.
    #[test]
    fn a_failed_field_confirm_keeps_the_picker_context_for_retry() {
        let tree = TempTree::new("retry-context");
        let file = tree.live_file("real live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        assert_eq!(
            handle_key(key(KeyCode::Char('O')), &mut model, &mut player),
            Step::Continue
        );
        model.set_open_listing(
            tree.path().to_path_buf(),
            Ok(vec![crate::input::DirectoryEntry {
                name: "real live.flac".to_owned(),
                is_dir: false,
                path: tree.path().join("real live.flac"),
            }]),
        );
        // Type a line naming nothing that exists, and confirm it.
        for c in "missing live.flac".chars() {
            model.modal_push(c);
        }
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        assert!(model.modal().is_some(), "the picker STAYS open");
        assert!(
            model.open_picker_dir().is_some(),
            "the displayed directory survives"
        );
        assert!(
            player.active_handle().is_none(),
            "the failed attempt touched no player state"
        );

        // Correct the line (backspace the wrong name away, type the
        // real one) and confirm again: the corrected file is selected.
        for _ in 0.."missing live.flac".len() {
            assert_eq!(
                handle_key(key(KeyCode::Backspace), &mut model, &mut player),
                Step::Continue
            );
        }
        for c in "real live.flac".chars() {
            model.modal_push(c);
        }
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(
            model.picker_subject(),
            PickerSubject::Listing {
                path: file,
                is_dir: false,
            },
            "the corrected line selects the real file"
        );
    }

    /// G1 F12: the frozen success routing — an Open-FOLDER lands on
    /// Playlist (its first candidate already started); an Add lands on
    /// Playlist without interrupting audio. (Open-FILE -> Now Playing
    /// is pinned by the picker-rows test.)
    #[test]
    fn picker_success_routes_follow_the_frozen_dispositions() {
        let tree = TempTree::new("success-routes");
        tree.live_file("first.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        // A folder Open: success -> Playlist.
        assert_eq!(
            handle_key(key(KeyCode::Char('O')), &mut model, &mut player),
            Step::Continue
        );
        for c in tree.path().to_string_lossy().chars() {
            model.modal_push(c);
        }
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        // Tab to [Open] and commit.
        for _ in 0..8 {
            handle_key(key(KeyCode::Tab), &mut model, &mut player);
            if model.focus() == Some(FocusId::PickerButton(crate::tui::model::ModalButton::Open)) {
                break;
            }
        }
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(model.modal(), None, "success closed the picker");
        assert_eq!(
            model.route(),
            TuiRoute::Playlist,
            "an Open-folder success lands on Playlist"
        );
        assert!(
            player.active_handle().is_some(),
            "the first candidate started"
        );

        // An Add from the picker: success -> Playlist, episode intact.
        tree.live_file("second.flac");
        assert_eq!(
            handle_key(key(KeyCode::Char('O')), &mut model, &mut player),
            Step::Continue
        );
        for c in tree.path().join("second.flac").to_string_lossy().chars() {
            model.modal_push(c);
        }
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        for _ in 0..8 {
            handle_key(key(KeyCode::Tab), &mut model, &mut player);
            if model.focus() == Some(FocusId::PickerButton(crate::tui::model::ModalButton::Add)) {
                break;
            }
        }
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(model.modal(), None);
        assert_eq!(model.route(), TuiRoute::Playlist);
        assert_eq!(
            player.playlist_playing_position(),
            Some(0),
            "no replacement"
        );
        assert_eq!(
            player.navigation_position(),
            Some((1, 2)),
            "the file appended"
        );
    }

    /// Walkthrough B's second leg (T0 picker freeze): [Use this folder]
    /// makes the DISPLAYED directory the subject, and [Open] then
    /// commits a folder Open WITHOUT entering it — the mouse/keyboard
    /// route a user needs when the listing already shows the folder
    /// they want.
    #[test]
    fn use_this_folder_commits_the_displayed_directory_without_entering_it() {
        let tree = TempTree::new("use-folder");
        tree.live_file("inside.flac");
        // The picker browses CANONICAL paths (navigate_picker_to), and
        // on Windows runners `std::env::temp_dir()` spells the user
        // profile in the 8.3 short form (RUNNER~1) that canonicalize
        // resolves away — the typed line navigates through the SAME
        // canonicalize + simplify transform, so the oracle must too.
        let canonical = |path: &Path| {
            simplify_verbatim(&std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
        };
        let displayed = canonical(tree.path());
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());
        refresh(&mut model, &player);

        assert_eq!(
            handle_key(key(KeyCode::Char('O')), &mut model, &mut player),
            Step::Continue
        );
        for c in tree.path().to_string_lossy().chars() {
            model.modal_push(c);
        }
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue,
            "field Enter navigates INTO the typed directory"
        );
        // The listing now shows the directory's CONTENTS; the user
        // walks back out one level so the target folder is the one
        // DISPLAYED, and presses [Use this folder] + [Open].
        assert_eq!(
            handle_key(key(KeyCode::Backspace), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(
            model.open_picker_dir(),
            Some(displayed.as_path()),
            "the parent of the target is displayed"
        );
        for _ in 0..8 {
            handle_key(key(KeyCode::Tab), &mut model, &mut player);
            if model.focus()
                == Some(FocusId::PickerButton(
                    crate::tui::model::ModalButton::UseFolder,
                ))
            {
                break;
            }
        }
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue,
            "Enter on [Use this folder] arms the displayed directory"
        );
        for _ in 0..8 {
            handle_key(key(KeyCode::Tab), &mut model, &mut player);
            if model.focus() == Some(FocusId::PickerButton(crate::tui::model::ModalButton::Open)) {
                break;
            }
        }
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut model, &mut player),
            Step::Continue
        );
        assert_eq!(model.modal(), None, "the commit closed the picker");
        assert_eq!(model.route(), TuiRoute::Playlist, "folder Open -> Playlist");
        assert!(
            player.active_handle().is_some(),
            "the displayed folder was opened, not entered"
        );
    }

    /// Walkthrough C (T0): an Add reports added/skipped/warnings — a
    /// folder Add over mixed content names the skipped entries instead
    /// of looking complete.
    #[test]
    fn an_add_reports_added_skipped_and_warnings() {
        let tree = TempTree::new("add-skip");
        tree.live_file("kept.flac");
        let skipped = tree.path().join("cover.jpg");
        fs::write(&skipped, b"not audio").expect("cover art file");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut model = TuiModel::new(String::new());

        let added = perform_add(&mut model, &mut player, tree.path());
        assert_eq!(added, Ok(1));
        let status = model.status().expect("feedback");
        assert!(status.starts_with("added 1 to the playlist ("), "{status}");
        assert!(status.contains("1 skipped"), "{status}");
    }
}
