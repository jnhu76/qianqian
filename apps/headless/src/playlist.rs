//! The temporary playlist: App-owned product navigation state and its
//! ordering / repeat policy (Issue #166, `WINDOWS-TUI-USABILITY-
//! CLOSURE-1`, amending ADR-PBK-002 D14.6's playlist freeze).
//!
//! Classification (deliberately unexciting, and the reason nothing here
//! is a Plugin): this is ordinary reference-player App state — the
//! smallest coherent model that expresses the earned product behavior.
//! It owns no composition identity, no Capability, no K0 participant
//! role, no Fact, no observable, no thread, no I/O. It is pure product
//! policy over paths, and the ONE thing outside it that knows about it
//! is [`crate::player::ReferencePlayerApp`].
//!
//! Truth classes:
//!
//! ```text
//! entries        the sources the user opened, in discovery order. A
//!                path is a source identity, never playback truth:
//!                playability is still witnessed by the F6 Open probe.
//! play_order     this App's traversal policy — a permutation of the
//!                entry ids. Product state, never a Fact.
//! playing        commit-on-activation navigation state: it moves on
//!                Open/replacement COMMIT evidence only, is never
//!                playback truth, and after a clean-failed Open it names
//!                the last committed entry (no episode is live then —
//!                honest, because the cursor is navigation state).
//! selected       presentation browsing state. It never moves playback.
//! order/repeat   the user's player preferences for this process.
//! ```
//!
//! # The four things that must not collapse into each other
//!
//! ```text
//! entry identity   index into `entries` — a stable track identity
//! play traversal   index into `play_order` — a position in the current
//!                  order, which is what N/P, EOF policy and the
//!                  playlist pane all walk
//! playing          the traversal position of the committed episode
//! selected         the traversal position the UI cursor is on
//! ```
//!
//! Entry identity and traversal position are two DIFFERENT index
//! spaces, and the only place they meet is [`TemporaryPlaylist`]'s
//! private accessors below ([`Self::entry_at`] /
//! [`Self::position_of_entry`]). Every public operation is written in
//! traversal-position terms and every path lookup goes through those
//! two functions, so a traversal position can never be silently used as
//! an entry id: that mistake has exactly one place to happen, and the
//! policy matrix in this module's tests pins it (including the Shuffle
//! cases where the two spaces genuinely disagree).
//!
//! # Shuffle
//!
//! Shuffle is a STABLE PERMUTATION, never a per-Next random choice
//! (which would repeat tracks and make Previous incoherent): a cycle is
//! built once, then Next/Previous/natural EOF all walk that one
//! permutation. Entering Shuffle anchors the currently committed entry
//! as the first position of a fresh cycle over the other entries
//! exactly once; leaving it restores the canonical traversal with the
//! traversal cursor following the committed entry to its canonical
//! location. Neither direction restarts or replaces the episode.
//!
//! The permutation MECHANISM is separated from the policy: it is
//! [`ShufflePermuter`], a ~30-line deterministic generator over a seed,
//! held as an ordinary private field. Production seeds it from OS
//! entropy; tests seed it explicitly, so every structural shuffle claim
//! (completeness, no duplicates, anchoring, determinism) is
//! reproducible. It is not a Plugin, not a Capability, not an RNG
//! abstraction framework (PBK-002 D13: an existing owner — this App
//! state — expresses it completely).

use std::path::{Path, PathBuf};

/// The playlist's traversal policy (Issue #166 §6). Two modes, and no
/// others: `Random`/`ShuffleOnce`-style third modes are not earned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackOrder {
    /// The canonical (discovery) order of the opened sources.
    Sequential,
    /// A stable permutation, rebuilt only when a cycle begins.
    Shuffle,
}

impl PlaybackOrder {
    /// The label the shell shows (never "Random", Issue #166 §25).
    pub fn label(self) -> &'static str {
        match self {
            Self::Sequential => "Sequential",
            Self::Shuffle => "Shuffle",
        }
    }

    /// The order the `R` key toggles to.
    fn toggled(self) -> Self {
        match self {
            Self::Sequential => Self::Shuffle,
            Self::Shuffle => Self::Sequential,
        }
    }
}

/// What happens at a natural end of the traversal (Issue #166 §12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    /// Stop at the end: the completed episode stays completed.
    Off,
    /// Begin the traversal again (a fresh permutation under Shuffle).
    All,
    /// Re-open the entry that just completed. Natural EOF only — it
    /// never traps manual Next/Previous (Issue #166 §14).
    One,
}

impl RepeatMode {
    /// The label the shell shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::All => "All",
            Self::One => "One",
        }
    }

    /// The mode the `L` key cycles to (Off → All → One → Off).
    fn cycled(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

/// The shuffle permutation mechanism: a small deterministic generator
/// over one seed. Ordinary local representation — see the module docs
/// for why this is not an abstraction framework.
#[derive(Debug, Clone)]
pub struct ShufflePermuter {
    state: u64,
}

impl ShufflePermuter {
    /// A permuter seeded from OS entropy. `RandomState` draws its keys
    /// from the operating system, which is the std-only entropy source
    /// available without adding a dependency; the process id and the
    /// wall clock are mixed in so two permuters in one process (or two
    /// launches inside one clock tick) still diverge.
    pub fn from_os_entropy() -> Self {
        use std::hash::{BuildHasher, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(0x9E37_79B9_7F4A_7C15);
        hasher.write_u64(u64::from(std::process::id()));
        if let Ok(since_epoch) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        {
            hasher.write_u64(since_epoch.as_nanos() as u64);
        }
        Self::seeded(hasher.finish())
    }

    /// A permuter with an explicit seed (the deterministic test seam).
    pub fn seeded(seed: u64) -> Self {
        // SplitMix64 degenerates at zero; any nonzero state is fine.
        Self {
            state: seed | 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// SplitMix64: one well-distributed 64-bit draw per step.
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Fisher–Yates: permute `ids` in place. A permutation produced here
    /// keeps every element exactly once by construction.
    fn shuffle(&mut self, ids: &mut [usize]) {
        for i in (1..ids.len()).rev() {
            let j = (self.next_u64() % (i as u64 + 1)) as usize;
            ids.swap(i, j);
        }
    }
}

/// What a commit installs for one automatic EOF transition. Built by
/// [`TemporaryPlaylist::eof_step`] and applied by
/// [`TemporaryPlaylist::commit_eof`] — the split exists so that a
/// refused or clean-failed auto-open cannot move the playing cursor
/// (Issue #166 §38/§39).
#[derive(Debug, Clone, PartialEq, Eq)]
enum EofCommit {
    /// The playing cursor moves to this traversal position; the
    /// traversal itself is unchanged.
    Position(usize),
    /// A fresh Shuffle cycle: the whole traversal is replaced, with the
    /// opened entry at position 0.
    Cycle(Vec<usize>),
}

/// One EOF-policy decision: the entry to re-open, plus what the commit
/// would install. [`TemporaryPlaylist::eof_step`] returning `None` means
/// the policy is inert (the traversal is over and must not advance).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EofStep {
    /// The source the policy selects. Clone of the playlist entry, so
    /// the Open can run while the playlist stays borrowable.
    pub candidate: PathBuf,
    commit: EofCommit,
}

/// One traversal position's rendering contract for the shell: the row
/// label source and the two independent markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row<'a> {
    /// The source path (entry identity) — the row label is its file name.
    pub path: &'a Path,
    /// This row is the committed playing position.
    pub playing: bool,
    /// This row is the UI selection.
    pub selected: bool,
}

/// The App-owned temporary playlist. See the module docs for the truth
/// classes and the four-space discipline.
#[derive(Debug)]
pub struct TemporaryPlaylist {
    /// Entry identity space, in discovery order.
    entries: Vec<PathBuf>,
    /// Traversal space: a permutation of `0..entries.len()`.
    play_order: Vec<usize>,
    /// Traversal position of the committed episode.
    playing: Option<usize>,
    /// Traversal position of the UI selection.
    selected: Option<usize>,
    order: PlaybackOrder,
    repeat: RepeatMode,
    shuffle: ShufflePermuter,
    /// Bumped on every observable change, so the shell can rebuild its
    /// presentation rows only when the playlist actually moved (a large
    /// playlist must not cost per-frame work — Issue #166 §21).
    revision: u64,
}

impl Default for TemporaryPlaylist {
    fn default() -> Self {
        Self::new()
    }
}

impl TemporaryPlaylist {
    /// An empty playlist with the default preferences (Sequential /
    /// Repeat Off) and an OS-seeded shuffle mechanism.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            play_order: Vec::new(),
            playing: None,
            selected: None,
            order: PlaybackOrder::Sequential,
            repeat: RepeatMode::Off,
            shuffle: ShufflePermuter::from_os_entropy(),
            revision: 0,
        }
    }

    /// An empty playlist with an explicitly seeded shuffle mechanism
    /// (the deterministic test seam).
    #[cfg(test)]
    pub fn new_seeded(seed: u64) -> Self {
        Self {
            shuffle: ShufflePermuter::seeded(seed),
            ..Self::new()
        }
    }

    /// Replace the whole playlist with `entries` (canonical/discovery
    /// order) and make entry `first` the committed entry at traversal
    /// position 0. Called ON Open/replacement commit evidence only.
    ///
    /// The player's ordering and repeat preferences SURVIVE (Issue #166
    /// §23: they are preferences for this process), and a Shuffle
    /// preference builds a fresh cycle anchored on `first` — so the
    /// committed entry is the start of the new cycle and the rest of the
    /// order is the active policy's output.
    pub fn establish(&mut self, entries: Vec<PathBuf>, first: usize) {
        if entries.is_empty() {
            self.entries.clear();
            self.play_order.clear();
            self.playing = None;
            self.selected = None;
            self.bump_revision();
            return;
        }
        let first = first.min(entries.len() - 1);
        self.entries = entries;
        self.play_order = self.traversal_anchored_on(Some(first));
        // The cursor follows the ENTRY, not position 0: under Sequential
        // the committed entry keeps its canonical position (only a
        // Shuffle cycle anchors it at the head). Reading position 0
        // here would silently commit a different track.
        self.playing = self.position_of_entry(first);
        self.selected = self.playing;
        self.bump_revision();
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn order(&self) -> PlaybackOrder {
        self.order
    }

    pub fn repeat(&self) -> RepeatMode {
        self.repeat
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Switch the traversal order (the `R` key), idempotently. The
    /// committed entry never moves and is never re-opened: entering
    /// Shuffle anchors it at the head of a fresh cycle over the other
    /// entries, leaving Shuffle restores the canonical traversal with
    /// the traversal cursor following it to its canonical location. The
    /// selection follows the ENTRY it was on (Issue #166 §9/§10).
    pub fn set_order(&mut self, order: PlaybackOrder) {
        if order == self.order {
            return;
        }
        self.order = order;
        let anchor = self.playing_entry();
        let selected_entry = self.selected_entry();
        self.play_order = self.traversal_anchored_on(anchor);
        self.playing = anchor.and_then(|entry| self.position_of_entry(entry));
        self.selected = selected_entry.and_then(|entry| self.position_of_entry(entry));
        self.bump_revision();
    }

    /// Toggle Sequential ↔ Shuffle (the `R` key). Returns the new order.
    pub fn toggle_order(&mut self) -> PlaybackOrder {
        self.set_order(self.order.toggled());
        self.order
    }

    /// Cycle Repeat Off → All → One → Off (the `L` key). Returns the new
    /// mode. Repeat policy never moves the traversal.
    pub fn cycle_repeat(&mut self) -> RepeatMode {
        self.repeat = self.repeat.cycled();
        self.bump_revision();
        self.repeat
    }

    /// The traversal position of the committed episode, if any. After a
    /// clean-failed Open no episode is live and this still names the
    /// last committed entry (D14.6: honest navigation state, never
    /// audible-source truth).
    pub fn playing_position(&self) -> Option<usize> {
        self.playing
    }

    /// The traversal position of the UI selection, if any.
    pub fn selected_position(&self) -> Option<usize> {
        self.selected
    }

    /// The committed entry's traversal position, 1-based, beside the
    /// traversal length — the shell's `Track:` projection.
    pub fn playing_ordinal(&self) -> Option<(usize, usize)> {
        Some((self.playing? + 1, self.play_order.len()))
    }

    /// Every row of the playlist pane, in TRAVERSAL order — the same
    /// order Next/Previous and natural EOF walk, so the pane is
    /// WYSIWYG: the row below the playing row is what `N` plays next.
    pub fn rows(&self) -> impl Iterator<Item = Row<'_>> {
        self.play_order
            .iter()
            .enumerate()
            .filter_map(|(position, &entry)| {
                Some(Row {
                    path: self.entries.get(entry)?.as_path(),
                    playing: self.playing == Some(position),
                    selected: self.selected == Some(position),
                })
            })
    }

    /// Move the selection one row later. Inert at the last row and on an
    /// empty playlist: the selection never wraps, and it NEVER changes
    /// playback (Issue #166 §18).
    pub fn select_next(&mut self) {
        let Some(selected) = self.selected else {
            return;
        };
        if selected + 1 < self.play_order.len() {
            self.selected = Some(selected + 1);
            self.bump_revision();
        }
    }

    /// Move the selection one row earlier. Inert at the first row.
    pub fn select_previous(&mut self) {
        let Some(selected) = self.selected else {
            return;
        };
        if selected > 0 {
            self.selected = Some(selected - 1);
            self.bump_revision();
        }
    }

    /// The selected entry's source path — the `Enter` candidate. `None`
    /// when nothing is selected (empty playlist).
    pub fn selected_path(&self) -> Option<&Path> {
        self.entries
            .get(self.selected_entry()?)
            .map(PathBuf::as_path)
    }

    /// The manual Next/Previous target (Issue #166 §34/§35): the entry
    /// one traversal position away, wrapping ONLY under Repeat All. A
    /// single-entry wrap is not a navigation and is inert; Repeat One
    /// never traps manual navigation. `None` = inert (no side effect, no
    /// probe, no command).
    pub fn manual_step(&self, forward: bool) -> Option<usize> {
        let playing = self.playing?;
        let last = self.play_order.len().checked_sub(1)?;
        let wrap = self.repeat == RepeatMode::All;
        let target = if forward {
            if playing < last {
                playing + 1
            } else if wrap {
                0
            } else {
                return None;
            }
        } else if playing > 0 {
            playing - 1
        } else if wrap {
            last
        } else {
            return None;
        };
        (target != playing).then_some(target)
    }

    /// The entry the manual navigation target opens.
    pub fn path_at(&self, position: usize) -> Option<&Path> {
        self.entries
            .get(self.entry_at(position)?)
            .map(PathBuf::as_path)
    }

    /// Commit one MANUAL navigation: the playing cursor moves to
    /// `position` and the selection follows it (Issue #166 §41 — N/P are
    /// explicit playback navigation, so browsing follows them).
    pub fn commit_navigation(&mut self, position: usize) {
        if self.entry_at(position).is_none() {
            return;
        }
        self.playing = Some(position);
        self.selected = Some(position);
        self.bump_revision();
    }

    /// What the EOF policy does with a COMPLETED episode (Issue #166
    /// §13). `None` = no automatic transition (Repeat Off at the end of
    /// the traversal). This is a query: it never installs a traversal —
    /// the caller opens the candidate and commits on Open success, so a
    /// refused auto-next cannot have moved anything.
    ///
    /// The one exception to "a query changes nothing observable": a
    /// Repeat All wrap of a Shuffle traversal DRAWS a fresh permutation
    /// here (it is part of the decision's payload, not of the installed
    /// state), which advances only the permuter's internal seed — the
    /// playlist's traversal, cursors and revision stay exactly as they
    /// were until [`Self::commit_eof`].
    pub fn eof_step(&mut self) -> Option<EofStep> {
        let playing = self.playing?;
        let last = self.play_order.len().checked_sub(1)?;
        let commit = match self.repeat {
            RepeatMode::One => EofCommit::Position(playing),
            RepeatMode::Off => {
                if playing < last {
                    EofCommit::Position(playing + 1)
                } else {
                    return None;
                }
            }
            RepeatMode::All => {
                if playing < last {
                    EofCommit::Position(playing + 1)
                } else {
                    match self.order {
                        PlaybackOrder::Sequential => EofCommit::Position(0),
                        PlaybackOrder::Shuffle => EofCommit::Cycle(self.fresh_shuffle_cycle()),
                    }
                }
            }
        };
        let entry = match &commit {
            EofCommit::Position(position) => self.entry_at(*position)?,
            EofCommit::Cycle(cycle) => *cycle.first()?,
        };
        Some(EofStep {
            candidate: self.entries.get(entry)?.clone(),
            commit,
        })
    }

    /// Commit one automatic EOF transition on Open success. The
    /// selection follows the new playing row if and only if it was ON
    /// the old playing row; a user who was browsing elsewhere keeps
    /// their place (Issue #166 §40).
    ///
    /// "Keeps their place" is by ENTRY, never by traversal position: a
    /// Repeat All wrap under Shuffle replaces the whole traversal, so a
    /// browsed row's numeric position would name a different source
    /// afterwards. The selection is re-anchored on the entry it was on,
    /// exactly as a reorder does ([`Self::set_order`]).
    pub fn commit_eof(&mut self, step: EofStep) {
        let selection_followed = self.selected == self.playing;
        let browsed = self.selected_entry();
        match step.commit {
            EofCommit::Position(position) => {
                self.playing = Some(position);
                if selection_followed {
                    self.selected = Some(position);
                }
            }
            EofCommit::Cycle(cycle) => {
                self.play_order = cycle;
                self.playing = Some(0);
                if selection_followed {
                    self.selected = Some(0);
                } else {
                    self.selected = browsed.and_then(|entry| self.position_of_entry(entry));
                }
            }
        }
        self.bump_revision();
    }

    // --- the two index spaces, and their only meeting point ----------

    /// Entry id at a traversal position. `play_order` is a permutation
    /// of the entry ids, so this is the one direction that is total.
    fn entry_at(&self, position: usize) -> Option<usize> {
        self.play_order.get(position).copied()
    }

    /// Traversal position of an entry id, if the entry is in the
    /// traversal (it always is: every entry appears exactly once).
    fn position_of_entry(&self, entry: usize) -> Option<usize> {
        self.play_order
            .iter()
            .position(|&candidate| candidate == entry)
    }

    /// The committed entry's id.
    fn playing_entry(&self) -> Option<usize> {
        self.entry_at(self.playing?)
    }

    /// The selected entry's id.
    fn selected_entry(&self) -> Option<usize> {
        self.entry_at(self.selected?)
    }

    /// The traversal for the CURRENT order, anchored so that `anchor`
    /// (an entry id) is traversal position 0 when Shuffle has to build a
    /// cycle. Sequential ignores the anchor (its traversal is canonical
    /// by definition). With no anchor, a Shuffle cycle permutes every
    /// entry.
    fn traversal_anchored_on(&mut self, anchor: Option<usize>) -> Vec<usize> {
        let ids: Vec<usize> = (0..self.entries.len()).collect();
        match (self.order, anchor) {
            (PlaybackOrder::Sequential, _) => ids,
            (PlaybackOrder::Shuffle, None) => {
                let mut order = ids;
                self.shuffle.shuffle(&mut order);
                order
            }
            (PlaybackOrder::Shuffle, Some(anchor)) => {
                let mut rest: Vec<usize> =
                    ids.into_iter().filter(|&entry| entry != anchor).collect();
                self.shuffle.shuffle(&mut rest);
                let mut order = Vec::with_capacity(rest.len() + 1);
                order.push(anchor);
                order.extend(rest);
                order
            }
        }
    }

    /// A fresh Shuffle cycle over EVERY entry, for a Repeat All wrap.
    /// The entry that just finished is not placed first when the
    /// traversal has more than one entry — a user-experience constraint
    /// pinned in the tests, not semantic authority (Issue #166 §13).
    fn fresh_shuffle_cycle(&mut self) -> Vec<usize> {
        let just_finished = self.play_order.last().copied();
        let mut cycle: Vec<usize> = (0..self.entries.len()).collect();
        self.shuffle.shuffle(&mut cycle);
        if cycle.len() > 1 && just_finished == cycle.first().copied() {
            cycle.swap(0, 1);
        }
        cycle
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// The structural invariants the policy is required to maintain
    /// (Issue #166 §38), as an executable predicate. Test-only: this is
    /// a diagnostic over the representation, never a product contract.
    #[cfg(test)]
    pub fn invariants_hold(&self) -> bool {
        if self.entries.is_empty() {
            return self.play_order.is_empty() && self.playing.is_none() && self.selected.is_none();
        }
        // The traversal contains every entry exactly once.
        let mut sorted = self.play_order.clone();
        sorted.sort_unstable();
        if sorted != (0..self.entries.len()).collect::<Vec<_>>() {
            return false;
        }
        // Both cursors point into the traversal.
        self.playing.is_some_and(|p| p < self.play_order.len())
            && self.selected.is_some_and(|p| p < self.play_order.len())
            // Sequential's traversal IS the canonical order.
            && (self.order == PlaybackOrder::Shuffle
                || self.play_order == (0..self.entries.len()).collect::<Vec<_>>())
    }
}

#[cfg(test)]
mod tests {
    //! The pure product-policy matrix (Issue #166 §46/§47): every order ×
    //! repeat combination over 0/1/2/5-entry playlists, plus the shuffle
    //! structure tests, plus the invariants. No terminal, no player, no
    //! filesystem — the whole matrix is the pure policy.

    use super::*;

    fn entries(n: usize) -> Vec<PathBuf> {
        (0..n)
            .map(|i| PathBuf::from(format!("/m/{:02}.flac", i + 1)))
            .collect()
    }

    /// A playlist over `n` entries with `first` committed, at the given
    /// preferences and with a fixed seed. Invariants are checked on
    /// every construction, so a broken fixture cannot silently weaken a
    /// later assertion.
    fn playlist(
        n: usize,
        first: usize,
        order: PlaybackOrder,
        repeat: RepeatMode,
    ) -> TemporaryPlaylist {
        let mut playlist = TemporaryPlaylist::new_seeded(0xC0FF_EE00);
        if order != PlaybackOrder::Sequential {
            playlist.set_order(order);
        }
        while playlist.repeat() != repeat {
            playlist.cycle_repeat();
        }
        playlist.establish(entries(n), first);
        assert!(
            playlist.invariants_hold(),
            "fixture broke the playlist invariants"
        );
        playlist
    }

    /// The traversal as entry ids, for order assertions.
    fn traversal(p: &TemporaryPlaylist) -> Vec<usize> {
        p.play_order.clone()
    }

    /// The traversal rendered as 1-based entry labels.
    fn labels(p: &TemporaryPlaylist) -> Vec<String> {
        p.rows()
            .map(|row| {
                row.path
                    .file_name()
                    .expect("fixture paths have file names")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    fn commit_step(p: &mut TemporaryPlaylist) -> Option<usize> {
        let step = p.eof_step()?;
        let target = match &step.commit {
            EofCommit::Position(position) => *position,
            EofCommit::Cycle(cycle) => {
                assert_eq!(
                    cycle.first().copied().and_then(|e| p.entries.get(e)),
                    Some(&step.candidate),
                    "a cycle step's candidate IS its first entry"
                );
                0
            }
        };
        p.commit_eof(step);
        assert!(p.invariants_hold());
        Some(target)
    }

    // --- The full policy matrix ------------------------------------

    /// The systematic matrix (Issue #166 §46): every order × repeat
    /// combination over 0/1/2/5-entry playlists, driven through every
    /// operation the shell can reach, with [`TemporaryPlaylist::
    /// invariants_hold`] checked after each step. The focused tests below
    /// pin each rule's exact semantics; this one pins that no combination
    /// of them can reach a broken state, panic, or invent a cursor.
    #[test]
    fn the_policy_matrix_holds_for_every_order_repeat_and_size() {
        for n in [0usize, 1, 2, 5] {
            for order in [PlaybackOrder::Sequential, PlaybackOrder::Shuffle] {
                for repeat in [RepeatMode::Off, RepeatMode::All, RepeatMode::One] {
                    let label = format!("n={n} order={order:?} repeat={repeat:?}");
                    let mut p = TemporaryPlaylist::new_seeded(0xA5A5_0000 + n as u64);
                    if order != PlaybackOrder::Sequential {
                        p.set_order(order);
                    }
                    while p.repeat() != repeat {
                        p.cycle_repeat();
                    }
                    p.establish(entries(n), 0);
                    assert!(p.invariants_hold(), "{label}: establishment");

                    // Initial state: exactly the committed entry (or no
                    // cursor at all on an empty playlist), never a
                    // fabricated one.
                    assert_eq!(p.len(), n, "{label}");
                    assert_eq!(p.is_empty(), n == 0, "{label}");
                    assert_eq!(p.playing_position(), (n > 0).then_some(0), "{label}");
                    assert_eq!(p.selected_position(), (n > 0).then_some(0), "{label}");
                    assert_eq!(
                        p.playing_ordinal(),
                        (n > 0).then_some((1, n)),
                        "{label}: the Track: projection"
                    );
                    assert_eq!(p.rows().count(), n, "{label}");
                    assert_eq!(
                        p.rows().filter(|row| row.playing).count(),
                        usize::from(n > 0),
                        "{label}: one committed row at most"
                    );

                    // Browsing: bounded, never wrapping, never playing.
                    let playing_before = p.playing_entry();
                    for _ in 0..(n + 2) {
                        p.select_next();
                        assert!(p.invariants_hold(), "{label}: select_next");
                    }
                    assert_eq!(
                        p.selected_position(),
                        (n > 0).then(|| n - 1),
                        "{label}: the selection stops at the last row"
                    );
                    assert_eq!(
                        p.playing_entry(),
                        playing_before,
                        "{label}: browsing never plays"
                    );
                    for _ in 0..(n + 2) {
                        p.select_previous();
                        assert!(p.invariants_hold(), "{label}: select_previous");
                    }
                    assert_eq!(
                        p.selected_position(),
                        (n > 0).then_some(0),
                        "{label}: the selection stops at the first row"
                    );

                    // Enter's candidate is the selected entry — the row
                    // the pane shows, not a position in another space.
                    let first = entries(n);
                    assert_eq!(
                        p.selected_path(),
                        (n > 0).then(|| first[0].as_path()),
                        "{label}: Enter's candidate"
                    );

                    // Next walks the traversal and is inert at the end
                    // unless Repeat All wraps; Previous mirrors it.
                    let forward = p.manual_step(true);
                    assert_eq!(
                        forward,
                        (n > 1).then_some(1),
                        "{label}: Next from the first row"
                    );
                    assert_eq!(
                        p.manual_step(false),
                        (n > 1 && repeat == RepeatMode::All).then(|| n - 1),
                        "{label}: Previous at the first row wraps only under Repeat All"
                    );
                    if let Some(target) = forward {
                        p.commit_navigation(target);
                        assert_eq!(p.playing_position(), Some(target), "{label}");
                        assert_eq!(
                            p.selected_position(),
                            Some(target),
                            "{label}: manual navigation takes the selection with it"
                        );
                        assert!(p.invariants_hold(), "{label}: commit_navigation");
                    }

                    // The natural-EOF policy answers coherently for the
                    // combination, and a committed step leaves a legal
                    // state (the caller commits only on Open success).
                    // The only inert combination is Repeat Off with the
                    // committed entry already at the traversal end.
                    let at_end = n > 0 && p.playing_position() == Some(n - 1);
                    match p.eof_step() {
                        None => assert!(
                            n == 0 || (repeat == RepeatMode::Off && at_end),
                            "{label}: Repeat {repeat:?} at the end (at_end={at_end}) must advance"
                        ),
                        Some(step) => {
                            let candidate = step.candidate.clone();
                            assert!(
                                p.rows().any(|row| row.path == candidate),
                                "{label}: the candidate is a real row"
                            );
                            p.commit_eof(step);
                            assert!(p.invariants_hold(), "{label}: commit_eof");
                            assert_eq!(
                                p.playing_entry().and_then(|entry| p.entries.get(entry)),
                                Some(&candidate),
                                "{label}: the committed entry is the opened one"
                            );
                        }
                    }

                    // Reordering never moves the committed entry and
                    // never loses either cursor; toggling back restores
                    // the canonical traversal.
                    let committed = p.playing_entry();
                    let selected = p.selected_entry();
                    p.toggle_order();
                    p.toggle_order();
                    assert_eq!(p.order(), order, "{label}: two toggles");
                    assert_eq!(
                        p.playing_entry(),
                        committed,
                        "{label}: reorder keeps the entry"
                    );
                    assert_eq!(
                        p.selected_entry(),
                        selected,
                        "{label}: reorder keeps the selection"
                    );
                    assert!(p.invariants_hold(), "{label}: order toggles");

                    // Repeat cycles through every mode and back, moving
                    // nothing at all.
                    let playing = p.playing_position();
                    for expected in [repeat.cycled(), repeat.cycled().cycled(), repeat] {
                        assert_eq!(p.cycle_repeat(), expected, "{label}");
                        assert_eq!(p.playing_position(), playing, "{label}");
                        assert!(p.invariants_hold(), "{label}: repeat cycling");
                    }

                    // Re-establishment replaces the traversal honestly.
                    p.establish(entries(n), 0);
                    assert_eq!(p.len(), n, "{label}: re-establishment");
                    assert_eq!(p.order(), order, "{label}: preferences survive");
                    assert_eq!(p.repeat(), repeat, "{label}: preferences survive");
                    assert!(p.invariants_hold(), "{label}: re-establishment");
                }
            }
        }
    }

    // --- Sequential ------------------------------------------------

    /// Sequential + Repeat Off: the traversal is the canonical order,
    /// Next walks forward and is inert at the end, Previous walks back
    /// and is inert at the beginning, and natural EOF advances one entry
    /// at a time and then STAYS (no wrap).
    #[test]
    fn sequential_off_walks_the_canonical_order_and_stops_at_the_end() {
        for n in [1usize, 2, 5] {
            let mut p = playlist(n, 0, PlaybackOrder::Sequential, RepeatMode::Off);
            assert_eq!(traversal(&p), (0..n).collect::<Vec<_>>());
            assert_eq!(p.playing_position(), Some(0));
            assert_eq!(p.selected_position(), Some(0));
            assert_eq!(p.playing_ordinal(), Some((1, n)));

            // Previous is inert at the first entry: no target at all.
            assert_eq!(p.manual_step(false), None);

            // Next walks forward, one position per call.
            for expected in 1..n {
                let target = p.manual_step(true).expect("next has a target");
                assert_eq!(target, expected);
                p.commit_navigation(target);
                assert_eq!(p.playing_position(), Some(expected));
                assert_eq!(
                    p.selected_position(),
                    Some(expected),
                    "manual navigation takes the selection with it"
                );
            }
            // At the last entry: inert.
            assert_eq!(p.manual_step(true), None);
            assert_eq!(p.playing_position(), Some(n - 1));

            // Natural EOF at the end: no automatic transition.
            assert!(p.eof_step().is_none(), "Repeat Off never wraps");
            assert_eq!(p.playing_position(), Some(n - 1));
        }
    }

    /// Natural EOF in the middle of a Sequential traversal advances
    /// exactly one entry, and the selection follows only when it was on
    /// the completed row (Issue #166 §40).
    #[test]
    fn sequential_eof_advances_one_entry_and_the_selection_follows_conditionally() {
        let mut p = playlist(5, 0, PlaybackOrder::Sequential, RepeatMode::Off);
        // Selection on the playing row: it follows.
        assert_eq!(commit_step(&mut p), Some(1));
        assert_eq!(p.playing_position(), Some(1));
        assert_eq!(p.selected_position(), Some(1));

        // Selection browsing elsewhere: it stays where the user left it.
        p.select_next();
        p.select_next();
        assert_eq!(p.selected_position(), Some(3));
        assert_eq!(commit_step(&mut p), Some(2));
        assert_eq!(p.playing_position(), Some(2));
        assert_eq!(p.selected_position(), Some(3));

        // Back on the playing row: it follows again.
        p.select_previous();
        assert_eq!(p.selected_position(), Some(2));
        assert_eq!(commit_step(&mut p), Some(3));
        assert_eq!(p.playing_position(), Some(3));
        assert_eq!(p.selected_position(), Some(3));
    }

    // --- Repeat All ------------------------------------------------

    /// Repeat All wraps at BOTH traversal ends, for natural EOF and for
    /// manual Next/Previous.
    #[test]
    fn repeat_all_wraps_at_both_ends() {
        let mut p = playlist(4, 0, PlaybackOrder::Sequential, RepeatMode::All);
        // Manual Previous at the first entry wraps to the last.
        assert_eq!(p.manual_step(false), Some(3));
        p.commit_navigation(3);
        // Manual Next at the last entry wraps to the first.
        assert_eq!(p.manual_step(true), Some(0));
        p.commit_navigation(0);

        // Natural EOF from the last entry begins the traversal again.
        p.commit_navigation(3);
        assert_eq!(commit_step(&mut p), Some(0));
        assert_eq!(p.playing_position(), Some(0));
        // …and mid-traversal it simply advances.
        assert_eq!(commit_step(&mut p), Some(1));
    }

    // --- Repeat One ------------------------------------------------

    /// Repeat One replays on natural EOF and NEVER traps manual
    /// navigation (Issue #166 §14): N/P behave like Repeat Off at the
    /// traversal boundary.
    #[test]
    fn repeat_one_replays_on_eof_and_never_traps_manual_navigation() {
        let mut p = playlist(3, 0, PlaybackOrder::Sequential, RepeatMode::One);
        // Natural EOF re-opens the SAME entry, at any position.
        assert_eq!(commit_step(&mut p), Some(0));
        assert_eq!(p.playing_position(), Some(0));
        p.commit_navigation(2);
        let step = p.eof_step().expect("Repeat One always replays");
        assert_eq!(
            step.candidate,
            PathBuf::from("/m/03.flac"),
            "Repeat One re-opens the entry that completed"
        );
        p.commit_eof(step);

        // Manual navigation is a traversal navigation, with OFF-like
        // boundaries (no wrap) — Repeat One must not trap the user.
        p.commit_navigation(0);
        assert_eq!(p.manual_step(false), None, "no wrap under Repeat One");
        assert_eq!(p.manual_step(true), Some(1));
        p.commit_navigation(2);
        assert_eq!(p.manual_step(true), None, "no wrap under Repeat One");
        assert_eq!(p.manual_step(false), Some(1));
    }

    // --- Single-entry playlists -------------------------------------

    /// Every policy combination is coherent for ONE track (Issue #166
    /// §36): the traversal never advances except through the wrap that
    /// re-opens the same entry, manual navigation is inert, and nothing
    /// panics.
    #[test]
    fn a_single_entry_playlist_is_coherent_under_every_policy() {
        for order in [PlaybackOrder::Sequential, PlaybackOrder::Shuffle] {
            for repeat in [RepeatMode::Off, RepeatMode::All, RepeatMode::One] {
                let mut p = playlist(1, 0, order, repeat);
                assert_eq!(p.len(), 1);
                assert_eq!(p.playing_position(), Some(0));
                assert_eq!(p.playing_ordinal(), Some((1, 1)));
                // Manual navigation is always inert for one entry.
                assert_eq!(p.manual_step(true), None, "{order:?}/{repeat:?}");
                assert_eq!(p.manual_step(false), None, "{order:?}/{repeat:?}");

                let replay = match repeat {
                    RepeatMode::Off => false,
                    RepeatMode::All | RepeatMode::One => true,
                };
                match p.eof_step() {
                    None => assert!(!replay, "{order:?}/{repeat:?} must wrap"),
                    Some(step) => {
                        assert!(replay, "{order:?}/{repeat:?} must stay");
                        assert_eq!(
                            step.candidate,
                            PathBuf::from("/m/01.flac"),
                            "a single-entry wrap re-opens that entry"
                        );
                        p.commit_eof(step);
                        assert_eq!(p.playing_position(), Some(0));
                    }
                }
                assert!(p.invariants_hold(), "{order:?}/{repeat:?}");
            }
        }
    }

    // --- Shuffle structure -----------------------------------------

    /// The traversal is always a permutation of the entry ids: every
    /// entry exactly once, no duplicates, none missing (Issue #166 §47).
    #[test]
    fn a_shuffle_cycle_contains_every_entry_exactly_once() {
        for n in [1usize, 2, 5, 17] {
            let p = playlist(n, 0, PlaybackOrder::Shuffle, RepeatMode::Off);
            let mut sorted = traversal(&p);
            sorted.sort_unstable();
            assert_eq!(
                sorted,
                (0..n).collect::<Vec<_>>(),
                "n={n}: the traversal is a permutation"
            );
            assert_eq!(labels(&p).len(), n);
        }
    }

    /// The same seed produces the same permutation; different seeds are
    /// free to differ. Structural determinism, not statistical
    /// randomness (Issue #166 §11/§47).
    #[test]
    fn the_same_seed_produces_the_same_permutation() {
        let first = playlist(8, 0, PlaybackOrder::Shuffle, RepeatMode::Off);
        let second = playlist(8, 0, PlaybackOrder::Shuffle, RepeatMode::Off);
        assert_eq!(
            traversal(&first),
            traversal(&second),
            "a seeded permuter is reproducible"
        );
        let mut other = TemporaryPlaylist::new_seeded(0x5EED_1234);
        other.set_order(PlaybackOrder::Shuffle);
        other.establish(entries(8), 0);
        // Not asserted to differ: a claim of difference would be a
        // statistical claim about the generator, which is not the
        // property under test. Only structure is pinned.
        assert!(other.invariants_hold());
    }

    /// Entering Shuffle with a committed entry anchors THAT entry as the
    /// head of a fresh cycle over every other entry exactly once, and
    /// never moves the committed entry (Issue #166 §9).
    #[test]
    fn entering_shuffle_anchors_the_committed_entry_without_moving_it() {
        let mut p = playlist(5, 0, PlaybackOrder::Sequential, RepeatMode::Off);
        p.commit_navigation(2);
        let committed = p.playing_position();
        let committed_path = p.path_at(2).expect("valid position").to_path_buf();

        p.set_order(PlaybackOrder::Shuffle);

        assert_eq!(
            p.playing_position(),
            Some(0),
            "the committed entry is anchored at the head of the new cycle"
        );
        assert_eq!(
            p.path_at(0),
            Some(committed_path.as_path()),
            "anchoring keeps the SAME entry committed"
        );
        assert!(p.invariants_hold());
        let mut sorted = traversal(&p);
        sorted.sort_unstable();
        assert_eq!(sorted, (0..5).collect::<Vec<_>>());
        assert_eq!(committed, Some(2), "the entry really did move position");
        // And toggling back restores the canonical traversal, with the
        // committed entry still committed.
        p.set_order(PlaybackOrder::Sequential);
        assert_eq!(
            p.path_at(p.playing_position().unwrap()),
            Some(committed_path.as_path())
        );
        assert_eq!(p.playing_position(), Some(2), "back at its canonical place");
    }

    /// Leaving Shuffle restores the canonical traversal and places the
    /// traversal cursor at the committed entry's canonical location
    /// (Issue #166 §10) — with Next/Previous then reading canonical
    /// neighbours. No playback is restarted by any of it.
    #[test]
    fn leaving_shuffle_restores_the_canonical_traversal() {
        // Canonical A B C D E, shuffled traversal D A E B C, committed E:
        // returning to Sequential needs Previous = B and Next = D.
        let mut p = playlist(5, 0, PlaybackOrder::Sequential, RepeatMode::Off);
        p.set_order(PlaybackOrder::Shuffle);
        p.commit_navigation(2); // position 2 of the shuffle traversal
        let committed = p.path_at(2).expect("valid").to_path_buf();

        p.set_order(PlaybackOrder::Sequential);

        assert_eq!(
            p.path_at(p.playing_position().unwrap()),
            Some(committed.as_path())
        );
        let canonical_position = committed
            .file_name()
            .and_then(|name| {
                name.to_string_lossy()
                    .trim_end_matches(".flac")
                    .parse::<usize>()
                    .ok()
            })
            .expect("fixture labels are ordinals")
            - 1;
        assert_eq!(p.playing_position(), Some(canonical_position));
        let previous = p.manual_step(false);
        let next = p.manual_step(true);
        assert_eq!(previous, canonical_position.checked_sub(1));
        assert_eq!(next, Some(canonical_position + 1));
    }

    /// Toggling the order never inspects an episode, so it can never
    /// restart or replace one: this test pins that the playlist
    /// operations leave NO other observable trace than the traversal and
    /// the cursors they are specified to move.
    #[test]
    fn order_toggling_moves_nothing_but_the_traversal_and_its_cursors() {
        let mut p = playlist(4, 1, PlaybackOrder::Sequential, RepeatMode::Off);
        let playing_entry_before = p.playing_entry().expect("committed");
        let selected_entry_before = p.selected_entry().expect("selected");
        for _ in 0..6 {
            p.toggle_order();
            assert_eq!(p.playing_entry(), Some(playing_entry_before));
            assert_eq!(p.selected_entry(), Some(selected_entry_before));
            assert!(p.invariants_hold());
        }
        assert_eq!(
            p.order(),
            PlaybackOrder::Sequential,
            "an even number of toggles"
        );
    }

    /// The selection is preserved across a reorder (it follows the
    /// ENTRY, not the position) and moving it never changes playback.
    #[test]
    fn the_selection_follows_its_entry_across_reorders_and_never_moves_playback() {
        let mut p = playlist(5, 0, PlaybackOrder::Sequential, RepeatMode::Off);
        p.commit_navigation(1);
        p.select_next();
        p.select_next();
        let selected_entry_before = p.selected_entry().expect("selected");
        assert_ne!(selected_entry_before, p.playing_entry().unwrap());

        p.set_order(PlaybackOrder::Shuffle);
        assert_eq!(p.selected_entry(), Some(selected_entry_before));
        assert_eq!(p.playing_entry(), Some(1), "playback did not move");

        p.set_order(PlaybackOrder::Sequential);
        assert_eq!(p.selected_entry(), Some(selected_entry_before));
        assert_eq!(p.playing_entry(), Some(1));
    }

    /// Selection movement is bounded and inert at both ends: it never
    /// wraps (selection is browsing, not a mode) and it never touches
    /// the playing cursor.
    #[test]
    fn selection_movement_is_bounded_and_never_touches_playback() {
        let mut p = playlist(3, 2, PlaybackOrder::Sequential, RepeatMode::Off);
        assert_eq!(p.selected_position(), Some(2));
        p.select_next();
        assert_eq!(p.selected_position(), Some(2), "inert at the last row");
        assert_eq!(p.playing_position(), Some(2));

        p.selected = Some(0);
        p.select_previous();
        assert_eq!(p.selected_position(), Some(0), "inert at the first row");
        assert_eq!(p.playing_position(), Some(2), "browsing never plays");

        // A traversal of two: one step each way is the whole range.
        let mut p = playlist(2, 0, PlaybackOrder::Sequential, RepeatMode::Off);
        p.select_next();
        assert_eq!(p.selected_position(), Some(1));
        p.select_previous();
        assert_eq!(p.selected_position(), Some(0));
    }

    /// Shuffle + natural EOF walks EXACTLY the permutation: the order
    /// the pane shows is the order EOF follows, and each entry appears
    /// once per cycle.
    #[test]
    fn shuffle_eof_walks_the_permutation_once() {
        let mut p = playlist(6, 0, PlaybackOrder::Shuffle, RepeatMode::Off);
        let expected: Vec<PathBuf> = p.rows().map(|row| row.path.to_path_buf()).collect();

        let mut visited = vec![expected[0].clone()];
        while let Some(step) = p.eof_step() {
            visited.push(step.candidate.clone());
            p.commit_eof(step);
        }
        assert_eq!(visited, expected, "EOF follows the pane's order exactly");
        assert_eq!(visited.len(), 6, "every entry exactly once, then stop");
        assert!(p.eof_step().is_none(), "Repeat Off stays at the end");
    }

    /// Manual Previous under Shuffle is the previous entry of the SAME
    /// permutation — never a random re-pick (Issue #166 §34).
    #[test]
    fn shuffle_manual_navigation_is_deterministic_in_both_directions() {
        let mut p = playlist(6, 0, PlaybackOrder::Shuffle, RepeatMode::Off);
        let order: Vec<PathBuf> = p.rows().map(|row| row.path.to_path_buf()).collect();

        for (position, expected) in order.iter().enumerate() {
            p.commit_navigation(position);
            assert_eq!(p.path_at(position), Some(expected.as_path()));
            let previous = p.manual_step(false);
            let next = p.manual_step(true);
            assert_eq!(previous, position.checked_sub(1));
            assert_eq!(next, (position + 1 < order.len()).then_some(position + 1));
        }
    }

    /// A Repeat All wrap under Shuffle replaces the WHOLE traversal, so a
    /// browsing user's selection must be kept by ENTRY, not by traversal
    /// position: keeping the number would silently move them to a
    /// different source (the defect this test was written for).
    #[test]
    fn a_repeat_all_wrap_keeps_a_browsing_selection_on_its_entry() {
        for seed in 1..40u64 {
            let mut p = TemporaryPlaylist::new_seeded(seed);
            p.set_order(PlaybackOrder::Shuffle);
            while p.repeat() != RepeatMode::All {
                p.cycle_repeat();
            }
            p.establish(entries(6), 0);
            // Walk to the end of the cycle (the wrap precondition).
            while p.playing_position() != Some(5) {
                let target = p.manual_step(true).expect("walk to the last row");
                p.commit_navigation(target);
            }
            // The user browses away from the playing row.
            p.select_previous();
            let browsed_entry = p.selected_entry().expect("a browsed row");
            let browsed_position = p.selected_position().expect("a browsed row");
            assert_ne!(browsed_entry, p.playing_entry().unwrap());

            let step = p.eof_step().expect("Repeat All wraps");
            assert!(matches!(step.commit, EofCommit::Cycle(_)));
            p.commit_eof(step);

            assert_eq!(
                p.selected_entry(),
                Some(browsed_entry),
                "seed {seed}: the browsed ENTRY is kept across the new cycle"
            );
            let new_position = p.selected_position().expect("still selected");
            assert_ne!(
                new_position, browsed_position,
                "seed {seed}: and the traversal really did move under it"
            );
            assert_eq!(p.playing_position(), Some(0), "seed {seed}");
            assert!(p.invariants_hold(), "seed {seed}");
        }
    }

    /// A Repeat All wrap of a Shuffle traversal installs a FRESH cycle
    /// that still contains every entry exactly once, and does not start
    /// with the entry that just finished when there is more than one
    /// entry (Issue #166 §13).
    #[test]
    fn a_repeat_all_shuffle_wrap_builds_a_fresh_cycle_without_an_immediate_repeat() {
        let mut p = playlist(7, 0, PlaybackOrder::Shuffle, RepeatMode::All);
        // Walk to the end of the first cycle.
        while p.playing_position() != Some(6) {
            let target = p.manual_step(true).expect("in range");
            p.commit_navigation(target);
        }
        let last_entry = p.playing_entry().expect("committed");

        let step = p.eof_step().expect("Repeat All wraps");
        let cycle = match &step.commit {
            EofCommit::Cycle(cycle) => cycle.clone(),
            other => panic!("a Shuffle wrap is a cycle, got {other:?}"),
        };
        assert_ne!(
            cycle.first().copied(),
            Some(last_entry),
            "the entry that just finished does not start the next cycle"
        );
        let mut sorted = cycle.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..7).collect::<Vec<_>>());
        p.commit_eof(step);
        assert_eq!(traversal(&p), cycle, "the commit installs the fresh cycle");
        assert_eq!(p.playing_position(), Some(0));
        assert!(p.invariants_hold());
    }

    /// Repeat All under Shuffle keeps cycling forever: a long walk never
    /// exhausts the policy, and each cycle visits every entry once.
    #[test]
    fn repeat_all_shuffle_cycles_indefinitely() {
        let mut p = playlist(4, 0, PlaybackOrder::Shuffle, RepeatMode::All);
        let mut counts = std::collections::BTreeMap::new();
        for _ in 0..8 {
            *counts.entry(p.playing_entry().unwrap()).or_insert(0usize) += 1;
            let step = p.eof_step().expect("Repeat All never exhausts");
            p.commit_eof(step);
            assert!(p.invariants_hold());
        }
        assert_eq!(counts.len(), 4, "every entry is reached across cycles");
    }

    // --- Empty playlist --------------------------------------------

    /// An empty playlist is a legal state, and EVERY operation on it is
    /// inert and panic-free (Issue #166 §37). Order/repeat still cycle:
    /// they are player preferences, not playback state.
    #[test]
    fn an_empty_playlist_is_inert_under_every_operation() {
        let mut p = TemporaryPlaylist::new_seeded(7);
        assert!(p.is_empty());
        assert!(p.invariants_hold());
        assert_eq!(p.playing_position(), None);
        assert_eq!(p.selected_position(), None);
        assert_eq!(p.playing_ordinal(), None);
        assert_eq!(p.selected_path(), None);
        assert_eq!(p.manual_step(true), None);
        assert_eq!(p.manual_step(false), None);
        assert_eq!(p.path_at(0), None);
        assert!(p.eof_step().is_none());
        assert_eq!(p.rows().count(), 0);

        p.select_next();
        p.select_previous();
        p.commit_navigation(0);
        p.commit_navigation(3);
        assert!(p.invariants_hold());

        // Preferences still toggle on an empty playlist.
        assert_eq!(p.order(), PlaybackOrder::Sequential);
        assert_eq!(p.toggle_order(), PlaybackOrder::Shuffle);
        assert_eq!(p.toggle_order(), PlaybackOrder::Sequential);
        for expected in [RepeatMode::All, RepeatMode::One, RepeatMode::Off] {
            assert_eq!(p.cycle_repeat(), expected);
        }
        assert!(p.invariants_hold());

        // Establishing an EMPTY list clears the cursors rather than
        // leaving dangling ones.
        let mut p = playlist(3, 0, PlaybackOrder::Sequential, RepeatMode::Off);
        p.establish(Vec::new(), 0);
        assert!(p.is_empty());
        assert_eq!(p.playing_position(), None);
        assert!(p.invariants_hold());
    }

    // --- Revision / pane rows --------------------------------------

    /// The revision moves whenever the shell's rows could differ, and
    /// stays put for a no-op — the seam behind the shell's "rebuild the
    /// rows only when the playlist moved" rule.
    #[test]
    fn the_revision_moves_only_when_the_rows_could_differ() {
        let mut p = playlist(3, 0, PlaybackOrder::Sequential, RepeatMode::Off);
        let settled = p.revision();

        p.select_next();
        assert_ne!(p.revision(), settled, "the selection moved");

        let after_select = p.revision();
        p.select_previous();
        p.select_previous();
        assert_eq!(p.revision(), after_select.wrapping_add(1));
        let at_first = p.revision();
        p.select_previous();
        assert_eq!(p.revision(), at_first, "an inert step is not a change");

        // An idempotent order call is not a change either.
        p.set_order(PlaybackOrder::Sequential);
        assert_eq!(p.revision(), at_first);
        p.toggle_order();
        assert_ne!(p.revision(), at_first, "a real reorder is a change");
    }

    /// The pane rows carry the two markers independently, and the SAME
    /// row can be both (Issue #166 §20).
    #[test]
    fn rows_mark_playing_and_selected_independently() {
        let mut p = playlist(4, 0, PlaybackOrder::Sequential, RepeatMode::Off);
        let marks: Vec<(bool, bool)> = p.rows().map(|r| (r.playing, r.selected)).collect();
        assert_eq!(
            marks,
            vec![(true, true), (false, false), (false, false), (false, false)]
        );

        p.select_next();
        p.select_next();
        let marks: Vec<(bool, bool)> = p.rows().map(|r| (r.playing, r.selected)).collect();
        assert_eq!(
            marks,
            vec![(true, false), (false, false), (false, true), (false, false)],
            "playing and selected are independent rows"
        );

        // Both markers on one row: select the playing row again.
        p.commit_navigation(2);
        let marks: Vec<(bool, bool)> = p.rows().map(|r| (r.playing, r.selected)).collect();
        assert_eq!(
            marks,
            vec![(false, false), (false, false), (true, true), (false, false)],
            "a row can be both the committed and the selected one"
        );
    }

    /// The `Track:` projection is the COMMITTED position; the pane's row
    /// order is the traversal. Both are 1-based for display.
    #[test]
    fn the_track_projection_reads_the_committed_position() {
        let mut p = playlist(5, 3, PlaybackOrder::Sequential, RepeatMode::Off);
        assert_eq!(p.playing_ordinal(), Some((4, 5)));
        assert_eq!(
            p.rows().map(|r| r.playing).collect::<Vec<_>>(),
            vec![false, false, false, true, false]
        );
        p.commit_navigation(0);
        assert_eq!(p.playing_ordinal(), Some((1, 5)));
    }

    /// Both ends of the library surface: an establishment always leaves a
    /// committed entry, and re-establishing replaces the whole traversal
    /// (the Open/temporary-playlist contract).
    #[test]
    fn establishing_replaces_the_traversal_and_reanchors_the_cursors() {
        let mut p = playlist(5, 3, PlaybackOrder::Shuffle, RepeatMode::All);
        p.select_next();
        p.establish(entries(2), 0);
        assert_eq!(p.len(), 2);
        assert_eq!(p.order(), PlaybackOrder::Shuffle, "preferences survive");
        assert_eq!(p.repeat(), RepeatMode::All);
        assert_eq!(p.playing_position(), Some(0));
        assert_eq!(p.selected_position(), Some(0));
        assert_eq!(p.playing_ordinal(), Some((1, 2)));
        assert!(p.invariants_hold());
        assert_eq!(
            p.path_at(0),
            Some(Path::new("/m/01.flac")),
            "the committed entry is the anchored head of the fresh cycle"
        );
    }
}
