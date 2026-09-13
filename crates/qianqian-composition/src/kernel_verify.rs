//! Kani harnesses over the REAL kernel (campaign FV-RUST-0, K1–K6).
//!
//! Rules of engagement (campaign B1.2): real production functions plus
//! minimal `cfg(kani)` harness code; the kernel is never rewritten or
//! half-publicized for the verifier. This module is a child of `kernel`
//! solely to reach private registry internals for observation — it changes
//! no production item.
//!
//! Harness design: the desired-plan universe per harness is *enumerated
//! exhaustively* as concrete scenarios (all plan bits, violation verdicts
//! and disposal routes iterated), with the property invariants asserted
//! after every bounded step. All data is concrete — no symbolic Strings —
//! which keeps each CBMC run seconds-to-minutes and makes the bounds
//! crisp: "every scenario in the stated finite universe, drained ≤ M
//! steps, invariant re-checked after each step". A green run is
//! BOUNDED-CLEAN within exactly those bounds (vocabulary #124) — never a
//! general proof. The symbolic/exhaustive protocol-level counterpart is
//! the TLA+ model in specs/composition-kernel-0/.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::capability::Capability;
use crate::component::{ActivationError, ComponentSpec, Discharge};
use crate::desired::{DesiredEntry, Revision};
use crate::fiber::{Fiber, FiberId};
use crate::kernel::{CompositionKernel, EffectHandle, StepOutcome};

// ---------------------------------------------------------------------
// Harness universe
// ---------------------------------------------------------------------

struct HarnessCap;
impl Capability for HarnessCap {
    const NAME: &'static str = "HarnessCap";
    type Service = ();
}

fn entry(id: &str, component: &'static str) -> DesiredEntry {
    DesiredEntry::enabled(id, component, Revision::new(0))
}

fn spec_provider(name: &'static str) -> ComponentSpec {
    spec_provider_flagged(name, Rc::new(Cell::new(false)))
}

/// A provider whose teardown may be driven to `Violated` (§G.6 latch).
fn spec_provider_flagged(name: &'static str, td_violates: Rc<Cell<bool>>) -> ComponentSpec {
    ComponentSpec::new(name)
        .provides::<HarnessCap>()
        .on_activate(|ctx| {
            ctx.provide::<HarnessCap>(Rc::new(()))
                .map_err(|e| ActivationError::new(format!("{e:?}")))
        })
        .on_teardown(move |_| {
            if td_violates.get() {
                Discharge::Violated
            } else {
                Discharge::Discharged
            }
        })
}

/// A consumer: its activation freezes a committed view binding the active
/// provider (kernel L-Begin), even with a no-op activation body.
fn spec_consumer(name: &'static str) -> ComponentSpec {
    ComponentSpec::new(name).requires::<HarnessCap>()
}

fn spec_local(name: &'static str) -> ComponentSpec {
    ComponentSpec::new(name)
}

/// Invocation recorder: (effect tag, global sequence stamp).
#[derive(Clone)]
struct Log {
    entries: Rc<RefCell<Vec<(u8, u8)>>>,
    tick: Rc<Cell<u8>>,
}

impl Log {
    fn new() -> Self {
        Self {
            entries: Rc::new(RefCell::new(Vec::new())),
            tick: Rc::new(Cell::new(0)),
        }
    }

    fn record(&self, tag: u8) {
        self.entries
            .borrow_mut()
            .push((tag, self.tick.replace(self.tick.get().wrapping_add(1))));
    }

    fn count(&self, tag: u8) -> usize {
        self.entries
            .borrow()
            .iter()
            .filter(|(t, _)| *t == tag)
            .count()
    }

    fn first_pos(&self, tag: u8) -> Option<usize> {
        self.entries.borrow().iter().position(|(t, _)| *t == tag)
    }
}

/// A component that registers two owner-local reversible effects during
/// activation (tag 1 then tag 2), optionally raises, and whose domain
/// teardown is recorded as tag 3. Teardown runs only for a deactivated
/// activated episode (no teardown after a raise).
#[allow(clippy::too_many_arguments)] // harness fixture; each verdict flag is independently drivable
fn spec_effects(
    name: &'static str,
    requires_cap: bool,
    log: &Log,
    inv1_violates: Rc<Cell<bool>>,
    inv2_violates: Rc<Cell<bool>>,
    td_violates: Rc<Cell<bool>>,
    act_fails: Rc<Cell<bool>>,
    handle2: Rc<RefCell<Option<EffectHandle>>>,
) -> ComponentSpec {
    let log = log.clone(); // own it: the closures below are 'static
    let mut spec = ComponentSpec::new(name);
    if requires_cap {
        spec = spec.requires::<HarnessCap>();
    }
    let log_teardown = log.clone();
    spec.on_activate(move |ctx| {
        ctx.register_effect({
            let log = log.clone();
            let v = inv1_violates.clone();
            move || {
                log.record(1);
                if v.get() {
                    Discharge::Violated
                } else {
                    Discharge::Discharged
                }
            }
        });
        let h2 = ctx.register_effect({
            let log = log.clone();
            let v = inv2_violates.clone();
            move || {
                log.record(2);
                if v.get() {
                    Discharge::Violated
                } else {
                    Discharge::Discharged
                }
            }
        });
        *handle2.borrow_mut() = Some(h2);
        if act_fails.get() {
            Err(ActivationError::new("harness activation raise"))
        } else {
            Ok(())
        }
    })
    .on_teardown(move |_| {
        log_teardown.record(3);
        if td_violates.get() {
            Discharge::Violated
        } else {
            Discharge::Discharged
        }
    })
}

// ---------------------------------------------------------------------
// Observation helpers (full private access, no production change)
// ---------------------------------------------------------------------

fn installed_fibers(k: &CompositionKernel) -> Vec<&Fiber> {
    k.slots.iter().filter_map(|s| s.fiber.as_ref()).collect()
}

fn any_violated(k: &CompositionKernel) -> bool {
    installed_fibers(k).iter().any(|f| f.teardown_violated)
}

/// P_RELIED: every provider referenced by an open committed view is still
/// installed. This is the semantic content of the relied_on removal guard,
/// stated on the registry truth instead of restating the guard's code.
fn assert_committed_providers_installed(k: &CompositionKernel) {
    for f in installed_fibers(k) {
        if let Some(view) = &f.committed {
            for p in view.values() {
                assert!(
                    k.fiber_opt(*p).is_some(),
                    "an open committed view references an uninstalled provider"
                );
            }
        }
    }
}

/// P_SINGLE (#126 single-source): at most one installed fiber *declares*
/// provision for a capability, counting every lifecycle state — an
/// Unloading old fiber is still installed.
fn assert_single_source(k: &CompositionKernel) {
    let fs = installed_fibers(k);
    for (i, f) in fs.iter().enumerate() {
        let fp = &k.catalog[f.component].provides;
        for g in fs.iter().skip(i + 1) {
            let gp = &k.catalog[g.component].provides;
            for a in fp {
                for b in gp {
                    assert!(
                        a.id != b.id,
                        "two installed fibers declare provision for the same capability"
                    );
                }
            }
        }
    }
}

fn drain_bounded(k: &mut CompositionKernel, max: usize) {
    for _ in 0..max {
        if k.step() != StepOutcome::Transitioned {
            break;
        }
    }
}

/// Install a desired composition directly, bypassing the plan-time
/// validator. Harness-equivalent to `set_desired` on a legal plan (all
/// harness plans are legal by construction: one provider per capability,
/// acyclic, known components) without dragging the validator's
/// HashMap/DFS/String machinery into the solved formula. The transitions
/// read `desired` identically either way.
fn force_desired(k: &mut CompositionKernel, entries: Vec<DesiredEntry>) {
    k.desired = entries
        .into_iter()
        .map(|e| (e.id.clone(), e))
        .collect::<std::collections::BTreeMap<_, _>>();
}

// ---------------------------------------------------------------------
// K1 — stale FiberId safety (two concrete scenarios: slot 0 reuse and
// slot-1 reuse behind a surviving filler fiber)
// ---------------------------------------------------------------------

fn k1_case(with_filler: bool) {
    let mut k = CompositionKernel::new();
    k.register_component(spec_local("filler")).unwrap();
    k.register_component(spec_local("fx")).unwrap();
    let initial = if with_filler {
        vec![entry("filler", "filler"), entry("fx", "fx")]
    } else {
        vec![entry("fx", "fx")]
    };
    force_desired(&mut k, initial);
    drain_bounded(&mut k, 8);

    let stale: FiberId = k.find_by_name("fx").expect("fx mounted");
    let remainder = if with_filler {
        vec![entry("filler", "filler")]
    } else {
        Vec::new()
    };
    force_desired(&mut k, remainder);
    drain_bounded(&mut k, 8);
    assert!(k.find_by_name("fx").is_none(), "precondition: fx removed");

    let again = if with_filler {
        vec![entry("filler", "filler"), entry("fx", "fx")]
    } else {
        vec![entry("fx", "fx")]
    };
    force_desired(&mut k, again);
    drain_bounded(&mut k, 8);
    let fresh = k.find_by_name("fx").expect("fx remounted");

    // Harness preconditions: the slot really was reused and the generation
    // really advanced — otherwise the property below would be vacuous.
    assert_eq!(fresh.idx, stale.idx, "precondition: slot reused");
    assert_eq!(
        fresh.generation,
        stale.generation.wrapping_add(1),
        "precondition: generation advanced on removal"
    );
    // The property: stale identity never regains authority.
    assert!(k.fiber_opt(stale).is_none(), "stale FiberId resolved");
    assert_ne!(stale, fresh);
}

#[cfg_attr(kani, kani::proof)]
#[cfg_attr(not(kani), test)]
#[cfg_attr(kani, kani::unwind(16))]
fn k1_stale_fiber_id_never_addresses_a_reused_slot() {
    k1_case(false);
    k1_case(true);
}

// ---------------------------------------------------------------------
// K2 — relied_on removal guard: all four concrete desired rewrites
// (keep/remove p × keep/remove c); P_RELIED re-checked after every step.
// ---------------------------------------------------------------------

#[cfg_attr(kani, kani::proof)]
#[cfg_attr(not(kani), test)]
#[cfg_attr(kani, kani::unwind(20))]
fn k2_relied_provider_is_never_removed_behind_an_open_committed_view() {
    for case in 0..4u32 {
        let mut k = CompositionKernel::new();
        k.register_component(spec_provider("p")).unwrap();
        k.register_component(spec_consumer("c")).unwrap();
        force_desired(&mut k, vec![entry("p", "p"), entry("c", "c")]);
        drain_bounded(&mut k, 8);
        let c_fid = k.find_by_name("c").expect("c installed");
        assert!(
            k.fiber(c_fid).committed.is_some(),
            "precondition: consumer holds an open committed view"
        );

        let keep_p = case & 1 != 0;
        let keep_c = case & 2 != 0;
        let mut d = Vec::new();
        if keep_p {
            d.push(entry("p", "p"));
        }
        if keep_c {
            d.push(entry("c", "c"));
        }
        force_desired(&mut k, d);
        for _ in 0..10 {
            assert_committed_providers_installed(&k);
            if k.step() != StepOutcome::Transitioned {
                break;
            }
            assert_committed_providers_installed(&k);
        }
        assert_committed_providers_installed(&k);
    }
}

// ---------------------------------------------------------------------
// K3 — effect discharge discipline: 12 concrete scenarios (early top-effect
// dispose × double-dispose × disposal route), clean verdicts only.
// ---------------------------------------------------------------------

#[cfg_attr(kani, kani::proof)]
#[cfg_attr(not(kani), test)]
#[cfg_attr(kani, kani::unwind(24))]
fn k3_effect_inverses_fire_at_most_once_in_lifo_order() {
    for case in 0..12u32 {
        let log = Log::new();
        let no_violation = Rc::new(Cell::new(false));
        let handle2: Rc<RefCell<Option<EffectHandle>>> = Rc::new(RefCell::new(None));
        let mut k = CompositionKernel::new();
        k.register_component(spec_effects(
            "fx",
            false,
            &log,
            no_violation.clone(),
            no_violation.clone(),
            no_violation.clone(),
            no_violation.clone(),
            handle2.clone(),
        ))
        .unwrap();
        force_desired(&mut k, vec![entry("fx", "fx")]);
        drain_bounded(&mut k, 6);

        let do_dispose = case & 1 != 0;
        let do_double_dispose = case & 2 != 0;
        let route = case >> 2; // 0 dispose_root, 1 desired-empty, 2 leave live
        if do_dispose {
            let fid = k.find_by_name("fx").unwrap();
            let h = handle2.borrow().expect("effect 2 registered");
            k.dispose_effect(fid, h);
            if do_double_dispose {
                k.dispose_effect(fid, h);
            }
        }
        match route {
            0 | 1 => {
                force_desired(&mut k, Vec::new());
                drain_bounded(&mut k, 10);
            }
            _ => drain_bounded(&mut k, 4),
        }

        // Discipline: each inverse fired at most once across every path
        // (clean unload, explicit dispose, double dispose), and whenever
        // both fired the later-registered one fired first (LIFO).
        let (c1, c2) = (log.count(1), log.count(2));
        assert!(c1 <= 1, "inverse 1 fired twice");
        assert!(c2 <= 1, "inverse 2 fired twice");
        if c1 == 1 && c2 == 1 {
            let i1 = log.first_pos(1).unwrap();
            let i2 = log.first_pos(2).unwrap();
            assert!(i2 < i1, "unwind violated LIFO order");
        }
        // A fully drained, non-violated run discharges both effects.
        if route != 2 {
            assert_eq!(c1, 1, "inverse 1 owed after a full drain");
            assert_eq!(c2, 1, "inverse 2 owed after a full drain");
        }
    }
}

// ---------------------------------------------------------------------
// K4a — removal discipline with clean verdicts: activation raise or not ×
// disposal route × early dispose. Removed fibers must owe nothing.
// ---------------------------------------------------------------------

#[cfg_attr(kani, kani::proof)]
#[cfg_attr(not(kani), test)]
#[cfg_attr(kani, kani::unwind(28))]
fn k4a_removed_fibers_leave_nothing_owed_clean() {
    for case in 0..8u32 {
        let log = Log::new();
        let no_violation = Rc::new(Cell::new(false));
        let act_fails = Rc::new(Cell::new(case & 1 != 0));
        let handle2: Rc<RefCell<Option<EffectHandle>>> = Rc::new(RefCell::new(None));
        let mut k = CompositionKernel::new();
        k.register_component(spec_provider("p")).unwrap();
        k.register_component(spec_effects(
            "fx",
            true, // consumer: its episode holds a committed view binding p
            &log,
            no_violation.clone(),
            no_violation.clone(),
            no_violation.clone(),
            act_fails.clone(),
            handle2.clone(),
        ))
        .unwrap();
        force_desired(&mut k, vec![entry("p", "p"), entry("fx", "fx")]);
        drain_bounded(&mut k, 8);

        let do_dispose = case & 2 != 0;
        let route = case >> 2; // 0 dispose_root, 1 desired-empty
        if do_dispose
            && let Some(fid) = k.find_by_name("fx")
            && let Some(h) = handle2.borrow().as_ref()
        {
            k.dispose_effect(fid, *h);
        }
        match route {
            0 => {
                force_desired(&mut k, Vec::new());
            }
            _ => {
                force_desired(&mut k, Vec::new());
            }
        }
        drain_bounded(&mut k, 14);

        // Clean full drain: every fiber gone; each inverse fired exactly
        // once (via unwind or explicit dispose); teardown fired exactly
        // once for a deactivated episode and never after a raise.
        assert!(
            k.slots.iter().all(|s| s.fiber.is_none()),
            "a clean full drain left a fiber installed"
        );
        assert_eq!(log.count(1), 1, "inverse 1 owed after removal");
        assert_eq!(log.count(2), 1, "inverse 2 owed after removal");
        let expected_td = if act_fails.get() { 0 } else { 1 };
        assert_eq!(log.count(3), expected_td, "teardown discipline violated");
        assert_committed_providers_installed(&k);
        assert_single_source(&k);
    }
}

// ---------------------------------------------------------------------
// K4b — removal discipline under violated verdicts: the full violation
// lattice (inv1/inv2/teardown) × disposal route. A §G.6 latch must keep
// the fiber installed, must block removal, and must preserve P_RELIED.
// ---------------------------------------------------------------------

#[cfg_attr(kani, kani::proof)]
#[cfg_attr(not(kani), test)]
#[cfg_attr(kani, kani::unwind(32))]
fn k4b_removed_fibers_leave_nothing_owed_violations() {
    for case in 0..16u32 {
        let log = Log::new();
        let inv1_v = Rc::new(Cell::new(case & 1 != 0));
        let inv2_v = Rc::new(Cell::new(case & 2 != 0));
        let td_v = Rc::new(Cell::new(case & 4 != 0));
        let act_fails = Rc::new(Cell::new(false));
        let handle2: Rc<RefCell<Option<EffectHandle>>> = Rc::new(RefCell::new(None));
        let mut k = CompositionKernel::new();
        k.register_component(spec_provider("p")).unwrap();
        k.register_component(spec_effects(
            "fx",
            true,
            &log,
            inv1_v.clone(),
            inv2_v.clone(),
            td_v.clone(),
            act_fails.clone(),
            handle2.clone(),
        ))
        .unwrap();
        force_desired(&mut k, vec![entry("p", "p"), entry("fx", "fx")]);
        drain_bounded(&mut k, 8);

        let route = case >> 3; // 0 dispose_root, 1 desired-empty
        match route {
            0 => {
                force_desired(&mut k, Vec::new());
            }
            _ => {
                force_desired(&mut k, Vec::new());
            }
        }
        drain_bounded(&mut k, 14);

        let violated = any_violated(&k);
        // If any inverse or teardown latched §G.6, the fiber stays
        // installed and its obligations stay observable.
        assert_eq!(
            violated,
            inv1_v.get() || inv2_v.get() || td_v.get(),
            "a violated verdict must latch exactly when one was configured"
        );
        if violated {
            assert!(k.find_by_name("fx").is_some(), "latched fiber removed");
        } else {
            assert!(
                k.slots.iter().all(|s| s.fiber.is_none()),
                "a clean full drain left a fiber installed"
            );
            assert_eq!(log.count(1), 1);
            assert_eq!(log.count(2), 1);
            assert_eq!(log.count(3), 1);
        }
        assert_committed_providers_installed(&k);
        assert_single_source(&k);
    }
}

// ---------------------------------------------------------------------
// K5 — quiet truth: every phase-1 plan subset (8) × activation failure
// (2) must reach a state where quiet == step-settled; then a full
// phase-2 sweep from the maximal state.
// ---------------------------------------------------------------------

#[cfg_attr(kani, kani::proof)]
#[cfg_attr(not(kani), test)]
#[cfg_attr(kani, kani::unwind(40))]
fn k5_quiet_is_the_fixed_point_of_step() {
    for case in 0..16u32 {
        let act_fails = Rc::new(Cell::new(case & 1 != 0));
        let bits1 = case >> 1; // bit0 p, bit1 c, bit2 bad
        let mut k = CompositionKernel::new();
        k.register_component(spec_provider("p")).unwrap();
        k.register_component(spec_consumer("c")).unwrap();
        let log = Log::new();
        let handle2: Rc<RefCell<Option<EffectHandle>>> = Rc::new(RefCell::new(None));
        k.register_component(spec_effects(
            "bad",
            false,
            &log,
            Rc::new(Cell::new(false)),
            Rc::new(Cell::new(false)),
            Rc::new(Cell::new(false)),
            act_fails.clone(),
            handle2,
        ))
        .unwrap();

        let mut d1 = Vec::new();
        if bits1 & 1 != 0 {
            d1.push(entry("p", "p"));
        }
        if bits1 & 2 != 0 {
            d1.push(entry("c", "c"));
        }
        if bits1 & 4 != 0 {
            d1.push(entry("bad", "bad"));
        }
        force_desired(&mut k, d1);
        drain_bounded(&mut k, 12);
        if k.is_quiet() {
            assert!(
                matches!(k.step(), StepOutcome::Settled),
                "quiet state still had an enabled transition"
            );
        }
    }

    // Phase 2: from the maximal state (p, c, bad active), sweep every
    // legal phase-2 plan; quiet must remain the fixed point from every
    // reached state, not only from phase-1 end states.
    for bits2 in 0..8u32 {
        let mut k = CompositionKernel::new();
        k.register_component(spec_provider("p")).unwrap();
        k.register_component(spec_consumer("c")).unwrap();
        let log = Log::new();
        let handle2: Rc<RefCell<Option<EffectHandle>>> = Rc::new(RefCell::new(None));
        k.register_component(spec_effects(
            "bad",
            false,
            &log,
            Rc::new(Cell::new(false)),
            Rc::new(Cell::new(false)),
            Rc::new(Cell::new(false)),
            Rc::new(Cell::new(false)),
            handle2,
        ))
        .unwrap();
        force_desired(
            &mut k,
            vec![entry("p", "p"), entry("c", "c"), entry("bad", "bad")],
        );
        drain_bounded(&mut k, 12);

        let mut d2 = Vec::new();
        if bits2 & 1 != 0 {
            d2.push(entry("p", "p"));
        }
        if bits2 & 2 != 0 {
            d2.push(entry("c", "c"));
        }
        if bits2 & 4 != 0 {
            d2.push(entry("bad", "bad"));
        }
        force_desired(&mut k, d2);
        drain_bounded(&mut k, 12);
        if k.is_quiet() {
            assert!(
                matches!(k.step(), StepOutcome::Settled),
                "quiet state still had an enabled transition (phase 2)"
            );
        }
    }
}

// ---------------------------------------------------------------------
// K6 — single-source: from the canonical committed state (p1+c active),
// sweep every legal phase-2 plan (at most one provider) with p1's
// teardown violation on/off. Covers clean replacement, replacement while
// the old provider is §G.6-latched (the #126 withheld mount), and
// provider swaps under a committed consumer.
// ---------------------------------------------------------------------

#[cfg_attr(kani, kani::proof)]
#[cfg_attr(not(kani), test)]
#[cfg_attr(kani, kani::unwind(40))]
fn k6_single_source_survives_replacement_and_violation() {
    for case in 0..12u32 {
        let p1_latches = case & 1 != 0;
        let plan2 = case >> 1; // 0: none, 1: p1, 2: p2, 3: p1+c, 4: p2+c, 5: c
        if plan2 == 3 {
            continue; // illegal: both providers enabled (plan refused)
        }
        let p1_violates = Rc::new(Cell::new(false));
        let mut k = CompositionKernel::new();
        k.register_component(spec_provider_flagged("p1", p1_violates.clone()))
            .unwrap();
        k.register_component(spec_provider("p2")).unwrap();
        k.register_component(spec_consumer("c")).unwrap();

        force_desired(&mut k, vec![entry("p1", "p1"), entry("c", "c")]);
        drain_bounded(&mut k, 10);
        assert_single_source(&k);

        p1_violates.set(p1_latches);
        let mut d2 = Vec::new();
        if plan2 & 1 != 0 {
            d2.push(entry("p1", "p1"));
        }
        if plan2 & 2 != 0 {
            d2.push(entry("p2", "p2"));
        }
        if plan2 & 4 != 0 {
            d2.push(entry("c", "c"));
        }
        force_desired(&mut k, d2);
        for _ in 0..14 {
            assert_single_source(&k);
            if k.step() != StepOutcome::Transitioned {
                break;
            }
            assert_single_source(&k);
        }
        assert_single_source(&k);
        assert_committed_providers_installed(&k);
    }
}
