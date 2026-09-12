//! The mechanism under test: coherent immutable-view publication with
//! per-resource refcounted reclamation.
//!
//! Test-only harness (evidence, not normative authority). Nothing here is
//! part of the `qianqian-audio-api` library API, and nothing here freezes a
//! production publication mechanism.
//!
//! Shape (safe Rust, std only):
//!
//! ```text
//! CONTROL SIDE
//!   build next immutable view            (control code, may allocate)
//!   publish(next)                        mutex-serialized whole-view
//!                                        replacement of the current slot
//!   certify_reclaimable(identity)        move a quiescent retired view
//!                                        from retired to reclaimable
//!   release_reclaimable(identity)        detach the ledger's last reference
//!                                        under the lock, drop it after
//!                                        unlocking -> physical release on
//!                                        the control side, outside the
//!                                        publication critical section
//!
//! REALTIME SIDE
//!   acquire()                            one clone of the current view
//!                                        (bind-time; pre-bound after that)
//!   execute_quantum(...)                 dereferences already-held handles
//!   drop the clone                       release; deferred if it may be last
//! ```
//!
//! The tested reader model holds one pre-bound [`Arc`] clone for a
//! long-lived execution interval, so per-quantum execution touches no
//! mechanism state at all. This acquisition granularity is specific to
//! this test representation and remains architecturally open. Reclamation
//! is per-resource through the resource's own reference count: a resource
//! is destroyed only when no view that references it is still held anywhere
//! (current, retired, reclaimable, or by any reader) — which is exactly the
//! P3 predicate for the tested representation.
//!
//! [`ViewState`] is the test-side readout of the P4 semantic lifecycle
//! (live / retired / reclaimable / released); it is a classification the
//! tests use, not a production requirement.

use std::sync::{Arc, Mutex};

use super::resources::UseAfterRelease;
use super::view::RealtimeView;

/// Semantic lifecycle state of a view (P4 readout).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewState {
    Live,
    Retired,
    Reclaimable,
    Released,
}

/// Everything the mechanism owns, guarded by one lock. Publication,
/// certification and physical release are all single-lock operations on the
/// control side; acquisition takes the same lock once at bind time.
struct Inner {
    current: Arc<RealtimeView>,
    retired: Vec<Arc<RealtimeView>>,
    reclaimable: Vec<Arc<RealtimeView>>,
}

/// The publication / retirement / reclamation mechanism under test.
pub struct PublishedViews {
    inner: Mutex<Inner>,
}

impl PublishedViews {
    /// Creates the mechanism with an initial published view.
    pub fn new(initial: Arc<RealtimeView>) -> Self {
        Self {
            inner: Mutex::new(Inner {
                current: initial,
                retired: Vec::new(),
                reclaimable: Vec::new(),
            }),
        }
    }

    /// Realtime-view publication: retires the old view (moves it into the
    /// retirement ledger) and replaces the whole immutable view as one
    /// mutex-guarded operation. Publication itself never touches resources
    /// and never waits for readers.
    pub fn publish(&self, next: Arc<RealtimeView>) {
        let mut inner = self.inner.lock().expect("publication lock");
        let old = std::mem::replace(&mut inner.current, next);
        inner.retired.push(old);
    }

    /// Publication with validation: if the next view fails validation, the
    /// current published view is left untouched (P1's engineering form).
    pub fn publish_checked(&self, next: Arc<RealtimeView>) -> Result<(), String> {
        let mut inner = self.inner.lock().expect("publication lock");
        if next.resources().is_empty() {
            return Err("a published view must carry a dereferenceable surface".to_string());
        }
        if next.topology_version() < inner.current.topology_version() {
            return Err("publication must not move topology backwards".to_string());
        }
        let old = std::mem::replace(&mut inner.current, next);
        inner.retired.push(old);
        Ok(())
    }

    /// Acquisition (bind-time): one clone of the current view. The returned
    /// view is pre-bound; after this call the reader executes through held
    /// handles and never touches the mechanism again.
    pub fn acquire(&self) -> Arc<RealtimeView> {
        self.inner.lock().expect("publication lock").current.clone()
    }

    pub fn current_identity(&self) -> u64 {
        self.inner
            .lock()
            .expect("publication lock")
            .current
            .identity()
    }

    pub fn retired_identities(&self) -> Vec<u64> {
        self.inner
            .lock()
            .expect("publication lock")
            .retired
            .iter()
            .map(|v| v.identity())
            .collect()
    }

    pub fn reclaimable_identities(&self) -> Vec<u64> {
        self.inner
            .lock()
            .expect("publication lock")
            .reclaimable
            .iter()
            .map(|v| v.identity())
            .collect()
    }

    pub fn state_of(&self, identity: u64) -> ViewState {
        let inner = self.inner.lock().expect("publication lock");
        if inner.current.identity() == identity {
            ViewState::Live
        } else if inner.retired.iter().any(|v| v.identity() == identity) {
            ViewState::Retired
        } else if inner.reclaimable.iter().any(|v| v.identity() == identity) {
            ViewState::Reclaimable
        } else {
            ViewState::Released
        }
    }

    /// Number of reader-held strong references to `view` (excluding the
    /// mechanism's own ledger reference). This is the quiescence readout:
    /// zero means no active execution and no queued reference can still
    /// dereference the view through any generation.
    pub fn reader_hold_count(&self, view: &Arc<RealtimeView>) -> usize {
        Arc::strong_count(view).saturating_sub(1)
    }

    /// Quiescence predicate for one retired view: no reader (active or
    /// queued) holds it. Because the reference count is per-`Arc`, this
    /// inherently covers every generation that references the same view —
    /// there is no per-generation accounting that could forget an older one.
    pub fn view_quiescent(&self, view: &Arc<RealtimeView>) -> bool {
        Arc::strong_count(view) == 1
    }

    /// Quiescence predicate resolved by view identity, for certification
    /// logic that only has the identity.
    pub fn is_quiescent_identity(&self, identity: u64) -> bool {
        let inner = self.inner.lock().expect("publication lock");
        inner
            .retired
            .iter()
            .find(|v| v.identity() == identity)
            .is_some_and(|v| Arc::strong_count(v) == 1)
    }

    /// Quiescence certification: moves a retired view to the reclaimable
    /// ledger, but only when its reader count is zero. Returns whether the
    /// certification happened. This is a semantic state change only — no
    /// physical release occurs here.
    pub fn certify_reclaimable(&self, identity: u64) -> bool {
        let mut inner = self.inner.lock().expect("publication lock");
        let position = inner.retired.iter().position(|v| v.identity() == identity);
        let Some(position) = position else {
            return false;
        };
        let view = &inner.retired[position];
        if !self.view_quiescent(view) {
            return false;
        }
        let view = inner.retired.remove(position);
        inner.reclaimable.push(view);
        true
    }

    /// Physical release of a reclaimable view: detaches the ledger's last
    /// reference under the publication lock and drops it only after the lock
    /// is released, so the view (and any resource no longer referenced by any
    /// other view or reader) is destroyed here, on the caller's thread, but
    /// **outside the publication critical section**.
    pub fn release_reclaimable(&self, identity: u64) -> bool {
        let released = {
            let mut inner = self.inner.lock().expect("publication lock");
            let position = inner
                .reclaimable
                .iter()
                .position(|v| v.identity() == identity);
            let Some(position) = position else {
                return false;
            };
            inner.reclaimable.remove(position)
        };
        drop(released);
        true
    }
}

/// Convenience wrapper for the honest reader shape in scenario tests.
/// A reader holds one pre-bound view and may execute quanta; dropping the
/// reader releases its reference in the current execution context.
pub struct Reader {
    view: Arc<RealtimeView>,
}

impl Reader {
    pub fn new(view: Arc<RealtimeView>) -> Self {
        Self { view }
    }

    pub fn view_identity(&self) -> u64 {
        self.view.identity()
    }

    pub fn view(&self) -> &Arc<RealtimeView> {
        &self.view
    }

    pub fn execute(&self, quanta: usize) -> Result<(), UseAfterRelease> {
        self.view.execute_quantum(quanta)
    }
}
