//! F5-GATE Experiment E2 — edge-cut protocol evidence (all platforms).
//!
//! The production `PcmEdge` is crate-private to `qianqian-playback`, so
//! this probe carries a minimal faithful copy of its synchronization
//! shape (Mutex ring + two condvars, first-wins monotone terminal) with
//! frame structs instead of raw f32 samples — the concurrency protocol
//! under test is identical, and structured frames make the stale-output
//! oracle exact. On top of it sits the candidate non-terminal
//! invalidate primitive and the frozen seek-discontinuity protocol
//! (D14.5 amendment):
//!
//! ```text
//! session (driver)
//!     records the seek command and parks the render leg at its
//!         loop-top gate (parked = holds no buffer)
//!     commits the cutover only after the worker's landing evidence
//!         AND the leg's park acknowledgment, then releases the leg
//! worker (producer)
//!     KEEPS PRODUCING until the leg's parked evidence is visible:
//!         stopping earlier could strand the leg inside a blocked
//!         read on an emptied edge (stall). The write path is
//!         bounded-slice and re-observes the command slot every
//!         slice, so the worker always reaches its serialization
//!         point without any destructive pre-purge.
//!     loop-top serialization point:
//!         song_seek BEFORE anything is invalidated; a refusal ends
//!             the cut pre-cut and inert (worst cost: one dead
//!             staging block; playback continues unchanged)
//!         success: discard staging (the freshly decoded, not-yet-
//!             written pre-cut block dies here), then
//!         edge.invalidate() -- THE one purge, on the worker's own
//!             path. After this returns, this thread writes only
//!             post-reposition PCM
//!         publish landing evidence; hold production until the
//!             session releases (the hold begins only after the leg
//!             is parked, so no reader is stranded)
//! render leg (consumer)
//!     loop-top park (internal seek quiescence, cut-attributed),
//!     then pull
//! ```
//!
//! Oracles:
//!
//! ```text
//! STALE-OUTPUT   a frame tagged pre-cut may never be handed out to a
//!                consumer that observed the commit barrier before
//!                its read began. Frames consumed BEFORE the commit
//!                are legal old output (the frozen invariant permits
//!                them). The harness snapshots before reading, which
//!                is the lenient direction; the commit gate on the
//!                park acknowledgment closes the remaining gap.
//! NO-SURVIVOR    diagnostic only: after the worker's invalidate the
//!                edge reports zero buffered frames at the commit.
//! NO-DEADLOCK    every run — including a producer blocked writing on
//!                a full edge when the cut begins — terminates.
//! ```
//!
//! Negative control: a worker that takes the command but SKIPS the
//! staging discard and re-writes its stale block only AFTER the
//! commit MUST trip STALE-OUTPUT. The invalidate primitive alone is
//! not safe; the serialization-point discipline — staging discard on
//! the worker's own path, song_seek before invalidation, and the
//! park-acknowledged commit — is what excludes stale PCM.

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

    /// Non-blocking helper for the harness's bounded-slice write
    /// (the ADR's frozen representation example: the worker's write
    /// wait observes the command slot, so it can always reach its
    /// serialization point regardless of edge occupancy — no
    /// destructive pre-purge). Writes what fits; returns the count.
    fn write_some(&self, src: &[Frame]) -> usize {
        let mut guard = self.state.lock().expect("edge lock");
        if guard.terminal != TERMINAL_OPEN {
            return 0;
        }
        let free = self.capacity - guard.buffered;
        let take = free.min(src.len());
        if take == 0 {
            return 0;
        }
        let write_pos = guard.write_pos;
        for i in 0..take {
            guard.ring[(write_pos + i) % self.capacity] = src[i];
        }
        guard.write_pos = (guard.write_pos + take) % self.capacity;
        guard.buffered += take;
        drop(guard);
        self.data_ready.notify_all();
        take
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
    /// data across the reset — in the frozen protocol that is the
    /// worker itself, exactly once, at its serialization point after a
    /// successful song_seek: the render leg is parked out of read
    /// (the commit is gated on the park acknowledgment) and the worker
    /// is the only writer, on its own path.
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

// --- protocol harness (F5-GATE frozen protocol, D14.5 amendment) --------
//
// Order under test: session records the command and parks the render
// leg; the worker reaches its serialization point REGARDLESS of edge
// occupancy (bounded-slice write observing the command slot — no
// destructive pre-purge), calls song_seek BEFORE anything is
// invalidated, and only on success purges the edge itself, publishes
// landing and holds production until release. A refusal invalidates
// nothing: playback continues from the pre-command content.

const BLOCK_FRAMES: usize = 64;
const EDGE_FRAMES: usize = 512;
const RUN_CAP: Duration = Duration::from_secs(10);

struct Shared {
    edge: Edge,
    /// Worker command slot, taken at the worker's loop-top serialization
    /// point. `Some(landing)` while a seek is pending pickup.
    command: Mutex<Option<u64>>,
    wake_worker: Condvar,
    /// Render-leg park flag (internal seek quiescence; cut-attributed —
    /// structurally separate from any pause concept in this harness).
    hold: Mutex<bool>,
    hold_cv: Condvar,
    /// Cutover commit barrier (session-owned protocol state).
    committed: AtomicBool,
    /// Consumer's park acknowledgment: the leg reached its loop-top gate
    /// and is parked. The frozen protocol gates the commit on this
    /// evidence (the render leg must be out of read and hold no buffer
    /// at the commit); without it the commit races an in-flight read.
    parked_ack: AtomicBool,
    /// Worker's landing evidence (landing + 1 encoding), published
    /// strictly after its own purge.
    landing: AtomicU64,
    /// Seek-refused evidence (song_seek rejection): nothing was
    /// invalidated; playback continues from the pre-command content.
    seek_failed: AtomicBool,
    /// Worker-side note: the command has been picked up at the loop top
    /// and the seek awaits the render leg's parked evidence. Production
    /// CONTINUES while this is set (that is what lets the leg reach its
    /// gate promptly).
    seek_pending: AtomicBool,
    /// Release with basis (commit route): the worker holds production
    /// between landing and this flag.
    released: AtomicBool,
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
    /// Park the consumer first, then let the producer fill the edge and
    /// block inside its bounded-slice write before the command is
    /// planted (T4/T5 shape: the worker must still reach its
    /// serialization point without any pre-purge).
    blocked_producer: bool,
    /// song_seek refuses (SEEK_* error): the protocol must stay inert —
    /// no purge, no landing, no commit — and playback continues.
    seek_refused: bool,
    /// Negative control: the worker takes the command but SKIPS the
    /// staging discard — its in-flight pre-cut block is written after
    /// the purge.
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
            seek_refused: false,
            rogue_staging: false,
        }
    }
}

enum Outcome {
    /// No stale frame after the commit barrier; protocol invariants held.
    Clean {
        consumed_post_commit: usize,
        buffered_at_commit_observation: usize,
    },
    /// At least one stale frame after the commit barrier (witnesses).
    Stale { witnesses: Vec<(u64, u64)> },
    /// Refusal path: protocol stayed inert (no purge, no landing, no
    /// commit) and old production continued to EOF.
    Refused { frames_consumed: usize },
}

fn park_at_gate(shared: &Shared) {
    let mut parked = shared.hold.lock().expect("hold lock");
    if *parked {
        shared.parked_ack.store(true, Ordering::Release);
    }
    while *parked && !shared.stop.load(Ordering::Acquire) {
        let (p, _) = shared
            .hold_cv
            .wait_timeout(parked, Duration::from_millis(1))
            .expect("hold wait");
        parked = p;
    }
    shared.parked_ack.store(false, Ordering::Release);
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
        parked_ack: AtomicBool::new(false),
        landing: AtomicU64::new(0),
        seek_failed: AtomicBool::new(false),
        seek_pending: AtomicBool::new(false),
        released: AtomicBool::new(false),
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
            // The picked-up command: the seek runs once the leg's parked
            // evidence arrives.
            let mut pending_landing: Option<u64> = None;
            // The most recent pre-cut block: the staging a rogue worker
            // fails to discard.
            let mut last_staging: Vec<Frame> = Vec::new();
            loop {
                // --- loop-top serialization point ---
                if let Some(landing) = shared.command.lock().expect("cmd lock").take() {
                    // Note the pickup; the seek itself waits for the
                    // render leg's parked evidence. Production CONTINUES
                    // meanwhile (that is what lets the leg reach its
                    // loop-top gate promptly — stopping production here
                    // could strand a leg inside a blocked read on an
                    // empty edge and stall the protocol).
                    pending_landing = Some(landing);
                    shared.seek_pending.store(true, Ordering::Release);
                }
                let do_seek = pending_landing.is_some()
                    && shared.parked_ack.load(Ordering::Acquire);
                if do_seek {
                    shared.seek_pending.store(false, Ordering::Release);
                }
                if let Some(landing) = if do_seek { pending_landing.take() } else { None } {
                    // song_seek happens HERE, before anything is
                    // invalidated (frozen order), and strictly after the
                    // leg's parked evidence.
                    if sc.seek_refused {
                        // Refusal: no invalidation at all; production
                        // continues from the current cursor; the command
                        // is consumed. (The driver releases the leg.)
                        shared.seek_failed.store(true, Ordering::Release);
                    } else {
                        // Success: staging discard, then the worker's own
                        // purge — the load-bearing cut — then landing,
                        // then hold production until release.
                        if sc.rogue_staging && !last_staging.is_empty() {
                            // NEGATIVE CONTROL: the rogue does not
                            // discard; it re-writes the in-flight block
                            // after the purge (below).
                        } else {
                            last_staging.clear();
                        }
                        shared.edge.invalidate();
                        shared.landing.store(landing + 1, Ordering::Release);
                        if sc.rogue_staging && !last_staging.is_empty() {
                            // NEGATIVE CONTROL, part 2: hold the stale
                            // block until AFTER the commit, then write it.
                            // A stale block racing into the pre-commit
                            // window proves nothing — pre-commit output is
                            // legal (the consumer may legitimately have up
                            // to one in-flight read) — so the control must
                            // exercise the post-commit violation shape the
                            // oracle exists for.
                            while !shared.committed.load(Ordering::Acquire)
                                && !shared.stop.load(Ordering::Acquire)
                            {
                                std::thread::sleep(Duration::from_micros(50));
                            }
                            let _ = shared.edge.write(&last_staging);
                            last_staging.clear();
                        }
                        // Production hold: bounded wait, terminal-aware.
                        let mut hold = shared.released.load(Ordering::Acquire);
                        while !hold && !shared.stop.load(Ordering::Acquire) {
                            std::thread::sleep(Duration::from_micros(100));
                            hold = shared.released.load(Ordering::Acquire);
                        }
                        epoch = 1;
                        src_pos = landing;
                    }
                }
                // EOF belongs to the current epoch's decode budget: a cut
                // freezes the pre-cut budget below its planned total, and
                // the decoder's EOF after the reposition is what ends
                // production — the same shape as a real seek near EOF.
                if epoch == 1 && produced_post >= sc.post_frames {
                    shared.edge.close_eof();
                    break;
                }
                let seeking = pending_landing.is_some();
                if epoch == 0 && produced_pre >= sc.pre_frames && !seeking && !sc.seek_refused {
                    // Honest worker waiting for the command at its loop
                    // top (pre-cut budget exhausted, no command yet).
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
                if epoch == 0 && sc.seek_refused && produced_pre >= sc.pre_frames {
                    // A refused seek consumed the command: production
                    // simply continues (the decoder never moved).
                    shared.edge.close_eof();
                    break;
                }
                let n = if epoch == 0 {
                    // While a pickup awaits the parked evidence, the old
                    // budget is a floor: production keeps flowing (that is
                    // the point) and the purge later drops whatever
                    // accumulated.
                    if seeking {
                        BLOCK_FRAMES
                    } else {
                        BLOCK_FRAMES.min(sc.pre_frames - produced_pre)
                    }
                } else {
                    BLOCK_FRAMES.min(sc.post_frames - produced_post)
                };
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
                // Bounded-slice write observing the command slot (the
                // frozen representation example): the worker can always
                // reach its serialization point regardless of edge
                // occupancy, with no destructive pre-purge.
                let mut off = 0usize;
                let mut abandoned = false;
                loop {
                    if off == block.len() {
                        break;
                    }
                    if shared
                        .command
                        .lock()
                        .expect("cmd lock")
                        .is_some()
                        && shared.parked_ack.load(Ordering::Acquire)
                    {
                        // The seek can proceed now (leg parked): abandon
                        // the in-flight staging block — at most one
                        // staging buffer, the only content a seek can ever
                        // cost (and only when the decoder refuses; on
                        // success this block dies in the purge anyway).
                        abandoned = true;
                        break;
                    }
                    let wrote = shared.edge.write_some(&block[off..]);
                    off += wrote;
                    if wrote == 0 {
                        std::thread::sleep(Duration::from_micros(100));
                    }
                }
                let _ = abandoned;
                if shared.stop.load(Ordering::Acquire) {
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
                // Snapshot BEFORE the read. The read is attributed with
                // the barrier state as of its start, so a commit that
                // lands mid-read classifies that read as PRE-commit —
                // the lenient direction for the stale oracle. The gap is
                // closed by scenario construction: the commit only fires
                // while the consumer is parked (no read in flight at
                // commit), which is itself a frozen protocol invariant.
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

    // Driver (session double). Fresh deadline per phase.
    let phase_deadline = || Instant::now() + RUN_CAP;
    if sc.blocked_producer {
        // Park first, then let the producer fill the edge and block in
        // its bounded-slice write — the T4/T5 starting state.
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
        // Progress-based pre-flow (occupancy-based waits flake: a fast
        // consumer keeps the queue near zero while the flow is healthy).
        let dl = phase_deadline();
        let half = sc.pre_frames / 2;
        while shared.produced_pre.load(Ordering::Acquire) < half as u64 {
            if Instant::now() > dl {
                return Err("pre-cut flow never reached half budget".into());
            }
            std::thread::sleep(Duration::from_micros(50));
        }
    }
    // Plant the seek command…
    *shared.command.lock().expect("cmd lock") = Some(sc.landing);
    shared.wake_worker.notify_all();
    // …and park the render leg (cut-attributed internal quiescence).
    *shared.hold.lock().expect("hold lock") = true;

    // Wait for the landing evidence or the refusal.
    let dl = phase_deadline();
    loop {
        if shared.seek_failed.load(Ordering::Acquire) {
            break;
        }
        if shared.landing.load(Ordering::Acquire) != 0 {
            break;
        }
        if Instant::now() > dl {
            return Err("worker never published landing or refusal".into());
        }
        std::thread::sleep(Duration::from_micros(50));
    }

    if sc.seek_refused {
        // Refusal path: nothing may have been invalidated and nothing
        // may commit. Release the leg; playback continues from the
        // pre-command content to natural EOF.
        if shared.landing.load(Ordering::Acquire) != 0 {
            return Err("refused seek produced a landing".into());
        }
        *shared.hold.lock().expect("hold lock") = false;
        shared.hold_cv.notify_all();
        let dl = phase_deadline();
        {
            let mut done = shared.worker_done.lock().expect("done lock");
            while !*done {
                let (d, _) = shared
                    .worker_cv
                    .wait_timeout(done, Duration::from_millis(5))
                    .expect("done wait");
                done = d;
                if Instant::now() > dl {
                    return Err("worker never finished after refusal".into());
                }
            }
        }
        shared.edge.stop();
        let _ = consumer.join();
        let _ = producer.join();
        if shared.committed.load(Ordering::Acquire) {
            return Err("refused seek committed a cutover".into());
        }
        let out = shared.output.lock().expect("out lock");
        let post_commit = out.iter().filter(|&&(c, _, _)| c).count();
        if post_commit != 0 {
            return Err("refused seek produced post-commit output".into());
        }
        return Ok(Outcome::Refused { frames_consumed: out.len() });
    }

    // The frozen commit precondition includes the leg actually being
    // parked (engagement evidence) — wait for the acknowledgment, or the
    // commit races an in-flight read and the oracle misclassifies it.
    let dl = phase_deadline();
    while !shared.parked_ack.load(Ordering::Acquire) {
        if Instant::now() > dl {
            return Err("consumer never reached the park (no engagement evidence)".into());
        }
        std::thread::sleep(Duration::from_micros(50));
    }
    // Diagnostic only (record, never a verdict): frames may
    // legitimately already be queued — staleness is decided by the
    // STALE-OUTPUT oracle below.
    let buffered_at_commit_observation = shared.edge.buffered();
    // Commit, then release with the basis (worker resumes production).
    shared.committed.store(true, Ordering::Release);
    shared.released.store(true, Ordering::Release);
    *shared.hold.lock().expect("hold lock") = false;
    shared.hold_cv.notify_all();
    // EOF ends every run (the rogue finishes its post-cut budget too).
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
            label: "seek-refused-inert",
            seed: 17,
            seek_refused: true,
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
    let only_seed: Option<u64> = std::env::var("F5EDGE_SEED").ok().and_then(|v| v.parse().ok());
    for sc in &scenarios {
        if let Some(sd) = only_seed { if sc.seed != sd { continue; } }
        let is_rogue = sc.rogue_staging;
        match run_scenario(sc) {
            Ok(Outcome::Clean {
                consumed_post_commit,
                buffered_at_commit_observation,
            }) => {
                if is_rogue {
                    println!(
                        "F5EDGE {} seed={} UNEXPECTED_CLEAN post_commit_frames={consumed_post_commit} buffered_at_commit={buffered_at_commit_observation} (negative control did not fire)",
                        sc.label, sc.seed
                    );
                    failures += 1;
                }
                let _ = (consumed_post_commit, buffered_at_commit_observation);
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
            Ok(Outcome::Refused { frames_consumed }) => {
                // The refusal path is its own assertion set inside the
                // run (inert: no purge, no landing, no commit).
                let _ = frames_consumed;
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
