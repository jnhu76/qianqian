//! The closed diagnostic surface (design §I.1, §R budget): projections of
//! kernel truth, read-only, payload-free. Anything outside these surfaces is
//! private; anything needing domain session or playback intent is an architecture
//! violation on sight (§J.3).
//!
//! Private fiber generations never appear here; provider identity is
//! compared at component level, up to generation renaming (Lemma 61).

use std::collections::{BTreeMap, BTreeSet};

use crate::fiber::FiberState;

/// Per-fiber lifecycle truth (§I.1 surface 2, including the §G.6 violation
/// flag folded into it — not a separate diagnostic concept).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FiberDiagnostic {
    pub state: FiberState,
    /// A recorded FAILED activation outcome (design §F.5).
    pub failed_outcome: bool,
    /// Latched teardown-contract violation (design §G.6).
    pub teardown_violated: bool,
}

/// One composition-visible relation projected from relation-bearing Effect
/// provenance (§I.1 surfaces 3–5, §K.4 single-authority rule): a live
/// binding / data-edge / cross-fiber contribution owned by `owner` against
/// provider `provider`. Teardown state derives from the owner's lifecycle.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct RelationDiagnostic {
    pub owner: String,
    pub provider: String,
    pub capability: &'static str,
}

/// The closed composition-truth snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionSnapshot {
    /// Fiber name -> lifecycle truth.
    pub fibers: BTreeMap<String, FiberDiagnostic>,
    /// Known capability name -> provider fiber name, or `None` when
    /// unresolvable right now (absent / Pending-only / withdrawing).
    pub capabilities: BTreeMap<String, Option<String>>,
    /// Provision projection: installed fibers currently holding a live
    /// provision effect per capability name, regardless of lifecycle state.
    /// The single-source pointwise invariant (§E.4) reads this: at most one
    /// installed provider per capability at every observable state.
    pub provisions: BTreeMap<String, BTreeSet<String>>,
    /// Live relation-bearing bindings (§K.4 projection).
    pub relations: BTreeSet<RelationDiagnostic>,
    /// The frozen quiescence predicate (§L.1).
    pub quiet: bool,
}
