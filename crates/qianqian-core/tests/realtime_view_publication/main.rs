//! Test-only executable evidence for the realtime-view publication /
//! reclamation mechanism.
//!
//! Nothing in this file tree is part of the `qianqian-core` library API.
//!
//! The mechanism under test: coherent immutable-view publication with
//! per-resource refcounted reclamation (safe Rust, std only). The realtime
//! side acquires one pre-bound view at bind time and executes quanta
//! through already-held handles; the control side publishes, retires,
//! certifies quiescence and physically releases. The scenarios exercise the
//! normative P1–P5 semantics (coherent publication, retired-view closure,
//! all-generation quiescence, lifecycle separation, conditional progress)
//! with adversarial twins whose kills are read from execution state.
//!
//! Layout:
//!
//! ```text
//! realtime_view_publication/main.rs          scenario + mutation tests
//! realtime_view_publication/resources.rs     tracked resources + observer oracle
//! realtime_view_publication/view.rs          the immutable realtime view
//! realtime_view_publication/publication.rs   the mechanism under test
//! realtime_view_publication/mutations.rs     adversarial twins + weak-handle reader
//! ```
//!
//! Run:
//!
//! ```bash
//! cargo test -p qianqian-core --test realtime_view_publication
//! ```

#[path = "../common/counting_allocator.rs"]
mod counting_allocator;
mod mutations;
mod publication;
mod resources;
mod view;

use std::sync::Arc;
use std::sync::mpsc;

use counting_allocator::run_counting_allocations;
use mutations::{
    CertificationBug, CertificationTwin, PublisherWaitsForReader, ReleaseAtPublication,
    SplitPublication, StaleAcquisition, WeakHandlesReader,
};
use publication::{PublishedViews, Reader, ViewState};
use resources::{DestructorGate, EventLog, ResourceEvent, TrackedResource, new_event_log};
use view::RealtimeView;

/// Builds one immutable view with freshly tracked resources.
fn make_view(
    identity: u64,
    topology_version: u64,
    participants: &[&'static str],
    resources: Vec<Arc<TrackedResource>>,
    _log: EventLog,
) -> Arc<RealtimeView> {
    Arc::new(RealtimeView::new(
        identity,
        topology_version,
        participants.to_vec(),
        resources,
    ))
}

/// Creates one tracked resource per id.
fn tracked_resources(ids: &[u64], log: &EventLog) -> Vec<Arc<TrackedResource>> {
    ids.iter()
        .map(|id| TrackedResource::new(*id, log.clone()))
        .collect()
}

/// All destruction events from the observer log, in order.
fn destroyed_events(log: &EventLog) -> Vec<(u64, String)> {
    log.lock()
        .expect("observer log lock")
        .iter()
        .filter_map(|event| match event {
            ResourceEvent::Destroyed { id, context } => Some((*id, context.clone())),
            _ => None,
        })
        .collect()
}

/// The three-participant shape of the direct-flow baseline.
const FLOW_PARTICIPANTS: &[&str] = &["stream_source", "processing_stage", "stream_sink"];

// ---------------------------------------------------------------------------
// P1 — coherent publication
// ---------------------------------------------------------------------------

#[test]
fn acquisition_observes_whole_views_only() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    let publication = PublishedViews::new(view0);

    let first = publication.acquire();
    assert_eq!(first.identity(), 0);
    assert!(first.has_participant("processing_stage"));
    assert_eq!(first.resource_ids(), vec![10, 11]);

    publication.publish(view1);

    // After publication the reader observes the whole new view: identity,
    // participant topology and resource membership all belong together.
    let second = publication.acquire();
    assert_eq!(second.identity(), 1);
    assert!(second.has_participant("stream_sink"));
    assert_eq!(second.resource_ids(), vec![12, 13]);
    assert_eq!(publication.current_identity(), 1);
}

#[test]
fn split_publication_produces_mixed_generation_observation() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    let twin = SplitPublication::new(&view0);

    // The defect: the topology half lands before the resource half. A reader
    // acquiring in the window observes topology from the new view and the
    // resource table from the old view — the split-publication witness.
    twin.publish_topology_step(&view1);
    let mixed = twin.acquire();
    assert!(
        !mixed.is_coherent(),
        "the split twin lets a reader observe a mixed-generation view"
    );
    assert_eq!(mixed.topology_identity, 1);
    assert_eq!(mixed.topology_version, 1);
    assert_eq!(mixed.resource_identity, 0);
    // The mixed observation carries the new topology with the old resource
    // table — the exact half-N / half-N+1 shape.
    assert!(mixed.participants.contains(&"processing_stage"));
    assert_eq!(
        mixed.resources.iter().map(|r| r.id()).collect::<Vec<_>>(),
        vec![10, 11]
    );

    // Once the second half lands, the same twin produces a whole view —
    // coherence is an ordering property, not a type property.
    twin.publish_resources_step(&view1);
    let whole = twin.acquire();
    assert!(
        whole.is_coherent(),
        "a completed split publication is whole"
    );
}

// ---------------------------------------------------------------------------
// P2 — retired-view closure
// ---------------------------------------------------------------------------

#[test]
fn new_acquisition_never_enters_a_retired_view() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    let publication = PublishedViews::new(view0);

    // Reader A holds the old view before publication.
    let existing = Reader::new(publication.acquire());
    publication.publish(view1);

    // Reader B acquires after publication and must get the new view.
    let fresh = Reader::new(publication.acquire());
    assert_eq!(
        fresh.view_identity(),
        1,
        "new acquisition must not enter the retired view"
    );
    assert_eq!(
        existing.view_identity(),
        0,
        "the existing holder keeps its view"
    );

    // The existing holder may legally finish using the retired view.
    existing
        .execute(3)
        .expect("the existing holder finishes legally");
    assert_eq!(publication.state_of(0), ViewState::Retired);
}

#[test]
fn stale_acquisition_enters_a_retired_view_and_is_detected() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    let twin = StaleAcquisition::new(view0);

    // A reader observes the current identity, then publication happens.
    assert_eq!(twin.acquire().identity(), 0);
    twin.publish(view1);

    // The defect: a subsequent acquisition resolves the stale identity
    // without revalidating closure, so it enters the retired view.
    let stale = twin
        .acquire_stale(0)
        .expect("the stale twin hands out the retired view");
    assert_eq!(
        stale.identity(),
        0,
        "the stale acquisition entered a retired view"
    );
    assert_eq!(
        twin.acquire().identity(),
        1,
        "the honest acquisition path returns the current view"
    );
    assert_ne!(
        stale.identity(),
        twin.acquire().identity(),
        "the stale acquisition returned a retired view while the current view is 1"
    );
}

// ---------------------------------------------------------------------------
// P3 — quiescence before reclamation across generations
// ---------------------------------------------------------------------------

#[test]
fn old_reader_finishes_after_publication() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    let publication = PublishedViews::new(view0);

    let old_reader = Reader::new(publication.acquire());
    publication.publish(view1);

    // The old reader continues executing the retired view: its resources
    // must remain valid until the reader exits.
    old_reader
        .execute(5)
        .expect("the old reader dereferences view 0 legally after publication");
    assert_eq!(publication.reader_hold_count(old_reader.view()), 1);
    assert!(
        old_reader.view().resources().iter().all(|r| r.is_alive()),
        "the old view's resources are still alive while the old reader holds it"
    );
    assert!(
        !publication.certify_reclaimable(0),
        "the old reader keeps the retired view non-reclaimable"
    );
    assert_eq!(publication.state_of(0), ViewState::Retired);
}

#[test]
fn release_at_publication_is_an_observable_use_after_release() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    let twin = ReleaseAtPublication::new(view0);

    // The stale-bound-handle reader holds only weak handles and re-resolves
    // each dereference (the decoupled-lifetime shape). Before publication
    // its dereferences are legal.
    let old_reader = {
        let view = twin.acquire();
        let reader = WeakHandlesReader::from_view(&view);
        drop(view);
        reader
    };
    assert_eq!(
        old_reader.view_identity(),
        0,
        "the weak reader holds view 0"
    );
    old_reader
        .execute_quantum(1)
        .expect("dereference is legal before publication");

    // The defect: publication releases the old view immediately.
    twin.publish(view1);

    // The old reader's next dereference observes the release.
    let error = old_reader
        .execute_quantum(1)
        .expect_err("release at publication must be observable as use-after-release");
    assert!(
        matches!(error, resources::UseAfterRelease { id: 10 | 11 }),
        "the failing dereference names the released resource: {error:?}"
    );
}

#[test]
fn queued_reference_counts_against_reclamation() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    let publication = PublishedViews::new(view0);

    // Acquire a reference and queue it: execution has not started.
    let queued = publication.acquire();
    publication.publish(view1);

    assert!(
        !publication.certify_reclaimable(0),
        "a queued reference counts against quiescence: N must not be reclaimable"
    );

    // The queued reference may legally execute later.
    queued
        .execute_quantum(2)
        .expect("the queued reference dereferences view 0 legally");
    assert_eq!(publication.state_of(0), ViewState::Retired);

    // Only when the queued reference exits does progress become possible.
    drop(queued);
    assert!(
        publication.certify_reclaimable(0),
        "quiescence is recognized after the queued reference exits"
    );
}

#[test]
fn certification_ignoring_queued_references_is_detected() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    let view2 = make_view(
        2,
        2,
        FLOW_PARTICIPANTS,
        tracked_resources(&[14, 15], &log),
        log.clone(),
    );
    let publication = PublishedViews::new(view0);
    let twin = CertificationTwin::new(publication, CertificationBug::ActiveExecutionsOnly);

    // A reference to view 0 is acquired and queued (not executing); view 1
    // is actively executing; view 2 becomes current.
    let queued = twin.inner().acquire();
    twin.inner().publish(view1);
    let active = twin.inner().acquire();
    twin.inner().publish(view2);
    twin.begin_execution(1);
    active
        .execute_quantum(1)
        .expect("the active execution dereferences view 1 legally");

    // The defect: certification counts only *active* executions, so the
    // queued reference to view 0 is invisible and the view is claimed
    // reclaimable — while the actively executing view 1 is (correctly) kept.
    let claims = twin.certify_all();
    assert!(
        claims.contains(&0),
        "the twin claims the view with the queued reference is reclaimable"
    );
    assert!(
        !claims.contains(&1),
        "the twin keeps the view with an active execution"
    );
    assert_eq!(twin.certified_claims(), claims);
    assert!(
        twin.inner().reader_hold_count(&queued) > 0,
        "the queued reference still holds the view: the claim contradicts the mechanism state"
    );
    twin.end_execution(1);

    // The honest mechanism refuses the same certification.
    assert!(
        !twin.inner().certify_reclaimable(0),
        "the honest certification requires all queued references to be gone"
    );
    drop(queued);
    assert!(twin.inner().certify_reclaimable(0));
}

#[test]
fn three_generations_overlap_with_shared_resources() {
    let log = new_event_log();
    let a = TrackedResource::new(10, log.clone());
    let b = TrackedResource::new(11, log.clone());
    let c = TrackedResource::new(12, log.clone());
    let d = TrackedResource::new(13, log.clone());
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        vec![a.clone(), b.clone()],
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        vec![a.clone(), c.clone()],
        log.clone(),
    );
    let view2 = make_view(2, 2, FLOW_PARTICIPANTS, vec![a.clone(), d], log.clone());
    let publication = PublishedViews::new(view0);

    // R1 holds N, R2 holds N+1, current becomes N+2.
    let reader1 = Reader::new(publication.acquire());
    publication.publish(view1);
    let reader2 = Reader::new(publication.acquire());
    publication.publish(view2);

    assert_eq!(publication.state_of(0), ViewState::Retired);
    assert_eq!(publication.state_of(1), ViewState::Retired);
    assert_eq!(publication.state_of(2), ViewState::Live);

    // Reclaim eligibility depends on ANY still-reachable generation: neither
    // retired view is reclaimable while its own reader holds it.
    assert!(!publication.certify_reclaimable(0), "R1 holds N");
    assert!(!publication.certify_reclaimable(1), "R2 holds N+1");

    // R1 exits: N becomes reclaimable, N+1 still not.
    drop(reader1);
    assert!(publication.certify_reclaimable(0));
    assert!(!publication.certify_reclaimable(1), "R2 still holds N+1");

    // R2 exits: N+1 becomes reclaimable; N+2 stays live.
    drop(reader2);
    assert!(publication.certify_reclaimable(1));
    assert_eq!(publication.state_of(2), ViewState::Live);
    assert!(
        publication.release_reclaimable(0) && publication.release_reclaimable(1),
        "both retired generations are physically released"
    );
}

#[test]
fn certification_ignoring_older_generations_is_detected() {
    let log = new_event_log();
    let a = TrackedResource::new(10, log.clone());
    let b = TrackedResource::new(11, log.clone());
    let c = TrackedResource::new(12, log.clone());
    let d = TrackedResource::new(13, log.clone());
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        vec![a.clone(), b.clone()],
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        vec![a.clone(), c.clone()],
        log.clone(),
    );
    let view2 = make_view(2, 2, FLOW_PARTICIPANTS, vec![a.clone(), d], log.clone());
    let publication = PublishedViews::new(view0);
    let twin = CertificationTwin::new(publication, CertificationBug::LatestRetiredOnly);

    // R1 holds N; R2 holds N+1 and exits; current becomes N+2.
    let reader1 = twin.inner().acquire();
    twin.inner().publish(view1);
    let reader2 = twin.inner().acquire();
    twin.inner().publish(view2);
    drop(reader2);

    // The latest retired view (N+1) is quiescent. The defect: the twin
    // certifies every retired view, forgetting that R1 still holds N.
    let claims = twin.certify_all();
    assert!(
        claims.contains(&0),
        "the twin claims the older generation is reclaimable"
    );
    assert!(claims.contains(&1));
    assert!(
        twin.inner().reader_hold_count(&reader1) > 0,
        "R1 still holds N: the claim contradicts the mechanism state"
    );

    // The honest mechanism certifies each generation by its own quiescence.
    assert!(
        !twin.inner().certify_reclaimable(0),
        "N is not quiescent while R1 holds it"
    );
    drop(reader1);
    assert!(twin.inner().certify_reclaimable(0));
}

#[test]
fn shared_resources_survive_per_view_retirement() {
    let log = new_event_log();
    let a = TrackedResource::new(10, log.clone());
    let b = TrackedResource::new(11, log.clone());
    let c = TrackedResource::new(12, log.clone());
    // Aliveness is observed through weak handles so the test's own strong
    // references do not keep the resources alive.
    let a_weak = Arc::downgrade(&a);
    let b_weak = Arc::downgrade(&b);
    let c_weak = Arc::downgrade(&c);
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        vec![a.clone(), b.clone()],
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        vec![a.clone(), c.clone()],
        log.clone(),
    );
    // A later publication retires view 1 so it can be certified and
    // physically released; the successor does not reference A.
    let view2 = make_view(
        2,
        2,
        FLOW_PARTICIPANTS,
        tracked_resources(&[20, 21], &log),
        log.clone(),
    );
    drop(a);
    drop(b);
    drop(c);
    let publication = PublishedViews::new(view0);

    let old_reader = Reader::new(publication.acquire());
    publication.publish(view1);
    publication.publish(view2);
    drop(old_reader);

    // View N retires and is physically released…
    assert!(publication.certify_reclaimable(0));
    assert!(publication.release_reclaimable(0));

    // …but resource A is shared with N+1: per-view retirement must not
    // release a resource that another generation still references.
    assert!(
        a_weak.upgrade().is_some(),
        "shared resource A survives the release of view N"
    );
    assert!(
        b_weak.upgrade().is_none(),
        "resource B existed only in N and is released with it"
    );
    assert!(c_weak.upgrade().is_some());

    // Only when the last referencing view is released does A go.
    assert!(publication.certify_reclaimable(1));
    assert!(publication.release_reclaimable(1));
    assert!(
        a_weak.upgrade().is_none(),
        "A is destroyed when the last referencing view is released"
    );
    assert!(c_weak.upgrade().is_none());
}

// ---------------------------------------------------------------------------
// P4 / P5 — lifecycle separation and conditional progress
// ---------------------------------------------------------------------------

#[test]
fn stalled_reader_blocks_reclaimability_until_it_exits() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    let publication = PublishedViews::new(view0);

    // A stalled reader never exits during the observation window.
    let stalled = Reader::new(publication.acquire());
    publication.publish(view1);

    // Retirement happened; reclaimability did not — this is a legal state,
    // not a defect (P4: retired != reclaimable; P5: progress is conditional).
    assert_eq!(publication.state_of(0), ViewState::Retired);
    assert!(
        !publication.certify_reclaimable(0),
        "a stalled reader blocks reclaimability without timing out as a failure"
    );
    stalled
        .execute(1)
        .expect("the stalled reader still dereferences N legally");

    // The stalled reader finally exits; progress becomes possible.
    drop(stalled);
    assert!(
        publication.certify_reclaimable(0),
        "quiescence is recognized once the reader exits"
    );
    assert_eq!(
        publication.reclaimable_identities(),
        vec![0],
        "the certified view moved into the reclaimable ledger"
    );
    assert_eq!(publication.state_of(0), ViewState::Reclaimable);

    assert!(publication.release_reclaimable(0));
    assert_eq!(publication.state_of(0), ViewState::Released);
}

#[test]
fn reclaimable_does_not_mean_released() {
    let log = new_event_log();
    let resources = tracked_resources(&[10, 11], &log);
    let weak_handles: Vec<_> = resources.iter().map(Arc::downgrade).collect();
    let view0 = make_view(0, 0, FLOW_PARTICIPANTS, resources, log.clone());
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    let publication = PublishedViews::new(view0);

    publication.publish(view1);
    assert!(publication.certify_reclaimable(0));
    assert_eq!(publication.state_of(0), ViewState::Reclaimable);

    // Certification is a semantic state change: the resources are still
    // alive and have not been physically destroyed.
    assert!(
        weak_handles.iter().all(|handle| handle.upgrade().is_some()),
        "reclaimable does not mean physically released"
    );
    assert!(
        destroyed_events(&log).is_empty(),
        "nothing was destroyed at certification"
    );

    // Physical release is a separate later event.
    assert!(publication.release_reclaimable(0));
    let destroyed = destroyed_events(&log);
    assert_eq!(
        destroyed.len(),
        2,
        "physical release destroys the view's resources"
    );
    assert!(weak_handles.iter().all(|handle| handle.upgrade().is_none()));
}

// ---------------------------------------------------------------------------
// Final destructor authority
// ---------------------------------------------------------------------------

#[test]
fn final_destruction_runs_on_the_control_thread_when_reader_releases_first() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    let publication = Arc::new(PublishedViews::new(view0));

    let (acquired_tx, acquired_rx) = mpsc::channel();
    let (exit_signal_tx, exit_signal_rx) = mpsc::channel();
    let (released_tx, released_rx) = mpsc::channel();
    let reader_publication = publication.clone();
    let reader_handle = std::thread::Builder::new()
        .name("rt-reader".to_string())
        .spawn(move || {
            let view = reader_publication.acquire();
            acquired_tx.send(()).expect("the reader acquired");
            view.execute_quantum(1).expect("the held view is valid");
            exit_signal_rx
                .recv()
                .expect("control asks the reader to exit");
            drop(view);
            released_tx.send(()).expect("reader release is reported");
        })
        .expect("the reader thread spawns");

    // Deterministic: the reader holds view 0 before publication happens.
    acquired_rx.recv().expect("the reader acquired view 0");
    publication.publish(view1);
    assert!(
        !publication.certify_reclaimable(0),
        "the reader still holds view 0: certification must not pass"
    );

    // The reader exits on its own thread; the ledger still holds view 0.
    exit_signal_tx.send(()).expect("exit signal");
    released_rx.recv().expect("reader released");

    // Physical release happens on the control thread, not the reader thread.
    assert!(publication.certify_reclaimable(0));
    assert!(publication.release_reclaimable(0));
    reader_handle.join().expect("the reader thread joins");

    let destroyed = destroyed_events(&log);
    assert_eq!(
        destroyed.len(),
        2,
        "the retired view's resources are destroyed"
    );
    assert!(
        destroyed
            .iter()
            .all(|(_, context)| !context.starts_with("rt-reader")),
        "no resource was destroyed on the realtime reader thread: {destroyed:?}"
    );
}

#[test]
fn final_drop_on_the_reader_thread_destroys_there_hazard_witness() {
    let log = new_event_log();
    let view = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );

    // The extracted pre-bound shape: the flow is handed to the realtime side
    // as its only strong reference, and no control-side ledger holds it.
    // The last drop therefore runs wherever the flow dies.
    let reader_handle = std::thread::Builder::new()
        .name("rt-reader".to_string())
        .spawn(move || drop(view))
        .expect("the reader thread spawns");
    reader_handle.join().expect("the reader thread joins");

    let destroyed = destroyed_events(&log);
    assert_eq!(
        destroyed.len(),
        2,
        "the flow's resources are destroyed at the final drop"
    );
    assert!(
        destroyed
            .iter()
            .all(|(_, context)| context.starts_with("rt-reader")),
        "with no ledger, physical destruction runs on the realtime reader thread: {destroyed:?}"
    );
}

// ---------------------------------------------------------------------------
// Physical destruction outside the publication lock
// ---------------------------------------------------------------------------

#[test]
fn blocking_destruction_does_not_hold_the_publication_lock() {
    let log = new_event_log();
    let destructor_gate = DestructorGate::new();
    let (acquire_done_tx, acquire_done_rx) = mpsc::channel();

    // View 0 carries one resource whose destructor blocks until the test
    // releases the gate; view 1 is the successor.
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        vec![TrackedResource::new_blocking(
            10,
            log.clone(),
            destructor_gate.clone(),
        )],
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[20, 21], &log),
        log.clone(),
    );
    let publication = Arc::new(PublishedViews::new(view0));
    publication.publish(view1);
    assert!(
        publication.certify_reclaimable(0),
        "view 0 has no reader and is certified reclaimable"
    );

    // Control thread: physical release must detach the view under the lock
    // and drop it only after unlocking, so the blocking destructor runs
    // outside the publication critical section.
    let control_publication = publication.clone();
    let control = std::thread::spawn(move || {
        assert!(
            control_publication.release_reclaimable(0),
            "the reclaimable view is physically released"
        );
    });

    // Deterministic: wait until the resource destructor has entered and is
    // blocked inside `Drop` before allowing any acquire probe.
    destructor_gate.wait_entered();

    // While the destructor remains blocked, another reader must be able to
    // acquire the current view: physical destruction must not hold the
    // publication lock.
    let probe_publication = publication.clone();
    let probe = std::thread::spawn(move || {
        let view = probe_publication.acquire();
        let identity = view.identity();
        drop(view);
        acquire_done_tx.send(identity).expect("the probe acquired");
    });

    // If release_reclaimable dropped the view inside the lock, the probe
    // would be stuck on the mutex and this receive would time out.
    let identity = acquire_done_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("acquire must progress while the destructor is blocked");
    assert_eq!(
        identity, 1,
        "the probe acquires the current view while destruction is in progress"
    );

    // Only now release the destructor so the control thread can finish.
    destructor_gate.release();
    control.join().expect("the control thread joins");
    probe.join().expect("the probe thread joins");

    let destroyed = destroyed_events(&log);
    assert_eq!(
        destroyed.len(),
        1,
        "the blocking resource is destroyed exactly once"
    );
}

// ---------------------------------------------------------------------------
// Realtime acquire/release engineering
// ---------------------------------------------------------------------------

#[test]
fn acquisition_and_release_are_allocation_free() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    let publication = PublishedViews::new(view0);
    publication.publish(view1);

    let (acquired, acquire_allocations) = run_counting_allocations(|| publication.acquire());
    assert_eq!(
        acquire_allocations, 0,
        "bind-time acquisition performs no heap allocation"
    );
    assert_eq!(acquired.identity(), 1);

    let ((), release_allocations) = run_counting_allocations(|| drop(acquired));
    assert_eq!(
        release_allocations, 0,
        "reader release performs no heap allocation"
    );

    // Steady-state acquire/release cycles are allocation-free.
    let ((), cycle_allocations) = run_counting_allocations(|| {
        for _ in 0..64 {
            let view = publication.acquire();
            drop(view);
        }
    });
    assert_eq!(
        cycle_allocations, 0,
        "steady-state acquire/release cycles allocate nothing"
    );
}

#[test]
fn pre_bound_reader_executes_without_the_publication_mechanism() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let publication = PublishedViews::new(view0);

    let view = publication.acquire();
    // The mechanism can be dropped entirely: the pre-bound reader executes
    // through held handles and never needs the publication slot again
    // (structural evidence that the quantum path carries no lock).
    drop(publication);
    view.execute_quantum(3)
        .expect("execution dereferences only already-held handles");
}

#[test]
fn publication_does_not_wait_for_readers() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    let publication = Arc::new(PublishedViews::new(view0));

    // A reader acquires and executes concurrently with publication; the
    // honest publish returns without waiting for anything.
    let reader_publication = publication.clone();
    let reader_handle = std::thread::spawn(move || {
        let view = reader_publication.acquire();
        let identity = view.identity();
        view.execute_quantum(2)
            .expect("the reader executes a whole view");
        drop(view);
        identity
    });

    publication.publish(view1);
    let identity = reader_handle.join().expect("the reader thread joins");
    assert!(
        identity == 0 || identity == 1,
        "the reader observed a whole view, never a mix: {identity}"
    );
}

#[test]
fn publisher_waiting_for_reader_blocks_reader_acquisition() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    let publication = Arc::new(PublisherWaitsForReader::new(view0));

    let (lock_held_tx, lock_held_rx) = mpsc::channel();
    let (reader_exit_tx, reader_exit_rx) = mpsc::channel();
    let (will_acquire_tx, will_acquire_rx) = mpsc::channel();
    let (acquired_tx, acquired_rx) = mpsc::channel();

    let publisher_publication = publication.clone();
    let publisher_handle = std::thread::spawn(move || {
        publisher_publication.publish_blocking(view1, lock_held_tx, reader_exit_rx);
    });
    // Deterministic: wait until the publisher holds the shared slot lock.
    lock_held_rx.recv().expect("the publisher holds the lock");

    let reader_publication = publication.clone();
    let reader_handle = std::thread::Builder::new()
        .name("rt-reader".to_string())
        .spawn(move || {
            will_acquire_tx
                .send(())
                .expect("the reader is about to acquire");
            let view = reader_publication.acquire();
            acquired_tx
                .send(view.identity())
                .expect("the reader acquired");
        })
        .expect("the reader thread spawns");

    // The reader is now waiting on the lock the publisher holds while it
    // waits for the reader: the control-side wait blocks the realtime side.
    will_acquire_rx.recv().expect("the reader signaled");
    assert!(
        acquired_rx.try_recv().is_err(),
        "the realtime reader must be blocked while the publisher waits with the lock held"
    );

    // Resolve the wait (test-side); only then can the reader acquire.
    reader_exit_tx.send(()).expect("the wait resolves");
    publisher_handle.join().expect("the publisher thread joins");
    let identity = acquired_rx.recv().expect("the reader eventually acquires");
    reader_handle.join().expect("the reader thread joins");
    assert_eq!(
        identity, 1,
        "after the wait resolves the reader acquires the new view"
    );
}

// ---------------------------------------------------------------------------
// Publication atomicity and release ordering
// ---------------------------------------------------------------------------

#[test]
fn failed_publication_leaves_the_current_view_untouched() {
    let log = new_event_log();
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    );
    let publication = PublishedViews::new(view0);

    // A next view that fails validation (no dereferenceable surface) must
    // not disturb the published view.
    let invalid = Arc::new(RealtimeView::new(5, 1, FLOW_PARTICIPANTS.to_vec(), vec![]));
    assert!(publication.publish_checked(invalid).is_err());
    assert_eq!(
        publication.current_identity(),
        0,
        "the current view survives a failed publication"
    );
    let acquired = publication.acquire();
    assert_eq!(acquired.identity(), 0);
    acquired
        .execute_quantum(1)
        .expect("the old view still executes");

    // A valid publication afterwards succeeds.
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        tracked_resources(&[12, 13], &log),
        log.clone(),
    );
    assert!(publication.publish_checked(view1).is_ok());
    assert_eq!(publication.current_identity(), 1);
}

#[test]
fn resources_are_destroyed_only_after_all_referencing_views_are_released() {
    let log = new_event_log();
    let a = TrackedResource::new(10, log.clone());
    let b = TrackedResource::new(11, log.clone());
    let c = TrackedResource::new(12, log.clone());
    let a_weak = Arc::downgrade(&a);
    let b_weak = Arc::downgrade(&b);
    let c_weak = Arc::downgrade(&c);
    let view0 = make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        vec![a.clone(), b.clone()],
        log.clone(),
    );
    let view1 = make_view(
        1,
        1,
        FLOW_PARTICIPANTS,
        vec![a.clone(), c.clone()],
        log.clone(),
    );
    // A later publication retires view 1 so it can be certified and
    // physically released; the successor does not reference A.
    let view2 = make_view(
        2,
        2,
        FLOW_PARTICIPANTS,
        tracked_resources(&[20, 21], &log),
        log.clone(),
    );
    drop(a);
    drop(b);
    drop(c);
    let publication = PublishedViews::new(view0);

    let old_reader = Reader::new(publication.acquire());
    publication.publish(view1);
    publication.publish(view2);
    drop(old_reader);

    assert!(publication.certify_reclaimable(0));
    assert!(publication.release_reclaimable(0));
    // B was only in view 0 and is gone; A is still referenced by view 1.
    assert!(a_weak.upgrade().is_some() && b_weak.upgrade().is_none() && c_weak.upgrade().is_some());

    assert!(publication.certify_reclaimable(1));
    assert!(publication.release_reclaimable(1));
    // A is destroyed only after the last referencing view is released.
    assert!(a_weak.upgrade().is_none() && c_weak.upgrade().is_none());

    // Ordering: destruction happens after every dereference, and the
    // per-view resource dies with the release of the last referencing view.
    let events = log.lock().expect("observer log lock");
    let positions: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, event)| matches!(event, ResourceEvent::Destroyed { .. }))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        positions.len(),
        3,
        "A, B and C are each destroyed exactly once"
    );
    let dereferences_after_destroy: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(index, event)| {
            matches!(event, ResourceEvent::Dereferenced { .. })
                && positions.iter().any(|position| *position < *index)
        })
        .map(|(index, _)| index)
        .collect();
    assert!(
        dereferences_after_destroy.is_empty(),
        "no resource is dereferenced after it was destroyed: {dereferences_after_destroy:?}"
    );
}

// ---------------------------------------------------------------------------
// Concurrent stress smoke: real interleavings, whole views only
// ---------------------------------------------------------------------------

#[test]
fn concurrent_publication_and_readers_observe_only_whole_views() {
    let log = new_event_log();
    let publication = Arc::new(PublishedViews::new(make_view(
        0,
        0,
        FLOW_PARTICIPANTS,
        tracked_resources(&[10, 11], &log),
        log.clone(),
    )));

    let publisher_publication = publication.clone();
    let publisher = std::thread::Builder::new()
        .name("publisher".to_string())
        .spawn(move || {
            for generation in 1..=4u64 {
                let view = make_view(
                    generation,
                    generation,
                    FLOW_PARTICIPANTS,
                    tracked_resources(&[100 + generation, 110 + generation], &log),
                    log.clone(),
                );
                publisher_publication.publish(view);
            }
        })
        .expect("the publisher thread spawns");

    let mut readers = Vec::new();
    for reader_index in 0..4u64 {
        let reader_publication = publication.clone();
        readers.push(
            std::thread::Builder::new()
                .name(format!("rt-reader-{reader_index}"))
                .spawn(move || {
                    for _ in 0..500 {
                        let view = reader_publication.acquire();
                        // P1 under real interleavings: every acquisition is a
                        // whole view — identity and resource membership always
                        // belong to the same publication.
                        let expected: Vec<u64> = match view.identity() {
                            0 => vec![10, 11],
                            generation if (1..=4).contains(&generation) => {
                                vec![100 + generation, 110 + generation]
                            }
                            other => panic!("unexpected view identity: {other}"),
                        };
                        assert_eq!(
                            view.resource_ids(),
                            expected,
                            "the reader observes a whole view, never a mix"
                        );
                        // P3 under real interleavings: no released resource is
                        // ever dereferenced (resources are never released
                        // while a reader holds the view).
                        view.execute_quantum(1)
                            .expect("no released resource is ever dereferenced");
                        drop(view);
                    }
                })
                .expect("the reader thread spawns"),
        );
    }

    publisher.join().expect("the publisher thread joins");
    for reader in readers {
        reader.join().expect("the reader thread joins");
    }

    // Nothing was released in this scenario, so nothing was destroyed while
    // readers were active.
    assert_eq!(publication.current_identity(), 4);
}
