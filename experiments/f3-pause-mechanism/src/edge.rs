//! The bounded PCM edge — a faithful copy of the production
//! `qianqian-playback` `PcmEdge` synchronization semantics (Mutex +
//! two Condvars, first-wins terminals, blocking write/read, torn-frame
//! guard), copied because the product edge is crate-private and this is
//! evidence code.
//!
//! Deliberately NOT changed for pause: the edge stays exactly the
//! production shape. Pause is a consumer-side mechanism (render loop
//! gate); a pause that mutated the edge would be a different, unearned
//! mechanism.

use std::sync::{Condvar, Mutex};

/// Why the producer stopped writing (product `WriteOutcome` shape).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteOutcome {
    Written,
    Stopped,
}

/// Terminal state of the edge (product `EdgeTerminal` shape).
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
    read_pos: usize,
    write_pos: usize,
    buffered: usize,
    terminal: u8,
}

/// The bounded ring edge. One producer, one consumer, kernel-free.
pub struct PcmEdge {
    channels: usize,
    capacity_samples: usize,
    state: Mutex<EdgeState>,
    space_freed: Condvar,
    data_ready: Condvar,
}

impl PcmEdge {
    pub fn new(channels: u16, capacity_frames: usize) -> Self {
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

    pub fn close_eof(&self) {
        self.set_terminal(TERMINAL_EOF);
    }

    pub fn fail(&self) {
        self.set_terminal(TERMINAL_FAILED);
    }

    pub fn stop(&self) {
        self.set_terminal(TERMINAL_STOPPED);
    }

    fn set_terminal(&self, terminal: u8) {
        {
            let mut guard = self.state.lock().expect("pcm edge lock");
            if guard.terminal == TERMINAL_OPEN {
                guard.terminal = terminal;
            }
        }
        self.space_freed.notify_all();
        self.data_ready.notify_all();
    }

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

    /// Frames currently buffered. Evidence diagnostic ONLY here (this
    /// whole crate is evidence); the product edge keeps this
    /// test-build-only on purpose and no product seam may expose it.
    pub fn buffered_frames(&self) -> usize {
        let guard = self.state.lock().expect("pcm edge lock");
        guard.buffered / self.channels
    }

    /// The pull side: `PcmPull`-shaped read used by the render loop.
    /// Same semantics as the product `RenderPcmInput` impl.
    pub fn read_frames(&self, dst: &mut [f32]) -> Pull {
        assert!(
            dst.len() >= self.channels,
            "a read destination smaller than one frame is a caller bug"
        );
        let mut guard = self.state.lock().expect("pcm edge lock");
        loop {
            if guard.terminal == TERMINAL_STOPPED || guard.terminal == TERMINAL_FAILED {
                return Pull::Stopped;
            }
            if guard.buffered > 0 {
                let want_frames = (dst.len() / self.channels).min(guard.buffered / self.channels);
                if want_frames == 0 {
                    if guard.terminal == TERMINAL_EOF {
                        return Pull::Eof;
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
                return Pull::Frames(want_frames);
            }
            if guard.terminal == TERMINAL_EOF {
                return Pull::Eof;
            }
            guard = self.data_ready.wait(guard).expect("pcm edge lock");
        }
    }
}

/// `PcmPull`-shaped outcome (product ports shape, locally spelled).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pull {
    Frames(usize),
    Eof,
    Stopped,
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
