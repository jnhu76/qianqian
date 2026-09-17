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

use std::sync::Arc;
use std::thread::JoinHandle;

use qianqian_audio_api::ports::{RenderGate, RenderPcmInput};

/// Release the routed pause intent, stop the data plane, then join.
/// Callers must not hold any lock the joining thread needs (the WASAPI
/// open-verdict mutex, for one). The production caller is the Windows
/// mechanism; on other platforms the protocol stays live through its
/// test, which pins the release-before-join order.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn abort_render_thread(
    handle: JoinHandle<()>,
    render_input: &Arc<dyn RenderPcmInput>,
    gate: &RenderGate,
) {
    gate.set_paused(false);
    render_input.stop();
    let _ = handle.join();
}

#[cfg(test)]
mod tests {
    use super::*;
    use qianqian_audio_api::ports::{GateEvent, PcmPull};
    use std::sync::mpsc;
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
}
