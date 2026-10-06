//! The Visualizer route's presentation state (G4): the active
//! visualization mode and the latest Observation Plane snapshot.
//!
//! Ownership discipline: the snapshot is #187's Observation Plane
//! telemetry — a display-only owned copy the shell re-reads every
//! refresh. It is neither playback state nor device truth, and nothing
//! here analyzes a signal: the view renders exactly what the snapshot
//! carries, and `None` renders honestly as unavailable (silence is a
//! real snapshot, not an absence).

use qianqian_playback::ObservationSnapshot;

use super::state::TuiModel;

/// One visualization mode (G4). The mode IS the control: three
/// buttons, one per mode, in the route's toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizerMode {
    /// 32 fixed log bands (40 Hz–16 kHz), dBFS per the snapshot.
    Spectrum,
    /// Per-channel block peak / RMS meters.
    Levels,
    /// The latest block's channel-mean waveform.
    Waveform,
}

/// The modes in render (and Tab) order.
pub const VISUALIZER_MODES: [VisualizerMode; 3] = [
    VisualizerMode::Spectrum,
    VisualizerMode::Levels,
    VisualizerMode::Waveform,
];

impl VisualizerMode {
    /// The button label.
    pub fn label(self) -> &'static str {
        match self {
            VisualizerMode::Spectrum => "[Spectrum]",
            VisualizerMode::Levels => "[Peak-RMS]",
            VisualizerMode::Waveform => "[Waveform]",
        }
    }

    /// The compact label — same controls, shorter spellings.
    pub fn compact_label(self) -> &'static str {
        match self {
            VisualizerMode::Spectrum => "[Spectrum]",
            VisualizerMode::Levels => "[Levels]",
            VisualizerMode::Waveform => "[Wave]",
        }
    }

    /// The visualization panel's title, naming what the display IS so
    /// a glance cannot mistake it for playback state.
    pub fn title(self) -> &'static str {
        match self {
            VisualizerMode::Spectrum => " Spectrum — 32 log bands 40 Hz–16 kHz, dBFS ",
            VisualizerMode::Levels => " Levels — block peak / RMS per channel ",
            VisualizerMode::Waveform => " Waveform — latest block, channel mean ",
        }
    }
}

impl TuiModel {
    /// The active visualization mode (presentation only).
    pub fn visualizer_mode(&self) -> VisualizerMode {
        self.visualizer_mode
    }

    /// Switch the visualization mode. The mode buttons' geometry does
    /// not move, so no invalidation: the arm and the regions survive.
    pub fn set_visualizer_mode(&mut self, mode: VisualizerMode) {
        self.visualizer_mode = mode;
    }

    /// The runtime's per-refresh projection of the Observation Plane
    /// (G4): the latest owned snapshot of the CURRENT episode's
    /// reader, or `None` while no telemetry is available (no episode,
    /// or the reader has nothing published). A plain store — the run
    /// loop redraws every tick, and the snapshot's arrival never moves
    /// a control.
    pub fn note_observation_snapshot(&mut self, snapshot: Option<ObservationSnapshot>) {
        self.snapshot = snapshot;
    }

    /// The latest snapshot, if one is available.
    pub fn observation_snapshot(&self) -> Option<&ObservationSnapshot> {
        self.snapshot.as_ref()
    }
}
