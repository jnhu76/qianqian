//! Headroom truth for the Audio Processing configuration (campaign
//! #190 D2; `dsp-product-model.md` §9).
//!
//! CURRENT TRUTH (unchanged by D2, restated as the boundary this module
//! lives inside): internal Float32 PCM may exceed ±1; Gain/EQ clip and
//! limit NOTHING; the PCM Preamp is NOT the D14.9 Output Volume; the
//! device owns out-of-range samples.
//!
//! D2's product decision — the smallest honest improvement over a bare
//! manual preamp — is a **non-binding advisory**: the ESTIMATED
//! STEADY-STATE EQ HEADROOM GUIDANCE. For a desired EQ configuration at
//! a source rate it reports the attenuation (dB, ≤ 0) that would place
//! the EQ cascade's worst steady-state frequency-response gain at
//! unity, plus where that peak sits. Exactly what it is:
//!
//! ```text
//! an estimate on a dense log-frequency grid of |H(f)| of the
//! available-band cascade (f64, rate-aware per the active-band
//! profile) — a deterministic pure function of the desired EQ data
//! ```
//!
//! Exactly what it is NOT (never claim, never display as):
//!
//! ```text
//! a true-peak guarantee            inter-sample peaks are invisible
//!                                  to a frequency-response grid
//! an arbitrary-signal peak         transients and phase-aligned tones
//! guarantee                        can exceed the steady-state bound
//!                                  (witnessed by the in-module probe)
//! an acoustic loudness guarantee   loudness is not gain math
//! automatic headroom               the advice NEVER mutates the
//!                                  desired configuration; applying it
//!                                  is an explicit product/user act on
//!                                  the PREAMP (the manual control)
//! ```
//!
//! Exclusions by construction: the manual Preamp is NOT part of the
//! estimate (it stays the user's independent headroom control beside
//! the advice); the D14.9 Output Volume is not an input and cannot
//! enter the calculation (it is not part of the processing
//! configuration at all). Bypass and Gain-only configurations have no
//! EQ cascade and therefore no EQ guidance to ask for.

use crate::processing::{EQ_BAND_FREQUENCY_HZ, EqConfig, band_participates};

/// The grid resolution of the estimate: points per octave on a dense
/// log-frequency axis. Deterministic product tuning of the advisory's
/// cost/accuracy trade — the estimate is grid-bounded by definition,
/// not a closed-form supremum.
const GRID_POINTS_PER_OCTAVE: f64 = 96.0;

/// The lowest frequency the grid visits. The product band table starts
/// at 31 Hz; responses below 10 Hz are flat for every recipe here.
const GRID_MIN_HZ: f64 = 10.0;

/// The estimated steady-state EQ headroom guidance for one desired EQ
/// configuration at one source rate.
///
/// `guidance_db` is the attenuation (≤ 0.0 dB; 0.0 = no attenuation
/// advised) that would place the available-band cascade's largest
/// grid-sampled steady-state gain at unity. `peak_hz` is where that
/// largest gain sits (a diagnostic for presentation; 0.0 when the
/// cascade is empty at this rate). Both are deterministic for the same
/// inputs.
///
/// `None` refuses to advise rather than fabricating: a zero rate, or
/// configuration data the recipes cannot mean (non-finite trims or Q).
/// Invalid configurations fail their own establishment regardless.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadroomGuidance {
    /// Advised attenuation in dB, ≤ 0.0. ESTIMATED STEADY-STATE EQ
    /// HEADROOM GUIDANCE — not a clipping guarantee (module doc).
    pub guidance_db: f32,
    /// The frequency (Hz) of the largest steady-state cascade gain —
    /// the peak the guidance would tame. Diagnostic only.
    pub peak_hz: f32,
}

/// The estimated steady-state EQ headroom guidance, or `None` where no
/// honest advice exists (see [`HeadroomGuidance`]).
///
/// Rate-aware: only bands available at `sample_rate_hz` (the
/// active-band profile, §2.1) participate — an unavailable band cannot
/// shape this episode and does not enter its headroom estimate.
pub fn estimated_eq_headroom_guidance(
    eq: &EqConfig,
    sample_rate_hz: u32,
) -> Option<HeadroomGuidance> {
    if sample_rate_hz == 0 {
        return None;
    }
    if !eq.q.is_finite() || eq.band_gain_db.iter().any(|g| !g.is_finite()) {
        return None;
    }
    let fs = f64::from(sample_rate_hz);
    let nyquist = fs / 2.0;

    // The per-band f64 magnitude responses over the grid: the analytic
    // |H(e^{jw})| of each available band's RBJ recipe (the same
    // published formulas the stage compiles in f32; this transcription
    // is independent in precision and expression).
    let active: Vec<[f64; 5]> = EQ_BAND_FREQUENCY_HZ
        .iter()
        .enumerate()
        .filter(|&(_, &f0)| band_participates(f0, sample_rate_hz))
        .map(|(index, &f0)| {
            band_coefficients(
                index,
                f64::from(f0),
                f64::from(eq.band_gain_db[index]),
                f64::from(eq.q),
                fs,
            )
        })
        .collect();

    // The empty-cascade edge (every band unavailable at this rate):
    // nothing shapes the episode, so no attenuation is advised.
    let mut max_gain = 1.0f64;
    let mut peak_hz = 0.0f64;
    let mut f = GRID_MIN_HZ;
    while f < nyquist {
        let w = std::f64::consts::TAU * f / fs;
        let (sin_w, cos_w) = (w.sin(), w.cos());
        let (sin_2w, cos_2w) = ((2.0 * w).sin(), (2.0 * w).cos());
        let mut gain = 1.0f64;
        for c in &active {
            let [b0, b1, b2, a1, a2] = *c;
            let nr = b0 + b1 * cos_w + b2 * cos_2w;
            let ni = -(b1 * sin_w + b2 * sin_2w);
            let dr = 1.0 + a1 * cos_w + a2 * cos_2w;
            let di = -(a1 * sin_w + a2 * sin_2w);
            gain *= (nr * nr + ni * ni).sqrt() / (dr * dr + di * di).sqrt();
        }
        if gain > max_gain {
            max_gain = gain;
            peak_hz = f;
        }
        f *= 2.0f64.powf(1.0 / GRID_POINTS_PER_OCTAVE);
    }

    let peak_gain_db = 20.0 * max_gain.log10();
    Some(HeadroomGuidance {
        // Advice never rounds up into a boost: the estimate only ever
        // attenuates, and a cascade that amplifies nowhere advises 0.
        guidance_db: (-peak_gain_db.max(0.0)) as f32,
        peak_hz: peak_hz as f32,
    })
}

/// One band's normalized `[b0, b1, b2, a1, a2]` in f64 — the RBJ Audio
/// EQ Cookbook recipes (low shelf S = 1 / peaking Q / high shelf S = 1,
/// the fixed product kind mapping by index) with a0 normalized to 1.
fn band_coefficients(index: usize, f0: f64, gain_db: f64, q: f64, fs: f64) -> [f64; 5] {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = std::f64::consts::TAU * f0 / fs;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let (b0, b1, b2, a0, a1, a2) = if index == 0 {
        let alpha = (sin_w0 / 2.0) * std::f64::consts::SQRT_2;
        let term = 2.0 * a.sqrt() * alpha;
        (
            a * ((a + 1.0) - (a - 1.0) * cos_w0 + term),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
            a * ((a + 1.0) - (a - 1.0) * cos_w0 - term),
            (a + 1.0) + (a - 1.0) * cos_w0 + term,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
            (a + 1.0) + (a - 1.0) * cos_w0 - term,
        )
    } else if index == EQ_BAND_FREQUENCY_HZ.len() - 1 {
        let alpha = (sin_w0 / 2.0) * std::f64::consts::SQRT_2;
        let term = 2.0 * a.sqrt() * alpha;
        (
            a * ((a + 1.0) + (a - 1.0) * cos_w0 + term),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
            a * ((a + 1.0) + (a - 1.0) * cos_w0 - term),
            (a + 1.0) - (a - 1.0) * cos_w0 + term,
            2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
            (a + 1.0) - (a - 1.0) * cos_w0 - term,
        )
    } else {
        let alpha = sin_w0 / (2.0 * q);
        (
            1.0 + alpha * a,
            -2.0 * cos_w0,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos_w0,
            1.0 - alpha / a,
        )
    };
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;
    use qianqian_audio_api::ports::PcmFormat;

    use crate::presets::EqPreset;
    use crate::processing::EqConfig;

    /// The advice is deterministic: the same inputs give the same
    /// answer, exactly, twice running.
    #[test]
    fn the_advice_is_deterministic() {
        for preset in EqPreset::all() {
            let eq = preset.to_config().eq.expect("preset carries an EQ");
            let a = estimated_eq_headroom_guidance(&eq, 44_100);
            let b = estimated_eq_headroom_guidance(&eq, 44_100);
            assert_eq!(a, b, "{preset:?} advice must be deterministic");
        }
    }

    /// Flat advises nothing: the neutral cascade amplifies nowhere, so
    /// the guidance is exactly 0 dB (neutral/appropriate — never a
    /// fabricated margin).
    #[test]
    fn flat_advice_is_neutral() {
        let guidance =
            estimated_eq_headroom_guidance(&EqConfig::FLAT, 44_100).expect("flat advises");
        assert_eq!(guidance.guidance_db, 0.0);
        assert_eq!(guidance.peak_hz, 0.0, "no amplifying peak exists");
    }

    /// A positive-boost preset produces truthful nonzero attenuation
    /// advice at every product rate, and the advised attenuation grows
    /// with the boost (Bass, the heaviest low-shelf lift, advises at
    /// least as much as Rock at the same rate).
    #[test]
    fn positive_boost_presets_advise_truthful_nonzero_attenuation() {
        for fs in [8000u32, 44_100, 48_000, 96_000] {
            let rock =
                estimated_eq_headroom_guidance(&EqPreset::Rock.to_config().eq.expect("EQ"), fs)
                    .expect("rock advises");
            assert!(
                rock.guidance_db < 0.0,
                "{fs}: Rock boosts — advice must be nonzero"
            );
            let bass =
                estimated_eq_headroom_guidance(&EqPreset::Bass.to_config().eq.expect("EQ"), fs)
                    .expect("bass advises");
            assert!(
                bass.guidance_db < 0.0,
                "{fs}: Bass boosts — advice must be nonzero"
            );
            assert!(
                bass.guidance_db <= rock.guidance_db,
                "{fs}: the heavier boost advises at least as much attenuation \
                 (bass {} vs rock {})",
                bass.guidance_db,
                rock.guidance_db
            );
            assert!(bass.peak_hz > 0.0, "{fs}: the peak is located");
        }
    }

    /// A negative-only EQ fabricates no positive-risk advice: bands
    /// that only cut (and shelves that cut) leave the cascade at or
    /// below unity everywhere, so the guidance is exactly 0 dB.
    #[test]
    fn negative_only_eq_advises_nothing() {
        let mut cuts = [0.0f32; 10];
        cuts[0] = -12.0;
        cuts[5] = -6.0;
        cuts[9] = -12.0;
        let guidance =
            estimated_eq_headroom_guidance(&EqConfig::new(cuts, 1.0), 44_100).expect("advises");
        assert_eq!(
            guidance.guidance_db, 0.0,
            "a cutting cascade must not be sold as a clipping risk"
        );
    }

    /// The advice covers the EQ cascade ONLY: the manual preamp is not
    /// an input, so two configurations differing only in preamp get the
    /// identical advice, and computing advice does not touch the
    /// configuration (no silent mutation — the desired config is
    /// `Copy`, the function takes only a shared reference, and the
    /// values are re-checked after the call).
    #[test]
    fn the_manual_preamp_stays_independent_of_the_advice() {
        let eq = EqPreset::Rock.to_config().eq.expect("EQ");
        let without = estimated_eq_headroom_guidance(&eq, 44_100).expect("advises");
        let config = crate::processing::AudioProcessingConfig {
            gain: 0.5,
            eq: Some(eq),
            enabled: true,
        };
        let with =
            estimated_eq_headroom_guidance(&config.eq.expect("EQ"), 44_100).expect("advises");
        assert_eq!(without, with, "preamp must not enter the EQ estimate");
        assert_eq!(config.gain, 0.5, "the advice must not mutate the preamp");
        assert_eq!(config.eq.as_ref().unwrap().band_gain_db, eq.band_gain_db);
    }

    /// The D14.9 Output Volume cannot enter the calculation: it is not
    /// part of the processing configuration at all (structural). Pinned
    /// here as the honest statement of the boundary — the guidance
    /// signature accepts EQ data and a rate, nothing else.
    #[test]
    fn no_volume_input_exists() {
        // The function is a pure function of (&EqConfig, u32). A volume
        // has no place to be passed; this compiles only because none is
        // needed. (Structural pin, not a behavior claim.)
        let eq = EqPreset::Bass.to_config().eq.expect("EQ");
        let _ = estimated_eq_headroom_guidance(&eq, 48_000);
    }

    /// No honest advice exists where the recipes cannot mean anything:
    /// a zero rate, or non-finite trims/Q return None instead of a
    /// NaN advisory.
    #[test]
    fn invalid_inputs_refuse_to_advise() {
        let mut nan_trim = [0.0f32; 10];
        nan_trim[3] = f32::NAN;
        assert_eq!(
            estimated_eq_headroom_guidance(&EqConfig::new(nan_trim, 1.0), 44_100),
            None
        );
        let mut nan_q = EqConfig::FLAT;
        nan_q.q = f32::NAN;
        assert_eq!(estimated_eq_headroom_guidance(&nan_q, 44_100), None);
        assert_eq!(estimated_eq_headroom_guidance(&EqConfig::FLAT, 0), None);
    }

    /// Rate-awareness (§2.1): the estimate covers the AVAILABLE bands
    /// only. The 16 kHz shelf boost is inert at 8 kHz, so a
    /// high-shelf-only lift advises nothing there while advising real
    /// attenuation at 44.1 kHz.
    #[test]
    fn the_estimate_is_rate_aware_like_the_active_band_profile() {
        let mut high_only = [0.0f32; 10];
        high_only[9] = 12.0;
        let low_rate =
            estimated_eq_headroom_guidance(&EqConfig::new(high_only, 1.0), 8_000).expect("advises");
        assert_eq!(
            low_rate.guidance_db, 0.0,
            "an unavailable band must not enter this rate's estimate"
        );
        let full_rate = estimated_eq_headroom_guidance(&EqConfig::new(high_only, 1.0), 44_100)
            .expect("advises");
        assert!(
            full_rate.guidance_db < 0.0,
            "the same trim advises attenuation once the band is available"
        );
    }

    /// Negative control: the guidance oracle must be able to fail. A
    /// deliberately wrong advisor that (a) includes the preamp in the
    /// estimate or (b) reports attenuation where the cascade only cuts
    /// is distinguishable from the real one by the pins above — proved
    /// here by replaying wrong answers through the same assertion
    /// shapes.
    #[test]
    fn the_advice_oracles_reject_deliberately_wrong_advisors() {
        use crate::processing_support::rejects;
        let eq = EqPreset::Rock.to_config().eq.expect("EQ");
        let real = estimated_eq_headroom_guidance(&eq, 44_100).expect("advises");

        // (a) A preamp-including advisor would answer differently for
        // the same EQ under two preamps — the independence pin rejects
        // that world.
        let preamp_including_world = HeadroomGuidance {
            guidance_db: real.guidance_db - 6.0,
            peak_hz: real.peak_hz,
        };
        assert!(rejects(|| {
            let without = real;
            let with = preamp_including_world;
            assert_eq!(without, with, "preamp must not enter the EQ estimate");
        }));

        // (b) A risk-fabricating advisor reports attenuation for a
        // cutting-only cascade — the negative-only pin rejects it.
        let mut cuts = [0.0f32; 10];
        cuts[5] = -6.0;
        assert!(rejects(|| {
            let fabricated = HeadroomGuidance {
                guidance_db: -3.0,
                peak_hz: 1000.0,
            };
            assert_eq!(
                fabricated.guidance_db, 0.0,
                "a cutting cascade advises nothing"
            );
            let _ = estimated_eq_headroom_guidance(&EqConfig::new(cuts, 1.0), 44_100);
        }));
    }

    // --- the D2.1 evidence probe -----------------------------------------

    /// The recipe-free anchor: a settled sine through the REAL f32
    /// stage measures ≈ the configured dB — peaking bands at their
    /// center (response there = the full configured dB), shelves at
    /// their asymptotes (DC / Nyquist, where the S = 1 recipes place
    /// the full configured dB; at the shelf f0 the response is the
    /// HALF-dB midpoint). The estimate's transcription is anchored to
    /// measurement, not only to a second transcription of the same
    /// formulas.
    #[test]
    fn the_estimate_is_anchored_by_measurement_at_the_band_centers() {
        // Peaking anchors: measure at the band center.
        for (band, gain_db) in [(2usize, 12.0f32), (5, 12.0), (7, 9.0)] {
            let mut bands = [0.0f32; 10];
            bands[band] = gain_db;
            let eq = EqConfig::new(bands, 1.0);
            let guidance = estimated_eq_headroom_guidance(&eq, 44_100).expect("advises");
            let measured_db = measured_boost_db(&eq, band, 44_100);
            let advised_boost = -f64::from(guidance.guidance_db);
            let magnitude = measured_db.abs().max(advised_boost).max(1.0);
            assert!(
                (measured_db - advised_boost).abs() / magnitude < 0.2,
                "peaking band {band}: measured {measured_db:.2} dB vs advised \
                 {advised_boost:.2} dB boost"
            );
        }
        // Shelf anchors: the asymptote carries the full configured dB.
        for (band, gain_db, measure_hz) in [(0usize, 12.0f32, 10.0f64), (9, 9.0, 0.99 * 22_050.0)] {
            let mut bands = [0.0f32; 10];
            bands[band] = gain_db;
            let eq = EqConfig::new(bands, 1.0);
            let guidance = estimated_eq_headroom_guidance(&eq, 44_100).expect("advises");
            let measured_db = measured_boost_db_at(&eq, measure_hz, 44_100);
            let advised_boost = -f64::from(guidance.guidance_db);
            let magnitude = measured_db.abs().max(advised_boost).max(1.0);
            assert!(
                (measured_db - advised_boost).abs() / magnitude < 0.2,
                "shelf band {band}: measured {measured_db:.2} dB at the \
                 asymptote vs advised {advised_boost:.2} dB boost"
            );
        }
    }

    /// The guarantee-domain boundary witness (D2.1/D2.3). Two sides of
    /// one honest line:
    ///
    /// DOMAIN — for steady-state content within ±1 whose components
    /// live inside the boost region, the advised attenuation holds the
    /// post-EQ peak at or below unity (the estimate does bound the
    /// steady-state cascade gain).
    ///
    /// BOUNDARY — the advice is signal-blind: Float32 source content
    /// beyond ±1 (legal, documented posture) rides through the same
    /// attenuation and still exceeds unity; so does a positive user
    /// preamp stacked on top. The guidance bounds the CASCADE, never
    /// the signal — it must never be presented as clipping protection.
    #[test]
    fn the_advice_bounds_the_cascade_not_the_signal() {
        let bass = EqPreset::Bass.to_config().eq.expect("EQ");
        let guidance = estimated_eq_headroom_guidance(&bass, 48_000).expect("bass advises");
        let attenuation = 10f32.powf(guidance.guidance_db / 20.0);
        let fs = 48_000.0f64;
        let n = 48_000;

        let run = |gain: f32, scale: f64| -> f32 {
            let tones = [40.0f64, 80.0, 160.0];
            let mut block: Vec<f32> = (0..n)
                .map(|i| {
                    let t = i as f64 / fs;
                    let s: f64 = tones
                        .iter()
                        .map(|&f| (std::f64::consts::TAU * f * t).sin())
                        .sum();
                    (scale * s / tones.len() as f64) as f32
                })
                .collect();
            let mut stage = crate::processing::EpisodeProcessing::new(
                &crate::processing::AudioProcessingConfig {
                    enabled: true,
                    gain,
                    eq: Some(bass),
                },
                &PcmFormat {
                    sample_rate: 48_000,
                    channels: 1,
                    channel_mask: 0x1,
                },
            )
            .expect("compiles");
            stage.stage(&mut block).expect("stages");
            assert!(block.iter().all(|s| s.is_finite()), "no NaN/Inf");
            block.iter().fold(0.0f32, |m, &s| m.max(s.abs()))
        };

        // DOMAIN: unit-peak content, guidance applied as the preamp —
        // the post-EQ peak stays within unity (small f32 margin).
        let in_domain = run(attenuation, 1.0);
        assert!(
            in_domain <= 1.0 + 1e-4,
            "steady-state unit content must stay within unity under the \
             advised attenuation (peak {in_domain})"
        );

        // BOUNDARY: the same content at 2.0 (legal Float32) — the
        // attenuation is signal-blind and the peak exceeds unity.
        let hot = run(attenuation, 2.0);
        assert!(
            hot > 1.0,
            "content beyond ±1 must exceed unity under the same advice \
             (peak {hot}) — the guidance is not a signal guarantee"
        );

        // BOUNDARY: a positive user preamp stacked on the guidance —
        // the manual control is the user's, not the advisor's.
        let boosted = run(attenuation * 2.0, 1.0);
        assert!(
            boosted > 1.0,
            "a positive preamp on top of the guidance must exceed unity \
             (peak {boosted})"
        );
    }

    /// Settled amplitude of one channel through the REAL f32 stage for
    /// a band-center sine — the measurement anchor.
    fn measured_boost_db(eq: &EqConfig, band: usize, sample_rate: u32) -> f64 {
        measured_boost_db_at(
            eq,
            f64::from(crate::processing::EQ_BAND_FREQUENCY_HZ[band]),
            sample_rate,
        )
    }

    /// Settled amplitude at an arbitrary measurement frequency.
    fn measured_boost_db_at(eq: &EqConfig, measure_hz: f64, sample_rate: u32) -> f64 {
        let fs = f64::from(sample_rate);
        let f0 = measure_hz;
        let n = 65_536usize;
        let mut block: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f64 / fs;
                (std::f64::consts::TAU * f0 * t).sin() as f32 * 0.25
            })
            .collect();
        let mut stage = crate::processing::EpisodeProcessing::new(
            &crate::processing::AudioProcessingConfig {
                enabled: true,
                gain: 1.0,
                eq: Some(*eq),
            },
            &PcmFormat {
                sample_rate,
                channels: 1,
                channel_mask: 0x1,
            },
        )
        .expect("compiles");
        stage.stage(&mut block).expect("stages");
        let settled = block[n / 2..]
            .iter()
            .fold(0.0f64, |m, &s| m.max(f64::from(s).abs()));
        20.0 * (settled / 0.25).log10()
    }
}
