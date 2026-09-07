//! Desired composition (Reconcile's input datum) and its revision identity.
//!
//! The desired revision identity is an opaque, equality-only token: the
//! kernel compares it, never interprets or derives it (design §L.5, R1–R8).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// Desired incarnation intent (design §L.5). Its entire semantic content is
/// the equality domain: same token = same incarnation, different token =
/// fresh incarnation requested. `fresh()` is a caller-side convenience; the
/// kernel performs no derivation of any kind.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Revision(u64);

static REVISION_COUNTER: AtomicU64 = AtomicU64::new(1);

impl Revision {
    pub fn new(n: u64) -> Self {
        Self(n)
    }

    /// Operator-side factory for a token distinct from every previous one.
    pub fn fresh() -> Self {
        Self(REVISION_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// One desired-composition entry: component identity, enabled-state, and the
/// desired revision identity (design §L.1). In-memory only; no config files.
#[derive(Clone, Debug)]
pub struct DesiredEntry {
    pub id: String,
    pub component: &'static str,
    pub revision: Revision,
    pub enabled: bool,
}

impl DesiredEntry {
    pub fn enabled(id: &str, component: &'static str, revision: Revision) -> Self {
        Self {
            id: id.to_owned(),
            component,
            revision,
            enabled: true,
        }
    }

    pub fn disabled(id: &str, component: &'static str, revision: Revision) -> Self {
        Self {
            id: id.to_owned(),
            component,
            revision,
            enabled: false,
        }
    }
}

/// Composition/plan errors (design §E.2, §L.4). The plan is refused; the
/// previous composition is kept; ambiguity never becomes runtime state.
#[derive(Clone, Debug)]
pub enum CompositionError {
    UnknownComponent {
        entry: String,
        component: String,
    },
    DuplicateEntry {
        id: String,
    },
    AmbiguousProvider {
        capability: &'static str,
        providers: Vec<String>,
    },
    DependencyCycle {
        capability: &'static str,
    },
}

impl fmt::Display for CompositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompositionError::UnknownComponent { entry, component } => write!(
                f,
                "desired entry '{entry}' names unknown component '{component}'"
            ),
            CompositionError::DuplicateEntry { id } => {
                write!(f, "desired entry id '{id}' appears more than once")
            }
            CompositionError::AmbiguousProvider {
                capability,
                providers,
            } => write!(
                f,
                "capability '{capability}' has {} enabled desired providers ({providers:?}); \
                 required-single forbids ambiguity",
                providers.len()
            ),
            CompositionError::DependencyCycle { capability } => {
                write!(f, "dependency cycle detected via capability '{capability}'")
            }
        }
    }
}

/// The set of plan-refusal reasons from one `set_desired` call.
#[derive(Clone, Debug)]
pub struct CompositionErrors(pub Vec<CompositionError>);

impl fmt::Display for CompositionErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "desired composition refused ({} error(s)):",
            self.0.len()
        )?;
        for e in &self.0 {
            write!(f, " {e};")?;
        }
        Ok(())
    }
}

pub(crate) type DesiredMap = BTreeMap<String, DesiredEntry>;
