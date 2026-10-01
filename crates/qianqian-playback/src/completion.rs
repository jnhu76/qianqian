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
    DrainSignal, DrainVerdict, GateEvent, OutputLevel, PcmFormat, PositionEvidence, RenderGate,
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
    /// An AUDIO PROCESSING failure published by the decode worker
    /// (ADR-PBK-002 D14.11): the same D11 `Failed` terminal class as
    /// `decode_failure`, with its own evidence slot so the internal
    /// diagnosis stays truthful about the failure's origin — a
    /// processing failure must not masquerade as a decode failure merely
    /// because the current execution placement shares the decode worker.
    /// The stage spelling of the settled diagnostic ("processing: ...")
    /// carries the distinction; the terminal vocabulary does not.
    /// I0 scope: only the disposable Gain probe can produce a processing
    /// failure, so the whole route is compiled in this crate's test
    /// build only; production Gain (Issue #177 I1) earns the production
    /// route with its first real consumer.
    #[cfg(all(test, not(loom)))]
    processing_failure: Option<String>,
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
    /// Worker-liveness evidence (D14.5 implementation corrective-1):
    /// set by [`SessionCompletion::worker_exited`] — the single exit
    /// funnel — under the completion lock, BEFORE the worker's stranded-
    /// seek cleanup runs. It is the acceptance side of the seek/worker-
    /// exit linearization: an accepted seek must either be resolved by a
    /// live worker or aborted by that worker's exit cleanup; a request
    /// that passes acceptance after `worker_gone` is set would have no
    /// resolver left, so acceptance rejects it. Never public surface.
    worker_gone: bool,
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
    /// Belongs to the CURRENT cut cycle — reset when the next seek is
    /// accepted (implementation corrective-1: a previous cut's landing
    /// must never satisfy a later commit boundary, the same
    /// current-engagement attribution discipline D14.7 freezes for
    /// pause). Mechanism evidence; never a Fact, never product surface.
    seek_landing: Option<Option<u64>>,
    /// A [`SeekSlot`] command that the worker picked up and resolved as
    /// a proven pre-mutation refusal (`RefusedUnchanged`): inert
    /// protocol history of the CURRENT cut cycle, reset at the next
    /// acceptance like the other cut latches.
    seek_refused: bool,
    /// The session's cutover-commit record (D14.5): true iff the
    /// session-owned protocol path evaluated `landing published ∧ edge
    /// invalidated ∧ tail quiesced ∧ leg parked ∧ episode unsettled`
    /// and recorded the commit. Belongs to the CURRENT cut cycle (reset
    /// at the next acceptance). Protocol state owned by the session —
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
    /// The episode's output-level cell (ADR-PBK-002 D14.9), created
    /// here and handed to the same render stream through its open
    /// request — the same ownership posture as the position cell: the
    /// mechanism applies it on its own path with relaxed atomic reads,
    /// so the cell never shares a lock with the settlement state. The
    /// application routes into it ONLY through the episode seam's
    /// idempotent `request_output_level` command; it is application
    /// configuration in transit, never a Fact and never settlement
    /// input.
    output_level: OutputLevel,
}

impl Default for SessionCompletion {
    fn default() -> Self {
        Self::new()
    }
}

/// What ONE [`SessionCompletion::seek_cutover_decision`] sample concluded
/// about an applied cut (ADR-PBK-002 D14.5). Three-valued on purpose:
/// "the commit boundary does not hold yet" and "the episode is ending"
/// are different statements about different things, and only the second
/// one may release a purged cut's render leg without its rebase.
/// Crate-internal protocol vocabulary — never a Fact, never a terminal
/// variant, never public surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CutoverDecision {
    /// The commit boundary held; the commit is recorded and the rebase
    /// release routed to the render leg.
    Committed,
    /// An episode ending is recorded (stop intent, a committed terminal
    /// Fact, or the teardown release); the release is routed as an abort.
    Aborted,
    /// The boundary is not satisfiable yet — current park/quiescence
    /// evidence is missing. Nothing recorded, nothing routed.
    Pending,
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
                        #[cfg(all(test, not(loom)))]
                        processing_failure: None,
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
                        worker_gone: false,
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
                    output_level: OutputLevel::new(),
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

    /// The decode worker reports an AUDIO PROCESSING failure
    /// (ADR-PBK-002 D14.11): the same D11 `Failed` terminal class, via
    /// its own publication so the internal diagnosis stays truthful
    /// about the processing origin instead of borrowing the decode
    /// label. First failure wins against the decode slot — worker-leg
    /// failure publications are sequential on the one worker thread, so
    /// the arbitration is defensive only. No bypass, no partial result:
    /// a failed processor's output is not trustworthy.
    ///
    /// I0 scope: compiled in this crate's test build only — the
    /// disposable Gain probe is the only publisher; production Gain
    /// (Issue #177 I1) earns the production route.
    #[cfg(all(test, not(loom)))]
    pub(crate) fn processing_failed(&self, message: &str) {
        self.publish(|state| {
            if state.decode_failure.is_none() && state.processing_failure.is_none() {
                state.processing_failure = Some(message.to_owned());
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
    /// the decode worker is still alive (an accepted seek needs a live
    ///     resolver — implementation corrective-1)
    /// no seek already in flight (one-seek policy; no queueing)
    /// ```
    ///
    /// `target` is source-relative media time, non-negative by type.
    /// Beyond-duration targets pass through: the PROVIDER decides
    /// validity and clamping (duration evidence is never consulted
    /// here). Acceptance records the command and parks the leg; it does
    /// NOT imply a cutover — "a seek request is not a cutover".
    ///
    /// The acceptance is one atomic unit (implementation corrective-1,
    /// the seek/worker-exit linearization): the second hold below
    /// re-validates every condition, plants the command, resets the
    /// current cut cycle's evidence, and routes the cut's park — all
    /// under ONE completion-lock hold. A seek is therefore either wholly
    /// accepted before the worker's exit publication (whose stranded-
    /// seek cleanup runs after it and aborts exactly this plant) or
    /// wholly rejected after it; no interleaving can leave a planted
    /// command whose only resolver has left. Without this, a
    /// `request_seek` × worker-EOF interleaving could route a hold
    /// nobody releases and wedge the episode's final drain — D11
    /// Completed would never settle.
    pub(crate) fn request_seek(&self, target: Duration) {
        // First hold: cheap reject against the command state and worker
        // liveness. The edge is reached only after this lock is dropped
        // (the established completion→edge discipline: `request_stop`'s
        // pattern). A terminal that changes in the window between the
        // holds is caught again by the second hold's re-validation and
        // at the worker's own serialization point — defense lines, not
        // one.
        let edge = {
            let guard = self.state.state.lock().expect("completion lock");
            if guard.outcome.is_some()
                || guard.stop_requested
                || guard.teardown_released
                || guard.activation_failure.is_some()
                || guard.worker_gone
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
        // The atomic acceptance unit (see the doc above). Lock order:
        // completion state → seek slot → gate intent, each nested only
        // in that direction (the routers' established
        // gate-intent-under-completion-lock discipline; the slot is
        // nested here for the first time — nothing ever takes the state
        // lock while holding the slot, so no cycle is reachable).
        {
            let mut guard = self.state.state.lock().expect("completion lock");
            if guard.outcome.is_some()
                || guard.stop_requested
                || guard.teardown_released
                || guard.activation_failure.is_some()
                || guard.worker_gone
            {
                return;
            }
            {
                // Plant under the slot lock: the one-seek policy is
                // decided here, atomically with the planting. No
                // queueing, no coalescing, no request identity — this
                // is why no SeekId exists.
                let mut slot = self.state.seek_slot.lock().expect("seek slot lock");
                if slot.in_flight {
                    return; // one seek in flight; the second request is inert
                }
                slot.in_flight = true;
                slot.command = Some(target);
            }
            // A NEW cut cycle owns fresh evidence: the previous cycle's
            // landing/refusal/commit latches must never satisfy THIS
            // cycle's commit boundary (the same current-engagement
            // attribution discipline D14.7 freezes for pause). Safe
            // against the worker's in-flight protocol: the one-seek slot
            // only frees after the previous protocol fully resolved,
            // and the worker's next-cycle publications serialize after
            // this hold through this same lock.
            guard.seek_landing = None;
            guard.seek_refused = false;
            guard.cut_committed = false;
            // The cut's park routes INSIDE this hold, so it can never
            // lag the plant: the worker-exit cleanup always finds and
            // releases exactly what an acceptance routed.
            self.state.gate.set_seek_hold(true);
        }
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
    /// the session-recorded episode endings of
    /// [`episode_ending_evidence`] — stop intent, a committed terminal
    /// Fact, or the authority-owned teardown release. Every seek step
    /// re-checks them, so stop always wins. The frozen failure policy's
    /// third episode-ending class — "data plane not Open" (edge terminal
    /// != Open) — is deliberately NOT here: it is the worker's own read,
    /// taken on its own path without reaching the data plane under this
    /// lock, in the same class as the recorded endings above. The
    /// applied cut's wait does not call this directly — it reads the
    /// same evidence through [`SessionCompletion::seek_cutover_decision`],
    /// which samples it atomically with the commit boundary — but the
    /// predicate is identical by construction.
    pub(crate) fn seek_aborted(&self) -> bool {
        let guard = self.state.state.lock().expect("completion lock");
        episode_ending_evidence(&guard)
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

    /// The session's cutover disposition for one APPLIED cut (D14.5
    /// commit boundary), sampled — together with the episode-ending
    /// latches — in ONE completion-lock hold: the same lock stop intent
    /// and every terminal publication linearize through, so one call is
    /// one real-instant view of the whole decision. There is no second
    /// sample for a transient evidence gap to land between.
    ///
    /// ```text
    /// Committed   the boundary held: landing published ∧ leg parked ∧
    ///             THAT park's tail quiesced ∧ episode unsettled (the
    ///             "edge invalidated" conjunct is the caller's program
    ///             order and already true by the time it asks). The
    ///             commit is recorded and the rebase release routed.
    /// Aborted     an episode ending is recorded — stop intent, a
    ///             committed terminal Fact, or the authority-owned
    ///             teardown release. No commit, no rebase, no partial
    ///             state; the release is routed as an abort.
    /// Pending     the boundary is not satisfiable YET. NOT an abort:
    ///             missing current park/quiescence evidence is a
    ///             statement about the evidence, never about the
    ///             episode. Nothing is recorded and nothing is routed;
    ///             the caller keeps waiting.
    /// ```
    ///
    /// The atomicity and the three-valuedness are one corrective
    /// (implementation corrective-3). The evidence a commit reads is
    /// latched per park, and the leg's pause→cut park handover publishes
    /// Disengaged-then-SeekEngaged; a two-sample spelling (wait on one
    /// read, decide on another) could therefore observe the conjunction
    /// and then a gap, and classifying that gap as an abort would
    /// release the leg with NO rebase strictly after the provider
    /// applied and the edge was purged — silently restoring the pre-cut
    /// position accounting through an evidence artifact. The only exits
    /// from an applied cut are `Committed` or an episode ending.
    ///
    /// Preconditions the CALLER owns (worker program order, not lock
    /// state): the provider succeeded, the staging was discarded, and
    /// `edge.invalidate()` has already returned on the worker's path.
    /// The one episode-ending condition this cannot read without
    /// reaching the data plane — "data plane not Open" (the frozen
    /// failure policy's own class: edge terminal != Open, which includes
    /// the post-EOF drain window and a device abort's stop) — is the
    /// caller's: it tests the edge on its own path, outside this lock,
    /// and takes the abort route there. No new terminal evidence is
    /// published on that route: the data plane's owner settles the
    /// episode through the existing D11 precedence.
    ///
    /// The one-seek slot is NOT freed here, on the Committed branch: a
    /// committed release is part of the cut until the LEG has consumed
    /// it (a later hold would wipe an unconsumed `Committed` and lose
    /// the rebase — the seek matrices caught that exact pause-shaped
    /// interleaving), so the worker keeps the slot occupied until it
    /// observes `seek_release_pending() == false` and then frees it. On
    /// the abort branch no free is needed at all: every abort condition
    /// (stop intent, settled, teardown release) implies the episode is
    /// ending, so the slot is never consulted again.
    pub(crate) fn seek_cutover_decision(&self, landing: Option<u64>) -> CutoverDecision {
        let decision = {
            let mut guard = self.state.state.lock().expect("completion lock");
            let episode_ending = episode_ending_evidence(&guard);
            let parked_and_quiesced = (guard.engaged && guard.tail_quiesced)
                || (guard.seek_engaged && guard.seek_tail_quiesced);
            if episode_ending {
                CutoverDecision::Aborted
            } else if guard.seek_landing.is_some() && parked_and_quiesced {
                guard.cut_committed = true;
                CutoverDecision::Committed
            } else {
                CutoverDecision::Pending
            }
        };
        match decision {
            CutoverDecision::Committed => self
                .state
                .gate
                .release_seek_hold(SeekParkRelease::Committed { landing }),
            CutoverDecision::Aborted => self.state.gate.release_seek_hold(SeekParkRelease::Aborted),
            CutoverDecision::Pending => {}
        }
        decision
    }

    /// Whether the current cut's routed release still awaits the leg's
    /// consumption. The worker polls this after a commit and frees the
    /// one-seek slot only when it clears — see
    /// [`SessionCompletion::seek_cutover_decision`].
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
        self.state
            .seek_slot
            .lock()
            .expect("seek slot lock")
            .in_flight
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

    /// The episode's output-level cell, handed to the output provider
    /// at activation (D14.9). Session-internal binding seam: the
    /// application routes into the cell only through the episode seam's
    /// idempotent command, and never reads it back (D14.9 forbids a
    /// mechanism readback on the product read side).
    pub(crate) fn output_level(&self) -> OutputLevel {
        self.state.output_level.clone()
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
    /// This is the worker's single exit funnel (normal and panic paths):
    /// it publishes the terminal evidence AND — first, under the same
    /// lock hold — marks the worker gone, which is the acceptance side
    /// of the seek/worker-exit linearization. The caller MUST run
    /// [`SessionCompletion::abort_stranded_seek`] after this returns.
    /// The publication and the settlement step run under one lock hold,
    /// inside this call.
    pub(crate) fn worker_exited(&self, terminal: EdgeTerminal) {
        self.publish(|state| {
            state.worker_terminal = Some(terminal);
            state.worker_gone = true;
        });
    }

    /// The decode worker's exit duty (implementation corrective-1): a
    /// seek accepted before this worker's exit can never be resolved by
    /// a worker that is leaving — abort it here, strictly AFTER
    /// [`SessionCompletion::worker_exited`] published `worker_gone`.
    /// Acceptance linearizes against that publication: a plant whose
    /// acceptance hold ran before it is found and cleared here; a plant
    /// attempted after it is rejected by the acceptance re-validation.
    /// Every exit path reaches this — including a refusal whose
    /// preserved remainder was cut short by a stop (slot still occupied)
    /// — and re-routing an abort is idempotent: the episode is ending,
    /// the leg consumes-or-ignores the payload, and the data plane
    /// decides.
    pub(crate) fn abort_stranded_seek(&self) {
        let stranded = {
            let mut slot = self.state.seek_slot.lock().expect("seek slot lock");
            if slot.in_flight || slot.command.is_some() {
                slot.in_flight = false;
                slot.command = None;
                true
            } else {
                false
            }
        };
        if stranded {
            // No commit, no rebase, no partial state: the protocol's
            // only resolver is gone.
            self.state.gate.release_seek_hold(SeekParkRelease::Aborted);
        }
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

/// The episode endings the SESSION records — the frozen failure policy's
/// pre-cut abort classes minus the worker-read "data plane not Open"
/// (which cannot be read here without reaching the data plane under this
/// lock), and the seek protocol's only abort conditions. ONE spelling:
/// [`SessionCompletion::seek_aborted`] and the cutover decision must
/// agree exactly — a cut's wait re-checks the endings while the decision
/// samples them with the commit boundary, so a drift between the two
/// would be a liveness or a lost-rebase defect, not a nuance.
fn episode_ending_evidence(state: &CompletionState) -> bool {
    state.stop_requested || state.outcome.is_some() || state.teardown_released
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
    // Worker-side failure is authoritative over everything downstream:
    // the failure was published before the edge was failed. The stage
    // spelling keeps the origin truthful (D14.11): a decode failure and
    // (where the processing route is compiled, see CompletionState) an
    // audio-processing failure settle the same `Failed` terminal class
    // through their own evidence slots.
    if let Some(message) = &state.decode_failure {
        return Some(SessionOutcome::Failed {
            stage: format!("decode: {message}"),
        });
    }
    #[cfg(all(test, not(loom)))]
    if let Some(message) = &state.processing_failure {
        return Some(SessionOutcome::Failed {
            stage: format!("processing: {message}"),
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
    use qianqian_audio_api::ports::{GateSlice, RenderGate, TailProbeOutcome};

    /// Drive the unified loop-top gate and capture any routed release
    /// payload the leg consumes. The tail probe never quiesces, so a
    /// routed hold would park this call — tests that use this helper
    /// drive consumption shapes only (payload routed before arrival, or
    /// nothing routed).
    #[cfg(not(loom))]
    fn consume_release(gate: &RenderGate) -> Option<SeekParkRelease> {
        let captured = std::cell::Cell::new(None);
        gate.park_loop_top(|slice| match slice {
            GateSlice::TailProbe => TailProbeOutcome::Pending,
            GateSlice::SeekRelease(release) => {
                captured.set(Some(release));
                TailProbeOutcome::Pending
            }
        });
        captured.into_inner()
    }

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

    /// The commit boundary is a conjunction, evaluated in ONE atomic
    /// sample: (landing published) ∧ (leg parked ∧ THAT engagement's
    /// tail quiesced, under either attribution) ∧ episode unsettled.
    /// A missing conjunct is PENDING — nothing recorded, NOTHING
    /// routed, the protocol keeps waiting (releasing the leg there is
    /// exactly how an applied cut would lose its rebase). A recorded
    /// episode ending is the only abort, and it routes the abort
    /// release; the full conjunction commits and routes the landing
    /// payload exactly once; an UNKNOWN landing commits too — stale
    /// exclusion is independent of landing knowledge.
    #[cfg(not(loom))]
    #[test]
    fn the_commit_boundary_requires_its_full_conjunction() {
        // Parked + quiesced, but the landing was never published.
        let completion = SessionCompletion::new();
        let core = completion.state.clone();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Engaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::TailQuiesced));
        assert_eq!(
            completion.seek_cutover_decision(Some(123)),
            CutoverDecision::Pending
        );
        assert!(
            !completion.seek_protocol_state().1,
            "a pending boundary records no commit"
        );
        assert_eq!(
            consume_release(&completion.render_gate()),
            None,
            "a pending boundary routes NOTHING: the leg stays parked \
             rather than resuming without its rebase"
        );

        // The park half must actually BE there: landing published and
        // the episode unsettled, but no engagement ever published its
        // quiescence — no commit (the commit boundary is not reachable
        // by landing knowledge alone).
        let completion = SessionCompletion::new();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        completion.seek_landing_published(Some(7));
        assert_eq!(
            completion.seek_cutover_decision(Some(7)),
            CutoverDecision::Pending
        );
        assert_eq!(
            consume_release(&completion.render_gate()),
            None,
            "no park evidence, no release — the boundary is not \
             satisfiable by landing knowledge alone"
        );

        // Stop intent recorded before the decision wins the race: the
        // one abort, and it releases the leg.
        let completion = SessionCompletion::new();
        let core = completion.state.clone();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        completion.request_stop();
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Engaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::TailQuiesced));
        completion.seek_landing_published(Some(123));
        assert_eq!(
            completion.seek_cutover_decision(Some(123)),
            CutoverDecision::Aborted
        );
        assert_eq!(
            consume_release(&completion.render_gate()),
            Some(SeekParkRelease::Aborted),
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
        assert_eq!(
            completion.seek_cutover_decision(Some(123)),
            CutoverDecision::Committed
        );
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
            consume_release(&completion.render_gate()),
            Some(SeekParkRelease::Committed { landing: Some(123) }),
            "the commit's rebase payload carries the ACTUAL landing"
        );
        assert_eq!(
            consume_release(&completion.render_gate()),
            None,
            "the payload is consumed exactly once"
        );
        // The worker's post-consumption duty (session.rs): once the
        // release is consumed the resolved cut frees the slot for a
        // later seek.
        completion.clear_seek_in_flight();
        assert!(!completion.seek_in_flight());
        assert!(!completion.seek_release_pending());

        // The data plane ending under a committed cut (session.rs takes
        // this exit): freeing the slot leaves an unconsumed payload
        // ROUTED — the exiting leg still reads what the decision routed,
        // and no second release is manufactured over it. (The payload is
        // only ever routed by a decision; a slot free never touches it.)
        let completion = SessionCompletion::new();
        let core = completion.state.clone();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        completion.request_seek(Duration::from_secs(1));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::SeekEngaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::SeekTailQuiesced));
        completion.seek_landing_published(Some(9));
        assert_eq!(
            completion.seek_cutover_decision(Some(9)),
            CutoverDecision::Committed
        );
        completion.clear_seek_in_flight();
        assert!(
            completion.seek_release_pending(),
            "the routed rebase survives the slot free: the leg consumes, \
             it is not overwritten"
        );
        assert_eq!(
            consume_release(&completion.render_gate()),
            Some(SeekParkRelease::Committed { landing: Some(9) })
        );

        // An UNKNOWN landing commits too: the payload carries None, and
        // the projection's withdrawal is the leg's own discipline.
        let completion = SessionCompletion::new();
        let core = completion.state.clone();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Engaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::TailQuiesced));
        completion.seek_landing_published(None);
        assert_eq!(
            completion.seek_cutover_decision(None),
            CutoverDecision::Committed
        );
        assert_eq!(
            consume_release(&completion.render_gate()),
            Some(SeekParkRelease::Committed { landing: None }),
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
        assert_eq!(
            completion.seek_cutover_decision(Some(55)),
            CutoverDecision::Committed,
            "the cut-attributed park is the same evidence class"
        );
    }

    /// Implementation corrective-3 (the lost-rebase race). The commit
    /// boundary reads per-park latched evidence, and the render leg's
    /// pause→cut park handover publishes `Disengaged` (its pause park
    /// ended) before `SeekEngaged` (the hold parks it again) — so the
    /// conjunction is momentarily unsatisfiable IN the handover even
    /// though the cut is already applied and purged. A decision taken
    /// there must be PENDING, never an abort: an abort release resumes
    /// the leg with its pre-cut position accounting, i.e. reverts the
    /// applied cut through an evidence artifact. The handover shape is
    /// driven here step by step through the real attribution function.
    #[cfg(not(loom))]
    #[test]
    fn a_park_handover_evidence_gap_is_pending_and_never_an_abort() {
        let completion = SessionCompletion::new();
        let core = completion.state.clone();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        completion.request_seek(Duration::from_secs(1));
        // Paused episode: pause engagement + its quiescence preceded the
        // cut, and the cut's landing is already published (the provider
        // applied; the edge purge is the worker's program order).
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Engaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::TailQuiesced));
        completion.seek_landing_published(Some(41));
        assert_eq!(
            completion.seek_cutover_decision(Some(41)),
            CutoverDecision::Committed,
            "precondition: the boundary is satisfiable under the pause park"
        );
        let _ = consume_release(&completion.render_gate());
        completion.clear_seek_in_flight();

        // Cycle 2, this time sampled IN the handover: the pause park has
        // disengaged and the cut park has not engaged yet.
        completion.request_seek(Duration::from_secs(2));
        completion.seek_landing_published(Some(82));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Disengaged));
        assert_eq!(
            completion.seek_cutover_decision(Some(82)),
            CutoverDecision::Pending,
            "a missing park sample is a statement about the EVIDENCE, not \
             about the episode"
        );
        assert!(
            !completion.seek_release_pending(),
            "the leg must stay parked (the cut's hold is still routed, no \
             release payload exists): an abort release here would resume it \
             with the PRE-CUT position accounting while the provider is \
             already at the new landing"
        );
        assert!(
            !completion.seek_protocol_state().1,
            "no commit is recorded from a pending sample either"
        );

        // The cut park engages and quiesces: the SAME decision now
        // commits — the protocol was waiting, not failing.
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::SeekEngaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::SeekTailQuiesced));
        assert_eq!(
            completion.seek_cutover_decision(Some(82)),
            CutoverDecision::Committed
        );
        assert_eq!(
            consume_release(&completion.render_gate()),
            Some(SeekParkRelease::Committed { landing: Some(82) })
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

    /// Implementation corrective-1 (C3, current-cut attribution): the
    /// per-cut evidence latches belong to the CURRENT cut cycle. A
    /// second accepted seek resets them, so cycle 1's landing can never
    /// satisfy cycle 2's commit boundary — the commit predicate must be
    /// discharged by cycle 2's OWN landing publication, exactly the
    /// current-engagement discipline D14.7 freezes for pause. (Before
    /// the reset existed, `seek_landing` was episode-first-wins and a
    /// second commit could ride cycle 1's evidence.)
    #[cfg(not(loom))]
    #[test]
    fn a_second_accepted_seek_gets_fresh_cut_evidence() {
        let completion = SessionCompletion::new();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        let core = completion.state.clone();

        // Cycle 1 runs to a full commit: landing 10 published, commit,
        // payload consumed, slot freed.
        completion.request_seek(Duration::from_secs(1));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Engaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::TailQuiesced));
        completion.seek_landing_published(Some(10));
        assert_eq!(
            completion.seek_cutover_decision(Some(10)),
            CutoverDecision::Committed
        );
        assert_eq!(
            consume_release(&completion.render_gate()),
            Some(SeekParkRelease::Committed { landing: Some(10) })
        );
        completion.clear_seek_in_flight();
        assert_eq!(
            completion.seek_protocol_state(),
            (false, true, Some(Some(10))),
            "precondition: cycle 1's evidence is latched"
        );

        // Accepting cycle 2 resets the latches...
        completion.request_seek(Duration::from_secs(2));
        assert_eq!(
            completion.seek_protocol_state(),
            (false, false, None),
            "a new cut cycle must not inherit the previous cycle's \
             landing, refusal or commit evidence"
        );

        // ...so cycle 1's landing no longer satisfies the commit
        // boundary: cycle 2 is PENDING on its own evidence — not an
        // abort, and nothing is routed.
        assert_eq!(
            completion.seek_cutover_decision(Some(10)),
            CutoverDecision::Pending,
            "cycle 2's commit must require cycle 2's own landing evidence"
        );
        assert!(
            !completion.seek_release_pending(),
            "a pending cycle routes nothing"
        );

        // Publishing cycle 2's OWN landing is what discharges the
        // boundary.
        completion.seek_landing_published(Some(20));
        assert_eq!(
            completion.seek_cutover_decision(Some(20)),
            CutoverDecision::Committed
        );
        assert_eq!(
            consume_release(&completion.render_gate()),
            Some(SeekParkRelease::Committed { landing: Some(20) })
        );
    }

    /// Implementation corrective-1 (C2, seek/worker-exit
    /// linearization): an accepted seek either lives to be resolved by
    /// its worker or is aborted by that worker's exit — never stranded
    /// under a hold nobody will release (which would park the leg past
    /// the final drain and wedge D11 Completed). Both linearization
    /// sides, driven deterministically through the real acceptance and
    /// the real exit funnel:
    ///
    /// ```text
    /// (a) plant before the worker's exit publication
    ///     → the exit cleanup finds and aborts exactly that plant;
    /// (b) request after the worker's exit publication
    ///     → acceptance rejects (no live resolver), no plant, no hold.
    /// ```
    ///
    /// Every acceptance hold is atomic (plant + latch reset + hold
    /// routing under ONE completion-lock hold), so these two sides
    /// exhaust the interleavings: a partial acceptance cannot exist.
    #[cfg(not(loom))]
    #[test]
    fn an_accepted_seek_cannot_outlive_its_worker_exit() {
        // (a) The plant precedes the worker's exit.
        let completion = SessionCompletion::new();
        let edge = Arc::new(PcmEdge::new(2, 8192));
        completion.bind_stop_target(edge.clone());
        completion.request_seek(Duration::from_secs(3));
        assert!(completion.seek_in_flight(), "precondition: accepted");
        // The worker's real EOF exit shape, on the funnel's order:
        // worker_gone publication first, then the stranded-seek cleanup.
        edge.close_eof();
        completion.worker_exited(EdgeTerminal::Eof);
        completion.abort_stranded_seek();
        assert!(
            !completion.seek_in_flight(),
            "the exit cleanup freed the slot of the plant it found"
        );
        // The routed abort reaches the leg exactly once: no hold remains
        // to park it, no payload lingers.
        assert_eq!(
            consume_release(&completion.render_gate()),
            Some(SeekParkRelease::Aborted),
        );
        assert_eq!(consume_release(&completion.render_gate()), None);

        // (b) The request follows the worker's exit.
        let completion = SessionCompletion::new();
        let edge = Arc::new(PcmEdge::new(2, 8192));
        completion.bind_stop_target(edge.clone());
        edge.close_eof();
        completion.worker_exited(EdgeTerminal::Eof);
        completion.abort_stranded_seek();
        completion.request_seek(Duration::from_secs(3));
        assert!(
            !completion.seek_in_flight(),
            "acceptance after the worker's exit publication is rejected: \
             a seek with no live resolver must never plant"
        );
        assert_eq!(
            consume_release(&completion.render_gate()),
            None,
            "nothing was routed"
        );
    }
}
