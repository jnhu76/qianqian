//! Host input preparation for the product launch (U1, Issue #166):
//! user-supplied paths — a file, a folder, or several of either —
//! expanded into one ordered list of playable candidates.
//!
//! Classification of this code (deliberately unexciting): it is an
//! ordinary application function of the composition root, NOT a Plugin
//! (PBK-002 D13: an existing owner — the App's input preparation —
//! expresses it completely; no independent composition identity is
//! claimed), and it is NOT playback semantics. It knows the filesystem
//! and nothing else: no decode capability, no probe, no PCM, no
//! composition types.
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
//! path depth.

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
                "no playable files; {skipped} entries skipped",
                skipped = self.skipped
            )
        } else {
            "no playable files".to_owned()
        }
    }

    /// One-line presentation summary of a non-empty expansion
    /// ("3 candidates, 1 skipped"). "Candidates" is the honest noun:
    /// playability is witnessed downstream, not here.
    pub fn summary(&self) -> String {
        let mut summary = format!("{} candidates", self.accepted.len());
        if self.skipped > 0 {
            summary.push_str(&format!(", {} skipped", self.skipped));
        }
        summary
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
        player.seed_startup_playlist(expanded.accepted.clone());
    }
    Some(outcome)
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
        assert_eq!(expanded.refusal(), "no playable files");
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
