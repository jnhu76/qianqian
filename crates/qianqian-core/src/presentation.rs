//! Presentation: UI-facing state and actions.
//!
//! UI hosts consume product/domain projections; they do not inspect
//! `MusicKernel` or `TransportKernel` internals and never interpret raw
//! playback-temporal evidence. `PlaybackState` is already product meaning,
//! so mapping it to a UI view remains a Music-domain concern.

use crate::music::PlaybackState;

/// UI-facing player state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerView {
    pub playing: bool,
}

/// UI-facing player action translated from user input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerAction {
    Play,
    Pause,
}

impl From<PlaybackState> for PlayerView {
    fn from(state: PlaybackState) -> Self {
        Self {
            playing: state == PlaybackState::Playing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::PlaybackState;

    #[test]
    fn playback_state_maps_to_player_view() {
        assert!(PlayerView::from(PlaybackState::Playing).playing);
        assert!(!PlayerView::from(PlaybackState::Idle).playing);
        assert!(!PlayerView::from(PlaybackState::Paused).playing);
        assert!(!PlayerView::from(PlaybackState::Ended).playing);
    }
}
