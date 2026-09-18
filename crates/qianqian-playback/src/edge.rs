//! The bounded PCM data edge: one producer, one consumer, kernel-free
//! (first-audible-slice design §4). Preallocated once at construction;
//! the steady-state read/write path performs no allocation. Terminals —
//! EOF, failure, stop — always unblock both endpoints so no lifecycle
//! path can wedge the data plane.
//!
//! The synchronization primitives are selected by `cfg(loom)` (campaign
//! FV-CONC-0): the loom types are drop-in for the std ones and preserve
//! the synchronization semantics, so loom explores the real edge code
//! rather than a test copy. Outside a loom build this file is unchanged.

#[cfg(loom)]
use loom::sync::{Condvar, Mutex};
#[cfg(not(loom))]
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use qianqian_audio_api::ports::{PcmPull, RenderPcmInput};

/// Terminal state of the edge as seen by the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EdgeTerminal {
    Open,
    Eof,
    Failed,
    Stopped,
}

const TERMINAL_OPEN: u8 = 0;
const TERMINAL_EOF: u8 = 1;
const TERMINAL_FAILED: u8 = 2;
const TERMINAL_STOPPED: u8 = 3;

struct EdgeState {
    samples: Box<[f32]>,
    /// Sample-granularity ring cursor.
    read_pos: usize,
    write_pos: usize,
    buffered: usize,
    /// Terminal: one of TERMINAL_*, Open is the absence of one.
    terminal: u8,
}

pub(crate) struct PcmEdge {
    channels: usize,
    capacity_samples: usize,
    state: Mutex<EdgeState>,
    /// Producer waits here for free space.
    space_freed: Condvar,
    /// Consumer waits here for data.
    data_ready: Condvar,
}

impl PcmEdge {
    /// A bounded, preallocated edge for interleaved float32 frames.
    pub(crate) fn new(channels: u16, capacity_frames: usize) -> Self {
        assert!(channels > 0, "an edge without channels cannot exist");
        assert!(
            capacity_frames > 0,
            "an unbounded or empty edge is not an edge"
        );
        Self {
            channels: usize::from(channels),
            capacity_samples: capacity_frames * usize::from(channels),
            state: Mutex::new(EdgeState {
                samples: vec![0.0; capacity_frames * usize::from(channels)].into_boxed_slice(),
                read_pos: 0,
                write_pos: 0,
                buffered: 0,
                terminal: TERMINAL_OPEN,
            }),
            space_freed: Condvar::new(),
            data_ready: Condvar::new(),
        }
    }

    /// Producer side, bounded slice: write what currently fits and
    /// return immediately with the sample count accepted (0 when the
    /// edge is full or a terminal is set). This is the F5 seek
    /// protocol's bounded-slice write primitive: the decode worker's
    /// write wait observes the seek command slot between slices, so the
    /// worker always reaches its serialization point with bounded
    /// latency regardless of edge occupancy — no destructive pre-purge
    /// (D14.5).
    ///
    /// Partial writes leave the accepted prefix in the ring (FIFO
    /// integrity is unchanged); the caller owns the rest of the slice.
    pub(crate) fn write_some(&self, src: &[f32]) -> usize {
        let mut guard = self.state.lock().expect("pcm edge lock");
        if guard.terminal != TERMINAL_OPEN {
            return 0;
        }
        let free = self.capacity_samples - guard.buffered;
        let take = free.min(src.len());
        if take == 0 {
            return 0;
        }
        let write_pos = guard.write_pos;
        copy_into_ring(&mut guard.samples, write_pos, &src[..take]);
        guard.write_pos = (guard.write_pos + take) % self.capacity_samples;
        guard.buffered += take;
        drop(guard);
        self.data_ready.notify_all();
        take
    }

    /// Producer wait for free space or a state change, bounded by
    /// `slice`: the interruptible write loop's back-off between bounded
    /// slices. Wakes on space freed, any terminal, or the timeout (the
    /// timeout is the backstop that re-observes the seek command slot;
    /// a missed notify costs one slice of latency, never correctness).
    /// The caller holds no other lock across this wait.
    pub(crate) fn wait_for_space(&self, slice: Duration) {
        // `mut` for the loom branch below (it reassigns the guard across
        // wait_timeout returns); the std branch uses wait_timeout_while
        // and never reassigns, hence the allow.
        #[allow(unused_mut)]
        let mut guard = self.state.lock().expect("pcm edge lock");
        #[cfg(loom)]
        {
            // loom's Condvar has no wait_timeout_while; this loop is the
            // same contract — wait while open-and-full, bounded by the
            // slice, spuriously-wake safe — and loom models the timeout
            // branch of `wait_timeout` itself.
            while guard.terminal == TERMINAL_OPEN && guard.buffered == self.capacity_samples {
                let (woken, _) = self
                    .space_freed
                    .wait_timeout(guard, slice)
                    .expect("pcm edge lock");
                guard = woken;
            }
        }
        #[cfg(not(loom))]
        {
            let _ = self
                .space_freed
                .wait_timeout_while(guard, slice, |state| {
                    state.terminal == TERMINAL_OPEN && state.buffered == self.capacity_samples
                })
                .expect("pcm edge lock");
        }
    }

    /// The F5 non-terminal invalidate (ADR-PBK-002 D14.5): drop every
    /// buffered frame WITHOUT touching the terminal — the edge stays
    /// Open, first-wins terminal semantics are untouched, and both
    /// endpoints are woken (a consumer parked on an emptied edge must
    /// re-observe; a producer parked on a full edge gets its space).
    /// O(1): cursor reset, no data movement.
    ///
    /// Correctness contract: the caller must have proven no endpoint can
    /// still deliver stale data across the reset. In the frozen protocol
    /// that is the decode worker itself, exactly once, at its
    /// serialization point strictly AFTER a successful provider seek:
    /// the render leg is parked out of read (the commit is gated on
    /// park evidence) and the worker is the only producer, on its own
    /// path. This primitive is NOT exposed as a generic application
    /// operation — the discipline, not the primitive, is the
    /// load-bearing stale-PCM exclusion.
    pub(crate) fn invalidate(&self) {
        {
            let mut guard = self.state.lock().expect("pcm edge lock");
            guard.read_pos = 0;
            guard.write_pos = 0;
            guard.buffered = 0;
        }
        self.space_freed.notify_all();
        self.data_ready.notify_all();
    }

    /// Producer committed EOF: consumers drain what remains, then see
    /// [`PcmPull::Eof`].
    pub(crate) fn close_eof(&self) {
        self.set_terminal(TERMINAL_EOF);
    }

    /// Producer failed: consumers stop immediately; the failure detail is
    /// published through the session completion, not the edge.
    pub(crate) fn fail(&self) {
        self.set_terminal(TERMINAL_FAILED);
    }

    /// Request the data plane to stop; both endpoints unblock with
    /// terminal outcomes. Idempotent.
    pub(crate) fn stop(&self) {
        self.set_terminal(TERMINAL_STOPPED);
    }

    fn set_terminal(&self, terminal: u8) {
        {
            let mut guard = self.state.lock().expect("pcm edge lock");
            // First terminal wins: a committed EOF is not overwritten by a
            // late stop, and a failure is not downgraded to a stop.
            if guard.terminal == TERMINAL_OPEN {
                guard.terminal = terminal;
            }
        }
        self.space_freed.notify_all();
        self.data_ready.notify_all();
    }

    /// Current terminal state (session bookkeeping, not a hot-path call).
    pub(crate) fn terminal(&self) -> EdgeTerminal {
        let guard = self.state.lock().expect("pcm edge lock");
        match guard.terminal {
            TERMINAL_OPEN => EdgeTerminal::Open,
            TERMINAL_EOF => EdgeTerminal::Eof,
            TERMINAL_FAILED => EdgeTerminal::Failed,
            TERMINAL_STOPPED => EdgeTerminal::Stopped,
            _ => unreachable!("unknown terminal encoding"),
        }
    }

    /// Frames currently buffered (diagnostics). Test-only: the
    /// white-box settlement tests are its only consumers; no
    /// production path reads edge occupancy (D14.2/D14.3).
    #[cfg(all(test, not(loom)))]
    pub(crate) fn buffered_frames(&self) -> usize {
        let guard = self.state.lock().expect("pcm edge lock");
        guard.buffered / self.channels
    }
}

impl RenderPcmInput for PcmEdge {
    fn read_frames(&self, dst: &mut [f32]) -> PcmPull {
        assert!(
            dst.len() >= self.channels,
            "a read destination smaller than one frame is a caller bug"
        );
        let mut guard = self.state.lock().expect("pcm edge lock");
        loop {
            // A failed or stopped edge stops the consumer immediately —
            // buffered frames are abandoned, because the session outcome
            // is already decided (terminal checks precede the data check).
            if guard.terminal == TERMINAL_STOPPED || guard.terminal == TERMINAL_FAILED {
                return PcmPull::Stopped;
            }
            if guard.buffered > 0 {
                let want_frames = (dst.len() / self.channels).min(guard.buffered / self.channels);
                if want_frames == 0 {
                    // Only a partial frame is buffered: wait for the rest
                    // rather than handing the consumer a torn frame —
                    // unless the producer is gone (EOF), in which case the
                    // torn remainder is dropped: no terminal may wedge a
                    // reader behind data it can never consume.
                    if guard.terminal == TERMINAL_EOF {
                        return PcmPull::Eof;
                    }
                    guard = self.data_ready.wait(guard).expect("pcm edge lock");
                    continue;
                }
                let take = want_frames * self.channels;
                copy_from_ring(&guard.samples, guard.read_pos, &mut dst[..take]);
                guard.read_pos = (guard.read_pos + take) % self.capacity_samples;
                guard.buffered -= take;
                drop(guard);
                self.space_freed.notify_all();
                return PcmPull::Frames(want_frames);
            }
            if guard.terminal == TERMINAL_EOF {
                return PcmPull::Eof;
            }
            guard = self.data_ready.wait(guard).expect("pcm edge lock");
        }
    }

    fn stop(&self) {
        PcmEdge::stop(self)
    }
}

fn copy_into_ring(ring: &mut [f32], write_pos: usize, src: &[f32]) {
    let tail = ring.len() - write_pos;
    if src.len() <= tail {
        ring[write_pos..write_pos + src.len()].copy_from_slice(src);
    } else {
        let (head, rest) = src.split_at(tail);
        ring[write_pos..].copy_from_slice(head);
        ring[..rest.len()].copy_from_slice(rest);
    }
}

fn copy_from_ring(ring: &[f32], read_pos: usize, dst: &mut [f32]) {
    let tail = ring.len() - read_pos;
    if dst.len() <= tail {
        dst.copy_from_slice(&ring[read_pos..read_pos + dst.len()]);
    } else {
        let (head, rest) = dst.split_at_mut(tail);
        head.copy_from_slice(&ring[read_pos..]);
        rest.copy_from_slice(&ring[..rest.len()]);
    }
}
