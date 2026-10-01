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
//! Tolerances are justified, not guessed: the f64 reference uses the
//! same RECIPE but an independent derivation, so the pipeline-vs-
//! reference difference is bounded by f32 coefficient rounding
//! (~1e-7 relative) contracted by the recursion (|poles| < 1), giving
//! orders of magnitude of margin at 1e-3 relative. The FLAT equality
//! needs no tolerance at all: with every band at 0 dB the normalized
//! numerator and denominator coefficients coincide exactly, the state
//! stays at rest, and the cascade is the identity BIT-EXACTLY — the
//! strongest product oracle (I4's Flat preset rides on this).

use std::time::{Duration, Instant};

use qianqian_audio_api::ports::PcmFormat;
use qianqian_audio_api::ports::ProviderSeekOutcome;

use crate::handle::EpisodeTerminalOutcome;
use crate::processing::{
    AudioProcessingConfig, EQ_BAND_FREQUENCY_HZ, EQ_MAX_BAND_GAIN_DB, EpisodeProcessing, EqConfig,
    EqStage,
};
use crate::processing_support::{
    EIGHT_SECONDS, TEST_RATE, assert_processed_exactly_at, episode, wait_until,
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

/// The RBJ Audio EQ Cookbook recipes, re-derived independently in f64
/// (the oracle's own source, deliberately separate from the production
/// f32 compilation). Returns the normalized `[b0, b1, b2, a1, a2]`.
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
    // integers, negative values, subnormals-adjacent magnitudes.
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
/// independent f64 coefficients (±0.5 dB; the analytic value itself is
/// the recipe's design target |H(e^{jw0})| = A). Channel independence
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
/// non-finite or out-of-bound band gains, a zero sample rate, and —
/// the format-DEPENDENT half — any band at or above the source's
/// Nyquist frequency. Nothing is clamped; nothing poisons the state.
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
    // Format-dependent: an 8 kHz source has a 4 kHz Nyquist, so the
    // 4/8/16 kHz product bands cannot compile for it.
    let low_rate = PcmFormat {
        sample_rate: 8000,
        channels: 2,
        channel_mask: 0x3,
    };
    let err = EqStage::new(&EqConfig::FLAT, &low_rate)
        .expect_err("bands at/above Nyquist must fail compilation");
    assert!(
        err.contains("Nyquist"),
        "the diagnostic must name the Nyquist conflict: {err}"
    );
}

/// The stage compiles against a source format as a WHOLE: at and above
/// 32 kHz every product band sits below Nyquist and the stage compiles;
/// below, the first band at/above Nyquist refuses the establishment
/// with a truthful diagnostic (the per-band stability grid itself lives
/// in processing.rs, where the compiled coefficients are inspectable).
#[test]
fn the_stage_refuses_sources_whose_nyquist_cuts_the_band_table() {
    for fs in [8000u32, 16000, 22050, 32000] {
        let format = PcmFormat {
            sample_rate: fs,
            channels: 2,
            channel_mask: 0x3,
        };
        let err = EqStage::new(&EqConfig::FLAT, &format)
            .expect_err("a source rate at/below 32 kHz cuts the 16 kHz band");
        assert!(err.contains("Nyquist"), "{fs}: {err}");
    }
    for fs in [44100u32, 48000, 96000, 192000] {
        let format = PcmFormat {
            sample_rate: fs,
            channels: 2,
            channel_mask: 0x3,
        };
        assert!(
            EqStage::new(&EqConfig::FLAT, &format).is_ok(),
            "{fs} Hz carries the full product band table"
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
