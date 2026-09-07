//! The Composition Kernel engine: registry truth, lifecycle rules, and
//! Reconcile (design §F.3, §G, §E.4, §L).
//!
//! The control plane is synchronous and serialized (§N, B20): every public
//! operation runs on the caller's thread; `step` performs at most one
//! enabled transition and `settle` drives to quiescence or a blocked state.

use std::any::TypeId;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::capability::CapabilityKey;
use crate::component::{ActivationError, ComponentRegistrationError, ComponentSpec, Discharge};
use crate::context::{ActivationCtx, TeardownCtx};
use crate::desired::{CompositionError, CompositionErrors, DesiredEntry, DesiredMap};
use crate::diagnostic::{CompositionSnapshot, FiberDiagnostic, RelationDiagnostic};
use crate::fiber::{EffectPayload, Fiber, FiberId, FiberState};

/// Handle for explicit early dispose of an owned effect (B26). Handles are
/// private identities; disposing twice is an idempotent no-op and handles
/// never survive their episode with observable meaning.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct EffectHandle(u64);

/// What one orchestration/lifecycle step did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepOutcome {
    /// One enabled transition was performed.
    Transitioned,
    /// No transition is enabled and nothing is latched: quiescent.
    Settled,
    /// No further transition is legal (a §G.6 violation is latched): the run
    /// is deliberately, loudly blocked (design §G.6).
    Blocked,
}

struct Slot {
    generation: u32,
    fiber: Option<Fiber>,
}

/// The generic Composition Kernel. Knows Context, Capability, Fiber, Effect
/// and Reconcile — nothing else (constitution; primitive budget §D.6/§R).
pub struct Kernel {
    catalog: BTreeMap<&'static str, ComponentSpec>,
    desired: DesiredMap,
    slots: Vec<Slot>,
    effect_handles: u64,
    /// Executable RT-firewall witness (implementation ADR D9): counts every
    /// public kernel operation so fixtures can prove a payload loop performs
    /// zero kernel operations. Not a diagnostic surface.
    ops: Cell<u64>,
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}

impl Kernel {
    pub fn new() -> Self {
        Self {
            catalog: BTreeMap::new(),
            desired: BTreeMap::new(),
            slots: Vec::new(),
            effect_handles: 0,
            ops: Cell::new(0),
        }
    }

    /// Register a component definition — the paper's static `(d, p, e)`
    /// (design §F.1). Component definitions are immutable for the kernel's
    /// lifetime: a name may be registered at most once (K0 has no component
    /// hot replacement), and the diagnostic `NAME`s of declared capabilities
    /// must be unique within the kernel (design §E.1: identity is the
    /// `TypeId`, `NAME` is the vocabulary the §I.1 surfaces key by). On
    /// refusal nothing is changed; the reason is returned.
    pub fn register_component(
        &mut self,
        spec: ComponentSpec,
    ) -> Result<(), ComponentRegistrationError> {
        self.count_op();
        if self.catalog.contains_key(spec.name()) {
            return Err(ComponentRegistrationError::DuplicateName { name: spec.name() });
        }
        // Per-kernel diagnostic-name uniqueness: two distinct capability
        // types sharing a NAME would merge in the name-keyed snapshot maps
        // and let the single-source oracle lie. Same TYPEID + same NAME is
        // the same contract, always fine.
        let new_keys: Vec<CapabilityKey> = spec
            .requires
            .iter()
            .chain(spec.provides.iter())
            .copied()
            .collect();
        for (i, a) in new_keys.iter().enumerate() {
            for b in new_keys.iter().skip(i + 1).chain(
                self.catalog
                    .values()
                    .flat_map(|s| s.requires.iter().chain(s.provides.iter())),
            ) {
                if a.id != b.id && a.name == b.name {
                    return Err(ComponentRegistrationError::DuplicateCapabilityName {
                        name: a.name,
                    });
                }
            }
        }
        self.catalog.insert(spec.name(), spec);
        Ok(())
    }

    /// Install a desired composition. Plan-time checks (§L.4) run first: on
    /// any composition error the plan is refused and the previous desired
    /// composition is kept. Diffing happens lazily in `step`/`settle`.
    pub fn set_desired(&mut self, entries: Vec<DesiredEntry>) -> Result<(), CompositionErrors> {
        self.count_op();
        let map = self.validate(&entries)?;
        self.desired = map;
        Ok(())
    }

    /// Perform at most one enabled transition (§F.3 rules; §E.4 staging is
    /// directly observable because each staging boundary is its own step).
    pub fn step(&mut self) -> StepOutcome {
        self.count_op();
        // 1. An eligible unload (L-Unload is guarded by ¬relied, Thm 70/73).
        if let Some(fid) = self.eligible_unload() {
            self.unload_fiber(fid);
            return StepOutcome::Transitioned;
        }
        // 2. Divert / retire an Active fiber whose target moved (L-Leave).
        if let Some(fid) = self.divert_candidate() {
            self.fiber_mut(fid).state = FiberState::Unloading;
            return StepOutcome::Transitioned;
        }
        // 3. Retire a running generation whose desired entry changed or
        //    vanished (revision / orphan / disabled — §L.2, §L.5).
        if let Some(fid) = self.retire_mismatch_candidate() {
            self.fiber_mut(fid).retired = true;
            return StepOutcome::Transitioned;
        }
        // 4. Remove a drained retired fiber (O-Remove; Thm 64 / Cor 69).
        if let Some(idx) = self.removal_candidate() {
            let slot = &mut self.slots[idx];
            slot.fiber = None;
            slot.generation = slot.generation.wrapping_add(1);
            return StepOutcome::Transitioned;
        }
        // 5. Mount an absent enabled desired entry (O-Insert). Only now —
        //    after the old generation's removal — is a replacement inserted
        //    (staged replacement, §E.4 steps 6–7).
        if let Some(entry) = self.mount_candidate() {
            self.mount_fiber(entry);
            return StepOutcome::Transitioned;
        }
        // 6. Activate an eligible Pending fiber (L-Begin: committed view
        //    frozen; one bounded run of e; §F.3).
        if let Some(fid) = self.activation_candidate() {
            self.activate_fiber(fid);
            return StepOutcome::Transitioned;
        }
        if self.any_violation() {
            StepOutcome::Blocked
        } else {
            StepOutcome::Settled
        }
    }

    /// Drive the control plane to quiescence or to a blocked state
    /// (§L.1; termination under L.4 preconditions, Thm 73).
    pub fn settle(&mut self) {
        self.count_op();
        while self.step() == StepOutcome::Transitioned {}
    }

    /// The frozen quiescence predicate (§L.1 clauses 1–5): no transition
    /// enabled or in flight, stable states reached, FAILED quiet-legal, no
    /// latched violation, no owed staged step. quiet ≠ healthy ≠ successful.
    pub fn is_quiet(&self) -> bool {
        self.count_op();
        self.quiet_now()
    }

    /// Root disposal: retire everything and drain (§L.2). A latched §G.6
    /// violation is visible in the snapshot; disposal does not claim
    /// completion past it.
    pub fn dispose_root(&mut self) {
        self.count_op();
        // The empty composition is always legal, so this cannot fail.
        let _ = self.set_desired_inner(Vec::new());
        self.settle();
    }

    /// The closed composition-truth diagnostic snapshot (§I.1).
    pub fn snapshot(&self) -> CompositionSnapshot {
        self.count_op();
        let mut fibers = BTreeMap::new();
        let mut relations = BTreeSet::new();
        for slot in &self.slots {
            let Some(f) = &slot.fiber else { continue };
            fibers.insert(
                f.name.clone(),
                FiberDiagnostic {
                    state: f.state,
                    failed_outcome: f.state == FiberState::Failed,
                    teardown_violated: f.teardown_violated,
                },
            );
            for e in &f.effects {
                if let Some((key, Some(provider))) = e.relation() {
                    let provider_name = self
                        .fiber_by_id(provider)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| "<removed>".to_owned());
                    relations.insert(RelationDiagnostic {
                        owner: f.name.clone(),
                        provider: provider_name,
                        capability: key.name,
                    });
                }
            }
        }
        // Known capabilities = declared keys of installed fibers' components
        // and of desired components (§I.1 surface 1: absent or provided by X).
        let mut known: BTreeMap<TypeId, CapabilityKey> = BTreeMap::new();
        let consider = |spec: &ComponentSpec, known: &mut BTreeMap<TypeId, CapabilityKey>| {
            for k in spec.requires.iter().chain(spec.provides.iter()) {
                known.insert(k.id, *k);
            }
        };
        for slot in &self.slots {
            if let Some(f) = &slot.fiber
                && let Some(spec) = self.catalog.get(f.component)
            {
                consider(spec, &mut known);
            }
        }
        for entry in self.desired.values() {
            if let Some(spec) = self.catalog.get(entry.component) {
                consider(spec, &mut known);
            }
        }
        let mut capabilities = BTreeMap::new();
        for key in known.values() {
            let providers = self.active_providers_of(key);
            let provider = if providers.len() == 1 {
                self.fiber_by_id(providers[0]).map(|f| f.name.clone())
            } else {
                None
            };
            capabilities.insert(key.name.to_string(), provider);
        }
        // Provision projection over ALL installed fibers (any lifecycle
        // state): the authority for the pointwise single-source invariant.
        let mut provisions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for slot in &self.slots {
            let Some(f) = &slot.fiber else { continue };
            for e in &f.effects {
                if let crate::fiber::EffectPayload::Provision { key, .. } = &e.payload {
                    provisions
                        .entry(key.name.to_string())
                        .or_default()
                        .insert(f.name.clone());
                }
            }
        }
        // Committed-binding projection (§I.1 surface 3): consumer fiber ->
        // capability -> provider fiber, for every installed fiber with an
        // open episode-fixed committed view. Read-only derivation from
        // `Fiber.committed` — never a second mutable registry (§K.4). A
        // plain resolve with no relation Effect still appears here; a
        // §G.6-latched episode keeps its committed view open, so its
        // binding stays visible instead of vanishing.
        let mut committed: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for slot in &self.slots {
            let Some(f) = &slot.fiber else { continue };
            let Some(view) = &f.committed else { continue };
            let Some(spec) = self.catalog.get(f.component) else {
                continue;
            };
            let mut binds = BTreeMap::new();
            for (cap_id, provider) in view {
                let Some(key) = spec.requires.iter().find(|k| k.id == *cap_id) else {
                    continue;
                };
                let provider_name = self
                    .fiber_by_id(*provider)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "<removed>".to_owned());
                binds.insert(key.name.to_owned(), provider_name);
            }
            committed.insert(f.name.clone(), binds);
        }
        CompositionSnapshot {
            fibers,
            capabilities,
            provisions,
            relations,
            committed,
            quiet: self.quiet_now(),
        }
    }

    /// RT-firewall witness readout (implementation ADR D9). Counts every
    /// public kernel operation performed so far. This is a test witness, not
    /// a diagnostic surface, and payload code must never need it.
    #[doc(hidden)]
    pub fn debug_op_count(&self) -> u64 {
        self.ops.get()
    }

    // ------------------------------------------------------------------
    // Plan validation (§L.4)
    // ------------------------------------------------------------------

    fn set_desired_inner(&mut self, entries: Vec<DesiredEntry>) -> Result<(), CompositionErrors> {
        let map = self.validate(&entries)?;
        self.desired = map;
        Ok(())
    }

    fn validate(&self, entries: &[DesiredEntry]) -> Result<DesiredMap, CompositionErrors> {
        let mut errors = Vec::new();
        let mut map: DesiredMap = BTreeMap::new();
        for e in entries {
            if !self.catalog.contains_key(e.component) {
                errors.push(CompositionError::UnknownComponent {
                    entry: e.id.clone(),
                    component: e.component.to_string(),
                });
                continue;
            }
            if map.insert(e.id.clone(), e.clone()).is_some() {
                errors.push(CompositionError::DuplicateEntry { id: e.id.clone() });
            }
        }
        // Required-single ambiguity over ENABLED entries (§E.2).
        let mut key_providers: HashMap<TypeId, (&'static str, Vec<String>)> = HashMap::new();
        for e in map.values() {
            if !e.enabled {
                continue;
            }
            for k in &self.catalog[e.component].provides {
                key_providers
                    .entry(k.id)
                    .or_insert_with(|| (k.name, Vec::new()))
                    .1
                    .push(e.id.clone());
            }
        }
        for (name, providers) in key_providers.values() {
            if providers.len() > 1 {
                errors.push(CompositionError::AmbiguousProvider {
                    capability: name,
                    providers: providers.clone(),
                });
            }
        }
        // Owner entry per provided key (last wins; ambiguity above already
        // refuses the plan when a key has two providers).
        let mut key_owner: HashMap<TypeId, String> = HashMap::new();
        for e in map.values() {
            if !e.enabled {
                continue;
            }
            for k in &self.catalog[e.component].provides {
                key_owner.insert(k.id, e.id.clone());
            }
        }
        // Acyclicity of the requirement graph over enabled entries (B24),
        // including the one-node self-provided-requirement cycle.
        let mut adjacency: HashMap<String, Vec<(String, &'static str)>> = HashMap::new();
        for e in map.values() {
            if !e.enabled {
                continue;
            }
            for k in &self.catalog[e.component].requires {
                if let Some(owner) = key_owner.get(&k.id) {
                    if owner == &e.id {
                        errors.push(CompositionError::DependencyCycle { capability: k.name });
                    } else {
                        adjacency
                            .entry(e.id.clone())
                            .or_default()
                            .push((owner.clone(), k.name));
                    }
                }
            }
        }
        let mut color: HashMap<String, u8> = HashMap::new();
        let mut roots: Vec<String> = adjacency.keys().cloned().collect();
        roots.sort();
        for root in roots {
            if color.get(&root).copied().unwrap_or(0) == 0 {
                dfs_cycle(&root, &adjacency, &mut color, &mut errors);
            }
        }
        if errors.is_empty() {
            Ok(map)
        } else {
            Err(CompositionErrors(errors))
        }
    }

    // ------------------------------------------------------------------
    // Transition selection (§F.3, §L.2)
    // ------------------------------------------------------------------

    fn eligible_unload(&self) -> Option<FiberId> {
        for slot in &self.slots {
            let Some(f) = &slot.fiber else { continue };
            if f.state == FiberState::Unloading && !f.teardown_violated && !self.relied_on(f.id) {
                return Some(f.id);
            }
        }
        None
    }

    fn divert_candidate(&self) -> Option<FiberId> {
        for slot in &self.slots {
            let Some(f) = &slot.fiber else { continue };
            if f.state == FiberState::Active && (f.retired || self.committed_view_stale(f)) {
                return Some(f.id);
            }
        }
        None
    }

    fn retire_mismatch_candidate(&self) -> Option<FiberId> {
        for slot in &self.slots {
            let Some(f) = &slot.fiber else { continue };
            if f.retired || f.state == FiberState::Unloading || f.state == FiberState::Activating {
                continue;
            }
            let mismatch = match self.desired.get(&f.name) {
                None => true,
                Some(e) => !e.enabled || e.revision != f.revision || e.component != f.component,
            };
            if mismatch {
                return Some(f.id);
            }
        }
        None
    }

    fn removal_candidate(&self) -> Option<usize> {
        for (idx, slot) in self.slots.iter().enumerate() {
            let Some(f) = &slot.fiber else { continue };
            if f.retired
                && !f.teardown_violated
                && f.committed.is_none()
                && f.effects.is_empty()
                && matches!(f.state, FiberState::Pending | FiberState::Failed)
            {
                return Some(idx);
            }
        }
        None
    }

    fn mount_candidate(&self) -> Option<DesiredEntry> {
        for e in self.desired.values() {
            if e.enabled && self.find_by_name(&e.id).is_none() {
                return Some(e.clone());
            }
        }
        None
    }

    fn activation_candidate(&self) -> Option<FiberId> {
        for slot in &self.slots {
            let Some(f) = &slot.fiber else { continue };
            if f.state == FiberState::Pending && !f.retired && self.activation_ready(f) {
                return Some(f.id);
            }
        }
        None
    }

    // ------------------------------------------------------------------
    // Transitions
    // ------------------------------------------------------------------

    fn mount_fiber(&mut self, entry: DesiredEntry) {
        let idx = match self.slots.iter().position(|s| s.fiber.is_none()) {
            Some(i) => i,
            None => {
                self.slots.push(Slot {
                    generation: 0,
                    fiber: None,
                });
                self.slots.len() - 1
            }
        };
        let fid = FiberId {
            idx: idx as u32,
            generation: self.slots[idx].generation,
        };
        self.slots[idx].fiber = Some(Fiber {
            id: fid,
            name: entry.id,
            component: entry.component,
            revision: entry.revision,
            retired: false,
            state: FiberState::Pending,
            teardown_violated: false,
            pending_error: None,
            committed: None,
            effects: Vec::new(),
        });
    }

    fn activate_fiber(&mut self, fid: FiberId) {
        let spec = {
            let f = self.fiber(fid);
            self.catalog[f.component].clone()
        };
        // L-Begin: freeze the committed view from current ACTIVE providers.
        let mut view = BTreeMap::new();
        for k in &spec.requires {
            let providers = self.active_providers_of(k);
            if providers.len() != 1 {
                // Defensive: the candidate check guarantees exactly one.
                return;
            }
            view.insert(k.id, providers[0]);
        }
        {
            let f = self.fiber_mut(fid);
            f.state = FiberState::Activating;
            f.committed = Some(view);
        }
        let outcome = {
            let mut ctx = ActivationCtx {
                kernel: self,
                fiber: fid,
                requires: spec.requires.clone(),
                provides: spec.provides.clone(),
            };
            (spec.activate)(&mut ctx)
        };
        // Totality on provision (§L.4): a completed activation must have
        // installed every declared key; otherwise it is a raise.
        let raise = match outcome {
            Ok(()) => {
                let missing = spec
                    .provides
                    .iter()
                    .any(|k| !self.fiber(fid).provides_key(k));
                missing.then(|| {
                    ActivationError::new(
                        "completed activation did not install every declared provision",
                    )
                })
            }
            Err(e) => Some(e),
        };
        // On success the completed episode keeps its owned effects installed
        // (§F.3: "effects owned; provisions installed"); the unwind runs only
        // on the raise path.
        if let Some(err) = raise {
            // B19: a raise lands in Unloading first; FAILED is only recorded
            // by a fully discharged unwind (§F.3, Corrective-2).
            {
                let f = self.fiber_mut(fid);
                f.state = FiberState::Unloading;
                f.pending_error = Some(err);
            }
            // A dispose-violation latched earlier keeps the episode open: no
            // unwind, no close, no outcome (§G.6 — no exit while latched).
            if self.fiber(fid).teardown_violated {
                return;
            }
            let verdict = self.run_unwind(fid);
            if verdict == Discharge::Violated {
                // The violated unwind never reaches FAILED: latched in
                // Unloading with the pending activation error as episode
                // metadata (§G.6). The committed view stays open — the
                // episode has not closed.
                self.fiber_mut(fid).teardown_violated = true;
                return;
            }
            // Full discharge closes the episode: the committed view is
            // discarded last (§F.3), the FAILED outcome is recorded.
            let f = self.fiber_mut(fid);
            f.committed = None;
            f.state = FiberState::Failed;
        } else if self.fiber(fid).teardown_violated {
            // §G.6 priority: TEARDOWN_VIOLATED ⇒ NEVER ACTIVE. A dispose
            // violation latched during activation (an explicit
            // `ctx.dispose()` whose inverse returned Violated) keeps the
            // episode open in Unloading even though activation itself
            // returned Ok: no unwind, no close, no Active landing. Not
            // ACTIVE means the fiber never joins NEW resolution
            // (`active_providers_of` gates on Active), so no later consumer
            // can commit to it, and the run stays loudly Blocked
            // (Corrective-1 review A21).
            self.fiber_mut(fid).state = FiberState::Unloading;
        } else {
            self.fiber_mut(fid).state = FiberState::Active;
        }
    }

    fn unload_fiber(&mut self, fid: FiberId) {
        // The violated latch has no exit (§F.2/§G.6): the episode may not
        // close over a violated teardown contract.
        if self.fiber(fid).teardown_violated {
            return;
        }
        let verdict = self.run_unwind(fid);
        if verdict == Discharge::Violated {
            self.fiber_mut(fid).teardown_violated = true;
            return;
        }
        // Domain teardown obligations discharge inside the window; the
        // kernel sees only the verdict (§G.6, §H.5.1 fence). Never run after
        // a raised activation (no activated episode existed; ADR D8).
        if self.fiber(fid).pending_error.is_none() {
            let teardown = {
                let f = self.fiber(fid);
                self.catalog[f.component].teardown.clone()
            };
            let violated = {
                let mut tctx = TeardownCtx {
                    kernel: self,
                    fiber: fid,
                };
                teardown(&mut tctx) == Discharge::Violated
            };
            if violated {
                self.fiber_mut(fid).teardown_violated = true;
                return;
            }
        }
        let f = self.fiber_mut(fid);
        f.committed = None;
        f.state = if f.pending_error.is_some() {
            FiberState::Failed
        } else {
            FiberState::Pending
        };
    }

    /// LIFO unwind of the owned-effect accumulator (§H.1). A violated
    /// inverse stops the unwind: the remaining effects stay un-discharged
    /// and unclaimed — partial discharge is a violation, never a success
    /// with notes (§O.2, §G.6).
    ///
    /// The violated effect's record is NOT dropped: its inverse was
    /// consumed (it is never retried), but the record stays in the
    /// accumulator as the authoritative provenance tombstone, so the
    /// composition relation it bears remains observable (§K.4 single
    /// authority — a violated teardown must not pretend the binding was
    /// cleaned). Removal stays blocked on the non-empty accumulator.
    fn run_unwind(&mut self, fid: FiberId) -> Discharge {
        loop {
            // Take the top payload without dropping the record; the
            // `Violated` marker is the discharge-state tombstone. A clean
            // inverse pops the record; a violated one leaves it in place.
            let payload = {
                let f = self.fiber_mut(fid);
                let Some(record) = f.effects.last_mut() else {
                    return Discharge::Discharged;
                };
                std::mem::replace(&mut record.payload, EffectPayload::Violated)
            };
            match payload {
                // Removal discharges a provision; the service value drops
                // with the record (the single authority, §K.4).
                EffectPayload::Provision { .. } => {
                    self.fiber_mut(fid).effects.pop();
                }
                EffectPayload::Inverse(inverse) => {
                    if inverse() == Discharge::Violated {
                        return Discharge::Violated;
                    }
                    self.fiber_mut(fid).effects.pop();
                }
                EffectPayload::Violated => {
                    unreachable!("a violated tombstone is never unwound again")
                }
            }
        }
    }

    /// Explicit dispose of an owned effect (B26: idempotent no-op for
    /// unknown handles). A violated inverse latches §G.6 immediately and
    /// keeps the authoritative provenance record in place — the binding it
    /// bears is not discharged and must not vanish from diagnostics (§K.4).
    pub(crate) fn dispose_effect(&mut self, fid: FiberId, handle: EffectHandle) {
        let position = self
            .fiber(fid)
            .effects
            .iter()
            .position(|e| e.handle == handle);
        let Some(position) = position else { return };
        let payload = std::mem::replace(
            &mut self.fiber_mut(fid).effects[position].payload,
            EffectPayload::Violated,
        );
        match payload {
            EffectPayload::Provision { .. } => {
                self.fiber_mut(fid).effects.remove(position);
            }
            EffectPayload::Inverse(inverse) => {
                if inverse() == Discharge::Violated {
                    self.fiber_mut(fid).teardown_violated = true;
                    // The record stays as the tombstone; a second dispose of
                    // the same handle is an idempotent no-op on it (B26).
                } else {
                    self.fiber_mut(fid).effects.remove(position);
                }
            }
            EffectPayload::Violated => {
                // Already consumed: idempotent no-op (B26).
            }
        }
    }

    // ------------------------------------------------------------------
    // Resolution and registry truth
    // ------------------------------------------------------------------

    /// New-resolution eligibility (§E.3): exactly the ACTIVE fibers that
    /// currently provide the key. An Unloading provider never appears here.
    fn active_providers_of(&self, key: &CapabilityKey) -> Vec<FiberId> {
        self.active_providers_of_id(key.id)
    }

    fn active_providers_of_id(&self, id: TypeId) -> Vec<FiberId> {
        let mut providers = Vec::new();
        for slot in &self.slots {
            let Some(f) = &slot.fiber else { continue };
            if f.state == FiberState::Active
                && f.effects.iter().any(|e| {
                    matches!(
                        &e.payload,
                        EffectPayload::Provision { key, .. } if key.id == id
                    )
                })
            {
                providers.push(f.id);
            }
        }
        providers
    }

    fn committed_view_stale(&self, f: &Fiber) -> bool {
        let Some(view) = &f.committed else {
            return true;
        };
        view.iter().any(|(id, provider)| {
            let current = self.active_providers_of_id(*id);
            current.as_slice() != [*provider].as_slice()
        })
    }

    fn relied_on(&self, fid: FiberId) -> bool {
        self.slots.iter().any(|slot| {
            slot.fiber.as_ref().is_some_and(|f| {
                f.id != fid
                    && f.committed
                        .as_ref()
                        .is_some_and(|view| view.values().any(|p| *p == fid))
            })
        })
    }

    fn activation_ready(&self, f: &Fiber) -> bool {
        self.catalog[f.component]
            .requires
            .iter()
            .all(|k| self.active_providers_of(k).len() == 1)
    }

    fn any_violation(&self) -> bool {
        self.slots
            .iter()
            .any(|s| s.fiber.as_ref().is_some_and(|f| f.teardown_violated))
    }

    fn quiet_now(&self) -> bool {
        !self.any_violation() && !self.has_pending_work()
    }

    /// Every §L.1 blocker, as pure truth (no mutation).
    fn has_pending_work(&self) -> bool {
        for slot in &self.slots {
            let Some(f) = &slot.fiber else { continue };
            match f.state {
                // In-flight transitions, including guarded unloads and the
                // owed new-mount of a staged replacement (clauses 1 and 5).
                FiberState::Activating | FiberState::Unloading => return true,
                FiberState::Active => {
                    if f.retired || self.committed_view_stale(f) {
                        return true;
                    }
                }
                FiberState::Pending => {
                    if f.retired || self.activation_ready(f) {
                        return true;
                    }
                }
                // FAILED is quiet-legal (clause 3); nothing auto-retries
                // (B19) — but a retired FAILED generation still owes its
                // removal before the staged replacement can continue.
                FiberState::Failed => {
                    if f.retired {
                        return true;
                    }
                }
            }
        }
        for (id, e) in &self.desired {
            match self.find_by_name(id).map(|fid| self.fiber(fid)) {
                None => {
                    if e.enabled {
                        return true; // mount owed
                    }
                }
                Some(f) => {
                    let mismatch =
                        !e.enabled || e.revision != f.revision || e.component != f.component;
                    if mismatch && !f.retired {
                        return true; // retire owed
                    }
                }
            }
        }
        // Orphan fibers whose desired entry vanished.
        self.slots.iter().any(|slot| {
            slot.fiber
                .as_ref()
                .is_some_and(|f| !self.desired.contains_key(&f.name) && !f.retired)
        })
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    pub(crate) fn fiber(&self, fid: FiberId) -> &Fiber {
        self.fiber_opt(fid).expect("live fiber id")
    }

    pub(crate) fn fiber_mut(&mut self, fid: FiberId) -> &mut Fiber {
        let slot = self
            .slots
            .get_mut(fid.idx as usize)
            .expect("live fiber slot");
        slot.fiber
            .as_mut()
            .filter(|f| f.id == fid)
            .expect("live fiber id")
    }

    fn fiber_by_id(&self, fid: FiberId) -> Option<&Fiber> {
        self.fiber_opt(fid)
    }

    fn fiber_opt(&self, fid: FiberId) -> Option<&Fiber> {
        self.slots
            .get(fid.idx as usize)
            .and_then(|s| s.fiber.as_ref())
            .filter(|f| f.id == fid)
    }

    fn find_by_name(&self, name: &str) -> Option<FiberId> {
        self.slots
            .iter()
            .find_map(|s| s.fiber.as_ref().filter(|f| f.name == name).map(|f| f.id))
    }

    pub(crate) fn is_declared(&self, fid: FiberId, key: &CapabilityKey) -> bool {
        let f = self.fiber(fid);
        let Some(spec) = self.catalog.get(f.component) else {
            return false;
        };
        spec.requires.iter().any(|k| k.id == key.id) || spec.provides.iter().any(|k| k.id == key.id)
    }

    pub(crate) fn next_handle(&mut self) -> EffectHandle {
        self.effect_handles += 1;
        EffectHandle(self.effect_handles)
    }

    fn count_op(&self) {
        self.ops.set(self.ops.get() + 1);
    }
}

/// White/grey/black DFS over enabled entries; reports the first back edge as
/// a dependency-cycle composition error (B24: detectable from declarations).
fn dfs_cycle(
    node: &str,
    adjacency: &HashMap<String, Vec<(String, &'static str)>>,
    color: &mut HashMap<String, u8>,
    errors: &mut Vec<CompositionError>,
) {
    color.insert(node.to_owned(), 1);
    if let Some(edges) = adjacency.get(node) {
        for (next, capability) in edges {
            match color.get(next).copied().unwrap_or(0) {
                0 => dfs_cycle(next, adjacency, color, errors),
                1 if !errors
                    .iter()
                    .any(|e| matches!(e, CompositionError::DependencyCycle { .. })) =>
                {
                    errors.push(CompositionError::DependencyCycle { capability });
                }
                _ => {}
            }
        }
    }
    color.insert(node.to_owned(), 2);
}
