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

impl SessionCompletion {
    pub fn new() -> Self {
        Self {
            state: Arc::new(CompletionArc {
                state: Mutex::new(CompletionState {
                    outcome: None,
                    worker_terminal: None,
                    decode_failure: None,
                    source_format: None,
                }),
                signal: Condvar::new(),
                drain: DrainSignal::new(),
            }),
        }
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
            // The drain verdict has no condvar wired into this one; a
            // bounded poll is the control-plane bridge. The render side
            // also always wakes us via worker_exited before exiting in
            // practice, so the poll is a backstop, not the mechanism.
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
    if let Some(terminal) = state.worker_terminal {
        if terminal == EdgeTerminal::Failed {
            state.outcome = Some(SessionOutcome::Failed {
                stage: "decode".to_owned(),
            });
            return state.outcome.clone();
        }
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
