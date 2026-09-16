//! The render-loop mechanism variants under test — the production
//! `steady_loop` order (wait → padding → GetBuffer → read → release)
//! with the two candidate pause mechanisms:
//!
//! ```text
//! A  GateOnly           park at the loop top, strictly before GetBuffer
//! B  GatePlusDeviceStop the same park, wrapped in device Stop/Start
//! ```
//!
//! Stop semantics for both: the gate's stop wake is
//! **unpark-and-continue** — the gate never aborts the loop; the loop
//! proceeds into its steady iteration and the data-plane terminal
//! (`PcmPull::Stopped` mid-play, natural EOF+drain post-EOF) decides.
//! This is what keeps every D11 decision-table history unchanged.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::edge::{PcmEdge, Pull};
use crate::events::{Event, Log};
use crate::gate::PauseGate;
use crate::sim::DeviceOps;

/// Which pause mechanism the loop exercises.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mechanism {
    /// A: explicit render-loop pause gate, located before GetBuffer.
    GateOnly,
    /// B: the same gate plus device-level Stop/Start around the park.
    GatePlusDeviceStop,
}

/// How the render leg terminated (`LoopOutcome` shape).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopOutcome {
    /// EOF was pulled and the device drained to zero padding.
    Drained,
    /// A data-plane or device terminal prevented further progress.
    Aborted,
}

/// Bounds for the EOF drain wait (mirrors the product `DRAIN_CAP`
/// posture; the sim drains quickly).
const DRAIN_CAP: Duration = Duration::from_secs(5);

/// Mechanism-A/B render loop. One call = one render leg (thread body).
pub fn run_gated_loop(
    edge: Arc<PcmEdge>,
    device: Arc<dyn DeviceOps>,
    gate: Arc<PauseGate>,
    mechanism: Mechanism,
    channels: u16,
    log: Log,
) -> LoopOutcome {
    let channels = usize::from(channels);
    let mut dst = vec![0.0f32; device.buffer_frames() * channels];
    loop {
        // ---- pause gate: before any device interaction this iteration,
        //      strictly before GetBuffer; never while holding one. ----
        log.push(Event::GateCheck);
        match mechanism {
            Mechanism::GateOnly => gate.park_while_paused(),
            Mechanism::GatePlusDeviceStop => {
                let o = gate.observe();
                if o.pause_requested && !o.stopped {
                    device.device_stop();
                    gate.park_while_paused();
                    device.device_start();
                }
            }
        }

        device.wait_period();
        let padding = device.padding();
        let available = device.buffer_frames().saturating_sub(padding);
        if available == 0 {
            continue;
        }

        let grant = device.get_buffer(available);
        if grant == 0 {
            continue;
        }
        match edge.read_frames(&mut dst[..grant * channels]) {
            Pull::Frames(n) => {
                device.release_buffer(n);
                log.push(Event::Pull(Some(n)));
            }
            Pull::Eof => {
                device.release_buffer(0);
                log.push(Event::Pull(None));
                break drain_to_zero(&*device);
            }
            Pull::Stopped => {
                device.release_buffer(0);
                log.push(Event::Pull(None));
                break LoopOutcome::Aborted;
            }
        }
    }
}

fn drain_to_zero(device: &dyn DeviceOps) -> LoopOutcome {
    let deadline = Instant::now() + DRAIN_CAP;
    loop {
        if device.padding() == 0 {
            return LoopOutcome::Drained;
        }
        if Instant::now() > deadline {
            return LoopOutcome::Aborted;
        }
        device.wait_period();
    }
}

/// NEGATIVE-CONTROL loop: a deliberately broken shape that parks AFTER
/// GetBuffer, holding the device buffer across the pause. The scenario
/// suite uses it to prove the "no device buffer is ever held across a
/// parked pause" oracle is non-vacuous (it must catch this loop).
pub fn run_buffer_held_across_pause_loop(
    edge: Arc<PcmEdge>,
    device: Arc<dyn DeviceOps>,
    gate: Arc<PauseGate>,
    channels: u16,
    log: Log,
    running: Arc<AtomicBool>,
) -> LoopOutcome {
    let channels = usize::from(channels);
    let mut dst = vec![0.0f32; device.buffer_frames() * channels];
    loop {
        if !running.load(Ordering::Acquire) {
            break LoopOutcome::Aborted;
        }
        device.wait_period();
        let available = device.buffer_frames().saturating_sub(device.padding());
        if available == 0 {
            continue;
        }
        let grant = device.get_buffer(available);
        if grant == 0 {
            continue;
        }
        // BROKEN: the park happens while the device buffer is held.
        gate.park_while_paused();
        match edge.read_frames(&mut dst[..grant * channels]) {
            Pull::Frames(n) => {
                device.release_buffer(n);
                log.push(Event::Pull(Some(n)));
            }
            Pull::Eof | Pull::Stopped => {
                device.release_buffer(0);
                log.push(Event::Pull(None));
                break LoopOutcome::Aborted;
            }
        }
    }
}
