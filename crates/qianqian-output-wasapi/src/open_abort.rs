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
    use qianqian_audio_api::ports::{GateEvent, PcmPull};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;

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

        // Pre-activation pause: intent is routed before any leg exists.
        gate.set_paused(true);
        let leg = {
            let gate = gate.clone();
            std::thread::spawn(move || {
                // The steady loop's loop-top posture: park before any
                // read, quiescence unobserved.
                gate.park_while_paused(|| false)
            })
        };
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
    }

    /// The leg's first read holds until the abort's stop has been
    /// observed AND the test releases it, so the hostile re-pause below
    /// lands while the leg is deterministically alive between two
    /// loop-top gate visits. Its first pull is deliberately
    /// non-terminal, forcing the leg back to the gate — the exact point
    /// a re-park wedge would form.
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

        let (stopped_tx, stopped_rx) = mpsc::channel();
        let held = Arc::new(HeldInput::new(stopped_tx));
        let input: Arc<dyn RenderPcmInput> = held.clone();

        // The steady loop's loop-top posture: the gate is checked before
        // EVERY read, so the leg re-visits it after any non-terminal
        // pull.
        let leg = {
            let gate = gate.clone();
            let input = input.clone();
            std::thread::spawn(move || {
                let mut buf = [0.0f32; 64];
                loop {
                    gate.park_while_paused(|| false);
                    if matches!(input.read_frames(&mut buf), PcmPull::Stopped) {
                        break;
                    }
                }
            })
        };

        gate.set_paused(true);
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
