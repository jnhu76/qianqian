//! Episode Audio Processing (ADR-PBK-002 D14.11, Issue #177 Stage 2
//! I1/I2). Two types, one frozen seam:
//!
//! ```text
//! AudioProcessingConfig    the DESIRED product configuration, owned by
//!                          the application/product-control layer and
//!                          handed to the episode at establishment
//! EpisodeProcessing        the APPLIED episode-fixed snapshot, owned by
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
//! (stateful EQ, I3) join by evidence under the same rules — the
//! representation re-earns itself there, not here.
//!
//! Configuration is EPISODE-FIXED (the D14.11 four-way model, case B):
//! the applied snapshot is built once at activation and never updated
//! live. A changed desired configuration takes effect at the next
//! episode's establishment. Live parameter update remains OPEN and
//! unearned.

/// The desired Audio Processing configuration (D14.11: application /
/// product-control layer owns it; this type is only its transport into
/// episode establishment — never K0 state, never a Capability payload,
/// never SongCore, never an Output-backend concern).
///
/// Validation happens at episode establishment: an invalid desired
/// configuration fails the activation cleanly (`activation_error`) —
/// it never silently clamps, substitutes defaults, or produces NaN
/// audio (fail-closed, the same conservative posture as D14.11's
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
    pub gain: f32,
}

impl AudioProcessingConfig {
    /// The transparent bypass configuration (processing disabled).
    pub const BYPASS: Self = Self {
        enabled: false,
        gain: 1.0,
    };

    /// An enabled configuration applying the linear gain `factor`.
    pub fn gain(factor: f32) -> Self {
        Self {
            enabled: true,
            gain: factor,
        }
    }

    /// Establish-time validation. A well-formed configuration is total:
    /// the gain must be finite and non-negative regardless of `enabled`
    /// (a NaN/Inf/negative gain is an invalid configuration, not an
    /// inert one — product configuration that cannot mean anything
    /// fails loudly instead of silently meaning nothing). Polarity
    /// inversion is not an authorized product semantic for this slice.
    pub fn validate(&self) -> Result<(), String> {
        if !self.gain.is_finite() {
            return Err(format!("gain must be finite, got {}", self.gain));
        }
        if self.gain < 0.0 {
            return Err(format!("gain must be non-negative, got {}", self.gain));
        }
        Ok(())
    }
}

/// The stage-closure shape of the test-only processor injection: one
/// staging block in place, `Err` is the unrecoverable processing
/// failure.
#[cfg(all(test, not(loom)))]
type TestStage = Box<dyn FnMut(&mut [f32]) -> Result<(), String> + Send>;

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
            #[cfg(all(test, not(loom)))]
            Self::TestDriven { .. } => f.write_str("EpisodeProcessing::TestDriven(..)"),
        }
    }
}

impl EpisodeProcessing {
    /// Compile the applied snapshot from the desired configuration.
    /// Called exactly once per episode, at activation, on the session's
    /// establishment path; `Err` fails the activation before any
    /// resource is acquired.
    pub(crate) fn new(config: &AudioProcessingConfig) -> Result<Self, String> {
        config.validate()?;
        Ok(if config.enabled {
            Self::Gain {
                factor: config.gain,
            }
        } else {
            Self::Bypass
        })
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
            #[cfg(all(test, not(loom)))]
            Self::TestDriven { invalidate, .. } => invalidate(),
        }
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;

    /// A well-formed enabled config compiles to the Gain state.
    #[test]
    fn an_enabled_config_compiles_to_scalar_gain() {
        let mut processing = EpisodeProcessing::new(&AudioProcessingConfig::gain(0.5))
            .expect("valid config establishes");
        let mut block = [2.0f32, 4.0];
        processing.stage(&mut block).expect("gain stage succeeds");
        assert_eq!(block, [1.0, 2.0]);
    }

    /// Bypass is transparent bit-for-bit: the block is untouched.
    #[test]
    fn bypass_passes_the_block_through_untouched() {
        let mut processing =
            EpisodeProcessing::new(&AudioProcessingConfig::BYPASS).expect("bypass establishes");
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
            EpisodeProcessing::new(&AudioProcessingConfig::gain(2.0)).expect("valid config");
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
            let err = EpisodeProcessing::new(&AudioProcessingConfig::gain(gain))
                .expect_err("invalid gain must fail establishment");
            assert!(!err.is_empty(), "gain {gain}: a diagnostic is carried");
        }
        // Bypass with a valid gain value still establishes.
        EpisodeProcessing::new(&AudioProcessingConfig::BYPASS).expect("bypass establishes");
    }
}
