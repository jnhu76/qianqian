//! The F6 reference player: the application composition owner of
//! Open/replacement (ADR-PBK-002 D14.6, the F6-AUTHORITY-PROMOTION-1
//! amendment).
//!
//! One process-level reference-player host sequentially owns multiple
//! NON-OVERLAPPING [`QianqianApp`] composition roots — one per playback
//! episode (D14.6 mechanism C3, recorded in D1). The player itself is
//! neither a Plugin, nor a Capability, nor a K0 participant: Open is an
//! application composition Command, and this module is its one owner.
//!
//! The frozen replacement sequence (D14.6), executed synchronously on
//! the App thread (repeated Open is App-thread-serialized):
//!
//! ```text
//! candidate probe            invalid ⇒ REFUSED, old untouched
//! if an old root exists:
//!     request_stop() (iff unsettled) → wait_terminal() (D11 truth)
//!     → dispose() → require authoritative Discharged
//! construct fresh QianqianApp → activate desired composition
//!     → require authoritative Activated
//! ```
//!
//! Truth-class discipline (the reason the seams below are spelled the
//! way they are):
//!
//! ```text
//! probe          one stateless decode-provider query, off the live
//!                playback path; its facts are ADVISORY preflight
//!                evidence for this decision, never episode truth
//! Activated      the WHOLE fresh composition established the episode,
//!                read from the authority that ran the operation — the
//!                Session activation return carried by the paired attempt.
//!                Source/diagnostic observations never decide it. NEVER a
//!                CompositionSnapshot read (PBK-001 §2.3: snapshots are
//!                read-side diagnostics, and the kernel's structural
//!                view of a fiber is not the episode's activation truth)
//! old-side clear the disposal's authoritative operation verdict
//!                (DisposeVerdict), never a snapshot projection
//! ```
//!
//! Failure classes are frozen (D14.6; no rollback anywhere):
//!
//! ```text
//! invalid candidate    refused before any destructive step
//! old teardown failure TeardownViolated ⇒ FAIL-STOP: the violated root
//!                      is retained until process termination and no
//!                      further Open runs in this process
//! new activation       FAILURE-CLEAN: the attempted fresh root is
//! failure              authoritatively disposed before the failure
//!                      returns; cleanup Discharged ⇒ ActivationFailed-
//!                      Clean (no runtime remains; a later Open is
//!                      legal), cleanup TeardownViolated ⇒ FAIL-STOP
//!                      retaining the attempted root
//! ```
//!
//! Playlist / navigation (D14.6, AMENDED by the 2026-09-19
//! post-Phase-F product amendment recorded in D14.6 — Issue #166
//! `WINDOWS-TUI-USABILITY-CLOSURE-1`): the temporary playlist, its
//! traversal order and the repeat policy are APPLICATION NAVIGATION
//! STATE owned by this player (see [`crate::playlist`]); nothing
//! outside the App ever reads them, and no PlaylistFact /
//! CurrentTrackFact / PlaylistPlugin / NavigationPlugin exists. The
//! committed cursor is commit-on-activation: it moves only on
//! replacement commit evidence and is never playback truth. Next /
//! Previous / Enter-on-selected select a candidate and invoke the same
//! Open replacement; a probe refusal or a clean activation failure
//! leaves the committed cursor exactly where it was.
//!
//! What THIS module adds on top of the playlist's pure policy is the
//! one thing the playlist cannot own — the D11 observation:
//!
//! ```text
//! Completed Fact (D11, the Playback Session's authority)
//!     ↓ observed by the App (never forged, never inferred elsewhere)
//! App EOF policy (playlist: order + repeat)
//!     ↓
//! the SAME existing Open replacement
//! ```
//!
//! `Stopped` and `Failed` NEVER auto-advance (no silent failed-track
//! skip: that belongs to the later file/media phase), and the App
//! consumes a completed episode's EOF policy AT MOST ONCE
//! ([`ReferencePlayerApp::poll_eof_policy`]): the consumed flag lives on
//! the episode record, is set before the attempt, and dies with the
//! episode — so a stale repeated observation cannot advance twice and a
//! refused auto-next is never retried. The Playback Session stays
//! playlist-blind: it establishes terminal Facts and nothing else.
//!
//! A latched §G.6 violation permanently disables every replacement
//! (navigation, selection, EOF policy) too; no recovery path exists.
//!
//! The provider set behind [`EpisodeStart`] is the wiring's business
//! (the real host mounts the SongCore decode Plugin and the Output
//! Plugin with its host-selected WASAPI backend;
//! the unit matrix mounts fake providers over the REAL kernel and the
//! REAL playback session). Nothing in this module knows PCM, devices
//! or platform mechanisms.

use std::path::{Path, PathBuf};

use qianqian_app::QianqianApp;
use qianqian_composition::{CompositionSnapshot, DisposeVerdict};
use qianqian_playback::{
    AudioProcessingConfig, EpisodeTerminalOutcome, EqConfig, EqPreset, PlaybackSessionHandle,
    PlaybackSessionObservation,
};

use crate::playlist::{PlaybackOrder, RepeatMode, Row, TemporaryPlaylist};

/// The ONE observation the App's EOF policy reacts to: a committed D11
/// `Completed` Fact. `Stopped`, `Failed` and "no terminal Fact yet" are
/// all inert — the check is an EQUALITY against `Completed`, so no other
/// outcome can advance the playlist however it arrived, and nothing here
/// infers a terminal from mechanism evidence.
fn completed_fact(observation: &PlaybackSessionObservation) -> bool {
    observation.terminal_outcome == Some(EpisodeTerminalOutcome::Completed)
}

/// The two seams a host wires to mount one fresh episode composition:
/// the D14.6 candidate probe and the D14.6 fresh-root start. Together
/// they are the whole platform dependence of the reference player; the
/// unit matrix swaps in fakes over the real kernel and session.
pub trait EpisodeStart {
    /// Probe one Open candidate OFF the live playback path
    /// (probe-before-destruction). `Ok` means the source opened as a
    /// decode endpoint and its declared facts were read (open → facts →
    /// close, no PCM); `Err` is the refusal diagnostic. The facts
    /// themselves are advisory preflight evidence for THIS decision —
    /// the authoritative source evidence is the new activation's own.
    fn probe(&self, candidate: &Path) -> Result<(), String>;

    /// Construct a FRESH composition root for `source` and drive its
    /// desired composition to quiescence. `initial_output_level` is the
    /// App's desired stream factor (0..=100, D14.9) routed into the
    /// episode BEFORE activation, so the mechanism applies it at stream
    /// open. Never disposes anything: the CALLER owns the returned root
    /// on both arms (commit, or the failure-clean disposal).
    fn start(
        &self,
        source: &Path,
        initial_output_level: u8,
        processing: AudioProcessingConfig,
    ) -> StartAttempt;
}

/// One fresh-root start attempt carrying the D14.6 operation result.
/// Replacement consumes it after authoritative old-side clearance.
pub struct StartAttempt {
    /// The fresh root. The caller must either commit it or
    /// authoritatively dispose it; dropping it without `dispose` runs
    /// no teardown inverses.
    pub runtime: QianqianApp,
    /// The episode seam of the mounted session. May reference no live
    /// session (e.g. the composition was refused before activation);
    /// establishment remains independent of its read-side observations.
    pub handle: PlaybackSessionHandle,
    /// The completed whole fresh-assembly result, never inferred from
    /// source evidence, diagnostics, Fiber state or terminal settlement.
    pub establishment: qianqian_playback::EstablishmentResult,
}

/// The result of one Open operation: application composition feedback
/// (D14.6) — operation results, never a playback semantic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenOutcome {
    /// Replacement committed: the old side was cleared (or absent) and
    /// the fresh episode's authoritative activation evidence is
    /// established. The new episode is [`ReferencePlayerApp::active_handle`].
    Opened,
    /// The candidate was refused before any destructive step; the old
    /// episode continues untouched.
    Refused { diagnostic: String },
    /// The old side is gone; the fresh start did not establish and its
    /// attempted root was authoritatively disposed (Discharged). No
    /// runtime remains; a later Open is legal. The old episode does NOT
    /// come back.
    ActivationFailedClean { diagnostic: String },
    /// A latched teardown-contract violation (§G.6): no exit. The
    /// violated root is retained by the player and no further Open runs
    /// in this process.
    FailStop { diagnostic: String },
}

/// The end-of-host report of [`ReferencePlayerApp::quit`].
#[derive(Debug)]
pub struct QuitReport {
    /// The settled terminal outcome of the episode that was live at
    /// quit (D11 truth via `wait_terminal`), or the already-committed
    /// one; `None` when no episode was live (or the host is fail-stopped).
    pub terminal: Option<EpisodeTerminalOutcome>,
    /// The settled episode's failure diagnostic, if it published one.
    /// Presentation text, never semantics.
    pub diagnostic: Option<String>,
    /// The authoritative disposal verdict of the disposed root, if one
    /// was disposed by this quit.
    pub disposal: Option<DisposeVerdict>,
    /// The post-disposal snapshot of the disposed root (read-side
    /// diagnostics for the disposal report).
    pub snapshot: Option<CompositionSnapshot>,
}

/// The one live episode: a whole `QianqianApp` composition root plus
/// the episode seam it mounted. The player never looks inside the root
/// except through the two authority-owned control operations
/// (`dispose`) and the episode seam.
struct ActiveEpisode {
    runtime: QianqianApp,
    handle: PlaybackSessionHandle,
    source: PathBuf,
    /// Whether this episode's EOF policy has been run. App bookkeeping,
    /// not playback semantics: it lives ON the episode record, so it
    /// starts fresh with each commit, dies with the episode it belongs
    /// to, and makes "one episode + one Completed Fact ⇒ at most one
    /// automatic transition attempt" structural — a stale observation of
    /// a retired episode cannot reach a live flag (Issue #166 §16).
    eof_consumed: bool,
}

/// A root whose disposal latched a §G.6 teardown violation. Retained
/// until process termination: dropping it runs no teardown inverses,
/// and disposing it again is not a thing the protocol has. No recovery
/// is assumed and none is offered.
struct RetainedViolatedRoot {
    _runtime: QianqianApp,
}

/// The reference player's composition state: at most one live episode,
/// plus the fail-stop latch and any retained violated root — and the
/// temporary playlist with its traversal order, repeat policy and two
/// independent cursors (the AMENDED D14.6 playlist authority, Issue
/// #166 §4/§5). The playlist ([`TemporaryPlaylist`]) owns the pure
/// product policy; this struct owns the D11 observation the policy
/// reacts to and the Open replacement it drives. The committed cursor
/// moved only on F6 replacement commit evidence and is never playback
/// truth — the read side stays the D14.2 observation.
pub struct ReferencePlayerApp<S: EpisodeStart> {
    start: S,
    active: Option<ActiveEpisode>,
    violated: Option<RetainedViolatedRoot>,
    fail_stop: Option<String>,
    playlist: TemporaryPlaylist,
    /// The App's desired stream factor, `0..=100` (D14.9: integer,
    /// clamped; the TUI steps it by 5). Application configuration —
    /// NOT a Fact, NOT mechanism evidence, never a readback. It
    /// survives episode replacement because replacement rebuilds the
    /// episode, not the App; each fresh episode receives it BEFORE
    /// activation and its mechanism applies it at stream open.
    desired_volume: u8,
    /// Complete process-level desired DSP data; never an applied readback.
    desired_processing: AudioProcessingConfig,
}

impl<S: EpisodeStart> ReferencePlayerApp<S> {
    /// A player with no episode, no latch, no retained root and an
    /// empty playlist.
    pub fn new(start: S) -> Self {
        Self {
            start,
            active: None,
            violated: None,
            fail_stop: None,
            playlist: TemporaryPlaylist::new(),
            desired_volume: 100,
            desired_processing: AudioProcessingConfig::BYPASS,
        }
    }

    /// A player whose shuffle mechanism is explicitly seeded — the
    /// deterministic test seam for the shuffle-driven policy.
    #[cfg(test)]
    pub(crate) fn new_seeded(start: S, seed: u64) -> Self {
        Self {
            playlist: TemporaryPlaylist::new_seeded(seed),
            ..Self::new(start)
        }
    }

    /// The live episode's seam, if an episode is committed.
    pub fn active_handle(&self) -> Option<&PlaybackSessionHandle> {
        Some(&self.active.as_ref()?.handle)
    }

    /// The live episode's source path, if an episode is committed.
    pub fn active_source(&self) -> Option<&Path> {
        Some(self.active.as_ref()?.source.as_path())
    }

    /// One scan-time media probe of a candidate, through the same
    /// stateless decode-provider query the Open replacement's frozen
    /// sequence uses (probe-before-destruction). Advisory preflight
    /// evidence for input-expansion filtering: `Ok` never commits
    /// anything and `Err` never touches the live episode — the
    /// authoritative source evidence remains the episode activation's
    /// own.
    pub fn probe_candidate(&self, candidate: &Path) -> Result<(), String> {
        self.start.probe(candidate)
    }

    /// Establish the playlist that rides an Open commit: `entries` are
    /// the accepted candidates in canonical order and `entries[0]` IS
    /// the committed episode, so the traversal cursor starts on it. The
    /// transport calls this ONCE, right after the first Open committed;
    /// an empty list leaves the navigation state untouched (there is no
    /// committed entry to point at). A direct Open REPLACES the list;
    /// Add uses the separate append disposition.
    pub fn establish_playlist(&mut self, entries: Vec<PathBuf>) {
        if entries.is_empty() {
            return;
        }
        // The contract, made executable: establishment RIDES the Open
        // commit, so entry 0 IS the episode that just committed. A list
        // whose head is some other source would install a committed
        // cursor naming a track that is not the live one.
        debug_assert_eq!(
            self.active.as_ref().map(|episode| episode.source.as_path()),
            entries.first().map(PathBuf::as_path),
            "the playlist is established on Open commit evidence: entry 0 IS the committed episode"
        );
        self.playlist.establish(entries, 0);
    }

    /// The App's desired stream factor (0..=100). The read side means
    /// exactly this configured value — never the effective acoustic
    /// level, the Windows session master, or any mechanism readback
    /// (D14.9).
    pub fn desired_volume(&self) -> u8 {
        self.desired_volume
    }

    /// Product intent only. Applied configuration is not reported.
    pub fn desired_processing(&self) -> AudioProcessingConfig {
        self.desired_processing
    }

    pub fn set_processing_enabled(&mut self, enabled: bool) -> Result<(), String> {
        let candidate = AudioProcessingConfig {
            enabled,
            ..self.desired_processing
        };
        self.update_processing(candidate, |handle| handle.set_processing_enabled(enabled))
    }

    pub fn set_preamp(&mut self, gain: f32) -> Result<(), String> {
        let candidate = AudioProcessingConfig {
            gain,
            ..self.desired_processing
        };
        self.update_processing(candidate, |handle| handle.set_preamp(gain))
    }

    pub fn set_eq_config(&mut self, eq: EqConfig) -> Result<(), String> {
        let candidate = AudioProcessingConfig {
            eq: Some(eq),
            ..self.desired_processing
        };
        self.update_processing(candidate, |handle| handle.set_eq_config(eq))
    }

    pub fn set_eq_preset(&mut self, preset: EqPreset) -> Result<(), String> {
        self.update_processing(preset.to_config(), |handle| handle.set_eq_preset(preset))
    }

    fn update_processing(
        &mut self,
        candidate: AudioProcessingConfig,
        command: impl FnOnce(&PlaybackSessionHandle) -> Result<(), String>,
    ) -> Result<(), String> {
        if let Some(handle) = self.active_handle()
            && handle.observe().terminal_outcome.is_none()
        {
            // The existing command records immediate refusal evidence. Ok
            // records desired only; worker acceptance/application comes later.
            command(handle)?;
        } else {
            candidate.validate()?;
        }
        self.desired_processing = candidate;
        Ok(())
    }

    /// Append candidates admitted by the shared host preparation. Never
    /// establishes/replaces an episode; separate Adds may repeat a source.
    pub fn append_admitted(&mut self, entries: Vec<PathBuf>) -> Result<usize, String> {
        self.require_list_editable()?;
        Ok(self.playlist.append(entries))
    }

    /// Direct selection uses the current App traversal, not a persistent UI id.
    pub fn select_track(&mut self, position: usize) -> bool {
        self.playlist.select(position)
    }

    /// Retire first iff this selected entry owns the actual episode.
    /// A disposal failure latches fail-stop and leaves the list untouched.
    pub fn remove_selected(&mut self) -> Result<bool, String> {
        self.require_list_editable()?;
        if self.active.is_some()
            && self.playlist.selected_position() == self.playlist.playing_position()
        {
            self.retire_for_list_edit()?;
        }
        Ok(self.playlist.remove_selected())
    }

    pub fn clear_playlist(&mut self) -> Result<(), String> {
        self.require_list_editable()?;
        self.retire_for_list_edit()?;
        self.playlist.clear();
        Ok(())
    }

    fn require_list_editable(&self) -> Result<(), String> {
        match &self.fail_stop {
            Some(reason) => Err(reason.clone()),
            None => Ok(()),
        }
    }

    fn retire_for_list_edit(&mut self) -> Result<(), String> {
        if let Some(old) = self.active.take() {
            self.retire_old_episode(old)?;
        }
        Ok(())
    }

    /// Change the desired stream factor by `delta` (clamped to
    /// 0..=100), and route it into the live episode's seam if one is
    /// committed (idempotent Command; the mechanism applies it at its
    /// loop top). Returns the new desired value. A volume command never
    /// touches terminal truth.
    pub fn change_volume(&mut self, delta: i16) -> u8 {
        let desired = (i16::from(self.desired_volume) + delta).clamp(0, 100) as u8;
        if desired != self.desired_volume {
            self.desired_volume = desired;
            if let Some(handle) = self.active.as_ref().map(|e| e.handle.clone()) {
                handle.request_output_level(desired);
            }
        }
        desired
    }

    /// The navigation projection the shell renders: the 1-based
    /// position of the COMMITTED entry and the playlist length.
    /// Presentation of application navigation state — never playback
    /// truth, never an observable beyond this App's own shell (D14.6:
    /// no PlaylistFact / CurrentTrackFact exists).
    pub fn navigation_position(&self) -> Option<(usize, usize)> {
        self.playlist.playing_ordinal()
    }

    /// The temporary playlist's revision: bumped whenever its rows or
    /// markers could differ. The shell rebuilds its row labels only when
    /// this moves, so a huge playlist costs nothing per frame.
    pub fn playlist_revision(&self) -> u64 {
        self.playlist.revision()
    }

    /// The playlists's traversal order preference.
    pub fn playlist_order(&self) -> PlaybackOrder {
        self.playlist.order()
    }

    /// The playlist's repeat preference.
    pub fn playlist_repeat(&self) -> RepeatMode {
        self.playlist.repeat()
    }

    /// The committed entry's traversal position.
    pub fn playlist_playing_position(&self) -> Option<usize> {
        self.playlist.playing_position()
    }

    /// The UI selection's traversal position.
    pub fn playlist_selected_position(&self) -> Option<usize> {
        self.playlist.selected_position()
    }

    /// The playlist rows in traversal order, each carrying its two
    /// independent markers (Issue #166 §20). Presentation of the App's
    /// own navigation state.
    pub fn playlist_rows(&self) -> impl Iterator<Item = Row<'_>> {
        self.playlist.rows()
    }

    /// Whether the §G.6 fail-stop latch is set (no further Open runs).
    pub fn is_fail_stopped(&self) -> bool {
        self.fail_stop.is_some()
    }

    /// The fail-stop diagnostic, once latched.
    pub fn fail_stop_reason(&self) -> Option<&str> {
        self.fail_stop.as_deref()
    }

    /// The Open composition command (D14.6): replace the whole episode
    /// composition with one for `candidate`. Runs the frozen replacement
    /// sequence synchronously; see the module docs for the truth-class
    /// and failure-class contract. A DIRECT Open also replaces the
    /// playlist with the single opened path and commits its only entry —
    /// ON COMMIT only (D14.6 playlist closure): a refusal and a clean
    /// activation failure leave the navigation state exactly as it was.
    /// The ordering/repeat preferences survive (Issue #166 §23).
    pub fn open(&mut self, candidate: &Path) -> OpenOutcome {
        let outcome = self.replace_episode(candidate);
        if matches!(outcome, OpenOutcome::Opened) {
            self.playlist.establish(vec![candidate.to_owned()], 0);
        }
        outcome
    }

    /// Next (D14.6 + Issue #166 §35): the traversal position the manual
    /// policy selects, invoked through the same Open replacement.
    /// `None` = inert (no committed entry, or the traversal boundary
    /// with no wrap — no probe, no command, no navigation state moved).
    /// The SHELL may still report the refusal as its own operation
    /// feedback ("no next track"); that text is presentation, not
    /// episode or navigation state. The committed cursor moves only on replacement commit
    /// evidence, and the selection follows it (Issue #166 §41).
    pub fn next_track(&mut self) -> Option<OpenOutcome> {
        self.navigate(true)
    }

    /// Previous (Issue #166 §34): the mirror of [`Self::next_track`],
    /// walking the SAME traversal order — under Shuffle that is the
    /// previous entry of the current permutation, never a random
    /// re-pick.
    pub fn previous_track(&mut self) -> Option<OpenOutcome> {
        self.navigate(false)
    }

    /// One manual navigation: choose the policy's candidate, Open it
    /// through the frozen replacement, and commit the cursor only on
    /// commit evidence.
    fn navigate(&mut self, forward: bool) -> Option<OpenOutcome> {
        let position = self.playlist.manual_step(forward)?;
        let candidate = self.playlist.path_at(position)?.to_owned();
        let outcome = self.replace_episode(&candidate);
        if matches!(outcome, OpenOutcome::Opened) {
            self.playlist.commit_navigation(position);
        }
        Some(outcome)
    }

    /// `Enter`: play the SELECTED playlist entry through the SAME
    /// Open/replacement path (Issue #166 §19). `None` = nothing is
    /// selected, or selection is the unsettled current entry. Selection movement itself never
    /// reaches here — browsing cannot play anything.
    pub fn play_selected(&mut self) -> Option<OpenOutcome> {
        if self.selected_is_live_episode() {
            return None;
        }
        let position = self.playlist.selected_position()?;
        let candidate = self.playlist.path_at(position)?.to_owned();
        let outcome = self.replace_episode(&candidate);
        if matches!(outcome, OpenOutcome::Opened) {
            // The committed cursor moves ON COMMIT; the selection is
            // already on this row, so it simply stays there.
            self.playlist.commit_navigation(position);
        }
        Some(outcome)
    }

    /// Home Play replays the terminal current entry independently of browsing;
    /// with no episode it plays selection. An unsettled episode is inert here
    /// (the client uses its existing pause/resume commands instead).
    pub fn play_current(&mut self) -> Option<OpenOutcome> {
        let Some(episode) = self.active.as_ref() else {
            return self.play_selected();
        };
        episode.handle.observe().terminal_outcome?;
        let position = self.playlist.playing_position()?;
        let candidate = self.playlist.path_at(position)?.to_owned();
        let outcome = self.replace_episode(&candidate);
        if outcome == OpenOutcome::Opened {
            // Replaying does not move a separate browsing selection.
            self.playlist.commit_replay(position);
        }
        Some(outcome)
    }

    /// Whether the SELECTED row IS the committed episode's row while
    /// that episode is still unsettled. App truth over the two cursors
    /// plus the D11 read side — the shell's Enter rule reads this
    /// instead of re-deriving it: re-playing the live row would be a
    /// replacement (a restart), which is never what the Enter-on-
    /// selected-row affordance means while it is already playing. A
    /// settled episode (Completed/Stopped/Failed) is NOT live: Enter
    /// legitimately replays the row.
    pub fn selected_is_live_episode(&self) -> bool {
        let Some(episode) = self.active.as_ref() else {
            return false;
        };
        if episode.handle.observe().terminal_outcome.is_some() {
            return false;
        }
        self.playlist.selected_position() == self.playlist.playing_position()
    }

    /// Move the UI selection one row later. Presentation only: it never
    /// opens anything and never changes playback (Issue #166 §18).
    pub fn select_next_track(&mut self) {
        self.playlist.select_next();
    }

    /// Move the UI selection one row earlier. Presentation only.
    pub fn select_previous_track(&mut self) {
        self.playlist.select_previous();
    }

    /// The `R` key: toggle Sequential ↔ Shuffle. The committed entry is
    /// re-anchored, never restarted and never re-opened (Issue #166 §9).
    /// Returns the new order.
    pub fn toggle_order(&mut self) -> PlaybackOrder {
        self.playlist.toggle_order()
    }

    /// Set the traversal order (the `--shuffle` startup preference,
    /// Issue #166 §24). Idempotent.
    pub fn set_order(&mut self, order: PlaybackOrder) {
        self.playlist.set_order(order);
    }

    /// The `L` key: cycle Repeat Off → All → One → Off. Returns the new
    /// mode.
    pub fn cycle_repeat(&mut self) -> RepeatMode {
        self.playlist.cycle_repeat()
    }

    /// Run the App's natural-EOF policy for the live episode, if it is
    /// due (Issue #166 §13/§16/§17).
    ///
    /// The policy fires for a COMMITTED `Completed` Fact and for nothing
    /// else: `Stopped` and `Failed` never auto-advance, and no
    /// failed-track skip exists in this phase. The episode's policy is
    /// consumed BEFORE the attempt, so the ~150 ms refresh loop cannot
    /// advance twice on one Completed Fact, and a refused auto-next is
    /// not retried (no skip cascade — the failure is reported and the
    /// traversal stays truthful).
    ///
    /// `None` = nothing was due (no episode, the policy already ran for
    /// this episode, the terminal outcome is not Completed, or the
    /// policy is inert at the end of the traversal).
    pub fn poll_eof_policy(&mut self) -> Option<OpenOutcome> {
        if self.fail_stop.is_some() {
            return None;
        }
        let due = self.active.as_ref().is_some_and(|episode| {
            !episode.eof_consumed && completed_fact(&episode.handle.observe())
        });
        if !due {
            return None;
        }
        // Consume FIRST: at most one automatic transition attempt per
        // completed episode, however many refreshes observe it.
        if let Some(episode) = self.active.as_mut() {
            episode.eof_consumed = true;
        }
        let step = self.playlist.eof_step()?;
        let outcome = self.replace_episode(&step.candidate);
        if matches!(outcome, OpenOutcome::Opened) {
            self.playlist.commit_eof(step);
        }
        Some(outcome)
    }

    /// The frozen D14.6 replacement sequence itself (probe → old-side
    /// clear → fresh root → authority-result commit), owning no
    /// navigation state.
    fn replace_episode(&mut self, candidate: &Path) -> OpenOutcome {
        // The §G.6 latch has no exit: once set, no further Open runs in
        // this process (and the retained root stays retained).
        if let Some(reason) = self.fail_stop.as_deref() {
            return OpenOutcome::FailStop {
                diagnostic: reason.to_owned(),
            };
        }

        // 1. Probe-before-destruction: an invalid candidate is refused
        //    before any destructive step and live playback continues.
        if let Err(diagnostic) = self.start.probe(candidate) {
            return OpenOutcome::Refused { diagnostic };
        }

        // 2. Old-side clear.
        if let Some(old) = self.active.take()
            && let Err(detail) = self.retire_old_episode(old)
        {
            return OpenOutcome::FailStop { diagnostic: detail };
        }

        // 3. Fresh start, then the replacement commit condition: the
        //    old-side clear (the arm above) AND the authoritative
        //    whole fresh-assembly result.
        let attempt = self
            .start
            .start(candidate, self.desired_volume, self.desired_processing);
        match attempt.establishment {
            qianqian_playback::EstablishmentResult::Established => {
                self.active = Some(ActiveEpisode {
                    runtime: attempt.runtime,
                    handle: attempt.handle,
                    source: candidate.to_owned(),
                    eof_consumed: false,
                });
                OpenOutcome::Opened
            }
            qianqian_playback::EstablishmentResult::NotEstablished { diagnostic } => self
                .failure_clean_start(
                    attempt.runtime,
                    diagnostic.unwrap_or_else(|| {
                        "activation did not establish a playback session".to_owned()
                    }),
                ),
        }
    }

    /// The old-side clear of one replacement (D14.6): record stop intent
    /// iff the episode is unsettled, wait for the authoritative D11
    /// settlement, dispose, and require the authoritative Discharged.
    /// An already-terminal old skips stop and wait and is disposed
    /// directly. On `TeardownViolated` the root is retained and the
    /// fail-stop latch is set.
    fn retire_old_episode(&mut self, mut old: ActiveEpisode) -> Result<(), String> {
        if old.handle.observe().terminal_outcome.is_none() {
            old.handle.request_stop();
            // The D11 truth of the OLD episode: Open waits for it and
            // consumes nothing of it — the old episode keeps its own
            // committed terminal.
            let _ = old.handle.wait_terminal();
        }
        let outcome = old.runtime.dispose();
        match outcome.verdict {
            DisposeVerdict::Discharged => Ok(()),
            DisposeVerdict::TeardownViolated => {
                let detail = format!(
                    "old episode teardown violated (§G.6 latch; no exit): {}",
                    old.source.display()
                );
                self.latch_fail_stop(old.runtime, detail.clone());
                Err(detail)
            }
        }
    }

    /// The failure-clean start (D14.6): the attempted fresh root is
    /// authoritatively disposed before the failure returns. Cleanup
    /// `Discharged` ⇒ `ActivationFailedClean`, no runtime remains;
    /// cleanup `TeardownViolated` ⇒ fail-stop retaining the attempted
    /// root.
    fn failure_clean_start(
        &mut self,
        mut attempted: QianqianApp,
        diagnostic: String,
    ) -> OpenOutcome {
        let outcome = attempted.dispose();
        match outcome.verdict {
            DisposeVerdict::Discharged => OpenOutcome::ActivationFailedClean { diagnostic },
            DisposeVerdict::TeardownViolated => {
                let detail = format!(
                    "attempted episode cleanup violated teardown (§G.6 latch; no exit): \
                     {diagnostic}"
                );
                self.latch_fail_stop(attempted, detail.clone());
                OpenOutcome::FailStop { diagnostic: detail }
            }
        }
    }

    fn latch_fail_stop(&mut self, root: QianqianApp, detail: String) {
        self.violated = Some(RetainedViolatedRoot { _runtime: root });
        self.fail_stop = Some(detail);
    }

    /// Quit the host: settle the live episode (stop intent iff
    /// unsettled, wait for the D11 truth), dispose its root, and report.
    /// A fail-stopped host retains its violated root and reports without
    /// touching anything.
    pub fn quit(&mut self) -> QuitReport {
        if self.is_fail_stopped() {
            return QuitReport {
                terminal: None,
                diagnostic: None,
                disposal: None,
                snapshot: None,
            };
        }
        let Some(mut active) = self.active.take() else {
            return QuitReport {
                terminal: None,
                diagnostic: None,
                disposal: None,
                snapshot: None,
            };
        };
        if active.handle.observe().terminal_outcome.is_none() {
            active.handle.request_stop();
            let _ = active.handle.wait_terminal();
        }
        let observation = active.handle.observe();
        let outcome = active.runtime.dispose();
        let report = QuitReport {
            terminal: observation.terminal_outcome,
            diagnostic: observation.failure_diagnostic,
            disposal: Some(outcome.verdict),
            snapshot: Some(outcome.snapshot),
        };
        if outcome.verdict == DisposeVerdict::TeardownViolated {
            // Quit-time violation: retain the root honestly (drop runs
            // no teardown inverses) and latch. The process is exiting;
            // recovery is not assumed here either.
            self.latch_fail_stop(
                active.runtime,
                "quit disposal reported a latched teardown violation".to_owned(),
            );
        }
        report
    }
}

#[cfg(test)]
pub(crate) mod tests {
    //! The C7 unit matrix: the frozen D14.6 replacement semantics over
    //! the REAL composition kernel and the REAL playback session, with
    //! fake decode/output providers and a fake probe. The ordering log
    //! is written from kernel-mediated points only (provider activation,
    //! registered teardown effects, the probe call) — never by
    //! instrumenting the player — so the sequence assertions pin the
    //! player's actual call order.
    //!
    //! The fake episode source and the standard scenario paths are
    //! `pub(crate)` so sibling modules' tests (input expansion, TUI
    //! shell routing) can drive the same harness over the same kernel;
    //! cfg(test) keeps all of it out of the product.

    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use qianqian_audio_api::ports::{
        AudioOutput, AudioOutputCapability, DecodeError, DecodeOpenError, DecodeOutcome,
        DecodedPcmStream, DrainSignal, DrainVerdict, GateSlice, OutputError, ParkOutcome,
        PcmDecode, PcmDecodeCapability, PcmFormat, PcmPull, ProviderSeekOutcome, RenderGate,
        RenderRequest, RenderStream, TailProbeOutcome,
    };
    use qianqian_composition::{ComponentSpec, Discharge};

    use super::*;

    const FORMAT: PcmFormat = PcmFormat {
        sample_rate: 44_100,
        channels: 2,
        channel_mask: 0x3,
    };

    fn is_invalid(path: &Path) -> bool {
        path.to_string_lossy().contains("invalid")
    }

    fn is_failstart(path: &Path) -> bool {
        path.to_string_lossy().contains("failstart")
    }

    fn is_finite(path: &Path) -> bool {
        path.to_string_lossy().contains("finite")
    }

    /// The fake render leg: mirrors the real mechanism's observable
    /// shape — ONE loop-top gate check before every read, the data
    /// plane decides after release, the drain verdict publishes once on
    /// exit. The fake device holds nothing queued, ever, so every tail
    /// probe answers quiesced.
    fn fake_render_leg(
        input: Arc<dyn qianqian_audio_api::ports::RenderPcmInput>,
        gate: RenderGate,
        channels: usize,
    ) -> DrainVerdict {
        let mut dst = vec![0.0f32; 256 * channels];
        loop {
            if matches!(
                gate.park_loop_top(|slice| match slice {
                    GateSlice::TailProbe => TailProbeOutcome::Quiesced,
                    GateSlice::SeekRelease(_) => TailProbeOutcome::Quiesced,
                }),
                ParkOutcome::TailProbeFailed
            ) {
                return DrainVerdict::Aborted;
            }
            match input.read_frames(&mut dst) {
                PcmPull::Frames(_) => {
                    // Pace the fake leg: bounded, off-RT, test-only.
                    std::thread::sleep(Duration::from_millis(1));
                }
                PcmPull::Eof => return DrainVerdict::Drained,
                PcmPull::Stopped => return DrainVerdict::Aborted,
            }
        }
    }

    /// The fake acquired stream: stop → join, mirroring the real
    /// mechanism's owner-local inverse order (stop wakes the leg, join
    /// reaps it, release is trivial).
    struct FakeRenderStream {
        input: Arc<dyn qianqian_audio_api::ports::RenderPcmInput>,
        join: Option<std::thread::JoinHandle<()>>,
    }

    impl RenderStream for FakeRenderStream {
        fn negotiated_format(&self) -> PcmFormat {
            FORMAT
        }
        fn stop_and_join(mut self: Box<Self>) {
            self.input.stop();
            if let Some(handle) = self.join.take() {
                let _ = handle.join();
            }
        }
    }

    struct FakeOutput;

    impl AudioOutput for FakeOutput {
        fn open_stream(
            &self,
            request: RenderRequest,
        ) -> Result<Box<dyn RenderStream>, OutputError> {
            let input = request.input.clone();
            let drain: DrainSignal = request.drain.clone();
            let gate = request.gate;
            let channels = usize::from(request.format.channels);
            let join = std::thread::Builder::new()
                .name("fake-render".into())
                .spawn(move || {
                    let verdict = fake_render_leg(input, gate, channels);
                    drain.complete(verdict);
                })
                .map_err(|e| OutputError {
                    message: format!("fake render leg spawn failed: {e}"),
                })?;
            Ok(Box::new(FakeRenderStream {
                input: request.input,
                join: Some(join),
            }))
        }
    }

    /// The fake decode endpoint: silence, in bounded blocks. `finite`
    /// paths serve 8 blocks then EOF; everything else is endless.
    struct FakePcmStream {
        remaining_blocks: usize,
        /// The fixture's event log. The stream records the seek requests
        /// that actually reach the decoder, so a test can witness that a
        /// shell command became a D14.5 provider request WITHOUT
        /// inventing a read-side seek state (the outcome below stays the
        /// frozen `RefusedUnchanged`, which is a legal provider verdict).
        log: Log,
    }

    impl DecodedPcmStream for FakePcmStream {
        fn format(&self) -> PcmFormat {
            FORMAT
        }
        fn source_duration(&self) -> Option<Duration> {
            None
        }
        fn read_frames(&mut self, dst: &mut [f32]) -> Result<DecodeOutcome, DecodeError> {
            if self.remaining_blocks == 0 {
                return Ok(DecodeOutcome::Eof);
            }
            self.remaining_blocks -= 1;
            let frames = (dst.len() / usize::from(FORMAT.channels)).min(1024);
            Ok(DecodeOutcome::Frames(frames))
        }
        fn seek(&mut self, target: Duration) -> ProviderSeekOutcome {
            log_event(&self.log, format!("seek {}ms", target.as_millis()));
            ProviderSeekOutcome::RefusedUnchanged
        }
    }

    struct FakeDecode {
        log: Log,
    }

    impl PcmDecode for FakeDecode {
        fn open_media(&self, path: &Path) -> Result<Box<dyn DecodedPcmStream>, DecodeOpenError> {
            if is_invalid(path) {
                return Err(DecodeOpenError {
                    message: format!("unsupported container: {}", path.display()),
                });
            }
            if is_failstart(path) {
                return Err(DecodeOpenError {
                    message: "decode open failed after a passing probe".to_owned(),
                });
            }
            Ok(Box::new(FakePcmStream {
                remaining_blocks: if is_finite(path) { 8 } else { usize::MAX },
                log: self.log.clone(),
            }))
        }
    }

    fn log_event(log: &Arc<Mutex<Vec<String>>>, event: String) {
        log.lock().expect("ordering log").push(event);
    }

    /// The kernel-mediated ordering log: probe calls, provider
    /// activations and teardown effects write here; the tests assert the
    /// frozen sequence from it.
    type Log = Arc<Mutex<Vec<String>>>;

    fn fake_provider(
        name: &'static str,
        log: Log,
        root: u64,
        violating: bool,
    ) -> impl Fn(
        &mut qianqian_composition::ActivationCtx<'_>,
    ) -> Result<(), qianqian_composition::ActivationError> {
        let activate_log = log.clone();
        move |ctx| {
            log_event(&activate_log, format!("activate {root} {name}"));
            let teardown_log = log.clone();
            ctx.register_effect(move || {
                log_event(&teardown_log, format!("teardown {root} {name}"));
                if violating {
                    Discharge::Violated
                } else {
                    Discharge::Discharged
                }
            });
            match name {
                "decode" => ctx
                    .provide::<PcmDecodeCapability>(Rc::new(FakeDecode {
                        log: activate_log.clone(),
                    }))
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}"))),
                _ => ctx
                    .provide::<AudioOutputCapability>(Rc::new(FakeOutput))
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}"))),
            }
        }
    }

    /// The fake host wiring: fresh roots mounting fake providers + the
    /// REAL playback session over the REAL kernel, with an event log
    /// and a generation counter. Scenario flags:
    ///
    /// - `run_session_activation == false`: a deliberate no-op activation
    ///   returns Ok and becomes Active without producing the paired Session
    ///   result. This pins snapshot/diagnostic independence.
    /// - `violating_cleanup`: the fresh roots' provider effects return
    ///   `Violated`, so any disposal of a fresh root latches §G.6.
    /// - `refuse_composition`: both canonical providers declare Output,
    ///   so required-single ambiguity refuses the plan before activation.
    pub(crate) struct FakeEpisodeSource {
        pub(crate) log: Log,
        generation: AtomicU64,
        run_session_activation: bool,
        violating_cleanup: bool,
        refuse_composition: bool,
        start_levels: Arc<Mutex<Vec<u8>>>,
        start_processing: Arc<Mutex<Vec<AudioProcessingConfig>>>,
    }

    impl FakeEpisodeSource {
        pub(crate) fn new() -> Self {
            Self {
                log: Arc::new(Mutex::new(Vec::new())),
                generation: AtomicU64::new(0),
                run_session_activation: true,
                violating_cleanup: false,
                refuse_composition: false,
                start_levels: Arc::new(Mutex::new(Vec::new())),
                start_processing: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn without_session_activation(mut self) -> Self {
            self.run_session_activation = false;
            self
        }
    }

    impl EpisodeStart for FakeEpisodeSource {
        fn probe(&self, candidate: &Path) -> Result<(), String> {
            log_event(&self.log, format!("probe {}", candidate.display()));
            if is_invalid(candidate) {
                return Err(format!("unsupported container: {}", candidate.display()));
            }
            Ok(())
        }

        fn start(
            &self,
            source: &Path,
            initial_output_level: u8,
            processing: AudioProcessingConfig,
        ) -> StartAttempt {
            let root = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
            self.start_processing.lock().unwrap().push(processing);
            let mut decode = ComponentSpec::new("fake_decode_plugin")
                .provides::<PcmDecodeCapability>()
                .on_activate(fake_provider(
                    "decode",
                    self.log.clone(),
                    root,
                    self.violating_cleanup,
                ));
            // A genuine plan refusal inside the canonical three-role scope.
            if self.refuse_composition {
                decode = decode.provides::<AudioOutputCapability>();
            }
            let output = ComponentSpec::new("fake_output_plugin")
                .provides::<AudioOutputCapability>()
                .on_activate(fake_provider(
                    "output",
                    self.log.clone(),
                    root,
                    self.violating_cleanup,
                ));
            self.start_levels
                .lock()
                .expect("start levels")
                .push(initial_output_level);
            let mounted = PlaybackSessionHandle::new();
            mounted.request_output_level(initial_output_level);
            let (spec, attempt) = qianqian_playback::playback_session_spec_with_establishment(
                source.to_path_buf(),
                mounted.clone(),
                processing,
            );
            // Deliberate false-positive projection: an Active no-op Session
            // cannot produce the paired real Session activation result.
            let spec = if self.run_session_activation {
                spec
            } else {
                spec.on_activate(|_| Ok(()))
            };
            crate::assembly::establish_specs_for_test(decode, output, spec, attempt, mounted).start
        }
    }

    // #188 T1A: product operations through real K0/session retirement, with
    // fake decode/output providers. No device/platform claim is made here.
    #[test]
    fn append_and_noncurrent_remove_preserve_the_episode_and_entry_relations() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new_seeded(source, 188);
        assert_eq!(player.open(Path::new(LIVE_A)), OpenOutcome::Opened);
        player.establish_playlist(vec![
            LIVE_A.into(),
            LIVE_B.into(),
            "/media/live-c.flac".into(),
        ]);
        player.set_order(PlaybackOrder::Shuffle);
        assert!(player.select_track(2));
        let prefix: Vec<_> = player
            .playlist_rows()
            .map(|row| row.path.to_owned())
            .collect();
        let selected = prefix[2].clone();
        let old = player.active_handle().unwrap().clone();
        let events = log.lock().unwrap().len();
        assert_eq!(
            player.append_admitted(vec![LIVE_A.into(), LIVE_B.into()]),
            Ok(2)
        );
        assert_eq!(
            player
                .playlist_rows()
                .take(3)
                .map(|row| row.path.to_owned())
                .collect::<Vec<_>>(),
            prefix
        );
        assert_eq!(player.playlist.selected_path(), Some(selected.as_path()));
        assert!(!player.select_track(99));
        assert_eq!(player.remove_selected(), Ok(true));
        assert_eq!(
            log.lock().unwrap().len(),
            events,
            "no probe/start/stop/disposal for list-only edits"
        );
        assert_eq!(old.observe().terminal_outcome, None);
        assert_eq!(player.active_source(), Some(Path::new(LIVE_A)));
        assert!(cursor_names_active_source(&player));
        assert_eq!(player.playlist_selected_position(), Some(2));
        assert!(player.playlist.invariants_hold());
        player.quit();
    }

    #[test]
    fn remove_current_and_clear_retire_before_mutating_and_keep_preferences() {
        for clear in [false, true] {
            let source = FakeEpisodeSource::new();
            let log = source.log.clone();
            let mut player = ReferencePlayerApp::new(source);
            player.set_eq_preset(EqPreset::Rock).unwrap();
            player.change_volume(-35);
            player.cycle_repeat();
            assert_eq!(player.open(Path::new(LIVE_A)), OpenOutcome::Opened);
            player.establish_playlist(vec![LIVE_A.into(), LIVE_B.into()]);
            let old = player.active_handle().unwrap().clone();
            old.request_pause();
            let before = log.lock().unwrap().len();
            if clear {
                player.clear_playlist().unwrap();
            } else {
                assert_eq!(player.remove_selected(), Ok(true));
            }
            assert_eq!(
                old.observe().terminal_outcome,
                Some(EpisodeTerminalOutcome::Stopped)
            );
            assert!(player.active_handle().is_none());
            assert_eq!(player.playlist_playing_position(), None);
            assert_eq!(
                player.playlist_selected_position(),
                if clear { None } else { Some(0) }
            );
            assert_eq!(
                player
                    .playlist_rows()
                    .map(|row| row.path)
                    .collect::<Vec<_>>(),
                if clear {
                    Vec::new()
                } else {
                    vec![Path::new(LIVE_B)]
                }
            );
            let events = log.lock().unwrap();
            assert!(
                events[before..]
                    .iter()
                    .any(|event| event.starts_with("teardown")),
                "provider resources retired: {events:?}"
            );
            assert!(
                !events[before..]
                    .iter()
                    .any(|event| event.starts_with("probe")),
                "no successor autoplay"
            );
            assert_eq!(player.desired_volume(), 65);
            assert_eq!(player.desired_processing(), EqPreset::Rock.to_config());
            assert_eq!(player.playlist_repeat(), RepeatMode::All);
            assert!(player.playlist.invariants_hold());
        }
    }

    #[test]
    fn terminal_current_edit_disposes_without_relabeling_or_stopping() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = ReferencePlayerApp::new(source);
        assert_eq!(player.open(Path::new(A)), OpenOutcome::Opened);
        let old = player.active_handle().unwrap().clone();
        assert_eq!(old.wait_terminal(), EpisodeTerminalOutcome::Completed);
        let before = log.lock().unwrap().len();
        player.clear_playlist().unwrap();
        assert!(!old.observe().stop_requested);
        assert_eq!(old.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert!(
            log.lock().unwrap()[before..]
                .iter()
                .any(|event| event.starts_with("teardown"))
        );
        assert!(player.active_handle().is_none());
    }

    #[test]
    fn retirement_failure_preserves_the_whole_list_and_latches_edit_refusal() {
        for clear in [false, true] {
            let mut source = FakeEpisodeSource::new();
            source.violating_cleanup = true;
            let mut player = ReferencePlayerApp::new(source);
            assert_eq!(player.open(Path::new(LIVE_A)), OpenOutcome::Opened);
            player.establish_playlist(vec![LIVE_A.into(), LIVE_B.into()]);
            let rows: Vec<_> = player
                .playlist_rows()
                .map(|row| (row.path.to_owned(), row.selected, row.playing))
                .collect();
            let revision = player.playlist_revision();
            let result = if clear {
                player.clear_playlist().map(|_| true)
            } else {
                player.remove_selected()
            };
            assert!(result.is_err());
            assert!(player.is_fail_stopped());
            assert_eq!(player.playlist_revision(), revision);
            assert_eq!(
                player
                    .playlist_rows()
                    .map(|row| (row.path.to_owned(), row.selected, row.playing))
                    .collect::<Vec<_>>(),
                rows
            );
            assert!(player.clear_playlist().is_err());
            assert!(player.remove_selected().is_err());
            assert!(player.append_admitted(vec![LIVE_B.into()]).is_err());
            assert!(matches!(
                player.play_current(),
                Some(OpenOutcome::FailStop { .. })
            ));
        }
    }

    #[test]
    fn home_replay_uses_terminal_current_while_browsing_another_entry() {
        let mut player = ReferencePlayerApp::new(FakeEpisodeSource::new());
        player
            .append_admitted(vec![LIVE_A.into(), LIVE_B.into()])
            .unwrap();
        assert_eq!(player.navigation_position(), None);
        assert!(player.active_handle().is_none());
        assert_eq!(player.play_current(), Some(OpenOutcome::Opened));
        assert_eq!(
            player.play_selected(),
            None,
            "unsettled current is not restarted"
        );
        let old = player.active_handle().unwrap().clone();
        old.request_stop();
        assert_eq!(old.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        assert!(player.select_track(1));
        assert_eq!(player.play_current(), Some(OpenOutcome::Opened));
        assert_eq!(player.active_source(), Some(Path::new(LIVE_A)));
        assert_eq!(player.playlist_selected_position(), Some(1));
        assert_eq!(old.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        assert_eq!(player.play_selected(), Some(OpenOutcome::Opened));
        assert_eq!(player.active_source(), Some(Path::new(LIVE_B)));
        player.quit();
    }

    #[test]
    fn desired_dsp_persists_and_live_refusal_keeps_it_truthful() {
        let source = FakeEpisodeSource::new();
        let seeds = source.start_processing.clone();
        let mut player = ReferencePlayerApp::new(source);
        player.set_eq_preset(EqPreset::Rock).unwrap();
        player.set_preamp(0.5).unwrap();
        player.set_processing_enabled(false).unwrap();
        let desired = player.desired_processing();
        assert!(player.active_handle().is_none());
        assert!(player.set_preamp(f32::NAN).is_err());
        assert_eq!(player.desired_processing(), desired);
        assert_eq!(player.open(Path::new(LIVE_A)), OpenOutcome::Opened);
        assert_eq!(seeds.lock().unwrap().last(), Some(&desired));
        player.set_processing_enabled(true).unwrap();
        player.set_preamp(0.25).unwrap();
        let mut eq = EqConfig::FLAT;
        eq.band_gain_db[2] = 3.0;
        player.set_eq_config(eq).unwrap();
        let desired = player.desired_processing();
        eq.band_gain_db[9] = 19.0;
        assert!(player.set_eq_config(eq).is_err());
        assert_eq!(player.desired_processing(), desired);
        assert!(
            player
                .active_handle()
                .unwrap()
                .observe()
                .last_processing_refusal
                .is_some()
        );
        assert_eq!(player.open(Path::new(LIVE_B)), OpenOutcome::Opened);
        assert_eq!(seeds.lock().unwrap().last(), Some(&desired));
        player.set_eq_preset(EqPreset::Flat).unwrap();
        assert_eq!(player.desired_processing(), EqPreset::Flat.to_config());
        assert!(
            player.desired_processing().enabled,
            "Flat is a preset, not bypass"
        );
        player.clear_playlist().unwrap();
        player.set_preamp(0.0).unwrap();
        assert!(player.active_handle().is_none());
        assert_eq!(player.open(Path::new(LIVE_A)), OpenOutcome::Opened);
        assert_eq!(
            seeds.lock().unwrap().last(),
            Some(&player.desired_processing())
        );
        player.quit();
    }

    fn player_with(source: FakeEpisodeSource) -> ReferencePlayerApp<FakeEpisodeSource> {
        ReferencePlayerApp::new(source)
    }

    /// A player whose shuffle permutation is seeded, so the Shuffle
    /// policy's structural claims are reproducible (Issue #166 §11).
    fn seeded_player(seed: u64) -> ReferencePlayerApp<FakeEpisodeSource> {
        ReferencePlayerApp::new_seeded(FakeEpisodeSource::new(), seed)
    }

    pub(crate) const A: &str = "/media/finite-a.flac";
    pub(crate) const B: &str = "/media/finite-b.flac";
    /// Endless sources keep the first episode live until the
    /// replacement stops it.
    pub(crate) const LIVE_A: &str = "/media/live-a.flac";
    pub(crate) const LIVE_B: &str = "/media/live-b.flac";

    fn opened(outcome: &OpenOutcome) -> bool {
        matches!(outcome, OpenOutcome::Opened)
    }

    /// C7-1: the first Open faces no old side at all — no disposal is
    /// forged (no root, no verdict), the fresh episode establishes, and
    /// the player exposes its seam.
    #[test]
    fn first_open_establishes_without_forging_a_disposal() {
        let source = FakeEpisodeSource::new();
        let events_handle = source.log.clone();
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(LIVE_A))));

        let events = events_handle.lock().unwrap().clone();
        assert!(
            !events.iter().any(|e| e.starts_with("teardown")),
            "no root existed: no disposal may be forged\n{events:?}"
        );
        let observation = player.active_handle().expect("committed").observe();
        assert_eq!(observation.source_format, Some(FORMAT));
        assert_eq!(observation.activation_error, None);
        assert_eq!(player.active_source(), Some(Path::new(LIVE_A)));
    }

    /// C7-2: replacing a LIVE episode runs the frozen sequence — probe,
    /// then old stop/wait/dispose (authoritative Discharged), then the
    /// fresh start — and the old episode keeps its own D11 terminal.
    #[test]
    fn replacement_runs_the_frozen_sequence_over_a_live_episode() {
        let source = FakeEpisodeSource::new();
        let events_handle = source.log.clone();
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(LIVE_A))));
        let old = player.active_handle().expect("committed").clone();

        assert!(opened(&player.open(Path::new(LIVE_B))));

        let events = events_handle.lock().unwrap().clone();
        let probe_a = events.iter().position(|e| e == "probe /media/live-a.flac");
        let probe_b = events.iter().position(|e| e == "probe /media/live-b.flac");
        let activate_1 = events.iter().position(|e| e.starts_with("activate 1 "));
        let teardown_1 = events.iter().position(|e| e.starts_with("teardown 1 "));
        let activate_2 = events.iter().position(|e| e.starts_with("activate 2 "));
        // probe a < activate 1 (first episode)
        assert!(
            probe_a.unwrap() < activate_1.unwrap(),
            "the first root starts after its probe\n{events:?}"
        );
        // probe b < teardown 1 < activate 2: the probe precedes every
        // destructive step; the old root is fully retired before the
        // new one activates (no overlap).
        assert!(
            probe_b.unwrap() < teardown_1.unwrap() && teardown_1.unwrap() < activate_2.unwrap(),
            "frozen replacement sequence violated\n{events:?}"
        );

        // The old episode keeps its own committed terminal (D11), and
        // the replacement recorded stop intent on it.
        let old_observation = old.observe();
        assert_eq!(
            old_observation.terminal_outcome,
            Some(EpisodeTerminalOutcome::Stopped),
            "the replaced episode settles Stopped"
        );
        assert!(old_observation.stop_requested);

        // The new episode is live and established.
        let observation = player.active_handle().expect("committed").observe();
        assert_eq!(observation.source_format, Some(FORMAT));
        assert_eq!(observation.activation_error, None);
        assert_eq!(player.active_source(), Some(Path::new(LIVE_B)));
    }

    /// C7-3: an invalid candidate is REFUSED before any destructive
    /// step — the old episode keeps playing untouched (no stop intent,
    /// no settlement), and no new root was ever constructed.
    #[test]
    fn an_invalid_candidate_is_refused_before_any_destructive_step() {
        let source = FakeEpisodeSource::new();
        let events_handle = source.log.clone();
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(LIVE_A))));
        let old = player.active_handle().expect("committed").clone();

        let outcome = player.open(Path::new("/media/invalid-candidate.txt"));
        assert_eq!(
            outcome,
            OpenOutcome::Refused {
                diagnostic: "unsupported container: /media/invalid-candidate.txt".to_owned()
            }
        );

        let events = events_handle.lock().unwrap().clone();
        assert!(
            events
                .iter()
                .any(|e| e == "probe /media/invalid-candidate.txt"),
            "the probe ran (that is what refused)\n{events:?}"
        );
        assert!(
            !events.iter().any(|e| e.starts_with("teardown 1 ")),
            "the old root must not be touched by a refusal\n{events:?}"
        );
        assert!(
            !events.iter().any(|e| e.starts_with("activate 2 ")),
            "no fresh root may be constructed for a refused candidate\n{events:?}"
        );

        // The old episode continues: no stop intent, still unsettled.
        let old_observation = old.observe();
        assert!(!old_observation.stop_requested);
        assert_eq!(old_observation.terminal_outcome, None);
        assert_eq!(player.active_source(), Some(Path::new(LIVE_A)));
    }

    /// C7-4: an already-terminal old episode is cleared WITHOUT a stop
    /// command (skip stop/wait, dispose directly) and the replacement
    /// still commits.
    #[test]
    fn an_already_terminal_old_is_cleared_without_a_stop_command() {
        let source = FakeEpisodeSource::new();
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(A))));
        let old = player.active_handle().expect("committed").clone();
        // The finite source completes on its own.
        assert_eq!(old.wait_terminal(), EpisodeTerminalOutcome::Completed);

        assert!(opened(&player.open(Path::new(B))));

        let old_observation = old.observe();
        assert_eq!(
            old_observation.terminal_outcome,
            Some(EpisodeTerminalOutcome::Completed)
        );
        assert!(
            !old_observation.stop_requested,
            "the skip arm must not record stop intent on a settled episode"
        );
        assert_eq!(player.active_source(), Some(Path::new(B)));
    }

    /// C7-5: a PAUSED old episode settles through the ordinary D14.7
    /// semantics (stop-from-paused ⇒ Stopped) and the replacement
    /// commits. No new failure class, no special arm.
    #[test]
    fn a_paused_old_settles_through_the_ordinary_stop_semantics() {
        let source = FakeEpisodeSource::new();
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(LIVE_A))));
        let old = player.active_handle().expect("committed").clone();
        old.request_pause();

        assert!(opened(&player.open(Path::new(LIVE_B))));

        let old_observation = old.observe();
        assert_eq!(
            old_observation.terminal_outcome,
            Some(EpisodeTerminalOutcome::Stopped),
            "stop-from-paused settles the ordinary Stopped terminal"
        );
        assert_eq!(player.active_source(), Some(Path::new(LIVE_B)));
    }

    /// C7-6: Open during an in-flight seek — replacement owns the
    /// larger lifecycle; the frozen D14.5 rules govern the cut. The
    /// old episode settles (no wedge), the new one commits.
    #[test]
    fn open_during_a_seek_replacement_owns_the_larger_lifecycle() {
        let source = FakeEpisodeSource::new();
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(LIVE_A))));
        let old = player.active_handle().expect("committed").clone();
        old.request_seek(Duration::from_secs(5));

        assert!(opened(&player.open(Path::new(LIVE_B))));
        assert_eq!(
            old.observe().terminal_outcome,
            Some(EpisodeTerminalOutcome::Stopped)
        );
        assert_eq!(player.active_source(), Some(Path::new(LIVE_B)));
    }

    /// C7-7: a fresh start that fails AFTER the old side is gone is
    /// failure-clean — the attempted root is authoritatively disposed,
    /// no runtime remains, and a later Open is legal.
    #[test]
    fn activation_failure_after_the_old_side_is_gone_is_failure_clean() {
        let source = FakeEpisodeSource::new();
        let events_handle = source.log.clone();
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(LIVE_A))));

        // The probe passes; the fresh session's decode open raises.
        let outcome = player.open(Path::new("/media/failstart.flac"));
        assert_eq!(
            outcome,
            OpenOutcome::ActivationFailedClean {
                diagnostic: "decode open failed: decode open failed after a passing probe"
                    .to_owned()
            }
        );

        let events = events_handle.lock().unwrap().clone();
        assert!(
            events.iter().any(|e| e.starts_with("teardown 1 ")),
            "the old side is gone\n{events:?}"
        );
        assert!(
            events.iter().any(|e| e.starts_with("activate 2 ")),
            "the attempted root was constructed\n{events:?}"
        );
        assert!(
            events.iter().any(|e| e.starts_with("teardown 2 ")),
            "the attempted root was authoritatively disposed (failure-clean)\n{events:?}"
        );

        // No runtime remains (the player holds no root) and a later
        // Open is legal.
        assert!(player.active_handle().is_none());
        assert!(player.active_source().is_none());
        assert!(!player.is_fail_stopped());
        assert!(opened(&player.open(Path::new(LIVE_B))));
        assert_eq!(player.active_source(), Some(Path::new(LIVE_B)));
    }

    /// C7-8: a fresh start refused by its own composition is failure-
    /// clean too — the seam's refusal diagnostic comes back and the
    /// attempted root is disposed.
    #[test]
    fn a_composition_refused_start_is_failure_clean() {
        let mut source = FakeEpisodeSource::new();
        let events_handle = source.log.clone();
        source.refuse_composition = true;
        let mut player = player_with(source);

        let outcome = player.open(Path::new(LIVE_A));
        let OpenOutcome::ActivationFailedClean { diagnostic } = outcome else {
            panic!("a refused start is failure-clean, got {outcome:?}");
        };
        assert!(
            diagnostic.contains("ghost_plugin") || diagnostic.contains("refused"),
            "the composition errors surface as the diagnostic: {diagnostic}"
        );
        let events = events_handle.lock().unwrap().clone();
        // A refused desired composition establishes nothing: no
        // activation ever ran, so the failure-clean disposal of the
        // attempted (empty) root runs no effects either. The absence of
        // any kernel-mediated event IS the honest shape of this
        // failure class.
        assert_eq!(
            events,
            vec!["probe /media/live-a.flac".to_owned()],
            "a refused composition activates nothing and disposes an empty root\n{events:?}"
        );
        assert!(player.active_handle().is_none());
    }

    /// C7-9: an OLD-side teardown violation fails stop — the violated
    /// root is retained, no new root is ever constructed, and every
    /// later Open is refused by the latch.
    #[test]
    fn an_old_side_teardown_violation_fails_stop() {
        let mut source = FakeEpisodeSource::new();
        let events_handle = source.log.clone();
        // The flag applies to every fresh root, which is exactly the
        // point for the OLD-side arm: root 1 commits (its effects only
        // matter at disposal), and its retirement is where §G.6 latches.
        source.violating_cleanup = true;
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(LIVE_A))));

        let outcome = player.open(Path::new(LIVE_B));
        let OpenOutcome::FailStop { diagnostic } = outcome else {
            panic!("an old-side teardown violation must fail stop, got {outcome:?}");
        };
        assert!(
            diagnostic.contains("old episode teardown violated"),
            "{diagnostic}"
        );

        let events = events_handle.lock().unwrap().clone();
        assert!(
            events.iter().any(|e| e.starts_with("teardown 1 ")),
            "the old disposal ran (that is where the violation latched)\n{events:?}"
        );
        assert!(
            !events.iter().any(|e| e.starts_with("activate 2 ")),
            "no new episode may be constructed after a §G.6 latch\n{events:?}"
        );

        assert!(player.is_fail_stopped());
        assert!(
            player.fail_stop_reason().is_some(),
            "the latch keeps its diagnostic"
        );
        // Every later Open is refused by the latch — no probe, no start.
        let next = events_handle.lock().unwrap().len();
        assert!(matches!(
            player.open(Path::new(LIVE_B)),
            OpenOutcome::FailStop { .. }
        ));
        assert_eq!(
            events_handle.lock().unwrap().len(),
            next,
            "a fail-stopped Open runs nothing, not even the probe"
        );
        assert!(
            player.active_handle().is_none(),
            "the old arm took the root"
        );
    }

    /// C7-10: a fresh start whose CLEANUP violates fails stop too — the
    /// attempted root is retained, not dropped, and the latch is set.
    #[test]
    fn a_violating_cleanup_of_an_attempted_root_fails_stop() {
        let mut source = FakeEpisodeSource::new();
        let events_handle = source.log.clone();
        source.violating_cleanup = true;
        let mut player = player_with(source);

        // No old side: the very first start fails its session activation
        // (failstart) and then its cleanup violates.
        let outcome = player.open(Path::new("/media/failstart.flac"));
        let OpenOutcome::FailStop { diagnostic } = outcome else {
            panic!("a violating cleanup must fail stop, got {outcome:?}");
        };
        assert!(
            diagnostic.contains("cleanup violated teardown"),
            "{diagnostic}"
        );

        let events = events_handle.lock().unwrap().clone();
        assert!(
            events.iter().any(|e| e.starts_with("activate 1 ")),
            "the attempted root was constructed\n{events:?}"
        );
        assert!(
            events.iter().any(|e| e.starts_with("teardown 1 ")),
            "the failure-clean disposal ran (its verdict latched)\n{events:?}"
        );
        assert!(player.is_fail_stopped());
    }

    /// C1 negative control: K0 Active and no diagnostic cannot substitute
    /// for the required Session's authority-owned completed-attempt result.
    #[test]
    fn active_projection_and_no_diagnostic_cannot_establish_a_session() {
        let source = FakeEpisodeSource::new().without_session_activation();
        let events_handle = source.log.clone();
        let mut player = player_with(source);

        let outcome = player.open(Path::new(LIVE_A));
        let OpenOutcome::ActivationFailedClean { diagnostic } = outcome else {
            panic!(
                "an unacknowledged activation must NOT commit (snapshot says Active; \
                 the seam publishes nothing), got {outcome:?}"
            );
        };
        assert!(
            diagnostic.contains("activation did not establish"),
            "no activation failure was published; the honest diagnostic is the \
             establishment fallback: {diagnostic}"
        );
        let events = events_handle.lock().unwrap().clone();
        assert!(
            events.iter().any(|e| e.starts_with("teardown 1 ")),
            "the attempted (and, in kernel truth, live) root was disposed\n{events:?}"
        );
        assert!(player.active_handle().is_none());
    }

    /// C7-12: repeated replacement stays clean — every old root
    /// Discharged, every new episode established, the sequence frozen.
    #[test]
    fn repeated_replacement_stays_clean() {
        let source = FakeEpisodeSource::new();
        let events_handle = source.log.clone();
        let mut player = player_with(source);

        for (n, path) in [LIVE_A, LIVE_B, A, B].iter().enumerate() {
            assert!(opened(&player.open(Path::new(path))), "open #{n}");
        }
        assert_eq!(player.active_source(), Some(Path::new(B)));

        let events = events_handle.lock().unwrap().clone();
        for root in 1..=3u64 {
            let activate = events
                .iter()
                .position(|e| e.starts_with(&format!("activate {root} ")))
                .unwrap_or_else(|| panic!("missing activate {root}\n{events:?}"));
            let teardown = events
                .iter()
                .position(|e| e.starts_with(&format!("teardown {root} ")))
                .unwrap_or_else(|| panic!("missing teardown {root}\n{events:?}"));
            let next_activate = events
                .iter()
                .position(|e| e.starts_with(&format!("activate {} ", root + 1)));
            assert!(
                activate < teardown,
                "root {root} must retire before the next root starts\n{events:?}"
            );
            if let Some(next) = next_activate {
                assert!(
                    teardown < next,
                    "root {root}'s disposal precedes root {}'s activation\n{events:?}",
                    root + 1
                );
            }
        }
    }

    // --- Stage D: the D14.6 playlist / navigation closure -------------

    const C: &str = "/media/live-c.flac";

    /// Direct Open REPLACES the playlist with the single opened path
    /// and selects index 0 — on commit only (D14.6).
    #[test]
    fn direct_open_replaces_the_playlist_on_commit() {
        let source = FakeEpisodeSource::new();
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(LIVE_A))));
        player.establish_playlist(vec![PathBuf::from(LIVE_A), PathBuf::from(B)]);
        assert_eq!(player.navigation_position(), Some((1, 2)));

        // A committed direct Open replaces the whole playlist.
        assert!(opened(&player.open(Path::new(LIVE_B))));
        assert_eq!(
            player.navigation_position(),
            Some((1, 1)),
            "the playlist is now exactly the opened path at index 0"
        );
        assert_eq!(player.active_source(), Some(Path::new(LIVE_B)));

        // A REFUSED direct Open leaves the navigation state untouched.
        assert!(matches!(
            player.open(Path::new("/media/invalid-x.txt")),
            OpenOutcome::Refused { .. }
        ));
        assert_eq!(player.navigation_position(), Some((1, 1)));

        // A CLEAN-FAILED direct Open leaves it untouched too: the
        // cursor then names a track that no longer plays — honest
        // navigation state, not audible-source truth.
        assert!(matches!(
            player.open(Path::new("/media/failstart.flac")),
            OpenOutcome::ActivationFailedClean { .. }
        ));
        assert_eq!(player.navigation_position(), Some((1, 1)));
        assert!(player.active_handle().is_none());
    }

    /// Next selects the entry AFTER the cursor and commits the cursor
    /// only on replacement commit evidence.
    #[test]
    fn next_commits_the_cursor_on_the_replacement_commit() {
        let source = FakeEpisodeSource::new();
        let events_handle = source.log.clone();
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(LIVE_A))));
        player.establish_playlist(vec![
            PathBuf::from(LIVE_A),
            PathBuf::from(LIVE_B),
            PathBuf::from(C),
        ]);
        let a = player.active_handle().expect("committed").clone();

        assert_eq!(
            player.next_track(),
            Some(OpenOutcome::Opened),
            "next opens the entry after the cursor"
        );
        assert_eq!(player.navigation_position(), Some((2, 3)));
        assert_eq!(player.active_source(), Some(Path::new(LIVE_B)));
        assert_eq!(
            a.observe().terminal_outcome,
            Some(EpisodeTerminalOutcome::Stopped),
            "the previous entry's episode settles through the same D14.6 sequence"
        );
        let events = events_handle.lock().unwrap().clone();
        assert!(
            events.iter().any(|e| e == "probe /media/live-b.flac")
                && events.iter().any(|e| e.starts_with("teardown 1 "))
                && events.iter().any(|e| e.starts_with("activate 2 ")),
            "next IS an Open replacement (same frozen sequence)\n{events:?}"
        );
    }

    /// Both ends are inert: no wrap, no probe, no side effect.
    #[test]
    fn navigation_at_both_ends_is_inert() {
        let source = FakeEpisodeSource::new();
        let events_handle = source.log.clone();
        let mut player = player_with(source);

        // No playlist at all: inert.
        assert_eq!(player.next_track(), None);
        assert_eq!(player.previous_track(), None);

        assert!(opened(&player.open(Path::new(LIVE_A))));
        player.establish_playlist(vec![PathBuf::from(LIVE_A), PathBuf::from(LIVE_B)]);

        // At the FIRST entry: previous is inert.
        assert_eq!(player.previous_track(), None);
        assert_eq!(player.navigation_position(), Some((1, 2)));

        // Advance to the LAST entry, then next is inert.
        assert_eq!(player.next_track(), Some(OpenOutcome::Opened));
        assert_eq!(player.navigation_position(), Some((2, 2)));
        let next_event_count = events_handle.lock().unwrap().len();
        assert_eq!(player.next_track(), None, "no wrap");
        assert_eq!(
            events_handle.lock().unwrap().len(),
            next_event_count,
            "an inert navigation runs nothing, not even a probe"
        );
        assert_eq!(player.active_source(), Some(Path::new(LIVE_B)));
    }

    /// A probe refusal during navigation leaves index AND playback
    /// untouched (one keypress advances at most one candidate).
    #[test]
    fn navigation_refusal_leaves_index_and_playback_untouched() {
        let source = FakeEpisodeSource::new();
        let events_handle = source.log.clone();
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(LIVE_A))));
        player.establish_playlist(vec![
            PathBuf::from(LIVE_A),
            PathBuf::from("/media/invalid-mid.flac"),
            PathBuf::from(C),
        ]);

        assert!(matches!(
            player.next_track(),
            Some(OpenOutcome::Refused { .. })
        ));
        assert_eq!(
            player.navigation_position(),
            Some((1, 3)),
            "the cursor did not move"
        );
        assert_eq!(player.active_source(), Some(Path::new(LIVE_A)));
        let events = events_handle.lock().unwrap().clone();
        assert!(
            !events.iter().any(|e| e.starts_with("teardown 1 ")),
            "the live episode was not touched\n{events:?}"
        );

        // The NEXT keypress advances at most one candidate: it selects
        // the entry after the CURSOR — the refused entry sits at
        // cursor+1, so the player retries it. No auto-skip exists.
        assert!(matches!(
            player.next_track(),
            Some(OpenOutcome::Refused { .. })
        ));
        assert_eq!(player.navigation_position(), Some((1, 3)));
    }

    /// Post-destruction activation failure keeps the cursor at the old
    /// entry with no episode and no runtime residue (D14.6).
    #[test]
    fn navigation_activation_failure_keeps_the_cursor_at_the_old_entry() {
        let source = FakeEpisodeSource::new();
        let events_handle = source.log.clone();
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(LIVE_A))));
        player.establish_playlist(vec![
            PathBuf::from(LIVE_A),
            PathBuf::from("/media/failstart.flac"),
            PathBuf::from(C),
        ]);

        assert!(matches!(
            player.next_track(),
            Some(OpenOutcome::ActivationFailedClean { .. })
        ));
        assert_eq!(player.navigation_position(), Some((1, 3)));
        assert!(player.active_handle().is_none(), "no episode survives");
        let events = events_handle.lock().unwrap().clone();
        assert!(
            events.iter().any(|e| e.starts_with("teardown 2 ")),
            "the attempted root was disposed (failure-clean)\n{events:?}"
        );

        // A later navigation is legal and cursor-driven: the next press
        // retries the failed entry (cursor never moved), which fails
        // again — the escape is a direct Open, not an auto-skip.
        assert!(matches!(
            player.next_track(),
            Some(OpenOutcome::ActivationFailedClean { .. })
        ));
        assert_eq!(player.navigation_position(), Some((1, 3)));
    }

    /// A latched §G.6 violation disables navigation: any replacement a
    /// selection reaches refuses through the same latch (the inert ends
    /// stay inert — selection precedes the latch).
    #[test]
    fn fail_stop_disables_navigation() {
        let mut source = FakeEpisodeSource::new();
        source.violating_cleanup = true;
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(LIVE_A))));
        player.establish_playlist(vec![PathBuf::from(LIVE_A), PathBuf::from(LIVE_B)]);

        // The direct Open of the failing candidate retires the live
        // entry first; that disposal violates (the composer's flag), so
        // the OLD-side arm latches. The cursor stays at the old entry.
        assert!(matches!(
            player.open(Path::new("/media/failstart.flac")),
            OpenOutcome::FailStop { .. }
        ));
        assert_eq!(player.navigation_position(), Some((1, 2)));

        // The end behind the cursor is inert as always (selection
        // precedes the latch).
        assert_eq!(player.previous_track(), None);
        // The reachable end refuses through the latch.
        assert!(matches!(
            player.next_track(),
            Some(OpenOutcome::FailStop { .. })
        ));
    }

    /// The navigation projection follows the CURSOR, not the episode:
    /// a naturally-settled episode does not move or clear it — the
    /// cursor is navigation state, never playback truth (D14.6).
    #[test]
    fn the_navigation_projection_follows_the_cursor_not_the_episode() {
        let source = FakeEpisodeSource::new();
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(A))));
        player.establish_playlist(vec![PathBuf::from(A), PathBuf::from(B)]);
        let handle = player.active_handle().expect("committed").clone();

        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_eq!(
            player.navigation_position(),
            Some((1, 2)),
            "the episode settled; the navigation state did not move"
        );
    }

    // --- Stage F: the D14.9 desired-volume ownership -------------------

    /// The default desired level is unity (100): a fresh App never
    /// attenuates by surprise. change_volume clamps at BOTH ends and
    /// routes into the live episode's seam; the level survives episode
    /// replacement because it is App state, and each fresh start
    /// receives it BEFORE activation.
    #[test]
    fn desired_volume_defaults_clamps_and_survives_replacement() {
        let source = FakeEpisodeSource::new();
        let levels = Arc::clone(&source.start_levels);
        let mut player = player_with(source);
        assert_eq!(player.desired_volume(), 100);

        assert_eq!(player.change_volume(-30), 70);
        assert_eq!(player.change_volume(-30), 40);
        assert_eq!(player.change_volume(-30), 10);
        assert_eq!(
            player.change_volume(-30),
            0,
            "clamped at silence, never below"
        );
        assert_eq!(player.change_volume(-30), 0);
        assert_eq!(player.change_volume(25), 25);
        assert_eq!(
            player.change_volume(500),
            100,
            "clamped at unity, never above"
        );

        // Routing with no live episode is a no-op of state only.
        assert!(player.active_handle().is_none());

        // The level survives replacement and rides INTO the fresh start:
        // the fake composer records the level it received BEFORE
        // activation (the value the mechanism's open-time apply reads).
        player.change_volume(-20); // 80
        assert!(opened(&player.open(Path::new(LIVE_A))));
        assert_eq!(player.desired_volume(), 80);
        assert_eq!(
            levels.lock().expect("start levels").as_slice(),
            &[80u8],
            "the fresh start received the App's desired level pre-activation"
        );
        let handle = player.active_handle().expect("committed").clone();
        handle.request_output_level(45);
        assert_eq!(
            player.desired_volume(),
            80,
            "the read side is the App's own state, not the episode's"
        );
    }

    /// The volume keys step by 5 and never touch the episode's truth:
    /// a routed change leaves the episode unsettled.
    #[test]
    fn change_volume_routes_without_touching_terminal_truth() {
        let source = FakeEpisodeSource::new();
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(LIVE_A))));
        let handle = player.active_handle().expect("committed").clone();

        let before = handle.observe();
        assert_eq!(player.change_volume(-25), 75);
        let after = handle.observe();
        assert_eq!(after.terminal_outcome, None);
        assert_eq!(after.stop_requested, before.stop_requested);
        assert_eq!(after.pause_requested, before.pause_requested);
    }

    // --- Stage G: the App's playlist policy (Issue #166) --------------

    /// `n` FINITE sources: each reaches a genuine D11 `Completed` after
    /// a few bounded blocks (the fake leg paces 1 ms per block).
    fn finite(n: usize) -> Vec<PathBuf> {
        (0..n)
            .map(|i| PathBuf::from(format!("/media/finite-{i:02}.flac")))
            .collect()
    }

    /// `n` endless sources: an episode over one keeps running until a
    /// replacement stops it.
    fn live(n: usize) -> Vec<PathBuf> {
        (0..n)
            .map(|i| PathBuf::from(format!("/media/live-{i:02}.flac")))
            .collect()
    }

    fn event_count(log: &Log) -> usize {
        log.lock().expect("ordering log").len()
    }

    /// The committed source and the playlist's `playing` marker name the
    /// same row — the invariant the whole cursor discipline exists for.
    fn cursor_names_active_source<S: EpisodeStart>(player: &ReferencePlayerApp<S>) -> bool {
        player
            .playlist_rows()
            .any(|row| row.playing && Some(row.path) == player.active_source())
    }

    #[test]
    fn the_playing_cursor_names_the_committed_source() {
        let mut player = player_with(FakeEpisodeSource::new());
        let files = live(3);
        assert!(opened(&player.open(&files[0])));
        player.establish_playlist(files.clone());
        assert!(cursor_names_active_source(&player));

        assert_eq!(player.next_track(), Some(OpenOutcome::Opened));
        assert!(cursor_names_active_source(&player));
        assert_eq!(player.active_source(), Some(files[1].as_path()));

        assert_eq!(player.previous_track(), Some(OpenOutcome::Opened));
        assert!(cursor_names_active_source(&player));
        assert_eq!(player.active_source(), Some(files[0].as_path()));
    }

    /// Selection movement is presentation: it never probes, never opens,
    /// and never moves the committed cursor (Issue #166 §18).
    #[test]
    fn selection_movement_never_touches_playback() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = player_with(source);
        let files = live(4);
        assert!(opened(&player.open(&files[0])));
        player.establish_playlist(files.clone());
        let before = event_count(&log);
        let playing = player.playlist_playing_position();

        for _ in 0..3 {
            player.select_next_track();
        }
        assert_eq!(player.playlist_selected_position(), Some(3));
        player.select_next_track();
        assert_eq!(
            player.playlist_selected_position(),
            Some(3),
            "the selection is inert at the last row"
        );
        for _ in 0..5 {
            player.select_previous_track();
        }
        assert_eq!(player.playlist_selected_position(), Some(0));
        assert_eq!(
            player.playlist_playing_position(),
            playing,
            "browsing never moves the committed cursor"
        );
        assert_eq!(player.active_source(), Some(files[0].as_path()));
        assert_eq!(
            event_count(&log),
            before,
            "no probe, no open — selection is presentation only"
        );
    }

    /// `Enter` plays the SELECTED row through the same replacement, and
    /// the committed cursor moves only on commit evidence; a refusal
    /// leaves both cursors where the user put them (Issue #166 §19).
    #[test]
    fn enter_plays_the_selection_and_commits_only_on_success() {
        let mut player = player_with(FakeEpisodeSource::new());
        let files = live(3);
        assert!(opened(&player.open(&files[0])));
        player.establish_playlist(files.clone());

        player.select_next_track();
        player.select_next_track();
        assert_eq!(player.playlist_selected_position(), Some(2));
        assert_eq!(
            player.play_selected(),
            Some(OpenOutcome::Opened),
            "Enter opens the selected row"
        );
        assert_eq!(player.active_source(), Some(files[2].as_path()));
        assert_eq!(player.playlist_playing_position(), Some(2));
        assert_eq!(
            player.playlist_selected_position(),
            Some(2),
            "the selection stays on the row it played"
        );

        // A REFUSED selection: the committed cursor must not move. The
        // list is re-established with the committed source at entry 0
        // (the one legal shape of the commit-riding seed).
        let dud = PathBuf::from("/media/invalid-dud.flac");
        player.establish_playlist(vec![files[2].clone(), dud, files[0].clone()]);
        assert_eq!(player.playlist_playing_position(), Some(0));
        assert!(cursor_names_active_source(&player));
        player.select_next_track();
        assert_eq!(player.playlist_selected_position(), Some(1));
        assert!(matches!(
            player.play_selected(),
            Some(OpenOutcome::Refused { .. })
        ));
        assert_eq!(
            player.playlist_playing_position(),
            Some(0),
            "an attempted Open never moves the committed cursor"
        );
        assert_eq!(player.active_source(), Some(files[2].as_path()));
        assert!(cursor_names_active_source(&player));
    }

    /// The manual Next/Previous policy: one entry per press, wrapping
    /// only under Repeat All, and the selection follows the committed
    /// cursor (Issue #166 §41).
    #[test]
    fn manual_navigation_walks_the_traversal_and_wraps_only_under_repeat_all() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = player_with(source);
        let files = live(3);
        assert!(opened(&player.open(&files[0])));
        player.establish_playlist(files.clone());

        // Repeat Off: inert at both ends.
        assert_eq!(player.previous_track(), None);
        assert_eq!(player.next_track(), Some(OpenOutcome::Opened));
        assert_eq!(player.next_track(), Some(OpenOutcome::Opened));
        assert_eq!(player.active_source(), Some(files[2].as_path()));
        assert_eq!(player.playlist_selected_position(), Some(2));
        let before = event_count(&log);
        assert_eq!(player.next_track(), None, "no wrap under Repeat Off");
        assert_eq!(
            event_count(&log),
            before,
            "an inert navigation runs nothing, not even a probe"
        );

        // Repeat All: both ends wrap.
        assert_eq!(player.cycle_repeat(), RepeatMode::All);
        assert_eq!(player.next_track(), Some(OpenOutcome::Opened));
        assert_eq!(player.active_source(), Some(files[0].as_path()));
        assert_eq!(player.playlist_selected_position(), Some(0));
        assert_eq!(player.previous_track(), Some(OpenOutcome::Opened));
        assert_eq!(player.active_source(), Some(files[2].as_path()));

        // Repeat One never traps manual navigation: it behaves like
        // Repeat Off at the boundary.
        assert_eq!(player.cycle_repeat(), RepeatMode::One);
        assert_eq!(player.next_track(), None);
        assert_eq!(player.previous_track(), Some(OpenOutcome::Opened));
    }

    /// `R` re-anchors the traversal and NOTHING else: no re-open, no
    /// probe, no teardown, the committed episode untouched (Issue #166
    /// §9/§10). `L` moves no traversal at all.
    #[test]
    fn order_and_repeat_keys_never_touch_the_episode() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = player_with(source);
        let files = live(4);
        assert!(opened(&player.open(&files[0])));
        player.establish_playlist(files.clone());
        // Walk off the anchored first entry so the re-anchor really has
        // a non-zero traversal position to preserve.
        assert_eq!(player.next_track(), Some(OpenOutcome::Opened));
        assert_eq!(player.playlist_playing_position(), Some(1));
        let live_handle = player.active_handle().expect("committed").clone();
        let before = event_count(&log);

        assert_eq!(player.toggle_order(), PlaybackOrder::Shuffle);
        assert!(cursor_names_active_source(&player), "R keeps the track");
        assert_eq!(player.cycle_repeat(), RepeatMode::All);
        assert_eq!(player.cycle_repeat(), RepeatMode::One);
        assert_eq!(player.cycle_repeat(), RepeatMode::Off);
        assert_eq!(player.toggle_order(), PlaybackOrder::Sequential);
        assert!(cursor_names_active_source(&player));

        assert_eq!(
            event_count(&log),
            before,
            "order/repeat are policy state: no probe, no teardown, no activation"
        );
        let observation = live_handle.observe();
        assert_eq!(observation.terminal_outcome, None, "still playing");
        assert!(!observation.stop_requested);
    }

    /// The exactly-once EOF property (Issue #166 §16/§48): ONE completed
    /// episode drives at most ONE automatic transition attempt, however
    /// many refreshes observe the same Completed Fact.
    #[test]
    fn one_completed_episode_drives_exactly_one_transition() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = player_with(source);
        let files = vec![
            PathBuf::from("/media/finite-00.flac"),
            PathBuf::from("/media/live-01.flac"),
            PathBuf::from("/media/live-02.flac"),
        ];
        assert!(opened(&player.open(&files[0])));
        player.establish_playlist(files.clone());

        let completed = player.active_handle().expect("committed").clone();
        assert_eq!(completed.wait_terminal(), EpisodeTerminalOutcome::Completed);

        // The first observation of the Completed Fact advances exactly
        // one entry.
        assert_eq!(player.poll_eof_policy(), Some(OpenOutcome::Opened));
        assert_eq!(player.active_source(), Some(files[1].as_path()));
        assert_eq!(player.playlist_playing_position(), Some(1));

        // 100 further refreshes observe the SAME fact (the retired
        // episode's seam still publishes it) and must attempt NOTHING:
        // the new episode is endless, so no legitimate transition can
        // explain an event, and the count is the oracle.
        let after = event_count(&log);
        for _ in 0..100 {
            assert_eq!(player.poll_eof_policy(), None);
        }
        assert_eq!(
            event_count(&log),
            after,
            "one episode, one Completed Fact, one transition attempt"
        );
        assert_eq!(player.active_source(), Some(files[1].as_path()));
        assert_eq!(
            completed.observe().terminal_outcome,
            Some(EpisodeTerminalOutcome::Completed),
            "the retired episode keeps its own committed terminal"
        );
    }

    /// A chain of finite episodes advances once per completed episode
    /// and then stops at the traversal end under Repeat Off.
    #[test]
    fn a_finite_chain_advances_once_per_completed_episode() {
        let mut player = player_with(FakeEpisodeSource::new());
        let files = finite(3);
        assert!(opened(&player.open(&files[0])));
        player.establish_playlist(files.clone());

        for (step, expected) in files.iter().enumerate().skip(1) {
            let handle = player.active_handle().expect("committed").clone();
            assert_eq!(
                handle.wait_terminal(),
                EpisodeTerminalOutcome::Completed,
                "step {step}"
            );
            assert_eq!(player.poll_eof_policy(), Some(OpenOutcome::Opened));
            assert_eq!(player.active_source(), Some(expected.as_path()));
            assert!(cursor_names_active_source(&player));
        }

        // The LAST entry completes: Repeat Off means the traversal is
        // over — no wrap, nothing to open, and the terminal stays.
        let handle = player.active_handle().expect("committed").clone();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_eq!(
            player.poll_eof_policy(),
            None,
            "Repeat Off must not wrap the traversal"
        );
        assert_eq!(
            player
                .active_handle()
                .expect("committed")
                .observe()
                .terminal_outcome,
            Some(EpisodeTerminalOutcome::Completed),
            "the completed episode stays committed and truthful"
        );
    }

    /// Repeat All wraps the chain back to the first entry; Repeat One
    /// replays the SAME source through the same re-open path.
    #[test]
    fn repeat_all_wraps_the_chain_and_repeat_one_replays_the_same_source() {
        let mut player = player_with(FakeEpisodeSource::new());
        let files = finite(3);
        assert!(opened(&player.open(&files[0])));
        player.establish_playlist(files.clone());
        // Walk to the last entry so both wrap arms are exercised from
        // the traversal boundary.
        assert_eq!(player.next_track(), Some(OpenOutcome::Opened));
        assert_eq!(player.next_track(), Some(OpenOutcome::Opened));
        assert_eq!(player.playlist_playing_position(), Some(2));

        // Repeat One: the same source is re-opened, not advanced.
        assert_eq!(player.cycle_repeat(), RepeatMode::All);
        assert_eq!(player.cycle_repeat(), RepeatMode::One);
        let handle = player.active_handle().expect("committed").clone();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_eq!(player.poll_eof_policy(), Some(OpenOutcome::Opened));
        assert_eq!(
            player.active_source(),
            Some(files[2].as_path()),
            "Repeat One re-opens the entry that completed"
        );
        assert_eq!(
            player.playlist_playing_position(),
            player.playlist_selected_position(),
            "a replayed row keeps the selection on it"
        );

        // Repeat All from the LAST entry wraps to the first.
        assert_eq!(player.cycle_repeat(), RepeatMode::Off);
        assert_eq!(player.cycle_repeat(), RepeatMode::All);
        let handle = player.active_handle().expect("committed").clone();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_eq!(player.poll_eof_policy(), Some(OpenOutcome::Opened));
        assert_eq!(player.active_source(), Some(files[0].as_path()));
        assert_eq!(player.playlist_playing_position(), Some(0));
    }

    /// A REFUSED auto-next is reported and NOT retried — no skip
    /// cascade, and the committed cursor stays on the completed entry
    /// (Issue #166 §17/§48).
    #[test]
    fn a_refused_auto_next_is_reported_and_never_retried_or_skipped() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = player_with(source);
        let files = vec![
            PathBuf::from("/media/finite-00.flac"),
            PathBuf::from("/media/invalid-next.flac"),
            PathBuf::from("/media/live-02.flac"),
        ];
        assert!(opened(&player.open(&files[0])));
        player.establish_playlist(files.clone());

        let handle = player.active_handle().expect("committed").clone();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert!(matches!(
            player.poll_eof_policy(),
            Some(OpenOutcome::Refused { .. })
        ));
        assert_eq!(
            player.playlist_playing_position(),
            Some(0),
            "the refused step did not move the committed cursor"
        );
        assert_eq!(
            player.active_source(),
            Some(files[0].as_path()),
            "the completed episode is still the committed one"
        );
        assert!(
            log.lock()
                .unwrap()
                .iter()
                .any(|event| event.contains("invalid-next")),
            "the refusal really was the refused candidate"
        );

        // Repeated refreshes: no retry of the refused candidate, and
        // certainly no skip to the third entry.
        let after = event_count(&log);
        for _ in 0..50 {
            assert_eq!(player.poll_eof_policy(), None);
        }
        assert_eq!(
            event_count(&log),
            after,
            "the policy for that episode is spent"
        );
        assert_eq!(player.active_source(), Some(files[0].as_path()));
    }

    /// A clean-failed auto-next leaves no episode and no cursor
    /// movement, and the policy is still spent (no retry, no cascade).
    #[test]
    fn a_clean_failed_auto_next_leaves_no_episode_and_is_spent() {
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = player_with(source);
        let files = vec![
            PathBuf::from("/media/finite-00.flac"),
            PathBuf::from("/media/failstart.flac"),
            PathBuf::from("/media/live-02.flac"),
        ];
        assert!(opened(&player.open(&files[0])));
        player.establish_playlist(files.clone());

        let handle = player.active_handle().expect("committed").clone();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert!(matches!(
            player.poll_eof_policy(),
            Some(OpenOutcome::ActivationFailedClean { .. })
        ));
        assert!(player.active_handle().is_none(), "no runtime remains");
        assert_eq!(
            player.playlist_playing_position(),
            Some(0),
            "the cursor still names the last committed entry"
        );

        let after = event_count(&log);
        for _ in 0..50 {
            assert_eq!(player.poll_eof_policy(), None);
        }
        assert_eq!(event_count(&log), after, "no retry, no cascade");
    }

    /// `Stopped` and `Failed` never auto-advance (Issue #166 §15). The
    /// policy reacts to the D11 terminal outcome by EQUALITY with
    /// `Completed`, so the two other outcomes are inert by construction
    /// — the guard is exercised over all four observable states, and the
    /// integration half drives a REAL episode to `Stopped`.
    #[test]
    fn stopped_and_failed_never_auto_advance() {
        for outcome in [
            None,
            Some(EpisodeTerminalOutcome::Stopped),
            Some(EpisodeTerminalOutcome::Failed),
        ] {
            let mut observation = PlaybackSessionHandle::new().observe();
            observation.terminal_outcome = outcome;
            assert!(
                !completed_fact(&observation),
                "{outcome:?} must not trigger the EOF policy"
            );
        }
        let mut observation = PlaybackSessionHandle::new().observe();
        observation.terminal_outcome = Some(EpisodeTerminalOutcome::Completed);
        assert!(completed_fact(&observation));

        // Integration: a Stopped episode stays exactly where it is.
        let source = FakeEpisodeSource::new();
        let log = source.log.clone();
        let mut player = player_with(source);
        let files = live(3);
        assert!(opened(&player.open(&files[0])));
        player.establish_playlist(files.clone());
        let handle = player.active_handle().expect("committed").clone();

        let before = event_count(&log);
        assert_eq!(player.poll_eof_policy(), None, "unsettled: nothing to do");

        handle.request_stop();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        for _ in 0..50 {
            assert_eq!(
                player.poll_eof_policy(),
                None,
                "Stopped must never auto-advance"
            );
        }
        assert_eq!(event_count(&log), before);
        assert_eq!(player.active_source(), Some(files[0].as_path()));
    }

    /// The §G.6 latch disables the EOF policy as it disables every other
    /// replacement: a fail-stopped player attempts nothing.
    #[test]
    fn fail_stop_disables_the_eof_policy() {
        let mut source = FakeEpisodeSource::new();
        source.violating_cleanup = true;
        let log = source.log.clone();
        let mut player = player_with(source);
        let files = finite(2);
        assert!(opened(&player.open(&files[0])));
        player.establish_playlist(files.clone());

        let handle = player.active_handle().expect("committed").clone();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        // The auto-next retires the completed episode first; that
        // disposal violates (the composer's flag) and latches §G.6.
        assert!(matches!(
            player.poll_eof_policy(),
            Some(OpenOutcome::FailStop { .. })
        ));
        assert!(player.is_fail_stopped());
        let after = event_count(&log);
        assert_eq!(player.poll_eof_policy(), None);
        assert_eq!(event_count(&log), after, "a latched player runs nothing");
        // Navigation refuses through the same latch.
        assert!(matches!(
            player.next_track(),
            Some(OpenOutcome::FailStop { .. })
        ));
        assert!(player.is_fail_stopped());
    }

    /// A committed Open REPLACES the playlist while the ordering and
    /// repeat preferences survive (Issue #166 §23) — and under Shuffle
    /// the fresh list is anchored on the committed entry.
    #[test]
    fn open_replaces_the_playlist_and_keeps_the_preferences() {
        let mut player = player_with(FakeEpisodeSource::new());
        let first = live(4);
        assert!(opened(&player.open(&first[0])));
        player.establish_playlist(first.clone());
        assert_eq!(player.cycle_repeat(), RepeatMode::All);
        assert_eq!(player.toggle_order(), PlaybackOrder::Shuffle);

        let second = live(3);
        assert!(opened(&player.open(&second[0])));
        assert_eq!(
            player.navigation_position(),
            Some((1, 1)),
            "a direct Open replaces the playlist with the single opened path"
        );
        assert_eq!(player.playlist_order(), PlaybackOrder::Shuffle);
        assert_eq!(player.playlist_repeat(), RepeatMode::All);

        // An expanded Open establishes the whole accepted list ON
        // commit, anchored on the committed first candidate.
        let expanded = vec![second[0].clone(), second[1].clone(), second[2].clone()];
        player.establish_playlist(expanded);
        assert_eq!(player.navigation_position(), Some((1, 3)));
        assert!(cursor_names_active_source(&player));
        let rows = player
            .playlist_rows()
            .map(|row| row.path.to_path_buf())
            .collect::<Vec<_>>();
        assert_eq!(
            rows[0], second[0],
            "the cycle starts at the committed entry"
        );
    }

    /// Shuffle walks the permutation for the pane, for manual navigation
    /// and for natural EOF — one order, three readers — and no entry
    /// appears twice in one cycle (Issue #166 §8/§47).
    #[test]
    fn shuffle_drives_the_pane_navigation_and_eof_from_one_permutation() {
        let mut player = seeded_player(0xA11C_E5EE);
        let files = live(5);
        assert!(opened(&player.open(&files[0])));
        player.establish_playlist(files.clone());
        player.set_order(PlaybackOrder::Shuffle);

        let pane: Vec<PathBuf> = player
            .playlist_rows()
            .map(|row| row.path.to_path_buf())
            .collect();
        assert_eq!(pane.len(), 5);
        assert_eq!(pane[0], files[0], "the committed entry anchors the cycle");
        let mut unique = pane.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 5, "every entry exactly once: {pane:?}");

        // Manual Next walks the pane order.
        for expected in pane.iter().skip(1) {
            assert_eq!(player.next_track(), Some(OpenOutcome::Opened));
            assert_eq!(player.active_source(), Some(expected.as_path()));
        }
        assert_eq!(player.next_track(), None, "Repeat Off ends the traversal");

        // …and Previous walks it backwards, deterministically.
        for expected in pane.iter().rev().skip(1) {
            assert_eq!(player.previous_track(), Some(OpenOutcome::Opened));
            assert_eq!(player.active_source(), Some(expected.as_path()));
        }
    }

    /// A transition into an episode that ends immediately cannot make the
    /// policy re-enter itself: the flag belongs to the episode, and a
    /// brand-new episode legitimately gets its own single shot.
    #[test]
    fn a_transition_into_a_completed_episode_advances_at_most_once_more() {
        let mut player = player_with(FakeEpisodeSource::new());
        let files = finite(2);
        assert!(opened(&player.open(&files[0])));
        player.establish_playlist(files.clone());

        let handle = player.active_handle().expect("committed").clone();
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_eq!(player.poll_eof_policy(), Some(OpenOutcome::Opened));

        // The second episode is finite too. Its own Completed Fact earns
        // its own single attempt — and Repeat Off stops there.
        let second = player.active_handle().expect("committed").clone();
        assert_eq!(second.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_eq!(
            player.poll_eof_policy(),
            None,
            "the traversal is over; the last episode stays completed"
        );
        assert_eq!(player.active_source(), Some(files[1].as_path()));
    }

    /// C7-13: quit settles, disposes and reports — the live episode
    /// takes the ordinary Stopped terminal through the player's stop,
    /// the root discharges, and the report carries the D11 truth plus
    /// the disposal diagnostics.
    #[test]
    fn quit_settles_disposes_and_reports() {
        let source = FakeEpisodeSource::new();
        let mut player = player_with(source);
        assert!(opened(&player.open(Path::new(LIVE_A))));

        let report = player.quit();
        assert_eq!(report.terminal, Some(EpisodeTerminalOutcome::Stopped));
        assert_eq!(report.diagnostic, None);
        assert_eq!(report.disposal, Some(DisposeVerdict::Discharged));
        let snapshot = report.snapshot.expect("a root was disposed");
        assert!(snapshot.quiet);
        assert!(snapshot.fibers.is_empty());
        assert!(player.active_handle().is_none());

        // Quitting again (no episode) is honest and quiet.
        let report = player.quit();
        assert_eq!(report.terminal, None);
        assert_eq!(report.disposal, None);
    }

    /// C7-14: a fail-stopped host reports without touching the retained
    /// root — no disposal runs again, no outcome is invented.
    #[test]
    fn a_fail_stopped_quit_reports_and_retains() {
        let mut source = FakeEpisodeSource::new();
        source.violating_cleanup = true;
        let mut player = player_with(source);
        assert!(matches!(
            player.open(Path::new("/media/failstart.flac")),
            OpenOutcome::FailStop { .. }
        ));

        let report = player.quit();
        assert_eq!(report.terminal, None, "no episode was live");
        assert_eq!(
            report.disposal, None,
            "the violated root is retained, not re-disposed"
        );
        assert_eq!(report.snapshot, None);
        assert!(player.is_fail_stopped());
    }

    // --- Navigation-burst count matrix (research/navigation-burst-
    // boundary-0). This pins the PLAYER API contract — one manual-step
    // call is one complete D14.6 replacement — which is exactly the
    // substrate a burst of raw shell keys replays once per key today.
    // It does NOT pin any shell/interaction behavior: a future
    // coalescing policy lives ABOVE `next_track`/`previous_track` and
    // leaves these counts untouched. The counts below are derived from
    // the kernel-mediated event log, not from assumptions.

    /// Eight-track burst playlist (no wrap under Repeat Off; both
    /// directions have room from the committed start position 3).
    const BURST_TRACKS: [&str; 8] = [
        "/media/burst-0.flac",
        "/media/burst-1.flac",
        "/media/burst-2.flac",
        "/media/burst-3.flac",
        "/media/burst-4.flac",
        "/media/burst-5.flac",
        "/media/burst-6.flac",
        "/media/burst-7.flac",
    ];

    /// Count one episode root's activation / teardown lines.
    fn count_events(events: &[String], prefix: &str) -> usize {
        events.iter().filter(|e| e.starts_with(prefix)).count()
    }

    /// The policy expectation, derived from the frozen U2 traversal
    /// rule (Sequential, Repeat Off: inert at both ends), applied
    /// independently of the player under test.
    fn expected_final_position(start: usize, keys: &[bool]) -> usize {
        let mut pos = start;
        for &forward in keys {
            let next = if forward {
                Some(pos + 1)
            } else {
                pos.checked_sub(1)
            };
            if let Some(p) = next.filter(|p| *p < BURST_TRACKS.len()) {
                pos = p;
            }
        }
        pos
    }

    #[test]
    fn burst_matrix_one_full_replacement_per_raw_manual_step() {
        // (name, keys; true = Next, false = Previous)
        let sequences: [(&str, Vec<bool>); 12] = [
            ("R1 N", vec![true]),
            ("R2 NN", vec![true, true]),
            ("R3 NNN", vec![true, true, true]),
            ("R4 NNNNN", vec![true; 5]),
            ("R5 P", vec![false]),
            ("R6 PPPP", vec![false; 4]),
            ("R7 NNPP", vec![true, true, false, false]),
            (
                "R8 NNNPPNNP",
                vec![true, true, true, false, false, true, true, false],
            ),
            ("R9 NPNPNP", vec![true, false, true, false, true, false]),
            ("R10 NP", vec![true, false]),
            ("R11 NNP", vec![true, true, false]),
            (
                "R12 NNPPNNPP",
                vec![true, true, false, false, true, true, false, false],
            ),
        ];
        const START: usize = 3;

        println!("BURST_MATRIX (player API; one call per raw key)");
        println!("input       keys opened probes starts retires final_pos wall_ms");
        for (name, keys) in sequences {
            let source = FakeEpisodeSource::new();
            let events_handle = source.log.clone();
            let mut player = player_with(source);
            assert!(opened(&player.open(Path::new(BURST_TRACKS[0]))));
            player.establish_playlist(BURST_TRACKS.iter().map(PathBuf::from).collect::<Vec<_>>());
            // Walk to the middle so every sequence has room in BOTH
            // directions (Repeat Off: the ends are inert); the baseline
            // snapshot below excludes this setup from the counts.
            for _ in 0..START {
                assert!(matches!(player.next_track(), Some(OpenOutcome::Opened)));
            }
            let baseline_probes = count_events(&events_handle.lock().unwrap(), "probe ");
            let baseline_starts = count_events(&events_handle.lock().unwrap(), "activate ");
            let baseline_retires = count_events(&events_handle.lock().unwrap(), "teardown ");

            let t0 = std::time::Instant::now();
            let mut opened_count = 0usize;
            for &forward in &keys {
                let outcome = if forward {
                    player.next_track()
                } else {
                    player.previous_track()
                };
                if matches!(outcome, Some(OpenOutcome::Opened)) {
                    opened_count += 1;
                }
            }
            let wall = t0.elapsed();

            let events = events_handle.lock().unwrap().clone();
            let probes = count_events(&events, "probe ") - baseline_probes;
            // Each fresh root emits two activation lines (decode +
            // output); activations/2 = episode starts.
            let starts = (count_events(&events, "activate ") - baseline_starts) / 2;
            // Retirements: each retired root emits two teardown lines
            // (decode + output), same convention as the starts above.
            let retires = (count_events(&events, "teardown ") - baseline_retires) / 2;

            assert_eq!(
                probes, opened_count,
                "{name}: every non-inert manual step probes exactly once\n{events:?}"
            );
            assert_eq!(
                starts, opened_count,
                "{name}: every non-inert manual step starts exactly one fresh episode\n{events:?}"
            );
            assert_eq!(
                retires, opened_count,
                "{name}: every replacement retires exactly one old episode\n{events:?}"
            );
            assert_eq!(
                player.navigation_position(),
                Some((
                    // navigation_position is 1-based (the shell's
                    // "Track: n/len" line); the simulation is 0-based.
                    expected_final_position(START, &keys) + 1,
                    BURST_TRACKS.len()
                )),
                "{name}: final committed position follows the traversal policy"
            );
            assert_eq!(
                player.active_source(),
                Some(Path::new(
                    BURST_TRACKS[expected_final_position(START, &keys)]
                )),
                "{name}: the committed source is the final policy target"
            );

            println!(
                "{name:<11} {:>4} {:>6} {:>6} {:>6} {:>7} {:>9} {:>7.1}",
                keys.len(),
                opened_count,
                probes,
                starts,
                retires,
                player
                    .navigation_position()
                    .map(|(p, _)| p)
                    .unwrap_or_default(),
                wall.as_secs_f64() * 1000.0,
            );
        }

        // The queueing dimension (§10) is NOT observable at this layer:
        // each call here is already a consumed key. A call that arrives
        // while a replacement runs cannot exist in-process — callers are
        // serialized by &mut self. What queues between replacements is
        // the OS/terminal input queue upstream of the shell loop,
        // measured on the real host in the campaign's Windows leg.
        //
        // NP shows the absence of any zero-Open collapse today: the net
        // target equals the committed track, yet BOTH keys ran a full
        // replacement (asserted above via R10's counts: 2 and 2).
    }
}
