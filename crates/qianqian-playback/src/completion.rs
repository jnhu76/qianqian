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
//!                             and evaluates D11 synchronously on the
//!                             render leg's call stack, committing only
//!                             if decisive, before complete returns
//! ```
//!
//! This realization uses no settlement watcher, resolver thread, or
//! asynchronous settlement gap (D14.3 requires authority-owned progress).
//! `request_stop` records
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
//! Outcome precedence, stated once here: a published worker-side failure
//! (decode or audio processing, first publication wins) dominates
//! everything (it is checked first and is not relabelled by stop intent);
//! otherwise the drain verdict plus the worker's exit terminal decide,
//! with recorded stop intent disambiguating an aborted drain between a
//! user stop and a device failure.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use qianqian_audio_api::ports::{
    DrainSignal, DrainVerdict, GateEvent, OutputLevel, PcmFormat, PositionEvidence, RenderGate,
    SeekParkRelease,
};

use crate::edge::{EdgeTerminal, PcmEdge};
use crate::handle::{EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionObservation};
use crate::live::ProcessingControl;

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

/// Which worker-side leg published the first unrecoverable failure
/// (ADR-PBK-002 D14.11: the internal diagnosis must stay truthful about
/// the failure's origin — a processing failure must not masquerade as a
/// decode failure merely because the current execution placement shares
/// the decode worker thread). Crate-internal diagnostic vocabulary:
/// never a semantic terminal variant, never public surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkerFailureOrigin {
    /// The decode endpoint/worker itself failed (D14.5's
    /// MutatedThenFailed route included).
    Decode,
    /// The AUDIO PROCESSING stage failed (D14.11).
    Processing,
}

/// One worker-side failure record: the FIRST unrecoverable failure
/// publication on the worker leg — origin and diagnostic together.
/// First-wins is the whole arbitration contract (the worker leg's
/// failure publications are sequential on the one worker thread, so the
/// guard is defensive only): the first publication determines the
/// diagnostic origin, and later worker-side failure publications cannot
/// alter it. The public terminal stays the single D11 `Failed` class;
/// only the stage spelling of the presentation diagnostic carries the
/// origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkerFailure {
    origin: WorkerFailureOrigin,
    diagnostic: String,
}

impl WorkerFailure {
    /// The failure stage of the settled diagnostic — the truth-class
    /// boundary presentation form (D14.2): `decode: …` or `processing:
    /// …`, never a new public variant.
    fn stage(&self) -> String {
        match self.origin {
            WorkerFailureOrigin::Decode => format!("decode: {}", self.diagnostic),
            WorkerFailureOrigin::Processing => format!("processing: {}", self.diagnostic),
        }
    }
}

struct CompletionState {
    outcome: Option<SessionOutcome>,
    /// Terminal the decode worker observed on the edge at its exit.
    worker_terminal: Option<EdgeTerminal>,
    /// The first worker-side unrecoverable failure (decode or audio
    /// processing), if any. See [`WorkerFailure`].
    worker_failure: Option<WorkerFailure>,
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
    /// holding the state lock. Its storage is separate and its writer
    /// never takes the settlement lock; cache-line separation is not
    /// guaranteed by this representation.
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
    /// Product-control's Audio Processing state (campaign #190 D4): the
    /// desired whole configuration, the depth-1 latest-wins pending
    /// slot, and the last refusal diagnostic. Created with the
    /// completion so the application seam can route typed `set_*`
    /// commands from the moment a handle exists; the decode worker
    /// holds the same `Arc` and picks pending updates up once per whole
    /// staging block at the fresh-block boundary. Command state in
    /// transit — never a Fact, never settlement input, never read on
    /// the per-block PCM path.
    processing: Arc<ProcessingControl>,
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
                        worker_failure: None,
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
                    // The establishment default is the transparent
                    // BYPASS configuration, exactly the D14.11 default
                    // of `playback_session_spec`; the spec constructors
                    // bind the application's desired configuration
                    // through `processing().establish(..)`.
                    processing: Arc::new(ProcessingControl::new(
                        crate::processing::AudioProcessingConfig::BYPASS,
                    )),
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
    /// render leg's first `complete` publishes and evaluates D11 synchronously
    /// on that leg's call stack. It commits only when the evidence is decisive;
    /// publication by itself does not acknowledge terminal truth or thread exit.
    pub(crate) fn drain_signal(&self) -> DrainSignal {
        self.state.drain.clone()
    }

    /// The decode worker (or its panic guard) reports a decode failure.
    /// The first worker-side failure publication — decode or processing
    /// — wins ([`WorkerFailure`]); later calls are no-ops. The
    /// publication and the settlement step run under one lock hold,
    /// inside this call.
    pub(crate) fn decode_failed(&self, message: &str) {
        self.publish_worker_failure(WorkerFailureOrigin::Decode, message);
    }

    /// The decode worker reports an AUDIO PROCESSING failure
    /// (ADR-PBK-002 D14.11): the same D11 `Failed` terminal class, via
    /// the one worker-failure record with the processing origin, so the
    /// internal diagnosis stays truthful instead of borrowing the decode
    /// label. First worker-side failure publication wins — see
    /// [`WorkerFailure`]. Production route (Issue #177 I1): the
    /// episode-owned processing runtime's `stage` is the only publisher;
    /// the worker routes the failure here and fails the edge. No
    /// bypass, no partial result: a failed processor's output is not
    /// trustworthy.
    pub(crate) fn processing_failed(&self, message: &str) {
        self.publish_worker_failure(WorkerFailureOrigin::Processing, message);
    }

    /// The one worker-failure publication path (first wins).
    fn publish_worker_failure(&self, origin: WorkerFailureOrigin, message: &str) {
        self.publish(|state| {
            if state.worker_failure.is_none() {
                state.worker_failure = Some(WorkerFailure {
                    origin,
                    diagnostic: message.to_owned(),
                });
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
    /// teardown release the gate and notify its capped wait), and
    /// terminal settlement suppresses further pause routing. A render leg
    /// may still execute until its separate stop/join acknowledgement.
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
    /// Internal recording is one completion-lock unit: recheck episode
    /// conditions, reserve the free slot, reset cut evidence and route
    /// the hold. Edge Open was sampled separately and is NOT rechecked
    /// by that unit. A plant refines to semantic Accepted only if the
    /// actual D14.5 conjunction holds there (temporal semantics §6.1).
    /// A non-Open plant is never-Accepted Refused/Inert bookkeeping:
    /// irreversible edge terminals and worker checks prevent provider
    /// seek/purge/rebase; the exit funnel clears any stranded slot/hold.
    ///
    /// Recording still serializes with worker-gone publication: a plant
    /// before it is found by exit cleanup; a request after it is rejected.
    /// That cleanup guarantee does not make every plant Accepted.
    pub(crate) fn request_seek(&self, target: Duration) {
        // First hold: cheap reject against the command state and worker
        // liveness. The edge is reached only after this lock is dropped
        // (the established completion→edge discipline: `request_stop`'s
        // pattern). Session ending is rechecked by the second hold;
        // edge ending is checked separately by the worker. These
        // samples do not form one atomic joint eligibility predicate.
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
        // The atomic internal-record unit (see the refinement above). Lock order:
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
            // A NEW cut cycle owns its own OPERATION evidence: the
            // previous cycle's landing/refusal/commit latches are reset
            // here, at recording, and must never satisfy THIS cycle's
            // commit boundary (the same current-engagement attribution
            // discipline D14.7 freezes for pause). Safe against the
            // worker's in-flight protocol: the one-seek slot only frees
            // after the previous protocol fully resolved, and the
            // worker's next-cycle publications serialize after this
            // hold through this same lock.
            guard.seek_landing = None;
            guard.seek_refused = false;
            guard.cut_committed = false;
            // The cut's park routes INSIDE this hold, so it can never
            // lag the plant: the worker-exit cleanup always finds and
            // releases exactly what an internal record routed.
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
    /// next slice re-peeks. The slice caps a requested capacity wait,
    /// not repeated contention, scheduling or time to the serialization point.
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

    /// Product-control's Audio Processing state (campaign #190 D4): the
    /// application seam routes typed processing commands through it, and
    /// the decode worker's live runtime holds the same `Arc`.
    pub(crate) fn processing(&self) -> Arc<ProcessingControl> {
        self.state.processing.clone()
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
    /// lock hold — marks the worker gone, which orders recording
    /// against worker exit; it does not certify edge eligibility.
    /// The caller MUST run
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
    /// Recording serializes against that publication: a plant whose
    /// recording hold ran before it is found and cleared here, including
    /// a never-Accepted ending-raced plant; a request after it is rejected.
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

    /// Pure observation. Completion-state fields share one lock hold
    /// (no torn combinations such as `Stopped` with `stop_requested == false`).
    /// Position and DSP refusal are read from independently written cells;
    /// no cross-cell single-instant or freshness promise covers them.
    ///
    /// Position is a projection: it is one pure load of the episode's
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
            last_processing_refusal: self.state.processing.last_refusal(),
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
/// maintain the pause latches, seek events maintain the seek latches, so
/// a cut-attributed seek park can never satisfy the Paused
/// establishment (which reads only the pause pair). The converse is
/// deliberately NOT separate: the D14.5 seek commit reads the physical
/// parked-and-quiesced conjunction under EITHER attribution — the seek
/// pair or the pause pair — because a paused episode's already-quiesced
/// tail satisfies the output-cut precondition (the frozen D14.5 pause
/// interaction). Seek events never touch the pause latches, so a cut
/// park can never fabricate `Paused` truth.
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
    // In this Session resolver, worker-failure evidence has precedence:
    // the failure was published before the edge was failed. The stage
    // spelling keeps the origin truthful (D14.11): a decode failure and
    // an audio-processing failure settle the same `Failed` terminal
    // class through the one worker-failure record, each with its own
    // origin spelling.
    if let Some(failure) = &state.worker_failure {
        return Some(SessionOutcome::Failed {
            stage: failure.stage(),
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
                // wakes edge waiters. Worker-exit progress still depends
                // on scheduling and outstanding native decoder calls returning.
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
pub(crate) mod tests {
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

    /// S6-F01: one consumer can hold a fetched chunk while the ring is
    /// refilled. Failure publication does not atomically fail the edge.
    #[cfg(not(loom))]
    #[test]
    fn failed_fact_can_coexist_with_full_ring_and_fetched_pcm() {
        use qianqian_audio_api::ports::{PcmPull, RenderPcmInput};

        let completion = SessionCompletion::new();
        let edge = Arc::new(PcmEdge::new(2, 8));
        completion.bind_stop_target(edge.clone());
        assert_eq!(edge.write_some(&[1.0; 16]), 16);
        let mut fetched = [0.0; 4];
        assert_eq!(edge.read_frames(&mut fetched), PcmPull::Frames(2));
        assert_eq!(edge.write_some(&[2.0; 4]), 4);

        completion.decode_failed("next provider read failed");
        assert_eq!(edge.terminal(), EdgeTerminal::Open);
        assert_eq!(
            completion.observe_snapshot().terminal_outcome,
            Some(EpisodeTerminalOutcome::Failed)
        );
        assert_eq!(edge.buffered_frames(), 8);
        assert_eq!(fetched, [1.0; 4]);
        assert_eq!(edge.buffered_frames() + fetched.len() / 2, 10);
        edge.fail();
        assert_eq!(edge.read_frames(&mut fetched), PcmPull::Stopped);
        assert_eq!(edge.buffered_frames(), 8, "abandonment is not a purge");
    }

    /// Test-local boundary fixture shared with the real-worker oracle.
    /// A's slot-release critical section is held by this driver; B uses
    /// the real request method. No production path gains a test hook.
    #[cfg(not(loom))]
    pub(crate) fn record_after_delayed_open_sample(
        ending_before_free: bool,
    ) -> (SessionCompletion, Arc<PcmEdge>) {
        use std::sync::TryLockError;
        use std::time::Instant;

        let completion = SessionCompletion::new();
        let edge = Arc::new(PcmEdge::new(2, 8));
        completion.bind_stop_target(edge.clone());
        assert_eq!(edge.write_some(&[7.0; 8]), 8);
        let position = completion.position_evidence();
        position.publish_consumed(4, 0);

        // A is genuinely Open-at-plant Accepted. Its worker already took
        // the command; select its non-committing ending/release seam.
        completion.request_seek(Duration::from_secs(1));
        assert_eq!(completion.take_seek_command(), Some(Duration::from_secs(1)));
        let mut slot = completion.state.seek_slot.lock().expect("seek slot lock");
        assert!(slot.in_flight);
        let references_before_b = Arc::strong_count(&edge);
        let mut request = None;

        // Holding edge state makes B stop between its two completion
        // holds. The cloned edge plus acquired completion guard proves
        // its FIRST hold has finished, without sleeps or scheduler odds.
        crate::edge::test_sync::with_terminal_sample_blocked(&edge, || {
            let b = completion.clone();
            request = Some(std::thread::spawn(move || {
                b.request_seek(Duration::from_secs(2))
            }));
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let guard = completion.state.state.lock().expect("completion lock");
                if Arc::strong_count(&edge) > references_before_b {
                    drop(guard);
                    break;
                }
                drop(guard);
                assert!(Instant::now() < deadline, "B never reached its edge sample");
                std::thread::yield_now();
            }
        });
        // First hold is known finished; only B's SECOND hold can now
        // own completion while blocked on the occupied slot. Open was
        // observed before that hold. Timeout is a backstop, not ordering.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match completion.state.state.try_lock() {
                Err(TryLockError::WouldBlock) => break,
                Ok(guard) => drop(guard),
                Err(TryLockError::Poisoned(_)) => panic!("completion lock poisoned"),
            }
            assert!(Instant::now() < deadline, "B never reached its second hold");
            std::thread::yield_now();
        }

        if ending_before_free {
            edge.stop();
            assert_eq!(edge.terminal(), EdgeTerminal::Stopped);
            assert!(slot.in_flight, "no Open/free interval during B");
        } else {
            assert_eq!(edge.terminal(), EdgeTerminal::Open);
        }
        // A's no-commit resolution releases the gate, then clears the
        // slot. Drive that existing clear critical section while B cannot
        // enter it; its two assignments are the only fixture mutation.
        completion.release_seek_park();
        slot.in_flight = false;
        slot.command = None;
        drop(slot);
        request.take().unwrap().join().expect("B returned");

        assert!(
            completion.seek_in_flight(),
            "B really planted after the selected ending/Open branch"
        );
        assert_eq!(
            completion.state.seek_slot.lock().unwrap().command,
            Some(Duration::from_secs(2))
        );
        assert_eq!(completion.seek_protocol_state(), (false, false, None));
        assert_eq!(edge.buffered_frames(), 4, "B did not purge PCM");
        assert_eq!(position.published(), Some(4), "B did not rebase");
        let state = completion.state.state.lock().unwrap();
        assert!(!state.worker_gone && !state.stop_requested && state.outcome.is_none());
        drop(state);

        (completion, edge)
    }

    /// S6-F02 exact busy→Stopped→free witness and the returning worker's
    /// real exit funnel. This boundary oracle does not run a device or
    /// resume A's whole worker history. The companion session oracle
    /// drives B's record through real worker/provider/gate seams.
    #[cfg(not(loom))]
    #[test]
    fn sampled_open_busy_then_stopped_free_plant_is_never_accepted() {
        let (completion, edge) = record_after_delayed_open_sample(true);
        let position = completion.position_evidence();
        // The ending render's Aborted evidence alone is not terminal
        // truth. B remains unpicked, so the sole worker has executed no
        // B provider operation. Its exiting path clears, not executes it.
        completion.drain_signal().complete(DrainVerdict::Aborted);
        assert_eq!(completion.committed(), None);
        completion.worker_exited(EdgeTerminal::Stopped);
        completion.abort_stranded_seek();
        assert!(!completion.seek_in_flight());
        assert_eq!(completion.take_seek_command(), None);
        assert_eq!(
            consume_release(&completion.render_gate()),
            Some(SeekParkRelease::Aborted)
        );
        assert_eq!(consume_release(&completion.render_gate()), None);
        assert_eq!(edge.buffered_frames(), 4);
        assert_eq!(position.published(), Some(4));
        assert_eq!(
            completion.observe_snapshot().terminal_outcome,
            Some(EpisodeTerminalOutcome::Failed)
        );
        assert_eq!(completion.seek_protocol_state(), (false, false, None));
        completion.request_stop();
        assert_eq!(
            completion.observe_snapshot().terminal_outcome,
            Some(EpisodeTerminalOutcome::Failed)
        );
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
    /// Each recording hold is atomic against worker-gone (plant + reset +
    /// hold routing). These two sides exhaust that cleanup ordering, not
    /// the separately sampled edge eligibility; the ending-race oracle
    /// below distinguishes a private plant from semantic Accepted.
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
    // --- TEMPORAL-MODEL-0 campaign S0 provenance probes (issue #195) ----
    //
    // Research evidence for the load-sensitive-flake campaign: they pin,
    // deterministically and without any scheduler premise, WHICH evidence
    // class can ground the pended-seek actionability precondition and
    // what can — and cannot — satisfy a new seek cycle. They drive the
    // real publication boundary (apply_gate_event / leg_parked_evidence /
    // seek_cutover_decision) exactly as the leg and worker do; no
    // product surface is involved. Like every white-box probe here, each
    // test releases or consumes whatever intent it routed before it
    // drops the completion — no routed-state debris outlives a probe.
    //
    // Partitioned out of loom builds (audit corrective): the probes drive
    // the verification-only readers (`seek_in_flight`, `consume_release`)
    // and the gate slice vocabulary, which exist only outside loom.

    /// The #195 scenario's provenance: with no pause intent routed, the
    /// ONLY park evidence a pended seek can act on is its own cut park
    /// (SeekEngaged). The "observed but not yet actionable" phase is
    /// real but its length is a scheduling fact — the precondition flips
    /// at the leg's next loop-top, not at any protocol state.
    #[test]
    #[cfg(not(loom))]
    fn s0_a_pended_seeks_actionability_is_its_own_cut_park_absent_pause_intent() {
        let completion = SessionCompletion::new();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        let core = completion.state.clone();

        completion.request_seek(Duration::from_secs(1));
        assert!(
            completion.seek_in_flight(),
            "precondition: the seek planted (unsettled, Open plane, live worker)"
        );
        assert!(
            !completion.leg_parked_evidence(),
            "before the leg's next loop-top the seek is observed but NOT              actionable — the window whose length the historical flake              premised on"
        );

        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::SeekEngaged));
        let guard = core.state.lock().expect("completion lock");
        assert!(
            guard.seek_engaged && !guard.engaged && !guard.pause_requested,
            "provenance: the park evidence is cut-attributed (SeekEngaged); \
             no pause engagement exists in this scenario to misattribute"
        );
        drop(guard);
        assert!(
            completion.leg_parked_evidence(),
            "the same physical latch the worker's write-path predicate reads"
        );

        // Leave no routed intent behind (the pended seek resolves as an
        // aborted cut, exactly as a stop would have released it).
        completion.release_seek_without_commit();
        assert_eq!(
            consume_release(&completion.render_gate()),
            Some(SeekParkRelease::Aborted),
        );
        assert_eq!(consume_release(&completion.render_gate()), None);
    }

    /// Starvation is not park evidence: an intent-free loop-top parks
    /// nothing and publishes nothing (a data-starved leg waits inside
    /// its read, after the gate). And the dual attribution is by design:
    /// a paused episode's engagement IS the physical park class the
    /// precondition accepts (the frozen paused-seek reuse, D14.5).
    #[test]
    #[cfg(not(loom))]
    fn s0_starvation_produces_no_park_evidence_and_pause_attribution_is_the_frozen_reuse() {
        // (a) intent-free loop-top: no park, no evidence.
        {
            let completion = SessionCompletion::new();
            completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
            completion.render_gate().park_loop_top(|slice| match slice {
                GateSlice::TailProbe => TailProbeOutcome::Quiesced,
                GateSlice::SeekRelease(_) => TailProbeOutcome::Pending,
            });
            let guard = completion.state.state.lock().expect("completion lock");
            assert!(
                !guard.engaged && !guard.seek_engaged && !guard.pause_requested,
                "an intent-free loop-top fabricated no park evidence"
            );
        }
        // (b) pause attribution: engagement grounds the precondition,
        // provenance distinguishable in the evidence stream.
        {
            let completion = SessionCompletion::new();
            completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
            let core = completion.state.clone();
            completion.request_pause();
            publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Engaged));
            let guard = core.state.lock().expect("completion lock");
            assert!(
                guard.engaged && !guard.seek_engaged,
                "provenance: pause-attributed engagement, not a seek park"
            );
            drop(guard);
            assert!(
                completion.leg_parked_evidence(),
                "a paused episode's seek reuses the pause park (the frozen \
                 dual attribution)"
            );
            // Release the pause intent; the leg's Disengaged acknowledgment
            // clears the engagement latch.
            completion.request_resume();
            publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Disengaged));
        }
    }

    /// Cycle attribution without identity: operation evidence (landing,
    /// commit) cannot cross seek cycles — acceptance resets it and only
    /// the new cycle's own publications can satisfy its commit boundary —
    /// and a CLEARED park cannot ground a new cycle either. The cut
    /// discipline is carried by the evidence lifecycle, not by a token.
    #[test]
    #[cfg(not(loom))]
    fn s0_operation_evidence_cannot_cross_cycles_and_cleared_parks_ground_nothing() {
        let completion = SessionCompletion::new();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        let core = completion.state.clone();

        // Cycle 1 parks, quiesces, lands, and commits lawfully.
        completion.request_seek(Duration::from_secs(1));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::SeekEngaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::SeekTailQuiesced));
        completion.seek_landing_published(Some(42));
        assert_eq!(
            completion.seek_cutover_decision(Some(42)),
            CutoverDecision::Committed,
            "precondition: cycle 1 commits on its own evidence"
        );

        // The leg leaves the park (release consumed; SeekDisengaged
        // published on the leg's thread before the slot can free).
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::SeekDisengaged));
        completion.clear_seek_in_flight();

        // Cycle 2: acceptance resets the per-cycle operation evidence,
        // and cycle 1's cleared park grounds nothing.
        completion.request_seek(Duration::from_secs(2));
        {
            let guard = core.state.lock().expect("completion lock");
            assert_eq!(guard.seek_landing, None, "landing cannot cross cycles");
            assert!(!guard.cut_committed, "a commit cannot cross cycles");
            assert!(!guard.seek_engaged, "the prior park was disengaged");
        }
        assert!(
            !completion.leg_parked_evidence(),
            "stale park evidence cannot ground a new cycle"
        );
        assert_eq!(
            completion.seek_cutover_decision(Some(43)),
            CutoverDecision::Pending,
            "a fresh landing alone does not commit: the cycle needs ITS OWN \
             current park"
        );

        // Cycle 2's OWN park + quiescence + its own landing commit.
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::SeekEngaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::SeekTailQuiesced));
        completion.seek_landing_published(Some(43));
        assert_eq!(
            completion.seek_cutover_decision(Some(43)),
            CutoverDecision::Committed,
            "the second seek's own evidence satisfies the second cycle \
             (the corrective-1 attribution discipline)"
        );
        // The second commit's release payload is consumed by the leg.
        assert_eq!(
            consume_release(&completion.render_gate()),
            Some(SeekParkRelease::Committed { landing: Some(43) }),
        );
        assert_eq!(consume_release(&completion.render_gate()), None);
    }

    /// The paused-cut route: pause engagement + ITS quiescence satisfy
    /// the commit boundary for a seek accepted while parked (D14.5:
    /// "a paused episode's already-quiesced tail satisfies the
    /// output-cut precondition") — the conjunction is attribution-blind
    /// BY CONTRACT because both classes prove the same physical fact.
    #[test]
    #[cfg(not(loom))]
    fn s0_a_paused_episodes_quiesced_park_commits_the_cut() {
        let completion = SessionCompletion::new();
        completion.bind_stop_target(Arc::new(PcmEdge::new(2, 8192)));
        let core = completion.state.clone();

        completion.request_pause();
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Engaged));
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::TailQuiesced));
        completion.request_seek(Duration::from_secs(1));
        assert!(
            completion.leg_parked_evidence(),
            "the pause park grounds the precondition instantly (leg already parked)"
        );
        completion.seek_landing_published(Some(7));
        assert_eq!(
            completion.seek_cutover_decision(Some(7)),
            CutoverDecision::Committed,
            "the paused episode's seek cuts on the pause-attributed park pair"
        );
        // Release the pause intent FIRST: a consume_release while the
        // pause is still routed would legitimately park forever (the
        // D14.7 park waits for a release only a resume routes, and this
        // probe's tail probe never quiesces).
        completion.request_resume();
        publish_evidence(&core, |s| apply_gate_event(s, GateEvent::Disengaged));
        // The committed release is consumed by the leg; no routed debris.
        assert_eq!(
            consume_release(&completion.render_gate()),
            Some(SeekParkRelease::Committed { landing: Some(7) }),
        );
        assert_eq!(consume_release(&completion.render_gate()), None);
    }
}
