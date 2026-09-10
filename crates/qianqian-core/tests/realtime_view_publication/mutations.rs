//! Adversarial twins for the realtime-view publication / reclamation
//! mechanism evidence.
//!
//! Test-only. Every twin is a real executable mechanism variant with a
//! deliberately introduced defect; every kill test reads its oracle from
//! execution state (a failed dereference, a mixed-generation observation, a
//! reclaimable-with-holder certification, a blocked realtime reader) —
//! never from the twin's self-report.
//!
//! The twin set mirrors the formal model's mutation classes without carrying
//! model filenames or numbers into the code: release before quiescence,
//! split publication, stale entry, latest-retired-only reclamation, and
//! queued-reference forgetting. The mapping table lives in the evidence
//! record, not here.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, Weak};

use super::publication::PublishedViews;
use super::resources::{TrackedResource, UseAfterRelease};
use super::view::RealtimeView;

// ---------------------------------------------------------------------------
// The weak-handle reader shape (decoupled lifetime)
// ---------------------------------------------------------------------------

/// A reader that holds only weak residual handles to a view's resources and
/// re-resolves each dereference. This is the stale-bound-handle shape: the
/// handle survives the resource's release, so release is observable as a
/// failed upgrade instead of being silent. The honest mechanism's readers
/// hold strong pre-bound clones and never hit this path.
pub struct WeakHandlesReader {
    identity: u64,
    handles: Vec<(u64, Weak<TrackedResource>)>,
}

impl WeakHandlesReader {
    pub fn from_view(view: &Arc<RealtimeView>) -> Self {
        Self {
            identity: view.identity(),
            handles: view
                .resources()
                .iter()
                .map(|r| (r.id(), Arc::downgrade(r)))
                .collect(),
        }
    }

    pub fn view_identity(&self) -> u64 {
        self.identity
    }

    /// Dereferences every held handle once per quantum. A handle whose
    /// resource was released fails to upgrade — the observable form of a
    /// use-after-release witness.
    pub fn execute_quantum(&self, quanta: usize) -> Result<(), UseAfterRelease> {
        for _ in 0..quanta {
            for (id, handle) in &self.handles {
                let resource = handle.upgrade().ok_or(UseAfterRelease { id: *id })?;
                resource.touch()?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Twin: release at publication (no retirement ledger, no certification)
// ---------------------------------------------------------------------------

/// Publishes by swapping the slot and immediately dropping the old view:
/// resources become eligible for destruction the moment a newer view is
/// published, with no quiescence certification. Any reader still holding
/// dereferenceable state through a weak handle observes release as an error.
pub struct ReleaseAtPublication {
    slot: Mutex<Arc<RealtimeView>>,
}

impl ReleaseAtPublication {
    pub fn new(initial: Arc<RealtimeView>) -> Self {
        Self {
            slot: Mutex::new(initial),
        }
    }

    pub fn publish(&self, next: Arc<RealtimeView>) {
        let mut guard = self.slot.lock().expect("publication lock");
        let old = std::mem::replace(&mut *guard, next);
        // The defect: release at publication, no retirement ledger.
        drop(old);
    }

    pub fn acquire(&self) -> Arc<RealtimeView> {
        self.slot.lock().expect("publication lock").clone()
    }
}

// ---------------------------------------------------------------------------
// Twin: split publication (topology and resource table published separately)
// ---------------------------------------------------------------------------

/// What a reader of the split twin observes in one acquisition.
#[derive(Debug)]
pub struct SplitViewObservation {
    pub topology_identity: u64,
    pub topology_version: u64,
    pub participants: Vec<&'static str>,
    pub resource_identity: u64,
    pub resources: Vec<Arc<TrackedResource>>,
}

impl SplitViewObservation {
    /// Whether the observed halves come from the same view generation.
    pub fn is_coherent(&self) -> bool {
        self.topology_identity == self.resource_identity
    }
}

/// Publishes a view in two uncoordinated steps — topology half first,
/// resource table second. A reader acquiring between the steps observes a
/// mixed-generation view (the split-publication defect).
pub struct SplitPublication {
    topology_half: Mutex<(u64, u64, Vec<&'static str>)>,
    resources_half: Mutex<(u64, Vec<Arc<TrackedResource>>)>,
}

impl SplitPublication {
    pub fn new(view: &RealtimeView) -> Self {
        Self {
            topology_half: Mutex::new((
                view.identity(),
                view.topology_version(),
                view.participants().to_vec(),
            )),
            resources_half: Mutex::new((view.identity(), view.resources().to_vec())),
        }
    }

    pub fn publish_topology_step(&self, view: &RealtimeView) {
        *self.topology_half.lock().expect("topology half lock") = (
            view.identity(),
            view.topology_version(),
            view.participants().to_vec(),
        );
    }

    pub fn publish_resources_step(&self, view: &RealtimeView) {
        *self.resources_half.lock().expect("resources half lock") =
            (view.identity(), view.resources().to_vec());
    }

    /// One acquisition: reads the topology half, then the resource half.
    /// Between the two reads another step may have landed, producing a
    /// mixed-generation observation.
    pub fn acquire(&self) -> SplitViewObservation {
        let (topology_identity, topology_version, participants) = self
            .topology_half
            .lock()
            .expect("topology half lock")
            .clone();
        let (resource_identity, resources) = self
            .resources_half
            .lock()
            .expect("resources half lock")
            .clone();
        SplitViewObservation {
            topology_identity,
            topology_version,
            participants,
            resource_identity,
            resources,
        }
    }
}

// ---------------------------------------------------------------------------
// Twin: stale acquisition (retired views stay acquirable)
// ---------------------------------------------------------------------------

/// Acquisition defect: a view can be resolved by a previously observed
/// identity without revalidating that it is still the published view, so a
/// new acquisition can enter a retired view.
pub struct StaleAcquisition {
    current: Mutex<u64>,
    views: Mutex<HashMap<u64, Arc<RealtimeView>>>,
}

impl StaleAcquisition {
    pub fn new(initial: Arc<RealtimeView>) -> Self {
        let identity = initial.identity();
        let mut views = HashMap::new();
        views.insert(identity, initial);
        Self {
            current: Mutex::new(identity),
            views: Mutex::new(views),
        }
    }

    pub fn publish(&self, view: Arc<RealtimeView>) {
        self.views
            .lock()
            .expect("view map lock")
            .insert(view.identity(), view.clone());
        *self.current.lock().expect("current lock") = view.identity();
    }

    pub fn acquire(&self) -> Arc<RealtimeView> {
        let identity = *self.current.lock().expect("current lock");
        self.views
            .lock()
            .expect("view map lock")
            .get(&identity)
            .expect("the current view is registered")
            .clone()
    }

    /// The defect: resolves a cached identity without checking closure, so
    /// it can hand out a retired view as if it were newly acquired.
    pub fn acquire_stale(&self, cached_identity: u64) -> Option<Arc<RealtimeView>> {
        self.views
            .lock()
            .expect("view map lock")
            .get(&cached_identity)
            .cloned()
    }
}

// ---------------------------------------------------------------------------
// Twins: certification predicate defects (reclaimable-with-holder)
// ---------------------------------------------------------------------------

/// Which certification shortcut the twin takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CertificationBug {
    /// Certifies every retired view whenever the *latest* retired view is
    /// quiescent, forgetting older generations' readers.
    LatestRetiredOnly,
    /// Certifies every retired view with no *actively executing* reader,
    /// forgetting acquired-but-queued references.
    ActiveExecutionsOnly,
}

/// Certification twin: a control-side certification logic with a defect,
/// layered over the honest mechanism. It records the identities it claims
/// are reclaimable; the oracle then checks whether any claim contradicts the
/// mechanism's actual reader-hold counts.
pub struct CertificationTwin {
    inner: PublishedViews,
    bug: CertificationBug,
    certified_claims: Mutex<Vec<u64>>,
    active_executions: Mutex<HashSet<u64>>,
}

impl CertificationTwin {
    pub fn new(inner: PublishedViews, bug: CertificationBug) -> Self {
        Self {
            inner,
            bug,
            certified_claims: Mutex::new(Vec::new()),
            active_executions: Mutex::new(HashSet::new()),
        }
    }

    pub fn begin_execution(&self, identity: u64) {
        self.active_executions
            .lock()
            .expect("execution tracker lock")
            .insert(identity);
    }

    pub fn end_execution(&self, identity: u64) {
        self.active_executions
            .lock()
            .expect("execution tracker lock")
            .remove(&identity);
    }

    /// Runs the defective certification and returns the identities it
    /// claimed reclaimable.
    pub fn certify_all(&self) -> Vec<u64> {
        let retired = self.inner.retired_identities();
        let claims: Vec<u64> = match self.bug {
            CertificationBug::LatestRetiredOnly => {
                let latest = retired.last().copied();
                if latest.is_some_and(|id| self.inner.is_quiescent_identity(id)) {
                    retired
                } else {
                    Vec::new()
                }
            }
            CertificationBug::ActiveExecutionsOnly => {
                let active = self
                    .active_executions
                    .lock()
                    .expect("execution tracker lock")
                    .clone();
                retired
                    .into_iter()
                    .filter(|id| !active.contains(id))
                    .collect()
            }
        };
        self.certified_claims
            .lock()
            .expect("claims lock")
            .extend(claims.iter().copied());
        claims
    }

    pub fn certified_claims(&self) -> Vec<u64> {
        self.certified_claims.lock().expect("claims lock").clone()
    }

    pub fn inner(&self) -> &PublishedViews {
        &self.inner
    }
}

// ---------------------------------------------------------------------------
// Twin: publisher waits for readers while holding the shared lock
// ---------------------------------------------------------------------------

/// Publication defect: the publisher waits for an exiting realtime reader
/// while still holding the shared slot lock, so a reader that needs the
/// lock (a bind-time acquisition) is blocked by the control side.
pub struct PublisherWaitsForReader {
    slot: Mutex<Arc<RealtimeView>>,
}

impl PublisherWaitsForReader {
    pub fn new(initial: Arc<RealtimeView>) -> Self {
        Self {
            slot: Mutex::new(initial),
        }
    }

    /// Publishes while holding the slot lock and waiting for `reader_exit`.
    /// Signals `lock_held` only after the lock is taken, so a test can
    /// deterministically observe that acquisition is blocked meanwhile.
    pub fn publish_blocking(
        &self,
        next: Arc<RealtimeView>,
        lock_held: std::sync::mpsc::Sender<()>,
        reader_exit: Receiver<()>,
    ) {
        let mut guard = self.slot.lock().expect("publication lock");
        let _ = lock_held.send(());
        let old = std::mem::replace(&mut *guard, next);
        // The defect: wait for the reader while the lock is held.
        let _ = reader_exit.recv();
        drop(old);
    }

    pub fn acquire(&self) -> Arc<RealtimeView> {
        self.slot.lock().expect("publication lock").clone()
    }
}
