//! The Audio route body (G3): the App's DESIRED DSP configuration, the
//! route's TUI-local draft over it, the ten-band EQ steppers and the
//! toolbar — all presentation over the T1A seams.
//!
//! The route's one standing truth claim is [`APPLIED_NOT_REPORTED`]:
//! the seams commit desired state, no applied-DSP readback exists in
//! the product, and the route says so on every frame instead of
//! implying one. The headroom line (when shown) is the playback
//! crate's own advisory, worded as an estimate, never a guarantee.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};

use super::bold;
use crate::tui::model::{
    AUDIO_BUTTONS, AudioButton, EqAdjust, FocusId, HitRegion, HitTarget, TuiModel,
};
use qianqian_playback::{EQ_BAND_FREQUENCY_HZ, EQ_MAX_BAND_GAIN_DB};

/// The Audio route's standing truthfulness line (G3): the shell
/// commits DESIRED DSP through the App's seams; applied configuration
/// is not reported, and the route never implies otherwise.
pub const APPLIED_NOT_REPORTED: &str = "Applied: not reported.";

/// The draft-less desired label, before the first refresh.
const DESIRED_PENDING: &str = "DSP (desired): pending";

pub(super) fn draw_audio(
    frame: &mut Frame,
    model: &mut TuiModel,
    area: Rect,
    regions: &mut Vec<HitRegion>,
) {
    let compact = model.compact_layout();
    let info = info_lines(model, compact);
    let info_rows = info.len() as u16;
    // The toolbar is always exactly two measured rows (never a clipped
    // active hit target); the band section takes the rest.
    let [info_area, toolbar_area, bands_area] = Layout::vertical([
        Constraint::Length(info_rows),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .areas(area);

    frame.render_widget(ratatui::text::Text::from(info), info_area);

    draw_toolbar(frame, model, toolbar_area, compact, regions);
    if compact {
        draw_bands_summary(frame, model, bands_area);
    } else {
        draw_bands_editor(frame, model, bands_area, regions);
    }
}

/// The route's information block: the desired summary, the preamp
/// factor the route displays (T0: a numeric LINEAR gain factor — the
/// draft's while one is open, its own line saying so), the EQ draft's
/// line, the standing applied-not-reported line, the last processing
/// refusal (while the episode carries one), and the headroom advisory
/// (while there is honest advice to show).
fn info_lines(model: &TuiModel, compact: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(
        model
            .desired_dsp_label()
            .unwrap_or(DESIRED_PENDING)
            .to_owned(),
    ));
    match model.audio_preamp_summary() {
        Some(draft_line) => lines.push(Line::from(draft_line)),
        None => {
            if let Some(gain) = model.audio_preamp_display() {
                lines.push(Line::from(format!("Preamp (linear): {gain:.3}x")));
            }
        }
    }
    if let Some(eq_line) = model.audio_eq_summary() {
        lines.push(Line::from(eq_line));
    }
    lines.push(Line::from(APPLIED_NOT_REPORTED.to_owned()));
    if let Some(refusal) = &model.observation().last_processing_refusal {
        lines.push(Line::from(format!("last processing refusal: {refusal}")));
    }
    if let Some(headroom) = model.audio_headroom_label() {
        lines.push(Line::from(headroom));
    }
    if compact {
        lines.push(Line::from(
            "(band editing needs a wider terminal)".to_owned(),
        ));
    }
    lines
}

/// The Audio toolbar: the per-operation controls in two fixed measured
/// rows — the playlist toolbar's idiom (label-driven cells; a clipped
/// active hit target is a T0 violation). Row 1 is the enablement and
/// the preamp operation; row 2 the EQ operation and the preset picker.
fn draw_toolbar(
    frame: &mut Frame,
    model: &mut TuiModel,
    area: Rect,
    compact: bool,
    regions: &mut Vec<HitRegion>,
) {
    let [first, second] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
    let (row1, row2) = AUDIO_BUTTONS.split_at(4);
    draw_button_row(frame, model, first, row1, compact, regions);
    draw_button_row(frame, model, second, row2, compact, regions);
}

fn draw_button_row(
    frame: &mut Frame,
    model: &mut TuiModel,
    area: Rect,
    buttons: &[AudioButton],
    compact: bool,
    regions: &mut Vec<HitRegion>,
) {
    let enabled = model.audio_desired_enabled();
    let widths: Vec<Constraint> = buttons
        .iter()
        .map(|button| Constraint::Length(button.label(enabled, compact).chars().count() as u16 + 2))
        .collect();
    let cells = Layout::horizontal(widths).split(area);
    for (button, cell) in buttons.iter().zip(cells.iter()) {
        let label = button.label(enabled, compact);
        let focused = model.focus() == Some(FocusId::AudioButton(*button));
        // A plain text button, the T0 wireframe's `[Add File...]`
        // shape; focus is REVERSED (never colour alone).
        let paragraph = Paragraph::new(Line::from(label).centered());
        frame.render_widget(
            if focused {
                paragraph.style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                paragraph
            },
            *cell,
        );
        regions.push(HitRegion {
            area: *cell,
            target: HitTarget::AudioButton(*button),
        });
    }
}

/// The ten-band editor (Normal/Wide): a bordered grid, two bands per
/// row, each band a frequency label plus − / + steppers over its trim.
/// The published regions come from the same sub-Rects that drew the
/// steppers. Availability is marked per band (`*`), never hidden: an
/// inert band stays visible and editable — its trim stays in the
/// desired configuration and re-activates on an episode whose source
/// domain includes it.
fn draw_bands_editor(
    frame: &mut Frame,
    model: &mut TuiModel,
    area: Rect,
    regions: &mut Vec<HitRegion>,
) {
    let availability = model.audio_band_availability();
    let trims = current_trims(model);
    let block = Block::bordered().title(bold(" EQ bands "));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Length(1); 5]).split(inner);
    for row in 0..5 {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(rows[row]);
        draw_band_cell(frame, model, left, row, availability, trims, regions);
        draw_band_cell(frame, model, right, row + 5, availability, trims, regions);
    }
    // The legend shares the block's last row when there is one to
    // spare; below that the markers stay defined by the toolbar-free
    // info block instead.
    if inner.height > 5 {
        let legend_row = inner.y + inner.height - 1;
        frame.render_widget(
            Paragraph::new(Line::from(format!(
                "* inert at this source rate (band bound ±{EQ_MAX_BAND_GAIN_DB:.0} dB)"
            ))),
            Rect::new(inner.x, legend_row, inner.width, 1),
        );
    }
}

/// One band's cell: the fixed segment layout the regions mirror —
/// frequency, [−], trim, [+], availability marker.
fn draw_band_cell(
    frame: &mut Frame,
    model: &mut TuiModel,
    area: Rect,
    band: usize,
    availability: [Option<bool>; 10],
    trims: [f32; 10],
    regions: &mut Vec<HitRegion>,
) {
    let [freq, cut, trim, boost, inert] = Layout::horizontal([
        Constraint::Length(7),
        Constraint::Length(3),
        Constraint::Length(8),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .areas(area);

    let frequency = EQ_BAND_FREQUENCY_HZ[band];
    let freq_label = if frequency < 1000.0 {
        format!("{frequency:.0} Hz")
    } else {
        format!("{:.0} kHz", frequency / 1000.0)
    };
    frame.render_widget(Paragraph::new(Line::from(freq_label)), freq);
    frame.render_widget(
        Paragraph::new(Line::from(format!("{:+.1} dB", trims[band]))),
        trim,
    );

    for (cell, adjust) in [(cut, EqAdjust::Cut), (boost, EqAdjust::Boost)] {
        let label = match adjust {
            EqAdjust::Cut => "[−]",
            EqAdjust::Boost => "[+]",
        };
        let focused = model.focus() == Some(FocusId::EqBand { band, adjust });
        let paragraph = Paragraph::new(Line::from(label));
        frame.render_widget(
            if focused {
                paragraph.style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                paragraph
            },
            cell,
        );
        regions.push(HitRegion {
            area: cell,
            target: HitTarget::EqBand { band, adjust },
        });
    }

    let inert_label = match availability[band] {
        Some(false) => " *",
        _ => "  ",
    };
    frame.render_widget(Paragraph::new(Line::from(inert_label)), inert);
}

/// The trims the route currently shows: the EQ draft's when one is
/// open, the desired configuration's otherwise (before the first
/// refresh: the neutral stage — honest defaults, not fabricated state).
fn current_trims(model: &TuiModel) -> [f32; 10] {
    let eq = match model.audio_eq_draft() {
        Some(draft) => Some(draft.value()),
        None => model.desired_eq(),
    };
    eq.map(|eq| eq.band_gain_db).unwrap_or([0.0; 10])
}

/// The compact degradation (§27): the trims as a read-only summary —
/// the numbers stay truthful, the steppers and their regions drop
/// rather than clip.
fn draw_bands_summary(frame: &mut Frame, model: &mut TuiModel, area: Rect) {
    let trims = current_trims(model);
    let mut lines: Vec<Line<'static>> = Vec::new();
    for half in [0, 5] {
        let line: String = EQ_BAND_FREQUENCY_HZ[half..half + 5]
            .iter()
            .zip(trims[half..half + 5].iter())
            .map(|(f, t)| {
                let name = if *f < 1000.0 {
                    format!("{f:.0}")
                } else {
                    format!("{:.0}k", f / 1000.0)
                };
                format!("{name}:{t:+.0} ")
            })
            .collect();
        lines.push(Line::from(line));
    }
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(bold(" EQ bands "))),
        area,
    );
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use super::*;
    use crate::tui::model::TuiModel;
    use crate::tui::model::{FocusId, TuiAction};
    use crate::tui::view::testutil::{pending, plain_model, rendered, scan};
    use qianqian_audio_api::ports::PcmFormat;
    use qianqian_playback::{AudioProcessingConfig, EqPreset, PlaybackSessionObservation};

    /// The route's truth lines (G3): the desired summary, the standing
    /// applied-not-reported anchor, and the band grid — with no
    /// applied-state or playback claim anywhere in the frame.
    #[test]
    fn the_audio_route_renders_truth_lines_and_the_band_grid() {
        let mut model = plain_model();
        model.note_desired_processing(AudioProcessingConfig {
            enabled: false,
            gain: 1.0,
            eq: None,
        });
        model.set_route(crate::tui::model::TuiRoute::Audio);
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("DSP (desired): off (bypass)"), "{text}");
        assert!(text.contains(APPLIED_NOT_REPORTED), "{text}");
        assert!(text.contains("EQ bands"), "{text}");
        assert!(text.contains("31 Hz"), "{text}");
        assert!(text.contains("16 kHz"), "{text}");
        assert!(
            scan(&text).is_none(),
            "no forbidden playback claim on the audio route"
        );
    }

    /// The band and toolbar regions are published from the drawn
    /// cells, and activating one converges on the same draft-edit
    /// action the decoders produce (§30 parity).
    #[test]
    fn the_audio_route_publishes_band_and_toolbar_regions() {
        let mut model = plain_model();
        model.set_route(crate::tui::model::TuiRoute::Audio);
        rendered(&mut model, 100, 30);

        let regions = model.regions();
        assert!(
            regions
                .iter()
                .any(|region| region.target == HitTarget::AudioButton(AudioButton::EqCommit)),
            "the toolbar's [Apply EQ] is hit-testable"
        );
        assert!(
            regions
                .iter()
                .any(|region| region.target == HitTarget::AudioButton(AudioButton::PreampCommit)),
            "the toolbar's [Set preamp] is hit-testable"
        );
        assert!(
            regions.iter().any(|region| region.target
                == HitTarget::EqBand {
                    band: 9,
                    adjust: EqAdjust::Boost
                }),
            "every band stepper is hit-testable"
        );

        // The focus activation is the action the click would dispatch
        // (§30: keyboard and mouse converge on one vocabulary).
        model.set_focus(Some(FocusId::EqBand {
            band: 2,
            adjust: EqAdjust::Cut,
        }));
        assert_eq!(model.activation(), Some(TuiAction::DspEqBandStep(2, -1)));
    }

    /// The compact degradation (§27): the steppers and their regions
    /// drop rather than clip; the trims stay as a read-only summary
    /// with an honest note.
    #[test]
    fn the_audio_route_degrades_in_compact() {
        let mut model = plain_model();
        model.set_route(crate::tui::model::TuiRoute::Audio);
        let text = rendered(&mut model, 50, 16);
        assert!(text.contains("EQ bands"), "{text}");
        assert!(
            text.contains("band editing needs a wider terminal"),
            "{text}"
        );
        assert!(
            !model
                .regions()
                .iter()
                .any(|region| matches!(region.target, HitTarget::EqBand { .. })),
            "no band stepper regions below the editing class"
        );
        assert!(
            model
                .regions()
                .iter()
                .any(|region| matches!(region.target, HitTarget::AudioButton(_))),
            "the toolbar stays interactive"
        );
    }

    /// The headroom advisory appears exactly when there is honest
    /// advice: an EQ stage (draft or desired) AND a source rate. The
    /// wording says estimate and advisory — never a clipping
    /// guarantee.
    #[test]
    fn the_headroom_line_appears_only_with_eq_and_rate() {
        // No EQ configured (and, here, no source rate either): no
        // line at all.
        let mut model = TuiModel::new("song.flac");
        model.set_route(crate::tui::model::TuiRoute::Audio);
        assert!(model.audio_headroom_label().is_none());

        model.note_desired_processing(EqPreset::Bass.to_config());
        assert!(
            model.audio_headroom_label().is_none(),
            "an EQ without a source rate advises nothing here"
        );

        model.update(PlaybackSessionObservation {
            source_format: Some(PcmFormat {
                sample_rate: 48_000,
                channels: 2,
                channel_mask: 0x3,
            }),
            ..pending()
        });
        let line = model
            .audio_headroom_label()
            .expect("advises once the rate exists");
        assert!(
            line.contains("Estimated steady-state EQ headroom guidance"),
            "{line}"
        );
        assert!(line.contains("advisory"), "{line}");
        model.set_route(crate::tui::model::TuiRoute::Audio);
        let text = rendered(&mut model, 100, 30);
        assert!(text.contains("headroom guidance"), "{text}");
    }

    /// The episode's last processing refusal shows verbatim while the
    /// observation carries one, and drops when it does not.
    #[test]
    fn the_refusal_line_shows_while_the_episode_carries_one() {
        let mut model = plain_model();
        model.set_route(crate::tui::model::TuiRoute::Audio);
        model.update(PlaybackSessionObservation {
            last_processing_refusal: Some("preamp refused: gain out of range".to_owned()),
            ..pending()
        });
        let text = rendered(&mut model, 100, 30);
        assert!(
            text.contains("last processing refusal: preamp refused: gain out of range"),
            "{text}"
        );
    }
}
