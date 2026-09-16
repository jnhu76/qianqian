//! F3-GATE synchronization-shape scenario suite (mechanism evidence).
//!
//! Each scenario drives the production synchronization shape (edge ×
//! worker × render loop) through a pause mechanism and asserts the
//! liveness/truthfulness properties the gate decision needs. All
//! assertions are qualitative with generous margins; nothing here is a
//! product timing contract.
//!
//! Terminology mapping for the D11 resolver-consistency claims: the
//! scenarios establish **mechanism evidence histories** (worker exit
//! terminal × drain outcome × stop-intent ordering). How each history
//! classifies is already frozen and exhaustively pinned by the merged
//! D11 decision-table oracle (PR #144; 48-tuple Rust/TLA byte-compare).
//! The gate shape must only produce histories whose classification is
//! honest; scenarios 2/4/5 pin those histories.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use f3_pause_mechanism::edge::PcmEdge;
use f3_pause_mechanism::establishment::{
    paused_projection, paused_projection_mutated_engagement_only, resumed_projection,
    EstablishmentInputs,
};
use f3_pause_mechanism::events::{Event, Log};
use f3_pause_mechanism::gate::PauseGate;
use f3_pause_mechanism::render::{
    run_buffer_held_across_pause_loop, run_gated_loop, LoopOutcome, Mechanism,
};
use f3_pause_mechanism::sim::{DeviceOps, SimDevice};
use f3_pause_mechanism::worker::{spawn_worker, Source, WorkerExit};

const CHANNELS: u16 = 2;
const EDGE_CAPACITY: usize = 8192;
const DEVICE_FRAMES: usize = 4096;
const PERIOD: usize = 512;
const CHUNK: usize = 1024;
const TICK: Duration = Duration::from_millis(3);

const ENGAGE_TIMEOUT: Duration = Duration::from_secs(2);
const JOIN_TIMEOUT: Duration = Duration::from_secs(5);
const GRACE: Duration = Duration::from_millis(300);

struct Rig {
    edge: Arc<PcmEdge>,
    device: Arc<SimDevice>,
    gate: Arc<PauseGate>,
    log: Log,
    inflight: Arc<AtomicBool>,
}

fn rig() -> Rig {
    let log = Log::new();
    Rig {
        edge: Arc::new(PcmEdge::new(CHANNELS, EDGE_CAPACITY)),
        device: SimDevice::new(DEVICE_FRAMES, PERIOD, TICK, log.clone()),
        gate: Arc::new(PauseGate::with_log(log.clone())),
        log,
        inflight: Arc::new(AtomicBool::new(false)),
    }
}

impl Rig {
    fn spawn_render(self: &Rig, mechanism: Mechanism) -> std::thread::JoinHandle<LoopOutcome> {
        let edge = self.edge.clone();
        let device: Arc<dyn DeviceOps> = self.device.clone();
        let gate = self.gate.clone();
        let log = self.log.clone();
        std::thread::Builder::new()
            .name("sim-render".into())
            .spawn(move || run_gated_loop(edge, device, gate, mechanism, CHANNELS, log))
            .expect("render spawn")
    }

    /// The episode stop path in the shape the gate mechanism requires:
    /// the data-plane stop (product request_stop routing) plus the gate
    /// stop release (the new wake seam the freeze proposes).
    fn session_stop(&self) {
        self.log.push(Event::StopReleased);
        self.edge.stop();
        self.gate.release_stop();
    }

    fn join_drainer(&self) {
        self.device.join_drainer();
    }
}

fn wait_for(predicate: impl Fn() -> bool, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    predicate()
}

fn wait_engaged(gate: &PauseGate) -> bool {
    wait_for(|| gate.observe().engaged, ENGAGE_TIMEOUT)
}

fn wait_disengaged(gate: &PauseGate) -> bool {
    wait_for(|| !gate.observe().engaged, ENGAGE_TIMEOUT)
}

fn join_within<T: Send + 'static>(
    handle: std::thread::JoinHandle<T>,
    timeout: Duration,
) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let out = handle.join().expect("joined thread panicked");
        let _ = tx.send(out);
    });
    rx.recv_timeout(timeout).ok()
}

/// The core truthfulness oracle: the render loop must never hold a
/// device buffer across a parked pause. Tracks GetBuffer→Release
/// pairing: entering the parked state with an unpaired GetBuffer, or
/// any buffer activity while parked, is a violation. (The
/// negative-control scenario feeds the deliberately broken
/// hold-buffer-across-pause loop through this oracle to prove it is
/// not vacuous.)
fn scan_park_safety(log: &Log, require_unparked_at_end: bool) {
    let entries = log.all();
    let mut parked = false;
    let mut buffer_open = false;
    for e in &entries {
        match e.event {
            Event::Engaged => {
                assert!(!parked, "double engagement without disengagement");
                assert!(
                    !buffer_open,
                    "engaged while holding a device buffer (GetBuffer held across pause)"
                );
                parked = true;
            }
            Event::Disengaged => {
                assert!(parked, "disengagement without engagement");
                parked = false;
            }
            Event::GetBufferEnter => {
                assert!(!parked, "GetBuffer entered while parked");
                buffer_open = true;
            }
            Event::ReleaseBuffer => {
                assert!(!parked, "device buffer released while parked");
                buffer_open = false;
            }
            _ => {}
        }
    }
    if require_unparked_at_end {
        assert!(!parked, "scenario ended while still parked");
    }
}

/// Mid-pause incremental check: buffer pairing and parked-state purity
/// only; the leg is legitimately still parked.
fn assert_no_device_buffer_while_parked(log: &Log) {
    scan_park_safety(log, false);
}

/// End-of-scenario check: the leg must also have left the parked state.
fn assert_no_device_buffer_across_park(log: &Log) {
    scan_park_safety(log, true);
}

#[test]
fn gate_a_parks_before_getbuffer_and_blocks_producer_through_bounded_backpressure() {
    let rig = rig();
    rig.device.start_drainer();
    let worker = spawn_worker(
        Source::EofAfter(1_000_000),
        rig.edge.clone(),
        CHUNK,
        CHANNELS,
        rig.log.clone(),
        rig.inflight.clone(),
    );
    let render = rig.spawn_render(Mechanism::GateOnly);

    // Establish steady playback, then pause and observe the ack.
    assert!(wait_for(
        || rig
            .log
            .matching(|e| matches!(e, Event::Pull(Some(_))))
            .len()
            >= 3,
        ENGAGE_TIMEOUT
    ));
    rig.log.push(Event::PauseRequested);
    rig.gate.request_pause();
    assert!(
        wait_engaged(&rig.gate),
        "render leg never acknowledged engagement"
    );
    assert_no_device_buffer_while_parked(&rig.log);

    // Decoder progression stops through bounded backpressure: the edge
    // fills to capacity and the producer stays in-flight with no
    // progress, because the parked consumer no longer drains.
    assert!(
        wait_for(
            || rig.inflight.load(Ordering::Acquire) && rig.edge.buffered_frames() == EDGE_CAPACITY,
            ENGAGE_TIMEOUT
        ),
        "producer never reached the full-edge blocked state"
    );
    let pulls_at_block = rig
        .log
        .matching(|e| matches!(e, Event::Pull(Some(_))))
        .len();
    std::thread::sleep(GRACE);
    assert!(
        rig.inflight.load(Ordering::Acquire),
        "producer escaped the full edge while parked"
    );
    assert_eq!(
        rig.log
            .matching(|e| matches!(e, Event::Pull(Some(_))))
            .len(),
        pulls_at_block,
        "consumer pulled frames while paused"
    );

    // Mechanism A physics: the already-submitted tail plays out
    // (padding drains to zero) while the edge contents are preserved.
    assert!(
        wait_for(|| rig.device.padding() == 0, ENGAGE_TIMEOUT),
        "already-submitted audio did not play out during pause"
    );
    let edge_at_pause = rig.edge.buffered_frames();
    std::thread::sleep(GRACE);
    assert_eq!(
        rig.edge.buffered_frames(),
        edge_at_pause,
        "edge contents changed while parked (consumed or discarded)"
    );

    // Resume: consumption resumes, the producer unblocks.
    rig.log.push(Event::ResumeRequested);
    rig.gate.request_resume();
    assert!(wait_disengaged(&rig.gate));
    assert!(
        wait_for(
            || rig
                .log
                .matching(|e| matches!(e, Event::Pull(Some(_))))
                .len()
                > pulls_at_block,
            ENGAGE_TIMEOUT
        ),
        "no consumer progress after resume"
    );

    rig.session_stop();
    let outcome = join_within(render, JOIN_TIMEOUT).expect("render leg did not exit after stop");
    assert_eq!(outcome, LoopOutcome::Aborted);
    assert_eq!(
        join_within(worker, JOIN_TIMEOUT),
        Some(WorkerExit::Stopped),
        "producer did not stop through the bounded backpressure wake"
    );
    rig.join_drainer();
    assert_no_device_buffer_across_park(&rig.log);
}

#[test]
fn gate_a_stop_from_paused_mid_play_wakes_every_blocked_participant() {
    let rig = rig();
    rig.device.start_drainer();
    let worker = spawn_worker(
        Source::EofAfter(1_000_000),
        rig.edge.clone(),
        CHUNK,
        CHANNELS,
        rig.log.clone(),
        rig.inflight.clone(),
    );
    let render = rig.spawn_render(Mechanism::GateOnly);

    assert!(wait_for(
        || rig
            .log
            .matching(|e| matches!(e, Event::Pull(Some(_))))
            .len()
            >= 2,
        ENGAGE_TIMEOUT
    ));
    rig.gate.request_pause();
    assert!(wait_engaged(&rig.gate));

    // Stop from paused: every blocked participant must wake and exit
    // within a bounded latency. The gate wake is unpark-and-continue:
    // the loop proceeds once more, the data-plane Stopped terminal
    // decides, and the leg aborts — the same history shape as today's
    // mid-play stop (worker Stopped × drain Aborted × intent → Stopped).
    let stop_at = Instant::now();
    rig.session_stop();

    let outcome =
        join_within(render, JOIN_TIMEOUT).expect("parked render leg did not wake on stop");
    assert_eq!(outcome, LoopOutcome::Aborted);
    assert_eq!(join_within(worker, JOIN_TIMEOUT), Some(WorkerExit::Stopped));
    assert!(
        stop_at.elapsed() < JOIN_TIMEOUT,
        "stop-from-paused wake exceeded the bounded latency"
    );
    rig.join_drainer();
    assert_no_device_buffer_across_park(&rig.log);
}

#[test]
fn gate_a_eof_while_paused_stays_unsettled_until_resume_then_drains() {
    let rig = rig();
    rig.device.start_drainer();
    let worker = spawn_worker(
        Source::EofAfter(6000),
        rig.edge.clone(),
        CHUNK,
        CHANNELS,
        rig.log.clone(),
        rig.inflight.clone(),
    );
    let render = rig.spawn_render(Mechanism::GateOnly);

    // Pause before the source is exhausted (small total: the producer
    // can still reach EOF into the edge while the consumer is parked).
    rig.gate.request_pause();
    assert!(wait_engaged(&rig.gate));
    assert_no_device_buffer_while_parked(&rig.log);

    // The producer reaches EOF while the leg is parked; no drain
    // happens (nothing pulls) — at the session level this history stays
    // unsettled (no drain verdict) until the leg moves again.
    assert_eq!(join_within(worker, JOIN_TIMEOUT), Some(WorkerExit::Eof));
    let pulls_at_pause = rig.log.matching(|e| matches!(e, Event::Pull(_))).len();
    std::thread::sleep(GRACE);
    assert_eq!(
        rig.log.matching(|e| matches!(e, Event::Pull(_))).len(),
        pulls_at_pause,
        "parked leg kept pulling after EOF"
    );

    // Resume: the leg consumes the buffered tail, sees EOF, and drains.
    rig.gate.request_resume();
    assert!(wait_disengaged(&rig.gate));
    let outcome = join_within(render, JOIN_TIMEOUT).expect("render leg did not drain after resume");
    assert_eq!(outcome, LoopOutcome::Drained);
    rig.join_drainer();
    assert_no_device_buffer_across_park(&rig.log);
}

#[test]
fn gate_a_stop_after_eof_while_paused_continues_to_drained_not_aborted() {
    let rig = rig();
    rig.device.start_drainer();
    let worker = spawn_worker(
        Source::EofAfter(6000),
        rig.edge.clone(),
        CHUNK,
        CHANNELS,
        rig.log.clone(),
        rig.inflight.clone(),
    );
    let render = rig.spawn_render(Mechanism::GateOnly);

    rig.gate.request_pause();
    assert!(wait_engaged(&rig.gate));
    assert_eq!(join_within(worker, JOIN_TIMEOUT), Some(WorkerExit::Eof));

    // Stop while parked after EOF. Unpark-and-continue: the leg
    // finishes its iteration, plays/drains the tail, and lands Drained
    // (worker Eof × drain Drained → the frozen table's Completed
    // history, consistent with today's stop-after-EOF). The gate must
    // NOT force an abort, which would fabricate a drain-Aborted ×
    // worker-Eof device-failure history.
    rig.session_stop();
    let outcome = join_within(render, JOIN_TIMEOUT).expect("parked leg did not wake on stop");
    assert_eq!(
        outcome,
        LoopOutcome::Drained,
        "stop-from-paused-after-EOF must let the data plane finish, not abort"
    );
    rig.join_drainer();
}

#[test]
fn gate_a_failure_while_paused_and_teardown_still_joins_the_parked_leg() {
    let rig = rig();
    rig.device.start_drainer();
    let worker = spawn_worker(
        Source::FailAfter(3000),
        rig.edge.clone(),
        CHUNK,
        CHANNELS,
        rig.log.clone(),
        rig.inflight.clone(),
    );
    let render = rig.spawn_render(Mechanism::GateOnly);

    rig.gate.request_pause();
    assert!(wait_engaged(&rig.gate));

    // The producer fails while the leg is parked (at the session level
    // the failure evidence settles immediately by precedence). The
    // parked leg must still be joinable through the gate's stop wake.
    assert_eq!(join_within(worker, JOIN_TIMEOUT), Some(WorkerExit::Failed));
    assert_eq!(
        rig.edge.terminal(),
        f3_pause_mechanism::edge::EdgeTerminal::Failed
    );

    rig.session_stop();
    let outcome =
        join_within(render, JOIN_TIMEOUT).expect("parked leg did not wake after failure teardown");
    assert_eq!(outcome, LoopOutcome::Aborted);
    rig.join_drainer();
}

#[test]
fn repeated_pause_resume_cycles_are_live_and_consumption_is_monotone() {
    let rig = rig();
    rig.device.start_drainer();
    let _worker = spawn_worker(
        Source::EofAfter(1_000_000),
        rig.edge.clone(),
        CHUNK,
        CHANNELS,
        rig.log.clone(),
        rig.inflight.clone(),
    );
    let render = rig.spawn_render(Mechanism::GateOnly);

    let mut last_pulls = 0usize;
    for cycle in 0..32 {
        rig.gate.request_pause();
        assert!(wait_engaged(&rig.gate), "cycle {cycle}: no engagement");
        assert_no_device_buffer_while_parked(&rig.log);
        rig.gate.request_resume();
        assert!(
            wait_disengaged(&rig.gate),
            "cycle {cycle}: no disengagement"
        );
        assert!(
            wait_for(
                || rig
                    .log
                    .matching(|e| matches!(e, Event::Pull(Some(_))))
                    .len()
                    > last_pulls,
                ENGAGE_TIMEOUT
            ),
            "cycle {cycle}: no consumer progress after resume"
        );
        last_pulls = rig
            .log
            .matching(|e| matches!(e, Event::Pull(Some(_))))
            .len();
    }

    rig.session_stop();
    assert_eq!(
        join_within(render, JOIN_TIMEOUT),
        Some(LoopOutcome::Aborted)
    );
    rig.join_drainer();
    assert_no_device_buffer_across_park(&rig.log);
}

#[test]
fn pause_commanded_before_first_iteration_engages_without_any_getbuffer() {
    let rig = rig();
    rig.device.start_drainer();
    let _worker = spawn_worker(
        Source::EofAfter(1_000_000),
        rig.edge.clone(),
        CHUNK,
        CHANNELS,
        rig.log.clone(),
        rig.inflight.clone(),
    );

    // Pause is commanded BEFORE the render leg exists: the very first
    // gate check parks before any device interaction.
    rig.gate.request_pause();
    let render = rig.spawn_render(Mechanism::GateOnly);
    assert!(wait_engaged(&rig.gate));
    assert!(
        rig.log
            .matching(|e| matches!(e, Event::GetBufferEnter))
            .is_empty(),
        "GetBuffer ran before the first parked gate check"
    );
    assert_no_device_buffer_while_parked(&rig.log);

    rig.gate.request_resume();
    assert!(wait_disengaged(&rig.gate));
    assert!(wait_for(
        || !rig
            .log
            .matching(|e| matches!(e, Event::Pull(Some(_))))
            .is_empty(),
        ENGAGE_TIMEOUT
    ));

    rig.session_stop();
    assert_eq!(
        join_within(render, JOIN_TIMEOUT),
        Some(LoopOutcome::Aborted)
    );
    rig.join_drainer();
}

#[test]
fn mechanism_b_freezes_device_consumption_across_the_park() {
    let rig = rig();
    rig.device.start_drainer();
    let worker = spawn_worker(
        Source::EofAfter(1_000_000),
        rig.edge.clone(),
        CHUNK,
        CHANNELS,
        rig.log.clone(),
        rig.inflight.clone(),
    );
    let render = rig.spawn_render(Mechanism::GatePlusDeviceStop);

    assert!(wait_for(
        || rig
            .log
            .matching(|e| matches!(e, Event::Pull(Some(_))))
            .len()
            >= 3,
        ENGAGE_TIMEOUT
    ));
    rig.gate.request_pause();
    assert!(wait_engaged(&rig.gate));
    assert!(wait_for(
        || rig.log.matching(|e| matches!(e, Event::DeviceStop)).len() == 1,
        ENGAGE_TIMEOUT
    ));

    // Mechanism B physics: device consumption is frozen — padding does
    // not drain while parked, and no consumption events occur between
    // DeviceStop and DeviceStart.
    let padding_at_pause = rig.device.padding();
    std::thread::sleep(GRACE);
    let padding_now = rig.device.padding();
    assert_eq!(
        padding_now, padding_at_pause,
        "device kept consuming submitted audio across a mechanism-B pause"
    );
    assert!(
        rig.device.padding() > 0,
        "expected submitted audio to still be frozen"
    );
    // Corrected D14.7 (F3-GATE-CORRECTIVE-1): mechanism B freezes the
    // submitted tail, so tail quiescence can never be observed while
    // parked and the Paused projection must stay false for the whole
    // park — engagement alone never establishes Paused. This is the
    // establishment-side reason B is not the selected mechanism.
    assert!(!paused_projection(EstablishmentInputs {
        settled: false,
        pause_intent: rig.gate.pause_requested(),
        engaged: rig.gate.observe().engaged,
        tail_quiesced: false,
    }));
    {
        let entries = rig.log.all();
        let mut in_freeze = false;
        let mut consumed_in_freeze = 0usize;
        for e in &entries {
            match e.event {
                Event::DeviceStop => in_freeze = true,
                Event::DeviceStart => in_freeze = false,
                Event::DeviceConsumed(_) if in_freeze => consumed_in_freeze += 1,
                _ => {}
            }
        }
        assert_eq!(
            consumed_in_freeze, 0,
            "device consumed submitted audio while mechanism-B-frozen"
        );
    }
    assert_no_device_buffer_while_parked(&rig.log);

    rig.gate.request_resume();
    assert!(wait_disengaged(&rig.gate));
    assert!(wait_for(
        || rig.log.matching(|e| matches!(e, Event::DeviceStart)).len() == 1,
        ENGAGE_TIMEOUT
    ));

    rig.session_stop();
    assert_eq!(
        join_within(render, JOIN_TIMEOUT),
        Some(LoopOutcome::Aborted)
    );
    assert_eq!(join_within(worker, JOIN_TIMEOUT), Some(WorkerExit::Stopped));
    rig.join_drainer();
    assert_no_device_buffer_across_park(&rig.log);
}

#[test]
fn negative_control_buffer_held_across_pause_is_caught_by_the_oracle() {
    let rig = rig();
    rig.device.start_drainer();
    let running = Arc::new(AtomicBool::new(true));
    let device: Arc<dyn DeviceOps> = rig.device.clone();
    let broken = {
        let edge = rig.edge.clone();
        let gate = rig.gate.clone();
        let log = rig.log.clone();
        let running = running.clone();
        std::thread::Builder::new()
            .name("sim-render-broken".into())
            .spawn(move || {
                run_buffer_held_across_pause_loop(edge, device, gate, CHANNELS, log, running)
            })
            .expect("render spawn")
    };
    let _worker = spawn_worker(
        Source::EofAfter(1_000_000),
        rig.edge.clone(),
        CHUNK,
        CHANNELS,
        rig.log.clone(),
        rig.inflight.clone(),
    );

    assert!(wait_for(
        || rig
            .log
            .matching(|e| matches!(e, Event::Pull(Some(_))))
            .len()
            >= 2,
        ENGAGE_TIMEOUT
    ));
    rig.gate.request_pause();
    assert!(wait_engaged(&rig.gate));
    std::thread::sleep(GRACE);

    // The oracle MUST catch the broken shape: device buffer activity
    // (GetBuffer without a paired release) while parked. Prove it by
    // running the same check and asserting it fails.
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_no_device_buffer_while_parked(&rig.log);
    }))
    .is_err();
    assert!(
        caught,
        "the no-buffer-across-park oracle accepted the broken loop (vacuous oracle)"
    );

    running.store(false, Ordering::Release);
    rig.gate.release_stop();
    let _ = join_within(broken, JOIN_TIMEOUT);
    rig.join_drainer();
}

/// CORRECTIVE-1 scenario: the Paused projection must wait for
/// output-tail quiescence, not merely for render engagement.
///
/// Physical background (D14.7 corrective): mechanism A's own probe
/// proves that after the engage ack the already-submitted device audio
/// keeps playing until the WASAPI shared-mode padding drains
/// (padding 576 → 0, ~12 ms at the observed endpoint). Engagement is
/// therefore NOT an audible pause. This scenario deterministically
/// holds the drain timeline so the exact reviewed state —
/// terminal=none ∧ pause intent ∧ engaged ∧ output padding > 0 — is
/// sampled, and proves the corrected projection does not claim Paused
/// there; it becomes true exactly when the post-engagement padding==0
/// observation (tail-quiescence evidence) arrives.
#[test]
fn paused_projection_establishes_only_after_output_tail_quiescence() {
    let rig = rig();
    rig.device.start_drainer();
    let worker = spawn_worker(
        Source::EofAfter(1_000_000),
        rig.edge.clone(),
        CHUNK,
        CHANNELS,
        rig.log.clone(),
        rig.inflight.clone(),
    );
    let render = rig.spawn_render(Mechanism::GateOnly);

    assert!(wait_for(
        || rig
            .log
            .matching(|e| matches!(e, Event::Pull(Some(_))))
            .len()
            >= 3,
        ENGAGE_TIMEOUT
    ));

    // Freeze the drain timeline and fill the device buffer, so the
    // pending-tail state is deterministic (the physical probe measures
    // the same state in real time). The loop keeps submitting until the
    // buffer is full, then parks at the gate on the pause command.
    rig.device.hold_drain(true);
    assert!(
        wait_for(
            || rig.device.padding() >= DEVICE_FRAMES - PERIOD,
            ENGAGE_TIMEOUT
        ),
        "device buffer never filled for the deterministic pending-tail state"
    );
    rig.log.push(Event::PauseRequested);
    rig.gate.request_pause();
    assert!(wait_engaged(&rig.gate));
    assert_no_device_buffer_while_parked(&rig.log);
    assert!(
        rig.device.padding() > 0,
        "drain was held but the device tail already drained"
    );

    // The reviewed state: engaged, intent recorded, unsettled, and the
    // submitted tail still pending. Paused MUST NOT yet be claimed.
    let pending = EstablishmentInputs {
        settled: false,
        pause_intent: rig.gate.pause_requested(),
        engaged: rig.gate.observe().engaged,
        tail_quiesced: false,
    };
    assert!(
        !paused_projection(pending),
        "Paused claimed while the submitted tail is still pending (engagement is not an audible pause)"
    );

    // Release the drain: the tail plays out. The first post-engagement
    // padding==0 observation is the tail-quiescence evidence (no
    // submission can have happened in between: the park-safety oracle
    // above pins zero GetBuffer/Release while parked).
    rig.device.hold_drain(false);
    assert!(
        wait_for(|| rig.device.padding() == 0, ENGAGE_TIMEOUT),
        "submitted tail never drained to the quiescence observation"
    );
    assert_no_device_buffer_while_parked(&rig.log);
    let quiesced = EstablishmentInputs {
        tail_quiesced: true,
        ..pending
    };
    assert!(
        paused_projection(quiesced),
        "Paused never became true even after the output tail quiesced"
    );

    // Resume: the projection symmetric flips to Resumed (pause control
    // no longer established; render submission re-enabled — NOT a claim
    // of already-audible audio).
    rig.log.push(Event::ResumeRequested);
    rig.gate.request_resume();
    assert!(wait_disengaged(&rig.gate));
    let resumed = EstablishmentInputs {
        pause_intent: false,
        engaged: false,
        ..quiesced
    };
    assert!(
        resumed_projection(resumed) && !paused_projection(resumed),
        "Resumed projection semantics diverged from the disengagement state"
    );
    assert!(
        wait_for(
            || rig
                .log
                .matching(|e| matches!(e, Event::Pull(Some(_))))
                .len()
                > 3,
            ENGAGE_TIMEOUT
        ),
        "no consumer progress after resume"
    );

    rig.session_stop();
    assert_eq!(
        join_within(render, JOIN_TIMEOUT),
        Some(LoopOutcome::Aborted)
    );
    assert_eq!(join_within(worker, JOIN_TIMEOUT), Some(WorkerExit::Stopped));
    rig.join_drainer();
    assert_no_device_buffer_across_park(&rig.log);
}

/// CORRECTIVE-1 negative control: the exact bug human review found.
/// The oracle is `!paused_projection(state)` evaluated at
/// terminal=none ∧ pause intent ∧ engaged ∧ output padding > 0. The
/// control proves the oracle is non-vacuous by running the
/// PRE-CORRECTIVE definition (`Paused := unsettled ∧ intent ∧
/// engagement`) through the same oracle — it must fail RED — and by
/// pinning the corrected truth table over all input combinations,
/// differing from the mutant exactly where tail quiescence matters.
#[test]
fn negative_control_engagement_without_tail_quiescence_never_establishes_paused() {
    // 1. The reviewed state through the corrected oracle: GREEN.
    let reviewed = EstablishmentInputs {
        settled: false,
        pause_intent: true,
        engaged: true,
        tail_quiesced: false,
    };
    assert!(!paused_projection(reviewed));

    // 2. The mutation (pre-corrective engagement-only conjunction)
    //    through the same oracle: must fail RED, i.e. the oracle
    //    demonstrably catches the exact reviewed defect.
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!(
            !paused_projection_mutated_engagement_only(reviewed),
            "pre-corrective definition claimed Paused in the reviewed state (expected for the mutation)"
        );
    }))
    .is_err();
    assert!(
        caught,
        "the engagement-only mutation was NOT caught by the establishment oracle (vacuous oracle)"
    );

    // 3. Full truth table: the corrected projection is the frozen
    //    conjunction, and the mutant disagrees exactly when the tail
    //    evidence is missing (and never anywhere else).
    for settled in [false, true] {
        for intent in [false, true] {
            for engaged in [false, true] {
                for tail in [false, true] {
                    let i = EstablishmentInputs {
                        settled,
                        pause_intent: intent,
                        engaged,
                        tail_quiesced: tail,
                    };
                    let expected = !settled && intent && engaged && tail;
                    assert_eq!(
                        paused_projection(i),
                        expected,
                        "corrected Paused truth table diverged at {i:?}"
                    );
                    let mutant = !settled && intent && engaged;
                    assert_eq!(
                        paused_projection_mutated_engagement_only(i),
                        mutant,
                        "mutation subject diverged from the pre-corrective definition at {i:?}"
                    );
                    if mutant && !expected {
                        assert!(
                            !tail,
                            "mutant/corrected disagreement outside the pending-tail state"
                        );
                    }
                }
            }
        }
    }
}
