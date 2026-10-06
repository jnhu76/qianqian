//! The Audio route's TUI-LOCAL per-operation drafts over the App's
//! DESIRED DSP configuration (G3, aligned to the T0 per-operation
//! contract), plus the route's read-side labels.
//!
//! Ownership discipline (the campaign's frozen state table): the
//! desired DSP configuration is the ReferencePlayerApp's — the shell
//! never holds DSP truth of its own. What lives here is presentation,
//! and T0 freezes its SHAPE: **editing is per operation, not a global
//! Apply transaction.** Each of the four admitted operations carries
//! its own local draft and commits through its OWN App seam:
//!
//! ```text
//! Enabled  -> set_processing_enabled   (dispatch commits directly)
//! Preset   -> set_eq_preset            (the WHOLE-configuration seam:
//!                                       processing on, unity preamp,
//!                                       the preset's Q/trims)
//! Preamp   -> set_preamp               ([Set preamp] / [Cancel edit])
//! EQ       -> set_eq_config            ([Apply EQ] / [Revert draft],
//!                                       exactly one call)
//! ```
//!
//! Cancel/Revert restore the latest desired value and send no command.
//! A draft is discarded when ITS field of the desired configuration
//! changed elsewhere (the staleness witness), with a visible notice.
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

/// Whether two linear preamp gains are the same at this UI's display
/// resolution. The stepper's accumulation can leave a ~ULP difference
/// at the SAME displayed factor; "unchanged" is evaluated at a
/// resolution coarser than that noise and far finer than the stepper's
/// own step (1e-3 relative), so a net-zero step pair never reads as an
/// edit. The enablement and the EQ compare exactly — their edits are
/// exact arithmetic.
fn same_gain(a: f32, b: f32) -> bool {
    const GAIN_EPS_RELATIVE: f32 = 1e-3;
    (a - b).abs() <= GAIN_EPS_RELATIVE * a.max(b)
}

/// One visible preamp stepper press, in linear gain. The STEP is
/// presentation tuning; the DOMAIN is not: the preamp's domain is the
/// App's own — finite nonnegative linear gain, including true silence
/// (0.0), with no manual upper bound frozen by any authority. The TUI
/// adds no product bound of its own; the App's seam re-validates every
/// commit.
const PREAMP_STEP_LINEAR: f32 = 0.1;

/// The preamp operation's local draft: the linear gain the route first
/// saw (`base`, the staleness witness) and the edited `value`.
/// Presentation until [Set preamp] commits it through the App's own
/// seam; [Cancel edit] restores the latest desired value and sends
/// nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreampDraft {
    base: f32,
    value: f32,
}

impl PreampDraft {
    /// The edited factor (what [Set preamp] would commit).
    pub fn value(&self) -> f32 {
        self.value
    }

    /// Whether the draft differs from the base it seeded from.
    pub fn dirty(&self) -> bool {
        !same_gain(self.value, self.base)
    }
}

/// The EQ operation's local draft: one WHOLE [`EqConfig`], seeded from
/// the desired stage — or from the public FLAT stage when no stage is
/// configured — so its Q is preserved; the band steppers edit trims
/// only. Presentation until [Apply EQ] commits it through exactly one
/// `set_eq_config`, which preserves enabled and preamp; [Revert draft]
/// restores the latest desired stage and sends nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqDraft {
    base: Option<EqConfig>,
    value: EqConfig,
}

impl EqDraft {
    /// The edited stage (what [Apply EQ] would commit).
    pub fn value(&self) -> EqConfig {
        self.value
    }

    /// Clean means "the draft stages nothing the desired stage does
    /// not already carry": an untouched neutral draft over a missing
    /// stage is the observational identity, not an edit.
    pub fn dirty(&self) -> bool {
        self.value != self.base.unwrap_or(EqConfig::FLAT)
    }
}

impl TuiModel {
    /// The preamp operation's open draft, if any. `None` = the route
    /// shows the desired preamp read-only; the first preamp edit opens
    /// a draft seeded from it.
    pub fn audio_preamp_draft(&self) -> Option<&PreampDraft> {
        self.preamp_draft.as_ref()
    }

    /// The EQ operation's open draft, if any. `None` = the route shows
    /// the desired stage read-only; the first band edit opens a draft
    /// seeded from it (or from the neutral stage).
    pub fn audio_eq_draft(&self) -> Option<&EqDraft> {
        self.eq_draft.as_ref()
    }

    /// The linear preamp factor the route displays: the draft's while
    /// one is open, else the desired configuration's. `None` before
    /// the first refresh — nothing has been observed yet, and the
    /// shell fabricates no number.
    pub fn audio_preamp_display(&self) -> Option<f32> {
        match self.preamp_draft.as_ref() {
            Some(draft) => Some(draft.value),
            None => self.desired_processing.map(|config| config.gain),
        }
    }

    /// The enablement the toolbar's contextual [DSP: on/off] label
    /// shows: the desired configuration's (the toggle commits directly
    /// through the App's seam — the operation has no draft). Before
    /// the first refresh there is nothing observed — the label says
    /// on, the shell's neutral posture.
    pub fn audio_desired_enabled(&self) -> bool {
        self.desired_processing
            .as_ref()
            .map(|config| config.enabled)
            .unwrap_or(true)
    }

    /// The desired configuration's EQ stage, while one is observed.
    pub fn desired_eq(&self) -> Option<EqConfig> {
        self.desired_processing
            .as_ref()
            .and_then(|config| config.eq)
    }

    /// Seed the preamp draft if none is open, then edit it. A no-op
    /// when the shell has not observed a desired configuration yet
    /// (before the first refresh there is nothing to seed from — the
    /// route shows `pending` and the controls arm nothing).
    fn audio_edit_preamp(&mut self, edit: impl FnOnce(&mut PreampDraft)) {
        let Some(config) = self.desired_processing else {
            return;
        };
        let draft = self.preamp_draft.get_or_insert(PreampDraft {
            base: config.gain,
            value: config.gain,
        });
        edit(draft);
    }

    /// Seed the EQ draft if none is open, then edit it (see
    /// [`Self::audio_edit_preamp`] for the pre-refresh no-op).
    fn audio_edit_eq(&mut self, edit: impl FnOnce(&mut EqDraft)) {
        let Some(config) = self.desired_processing else {
            return;
        };
        let draft = self.eq_draft.get_or_insert_with(|| EqDraft {
            base: config.eq,
            value: config.eq.unwrap_or(EqConfig::FLAT),
        });
        edit(draft);
    }

    /// Draft edit: step the preamp `presses` visible presses. Linear
    /// additive steps; the bottom saturates at true silence — 0.0 is
    /// IN the frozen domain — and there is no upper clamp: no manual
    /// preamp range is frozen by any authority (T0), so inventing one
    /// here would be product policy at the UI layer. The App's seam
    /// remains the authority and re-validates every commit.
    pub fn audio_preamp_step(&mut self, presses: i32) {
        self.audio_edit_preamp(|draft| {
            for _ in 0..presses.unsigned_abs() {
                draft.value = if presses < 0 {
                    (draft.value - PREAMP_STEP_LINEAR).max(0.0)
                } else {
                    draft.value + PREAMP_STEP_LINEAR
                };
            }
        });
    }

    /// Draft edit: step one band's trim `delta_db` decibels, clamped
    /// to the product band bound — ±18 dB is the trim domain the
    /// playback crate itself freezes, not a UI invention. Editing the
    /// EQ when no stage is configured seeds the neutral stage (the
    /// flat EQ — observationally the identity, the honest starting
    /// point for custom trims). An out-of-range band index is a
    /// decoder bug, not a product state; the edit is inert rather than
    /// panicking.
    pub fn audio_eq_band_step(&mut self, band: usize, delta_db: f32) {
        self.audio_edit_eq(|draft| {
            if band >= EQ_BAND_FREQUENCY_HZ.len() {
                return;
            }
            draft.value.band_gain_db[band] = (draft.value.band_gain_db[band] + delta_db)
                .clamp(-EQ_MAX_BAND_GAIN_DB, EQ_MAX_BAND_GAIN_DB);
        });
    }

    /// [Cancel edit]: discard the preamp draft. Returns whether it was
    /// dirty, so the caller can say so honestly.
    pub fn audio_cancel_preamp(&mut self) -> bool {
        self.preamp_draft.take().is_some_and(|draft| draft.dirty())
    }

    /// [Revert draft]: discard the EQ draft. Returns whether it was
    /// dirty, so the caller can say so honestly.
    pub fn audio_revert_eq(&mut self) -> bool {
        self.eq_draft.take().is_some_and(|draft| draft.dirty())
    }

    /// The runtime's per-refresh projection of the App's desired DSP
    /// configuration (the one write path for it): re-derive the summary
    /// label, and discard a draft whose OWN field no longer matches —
    /// the desired configuration changed somewhere else, so that draft
    /// no longer describes what the user sees. The discard leaves an
    /// honest notice; a draft over an untouched field survives.
    pub fn note_desired_processing(&mut self, config: AudioProcessingConfig) {
        self.desired_processing = Some(config);
        self.set_desired_dsp(super::projection::dsp_summary(&config));
        let mut discarded: Vec<&'static str> = Vec::new();
        if let Some(draft) = self.preamp_draft.as_ref()
            && !same_gain(draft.base, config.gain)
        {
            self.preamp_draft = None;
            discarded.push("preamp draft");
        }
        if let Some(draft) = self.eq_draft.as_ref()
            && draft.base != config.eq
        {
            self.eq_draft = None;
            discarded.push("EQ draft");
        }
        match discarded.as_slice() {
            [] => {}
            [one] => self.set_status(Some(format!(
                "{one} discarded: the desired DSP changed elsewhere"
            ))),
            _ => self.set_status(Some(
                "drafts discarded: the desired DSP changed elsewhere".to_owned(),
            )),
        }
    }

    /// The runtime's report that the preamp operation committed: the
    /// draft's value IS the desired configuration now, so the draft
    /// closes and the route reads the seam's result.
    pub fn audio_note_preamp_committed(&mut self) {
        self.preamp_draft = None;
    }

    /// The runtime's report that the EQ operation committed (one
    /// `set_eq_config`): the draft's stage IS the desired stage now.
    pub fn audio_note_eq_committed(&mut self) {
        self.eq_draft = None;
    }

    /// The runtime's report that the PRESET operation committed: the
    /// whole desired configuration was replaced (processing on, unity
    /// preamp, the preset's Q/trims), so any open drafts described a
    /// configuration that no longer exists and are discarded. Returns
    /// whether anything was discarded, for the runtime's honest status.
    pub fn audio_note_preset_committed(&mut self) -> bool {
        let preamp = self.preamp_draft.take().is_some();
        let eq = self.eq_draft.take().is_some();
        preamp || eq
    }

    /// The preamp draft's summary line, while one is open. Always a
    /// desired-state statement about the DRAFT.
    pub fn audio_preamp_summary(&self) -> Option<String> {
        self.preamp_draft.as_ref().map(|draft| {
            let marker = if draft.dirty() { " [unsaved]" } else { "" };
            format!("Preamp draft: {:.3}x (linear){marker}", draft.value)
        })
    }

    /// The EQ draft's summary line, while one is open. Always a
    /// desired-state statement about the DRAFT.
    pub fn audio_eq_summary(&self) -> Option<String> {
        self.eq_draft.as_ref().map(|draft| {
            let marker = if draft.dirty() { " [unsaved]" } else { "" };
            format!(
                "EQ draft: {}{marker}",
                super::projection::eq_stage_summary(&draft.value)
            )
        })
    }

    /// The preset the presets modal's cursor selects. `None` with no
    /// cursor (nothing selected yet). The modal closes; COMMITTING the
    /// choice is the runtime's move — the preset is the
    /// whole-configuration operation, performed through the App's own
    /// `set_eq_preset` seam, not a draft edit.
    pub fn activate_preset_selection(&mut self) -> Option<EqPreset> {
        let cursor = match self.modal.as_ref() {
            Some(Modal::Presets { cursor }) => *cursor,
            _ => return None,
        };
        let presets = EqPreset::all();
        let preset = presets.get(cursor?)?;
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
    /// steady-state EQ headroom guidance of the EQ the route displays
    /// (the draft's while one is open, the desired one otherwise) at
    /// the episode's source rate. `None` honestly means "no advice to
    /// show" — no EQ stage, no source rate, or no honest advice for
    /// the data. The line says what the advisory is and is not: an
    /// estimate, not a clipping guarantee (headroom.rs's own boundary).
    pub fn audio_headroom_label(&self) -> Option<String> {
        let eq = match self.eq_draft.as_ref() {
            Some(draft) => Some(draft.value()),
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

    /// The first preamp edit opens a preamp draft seeded from the
    /// desired configuration; the edit marks it dirty and the summary
    /// line says so. The EQ operation is untouched.
    #[test]
    fn the_first_preamp_edit_seeds_a_draft_and_marks_it_dirty() {
        let mut model = model_with_desired(bypass());
        assert!(
            model.audio_preamp_draft().is_none(),
            "no draft before an edit"
        );
        assert!(model.audio_preamp_summary().is_none());

        model.audio_preamp_step(1);
        let draft = model.audio_preamp_draft().expect("the edit opened a draft");
        assert!(draft.dirty());
        assert!((draft.value() - 1.1).abs() < 1e-6, "one press: +0.1 linear");
        let summary = model.audio_preamp_summary().expect("summary");
        assert!(summary.contains("[unsaved]"), "{summary}");
        assert!(summary.contains("1.100x"), "{summary}");
        assert!(
            model.audio_eq_draft().is_none(),
            "the EQ operation is its own"
        );
    }

    /// The preamp stepper saturates at true silence — 0.0 is IN the
    /// frozen domain — and adds NO upper bound: the domain is finite
    /// nonnegative linear gain with no frozen manual range (T0), and
    /// the UI invents none. The App's seam re-validates every commit.
    #[test]
    fn the_preamp_stepper_silences_at_zero_and_invents_no_ceiling() {
        let mut model = model_with_desired(bypass());
        model.audio_preamp_step(-1_000);
        let silenced = model.audio_preamp_draft().expect("draft").value();
        assert_eq!(silenced, 0.0, "the bottom of the domain is true silence");

        model.audio_preamp_step(1_000);
        let boosted = model.audio_preamp_draft().expect("draft").value();
        assert!(
            boosted > 99.0 && boosted.is_finite(),
            "no invented ceiling: {boosted}"
        );
    }

    /// A net-zero preamp round trip is NOT an edit: the draft reads
    /// clean at the display resolution instead of carrying an
    /// "[unsaved]" mark over ~ULP accumulation noise.
    #[test]
    fn a_net_zero_preamp_round_trip_is_not_an_edit() {
        let mut model = model_with_desired(bypass());
        model.audio_preamp_step(1);
        assert!(
            model.audio_preamp_draft().expect("draft").dirty(),
            "one real step is an edit"
        );
        model.audio_preamp_step(-1);
        let draft = model.audio_preamp_draft().expect("the draft stays open");
        assert!(
            !draft.dirty(),
            "the round trip lands on the same displayed factor: {:?}",
            draft.value()
        );
    }

    /// The band stepper clamps at the product band bound (±18 dB) and
    /// refuses nothing — the control edits the draft, the seam
    /// validates on apply. Editing with no configured EQ stage seeds
    /// the neutral stage. The preamp operation stays untouched.
    #[test]
    fn the_band_stepper_clamps_at_the_product_band_bound() {
        let mut model = model_with_desired(bypass());
        model.audio_eq_band_step(5, 100.0);
        let boosted = model.audio_eq_draft().expect("draft").value().band_gain_db;
        assert_eq!(boosted[5], EQ_MAX_BAND_GAIN_DB);

        model.audio_eq_band_step(5, -200.0);
        let cut = model.audio_eq_draft().expect("draft").value();
        assert_eq!(cut.band_gain_db[5], -EQ_MAX_BAND_GAIN_DB);

        // An out-of-range band index is inert, not a panic.
        model.audio_eq_band_step(10, 5.0);
        assert!(model.audio_preamp_draft().is_none());
    }

    /// An untouched neutral EQ draft over a missing stage is the
    /// observational identity: it does not read as an edit.
    #[test]
    fn an_untouched_neutral_eq_draft_over_no_stage_is_clean() {
        let mut model = model_with_desired(bypass());
        model.audio_eq_band_step(3, 0.0);
        let draft = model.audio_eq_draft().expect("the edit opened a draft");
        assert!(!draft.dirty(), "a no-op step edits nothing: {draft:?}");
        let summary = model
            .audio_eq_summary()
            .expect("the draft line shows while the draft is open");
        assert!(
            !summary.contains("[unsaved]"),
            "a clean draft carries no unsaved mark: {summary}"
        );
    }

    /// [Cancel edit] and [Revert draft] discard their OWN operation's
    /// draft and report whether it was dirty; the other operation's
    /// draft survives.
    #[test]
    fn cancel_and_revert_discard_their_own_operation_only() {
        let mut model = model_with_desired(bypass());
        assert!(!model.audio_cancel_preamp(), "nothing to discard");
        assert!(!model.audio_revert_eq(), "nothing to revert");

        model.audio_preamp_step(1);
        model.audio_eq_band_step(0, 3.0);
        assert!(
            model.audio_cancel_preamp(),
            "the dirty preamp draft discarded"
        );
        assert!(model.audio_preamp_draft().is_none());
        assert!(model.audio_eq_draft().is_some(), "the EQ draft survives");
        assert!(model.audio_revert_eq(), "the dirty EQ draft reverted");
        assert!(model.audio_eq_draft().is_none());
    }

    /// The staleness rule (G3, per operation): when the App's desired
    /// configuration changes somewhere else, only the drafts whose OWN
    /// field no longer matches are discarded, each with an honest
    /// notice — they no longer describe what the user sees.
    #[test]
    fn a_desired_change_elsewhere_discards_the_stale_drafts_with_a_notice() {
        let mut model = model_with_desired(bypass());
        model.audio_preamp_step(1);
        model.audio_eq_band_step(2, 2.0);
        assert!(model.audio_preamp_draft().is_some());
        assert!(model.audio_eq_draft().is_some());

        // The same configuration again: no staleness.
        model.note_desired_processing(bypass());
        assert!(model.audio_preamp_draft().is_some());
        assert!(model.audio_eq_draft().is_some());

        // Only the preamp moved: the preamp draft is stale, the EQ
        // draft survives (its field is untouched).
        let changed = AudioProcessingConfig {
            gain: 0.5,
            ..bypass()
        };
        model.note_desired_processing(changed);
        assert!(
            model.audio_preamp_draft().is_none(),
            "the stale draft discarded"
        );
        assert!(
            model.audio_eq_draft().is_some(),
            "the untouched draft survives"
        );
        assert!(
            model
                .status()
                .is_some_and(|status| status.contains("preamp draft discarded")),
            "{}",
            model.status().unwrap_or_default()
        );

        // Only the EQ moved now: the EQ draft goes, with its notice.
        let mut changed = bypass();
        changed.eq = Some(EqConfig::FLAT);
        model.note_desired_processing(changed);
        assert!(model.audio_eq_draft().is_none());
        assert!(
            model
                .status()
                .is_some_and(|status| status.contains("EQ draft discarded")),
            "{}",
            model.status().unwrap_or_default()
        );
    }

    /// The preset operation reads the DESIRED configuration for its
    /// preselection and commits nothing itself — the runtime performs
    /// the whole-config commit through the App's seam. Activating the
    /// cursor only reports the choice and closes the menu.
    #[test]
    fn the_presets_menu_moves_its_cursor_and_reports_the_choice() {
        let mut model = model_with_desired(bypass());
        model.open_modal(ModalKind::Presets);
        assert!(model.modal().is_some());
        assert!(
            model.focus_cycle() == vec![FocusId::PickerList],
            "the menu's cycle is its one list"
        );
        let modal = model.modal().expect("open");
        assert!(
            matches!(modal, PresetsModal { cursor: None }),
            "bypass matches no preset: no cursor"
        );

        model.move_presets_cursor(PlaylistCursor::Next);
        model.move_presets_cursor(PlaylistCursor::Next);
        let preset = model
            .activate_preset_selection()
            .expect("a cursor selection");
        assert_eq!(preset, EqPreset::Jazz, "no cursor, two downs: flat, jazz");
        assert!(model.modal().is_none(), "the menu closed");
        assert!(
            model.audio_preamp_draft().is_none() && model.audio_eq_draft().is_none(),
            "the menu itself edits no draft"
        );

        // The menu re-opens preselected on the DESIRED configuration's
        // preset.
        model.open_modal(ModalKind::Presets);
        let modal = model.modal().expect("open");
        assert!(
            matches!(modal, PresetsModal { cursor: None }),
            "the desired configuration is still bypass: no preset matches"
        );
    }

    /// The Audio route's focus targets carry their actions: the
    /// toolbar's per-operation controls converge on the same actions
    /// the decoders produce.
    #[test]
    fn the_audio_focus_targets_carry_their_operation_actions() {
        let mut model = model_with_desired(bypass());
        model.set_route(TuiRoute::Audio);
        model.set_focus(Some(FocusId::AudioButton(AudioButton::Enabled)));
        assert_eq!(model.activation(), Some(TuiAction::DspToggleEnabled));
        model.set_focus(Some(FocusId::AudioButton(AudioButton::PreampCommit)));
        assert_eq!(model.activation(), Some(TuiAction::DspPreampCommit));
        model.set_focus(Some(FocusId::AudioButton(AudioButton::EqCommit)));
        assert_eq!(model.activation(), Some(TuiAction::DspApplyEq));
        model.set_focus(Some(FocusId::EqBand {
            band: 3,
            adjust: EqAdjust::Boost,
        }));
        assert_eq!(model.activation(), Some(TuiAction::DspEqBandStep(3, 1)));
    }
}
