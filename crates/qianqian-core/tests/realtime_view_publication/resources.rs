//! Synthetic tracked resources and the external observer oracle for the
//! realtime-view publication / reclamation mechanism evidence.
//!
//! Test-only harness (evidence, not normative authority). Nothing here is
//! part of the `qianqian-core` library API.
//!
//! A [`TrackedResource`] is a real object whose lifetime is recorded from
//! *outside* the resource itself: creation, every dereference, and
//! destruction land in a shared [`EventLog`]. There is no cooperative
//! self-report — the resource does not decide what is observed, and a broken
//! mechanism cannot suppress the record.
//!
//! Destruction also flips an external tombstone, so a handle that survives
//! the resource's release produces an observable [`UseAfterRelease`] error on
//! the next dereference. That is the safe-Rust model of a semantic
//! use-after-release witness: the danger is made observable without
//! constructing actual memory unsafety.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// The execution context (thread label) that performed an observed action.
pub type ExecutionContext = String;

/// One externally recorded resource-lifetime event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceEvent {
    Created { id: u64, context: ExecutionContext },
    Dereferenced { id: u64, context: ExecutionContext },
    Destroyed { id: u64, context: ExecutionContext },
}

/// Shared, observer-side append-only record of resource lifetime events.
/// Lives outside every resource and is never written by the mechanism under
/// test; tests read it after the run.
pub type EventLog = Arc<Mutex<Vec<ResourceEvent>>>;

pub fn new_event_log() -> EventLog {
    Arc::new(Mutex::new(Vec::new()))
}

/// A short label for the current execution context (thread name + id).
pub fn current_context() -> ExecutionContext {
    let thread = std::thread::current();
    match thread.name() {
        Some(name) => format!("{name}#{:?}", thread.id()),
        None => format!("unnamed#{:?}", thread.id()),
    }
}

/// Dereference of a resource whose release already happened. This is the
/// observable form of a semantic use-after-release witness: the handle
/// survived the release, and the next dereference is an error rather than
/// undefined behavior.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UseAfterRelease {
    pub id: u64,
}

/// The per-resource observer: an append-only event log plus the destruction
/// tombstone. One observer exists per resource; the resource holds it by
/// shared reference so the tombstone outlives the resource object itself.
#[derive(Debug)]
struct ResourceObserver {
    log: EventLog,
    alive: AtomicBool,
}

/// A synthetic resource whose whole lifetime is externally observed.
///
/// Safe Rust guarantees the object is not destroyed while any strong
/// [`Arc`] handle to it exists; the tombstone exists so that a *weak*
/// residual handle (the stale-bound-handle shape) can observe the release
/// as a real error instead of being silent.
#[derive(Debug)]
pub struct TrackedResource {
    id: u64,
    observer: Arc<ResourceObserver>,
}

impl TrackedResource {
    pub fn new(id: u64, log: EventLog) -> Arc<Self> {
        let observer = Arc::new(ResourceObserver {
            log: log.clone(),
            alive: AtomicBool::new(true),
        });
        let resource = Arc::new(Self { id, observer });
        log.lock()
            .expect("observer log lock")
            .push(ResourceEvent::Created {
                id,
                context: current_context(),
            });
        resource
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// One real dereference of the resource. Fails observably if the
    /// resource has already been released.
    pub fn touch(&self) -> Result<(), UseAfterRelease> {
        if !self.observer.alive.load(Ordering::SeqCst) {
            return Err(UseAfterRelease { id: self.id });
        }
        self.observer
            .log
            .lock()
            .expect("observer log lock")
            .push(ResourceEvent::Dereferenced {
                id: self.id,
                context: current_context(),
            });
        Ok(())
    }

    pub fn is_alive(&self) -> bool {
        self.observer.alive.load(Ordering::SeqCst)
    }
}

impl Drop for TrackedResource {
    fn drop(&mut self) {
        self.observer.alive.store(false, Ordering::SeqCst);
        self.observer
            .log
            .lock()
            .expect("observer log lock")
            .push(ResourceEvent::Destroyed {
                id: self.id,
                context: current_context(),
            });
    }
}
