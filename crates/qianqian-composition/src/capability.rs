//! Capability identity and service storage (implementation ADR D2).
//!
//! A capability identifies a service contract that can be required and
//! provided. Identity is the contract definition site (`TypeId` of the key
//! type), never the provider, the service object address, or any payload.

use std::any::{Any, TypeId};
use std::rc::Rc;

/// Identity of a service contract that can be required and provided.
///
/// K0 cardinality is required-single (design §E.2). The `NAME` is diagnostic
/// vocabulary only and never participates in identity or equality.
pub trait Capability: 'static {
    const NAME: &'static str;
    type Service: ?Sized + 'static;
}

/// Typed capability key: identity plus the diagnostic name.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CapabilityKey {
    pub(crate) id: TypeId,
    pub name: &'static str,
}

impl CapabilityKey {
    pub fn of<K: Capability>() -> Self {
        Self {
            id: TypeId::of::<K>(),
            name: K::NAME,
        }
    }
}

/// Sized wrapper making an object-safe service storable as `Rc<dyn Any>`
/// and recoverable as `Rc<K::Service>` at resolution.
pub(crate) struct ServiceValue<K: ?Sized + 'static> {
    pub(crate) service: Rc<K>,
}

pub(crate) fn erase_service<K: Capability>(service: Rc<K::Service>) -> Rc<dyn Any> {
    Rc::new(ServiceValue { service })
}

pub(crate) fn unerase_service<K: Capability>(erased: &Rc<dyn Any>) -> Option<Rc<K::Service>> {
    erased
        .clone()
        .downcast::<ServiceValue<K::Service>>()
        .ok()
        .map(|v| v.service.clone())
}
