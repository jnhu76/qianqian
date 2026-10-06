//! The Visualizer route body (G4): the three mode buttons and the
//! active visualization, rendered straight from the Observation Plane
//! snapshot the model re-reads every refresh.
//!
//! The route performs NO signal analysis: every glyph below is a
//! rescaled copy of data the playback crate already measured (the
//! snapshot's own dBFS bands, peak/RMS samples and waveform means).
//! `None` renders honestly as unavailable — silence is a real
//! snapshot, an absence is not silence (the #187 truth split). All
//! drawing is glyph-based, so the display stays readable in a
//! monochrome terminal.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};

use super::bold;
use crate::tui::model::{
    FocusId, HitRegion, HitTarget, TuiModel, VISUALIZER_MODES, VisualizerMode,
};
use qianqian_playback::{ObservationSnapshot, SPECTRUM_BANDS, SPECTRUM_FLOOR_DBFS};

/// The partial-height bar glyphs, low to high: the last segment of a
/// bar whose height is not an integral number of cells.
const BAR_GLYPHS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub(super) fn draw_visualizer(
    frame: &mut Frame,
    model: &mut TuiModel,
    area: Rect,
    regions: &mut Vec<HitRegion>,
) {
    let compact = model.compact_layout();
    let [toolbar, panel] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    draw_mode_toolbar(frame, model, toolbar, compact, regions);

    let mode = model.visualizer_mode();
    let block = Block::bordered().title(bold(mode.title()));
    let inner = block.inner(panel);
    frame.render_widget(block, panel);
    if inner.is_empty() {
        return;
    }

    match model.observation_snapshot() {
        Some(snapshot) => match mode {
            VisualizerMode::Spectrum => draw_spectrum(frame, snapshot, inner),
            VisualizerMode::Levels => draw_levels(frame, snapshot, inner),
            VisualizerMode::Waveform => draw_waveform(frame, snapshot, inner),
        },
        None => {
            // No telemetry published (no episode, or nothing since the
            // last cut): say so instead of fabricating a flat line —
            // and without naming an episode that may not exist.
            frame.render_widget(
                Paragraph::new(Line::from("Visualization unavailable — no telemetry.")),
                inner,
            );
        }
    }
}

/// The three mode buttons in one measured row (the shared toolbar
/// idiom). The ACTIVE mode renders bold; the FOCUSED one reversed.
fn draw_mode_toolbar(
    frame: &mut Frame,
    model: &mut TuiModel,
    area: Rect,
    compact: bool,
    regions: &mut Vec<HitRegion>,
) {
    let widths: Vec<Constraint> = VISUALIZER_MODES
        .iter()
        .map(|mode| {
            let label = if compact {
                mode.compact_label()
            } else {
                mode.label()
            };
            Constraint::Length(label.chars().count() as u16 + 2)
        })
        .collect();
    let cells = Layout::horizontal(widths).split(area);
    for (mode, cell) in VISUALIZER_MODES.iter().zip(cells.iter()) {
        let label = if compact {
            mode.compact_label()
        } else {
            mode.label()
        };
        let active = *mode == model.visualizer_mode();
        let focused = model.focus() == Some(FocusId::VisualizerMode(*mode));
        let paragraph = Paragraph::new(Line::from(label).centered());
        let style = if focused {
            Style::default().add_modifier(Modifier::REVERSED)
        } else if active {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        frame.render_widget(paragraph.style(style), *cell);
        regions.push(HitRegion {
            area: *cell,
            target: HitTarget::VisualizerMode(*mode),
        });
    }
}

/// The 32 log bands as a bar chart: each band's dBFS rescaled from
/// [`SPECTRUM_FLOOR_DBFS`] to unity, mapped onto the available columns
/// (bands compress; they never drop below one column each), with the
/// top cell fractional. This is the snapshot's musical display — not a
/// calibrated analyzer, and the title says which.
fn draw_spectrum(frame: &mut Frame, snapshot: &ObservationSnapshot, inner: Rect) {
    let columns = (inner.width as usize / SPECTRUM_BANDS).max(1);
    let bands_drawn = (inner.width as usize / columns).min(SPECTRUM_BANDS);
    let buffer = frame.buffer_mut();
    for (band, column) in (0..bands_drawn).flat_map(|band| {
        let offset = band * columns;
        (0..columns).map(move |c| (band, offset + c))
    }) {
        let dbfs = snapshot.spectrum_dbfs[band];
        let value = ((dbfs - SPECTRUM_FLOOR_DBFS) / -SPECTRUM_FLOOR_DBFS).clamp(0.0, 1.0);
        let height = value * f32::from(inner.height);
        let full_cells = height.floor() as u16;
        let fraction = height - full_cells as f32;
        for row in 0..inner.height {
            let y = inner.y + inner.height - 1 - row;
            let glyph = if row < full_cells {
                '█'
            } else if row == full_cells && fraction >= 0.125 {
                BAR_GLYPHS[(fraction * 8.0).clamp(0.0, 7.0) as usize]
            } else {
                ' '
            };
            if glyph != ' ' {
                let x = inner.x + column as u16;
                buffer[(x, y)].set_char(glyph);
            }
        }
    }
}

/// Per-channel peak/RMS meters: the RMS as a filled bar, the block
/// peak as a marker over it. Values above unity remain visible as
/// overload — the meter clamps to its width but flags the overload
/// rather than hiding it (the snapshot never clips its samples).
fn draw_levels(frame: &mut Frame, snapshot: &ObservationSnapshot, inner: Rect) {
    let channels = snapshot.format.channels.max(1) as usize;
    let rows = inner.height as usize;
    let label_width = 4usize;
    // One trailing column is RESERVED for the overload flag, so a peak
    // above unity always has somewhere honest to be.
    let meter_width = inner.width.saturating_sub(label_width as u16 + 2).max(1) as usize;
    let buffer = frame.buffer_mut();
    for (channel, row) in (0..channels).take(rows).enumerate() {
        let y = inner.y + row as u16;
        let label = format!("ch{channel:<2}");
        for (offset, glyph) in label.chars().enumerate() {
            buffer[(inner.x + offset as u16, y)].set_char(glyph);
        }
        let Some(level) = snapshot.channel_levels.get(channel) else {
            continue;
        };
        let rms_cells = (level.rms.clamp(0.0, 1.0) * meter_width as f32) as usize;
        let peak_cells = (level.peak.clamp(0.0, 1.0) * meter_width as f32) as usize;
        for cell in 0..meter_width {
            let x = inner.x + label_width as u16 + 1 + cell as u16;
            let glyph = if cell < rms_cells {
                '█'
            } else if cell == peak_cells && peak_cells > rms_cells {
                '▓'
            } else if cell < peak_cells {
                '░'
            } else {
                '·'
            };
            buffer[(x, y)].set_char(glyph);
        }
        // Overload: a peak above unity renders as a flag past the bar,
        // never as silence about the excess.
        if level.peak > 1.0 {
            let x = inner.x + label_width as u16 + meter_width as u16 + 1;
            if x < inner.x + inner.width {
                buffer[(x, y)].set_char('!');
            }
        }
    }
}

/// The latest block's channel-mean waveform: 64 bucket averages spread
/// across the panel, drawn symmetric around the midline. Values can
/// exceed unity (the snapshot does not clip); the display clamps to
/// the panel — a display choice, not a claim about the signal.
fn draw_waveform(frame: &mut Frame, snapshot: &ObservationSnapshot, inner: Rect) {
    let buffer = frame.buffer_mut();
    let mid = inner.y + inner.height / 2;
    let half = (inner.height / 2).max(1) as f32;
    for column in 0..inner.width {
        let index = u32::from(column) * 64 / u32::from(inner.width.max(1));
        let index = index.min(63) as usize;
        let amplitude = snapshot.waveform[index];
        let spread = (amplitude.abs() * half).clamp(0.0, half);
        let top = mid.saturating_sub(spread as u16);
        let bottom = (mid + spread as u16).min(inner.y + inner.height - 1);
        let x = inner.x + column;
        buffer[(x, mid)].set_char('·');
        for y in top..=bottom {
            buffer[(x, y)].set_char('█');
        }
    }
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use super::*;
    use crate::tui::model::{FocusId, TuiAction, TuiRoute};
    use crate::tui::view::testutil::{plain_model, rendered};
    use qianqian_audio_api::ports::PcmFormat;
    use qianqian_playback::ChannelLevel;

    /// A synthetic snapshot with a mid-scale spectrum, one hot channel,
    /// and a visible waveform — everything the renderers read.
    fn snapshot() -> ObservationSnapshot {
        let mut spectrum = [SPECTRUM_FLOOR_DBFS; SPECTRUM_BANDS];
        for band in spectrum.iter_mut().take(8) {
            *band = -20.0;
        }
        ObservationSnapshot {
            format: PcmFormat {
                sample_rate: 48_000,
                channels: 2,
                channel_mask: 0x3,
            },
            spectrum_dbfs: spectrum,
            channel_levels: vec![
                ChannelLevel {
                    peak: 1.6,
                    rms: 0.25,
                },
                ChannelLevel {
                    peak: 0.8,
                    rms: 0.5,
                },
            ]
            .into_boxed_slice(),
            waveform: [0.0; 64],
        }
    }

    /// The spectrum mode renders the snapshot's bars — and the mode
    /// buttons are published as hit regions.
    #[test]
    fn the_spectrum_mode_renders_bars_from_the_snapshot() {
        let mut model = plain_model();
        model.set_route(TuiRoute::Visualizer);
        model.note_observation_snapshot(Some(snapshot()));
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("Spectrum — 32 log bands"), "{text}");
        assert!(
            text.contains('█'),
            "the elevated bands draw as filled bars: {text}"
        );
        assert!(
            model
                .regions()
                .iter()
                .any(|region| region.target == HitTarget::VisualizerMode(VisualizerMode::Waveform)),
            "the mode buttons are hit-testable"
        );
    }

    /// Switching modes (through the same action the decoders dispatch)
    /// switches the panel — presentation only, the snapshot untouched.
    #[test]
    fn the_mode_action_switches_the_panel() {
        let mut model = plain_model();
        model.set_route(TuiRoute::Visualizer);
        model.note_observation_snapshot(Some(snapshot()));

        model.set_focus(Some(FocusId::VisualizerMode(VisualizerMode::Levels)));
        assert_eq!(
            model.activation(),
            Some(TuiAction::SetVisualizerMode(VisualizerMode::Levels)),
            "keyboard and mouse converge on one action (§30)"
        );
        model.set_visualizer_mode(VisualizerMode::Levels);
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("ch0"), "levels label the channels: {text}");
        assert!(text.contains('█'), "the RMS bar draws: {text}");
        assert!(
            text.contains('!'),
            "a peak above unity flags overload: {text}"
        );

        model.set_visualizer_mode(VisualizerMode::Waveform);
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("Waveform — latest block"), "{text}");
    }

    /// A floor-only spectrum (real silence) draws nothing above the
    /// floor — silence is a real snapshot, rendered as such.
    #[test]
    fn silence_is_a_real_snapshot_not_an_absence() {
        let mut silent = snapshot();
        silent.spectrum_dbfs = [SPECTRUM_FLOOR_DBFS; SPECTRUM_BANDS];
        silent.waveform = [0.0; 64];
        let mut model = plain_model();
        model.set_route(TuiRoute::Visualizer);
        model.note_observation_snapshot(Some(silent));
        let text = rendered(&mut model, 100, 30);
        assert!(
            !text.contains("unavailable"),
            "a published silence snapshot is available: {text}"
        );
    }

    /// The compact class keeps the three modes interactive with the
    /// short labels; the panel degrades by size, not by truth.
    #[test]
    fn the_visualizer_route_degrades_in_compact() {
        let mut model = plain_model();
        model.set_route(TuiRoute::Visualizer);
        model.note_observation_snapshot(Some(snapshot()));
        let text = rendered(&mut model, 50, 16);
        assert!(text.contains("[Spectrum]"), "{text}");
        assert!(text.contains("[Levels]"), "{text}");
        assert!(text.contains("[Wave]"), "{text}");
    }
}
