//! The ONE active modal (there is no stack): its kinds, the Open
//! picker's presentation draft, the editing vocabulary, the picker's
//! buttons and the deterministic submission subject.

use std::time::Duration;

use super::actions::PlaylistCursor;

/// The modal kind to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalKind {
    /// The Open input line (D14.6): a literal file-or-folder path.
    Open,
    /// The GoTo exact-seek line (Issue #166 §27).
    GoTo,
    /// The keyboard/mouse help overlay.
    Help,
}

/// One entry of the Open picker's listing: the display name, the kind
/// the runtime's `list_directory` classified it as, and the entry's
/// NATIVE filesystem identity — the path a selection commits (G1 F10).
/// A presentation draft inside the modal — no admission happened by
/// listing anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerEntry {
    pub name: String,
    pub is_dir: bool,
    /// The synthesized `..` row: descending goes to the parent
    /// directory (G1 §8 parent navigation).
    pub is_parent: bool,
    /// The entry's native path: the listed directory joined with the
    /// filesystem's own file name — never a reconstruction from the
    /// lossy display string (for the `..` row: the parent directory).
    pub path: std::path::PathBuf,
}

/// The Open modal as a terminal-native picker (G1 §8): an editable
/// path line over a one-level listing of the directory it names, with
/// the commit buttons and their accelerators. The listing is a
/// presentation draft the runtime refreshes (the model performs no
/// I/O); admission still happens only at commit, through the same
/// shared input expansion as before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPicker {
    /// The path line the user can type into (the pre-picker editing
    /// semantics, unchanged).
    pub input: String,
    /// The directory the listing shows, when one has been listed.
    pub dir: Option<std::path::PathBuf>,
    /// The listed entries, runtime-supplied (directories first, then
    /// audio-candidate files).
    pub entries: Vec<PickerEntry>,
    /// The listing cursor (the selection), when the listing has rows.
    pub cursor: Option<usize>,
    /// The listing's honest failure diagnostic (an unreadable
    /// directory), shown inside the modal instead of a fabricated
    /// empty list.
    pub error: Option<String>,
    /// Whether the [Use this folder] button has made the DISPLAYED
    /// directory the submission subject (T0 picker freeze: "Use this
    /// folder selects the displayed directory as the target"). It dies
    /// with the intent it represented: any navigation, listing
    /// refresh, field edit or row selection replaces it with a fresh
    /// subject.
    pub folder_target: bool,
}

/// The ONE active modal (§23/§24), replacing the old collection of
/// modal booleans. At most one exists; there is no modal stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    Open(OpenPicker),
    GoTo { input: String },
    Help,
}

impl Modal {
    /// Which kind this modal is.
    pub fn kind(&self) -> ModalKind {
        match self {
            Modal::Open(_) => ModalKind::Open,
            Modal::GoTo { .. } => ModalKind::GoTo,
            Modal::Help => ModalKind::Help,
        }
    }

    /// The modal's text content, for the two text modals.
    #[cfg(test)]
    pub fn input(&self) -> Option<&str> {
        match self {
            Modal::Open(picker) => Some(picker.input.as_str()),
            Modal::GoTo { input } => Some(input),
            Modal::Help => None,
        }
    }
}

/// One editing step inside a text modal. The modal's editing keys are
/// actions like any other, so a modal key press converges on the same
/// single dispatch boundary as everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalInput {
    Char(char),
    Backspace,
    /// Enter on the path field: navigate the typed directory or select
    /// the named file — never a commit (T0 picker freeze: the field
    /// line never starts playback; Open/Add need their buttons).
    Confirm,
    /// Enter on the picker listing: a directory (or the `..` row)
    /// navigates, a file is SELECTED — the frozen T0 rule that keeps
    /// submission off the rows and on the explicit buttons.
    ListActivate,
    /// The picker listing's cursor moves (arrows, wheel, a row click).
    ListMove(PlaylistCursor),
    /// The picker steps up to the parent directory (Backspace on the
    /// list, the `..` row's own activation).
    ListParent,
    /// The [Open] button (or its accelerator): commit the picked
    /// subject through the Open composition.
    CommitOpen,
    /// The [Add to Playlist] button (or its accelerator): append the
    /// picked subject through the same shared input expansion.
    CommitAdd,
    /// The [Use this folder] button: the DISPLAYED directory becomes
    /// the submission subject (T0 picker freeze), so a folder can be
    /// opened or added without entering it.
    UseFolder,
    /// Esc (or `?` for Help): close the modal. The closing event is
    /// consumed by the modal — it never also acts on the background.
    Cancel,
}

/// One of the Open picker's visible buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalButton {
    Open,
    Add,
    /// Make the displayed directory the submission target — one of the
    /// T0 picker's frozen visible navigation controls, so a folder can
    /// be committed without entering it.
    UseFolder,
    /// Descend into the selected directory row — the mouse's
    /// [`ModalInput::ListActivate`] (G1 F04: every visible core picker
    /// operation must be mouse-reachable; "Enter folder" is one of the
    /// T0 picker's frozen visible navigation controls).
    EnterFolder,
    Cancel,
}

/// What confirming the active modal decided. The GoTo reader is the
/// EXISTING [`crate::cli::parse_seek_time`] — the shell's one time
/// grammar, shared with the scriptable transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalConfirm {
    /// Nothing to do (a Help confirm).
    Nothing,
    /// The confirmed seek target; the modal is already closed.
    Seek(Duration),
    /// The token is not a readable time. The GoTo modal STAYS OPEN for
    /// correction and the shell shows the bounded diagnostic — a
    /// malformed seek intent is never sent.
    Unreadable(&'static str),
}

/// The Open picker's ONE deterministic submission subject (G1 §8, the
/// frozen "one target" model). The subject is the listing selection
/// while one exists; when none does, it is the typed path line — the
/// line the field visibly shows, normalized by the RUNTIME against the
/// displayed directory (T0: relative text resolves against the
/// displayed directory, and native identities stay native). Editing the
/// field clears the selection, so a stale row can never hide behind a
/// freshly edited target — the user commits exactly what they see
/// selected, or exactly what they typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerSubject {
    /// The selected listing entry, with its NATIVE path and the kind
    /// the listing classified it as.
    Listing {
        path: std::path::PathBuf,
        is_dir: bool,
    },
    /// The typed path line, as typed. The runtime expands a leading
    /// `~` and resolves a relative line against the picker's displayed
    /// directory at commit time.
    Typed(String),
    /// Nothing to commit: no selection and an empty line.
    None,
}
