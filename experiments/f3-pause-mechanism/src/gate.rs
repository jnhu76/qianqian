//! The session-owned pause gate — the mechanism candidate A control
//! object, in the shape the F3-GATE evidence recommends the authority
//! freeze on (ADR-PBK-002 D14.7 proposal).
//!
//! Semantics frozen by this evidence shape (semantic spine, not
//! representation):
//!
//! ```text
//! pause_requested   Command state. Session writes it under the gate
//!                   lock. Idempotent; monotone per pause/resume cycle.
//! engaged           Mechanism acknowledgment. Written ONLY by the
//!                   render loop under the same lock, immediately before
//!                   it parks — this is what makes "Paused truthfully
//!                   established" an observed mechanism fact instead of
//!                   an inference from the command.
//! stopped           Stop release. Written by the episode stop path /
//!                   teardown under the same lock BEFORE (or alongside)
//!                   the data-plane stop. Wake semantics are
//!                   "unpark-and-continue": the gate never aborts the
//!                   render leg; the data-plane terminal decides. This
//!                   is what keeps every D11 decision-table history
//!                   unchanged (see RESULTS.md §resolver-consistency).
//! ```
//!
//! The park itself is bounded (`PARK_SLICE`): commands are also
//! notified, but the bound caps stop/resume latency even if a notify is
//! missed, so no liveness property depends on notify delivery.

use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crate::events::{Event, Log};

/// Upper bound for one parked slice. Command notifies wake immediately;
/// this bound is the miss-tolerance ceiling for engage/resume/stop
/// latency through the gate.
pub const PARK_SLICE: Duration = Duration::from_millis(25);

#[derive(Debug)]
struct GateState {
    pause_requested: bool,
    engaged: bool,
    stopped: bool,
}

#[derive(Debug)]
pub struct PauseGate {
    state: Mutex<GateState>,
    signal: Condvar,
    /// Evidence log for the engagement acknowledgments. The ack events
    /// are pushed here at the publication point itself (under the same
    /// lock hold as the state change), so the scenario oracles observe
    /// the real ack order, not a re-derivation. `None` in consumers
    /// that do not need the event trail (the physical probe polls
    /// [`PauseGate::observe`] directly).
    log: Option<Log>,
}

impl Default for PauseGate {
    fn default() -> Self {
        Self::new()
    }
}

impl PauseGate {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(GateState {
                pause_requested: false,
                engaged: false,
                stopped: false,
            }),
            signal: Condvar::new(),
            log: None,
        }
    }

    /// A gate that records its engagement acknowledgments as events.
    pub fn with_log(log: Log) -> Self {
        Self {
            log: Some(log),
            ..Self::new()
        }
    }

    fn ack(&self, event: Event) {
        if let Some(log) = &self.log {
            log.push(event);
        }
    }

    /// Record pause intent (Command). Idempotent. A stop already
    /// released cannot be paused past.
    pub fn request_pause(&self) {
        let mut g = self.state.lock().expect("pause gate lock");
        if !g.stopped {
            g.pause_requested = true;
        }
        drop(g);
        self.signal.notify_all();
    }

    /// Release pause (Command). Idempotent.
    pub fn request_resume(&self) {
        let mut g = self.state.lock().expect("pause gate lock");
        g.pause_requested = false;
        drop(g);
        self.signal.notify_all();
    }

    /// Release stop (Command + teardown wake). "Unpark-and-continue":
    /// the render leg resumes the loop and the data-plane terminal
    /// decides; the gate itself never aborts anything. Idempotent.
    pub fn release_stop(&self) {
        let mut g = self.state.lock().expect("pause gate lock");
        g.stopped = true;
        g.pause_requested = false;
        drop(g);
        self.signal.notify_all();
    }

    /// Command-state readback (evidence/diagnostic read).
    pub fn pause_requested(&self) -> bool {
        self.state.lock().expect("pause gate lock").pause_requested
    }

    /// One coherent observation of the gate truth classes: command
    /// state + mechanism acknowledgment, taken under one lock so the
    /// pair coexisted at one instant.
    pub fn observe(&self) -> GateObservation {
        let g = self.state.lock().expect("pause gate lock");
        GateObservation {
            pause_requested: g.pause_requested,
            engaged: g.engaged,
            stopped: g.stopped,
        }
    }

    /// Render-loop side: park while pause is requested and stop has not
    /// been released. Returns once the loop may proceed into its steady
    /// iteration. NEVER called while holding a device buffer: the
    /// caller parks strictly before GetBuffer.
    ///
    /// Publishes the engagement acknowledgment as a side effect of
    /// actually parking, and clears it when the loop proceeds.
    pub fn park_while_paused(&self) {
        let mut g = self.state.lock().expect("pause gate lock");
        if g.pause_requested && !g.stopped {
            // Engagement ack: observed command + actually parking.
            g.engaged = true;
            self.ack(Event::Engaged);
            drop(g);
            self.signal.notify_all();
            g = self.state.lock().expect("pause gate lock");
            while g.pause_requested && !g.stopped {
                let (next, _timed_out) = self
                    .signal
                    .wait_timeout(g, PARK_SLICE)
                    .expect("pause gate wait poisoned");
                g = next;
            }
        }
        if g.engaged {
            // Disengagement ack: the loop is about to consume again.
            g.engaged = false;
            self.ack(Event::Disengaged);
            drop(g);
            self.signal.notify_all();
        }
    }
}

/// One coherent gate observation: `engaged` is mechanism evidence (the
/// render leg parked), `pause_requested` is command state; "truthfully
/// paused" per the D14.7 proposal is the conjunction, never either
/// alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateObservation {
    pub pause_requested: bool,
    pub engaged: bool,
    pub stopped: bool,
}
