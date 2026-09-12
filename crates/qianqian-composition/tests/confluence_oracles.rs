//! Composition confluence and failure-sanitation oracles (#70 Stage 11.H,
//! oracle family H; design §M).
//!
//! Only legal, failure-free histories enter the theorem-backed confluence
//! comparisons: the settled composition truth must equal a clean
//! construction of the final desired composition at the closed §I.1
//! surfaces. Failure histories never widen Thm 80 — they are judged by the
//! §M.4 sanitation oracle.

mod common;

use common::*;
use qianqian_composition::{ComponentSpec, CompositionKernel, DesiredEntry, Revision, StepOutcome};

/// A standard generic component set, registered identically in history and
/// fresh builds (names are the confluence identity; generations are not).
fn standard_kernel() -> CompositionKernel {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_provider("svc_a", "a", &l))
        .expect("component registered");
    k.register_component(tag_provider("svc_b", "b", &l))
        .expect("component registered");
    k.register_component(tag_consumer("consumer", &l))
        .expect("component registered");
    k.register_component(listeners_provider("registry"))
        .expect("component registered");
    k.register_component(listener_consumer("listener_x"))
        .expect("component registered");
    k.register_component(listener_consumer("listener_y"))
        .expect("component registered");
    k
}

fn snap_fibers(k: &CompositionKernel) -> String {
    // Deterministic rendering for diff messages.
    let s = k.snapshot();
    format!("{s:?}")
}

/// M1 + M2: provider flap and A1→A2→A1 round trips. The settled truth equals
/// a fresh build of the final desired composition; no ghost generations.
#[test]
fn m1_m2_history_with_flaps_and_replacements_conflates() {
    let final_desired = || {
        vec![
            DesiredEntry::enabled("svc", "svc_b", Revision::new(9)),
            DesiredEntry::enabled("consumer", "consumer", Revision::fresh()),
            DesiredEntry::enabled("registry", "registry", Revision::fresh()),
            DesiredEntry::enabled("listener_x", "listener_x", Revision::fresh()),
        ]
    };

    let mut h = standard_kernel();
    // History: mount generation 1, revise through generations (staged
    // replacements of the single provider entry), add and flap the listener
    // set, and revise the provider component a→b.
    h.set_desired(vec![
        DesiredEntry::enabled("svc", "svc_a", Revision::new(1)),
        DesiredEntry::enabled("consumer", "consumer", Revision::fresh()),
    ])
    .expect("legal");
    h.settle();
    h.set_desired(vec![
        DesiredEntry::enabled("svc", "svc_a", Revision::new(2)),
        DesiredEntry::enabled("consumer", "consumer", Revision::fresh()),
    ])
    .expect("legal");
    h.settle();
    h.set_desired(vec![
        DesiredEntry::enabled("svc", "svc_b", Revision::new(3)),
        DesiredEntry::enabled("consumer", "consumer", Revision::fresh()),
        DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        DesiredEntry::enabled("listener_x", "listener_x", Revision::fresh()),
        DesiredEntry::enabled("listener_y", "listener_y", Revision::fresh()),
    ])
    .expect("legal");
    h.settle();
    // Registry flap: present → absent → present.
    h.set_desired(vec![
        DesiredEntry::enabled("svc", "svc_b", Revision::new(9)),
        DesiredEntry::enabled("consumer", "consumer", Revision::fresh()),
    ])
    .expect("legal");
    h.settle();
    h.set_desired(final_desired()).expect("legal");
    h.settle();

    let mut fresh = standard_kernel();
    fresh.set_desired(final_desired()).expect("legal");
    fresh.settle();

    assert_eq!(
        h.snapshot(),
        fresh.snapshot(),
        "the settled history must equal a clean construction of the final \
         desired composition (Thm 80, §I.1 surfaces)\nhistory={}\nfresh={}",
        snap_fibers(&h),
        snap_fibers(&fresh),
    );
}

/// M3: consumer mounted before provider vs after provider — the order of
/// mounting is not observable at quiescence (B24 loader argument).
#[test]
fn m3_mount_order_is_invisible_at_quiescence() {
    let desired = || {
        vec![
            DesiredEntry::enabled("svc", "svc_a", Revision::new(1)),
            DesiredEntry::enabled("consumer", "consumer", Revision::fresh()),
            DesiredEntry::enabled("registry", "registry", Revision::fresh()),
            DesiredEntry::enabled("listener_x", "listener_x", Revision::fresh()),
        ]
    };
    let mut consumer_first = standard_kernel();
    consumer_first
        .set_desired(vec![
            DesiredEntry::enabled("consumer", "consumer", Revision::fresh()),
            DesiredEntry::enabled("listener_x", "listener_x", Revision::fresh()),
            DesiredEntry::enabled("svc", "svc_a", Revision::new(1)),
            DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        ])
        .expect("legal");
    consumer_first.settle();
    consumer_first.set_desired(desired()).expect("legal");
    consumer_first.settle();

    let mut provider_first = standard_kernel();
    provider_first.set_desired(desired()).expect("legal");
    provider_first.settle();

    assert_eq!(consumer_first.snapshot(), provider_first.snapshot());
}

/// M5: same-key contributions added/removed in opposite orders reach the
/// same final listener truth; dispatch truth equals registration truth by
/// construction of the fixture tokens (H.4).
#[test]
fn m5_contribution_orders_conflate() {
    let final_desired = || {
        vec![
            DesiredEntry::enabled("registry", "registry", Revision::fresh()),
            DesiredEntry::enabled("listener_x", "listener_x", Revision::fresh()),
            DesiredEntry::enabled("listener_y", "listener_y", Revision::fresh()),
        ]
    };
    // Order 1: x then y.
    let mut o1 = standard_kernel();
    o1.set_desired(vec![DesiredEntry::enabled(
        "registry",
        "registry",
        Revision::fresh(),
    )])
    .expect("legal");
    o1.settle();
    o1.set_desired(vec![
        DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        DesiredEntry::enabled("listener_x", "listener_x", Revision::fresh()),
    ])
    .expect("legal");
    o1.settle();
    o1.set_desired(final_desired()).expect("legal");
    o1.settle();

    // Order 2: y then x, with an intermediate removal of x.
    let mut o2 = standard_kernel();
    o2.set_desired(vec![
        DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        DesiredEntry::enabled("listener_y", "listener_y", Revision::fresh()),
    ])
    .expect("legal");
    o2.settle();
    o2.set_desired(vec![
        DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        DesiredEntry::enabled("listener_y", "listener_y", Revision::fresh()),
        DesiredEntry::enabled("listener_x", "listener_x", Revision::fresh()),
    ])
    .expect("legal");
    o2.settle();
    o2.set_desired(vec![
        DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        DesiredEntry::enabled("listener_y", "listener_y", Revision::fresh()),
    ])
    .expect("legal");
    o2.settle();
    o2.set_desired(final_desired()).expect("legal");
    o2.settle();

    assert_eq!(o1.snapshot(), o2.snapshot());
}

/// M13: root disposal from any quiescent state leaves the empty registry;
/// composition-owned resources are all released.
#[test]
fn m13_root_disposal_from_quiescent_state() {
    let mut k = standard_kernel();
    k.set_desired(vec![
        DesiredEntry::enabled("svc", "svc_a", Revision::new(1)),
        DesiredEntry::enabled("consumer", "consumer", Revision::fresh()),
        DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        DesiredEntry::enabled("listener_y", "listener_y", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    assert!(k.snapshot().quiet);

    k.dispose_root();
    let snap = k.snapshot();
    assert!(snap.fibers.is_empty(), "empty registry");
    assert!(snap.relations.is_empty(), "no surviving relations");
    assert!(snap.provisions.values().all(|v| v.is_empty()));
    assert!(snap.quiet);
}

// ---------------------------------------------------------------------------
// M4 — failure/recovery sanitation oracle (NOT a confluence history).
// ---------------------------------------------------------------------------

/// Activation fails once → revise succeeds. Assertions: the failed attempt
/// fully discharged; FAILED visible; the failed generation removed by an
/// explicit visible revision; no ghost composition state; the fresh
/// generation settles normally — its settled truth equals a clean build of
/// it. The whole history stays outside Thm 80 (§M.3).
#[test]
fn m4_fail_then_revise_is_sanitary() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(flaky_provider("flaky", 1, &l))
        .expect("component registered");
    k.register_component(tag_consumer("consumer", &l))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("flaky", "flaky", Revision::new(1)),
        DesiredEntry::enabled("consumer", "consumer", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();

    // Sanitation of the failed generation.
    let snap = k.snapshot();
    let flaky = snap.fibers.get("flaky").expect("present");
    assert_eq!(flaky.state, qianqian_composition::FiberState::Failed);
    assert!(flaky.failed_outcome, "FAILED visible");
    assert!(!flaky.teardown_violated, "fully discharged");
    assert_eq!(
        snap.fibers.get("consumer").map(|f| f.state),
        Some(qianqian_composition::FiberState::Pending),
        "the consumer degraded honestly, never crashed"
    );
    assert!(snap.quiet);

    // Explicit visible revision: fresh desired incarnation.
    k.set_desired(vec![
        DesiredEntry::enabled("flaky", "flaky", Revision::new(2)),
        DesiredEntry::enabled("consumer", "consumer", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();

    let snap = k.snapshot();
    let flaky = snap.fibers.get("flaky").expect("present");
    assert_eq!(flaky.state, qianqian_composition::FiberState::Active);
    assert!(
        !flaky.failed_outcome,
        "the fresh generation carries no failure"
    );
    assert_eq!(
        entries(&l)
            .iter()
            .filter(|e| *e == "flaky:attempt1")
            .count(),
        1,
        "the fresh generation activated exactly once — no silent reuse of the FAILED episode"
    );
    assert!(snap.quiet);

    // The settled post-recovery truth equals a clean build of the same
    // desired composition (the H′ suffix comparison, §M.3).
    let mut fresh = CompositionKernel::new();
    let lf = log();
    // A clean build registers an always-succeeding provider under the same
    // component name: the failure never happened in this world.
    fresh
        .register_component(always_provider("flaky", &lf))
        .expect("component registered");
    fresh
        .register_component(tag_consumer("consumer", &lf))
        .expect("component registered");
    fresh
        .set_desired(vec![
            DesiredEntry::enabled("flaky", "flaky", Revision::new(2)),
            DesiredEntry::enabled("consumer", "consumer", Revision::fresh()),
        ])
        .expect("legal");
    fresh.settle();
    assert_eq!(k.snapshot(), fresh.snapshot());
}

fn always_provider(name: &'static str, l: &Log) -> ComponentSpec {
    tag_provider(name, name, l)
}

/// A violated teardown is never sanitizable by revision: the latch holds,
/// step stays Blocked, and root disposal does not claim completion (§G.6).
#[test]
fn m4_shape_violated_teardown_is_not_sanitizable() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(violating_teardown_component("v", &l))
        .expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled("v", "v", Revision::fresh())])
        .expect("legal");
    k.settle();
    assert_eq!(
        k.snapshot().fibers.get("v").map(|f| f.state),
        Some(qianqian_composition::FiberState::Active)
    );

    // Withdraw: the teardown obligation violates.
    k.set_desired(Vec::new()).expect("legal");
    k.settle();
    let snap = k.snapshot();
    let v = snap.fibers.get("v").expect("the fiber stays installed");
    assert_eq!(v.state, qianqian_composition::FiberState::Unloading);
    assert!(v.teardown_violated);
    assert!(!snap.quiet);
    assert_eq!(k.step(), StepOutcome::Blocked);

    // Even a fresh desired incarnation cannot silently clear the latch:
    // the fiber is not removable, so no revision can mount past it.
    k.set_desired(vec![DesiredEntry::enabled("v", "v", Revision::new(2))])
        .expect("legal");
    k.settle();
    let snap = k.snapshot();
    assert!(snap.fibers.contains_key("v"), "still installed");
    assert!(snap.fibers.get("v").unwrap().teardown_violated);
    assert!(
        !snap.quiet,
        "no revision silently clears a latched violation"
    );
}
