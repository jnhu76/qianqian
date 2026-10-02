//! Episode Audio Processing (ADR-PBK-002 D14.11, Issue #177 Stage 2
//! I1-I3). The desired/applied pair and, since I3, the first stateful
//! production processor (a 10-band biquad EQ cascade):
//!
//! ```text
//! AudioProcessingConfig    the DESIRED product configuration, owned by
//!                          the application/product-control layer and
//!                          handed to the episode at establishment
//! EpisodeProcessing        the APPLIED episode snapshot, owned by
//!                          the Playback Session as a subordinate
//!                          episode resource (the same D6 class as the
//!                          decode endpoint / worker / PcmEdge)
//! ```
//!
//! The processing itself runs at the frozen decode-worker staging
//! placement — after `read_frames`, before the PcmEdge write — through
//! [`EpisodeProcessing::stage`], called once per whole staging block by
//! the decode worker (session.rs). The processing class is the D14.11
//! minimum: source-rate preserving, source-layout preserving,
//! frame-count preserving, bounded causal, no mandatory pending output
//! at EOF. Realtime posture: no per-block K0 work, no capability or
//! context resolution, no dispatch, no I/O, no allocation — the stage
//! is an in-place transform of the caller's buffer.
//!
//! NOT a Plugin, NOT a Capability, NOT a processor framework (D13
//! negative ruling): one enum with the states the current product
//! configuration can express, private to this crate. New processors
//! join by evidence under the same rules — the representation re-earns
//! itself there, not here.
//!
//! Configuration binding (D14.11 as extended by the 2026-10-02
//! live-admission amendment, dsp-product-model.md §7.3): the applied
//! snapshot is compiled ONCE at activation from product-control's
//! desired configuration, and the four §7.3 live-authorized operation
//! classes (scalar preamp, 10-band GEQ band gains, factory-preset
//! switch, processing enabled/bypass toggle) may then be applied live
//! through the handle's typed `set_*` commands ([`crate::live`]: the
//! depth-1 latest-wins pending cell and the Model C dual-processor
//! crossfade). Everything else stays episode-fixed until separately
//! earned.

use qianqian_audio_api::ports::PcmFormat;

/// The desired Audio Processing configuration (D14.11: application /
/// product-control layer owns it; this type is its transport into
/// episode establishment and into the live `set_*` commands — never K0
/// state, never a Capability payload, never SongCore, never an
/// Output-backend concern).
///
/// Validation happens before acceptance: an invalid desired
/// configuration fails the activation cleanly (`activation_error`) at
/// establishment, and a live `set_*` command is REFUSED with an honest
/// diagnostic (the old configuration keeps running bit-exactly) — it
/// never silently clamps, substitutes defaults, or produces NaN audio
/// (fail-closed, the same conservative posture as D14.11's
/// "bypass/recovery not authorized").
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioProcessingConfig {
    /// Whether the episode runs processing at all. `false` is the
    /// transparent BYPASS configuration: the staging blocks reach the
    /// edge untouched, and `gain` is inert. (Bypass is a configuration,
    /// not a processor that runs and does nothing — there is no
    /// resident processor to disable.)
    pub enabled: bool,
    /// The linear gain factor applied to every sample when enabled.
    ///
    /// ```text
    /// 1.0   unity (bit-exact identity on normal f32 values)
    /// <1.0  attenuation
    /// 0.0   true silence (every sample exactly +0.0; frames conserved)
    /// >1.0  positive gain — the PCM-domain result may exceed ±1.0
    /// ```
    ///
    /// Positive gain is NOT clipped or limited by the processing stage:
    /// the internal Float32 PCM may exceed ±1.0, and what the device
    /// then does with out-of-range samples is the output mechanism's
    /// behavior (D14.9 Volume is a separate, distinct mechanism — the
    /// device/output-stream realization — and neither implements nor
    /// writes the other). Gain semantics are documented, never silent.
    ///
    /// Product order (I3, frozen by the runtime shape): this Gain stage
    /// is the PREAMP and runs FIRST, the EQ stage (if configured) runs
    /// second — an explicit fixed order, never a registration or
    /// iteration order. No automatic headroom compensation exists
    /// between them; the internal Float32 range covers positive EQ
    /// sums, unclipped and unlimitied.
    pub gain: f32,
    /// The EQ stage (I3): a fixed 10-band biquad cascade. `None` runs
    /// no EQ; `Some` config is PRODUCT DATA (band trims in dB + the
    /// peaking-band Q), validated and compiled against the episode's
    /// source format at establishment. Inert under bypass.
    pub eq: Option<EqConfig>,
}

impl AudioProcessingConfig {
    /// The transparent bypass configuration (processing disabled).
    pub const BYPASS: Self = Self {
        enabled: false,
        gain: 1.0,
        eq: None,
    };

    /// An enabled configuration applying the linear gain `factor`.
    pub fn gain(factor: f32) -> Self {
        Self {
            enabled: true,
            gain: factor,
            eq: None,
        }
    }

    /// An enabled configuration with a neutral preamp and the given EQ
    /// stage.
    pub fn eq(eq: EqConfig) -> Self {
        Self {
            enabled: true,
            gain: 1.0,
            eq: Some(eq),
        }
    }

    /// Establish-time validation (the intrinsic half). A well-formed
    /// configuration is total: the gain must be finite and non-negative
    /// and the EQ bands representable — product configuration that
    /// cannot mean anything fails loudly instead of silently meaning
    /// nothing. Polarity inversion is not an authorized product
    /// semantic for this slice. The format-dependent half — which
    /// bands are AVAILABLE at the episode's source rate — is not an
    /// invalidity and never refuses: it decides participation when the
    /// EQ compiles against the episode's source format (the rate-aware
    /// active-band profile, [`band_participates`]).
    pub fn validate(&self) -> Result<(), String> {
        if !self.gain.is_finite() {
            return Err(format!("gain must be finite, got {}", self.gain));
        }
        if self.gain < 0.0 {
            return Err(format!("gain must be non-negative, got {}", self.gain));
        }
        if let Some(eq) = &self.eq {
            eq.validate()?;
        }
        Ok(())
    }
}

/// The fixed 10-band product EQ set, in Hz (Issue #177 I3). PRODUCT
/// CONFIGURATION, not architecture authority: the conventional
/// graphic-EQ centers. Band 0 is a LOW SHELF, bands 1–8 are PEAKING,
/// band 9 is a HIGH SHELF — the kind mapping is part of this product
/// table, not a registry or a plugin taxonomy. The table is fixed for
/// the slice; band KINDS and centers are never semantic authority.
pub(crate) const EQ_BAND_FREQUENCY_HZ: [f32; 10] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// The product bound on per-band trim. A product tuning decision (what
/// a graphic EQ exposes), documented here because I3.6 requires the
/// headroom posture to be explicit: +18 dB on every band stays finite
/// in internal Float32 (pinned by an oracle), the device owns
/// out-of-range samples, and no limiter or clip exists.
pub(crate) const EQ_MAX_BAND_GAIN_DB: f32 = 18.0;

/// The rate-aware AVAILABILITY rule of the 10-band product EQ
/// (dsp-product-model.md §2.1, the accepted LOW_RATE_EQ_POLICY =
/// rate-aware active-band profile): a fixed band participates in an
/// episode's cascade iff its center is strictly below that source's
/// Nyquist frequency.
///
/// Semantics, frozen by the product authority:
///
/// ```text
/// available band      compiled normally; its trim shapes the episode
/// unavailable band    INERT for this episode (not compiled — at or
///                     above Nyquist the band's w0 degenerates and no
///                     source content exists there to shape); its
///                     configured trim STAYS in the desired
///                     configuration and becomes active again on an
///                     episode whose source domain includes the band
/// never a refusal     availability never fails establishment and
///                     never changes the processing class (still
///                     source-rate/layout/frame preserving, bounded
///                     causal, no EOF drain)
/// ```
///
/// Strictness is load-bearing: at exactly Nyquist (w0 = π) the f32
/// trigonometry collapses toward the degenerate recursion the per-band
/// stability check would then have to refuse, so the boundary band
/// belongs to the unavailable side. [`EqStage::new`] is the only caller
/// in production; the crate oracles restate the rule independently.
pub(crate) fn band_participates(center_hz: f32, sample_rate_hz: u32) -> bool {
    center_hz * 2.0 < sample_rate_hz as f32
}

/// The desired EQ configuration: per-band trims in dB for the fixed
/// product band table plus the peaking bands' Q. Pure DATA — not a
/// processor, not a plugin, not per-band objects with independent
/// lifecycle (Issue #177 I4's preset taxonomy applies to this too:
/// presets are configuration data over this same struct).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EqConfig {
    /// Trim in dB for each band of [`EQ_BAND_FREQUENCY_HZ`], in band
    /// order. 0.0 = neutral band.
    pub band_gain_db: [f32; 10],
    /// Q of the peaking bands (the two shelves use the cookbook's S = 1
    /// slope, which has no separate Q). Product tuning.
    pub q: f32,
}

impl EqConfig {
    /// The neutral EQ: every band 0 dB (observationally the identity —
    /// pinned BIT-EXACT by an oracle, the strongest product oracle).
    pub const FLAT: Self = Self {
        band_gain_db: [0.0; 10],
        q: 1.0,
    };

    /// An EQ configuration from band trims and the peaking Q.
    pub fn new(band_gain_db: [f32; 10], q: f32) -> Self {
        Self { band_gain_db, q }
    }

    /// Intrinsic validation (format-independent). Runs at establishment;
    /// the format-dependent part (which bands are available at this
    /// source rate) is decided by the availability rule when the stage
    /// compiles — availability is not invalidity.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !self.q.is_finite() || self.q <= 0.0 {
            return Err(format!("EQ q must be finite and positive, got {}", self.q));
        }
        for (band, gain_db) in self.band_gain_db.iter().enumerate() {
            if !gain_db.is_finite() {
                return Err(format!("EQ band {band} gain must be finite, got {gain_db}"));
            }
            if gain_db.abs() > EQ_MAX_BAND_GAIN_DB {
                return Err(format!(
                    "EQ band {band} gain {} dB exceeds the product bound \
                     ±{EQ_MAX_BAND_GAIN_DB} dB",
                    gain_db
                ));
            }
        }
        Ok(())
    }
}

/// The stage-closure shape of the test-only processor injection: one
/// staging block in place, `Err` is the unrecoverable processing
/// failure.
#[cfg(all(test, not(loom)))]
type TestStage = Box<dyn FnMut(&mut [f32]) -> Result<(), String> + Send>;

/// One compiled biquad band: its fixed-table index, the normalized
/// coefficients (a0 = 1) and the per-channel Transposed Direct Form II
/// state. Episode-owned; retired with the episode.
#[derive(Debug)]
struct BiquadBand {
    // Read only by the test-gated active-band observation; dead in the
    // shipped and loom builds.
    #[cfg_attr(not(all(test, not(loom))), allow(dead_code))]
    index: usize,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    s1: Vec<f32>,
    s2: Vec<f32>,
}

/// The compiled EQ stage: the episode's ACTIVE band cascade in explicit
/// fixed product order (available bands of band 0 → 9, lowest first),
/// one independent state per source channel. Steady-state posture: in
/// place, allocation-free, per-sample work independent of block
/// boundaries (the fragmentation invariance the I2 probe proved for the
/// transport is inherited by construction — same per-sample op
/// sequence).
#[derive(Debug)]
pub(crate) struct EqStage {
    bands: Vec<BiquadBand>,
    channels: usize,
}

impl EqStage {
    /// Compile the cascade against the episode's source format. Runs
    /// the config's intrinsic validation itself (defense in depth),
    /// then applies the rate-aware ACTIVE-BAND profile
    /// (dsp-product-model.md §2.1): a fixed product band participates
    /// in this episode's cascade iff its center is strictly below the
    /// source's Nyquist frequency ([`band_participates`]); the rest are
    /// unavailable in this source's domain and are not compiled. `pub`
    /// (crate) for the in-crate DSP oracles; product code reaches the
    /// stage only through [`EpisodeProcessing::new`].
    pub(crate) fn new(config: &EqConfig, format: &PcmFormat) -> Result<Self, String> {
        config.validate()?;
        if format.sample_rate == 0 {
            return Err("sample rate must be positive".to_owned());
        }
        let channels = usize::from(format.channels);
        if channels == 0 {
            return Err("a source without channels cannot be processed".to_owned());
        }
        let fs = format.sample_rate as f32;
        let mut bands = Vec::with_capacity(EQ_BAND_FREQUENCY_HZ.len());
        for (index, &f0) in EQ_BAND_FREQUENCY_HZ.iter().enumerate() {
            if !band_participates(f0, format.sample_rate) {
                continue;
            }
            bands.push(BiquadBand::compiled(
                index,
                f0,
                config.band_gain_db[index],
                config.q,
                fs,
                channels,
            )?);
        }
        Ok(Self { bands, channels })
    }

    /// The episode's ACTIVE band indices — which fixed product bands
    /// participate in this cascade under the rate-aware availability
    /// rule, in cascade order. Test-gated: the crate-internal honesty
    /// observation for the oracles (pinning the exact SET, not just a
    /// count); the product-facing read model is a separate product
    /// decision (dsp-product-model.md §2.1, D5).
    #[cfg(all(test, not(loom)))]
    pub(crate) fn active_band_indices(&self) -> Vec<usize> {
        self.bands.iter().map(|band| band.index).collect()
    }

    /// The cascade, in place, in explicit fixed band order. Frame-count
    /// and layout preserving by construction: every sample passes every
    /// band, channel states stay independent.
    pub(crate) fn stage(&mut self, block: &mut [f32]) {
        for frame in block.chunks_exact_mut(self.channels) {
            for (c, sample) in frame.iter_mut().enumerate() {
                let mut x = *sample;
                for band in &mut self.bands {
                    x = band.step(x, c);
                }
                *sample = x;
            }
        }
    }

    /// The Applied-seek invalidation target (D14.11): the coefficients
    /// are format-derived constants and stay; the per-channel state
    /// returns to its episode-start rest. "Fresh-instance observational
    /// equivalence" — a reset stage behaves exactly like a newly
    /// compiled one (pinned by an oracle).
    fn reset(&mut self) {
        for band in &mut self.bands {
            for s in band.s1.iter_mut() {
                *s = 0.0;
            }
            for s in band.s2.iter_mut() {
                *s = 0.0;
            }
        }
    }
}

impl BiquadBand {
    /// Compile one band. Coefficient formulas: RBJ Audio EQ Cookbook
    /// (Robert Bristow-Johnson, "Audio EQ Cookbook",
    /// https://www.w3.org/2011/audio/audio-eq-cookbook.html — the
    /// standard published peaking/shelving biquad recipes), realized in
    /// Transposed Direct Form II with a0 normalized to 1. Dependent
    /// quantities, pinned here explicitly: `w0 = 2π·f0/fs` carries the
    /// SOURCE-RATE dependence; the peaking bands use the configured Q;
    /// the shelves use the cookbook's S = 1 slope; `A = 10^(dB/40)` is
    /// the gain mapping. Design targets that fall out of the algebra:
    /// the peaking band's response at its center frequency and both
    /// shelves' asymptotic gains equal `A²` — the full configured dB
    /// (`20·log10(A²) = dB`). For valid parameters (f0 < Nyquist,
    /// q > 0, finite gain) the recipe's poles lie inside the unit
    /// circle, so the output is bounded and finite; compilation refuses
    /// anything the f32 arithmetic degraded (non-finite or unstable)
    /// rather than poisoning the recursion (I3.5).
    fn compiled(
        index: usize,
        f0: f32,
        gain_db: f32,
        q: f32,
        fs: f32,
        channels: usize,
    ) -> Result<Self, String> {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = std::f32::consts::TAU * f0 / fs;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let (b0, b1, b2, a0, a1, a2) = if index == 0 {
            // Low shelf (S = 1: (A + 1/A)(1/S - 1) vanishes, so
            // alpha = sin(w0)/2 * sqrt(2)).
            let alpha = (sin_w0 / 2.0) * std::f32::consts::SQRT_2;
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
            // High shelf (same S = 1 alpha as the low shelf).
            let alpha = (sin_w0 / 2.0) * std::f32::consts::SQRT_2;
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
        let (b0, b1, b2, a1, a2) = (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0);
        for coefficient in [b0, b1, b2, a1, a2] {
            if !coefficient.is_finite() {
                return Err(format!(
                    "EQ band {index} compiled to a non-finite coefficient"
                ));
            }
        }
        // Monic-quadratic pole test (Jury): the recursion is bounded
        // iff |a2| < 1 and |a1| < 1 + a2. SCOPE: this is a per-band
        // coefficient sanity check, NOT the availability policy — which
        // bands participate lives in the rate-aware rule EqStage::new
        // applies before any compilation. (Near the f32 boundary — w0
        // within ~2e-4 rad of π — cos(w0) collapses and this check can
        // refuse a merely degenerate band; unreachable at any standard
        // rate with the fixed product table, and a safe fail-closed
        // either way.)
        if !(a2.abs() < 1.0 && a1.abs() < 1.0 + a2) {
            return Err(format!("EQ band {index} compiled to an unstable recursion"));
        }
        Ok(Self {
            index,
            b0,
            b1,
            b2,
            a1,
            a2,
            s1: vec![0.0; channels],
            s2: vec![0.0; channels],
        })
    }

    /// One Transposed Direct Form II step for channel `c`:
    ///
    /// ```text
    /// y[n] = b0·x[n] + s1        s1' = b1·x[n] − a1·y[n] + s2
    ///                               s2' = b2·x[n] − a2·y[n]
    /// ```
    fn step(&mut self, x: f32, c: usize) -> f32 {
        let y = self.b0 * x + self.s1[c];
        self.s1[c] = self.b1 * x - self.a1 * y + self.s2[c];
        self.s2[c] = self.b2 * x - self.a2 * y;
        y
    }
}

/// The episode-owned processing runtime: the applied snapshot compiled
/// at activation, carried by the decode worker for the episode's whole
/// lifetime, and retired with it (Open/replacement: the old episode's
/// processing state dies with the episode; the next episode compiles a
/// fresh snapshot from its own desired configuration).
///
/// `Debug` is hand-written because the test-only variant carries boxed
/// closures (a derived impl cannot print them; the derived shape would
/// otherwise be the only reason to restrict the variant).
pub(crate) enum EpisodeProcessing {
    /// Transparent bypass: the stage passes the block through untouched.
    Bypass,
    /// Scalar Gain: every sample of the staging block is scaled by the
    /// linear factor once, in place. Stateless: no signal-derived
    /// history, so history invalidation is a no-op.
    Gain { factor: f32 },
    /// Gain (preamp) THEN the EQ cascade — the explicit fixed product
    /// order (I3.8), pinned by the variant's shape and an oracle, never
    /// by registration or iteration. The EQ carries the episode's
    /// signal-derived history: this is the state the Applied-seek
    /// invalidation returns to rest, the pause preserves, and the
    /// refusal must not disturb (D14.11, proven by the I2 probe).
    GainThenEq { factor: f32, eq: EqStage },
    /// Test-only processor injection (never shipped): the stage and the
    /// history invalidation are arbitrary closures, so the in-crate
    /// oracles can drive the REAL composition with a deliberate
    /// processor — the I1/I2 failure route, the I2 StatefulProbe and its
    /// mutation knobs — without any product seam for test doubles. No
    /// product configuration reaches this state.
    #[cfg(all(test, not(loom)))]
    TestDriven {
        stage: TestStage,
        invalidate: Box<dyn FnMut() + Send>,
    },
}

impl std::fmt::Debug for EpisodeProcessing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bypass => f.write_str("EpisodeProcessing::Bypass"),
            Self::Gain { factor } => f
                .debug_struct("EpisodeProcessing::Gain")
                .field("factor", factor)
                .finish(),
            Self::GainThenEq { factor, eq } => f
                .debug_struct("EpisodeProcessing::GainThenEq")
                .field("factor", factor)
                .field("eq", eq)
                .finish(),
            #[cfg(all(test, not(loom)))]
            Self::TestDriven { .. } => f.write_str("EpisodeProcessing::TestDriven(..)"),
        }
    }
}

impl EpisodeProcessing {
    /// Compile the applied snapshot from the desired configuration,
    /// bound to the EPISODE's source format (D14.11: processing state
    /// whose semantics depend on the source PCM format is bound to that
    /// episode's format). Called exactly once per episode, at
    /// activation, after the decode endpoint is open — the EQ stage's
    /// coefficients are a function of the source sample rate, so the
    /// compile cannot run before the format is known; a failure here
    /// (invalid configuration) raises the activation and unwinds the
    /// open endpoint through the ordinary RAII. Source-rate
    /// availability of the fixed bands is not a failure mode: the
    /// rate-aware active-band profile compiles whatever participates
    /// ([`band_participates`]).
    pub(crate) fn new(config: &AudioProcessingConfig, format: &PcmFormat) -> Result<Self, String> {
        config.validate()?;
        if !config.enabled {
            return Ok(Self::Bypass);
        }
        match &config.eq {
            None => Ok(Self::Gain {
                factor: config.gain,
            }),
            Some(eq_config) => Ok(Self::GainThenEq {
                factor: config.gain,
                eq: EqStage::new(eq_config, format)?,
            }),
        }
    }

    /// The test-only processor injection (see the variant doc). Crate-
    /// internal: reached by the white-box oracles through the cfg(test)
    /// session constructor; no product path constructs it.
    #[cfg(all(test, not(loom)))]
    pub(crate) fn test_driven(stage: TestStage, invalidate: Box<dyn FnMut() + Send>) -> Self {
        Self::TestDriven { stage, invalidate }
    }

    /// The D14.11 processing stage, executed once per whole staging
    /// block at the frozen decode-worker placement. In place,
    /// allocation-free, frame-count preserving by construction (it
    /// touches sample values only), bounded causal, no pending output
    /// at EOF.
    ///
    /// `Err` is the unrecoverable processing failure: the worker routes
    /// it through the existing D11 `Failed` publication with the
    /// truthful processing-origin diagnostic (never a decode label,
    /// never a new public terminal variant). No bypass, no partial
    /// result — a failed processor's output is not trustworthy.
    pub(crate) fn stage(&mut self, block: &mut [f32]) -> Result<(), String> {
        match self {
            Self::Bypass => Ok(()),
            Self::Gain { factor } => {
                let factor = *factor;
                for sample in block.iter_mut() {
                    *sample *= factor;
                }
                Ok(())
            }
            Self::GainThenEq { factor, eq } => {
                let factor = *factor;
                for sample in block.iter_mut() {
                    *sample *= factor;
                }
                eq.stage(block);
                Ok(())
            }
            #[cfg(all(test, not(loom)))]
            Self::TestDriven { stage, .. } => stage(block),
        }
    }

    /// The D14.11 Applied-seek obligation: invalidate ALL pre-cut
    /// signal-derived processing history, before any post-cut PCM is
    /// processed. The worker calls this exactly once per APPLIED cut,
    /// beside the staging discard and the edge purge; a refused seek
    /// NEVER reaches it (RefusedUnchanged preserves history), pause
    /// never reaches it (pause preserves history), and Open/replacement
    /// retires the whole runtime with the episode (fresh state is
    /// structural). The acceptance semantics are D14.11's "fresh-
    /// instance observational equivalence", not this method's name —
    /// the realization (reset call / rebuild / state swap) stays open
    /// representation.
    ///
    /// Stateless processors (Bypass, Gain) have no signal-derived
    /// history, so their invalidation is a no-op.
    pub(crate) fn invalidate_signal_history(&mut self) {
        match self {
            Self::Bypass | Self::Gain { .. } => {}
            Self::GainThenEq { eq, .. } => eq.reset(),
            #[cfg(all(test, not(loom)))]
            Self::TestDriven { invalidate, .. } => invalidate(),
        }
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;

    /// The format the unit tests compile against.
    fn test_format() -> PcmFormat {
        PcmFormat {
            sample_rate: 44100,
            channels: 2,
            channel_mask: 0x3,
        }
    }

    /// A well-formed enabled config compiles to the Gain state.
    #[test]
    fn an_enabled_config_compiles_to_scalar_gain() {
        let mut processing =
            EpisodeProcessing::new(&AudioProcessingConfig::gain(0.5), &test_format())
                .expect("valid config establishes");
        let mut block = [2.0f32, 4.0];
        processing.stage(&mut block).expect("gain stage succeeds");
        assert_eq!(block, [1.0, 2.0]);
    }

    /// Bypass is transparent bit-for-bit: the block is untouched.
    #[test]
    fn bypass_passes_the_block_through_untouched() {
        let mut processing = EpisodeProcessing::new(&AudioProcessingConfig::BYPASS, &test_format())
            .expect("bypass establishes");
        let mut block = [0.5f32, -0.25, 7.0];
        processing.stage(&mut block).expect("bypass stage succeeds");
        assert_eq!(block, [0.5, -0.25, 7.0]);
    }

    /// Positive gain above unity is documented, not clipped: the stage
    /// multiplies and lets the result exceed ±1.0 (the device, not the
    /// processing stage, owns out-of-range behavior).
    #[test]
    fn positive_gain_is_not_clipped_by_the_stage() {
        let mut processing =
            EpisodeProcessing::new(&AudioProcessingConfig::gain(2.0), &test_format())
                .expect("valid config");
        let mut block = [0.6f32, -0.6];
        processing.stage(&mut block).expect("gain stage succeeds");
        assert_eq!(block, [1.2, -1.2]);
    }

    /// Invalid desired configurations fail establishment: NaN, ±Inf and
    /// negative gains are rejected with a diagnostic; nothing is
    /// clamped or substituted.
    #[test]
    fn invalid_gains_fail_establishment() {
        for gain in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.5] {
            let err = EpisodeProcessing::new(&AudioProcessingConfig::gain(gain), &test_format())
                .expect_err("invalid gain must fail establishment");
            assert!(!err.is_empty(), "gain {gain}: a diagnostic is carried");
        }
        // Bypass with a valid gain value still establishes.
        EpisodeProcessing::new(&AudioProcessingConfig::BYPASS, &test_format())
            .expect("bypass establishes");
    }

    /// A neutral EQ compiles and is the identity on this block; an
    /// EQ-carrying config compiles to the GainThenEq state (I3).
    #[test]
    fn an_eq_config_compiles_to_the_gain_then_eq_state() {
        let config = AudioProcessingConfig::eq(EqConfig::FLAT);
        let mut processing = EpisodeProcessing::new(&config, &test_format()).expect("compiles");
        let mut block = [1.0f32, -2.0, 3.5, -4.25];
        processing.stage(&mut block).expect("stage succeeds");
        assert_eq!(
            block,
            [1.0, -2.0, 3.5, -4.25],
            "a flat EQ stage is the identity bit-exactly"
        );
    }

    /// Per-band stability across the product parameter grid: every
    /// compilable (band, gain, Q, source rate) combination yields
    /// FINITE coefficients satisfying the monic-quadratic pole bound
    /// (|a2| < 1, |a1| < 1 + a2) — bounded output by construction; every
    /// combination with the band at/above Nyquist is refused.
    #[test]
    fn the_band_stability_grid_holds_across_product_parameters() {
        for (band, &f0_f32) in EQ_BAND_FREQUENCY_HZ.iter().enumerate() {
            let f0 = f64::from(f0_f32);
            for gain_db in [-18.0f32, -12.0, -6.0, 0.0, 6.0, 12.0, 18.0] {
                for q in [0.5f32, 1.0, 2.0, 4.0] {
                    // 16000/32000 put the 8/16 kHz bands EXACTLY at
                    // Nyquist: the in-tree witness that the boundary
                    // recursion degenerates and the per-band check
                    // refuses it (the load-bearing strictness of the
                    // rate-aware availability rule, §2.1).
                    for fs in [8000u32, 16000, 22050, 32000, 44100, 48000, 96000, 192000] {
                        let nyquist = f64::from(fs) / 2.0;
                        match BiquadBand::compiled(band, f0_f32, gain_db, q, fs as f32, 2) {
                            Ok(compiled) => {
                                assert!(f0 < nyquist);
                                assert!(
                                    compiled.a2.abs() < 1.0
                                        && compiled.a1.abs() < 1.0 + compiled.a2,
                                    "band {band} unstable at {fs} Hz \
                                     (gain {gain_db}, q {q})"
                                );
                            }
                            Err(_) => assert!(
                                f0 >= nyquist,
                                "band {band} refused at {fs} Hz without a \
                                 Nyquist conflict (gain {gain_db}, q {q})"
                            ),
                        }
                    }
                }
            }
        }
    }
}
