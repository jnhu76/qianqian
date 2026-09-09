//! Music Kernel (experimental evidence): music-domain/product semantics of
//! an earlier Playback architecture experiment.
//!
//! `MusicKernel` decides user/product meaning such as playback-state meaning,
//! selection, playlist/repeat/shuffle policy, and what a terminal transport
//! outcome means for the product. Playback-temporal truth is deliberately not
//! owned here: cursor/window/generation/fence/raw playback evidence belongs to
//! `crate::transport::TransportKernel`.
//!
//! Mechanisms produce evidence; the owning semantic authority interprets it.
//! This shell freezes authority boundaries only, not final playback APIs.

use crate::transport::{FactRevision, StampedFact};

/// User-visible playback state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaybackState {
    #[default]
    Idle,
    Ready,
    Playing,
    Paused,
    Ended,
}

/// Typed derived facts TransportKernel hands to MusicKernel.
///
/// Raw playback evidence (decode results, EOF, submitted/rendered media,
/// fence verdicts) is interpreted exactly once by TransportKernel;
/// MusicKernel only ever sees these derived facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportFact {
    /// The transport reached natural drain: producer terminal, no
    /// in-flight media, no submitted-but-unrendered media, no fence in
    /// flight. Product meaning (ENDED/repeat/next) is MusicKernel's call.
    NaturallyDrained,
    /// A terminal stop fence completed: the transport holds no active
    /// window. Stopped is a different product interpretation than ENDED.
    Stopped,
    /// A prepared contribution terminated before readiness (for example
    /// a decoder EOF before priming) and was dropped with this explicit
    /// outcome; it never silently became ready.
    PreparedAbandonedBeforeReadiness,
}

/// Authority for music-domain and product semantics.
#[derive(Debug, Default)]
pub struct MusicKernel {
    state: PlaybackState,
    stale_facts_dropped: u64,
}

impl MusicKernel {
    pub fn new() -> Self {
        Self {
            state: PlaybackState::Idle,
            stale_facts_dropped: 0,
        }
    }

    pub fn state(&self) -> PlaybackState {
        self.state
    }

    /// Facts whose delivery crossed an invalidating temporal mutation.
    /// Dropped, never interpreted; the count makes the drop observable.
    pub fn stale_facts_dropped(&self) -> u64 {
        self.stale_facts_dropped
    }

    /// Interpret one derived transport fact. Raw playback evidence never
    /// reaches this kernel; each fact is interpreted exactly once, here.
    ///
    /// `current` is the transport's [`FactRevision`] read in the same
    /// synchronous delivery transaction that observes the fact. A fact
    /// stamped with a superseded revision describes a temporal state that
    /// no longer exists — it is dropped as stale, never interpreted; the
    /// product meaning of the superseded state is carried by whatever
    /// superseded it (new episode, promotion, stop).
    ///
    /// Terminal-state interpretation policy: a natural drain means the
    /// media ENDED; a completed stop fence means stopped (a distinct
    /// interpretation, expressed as Idle rather than Ended); an abandoned
    /// pre-ready prepared contribution leaves product state untouched.
    pub fn observe(&mut self, fact: StampedFact, current: FactRevision) {
        if fact.revision != current {
            self.stale_facts_dropped += 1;
            return;
        }
        match fact.fact {
            TransportFact::NaturallyDrained => self.state = PlaybackState::Ended,
            TransportFact::Stopped => self.state = PlaybackState::Idle,
            TransportFact::PreparedAbandonedBeforeReadiness => {}
        }
    }

    /// Interpret a batch of derived facts in arrival order under one
    /// delivery-transaction revision.
    pub fn observe_all(
        &mut self,
        facts: impl IntoIterator<Item = StampedFact>,
        current: FactRevision,
    ) {
        for fact in facts {
            self.observe(fact, current);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_music_kernel_is_idle() {
        let kernel = MusicKernel::new();
        assert_eq!(kernel.state(), PlaybackState::Idle);
    }
}
