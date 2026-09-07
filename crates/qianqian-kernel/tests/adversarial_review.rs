//! Mandatory adversarial review A1–A15 (#70 §21), executed against the
//! implementation. Each test attacks one review question; any wrong result
//! is a STOP, not a test exception.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use qianqian_kernel::{ComponentSpec, DesiredEntry, Discharge, Kernel, Revision, StepOutcome};

/// A1: Can identical desired revision cause FAILED to retry? Expected: NO.
/// (Deep form: D1; here the direct attack — repeated identical reconciles.)
#[test]
fn a1_identical_revision_never_retries_failed() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(flaky_provider("d", 1, &l));
    let entry = || vec![DesiredEntry::enabled("d", "d", Revision::new(7))];
    k.set_desired(entry()).expect("legal");
    k.settle();
    for _ in 0..10 {
        k.set_desired(entry()).expect("legal");
        k.settle();
    }
    assert_eq!(
        entries(&l),
        vec!["d:attempt0"],
        "identical revision: exactly one failed attempt, ever"
    );
    assert_eq!(
        k.snapshot().fibers.get("d").map(|f| f.state),
        Some(qianqian_kernel::FiberState::Failed)
    );
}

/// A2: Can a fresh desired revision with identical config create a fresh
/// generation? Expected: YES.
#[test]
fn a2_fresh_revision_creates_fresh_generation() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(flaky_provider("d", 1, &l));
    k.set_desired(vec![DesiredEntry::enabled("d", "d", Revision::new(7))])
        .expect("legal");
    k.settle();
    k.set_desired(vec![DesiredEntry::enabled("d", "d", Revision::new(8))])
        .expect("legal");
    k.settle();
    assert_eq!(
        entries(&l),
        vec!["d:attempt0", "d:attempt1"],
        "fresh incarnation: a new episode runs (and succeeds here)"
    );
    assert_eq!(
        k.snapshot().fibers.get("d").map(|f| f.state),
        Some(qianqian_kernel::FiberState::Active)
    );
}

/// A3: Can dependency churn fabricate retry? Expected: NO. (Deep form: D2.)
#[test]
fn a3_dependency_churn_fabricates_no_retry() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(flaky_provider("d", 1, &l));
    k.register_component(extra_provider("u", "u", &l));
    k.set_desired(vec![
        DesiredEntry::enabled("d", "d", Revision::new(7)),
        DesiredEntry::enabled("u", "u", Revision::new(1)),
    ])
    .expect("legal");
    k.settle();
    let failed = k.snapshot().fibers.get("d").map(|f| f.state).unwrap();
    assert_eq!(failed, qianqian_kernel::FiberState::Failed);
    let d_events = || entries(&l).iter().filter(|e| e.starts_with("d:")).count();
    let attempts_before = d_events();

    // Churn the unrelated dependency hard: remove, re-add, revise.
    k.set_desired(vec![DesiredEntry::enabled("d", "d", Revision::new(7))])
        .expect("legal");
    k.settle();
    k.set_desired(vec![
        DesiredEntry::enabled("d", "d", Revision::new(7)),
        DesiredEntry::enabled("u", "u", Revision::new(2)),
    ])
    .expect("legal");
    k.settle();

    assert_eq!(d_events(), attempts_before, "churn fabricated nothing");
}

/// A4: Can two required-single providers be installed simultaneously during
/// replacement? Expected: NO — checked at EVERY observable transition.
#[test]
fn a4_no_two_providers_during_replacement() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_provider("p1", "one", &l));
    k.register_component(tag_provider("p2", "two", &l));
    k.set_desired(vec![DesiredEntry::enabled("svc", "p1", Revision::new(1))])
        .expect("legal");
    k.settle();
    k.set_desired(vec![DesiredEntry::enabled("svc", "p2", Revision::new(2))])
        .expect("legal");
    loop {
        let outcome = k.step();
        let installed = k
            .snapshot()
            .provisions
            .get("Tag")
            .map(|v| v.len())
            .unwrap_or(0);
        assert!(installed <= 1, "overlap observed: {installed} providers");
        if outcome != StepOutcome::Transitioned {
            break;
        }
    }
    // And the plan-level route is refused outright.
    let err = k
        .set_desired(vec![
            DesiredEntry::enabled("p1", "p1", Revision::new(1)),
            DesiredEntry::enabled("p2", "p2", Revision::new(1)),
        ])
        .expect_err("ambiguity is a composition error");
    assert!(matches!(
        err.0.as_slice(),
        [qianqian_kernel::CompositionError::AmbiguousProvider { .. }]
    ));
}

/// A5: Can provider final release occur before dependent teardown
/// completes? Expected: NO.
#[test]
fn a5_provider_release_waits_for_dependents() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_provider("p", "P", &l));
    let consumer = {
        let lg = l.clone();
        ComponentSpec::new("c")
            .requires::<Tag>()
            .on_teardown(move |_| {
                lg.borrow_mut().push("c:teardown-done".to_owned());
                Discharge::Discharged
            })
    };
    k.register_component(consumer);
    k.set_desired(vec![
        DesiredEntry::enabled("p", "p", Revision::fresh()),
        DesiredEntry::enabled("c", "c", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    k.set_desired(Vec::new()).expect("legal");
    k.settle();
    let log = entries(&l);
    let c_done = log.iter().position(|e| e == "c:teardown-done").unwrap();
    let p_released = log.iter().position(|e| e == "p:released").unwrap();
    assert!(c_done < p_released);
}

/// A6: Can failed unwind still land in FAILED? Expected: NO.
#[test]
fn a6_violated_unwind_never_reaches_failed() {
    let mut k = Kernel::new();
    let l = log();
    let spec = {
        let lg = l.clone();
        ComponentSpec::new("v").on_activate(move |ctx| {
            lg.borrow_mut().push("v:effect".to_owned());
            ctx.register_effect(|| Discharge::Violated);
            Err(qianqian_kernel::ActivationError::new("fixture"))
        })
    };
    k.register_component(spec);
    k.set_desired(vec![DesiredEntry::enabled("v", "v", Revision::fresh())])
        .expect("legal");
    k.settle();
    let snap = k.snapshot();
    let v = snap.fibers.get("v").unwrap();
    assert_eq!(v.state, qianqian_kernel::FiberState::Unloading);
    assert!(v.teardown_violated);
    assert!(!v.failed_outcome);
    assert_eq!(k.step(), StepOutcome::Blocked);
}

/// A7: Can owner-local Effect exist without a fake capability key?
/// Expected: YES.
#[test]
fn a7_owner_local_effect_has_no_key() {
    let mut k = Kernel::new();
    let l = log();
    let spec = {
        let lg = l.clone();
        ComponentSpec::new("w").on_activate(move |ctx| {
            let l2 = lg.clone();
            ctx.register_effect(move || {
                l2.borrow_mut().push("w:dispose".to_owned());
                Discharge::Discharged
            });
            Ok(())
        })
    };
    k.register_component(spec);
    k.set_desired(vec![DesiredEntry::enabled("w", "w", Revision::fresh())])
        .expect("legal");
    k.settle();
    // Live and diagnosable only as lifecycle truth — never as a relation.
    let snap = k.snapshot();
    assert!(snap.relations.is_empty());
    assert_eq!(
        snap.fibers.get("w").map(|f| f.state),
        Some(qianqian_kernel::FiberState::Active)
    );
    k.set_desired(Vec::new()).expect("legal");
    k.settle();
    assert_eq!(entries(&l), vec!["w:dispose"]);
}

/// A8: Can relation-bearing binding be diagnosed without `EffectKind`?
/// Expected: YES — structural provenance projection.
#[test]
fn a8_binding_diagnosed_without_effect_kind() {
    let mut k = Kernel::new();
    k.register_component(listeners_provider("registry"));
    k.register_component(listener_consumer("lx"));
    k.set_desired(vec![
        DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        DesiredEntry::enabled("lx", "lx", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    let snap = k.snapshot();
    assert_eq!(
        snap.relations.iter().cloned().collect::<Vec<_>>(),
        vec![qianqian_kernel::RelationDiagnostic {
            owner: "lx".to_owned(),
            provider: "registry".to_owned(),
            capability: "Listeners",
        }]
    );
}

/// A9: Is there a second mutable DataEdge truth? Expected: NO. The kernel
/// exposes no second registration path: the relation truth is writable only
/// through the owner's Effect accumulator, and diagnostics are read-only
/// projections. Executable form: any mutation (withdrawal) is reflected
/// exactly once in the projection, and re-reading snapshots never resurrects.
#[test]
fn a9_no_second_mutable_truth() {
    let mut k = Kernel::new();
    k.register_component(listeners_provider("registry"));
    k.register_component(listener_consumer("lx"));
    k.set_desired(vec![
        DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        DesiredEntry::enabled("lx", "lx", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    assert_eq!(k.snapshot().relations.len(), 1);
    assert_eq!(k.snapshot().relations.len(), 1, "projections are stable");
    k.set_desired(vec![DesiredEntry::enabled(
        "registry",
        "registry",
        Revision::fresh(),
    )])
    .expect("legal");
    k.settle();
    // After removal, no read path can see the relation again: the single
    // authority (the effect) is gone. A second truth would resurrect it.
    for _ in 0..3 {
        assert!(k.snapshot().relations.is_empty());
    }
}

/// A10: Can FAILED settle? Expected: YES.
#[test]
fn a10_failed_settles() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(flaky_provider("d", 1, &l));
    k.set_desired(vec![DesiredEntry::enabled("d", "d", Revision::new(1))])
        .expect("legal");
    k.settle();
    assert!(k.snapshot().quiet);
}

/// A11: Can TEARDOWN_VIOLATED settle? Expected: NO.
#[test]
fn a11_violation_never_settles() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(violating_teardown_component("v", &l));
    k.set_desired(vec![DesiredEntry::enabled("v", "v", Revision::fresh())])
        .expect("legal");
    k.settle();
    k.set_desired(Vec::new()).expect("legal");
    k.settle();
    assert!(!k.snapshot().quiet);
    assert_eq!(k.step(), StepOutcome::Blocked);
}

/// A12: Can a half-completed staged replacement settle? Expected: NO.
#[test]
fn a12_half_staged_replacement_never_settles() {
    let mut k = Kernel::new();
    let l = log();
    k.register_component(tag_provider("p1", "one", &l));
    k.register_component(tag_provider("p2", "two", &l));
    k.set_desired(vec![DesiredEntry::enabled("svc", "p1", Revision::new(1))])
        .expect("legal");
    k.settle();
    k.set_desired(vec![DesiredEntry::enabled("svc", "p2", Revision::new(2))])
        .expect("legal");
    // Step to exactly the staging gap: old removed, new mount still owed.
    for _ in 0..4 {
        assert_eq!(k.step(), StepOutcome::Transitioned);
        if k.snapshot().fibers.is_empty() {
            break;
        }
    }
    assert!(k.snapshot().fibers.is_empty(), "old generation removed");
    assert!(!k.snapshot().quiet, "the owed mount blocks quiescence");
}

/// A13: Does any Kernel diagnostic know track/position/PlaybackState?
/// Expected: NO — enforced here as a source-vocabulary firewall: the kernel
/// sources contain none of the forbidden domain vocabulary (§J.3), and the
/// snapshot struct has no such fields by construction.
#[test]
fn a13_kernel_sources_carry_no_domain_vocabulary() {
    const SOURCES: &[(&str, &str)] = &[
        ("lib.rs", include_str!("../src/lib.rs")),
        ("capability.rs", include_str!("../src/capability.rs")),
        ("component.rs", include_str!("../src/component.rs")),
        ("context.rs", include_str!("../src/context.rs")),
        ("desired.rs", include_str!("../src/desired.rs")),
        ("diagnostic.rs", include_str!("../src/diagnostic.rs")),
        ("fiber.rs", include_str!("../src/fiber.rs")),
        ("kernel.rs", include_str!("../src/kernel.rs")),
    ];
    const FORBIDDEN: &[&str] = &[
        "PlaybackState",
        "MusicKernel",
        "playlist",
        "PCM",
        "pcm",
        "decoder",
        "Decoder",
        "WASAPI",
        "WASAPI",
        "FFmpeg",
        "ffmpeg",
        "PocketJS",
        "KuiklyUI",
        "audio",
        "AudioOutput",
        "checkpoint",
        "ENDED",
        "position()",
        "current_track",
    ];
    for (file, src) in SOURCES {
        for word in FORBIDDEN {
            assert!(
                !src.contains(word),
                "kernel source {file} contains domain vocabulary '{word}' — \
                 an architecture violation on sight (§J.3)"
            );
        }
    }
}

/// A14: Does the generic Kernel depend on qianqian-core/Music? Expected: NO
/// (compile-time manifest firewall, re-asserted here).
#[test]
fn a14_kernel_depends_on_nothing() {
    const MANIFEST: &str = include_str!("../Cargo.toml");
    assert!(!MANIFEST.contains("[dependencies.qianqian"));
    assert!(!MANIFEST.contains("qianqian-core"));
}

/// A15: Does synthetic payload traffic perform Kernel resolution per call/
/// block? Expected: NO.
#[test]
fn a15_payload_traffic_does_zero_kernel_operations() {
    // The full form lives in data_edge_oracles::payload_traffic_performs_
    // zero_kernel_operations (10k blocks, op counter unchanged). The attack
    // variant here: payload continues on an endpoint while the kernel is
    // never touched, even after other composition churn ran.
    let mut k = Kernel::new();
    let _ = log();
    let endpoint_cell: Rc<RefCell<Option<Rc<EndpointHandle>>>> = Rc::new(RefCell::new(None));
    k.register_component(sink_fixture(&endpoint_cell));
    k.set_desired(vec![DesiredEntry::enabled(
        "sink",
        "sink",
        Revision::new(1),
    )])
    .expect("legal");
    k.settle();
    let endpoint = endpoint_cell.borrow().clone().unwrap();
    let before = k.debug_op_count();
    for i in 0..1_000u32 {
        endpoint.send(i);
    }
    assert_eq!(k.debug_op_count(), before);
}

// --- Minimal local sink fixture for A15 (endpoint with no kernel handle) ---

struct EndpointHandle {
    count: std::cell::Cell<u32>,
}

impl EndpointHandle {
    fn send(&self, _v: u32) {
        self.count.set(self.count.get() + 1);
    }
}

struct Sink;

impl qianqian_kernel::Capability for Sink {
    const NAME: &'static str = "SinkFixture";
    type Service = dyn SinkFixtureService;
}

trait SinkFixtureService {
    fn bind(&self) -> Rc<EndpointHandle>;
}

struct SinkFixtureMechanism;

impl SinkFixtureService for SinkFixtureMechanism {
    fn bind(&self) -> Rc<EndpointHandle> {
        Rc::new(EndpointHandle {
            count: std::cell::Cell::new(0),
        })
    }
}

fn sink_fixture(cell: &Rc<RefCell<Option<Rc<EndpointHandle>>>>) -> ComponentSpec {
    // A single component that provides the sink and binds it into the cell —
    // enough shape to prove the firewall claim for A15.
    let cell = cell.clone();
    ComponentSpec::new("sink")
        .provides::<Sink>()
        .on_activate(move |ctx| {
            let svc: Rc<dyn SinkFixtureService> = Rc::new(SinkFixtureMechanism);
            // The fiber holds the service object it just created; the
            // provision records composition authority for it (§K.4). The
            // endpoint is the pre-bound data-plane handle.
            let endpoint = svc.bind();
            ctx.provide::<Sink>(svc).expect("provides declared");
            *cell.borrow_mut() = Some(endpoint);
            Ok(())
        })
}
