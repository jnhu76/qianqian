//! Host input preparation for the product launch (U1, Issue #166):
//! user-supplied paths — a file, a folder, or several of either —
//! expanded into one ordered list of playable candidates.
//!
//! Classification of this code (deliberately unexciting): it is an
//! ordinary application function of the composition root, NOT a Plugin
//! (PBK-002 D13: an existing owner — the App's input preparation —
//! expresses it completely; no independent composition identity is
//! claimed), and it is NOT playback semantics. The ENUMERATION knows
//! the filesystem and nothing else: no decode capability, no probe, no
//! PCM, no composition types. (`open_expanded` and `prepare_startup`
//! then hand the accepted candidates to the player's EXISTING Open
//! path — they add no classification knowledge of their own.)
//!
//! Truth-class discipline:
//!
//! ```text
//! accepted     enumeration candidates in deterministic order. An
//!              accepted path is a CANDIDATE, never playback truth:
//!              whether any file is actually playable is witnessed by
//!              the existing decode/media preflight (the F6 Open
//!              probe), exactly as before. The extension prefilter
//!              below only reduces useless probe calls for enumerated
//!              directory contents; extension alone is never treated
//!              as proof of playability, and an EXPLICITLY named file
//!              bypasses the prefilter entirely so the probe keeps the
//!              final word.
//! skipped      enumerated entries classified as not-audio-candidate
//!              (wrong/absent extension, symlinks/junctions, anything
//!              that is not a regular file). Bounded counts, not
//!              verdicts.
//! diagnostics  bounded filesystem errors (unreadable paths, listing
//!              failures). Never panics; never unbounded.
//! ```
//!
//! Traversal contract (Issue #166 §5/§7): recursive over real
//! directories; regular files only; directory symlinks and junctions
//! are never followed (which also makes traversal cycles impossible —
//! the only cycles a filesystem can form go through symlinks);
//! per-directory listing is sorted by entry name, so enumeration order
//! is normalized into a deterministic, path-sorted order and the raw
//! directory order never becomes playlist order; roots are processed
//! in the order the user typed them; Unicode/CJK/space paths are
//! ordinary paths. Recursion depth is bounded by the filesystem's own
//! path depth. Enumeration is synchronous and UNBOUNDED in entry count
//! by design: it runs on the caller's thread inside the same
//! documented synchronous-Open stall (see `tui::runtime`), never on
//! any playback/realtime thread.

use std::path::{Path, PathBuf};

use crate::player::{EpisodeStart, OpenOutcome, ReferencePlayerApp};

/// The result of expanding user input roots into playable candidates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExpandedInputs {
    /// Enumeration candidates, in the deterministic order they should
    /// be presented/opened. Candidates, not playability verdicts —
    /// the Open probe stays the witness.
    pub accepted: Vec<PathBuf>,
    /// Enumerated entries classified as non-candidates (unsupported
    /// extension, symlinks/junctions, non-regular files).
    pub skipped: usize,
    /// Bounded filesystem diagnostics, in encounter order.
    pub diagnostics: Vec<String>,
    /// How many further diagnostics were suppressed by
    /// [`MAX_DIAGNOSTICS`]. Presentation counts, never semantics.
    suppressed_diagnostics: usize,
}

/// Diagnostics kept before suppression kicks in, so a pathological
/// tree cannot turn an Open into an unbounded report.
const MAX_DIAGNOSTICS: usize = 8;

impl ExpandedInputs {
    fn push_diagnostic(&mut self, diagnostic: String) {
        if self.diagnostics.len() < MAX_DIAGNOSTICS {
            self.diagnostics.push(diagnostic);
        } else {
            self.suppressed_diagnostics += 1;
        }
    }

    /// Why there is nothing to open. Used only when [`Self::accepted`]
    /// is empty: the first diagnostic if the filesystem reported one
    /// (with the suppressed count when present), otherwise the honest
    /// classification summary.
    pub fn refusal(&self) -> String {
        if let Some(first) = self.diagnostics.first() {
            if self.suppressed_diagnostics > 0 {
                format!("{first} (+{} more)", self.suppressed_diagnostics)
            } else {
                first.clone()
            }
        } else if self.skipped > 0 {
            format!(
                "no audio candidates; {skipped} entries skipped",
                skipped = self.skipped
            )
        } else {
            "no audio candidates".to_owned()
        }
    }

    /// One-line presentation summary of a non-empty expansion
    /// ("2 candidates, 1 skipped"), extended with the bounded
    /// scan-warning count when the traversal was PARTIAL
    /// ("...; 2 scan warnings (+3 more)") — a partially unreadable
    /// folder must never present as completely loaded (U1 corrective
    /// REQUIRED-2). "Candidates" is the honest noun: playability is
    /// witnessed downstream, not here.
    pub fn summary(&self) -> String {
        let mut summary = format!(
            "{} candidate{}",
            self.accepted.len(),
            if self.accepted.len() == 1 { "" } else { "s" }
        );
        if self.skipped > 0 {
            summary.push_str(&format!(", {} skipped", self.skipped));
        }
        if !self.accepted.is_empty() && !self.diagnostics.is_empty() {
            summary.push_str(&format!(
                "; {} scan warning{}",
                self.diagnostics.len(),
                if self.diagnostics.len() == 1 { "" } else { "s" }
            ));
            if self.suppressed_diagnostics > 0 {
                summary.push_str(&format!(" (+{} more)", self.suppressed_diagnostics));
            }
        }
        summary
    }

    /// The bounded scan-warning detail lines for an expansion that
    /// FOUND candidates but also hit filesystem errors — a partial
    /// traversal. Every diagnostic becomes one `scan warning: ...`
    /// line; a suppressed tail is counted on a final `(+N more)` line,
    /// so the block stays bounded however pathological the tree.
    ///
    /// Empty when there is nothing to warn about: a complete
    /// enumeration (no diagnostics), or a refused expansion (no
    /// candidates — [`Self::refusal`] carries the diagnostics
    /// instead).
    ///
    /// Truth class: application/host diagnostics about the
    /// ENUMERATION. Never playback Facts, never terminal outcomes,
    /// never activation failures.
    pub fn scan_warnings(&self) -> Vec<String> {
        if self.accepted.is_empty() || self.diagnostics.is_empty() {
            return Vec::new();
        }
        let mut lines: Vec<String> = self
            .diagnostics
            .iter()
            .map(|diagnostic| format!("scan warning: {diagnostic}"))
            .collect();
        if self.suppressed_diagnostics > 0 {
            lines.push(format!("(+{} more)", self.suppressed_diagnostics));
        }
        lines
    }

    /// The shell status block for a successful Open over this
    /// expansion: `opened <first candidate>`, with the [`Self::summary`]
    /// in parentheses whenever there is something to report (more
    /// candidates, skips, or scan warnings) and the bounded
    /// [`Self::scan_warnings`] detail lines after it. The block may be
    /// multi-line; the shell renders each line on its own row.
    pub fn opened_status(&self) -> String {
        let opened = match self.accepted.first() {
            Some(first) => format!("opened {}", first.display()),
            None => "opened".to_owned(),
        };
        let worth_summarizing =
            self.accepted.len() > 1 || self.skipped > 0 || !self.diagnostics.is_empty();
        if !worth_summarizing {
            return opened;
        }
        let mut status = format!("{opened} ({})", self.summary());
        for line in self.scan_warnings() {
            status.push('\n');
            status.push_str(&line);
        }
        status
    }
}

/// Expand user-supplied input roots (files and/or directories) into
/// one candidate list. See the module docs for the traversal and
/// truth-class contract.
pub fn expand_inputs<I>(roots: I) -> ExpandedInputs
where
    I: IntoIterator,
    I::Item: AsRef<Path>,
{
    let mut expanded = ExpandedInputs::default();
    for root in roots {
        let root = root.as_ref();
        // symlink_metadata never follows the root link, so a symlinked
        // root is reported as what it is instead of silently expanding
        // its target (or its cycle).
        match std::fs::symlink_metadata(root) {
            Err(error) => {
                expanded.push_diagnostic(format!("cannot read {}: {error}", root.display()))
            }
            Ok(meta) if meta.file_type().is_dir() => walk_directory(root, &mut expanded),
            Ok(meta) if meta.file_type().is_file() => {
                // An explicitly named file bypasses the extension
                // prefilter: the decode probe, not the extension, is
                // the playability witness (Issue #166 §5).
                expanded.accepted.push(root.to_path_buf());
            }
            Ok(meta) if meta.file_type().is_symlink() => {
                expanded.push_diagnostic(format!("skipped symbolic link {}", root.display()))
            }
            Ok(_) => expanded.push_diagnostic(format!("not a regular file: {}", root.display())),
        }
    }
    expanded
}

/// Open the expansion's FIRST candidate through the existing F6 Open
/// replacement, and ON COMMIT seed the player's navigation list with
/// the whole accepted candidate list (entry 0 IS the committed episode
/// — the same discipline as the argv startup seed). `None` = the
/// expansion produced no candidate, so the player was not touched at
/// all: no episode destroyed, no navigation state changed. A refused
/// or clean-failed first candidate commits no list either — the
/// misleading-list hazard of Issue #166 §10 cannot arise, because the
/// seed rides the same commit evidence as the episode itself.
pub fn open_expanded<S: EpisodeStart>(
    player: &mut ReferencePlayerApp<S>,
    expanded: &ExpandedInputs,
) -> Option<OpenOutcome> {
    let first = expanded.accepted.first()?;
    let outcome = player.open(first);
    if outcome == OpenOutcome::Opened {
        player.establish_playlist(expanded.accepted.clone());
    }
    Some(outcome)
}

/// What a product startup prepared from its argv before the shell
/// starts (U1, Issue #166). Every field is host input preparation and
/// its presentation — none is playback truth.
#[derive(Debug)]
pub struct StartupPreparation {
    /// The full input expansion. The DEFAULT (empty) expansion for an
    /// interactive launch, which expands nothing.
    pub expansion: ExpandedInputs,
    /// The startup Open's outcome through the F6 replacement, or
    /// `None` when no Open was attempted or no candidate existed.
    pub startup_open: Option<OpenOutcome>,
    /// The shell's initial operation-feedback block — the same
    /// vocabulary the O key reports, possibly multi-line (the bounded
    /// scan-warning detail). `None` = no feedback at all: the quiet
    /// complete single-file start, or the interactive launch that
    /// attempted nothing.
    pub initial_status: Option<String>,
}

/// The product launch's whole argv preparation (U1 corrective
/// REQUIRED-1): the two startup shapes are distinguished BEFORE any
/// Open. An INTERACTIVE launch (no argv paths) expands nothing, opens
/// nothing, and reports nothing — an operation that was never
/// attempted owes no refusal, and the shell's truthful idle page is
/// the whole startup story. An ARGV-driven start expands its paths and
/// opens the first candidate through the existing F6 replacement,
/// reporting honestly — including the empty/unreadable-expansion
/// refusal, which stays an argv-only shape.
pub fn prepare_startup<S: EpisodeStart>(
    files: &[PathBuf],
    player: &mut ReferencePlayerApp<S>,
) -> StartupPreparation {
    if files.is_empty() {
        return StartupPreparation {
            expansion: ExpandedInputs::default(),
            startup_open: None,
            initial_status: None,
        };
    }
    let expansion = expand_inputs(files);
    let startup_open = open_expanded(player, &expansion);
    let initial_status = startup_feedback(&expansion, startup_open.as_ref());
    StartupPreparation {
        expansion,
        startup_open,
        initial_status,
    }
}

/// The shell's initial feedback for an ARGV-driven start: the
/// expansion summary plus the startup Open's outcome. `None` on the
/// quiet successful single-file start (the old-world behavior); a
/// folder start summarizes what got seeded because that is the user's
/// only view of the expansion; a partial scan always speaks.
fn startup_feedback(
    expansion: &ExpandedInputs,
    startup_open: Option<&OpenOutcome>,
) -> Option<String> {
    match startup_open {
        None => {
            // No candidate anywhere in the argv paths: an honest
            // refusal, and the shell still runs so the user can open a
            // valid source. Same operation vocabulary as the in-shell
            // refusal.
            Some(format!("open refused: {}", expansion.refusal()))
        }
        Some(OpenOutcome::Opened) => {
            // The old-world quiet start: exactly one accepted
            // candidate, nothing skipped, complete enumeration — no
            // feedback line at all. Anything else reports (a folder
            // start's seed summary, a partial scan's warnings).
            let complete_single_file = expansion.accepted.len() == 1
                && expansion.skipped == 0
                && expansion.diagnostics.is_empty();
            if complete_single_file {
                None
            } else {
                Some(expansion.opened_status())
            }
        }
        Some(OpenOutcome::Refused { diagnostic }) => Some(format!("open refused: {diagnostic}")),
        Some(OpenOutcome::ActivationFailedClean { diagnostic }) => {
            Some(format!("open failed (clean): {diagnostic}"))
        }
        Some(OpenOutcome::FailStop { .. }) => {
            // The transport checks the returned outcome as soon as this
            // returns and never enters a shell over a §G.6 latch, so
            // this feedback would never be shown: None, and the
            // fail-stop report stays the transport's business.
            None
        }
    }
}

/// Extensions treated as audio candidates during DIRECTORY ENUMERATION
/// only. Cheap prefilter to avoid probing obvious non-audio; lowercase
/// and compared case-insensitively. Deliberately broad and deliberately
/// NOT authoritative: an explicit file path skips this list, and the
/// decode probe remains the playability witness for everything on it.
const AUDIO_EXTENSIONS: [&str; 18] = [
    "aac", "aif", "aiff", "alac", "ape", "caf", "dff", "dsf", "flac", "m4a", "m4b", "mp3", "oga",
    "ogg", "opus", "wav", "wma", "wv",
];

fn is_audio_candidate(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        let extension = extension.to_string_lossy().to_ascii_lowercase();
        AUDIO_EXTENSIONS.contains(&extension.as_str())
    })
}

/// Deterministic pre-order walk: each directory's entries are sorted by
/// name and visited in that order, descending into subdirectories as
/// they are met, so the candidate order is path-sorted and stable
/// across runs and machines. `DirEntry::file_type` never follows
/// symlinks, so directory symlinks/junctions are classified (skipped),
/// never traversed — traversal cycles are impossible by construction.
fn walk_directory(dir: &Path, expanded: &mut ExpandedInputs) {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(read_dir) => read_dir,
        Err(error) => {
            expanded.push_diagnostic(format!("cannot read {}: {error}", dir.display()));
            return;
        }
    };
    let mut entries = Vec::new();
    for entry in read_dir {
        match entry {
            Ok(entry) => entries.push(entry),
            Err(error) => {
                expanded.push_diagnostic(format!("cannot list {}: {error}", dir.display()))
            }
        }
    }
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        match entry.file_type() {
            Err(error) => {
                expanded.push_diagnostic(format!("cannot inspect {}: {error}", path.display()))
            }
            Ok(kind) if kind.is_dir() => walk_directory(&path, expanded),
            Ok(kind) if kind.is_file() => {
                if is_audio_candidate(&path) {
                    expanded.accepted.push(path);
                } else {
                    expanded.skipped += 1;
                }
            }
            Ok(_) => {
                // Symlinks (including directory junctions on Windows)
                // and every other non-regular entry: classified, never
                // followed.
                expanded.skipped += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Deterministic filesystem-enumeration tests over real temporary
    //! trees. These pin the ENUMERATION contract; the provider-probe
    //! (actual playability) integration stays with the F6 Open path —
    //! neither test class weakens the other (Issue #166 §14).

    use std::fs;
    use std::path::Path;

    use super::*;

    /// A fresh unique temporary directory for one test.
    struct TempTree(PathBuf);

    impl TempTree {
        fn new(name: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "qianqian-input-test-{}-{}",
                name,
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&base);
            fs::create_dir_all(&base).expect("temp tree root");
            Self(base)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn file(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("parent dir");
            }
            fs::write(&path, b"not really audio; enumeration never reads contents")
                .expect("temp file");
            path
        }

        fn dir(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(&path).expect("temp dir");
            path
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_single_regular_file_is_accepted_as_itself() {
        let tree = TempTree::new("single-file");
        let file = tree.file("song.flac");

        let expanded = expand_inputs([&file]);
        assert_eq!(expanded.accepted, vec![file]);
        assert_eq!(expanded.skipped, 0);
        assert!(expanded.diagnostics.is_empty());
    }

    /// An explicitly named file is accepted regardless of extension —
    /// the prefilter exists to slim down directory enumeration, not to
    /// second-guess a path the user named. The Open probe stays the
    /// playability witness for it.
    #[test]
    fn an_explicit_file_bypasses_the_extension_prefilter() {
        let tree = TempTree::new("explicit-weird");
        let file = tree.file("mystery.dat");

        let expanded = expand_inputs([&file]);
        assert_eq!(expanded.accepted, vec![file]);
    }

    #[test]
    fn a_directory_is_enumerated_sorted_with_unsupported_entries_skipped() {
        let tree = TempTree::new("flat-dir");
        // Created deliberately out of lexicographic order.
        tree.file("02 second.flac");
        tree.file("01 first.MP3");
        tree.file("cover.txt");
        tree.file("no_extension");
        tree.file("10 三番目 opus.ogg");

        let expanded = expand_inputs([tree.path()]);
        assert_eq!(
            expanded.accepted,
            vec![
                tree.path().join("01 first.MP3"),
                tree.path().join("02 second.flac"),
                tree.path().join("10 三番目 opus.ogg"),
            ],
            "path-sorted, case-insensitive extension, CJK and spaces kept"
        );
        assert_eq!(expanded.skipped, 2, "cover.txt and no_extension");
        assert!(expanded.diagnostics.is_empty());
    }

    #[test]
    fn nested_directories_enumerate_in_deterministic_preorder() {
        let tree = TempTree::new("nested");
        tree.file("b-album/02 b2.flac");
        tree.file("b-album/01 b1.flac");
        tree.file("a-single.flac");
        tree.file("c-album/disc2/02 deep.flac");
        tree.file("c-album/disc1/01 deeper.flac");

        let expanded = expand_inputs([tree.path()]);
        assert_eq!(
            expanded.accepted,
            vec![
                tree.path().join("a-single.flac"),
                tree.path().join("b-album/01 b1.flac"),
                tree.path().join("b-album/02 b2.flac"),
                tree.path().join("c-album/disc1/01 deeper.flac"),
                tree.path().join("c-album/disc2/02 deep.flac"),
            ],
            "path-sorted preorder: files and dirs interleave by name"
        );
    }

    /// The same tree expands to the same order on every call — the raw
    /// directory enumeration order must never leak into the result.
    #[test]
    fn enumeration_order_is_stable_across_calls() {
        let tree = TempTree::new("stability");
        for name in [
            "x.flac", "a.flac", "m.flac", "b/1.flac", "b/2.flac", "0.flac",
        ] {
            tree.file(name);
        }
        let first = expand_inputs([tree.path()]);
        let second = expand_inputs([tree.path()]);
        assert_eq!(first.accepted, second.accepted);
        assert!(!first.accepted.is_empty());
    }

    /// Multiple roots keep the USER's root order; each root's contents
    /// are internally deterministic. Play "B" then "A" and B's tracks
    /// come first even though A sorts lower.
    #[test]
    fn multiple_roots_keep_the_typed_order_with_sorted_contents() {
        let tree = TempTree::new("multi-root");
        let second = tree.dir("b second");
        tree.file("a first/1.flac");
        tree.file("a first/2.flac");
        tree.file("b second/1.flac");

        let expanded = expand_inputs([tree.path().join("b second"), tree.path().join("a first")]);
        assert_eq!(
            expanded.accepted,
            vec![
                second.join("1.flac"),
                tree.path().join("a first/1.flac"),
                tree.path().join("a first/2.flac"),
            ]
        );
    }

    #[test]
    fn an_empty_directory_refuses_without_diagnostics() {
        let tree = TempTree::new("empty-dir");
        let empty = tree.dir("empty");

        let expanded = expand_inputs([&empty]);
        assert!(expanded.accepted.is_empty());
        assert_eq!(expanded.skipped, 0);
        assert!(expanded.diagnostics.is_empty());
        assert_eq!(expanded.refusal(), "no audio candidates");
    }

    #[test]
    fn a_nonexistent_path_refuses_with_a_diagnostic() {
        let tree = TempTree::new("missing");
        let missing = tree.path().join("does-not-exist");

        let expanded = expand_inputs([&missing]);
        assert!(expanded.accepted.is_empty());
        assert_eq!(expanded.diagnostics.len(), 1);
        assert!(
            expanded.diagnostics[0].starts_with("cannot read "),
            "the diagnostic names the operation: {:?}",
            expanded.diagnostics[0]
        );
        assert_eq!(expanded.refusal(), expanded.diagnostics[0]);
    }

    /// A directory whose listing is denied produces a bounded
    /// diagnostic and does not abort the remaining roots.
    /// Permission bits are a unix simulation; on other platforms the
    /// equivalent case is covered by the nonexistent-path diagnostic.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_directory_becomes_a_bounded_diagnostic() {
        use std::os::unix::fs::PermissionsExt;

        let tree = TempTree::new("unreadable");
        let locked = tree.dir("locked");
        tree.file("kept.flac");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000))
            .expect("lock the directory");

        let expanded = expand_inputs([tree.path()]);
        // Restore FIRST so cleanup can remove the tree even on failure.
        let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o755));

        assert_eq!(expanded.accepted, vec![tree.path().join("kept.flac")]);
        assert_eq!(expanded.diagnostics.len(), 1);
        assert!(
            expanded.diagnostics[0].starts_with("cannot read "),
            "{:?}",
            expanded.diagnostics
        );
    }

    /// Directory symlinks are never followed — which is also what makes
    /// traversal cycles impossible. A `inner → root` link cannot hang
    /// or duplicate the tree; the link itself is a skipped entry.
    #[cfg(unix)]
    #[test]
    fn a_directory_symlink_cycle_is_never_followed() {
        let tree = TempTree::new("symlink-cycle");
        tree.file("real.flac");
        std::os::unix::fs::symlink(tree.path(), tree.path().join("inner"))
            .expect("create the cycle link");

        let expanded = expand_inputs([tree.path()]);
        assert_eq!(expanded.accepted, vec![tree.path().join("real.flac")]);
        assert_eq!(expanded.skipped, 1, "the symlink entry, once");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_root_is_reported_not_expanded() {
        let tree = TempTree::new("symlink-root");
        tree.file("real.flac");
        let link = tree.path().join("link");
        std::os::unix::fs::symlink(tree.path(), &link).expect("root link");

        let expanded = expand_inputs([&link]);
        assert!(expanded.accepted.is_empty(), "the target must not expand");
        assert!(expanded.diagnostics[0].starts_with("skipped symbolic link"));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_file_is_skipped_not_accepted() {
        let tree = TempTree::new("symlink-file");
        let target = tree.file("target.flac");
        let link = tree.path().join("link.flac");
        std::os::unix::fs::symlink(&target, &link).expect("file link");

        let expanded = expand_inputs([tree.path()]);
        assert_eq!(
            expanded.accepted,
            vec![target],
            "only the regular file; the link is a classified skip"
        );
        assert_eq!(expanded.skipped, 1);
    }

    /// More errors than the diagnostic budget must stay bounded: the
    /// report carries the cap and counts the rest, never unbounded.
    #[cfg(unix)]
    #[test]
    fn diagnostics_are_bounded_under_a_pathological_tree() {
        use std::os::unix::fs::PermissionsExt;

        let tree = TempTree::new("diagnostic-cap");
        // One MORE unreadable directory than the diagnostic cap.
        for n in 0..MAX_DIAGNOSTICS + 1 {
            let locked = tree.dir(&format!("locked-{n}"));
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o000))
                .expect("lock the directory");
        }
        let expanded = expand_inputs([tree.path()]);
        for n in 0..MAX_DIAGNOSTICS + 1 {
            let _ = fs::set_permissions(
                tree.path().join(format!("locked-{n}")),
                fs::Permissions::from_mode(0o755),
            );
        }

        assert_eq!(expanded.diagnostics.len(), MAX_DIAGNOSTICS);
        assert_eq!(expanded.suppressed_diagnostics, 1);
        assert!(
            expanded.refusal().ends_with("(+1 more)"),
            "the suppressed count is presented: {:?}",
            expanded.refusal()
        );
    }

    #[test]
    fn summary_counts_candidates_and_skips() {
        let expanded = ExpandedInputs {
            accepted: vec![PathBuf::from("a.flac"), PathBuf::from("b.flac")],
            skipped: 0,
            diagnostics: Vec::new(),
            suppressed_diagnostics: 0,
        };
        assert_eq!(expanded.summary(), "2 candidates");
        let expanded = ExpandedInputs {
            skipped: 3,
            ..expanded
        };
        assert_eq!(expanded.summary(), "2 candidates, 3 skipped");
        // One candidate reads singular.
        let one = ExpandedInputs {
            accepted: vec![PathBuf::from("a.flac")],
            ..expanded
        };
        assert_eq!(one.summary(), "1 candidate, 3 skipped");
        // A partial scan extends the line instead of hiding (U1
        // corrective REQUIRED-2), singular and plural, with the
        // suppressed count when present.
        let partial = ExpandedInputs {
            diagnostics: vec!["cannot read D:\\Music\\album-c: access denied".to_owned()],
            ..one
        };
        assert_eq!(partial.summary(), "1 candidate, 3 skipped; 1 scan warning");
        let worse = ExpandedInputs {
            diagnostics: vec![
                "cannot read d1: e1".to_owned(),
                "cannot read d2: e2".to_owned(),
            ],
            suppressed_diagnostics: 4,
            ..partial
        };
        assert_eq!(
            worse.summary(),
            "1 candidate, 3 skipped; 2 scan warnings (+4 more)"
        );
    }

    // --- opened_status: the successful Open's presentation block -----

    /// U1 corrective REQUIRED-2: candidates exist AND the scan was
    /// partial — the opened line counts the warnings and the bounded
    /// detail lines follow it, so a partially unreadable folder never
    /// looks complete.
    #[test]
    fn a_partial_scan_still_reports_when_a_candidate_opens() {
        let tree = TempTree::new("partial-opened");
        let file = tree.file("kept.flac");
        let missing = tree.path().join("locked-away");

        let expanded = expand_inputs([&missing, &file]);
        assert_eq!(
            expanded.accepted,
            vec![file.clone()],
            "the candidate survived"
        );
        assert_eq!(expanded.diagnostics.len(), 1, "the failure was recorded");

        let status = expanded.opened_status();
        assert!(
            status.starts_with(&format!("opened {} (", file.display())),
            "{status}"
        );
        assert!(status.contains("; 1 scan warning"), "{status}");
        assert!(
            status.contains("\nscan warning: cannot read "),
            "the diagnostic itself is not silently discarded: {status}"
        );
    }

    /// A complete enumeration keeps the old compact shapes: a single
    /// file reads plainly, a multi-candidate folder keeps its count.
    #[test]
    fn a_complete_expansion_keeps_the_compact_opened_lines() {
        let single = ExpandedInputs {
            accepted: vec![PathBuf::from("only.flac")],
            ..ExpandedInputs::default()
        };
        assert_eq!(single.opened_status(), "opened only.flac");
        let folder = ExpandedInputs {
            accepted: vec![PathBuf::from("a.flac"), PathBuf::from("b.flac")],
            skipped: 1,
            ..ExpandedInputs::default()
        };
        assert_eq!(
            folder.opened_status(),
            "opened a.flac (2 candidates, 1 skipped)"
        );
    }

    /// The warning block stays bounded under a pathological tree: the
    /// cap's detail lines plus the suppressed count (REQUIRED-2's
    /// bounded presentation).
    #[cfg(unix)]
    #[test]
    fn the_scan_warning_block_stays_bounded_under_a_pathological_tree() {
        use std::os::unix::fs::PermissionsExt;

        let tree = TempTree::new("warning-block-cap");
        let kept = tree.file("kept.flac");
        for n in 0..MAX_DIAGNOSTICS + 1 {
            let locked = tree.dir(&format!("locked-{n}"));
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o000))
                .expect("lock the directory");
        }
        let expanded = expand_inputs([tree.path()]);
        for n in 0..MAX_DIAGNOSTICS + 1 {
            let _ = fs::set_permissions(
                tree.path().join(format!("locked-{n}")),
                fs::Permissions::from_mode(0o755),
            );
        }

        assert_eq!(expanded.accepted, vec![kept]);
        let warnings = expanded.scan_warnings();
        assert_eq!(
            warnings.len(),
            MAX_DIAGNOSTICS + 1,
            "the cap's detail lines plus one (+N more) line: {warnings:?}"
        );
        assert!(
            warnings
                .iter()
                .all(|line| line.starts_with("scan warning: ") || line == "(+1 more)"),
            "{warnings:?}"
        );
        assert!(
            expanded.summary().ends_with("; 8 scan warnings (+1 more)"),
            "{}",
            expanded.summary()
        );
    }

    // --- prepare_startup: the two launch shapes (U1 corrective) ------

    /// REQUIRED-1: a bare launch expands nothing, opens nothing, and
    /// reports nothing — no fabricated refusal for an Open that was
    /// never attempted.
    #[test]
    fn the_interactive_launch_prepares_nothing_and_reports_nothing() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);

        let preparation = prepare_startup(&[], &mut player);

        assert!(preparation.expansion.accepted.is_empty());
        assert_eq!(preparation.startup_open, None, "no startup Open");
        assert_eq!(
            preparation.initial_status, None,
            "no operation feedback for an operation never attempted"
        );
        assert!(player.active_handle().is_none(), "no episode");
        assert_eq!(player.navigation_position(), None, "no playlist");
        assert!(log.lock().unwrap().is_empty(), "not even a probe ran");
    }

    /// REQUIRED-1's other half: the ARGV shape keeps its truthful
    /// refusal — zero candidates from real argv paths still reports
    /// `open refused: ...` (argv error reporting is not weakened).
    #[test]
    fn an_argv_start_over_an_empty_expansion_keeps_the_refusal() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let missing = PathBuf::from("/qianqian-corrective-does-not-exist");

        let preparation = prepare_startup(&[missing], &mut player);

        assert_eq!(preparation.startup_open, None);
        let status = preparation
            .initial_status
            .expect("an argv-driven emptiness is reported");
        assert!(
            status.starts_with("open refused: "),
            "the argv path keeps its refusal: {status}"
        );
        assert!(player.active_handle().is_none());
    }

    /// REQUIRED-2 full-stack: an argv start whose expansion partially
    /// failed still opens its candidate AND reports the partial scan
    /// in the very status block the shell will show.
    #[test]
    fn an_argv_start_over_a_partially_unreadable_folder_reports_the_warnings() {
        let tree = TempTree::new("startup-partial");
        let file = tree.file("kept live.flac");
        let missing = tree.path().join("locked-away");

        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let preparation = prepare_startup(&[missing, file.clone()], &mut player);

        assert_eq!(preparation.startup_open, Some(OpenOutcome::Opened));
        assert_eq!(player.active_source(), Some(file.as_path()));
        let status = preparation
            .initial_status
            .expect("a partial scan is reported");
        assert!(status.contains("; 1 scan warning"), "{status}");
        assert!(
            status.contains("\nscan warning: cannot read "),
            "the bounded detail rides the status block: {status}"
        );
        assert_eq!(player.navigation_position(), Some((1, 1)));
    }

    /// The quiet single-file start stays quiet (the old-world
    /// contract), even through prepare_startup.
    #[test]
    fn a_complete_single_file_argv_start_stays_quiet() {
        // A REAL file: the expansion reads the filesystem, so the
        // fake-harness LIVE_A constant would never be accepted.
        let tree = TempTree::new("startup-quiet");
        let file = tree.file("only live.flac");
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());

        let preparation = prepare_startup(std::slice::from_ref(&file), &mut player);

        assert_eq!(preparation.startup_open, Some(OpenOutcome::Opened));
        assert_eq!(preparation.initial_status, None);
        assert_eq!(player.active_source(), Some(file.as_path()));
    }

    // --- open_expanded: first-candidate Open + commit-riding seed -----

    use crate::player::tests::{FakeEpisodeSource, LIVE_A, LIVE_B};

    /// The first candidate opens through the frozen replacement and the
    /// whole candidate list is seeded ON COMMIT (cursor at entry 0).
    #[test]
    fn open_expanded_opens_the_first_candidate_and_seeds_on_commit() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let expanded = ExpandedInputs {
            accepted: vec![PathBuf::from(LIVE_A), PathBuf::from(LIVE_B)],
            skipped: 2,
            diagnostics: Vec::new(),
            suppressed_diagnostics: 0,
        };

        let outcome = open_expanded(&mut player, &expanded).expect("a candidate existed");
        assert_eq!(outcome, OpenOutcome::Opened);
        assert_eq!(player.active_source(), Some(Path::new(LIVE_A)));
        assert_eq!(
            player.navigation_position(),
            Some((1, 2)),
            "the accepted list is the navigation state, cursor on the committed entry"
        );
    }

    /// An empty expansion touches nothing: no episode, no playlist, no
    /// probe call — the empty/unreadable-folder refusal cannot destroy
    /// current playback (Issue #166 §10).
    #[test]
    fn open_expanded_over_an_empty_expansion_touches_nothing() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        assert_eq!(
            open_expanded(&mut player, &ExpandedInputs::default()),
            None,
            "no candidates: nothing to open, nothing touched"
        );
        assert!(player.active_handle().is_none());
        assert!(log.lock().unwrap().is_empty(), "not even a probe ran");
    }

    /// A refused FIRST candidate commits no list: the navigation state
    /// keeps exactly what it had (no misleading new list).
    #[test]
    fn a_refused_first_candidate_commits_no_list() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        assert_eq!(
            open_expanded(
                &mut player,
                &ExpandedInputs {
                    accepted: vec![
                        PathBuf::from("/media/invalid-x.flac"),
                        PathBuf::from(LIVE_B)
                    ],
                    skipped: 0,
                    diagnostics: Vec::new(),
                    suppressed_diagnostics: 0,
                }
            ),
            Some(OpenOutcome::Refused {
                diagnostic: "unsupported container: /media/invalid-x.flac".to_owned()
            })
        );
        assert_eq!(
            player.navigation_position(),
            None,
            "no playlist was committed behind the refusal"
        );
    }
}
