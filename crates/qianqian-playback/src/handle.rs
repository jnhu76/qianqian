//! The application-facing episode seam (F2, reality-gate-2 verdict B;
//! ADR-PBK-002 D14.2). One handle references exactly one playback
//! episode and gives the application — headless today, a future UI
//! adapter tomorrow — exactly these rights:
//!
//! ```text
//! request_stop()     record stop intent (Command, not a Fact)
//! request_pause()    record pause intent (Command, not a Fact; D14.7)
//! request_resume()   release a recorded pause intent (Command)
//! request_seek()     record a seek command (Command, not a Fact; D14.5)
//! observe()          pure read of terminal/command/evidence fields
//! wait_terminal()    pure blocking wait for the committed terminal Fact
//! ```
//!
//! Completion-lock fields share one read; Position and DSP refusal use
//! separate cells. No cross-cell snapshot or freshness bound is promised.
//!
//! The handle is NOT a Plugin, Capability, K0 primitive, global store or
//! second lifecycle owner, and it carries no mechanism rights: terminal
//! evidence publication and D11 settlement stay on session-owned paths
//! behind the crate boundary ([`crate::completion`]). Nothing the
//! application can call here can create or relabel the terminal Fact.
//!
//! `PlaybackSessionObservation` keeps the truth classes of its fields
//! explicit and separate:
//!
//! ```text
//! terminal_outcome    Fact; None means only "no terminal Fact committed
//!                     yet" — it is not Playing/Starting/Paused or any
//!                     fourth outcome
//! failure_diagnostic  presentation text for a Failed outcome; never part
//!                     of the semantic contract
//! stop_requested      Command state; true does not mean Stopping
//! pause_requested     Command state (D14.7); true does not mean Paused
//! source_format       mechanism evidence, not source identity and not a
//!                     playback semantic state
//! source_duration     optional source-scoped mechanism evidence from the
//!                     decode probe — NOT exact, and None means unknown
//! position            Projection (D14.8): one pure load of the episode's
//!                     render-leg position-evidence cell, in source PCM
//!                     frames, derived only while the episode is live
//!                     and unsettled. None means "no sample published
//!                     yet" (or withdrawn after a terminal Fact or an
//!                     activation failure) — never position zero, and
//!                     never a claim about the acoustic instant
//! pause_engagement    mechanism evidence: the CURRENT pause
//!                     engagement's render-gate / output-tail state
//! activation_error    activation diagnostic; never a terminal Failed
//! ```
//!
//! `paused()` is the D14.7 derived Projection over those fields —
//! derived visibility only: never a correctness basis for resume/stop
//! legality, teardown, terminal settlement, resource lifetime,
//! mechanism wakeup, or K0 lifecycle transitions. `Resumed` is NOT a
//! projection (D14.7 AUTHORITY-CORRECTIVE): disengagement evidence
//! proves only that the gate's current park ended, not that a viable
//! render leg remains to submit future audio — a never-activated /
//! open-aborted episode permanently closes and joins that leg. Resume
//! stays command state only, and disengagement stays crate-internal
//! mechanism evidence (see [`crate::completion`]).
//!
//! Fields the current architecture has not earned (Playing/Starting/
//! Stopping, transport states, buffer health, source identity, K0
//! FiberState, and every raw mechanism counter — a handed-off total, a
//! device tail, a raw estimate, a decoded count) are structurally
//! absent: the application reads ONE position sample and reconstructs
//! no device state from two cells.

use std::time::Duration;

use qianqian_audio_api::ports::PcmFormat;

use crate::completion::{SessionCompletion, SessionOutcome};
use crate::presets::EqPreset;
use crate::processing::EqConfig;

/// The stable semantic terminal outcome of one playback episode
/// (ADR-PBK-002 D14.2): exactly these three variants and nothing else.
///
/// Diagnostics — which leg failed and why — are deliberately NOT part of
/// this contract. They are carried separately as
/// [`PlaybackSessionObservation::failure_diagnostic`] and may change
/// freely; no semantic subclass (decode-failed, device-failed, …) is
/// representable in or derivable from this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpisodeTerminalOutcome {
    /// EOF was produced, drained and played out.
    Completed,
    /// Stop intent was recorded at the decisive evidence boundary and no
    /// higher-precedence failure classification won.
    Stopped,
    /// The episode failed before completion. The reason is diagnostic
    /// only: read `failure_diagnostic` for presentation, never for
    /// semantic dispatch.
    Failed,
}

/// The mechanism-evidence state of an episode's CURRENT pause
/// engagement (ADR-PBK-002 D14.7). This is the observation spelling of
/// the engagement / output-tail-quiescence evidence latches — mechanism
/// evidence, not a semantic transport state, and never a correctness
/// basis for anything. The derived Paused projection requires
/// [`PauseEngagement::TailQuiesced`]; a previous pause cycle's
/// quiescence never satisfies a later pause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseEngagement {
    /// No engagement is latched: the episode never paused, or its most
    /// recent pause engagement was released.
    Disengaged,
    /// The render leg is parked at the pre-GetBuffer pause gate, but
    /// this engagement's output tail has not been observed quiesced yet
    /// (frames submitted before engagement may still be queued to play).
    Engaged,
    /// Engaged, and the CURRENT engagement's output tail was observed
    /// quiesced: no frame submitted before engagement remains queued
    /// for rendering by this output session.
    TailQuiesced,
}

/// Pure observation of one playback episode. Completion-lock fields
/// coexisted at one instant; `position` and `last_processing_refusal`
/// come from independent cells with no cross-cell freshness bound.
/// Neither sample names the instant at which terminal truth was read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackSessionObservation {
    /// The committed terminal outcome, or `None` while no terminal Fact
    /// has been committed yet (pending).
    pub terminal_outcome: Option<EpisodeTerminalOutcome>,
    /// Why the episode failed, when it failed and a diagnostic was
    /// published. Presentation text, NOT part of the semantic contract:
    /// `Completed`/`Stopped` observations never carry one, and a `Failed`
    /// observation may or may not. Its presence, absence or spelling may
    /// change without a semantic change.
    pub failure_diagnostic: Option<String>,
    /// Whether stop intent has been recorded. Command state, not
    /// outcome truth.
    pub stop_requested: bool,
    /// Whether pause intent has been recorded (D14.7). Command state,
    /// not playback truth: true does not mean Paused.
    pub pause_requested: bool,
    /// The episode's source PCM format once activation published it.
    /// Mechanism evidence.
    pub source_format: Option<PcmFormat>,
    /// The duration the decode mechanism reported for this source at
    /// probe/open time (D14.8), or `None` when it reported none.
    ///
    /// Truth class: optional source-scoped **Mechanism Evidence** —
    /// NEVER a Fact, and not exact in general (the container's own
    /// declaration may over-claim what the source actually decodes to).
    /// `None` means unknown, never zero. Unlike `position`, this evidence
    /// is not a playback-state projection: it stays observable after the
    /// terminal Fact, like `source_format`.
    pub source_duration: Option<Duration>,
    /// The current episode's position Projection (D14.8): one pure load
    /// of the render leg's published sample — the source-relative
    /// location of device-consumed presentation, in source PCM frames.
    ///
    /// Truth class: **Projection**. Never a Fact, never a transport
    /// state, and never a correctness basis for control, lifetime,
    /// settlement or K0 lifecycle. It is not a decoded, submitted or
    /// audible position: it deliberately excludes all latency
    /// downstream of the device-consumption point (engine queue,
    /// hardware, DAC).
    ///
    /// `None` means no sample exists: before the render mechanism
    /// publishes its first one, and again once the episode stops being a
    /// live one — a committed terminal Fact withdraws the projection (no
    /// final-position value is stored), and so does a recorded activation
    /// failure, because the mechanism that publishes is opened before the
    /// last fallible activation step, so a raising activation can leave
    /// samples in the cell for an episode that never played. It never
    /// means position zero, and it is never fabricated for a
    /// never-activated episode.
    ///
    /// The sample is exact only for the instant the render leg read its
    /// tail; this read promises no freshness bound (the age of a sample
    /// is the reader's poll interval plus the mechanism's publication
    /// cadence, not a concurrency invariant). What it does promise:
    /// monotone between committed seek discontinuities (a cut may rebase
    /// backward once), never above the writer's own handed-off
    /// accounting, never fabricated.
    ///
    /// Pause coupling (D14.8): while parked at the D14.7 gate no new
    /// frame is submitted, but the queued tail can still drain and advance
    /// the sample. It freezes at current-engagement tail quiescence,
    /// later than the pause command or engagement alone. A release ends
    /// the park and lets the loop proceed once more (resume, or the stop that wakes it), so an
    /// observation taken before a release is a sample, not a latch: the
    /// writer may still publish one more in-flight block, exactly the
    /// advance the frozen rule measures at command time. It stays within
    /// the never-above-the-accounting promise throughout.
    pub position: Option<u64>,
    /// The current pause engagement's mechanism-evidence state
    /// (D14.7). Evidence, not a semantic transport state. Meaningful
    /// only while `terminal_outcome` is `None`: publication is
    /// first-wins and closes at settlement, so after a terminal Fact
    /// the latched spelling may outlive the leg — the `paused`
    /// projection guards on the unsettled state, and any other
    /// consumer of this field must too.
    pub pause_engagement: PauseEngagement,
    /// Why activation raised, if it did. Diagnostic; an episode that
    /// never started has no terminal Fact and must not be forged into
    /// `Failed`.
    pub activation_error: Option<String>,
    /// The most recent Audio Processing update refusal diagnostic
    /// (campaign #190 D4), or `None` while no update has been refused.
    ///
    /// Truth class: **mechanism evidence / diagnostic** — the same class
    /// as `failure_diagnostic`: presentation text, never part of the
    /// semantic contract, never a Fact, and never a correctness basis.
    /// It reports WHY the most recent refused `set_*` command left the
    /// old configuration running (invalid data, compile refusal); it is
    /// cleared by the next intrinsically valid Desired record, before
    /// worker Accepted/Applied. Like `position`, it is read
    /// from its own cell rather than the single observation lock, so it
    /// carries no freshness bound relative to the other fields.
    pub last_processing_refusal: Option<String>,
}

impl PlaybackSessionObservation {
    /// The Paused projection (D14.7): the episode is unsettled, pause
    /// intent is recorded, the render leg is engaged at the gate, AND the
    /// current engagement's output tail was observed quiesced. A derived
    /// projection only — never a correctness basis for control,
    /// lifetime, settlement or K0 lifecycle.
    pub fn paused(&self) -> bool {
        self.terminal_outcome.is_none()
            && self.pause_requested
            && self.pause_engagement == PauseEngagement::TailQuiesced
    }
}

/// The public application handle to one playback episode: the reference
/// the App / headless / a future UI adapter holds while the episode is
/// live. Clones reference the same episode; one handle serves exactly
/// one episode (do not re-use across a retried or restarted episode).
#[derive(Clone)]
pub struct PlaybackSessionHandle {
    pub(crate) completion: SessionCompletion,
}

impl Default for PlaybackSessionHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaybackSessionHandle {
    /// A handle for one not-yet-activated episode. Pass it to
    /// [`crate::playback_session_spec`] when installing the session.
    pub fn new() -> Self {
        Self {
            completion: SessionCompletion::new(),
        }
    }

    /// Request the episode to stop. Command only: records stop intent
    /// and routes it to the session-owned data-plane stop. Idempotent
    /// and monotone; it never relabels an already-committed outcome.
    pub fn request_stop(&self) {
        self.completion.request_stop();
    }

    /// Request the episode to pause (ADR-PBK-002 D14.7). Command only:
    /// records pause intent and routes it to the episode's render gate;
    /// the mechanism parks its render leg before any device buffer is
    /// held. Same-episode, non-terminal: this never settles an outcome.
    /// Whether the episode actually establishes `paused()` is mechanism
    /// evidence the observation derives. Idempotent; inert history after
    /// settlement.
    pub fn request_pause(&self) {
        self.completion.request_pause();
    }

    /// Release a recorded pause (D14.7). Command only: clears pause
    /// intent and releases the gate; the render leg proceeds and the
    /// data plane decides what its next read sees. Idempotent; inert
    /// history after settlement.
    pub fn request_resume(&self) {
        self.completion.request_resume();
    }

    /// Request a seek to `target` — source-relative media time
    /// (ADR-PBK-002 D14.5). Infallible Command, non-negative by type;
    /// it records the seek and routes the cut's park to the render
    /// mechanism when — and only when — every frozen acceptance
    /// condition holds (episode unsettled, data plane Open, no stop
    /// intent, no seek already in flight). Invalid moments are inert,
    /// exactly like late stop/pause intent; a second seek while one is
    /// in flight is inert (one-seek policy — no queueing, no
    /// coalescing, no latest-wins, no request identity).
    ///
    /// THIS COMMAND IS NOT A CUTOVER, and its acceptance is not a
    /// success result: acceptance records intent only. The decoder
    /// reposition, the edge purge, the physical output cutover, and the
    /// actual landing stay separable protocol states owned by the
    /// session's own execution paths. There is deliberately no public
    /// positive seek state and no seek completion Fact: the observable
    /// consequences are the Position jump at the committed cutover
    /// (Position rebases to the decoder's ACTUAL landing — never the
    /// requested target — and withdraws for the episode if the landing
    /// was unknown) and, for a destructive provider failure, the
    /// ordinary terminal `Failed` through the existing D11 path.
    ///
    /// Pause intent survives a seek: a paused episode is seekable, the
    /// seek never implicitly resumes, and `paused()` keeps evaluating
    /// from the frozen D14.7 establishment chain.
    pub fn request_seek(&self, target: Duration) {
        self.completion.request_seek(target);
    }

    /// Request the episode's output level (ADR-PBK-002 D14.9): the
    /// App's desired stream factor, `0..=100` (clamped; values above
    /// 100 are capped at 100). Idempotent Command, same-episode,
    /// non-terminal: it routes into the session-owned output-level cell
    /// the mechanism applies at its loop top, and it NEVER establishes
    /// or settles terminal truth — no PCM topology cut, no edge flush,
    /// no position reset, no discontinuity. A volume command after the
    /// terminal Fact is inert command history, like late stop intent.
    /// The value means exactly the App's desired level — never
    /// the effective acoustic level, the Windows session master, or any
    /// mechanism readback. The FACTOR realization of that level is the
    /// perceptual taper [`desired_level_to_factor`] (field round 4:
    /// the former linear division put all the audible travel in the
    /// bottom quarter of the control).
    pub fn request_output_level(&self, level: u8) {
        self.completion
            .output_level()
            .route(desired_level_to_factor(level));
    }

    /// Set the desired Audio Processing configuration's enabled state
    /// (campaign #190 D4; `dsp-product-model.md` §7.3 processing
    /// enabled/bypass toggle). A typed, idempotent Command — never
    /// `set_dsp_parameter(name, value)`: the four live-authorized
    /// operations each have their own typed entry point, and no
    /// parameter addressing exists.
    ///
    /// The desired configuration is a WHOLE typed value; each `set_*`
    /// changes exactly the field(s) its name denotes (bypass keeps the
    /// gain/EQ fields in the desired configuration, inert) and the
    /// pending update is the resulting whole configuration, depth one,
    /// latest wins. The command boundary validates the candidate
    /// intrinsically: an invalid candidate is refused HERE with an
    /// honest diagnostic (returned and also observable through
    /// [`PlaybackSessionObservation::last_processing_refusal`]), and the
    /// old configuration keeps running bit-exactly.
    ///
    /// The three stages stay distinct (§7.3 vocabulary): an `Ok` here is
    /// a coherent DESIRED update (intrinsic validation only); the
    /// semantic ACCEPTANCE is the worker's pickup-time compile against
    /// the episode format; APPLY is the next whole staging block that
    /// has not yet been DSP-processed, through the authorized bounded
    /// crossfade — already-processed PCM is never reprocessed. The
    /// command is intent, not a claim that the sound has changed.
    /// Commands recorded after the terminal Fact are inert command
    /// history, like late stop intent.
    pub fn set_processing_enabled(&self, enabled: bool) -> Result<(), String> {
        self.completion.processing().set_enabled(enabled)
    }

    /// Set the desired preamp: the linear gain factor of the processing
    /// stage, in the same unit the frozen
    /// [`crate::processing::AudioProcessingConfig::gain`] field documents (`1.0` unity,
    /// `<1.0` attenuation, `0.0` true silence, `>1.0` positive gain —
    /// never clipped or limited here; presentation layers may display
    /// dB). Refused when the factor is not finite or negative.
    /// See [`PlaybackSessionHandle::set_processing_enabled`] for the
    /// command/acceptance/apply contract these setters share.
    pub fn set_preamp(&self, factor: f32) -> Result<(), String> {
        self.completion.processing().set_preamp(factor)
    }

    /// Set the desired custom EQ configuration (the 10-band product
    /// trims + peaking Q, [`EqConfig`]). Replaces the EQ field only; it
    /// does not implicitly toggle `enabled`, and it leaves the preamp
    /// as-is. See
    /// [`PlaybackSessionHandle::set_processing_enabled`] for the shared
    /// command/acceptance/apply contract.
    pub fn set_eq_config(&self, eq: EqConfig) -> Result<(), String> {
        self.completion.processing().set_eq_config(eq)
    }

    /// Select a factory preset as the desired configuration
    /// ([`EqPreset`]). A preset is a WHOLE recorded desired
    /// configuration ([`EqPreset::to_config`], its unity preamp
    /// included — exactly the resolution the establishment path uses),
    /// not a processor and not a partial patch. See
    /// [`PlaybackSessionHandle::set_processing_enabled`] for the shared
    /// command/acceptance/apply contract.
    pub fn set_eq_preset(&self, preset: EqPreset) -> Result<(), String> {
        self.completion.processing().set_eq_preset(preset)
    }

    /// Pure observation with the field coherence scope documented on
    /// [`PlaybackSessionObservation`]. No resolve,
    /// no commit, no lifecycle action, no edge or drain operation.
    /// Repeating it changes nothing.
    pub fn observe(&self) -> PlaybackSessionObservation {
        self.completion.observe_snapshot()
    }

    /// Block until the episode's terminal Fact is committed, then return
    /// its stable semantic outcome. Pure wait: settlement is
    /// authority-owned and is never triggered or advanced by this call.
    /// A failure diagnostic, if any, is read separately through
    /// [`PlaybackSessionHandle::observe`]; it is presentation, not
    /// semantics.
    pub fn wait_terminal(&self) -> EpisodeTerminalOutcome {
        let outcome: SessionOutcome = self.completion.wait_terminal();
        outcome.split().0
    }
}

/// The App's desired level (0..=100, the shell's ±5 steps) realized as
/// the stream factor the mechanism applies. A PERCEPTUAL taper, not a
/// linear division (field round 4: `level/100.0` made every press
/// above ~70 inaudible and crammed all the audible travel into the
/// bottom quarter — loudness perception is roughly logarithmic in
/// amplitude, which D14.9 explicitly declines to promise away: "no dB
/// curve promise" cuts both ways, and the listening release is the
/// evidence that the linear realization does not feel like a volume
/// control).
///
/// The frozen endpoints and step feel:
///
/// ```text
/// 100 → exactly 1.0  (unity: no attenuation at full, and the
///                      Tier-1 bit-transparent submission path is
///                      untouched at the top of the control)
///   0 → exactly 0.0  (true silence, not -inf dB arithmetic)
/// 1..=99 → 10^(-0.03 * (100 - level)): a −60 dB control range with
///          EXACTLY 3 dB per 5-step press — right at the just-
///          noticeable loudness difference, so a press feels alike
///          near the top and near the bottom
/// ```
///
/// 50 is therefore −30 dB, NOT half amplitude — and that is honest:
/// the displayed value is the App's desired CONTROL position (D14.9
/// forbids reading it as an acoustic level), and this mapping is what
/// makes equal control steps feel equal.
fn desired_level_to_factor(level: u8) -> f32 {
    match level.min(100) {
        0 => 0.0,
        100 => 1.0,
        l => 10f32.powf(-0.03 * f32::from(100 - l)),
    }
}

#[cfg(test)]
mod desired_level_tests {
    use super::desired_level_to_factor as factor;

    /// The frozen endpoints: unity at the top (the Tier-1 path stays
    /// bit-transparent at 100), true silence at the bottom.
    #[test]
    fn the_endpoints_are_exact() {
        assert_eq!(factor(100), 1.0);
        assert_eq!(factor(0), 0.0);
        assert_eq!(factor(255), 1.0, "values above 100 clamp to unity");
    }

    /// Exactly 3 dB per 5-step press across the whole control: the
    /// ratio of two factors 5 steps apart is 10^(∓3/20), everywhere
    /// (f32-relative tolerance — the exponent arithmetic wobbles ~1e-4).
    #[test]
    fn every_five_step_press_is_three_db() {
        let step_down = 10f32.powf(-3.0 / 20.0);
        for l in (5..=95).step_by(5) {
            let ratio = factor(l) / factor(l + 5);
            assert!(
                (ratio - step_down).abs() / step_down < 1e-3,
                "level {l}: ratio {ratio} != {step_down}"
            );
        }
    }

    /// Monotone and strictly decreasing below unity; the anchor points
    /// match the −60 dB range table (50 ≈ −30 dB).
    #[test]
    fn monotone_with_the_documented_anchors() {
        let mut prev = factor(100);
        assert_eq!(prev, 1.0);
        for l in (0..100).rev() {
            let f = factor(l);
            assert!(f <= prev, "factor rose at level {l}");
            prev = f;
        }
        let half = factor(50);
        let expect = 10f32.powf(-1.5); // −30 dB
        assert!(
            (half - expect).abs() < 1e-6,
            "level 50 factor {half} != {expect}"
        );
    }
}
