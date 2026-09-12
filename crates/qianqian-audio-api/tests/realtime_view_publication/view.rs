//! The immutable realtime view published by the mechanism.
//!
//! Test-only harness (evidence, not normative authority). A [`RealtimeView`]
//! is the minimal pre-bound direct-flow-like shape: an identity, a topology
//! label, the participant set it binds, and the tracked resources it may
//! dereference. It is immutable once built — publication swaps whole views,
//! never fields of a live view (that is what makes coherent publication
//! expressible at all).
//!
//! The actual PCM participant bindings are the direct-flow experiment's
//! evidence (a separate rung); here the dereferenceable surface is modeled
//! by tracked resources so that every dereference is externally observable.
//! No playback semantics of any kind are introduced.

use std::sync::Arc;

use super::resources::{TrackedResource, UseAfterRelease};

/// A published realtime view: everything a realtime reader needs to execute
/// one or more quanta without entering composition or control machinery.
///
/// `identity` is a fresh monotonic view identity; `topology_version` labels
/// the participant topology generation; `participants` names the bound
/// participants; `resources` is the dereferenceable surface (participant
/// handles and buffers modeled as tracked resources). A view is immutable
/// after construction.
#[derive(Debug)]
pub struct RealtimeView {
    identity: u64,
    topology_version: u64,
    participants: Vec<&'static str>,
    resources: Vec<Arc<TrackedResource>>,
}

impl RealtimeView {
    pub fn new(
        identity: u64,
        topology_version: u64,
        participants: Vec<&'static str>,
        resources: Vec<Arc<TrackedResource>>,
    ) -> Self {
        Self {
            identity,
            topology_version,
            participants,
            resources,
        }
    }

    pub fn identity(&self) -> u64 {
        self.identity
    }

    pub fn topology_version(&self) -> u64 {
        self.topology_version
    }

    pub fn participants(&self) -> &[&'static str] {
        &self.participants
    }

    pub fn has_participant(&self, name: &str) -> bool {
        self.participants.contains(&name)
    }

    /// The dereferenceable surface: every resource a reader may touch while
    /// executing through this view.
    pub fn resources(&self) -> &[Arc<TrackedResource>] {
        &self.resources
    }

    pub fn resource_ids(&self) -> Vec<u64> {
        self.resources.iter().map(|r| r.id()).collect()
    }

    /// Executes one or more quanta through this view by dereferencing its
    /// pre-bound surface. This is the reader's realtime path: it touches
    /// only already-held handles and performs no mechanism operation.
    pub fn execute_quantum(&self, quanta: usize) -> Result<(), UseAfterRelease> {
        for _ in 0..quanta {
            for resource in &self.resources {
                resource.touch()?;
            }
        }
        Ok(())
    }
}
