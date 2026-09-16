//! The simulated device: a `DeviceOps` implementation modeling the
//! essential physics of the shared-mode event-driven render target —
//! a device buffer of `buffer_frames`, submitted audio consumed
//! independently of the render loop (padding drains), GetBuffer handing
//! out only the unsubmitted space, and — for mechanism B — a
//! device-level Stop that freezes consumption.
//!
//! What is deliberately NOT modeled: acoustics, mixer behavior, clock
//! drift. Those are the Windows probe's business; the scenarios only
//! need consumption independence and buffer accounting to be faithful.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::events::{Event, Log};

/// Device-side operations the render loop needs. The real WASAPI
/// surface (probe) implements the same order natively.
pub trait DeviceOps: Send + Sync {
    /// Wait for one device period (event-driven cadence).
    fn wait_period(&self);
    /// Frames submitted but not yet consumed by the device.
    fn padding(&self) -> usize;
    /// Total device buffer capacity in frames.
    fn buffer_frames(&self) -> usize;
    /// Acquire `frames` frames of device buffer (never called while
    /// parked by the gated loop). Returns the payload slice length
    /// granted.
    fn get_buffer(&self, frames: usize) -> usize;
    /// Release `frames` frames as submitted.
    fn release_buffer(&self, frames: usize);
    /// Mechanism B only: freeze device consumption.
    fn device_stop(&self);
    /// Mechanism B only: resume device consumption.
    fn device_start(&self);
}

struct SimDeviceState {
    padding: usize,
    consuming: bool,
    stopped_by_mechanism: bool,
}

pub struct SimDevice {
    buffer_frames: usize,
    period_frames: usize,
    tick: Duration,
    state: Mutex<SimDeviceState>,
    log: Log,
    drainer: Mutex<Option<JoinHandle<()>>>,
    stop_drainer: Arc<std::sync::atomic::AtomicBool>,
    /// Test-only determinism control: when held, the drainer thread
    /// suspends consumption WITHOUT any device-level Stop semantics or
    /// events (mechanism-B's `device_stop` is a different, semantic
    /// operation). It freezes the drain timeline so a scenario can
    /// deterministically sample the engaged-with-pending-tail state
    /// that the physical probe measures in real time.
    drain_held: Arc<std::sync::atomic::AtomicBool>,
}

impl SimDevice {
    /// A simulated device. `period_frames` is how much submitted audio
    /// one consumption tick plays out.
    pub fn new(buffer_frames: usize, period_frames: usize, tick: Duration, log: Log) -> Arc<Self> {
        Arc::new(Self {
            buffer_frames,
            period_frames,
            tick,
            state: Mutex::new(SimDeviceState {
                padding: 0,
                consuming: true,
                stopped_by_mechanism: false,
            }),
            log,
            drainer: Mutex::new(None),
            stop_drainer: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            drain_held: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// Start the independent consumption thread: while the device is
    /// consuming and padding > 0, one period is consumed per tick.
    pub fn start_drainer(self: &Arc<Self>) {
        let me = Arc::clone(self);
        let handle = std::thread::Builder::new()
            .name("sim-device-drain".into())
            .spawn(move || loop {
                if me.stop_drainer.load(std::sync::atomic::Ordering::Acquire) {
                    return;
                }
                std::thread::sleep(me.tick);
                let mut st = me.state.lock().expect("sim device lock");
                if st.consuming
                    && st.padding > 0
                    && !me.drain_held.load(std::sync::atomic::Ordering::Acquire)
                {
                    let take = st.padding.min(me.period_frames);
                    st.padding -= take;
                    drop(st);
                    me.log.push(Event::DeviceConsumed(take));
                }
            })
            .expect("drainer spawn");
        *self.drainer.lock().expect("drainer lock") = Some(handle);
    }

    /// Join the drainer (test teardown).
    pub fn join_drainer(&self) {
        self.stop_drainer
            .store(true, std::sync::atomic::Ordering::Release);
        if let Some(h) = self.drainer.lock().expect("drainer lock").take() {
            let _ = h.join();
        }
    }

    /// Test-only: suspend/resume the drainer's consumption timeline
    /// without device-level Stop semantics or events (see field doc).
    pub fn hold_drain(&self, hold: bool) {
        self.drain_held
            .store(hold, std::sync::atomic::Ordering::Release);
    }
}

impl DeviceOps for SimDevice {
    fn wait_period(&self) {
        std::thread::sleep(self.tick);
    }

    fn padding(&self) -> usize {
        self.state.lock().expect("sim device lock").padding
    }

    fn buffer_frames(&self) -> usize {
        self.buffer_frames
    }

    fn get_buffer(&self, frames: usize) -> usize {
        let available = self.buffer_frames.saturating_sub(self.padding());
        let grant = frames.min(available);
        self.log.push(Event::GetBufferEnter);
        grant
    }

    fn release_buffer(&self, frames: usize) {
        let mut st = self.state.lock().expect("sim device lock");
        st.padding = (st.padding + frames).min(self.buffer_frames);
        drop(st);
        self.log.push(Event::ReleaseBuffer);
    }

    fn device_stop(&self) {
        let mut st = self.state.lock().expect("sim device lock");
        st.stopped_by_mechanism = true;
        st.consuming = false;
        drop(st);
        self.log.push(Event::DeviceStop);
    }

    fn device_start(&self) {
        let mut st = self.state.lock().expect("sim device lock");
        st.stopped_by_mechanism = false;
        st.consuming = true;
        drop(st);
        self.log.push(Event::DeviceStart);
    }
}
