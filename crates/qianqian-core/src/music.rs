//! Music Kernel: authority for music-domain and product semantics.
//!
//! `MusicKernel` decides user/product meaning such as playback-state meaning,
//! selection, playlist/repeat/shuffle policy, and what a terminal transport
//! outcome means for the product. Playback-temporal truth is deliberately not
//! owned here: cursor/window/generation/fence/raw playback evidence belongs to
//! `crate::transport::TransportKernel`.
//!
//! Mechanisms produce evidence; the owning semantic authority interprets it.
//! This shell freezes authority boundaries only, not final playback APIs.

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
}

impl MusicKernel {
    pub fn new() -> Self {
        Self {
            state: PlaybackState::Idle,
        }
    }

    pub fn state(&self) -> PlaybackState {
        self.state
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
