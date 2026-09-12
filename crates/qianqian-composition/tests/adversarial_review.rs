//! Mandatory adversarial review A1–A21 (#70 §21 + implementation
//! Corrective-1, review 5128371083; A21 = review 5128815134), executed
//! against the implementation. A16–A21 are the corrective oracles
//! (P0-1…P1-5 + the A21 landing-rule oracle); each test attacks one review
//! question; any wrong result is a STOP, not a test exception.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use qianqian_composition::{
    ComponentSpec, CompositionKernel, DesiredEntry, Discharge, Revision, StepOutcome,
};

/// A1: Can identical desired revision cause FAILED to retry? Expected: NO.
/// (Deep form: D1; here the direct attack — repeated identical reconciles.)
#[test]
fn a1_identical_revision_never_retries_failed() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(flaky_provider("d", 1, &l))
        .expect("component registered");
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
        Some(qianqian_composition::FiberState::Failed)
    );
}

/// A2: Can a fresh desired revision with identical config create a fresh
/// generation? Expected: YES.
#[test]
fn a2_fresh_revision_creates_fresh_generation() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(flaky_provider("d", 1, &l))
        .expect("component registered");
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
        Some(qianqian_composition::FiberState::Active)
    );
}

/// A3: Can dependency churn fabricate retry? Expected: NO. (Deep form: D2.)
#[test]
fn a3_dependency_churn_fabricates_no_retry() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(flaky_provider("d", 1, &l))
        .expect("component registered");
    k.register_component(extra_provider("u", "u", &l))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("d", "d", Revision::new(7)),
        DesiredEntry::enabled("u", "u", Revision::new(1)),
    ])
    .expect("legal");
    k.settle();
    let failed = k.snapshot().fibers.get("d").map(|f| f.state).unwrap();
    assert_eq!(failed, qianqian_composition::FiberState::Failed);
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
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_provider("p1", "one", &l))
        .expect("component registered");
    k.register_component(tag_provider("p2", "two", &l))
        .expect("component registered");
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
        [qianqian_composition::CompositionError::AmbiguousProvider { .. }]
    ));
}

/// A5: Can provider final release occur before dependent teardown
/// completes? Expected: NO.
#[test]
fn a5_provider_release_waits_for_dependents() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_provider("p", "P", &l))
        .expect("component registered");
    let consumer = {
        let lg = l.clone();
        ComponentSpec::new("c")
            .requires::<Tag>()
            .on_teardown(move |_| {
                lg.borrow_mut().push("c:teardown-done".to_owned());
                Discharge::Discharged
            })
    };
    k.register_component(consumer)
        .expect("component registered");
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
    let mut k = CompositionKernel::new();
    let l = log();
    let spec = {
        let lg = l.clone();
        ComponentSpec::new("v").on_activate(move |ctx| {
            lg.borrow_mut().push("v:effect".to_owned());
            ctx.register_effect(|| Discharge::Violated);
            Err(qianqian_composition::ActivationError::new("fixture"))
        })
    };
    k.register_component(spec).expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled("v", "v", Revision::fresh())])
        .expect("legal");
    k.settle();
    let snap = k.snapshot();
    let v = snap.fibers.get("v").unwrap();
    assert_eq!(v.state, qianqian_composition::FiberState::Unloading);
    assert!(v.teardown_violated);
    assert!(!v.failed_outcome);
    assert_eq!(k.step(), StepOutcome::Blocked);
}

/// A7: Can owner-local Effect exist without a fake capability key?
/// Expected: YES.
#[test]
fn a7_owner_local_effect_has_no_key() {
    let mut k = CompositionKernel::new();
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
    k.register_component(spec).expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled("w", "w", Revision::fresh())])
        .expect("legal");
    k.settle();
    // Live and diagnosable only as lifecycle truth — never as a relation.
    let snap = k.snapshot();
    assert!(snap.relations.is_empty());
    assert_eq!(
        snap.fibers.get("w").map(|f| f.state),
        Some(qianqian_composition::FiberState::Active)
    );
    k.set_desired(Vec::new()).expect("legal");
    k.settle();
    assert_eq!(entries(&l), vec!["w:dispose"]);
}

/// A8: Can relation-bearing binding be diagnosed without `EffectKind`?
/// Expected: YES — structural provenance projection.
#[test]
fn a8_binding_diagnosed_without_effect_kind() {
    let mut k = CompositionKernel::new();
    k.register_component(listeners_provider("registry"))
        .expect("component registered");
    k.register_component(listener_consumer("lx"))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        DesiredEntry::enabled("lx", "lx", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    let snap = k.snapshot();
    assert_eq!(
        snap.relations.iter().cloned().collect::<Vec<_>>(),
        vec![qianqian_composition::RelationDiagnostic {
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
    let mut k = CompositionKernel::new();
    k.register_component(listeners_provider("registry"))
        .expect("component registered");
    k.register_component(listener_consumer("lx"))
        .expect("component registered");
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
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(flaky_provider("d", 1, &l))
        .expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled("d", "d", Revision::new(1))])
        .expect("legal");
    k.settle();
    assert!(k.snapshot().quiet);
}

/// A11: Can TEARDOWN_VIOLATED settle? Expected: NO.
#[test]
fn a11_violation_never_settles() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(violating_teardown_component("v", &l))
        .expect("component registered");
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
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_provider("p1", "one", &l))
        .expect("component registered");
    k.register_component(tag_provider("p2", "two", &l))
        .expect("component registered");
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
    let mut k = CompositionKernel::new();
    let _ = log();
    let endpoint_cell: Rc<RefCell<Option<Rc<EndpointHandle>>>> = Rc::new(RefCell::new(None));
    k.register_component(sink_fixture(&endpoint_cell))
        .expect("component registered");
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

// ---------------------------------------------------------------------------
// Implementation Corrective-1 (review 5128371083): A16–A20
// ---------------------------------------------------------------------------

/// A fixture component whose activation and teardown both log a version tag.
fn logging_component(name: &'static str, version: &'static str, l: &Log) -> ComponentSpec {
    let log_a = l.clone();
    let log_t = l.clone();
    ComponentSpec::new(name)
        .on_activate(move |_| {
            log_a
                .borrow_mut()
                .push(format!("{name}:v{version}:activated"));
            Ok(())
        })
        .on_teardown(move |_| {
            log_t
                .borrow_mut()
                .push(format!("{name}:v{version}:teardown"));
            Discharge::Discharged
        })
}

/// A consumer whose relation-bearing binding effect's inverse violates:
/// the binding's provenance must stay authoritative even though the inverse
/// could not discharge (§K.4 — diagnostics are projections of the one
/// mutable authority, and a violated teardown must not pretend the binding
/// was cleaned).
fn violating_binding_consumer(name: &'static str) -> ComponentSpec {
    ComponentSpec::new(name)
        .requires::<Tag>()
        .on_activate(move |ctx| {
            let binding = ctx
                .resolve::<Tag>()
                .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
            ctx.register_relation(&binding, || Discharge::Violated);
            Ok(())
        })
}

/// Two distinct capability types sharing one diagnostic NAME — the P1-5
/// collision shape. Capability identity is the TypeId (§E.1); the snapshot
/// keys capability maps by NAME, so a shared name in one kernel would merge
/// distinct contracts in the §I.1 surface and let single-source oracles lie.
struct OutputA;
impl qianqian_composition::Capability for OutputA {
    const NAME: &'static str = "Output";
    type Service = dyn TagService;
}

struct OutputB;
impl qianqian_composition::Capability for OutputB {
    const NAME: &'static str = "Output";
    type Service = dyn TagService;
}

/// A16 (P0-1): Can a component definition be re-registered while an
/// instance of it is mounted (silent HMR)? Expected: NO — component
/// definitions are the static `(d, p, e)` (design §F.1), immutable for the
/// kernel's lifetime. The review's attack: `X-v1 ACTIVE → register X-v2 →
/// v1 卸载时跑 teardown-v2`. Registration of the same name is refused, so
/// teardown always runs the definition the fiber was mounted from.
#[test]
fn a16_component_definitions_are_immutable() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(logging_component("x", "1", &l))
        .expect("first registration is legal");
    k.set_desired(vec![DesiredEntry::enabled("x", "x", Revision::fresh())])
        .expect("legal");
    k.settle();
    assert_eq!(entries(&l), vec!["x:v1:activated"]);

    // The HMR attack: register X-v2 over the same name while X-v1 is ACTIVE.
    let err = k
        .register_component(logging_component("x", "2", &l))
        .expect_err("same-name re-registration is refused — no hot replacement");
    assert!(matches!(
        err,
        qianqian_composition::ComponentRegistrationError::DuplicateName { name: "x" }
    ));

    // Unload: the teardown that runs belongs to the ORIGINAL definition.
    k.set_desired(Vec::new()).expect("legal");
    k.settle();
    assert_eq!(
        entries(&l),
        vec!["x:v1:activated", "x:v1:teardown"],
        "the mounted fiber's (d, p, e) never silently changed"
    );
    assert!(k.snapshot().quiet);
}

/// A17 (P0-2): Does a violated inverse erase the binding's authoritative
/// provenance record? Expected: NO. The inverse is consumed (never
/// retried), but the record stays as a discharge-state tombstone: the
/// composition relation remains observable (§K.4 single authority), the
/// fiber stays installed, removal stays blocked, and the run is loudly
/// Blocked — the snapshot never shows "relation gone" next to a latched
/// TEARDOWN_VIOLATED.
#[test]
fn a17_violated_inverse_keeps_provenance_authority() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_provider("p", "P", &l))
        .expect("component registered");
    k.register_component(violating_binding_consumer("c"))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("p", "p", Revision::fresh()),
        DesiredEntry::enabled("c", "c", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();
    assert_eq!(
        k.snapshot().relations.iter().cloned().collect::<Vec<_>>(),
        vec![qianqian_composition::RelationDiagnostic {
            owner: "c".to_owned(),
            provider: "p".to_owned(),
            capability: "Tag",
        }]
    );

    // Withdraw the consumer: its binding inverse violates.
    k.set_desired(vec![DesiredEntry::enabled("p", "p", Revision::fresh())])
        .expect("legal");
    k.settle();
    let snap = k.snapshot();
    let c = snap.fibers.get("c").expect("the fiber stays installed");
    assert_eq!(c.state, qianqian_composition::FiberState::Unloading);
    assert!(c.teardown_violated, "TEARDOWN_VIOLATED latched");
    assert_eq!(
        snap.relations.iter().cloned().collect::<Vec<_>>(),
        vec![qianqian_composition::RelationDiagnostic {
            owner: "c".to_owned(),
            provider: "p".to_owned(),
            capability: "Tag",
        }],
        "the violated binding's provenance must NOT vanish from the snapshot"
    );
    assert!(!snap.quiet);
    assert_eq!(k.step(), StepOutcome::Blocked);
    // The tombstone blocks removal: no revision can silently clear it.
    k.set_desired(vec![DesiredEntry::enabled("c", "c", Revision::fresh())])
        .expect("legal");
    k.settle();
    assert!(k.snapshot().fibers.contains_key("c"));
    assert!(k.snapshot().fibers.get("c").unwrap().teardown_violated);
}

/// A18 (P0-3): Does the closed §I.1 surface prove the teardown-window
/// committed-binding facts? Expected: YES — the new `committed` projection
/// (consumer -> capability -> provider) makes the review's trace provable:
///
/// ```text
/// new resolution      -> old provider 已不可用  (capabilities = None)
/// consumer Unloading  -> committed binding 仍指向 old provider
/// reactivation        -> committed binding 改指 new provider
/// ```
///
/// A plain resolve with no relation Effect still appears here — `relations`
/// alone could not witness this window.
#[test]
fn a18_committed_bindings_trace_the_teardown_window() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_provider("p1", "one", &l))
        .expect("component registered");
    k.register_component(tag_provider("p2", "two", &l))
        .expect("component registered");
    k.register_component(tag_consumer("consumer", &l))
        .expect("component registered");
    // The consumer's desired incarnation is captured once and reused: only
    // the provider entry is revised by the replacement, so the step trace
    // is driven by the provider, not by consumer revision churn.
    let r_consumer = Revision::fresh();
    k.set_desired(vec![
        DesiredEntry::enabled("svc", "p1", Revision::new(1)),
        DesiredEntry::enabled("consumer", "consumer", r_consumer),
    ])
    .expect("legal");
    k.settle();
    let bound = |k: &CompositionKernel| {
        k.snapshot()
            .committed
            .get("consumer")
            .and_then(|m| m.get("Tag"))
            .cloned()
    };
    assert_eq!(
        bound(&k).as_deref(),
        Some("svc"),
        "an Active consumer's committed binding names its provider fiber"
    );
    // The plain-resolve consumer registers no relation Effect: `relations`
    // is empty while the committed binding exists (the P0-3 gap).
    assert!(k.snapshot().relations.is_empty());

    // Staged replacement svc@p1@R1 -> svc@p2@R2, one step at a time.
    k.set_desired(vec![
        DesiredEntry::enabled("svc", "p2", Revision::new(2)),
        DesiredEntry::enabled("consumer", "consumer", r_consumer),
    ])
    .expect("legal");
    assert_eq!(k.step(), StepOutcome::Transitioned); // 1. retire flag
    assert_eq!(k.step(), StepOutcome::Transitioned); // 2. L-Leave: svc -> Unloading
    let snap = k.snapshot();
    assert_eq!(
        snap.capabilities.get("Tag"),
        Some(&None),
        "the withdrawing provider is gone from NEW resolution"
    );
    assert_eq!(
        bound(&k).as_deref(),
        Some("svc"),
        "the committed binding still names the OLD provider while it withdraws"
    );
    assert_eq!(k.step(), StepOutcome::Transitioned); // 3. consumer -> Unloading
    let snap = k.snapshot();
    assert_eq!(
        snap.fibers.get("consumer").map(|f| f.state),
        Some(qianqian_composition::FiberState::Unloading)
    );
    assert_eq!(
        bound(&k).as_deref(),
        Some("svc"),
        "an Unloading consumer keeps its episode-fixed committed binding (B14 window)"
    );
    assert_eq!(k.step(), StepOutcome::Transitioned); // 4. consumer unloads (view closed)
    assert_eq!(
        bound(&k),
        None,
        "episode close discards the committed view last (§F.3)"
    );
    k.settle();
    assert!(k.snapshot().quiet);
    assert_eq!(
        bound(&k).as_deref(),
        Some("svc"),
        "reactivation commits the consumer to the NEW provider generation"
    );
    assert_eq!(
        entries(&l)
            .iter()
            .filter(|e| e.starts_with("consumer:bound-to-"))
            .collect::<Vec<_>>(),
        vec!["consumer:bound-to-one", "consumer:bound-to-two"],
        "the reactivated binding is against the new provider, not the old one (B30)"
    );
}

/// A19 (P1-4): Can `Revision::fresh()` collide with `Revision::new()`?
/// Expected: NO — raw operator tokens live strictly below the `fresh()`
/// domain, so a fresh incarnation can never be mistaken for an unchanged
/// raw one (R3/D3).
#[test]
fn a19_revision_tokens_never_collide() {
    let raw = [
        Revision::new(0),
        Revision::new(1),
        Revision::new(7),
        Revision::new((1 << 63) - 1),
    ];
    let fresh: Vec<Revision> = (0..64).map(|_| Revision::fresh()).collect();
    for a in &raw {
        for b in &fresh {
            assert_ne!(a, b, "raw and fresh token domains must be disjoint");
        }
    }
    for (i, a) in fresh.iter().enumerate() {
        for b in fresh.iter().skip(i + 1) {
            assert_ne!(a, b, "every fresh() call yields a distinct incarnation");
        }
    }
}

/// The disjoint-domain invariant is enforced, not documented: a raw token
/// inside the fresh() domain is a programmer error and panics loudly rather
/// than silently colliding with a future fresh incarnation.
#[test]
#[should_panic(expected = "Revision::fresh() token domain")]
fn a19b_raw_revision_cannot_enter_the_fresh_domain() {
    let _ = Revision::new(1 << 63);
}

/// Behavioral regression for the collision: presenting a fresh() token
/// right after a raw token MUST stage a visible revision (a new generation
/// mounts and activates). Under the old representation the process's first
/// fresh() token equaled Revision::new(1) and the revision silently became
/// a no-op (D3 violated).
#[test]
fn a19c_fresh_revision_after_raw_token_is_a_real_revision() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(tag_provider("p", "p1", &l))
        .expect("component registered");
    k.set_desired(vec![DesiredEntry::enabled("svc", "p", Revision::new(1))])
        .expect("legal");
    k.settle();
    let activations = || entries(&l).iter().filter(|e| *e == "p:activated").count();
    assert_eq!(activations(), 1);

    k.set_desired(vec![DesiredEntry::enabled("svc", "p", Revision::fresh())])
        .expect("legal");
    k.settle();
    assert_eq!(
        activations(),
        2,
        "a fresh() token must never equal the preceding raw token (R3/D3)"
    );
    assert!(k.snapshot().quiet);
}

/// A20 (P1-5): Can two distinct capability types share a diagnostic NAME in
/// one kernel? Expected: NO — refused at the registration door. Identity is
/// the TypeId; NAME is the vocabulary the §I.1 surfaces key by, so a
/// collision would merge distinct contracts and let single-source oracles
/// lie. The same TYPE under another component name stays legal.
#[test]
fn a20_capability_diagnostic_names_are_unique_per_kernel() {
    let mut k = CompositionKernel::new();
    let _ = log();
    k.register_component(ComponentSpec::new("a").provides::<OutputA>())
        .expect("first registration is legal");
    let err = k
        .register_component(ComponentSpec::new("b").provides::<OutputB>())
        .expect_err("a distinct capability type reusing the diagnostic NAME is refused");
    assert!(matches!(
        err,
        qianqian_composition::ComponentRegistrationError::DuplicateCapabilityName {
            name: "Output"
        }
    ));
    // Same contract (same TypeId) under a different component name is fine.
    k.register_component(ComponentSpec::new("b").provides::<OutputA>())
        .expect("the same capability type may be declared by another component");
    // Same-name re-registration of a component definition is refused (P0-1).
    let err = k
        .register_component(ComponentSpec::new("a").provides::<OutputA>())
        .expect_err("component definitions are immutable");
    assert!(matches!(
        err,
        qianqian_composition::ComponentRegistrationError::DuplicateName { name: "a" }
    ));
}

/// A21 (review 5128815134, P0): Can an explicit dispose-violation during
/// activation still land the fiber in ACTIVE? Expected: NO — a latched §G.6
/// violation outranks activation success. TEARDOWN_VIOLATED ⇒ NEVER ACTIVE:
/// the episode stays open in Unloading, the violated tombstone and the
/// provision record remain, the provided key never enters NEW resolution, a
/// later consumer requiring it can neither activate nor commit, quiet stays
/// false, and the run ends loudly Blocked.
#[test]
fn a21_explicit_dispose_violation_never_lands_active() {
    let mut k = CompositionKernel::new();
    let l = log();
    k.register_component(listeners_provider("registry"))
        .expect("component registered");
    k.register_component(dispose_violating_provider("p", "P", &l))
        .expect("component registered");
    k.register_component(tag_consumer("c", &l))
        .expect("component registered");
    k.set_desired(vec![
        DesiredEntry::enabled("registry", "registry", Revision::fresh()),
        DesiredEntry::enabled("p", "p", Revision::fresh()),
        DesiredEntry::enabled("c", "c", Revision::fresh()),
    ])
    .expect("legal");
    k.settle();

    let snap = k.snapshot();
    let p = snap.fibers.get("p").expect("the provider stays installed");
    assert_eq!(
        p.state,
        qianqian_composition::FiberState::Unloading,
        "TEARDOWN_VIOLATED ⇒ NEVER ACTIVE, even though activation returned Ok"
    );
    assert!(p.teardown_violated, "the dispose violation is latched");

    // The provided key never enters NEW resolution: the fiber is not ACTIVE,
    // so the key is unresolvable despite the provision record being present.
    assert_eq!(
        snap.capabilities.get("Tag"),
        Some(&None),
        "a poisoned provider must leave its key OUT of new resolution"
    );
    assert_eq!(
        snap.provisions.get("Tag").map(|v| v.len()),
        Some(1),
        "the installed provision record survives (installed-record may remain)"
    );

    // A new consumer requiring the key can neither activate nor commit.
    assert_eq!(
        snap.fibers.get("c").map(|f| f.state),
        Some(qianqian_composition::FiberState::Pending),
        "no consumer may activate or commit against the poisoned provider"
    );
    assert!(
        !entries(&l).iter().any(|e| e.starts_with("c:bound-to-")),
        "no consumer ever resolved the poisoned provider"
    );

    // The violated relation's provenance stays authoritative (§K.4): the
    // binding the violated inverse carried must not vanish from diagnostics.
    assert!(
        snap.relations
            .iter()
            .any(|r| { r.owner == "p" && r.provider == "registry" && r.capability == "Listeners" }),
        "the violated effect's provenance record must remain observable"
    );

    // A latched violation never settles and the run ends loudly Blocked.
    assert!(!snap.quiet, "TEARDOWN_VIOLATED never settles");
    assert_eq!(k.step(), StepOutcome::Blocked);

    // The episode never closes: withdrawing the provider from desired cannot
    // retire or remove the fiber; the name stays held, still latched.
    k.set_desired(vec![DesiredEntry::enabled(
        "registry",
        "registry",
        Revision::fresh(),
    )])
    .expect("legal");
    k.settle();
    assert!(k.snapshot().fibers.contains_key("p"));
    assert!(k.snapshot().fibers.get("p").unwrap().teardown_violated);
}

/// A provider whose activation explicitly disposes one of its own effects
/// whose inverse violates, then still provides its declared key and returns
/// Ok — the A21 attack shape. The correct landing is Unloading +
/// TEARDOWN_VIOLATED with the episode left open, never ACTIVE.
fn dispose_violating_provider(name: &'static str, tag: &'static str, l: &Log) -> ComponentSpec {
    let log = l.clone();
    ComponentSpec::new(name)
        .requires::<Listeners>()
        .provides::<Tag>()
        .on_activate(move |ctx| {
            let binding = ctx
                .resolve::<Listeners>()
                .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
            let lg = log.clone();
            let handle = ctx.register_relation(&binding, move || {
                lg.borrow_mut()
                    .push(format!("{name}:relation-inverse-violated"));
                Discharge::Violated
            });
            log.borrow_mut().push(format!("{name}:dispose-own-effect"));
            ctx.dispose(handle);
            ctx.provide::<Tag>(Rc::new(FixedTag(tag)))
                .expect("provides declared");
            log.borrow_mut().push(format!("{name}:activation-ok"));
            Ok(())
        })
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

impl qianqian_composition::Capability for Sink {
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
