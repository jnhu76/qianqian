//! I3 Production EQ oracles (Issue #177 Stage 2 / I3; ADR-PBK-002
//! D14.11). Two layers:
//!
//! STAGE LEVEL — the DSP mathematics: the compiled cascade is checked
//! against an independent f64 re-derivation of the RBJ Audio EQ
//! Cookbook recipes and against analytic frequency responses, plus the
//! stateful properties the I2 probe proved for the transport
//! (fragmentation invariance, per-channel independence, finite output
//! at maximum product settings, invalid-configuration rejection).
//!
//! COMPOSITION LEVEL — the production path: the config-carrying
//! establishment compiles the EQ against the episode's source format,
//! the cascade runs at the frozen decode-worker staging placement, and
//! the full D14.11 lifecycle matrix (seek refusal/applied/destructive,
//! pause, Open/replacement, EOF frame conservation) is rerun with real
//! signal-derived EQ state — the state the I2 probe proved the seam
//! carries.
//!
//! Tolerances are justified, not guessed: the f64 reference is
//! independent of the pipeline in PRECISION (f64) and RECURRENCE
//! STRUCTURE (DF1 vs TDF2), so the pipeline-vs-reference difference is
//! bounded by f32 coefficient rounding (~1e-7 relative) contracted by
//! the recursion (|poles| < 1), giving orders of magnitude of margin
//! at 1e-3 relative. The transcription's own fidelity is closed
//! separately: by the external cookbook comparison and by the
//! recipe-free design-target assertions (a band-center sine measures
//! the configured dB; a shelf's DC measures the configured dB). The FLAT equality
//! needs no tolerance at all: with every band at 0 dB the normalized
//! numerator and denominator coefficients coincide exactly, the state
//! stays at rest, and the cascade is the identity BIT-EXACTLY — the
//! strongest product oracle (I4's Flat preset rides on this).

use std::time::{Duration, Instant};

use qianqian_audio_api::ports::PcmFormat;
use qianqian_audio_api::ports::ProviderSeekOutcome;

use crate::handle::EpisodeTerminalOutcome;
use crate::presets::EqPreset;
use crate::processing::{
    AudioProcessingConfig, EQ_BAND_FREQUENCY_HZ, EQ_MAX_BAND_GAIN_DB, EpisodeProcessing, EqConfig,
    EqStage,
};
use crate::processing_support::{
    EIGHT_SECONDS, TEST_RATE, assert_processed_exactly_at, episode, rejects, wait_until,
};
use crate::test_common::{self, OutputBehavior, TEST_FORMAT};

const TEST_FS: f64 = 44_100.0;

fn test_format() -> PcmFormat {
    TEST_FORMAT
}

/// A config with one boosted/cut band and every other band neutral.
fn eq_config_one_band(band: usize, gain_db: f32) -> EqConfig {
    let mut bands = [0.0f32; 10];
    bands[band] = gain_db;
    EqConfig::new(bands, 1.0)
}

fn eq_config_all(gain_db: f32) -> EqConfig {
    EqConfig::new([gain_db; 10], 1.0)
}

// --- the independent f64 oracle ------------------------------------------

/// The RBJ Audio EQ Cookbook recipes transcribed in f64 (separate from
/// the production f32 compilation in precision and expression; the
/// transcription's fidelity to the PUBLISHED cookbook is additionally
/// witnessed by the recipe-free design-target assertions below).
/// Returns the normalized `[b0, b1, b2, a1, a2]`.
fn reference_coefficients(index: usize, f0: f64, gain_db: f64, q: f64, fs: f64) -> [f64; 5] {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = std::f64::consts::TAU * f0 / fs;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let (b0, b1, b2, a0, a1, a2) = if index == 0 {
        // Low shelf, S = 1.
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
        // High shelf, S = 1.
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
        // Peaking EQ.
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

/// Direct Form I in f64: one channel's stream through one band.
fn reference_df1(input: &[f64], c: &[f64; 5]) -> Vec<f64> {
    let [b0, b1, b2, a1, a2] = *c;
    let mut x1 = 0.0;
    let mut x2 = 0.0;
    let mut y1 = 0.0;
    let mut y2 = 0.0;
    input
        .iter()
        .map(|&x0| {
            let y0 = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
            x2 = x1;
            x1 = x0;
            y2 = y1;
            y1 = y0;
            y0
        })
        .collect()
}

/// The full cascade reference for one channel: the 10 bands in fixed
/// order, each a f64 DF1 over the recipe's own f64 coefficients.
fn reference_cascade(input: &[f64], config: &EqConfig, fs: f64) -> Vec<f64> {
    let mut signal = input.to_vec();
    for (index, &f0) in EQ_BAND_FREQUENCY_HZ.iter().enumerate() {
        let coefficients = reference_coefficients(
            index,
            f64::from(f0),
            f64::from(config.band_gain_db[index]),
            f64::from(config.q),
            fs,
        );
        signal = reference_df1(&signal, &coefficients);
    }
    signal
}

/// The analytic magnitude response |H(e^{jw})| in dB of one band's
/// normalized coefficients — DSP mathematics, independent of any
/// recursion.
fn analytic_gain_db(c: &[f64; 5], f: f64, fs: f64) -> f64 {
    let [b0, b1, b2, a1, a2] = *c;
    let w = std::f64::consts::TAU * f / fs;
    let (sin_w, cos_w) = (w.sin(), w.cos());
    let (sin_2w, cos_2w) = ((2.0 * w).sin(), (2.0 * w).cos());
    let nr = b0 + b1 * cos_w + b2 * cos_2w;
    let ni = -(b1 * sin_w + b2 * sin_2w);
    let dr = 1.0 + a1 * cos_w + a2 * cos_2w;
    let di = -(a1 * sin_w + a2 * sin_2w);
    20.0 * ((nr * nr + ni * ni).sqrt() / (dr * dr + di * di).sqrt()).log10()
}

/// The steady-state amplitude of a settled sinusoid: max |y| over the
/// last half of the stream.
fn settled_amplitude(signal: &[f64]) -> f64 {
    signal[signal.len() / 2..]
        .iter()
        .fold(0.0f64, |m, &y| m.max(y.abs()))
}

// --- stage level: the DSP mathematics -------------------------------------

/// FLAT is the identity BIT-EXACTLY: with every band at 0 dB the
/// normalized numerator and denominator coefficients coincide, the
/// TDF2 state stays at rest, and `y[n] = x[n]` exactly — no tolerance
/// needed or allowed. This is the oracle I4's Flat preset rides on.
#[test]
fn flat_eq_is_bit_exact_pass_through() {
    let mut stage = EqStage::new(&EqConfig::FLAT, &test_format()).expect("flat compiles");
    // Non-trivial content: the position-tag ramp past f32-exact small
    // integers, negative values, and sub-unity magnitudes.
    let mut block: Vec<f32> = (0..4096)
        .map(|i| {
            let x = i as f32;
            if i % 3 == 0 {
                -x
            } else if i % 5 == 0 {
                x * 0.001
            } else {
                x
            }
        })
        .collect();
    let expected = block.clone();
    stage.stage(&mut block);
    assert_eq!(
        block, expected,
        "a flat EQ must be the identity filter bit-exactly"
    );
}

/// The compiled cascade matches an independent f64 re-derivation of the
/// same recipes driven through a Direct Form I difference equation:
/// impulse response, sample for sample, within the f32 rounding bound
/// (relative tolerance 1e-3 — over three orders of magnitude above the
/// ~1e-7 coefficient rounding, contracted further by the stable
/// recursion).
#[test]
fn impulse_response_matches_the_independent_difference_equation() {
    let config = eq_config_one_band(5, 12.0);
    let mut stage = EqStage::new(&config, &test_format()).expect("compiles");
    let frames = 512;
    // Interleaved stereo impulse: channel-distinct tags on the nonzero
    // sample so each channel's response is independently witnessed.
    let mut input = vec![0.0f32; frames * 2];
    input[0] = 1.0;
    input[1] = 1.5;
    stage.stage(&mut input);

    for (channel, amplitude) in [(0usize, 1.0f64), (1usize, 1.5f64)] {
        let mut impulse = vec![0.0f64; frames];
        impulse[0] = amplitude;
        let expected = reference_cascade(&impulse, &config, TEST_FS);
        let actual: Vec<f64> = input[channel..]
            .chunks(2)
            .map(|f| f64::from(f[0]))
            .collect();
        let peak = expected.iter().fold(0.0f64, |m, &y| m.max(y.abs()));
        for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
            assert!(
                (a - e).abs() <= 1e-3 * peak,
                "impulse response frame {i} of channel {channel}: {a} vs \
                 reference {e}"
            );
        }
    }
}

/// A sine at a boosted band's center emerges at the configured gain;
/// sines well below and well above pass near unity — measured
/// steady-state amplitudes against the ANALYTIC response of the
/// independent f64 coefficients (±0.5 dB; the design target at the
/// band center is |H(e^{jw0})| = A² — the FULL configured dB). Channel independence
/// rides on the same run: the two channels carry different probe
/// frequencies and each matches its own analytic response.
#[test]
fn sine_responses_match_the_analytic_frequency_response() {
    let band = 5usize; // 1 kHz peaking
    let configured_db = 12.0f32;
    let config = eq_config_one_band(band, configured_db);
    let fs = TEST_FS;
    let duration_frames = TEST_RATE; // 1 s

    let mut stage = EqStage::new(&config, &test_format()).expect("compiles");
    let mut input = Vec::with_capacity(duration_frames * 2);
    for n in 0..duration_frames {
        let t = f64::from(n as u32) / fs;
        // Channel 0 probes the band center; channel 1 probes far below
        // it — two independent measurements in one pass.
        input.push(((std::f64::consts::TAU * 1000.0 * t).sin()) as f32);
        input.push(((std::f64::consts::TAU * 125.0 * t).sin()) as f32);
    }
    stage.stage(&mut input);
    let out_ch0: Vec<f64> = input.chunks(2).map(|fr| f64::from(fr[0])).collect();
    let out_ch1: Vec<f64> = input.chunks(2).map(|fr| f64::from(fr[1])).collect();

    // Band center: the analytic response of the whole cascade is the
    // boosted band's own response (all other bands are exact
    // identities at 0 dB).
    let coefficients = reference_coefficients(band, 1000.0, f64::from(configured_db), 1.0, fs);
    let analytic_center = analytic_gain_db(&coefficients, 1000.0, fs);
    let measured_center = 20.0 * settled_amplitude(&out_ch0).log10();
    assert!(
        (measured_center - analytic_center).abs() < 0.5,
        "band-center sine: measured {measured_center:.3} dB vs analytic \
         {analytic_center:.3} dB"
    );
    // Recipe-free anchor (the design target is A² = the FULL configured
    // dB, not half of it): the measured center response must equal the
    // configured trim without consulting the reference transcription.
    assert!(
        (measured_center - f64::from(configured_db)).abs() < 0.5,
        "band-center sine: measured {measured_center:.3} dB vs the \
         configured {configured_db} dB"
    );

    // Far below the band: near unity.
    let analytic_below = analytic_gain_db(&coefficients, 125.0, fs);
    let measured_below = 20.0 * settled_amplitude(&out_ch1).log10();
    assert!(
        (measured_below - analytic_below).abs() < 0.5,
        "below-band sine: measured {measured_below:.3} dB vs analytic \
         {analytic_below:.3} dB"
    );

    // Far above the band: near unity.
    let mut stage = EqStage::new(&config, &test_format()).expect("compiles");
    let mut input = Vec::with_capacity(duration_frames * 2);
    for n in 0..duration_frames {
        let t = f64::from(n as u32) / fs;
        input.push(((std::f64::consts::TAU * 8000.0 * t).sin()) as f32);
        input.push(0.0f32);
    }
    stage.stage(&mut input);
    let out_above: Vec<f64> = input.chunks(2).map(|fr| f64::from(fr[0])).collect();
    let analytic_above = analytic_gain_db(&coefficients, 8000.0, fs);
    let measured_above = 20.0 * settled_amplitude(&out_above).log10();
    assert!(
        (measured_above - analytic_above).abs() < 0.5,
        "above-band sine: measured {measured_above:.3} dB vs analytic \
         {analytic_above:.3} dB"
    );
}

/// The shelf extremes: a +12 dB low shelf lifts DC/low content by ~12 dB
/// and leaves high content near unity (measured against the same
/// analytic oracle).
#[test]
fn shelves_shape_the_extremes() {
    let config = eq_config_one_band(0, 12.0); // 31 Hz low shelf
    let fs = TEST_FS;
    let duration_frames = TEST_RATE;

    let run = |f: f64| -> f64 {
        let mut stage = EqStage::new(&config, &test_format()).expect("compiles");
        let mut input = Vec::with_capacity(duration_frames * 2);
        for n in 0..duration_frames {
            let t = f64::from(n as u32) / fs;
            let x = if f == 0.0 {
                1.0 // DC
            } else {
                (std::f64::consts::TAU * f * t).sin()
            };
            input.push(x as f32);
            input.push(x as f32);
        }
        stage.stage(&mut input);
        if f == 0.0 {
            // DC gain: the settled MEAN.
            let tail = &input[input.len() / 2..];
            let mean = tail.iter().sum::<f32>() / tail.len() as f32;
            20.0 * f64::from(mean).abs().log10()
        } else {
            20.0 * settled_amplitude(
                &input
                    .chunks(2)
                    .map(|fr| f64::from(fr[0]))
                    .collect::<Vec<_>>(),
            )
            .log10()
        }
    };

    let coefficients = reference_coefficients(0, 31.0, 12.0, 1.0, fs);
    let analytic_low = analytic_gain_db(&coefficients, 20.0, fs);
    let measured_low = run(20.0);
    assert!(
        (measured_low - analytic_low).abs() < 0.5,
        "low-frequency shelf lift: measured {measured_low:.3} dB vs \
         analytic {analytic_low:.3} dB"
    );

    let analytic_high = analytic_gain_db(&coefficients, 8000.0, fs);
    let measured_high = run(8000.0);
    assert!(
        (measured_high - analytic_high).abs() < 0.5,
        "high-frequency shelf passthrough: measured {measured_high:.3} dB \
         vs analytic {analytic_high:.3} dB"
    );

    // DC through a low shelf is A² — the FULL configured dB (the
    // recipe's A = 10^(dB/40) maps the shelf's asymptote to A²).
    let analytic_dc = 20.0 * (10f64.powf(12.0 / 20.0)).log10();
    let measured_dc = run(0.0);
    assert!(
        (measured_dc - analytic_dc).abs() < 0.5,
        "DC shelf gain: measured {measured_dc:.3} dB vs analytic \
         {analytic_dc:.3} dB"
    );
}

/// The cascade is invariant under staging fragmentation, BIT-EXACT —
/// the TDF2 per-sample operation sequence is identical under any
/// grouping (the property the I2 probe proved for the transport, now
/// for the production stateful processor).
#[test]
fn eq_cascade_is_invariant_under_staging_fragmentation() {
    let config = eq_config_all(6.0);
    let frames = 4_096usize;
    let input: Vec<f32> = (0..frames * 2).map(|i| (i as f32) * 0.5 - 1000.0).collect();

    let mut whole = input.clone();
    let mut whole_stage = EqStage::new(&config, &test_format()).expect("compiles");
    whole_stage.stage(&mut whole);

    let mut fragmented_stage = EqStage::new(&config, &test_format()).expect("compiles");
    let mut fragmented = Vec::with_capacity(input.len());
    let fragment_frames = [1usize, 7, 31, 64, 128, 257, 1024];
    let mut off = 0usize;
    for f in fragment_frames {
        let take = (f * 2).min(input.len() - off);
        let mut piece = input[off..off + take].to_vec();
        fragmented_stage.stage(&mut piece);
        fragmented.extend_from_slice(&piece);
        off += take;
    }
    let mut rest = input[off..].to_vec();
    fragmented_stage.stage(&mut rest);
    fragmented.extend_from_slice(&rest);

    assert_eq!(
        fragmented, whole,
        "the EQ cascade must be bit-exactly invariant under staging \
         fragmentation"
    );
}

/// A realistic maximum (+18 dB on all ten bands) over full-scale-ish
/// content stays FINITE: no NaN, no Inf — the internal Float32 range
/// carries positive EQ sums unclipped and unlimitied (the I3.6
/// headroom posture, proven not claimed).
#[test]
fn maximum_settings_produce_finite_output() {
    let config = eq_config_all(EQ_MAX_BAND_GAIN_DB);
    let mut stage = EqStage::new(&config, &test_format()).expect("compiles");
    let mut block: Vec<f32> = (0..8192)
        .map(|i| ((i % 97) as f32 / 97.0) * 2.0 - 1.0)
        .collect();
    stage.stage(&mut block);
    for (i, sample) in block.iter().enumerate() {
        assert!(
            sample.is_finite(),
            "sample {i} is non-finite at maximum product settings"
        );
    }
    // Unclipped by construction; with +18 dB on ten overlapping bands
    // the sum exceeds ±1.0 — that is the documented posture, and the
    // oracle asserts the signature exists (at least one sample above
    // unity) so "finite" cannot be mistaken for "quietly limited".
    assert!(
        block.iter().any(|s| s.abs() > 1.0),
        "the +18 dB cascade should exceed unity somewhere (no limiter)"
    );
}

/// Invalid configurations fail compilation (I3.5): non-positive/NaN Q,
/// non-finite or out-of-bound band gains, a zero sample rate — at any
/// source rate. Rate availability is NOT invalidity (the rate-aware
/// active-band profile makes unavailable bands inert instead); invalid
/// DATA still refuses. Nothing is clamped; nothing poisons the state.
#[test]
fn invalid_eq_configurations_fail_compilation() {
    // Non-positive Q.
    assert!(EqStage::new(&EqConfig::new([0.0; 10], 0.0), &test_format()).is_err());
    assert!(EqStage::new(&EqConfig::new([0.0; 10], -1.0), &test_format()).is_err());
    assert!(EqStage::new(&EqConfig::new([0.0; 10], f32::NAN), &test_format()).is_err());
    // Non-finite and out-of-bound band gains.
    let mut nan_gain = [0.0f32; 10];
    nan_gain[3] = f32::NAN;
    assert!(EqStage::new(&EqConfig::new(nan_gain, 1.0), &test_format()).is_err());
    let mut huge_gain = [0.0f32; 10];
    huge_gain[3] = EQ_MAX_BAND_GAIN_DB + 0.5;
    assert!(EqStage::new(&EqConfig::new(huge_gain, 1.0), &test_format()).is_err());
    // Zero sample rate.
    assert!(
        EqStage::new(
            &EqConfig::FLAT,
            &PcmFormat {
                sample_rate: 0,
                channels: 2,
                channel_mask: 0x3
            }
        )
        .is_err()
    );
    // Format-dependent INVALIDITY is data, not rate availability: at a
    // low source rate the unavailable bands are inert (the rate-aware
    // active-band profile, dsp-product-model.md §2.1), but NaN/out-of-bound
    // band DATA still fails compilation exactly as at any other rate —
    // on an AVAILABLE band and on an UNAVAILABLE band alike (intrinsic
    // validation is rate-independent; invalid data is not rendered
    // harmless by the band being unavailable).
    let mut low_rate_nan = [0.0f32; 10];
    low_rate_nan[2] = f32::NAN;
    let low_rate = PcmFormat {
        sample_rate: 8000,
        channels: 2,
        channel_mask: 0x3,
    };
    assert!(
        EqStage::new(&EqConfig::new(low_rate_nan, 1.0), &low_rate).is_err(),
        "invalid band data on an available band fails at a low rate too"
    );
    let mut unavailable_nan = [0.0f32; 10];
    unavailable_nan[9] = f32::NAN; // the 16 kHz band, unavailable at 8 kHz
    assert!(
        EqStage::new(&EqConfig::new(unavailable_nan, 1.0), &low_rate).is_err(),
        "invalid band data stays invalid on an unavailable band"
    );
}

// --- D1: the rate-aware active-band profile (dsp-product-model.md §2.1) ---

/// A stereo source format at an arbitrary product rate.
fn format_at(sample_rate: u32) -> PcmFormat {
    PcmFormat {
        sample_rate,
        channels: 2,
        channel_mask: 0x3,
    }
}

/// The accepted availability rule, stated independently: a fixed product
/// band participates in an episode's cascade iff its center is strictly
/// below that source's Nyquist frequency. The oracles derive the
/// expected active set from THIS function, never from the production
/// code path under test.
fn participates(center_hz: f64, sample_rate_hz: f64) -> bool {
    center_hz < sample_rate_hz / 2.0
}

/// The expected active-band index set at a source rate, per the
/// accepted rule.
fn expected_active_bands(sample_rate: u32) -> Vec<usize> {
    EQ_BAND_FREQUENCY_HZ
        .iter()
        .enumerate()
        .filter(|&(_, &f0)| participates(f64::from(f0), f64::from(sample_rate)))
        .map(|(index, _)| index)
        .collect()
}

/// The f64 reference cascade over an ARBITRARY availability rule (the
/// generalizer the negative controls use to represent wrong-rule worlds;
/// the accepted-rule cascade is the `accepted_rule` specialization).
fn reference_cascade_under(
    input: &[f64],
    config: &EqConfig,
    fs: f64,
    rule: impl Fn(f64, f64) -> bool,
) -> Vec<f64> {
    let mut signal = input.to_vec();
    for (index, &f0) in EQ_BAND_FREQUENCY_HZ.iter().enumerate() {
        if !rule(f64::from(f0), fs) {
            continue;
        }
        let coefficients = reference_coefficients(
            index,
            f64::from(f0),
            f64::from(config.band_gain_db[index]),
            f64::from(config.q),
            fs,
        );
        signal = reference_df1(&signal, &coefficients);
    }
    signal
}

/// The accepted-rule reference cascade: the oracle the production stage
/// is compared against at every rate.
fn reference_cascade_active(input: &[f64], config: &EqConfig, fs: f64) -> Vec<f64> {
    reference_cascade_under(input, config, fs, participates)
}

/// The stage compiles the RATE-AWARE ACTIVE-BAND PROFILE: at every
/// product rate the cascade carries exactly the bands strictly below
/// that source's Nyquist frequency — never the old whole-EQ refusal
/// (which made Flat fail where Processing Off succeeded), never an
/// inclusive rule that would compile a degenerate exactly-Nyquist band.
#[test]
fn the_stage_compiles_the_rate_active_band_profile_at_every_product_rate() {
    for fs in [8000u32, 16000, 22050, 32000, 44100, 48000, 96000] {
        let stage = EqStage::new(&EqConfig::FLAT, &format_at(fs))
            .unwrap_or_else(|e| panic!("{fs} Hz: a flat EQ must establish: {e}"));
        assert_eq!(
            stage.active_band_indices(),
            expected_active_bands(fs),
            "{fs} Hz: the active band set must be exactly the bands below Nyquist"
        );
    }
    // The documented anchors of the product table, as exact index sets:
    // 8 kHz keeps the 31 Hz–2 kHz bands, 22.05 kHz adds the 4/8 kHz
    // bands, the 16 kHz band stays excluded at exactly 32 kHz
    // (Nyquist = 16 kHz, strict rule) and first participates above it,
    // and every rate above 32 kHz carries all ten.
    assert_eq!(expected_active_bands(8000), vec![0, 1, 2, 3, 4, 5, 6]);
    assert_eq!(expected_active_bands(16000), vec![0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(
        expected_active_bands(22050),
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8]
    );
    assert_eq!(
        expected_active_bands(32000),
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8]
    );
    assert_eq!(
        expected_active_bands(44100),
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
    );
}

/// Flat means Processing Off at EVERY source rate: the neutral config
/// compiles its active bands and remains the identity BIT-EXACTLY (the
/// all-neutral coefficients coincide at any rate; the old policy made
/// Flat refuse below 32 kHz instead).
#[test]
fn flat_eq_is_bit_exact_identity_at_every_product_rate() {
    let mut block: Vec<f32> = (0..4096)
        .map(|i| {
            let x = i as f32;
            if i % 3 == 0 {
                -x
            } else if i % 5 == 0 {
                x * 0.001
            } else {
                x
            }
        })
        .collect();
    let expected = block.clone();
    for fs in [8000u32, 16000, 22050, 32000, 44100, 48000, 96000] {
        let mut stage = EqStage::new(&EqConfig::FLAT, &format_at(fs))
            .unwrap_or_else(|e| panic!("{fs} Hz: flat must establish: {e}"));
        stage.stage(&mut block);
        assert_eq!(
            block, expected,
            "{fs} Hz: a flat EQ must be the identity filter bit-exactly"
        );
    }
}

/// At a low source rate the cascade matches the independent f64
/// reference over the ACTIVE bands only, and a trim on an UNAVAILABLE
/// band is inert: it changes nothing bit-exactly (the stored trim stays
/// in the desired configuration and re-becomes active on a higher-rate
/// source — it must not silently shape this episode).
#[test]
fn at_low_rates_only_active_bands_shape_and_unavailable_trims_are_inert() {
    // Shape an ACTIVE band (125 Hz, +12 dB) and trim an UNAVAILABLE one
    // (16 kHz, +18 dB — invalid as data nowhere, unavailable at 8 kHz).
    let mut shaped = [0.0f32; 10];
    shaped[2] = 12.0;
    shaped[9] = 18.0;
    let shaped_config = EqConfig::new(shaped, 1.0);
    let mut inert = [0.0f32; 10];
    inert[2] = 12.0;
    let inert_config = EqConfig::new(inert, 1.0);

    let fs = 8000u32;
    let mut stage = EqStage::new(&shaped_config, &format_at(fs)).expect("low-rate EQ establishes");
    assert_eq!(
        stage.active_band_indices(),
        vec![0, 1, 2, 3, 4, 5, 6],
        "8 kHz carries the 31 Hz–2 kHz bands"
    );

    let frames = 512;
    let mut input = vec![0.0f32; frames * 2];
    input[0] = 1.0;
    input[1] = 1.5;
    let mut same_input = input.clone();
    stage.stage(&mut input);

    // The unavailable trim is inert BIT-EXACTLY: same active bands, same
    // coefficients, same output.
    let mut inert_stage = EqStage::new(&inert_config, &format_at(fs)).expect("establishes");
    inert_stage.stage(&mut same_input);
    assert_eq!(
        input, same_input,
        "a trim on an unavailable band must not shape this episode"
    );

    // And the shaped output matches the independent f64 reference over
    // exactly the active bands (the +18 dB at 16 kHz appears in the
    // desired config but NOT in the reference — inertness, both sides).
    for (channel, amplitude) in [(0usize, 1.0f64), (1usize, 1.5f64)] {
        let mut impulse = vec![0.0f64; frames];
        impulse[0] = amplitude;
        let expected = reference_cascade_active(&impulse, &shaped_config, f64::from(fs));
        let frames_i = 2;
        for (i, &y) in expected.iter().enumerate() {
            let produced = input[i * 2 + channel];
            let magnitude = y.abs().max(1.0);
            assert!(
                (f64::from(produced) - y).abs() / magnitude < 1e-3,
                "channel {channel} sample {i} (every {frames_i}): {produced} vs reference {y}"
            );
        }
    }
}

/// Frame-count and layout are conserved at every product rate: the
/// stage transforms the buffer in place, one output frame per input
/// frame, interleaved layout untouched, and the output stays finite
/// (no NaN/Inf) even at a boosted preset over the smallest active
/// profiles.
#[test]
fn frame_count_layout_and_finiteness_hold_across_the_rate_matrix() {
    let rock = EqPreset::Rock.to_config();
    let rock_eq = rock.eq.expect("rock carries an EQ config");
    let block: Vec<f32> = (0..2048)
        .map(|i| ((i as f32) * 0.125 - 128.0).mul_add(0.01, ((i % 7) as f32) * 0.05))
        .collect();
    let original_len = block.len();
    for fs in [8000u32, 16000, 22050, 32000, 44100, 48000, 96000] {
        let mut stage = EqStage::new(&rock_eq, &format_at(fs))
            .unwrap_or_else(|e| panic!("{fs} Hz: the Rock preset must establish: {e}"));
        let mut copy = block.clone();
        stage.stage(&mut copy);
        assert_eq!(copy.len(), original_len, "{fs} Hz: frame count conserved");
        assert!(
            copy.iter().all(|s| s.is_finite()),
            "{fs} Hz: every output sample finite (no NaN/Inf)"
        );
        // Layout: the interleaved stereo stream must stay a stream of
        // frames — the stage is an in-place per-sample transform, so the
        // buffer length in FRAMES is unchanged by construction; the
        // per-channel independence is witnessed by the channel-distinct
        // impulse oracles elsewhere in this suite.
    }
}

/// D1 negative controls for the rate policy. Honesty about mechanism:
/// control (b) is the machinery-level sensitivity proof — it builds the
/// wrong-rule world out of real signal machinery (the generalized f64
/// reference cascade) and proves the impulse-response oracle
/// distinguishes it from the accepted world. Controls (a) and (c) pin
/// the WRONG WORLDS' observable establishment outcomes as executable
/// documentation (what the old refusal / an inclusive rule would
/// produce, replayed through the same assertion shape the positive
/// oracles use); the suite's RED sensitivity to those policies is
/// mutation-witnessed — re-applying the old refusal to the production
/// path turns four tests RED, the too-aggressive predicate two, the
/// inclusive predicate four (campaign issue #190, D1 evidence).
#[test]
fn the_rate_policy_oracles_reject_the_old_and_deliberately_wrong_rules() {
    let frames = 256;
    let mut impulse = vec![0.0f64; frames];
    impulse[0] = 1.0;

    // (a) The OLD policy: every enabled EQ refuses at ≤32 kHz. Its
    // observable establishment outcome at a low rate is an Err; the
    // accepted world is an Ok profile — the same assertion shape the
    // positive oracles use distinguishes the two worlds.
    let old_policy_establishment: Result<Vec<usize>, String> =
        Err("EQ band 9 (16000 Hz) is at or above this source's Nyquist \
         frequency (11025 Hz at 22050 Hz source rate)"
            .to_owned());
    assert!(
        rejects(|| {
            let active = old_policy_establishment
                .clone()
                .expect("flat must establish at every product rate");
            assert_eq!(active, expected_active_bands(22050));
        }),
        "the establishment assertion shape must distinguish the old \
         whole-EQ refusal world from the accepted one"
    );

    // (b) A TOO-AGGRESSIVE rule (requires the center below fs/4)
    // silently drops the 16 kHz band at 44.1 kHz — distinguishable
    // exactly when the dropped band carries a nonzero trim.
    let high_band = eq_config_one_band(9, 12.0);
    let accepted = reference_cascade_active(&impulse, &high_band, TEST_FS);
    let too_aggressive =
        reference_cascade_under(&impulse, &high_band, TEST_FS, |f0, fs| f0 * 4.0 < fs);
    assert!(
        rejects(|| {
            for (a, b) in accepted.iter().zip(too_aggressive.iter()) {
                let magnitude = a.abs().max(1.0);
                assert!(
                    (a - b).abs() / magnitude < 1e-3,
                    "impulse responses must match the accepted-rule reference"
                );
            }
        }),
        "the reference oracle must distinguish the too-aggressive availability rule"
    );

    // (c) The INCLUSIVE rule (f0 ≤ Nyquist) admits the degenerate
    // exactly-Nyquist band at 32 kHz (w0 = π). Its observable
    // establishment outcome differs from the accepted world either way
    // the f32 arithmetic lands: the per-band stability check refuses
    // the w0 = π shelf's recursion (|a1| ≥ 1 + a2 up to rounding —
    // observed in practice, see the mutation evidence) → an
    // establishment error; or, were a rounding ever to compile it, the
    // profile would carry 10 bands where the accepted rule says 9.
    // Both wrong worlds are replayed through the same assertion shape.
    for inclusive_world in [
        Err::<Vec<usize>, String>("EQ band 9 compiled to an unstable recursion".to_owned()),
        Ok(vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]),
    ] {
        assert!(
            rejects(|| {
                let active = inclusive_world.expect("the accepted profile establishes");
                assert_eq!(active, expected_active_bands(32000));
            }),
            "the establishment assertion shape must distinguish the \
             inclusive (degenerate-band) world from the accepted one"
        );
    }
}

/// The product order is GAIN then EQ, pinned by the rounding: the
/// pipeline's output equals "multiply, then cascade" bit-exactly and
/// differs from "cascade, then multiply" (the two orders are
/// mathematically commuting linear operations — the f32 roundings are
/// not, and the oracle pins the implemented one).
#[test]
fn the_product_order_is_gain_then_eq() {
    // The gain must NOT be a power of two: multiplying by 2^k is exact
    // in f32, and an exact scalar commutes bit-for-bit with any linear
    // filter — the two orders would be indistinguishable. 1.5 rounds.
    let config = AudioProcessingConfig {
        enabled: true,
        gain: 1.5,
        eq: Some(eq_config_one_band(5, 12.0)),
    };
    let mut processing = EpisodeProcessing::new(&config, &test_format()).expect("compiles");

    let mut block: Vec<f32> = (0..2048).map(|i| (i as f32) * 0.25 - 256.0).collect();
    processing.stage(&mut block).expect("stage succeeds");

    // Gain-first reference: multiply, then a freshly compiled cascade.
    let mut gain_first = (0..2048)
        .map(|i| (i as f32) * 0.25 - 256.0)
        .collect::<Vec<_>>();
    for sample in gain_first.iter_mut() {
        *sample *= 1.5;
    }
    let mut eq_only = EqStage::new(&eq_config_one_band(5, 12.0), &test_format()).expect("compiles");
    eq_only.stage(&mut gain_first);
    assert_eq!(
        block, gain_first,
        "the pipeline must be exactly gain-then-EQ"
    );

    // EQ-first: a different rounding — the pipeline must not match it.
    let mut eq_first = (0..2048)
        .map(|i| (i as f32) * 0.25 - 256.0)
        .collect::<Vec<_>>();
    let mut eq_stage =
        EqStage::new(&eq_config_one_band(5, 12.0), &test_format()).expect("compiles");
    eq_stage.stage(&mut eq_first);
    for sample in eq_first.iter_mut() {
        *sample *= 1.5;
    }
    assert_ne!(
        block, eq_first,
        "gain-then-EQ and EQ-then-gain must differ in f32 rounding; the \
         oracle cannot pin the order otherwise"
    );
}

// --- composition level: the production path ------------------------------

/// The EQ through the REAL composition: the config-carrying
/// establishment compiles the cascade against the episode's source
/// format, and the consumed stream equals a freshly compiled stage fed
/// the same position-tagged source — BIT-EXACT, both channels. Frame
/// and format conservation ride on the same oracle.
#[test]
fn eq_processes_the_real_staging_seam() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = TEST_RATE * 2;
        let config = AudioProcessingConfig {
            enabled: true,
            gain: 1.0,
            eq: Some(eq_config_all(6.0)),
        };
        let (witnesses, handle, mut runtime) =
            episode(source_frames, OutputBehavior::Consume, config, Vec::new());
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);

        // The independent content oracle: a freshly compiled runtime
        // (same config, same format) driven over the raw position tags.
        let mut reference_processing =
            EpisodeProcessing::new(&config, &test_common::TEST_FORMAT).expect("compiles");
        let mut reference_input: Vec<f32> = (0..source_frames)
            .flat_map(|i| vec![i as f32, i as f32 + 0.5])
            .collect();
        reference_processing
            .stage(&mut reference_input)
            .expect("reference stage succeeds");
        let expected: Vec<Vec<f32>> = (0..2)
            .map(|c| reference_input[c..].chunks(2).map(|f| f[0]).collect())
            .collect();

        let values = witnesses.content();
        let values_ch1 = witnesses.content_ch1();
        // The fresh-stage reference has exactly source_frames frames, so
        // frame conservation is pinned by the equality below (the raw-tag
        // oracle does NOT apply — the content is EQ-processed).
        assert_eq!(values.len(), source_frames, "frame conservation");
        assert_eq!(
            values, expected[0],
            "channel 0 must equal the fresh-stage oracle"
        );
        assert_eq!(
            values_ch1, expected[1],
            "channel 1 must equal the fresh-stage oracle"
        );
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// A flat EQ is observationally the BYPASS at the real seam: the
/// consumed stream is the raw position-tagged source, BIT-EXACT (the
/// I4 Flat preset's oracle, proven at the composition level).
#[test]
fn flat_eq_is_indistinguishable_from_bypass_at_the_seam() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = 20_000;
        let (witnesses, handle, mut runtime) = episode(
            source_frames,
            OutputBehavior::Consume,
            AudioProcessingConfig::eq(EqConfig::FLAT),
            Vec::new(),
        );
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_processed_exactly_at(&witnesses.content(), 0.0, 1.0, source_frames, "channel 0");
        assert_processed_exactly_at(
            &witnesses.content_ch1(),
            0.5,
            1.0,
            source_frames,
            "channel 1",
        );
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// An invalid EQ configuration (non-positive Q) fails the EPISODE
/// ESTABLISHMENT exactly like an invalid gain: a truthful activation
/// diagnostic, no terminal Fact, no audio.
#[test]
fn an_invalid_eq_config_fails_establishment_without_a_terminal_fact() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let (witnesses, handle, mut runtime) = episode(
            20_000,
            OutputBehavior::Consume,
            AudioProcessingConfig {
                enabled: true,
                gain: 1.0,
                eq: Some(EqConfig::new([0.0; 10], 0.0)),
            },
            Vec::new(),
        );
        let observation = handle.observe();
        let error = observation
            .activation_error
            .as_deref()
            .expect("the activation must raise");
        assert!(
            error.contains("audio processing configuration invalid"),
            "the diagnostic must name the establishment failure: {error}"
        );
        assert_eq!(observation.terminal_outcome, None);
        assert_eq!(witnesses.consumed(), 0);
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet");
    });
}

// --- composition level: the stateful lifecycle matrix --------------------
//
// The I2 probe proved the SEAM carries stateful history; the EQ is the
// first production processor WITH such state, so the matrix is rerun
// with a real (+12 dB band) configuration. `assert_eq!(…, 1.0)` gain
// keeps the EQ the only non-identity stage.

fn boosted_eq_config() -> AudioProcessingConfig {
    AudioProcessingConfig {
        enabled: true,
        gain: 1.0,
        eq: Some(eq_config_one_band(2, 12.0)),
    }
}

/// RefusedUnchanged with EQ state in flight (deterministic mid-block
/// geometry, the I2 construction): pause first, full-edge witness,
/// seek, refusal, resume — the consumed stream equals the no-seek
/// control BIT-EXACT on both channels: the cascade's recursive state
/// and the processed remainder survive the refusal untouched.
#[test]
fn a_refused_seek_preserves_eq_history_as_the_no_seek_control() {
    use crate::session::EDGE_CAPACITY_FRAMES;
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let (control_w, control_handle, mut control_runtime) = episode(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            boosted_eq_config(),
            Vec::new(),
        );
        assert_eq!(
            control_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );
        let control_values = control_w.content();

        let handle = crate::handle::PlaybackSessionHandle::new();
        let (seeked_w, mut seeked_runtime) =
            crate::processing_support::episode_with_test_processor_and_handle(
                EIGHT_SECONDS,
                OutputBehavior::SlowConsume {
                    per_read: Duration::from_millis(1),
                },
                crate::processing::EpisodeProcessing::new(
                    &boosted_eq_config(),
                    &test_common::TEST_FORMAT,
                )
                .expect("compiles"),
                vec![ProviderSeekOutcome::RefusedUnchanged],
                handle.clone(),
            );
        wait_until(Duration::from_secs(5), || {
            handle.observe().position.is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        handle.request_pause();
        assert!(
            wait_until(Duration::from_secs(5), || handle.observe().paused()),
            "the Paused projection never established"
        );
        assert!(
            wait_until(Duration::from_secs(5), || handle
                .completion
                .buffered_frames()
                == Some(EDGE_CAPACITY_FRAMES)),
            "the edge never filled while paused"
        );
        handle.request_seek(Duration::from_secs(5));
        assert!(
            handle.observe().pause_requested,
            "pause intent must survive the refused seek"
        );
        handle.request_resume();
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "a refusal is not a failure and not a cut"
        );

        assert_eq!(
            seeked_w.content(),
            control_values,
            "channel 0: the refused seek must be content-indistinguishable \
             from the no-seek control"
        );
        assert_eq!(
            seeked_w.content_ch1(),
            control_w.content_ch1(),
            "channel 1: the same"
        );
        let snapshot_control = control_runtime.dispose().snapshot;
        assert!(snapshot_control.quiet);
        let snapshot = seeked_runtime.dispose().snapshot;
        assert!(snapshot.quiet);
    });
}

/// Applied with EQ state in flight: the pre-cut stretch equals the
/// control's prefix, the post-cut stretch equals a FRESHLY COMPILED
/// stage fed the exact post-landing tags (the D14.11 fresh-instance
/// equivalence, now for production state) — bit-exact, frame count
/// conserved.
#[test]
fn an_applied_seek_resumes_eq_from_fresh_state_at_the_landing() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let five_seconds = 5 * TEST_RATE;
        let landing_frames = EIGHT_SECONDS - five_seconds;

        let (control_w, control_handle, mut control_runtime) = episode(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            boosted_eq_config(),
            Vec::new(),
        );
        assert_eq!(
            control_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );
        let control_values = control_w.content();

        let (seeked_w, seeked_handle, mut seeked_runtime) = episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            boosted_eq_config(),
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            seeked_handle
                .observe()
                .position
                .is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        seeked_handle.request_seek(Duration::from_secs(5));
        assert!(
            wait_until(Duration::from_secs(5), || seeked_handle
                .observe()
                .position
                .is_some_and(|p| p >= five_seconds as u64)),
            "the position never rebased to the landing"
        );
        assert_eq!(
            seeked_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );

        let values = seeked_w.content();
        let cut = values
            .iter()
            .zip(control_values.iter())
            .position(|(a, c)| a != c)
            .expect("the applied cut must appear as a divergence from the control");
        assert_eq!(&values[..cut], &control_values[..cut]);

        // Fresh stage over the exact post-landing tags.
        let mut fresh = EpisodeProcessing::new(&boosted_eq_config(), &test_common::TEST_FORMAT)
            .expect("compiles");
        let mut post_input: Vec<f32> = (five_seconds..EIGHT_SECONDS)
            .flat_map(|i| vec![i as f32, i as f32 + 0.5])
            .collect();
        fresh.stage(&mut post_input).expect("stage succeeds");
        let expected_ch0: Vec<f32> = post_input.chunks(2).map(|f| f[0]).collect();
        let expected_ch1: Vec<f32> = post_input.chunks(2).map(|f| f[1]).collect();

        assert_eq!(
            values.len(),
            cut + landing_frames,
            "the post-cut stretch must conserve its frame count exactly"
        );
        assert_eq!(
            &values[cut..],
            expected_ch0.as_slice(),
            "the post-cut stretch must equal a fresh stage bit-exactly \
             (channel 0)"
        );
        assert_eq!(
            &seeked_w.content_ch1()[cut..],
            expected_ch1.as_slice(),
            "channel 1: the same"
        );
        let snapshot_control = control_runtime.dispose().snapshot;
        assert!(snapshot_control.quiet);
        let snapshot = seeked_runtime.dispose().snapshot;
        assert!(snapshot.quiet);
    });
}

/// Pause/resume preserves the EQ's recursive history: the consumed
/// stream equals the no-seek control bit-exact — with real
/// signal-derived state this is now observable (the stateless Gain
/// pause oracle could not distinguish a reset).
#[test]
fn a_pause_resume_cycle_preserves_eq_history() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = TEST_RATE * 4;
        let (control_w, control_handle, mut control_runtime) = episode(
            source_frames,
            OutputBehavior::Consume,
            boosted_eq_config(),
            Vec::new(),
        );
        assert_eq!(
            control_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );

        let (paused_w, paused_handle, mut paused_runtime) = episode(
            source_frames,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            boosted_eq_config(),
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            paused_handle
                .observe()
                .position
                .is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        paused_handle.request_pause();
        assert!(
            wait_until(Duration::from_secs(5), || paused_handle.observe().paused()),
            "the Paused projection never established"
        );
        paused_handle.request_resume();
        assert_eq!(
            paused_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );
        assert_eq!(paused_w.content(), control_w.content(), "channel 0");
        assert_eq!(paused_w.content_ch1(), control_w.content_ch1(), "channel 1");
        let snapshot_control = control_runtime.dispose().snapshot;
        assert!(snapshot_control.quiet);
        let snapshot = paused_runtime.dispose().snapshot;
        assert!(snapshot.quiet);
    });
}

/// A new episode starts from FRESH EQ state: two sequential episodes
/// over the same source both equal the same fresh-stage reference (and
/// each other) bit-exactly — no cross-episode state leak.
#[test]
fn a_new_episode_starts_from_fresh_eq_state() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = TEST_RATE * 3;
        let (first_w, first_handle, mut first_runtime) = episode(
            source_frames,
            OutputBehavior::Consume,
            boosted_eq_config(),
            Vec::new(),
        );
        assert_eq!(
            first_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );
        let first_values = first_w.content();
        let snapshot = first_runtime.dispose().snapshot;
        assert!(snapshot.quiet);

        let (second_w, second_handle, mut second_runtime) = episode(
            source_frames,
            OutputBehavior::Consume,
            boosted_eq_config(),
            Vec::new(),
        );
        assert_eq!(
            second_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );
        assert_eq!(
            second_w.content(),
            first_values,
            "no cross-episode state: the same source and config must \
             produce the same stream"
        );

        let mut fresh = EpisodeProcessing::new(&boosted_eq_config(), &test_common::TEST_FORMAT)
            .expect("compiles");
        let mut input: Vec<f32> = (0..source_frames)
            .flat_map(|i| vec![i as f32, i as f32 + 0.5])
            .collect();
        fresh.stage(&mut input).expect("stage succeeds");
        let expected: Vec<f32> = input.chunks(2).map(|f| f[0]).collect();
        assert_eq!(second_w.content(), expected, "fresh-state equivalence");
        let snapshot = second_runtime.dispose().snapshot;
        assert!(snapshot.quiet);
    });
}

/// A destructive provider seek never reconstructs the old EQ
/// continuation: D11 Failed, the decode-origin diagnostic stays
/// truthful, the pre-failure stretch equals the control's prefix, and
/// nothing is produced after the failure.
#[test]
fn a_mutated_then_failed_seek_never_reconstructs_eq_continuation() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let (control_w, control_handle, mut control_runtime) = episode(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            boosted_eq_config(),
            Vec::new(),
        );
        assert_eq!(
            control_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );
        let control_values = control_w.content();

        let (failed_w, failed_handle, mut failed_runtime) = episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            boosted_eq_config(),
            vec![ProviderSeekOutcome::MutatedThenFailed {
                diagnostic: "test destructive seek".to_owned(),
            }],
        );
        wait_until(Duration::from_secs(5), || {
            failed_handle
                .observe()
                .position
                .is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        failed_handle.request_seek(Duration::from_secs(5));
        assert_eq!(
            failed_handle.wait_terminal(),
            EpisodeTerminalOutcome::Failed
        );
        let diagnostic = failed_handle
            .observe()
            .failure_diagnostic
            .expect("a failed episode carries its diagnostic");
        assert!(diagnostic.starts_with("decode: seek failed:"));
        let values = failed_w.content();
        assert_eq!(
            &values[..],
            &control_values[..values.len()],
            "the pre-failure stretch must equal the control's prefix"
        );
        let stopped_at = failed_w.consumed();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            failed_w.consumed(),
            stopped_at,
            "no post-failure production"
        );
        let snapshot_control = control_runtime.dispose().snapshot;
        assert!(snapshot_control.quiet);
        let snapshot = failed_runtime.dispose().snapshot;
        assert!(snapshot.quiet);
    });
}

// --- performance gate (I3.10) ---------------------------------------------

/// The EQ's steady-state cost on the decode-worker staging executor:
/// all ten bands active, 1024-frame stereo staging (the production
/// block size), zero steady-state allocation (measured), median/p95/p99
/// wall time recorded. The bound asserted here is deliberately generous
/// (5 ms median per block — the block is 23 ms of audio at 44.1 kHz, so
/// the bound alone already proves the executor is not marginal); the
/// MEASURED figures are recorded in the campaign evidence.
///
/// The per-sample work is sample-rate independent (coefficients are
/// constants at run time), so 44.1 kHz and 48 kHz staging differ only
/// in their compiled coefficients — both are measured below to evidence
/// exactly that.
#[test]
fn eq_stage_cost_is_bounded_and_allocation_free() {
    fn measure(config: &EqConfig) -> (usize, Duration, Duration, Duration) {
        let mut stage = EqStage::new(config, &test_format()).expect("compiles");
        let mut block = vec![0.25f32; 1024 * 2];
        // Warm the path outside the measurement window.
        stage.stage(&mut block);
        let (_, allocations) =
            crate::edge_lifecycle_tests::counting_allocator::run_counting_allocations(|| {
                for _ in 0..2_000 {
                    stage.stage(&mut block);
                }
            });
        let mut samples = Vec::with_capacity(2_000);
        for _ in 0..2_000 {
            let start = Instant::now();
            stage.stage(&mut block);
            samples.push(start.elapsed());
        }
        samples.sort();
        let median = samples[1_000];
        let p95 = samples[1_900];
        let p99 = samples[1_980];
        (allocations, median, p95, p99)
    }

    let config_441 = eq_config_all(6.0);
    let (allocations, median, p95, p99) = measure(&config_441);
    assert_eq!(
        allocations, 0,
        "steady-state EQ must allocate nothing per staging block"
    );
    assert!(
        median < Duration::from_millis(5),
        "median per-block EQ cost {median:?} exceeds the generous 5 ms \
         bound — the decode-worker executor would be marginal"
    );
    println!(
        "EQ 44.1 kHz stereo, 1024-frame staging, all bands: median {median:?}, p95 {p95:?}, p99 {p99:?}, allocations {allocations}"
    );

    // 48 kHz: same per-sample work, different coefficients — measured
    // with the same statistic discipline as the 44.1 kHz leg (I3 review:
    // per-sample medians, not one window mean, and the allocation
    // assertion run for both).
    let config_48 = EqConfig::new([6.0; 10], 1.0);
    let format_48 = PcmFormat {
        sample_rate: 48000,
        channels: 2,
        channel_mask: 0x3,
    };
    let mut stage_48 = EqStage::new(&config_48, &format_48).expect("compiles");
    let mut block = vec![0.25f32; 1024 * 2];
    stage_48.stage(&mut block);
    let (_, allocations_48) =
        crate::edge_lifecycle_tests::counting_allocator::run_counting_allocations(|| {
            for _ in 0..2_000 {
                stage_48.stage(&mut block);
            }
        });
    assert_eq!(allocations_48, 0, "48 kHz staging allocates nothing too");
    let mut samples_48 = Vec::with_capacity(2_000);
    for _ in 0..2_000 {
        let start = Instant::now();
        stage_48.stage(&mut block);
        samples_48.push(start.elapsed());
    }
    samples_48.sort();
    let median_48 = samples_48[1_000];
    let p99_48 = samples_48[1_980];
    println!(
        "EQ 48 kHz stereo, 1024-frame staging, all bands: median {median_48:?}, p99 {p99_48:?}, allocations {allocations_48}"
    );
    assert!(
        median_48 < Duration::from_millis(5),
        "48 kHz staging cost {median_48:?} exceeds the bound"
    );
}

// --- I4: presets are configuration data through the real path ------------

/// A NAMED PRESET through the REAL composition: `EqPreset::Bass`
/// resolves to an ordinary AudioProcessingConfig, and the consumed
/// stream equals a freshly compiled stage over the same source —
/// BIT-EXACT, both channels. Presets are data, not processors: nothing
/// about the pipeline changes for a named selection.
#[test]
fn a_named_preset_processes_through_the_real_seam() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = TEST_RATE * 2;
        let (witnesses, handle, mut runtime) = episode(
            source_frames,
            OutputBehavior::Consume,
            EqPreset::Bass.to_config(),
            Vec::new(),
        );
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);

        let expected = fresh_stage_reference(&EqPreset::Bass.to_config(), source_frames);
        assert_eq!(witnesses.content(), expected[0], "channel 0");
        assert_eq!(witnesses.content_ch1(), expected[1], "channel 1");
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// A preset CHANGE applies at the NEXT episode's establishment (the
/// D14.11 episode-fixed applied snapshot, case B): episode 1 runs Bass;
/// episode 2 — a fresh establishment in the same process — runs Treble.
/// Each episode's consumed stream equals its OWN fresh-stage reference
/// bit-exactly, and the two differ (the change really applied at the
/// new establishment; no live update, no cross-episode state).
#[test]
fn a_preset_change_applies_at_the_next_episodes_establishment() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = TEST_RATE * 3;
        let (first_w, first_handle, mut first_runtime) = episode(
            source_frames,
            OutputBehavior::Consume,
            EqPreset::Bass.to_config(),
            Vec::new(),
        );
        assert_eq!(
            first_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );
        let first_values = first_w.content();
        let first_values_ch1 = first_w.content_ch1();
        let snapshot = first_runtime.dispose().snapshot;
        assert!(snapshot.quiet, "first episode teardown must stay quiet");

        let (second_w, second_handle, mut second_runtime) = episode(
            source_frames,
            OutputBehavior::Consume,
            EqPreset::Treble.to_config(),
            Vec::new(),
        );
        assert_eq!(
            second_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );
        let second_values = second_w.content();
        let second_values_ch1 = second_w.content_ch1();

        let expected_first = fresh_stage_reference(&EqPreset::Bass.to_config(), source_frames);
        let expected_second = fresh_stage_reference(&EqPreset::Treble.to_config(), source_frames);
        assert_eq!(first_values, expected_first[0], "episode 1: its own preset");
        assert_eq!(
            first_values_ch1, expected_first[1],
            "episode 1: its own preset, channel 1"
        );
        assert_eq!(
            second_values, expected_second[0],
            "episode 2: its own preset"
        );
        assert_eq!(
            second_values_ch1, expected_second[1],
            "episode 2: its own preset, channel 1"
        );
        assert_ne!(
            first_values, second_values,
            "the preset change must really apply at the new establishment"
        );
        let snapshot = second_runtime.dispose().snapshot;
        assert!(snapshot.quiet, "second episode teardown must stay quiet");
    });
}

/// The fresh-stage content reference for a desired configuration: the
/// same compiled runtime driven over the position-tagged source.
fn fresh_stage_reference(config: &AudioProcessingConfig, source_frames: usize) -> Vec<Vec<f32>> {
    let mut reference =
        EpisodeProcessing::new(config, &test_common::TEST_FORMAT).expect("compiles");
    let mut input: Vec<f32> = (0..source_frames)
        .flat_map(|i| vec![i as f32, i as f32 + 0.5])
        .collect();
    reference.stage(&mut input).expect("stage succeeds");
    (0..2)
        .map(|c| input[c..].chunks(2).map(|f| f[0]).collect())
        .collect()
}
