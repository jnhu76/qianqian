//! F5-GATE Experiment E2 — edge-cut protocol evidence (all platforms).
//!
//! The production `PcmEdge` is crate-private to `qianqian-playback`, so
//! this probe carries a minimal faithful copy of its synchronization
//! shape (Mutex ring + two condvars, first-wins monotone terminal) with
//! frame structs instead of raw f32 samples — the concurrency protocol
//! under test is identical, and structured frames make the stale-output
//! oracle exact. On top of it sits the candidate non-terminal
//! invalidate primitive and the proposed seek-discontinuity protocol:
//!
//! ```text
//! session (driver)
//!     parks the render leg at its loop-top gate (no buffer held)
//!     edge.invalidate()            -- phase 1: empties the queue and
//!                                     unblocks a producer blocked on a
//!                                     full edge; any in-flight block
//!                                     lands in the emptied ring and
//!                                     dies in phase 2
//!     waits for the worker's landing evidence
//!     commits the cutover; releases the render leg
//! worker (producer)
//!     loop-top serialization point:
//!         take seek command; discard staging (the freshly decoded,
//!             not-yet-written pre-cut block dies here);
//!         edge.invalidate()        -- phase 2: the load-bearing cut.
//!                                     After this returns, this thread
//!                                     writes only post-reposition PCM
//!         publish landing evidence
//!     produce post-cut frames
//! render leg (consumer)
//!     loop-top park (internal seek quiescence), then pull
//! ```
//!
//! Oracles:
//!
//! ```text
//! STALE-OUTPUT   a frame tagged pre-cut may never be handed out to a
//!                consumer that observed the commit barrier before its
//!                read began. Frames consumed BEFORE the commit are
//!                legal old output (the frozen invariant permits them).
//! NO-SURVIVOR    after the worker's phase-2 invalidate, the edge
//!                reports zero buffered frames at the commit.
//! NO-DEADLOCK    every run — including a producer blocked writing on a
//!                full edge when the cut begins — terminates.
//! ```
//!
//! Negative control: a producer that reaches its serialization point
//! but SKIPS the staging discard (writes its freshly decoded pre-cut
//! block after the phase-2 invalidate) MUST trip STALE-OUTPUT. The
//! invalidate primitive alone is not safe; the serialization-point
//! discipline — staging discard on the worker's own path — is what
//! excludes stale PCM.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

// --- minimal edge copy (qianqian-playback edge.rs shape) ----------------

#[derive(Clone, Copy, PartialEq, Debug)]
struct Frame {
    /// Production epoch: 0 = pre-cut, 1 = post-cut.
    epoch: u64,
    /// Source position on the synthetic timeline (may jump backward at
    /// the cut — backward seeks are legal).
    pos: u64,
}

struct EdgeState {
    ring: Box<[Frame]>,
    read_pos: usize,
    write_pos: usize,
    buffered: usize,
    terminal: u8,
}

const TERMINAL_OPEN: u8 = 0;
const TERMINAL_EOF: u8 = 1;
const TERMINAL_STOPPED: u8 = 3;

struct Edge {
    capacity: usize,
    state: Mutex<EdgeState>,
    space_freed: Condvar,
    data_ready: Condvar,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pull {
    Frames(usize),
    Eof,
    Stopped,
}

impl Edge {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            state: Mutex::new(EdgeState {
                ring: vec![Frame { epoch: 0, pos: 0 }; capacity].into_boxed_slice(),
                read_pos: 0,
                write_pos: 0,
                buffered: 0,
                terminal: TERMINAL_OPEN,
            }),
            space_freed: Condvar::new(),
            data_ready: Condvar::new(),
        }
    }

    /// Producer side: block until the whole slice is accepted or a
    /// terminal stops the write (production `write` shape).
    fn write(&self, src: &[Frame]) -> bool {
        let mut offset = 0usize;
        let mut guard = self.state.lock().expect("edge lock");
        loop {
            if guard.terminal != TERMINAL_OPEN {
                return false;
            }
            if offset == src.len() {
                return true;
            }
            let free = self.capacity - guard.buffered;
            if free == 0 {
                guard = self.space_freed.wait(guard).expect("edge lock");
                continue;
            }
            let take = free.min(src.len() - offset);
            let write_pos = guard.write_pos;
            for i in 0..take {
                guard.ring[(write_pos + i) % self.capacity] = src[offset + i];
            }
            guard.write_pos = (guard.write_pos + take) % self.capacity;
            guard.buffered += take;
            offset += take;
            drop(guard);
            self.data_ready.notify_all();
            guard = self.state.lock().expect("edge lock");
        }
    }

    /// Consumer side (production `read_frames` shape).
    fn read(&self, dst: &mut [Frame]) -> Pull {
        let mut guard = self.state.lock().expect("edge lock");
        loop {
            if guard.terminal == TERMINAL_STOPPED {
                return Pull::Stopped;
            }
            if guard.buffered > 0 {
                let take = dst.len().min(guard.buffered);
                let read_pos = guard.read_pos;
                for i in 0..take {
                    dst[i] = guard.ring[(read_pos + i) % self.capacity];
                }
                guard.read_pos = (guard.read_pos + take) % self.capacity;
                guard.buffered -= take;
                drop(guard);
                self.space_freed.notify_all();
                return Pull::Frames(take);
            }
            if guard.terminal == TERMINAL_EOF {
                return Pull::Eof;
            }
            guard = self.data_ready.wait(guard).expect("edge lock");
        }
    }

    /// The candidate F5 primitive: drop every buffered frame WITHOUT
    /// touching the terminal (flush ≠ terminal; the edge stays Open,
    /// first-wins terminal semantics untouched). Correctness contract:
    /// the caller must have proven no endpoint can still deliver stale
    /// data across the reset — in the protocol that is the session at
    /// phase 1 (safe because the consumer is parked out of read) and
    /// the worker itself at its serialization point (phase 2, safe
    /// because the caller is the only writer, on its own path).
    fn invalidate(&self) {
        {
            let mut guard = self.state.lock().expect("edge lock");
            guard.read_pos = 0;
            guard.write_pos = 0;
            guard.buffered = 0;
        }
        self.space_freed.notify_all();
        self.data_ready.notify_all();
    }

    fn close_eof(&self) {
        let mut guard = self.state.lock().expect("edge lock");
        if guard.terminal == TERMINAL_OPEN {
            guard.terminal = TERMINAL_EOF;
        }
        drop(guard);
        self.space_freed.notify_all();
        self.data_ready.notify_all();
    }

    fn stop(&self) {
        let mut guard = self.state.lock().expect("edge lock");
        if guard.terminal == TERMINAL_OPEN {
            guard.terminal = TERMINAL_STOPPED;
        }
        drop(guard);
        self.space_freed.notify_all();
        self.data_ready.notify_all();
    }

    fn buffered(&self) -> usize {
        self.state.lock().expect("edge lock").buffered
    }
}

// --- protocol harness ----------------------------------------------------

const BLOCK_FRAMES: usize = 64;
const EDGE_FRAMES: usize = 512;
const RUN_CAP: Duration = Duration::from_secs(10);

struct Shared {
    edge: Edge,
    /// Worker command slot, taken at the worker's loop-top serialization
    /// point. `Some(encoded)` while a seek is pending pickup; the
    /// landing is stored `landing + 1` so a legitimate landing of 0
    /// stays distinguishable from "not yet published".
    command: Mutex<Option<u64>>,
    wake_worker: Condvar,
    /// Render-leg park flag (internal seek quiescence). Truth-class
    /// separation from any pause state is structural here: nothing in
    /// this harness shares state with a pause concept.
    hold: Mutex<bool>,
    hold_cv: Condvar,
    /// Cutover commit barrier (session-owned protocol state).
    committed: AtomicBool,
    /// Worker's landing evidence (landing + 1 encoding), published
    /// strictly after its phase-2 invalidate.
    landing: AtomicU64,
    worker_done: Mutex<bool>,
    worker_cv: Condvar,
    /// Consumer output record: (committed-at-read-start, epoch, pos).
    output: Mutex<Vec<(bool, u64, u64)>>,
    stop: AtomicBool,
    /// Producer's pre-cut production progress (frames written), for the
    /// driver's progress-based scenario establishment.
    produced_pre: AtomicU64,
}

#[derive(Clone)]
struct Scenario {
    label: &'static str,
    seed: u64,
    pre_frames: usize,
    landing: u64,
    post_frames: usize,
    /// Park the consumer first, then let the producer fill the edge to
    /// capacity and block inside write() before the cut begins (T4/T5
    /// shape).
    blocked_producer: bool,
    /// Negative control: the worker takes the command and performs the
    /// phase-2 invalidate, but SKIPS the staging discard — its freshly
    /// decoded pre-cut block is written after the cut.
    rogue_staging: bool,
}

impl Default for Scenario {
    fn default() -> Self {
        Self {
            label: "run",
            seed: 1,
            pre_frames: 600,
            landing: 100,
            post_frames: 600,
            blocked_producer: false,
            rogue_staging: false,
        }
    }
}

#[derive(Debug)]
enum Outcome {
    /// No stale frame after the commit barrier.
    Clean {
        consumed_post_commit: usize,
        buffered_at_commit_observation: usize,
    },
    /// At least one stale frame after the commit barrier (witnesses).
    Stale { witnesses: Vec<(u64, u64)> },
}

fn park_at_gate(shared: &Shared) {
    let mut parked = shared.hold.lock().expect("hold lock");
    while *parked && !shared.stop.load(Ordering::Acquire) {
        let (p, _) = shared
            .hold_cv
            .wait_timeout(parked, Duration::from_millis(1))
            .expect("hold wait");
        parked = p;
    }
}

fn xorshift(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn run_scenario(sc: &Scenario) -> Result<Outcome, String> {
    let shared = Arc::new(Shared {
        edge: Edge::new(EDGE_FRAMES),
        command: Mutex::new(None),
        wake_worker: Condvar::new(),
        hold: Mutex::new(false),
        hold_cv: Condvar::new(),
        committed: AtomicBool::new(false),
        landing: AtomicU64::new(0),
        worker_done: Mutex::new(false),
        worker_cv: Condvar::new(),
        output: Mutex::new(Vec::new()),
        stop: AtomicBool::new(false),
        produced_pre: AtomicU64::new(0),
    });

    // Producer (decode-worker double).
    let producer = {
        let shared = Arc::clone(&shared);
        let sc = sc.clone();
        let mut rng = sc.seed | 1;
        std::thread::spawn(move || {
            let mut src_pos: u64 = 0;
            let mut epoch: u64 = 0;
            let mut produced_pre = 0usize;
            let mut produced_post = 0usize;
            // The most recent pre-cut block: the staging a rogue worker
            // fails to discard.
            let mut last_staging: Vec<Frame> = Vec::new();
            loop {
                // --- loop-top serialization point ---
                let picked = shared.command.lock().expect("cmd lock").take();
                if let Some(landing) = picked {
                    if !(sc.rogue_staging && !last_staging.is_empty()) {
                        // Honest: staging discard. The freshly decoded,
                        // not-yet-written pre-cut block dies here.
                        last_staging.clear();
                    } else if sc.rogue_staging {
                        // The captured old block was already written
                        // before the loop top; a rogue keeps it alive —
                        // it re-writes it right after the cut below.
                    }
                    shared.edge.invalidate();
                    shared.landing.store(landing + 1, Ordering::Release);
                    epoch = 1;
                    src_pos = landing;
                    if sc.rogue_staging && !last_staging.is_empty() {
                        // NEGATIVE CONTROL: the stale staging block is
                        // published into the freshly cut edge.
                        let _ = shared.edge.write(&last_staging);
                        last_staging.clear();
                    }
                }
                // EOF belongs to the current epoch's decode budget: a
                // cut that lands mid-production freezes the pre-cut
                // budget below its planned total, and the decoder's EOF
                // after the reposition is what ends production — the
                // same shape as a real seek near EOF.
                if epoch == 1 && produced_post >= sc.post_frames {
                    shared.edge.close_eof();
                    break;
                }
                if epoch == 0 && produced_pre >= sc.pre_frames {
                    // Honest worker waiting for the command at its loop
                    // top. (The real worker would keep producing old PCM
                    // until the command arrives; holding here keeps the
                    // scenario deterministic without weakening what the
                    // oracles see — the edge still holds the pre-cut
                    // queue the cut must purge.)
                    let mut cmd = shared.command.lock().expect("cmd lock");
                    while cmd.is_none() && !shared.stop.load(Ordering::Acquire) {
                        let (c, _) = shared
                            .wake_worker
                            .wait_timeout(cmd, Duration::from_millis(1))
                            .expect("worker wait");
                        cmd = c;
                    }
                    drop(cmd);
                    continue;
                }
                let n = BLOCK_FRAMES.min(if epoch == 0 {
                    sc.pre_frames - produced_pre
                } else {
                    sc.post_frames - produced_post
                });
                let block: Vec<Frame> = (0..n)
                    .map(|i| Frame { epoch, pos: src_pos + i as u64 })
                    .collect();
                src_pos += n as u64;
                if epoch == 0 {
                    produced_pre += n;
                    shared.produced_pre.store(produced_pre as u64, Ordering::Release);
                    last_staging = block.clone();
                } else {
                    produced_post += n;
                }
                if !shared.edge.write(&block) {
                    break;
                }
                let _ = xorshift(&mut rng);
                std::thread::sleep(Duration::from_micros(xorshift(&mut rng) % 30));
            }
            let mut done = shared.worker_done.lock().expect("done lock");
            *done = true;
            shared.worker_cv.notify_all();
        })
    };

    // Consumer (render-leg double): loop-top park, then pull.
    let consumer = {
        let shared = Arc::clone(&shared);
        std::thread::spawn(move || {
            let mut dst = vec![Frame { epoch: 0, pos: 0 }; BLOCK_FRAMES];
            loop {
                park_at_gate(&shared);
                if shared.stop.load(Ordering::Acquire) {
                    break;
                }
                // Barrier snapshot BEFORE the read (conservative: a flip
                // mid-read counts the frame as post-commit).
                let committed = shared.committed.load(Ordering::Acquire);
                match shared.edge.read(&mut dst) {
                    Pull::Frames(n) => {
                        let mut out = shared.output.lock().expect("out lock");
                        out.extend(dst[..n].iter().map(|f| (committed, f.epoch, f.pos)));
                    }
                    Pull::Eof | Pull::Stopped => break,
                }
            }
        })
    };

    // Driver (session double). Fresh deadline per phase: a phase that
    // burns its budget must fail THAT phase, not silently poison every
    // later wait with an already-expired deadline.
    let phase_deadline = || Instant::now() + RUN_CAP;
    if sc.blocked_producer {
        // Park first, then let the producer fill the edge and block in
        // write() — the T4/T5 starting state.
        *shared.hold.lock().expect("hold lock") = true;
        let mut full_observations = 0u32;
        let dl = phase_deadline();
        while full_observations < 20 && Instant::now() < dl {
            if shared.edge.buffered() >= EDGE_FRAMES {
                full_observations += 1;
            } else {
                full_observations = 0;
            }
            std::thread::sleep(Duration::from_micros(200));
        }
        if full_observations < 20 {
            return Err("could not establish a blocked producer".into());
        }
    } else {
        // Let the pre-cut region flow to at least half the pre-cut
        // budget (progress-based, not occupancy-based: a fast consumer
        // keeps occupancy near zero while the flow is perfectly healthy),
        // then park the consumer and let the producer exhaust its
        // pre-cut budget — the edge then holds the stale queue tail (or
        // the producer is blocked writing it), the state the cut purges.
        let dl = phase_deadline();
        let half = sc.pre_frames / 2;
        while shared.produced_pre.load(Ordering::Acquire) < half as u64 {
            if Instant::now() > dl {
                return Err("pre-cut flow never reached half budget".into());
            }
            std::thread::sleep(Duration::from_micros(50));
        }
        *shared.hold.lock().expect("hold lock") = true;
        let dl = phase_deadline();
        while shared.produced_pre.load(Ordering::Acquire) < sc.pre_frames as u64 {
            if Instant::now() > dl {
                return Err("producer never exhausted its pre-cut budget".into());
            }
            std::thread::sleep(Duration::from_micros(50));
        }
    }
    // Plant the seek command…
    *shared.command.lock().expect("cmd lock") = Some(sc.landing);
    shared.wake_worker.notify_all();
    // …phase 1: session-side invalidate (also the blocked-writer wake).
    shared.edge.invalidate();
    // …wait for the landing evidence (strictly after phase 2)…
    let dl = phase_deadline();
    while shared.landing.load(Ordering::Acquire) == 0 {
        if Instant::now() > dl {
            return Err("worker never published the landing".into());
        }
        std::thread::sleep(Duration::from_micros(50));
    }
    // Diagnostic only (record, never a verdict): frames may legitimately
    // already be queued here — the worker starts post-cut production
    // right after publishing the landing, so a non-zero count says
    // nothing about staleness. The staleness verdict is the STALE-OUTPUT
    // oracle below, which sees every frame the consumer is handed.
    let buffered_at_commit_observation = shared.edge.buffered();
    // …commit, then release the consumer.
    shared.committed.store(true, Ordering::Release);
    *shared.hold.lock().expect("hold lock") = false;
    shared.hold_cv.notify_all();
    // …EOF ends every run (the rogue finishes its post-cut budget too).
    {
        let dl = phase_deadline();
        let mut done = shared.worker_done.lock().expect("done lock");
        while !*done {
            let (d, _) = shared
                .worker_cv
                .wait_timeout(done, Duration::from_millis(5))
                .expect("done wait");
            done = d;
            if Instant::now() > dl {
                return Err("worker never finished".into());
            }
        }
    }
    shared.edge.stop();
    *shared.hold.lock().expect("hold lock") = false;
    shared.hold_cv.notify_all();
    if let Err(e) = consumer.join() {
        return Err(format!("consumer panicked: {e:?}"));
    }
    if let Err(e) = producer.join() {
        return Err(format!("producer panicked: {e:?}"));
    }

    // STALE-OUTPUT oracle over the output record.
    let out = shared.output.lock().expect("out lock");
    let mut consumed_post_commit = 0usize;
    let mut witnesses = Vec::new();
    for &(committed_at_start, epoch, pos) in out.iter() {
        if committed_at_start {
            consumed_post_commit += 1;
            if epoch == 0 {
                witnesses.push((epoch, pos));
            }
        }
    }
    if witnesses.is_empty() {
        Ok(Outcome::Clean {
            consumed_post_commit,
            buffered_at_commit_observation,
        })
    } else {
        Ok(Outcome::Stale { witnesses })
    }
}

fn main() {
    println!("F5EDGE BEGIN");
    let mut failures = 0usize;

    let mut scenarios = vec![
        Scenario { label: "deterministic-honest", ..Default::default() },
        Scenario {
            label: "backward-seek-honest",
            seed: 7,
            landing: 50,
            ..Default::default()
        },
        Scenario {
            label: "landing-zero-honest",
            seed: 9,
            landing: 0,
            ..Default::default()
        },
        Scenario {
            label: "blocked-producer-honest",
            seed: 11,
            blocked_producer: true,
            ..Default::default()
        },
        Scenario {
            label: "negative-control-rogue-staging",
            seed: 13,
            rogue_staging: true,
            ..Default::default()
        },
    ];
    for i in 0..200u64 {
        let mut seed = i.wrapping_mul(2654435761).wrapping_add(12345);
        let jitter = xorshift(&mut seed);
        scenarios.push(Scenario {
            label: "random-honest",
            seed: jitter,
            pre_frames: 400 + (jitter % 800) as usize,
            landing: jitter % 300,
            post_frames: 400,
            ..Default::default()
        });
    }
    for i in 0..50u64 {
        let mut seed = i.wrapping_mul(40503).wrapping_add(99991);
        let jitter = xorshift(&mut seed);
        scenarios.push(Scenario {
            label: "random-rogue-staging",
            seed: jitter,
            rogue_staging: true,
            ..Default::default()
        });
    }

    let mut rogue_runs = 0usize;
    let mut rogue_fired = 0usize;
    for sc in &scenarios {
        let is_rogue = sc.rogue_staging;
        let result = run_scenario(sc);
        match result {
            Ok(Outcome::Clean {
                consumed_post_commit,
                buffered_at_commit_observation,
            }) => {
                if is_rogue {
                    println!(
                        "F5EDGE {} seed={} UNEXPECTED_CLEAN post_commit_frames={consumed_post_commit} \
                         (negative control did not fire)",
                        sc.label, sc.seed
                    );
                    failures += 1;
                }
                let _ = buffered_at_commit_observation;
            }
            Ok(Outcome::Stale { witnesses }) => {
                if is_rogue {
                    rogue_runs += 1;
                    rogue_fired += 1;
                } else {
                    println!(
                        "F5EDGE {} seed={} STALE_OUTPUT_VIOLATION witnesses={:?}",
                        sc.label,
                        sc.seed,
                        &witnesses[..witnesses.len().min(4)]
                    );
                    failures += 1;
                }
            }
            Err(e) => {
                println!("F5EDGE {} seed={} RUN_FAILURE {e}", sc.label, sc.seed);
                failures += 1;
            }
        }
    }
    println!(
        "F5EDGE SUMMARY runs={} protocol_failures={} rogue_runs={} rogue_fired={}",
        scenarios.len(),
        failures,
        rogue_runs,
        rogue_fired
    );
    println!("F5EDGE END failures={failures}");
    std::process::exit(i32::from(failures > 0 || rogue_fired != rogue_runs || rogue_runs == 0));
}
