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

use qianqian_audio_api::ports::{
    GateEvent, GateSlice, ParkOutcome, RenderGate, SeekParkRelease, TailProbeOutcome,
};

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
/// release appended to the returned delivery sequence. Checking the whole
/// sequence distinguishes one delivery from duplicate callbacks of one payload.
fn gate_probe(
    gate: &RenderGate,
    mut tail: impl FnMut() -> bool,
) -> Arc<Mutex<Vec<SeekParkRelease>>> {
    let captured: Arc<Mutex<Vec<SeekParkRelease>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = captured.clone();
    gate.park_loop_top(|slice| match slice {
        GateSlice::TailProbe => {
            if tail() {
                TailProbeOutcome::Quiesced
            } else {
                TailProbeOutcome::Pending
            }
        }
        GateSlice::SeekRelease(release) => {
            sink.lock().expect("release lock").push(release);
            TailProbeOutcome::Pending
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
        captured.lock().expect("release lock").is_empty(),
        "nothing routed, nothing consumed"
    );
    assert_eq!(tail_calls, 0, "the tail observation runs only while parked");
    assert!(events.snapshot().is_empty(), "{:?}", events.snapshot());
    // A second check finds the same nothing: no hold routed means no
    // payload can exist to consume.
    let again = gate_probe(&gate, || true);
    assert!(again.lock().expect("release lock").is_empty());
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
        vec![SeekParkRelease::Aborted],
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
        vec![SeekParkRelease::Committed { landing: Some(7) }],
        "the committed payload reaches the leg even though it never parked"
    );
    let again = gate_probe(&gate2, || true);
    assert!(
        again.lock().expect("release lock").is_empty(),
        "the payload is consumed exactly once"
    );
    assert!(events2.snapshot().is_empty());
}

/// Own the gate and its leg BEFORE any fallible wait/assert. Drop releases
/// both park conditions, then observes exit and actually joins. Normal join
/// does not release the gate: the test must drive its intended exit first.
/// The deadline is a failure bound, never an ordering premise; a genuinely
/// stuck thread cannot be force-terminated by this owner.
struct GateLeg<T> {
    gate: RenderGate,
    leg: Option<std::thread::JoinHandle<T>>,
    joined: Arc<AtomicBool>,
}

impl<T> GateLeg<T> {
    fn new(gate: &RenderGate, leg: std::thread::JoinHandle<T>) -> Self {
        Self {
            gate: gate.clone(),
            leg: Some(leg),
            joined: Arc::new(AtomicBool::new(false)),
        }
    }

    fn join(mut self, what: &str) -> T {
        assert!(
            wait_until(Duration::from_secs(5), || self
                .leg
                .as_ref()
                .unwrap()
                .is_finished()),
            "leg never exited within 5s: {what}"
        );
        let result = self.leg.take().unwrap().join();
        self.joined.store(true, Ordering::SeqCst);
        match result {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
}

impl<T> Drop for GateLeg<T> {
    fn drop(&mut self) {
        if let Some(leg) = self.leg.take() {
            self.gate.close_and_release();
            let exited = wait_until(Duration::from_secs(5), || leg.is_finished());
            let joined = exited && leg.join().is_ok();
            self.joined.store(joined, Ordering::SeqCst);
            eprintln!("gate leg cleanup: exited_within_5s={exited}, joined={joined}");
            if !joined && !std::thread::panicking() {
                panic!("gate leg cleanup did not join cleanly");
            }
        }
    }
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
    let leg = GateLeg::new(&gate, leg);
    let parked_ok = wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst));
    if !parked_ok {
        // Release AND collect before failing the premise — no silently
        // detached leg on the error paths the negative controls inject
        // The owner also covers every later semantic assertion.
        panic!("the leg never parked under the routed hold within 5s");
    }
    // Witness the quiescence publication, then release: the mutex-published
    // append-only event log cannot be missed by polling, only observed
    // late, so seeing `SeekTailQuiesced` here establishes the real causal
    // edge (probe #2 returned Quiesced → publish → observed → release) a
    // fixed sleep cannot.
    let quiesced = wait_until(Duration::from_secs(5), || {
        events.snapshot().contains(&GateEvent::SeekTailQuiesced)
    });
    if !quiesced {
        // Same release-and-collect discipline for the publication premise.
        panic!("the seek park never published its tail quiescence within 5s");
    }
    // Diagnostic detection window for no-reprobe-after-quiescence — NOT a
    // correctness premise. Once quiescence is published, the guard stops
    // the probes, so a correct implementation's observation count is
    // already frozen here; this window only widens the exposure of an
    // implementation that wrongly keeps probing. It cannot make a correct
    // implementation red.
    std::thread::sleep(Duration::from_millis(25));
    gate.release_seek_hold(SeekParkRelease::Aborted);
    let (captured, observations) = leg.join("the seek-park leg after its abort release");
    assert_eq!(
        *captured.lock().expect("release lock"),
        vec![SeekParkRelease::Aborted]
    );
    assert_eq!(
        observations, 2,
        "quiescence on slice #2 stops the probes: {observations}"
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
    let leg = GateLeg::new(&gate, leg);
    if !wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst)) {
        panic!("the leg never parked under the routed hold");
    }
    gate.release_seek_hold(SeekParkRelease::Committed {
        landing: Some(44_100),
    });
    let captured = leg.join("the parked leg after its committed release");
    assert_eq!(
        *captured.lock().expect("release lock"),
        vec![SeekParkRelease::Committed {
            landing: Some(44_100)
        }]
    );
    // Consumed by the park's exit: the next gate check finds no hold
    // and no payload — one payload, one consumer.
    let again = gate_probe(&gate, || true);
    assert!(again.lock().expect("release lock").is_empty());
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
        let captured: Arc<Mutex<Vec<SeekParkRelease>>> = Arc::new(Mutex::new(Vec::new()));
        leg_gate.park_loop_top(|slice| match slice {
            GateSlice::TailProbe => {
                parked_clone.store(true, Ordering::SeqCst);
                TailProbeOutcome::Quiesced // on the first slice: the commit precondition
            }
            GateSlice::SeekRelease(release) => {
                // Consumed MID-PARK, on the leg's own path, while the
                // pause intent keeps it parked.
                captured.lock().expect("release lock").push(release);
                rebased_clone.store(true, Ordering::SeqCst);
                TailProbeOutcome::Pending
            }
        });
        captured
    });
    let leg = GateLeg::new(&gate, leg);
    if !wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst)) {
        panic!("the leg never parked under the pause");
    }
    if !wait_until(Duration::from_secs(5), || {
        events.snapshot().contains(&GateEvent::TailQuiesced)
    }) {
        panic!("the pause engagement never published its quiescence");
    }

    // The cutover commits while the leg is pause-parked. The payload
    // must reach the leg MID-PARK: the leg consumes it and KEEPS
    // PARKING.
    gate.release_seek_hold(SeekParkRelease::Committed { landing: Some(5) });
    if !wait_until(Duration::from_secs(5), || {
        rebased_while_parked.load(Ordering::SeqCst)
    }) {
        panic!("the committed rebase never reached the parked leg");
    }
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
    let captured = leg.join("the pause-parked leg after its resume");
    assert_eq!(
        *captured.lock().expect("release lock"),
        vec![SeekParkRelease::Committed { landing: Some(5) }],
        "the parked leg received exactly the commit's rebase instruction"
    );
    assert_eq!(
        events.snapshot(),
        vec![
            GateEvent::Engaged,
            GateEvent::TailQuiesced,
            GateEvent::Disengaged,
        ],
        "one continuous ordered pause cycle, no seek evidence"
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
    let leg = GateLeg::new(&gate, leg);
    if !wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst)) {
        panic!("the leg never parked under cycle 2's hold");
    }
    gate.release_seek_hold(SeekParkRelease::Aborted);
    let captured = leg.join("the cycle-2 leg after its abort release");
    assert_eq!(
        *captured.lock().expect("release lock"),
        vec![SeekParkRelease::Aborted],
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
    assert!(captured.lock().expect("release lock").is_empty());
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
    let leg = GateLeg::new(&gate2, leg);
    if !wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst)) {
        panic!("the leg never parked under the routed hold");
    }
    gate2.close_and_release();
    let captured = leg.join("the parked leg after the close");
    assert_eq!(
        *captured.lock().expect("release lock"),
        vec![SeekParkRelease::Aborted]
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
    let events = Events::default();
    let writer = events.clone();
    let gate = RenderGate::with_observer(move |event| writer.record(event));
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
    let leg = GateLeg::new(&gate, leg);
    if !wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst)) {
        panic!("the leg never parked under the routed hold");
    }
    assert!(
        wait_until(Duration::from_secs(5), || events
            .snapshot()
            .contains(&GateEvent::SeekTailQuiesced)),
        "first probe must publish quiescence"
    );
    gate.release_seek_hold(SeekParkRelease::Aborted);
    let captured = leg.join("the parked leg after its abort release");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "no probe after the quiesced first slice or release"
    );
    assert_eq!(*captured.lock().unwrap(), vec![SeekParkRelease::Aborted]);
    let again = gate_probe(&gate, || panic!("unheld gate must not probe"));
    assert!(again.lock().unwrap().is_empty());
}

/// F5 implementation corrective-4 (C9): inside the cut-attributed park,
/// a tail observation that itself FAILS must end the park bounded — it
/// is neither `SeekTailQuiesced` (a failed observation is not commit
/// evidence the worker could sample) nor an endless pending (nothing
/// owner-driven will ever satisfy a dead device). The park's ONE exit
/// funnel still hands the leg its release (`Aborted` when none was
/// routed — the once-on-this-leg consumption discipline holds on every
/// exit), the disengagement fence still publishes, and the gate reports
/// the failure back instead of aborting the leg itself. No release is
/// ever routed here: only the failed observation ends this park.
#[test]
fn a_failed_tail_observation_exits_the_seek_park_reported_and_settled() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));
    gate.set_seek_hold(true);
    let parked = Arc::new(AtomicBool::new(false));
    let parked_clone = parked.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    // The witness leg's handle is RETAINED: a premise failure must
    // release and collect the leg (RELEASE != JOIN), never detach it.
    let leg = {
        let gate = gate.clone();
        std::thread::spawn(move || {
            let captured: Arc<Mutex<Vec<SeekParkRelease>>> = Arc::new(Mutex::new(Vec::new()));
            let sink = captured.clone();
            let outcome = gate.park_loop_top(|slice| match slice {
                GateSlice::TailProbe => {
                    parked_clone.store(true, Ordering::SeqCst);
                    TailProbeOutcome::Failed
                }
                GateSlice::SeekRelease(release) => {
                    sink.lock().expect("release lock").push(release);
                    TailProbeOutcome::Pending
                }
            });
            let _ = tx.send((captured, outcome));
        })
    };
    let leg = GateLeg::new(&gate, leg);
    if !wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst)) {
        panic!("the leg never parked under the routed hold");
    }
    let received = rx.recv_timeout(Duration::from_secs(5)).ok();
    let Some((captured, outcome)) = received else {
        panic!("the park never ended on the failed observation within 5s (no release routed)");
    };
    leg.join("failed-tail leg after its reported exit");
    assert_eq!(
        outcome,
        ParkOutcome::TailProbeFailed,
        "the gate reports the mechanism's own failed observation; it never aborts the leg"
    );
    assert_eq!(
        *captured.lock().expect("release lock"),
        vec![SeekParkRelease::Aborted],
        "the failure exit still consumes the leg's release through the ONE exit funnel"
    );
    assert_eq!(
        events.snapshot(),
        vec![GateEvent::SeekEngaged, GateEvent::SeekDisengaged],
        "no seek quiescence may publish for a failed observation: {:?}",
        events.snapshot()
    );
}

/// Test-owned cleanup, not a product invariant: both pre-witness failure
/// and an assertion after a real park must preserve the panic and collect
/// seek-only / pause+seek legs. The joined flag is written AFTER join.
#[test]
fn assertions_before_and_after_park_release_and_join_the_owned_leg() {
    for paused in [false, true] {
        for after_park in [false, true] {
            let gate = RenderGate::new();
            gate.set_paused(paused);
            gate.set_seek_hold(true);
            let (tx, rx) = std::sync::mpsc::channel();
            let leg_gate = gate.clone();
            let leg = GateLeg::new(
                &gate,
                std::thread::spawn(move || {
                    gate_probe(&leg_gate, || {
                        let _ = tx.send(());
                        false
                    })
                }),
            );
            let joined = leg.joined.clone();
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let _owned = leg;
                if after_park {
                    rx.recv_timeout(Duration::from_secs(5))
                        .expect("real TailProbe entry");
                }
                panic!("original semantic assertion");
            }))
            .expect_err("the original assertion must propagate");
            assert_eq!(
                panic.downcast_ref::<&str>(),
                Some(&"original semantic assertion")
            );
            assert!(
                joined.load(Ordering::SeqCst),
                "release alone is insufficient"
            );
        }
    }
}
