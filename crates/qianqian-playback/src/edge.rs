//! The bounded PCM data edge: one producer, one consumer, kernel-free
//! (first-audible-slice design §4). Preallocated once at construction;
//! the steady-state read/write path performs no allocation. Terminals —
//! EOF, failure, stop — always unblock both endpoints so no lifecycle
//! path can wedge the data plane.

use std::sync::{Arc, Condvar, Mutex};

use qianqian_core::ports::{PcmFrameSource, PcmPull};

/// Why the producer stopped writing. Failure detail lives in the
/// session-owned completion signal, not in the edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteOutcome {
    /// The whole slice was accepted.
    Written,
    /// The edge was stopped (or failed) mid-write; the rest was dropped.
    Stopped,
}

/// Terminal state of the edge as seen by the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeTerminal {
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

pub struct PcmEdge {
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
    pub fn new(channels: u16, capacity_frames: usize) -> Self {
        assert!(channels > 0, "an edge without channels cannot exist");
        assert!(capacity_frames > 0, "an unbounded or empty edge is not an edge");
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

    /// Producer side: block until the whole slice is accepted, or a
    /// terminal stops the write.
    pub fn write(&self, src: &[f32]) -> WriteOutcome {
        let mut offset = 0usize;
        let mut guard = self.state.lock().expect("pcm edge lock");
        loop {
            if guard.terminal != TERMINAL_OPEN {
                return WriteOutcome::Stopped;
            }
            if offset == src.len() {
                return WriteOutcome::Written;
            }
            let free = self.capacity_samples - guard.buffered;
            if free == 0 {
                guard = self.space_freed.wait(guard).expect("pcm edge lock");
                continue;
            }
            let take = free.min(src.len() - offset);
            let write_pos = guard.write_pos;
            copy_into_ring(&mut guard.samples, write_pos, &src[offset..offset + take]);
            guard.write_pos = (guard.write_pos + take) % self.capacity_samples;
            guard.buffered += take;
            offset += take;
            drop(guard);
            self.data_ready.notify_all();
            guard = self.state.lock().expect("pcm edge lock");
        }
    }

    /// Producer committed EOF: consumers drain what remains, then see
    /// [`PcmPull::Eof`].
    pub fn close_eof(&self) {
        self.set_terminal(TERMINAL_EOF);
    }

    /// Producer failed: consumers stop immediately; the failure detail is
    /// published through the session completion, not the edge.
    pub fn fail(&self) {
        self.set_terminal(TERMINAL_FAILED);
    }

    /// Request the data plane to stop; both endpoints unblock with
    /// terminal outcomes. Idempotent.
    pub fn stop(&self) {
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
    pub fn terminal(&self) -> EdgeTerminal {
        let guard = self.state.lock().expect("pcm edge lock");
        match guard.terminal {
            TERMINAL_OPEN => EdgeTerminal::Open,
            TERMINAL_EOF => EdgeTerminal::Eof,
            TERMINAL_FAILED => EdgeTerminal::Failed,
            TERMINAL_STOPPED => EdgeTerminal::Stopped,
            _ => unreachable!("unknown terminal encoding"),
        }
    }

    /// Frames currently buffered (diagnostics).
    pub fn buffered_frames(&self) -> usize {
        let guard = self.state.lock().expect("pcm edge lock");
        guard.buffered / self.channels
    }
}

impl PcmFrameSource for PcmEdge {
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
                    // rather than handing the consumer a torn frame.
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

/// The edge is always shared between the session legs.
pub type SharedEdge = Arc<PcmEdge>;
