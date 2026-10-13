//! RenderGate protocol tests (F3, ADR-PBK-002 D14.7): the
//! session-owned pause gate's park/release discipline at the ports seam,
//! driven through the ONE unified loop-top operation these tests' render
//! providers inherit (`park_loop_top`). They pin: park in bounded
//! slices, acknowledge engagement / tail quiescence / disengagement as
//! evidence, never abort the caller's leg. (These tests route no seek
//! protocol; the seek shapes live in `render_gate_seek.rs`.)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use qianqian_audio_api::ports::{GateEvent, GateSlice, ParkOutcome, RenderGate, TailProbeOutcome};

/// Run the unified loop-top gate with `tail` as the tail probe, ignoring
/// seek releases (none is routed in these tests).
fn pause_only(gate: &RenderGate, mut tail: impl FnMut() -> bool) {
    pause_with_outcome(gate, move || {
        if tail() {
            TailProbeOutcome::Quiesced
        } else {
            TailProbeOutcome::Pending
        }
    });
}

/// [`pause_only`] with the leg answering the probe in its own truth
/// class — the failure injection point (F5 implementation corrective-4).
fn pause_with_outcome(
    gate: &RenderGate,
    mut tail: impl FnMut() -> TailProbeOutcome,
) -> ParkOutcome {
    gate.park_loop_top(|slice| match slice {
        GateSlice::TailProbe => tail(),
        GateSlice::SeekRelease(_) => TailProbeOutcome::Pending,
    })
}

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
    pause_only(&gate, || {
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
    pause_only(&gate, || {
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
            pause_only(&gate, || tail_calls.fetch_add(1, Ordering::SeqCst) >= 2);
            // The leg is back at its loop top; the gate must not park it
            // again on the same routed intent (it was released).
            pause_only(&gate, || false);
        })
    };

    let leg = GateLeg::new(&gate, leg);
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
    leg.join("the parked leg after its release");

    assert_eq!(
        events.snapshot(),
        vec![
            GateEvent::Engaged,
            GateEvent::TailQuiesced,
            GateEvent::Disengaged
        ]
    );
    assert_eq!(
        tail_calls.load(Ordering::SeqCst),
        3,
        "quiescence on probe 3 stops future probes"
    );
}

/// Each NEW engagement must re-observe quiescence: a previous cycle's
/// TailQuiesced never satisfies a later pause (the D14.7 corrective
/// invariant, at the mechanism level).
#[test]
fn tail_quiescence_belongs_to_each_engagement_separately() {
    let events = Events::default();
    let writer = events.clone();
    let gate = RenderGate::with_observer(move |event| writer.record(event));
    let mut expected = Vec::new();
    for cycle in 1..=2 {
        gate.set_paused(true);
        let leg_gate = gate.clone();
        let leg = GateLeg::new(
            &gate,
            std::thread::spawn(move || pause_only(&leg_gate, || true)),
        );
        assert!(
            wait_until(Duration::from_secs(5), || events
                .snapshot()
                .iter()
                .filter(|e| **e == GateEvent::TailQuiesced)
                .count()
                == cycle),
            "cycle {cycle} must earn its own quiescence"
        );
        gate.set_paused(false);
        leg.join("quiesced pause cycle");
        expected.extend([
            GateEvent::Engaged,
            GateEvent::TailQuiesced,
            GateEvent::Disengaged,
        ]);
        assert_eq!(
            events.snapshot(),
            expected,
            "each settled cycle has its own ordered evidence"
        );
    }

    // Real Pending callback, not a timed negative window. Observe the
    // entire cycle through release AND actual join, including exit events.
    gate.set_paused(true);
    let (tx, rx) = std::sync::mpsc::channel();
    let leg_gate = gate.clone();
    let leg = GateLeg::new(
        &gate,
        std::thread::spawn(move || {
            pause_only(&leg_gate, || {
                let _ = tx.send(());
                false
            })
        }),
    );
    rx.recv_timeout(Duration::from_secs(5))
        .expect("third cycle reached its real Pending probe");
    let mut prefix = expected.clone();
    prefix.push(GateEvent::Engaged);
    assert_eq!(
        events.snapshot(),
        prefix,
        "prior quiescence cannot authorize the Pending cycle"
    );
    gate.set_paused(false);
    leg.join("Pending pause cycle");
    expected.extend([GateEvent::Engaged, GateEvent::Disengaged]);
    assert_eq!(
        events.snapshot(),
        expected,
        "no quiescence, including at the release/exit outlet"
    );
}

/// The open-abort lifetime (D14.7 corrective-2): once a gate is closed,
/// no later pause intent — routed or hostile — can ever park a leg
/// again. A leg that arrives after the close finds the gate shut:
/// immediate return, no engagement, no events. There is no un-close.
/// The park runs on a helper thread under a bounded wait so a regressed
/// gate fails with a diagnosis instead of hanging the suite.
#[test]
fn a_closed_gate_never_parks_or_engages_again() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));

    gate.set_paused(true);
    gate.close_and_release();
    // Hostile later intent, routed after the close: inert by design.
    gate.set_paused(true);

    let tail_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (tx, rx) = std::sync::mpsc::channel();
    let leg = {
        let gate = gate.clone();
        let tail_calls = tail_calls.clone();
        std::thread::spawn(move || {
            pause_only(&gate, || {
                tail_calls.fetch_add(1, Ordering::SeqCst);
                false
            });
            let _ = tx.send(());
        })
    };
    let leg = GateLeg::new(&gate, leg);
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)),
        Ok(()),
        "the closed gate parked the leg: it never returned from \
         the loop-top gate"
    );
    leg.join("closed gate leg");
    assert_eq!(
        tail_calls.load(Ordering::SeqCst),
        0,
        "the closed gate must not park the leg"
    );
    assert!(
        events.snapshot().is_empty(),
        "the closed gate must publish no engagement evidence: {:?}",
        events.snapshot()
    );
}

/// F5 implementation corrective-4 (C9, the pause-attributed half): a
/// tail observation that itself FAILS is neither quiescence evidence
/// nor "not quiesced yet" — masking it as pending would park the leg
/// forever while only an owner release could wake it. The park must end
/// on its own, bounded, with NO TailQuiesced published (a failed
/// observation is not commit evidence) and the disengagement fence
/// still published, and the gate must report the failure back instead
/// of aborting the leg itself. The pause intent stays routed the whole
/// time — no release ever wakes this park.
#[test]
fn a_failed_tail_observation_exits_the_pause_park_without_quiescence() {
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));
    gate.set_paused(true);
    let parked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let parked_clone = parked.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let leg = {
        let gate = gate.clone();
        std::thread::spawn(move || {
            let outcome = pause_with_outcome(&gate, || {
                parked_clone.store(true, Ordering::SeqCst);
                TailProbeOutcome::Failed
            });
            let _ = tx.send(outcome);
        })
    };
    let leg = GateLeg::new(&gate, leg);
    assert!(
        wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst)),
        "the leg never parked"
    );
    let outcome = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the park must end on the failed observation, with pause intent still routed");
    leg.join("failed pause probe leg");
    assert_eq!(
        outcome,
        ParkOutcome::TailProbeFailed,
        "the gate reports the mechanism's own failed observation; it never aborts the leg"
    );
    assert_eq!(
        events.snapshot(),
        vec![GateEvent::Engaged, GateEvent::Disengaged],
        "no quiescence may publish for a failed observation: {:?}",
        events.snapshot()
    );
}

/// The failed-park exit with a REAL routed payload: a committed cut
/// whose release payload arrives while the leg is pause-parked, and
/// whose probe then fails, must still deliver the rebase instruction to
/// the leg (mid-park or on the failure exit — exactly once either way)
/// before the mechanism's failure path runs. The pause-park failure
/// exit's payload-delivery arm, exercised with a routed Committed
/// payload — the seek-side twin pins the exit funnel's Aborted shape
/// (render_gate_seek.rs).
#[test]
fn a_failed_pause_park_still_delivers_a_routed_committed_payload() {
    use qianqian_audio_api::ports::SeekParkRelease;
    use std::sync::Mutex;
    let events = Events::default();
    let events_clone = events.clone();
    let gate = RenderGate::with_observer(move |event| events_clone.record(event));
    gate.set_paused(true);
    let parked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let parked_clone = parked.clone();
    let captured: Arc<Mutex<Vec<SeekParkRelease>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = captured.clone();
    let failed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let failed_reader = failed.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let leg = {
        let gate = gate.clone();
        let failed_reader = failed_reader.clone();
        std::thread::spawn(move || {
            let outcome = gate.park_loop_top(|slice| match slice {
                GateSlice::TailProbe => {
                    parked_clone.store(true, Ordering::SeqCst);
                    if failed_reader.load(Ordering::SeqCst) {
                        TailProbeOutcome::Failed
                    } else {
                        TailProbeOutcome::Pending
                    }
                }
                GateSlice::SeekRelease(release) => {
                    sink.lock().expect("release lock").push(release);
                    TailProbeOutcome::Pending
                }
            });
            let _ = tx.send(outcome);
        })
    };
    let leg = GateLeg::new(&gate, leg);
    assert!(
        wait_until(Duration::from_secs(5), || parked.load(Ordering::SeqCst)),
        "the leg never parked"
    );
    // The cut commits while the leg is parked: the payload routes
    // MID-PARK and must reach the leg while it stays parked.
    gate.release_seek_hold(SeekParkRelease::Committed { landing: Some(9) });
    assert!(
        wait_until(Duration::from_secs(5), || {
            !captured.lock().expect("release lock").is_empty()
        }),
        "committed callback must arrive before forcing the failed-tail exit"
    );
    // Then the device's observation fails: the park must still end
    // bounded, with no quiescence published.
    failed.store(true, Ordering::SeqCst);
    let outcome = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the park must end on the failed observation");
    leg.join("failed pause probe leg");
    assert_eq!(outcome, ParkOutcome::TailProbeFailed);
    assert_eq!(
        *captured.lock().expect("release lock"),
        vec![SeekParkRelease::Committed { landing: Some(9) }],
        "the committed rebase reached the leg before the failure exit"
    );
    assert_eq!(
        events.snapshot(),
        vec![GateEvent::Engaged, GateEvent::Disengaged],
        "no quiescence may publish for the failed observation: {:?}",
        events.snapshot()
    );
}

#[test]
fn a_pause_assertion_after_engagement_releases_and_joins() {
    let gate = RenderGate::new();
    gate.set_paused(true);
    let (tx, rx) = std::sync::mpsc::channel();
    let leg_gate = gate.clone();
    let leg = GateLeg::new(
        &gate,
        std::thread::spawn(move || {
            pause_only(&leg_gate, || {
                let _ = tx.send(());
                false
            })
        }),
    );
    let joined = leg.joined.clone();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _owned = leg;
        rx.recv_timeout(Duration::from_secs(5))
            .expect("real Pending park");
        panic!("original pause assertion");
    }))
    .expect_err("assertion must propagate");
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"original pause assertion")
    );
    assert!(joined.load(Ordering::SeqCst));
}
