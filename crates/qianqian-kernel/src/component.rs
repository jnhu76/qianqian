//! Component definitions (the paper's static `(d, p, e)`): what a fiber
//! instance is made of. A component declares required/provided capability
//! keys, one bounded activation step, and one teardown closure whose verdict
//! is the kernel's entire teardown knowledge (design §F.1, §G.6, §D.6).

use std::rc::Rc;

use crate::capability::CapabilityKey;
use crate::context::{ActivationCtx, TeardownCtx};

/// The frozen one-verdict teardown truth (design §G.6). There is no partial
/// outcome: a `Violated` verdict is the §G.6 latch, never a result the
/// runtime continues past.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Discharge {
    Discharged,
    Violated,
}

/// Opaque pending activation error (episode metadata, design §F.5). The
/// kernel knows that a fiber failed, never the domain why (§J.3).
#[derive(Clone, Debug)]
pub struct ActivationError {
    pub message: String,
}

impl ActivationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

type ActivateFn = dyn Fn(&mut ActivationCtx<'_>) -> Result<(), ActivationError>;
type TeardownFn = dyn Fn(&mut TeardownCtx<'_>) -> Discharge;

/// Static, named, reusable component definition.
#[derive(Clone)]
pub struct ComponentSpec {
    pub(crate) name: &'static str,
    pub(crate) requires: Vec<CapabilityKey>,
    pub(crate) provides: Vec<CapabilityKey>,
    pub(crate) activate: Rc<ActivateFn>,
    pub(crate) teardown: Rc<TeardownFn>,
}

impl ComponentSpec {
    /// A component with no declarations and no-op activation/teardown.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            requires: Vec::new(),
            provides: Vec::new(),
            activate: Rc::new(|_| Ok(())),
            teardown: Rc::new(|_| Discharge::Discharged),
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Declare a required capability (consumer side of required-single).
    pub fn requires<K: crate::capability::Capability>(mut self) -> Self {
        let key = CapabilityKey::of::<K>();
        if !self.requires.iter().any(|k| k.id == key.id) {
            self.requires.push(key);
        }
        self
    }

    /// Declare a provided capability (provider side). The activation must
    /// install every declared provision on completion (design §L.4 totality).
    pub fn provides<K: crate::capability::Capability>(mut self) -> Self {
        let key = CapabilityKey::of::<K>();
        if !self.provides.iter().any(|k| k.id == key.id) {
            self.provides.push(key);
        }
        self
    }

    /// The one bounded activation step (design §F.2). `Err` is a raise.
    pub fn on_activate(
        mut self,
        f: impl Fn(&mut ActivationCtx<'_>) -> Result<(), ActivationError> + 'static,
    ) -> Self {
        self.activate = Rc::new(f);
        self
    }

    /// Discharge of the component's declared domain teardown obligations.
    /// The kernel observes only the verdict (design §H.5.1 fence, §G.6).
    /// Only invoked when an activated episode deactivates — never after a
    /// raised activation (implementation ADR D8).
    pub fn on_teardown(mut self, f: impl Fn(&mut TeardownCtx<'_>) -> Discharge + 'static) -> Self {
        self.teardown = Rc::new(f);
        self
    }
}
