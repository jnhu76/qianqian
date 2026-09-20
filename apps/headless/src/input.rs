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
//! accepted     scan candidates in deterministic order that the
//!              scan-time media probe ACCEPTED. An accepted path is
//!              still a CANDIDATE, never playback truth: the probe is
//!              the decode provider's advisory preflight evidence
//!              (open → facts → close, no PCM) and the episode
//!              activation's own evidence stays authoritative — a file
//!              can pass here and still fail later (or vanish), which
//!              is exactly why the runtime `Failed` terminal stays
//!              possible and is never auto-skipped. The extension
//!              prefilter below only reduces useless probe calls for
//!              enumerated directory contents; extension alone is never
//!              treated as proof of playability, and an EXPLICITLY named
//!              file bypasses the prefilter entirely so the probe keeps
//!              the final word.
//! skipped      enumerated entries classified as not-audio-candidate
//!              (wrong/absent extension, symlinks/junctions, anything
//!              that is not a regular file). Bounded counts, not
//!              verdicts.
//! duplicates   the SAME accepted path reached twice by input
//!              expansion (the same root named twice, or overlapping
//!              roots) is kept ONCE, first occurrence order. Lexical
//!              identity only — see [`dedup_key`]. No content
//!              fingerprint, no inode authority.
//! rejected     candidates the scan-time media probe refused (corrupt
//!              files, zero-byte audio extensions, renamed garbage).
//!              They never enter the playlist; bounded counts and
//!              bounded name detail, never verdicts about WHY beyond
//!              the probe's own diagnostic.
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
    /// Scan candidates the media probe accepted, in the deterministic
    /// order they should be presented/opened (see the module docs for
    /// the truth class: probe-accepted candidates, never playback
    /// truth).
    pub accepted: Vec<PathBuf>,
    /// Enumerated entries classified as non-candidates (unsupported
    /// extension, symlinks/junctions, non-regular files).
    pub skipped: usize,
    /// Exact-duplicate accepted paths removed while keeping the first
    /// occurrence (Issue: duplicate input roots / repeated explicit
    /// files). Lexical rule only — see [`dedup_key`].
    pub duplicates: usize,
    /// Candidates the scan-time media probe refused (corrupt or
    /// unplayable files that passed the extension prefilter).
    pub rejected: usize,
    /// Bounded file names of probe-rejected candidates, in encounter
    /// order.
    rejected_names: Vec<String>,
    /// How many further rejected names were suppressed by
    /// [`MAX_DIAGNOSTICS`]. Presentation counts, never semantics.
    suppressed_rejected: usize,
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

    /// Record one probe-rejected candidate: the bounded name detail
    /// and the count. The name shown is the FILE name (the row-label
    /// vocabulary); the full path stays in the caller's context.
    fn push_rejected(&mut self, path: &Path) {
        self.rejected += 1;
        let name = match path.file_name() {
            Some(name) => name.to_string_lossy().into_owned(),
            None => path.to_string_lossy().into_owned(),
        };
        if self.rejected_names.len() < MAX_DIAGNOSTICS {
            self.rejected_names.push(name);
        } else {
            self.suppressed_rejected += 1;
        }
    }

    /// Why there is nothing to open. Used only when [`Self::accepted`]
    /// is empty: the first filesystem diagnostic if one was reported
    /// (with the suppressed count when present), otherwise the honest
    /// classification summary — including the all-candidates-rejected
    /// shape of a folder whose every audio-looking file failed the
    /// media probe.
    pub fn refusal(&self) -> String {
        if let Some(first) = self.diagnostics.first() {
            if self.suppressed_diagnostics > 0 {
                format!("{first} (+{} more)", self.suppressed_diagnostics)
            } else {
                first.clone()
            }
        } else if self.rejected > 0 {
            let mut refusal = format!(
                "no playable audio files found; {rejected} unplayable",
                rejected = self.rejected
            );
            if self.suppressed_rejected > 0 {
                refusal.push_str(&format!(" (+{} more)", self.suppressed_rejected));
            }
            refusal
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
    /// ("2 candidates, 1 skipped"), extended with the duplicate and
    /// unplayable counts when present and with the bounded scan-warning
    /// count when the traversal was PARTIAL ("...; 2 scan warnings
    /// (+3 more)") — a partially unreadable folder must never present
    /// as completely loaded (U1 corrective REQUIRED-2). "Candidates" is
    /// the honest noun: playability is witnessed by the probe and the
    /// episode activation, not by this summary.
    pub fn summary(&self) -> String {
        let mut summary = format!(
            "{} candidate{}",
            self.accepted.len(),
            if self.accepted.len() == 1 { "" } else { "s" }
        );
        if self.skipped > 0 {
            summary.push_str(&format!(", {} skipped", self.skipped));
        }
        if self.duplicates > 0 {
            summary.push_str(&format!(
                ", {duplicates} duplicate{plural} removed",
                duplicates = self.duplicates,
                plural = if self.duplicates == 1 { "" } else { "s" }
            ));
        }
        if self.rejected > 0 {
            summary.push_str(&format!(
                ", {rejected} unplayable",
                rejected = self.rejected
            ));
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

    /// The bounded scan-finding detail lines for an expansion that
    /// FOUND candidates but also hit filesystem errors or probe
    /// rejections. Every filesystem diagnostic becomes one
    /// `scan warning: ...` line, every rejected candidate one
    /// `not playable: <name>` line; each suppressed tail is counted on
    /// a final `(+N more)` line, so the block stays bounded however
    /// pathological the tree.
    ///
    /// Empty when there is nothing to report: a complete enumeration
    /// (no diagnostics, no rejections), or a refused expansion (no
    /// candidates — [`Self::refusal`] carries the story instead).
    ///
    /// Truth class: application/host diagnostics about the SCAN.
    /// Never playback Facts, never terminal outcomes, never activation
    /// failures.
    pub fn scan_warnings(&self) -> Vec<String> {
        if self.accepted.is_empty() {
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
        lines.extend(
            self.rejected_names
                .iter()
                .map(|name| format!("not playable: {name}")),
        );
        if self.suppressed_rejected > 0 {
            lines.push(format!("(+{} more)", self.suppressed_rejected));
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
        let worth_summarizing = self.accepted.len() > 1
            || self.skipped > 0
            || self.duplicates > 0
            || self.rejected > 0
            || !self.diagnostics.is_empty();
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

/// The lexical duplicate-identity key (the precise normalization rule,
/// recorded because Windows path spelling makes it matter): the path
/// text exactly as enumerated, with Windows folding case and forward
/// slashes (the filesystem is case-insensitive there, and `D:/Music`
/// and `D:\Music` name the same tree). No content identity, no
/// file-ID authority — only "the same path reached twice".
fn dedup_key(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        text.to_lowercase().replace('/', "\\")
    } else {
        text.into_owned()
    }
}

/// Expand user-supplied input roots (files and/or directories) into
/// one candidate list, removing exact duplicate paths while keeping
/// first-occurrence order. See the module docs for the traversal and
/// truth-class contract.
pub fn expand_inputs<I>(roots: I) -> ExpandedInputs
where
    I: IntoIterator,
    I::Item: AsRef<Path>,
{
    let mut expanded = ExpandedInputs::default();
    let mut seen = std::collections::HashSet::new();
    for root in roots {
        let root = root.as_ref();
        // symlink_metadata never follows the root link, so a symlinked
        // root is reported as what it is instead of silently expanding
        // its target (or its cycle).
        match std::fs::symlink_metadata(root) {
            Err(error) => {
                expanded.push_diagnostic(format!("cannot read {}: {error}", root.display()))
            }
            Ok(meta) if meta.file_type().is_dir() => walk_directory(root, &mut expanded, &mut seen),
            Ok(meta) if meta.file_type().is_file() => {
                // An explicitly named file bypasses the extension
                // prefilter: the decode probe, not the extension, is
                // the playability witness (the same root spelled twice
                // is still one candidate).
                remember_candidate(&mut expanded, &mut seen, root.to_path_buf());
            }
            Ok(meta) if meta.file_type().is_symlink() => {
                expanded.push_diagnostic(format!("skipped symbolic link {}", root.display()))
            }
            Ok(_) => expanded.push_diagnostic(format!("not a regular file: {}", root.display())),
        }
    }
    expanded
}

/// Accept one enumerated/explicit candidate unless the exact same path
/// was already accepted: the FIRST occurrence keeps its place, a later
/// duplicate is counted and dropped.
fn remember_candidate(
    expanded: &mut ExpandedInputs,
    seen: &mut std::collections::HashSet<String>,
    path: PathBuf,
) {
    if seen.insert(dedup_key(&path)) {
        expanded.accepted.push(path);
    } else {
        expanded.duplicates += 1;
    }
}

/// Filter the expansion's candidates through the decode provider's
/// media probe (the playability pipeline's existing witness — no
/// parallel format detector is created here): a refused candidate
/// never enters the playlist and is counted/reported boundedly, and
/// the first SURVIVING candidate is what an Open should start from.
/// Advisory preflight evidence only — the episode activation's own
/// evidence stays authoritative, so a runtime `Failed` remains
/// possible and is never auto-skipped.
fn validate_with_probe<S: EpisodeStart>(
    player: &ReferencePlayerApp<S>,
    expanded: &mut ExpandedInputs,
) {
    let candidates = std::mem::take(&mut expanded.accepted);
    let mut kept = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        match player.probe_candidate(&candidate) {
            Ok(()) => kept.push(candidate),
            Err(_) => expanded.push_rejected(&candidate),
        }
    }
    expanded.accepted = kept;
}

/// Open the expansion's FIRST probe-accepted candidate through the
/// existing F6 Open replacement, and ON COMMIT seed the player's
/// navigation list with the whole accepted candidate list (entry 0 IS
/// the committed episode — the same discipline as the argv startup
/// seed). `None` = the expansion produced no playable candidate, so
/// the player was not touched at all: no episode destroyed, no
/// navigation state changed. A refused or clean-failed first candidate
/// commits no list either — the misleading-list hazard cannot arise,
/// because the seed rides the same commit evidence as the episode
/// itself.
pub fn open_expanded<S: EpisodeStart>(
    player: &mut ReferencePlayerApp<S>,
    expanded: &mut ExpandedInputs,
) -> Option<OpenOutcome> {
    validate_with_probe(player, expanded);
    let first = expanded.accepted.first()?.clone();
    let outcome = player.open(&first);
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
    let mut expansion = expand_inputs(files);
    let startup_open = open_expanded(player, &mut expansion);
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
/// only view of the expansion; a partial scan, duplicate removal or
/// probe rejection always speaks.
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
            // candidate, nothing skipped, removed or rejected, complete
            // enumeration — no feedback line at all. Anything else
            // reports (a folder start's seed summary, a partial scan's
            // warnings, unplayable files).
            let complete_single_file = expansion.accepted.len() == 1
                && expansion.skipped == 0
                && expansion.duplicates == 0
                && expansion.rejected == 0
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
/// Duplicate paths met through overlapping roots are removed by
/// [`remember_candidate`] on first-occurrence order.
fn walk_directory(
    dir: &Path,
    expanded: &mut ExpandedInputs,
    seen: &mut std::collections::HashSet<String>,
) {
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
            Ok(kind) if kind.is_dir() => walk_directory(&path, expanded, seen),
            Ok(kind) if kind.is_file() => {
                if is_audio_candidate(&path) {
                    remember_candidate(expanded, seen, path);
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
            ..ExpandedInputs::default()
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

    /// The summary extends with the duplicate and unplayable counts
    /// when present (duplicate inputs and probe rejections are part of
    /// what the user needs to see about a scan).
    #[test]
    fn summary_counts_duplicates_and_unplayable_files() {
        let expanded = ExpandedInputs {
            accepted: vec![PathBuf::from("a.flac")],
            duplicates: 2,
            rejected: 3,
            ..ExpandedInputs::default()
        };
        assert_eq!(
            expanded.summary(),
            "1 candidate, 2 duplicates removed, 3 unplayable"
        );
        let single = ExpandedInputs {
            duplicates: 1,
            ..expanded
        };
        assert_eq!(
            single.summary(),
            "1 candidate, 1 duplicate removed, 3 unplayable"
        );
        // The quiet complete single-file start vocabulary is unchanged
        // when nothing was deduped or rejected.
        let quiet = ExpandedInputs {
            duplicates: 0,
            rejected: 0,
            ..single
        };
        assert_eq!(quiet.summary(), "1 candidate");
    }

    // --- opened_status: the successful Open's presentation block -----

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

    /// Probe rejections ride the same bounded detail block under the
    /// `not playable:` vocabulary, so a folder with corrupt files is
    /// visible without being a failure storm.
    #[test]
    fn rejected_candidates_render_bounded_not_playable_lines() {
        let names: Vec<String> = (0..MAX_DIAGNOSTICS + 1)
            .map(|n| format!("broken-{n:02}.flac"))
            .collect();
        // The cap-consistent state: MAX_DIAGNOSTICS listed names, one
        // suppressed beyond them.
        let expanded = ExpandedInputs {
            accepted: vec![PathBuf::from("kept.flac")],
            rejected: names.len(),
            rejected_names: names[..MAX_DIAGNOSTICS].to_vec(),
            suppressed_rejected: 1,
            ..ExpandedInputs::default()
        };
        let status = expanded.opened_status();
        assert!(
            status.contains(&format!("1 candidate, {} unplayable", names.len())),
            "{status}"
        );
        assert!(
            status.contains(&format!("\nnot playable: {}", names[0])),
            "{status}"
        );
        assert!(
            status.contains(&format!("not playable: {}", names[MAX_DIAGNOSTICS - 1])),
            "the last name INSIDE the cap is listed: {status}"
        );
        assert!(
            !status.contains(&format!("not playable: {}", names[MAX_DIAGNOSTICS])),
            "the capped name is not listed: {status}"
        );
        assert!(status.contains("(+1 more)"), "{status}");
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

    // --- open_expanded: scan-time probe validation + commit-riding seed -

    use crate::player::tests::{FakeEpisodeSource, LIVE_A, LIVE_B};

    /// The first PROBE-ACCEPTED candidate opens through the frozen
    /// replacement and the whole accepted candidate list is seeded ON
    /// COMMIT (cursor at entry 0).
    #[test]
    fn open_expanded_opens_the_first_candidate_and_seeds_on_commit() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut expanded = ExpandedInputs {
            accepted: vec![PathBuf::from(LIVE_A), PathBuf::from(LIVE_B)],
            skipped: 2,
            ..ExpandedInputs::default()
        };

        let outcome = open_expanded(&mut player, &mut expanded).expect("a candidate existed");
        assert_eq!(outcome, OpenOutcome::Opened);
        assert_eq!(player.active_source(), Some(Path::new(LIVE_A)));
        assert_eq!(
            player.navigation_position(),
            Some((1, 2)),
            "the accepted list is the navigation state, cursor on the committed entry"
        );
    }

    /// A probe-REFUSED candidate is dropped at scan time (counted and
    /// reported boundedly) and the folder still opens its next good
    /// candidate — one corrupt first file must not refuse the whole
    /// folder.
    #[test]
    fn a_probe_refused_candidate_is_dropped_and_the_next_opens() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        let mut expanded = ExpandedInputs {
            accepted: vec![
                PathBuf::from("/media/invalid-x.flac"),
                PathBuf::from(LIVE_B),
            ],
            ..ExpandedInputs::default()
        };

        let outcome = open_expanded(&mut player, &mut expanded).expect("a playable candidate");
        assert_eq!(outcome, OpenOutcome::Opened);
        assert_eq!(player.active_source(), Some(Path::new(LIVE_B)));
        assert_eq!(player.navigation_position(), Some((1, 1)));
        assert_eq!(expanded.rejected, 1, "the corrupt candidate is counted");
        assert_eq!(
            expanded.rejected_names,
            vec!["invalid-x.flac".to_owned()],
            "the bounded name detail names it"
        );
        assert!(
            expanded
                .opened_status()
                .contains("not playable: invalid-x.flac"),
            "{}",
            expanded.opened_status()
        );
    }

    /// When EVERY candidate fails the probe the player is not touched
    /// at all: no episode, no playlist, no Open attempted — the honest
    /// all-corrupt refusal.
    #[test]
    fn an_all_corrupt_expansion_refuses_without_touching_the_player() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        let mut expanded = ExpandedInputs {
            accepted: vec![PathBuf::from("/media/invalid-a.flac")],
            ..ExpandedInputs::default()
        };

        assert_eq!(
            open_expanded(&mut player, &mut expanded),
            None,
            "no playable candidate: nothing to open, nothing touched"
        );
        assert!(player.active_handle().is_none());
        assert_eq!(
            log.lock().unwrap().len(),
            1,
            "exactly the one scan-time probe ran, no Open attempt"
        );
        assert_eq!(
            expanded.refusal(),
            "no playable audio files found; 1 unplayable"
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
            open_expanded(&mut player, &mut ExpandedInputs::default()),
            None,
            "no candidates: nothing to open, nothing touched"
        );
        assert!(player.active_handle().is_none());
        assert!(log.lock().unwrap().is_empty(), "not even a probe ran");
    }

    // --- duplicate inputs (Listening Release: exact-path dedup) ----

    /// The same root named twice yields each accepted path ONCE, in
    /// first-occurrence order; the duplicates are counted.
    #[test]
    fn a_root_named_twice_deduplicates_exact_paths() {
        let tree = TempTree::new("dup-root");
        let first = tree.file("a first.flac");
        let second = tree.file("b second.flac");

        let expanded = expand_inputs([tree.path(), tree.path()]);
        assert_eq!(
            expanded.accepted,
            vec![first, second],
            "each path once, first-occurrence order"
        );
        assert_eq!(expanded.duplicates, 2);
        assert_eq!(expanded.summary(), "2 candidates, 2 duplicates removed");
    }

    /// The same explicit file passed twice is one candidate; two
    /// different files stay two.
    #[test]
    fn an_explicit_file_passed_twice_is_one_candidate() {
        let tree = TempTree::new("dup-file");
        let a = tree.file("a.flac");
        let b = tree.file("b.flac");

        let expanded = expand_inputs([&a, &b, &a]);
        assert_eq!(expanded.accepted, vec![a, b]);
        assert_eq!(expanded.duplicates, 1);
    }

    /// Overlapping roots meet exactly once per path: the shared child
    /// keeps its FIRST-occurrence place and is not re-accepted when
    /// the second root walks over it again.
    #[test]
    fn overlapping_roots_deduplicate_shared_children() {
        let tree = TempTree::new("dup-overlap");
        let shared = tree.file("sub/shared.flac");
        let outer_only = tree.file("outer.flac");

        let expanded = expand_inputs([tree.path(), tree.path().join("sub").as_path()]);
        assert_eq!(
            expanded.accepted,
            vec![outer_only, shared],
            "outer first (first root's order), shared once"
        );
        assert_eq!(expanded.duplicates, 1);
    }

    /// The dedup key's precise lexical rule: exact path text on
    /// case-sensitive filesystems; case- and slash-folded on Windows
    /// (`D:/Music` and `D:\Music` name the same tree there). Pure
    /// string behavior, pinned on every host.
    #[test]
    fn the_dedup_key_folds_case_and_slashes_only_for_windows() {
        // The fold rule itself, exercised directly.
        assert_eq!(
            dedup_key(Path::new("D:\\Music\\A.FLAC")),
            if cfg!(windows) {
                "d:\\music\\a.flac".to_owned()
            } else {
                "D:\\Music\\A.FLAC".to_owned()
            }
        );
        // And the Windows-fold predicate as text: two spellings of the
        // same tree collapse only under the Windows rule.
        let left = "D:/Music/song.flac";
        let right = "D:\\MUSIC\\song.flac";
        let fold = |text: &str| text.to_lowercase().replace('/', "\\");
        assert_eq!(fold(left), fold(right));
    }

    /// A large real tree enumerates completely and stays ordered —
    /// the 1,000-entry playlist reality this campaign pins (structural
    /// claim only; timing belongs to the physical dogfood evidence).
    #[test]
    fn a_thousand_file_tree_enumerates_completely() {
        let tree = TempTree::new("thousand");
        for n in 0..1_000 {
            tree.file(&format!("album-{n:03}/track.flac"));
        }
        let expanded = expand_inputs([tree.path()]);
        assert_eq!(expanded.accepted.len(), 1_000);
        assert_eq!(expanded.duplicates, 0);
        // Path-sorted preorder: the first candidate is album-000's
        // Path-sorted preorder: the first candidate is album-000's
        // track and the last is album-999's.
        assert!(expanded.accepted[0].ends_with("album-000/track.flac"));
        assert!(expanded.accepted[999].ends_with("album-999/track.flac"));
    }

    /// The REAL SongCore media probe against the repository's
    /// committed fixtures and real corrupt-file shapes — the F2/F3
    /// classes of the physical failure matrix, exercised wherever the
    /// native decode artifact exists (the Linux dev host and the
    /// Windows build both qualify; plain CI hosts without the artifact
    /// never compile this module).
    #[cfg(all(test, feature = "playback"))]
    mod real_probe {
        use std::fs;
        use std::path::{Path, PathBuf};

        use super::super::{ExpandedInputs, expand_inputs, validate_with_probe};
        use crate::player::{EpisodeStart, ReferencePlayerApp};

        /// The real probe wiring (the same query `entry`'s
        /// RealEpisodeSource uses). These tests never START an
        /// episode — activation needs a real output device, which a
        /// scan test must not depend on — they witness the SCAN side:
        /// what the expansion keeps, counts and refuses.
        struct RealProbeOnly;

        impl EpisodeStart for RealProbeOnly {
            fn probe(&self, candidate: &Path) -> Result<(), String> {
                qianqian_decode_songcore::probe_media(candidate)
                    .map(|_facts| ())
                    .map_err(|e| e.message)
            }
            fn start(&self, _source: &Path, _level: u8) -> crate::player::StartAttempt {
                unreachable!("scan-hardening tests never start an episode")
            }
        }

        fn validated(tree: &Path) -> (ReferencePlayerApp<RealProbeOnly>, ExpandedInputs) {
            let player = ReferencePlayerApp::new(RealProbeOnly);
            let mut expansion = expand_inputs([tree]);
            validate_with_probe(&player, &mut expansion);
            (player, expansion)
        }

        /// A temp tree seeded with the repository's committed fixtures
        /// plus real corrupt-file shapes.
        struct RealTree(PathBuf);

        impl RealTree {
            fn new(name: &str, fixtures: &[&str]) -> Self {
                let root = std::env::temp_dir().join(format!(
                    "qianqian-realprobe-{}-{}-{name}",
                    std::process::id(),
                    fixtures.len()
                ));
                let _ = fs::remove_dir_all(&root);
                fs::create_dir_all(&root).expect("temp tree root");
                let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native/experiments");
                for fixture in fixtures {
                    fs::copy(
                        repo.join("songcore-equivalence/fixtures").join(fixture),
                        root.join(fixture),
                    )
                    .expect("committed fixture exists");
                }
                Self(root)
            }

            fn path(&self) -> &Path {
                &self.0
            }

            /// A garbage file with an audio-looking name (renamed
            /// non-audio bytes — the F3 class).
            fn garbage(&self, name: &str) -> PathBuf {
                let path = self.0.join(name);
                fs::write(
                    &path,
                    b"this is definitely not a flac stream, just text bytes",
                )
                .expect("garbage file");
                path
            }

            /// A zero-byte file with an audio extension (the F2 class).
            fn zero_byte(&self, name: &str) -> PathBuf {
                let path = self.0.join(name);
                fs::write(&path, b"").expect("zero-byte file");
                path
            }

            /// A quiet non-audio file (the F1 class: covers, notes).
            fn noise(&self, name: &str) -> PathBuf {
                let path = self.0.join(name);
                fs::write(&path, b"not audio at all").expect("noise file");
                path
            }
        }

        impl Drop for RealTree {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        /// The field-corpus shape (U2): an MP3 whose embedded cover art
        /// rides as an attached-picture stream this trimmed FFmpeg build
        /// has no decoder for. The cover is not audio truth, so the file
        /// must stay a playable candidate, keep its duration, and probe
        /// cleanly — the stream-info hunt it triggers is bounded and
        /// silent inside SongCore since the probe-cost fix (the physical
        /// transcript gate pins the clean screen; this test pins the
        /// acceptance side over the committed synthetic fixture).
        #[test]
        fn an_mp3_with_embedded_cover_art_stays_playable() {
            let source =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mp3-cbr-cover.mp3");
            let root = std::env::temp_dir()
                .join(format!("qianqian-realprobe-cover-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("temp tree root");
            let candidate = root.join("01 cover song.mp3");
            fs::copy(&source, &candidate).expect("cover fixture exists");

            let facts = qianqian_decode_songcore::probe_media(&candidate)
                .expect("cover-art mp3 probes as playable");
            assert_eq!(facts.format.sample_rate, 44100);
            assert!(facts.duration.is_some(), "the cover hides no duration");

            let player = ReferencePlayerApp::new(RealProbeOnly);
            let mut expansion = expand_inputs([root.as_path()]);
            validate_with_probe(&player, &mut expansion);
            assert_eq!(
                expansion.accepted,
                vec![candidate],
                "the cover-art mp3 is the one candidate"
            );
            assert_eq!(expansion.rejected, 0, "nothing to reject");
            assert_eq!(expansion.skipped, 0);

            let _ = fs::remove_dir_all(&root);
        }

        /// F1+F3 together: a realistic music folder — real track,
        /// cover/notes noise, a renamed-garbage "track" — keeps exactly
        /// its real track as a candidate, classifies the noise as quiet
        /// skips, and reports the corrupt file boundedly.
        #[test]
        fn a_realistic_folder_keeps_the_real_track_and_reports_the_corrupt_one() {
            let tree = RealTree::new("realistic", &["flac-16-44-stereo.flac"]);
            tree.garbage("03 broken take.flac");
            tree.noise("cover.jpg");
            tree.noise("notes.txt");
            tree.noise("lyric.lrc");

            let (_player, expansion) = validated(tree.path());

            assert_eq!(
                expansion.accepted,
                vec![tree.path().join("flac-16-44-stereo.flac")],
                "the real track is the one candidate"
            );
            assert_eq!(expansion.skipped, 3, "cover/notes/lrc are quiet skips");
            assert_eq!(expansion.rejected, 1, "the renamed garbage is rejected");
            let status = expansion.opened_status();
            assert!(
                status.contains("not playable: 03 broken take.flac"),
                "{status}"
            );
            assert!(!status.contains("cover.jpg"), "noise stays quiet: {status}");
        }

        /// F2: a zero-byte .mp3 never reaches the candidate list; the
        /// real MP3 beside it does.
        #[test]
        fn a_zero_byte_audio_file_is_rejected_not_listed() {
            let tree = RealTree::new("zerobyte", &["mp3-cbr-id3v23.mp3"]);
            tree.zero_byte("00 empty.mp3");

            let (_player, expansion) = validated(tree.path());

            assert_eq!(
                expansion.accepted,
                vec![tree.path().join("mp3-cbr-id3v23.mp3")]
            );
            assert_eq!(expansion.rejected, 1);
            assert_eq!(expansion.rejected_names, vec!["00 empty.mp3".to_owned()]);
        }

        /// The all-corrupt folder: an honest refusal with nothing
        /// playable claimed.
        #[test]
        fn an_all_corrupt_folder_refuses_honestly() {
            let tree = RealTree::new("allcorrupt", &[]);
            tree.garbage("a.flac");
            tree.garbage("b.mp3");

            let (_player, expansion) = validated(tree.path());

            assert!(expansion.accepted.is_empty());
            assert_eq!(
                expansion.refusal(),
                "no playable audio files found; 2 unplayable"
            );
        }

        /// A corrupt file that SORTS FIRST must not refuse the folder:
        /// after validation the first surviving candidate is the real
        /// track, so the startup Open has a playable first candidate.
        #[test]
        fn a_corrupt_first_file_does_not_block_the_real_track() {
            let tree = RealTree::new("corruptfirst", &["flac-16-44-stereo.flac"]);
            tree.garbage("00 broken.flac");

            let (_player, expansion) = validated(tree.path());

            assert_eq!(
                expansion.accepted.first().map(PathBuf::as_path),
                Some(tree.path().join("flac-16-44-stereo.flac").as_path()),
                "the real track is the first surviving candidate"
            );
            assert_eq!(expansion.rejected, 1);
        }
    }
}
