//! Component definitions (the paper's static `(d, p, e)`): what a fiber
//! instance is made of. A component declares required/provided capability
//! keys, one bounded activation step, and one teardown closure whose verdict
//! is the kernel's entire teardown knowledge (design §F.1, §G.6, §D.6).

use std::fmt;

use std::rc::Rc;

use crate::capability::CapabilityKey;
use crate::context::{ActivationCtx, TeardownCtx};

/// Why `CompositionKernel::register_component` refused a component definition. A
/// refusal never mutates the catalog: the previous definition (or the
/// previous diagnostic-name universe) is kept exactly as it was.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ComponentRegistrationError {
    /// A component definition with this name is already registered.
    /// Component definitions are the paper's static `(d, p, e)` (design
    /// §F.1), fixed for the kernel's lifetime: re-registration would let
    /// mounted fibers silently run a different definition at unload time —
    /// component hot replacement, which K0 does not have.
    DuplicateName { name: &'static str },
    /// Two distinct capability types in this kernel declare the same
    /// diagnostic `NAME`. Diagnostic identity is the `TypeId` (design §E.1);
    /// `NAME` is vocabulary, but the §I.1 surfaces key capability maps by
    /// it — a collision would merge distinct capabilities there and let
    /// single-source oracles lie. Rename one of the capabilities.
    DuplicateCapabilityName { name: &'static str },
}

impl fmt::Display for ComponentRegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateName { name } => write!(
                f,
                "component definition '{name}' is already registered; definitions are \
                 static for the kernel's lifetime (design §F.1) — re-registration would \
                 silently mutate mounted fibers (no component hot replacement in K0)"
            ),
            Self::DuplicateCapabilityName { name } => write!(
                f,
                "capability diagnostic name '{name}' is already declared by a distinct \
                 capability type in this kernel; §I.1 surfaces key by NAME (design §E.1) \
                 — rename one of the capabilities"
            ),
        }
    }
}

impl std::error::Error for ComponentRegistrationError {}

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
