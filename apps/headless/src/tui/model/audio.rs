//! The Audio route's TUI-LOCAL draft of the App's DESIRED DSP
//! configuration (G3), plus the route's read-side labels.
//!
//! Ownership discipline (the campaign's frozen state table): the
//! desired DSP configuration is the ReferencePlayerApp's — the shell
//! never holds DSP truth of its own. What lives here is presentation:
//! a DRAFT the route edits in place, committed only through the App's
//! own seams on an explicit [Apply], discarded on [Cancel] or when the
//! App's desired configuration moves underneath it (someone else
//! changed it — the draft no longer describes what the user saw).
//! Every label is a desired-state statement; the standing line
//! `Applied: not reported.` is the route's truthfulness anchor — no
//! surface in the shell claims an applied-DSP readback.

use qianqian_playback::{
    AudioProcessingConfig, EQ_BAND_FREQUENCY_HZ, EQ_MAX_BAND_GAIN_DB, EqConfig, EqPreset,
    estimated_eq_headroom_guidance,
};

use super::actions::PlaylistCursor;
use super::modal::Modal;
use super::state::TuiModel;

/// The preamp stepper's bounds, in dB (product tuning of the CONTROL;
/// the App's seam remains the authority and re-validates every
/// commit). The range spans true silence to a firm boost without
/// reaching the EQ band bound's recklessness.
const PREAMP_MIN_DB: f32 = -60.0;
const PREAMP_MAX_DB: f32 = 12.0;
/// The Audio route's draft: the desired configuration as the route
/// first saw it (`base`), plus the field edits on top. Edits are
/// presentation until [Apply] commits them field-by-field through the
/// App's seams; `dirty` is exactly "the draft differs from the base it
/// seeded from".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioDraft {
    /// The desired configuration the draft seeded from — the
    /// staleness witness: when the App's desired configuration no
    /// longer equals this, the draft describes a base the user cannot
    /// see and is discarded.
    base: AudioProcessingConfig,
    enabled: bool,
    /// The draft preamp as a linear gain factor (the App seam's unit).
    preamp: f32,
    /// The draft EQ stage (`None` = no EQ stage configured; the shell
    /// offers no "remove the EQ stage" edit — bypass is the off
    /// switch, and presets/custom trims configure the stage).
    eq: Option<EqConfig>,
}

impl AudioDraft {
    /// Seed a draft from a desired configuration.
    fn seed(config: AudioProcessingConfig) -> Self {
        Self {
            base: config,
            enabled: config.enabled,
            preamp: config.gain,
            eq: config.eq,
        }
    }

    /// The draft as a configuration (what [Apply] would commit).
    pub fn config(&self) -> AudioProcessingConfig {
        AudioProcessingConfig {
            enabled: self.enabled,
            gain: self.preamp,
            eq: self.eq,
        }
    }

    /// Whether the draft differs from the base it seeded from.
    pub fn dirty(&self) -> bool {
        self.config() != self.base
    }

    /// The base the draft seeded from (the staleness witness).
    pub fn base(&self) -> AudioProcessingConfig {
        self.base
    }
}

impl TuiModel {
    /// The route's open draft, if any. `None` = the route is showing
    /// the App's desired configuration read-only; the first edit opens
    /// a draft seeded from it.
    pub fn audio_draft(&self) -> Option<&AudioDraft> {
        self.audio_draft.as_ref()
    }

    /// The enablement the toolbar's contextual [DSP: on/off] label
    /// shows: the draft's when one is open (what activation WOULD
    /// commit), the desired configuration's otherwise. Before the
    /// first refresh there is nothing observed — the label says on,
    /// the shell's neutral posture, and the first edit seeds the draft
    /// from reality anyway.
    pub fn audio_enabled_label(&self) -> bool {
        match self.audio_draft.as_ref() {
            Some(draft) => draft.config().enabled,
            None => self
                .desired_processing
                .as_ref()
                .map(|config| config.enabled)
                .unwrap_or(true),
        }
    }

    /// The desired configuration's EQ stage, while one is observed.
    pub fn desired_eq(&self) -> Option<EqConfig> {
        self.desired_processing
            .as_ref()
            .and_then(|config| config.eq)
    }

    /// Seed the draft if none is open, then edit it. A no-op when the
    /// shell has not observed a desired configuration yet (before the
    /// first refresh there is nothing to seed from — the route shows
    /// `pending` and the controls arm nothing).
    fn audio_edit(&mut self, edit: impl FnOnce(&mut AudioDraft)) {
        let Some(config) = self.desired_processing else {
            return;
        };
        let draft = self
            .audio_draft
            .get_or_insert_with(|| AudioDraft::seed(config));
        edit(draft);
    }

    /// Draft edit: toggle the desired processing enablement.
    pub fn audio_toggle_enabled(&mut self) {
        self.audio_edit(|draft| draft.enabled = !draft.enabled);
    }

    /// Draft edit: step the preamp `delta_db` decibels, clamped to the
    /// stepper's control range. The App seam re-validates on apply.
    pub fn audio_preamp_step(&mut self, delta_db: f32) {
        self.audio_edit(|draft| {
            let db = 20.0 * draft.preamp.max(0.0).log10() + delta_db;
            let db = db.clamp(PREAMP_MIN_DB, PREAMP_MAX_DB);
            draft.preamp = 10f32.powf(db / 20.0);
        });
    }

    /// Draft edit: step one band's trim `delta_db` decibels, clamped
    /// to the product band bound. Editing the EQ when no stage is
    /// configured seeds the neutral stage (the flat EQ — observationally
    /// the identity, the honest starting point for custom trims).
    /// An out-of-range band index is a decoder bug, not a product
    /// state; the edit is inert rather than panicking.
    pub fn audio_eq_band_step(&mut self, band: usize, delta_db: f32) {
        self.audio_edit(|draft| {
            if band >= EQ_BAND_FREQUENCY_HZ.len() {
                return;
            }
            let eq = draft.eq.get_or_insert(EqConfig::FLAT);
            eq.band_gain_db[band] =
                (eq.band_gain_db[band] + delta_db).clamp(-EQ_MAX_BAND_GAIN_DB, EQ_MAX_BAND_GAIN_DB);
        });
    }

    /// Draft edit: fill the EQ stage from a preset's band trims. The
    /// preamp and the enablement are the user's independent edits and
    /// stay untouched — the App's own preset seam is a whole-config
    /// replacement, but the DRAFT edits fields, and the apply path
    /// commits the EQ through the config seam (the summary still
    /// recognizes the preset by its trim values).
    pub fn audio_select_preset(&mut self, preset: EqPreset) {
        self.audio_edit(|draft| {
            draft.eq = Some(preset.to_config().eq.expect("preset carries an EQ"))
        });
    }

    /// Discard the draft. Returns whether it was dirty, so the caller
    /// can say so honestly.
    pub fn audio_cancel_draft(&mut self) -> bool {
        self.audio_draft.take().is_some_and(|draft| draft.dirty())
    }

    /// The runtime's per-refresh projection of the App's desired DSP
    /// configuration (the one write path for it): re-derive the
    /// summary label, and discard a draft whose base no longer
    /// matches — the App's desired configuration changed somewhere
    /// else, so the draft no longer describes what the user sees. The
    /// discard leaves an honest notice.
    pub fn note_desired_processing(&mut self, config: AudioProcessingConfig) {
        self.desired_processing = Some(config);
        self.set_desired_dsp(super::projection::dsp_summary(&config));
        if let Some(draft) = self.audio_draft.as_ref()
            && draft.base() != config
        {
            self.audio_draft = None;
            self.set_status(Some(
                "draft discarded: the desired DSP changed elsewhere".to_owned(),
            ));
        }
    }

    /// After an apply attempt the draft's staleness witness re-syncs
    /// to the committed desired configuration — the base changes came
    /// from the shell's OWN apply, not from elsewhere, so the next
    /// refresh must not discard the draft as stale. A full success
    /// leaves the draft clean; a partial refusal keeps the pending
    /// edits dirty over the up-to-date base.
    pub fn audio_note_applied(&mut self, config: AudioProcessingConfig) {
        if let Some(draft) = self.audio_draft.as_mut() {
            draft.base = config;
        }
    }

    /// The draft's summary line, while a draft is open. Always a
    /// desired-state statement about the DRAFT.
    pub fn audio_draft_summary(&self) -> Option<String> {
        self.audio_draft.as_ref().map(|draft| {
            let marker = if draft.dirty() { " [unsaved]" } else { "" };
            format!(
                "Draft: {}{}",
                super::projection::dsp_summary(&draft.config()),
                marker
            )
        })
    }

    /// The preset the presets modal's cursor selects, applying it to
    /// the draft. `None` with no cursor (nothing selected yet). The
    /// modal closes; the edit is a draft edit — visible, not committed.
    pub fn activate_preset_selection(&mut self) -> Option<EqPreset> {
        let cursor = match self.modal.as_ref() {
            Some(Modal::Presets { cursor }) => *cursor,
            _ => return None,
        };
        let presets = EqPreset::all();
        let preset = presets.get(cursor?)?;
        self.audio_select_preset(*preset);
        self.close_modal();
        Some(*preset)
    }

    /// Move the presets modal's cursor (the modal's one list).
    pub fn move_presets_cursor(&mut self, cursor: PlaylistCursor) {
        let Some(Modal::Presets { cursor: slot }) = self.modal.as_mut() else {
            return;
        };
        let last = EqPreset::all().len() - 1;
        *slot = match (*slot, cursor) {
            (_, PlaylistCursor::Row(row)) => Some(row.min(last)),
            (Some(current), PlaylistCursor::Previous) => Some(current.saturating_sub(1)),
            (Some(current), PlaylistCursor::Next) => Some((current + 1).min(last)),
            (None, PlaylistCursor::Previous) => Some(0),
            (None, PlaylistCursor::Next) => Some(0),
        };
    }

    /// The 10-band availability profile for the route's band table
    /// (the pure product description, read against the episode's
    /// source rate): `Some(true)` available, `Some(false)` inert at
    /// this rate, `None` no rate is known (no episode). Reports
    /// availability only — never enablement or applied state.
    pub fn audio_band_availability(&self) -> [Option<bool>; 10] {
        qianqian_playback::eq_band_availability(
            self.observation
                .source_format
                .as_ref()
                .map(|f| f.sample_rate),
        )
    }

    /// The headroom advisory line for the route (G3): the estimated
    /// steady-state EQ headroom guidance of the DRAFT's EQ (or the
    /// desired one, while no draft is open) at the episode's source
    /// rate. `None` honestly means "no advice to show" — no EQ stage,
    /// no source rate, or no honest advice for the data. The line says
    /// what the advisory is and is not: an estimate, not a clipping
    /// guarantee (headroom.rs's own boundary).
    pub fn audio_headroom_label(&self) -> Option<String> {
        let eq = match self.audio_draft.as_ref() {
            Some(draft) => draft.config().eq,
            None => self.desired_processing.as_ref()?.eq,
        }?;
        let rate = self.observation.source_format.as_ref()?.sample_rate;
        let guidance = estimated_eq_headroom_guidance(&eq, rate)?;
        Some(match guidance.guidance_db {
            db if db < 0.0 => format!(
                "Estimated steady-state EQ headroom guidance: {db:+.1} dB @ {} Hz (advisory; not a clipping guarantee)",
                guidance.peak_hz
            ),
            _ => {
                "Estimated steady-state EQ headroom guidance: none (the cascade amplifies nowhere)"
                    .to_owned()
            }
        })
    }
}

#[cfg(test)]
mod tests {
    //! Tests for this submodule.

    use super::super::modal::Modal::Presets as PresetsModal;
    use super::*;
    use crate::tui::model::{AudioButton, EqAdjust, FocusId, ModalKind, TuiAction, TuiRoute};

    /// The shell's observed starting point for the draft tests: the
    /// bypass configuration, as a fresh App reports it.
    fn bypass() -> AudioProcessingConfig {
        AudioProcessingConfig {
            enabled: false,
            gain: 1.0,
            eq: None,
        }
    }

    fn model_with_desired(config: AudioProcessingConfig) -> TuiModel {
        let mut model = TuiModel::new("song.flac");
        model.note_desired_processing(config);
        model
    }

    /// The first edit opens a draft seeded from the desired
    /// configuration; the edit marks it dirty and the summary line
    /// says so.
    #[test]
    fn the_first_edit_seeds_a_draft_and_marks_it_dirty() {
        let mut model = model_with_desired(bypass());
        assert!(model.audio_draft().is_none(), "no draft before an edit");
        assert!(model.audio_draft_summary().is_none());

        model.audio_toggle_enabled();
        let draft = model.audio_draft().expect("the edit opened a draft");
        assert!(draft.dirty());
        assert!(draft.config().enabled, "the toggle flipped");
        assert_eq!(draft.config().gain, 1.0, "the rest seeds from desired");
        let summary = model.audio_draft_summary().expect("summary");
        assert!(summary.contains("[unsaved]"), "{summary}");
        assert!(summary.starts_with("Draft: DSP (desired): on"), "{summary}");
    }

    /// The preamp stepper clamps to its control range: far below at
    /// true-silence territory, far above at the +12 dB ceiling. The
    /// App's seam remains the authority; this is the control's range.
    #[test]
    fn the_preamp_stepper_clamps_to_its_control_range() {
        let mut model = model_with_desired(bypass());
        model.audio_preamp_step(10_000.0);
        let boosted = model.audio_draft().expect("draft").config().gain;
        let ceiling = 10f32.powf(12.0 / 20.0);
        assert!(
            (boosted - ceiling).abs() < 1e-4,
            "clamped to +12 dB: {boosted} vs {ceiling}"
        );

        model.audio_preamp_step(-10_000.0);
        let cut = model.audio_draft().expect("draft").config().gain;
        let floor = 10f32.powf(-60.0 / 20.0);
        assert!(
            (cut - floor).abs() < 1e-6,
            "clamped to −60 dB: {cut} vs {floor}"
        );
    }

    /// The band stepper clamps at the product band bound (±18 dB) and
    /// refuses nothing — the control edits the draft, the seam
    /// validates on apply. Editing with no configured EQ stage seeds
    /// the neutral stage.
    #[test]
    fn the_band_stepper_clamps_at_the_product_band_bound() {
        let mut model = model_with_desired(bypass());
        model.audio_eq_band_step(5, 100.0);
        let boosted = model
            .audio_draft()
            .expect("draft")
            .config()
            .eq
            .expect("editing seeded the neutral stage");
        assert_eq!(boosted.band_gain_db[5], EQ_MAX_BAND_GAIN_DB);

        model.audio_eq_band_step(5, -100.0);
        let cut = model
            .audio_draft()
            .expect("draft")
            .config()
            .eq
            .expect("stage");
        assert_eq!(cut.band_gain_db[5], -EQ_MAX_BAND_GAIN_DB);

        // An out-of-range band index is inert, not a panic.
        model.audio_eq_band_step(10, 5.0);
    }

    /// Selecting a preset fills the DRAFT's EQ stage and stays a
    /// draft — the preamp and enablement are the user's independent
    /// edits and remain untouched.
    #[test]
    fn selecting_a_preset_fills_the_draft_eq_and_stays_a_draft() {
        let mut model = model_with_desired(bypass());
        model.audio_preamp_step(-6.0);
        model.audio_select_preset(EqPreset::Rock);
        let draft = model.audio_draft().expect("draft");
        assert_eq!(
            draft.config().eq,
            Some(EqPreset::Rock.to_config().eq.expect("EQ")),
            "the preset's trims filled the stage"
        );
        assert!(
            (draft.config().gain - 10f32.powf(-6.0 / 20.0)).abs() < 1e-5,
            "the preamp edit survived the preset"
        );
        assert!(draft.dirty());
    }

    /// Cancel discards the draft and reports whether it was dirty; a
    /// clean draft discards silently.
    #[test]
    fn cancel_discards_the_draft_and_says_whether_it_was_dirty() {
        let mut model = model_with_desired(bypass());
        assert!(!model.audio_cancel_draft(), "nothing to discard");
        model.audio_toggle_enabled();
        assert!(model.audio_cancel_draft(), "the dirty draft discarded");
        assert!(model.audio_draft().is_none());
    }

    /// The staleness rule (G3): when the App's desired configuration
    /// changes somewhere else, a draft whose base no longer matches is
    /// discarded with an honest notice — it no longer describes what
    /// the user sees.
    #[test]
    fn a_desired_change_elsewhere_discards_the_stale_draft_with_a_notice() {
        let mut model = model_with_desired(bypass());
        model.audio_toggle_enabled();
        assert!(model.audio_draft().is_some());

        // The same configuration again: no staleness.
        model.note_desired_processing(bypass());
        assert!(model.audio_draft().is_some());

        let changed = AudioProcessingConfig {
            gain: 0.5,
            ..bypass()
        };
        model.note_desired_processing(changed);
        assert!(model.audio_draft().is_none(), "the stale draft discarded");
        assert!(
            model
                .status()
                .is_some_and(|status| status.contains("draft discarded")),
            "{}",
            model.status().unwrap_or_default()
        );
    }

    /// The presets menu: opening without a matching EQ starts with no
    /// cursor; ↑/↓ move and clamp; activating the cursor fills the
    /// draft and closes the menu.
    #[test]
    fn the_presets_menu_moves_its_cursor_and_applies_to_the_draft() {
        let mut model = model_with_desired(bypass());
        model.open_modal(ModalKind::Presets);
        assert!(model.modal().is_some());
        assert!(
            model.focus_cycle() == vec![FocusId::PickerList],
            "the menu's cycle is its one list"
        );

        model.move_presets_cursor(PlaylistCursor::Next);
        model.move_presets_cursor(PlaylistCursor::Next);
        let preset = model
            .activate_preset_selection()
            .expect("a cursor selection");
        assert_eq!(preset, EqPreset::Jazz, "no cursor, two downs: flat, jazz");
        assert!(model.modal().is_none(), "the menu closed");
        let draft = model.audio_draft().expect("the menu edited the draft");
        assert_eq!(
            draft.config().eq,
            Some(EqPreset::Jazz.to_config().eq.expect("EQ"))
        );

        // The menu re-opens preselected on the draft's preset.
        model.open_modal(ModalKind::Presets);
        let modal = model.modal().expect("open");
        assert!(
            matches!(modal, PresetsModal { cursor: Some(1) }),
            "jazz preselected: {modal:?}"
        );
    }

    /// The Audio route's new focus targets carry their actions: the
    /// toolbar steppers and the band steppers converge on the same
    /// draft-edit actions the decoders produce.
    #[test]
    fn the_audio_focus_targets_carry_draft_edit_actions() {
        let mut model = model_with_desired(bypass());
        model.set_route(TuiRoute::Audio);
        model.set_focus(Some(FocusId::AudioButton(AudioButton::Enabled)));
        assert_eq!(model.activation(), Some(TuiAction::DspToggleEnabled));
        model.set_focus(Some(FocusId::EqBand {
            band: 3,
            adjust: EqAdjust::Boost,
        }));
        assert_eq!(model.activation(), Some(TuiAction::DspEqBandStep(3, 1)));
    }
}
