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
    /// to a render stream. The gate routes pause intent to the mechanism
    /// and acknowledges engagement/tail-quiescence/disengagement back as
    /// mechanism evidence (never Facts, never settlement inputs).
    gate: RenderGate,
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
            self.state.gate.set_paused(false);
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

/// The one gate-evidence attribution rule (D14.7): what each render-gate
/// event does to the evidence latches. Named so the delayed-delivery
/// oracle below can drive the production arm synchronously — the real
/// observer above routes every event through this function, so a test
/// that calls it exercises exactly the attribution the leg performs.
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
}
