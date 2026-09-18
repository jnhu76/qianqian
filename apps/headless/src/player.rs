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
//!                session's own published evidence (format published ∧
//!                no published activation failure). NEVER a
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
//! Playlist / navigation (D14.6, closed by the same amendment): the
//! playlist and the current index are APPLICATION NAVIGATION STATE
//! owned by this player (`Vec<PathBuf>` + `Option<usize>`); nothing
//! outside the App ever reads them, and no PlaylistFact /
//! CurrentTrackFact / PlaylistPlugin / NavigationPlugin exists. The
//! index is commit-on-activation: it moves only on replacement commit
//! evidence (a direct Open replaces the playlist to the single opened
//! path at index 0; Next/Previous move it one entry, only on commit).
//! Both navigation ends are inert (no wrap, no side effect); there is
//! no repeat, no shuffle, no EOF auto-next, no failed-candidate
//! auto-skip. A clean activation failure leaves the cursor at the old
//! entry — which then names a track that no longer plays; honest,
//! because the cursor is navigation state, not audible-source truth.
//! A latched §G.6 violation permanently disables navigation too; no
//! recovery path exists.
//!
//! The provider set behind [`EpisodeStart`] is the wiring's business
//! (the real host mounts the SongCore decode + WASAPI output plugins;
//! the unit matrix mounts fake providers over the REAL kernel and the
//! REAL playback session). Nothing in this module knows PCM, devices
//! or platform mechanisms.

use std::path::{Path, PathBuf};

use qianqian_app::QianqianApp;
use qianqian_composition::{CompositionSnapshot, DisposeVerdict};
use qianqian_playback::{EpisodeTerminalOutcome, PlaybackSessionHandle};

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
    /// desired composition to quiescence. Never disposes anything: the
    /// CALLER owns the returned root on both arms (commit, or the
    /// failure-clean disposal).
    fn start(&self, source: &Path) -> StartAttempt;
}

/// One fresh-root start attempt. The start operation itself never
/// classifies establishment — that judgment is the replacement's
/// (below), read from the episode seam's authoritative evidence.
pub struct StartAttempt {
    /// The fresh root. The caller must either commit it or
    /// authoritatively dispose it; dropping it without `dispose` runs
    /// no teardown inverses.
    pub runtime: QianqianApp,
    /// The episode seam of the mounted session. May reference no live
    /// session (e.g. the composition was refused before activation);
    /// its observation then publishes nothing — which is exactly the
    /// honest "not established" evidence.
    pub handle: PlaybackSessionHandle,
    /// Why the start operation refused before any episode activation
    /// was attempted (component registration, composition refusal).
    /// `None` means the control sequence completed and the activation
    /// evidence on `handle` decides establishment.
    pub refused: Option<String>,
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
/// playlist / current-index APPLICATION NAVIGATION STATE (D14.6, the
/// playlist-authority closure): `Vec<PathBuf>` + `Option<usize>`,
/// owned here, read by nothing outside the App. The index is
/// commit-on-activation: it moves only on F6 replacement commit
/// evidence and is never playback truth — the read side stays the
/// D14.2 observation.
pub struct ReferencePlayerApp<S: EpisodeStart> {
    start: S,
    active: Option<ActiveEpisode>,
    violated: Option<RetainedViolatedRoot>,
    fail_stop: Option<String>,
    playlist: Vec<PathBuf>,
    cursor: Option<usize>,
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
            playlist: Vec::new(),
            cursor: None,
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

    /// Seed the STARTUP playlist (open representation per D14.6: the
    /// startup-args grammar details are not frozen). The transport
    /// calls this ONCE, right after the first Open committed:
    /// `entries[0]` IS the committed first episode, so the cursor
    /// starts at 0 on commit evidence. Nothing else ever appends to
    /// the playlist — a direct Open REPLACES it.
    pub fn seed_startup_playlist(&mut self, entries: Vec<PathBuf>) {
        if entries.is_empty() {
            // Total over the input: an empty seed leaves the navigation
            // state untouched (there is no committed entry to point at).
            return;
        }
        self.playlist = entries;
        self.cursor = Some(0);
    }

    /// The navigation projection the shell renders: the 1-based
    /// position of the cursor and the playlist length. Presentation of
    /// application navigation state — never playback truth, never an
    /// observable beyond this App's own shell (D14.6: no
    /// PlaylistFact / CurrentTrackFact exists).
    pub fn navigation_position(&self) -> Option<(usize, usize)> {
        Some((self.cursor? + 1, self.playlist.len()))
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
    /// playlist with the single opened path and selects index 0 — ON
    /// COMMIT only (D14.6 playlist closure): a refusal and a clean
    /// activation failure leave the navigation state exactly as it was.
    pub fn open(&mut self, candidate: &Path) -> OpenOutcome {
        let outcome = self.replace_episode(candidate);
        if matches!(outcome, OpenOutcome::Opened) {
            self.playlist = vec![candidate.to_owned()];
            self.cursor = Some(0);
        }
        outcome
    }

    /// Next (D14.6 playlist closure): select the candidate AFTER the
    /// cursor and invoke the same Open replacement. `None` = inert
    /// (no cursor, or the cursor already names the last entry — no
    /// wrap, no side effect, not even a probe). The cursor moves only
    /// on replacement commit evidence; a refusal or a clean activation
    /// failure leaves it where it was (one keypress advances at most
    /// one candidate).
    pub fn next_track(&mut self) -> Option<OpenOutcome> {
        let index = self
            .cursor
            .and_then(|cursor| cursor.checked_add(1))
            .filter(|&index| index < self.playlist.len())?;
        Some(self.navigate_to(index))
    }

    /// Previous (D14.6 playlist closure): the mirror of [`Self::next_track`].
    /// `None` = inert (no cursor, or the cursor already names the first
    /// entry — no wrap, no side effect).
    pub fn previous_track(&mut self) -> Option<OpenOutcome> {
        let index = self.cursor?.checked_sub(1)?;
        Some(self.navigate_to(index))
    }

    /// One navigation replacement: the SAME frozen sequence as a direct
    /// Open, with the cursor moved to the selected entry only on commit.
    fn navigate_to(&mut self, index: usize) -> OpenOutcome {
        let candidate = self.playlist[index].clone();
        let outcome = self.replace_episode(&candidate);
        if matches!(outcome, OpenOutcome::Opened) {
            self.cursor = Some(index);
        }
        outcome
    }

    /// The frozen D14.6 replacement sequence itself (probe → old-side
    /// clear → fresh root → authority-evidence commit), owning no
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
        //    activation evidence.
        let attempt = self.start.start(candidate);
        if let Some(refusal) = attempt.refused {
            return self.failure_clean_start(attempt.runtime, refusal);
        }
        // `Activated` over the whole fresh composition — the session's
        // own published evidence, never a snapshot read.
        let established = {
            let observation = attempt.handle.observe();
            observation.source_format.is_some() && observation.activation_error.is_none()
        };
        if established {
            self.active = Some(ActiveEpisode {
                runtime: attempt.runtime,
                handle: attempt.handle,
                source: candidate.to_owned(),
            });
            OpenOutcome::Opened
        } else {
            let diagnostic = attempt
                .handle
                .observe()
                .activation_error
                .unwrap_or_else(|| "activation did not establish a playback session".to_owned());
            self.failure_clean_start(attempt.runtime, diagnostic)
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
mod tests {
    //! The C7 unit matrix: the frozen D14.6 replacement semantics over
    //! the REAL composition kernel and the REAL playback session, with
    //! fake decode/output providers and a fake probe. The ordering log
    //! is written from kernel-mediated points only (provider activation,
    //! registered teardown effects, the probe call) — never by
    //! instrumenting the player — so the sequence assertions pin the
    //! player's actual call order.

    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use qianqian_app::QianqianApp;
    use qianqian_audio_api::ports::{
        AudioOutput, AudioOutputCapability, DecodeError, DecodeOpenError, DecodeOutcome,
        DecodedPcmStream, DrainSignal, DrainVerdict, GateSlice, OutputError, ParkOutcome,
        PcmDecode, PcmDecodeCapability, PcmFormat, PcmPull, ProviderSeekOutcome, RenderGate,
        RenderRequest, RenderStream, TailProbeOutcome,
    };
    use qianqian_composition::{ComponentSpec, DesiredEntry, Discharge, Revision};
    use qianqian_playback::playback_session_spec;

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
        fn seek(&mut self, _target: Duration) -> ProviderSeekOutcome {
            ProviderSeekOutcome::RefusedUnchanged
        }
    }

    struct FakeDecode;

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
                    .provide::<PcmDecodeCapability>(Rc::new(FakeDecode))
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
    /// - `attach_handle == false`: the returned seam handle is NOT the
    ///   handle the session was mounted with — the kernel snapshot
    ///   would report an Active session while the episode seam
    ///   publishes nothing. This is the §2.3 divergence scenario that
    ///   pins the authority-evidence rule.
    /// - `violating_cleanup`: the fresh roots' provider effects return
    ///   `Violated`, so any disposal of a fresh root latches §G.6.
    /// - `refuse_composition`: the desired composition references an
    ///   unregistered component, so `revise_desired` refuses.
    struct FakeEpisodeSource {
        log: Log,
        generation: AtomicU64,
        attach_handle: bool,
        violating_cleanup: bool,
        refuse_composition: bool,
    }

    impl FakeEpisodeSource {
        fn new() -> Self {
            Self {
                log: Arc::new(Mutex::new(Vec::new())),
                generation: AtomicU64::new(0),
                attach_handle: true,
                violating_cleanup: false,
                refuse_composition: false,
            }
        }

        fn without_handle_attachment(mut self) -> Self {
            self.attach_handle = false;
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

        fn start(&self, source: &Path) -> StartAttempt {
            let root = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
            let mut runtime = QianqianApp::new();
            runtime
                .register_component(
                    ComponentSpec::new("fake_decode_plugin")
                        .provides::<PcmDecodeCapability>()
                        .on_activate(fake_provider(
                            "decode",
                            self.log.clone(),
                            root,
                            self.violating_cleanup,
                        )),
                )
                .expect("fresh root admits the fake decode definition");
            runtime
                .register_component(
                    ComponentSpec::new("fake_output_plugin")
                        .provides::<AudioOutputCapability>()
                        .on_activate(fake_provider(
                            "output",
                            self.log.clone(),
                            root,
                            self.violating_cleanup,
                        )),
                )
                .expect("fresh root admits the fake output definition");
            let mounted = PlaybackSessionHandle::new();
            runtime
                .register_component(playback_session_spec(source.to_path_buf(), mounted.clone()))
                .expect("fresh root admits the playback session definition");

            let mut desired = vec![
                DesiredEntry::enabled("decode", "fake_decode_plugin", Revision::new(1)),
                DesiredEntry::enabled("output", "fake_output_plugin", Revision::new(1)),
                DesiredEntry::enabled("session", "playback_session", Revision::new(1)),
            ];
            if self.refuse_composition {
                desired.push(DesiredEntry::enabled(
                    "ghost",
                    "ghost_plugin",
                    Revision::new(1),
                ));
            }
            if let Err(errors) = runtime.revise_desired(desired) {
                return StartAttempt {
                    runtime,
                    handle: mounted,
                    refused: Some(format!("{errors}")),
                };
            }
            let handle = if self.attach_handle {
                mounted
            } else {
                // The divergence scenario: the mounted session runs on
                // `mounted`; the seam the player sees is an unattached
                // handle that publishes nothing.
                PlaybackSessionHandle::new()
            };
            StartAttempt {
                runtime,
                handle,
                refused: None,
            }
        }
    }

    fn player_with(source: FakeEpisodeSource) -> ReferencePlayerApp<FakeEpisodeSource> {
        ReferencePlayerApp::new(source)
    }

    const A: &str = "/media/finite-a.flac";
    const B: &str = "/media/finite-b.flac";
    /// Endless sources keep the first episode live until the
    /// replacement stops it.
    const LIVE_A: &str = "/media/live-a.flac";
    const LIVE_B: &str = "/media/live-b.flac";

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

    /// C7-11 (the §2.3 negative control): activation truth comes from
    /// the episode seam's published evidence, NEVER from the kernel
    /// snapshot. Here the mounted session is genuinely Active in kernel
    /// truth while the seam the player sees publishes nothing — a
    /// snapshot-reading implementation would commit; the authority-
    /// evidence implementation must fail clean and dispose the attempt.
    #[test]
    fn activation_truth_comes_from_the_episode_seam_never_from_the_snapshot() {
        let source = FakeEpisodeSource::new().without_handle_attachment();
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
        player.seed_startup_playlist(vec![PathBuf::from(LIVE_A), PathBuf::from(B)]);
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
        player.seed_startup_playlist(vec![
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
        player.seed_startup_playlist(vec![PathBuf::from(LIVE_A), PathBuf::from(LIVE_B)]);

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
        player.seed_startup_playlist(vec![
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
        player.seed_startup_playlist(vec![
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
        player.seed_startup_playlist(vec![PathBuf::from(LIVE_A), PathBuf::from(LIVE_B)]);

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
        player.seed_startup_playlist(vec![PathBuf::from(A), PathBuf::from(B)]);
        let handle = player.active_handle().expect("committed").clone();

        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_eq!(
            player.navigation_position(),
            Some((1, 2)),
            "the episode settled; the navigation state did not move"
        );
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
}
