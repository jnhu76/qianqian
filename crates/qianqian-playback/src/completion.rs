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
//!         ↓  every publication path ends in a session-owned
//!            settlement step on the same session-owned call stack
//! Playback Session-owned settlement (resolve + commit exactly once)
//!         ↓
//! observe / wait_terminal consume the committed Fact only
//! ```
//!
//! Three publication sites can complete the decisive evidence set, so
//! three session-owned paths run the settlement step:
//!
//! ```text
//! decode failure evidence   → settle on the worker call stack
//! worker terminal evidence  → settle on the worker call stack
//! drain verdict             → settle on the settlement watcher
//!                             (the verdict is published inside the
//!                             output provider's render thread — the one
//!                             site no session-owned call stack observes)
//! ```
//!
//! Neither `observe_snapshot` nor `wait_terminal` resolves: a consumer
//! call can never create the terminal Fact, and no consumer call is
//! required for it to appear ("worker evidence last" and "drain verdict
//! last" both commit autonomously).
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

use qianqian_audio_api::ports::{DrainSignal, DrainVerdict, PcmFormat};

use crate::edge::{EdgeTerminal, PcmEdge};
use crate::handle::PlaybackSessionObservation;

/// How one playback episode ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionOutcome {
    /// EOF was produced, drained and played out.
    Completed,
    /// The episode failed before completion. The stage names which leg
    /// failed first ("decode: ..." or "device") — a diagnostic, not a
    /// frozen semantic variant (D14.2).
    Failed { stage: String },
    /// Stop was requested before completion.
    Stopped,
}

struct CompletionState {
    outcome: Option<SessionOutcome>,
    /// Terminal the decode worker observed on the edge at its exit.
    worker_terminal: Option<EdgeTerminal>,
    decode_failure: Option<String>,
    /// Source PCM format, published once at session activation
    /// (mechanism-evidence diagnostic readback).
    source_format: Option<PcmFormat>,
    /// Why activation raised, published by the session itself (the
    /// kernel's diagnostic surface carries the verdict, not the message).
    activation_failure: Option<String>,
    /// Stop intent, recorded by [`SessionCompletion::request_stop`].
    /// Command state, not outcome truth: an episode that already ended
    /// (Completed/Failed) is not retroactively renamed by a late stop.
    stop_requested: bool,
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
}

impl Default for SessionCompletion {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionCompletion {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(CompletionArc {
                state: Mutex::new(CompletionState {
                    outcome: None,
                    worker_terminal: None,
                    decode_failure: None,
                    source_format: None,
                    activation_failure: None,
                    stop_requested: false,
                    stop_target: None,
                }),
                signal: Condvar::new(),
                drain: DrainSignal::new(),
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

    /// The drain signal handed to the render stream's open request.
    pub(crate) fn drain_signal(&self) -> DrainSignal {
        self.state.drain.clone()
    }

    /// The decode worker (or its panic guard) reports a decode failure.
    /// First failure wins; later calls are no-ops. Publishing this
    /// evidence ends in the session-owned settlement step on the same
    /// call stack (publication != commit).
    pub(crate) fn decode_failed(&self, message: &str) {
        {
            let mut guard = self.state.state.lock().expect("completion lock");
            if guard.decode_failure.is_none() {
                guard.decode_failure = Some(message.to_owned());
            }
        }
        self.settle_now();
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
            guard.stop_target.clone()
        };
        if let Some(edge) = target {
            edge.stop();
        }
    }

    /// Frames currently buffered on the session's edge, once bound.
    /// Test/verifier diagnostic only (F2 ruling, D14.3): mechanism
    /// evidence, NOT application observation and NOT UI contract. It
    /// exists only in test builds so it cannot drift into the product
    /// seam. `None` before the session bound its edge.
    #[cfg(test)]
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
    /// Publishing this evidence ends in the session-owned settlement
    /// step on the same call stack.
    pub(crate) fn worker_exited(&self, terminal: EdgeTerminal) {
        {
            let mut guard = self.state.state.lock().expect("completion lock");
            guard.worker_terminal = Some(terminal);
        }
        self.settle_now();
    }

    /// The session-owned settlement step (D11 commit-progress ownership):
    /// evaluate the terminal contract over the currently published
    /// evidence and commit exactly one terminal outcome if it is
    /// decisive. Idempotent; first-wins; notifies waiters only on the
    /// unsettled→committed transition.
    ///
    /// Only session-owned execution paths call this — the worker wrapper
    /// after evidence publication and the settlement watcher after the
    /// drain verdict. No consumer call reaches it.
    pub(crate) fn settle_now(&self) {
        let mut guard = self.state.state.lock().expect("completion lock");
        if guard.outcome.is_some() {
            return;
        }
        let outcome = resolve(&mut guard, &self.state.drain);
        if outcome.is_some() {
            drop(guard);
            self.state.signal.notify_all();
        }
    }

    /// Spawn the settlement watcher for one live episode: one tiny
    /// control-plane thread that blocks on the drain verdict and then
    /// runs the settlement step. It exists because the verdict is
    /// published inside the output provider's render thread — the one
    /// evidence site no session-owned call stack observes — so a
    /// drain-last episode (natural EOF is exactly that shape) would
    /// otherwise have no settlement trigger.
    ///
    /// The render leg publishes exactly one verdict on every exit path
    /// (including panics), and episode teardown stops and joins that leg
    /// (stream `stop_and_join`) before the session joins this watcher,
    /// so the `drain.wait()` below cannot wedge teardown. The watcher is
    /// single-shot: one verdict, one settlement attempt, exit.
    pub(crate) fn spawn_settlement_watcher(&self) -> std::io::Result<std::thread::JoinHandle<()>> {
        let completion = self.clone();
        std::thread::Builder::new()
            .name("qianqian-settle".into())
            .spawn(move || {
                completion.state.drain.wait();
                completion.settle_now();
            })
    }

    /// One coherent observation of the episode, taken under a single
    /// lock acquisition so the returned fields coexisted at one real
    /// instant (no torn combinations such as `Stopped` with
    /// `stop_requested == false`).
    pub(crate) fn observe_snapshot(&self) -> PlaybackSessionObservation {
        let guard = self.state.state.lock().expect("completion lock");
        PlaybackSessionObservation {
            terminal_outcome: guard.outcome.clone(),
            stop_requested: guard.stop_requested,
            source_format: guard.source_format,
            activation_error: guard.activation_failure.clone(),
        }
    }

    /// The committed terminal outcome, if any. Pure read: never settles.
    /// Verifier-facing (the decision-table oracle's read); not part of
    /// any runtime path.
    #[cfg(test)]
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
}

fn resolve(state: &mut CompletionState, drain: &DrainSignal) -> Option<SessionOutcome> {
    if state.outcome.is_some() {
        return state.outcome.clone();
    }
    // Decode failure is authoritative over everything downstream: the
    // failure was published before the edge was failed.
    if let Some(message) = &state.decode_failure {
        state.outcome = Some(SessionOutcome::Failed {
            stage: format!("decode: {message}"),
        });
        return state.outcome.clone();
    }
    if state.worker_terminal == Some(EdgeTerminal::Failed) {
        state.outcome = Some(SessionOutcome::Failed {
            stage: "decode".to_owned(),
        });
        return state.outcome.clone();
    }
    match drain.peek() {
        Some(DrainVerdict::Drained) => {
            if state.worker_terminal == Some(EdgeTerminal::Eof) {
                state.outcome = Some(SessionOutcome::Completed);
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
                    // publishes intent before it releases the edge, so
                    // any stop-caused Stopped necessarily observes it.
                    // Settlement runs on evidence publication paths, so
                    // the intent read here is the intent recorded by the
                    // decisive boundary (D11 late-command rule); a later
                    // stop cannot relabel this classification.
                    if state.stop_requested {
                        state.outcome = Some(SessionOutcome::Stopped);
                    } else {
                        state.outcome = Some(SessionOutcome::Failed {
                            stage: "device".to_owned(),
                        });
                    }
                }
                Some(_) => {
                    state.outcome = Some(SessionOutcome::Failed {
                        stage: "device".to_owned(),
                    });
                }
            }
        }
        None => {}
    }
    state.outcome.clone()
}
