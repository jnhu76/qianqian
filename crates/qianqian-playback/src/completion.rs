//! Episode terminal settlement core — the crate-internal realization of
//! the D11 designated semantic authority (ADR-PBK-002 §17) and the D14.3
//! authority-owned settlement shape. It is NOT the application seam: the
//! application-facing episode surface is [`crate::handle::
//! PlaybackSessionHandle`] (F2 reality-gate-2 verdict B); this type stays
//! an internal replaceable realization behind it.
//!
//! Settlement ownership (D11 / D14.3):
//!
//! ```text
//! mechanism evidence producers (worker wrapper, render leg via
//! DrainSignal, stop intent, activation diagnostics)
//!     publish evidence only
//!         ↓  every publication path runs the session-owned settlement
//!            step synchronously, under the one completion lock
//! Playback Session-owned settlement (resolve + commit exactly once)
//!         ↓
//! observe / wait_terminal consume the committed Fact only
//! ```
//!
//! Three publication sites can complete the decisive evidence set, and
//! all three settle on the publishing leg's own call stack:
//!
//! ```text
//! decode failure evidence   → settle inside decode_failed
//! worker terminal evidence  → settle inside worker_exited
//! drain verdict             → the session installs a one-shot observer
//!                             on its DrainSignal at construction, so
//!                             the first successful complete() publishes
//!                             and settles synchronously on the render
//!                             leg's call stack, before complete returns
//! ```
//!
//! There is no settlement watcher, resolver thread, or asynchronous gap
//! (D14.3 forbids a resolver thread outright). `request_stop` records
//! intent through the same completion lock, so the lock IS the decision
//! boundary: a stop linearized before the decisive publication
//! participates in the classification; a stop linearized after it cannot
//! relabel the already-committed outcome (D11 late-command rule).
//!
//! Neither `observe_snapshot` nor `wait_terminal` resolves: a consumer
//! call can never create the terminal Fact, and no consumer call is
//! required for it to appear ("worker evidence last" and "drain verdict
//! last" both commit autonomously — pinned by the M4/W4 witnesses).
//!
//! One core serves exactly one episode: activation binds one edge and
//! the outcome memoizes on first settlement, so do not re-use a
//! completion across a retried or restarted episode.
//!
//! Outcome precedence, stated once here: a published decode failure
//! dominates everything (it is checked first and is not relabelled by
//! stop intent); otherwise the drain verdict plus the worker's exit
//! terminal decide, with recorded stop intent disambiguating an aborted
//! drain between a user stop and a device failure.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use qianqian_audio_api::ports::{
    DrainSignal, DrainVerdict, GateEvent, PcmFormat, PositionEvidence, RenderGate,
    SeekParkRelease,
};

use crate::edge::{EdgeTerminal, PcmEdge};
use crate::handle::{EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionObservation};

/// How one playback episode ended. Crate-internal realization: the
/// public semantic contract is only the stable
/// [`EpisodeTerminalOutcome`] triple; the failure stage here is a
/// diagnostic (D14.2) and must not leak into the public enum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SessionOutcome {
    /// EOF was produced, drained and played out.
    Completed,
    /// The episode failed before completion. The stage names which leg
    /// failed first ("decode: ..." or "device") — a diagnostic, not a
    /// frozen semantic variant (D14.2).
    Failed { stage: String },
    /// Stop was requested before completion.
    Stopped,
}

impl SessionOutcome {
    /// Split into the stable public semantic outcome and the failure
    /// diagnostic carried alongside it. This is the single truth-class
    /// boundary (D14.2): `Completed`/`Stopped` can never carry a
    /// diagnostic, and a `Failed` diagnostic is presentation text — its
    /// presence, absence or spelling is not part of the semantic
    /// contract.
    pub(crate) fn split(self) -> (EpisodeTerminalOutcome, Option<String>) {
        match self {
            SessionOutcome::Completed => (EpisodeTerminalOutcome::Completed, None),
            SessionOutcome::Stopped => (EpisodeTerminalOutcome::Stopped, None),
            SessionOutcome::Failed { stage } => (EpisodeTerminalOutcome::Failed, Some(stage)),
        }
    }
}

struct CompletionState {
    outcome: Option<SessionOutcome>,
    /// Terminal the decode worker observed on the edge at its exit.
    worker_terminal: Option<EdgeTerminal>,
    decode_failure: Option<String>,
    /// The drain verdict, mirrored into the state by the session-owned
    /// observer installed on the DrainSignal (first wins). Mirroring
    /// keeps the resolver a pure function of the one lock-protected
    /// record: no settlement decision ever reads a second lock.
    drain_verdict: Option<DrainVerdict>,
    /// Source PCM format, published once at session activation
    /// (mechanism-evidence diagnostic readback).
    source_format: Option<PcmFormat>,
    /// Source duration evidence (D14.8), published once at session
    /// activation from the decode provider's probe/open report. Optional
    /// source-scoped Mechanism Evidence: unset means "no duration was
    /// reported" (unknown), which is never collapsed into zero and never
    /// estimated. It stays observable after a terminal Fact — it is
    /// source evidence, not a playback-state projection.
    source_duration: Option<Duration>,
    /// Why activation raised, published by the session itself (the
    /// kernel's diagnostic surface carries the verdict, not the message).
    activation_failure: Option<String>,
    /// Stop intent, recorded by [`SessionCompletion::request_stop`].
    /// Command state, not outcome truth: an episode that already ended
    /// (Completed/Failed) is not retroactively renamed by a late stop.
    stop_requested: bool,
    /// Pause intent (D14.7). Command state, same family as
    /// `stop_requested`: recorded by `request_pause`, cleared by
    /// `request_resume`; a later stop does not relabel it, and after
    /// settlement it is inert history (the establishment projection
    /// guards on the unsettled state instead).
    pause_requested: bool,
    /// Render engagement evidence (D14.7): the render leg is parked at
    /// the pre-GetBuffer gate of the CURRENT pause engagement and will
    /// submit no further PCM while parked. Never a Fact.
    engaged: bool,
    /// Output-tail quiescence evidence (D14.7): no frame submitted
    /// BEFORE the current engagement remains queued for rendering.
    /// Belongs to the current engagement only — reset on engagement and
    /// on disengagement, so a previous pause cycle's quiescence can
    /// never satisfy a later pause.
    tail_quiesced: bool,
    /// Disengagement-ack evidence latch (D14.7): the render leg's most
    /// recent engagement has disengaged. Attribution is exact at
    /// engagement granularity: same-leg event ordering makes
    /// `Disengaged(old)` happen-before `Engaged(new)`, so `Engaged` is
    /// the current-engagement fence and clears any evidence a prior
    /// engagement left — a previous cycle's disengagement can never be
    /// misattributed to the current engagement (D14.7 corrective-2).
    /// Crate-internal mechanism evidence only: it is NOT mirrored into
    /// the public observation (the D14.7 AUTHORITY-CORRECTIVE removed
    /// the `Resumed` projection this latch once existed to ground —
    /// disengagement cannot prove a viable render leg remains), and it
    /// never feeds control or settlement. Kept for current-engagement
    /// bookkeeping and as the verification subject of the attribution
    /// fence oracle.
    disengagement_observed: bool,
    /// Set once by [`SessionCompletion::release_pause_gate`] — the
    /// authority-owned teardown path has begun releasing the pause
    /// gate. Routing witness for the D14.7 teardown wake obligation: a
    /// pause that linearizes after this point must not re-park the leg,
    /// or the teardown's `stop_and_join` could never return.
    teardown_released: bool,
    /// The session's data-plane edge, bound by activation as the stop
    /// target. `None` until the episode binds one (or forever, if
    /// activation failed). The edge is the session-owned stop mechanism;
    /// this core only routes intent to it.
    stop_target: Option<Arc<PcmEdge>>,
    /// Seek-park engagement evidence (D14.5): the render leg is parked
    /// at the pre-GetBuffer gate under a routed seek hold (a
    /// cut-attributed park — never pause engagement, never `Paused`
    /// truth). Never a Fact; a commit precondition only.
    seek_engaged: bool,
    /// Output-tail quiescence of the CURRENT seek park (D14.5):
    /// padding == 0 observed while seek-parked. Belongs to the current
    /// seek park only; cleared on seek engagement and disengagement.
    seek_tail_quiesced: bool,
    /// Seek landing evidence (D14.5): the decode worker published
    /// "provider repositioned at L" (strictly after its edge purge).
    /// `None` = not published; `Some(None)` = published with an unknown
    /// landing (the Position projection is withdrawn for the episode).
    /// Mechanism evidence; never a Fact, never product surface.
    seek_landing: Option<Option<u64>>,
    /// A [`SeekSlot`] command that the worker picked up and resolved as
    /// a proven pre-mutation refusal (`RefusedUnchanged`): inert
    /// protocol history, recorded for verification only.
    seek_refused: bool,
    /// The session's cutover commit record (D14.5): true iff the
    /// session-owned protocol path evaluated `landing published ∧ edge
    /// invalidated ∧ tail quiesced ∧ leg parked ∧ episode unsettled`
    /// and recorded the commit. Protocol state owned by the session —
    /// NOT a Fact, NOT a terminal variant, NOT public surface. The
    /// observable consequences are the Position jump and the absence of
    /// stale audio.
    cut_committed: bool,
}

/// The seek command slot (D14.5): the one command a seek request may
/// plant for the decode worker to pick up at its loop-top serialization
/// path. A dedicated small lock — deliberately NOT the completion state
/// lock — because the worker peeks it once per staging block (the
/// frozen realtime-cost budget: "one Mutex try on a session-owned slot
/// per staging block", decode-side, off the RT path) and the completion
/// lock already carries every evidence publication.
#[derive(Default)]
struct SeekSlot {
    /// The planted seek target, `None` once the worker picked it up.
    command: Option<Duration>,
    /// One-seek-in-flight marker (D14.5 frozen policy): set at planting,
    /// cleared by the worker only when the protocol fully resolved
    /// (committed, refused with its remainder finished, abandoned, or
    /// destructive-failed). A second request while this is set is inert.
    in_flight: bool,
}

/// Crate-internal episode settlement core (see the module doc). The
/// application reaches this only through
/// [`crate::handle::PlaybackSessionHandle`].
#[derive(Clone)]
pub(crate) struct SessionCompletion {
    state: Arc<CompletionArc>,
}

struct CompletionArc {
    state: Mutex<CompletionState>,
    signal: Condvar,
    drain: DrainSignal,
    /// The episode's render pause gate (D14.7), created here with the
    /// evidence observer so it is in place before activation can hand it
    /// to a render stream. The gate routes pause intent and seek holds
    /// to the mechanism and acknowledges engagement/tail-quiescence/
    /// disengagement back as mechanism evidence (never Facts, never
    /// settlement inputs).
    gate: RenderGate,
    /// The seek command slot (D14.5), separate from `state` — see
    /// [`SeekSlot`].
    seek_slot: Mutex<SeekSlot>,
    /// The episode's position-evidence cell (D14.8), created here and
    /// handed to the same render stream through its open request. It is
    /// deliberately NOT part of the lock-protected state: the render leg
    /// publishes into it from its realtime path with one relaxed atomic
    /// update, and the observation reads it with one relaxed load while
    /// holding the state lock — so the hot atomic never shares a lock,
    /// and never a cache line, with the settlement state.
    position: PositionEvidence,
}

impl Default for SessionCompletion {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionCompletion {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new_cyclic(|core| {
                let gate = RenderGate::with_observer({
                    // Weak on purpose (same posture as the drain
                    // observer): the completion owns the gate, so a
                    // strong observer reference would be a cycle. An
                    // inert observer after teardown is exactly the right
                    // semantics.
                    let core = core.clone();
                    move |event| {
                        if let Some(core) = core.upgrade() {
                            publish_evidence(&core, |state| apply_gate_event(state, event));
                        }
                    }
                });
                CompletionArc {
                    state: Mutex::new(CompletionState {
                        outcome: None,
                        worker_terminal: None,
                        decode_failure: None,
                        drain_verdict: None,
                        source_format: None,
                        source_duration: None,
                        activation_failure: None,
                        stop_requested: false,
                        pause_requested: false,
                        engaged: false,
                        tail_quiesced: false,
                        disengagement_observed: false,
                        teardown_released: false,
                        stop_target: None,
                        seek_engaged: false,
                        seek_tail_quiesced: false,
                        seek_landing: None,
                        seek_refused: false,
                        cut_committed: false,
                    }),
                    signal: Condvar::new(),
                    drain: DrainSignal::with_on_complete({
                        // Weak on purpose: the completion owns the signal,
                        // so a strong observer reference would be a
                        // reference cycle. The signal outlives publication
                        // paths only through the render leg's own join
                        // ordering, so an inert observer after teardown is
                        // exactly the right semantics.
                        let core = core.clone();
                        move |verdict| {
                            if let Some(core) = core.upgrade() {
                                publish_evidence(&core, |state| {
                                    if state.drain_verdict.is_none() {
                                        state.drain_verdict = Some(verdict);
                                    }
                                });
                            }
                        }
                    }),
                    gate,
                    seek_slot: Mutex::new(SeekSlot::default()),
                    position: PositionEvidence::new(),
                }
            }),
        }
    }

    /// The session publishes its activation failure (first wins). This
    /// is a diagnostic, not a terminal Fact: an activation that never
    /// established a live episode leaves the terminal outcome `None`
    /// forever (D11 activation firewall).
    pub(crate) fn activation_failed(&self, message: &str) {
        let mut guard = self.state.state.lock().expect("completion lock");
        if guard.activation_failure.is_none() {
            guard.activation_failure = Some(message.to_owned());
        }
    }

    /// The session publishes the endpoint's source format at activation.
    pub(crate) fn set_source_format(&self, format: PcmFormat) {
        let mut guard = self.state.state.lock().expect("completion lock");
        if guard.source_format.is_none() {
            guard.source_format = Some(format);
        }
    }

    /// The session publishes the duration the decode mechanism reported
    /// for this source at probe/open time (D14.8). Called only when the
    /// provider reported one; a provider that reported none leaves the
    /// evidence unset, and unset observationally means unknown (`None`) —
    /// never zero, never an estimate. First-wins, like the format.
    pub(crate) fn set_source_duration(&self, duration: Duration) {
        let mut guard = self.state.state.lock().expect("completion lock");
        if guard.source_duration.is_none() {
            guard.source_duration = Some(duration);
        }
    }

    /// The drain signal handed to the render stream's open request. The
    /// session-owned drain observer is installed at construction; the
    /// render leg's first `complete` therefore publishes and settles
    /// synchronously on the render leg's call stack.
    pub(crate) fn drain_signal(&self) -> DrainSignal {
        self.state.drain.clone()
    }

    /// The decode worker (or its panic guard) reports a decode failure.
    /// First failure wins; later calls are no-ops. The publication and
    /// the settlement step run under one lock hold, inside this call.
    pub(crate) fn decode_failed(&self, message: &str) {
        self.publish(|state| {
            if state.decode_failure.is_none() {
                state.decode_failure = Some(message.to_owned());
            }
        });
    }

    /// Request the episode to stop.
    ///
    /// This is a command, not a fact: it records stop intent and releases
    /// the session's data-plane stop (which is idempotent and first-wins,
    /// so it never overwrites a committed EOF or failure). How the episode
    /// actually ends remains decided solely by the session-owned
    /// settlement step. Calling this after the outcome is settled is a
    /// no-op with respect to that outcome.
    ///
    /// Intent is recorded before the data-plane stop is released, so any
    /// terminal evidence caused by this stop necessarily observes the
    /// intent (D11 decision-time stability); a later stop cannot relabel
    /// an already-decisive classification.
    ///
    /// Safe from any thread and any state:
    /// - before the session bound an edge (not yet activated): intent is
    ///   recorded and applied the moment activation binds the edge;
    /// - while playing: both legs are woken with terminal outcomes;
    /// - after settlement: nothing changes.
    pub(crate) fn request_stop(&self) {
        let target = {
            let mut guard = self.state.state.lock().expect("completion lock");
            guard.stop_requested = true;
            // Stop releases the pause gate too (D14.7), under the same
            // lock hold that records the intent: gate routing is
            // linearized with the command state, so a pause command that
            // linearizes after this stop observes `stop_requested` and
            // cannot re-park the episode. A leg parked at the pre-
            // GetBuffer gate is not inside read_frames, so the
            // data-plane stop alone cannot wake it. The release publishes
            // disengagement evidence when the leg exits; the gate never
            // aborts the leg — the loop proceeds once more and the
            // data-plane terminal (edge stop, EOF, failure) decides the
            // outcome.
            //
            // A routed seek hold is released by the same token (D14.5:
            // stop wins over an in-flight cut; the hold must not keep
            // the leg parked across the shutdown). Any pending release
            // payload stays stored; a leg waking into a stopping episode
            // treats an unconsumed payload exactly like its own exit
            // path: consume, apply-or-ignore, and let the data-plane
            // terminal decide.
            self.state.gate.set_paused(false);
            self.state.gate.set_seek_hold(false);
            guard.stop_target.clone()
        };
        if let Some(edge) = target {
            edge.stop();
        }
    }

    /// Request the episode to pause (D14.7). Command only: records pause
    /// intent on the episode seam and routes it to the episode's render
    /// gate, whose loop-top check parks the render leg before any device
    /// buffer is held. Idempotent. How (and whether) the pause physically
    /// establishes remains mechanism evidence — the Paused projection is
    /// derived, never recorded here.
    ///
    /// Recording a NEW pause cycle (intent false→true) also clears any
    /// latched disengagement evidence as command-side hygiene; the
    /// attribution rule itself is the leg's Engaged event (see
    /// `apply_gate_event`), which fences a previous cycle's
    /// disengagement out of this one.
    ///
    /// Intent routing is linearized with the command state under the one
    /// completion lock. A pause that linearizes after stop intent —
    /// including the whole stop→settlement window — or after the
    /// teardown release has begun is recorded as inert command history
    /// but routes nothing: a released, released-then-stopping, or
    /// tearing-down episode must never be re-parked (D14.7: stop and
    /// teardown wake every parked participant with bounded latency), and
    /// a settled episode has no leg to park.
    pub(crate) fn request_pause(&self) {
        let mut guard = self.state.state.lock().expect("completion lock");
        if !guard.pause_requested {
            // Command-side hygiene only (D14.7 corrective-2): the event
            // attribution fence is the leg's own Engaged event, which
            // clears prior-cycle evidence the moment the current
            // engagement begins. This reset just keeps a stale
            // prior-cycle disengagement out of the latch in the window
            // before that engagement is observed. Repeated pauses
            // within one cycle change nothing.
            guard.disengagement_observed = false;
        }
        guard.pause_requested = true;
        if guard.stop_requested || guard.teardown_released || guard.outcome.is_some() {
            return;
        }
        self.state.gate.set_paused(true);
    }

    /// Release a recorded pause (D14.7). Command only: clears pause
    /// intent and releases the gate; the woken leg proceeds once more and
    /// the data plane decides what its next read sees. Idempotent.
    /// Release routing stays unconditional — a release can never wedge
    /// anything, and linearization under the one completion lock keeps
    /// the gate's view consistent with the recorded intent.
    pub(crate) fn request_resume(&self) {
        let mut guard = self.state.state.lock().expect("completion lock");
        guard.pause_requested = false;
        self.state.gate.set_paused(false);
    }

    // --- F5 seek protocol (ADR-PBK-002 D14.5) --------------------------------
    //
    // The seek command is a same-episode Command; the cutover protocol
    // runs on the decode worker (a session-owned execution path). These
    // methods are the session-owned control surface of that protocol:
    // acceptance, command planting, evidence reads for the worker, and
    // the commit/abort decisions. The render leg's rebase happens on
    // the leg's own path through the gate's release payload.

    /// Record a seek command (D14.5 acceptance) and route the cut's
    /// park to the render leg. Inert — a command with no semantic
    /// effect, exactly like late stop/pause — unless every frozen
    /// acceptance condition holds:
    ///
    /// ```text
    /// episode unsettled (no terminal Fact committed)
    /// data plane Open (the bound edge exists and its terminal is Open —
    ///     the post-EOF drain window is explicitly NOT seekable)
    /// no stop intent already recorded, no teardown release begun
    /// no seek already in flight (one-seek policy; no queueing)
    /// ```
    ///
    /// `target` is source-relative media time, non-negative by type.
    /// Beyond-duration targets pass through: the PROVIDER decides
    /// validity and clamping (duration evidence is never consulted
    /// here). Acceptance records the command and parks the leg; it does
    /// NOT imply a cutover — "a seek request is not a cutover".
    pub(crate) fn request_seek(&self, target: Duration) {
        // Acceptance reads the command state and the edge terminal; the
        // edge is reached only after this lock is dropped (the
        // established completion→edge discipline: `request_stop`'s
        // pattern). A terminal that changes in the window between this
        // check and the worker's pickup is caught again at the worker's
        // serialization point — acceptance and pickup are two defense
        // lines, not one.
        let edge = {
            let guard = self.state.state.lock().expect("completion lock");
            if guard.outcome.is_some()
                || guard.stop_requested
                || guard.teardown_released
                || guard.activation_failure.is_some()
            {
                return;
            }
            guard.stop_target.clone()
        };
        let Some(edge) = edge else {
            return; // never activated / no data plane
        };
        if edge.terminal() != crate::edge::EdgeTerminal::Open {
            return; // data plane not Open (includes the post-EOF drain window)
        }
        // Plant the command under the slot lock: the one-seek policy is
        // decided here, atomically with the planting. No queueing, no
        // coalescing, no request identity — this is why no SeekId
        // exists.
        {
            let mut slot = self.state.seek_slot.lock().expect("seek slot lock");
            if slot.in_flight {
                return; // one seek in flight; the second request is inert
            }
            slot.in_flight = true;
            slot.command = Some(target);
        }
        // Park the render leg for the cut. The leg may be parked by
        // pause already (a paused episode is seekable: its quiesced tail
        // satisfies the output-cut precondition and the pause intent
        // survives the seek) — routing the hold is harmless there; the
        // seek-park engages when the leg next reaches its loop top.
        self.state.gate.set_seek_hold(true);
    }

    /// The decode worker's loop-top pickup: take the planted command, if
    /// any. One call per staging block (the frozen per-block budget).
    pub(crate) fn take_seek_command(&self) -> Option<Duration> {
        let mut slot = self.state.seek_slot.lock().expect("seek slot lock");
        slot.command.take()
    }

    /// The worker's mid-write observation: is a seek command outstanding
    /// in the slot? Used by the interruptible write between bounded
    /// slices (with no other lock held). A `try_lock` peek: contention
    /// with a planting writer resolves to "not yet observed" and the
    /// next slice re-peeks — the serialization point is bounded by the
    /// wait slice, not by lock ordering.
    pub(crate) fn seek_command_observed(&self) -> bool {
        match self.state.seek_slot.try_lock() {
            Ok(slot) => slot.command.is_some(),
            Err(_) => false,
        }
    }

    /// "The render leg is parked at its pre-GetBuffer gate and holds no
    /// device buffer" — the D14.5 worker precondition for calling the
    /// provider, satisfied under EITHER attribution: a seek-parked leg
    /// (the cut's own park) or a pause-parked leg (a paused episode's
    /// seek reuses the current pause engagement, whose park is the same
    /// physical evidence class).
    pub(crate) fn leg_parked_evidence(&self) -> bool {
        let guard = self.state.state.lock().expect("completion lock");
        guard.engaged || guard.seek_engaged
    }

    /// Whether the seek protocol must abort instead of proceeding:
    /// stop intent recorded, the episode already settled, or the
    /// authority-owned teardown release has begun. Every wait in the
    /// protocol re-checks this, so stop always wins and no protocol
    /// step can wedge teardown.
    pub(crate) fn seek_aborted(&self) -> bool {
        let guard = self.state.state.lock().expect("completion lock");
        guard.stop_requested || guard.outcome.is_some() || guard.teardown_released
    }

    /// The D14.5 commit-boundary tail condition, paired per park kind:
    /// "output tail quiesced while the leg is parked" holds iff the leg
    /// is parked under one attribution AND THAT engagement observed its
    /// tail quiesced. The pairing is what keeps a previous park's
    /// quiescence from satisfying a later park.
    pub(crate) fn seek_tail_condition(&self) -> bool {
        let guard = self.state.state.lock().expect("completion lock");
        (guard.engaged && guard.tail_quiesced) || (guard.seek_engaged && guard.seek_tail_quiesced)
    }

    /// The worker publishes the three-class provider outcome's inert
    /// class (evidence only — no terminal effect, no purge, no rebase):
    /// a proven pre-mutation `RefusedUnchanged`.
    pub(crate) fn seek_refused(&self) {
        self.publish(|state| state.seek_refused = true);
    }

    /// The worker publishes the landing evidence AFTER its own edge
    /// purge (the frozen program order: song_seek → staging discard →
    /// invalidate → landing → hold). `None` landing = the provider
    /// could not report one (unknown stays unknown; the commit still
    /// happens — stale exclusion is independent of landing knowledge —
    /// and the Position projection is withdrawn for the episode).
    pub(crate) fn seek_landing_published(&self, landing: Option<u64>) {
        self.publish(|state| {
            if state.seek_landing.is_none() {
                state.seek_landing = Some(landing);
            }
        });
    }

    /// The session's cutover-commit decision (D14.5 commit boundary),
    /// evaluated and recorded atomically under the completion lock —
    /// the same lock stop intent linearizes through, so "stop before
    /// the commit wins" holds by construction. Returns `true` iff the
    /// commit was recorded; in that case the rebase release has been
    /// routed to the render leg. On `false` (stop/teardown/settled won
    /// the race) the release is routed as an abort instead: no commit,
    /// no rebase, no partial state.
    ///
    /// Preconditions the CALLER owns (worker program order, not lock
    /// state): the provider succeeded, the staging was discarded, and
    /// `edge.invalidate()` has already returned on the worker's path.
    /// Preconditions evaluated HERE under the lock: landing published,
    /// tail quiesced while the leg is parked, episode unsettled.
    ///
    /// The one-seek slot is NOT freed here, on either branch: a
    /// committed release is part of the cut until the LEG has consumed
    /// it (a later hold would wipe an unconsumed `Committed` and lose
    /// the rebase — the seek matrices caught that exact pause-shaped
    /// interleaving), so the worker keeps the slot occupied until it
    /// observes `seek_release_pending() == false` and then frees it. On
    /// the abort branch no free is needed at all: every abort condition
    /// (stop intent, settled, teardown release) implies the episode is
    /// ending, so the slot is never consulted again.
    pub(crate) fn commit_seek_cutover(&self, landing: Option<u64>) -> bool {
        let decision = {
            let mut guard = self.state.state.lock().expect("completion lock");
            let parked_and_quiesced = (guard.engaged && guard.tail_quiesced)
                || (guard.seek_engaged && guard.seek_tail_quiesced);
            let unsettled =
                guard.outcome.is_none() && !guard.stop_requested && !guard.teardown_released;
            if guard.seek_landing.is_some() && parked_and_quiesced && unsettled {
                guard.cut_committed = true;
                true
            } else {
                false
            }
        };
        if decision {
            self.state
                .gate
                .release_seek_hold(SeekParkRelease::Committed { landing });
        } else {
            self.state.gate.release_seek_hold(SeekParkRelease::Aborted);
        }
        decision
    }

    /// Whether the current cut's routed release still awaits the leg's
    /// consumption. The worker polls this after a commit and frees the
    /// one-seek slot only when it clears — see
    /// [`SessionCompletion::commit_seek_cutover`].
    pub(crate) fn seek_release_pending(&self) -> bool {
        self.state.gate.seek_release_pending()
    }

    /// Release the render leg from a routed seek park with NO rebase
    /// payload (an abort). The refusal path uses this alone: the leg
    /// resumes the old world immediately, and the one-in-flight slot
    /// stays occupied until the preserved remainder has been finished
    /// (a second seek must not park the leg mid-remainder).
    pub(crate) fn release_seek_park(&self) {
        self.state.gate.release_seek_hold(SeekParkRelease::Aborted);
    }

    /// Free the one-in-flight slot: the seek protocol fully resolved.
    pub(crate) fn clear_seek_in_flight(&self) {
        let mut slot = self.state.seek_slot.lock().expect("seek slot lock");
        slot.in_flight = false;
        slot.command = None;
    }

    /// Release the leg AND free the slot: the abandon / destructive
    /// failure / no-commit routes, where nothing of the protocol
    /// remains to finish.
    pub(crate) fn release_seek_without_commit(&self) {
        self.release_seek_park();
        self.clear_seek_in_flight();
    }

    /// Whether the one-in-flight slot is currently occupied (the frozen
    /// one-seek policy, observable for verification only).
    #[cfg(all(test, not(loom)))]
    pub(crate) fn seek_in_flight(&self) -> bool {
        self.state.seek_slot.lock().expect("seek slot lock").in_flight
    }

    /// Verification reads of the seek protocol latches (crate-internal
    /// only; never product surface). Partitioned out of loom builds
    /// with their test consumers.
    #[cfg(all(test, not(loom)))]
    pub(crate) fn seek_protocol_state(&self) -> (bool, bool, Option<Option<u64>>) {
        let guard = self.state.state.lock().expect("completion lock");
        (guard.seek_refused, guard.cut_committed, guard.seek_landing)
    }


    /// Release the pause gate WITHOUT touching the recorded pause intent
    /// (D14.7 teardown obligation): session settlement/teardown must wake
    /// every parked participant, because a leg parked at the gate cannot
    /// observe the data-plane stop, and `stop_and_join` must terminate.
    /// The leg's exit publishes disengagement evidence.
    ///
    /// The release linearizes pause routing under the same completion
    /// lock hold (the teardown-side twin of the `request_stop` rule):
    /// once this runs, a pause that linearizes after it is inert
    /// history and routes nothing, so no pause can re-park the leg
    /// between this release and the join that follows it.
    pub(crate) fn release_pause_gate(&self) {
        let mut guard = self.state.state.lock().expect("completion lock");
        guard.teardown_released = true;
        self.state.gate.set_paused(false);
        // The teardown wake obligation covers a seek-parked leg too
        // (D14.5): a leg parked at the seek gate is likewise not inside
        // read_frames, so the data-plane stop alone cannot wake it. The
        // release routes no payload: a seek park woken by teardown is an
        // aborted protocol (any already-stored payload stays stored and
        // is simply consumed-or-ignored by the exiting leg).
        self.state.gate.set_seek_hold(false);
    }

    /// The episode's render gate, handed to the output provider at
    /// activation. Session-internal binding seam: the application reaches
    /// the same routing only through `request_pause`/`request_resume`.
    pub(crate) fn render_gate(&self) -> RenderGate {
        self.state.gate.clone()
    }

    /// The episode's position-evidence cell, handed to the output
    /// provider at activation (D14.8). Session-internal binding seam: the
    /// application never reaches the cell, only the projection
    /// `observe_snapshot` derives from it.
    pub(crate) fn position_evidence(&self) -> PositionEvidence {
        self.state.position.clone()
    }

    /// Frames currently buffered on the session's edge, once bound.
    /// Test/verifier diagnostic only (F2 ruling, D14.3): mechanism
    /// evidence, NOT application observation and NOT UI contract. It
    /// exists only in test builds so it cannot drift into the product
    /// seam. `None` before the session bound its edge. Partitioned out of
    /// loom builds together with its only consumers (the thread-spawning
    /// white-box settlement tests).
    #[cfg(all(test, not(loom)))]
    pub(crate) fn buffered_frames(&self) -> Option<usize> {
        let guard = self.state.state.lock().expect("completion lock");
        guard
            .stop_target
            .as_ref()
            .map(|edge| edge.buffered_frames())
    }

    /// The session binds its data-plane edge as the stop target at
    /// activation. If stop intent was already recorded before the edge
    /// existed ("stop before the episode fully opened"), it is applied
    /// immediately, so the episode ends `Stopped` instead of playing past
    /// a stop that arrived first.
    ///
    /// Session-internal binding seam: the application reaches the same
    /// effect only through [`SessionCompletion::request_stop`].
    pub(crate) fn bind_stop_target(&self, edge: Arc<PcmEdge>) {
        let already_requested = {
            let mut guard = self.state.state.lock().expect("completion lock");
            // First binding wins; activation binds exactly once.
            if guard.stop_target.is_none() {
                guard.stop_target = Some(edge.clone());
            }
            guard.stop_requested
        };
        if already_requested {
            edge.stop();
        }
    }

    /// The decode worker wrapper reports the edge terminal at its exit.
    /// The publication and the settlement step run under one lock hold,
    /// inside this call.
    pub(crate) fn worker_exited(&self, terminal: EdgeTerminal) {
        self.publish(|state| state.worker_terminal = Some(terminal));
    }

    /// One coherent observation of the episode, taken under a single
    /// lock acquisition so the returned fields coexisted at one real
    /// instant (no torn combinations such as `Stopped` with
    /// `stop_requested == false`).
    ///
    /// The position sample is the one field that is not part of that
    /// single-instant promise: it is one pure load of the episode's
    /// position cell (D14.8), which the render leg publishes to
    /// independently and which gives no freshness bound. The load is
    /// taken only for an episode that is both live and unsettled — a
    /// committed terminal Fact, or a recorded activation failure,
    /// withdraws the projection — and it is a read: nothing here writes,
    /// clamps, or settles anything.
    pub(crate) fn observe_snapshot(&self) -> PlaybackSessionObservation {
        let guard = self.state.state.lock().expect("completion lock");
        let (terminal_outcome, failure_diagnostic) = match &guard.outcome {
            Some(outcome) => {
                let (semantic, diagnostic) = outcome.clone().split();
                (Some(semantic), diagnostic)
            }
            None => (None, None),
        };
        PlaybackSessionObservation {
            terminal_outcome,
            failure_diagnostic,
            stop_requested: guard.stop_requested,
            source_format: guard.source_format,
            source_duration: guard.source_duration,
            // Two withdrawal conditions, both read inside this lock
            // hold. The activation-failure one is not redundant with
            // settlement: the render mechanism opens — and starts
            // publishing from the loop-top readings it already takes —
            // BEFORE the last fallible activation step (the decode
            // worker spawn), and an open-aborted stream publishes from
            // the park slice of the very leg that is about to be
            // aborted. So an activation that raises can leave a
            // non-empty cell behind for an episode that never existed.
            // "Never activated" means no position, however many samples
            // the dying mechanism managed to publish.
            position: if terminal_outcome.is_none() && guard.activation_failure.is_none() {
                self.state.position.published()
            } else {
                // Withdrawal is this gate, not a cell write: the render
                // mechanism keeps whatever it last published until its
                // own teardown drops it, and a late publication by a
                // dying leg is simply not derived from.
                None
            },
            activation_error: guard.activation_failure.clone(),
            pause_requested: guard.pause_requested,
            pause_engagement: match (guard.engaged, guard.tail_quiesced) {
                (true, true) => PauseEngagement::TailQuiesced,
                (true, false) => PauseEngagement::Engaged,
                (false, _) => PauseEngagement::Disengaged,
            },
        }
    }

    /// The committed terminal outcome, if any. Pure read: never settles.
    /// Verifier-facing (the decision-table oracle's read); not part of
    /// any runtime path. Partitioned out of loom builds together with
    /// its only consumers (the white-box verifier suites).
    #[cfg(all(test, not(loom)))]
    pub(crate) fn committed(&self) -> Option<SessionOutcome> {
        self.state
            .state
            .lock()
            .expect("completion lock")
            .outcome
            .clone()
    }

    /// Block until the episode's terminal Fact is committed. Pure wait:
    /// it never evaluates the contract and never commits — settlement
    /// happens only on the session-owned publication paths, and every
    /// commit notifies this condvar.
    pub(crate) fn wait_terminal(&self) -> SessionOutcome {
        let mut guard = self.state.state.lock().expect("completion lock");
        loop {
            if let Some(outcome) = &guard.outcome {
                return outcome.clone();
            }
            guard = self
                .state
                .signal
                .wait(guard)
                .expect("completion wait poisoned");
        }
    }

    fn publish(&self, evidence: impl FnOnce(&mut CompletionState)) {
        publish_evidence(&self.state, evidence);
    }
}

/// The one gate-evidence attribution rule (D14.7 + the D14.5 seek
/// additions): what each render-gate event does to the evidence latches.
/// Named so the delayed-delivery oracle below can drive the production
/// arm synchronously — the real observer above routes every event
/// through this function, so a test that calls it exercises exactly the
/// attribution the leg performs.
///
/// Attribution stays structurally separate per park kind: pause events
/// maintain the pause latches, seek events maintain the seek latches. A
/// cut-attributed seek park therefore can never satisfy the Paused
/// establishment (which reads only the pause pair), and a pause park can
/// never satisfy the seek commit (which reads only the paired seek — or
/// paired pause — engagement+quiescence conjunction).
fn apply_gate_event(state: &mut CompletionState, event: GateEvent) {
    match event {
        // Engagement is the current-engagement fence: events on one
        // render leg are ordered, so a prior engagement's Disengaged
        // happens-before this Engaged. Whatever disengagement evidence
        // may be latched here belongs to an engagement that has already
        // ended — it must not be misattributed to the current
        // engagement (D14.7 corrective-2). Engagement also resets the
        // tail evidence: quiescence belongs to the current engagement
        // only.
        GateEvent::Engaged => {
            state.engaged = true;
            state.tail_quiesced = false;
            state.disengagement_observed = false;
        }
        GateEvent::TailQuiesced => {
            state.tail_quiesced = true;
        }
        GateEvent::Disengaged => {
            state.engaged = false;
            state.tail_quiesced = false;
            state.disengagement_observed = true;
        }
        // The seek park's evidence pair: same fence discipline as the
        // pause pair, attributed to the cut protocol only (D14.5).
        GateEvent::SeekEngaged => {
            state.seek_engaged = true;
            state.seek_tail_quiesced = false;
        }
        GateEvent::SeekTailQuiesced => {
            state.seek_tail_quiesced = true;
        }
        GateEvent::SeekDisengaged => {
            state.seek_engaged = false;
            state.seek_tail_quiesced = false;
        }
    }
}

/// The one serialization boundary (D14.3): publish evidence, evaluate
/// the terminal contract, and commit — all inside a single hold of the
/// completion lock, the same lock `request_stop` serializes through.
/// Stop intent is therefore read at the publication boundary itself, and
/// no publication path can return before an already-decisive
/// classification is committed. Idempotent; first-wins; notifies waiters
/// only on the unsettled→committed transition. Only session-owned
/// execution paths call this — the worker wrapper, the decode failure
/// reporter, and the drain observer. No consumer call reaches it.
fn publish_evidence(core: &CompletionArc, evidence: impl FnOnce(&mut CompletionState)) {
    let mut guard = core.state.lock().expect("completion lock");
    if guard.outcome.is_some() {
        return; // settled: first-wins, later evidence cannot relabel
    }
    evidence(&mut guard);
    if let Some(outcome) = resolve(&guard) {
        guard.outcome = Some(outcome);
        drop(guard);
        core.signal.notify_all();
    }
}

/// The terminal contract, pure function of the lock-protected evidence
/// record: decode failure first, then drain verdict × worker terminal,
/// with recorded stop intent disambiguating an aborted drain. Called
/// only from [`publish_evidence`], so `outcome` is still `None` here.
fn resolve(state: &CompletionState) -> Option<SessionOutcome> {
    // Decode failure is authoritative over everything downstream: the
    // failure was published before the edge was failed.
    if let Some(message) = &state.decode_failure {
        return Some(SessionOutcome::Failed {
            stage: format!("decode: {message}"),
        });
    }
    if state.worker_terminal == Some(EdgeTerminal::Failed) {
        return Some(SessionOutcome::Failed {
            stage: "decode".to_owned(),
        });
    }
    match state.drain_verdict {
        Some(DrainVerdict::Drained) => {
            if state.worker_terminal == Some(EdgeTerminal::Eof) {
                return Some(SessionOutcome::Completed);
            }
        }
        Some(DrainVerdict::Aborted) => {
            match state.worker_terminal {
                // Until the worker has exited, the outcome is not
                // decidable: an aborted render alone happens on every
                // stop (the render leg is typically the first to observe
                // it) and on a real device failure. Keep waiting — every
                // abort path releases the data-plane stop, and that stop
                // wakes the worker, so this always terminates.
                None => {}
                Some(EdgeTerminal::Stopped) => {
                    // The worker terminal alone cannot distinguish "the
                    // user stopped us" from "the render leg died and
                    // stopped the data plane on its way out": both land
                    // here with the identical Stopped terminal. Recorded
                    // stop intent is the discriminator — request_stop
                    // publishes intent through the same lock before it
                    // releases the edge, so a stop linearized before
                    // this decisive publication necessarily observes it,
                    // and a stop after it cannot relabel (the outcome
                    // is already committed).
                    if state.stop_requested {
                        return Some(SessionOutcome::Stopped);
                    }
                    return Some(SessionOutcome::Failed {
                        stage: "device".to_owned(),
                    });
                }
                Some(_) => {
                    return Some(SessionOutcome::Failed {
                        stage: "device".to_owned(),
                    });
                }
            }
        }
        None => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(loom))]
    use qianqian_audio_api::ports::SeekParkOutcome;

    /// D14.7 corrective-2, the delayed-delivery interleaving no leg-level
    /// test can reach deterministically: the render leg has observed
    /// resume #1's release, but its Disengaged #1 is still in flight when
    /// pause #2 routes. The stale event publishes into the new cycle —
    /// only the leg's own Engaged #2 (the current-engagement fence) may
    /// clear it, so the internal disengagement latch stays attributable
    /// to the CURRENT engagement.
    ///
    /// This is a MECHANISM attribution oracle, not a product-projection
    /// test: the D14.7 AUTHORITY-CORRECTIVE removed the public `Resumed`
    /// projection this latch once grounded (disengagement evidence
    /// cannot prove a viable render leg remains). The latch stays
    /// crate-internal, and its per-engagement attribution discipline
    /// stays pinned here because the same latch discipline keeps the
    /// `PauseEngagement` spelling honest across cycles.
    ///
    /// Drives the production attribution arm (`apply_gate_event`, exactly
    /// what the real gate observer runs) through the real publication
    /// boundary, so deleting the Engaged-arm reset turns this RED while
    /// every event shape stays real. The ordering constructed here is a
    /// legal interleaving: event delivery from the leg is asynchronous
    /// with command routing, bounded only by the park slice.
    #[test]
    fn a_prior_cycle_disengagement_never_answers_a_later_cycle_after_reengagement() {
        let completion = SessionCompletion::new();
        let core = completion.state.clone();
        let latched = || {
            core.state
                .lock()
                .expect("completion lock")
                .disengagement_observed
        };

        // Cycle 1: pause → engagement. (request_pause routes to the
        // episode's gate; the events below stand in for the leg's
        // acknowledgments.)
        completion.request_pause();
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Engaged));
        assert!(completion.observe_snapshot().pause_requested);

        // Resume #1 releases intent; the leg is about to deliver its
        // disengagement.
        completion.request_resume();

        // Pause #2 routes BEFORE that delivery lands — the leg has
        // passed its released check but not yet published.
        completion.request_pause();

        // ...and the PRIOR cycle's disengagement publishes into it.
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Disengaged));
        assert!(
            latched(),
            "precondition: the delayed prior-cycle event did land"
        );

        // The leg re-engages for cycle 2 — the fence must clear the
        // stale evidence.
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Engaged));
        assert!(
            !latched(),
            "a prior cycle's Disengaged must not survive the current engagement"
        );

        // Resume #2: before the CURRENT engagement disengages, there is
        // no current-engagement disengagement evidence.
        completion.request_resume();
        assert!(
            !latched(),
            "the current engagement's disengagement cannot be attributed before it happens"
        );

        // The current engagement disengages — NOW the evidence exists.
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Disengaged));
        assert!(latched());

        // None of this touched the terminal Fact: evidence latches are
        // not settlement inputs.
        assert_eq!(completion.observe_snapshot().terminal_outcome, None);
    }

    // --- F5 seek protocol, white-box (D14.5) --------------------------------
    //
    // The slot policy, the acceptance conditions, the commit conjunction
    // and the first-wins evidence latches are deliberately NOT product
    // surface (no public positive seek state), so their direct pins live
    // here inside the crate boundary. The end-to-end consequences are
    // pinned publicly by tests/seek_seam.rs. These tests consume the
    // crate-internal verification readers, which are excluded from loom
    // builds with their consumers.

    /// The one-seek policy at its storage: a request plants exactly one
    /// command and occupies the slot; the command is taken once; the
    /// slot stays occupied through pickup (it frees when the protocol
    /// RESOLVES, not when the worker picks the command up); a second
    /// request while occupied is inert; a resolved slot accepts again.
    #[cfg(not(loom))]
    #[test]
    fn the_seek_slot_plants_one_command_and_a_second_is_inert() {
        let completion = SessionCompletion::new();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));

        completion.request_seek(Duration::from_secs(5));
        assert!(
            completion.seek_in_flight(),
            "an accepted seek occupies the one-in-flight slot"
        );
        assert_eq!(completion.take_seek_command(), Some(Duration::from_secs(5)));
        assert_eq!(
            completion.take_seek_command(),
            None,
            "the command is taken exactly once"
        );
        assert!(
            completion.seek_in_flight(),
            "the slot stays occupied until the protocol resolves, not at \
             pickup — a second request must not slip in mid-protocol"
        );

        completion.request_seek(Duration::from_secs(7));
        assert_eq!(
            completion.take_seek_command(),
            None,
            "one seek in flight: the second request is inert (no queue, \
             no coalescing, no latest-wins)"
        );

        completion.clear_seek_in_flight();
        completion.request_seek(Duration::from_secs(7));
        assert_eq!(
            completion.take_seek_command(),
            Some(Duration::from_secs(7)),
            "a resolved seek frees the slot for a later one"
        );
    }

    /// Every frozen acceptance condition fails closed: no data plane,
    /// a non-Open edge (EOF drain window, failed edge), recorded stop
    /// intent, and a settled episode each leave the slot unoccupied —
    /// and a PAUSED episode is seekable (positive control: pause intent
    /// is not an acceptance condition).
    #[cfg(not(loom))]
    #[test]
    fn seek_acceptance_fails_closed_on_every_frozen_condition() {
        // Never activated: no data plane exists to cut.
        let completion = SessionCompletion::new();
        completion.request_seek(Duration::from_secs(1));
        assert!(!completion.seek_in_flight());

        // The post-EOF drain window: the edge terminal is no longer Open.
        let completion = SessionCompletion::new();
        let edge = Arc::new(PcmEdge::new(2, 8192));
        completion.bind_stop_target(edge.clone());
        edge.close_eof();
        completion.request_seek(Duration::from_secs(1));
        assert!(!completion.seek_in_flight());

        // A failed data plane is equally not seekable.
        let completion = SessionCompletion::new();
        let edge = Arc::new(PcmEdge::new(2, 8192));
        completion.bind_stop_target(edge.clone());
        edge.fail();
        completion.request_seek(Duration::from_secs(1));
        assert!(!completion.seek_in_flight());

        // Recorded stop intent.
        let completion = SessionCompletion::new();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        completion.request_stop();
        completion.request_seek(Duration::from_secs(1));
        assert!(!completion.seek_in_flight());

        // A settled episode: decode failure settles synchronously, and
        // the terminal Fact freezes the command surface.
        let completion = SessionCompletion::new();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        completion.decode_failed("test failure");
        assert_eq!(
            completion.observe_snapshot().terminal_outcome,
            Some(EpisodeTerminalOutcome::Failed)
        );
        completion.request_seek(Duration::from_secs(1));
        assert!(!completion.seek_in_flight());

        // Positive control: a paused episode is seekable.
        let completion = SessionCompletion::new();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        completion.request_pause();
        completion.request_seek(Duration::from_secs(1));
        assert!(
            completion.seek_in_flight(),
            "a paused episode is seekable — pause intent is not an \
             acceptance condition"
        );
    }

    /// The commit boundary is a conjunction, evaluated atomically:
    /// (landing published) ∧ (leg parked ∧ THAT engagement's tail
    /// quiesced, under either attribution) ∧ episode unsettled. Each
    /// missing conjunct routes an ABORT release (no wedged park); the
    /// full conjunction commits and routes the landing payload exactly
    /// once; an UNKNOWN landing commits too — stale exclusion is
    /// independent of landing knowledge — and stop intent recorded
    /// before the decision wins it.
    #[cfg(not(loom))]
    #[test]
    fn the_commit_boundary_requires_its_full_conjunction() {
        // Parked + quiesced, but the landing was never published.
        let completion = SessionCompletion::new();
        let core = completion.state.clone();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Engaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::TailQuiesced));
        assert!(!completion.commit_seek_cutover(Some(123)));
        assert_eq!(
            completion.render_gate().park_while_seek_hold(|| false),
            SeekParkOutcome::Released(SeekParkRelease::Aborted),
            "the losing decision still releases the leg"
        );

        // The park half must actually BE there: landing published and
        // the episode unsettled, but no engagement ever published its
        // quiescence — no commit (the commit boundary is not reachable
        // by landing knowledge alone).
        let completion = SessionCompletion::new();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        completion.seek_landing_published(Some(7));
        assert!(!completion.commit_seek_cutover(Some(7)));
        assert_eq!(
            completion.render_gate().park_while_seek_hold(|| false),
            SeekParkOutcome::Released(SeekParkRelease::Aborted),
        );

        // Stop intent recorded before the decision wins the race.
        let completion = SessionCompletion::new();
        let core = completion.state.clone();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        completion.request_stop();
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Engaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::TailQuiesced));
        completion.seek_landing_published(Some(123));
        assert!(!completion.commit_seek_cutover(Some(123)));
        assert_eq!(
            completion.render_gate().park_while_seek_hold(|| false),
            SeekParkOutcome::Released(SeekParkRelease::Aborted),
        );

        // The full conjunction commits; the payload reaches the leg
        // exactly once and the slot stays occupied through consumption.
        let completion = SessionCompletion::new();
        let core = completion.state.clone();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        // Plant the seek like a real acceptance would, so the one-seek
        // slot is genuinely occupied when the commit routes.
        completion.request_seek(Duration::from_secs(1));
        assert!(completion.seek_in_flight());
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Engaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::TailQuiesced));
        completion.seek_landing_published(Some(123));
        assert!(completion.commit_seek_cutover(Some(123)));
        let (refused, committed, landing) = completion.seek_protocol_state();
        assert!(!refused);
        assert!(committed);
        assert_eq!(landing, Some(Some(123)));
        assert!(
            completion.seek_in_flight(),
            "the slot stays occupied through the commit: the cut is not \
             resolved until the leg has CONSUMED the routed release (a \
             later seek's hold would wipe an unconsumed `Committed` and \
             lose the rebase)"
        );
        assert!(
            completion.seek_release_pending(),
            "the routed payload awaits the leg's consumption"
        );
        assert_eq!(
            completion.render_gate().park_while_seek_hold(|| false),
            SeekParkOutcome::Released(SeekParkRelease::Committed {
                landing: Some(123)
            }),
            "the commit's rebase payload carries the ACTUAL landing"
        );
        assert_eq!(
            completion.render_gate().park_while_seek_hold(|| false),
            SeekParkOutcome::NotParked,
            "the payload is consumed exactly once"
        );
        // The worker's post-consumption duty (session.rs): once the
        // release is consumed the resolved cut frees the slot for a
        // later seek.
        completion.clear_seek_in_flight();
        assert!(!completion.seek_in_flight());
        assert!(!completion.seek_release_pending());

        // An UNKNOWN landing commits too: the payload carries None, and
        // the projection's withdrawal is the leg's own discipline.
        let completion = SessionCompletion::new();
        let core = completion.state.clone();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Engaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::TailQuiesced));
        completion.seek_landing_published(None);
        assert!(completion.commit_seek_cutover(None));
        assert_eq!(
            completion.render_gate().park_while_seek_hold(|| false),
            SeekParkOutcome::Released(SeekParkRelease::Committed { landing: None }),
        );

        // The same conjunction holds under the SEEK attribution: the
        // cut's own park (SeekEngaged + ITS quiescence) satisfies the
        // boundary with no pause engagement at all.
        let completion = SessionCompletion::new();
        let core = completion.state.clone();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::SeekEngaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::SeekTailQuiesced));
        completion.seek_landing_published(Some(55));
        assert!(
            completion.commit_seek_cutover(Some(55)),
            "the cut-attributed park is the same evidence class"
        );
    }

    /// The refusal and landing latches are first-wins evidence: the
    /// first published value is the episode's truth, later publications
    /// are inert history — including "unknown" once published.
    #[cfg(not(loom))]
    #[test]
    fn the_refusal_and_landing_latches_are_first_wins_evidence() {
        let completion = SessionCompletion::new();
        completion.seek_refused();
        completion.seek_landing_published(Some(1));
        completion.seek_landing_published(Some(2));
        let (refused, committed, landing) = completion.seek_protocol_state();
        assert!(refused, "the inert outcome class was recorded");
        assert!(!committed, "a refusal never commits a cut");
        assert_eq!(landing, Some(Some(1)), "the first landing wins");

        let completion = SessionCompletion::new();
        completion.seek_landing_published(None);
        completion.seek_landing_published(Some(9));
        assert_eq!(
            completion.seek_protocol_state().2,
            Some(None),
            "unknown, once published, is the landing — never upgraded"
        );
    }
}
