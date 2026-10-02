//! The named product EQ presets (Issue #177 Stage 2 / I4;
//! ADR-PBK-002 D14.11). Presets are DATA — pure product configuration
//! over [`crate::AudioProcessingConfig`]/[`crate::EqConfig`] — not
//! Plugins, not processor types, not classes with independent
//! lifecycle, not new effects (the Issue #177 preset taxonomy, frozen:
//! "Preset = configuration data"). Selecting a preset constructs an
//! ordinary desired configuration; the episode binds it exactly like
//! any Custom configuration (episode-fixed applied snapshot, case B of
//! the D14.11 model; a preset change takes effect at the NEXT episode).
//!
//! The curves below are Qianqian PRODUCT TUNING — chosen for this
//! player's reference listening, deliberately NOT presented as
//! scientifically universal or canonical. The exact values are recorded
//! here (preamp: unity for every preset; band mapping: the fixed
//! [`crate::processing`-owned product table] 31 Hz low shelf, 8 peaking
//! bands, 16 kHz high shelf, in band order).

use crate::processing::{AudioProcessingConfig, EqConfig};

/// The named product EQ presets. `Flat` is the neutral anchor; the rest
/// are product tuning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EqPreset {
    /// Neutral: every band 0 dB, unity preamp — observationally the
    /// bypass (pinned BIT-EXACT by the I3 oracle at the real seam).
    Flat,
    /// Moderate low warmth, gently recessed mids, airy top.
    Jazz,
    /// Recessed lows, forward presence for vocals.
    Vocal,
    /// Warm lows and highs, slightly eased upper mids.
    Blues,
    /// The classic rock smile: firm lows and highs, eased mids.
    Rock,
    /// Gentle extremes, protected midrange for acoustic material.
    Classical,
    /// Low-frequency emphasis.
    Bass,
    /// High-frequency emphasis.
    Treble,
}

impl EqPreset {
    /// The preset's band trims in dB, in product band order (31 Hz …
    /// 16 kHz). THE RECORDED DATA — deterministic constants.
    pub fn band_gain_db(self) -> [f32; 10] {
        match self {
            Self::Flat => [0.0; 10],
            Self::Jazz => [4.0, 3.0, 1.0, 2.0, -1.0, 0.0, -1.0, 1.0, 3.0, 2.0],
            Self::Vocal => [-3.0, -2.0, 0.0, 3.0, 4.0, 4.0, 3.0, 1.0, 0.0, -1.0],
            Self::Blues => [3.0, 2.0, 1.0, 2.0, -1.0, 1.0, 0.0, 2.0, 3.0, 1.0],
            Self::Rock => [5.0, 4.0, 2.0, -1.0, -2.0, 1.0, 3.0, 4.0, 4.0, 2.0],
            Self::Classical => [3.0, 2.0, 0.0, 0.0, 0.0, 0.0, -1.0, -2.0, 2.0, 3.0],
            Self::Bass => [8.0, 7.0, 5.0, 2.0, 0.0, -1.0, -2.0, -1.0, 1.0, 2.0],
            Self::Treble => [-2.0, -1.0, 0.0, 0.0, 0.0, 1.0, 2.0, 4.0, 6.0, 7.0],
        }
    }

    /// Resolve the preset to the desired Audio Processing configuration:
    /// UNITY preamp (recorded product decision — the presets shape tone,
    /// not headroom; the volume and preamp remain separate controls) and
    /// the preset's band trims over the fixed product band table with
    /// the product Q. Deterministic: the same preset always resolves to
    /// the same configuration.
    pub fn to_config(self) -> AudioProcessingConfig {
        AudioProcessingConfig {
            enabled: true,
            gain: 1.0,
            eq: Some(EqConfig::new(self.band_gain_db(), 1.0)),
        }
    }

    /// Parse a preset by its lowercase CLI name (`flat`, `jazz`,
    /// `vocal`, `blues`, `rock`, `classical`, `bass`, `treble`). The
    /// shell-side selection helper for the `--eq` flag.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "flat" => Some(Self::Flat),
            "jazz" => Some(Self::Jazz),
            "vocal" => Some(Self::Vocal),
            "blues" => Some(Self::Blues),
            "rock" => Some(Self::Rock),
            "classical" => Some(Self::Classical),
            "bass" => Some(Self::Bass),
            "treble" => Some(Self::Treble),
            _ => None,
        }
    }

    /// The preset's lowercase CLI name — the exact inverse of
    /// [`Self::from_name`], so the product vocabulary lives in one
    /// place (shells build refusals from `all()` + `name()`, never a
    /// second hardcoded list).
    pub fn name(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Jazz => "jazz",
            Self::Vocal => "vocal",
            Self::Blues => "blues",
            Self::Rock => "rock",
            Self::Classical => "classical",
            Self::Bass => "bass",
            Self::Treble => "treble",
        }
    }

    /// Every preset, in the product's presentation order.
    pub fn all() -> [Self; 8] {
        [
            Self::Flat,
            Self::Jazz,
            Self::Vocal,
            Self::Blues,
            Self::Rock,
            Self::Classical,
            Self::Bass,
            Self::Treble,
        ]
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;
    use crate::processing::{AudioProcessingConfig, EqConfig};

    /// The recorded data is exactly what this module documents: each
    /// preset resolves deterministically and matches its recorded band
    /// table (the values ARE the product tuning record).
    #[test]
    fn presets_resolve_deterministically_to_the_recorded_data() {
        let expected: [(EqPreset, [f32; 10]); 8] = [
            (EqPreset::Flat, [0.0; 10]),
            (
                EqPreset::Jazz,
                [4.0, 3.0, 1.0, 2.0, -1.0, 0.0, -1.0, 1.0, 3.0, 2.0],
            ),
            (
                EqPreset::Vocal,
                [-3.0, -2.0, 0.0, 3.0, 4.0, 4.0, 3.0, 1.0, 0.0, -1.0],
            ),
            (
                EqPreset::Blues,
                [3.0, 2.0, 1.0, 2.0, -1.0, 1.0, 0.0, 2.0, 3.0, 1.0],
            ),
            (
                EqPreset::Rock,
                [5.0, 4.0, 2.0, -1.0, -2.0, 1.0, 3.0, 4.0, 4.0, 2.0],
            ),
            (
                EqPreset::Classical,
                [3.0, 2.0, 0.0, 0.0, 0.0, 0.0, -1.0, -2.0, 2.0, 3.0],
            ),
            (
                EqPreset::Bass,
                [8.0, 7.0, 5.0, 2.0, 0.0, -1.0, -2.0, -1.0, 1.0, 2.0],
            ),
            (
                EqPreset::Treble,
                [-2.0, -1.0, 0.0, 0.0, 0.0, 1.0, 2.0, 4.0, 6.0, 7.0],
            ),
        ];
        for (preset, bands) in expected {
            assert_eq!(preset.band_gain_db(), bands, "{preset:?} drifted");
            let first = preset.to_config();
            let second = preset.to_config();
            assert_eq!(first, second, "{preset:?} must resolve deterministically");
            assert!(first.enabled);
            assert_eq!(first.gain, 1.0, "{preset:?}: the recorded preamp is unity");
        }
    }

    /// Every preset resolves to a VALID configuration: finite, within
    /// the product bound, positive Q — establishment would compile it.
    #[test]
    fn every_preset_resolves_to_a_valid_configuration() {
        for preset in EqPreset::all() {
            preset
                .to_config()
                .validate()
                .unwrap_or_else(|e| panic!("{preset:?} does not validate: {e}"));
        }
    }

    /// The Flat preset resolves to exactly the neutral configuration the
    /// I3 oracles proved bit-exactly identical to bypass at the real
    /// seam — the strongest product oracle rides on the proven shape.
    #[test]
    fn the_flat_preset_is_the_proven_neutral_configuration() {
        assert_eq!(
            EqPreset::Flat.to_config(),
            AudioProcessingConfig::eq(EqConfig::FLAT),
        );
    }

    /// The CLI-side parser: every preset name round-trips; unknown,
    /// mixed-case and empty names are refused (the flag's value
    /// grammar).
    #[test]
    fn from_name_round_trips_every_preset_and_refuses_the_rest() {
        for preset in EqPreset::all() {
            let name = format!("{:?}", preset).to_lowercase();
            assert_eq!(EqPreset::from_name(&name), Some(preset), "{name}");
        }
        for bad in ["", "Off", "FLAT", "RocknRoll", "bass boost", "custom"] {
            assert_eq!(EqPreset::from_name(bad), None, "{bad:?}");
        }
    }

    /// `name()` is the exact inverse of `from_name` for every preset:
    /// one vocabulary, no drift between the parse table and the
    /// presentation table.
    #[test]
    fn name_is_the_exact_inverse_of_from_name() {
        for preset in EqPreset::all() {
            assert_eq!(EqPreset::from_name(preset.name()), Some(preset));
            assert_eq!(preset.name(), format!("{:?}", preset).to_lowercase());
        }
    }
}
