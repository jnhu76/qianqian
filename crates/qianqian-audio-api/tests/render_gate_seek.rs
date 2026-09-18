//! RenderGate seek-park protocol tests (F5, ADR-PBK-002 D14.5): the
//! cut-attributed park at the ports seam, driven through the ONE unified
//! loop-top operation (`park_loop_top`) every render provider inherits.
//! These pin the mechanism contract: park only under a routed seek hold,
//! publish only `Seek*` events (never pause evidence), consume the
//! release payload exactly once ON THE LEG'S PATH — at the park's exit,
//! on arrival with no park, or MID-PARK while the leg is held by pause —
//! and never abort the caller's leg.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use qianqian_audio_api::ports::{GateEvent, GateSlice, RenderGate, SeekParkRelease};

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

/// Run the unified loop-top gate: `tail` as the tail probe, every routed
/// release payload captured into the returned slot (last wins; the
/// session routes at most one per seek).
fn gate_probe(
    gate: &RenderGate,
    mut tail: impl FnMut() -> bool,
) -> Arc<Mutex<Option<SeekParkRelease>>> {
    let captured: Arc<Mutex<Option<SeekParkRelease>>> = Arc::new(Mutex::new(None));
    let sink = captured.clone();
    gate.park_loop_top(|slice| match slice {
        GateSlice::TailProbe => tail(),
        GateSlice::SeekRelease(release) => {
            *sink.lock().expect("release lock") = Some(release);
            false
        }
    });
    captured
}

/// No hold routed (and no payload awaiting): the gate must return
/// immediately, call the tail observation never, publish no events, and
/// consume no release payload.
#[test]
fn an_unheld_seek_gate_parks_nothing_and_publishes_nothing() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));
    let mut tail_calls = 0usize;
    let captured = gate_probe(&gate, || {
        tail_calls += 1;
        true
    });
    assert!(
        captured.lock().expect("release lock").is_none(),
        "nothing routed, nothing consumed"
    );
    assert_eq!(tail_calls, 0, "the tail observation runs only while parked");
    assert!(events.snapshot().is_empty(), "{:?}", events.snapshot());
    // A second check finds the same nothing: no hold routed means no
    // payload can exist to consume.
    let again = gate_probe(&gate, || true);
    assert!(again.lock().expect("release lock").is_none());
}

/// A release that lands before the leg reaches the gate: the hold is
/// already cleared, so the leg never parks and publishes no events —
/// but the payload is still CONSUMED at the gate check (the
/// payload-awaits-consumption shape: a cutover committed while the leg
/// was running, or its hold released before its arrival, must still
/// rebase at this gate before any further submission), exactly once.
#[test]
fn a_seek_release_landing_before_the_park_is_consumed_at_the_gate() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));
    gate.set_seek_hold(true);
    gate.release_seek_hold(SeekParkRelease::Aborted);
    let captured = gate_probe(&gate, || true);
    assert_eq!(
        *captured.lock().expect("release lock"),
        Some(SeekParkRelease::Aborted),
        "an aborted release routed pre-arrival is consumed without a park"
    );
    assert!(
        events.snapshot().is_empty(),
        "no park happened, no park evidence: {:?}",
        events.snapshot()
    );

    // The committed shape, and the take-once rule: the second call
    // finds no hold and no payload.
    let events2 = Events::default();
    let gate2 = RenderGate::with_observer({
        let events2 = events2.clone();
        move |event| events2.record(event)
    });
    gate2.set_seek_hold(true);
    gate2.release_seek_hold(SeekParkRelease::Committed { landing: Some(7) });
    let captured2 = gate_probe(&gate2, || true);
    assert_eq!(
        *captured2.lock().expect("release lock"),
        Some(SeekParkRelease::Committed { landing: Some(7) }),
        "the committed payload reaches the leg even though it never parked"
    );
    let again = gate_probe(&gate2, || true);
    assert!(
        again.lock().expect("release lock").is_none(),
        "the payload is consumed exactly once"
    );
    assert!(events2.snapshot().is_empty());
}

/// A full seek park: engagement, exactly-one quiescence, disengagement,
/// in order, with ONLY Seek* events — a cut-attributed park can never
/// fabricate pause evidence (D14.5 attribution separation).
#[test]
fn a_seek_park_publishes_only_seek_events_in_order() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));
    gate.set_seek_hold(true);
    let parked = Arc::new(AtomicBool::new(false));
    let parked_clone = parked.clone();
    let leg_gate = gate.clone();
    let leg = std::thread::spawn(move || {
        let mut observations = 0usize;
        let captured = gate_probe(&leg_gate, || {
            parked_clone.store(true, Ordering::SeqCst);
            observations += 1;
            observations >= 2 // quiesce on the second slice
        });
        (captured, observations)
    });
    assert!(
        wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst)),
        "the leg must park under the routed hold"
    );
    // Give the park slices time to run twice (quiescence on #2), then
    // more slices that must NOT re-publish quiescence.
    std::thread::sleep(Duration::from_millis(60));
    gate.release_seek_hold(SeekParkRelease::Aborted);
    let (captured, observations) = leg.join().expect("leg exits");
    assert_eq!(
        *captured.lock().expect("release lock"),
        Some(SeekParkRelease::Aborted)
    );
    assert!(
        (2..=4).contains(&observations),
        "bounded-slice parking: {observations} observations"
    );
    assert_eq!(
        events.snapshot(),
        vec![
            GateEvent::SeekEngaged,
            GateEvent::SeekTailQuiesced,
            GateEvent::SeekDisengaged,
        ],
        "exactly one quiescence per seek park, no pause events: {:?}",
        events.snapshot()
    );
}

/// The committed release carries its landing payload to the seek park's
/// exit, and the payload is consumed exactly once.
#[test]
fn a_committed_release_reaches_the_parked_leg_exactly_once() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));
    gate.set_seek_hold(true);
    let parked = Arc::new(AtomicBool::new(false));
    let parked_clone = parked.clone();
    let leg_gate = gate.clone();
    let leg = std::thread::spawn(move || {
        gate_probe(&leg_gate, || {
            parked_clone.store(true, Ordering::SeqCst);
            true
        })
    });
    assert!(wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst)));
    gate.release_seek_hold(SeekParkRelease::Committed {
        landing: Some(44_100),
    });
    let captured = leg.join().expect("leg exits");
    assert_eq!(
        *captured.lock().expect("release lock"),
        Some(SeekParkRelease::Committed {
            landing: Some(44_100)
        })
    );
    // Consumed by the park's exit: the next gate check finds no hold
    // and no payload — one payload, one consumer.
    let again = gate_probe(&gate, || true);
    assert!(again.lock().expect("release lock").is_none());
}

/// Implementation corrective-1 (C1): a committed cut cut MUST rebase a
/// PAUSE-PARKED leg while it STAYS PARKED — not at its next gate check
/// after the resume. Pause intent survives the seek; the rebase is
/// bookkeeping on the leg's own path (it submits nothing while parked);
/// and the whole exchange publishes pause events only, no seek park
/// events (there was no seek park).
#[test]
fn a_committed_release_rebases_a_pause_parked_leg_while_still_parked() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));
    // Pause intent first, then the seek hold: the episode is paused and
    // seekable. The leg parks at the gate under the PAUSE attribution
    // and stays there through the whole exchange.
    gate.set_paused(true);
    gate.set_seek_hold(true);

    let parked = Arc::new(AtomicBool::new(false));
    let parked_clone = parked.clone();
    let rebased_while_parked = Arc::new(AtomicBool::new(false));
    let rebased_clone = rebased_while_parked.clone();
    let leg_gate = gate.clone();
    let leg = std::thread::spawn(move || {
        let captured: Arc<Mutex<Option<SeekParkRelease>>> = Arc::new(Mutex::new(None));
        leg_gate.park_loop_top(|slice| match slice {
            GateSlice::TailProbe => {
                parked_clone.store(true, Ordering::SeqCst);
                true // quiesce on the first slice: the commit precondition
            }
            GateSlice::SeekRelease(release) => {
                // Consumed MID-PARK, on the leg's own path, while the
                // pause intent keeps it parked.
                *captured.lock().expect("release lock") = Some(release);
                rebased_clone.store(true, Ordering::SeqCst);
                false
            }
        });
        captured
    });
    assert!(wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst)));
    assert!(
        wait_until(Duration::from_secs(5), || events
            .snapshot()
            .contains(&GateEvent::TailQuiesced)),
        "precondition: the pause engagement published its quiescence"
    );

    // The cutover commits while the leg is pause-parked. The payload
    // must reach the leg MID-PARK: the leg consumes it and KEEPS
    // PARKING.
    gate.release_seek_hold(SeekParkRelease::Committed { landing: Some(5) });
    assert!(
        wait_until(Duration::from_secs(5), || rebased_while_parked
            .load(Ordering::SeqCst)),
        "the committed rebase never reached the parked leg"
    );
    // Still parked at that instant: no Disengaged may exist yet, and no
    // second engagement either — the leg never left the pause park.
    let mid_park = events.snapshot();
    assert!(
        !mid_park.contains(&GateEvent::Disengaged),
        "the rebase must happen while the leg is still parked: {mid_park:?}"
    );
    assert!(
        !mid_park
            .iter()
            .any(|event| matches!(event, GateEvent::SeekEngaged)),
        "no seek park happened: {mid_park:?}"
    );

    // ...and only then the pause itself resumes; the leg exits with
    // nothing further to consume.
    gate.set_paused(false);
    let captured = leg.join().expect("leg exits");
    assert_eq!(
        *captured.lock().expect("release lock"),
        Some(SeekParkRelease::Committed { landing: Some(5) }),
        "the parked leg received exactly the commit's rebase instruction"
    );
    let snapshot = events.snapshot();
    assert_eq!(
        snapshot
            .iter()
            .filter(|event| **event == GateEvent::Engaged)
            .count(),
        1,
        "one continuous pause engagement across the whole cut: {snapshot:?}"
    );
    assert!(
        snapshot.iter().all(|event| !matches!(
            event,
            GateEvent::SeekEngaged | GateEvent::SeekTailQuiesced | GateEvent::SeekDisengaged
        )),
        "no seek-park evidence without a seek park: {snapshot:?}"
    );
}

/// Routing a NEW hold resets any release payload a previous cycle left
/// unconsumed. This is a DEBRIS GUARD, not the mechanism that keeps
/// committed rebases safe: a routed `Committed` payload is part of its
/// cut until the leg consumes it, so the SESSION must never route a new
/// hold while one awaits — the one-seek slot stays occupied through
/// consumption (pinned by the completion's white-box tests and the
/// seek matrices' never-wiped-payload matrix). This gate-level wipe
/// only bounds the damage if that session-side duty were ever skipped.
#[test]
fn a_new_hold_drops_a_stale_unconsumed_release() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));
    // Cycle 1: hold + committed release, but the leg never parks and
    // nobody takes the payload.
    gate.set_seek_hold(true);
    gate.release_seek_hold(SeekParkRelease::Committed { landing: Some(9) });
    // Cycle 2: a new hold must not inherit cycle 1's payload.
    gate.set_seek_hold(true);
    let parked = Arc::new(AtomicBool::new(false));
    let parked_clone = parked.clone();
    let leg_gate = gate.clone();
    let leg = std::thread::spawn(move || {
        gate_probe(&leg_gate, || {
            parked_clone.store(true, Ordering::SeqCst);
            true
        })
    });
    assert!(wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst)));
    gate.release_seek_hold(SeekParkRelease::Aborted);
    let captured = leg.join().expect("leg exits");
    assert_eq!(
        *captured.lock().expect("release lock"),
        Some(SeekParkRelease::Aborted),
        "the stale Committed payload from cycle 1 must not answer cycle 2's park"
    );
}

/// A closed gate never parks a seek-held leg: the open-abort lifetime
/// makes every later hold inert, and a parked leg is woken by the close
/// with an Aborted payload (no payload was routed).
#[test]
fn a_closed_gate_never_seek_parks_again() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));
    // Close first: a hold routed afterwards routes nothing.
    gate.close_and_release();
    gate.set_seek_hold(true);
    let captured = gate_probe(&gate, || true);
    assert!(captured.lock().expect("release lock").is_none());
    // A leg already parked when the close lands is woken with bounded
    // latency and leaves with the abort shape.
    let events2 = Events::default();
    let gate2 = RenderGate::with_observer({
        let events2 = events2.clone();
        move |event| events2.record(event)
    });
    gate2.set_seek_hold(true);
    let parked = Arc::new(AtomicBool::new(false));
    let parked_clone = parked.clone();
    let leg_gate = gate2.clone();
    let leg = std::thread::spawn(move || {
        gate_probe(&leg_gate, || {
            parked_clone.store(true, Ordering::SeqCst);
            false // never quiesce: the close, not quiescence, must end the park
        })
    });
    assert!(wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst)));
    gate2.close_and_release();
    let captured = leg.join().expect("leg exits");
    assert_eq!(
        *captured.lock().expect("release lock"),
        Some(SeekParkRelease::Aborted)
    );
    assert!(events.snapshot().is_empty());
    assert_eq!(
        events2.snapshot(),
        vec![GateEvent::SeekEngaged, GateEvent::SeekDisengaged],
        "a close-woken park publishes engagement and disengagement only"
    );
}

/// The tail observation is called only between park slices while the
/// hold is routed — never before the park, never after the release.
#[test]
fn the_seek_park_calls_the_tail_observation_only_while_parked() {
    let gate = RenderGate::new();
    gate.set_seek_hold(true);
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_writer = calls.clone();
    let parked = Arc::new(AtomicBool::new(false));
    let parked_clone = parked.clone();
    let leg_gate = gate.clone();
    let leg = std::thread::spawn(move || {
        gate_probe(&leg_gate, || {
            calls_writer.fetch_add(1, Ordering::SeqCst);
            parked_clone.store(true, Ordering::SeqCst);
            true
        })
    });
    assert!(wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst)));
    let before = calls.load(Ordering::SeqCst);
    assert!(before >= 1, "at least the first slice's observation ran");
    gate.release_seek_hold(SeekParkRelease::Aborted);
    let _captured = leg.join().expect("leg exits");
    let after = calls.load(Ordering::SeqCst);
    assert!(
        after <= before + 1,
        "no observation runs after the release (before={before}, after={after})"
    );
}
