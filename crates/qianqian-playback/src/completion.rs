//! Session completion: session-owned truth about how one playback episode
//! ended. Not K0 semantic truth, not a Fact plane object — the Host waits
//! on it and then initiates disposal.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use qianqian_core::ports::{DrainSignal, DrainVerdict};

use crate::edge::EdgeTerminal;

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
    /// (diagnostic readback for the Host).
    source_format: Option<qianqian_core::ports::PcmFormat>,
    /// Why activation raised, published by the session itself (the
    /// kernel's diagnostic surface carries the verdict, not the message).
    activation_failure: Option<String>,
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
    pub fn set_source_format(&self, format: qianqian_core::ports::PcmFormat) {
        let mut guard = self.state.state.lock().expect("completion lock");
        if guard.source_format.is_none() {
            guard.source_format = Some(format);
        }
    }

    /// The source format of this episode, once activation published it.
    pub fn source_format(&self) -> Option<qianqian_core::ports::PcmFormat> {
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
            state.outcome = Some(if state.worker_terminal == Some(EdgeTerminal::Stopped) {
                SessionOutcome::Stopped
            } else {
                SessionOutcome::Failed {
                    stage: "device".to_owned(),
                }
            });
        }
        None => {}
    }
    state.outcome.clone()
}
