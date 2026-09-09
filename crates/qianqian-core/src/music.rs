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
