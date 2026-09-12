//! Context: the capability/dependency view visible to one fiber — the only
//! door through which a fiber reaches anything it did not create itself
//! (design §D.1). It is a *view*, not a payload bus and not a data store.
//!
//! The two resolution modes (implementation ADR D6, design §E.3/T.5) are
//! visibly distinct operations:
//!
//! - `ActivationCtx::resolve` — new-resolution commitment during activation;
//!   the provider must be ACTIVE (withdrawing providers cannot satisfy it).
//! - `TeardownCtx::resolve_committed` — teardown access through the
//!   episode-fixed committed view, readable through `Unloading` (B14).

use std::rc::Rc;

use crate::capability::{Capability, CapabilityKey, unerase_service};
use crate::component::Discharge;
use crate::fiber::{EffectPayload, EffectProvenance, EffectRecord, FiberId};
use crate::kernel::{CompositionKernel, EffectHandle};

/// Why a resolution attempt was rejected. The undeclared/inactive split is
/// the paper's `UNDECLARED_ACCESS` / `INACTIVE_ACCESS`, adopted semantically
/// (design §E.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResolveError {
    /// The key is outside the component's declared `requires ∪ provides`.
    Undeclared,
    /// Resolution attempted outside an open episode.
    InactiveAccess,
    /// Declared, but the committed view binds no provider.
    Unresolved,
    /// More than one candidate provider for new resolution. K0 never picks.
    Ambiguous,
    /// The fiber already has a live provision of this key.
    AlreadyProvided,
}

/// A resolved capability binding from the committed view: the service value
/// plus the provider fiber identity (structural provenance input, §K.4).
pub struct Binding<K: Capability> {
    service: Rc<K::Service>,
    provider: FiberId,
    provider_name: String,
    key: CapabilityKey,
}

impl<K: Capability> Binding<K> {
    /// Clone of the service value for direct data-plane use. Payload never
    /// flows through the kernel again after this (§K, §N).
    pub fn service(&self) -> Rc<K::Service> {
        self.service.clone()
    }

    /// Diagnostic name of the provider fiber.
    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    pub(crate) fn provider_id(&self) -> FiberId {
        self.provider
    }

    pub(crate) fn key(&self) -> CapabilityKey {
        self.key
    }
}

/// The fiber's door during its one bounded activation step (§F.2/F.3).
/// Exists only while the fiber is `Activating`.
pub struct ActivationCtx<'k> {
    pub(crate) kernel: &'k mut CompositionKernel,
    pub(crate) fiber: FiberId,
    pub(crate) requires: Vec<CapabilityKey>,
    pub(crate) provides: Vec<CapabilityKey>,
}

impl ActivationCtx<'_> {
    fn declared(&self, key: &CapabilityKey) -> bool {
        self.requires.iter().any(|k| k.id == key.id) || self.provides.iter().any(|k| k.id == key.id)
    }

    /// New-resolution commitment (design §E.3). Only ACTIVE providers are
    /// eligible; a withdrawing provider cannot satisfy new resolution.
    pub fn resolve<K: Capability>(&self) -> Result<Binding<K>, ResolveError> {
        let key = CapabilityKey::of::<K>();
        if !self.declared(&key) {
            return Err(ResolveError::Undeclared);
        }
        let view = self
            .kernel
            .fiber(self.fiber)
            .committed
            .as_ref()
            .ok_or(ResolveError::InactiveAccess)?;
        let provider = view.get(&key.id).ok_or(ResolveError::Unresolved)?;
        let erased = self
            .kernel
            .fiber(*provider)
            .provision_service(&key)
            .ok_or(ResolveError::Unresolved)?;
        // Typed storage invariant: provisions are only inserted by
        // `provide::<K>`, so the downcast cannot fail.
        let service = unerase_service::<K>(&erased).ok_or(ResolveError::Unresolved)?;
        Ok(Binding {
            service,
            provider: *provider,
            provider_name: self.kernel.fiber(*provider).name.clone(),
            key,
        })
    }

    /// Install a provision as an owned relation-bearing Effect: the single
    /// authority for that capability binding (design §B4, §K.4).
    pub fn provide<K: Capability>(&mut self, service: Rc<K::Service>) -> Result<(), ResolveError> {
        let key = CapabilityKey::of::<K>();
        if !self.provides.iter().any(|k| k.id == key.id) {
            return Err(ResolveError::Undeclared);
        }
        if self.kernel.fiber(self.fiber).provides_key(&key) {
            return Err(ResolveError::AlreadyProvided);
        }
        let handle = self.kernel.next_handle();
        self.kernel
            .fiber_mut(self.fiber)
            .effects
            .push(EffectRecord {
                handle,
                provenance: EffectProvenance::Relation {
                    key,
                    provider: None,
                },
                payload: EffectPayload::Provision {
                    key,
                    service: crate::capability::erase_service::<K>(service),
                },
            });
        Ok(())
    }

    /// Register an owner-local reversible effect (base triple only, no
    /// fabricated capability key — design §D.4 Corrective-5).
    pub fn register_effect(
        &mut self,
        inverse: impl FnOnce() -> Discharge + 'static,
    ) -> EffectHandle {
        let handle = self.kernel.next_handle();
        self.kernel
            .fiber_mut(self.fiber)
            .effects
            .push(EffectRecord {
                handle,
                provenance: EffectProvenance::OwnerLocal,
                payload: EffectPayload::Inverse(Box::new(inverse)),
            });
        handle
    }

    /// Register a relation-bearing effect (binding / data-edge / cross-fiber
    /// keyed contribution): base triple plus structural provenance naming
    /// the capability key and the provider fiber (design §D.4, §K.4).
    pub fn register_relation<K: Capability>(
        &mut self,
        binding: &Binding<K>,
        inverse: impl FnOnce() -> Discharge + 'static,
    ) -> EffectHandle {
        let handle = self.kernel.next_handle();
        self.kernel
            .fiber_mut(self.fiber)
            .effects
            .push(EffectRecord {
                handle,
                provenance: EffectProvenance::Relation {
                    key: binding.key(),
                    provider: Some(binding.provider_id()),
                },
                payload: EffectPayload::Inverse(Box::new(inverse)),
            });
        handle
    }

    /// Explicit early dispose (B26). Idempotent: disposing an unknown or
    /// already-disposed handle is a no-op. Effects cannot fire after their
    /// episode ends because episode close empties the accumulator.
    pub fn dispose(&mut self, handle: EffectHandle) {
        self.kernel.dispose_effect(self.fiber, handle);
    }
}

/// The fiber's teardown-time door. The committed view stays readable here —
/// this is the window in which a consumer closes provider-backed handles
/// (design §E.3, §G.2).
pub struct TeardownCtx<'k> {
    pub(crate) kernel: &'k mut CompositionKernel,
    pub(crate) fiber: FiberId,
}

impl TeardownCtx<'_> {
    /// Teardown access through the episode-fixed committed view (B14).
    pub fn resolve_committed<K: Capability>(&self) -> Result<Rc<K::Service>, ResolveError> {
        let key = CapabilityKey::of::<K>();
        if !self.kernel.is_declared(self.fiber, &key) {
            return Err(ResolveError::Undeclared);
        }
        let view = self
            .kernel
            .fiber(self.fiber)
            .committed
            .as_ref()
            .ok_or(ResolveError::InactiveAccess)?;
        let provider = view.get(&key.id).ok_or(ResolveError::Unresolved)?;
        let erased = self
            .kernel
            .fiber(*provider)
            .provision_service(&key)
            .ok_or(ResolveError::Unresolved)?;
        // Typed storage invariant as in `resolve`.
        unerase_service::<K>(&erased).ok_or(ResolveError::Unresolved)
    }
}
