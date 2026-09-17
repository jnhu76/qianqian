//! RenderGate protocol tests (F3, ADR-PBK-002 D14.7): the
//! session-owned pause gate's park/release discipline at the ports seam.
//! These pin the mechanism contract every render provider inherits:
//! park in bounded slices, acknowledge engagement / tail quiescence /
//! disengagement as evidence, never abort the caller's leg.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use qianqian_audio_api::ports::{GateEvent, RenderGate};

/// A bounded poll so timing assertions fail with a diagnosis, not a
/// hang.
fn wait_until(limit: Duration, mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = std::time::Instant::now() + limit;
    loop {
        if predicate() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[derive(Clone, Default)]
struct Events {
    seen: Arc<Mutex<Vec<GateEvent>>>,
}

impl Events {
    fn record(&self, event: GateEvent) {
        self.seen.lock().expect("events lock").push(event);
    }

    fn snapshot(&self) -> Vec<GateEvent> {
        self.seen.lock().expect("events lock").clone()
    }
}

/// No intent routed: the gate must return immediately, call the tail
/// observation never, and publish no evidence.
#[test]
fn an_unengaged_gate_parks_nothing_and_publishes_nothing() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));
    let mut tail_calls = 0usize;
    gate.park_while_paused(|| {
        tail_calls += 1;
        true
    });
    assert_eq!(tail_calls, 0, "the tail observation runs only while parked");
    assert!(events.snapshot().is_empty(), "{:?}", events.snapshot());
}

/// Release before the leg reaches the gate: nothing engages, nothing is
/// published.
#[test]
fn a_release_that_lands_before_the_park_publishes_nothing() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));
    gate.set_paused(true);
    gate.set_paused(false);
    let mut tail_calls = 0usize;
    gate.park_while_paused(|| {
        tail_calls += 1;
        true
    });
    assert_eq!(tail_calls, 0);
    assert!(events.snapshot().is_empty(), "{:?}", events.snapshot());
}

/// The full park cycle: engagement evidence on entry, the tail
/// observation between bounded slices, TailQuiesced at most once per
/// engagement, disengagement on release, and the caller returns to its
/// loop (the gate never aborts the leg).
#[test]
fn a_park_publishes_engagement_quiescence_and_disengagement_then_returns() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));

    gate.set_paused(true);
    let tail_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let leg = {
        let gate = gate.clone();
        let tail_calls = tail_calls.clone();
        std::thread::spawn(move || {
            // Quiescent only from the third observation on: the first
            // slices must not publish quiescence.
            gate.park_while_paused(|| tail_calls.fetch_add(1, Ordering::SeqCst) >= 2);
            // The leg is back at its loop top; the gate must not park it
            // again on the same routed intent (it was released).
            gate.park_while_paused(|| false);
        })
    };

    assert!(
        wait_until(Duration::from_secs(5), || events
            .snapshot()
            .contains(&GateEvent::Engaged)),
        "no engagement evidence: {:?}",
        events.snapshot()
    );
    assert!(
        wait_until(Duration::from_secs(5), || events
            .snapshot()
            .contains(&GateEvent::TailQuiesced)),
        "quiescence never published: {:?}",
        events.snapshot()
    );
    gate.set_paused(false);
    leg.join().expect("the parked leg must return");

    let seen = events.snapshot();
    let engaged = seen.iter().filter(|e| **e == GateEvent::Engaged).count();
    let quiesced = seen
        .iter()
        .filter(|e| **e == GateEvent::TailQuiesced)
        .count();
    let disengaged = seen.iter().filter(|e| **e == GateEvent::Disengaged).count();
    assert_eq!(engaged, 1, "exactly one engagement: {seen:?}");
    assert_eq!(
        quiesced, 1,
        "quiescence publishes once per engagement: {seen:?}"
    );
    assert_eq!(disengaged, 1, "exactly one disengagement: {seen:?}");
    assert!(
        tail_calls.load(Ordering::SeqCst) >= 2,
        "the tail observation runs between bounded slices"
    );
}

/// Each NEW engagement must re-observe quiescence: a previous cycle's
/// TailQuiesced never satisfies a later pause (the D14.7 corrective
/// invariant, at the mechanism level).
#[test]
fn tail_quiescence_belongs_to_each_engagement_separately() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));

    // Engagement 1: immediately quiescent.
    gate.set_paused(true);
    let leg = {
        let gate = gate.clone();
        std::thread::spawn(move || gate.park_while_paused(|| true))
    };
    assert!(
        wait_until(Duration::from_secs(5), || events
            .snapshot()
            .contains(&GateEvent::TailQuiesced)),
        "engagement 1 never quiesced: {:?}",
        events.snapshot()
    );
    gate.set_paused(false);
    leg.join().expect("leg 1 joins");

    // Engagement 2: the tail becomes quiescent again in this cycle. The
    // gate must RE-OBSERVE it (per-engagement reset, not a stuck latch):
    // a second TailQuiesced proves the fresh cycle published its own
    // evidence, and the phase before it proves the first cycle's
    // quiescence never leaked in.
    gate.set_paused(true);
    let leg = {
        let gate = gate.clone();
        std::thread::spawn(move || gate.park_while_paused(|| true))
    };
    assert!(
        wait_until(Duration::from_secs(5), || {
            let seen = events.snapshot();
            seen.iter().filter(|e| **e == GateEvent::Engaged).count() == 2
        }),
        "engagement 2 never latched: {:?}",
        events.snapshot()
    );
    // Between the two engagements no quiescence may exist, and the new
    // engagement must earn its own evidence through a fresh observation.
    assert!(
        wait_until(Duration::from_secs(5), || events
            .snapshot()
            .iter()
            .filter(|e| **e == GateEvent::TailQuiesced)
            .count()
            == 2),
        "engagement 2 never published its own quiescence: {:?}",
        events.snapshot()
    );
    let seen_final = events.snapshot();
    assert_eq!(
        seen_final
            .iter()
            .filter(|e| **e == GateEvent::Engaged)
            .count(),
        2,
        "exactly two engagements: {seen_final:?}"
    );
    gate.set_paused(false);
    leg.join().expect("leg 2 joins");

    // A final engagement whose tail NEVER goes quiescent must stay
    // engaged without a third TailQuiesced — the stale-latch direction
    // of the same invariant. The engagement is witnessed latched first,
    // so the negative assertion cannot pass vacuously on a leg that
    // never parked.
    gate.set_paused(true);
    let leg = {
        let gate = gate.clone();
        std::thread::spawn(move || gate.park_while_paused(|| false))
    };
    assert!(
        wait_until(Duration::from_secs(5), || {
            let seen = events.snapshot();
            seen.iter().filter(|e| **e == GateEvent::Engaged).count() == 3
        }),
        "engagement 3 never latched: {:?}",
        events.snapshot()
    );
    // The tail observation runs every bounded slice (~10ms); a stale
    // latch would have published a third TailQuiesced within this
    // window.
    std::thread::sleep(Duration::from_millis(50));
    let seen_after = events.snapshot();
    assert_eq!(
        seen_after
            .iter()
            .filter(|e| **e == GateEvent::TailQuiesced)
            .count(),
        2,
        "an engaged-but-not-quiescent cycle must not inherit prior \
         quiescence: {seen_after:?}"
    );
    gate.set_paused(false);
    leg.join().expect("leg 3 joins");
}

/// The open-abort lifetime (D14.7 corrective-2): once a gate is closed,
/// no later pause intent — routed or hostile — can ever park a leg
/// again. A leg that arrives after the close finds the gate shut:
/// immediate return, no engagement, no events. There is no un-close.
#[test]
fn a_closed_gate_never_parks_or_engages_again() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));

    gate.set_paused(true);
    gate.close_and_release();
    // Hostile later intent, routed after the close: inert by design.
    gate.set_paused(true);

    let mut tail_calls = 0usize;
    gate.park_while_paused(|| {
        tail_calls += 1;
        false
    });

    assert_eq!(tail_calls, 0, "the closed gate must not park the leg");
    assert!(
        events.snapshot().is_empty(),
        "the closed gate must publish no engagement evidence: {:?}",
        events.snapshot()
    );
}
