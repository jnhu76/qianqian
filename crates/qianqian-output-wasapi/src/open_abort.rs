//! Platform-independent open-abort protocol for gated render threads
//! (D14.7). A provider aborting a render thread whose stream was never
//! handed to the session (open failure / open timeout) must release the
//! routed pause intent BEFORE joining the leg: pause intent may already
//! sit at the gate (pre-activation pause is a supported episode-seam
//! state), and a leg parked at the gate cannot observe the data-plane
//! stop — joining it without the release would never return.
//!
//! Windows-only by mechanism, not by protocol: this module carries no
//! COM and compiles — and is tested — on every platform.
//!
//! Boundary note: the release here is the provider's own, not routed
//! through the episode completion lock (the stream was never handed to
//! a session, so there is nothing to linearize against). The release is
//! therefore made permanent at the mechanism itself —
//! [`RenderGate::close_and_release`]: once the abort begins, no later
//! pause intent on this gate (routed through a handle handed out before
//! activation settles, or otherwise) can re-park the leg the join is
//! waiting on. The gate is finished either way: its stream will never
//! become an episode.

use std::sync::Arc;
use std::thread::JoinHandle;

use qianqian_audio_api::ports::{RenderGate, RenderPcmInput};

/// Close the gate permanently, stop the data plane, then join. Callers
/// must not hold any lock the joining thread needs (the WASAPI
/// open-verdict mutex, for one). The production caller is the Windows
/// mechanism; on other platforms the protocol stays live through its
/// tests, which pin the close-before-join order and the closed gate's
/// immunity to a hostile later pause.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn abort_render_thread(
    handle: JoinHandle<()>,
    render_input: &Arc<dyn RenderPcmInput>,
    gate: &RenderGate,
) {
    gate.close_and_release();
    render_input.stop();
    let _ = handle.join();
}

#[cfg(test)]
mod tests {
    use super::*;
    use qianqian_audio_api::ports::{
        GateEvent, GateSlice, PcmPull, SeekParkRelease, TailProbeOutcome,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;

    // Test-only failure cleanup — a RELEASE guarantee, not a join
    // guarantee: on any exit (including an assert unwind) the gate this
    // test deliberately parked the leg behind is closed and released, so
    // no test-owned leg stays blocked behind fixture state. Whether the
    // leg has actually exited is observed separately, at the join
    // boundary.
    struct ReleaseGateOnDrop(RenderGate);

    impl Drop for ReleaseGateOnDrop {
        fn drop(&mut self) {
            self.0.close_and_release();
        }
    }

    struct IdleInput;

    impl RenderPcmInput for IdleInput {
        fn read_frames(&self, _dst: &mut [f32]) -> PcmPull {
            PcmPull::Stopped
        }
        fn stop(&self) {}
    }

    /// Bounded join: an abort that ever wedges fails here instead of
    /// hanging the suite.
    fn join_within(limit: Duration, f: impl FnOnce() + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            f();
            let _ = tx.send(());
        });
        assert_eq!(
            rx.recv_timeout(limit),
            Ok(()),
            "the open-abort join wedged: a parked leg was joined without \
             a gate release"
        );
    }

    /// The wedge this protocol prevents: pause intent routed before the
    /// leg ever reached the gate parks the leg even against a stopped
    /// data plane — only the release wakes it, so the release must
    /// precede the join.
    #[test]
    fn aborting_a_parked_leg_releases_the_gate_before_the_join() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = events.clone();
        let gate = RenderGate::with_observer(move |event| {
            recorder.lock().expect("events lock").push(event);
        });
        let _gate_release = ReleaseGateOnDrop(gate.clone());

        // Pre-activation pause: intent is routed before any leg exists.
        gate.set_paused(true);
        let (exit_tx, exit_rx) = mpsc::channel();
        let leg = {
            let gate = gate.clone();
            std::thread::spawn(move || {
                // The steady loop's loop-top posture: park before any
                // read, quiescence unobserved.
                gate.park_loop_top(|slice| match slice {
                    GateSlice::TailProbe => TailProbeOutcome::Pending,
                    GateSlice::SeekRelease(_) => TailProbeOutcome::Pending,
                });
                // Normal-exit witness: sent ONLY past the expected
                // post-park exit. abort_render_thread swallows a panicked
                // leg (`let _ = handle.join()`), so join success alone
                // cannot tell a normal exit from a panic; the post-join
                // assert on this witness turns an abnormal exit into a
                // test failure instead of a silent pass.
                let _ = exit_tx.send(());
            })
        };
        // Mutex-published gate events persist in the shared log, so this
        // poll cannot miss an engagement; the sleep only throttles the
        // re-check and the deadline is a fail-fast watchdog.
        let parked = {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                if events
                    .lock()
                    .expect("events lock")
                    .contains(&GateEvent::Engaged)
                {
                    break true;
                }
                if std::time::Instant::now() >= deadline {
                    break false;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        };
        assert!(parked, "the leg never reached the gate");

        let input: Arc<dyn RenderPcmInput> = Arc::new(IdleInput);
        join_within(Duration::from_secs(5), move || {
            abort_render_thread(leg, &input, &gate)
        });
        // Settlement boundary: abort_render_thread has joined the leg, so
        // its exit is final here — a missing witness now can only mean an
        // abnormal (panicked) exit that the abort join swallowed.
        assert_eq!(
            exit_rx.recv_timeout(Duration::from_secs(5)),
            Ok(()),
            "the leg did not exit through its expected post-park path: the \
             abort join swallows a panicked leg, so only this witness \
             proves a normal exit"
        );
    }

    /// Given a seek hold routed before any leg exists (the pre-worker
    /// establishment window's plant shape: `set_seek_hold(true)` inside
    /// the admission unit), and a leg whose first loop-top therefore
    /// seek-parks, when the open abort runs, then the permanent close must
    /// release the parked leg BEFORE the join, the leg must exit through
    /// its normal post-park path (not a panic the abort join swallows),
    /// and the gate must record exactly one seek engagement and one
    /// disengagement.
    ///
    /// PROPERTY / AUTHORITY: the D14.7 open-abort order pinned at the real
    /// [`abort_render_thread`] for the SEEK attribution (D14.5 owns the
    /// seek park itself; the pause-attribution fixtures in this module
    /// cover the pause park). A seek hold has no other discharge on the
    /// open-failure path: no decode worker exists (no exit funnel) and
    /// the session's teardown release is registered only after a
    /// successful open.
    ///
    /// REACHABILITY QUALIFIER (E2-C1 Stage 0, #298): a seek-parked leg at
    /// abort-close is production-reachable in the open-failure family
    /// only through the late-success-after-timeout race (a leg aborted on
    /// the Failed verdict never reaches its first gate visit). This
    /// fixture constructs the parked posture at the protocol seam; it
    /// does not claim every aborted leg parked.
    ///
    /// FALSIFIER (executed negative control, M-h): delete the
    /// `close_and_release` call in `abort_render_thread` and this test
    /// wedges at the join watchdog — no releaser exists on any schedule
    /// in this fixture world. Claim ceiling: that mutant reds the join
    /// watchdog before the post-join assertions, so it validates the
    /// close-before-join property, not the exit witness or the release
    /// payload (those are guarded by construction and inspection).
    #[test]
    fn aborting_a_seek_parked_leg_releases_the_gate_before_the_join() {
        // Given: a seek hold routed before the leg exists (the
        // pre-worker window's plant shape).
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = events.clone();
        let gate = RenderGate::with_observer(move |event| {
            recorder.lock().expect("events lock").push(event);
        });
        let _gate_release = ReleaseGateOnDrop(gate.clone());
        gate.set_seek_hold(true);

        // When: the steady loop's first gate visit parks on that hold.
        let releases = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (exit_tx, exit_rx) = mpsc::channel();
        let leg = {
            let gate = gate.clone();
            let releases = releases.clone();
            std::thread::spawn(move || {
                gate.park_loop_top(|slice| match slice {
                    GateSlice::TailProbe => TailProbeOutcome::Pending,
                    GateSlice::SeekRelease(release) => {
                        releases.lock().expect("release log").push(release);
                        TailProbeOutcome::Pending
                    }
                });
                // Normal-exit witness: sent ONLY past the expected
                // post-park exit. abort_render_thread swallows a panicked
                // leg (`let _ = handle.join()`), so join success alone
                // cannot tell a normal exit from a panic; the post-join
                // assert on this witness turns an abnormal exit into a
                // test failure instead of a silent pass.
                let _ = exit_tx.send(());
            })
        };
        // Mutex-published gate events persist in the shared log, so this
        // poll cannot miss an engagement; the sleep only throttles the
        // re-check and the deadline is a fail-fast watchdog.
        let parked = {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                if events
                    .lock()
                    .expect("events lock")
                    .contains(&GateEvent::SeekEngaged)
                {
                    break true;
                }
                if std::time::Instant::now() >= deadline {
                    break false;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        };
        assert!(parked, "the leg never seek-parked at the gate");

        // Then: the abort's close releases the seek park before its join.
        let input: Arc<dyn RenderPcmInput> = Arc::new(IdleInput);
        join_within(Duration::from_secs(5), move || {
            abort_render_thread(leg, &input, &gate)
        });

        // Settlement boundary: the abort joined the leg, so its exit is
        // final; only this witness separates a normal exit from a panic
        // the join swallowed.
        assert_eq!(
            exit_rx.recv_timeout(Duration::from_secs(5)),
            Ok(()),
            "the leg did not exit through its expected post-park path"
        );
        // One park cycle, one disengagement: the closed exit consumed the
        // synthesized abort release. (This fixture has a single loop-top
        // visit, so it does not itself witness a re-park attempt; the
        // closed-gate immunity is pinned by the hostile-re-pause fixture
        // below, where the leg is forced around another visit.)
        let recorded = events.lock().expect("events lock");
        assert_eq!(
            *releases.lock().expect("release log"),
            vec![SeekParkRelease::Aborted],
            "the closed exit must consume exactly the synthesized abort release"
        );
        assert_eq!(
            recorded
                .iter()
                .filter(|e| **e == GateEvent::SeekEngaged)
                .count(),
            1,
            "exactly one seek engagement: {recorded:?}"
        );
        assert_eq!(
            recorded
                .iter()
                .filter(|e| **e == GateEvent::SeekDisengaged)
                .count(),
            1,
            "the seek-parked leg disengaged exactly once: {recorded:?}"
        );
    }

    /// The leg's first read HOLDS — until the abort's stop has been
    /// observed AND the test releases it — so the leg stays
    /// deterministically BETWEEN two loop-top gate visits while the
    /// hostile re-pause lands: it cannot answer the gate, which is a
    /// wedge's precondition. Its release is deliberately a
    /// NON-TERMINAL pull, forcing one more loop-top visit AFTER the
    /// close — the exact point a re-park wedge would form. (A pause
    /// arriving mid-read and parking the leg at its NEXT visit is the
    /// gate's own timing property — no ordering between router and leg
    /// is required — and is pinned by the gate-level test in
    /// qianqian-audio-api, not here; THIS test pins the pre-parked
    /// state, the abort protocol's motivating state, which is why its
    /// setup routes the pause before the leg exists.)
    #[derive(Default)]
    struct HeldState {
        stopped: bool,
        let_through: bool,
    }

    struct HeldInput {
        first_call: AtomicBool,
        state: Mutex<HeldState>,
        cv: Condvar,
        on_stop: Mutex<Option<mpsc::Sender<()>>>,
    }

    impl HeldInput {
        fn new(on_stop: mpsc::Sender<()>) -> Self {
            Self {
                first_call: AtomicBool::new(true),
                state: Mutex::new(HeldState::default()),
                cv: Condvar::new(),
                on_stop: Mutex::new(Some(on_stop)),
            }
        }

        fn release(&self) {
            let mut state = self.state.lock().expect("held input lock");
            state.let_through = true;
            drop(state);
            self.cv.notify_all();
        }
    }

    // Test-only failure cleanup — a RELEASE guarantee, not a join
    // guarantee: on any exit the intentionally held read is stopped and
    // released so it cannot wait on this Condvar forever. The leg's
    // exit is observed at the abort join, not here.
    struct ReleaseHeldInputOnDrop(Arc<HeldInput>);

    impl Drop for ReleaseHeldInputOnDrop {
        fn drop(&mut self) {
            self.0.stop();
            self.0.release();
        }
    }

    impl RenderPcmInput for HeldInput {
        fn read_frames(&self, dst: &mut [f32]) -> PcmPull {
            if self.first_call.swap(false, Ordering::AcqRel) {
                let mut state = self.state.lock().expect("held input lock");
                while !state.stopped || !state.let_through {
                    state = self.cv.wait(state).expect("held input wait");
                }
                dst.fill(0.0);
                return PcmPull::Frames(dst.len());
            }
            PcmPull::Stopped
        }

        fn stop(&self) {
            let mut state = self.state.lock().expect("held input lock");
            state.stopped = true;
            if let Some(tx) = self.on_stop.lock().expect("on_stop lock").take() {
                let _ = tx.send(());
            }
            drop(state);
            self.cv.notify_all();
        }
    }

    /// The corrective-2 wedge: the abort's close is permanent, so a
    /// hostile pause routed AFTER it — the handle handed out before
    /// activation settles is a supported seam state — can never re-park
    /// the leg the join is waiting on. The abort is already past its
    /// close (proven by the observed data-plane stop) when the hostile
    /// intent lands; the leg is then forced around one more loop-top
    /// gate visit before it may exit. Without the closed state the
    /// hostile pause re-establishes the parked state there and the join
    /// wedges forever; with it the gate admits no second park.
    #[test]
    fn a_hostile_repause_after_the_close_cannot_repark_the_aborting_leg() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = events.clone();
        let gate = RenderGate::with_observer(move |event| {
            recorder.lock().expect("events lock").push(event);
        });
        let _gate_release = ReleaseGateOnDrop(gate.clone());

        // The pause is routed BEFORE the leg exists — the module doc's
        // own pre-activation state ("pause intent may already sit at the
        // gate") and the determinism requirement of this setup: the
        // leg's FIRST loop-top visit must find the pause already routed
        // so it parks (Engaged) instead of sailing through to the held
        // first read. The first read blocks until the abort phase, so a
        // leg that passed an unpaused gate could never come back to
        // park before `assert!(parked)` — the wait would be a
        // guaranteed deadlock (CI: the 5.00s timeout at this assert).
        // With the pause pre-routed, the hostile re-pause below still
        // lands while the leg is deterministically alive between two
        // loop-top gate visits: it sits in the held first read.
        gate.set_paused(true);

        let (stopped_tx, stopped_rx) = mpsc::channel();
        let held = Arc::new(HeldInput::new(stopped_tx));
        let _held_release = ReleaseHeldInputOnDrop(held.clone());
        let input: Arc<dyn RenderPcmInput> = held.clone();

        // The steady loop's loop-top posture: the gate is checked before
        // EVERY read, so the leg re-visits it after any non-terminal
        // pull.
        let (exit_tx, exit_rx) = mpsc::channel();
        let leg = {
            let gate = gate.clone();
            let input = input.clone();
            std::thread::spawn(move || {
                let mut buf = [0.0f32; 64];
                loop {
                    gate.park_loop_top(|slice| match slice {
                        GateSlice::TailProbe => TailProbeOutcome::Pending,
                        GateSlice::SeekRelease(_) => TailProbeOutcome::Pending,
                    });
                    if matches!(input.read_frames(&mut buf), PcmPull::Stopped) {
                        break;
                    }
                }
                // Normal-exit witness: sent ONLY after the expected
                // Stopped exit. abort_render_thread swallows a panicked
                // leg (`let _ = handle.join()`), so join success alone
                // cannot tell a normal exit from a panic; the post-join
                // assert on this witness turns an abnormal exit into a
                // test failure instead of a silent pass.
                let _ = exit_tx.send(());
            })
        };

        // Mutex-published gate events persist in the shared log, so this
        // poll cannot miss an engagement; the sleep only throttles the
        // re-check and the deadline is a fail-fast watchdog.
        let parked = {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                if events
                    .lock()
                    .expect("events lock")
                    .contains(&GateEvent::Engaged)
                {
                    break true;
                }
                if std::time::Instant::now() >= deadline {
                    break false;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        };
        assert!(parked, "the leg never reached the gate");

        let (done_tx, done_rx) = mpsc::channel();
        {
            let input = input.clone();
            let gate = gate.clone();
            std::thread::spawn(move || {
                abort_render_thread(leg, &input, &gate);
                let _ = done_tx.send(());
            });
        }

        // The stop is issued strictly after the close inside the abort,
        // so observing it proves the gate is already closed.
        assert_eq!(
            stopped_rx.recv_timeout(Duration::from_secs(5)),
            Ok(()),
            "the abort never reached its data-plane stop"
        );

        // Hostile re-pause on the closed gate: routed pause intent, the
        // exact shape a pre-activation handle's request_pause produces.
        // On a closed gate this is inert by design.
        gate.set_paused(true);

        // Let the held read finish; the leg must pass one more loop-top
        // gate visit before it can exit.
        held.release();

        assert_eq!(
            done_rx.recv_timeout(Duration::from_secs(5)),
            Ok(()),
            "the open-abort join wedged: a hostile re-pause after the close \
             re-parked the leg"
        );

        // Settlement boundary: abort_render_thread has joined the leg, so
        // its exit is final here — a missing witness now can only mean an
        // abnormal (panicked) exit that the abort join swallowed.
        assert_eq!(
            exit_rx.recv_timeout(Duration::from_secs(5)),
            Ok(()),
            "the leg did not exit through its expected Stopped path: the \
             abort join swallows a panicked leg, so only this witness \
             proves a normal exit"
        );

        // And the gate recorded exactly one engagement: it never
        // admitted a second park after the close.
        let recorded = events.lock().expect("events lock");
        assert_eq!(
            recorded
                .iter()
                .filter(|e| **e == GateEvent::Engaged)
                .count(),
            1,
            "the closed gate admitted a second park: {recorded:?}"
        );
        assert_eq!(
            recorded
                .iter()
                .filter(|e| **e == GateEvent::Disengaged)
                .count(),
            1,
            "the leg disengaged exactly once: {recorded:?}"
        );
    }
}
