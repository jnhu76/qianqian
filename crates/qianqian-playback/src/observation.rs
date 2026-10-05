//! The bounded audio observation tap (#187 O0/O1 decision, issue
//! comment OBSERVATION_PLANE-O0): a crate-private, lossy, read-only
//! branch off the decode worker's post-DSP staging seam.
//!
//! Topology (frozen at O0):
//!
//! ```text
//! decode → processing.stage (staging block)
//!              ├─ observation tap: nonblocking bounded offer
//!              │       └─ off-path analyst worker → latest snapshot
//!              └─ PcmEdge (D14.5-governed admission, untouched)
//! ```
//!
//! Authority boundary: the tap may inspect PCM; it must never modify
//! it, and nothing observed here is playback truth. Observation data
//! is non-authoritative, ephemeral, lossy, bounded and safe to drop —
//! dropping is normal telemetry policy, never an error, and never a
//! playback failure (an analyst failure retires observation only).
//!
//! Mechanism: an overwrite-latest slot of ONE staging block. `offer`
//! always succeeds (overwrite has no full state, so the producer never
//! waits for observation capacity), copies the block in under a short
//! mutex, and notifies. The consumer takes and clears under the same
//! mutex and analyzes OUTSIDE it. Both buffers are preallocated — the
//! steady-state path performs no allocation. This is the same
//! realization class as the episode's other latest-wins slot (DSP
//! pending, PBK-002 D14.11 live control), on a thread that already
//! takes the edge/control locks per block.
//!
//! Lifetime is structural, with no generation/epoch machinery: the tap
//! is created with the episode's decode worker, and the analyst worker
//! is spawned at worker entry; the worker's single exit funnel closes
//! the tap and joins the analyst before the worker's own thread
//! returns. Episode replacement builds a fresh worker → a fresh tap;
//! old observation state dies with the old worker frame and its
//! analyst thread, so nothing can observe across episodes.
//!
//! Discontinuity (D14.5 Applied cut): `invalidate` is called exactly
//! once, at the worker's existing Applied arm — it drops pending
//! pre-cut material and arms a one-bit boundary flag that rides the
//! next post-cut block, so pre-cut signal-derived state cannot be
//! delivered as post-cut observation. `RefusedUnchanged` never reaches
//! the call site: observation continues without a fake reset. Pause
//! owns no truth here either way: the pause gate holds the render leg,
//! not the producer, so bounded prefetch may keep producing (and
//! offering) until ordinary edge backpressure suspends it — and the
//! observation plane records no "paused" state.

use std::sync::{Arc, Condvar, Mutex};

use qianqian_audio_api::ports::PcmFormat;

/// One tap slot: the pending block plus the pending boundary bit.
struct Slot {
    /// Close request from the owning decode worker's exit funnel.
    closed: bool,
    /// A whole block is pending delivery.
    pending: bool,
    /// The next delivered block follows an Applied cut; consumed by
    /// the take that delivers it.
    after_cut: bool,
    /// Fixed-capacity staging-block storage (frames × channels).
    block: Box<[f32]>,
    /// Valid frames in `block` (frames × channels samples).
    frames: usize,
}

impl Slot {
    fn empty(capacity_samples: usize) -> Self {
        Self {
            closed: false,
            pending: false,
            after_cut: false,
            block: vec![0.0; capacity_samples].into_boxed_slice(),
            frames: 0,
        }
    }
}

/// The probe's internal latest-observation record: what the off-path
/// analyst publishes (O1 mechanism evidence only — no playback truth,
/// no public API).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ObservationSnapshot {
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
    /// Whole blocks delivered to the analyst.
    pub(crate) delivered_blocks: u64,
    /// Frames delivered (a multiple of the block frame count while the
    /// producer offers whole staging blocks).
    pub(crate) delivered_frames: u64,
    /// Delivered blocks that followed an Applied cut.
    pub(crate) cuts: u64,
    /// The analyst has observed close and exited (teardown evidence).
    pub(crate) worker_closed: bool,
}

struct Shared {
    /// Episode channel count; converts the interleaved slice length to
    /// the frame count the consumer sees.
    channels: usize,
    slot: Mutex<Slot>,
    block_ready: Condvar,
    record: Mutex<ObservationSnapshot>,
}

/// The bounded observation seam. Producer side (the decode worker):
/// [`offer`](ObservationTap::offer) / [`invalidate`](ObservationTap::invalidate)
/// / [`close`](ObservationTap::close). Off-path consumer: one analyst
/// thread spawned by [`spawn_worker`](ObservationTap::spawn_worker).
/// Clones share the one seam.
#[derive(Clone)]
pub(crate) struct ObservationTap {
    shared: Arc<Shared>,
}

impl ObservationTap {
    /// A tap for one episode: capacity is exactly one staging block at
    /// the episode's format.
    pub(crate) fn new(format: PcmFormat, staging_frames: usize) -> Self {
        let channels = usize::from(format.channels);
        assert!(channels > 0, "a tap without channels cannot exist");
        assert!(staging_frames > 0, "an empty tap is not a tap");
        Self {
            shared: Arc::new(Shared {
                channels,
                slot: Mutex::new(Slot::empty(staging_frames * channels)),
                block_ready: Condvar::new(),
                record: Mutex::new(ObservationSnapshot {
                    sample_rate: format.sample_rate,
                    channels: format.channels,
                    delivered_blocks: 0,
                    delivered_frames: 0,
                    cuts: 0,
                    worker_closed: false,
                }),
            }),
        }
    }

    /// Producer side, nonblocking: overwrite the slot with this block
    /// and notify the analyst. Always succeeds immediately — overwrite
    /// has no full state, so playback never waits for observation
    /// capacity. A slow analyst loses intermediate blocks by policy
    /// (latest wins). Dropping observation data is normal.
    ///
    /// `block` is a whole processed staging block (frames × channels);
    /// the caller's staging buffer is NOT retained or aliased.
    pub(crate) fn offer(&self, block: &[f32]) {
        let mut slot = self.lock_slot();
        let capacity = slot.block.len();
        assert!(
            block.len() <= capacity,
            "observation offer exceeds the staging-block capacity"
        );
        slot.block[..block.len()].copy_from_slice(block);
        slot.frames = block.len() / self.shared.channels;
        slot.pending = true;
        drop(slot);
        self.shared.block_ready.notify_all();
    }

    /// The D14.5 Applied-cut obligation, at the worker's Applied arm:
    /// pending pre-cut material can no longer be delivered, and the
    /// boundary bit arms so the NEXT post-cut block tells the analyst
    /// to reset its signal-derived state. RefusedUnchanged never calls
    /// this: observation continues without a fake reset.
    pub(crate) fn invalidate(&self) {
        let mut slot = self.lock_slot();
        slot.pending = false;
        slot.after_cut = true;
        drop(slot);
        self.shared.block_ready.notify_all();
    }

    /// Stop request from the owner's exit funnel: the analyst delivers
    /// any pending block, publishes its closed evidence, and exits.
    /// Idempotent.
    pub(crate) fn close(&self) {
        let mut slot = self.lock_slot();
        slot.closed = true;
        drop(slot);
        self.shared.block_ready.notify_all();
    }

    /// Consumer side, nonblocking: delivers the pending block into
    /// `dst` (capacity is reused across calls) as `(frames, after_cut)`,
    /// or `None` when nothing is pending. The boundary bit is consumed
    /// by the delivery that carries it. The production analyst uses
    /// [`Shared::wait_and_take`] instead; this primitive is the
    /// deterministic oracle seam, like the edge's test-only occupancy
    /// reader.
    #[cfg(all(test, not(loom)))]
    pub(crate) fn take_into(&self, dst: &mut Vec<f32>) -> Option<(usize, bool)> {
        let mut slot = self.lock_slot();
        if !slot.pending {
            return None;
        }
        let frames = slot.frames;
        let after_cut = slot.after_cut;
        let samples = frames * self.shared.channels;
        dst.clear();
        dst.extend_from_slice(&slot.block[..samples]);
        slot.pending = false;
        slot.after_cut = false;
        Some((frames, after_cut))
    }

    /// The off-path analyst: one thread, the smallest earned execution
    /// context. It owns its copy-out buffer and its derived record and
    /// never touches the production path. Spawn failure retires
    /// observation only (the caller proceeds without a worker).
    pub(crate) fn spawn_worker(&self) -> Option<std::thread::JoinHandle<()>> {
        let shared = Arc::clone(&self.shared);
        std::thread::Builder::new()
            .name("qianqian-observation".into())
            .spawn(move || analyst_loop(&shared))
            .ok()
    }

    /// The analyst's latest published record (probe/oracle evidence).
    #[cfg(all(test, not(loom)))]
    pub(crate) fn latest(&self) -> ObservationSnapshot {
        *self.lock_record()
    }

    /// Poison-immune locking: an analyst panic must never fail a later
    /// producer offer (observation loss is normal; playback failure is
    /// not an observation outcome). The slot holds no invariant worth
    /// a poison guard.
    fn lock_slot(&self) -> std::sync::MutexGuard<'_, Slot> {
        self.shared
            .slot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(all(test, not(loom)))]
    fn lock_record(&self) -> std::sync::MutexGuard<'_, ObservationSnapshot> {
        self.shared
            .record
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The off-path consumer: wait for the latest block, publish the
/// record, repeat — all analysis-adjacent state lives here, never on
/// the production path. Exits when the tap is closed and nothing is
/// pending; a pending block is delivered before the exit so the last
/// offered material is not silently lost at teardown.
fn analyst_loop(shared: &Shared) {
    let mut block = Vec::new();
    while let Some((frames, after_cut)) = shared.wait_and_take(&mut block) {
        let mut record = shared
            .record
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        record.delivered_blocks += 1;
        record.delivered_frames += frames as u64;
        if after_cut {
            record.cuts += 1;
        }
    }
    let mut record = shared
        .record
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    record.worker_closed = true;
}

impl Shared {
    /// Wait for something to deliver, then take it under the SAME lock
    /// hold — the wait, the close/pending recheck and the copy-out are
    /// one critical section, so an `invalidate` racing between wake and
    /// take can never be misread as an exit condition (only `closed`
    /// ends the loop). `None` = closed and nothing pending.
    fn wait_and_take(&self, dst: &mut Vec<f32>) -> Option<(usize, bool)> {
        let mut slot = self
            .slot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        slot = self
            .block_ready
            .wait_while(slot, |s| !s.closed && !s.pending)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !slot.pending {
            return None;
        }
        let frames = slot.frames;
        let after_cut = slot.after_cut;
        dst.clear();
        dst.extend_from_slice(&slot.block[..frames * self.channels]);
        slot.pending = false;
        slot.after_cut = false;
        Some((frames, after_cut))
    }
}
