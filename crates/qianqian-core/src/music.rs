//! Music Kernel: authority for player and music-domain semantics.
//!
//! Mechanism layers produce evidence; the Music Kernel decides
//! product meaning. R0 freezes ownership only, not final playback APIs.

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

/// Authority for player and music-domain semantics.
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
