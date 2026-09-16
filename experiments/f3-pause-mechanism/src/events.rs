//! Instrumented event log for the synchronization-shape scenarios.
//!
//! The scenarios' oracles read a total order of mechanism events after
//! all threads are joined. The log is evidence-only; nothing like it
//! exists in the product path.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Render loop entered the pause gate check (before any GetBuffer).
    GateCheck,
    /// Render loop published the engagement ack (parking).
    Engaged,
    /// Render loop published the disengagement ack (proceeding).
    Disengaged,
    /// Render loop entered GetBuffer (device buffer about to be held).
    GetBufferEnter,
    /// Render loop released the device buffer.
    ReleaseBuffer,
    /// Render pull returned (payload = frames; None = terminal pull).
    Pull(Option<usize>),
    /// Producer entered a write call.
    ProducerWriteEnter,
    /// Producer exited a write call (false = Stopped outcome).
    ProducerWriteExit(bool),
    /// Producer committed EOF and exited.
    ProducerEof,
    /// Producer failed and exited.
    ProducerFailed,
    /// Pause command recorded.
    PauseRequested,
    /// Resume command recorded.
    ResumeRequested,
    /// Stop released (gate + data plane).
    StopReleased,
    /// Device-level Stop (mechanism B only).
    DeviceStop,
    /// Device-level Start (mechanism B only).
    DeviceStart,
    /// Render loop exited (payload = abort?).
    RenderExited(bool),
    /// Simulated device consumed one period of submitted audio.
    DeviceConsumed(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub elapsed: Duration,
    pub event: Event,
}

#[derive(Clone, Debug)]
pub struct Log {
    inner: Arc<Mutex<Vec<Entry>>>,
    start: Instant,
}

impl Default for Log {
    fn default() -> Self {
        Self::new()
    }
}

impl Log {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
            start: Instant::now(),
        }
    }

    pub fn push(&self, event: Event) {
        self.inner.lock().expect("event log lock").push(Entry {
            elapsed: self.start.elapsed(),
            event,
        });
    }

    /// Snapshot of the entries matching a predicate.
    pub fn matching(&self, f: impl Fn(&Event) -> bool) -> Vec<Entry> {
        self.inner
            .lock()
            .expect("event log lock")
            .iter()
            .filter(|e| f(&e.event))
            .copied()
            .collect()
    }

    pub fn all(&self) -> Vec<Entry> {
        self.inner.lock().expect("event log lock").clone()
    }
}
