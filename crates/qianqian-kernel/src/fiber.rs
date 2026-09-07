//! Fiber internals: identity, lifecycle state, and the owned-effect
//! accumulator (the paper's concrete accumulator, design §D.3, §D.4, §F.2).
//!
//! Private identity is generational (implementation ADR D3); generation
//! values are never observational truth and never appear in diagnostics.

use std::any::TypeId;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::capability::CapabilityKey;
use crate::component::{ActivationError, Discharge};
use crate::desired::Revision;

/// Private generational fiber identity. Never exposed in diagnostics.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct FiberId {
    pub(crate) idx: u32,
    pub(crate) generation: u32,
}

/// Observable lifecycle vocabulary, exactly the frozen 7-state family with
/// `Absent` = not installed in the registry (design §F.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FiberState {
    Pending,
    Activating,
    Active,
    Unloading,
    Failed,
}

/// Structural provenance of one Effect (implementation ADR D5). This is the
/// frozen *conditional structural* field of the one Effect shape — present
/// only on relation-bearing effects — never a behavioral class: there is no
/// `EffectKind`/`EffectClass` and no `Option<Disposer>` (design §H.5, §D.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum EffectProvenance {
    /// Timer/watcher/local-handle/buffer class: carries no capability key
    /// and no fabricated relation (design §D.4, Corrective-5).
    OwnerLocal,
    /// Capability provision (`provider: None` — the provider is the owner),
    /// or a binding / data-edge / cross-fiber keyed contribution
    /// (`provider: Some` — the other fiber end of the relation, §K.4).
    Relation {
        key: CapabilityKey,
        provider: Option<FiberId>,
    },
}

pub(crate) enum EffectPayload {
    /// A provision: the single authority for that capability binding;
    /// owns the erased service value. Its inverse is structural removal.
    Provision {
        key: CapabilityKey,
        service: Rc<dyn std::any::Any>,
    },
    /// A total semantic obligation (design §H.7): once invoked it either
    /// discharges or latches §G.6 — there is no partial-success outcome.
    Inverse(Box<dyn FnOnce() -> Discharge>),
}

/// One Effect record: base triple (owner episode = the accumulator it lives
/// in; total inverse; LIFO position = the stack itself) plus the conditional
/// structural provenance (§D.4, implementation ADR D5).
pub(crate) struct EffectRecord {
    pub(crate) handle: crate::kernel::EffectHandle,
    pub(crate) provenance: EffectProvenance,
    pub(crate) payload: EffectPayload,
}

impl EffectRecord {
    /// Structural composition relation, if this effect bears one.
    pub(crate) fn relation(&self) -> Option<(CapabilityKey, Option<FiberId>)> {
        match &self.provenance {
            EffectProvenance::OwnerLocal => None,
            EffectProvenance::Relation { key, provider } => Some((*key, *provider)),
        }
    }
}

/// One live component instance: the unit of composition, ownership and
/// lifetime (design §D.3).
pub(crate) struct Fiber {
    pub(crate) id: FiberId,
    /// Observable diagnostic name = desired entry id (ADR D3). Unique among
    /// installed fibers; reusable only after removal (design §F.1).
    pub(crate) name: String,
    pub(crate) component: &'static str,
    pub(crate) revision: Revision,
    pub(crate) retired: bool,
    pub(crate) state: FiberState,
    /// Latched §G.6 condition — a diagnostic flag on an `Unloading` fiber,
    /// never an eighth lifecycle state (design §F.2 Corrective-2 note).
    pub(crate) teardown_violated: bool,
    /// Pending activation error as episode metadata (design §F.5). Becomes
    /// the FAILED outcome only via a fully discharged unwind.
    pub(crate) pending_error: Option<ActivationError>,
    /// Episode-fixed committed view: required capability key -> provider
    /// fiber. Written at activation start; readable through teardown (B14).
    pub(crate) committed: Option<BTreeMap<TypeId, FiberId>>,
    /// The owned-effect accumulator; unwound strictly LIFO (§H.1).
    pub(crate) effects: Vec<EffectRecord>,
}

impl Fiber {
    pub(crate) fn provides_key(&self, key: &CapabilityKey) -> bool {
        self.effects.iter().any(|e| {
            matches!(
                &e.payload,
                EffectPayload::Provision { key: k, .. } if k.id == key.id
            )
        })
    }

    pub(crate) fn provision_service(&self, key: &CapabilityKey) -> Option<Rc<dyn std::any::Any>> {
        self.effects.iter().find_map(|e| match &e.payload {
            EffectPayload::Provision { key: k, service } if k.id == key.id => Some(service.clone()),
            _ => None,
        })
    }
}
