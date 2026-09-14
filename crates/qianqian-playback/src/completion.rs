//! Session completion: the session-owned application-facing seam for one
//! playback episode. Control intent enters through [`SessionCompletion::
//! request_stop`]; resolved truth leaves through `wait` /
//! `try_resolve_now`. It is not K0 semantic truth, not a Fact plane
//! object — the App holds it, drives the episode with it, waits on it,
//! and then initiates disposal.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use qianqian_audio_api::ports::{DrainSignal, DrainVerdict};

use crate::edge::{EdgeTerminal, PcmEdge};

/// How one playback episode ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionOutcome {
    /// EOF was produced, drained and played out.
    Completed,
    /// The episode failed before completion. The stage names which leg
    /// failed first ("decode: ..." or "device").
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
    /// (diagnostic readback for the App).
    source_format: Option<qianqian_audio_api::ports::PcmFormat>,
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
    /// this handle only routes intent to it.
    stop_target: Option<Arc<PcmEdge>>,
}

#[derive(Clone)]
pub struct SessionCompletion {
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
    pub fn new() -> Self {
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

    /// The session publishes its activation failure (first wins).
    pub fn activation_failed(&self, message: &str) {
        let mut guard = self.state.state.lock().expect("completion lock");
        if guard.activation_failure.is_none() {
            guard.activation_failure = Some(message.to_owned());
        }
    }

    /// Why activation raised, if it did (Host diagnostics).
    pub fn activation_error(&self) -> Option<String> {
        self.state
            .state
            .lock()
            .expect("completion lock")
            .activation_failure
            .clone()
    }

    /// The session publishes the endpoint's source format at activation.
    pub fn set_source_format(&self, format: qianqian_audio_api::ports::PcmFormat) {
        let mut guard = self.state.state.lock().expect("completion lock");
        if guard.source_format.is_none() {
            guard.source_format = Some(format);
        }
    }

    /// The source format of this episode, once activation published it.
    pub fn source_format(&self) -> Option<qianqian_audio_api::ports::PcmFormat> {
        self.state
            .state
            .lock()
            .expect("completion lock")
            .source_format
    }

    /// The drain signal handed to the render stream's open request.
    pub fn drain_signal(&self) -> DrainSignal {
        self.state.drain.clone()
    }

    /// The decode worker (or its panic guard) reports a decode failure.
    /// First failure wins; later calls are no-ops.
    pub fn decode_failed(&self, message: &str) {
        let mut guard = self.state.state.lock().expect("completion lock");
        if guard.decode_failure.is_none() {
            guard.decode_failure = Some(message.to_owned());
        }
        drop(guard);
        self.state.signal.notify_all();
    }

    /// Request the episode to stop.
    ///
    /// This is a command, not a fact: it records stop intent and releases
    /// the session's data-plane stop (which is idempotent and first-wins,
    /// so it never overwrites a committed EOF or failure). How the episode
    /// actually ends remains decided solely by [`Self::resolve`]. Calling
    /// this after the outcome is resolved is a no-op with respect to that
    /// outcome.
    ///
    /// Safe from any thread and any state:
    /// - before the session bound an edge (not yet activated): intent is
    ///   recorded and applied the moment activation binds the edge;
    /// - while playing: both legs are woken with terminal outcomes;
    /// - after resolution: nothing changes.
    pub fn request_stop(&self) {
        let target = {
            let mut guard = self.state.state.lock().expect("completion lock");
            guard.stop_requested = true;
            guard.stop_target.clone()
        };
        if let Some(edge) = target {
            edge.stop();
        }
    }

    /// Whether stop intent has been recorded. Command-state visibility
    /// (F2 status will read it); it says nothing about the outcome.
    pub fn stop_requested(&self) -> bool {
        self.state
            .state
            .lock()
            .expect("completion lock")
            .stop_requested
    }

    /// Frames currently buffered on the session's edge, once bound.
    /// Diagnostic mechanism-evidence readback (same class as
    /// [`Self::source_format`]); it is not an outcome and carries no
    /// control authority. `None` before the session bound its edge.
    pub fn buffered_frames(&self) -> Option<usize> {
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
    pub fn bind_stop_target(&self, edge: Arc<PcmEdge>) {
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
    pub fn worker_exited(&self, terminal: EdgeTerminal) {
        let mut guard = self.state.state.lock().expect("completion lock");
        guard.worker_terminal = Some(terminal);
        drop(guard);
        self.state.signal.notify_all();
    }

    /// Resolve the current outcome from the observed legs, if decided.
    pub fn try_resolve_now(&self) -> Option<SessionOutcome> {
        let mut guard = self.state.state.lock().expect("completion lock");
        resolve(&mut guard, &self.state.drain)
    }

    /// Block until the episode resolves.
    pub fn wait(&self) -> SessionOutcome {
        let mut guard = self.state.state.lock().expect("completion lock");
        loop {
            if let Some(outcome) = resolve(&mut guard, &self.state.drain) {
                return outcome;
            }
            // The drain verdict lives on its own condvar inside
            // DrainSignal, which this wait cannot block on; the bounded
            // poll below is the bridge. 20 ms of Host-side latency is
            // irrelevant on a control-plane wait, and the state the poll
            // reads is written once per leg.
            let (next, _timeout) = self
                .state
                .signal
                .wait_timeout(guard, Duration::from_millis(20))
                .expect("completion wait poisoned");
            guard = next;
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
                // The worker's exit terminal is the authority for "this
                // abort was the stop, not a device failure". Until the
                // worker has exited, the outcome is not decidable: an
                // aborted render alone happens on every stop (the render
                // leg is typically the first to observe it). Keep
                // waiting — every abort path releases the data-plane
                // stop, and that stop wakes the worker, so this always
                // terminates.
                None => {}
                Some(EdgeTerminal::Stopped) => {
                    state.outcome = Some(SessionOutcome::Stopped);
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
