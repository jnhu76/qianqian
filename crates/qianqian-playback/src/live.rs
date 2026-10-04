//! Live Audio Processing (campaign #190 D4): the smallest production
//! mechanism realizing the `dsp-product-model.md` §7.3 live-update
//! admission contract, and the only live mechanism the crate ships.
//!
//! ```text
//! ProcessingControl   product-control's desired/update state for one
//!                     episode: the desired whole configuration, the
//!                     depth-1 latest-wins pending slot, and the last
//!                     refusal diagnostic (mechanism evidence)
//! LiveProcessing      the episode-owned runtime: the applied
//!                     configuration's processor, the optional Model C
//!                     transition, and the pickup that accepts a
//!                     pending update at the fresh-staging-block
//!                     boundary
//! ```
//!
//! Semantics are frozen by the product authority, not here:
//!
//! - exactly four live-authorized operation classes (scalar
//!   Preamp/Gain, 10-band GEQ band gains, factory-preset switch,
//!   processing enabled/bypass toggle) — the handle's typed `set_*`
//!   commands;
//! - coherent acceptance: ONE whole valid configuration, validated
//!   intrinsically at the command boundary (refusal is honest,
//!   diagnostic, and leaves the old configuration running bit-exactly)
//!   and compiled against the episode format at the pickup;
//! - apply boundary: the next whole staging block that has not yet been
//!   DSP-processed; already-processed remainder PCM is written exactly
//!   as processed and is NEVER reprocessed;
//! - transition: Model C dual-processor crossfade, sample-driven —
//!   the old side carries its live state, the new side starts from rest
//!   at the transition start, so the settled stream is exactly a fresh
//!   instance of the accepted configuration started there;
//! - rapid updates: complete-in-flight, latest-wins pending depth one;
//! - seek: `RefusedUnchanged` preserves everything; `Applied` drops the
//!   transition and recompiles the accepted configuration from rest;
//! - failure after acceptance follows the ordinary D11 route.
//!
//! This module adds NO Plugin, Capability, registry, graph or parameter
//! bus: one mutex-guarded product-control cell and one episode-owned
//! runtime behind the existing [`crate::session::ProcessingRuntime`]
//! staging seam. The overlap is same-thread, same-owner and
//! episode-bounded, so PBK-001 P1–P5 are not triggered. The cell's lock
//! protects bounded critical-section work — the worker touches it once per
//! fresh block (one `Option::take`), and a command's critical section
//! is one fixed-size compose+validate+commit over `Copy` data (only a
//! REFUSED command allocates its diagnostic) — so the per-block
//! pickup performs no I/O or condvar wait while holding it. This bounds
//! work under the lock, not contention, scheduling or update completion time.

use std::sync::{Arc, Mutex};

use qianqian_audio_api::ports::PcmFormat;

use crate::presets::EqPreset;
use crate::processing::{AudioProcessingConfig, EpisodeProcessing, EqConfig};
use crate::session::ProcessingRuntime;

/// The live transition length, in milliseconds of SOURCE time. Product
/// tuning recorded with evidence (not authority): the crossfade is
/// sample-driven, and this constant only fixes how long the authorized
/// dual-processor overlap lasts. The D4 evidence (transition-continuity
/// and performance oracles) records the realized per-sample step against
/// the content's own slew at this duration for the sharpest authorized
/// configuration jump.
pub(crate) const LIVE_TRANSITION_MS: u32 = 50;

/// The transition length in source frames for one sample rate: never
/// zero (a zero-length transition would be the rejected instant switch).
pub(crate) fn live_transition_frames(sample_rate_hz: u32) -> usize {
    ((u64::from(sample_rate_hz) * u64::from(LIVE_TRANSITION_MS)) / 1000).max(1) as usize
}

/// Product-control's processing state for one episode. Shared (one
/// `Arc`) between the application-facing handle and the episode's
/// decode worker — the same ownership posture as the D14.9 output-level
/// cell, with the same guarantees: command state in transit, never a
/// Fact, never settlement input, never read on the per-block PCM path
/// (the pickup reads it once per whole staging block, at the fresh-block
/// boundary).
pub(crate) struct ProcessingControl {
    state: Mutex<ProcessingControlState>,
}

struct ProcessingControlState {
    /// The desired whole configuration: what product-control currently
    /// wants. Always intrinsically valid, except for the
    /// establishment-time seed, whose validation is the activation
    /// compile (the unchanged D14.11 establishment failure route).
    desired: AudioProcessingConfig,
    /// The pending update, depth one, latest wins: replaced by a newer
    /// desired update, taken exactly once by the worker's pickup.
    pending: Option<AudioProcessingConfig>,
    /// The most recent refusal diagnostic (mechanism evidence): a
    /// refused `set_*` records why here, so a client that did not
    /// capture the command's return value still sees the honest
    /// refusal. Cleared by the next intrinsically valid Desired record;
    /// worker Accepted/Applied happens later.
    last_refusal: Option<String>,
}

impl ProcessingControl {
    pub(crate) fn new(desired: AudioProcessingConfig) -> Self {
        Self {
            state: Mutex::new(ProcessingControlState {
                desired,
                pending: None,
                last_refusal: None,
            }),
        }
    }

    /// The currently desired configuration. Test-only read: production
    /// reads the desired state through [`Self::bind_for_activation`] (the
    /// one linearization point); the D5 read model is a separate product
    /// decision.
    #[cfg(all(test, not(loom)))]
    pub(crate) fn desired(&self) -> AudioProcessingConfig {
        self.state.lock().unwrap().desired
    }

    /// Bind the establishment-time desired configuration (the
    /// [`crate::session::playback_session_spec_with_processing`]
    /// constructor call, the unchanged D14.11 handoff representation).
    /// Deliberately UNVALIDATED: an invalid establishment configuration
    /// must still fail the activation cleanly through the compile, not
    /// be refused here where there is no error channel.
    ///
    /// The pending slot is for LIVE updates only — establishment binds
    /// `desired` directly (the engine compiles its initial snapshot from
    /// it at activation), so the slot is CLEARED here: any pre-activation
    /// pending value is superseded by the constructor's initial Desired
    /// configuration, not replayed as a phantom first transition.
    /// This local reset does not authorize reusing a handle for another
    /// episode; fresh-core attachment remains the caller's precondition.
    pub(crate) fn establish(&self, desired: AudioProcessingConfig) {
        let mut state = self.state.lock().unwrap();
        state.desired = desired;
        state.pending = None;
    }

    /// The activation linearization point: return the desired
    /// configuration the episode's initial processor must compile, and
    /// consume any pending update in the SAME lock hold. A `set_*`
    /// between establishment and activation therefore folds into the
    /// INITIAL applied configuration — the engine compiles exactly that
    /// configuration and no phantom initial→same transition starts; a
    /// `set_*` after this bind lands in the pending slot and reaches the
    /// worker's pickup as an ordinary §7.3 live update.
    pub(crate) fn bind_for_activation(&self) -> AudioProcessingConfig {
        let mut state = self.state.lock().unwrap();
        state.pending = None;
        state.desired
    }

    /// Scalar preamp change: the linear gain factor of the whole desired
    /// configuration (the same unit the frozen configuration field
    /// documents; presentation layers may display dB).
    pub(crate) fn set_preamp(&self, factor: f32) -> Result<(), String> {
        self.update_desired(|desired| desired.gain = factor)
    }

    /// 10-band GEQ band-gain change: replace the desired EQ
    /// configuration. Does not implicitly toggle `enabled` — a field
    /// operation never silently changes an unrelated field.
    pub(crate) fn set_eq_config(&self, eq: EqConfig) -> Result<(), String> {
        self.update_desired(|desired| desired.eq = Some(eq))
    }

    /// Factory-preset switch: install the preset's whole recorded
    /// desired configuration ([`EqPreset::to_config`] — its unity
    /// preamp included, exactly as the establishment path resolves it).
    pub(crate) fn set_eq_preset(&self, preset: EqPreset) -> Result<(), String> {
        self.update_desired(|desired| *desired = preset.to_config())
    }

    /// Processing enabled/bypass toggle. Bypass is a configuration, not
    /// a processor: the gain/EQ fields stay in the desired
    /// configuration and are inert until processing is enabled again.
    pub(crate) fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        self.update_desired(|desired| desired.enabled = enabled)
    }

    /// The typed commands' one write path: compose the candidate from
    /// the CURRENT desired configuration and commit it under ONE lock
    /// hold — compose, intrinsic validation, and the desired+pending
    /// move are atomic, so two racing field commands can never compose
    /// from the same stale snapshot and silently lose one command's
    /// field change (MUTANT N9 pins the defective split-lock shape).
    ///
    /// Vocabulary (truthful, per §7.3): an `Ok` here is a coherent
    /// DESIRED update recorded after intrinsic validation — NOT yet the
    /// semantic Acceptance, which is the pickup-time compile against the
    /// episode format, and NOT yet Applied, which is the fresh-block
    /// boundary.
    pub(crate) fn update_desired(
        &self,
        compose: impl FnOnce(&mut AudioProcessingConfig),
    ) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        let mut candidate = state.desired;
        compose(&mut candidate);
        if let Err(diagnostic) = candidate.validate() {
            state.last_refusal = Some(diagnostic.clone());
            return Err(diagnostic);
        }
        commit_desired(&mut state, candidate);
        Ok(())
    }

    /// Record a whole desired update at the command boundary: intrinsic
    /// validation of the WHOLE candidate first; a refusal records the
    /// honest diagnostic and leaves the desired/pending state untouched
    /// (the old configuration keeps running bit-exactly — never a
    /// partial config, never a silent fallback to a different sound).
    /// The semantic ACCEPTANCE (the format-dependent compile) happens at
    /// the worker's pickup; the APPLY happens at the fresh-block
    /// boundary. Test-only: the typed commands compose through
    /// [`Self::update_desired`]; this whole-config entry survives for
    /// the oracle harness and the N9 mutant world.
    #[cfg(all(test, not(loom)))]
    fn route(&self, candidate: AudioProcessingConfig) -> Result<(), String> {
        if let Err(diagnostic) = candidate.validate() {
            let mut state = self.state.lock().unwrap();
            state.last_refusal = Some(diagnostic.clone());
            return Err(diagnostic);
        }
        let mut state = self.state.lock().unwrap();
        commit_desired(&mut state, candidate);
        Ok(())
    }

    /// The worker-side pickup: take the pending update, latest wins.
    fn take_pending(&self) -> Option<AudioProcessingConfig> {
        self.state.lock().unwrap().pending.take()
    }

    /// Record a pickup-time compile refusal (defense in depth: after
    /// `route` validation this cannot happen for the fixed product
    /// table, and the old configuration continues if it ever does).
    fn record_refusal(&self, diagnostic: String) {
        self.state.lock().unwrap().last_refusal = Some(diagnostic);
    }

    /// The most recent refusal diagnostic, if any (mechanism evidence).
    pub(crate) fn last_refusal(&self) -> Option<String> {
        self.state.lock().unwrap().last_refusal.clone()
    }

    /// Test-only whole-configuration routing: the oracle harness drives
    /// the SAME coherent Desired recording the typed `set_*` commands use, so
    /// the D3 oracles exercise the production control path.
    #[cfg(all(test, not(loom)))]
    pub(crate) fn route_whole(&self, candidate: AudioProcessingConfig) -> Result<(), String> {
        self.route(candidate)
    }

    /// Test-only read of the pending slot (the depth-1 latest-wins
    /// coalescing oracle).
    #[cfg(all(test, not(loom)))]
    pub(crate) fn pending(&self) -> Option<AudioProcessingConfig> {
        self.state.lock().unwrap().pending
    }

    /// Test-only plant of the pending slot WITHOUT touching `desired`:
    /// the establishment-clears-the-slot oracle and the replacement-leak
    /// mutant world need a stale pending that no legitimate command
    /// would produce.
    #[cfg(all(test, not(loom)))]
    pub(crate) fn plant_pending(&self, stale: AudioProcessingConfig) {
        self.state.lock().unwrap().pending = Some(stale);
    }
}

/// The commit half of both command paths (caller holds the lock): the
/// coherent desired update moves desired and pending together, as ONE
/// whole configuration. Re-issuing the configuration that is ALREADY
/// desired is a no-op — an identical re-command must not plant a
/// self-crossfade between two identical processors, and the pending slot
/// (which always carries the latest recorded update) is left to the
/// pickup exactly as it was.
fn commit_desired(state: &mut ProcessingControlState, candidate: AudioProcessingConfig) {
    if candidate != state.desired {
        state.desired = candidate;
        state.pending = Some(candidate);
    }
    state.last_refusal = None;
}

/// The crossfade state. Both sides are REAL episode processors: `from`
/// carries the pre-update signal history, `to` starts from rest at the
/// transition start (so the settled continuation is exactly a fresh
/// instance of the accepted configuration).
struct Transition {
    from: EpisodeProcessing,
    to: EpisodeProcessing,
    elapsed_frames: usize,
    total_frames: usize,
    /// Transition-owned input copy, dropped when the transition settles.
    /// It may grow for successive larger blocks; in the production
    /// episode its size is bounded by the decode staging block. This
    /// does not bound allocation count or elapsed allocation time.
    /// The steady path does not use this scratch buffer.
    scratch_in: Vec<f32>,
    /// The wall-clock witness of the previous stage call — used ONLY by
    /// the N4 mutation (a ramp advanced by wall time, the defect the
    /// pause oracle must catch).
    #[cfg_attr(not(all(test, not(loom))), allow(dead_code))]
    last_stage: Option<std::time::Instant>,
}

/// The production live-processing runtime. Drives the real staging seam
/// through [`ProcessingRuntime`]; the no-transition path is the applied
/// processor's own stage (one pending-slot check per whole block).
pub(crate) struct LiveProcessing {
    format: PcmFormat,
    channels: usize,
    steady: EpisodeProcessing,
    /// The ACCEPTED configuration: the transition target from the moment
    /// a transition starts, the realized configuration once settled.
    applied: AudioProcessingConfig,
    transition: Option<Transition>,
    transition_frames: usize,
    control: Arc<ProcessingControl>,
    #[cfg(all(test, not(loom)))]
    tap: tap::LiveTap,
}

impl LiveProcessing {
    /// Compile the initial applied snapshot and bind product-control's
    /// state. Fails establishment on an invalid initial configuration
    /// (the ordinary activation failure, unchanged from D14.11).
    pub(crate) fn new(
        desired: AudioProcessingConfig,
        format: &PcmFormat,
        control: Arc<ProcessingControl>,
    ) -> Result<Self, String> {
        Self::from_parts(
            |format| EpisodeProcessing::new(&desired, format),
            desired,
            format,
            live_transition_frames(format.sample_rate),
            control,
            #[cfg(all(test, not(loom)))]
            tap::LiveTap::disarmed(),
        )
    }

    fn from_parts(
        compile: impl FnOnce(&PcmFormat) -> Result<EpisodeProcessing, String>,
        applied: AudioProcessingConfig,
        format: &PcmFormat,
        transition_frames: usize,
        control: Arc<ProcessingControl>,
        #[cfg(all(test, not(loom)))] tap: tap::LiveTap,
    ) -> Result<Self, String> {
        let steady = compile(format)?;
        Ok(Self {
            format: *format,
            channels: usize::from(format.channels),
            steady,
            applied,
            transition: None,
            transition_frames,
            control,
            #[cfg(all(test, not(loom)))]
            tap,
        })
    }

    /// The semantic ACCEPTANCE step at the fresh-block pickup: compile
    /// the desired update against the episode format. (Vocabulary: the
    /// command boundary's `Ok` is intrinsic validation of the desired
    /// update; THIS compile is what Accepts it; the staging boundary
    /// Applies it.) A refusal is recorded as mechanism evidence and the
    /// old configuration continues — NEVER a processing failure, never a
    /// silent fallback to a different sound.
    fn accept(&mut self, desired: AudioProcessingConfig) {
        let compiled = EpisodeProcessing::new(&desired, &self.format);
        match compiled {
            Err(diagnostic) => {
                self.control.record_refusal(diagnostic.clone());
                #[cfg(all(test, not(loom)))]
                self.tap.record(tap::LiveEvent::UpdateRefused);
            }
            Ok(to) => {
                // Move the live processor out as the crossfade's `from`
                // side. `steady` takes a placeholder that is NEVER staged
                // while a transition owns the stage path (and an
                // invalidation recompiles it from `applied`).
                let from = std::mem::replace(&mut self.steady, EpisodeProcessing::Bypass);
                self.applied = desired;
                self.transition = Some(Transition {
                    from,
                    to,
                    elapsed_frames: 0,
                    total_frames: self.transition_frames,
                    scratch_in: Vec::new(),
                    last_stage: None,
                });
                #[cfg(all(test, not(loom)))]
                self.tap.record(tap::LiveEvent::TransitionStarted {
                    at_frame: self.tap.processed_frames(),
                    block: self.tap.blocks(),
                });
            }
        }
    }
}

impl ProcessingRuntime for LiveProcessing {
    fn stage(&mut self, block: &mut [f32]) -> Result<(), String> {
        let frames = block.len() / self.channels;
        #[cfg(all(test, not(loom)))]
        {
            self.tap.advance(frames);
        }
        // Test-only defect knobs, read before the transition borrow.
        #[cfg(all(test, not(loom)))]
        let (wallclock_ramp, instant_bypass) = (
            self.tap.wallclock_ramp(),
            self.tap.instant_bypass() && !self.applied.enabled,
        );

        let Some(transition) = self.transition.as_mut() else {
            return self.steady.stage(block);
        };

        // Mutation N4 (wall-clock ramp): advance the ramp by the WALL
        // time since the previous stage call on top of the processed
        // frames — progress no processed PCM justifies. The honest
        // engine never does this.
        #[cfg(all(test, not(loom)))]
        if wallclock_ramp {
            let now = std::time::Instant::now();
            if let Some(last) = transition.last_stage {
                let wall_frames = (now.duration_since(last).as_secs_f64()
                    * f64::from(self.format.sample_rate))
                    as usize;
                transition.elapsed_frames += wall_frames;
            }
            transition.last_stage = Some(now);
        }

        let channels = self.channels;
        let total = transition.total_frames;
        let base = transition.elapsed_frames;
        // Both sides process the SAME input; the block arrives as the
        // input and leaves as the blend. Transition scratch may grow
        // as blocks require within the episode staging bound; the
        // steady path adds no scratch allocation.
        transition.scratch_in.clear();
        transition.scratch_in.extend_from_slice(block);

        transition.from.stage(block)?;
        transition.to.stage(&mut transition.scratch_in)?;

        for frame in 0..frames {
            let local = base + frame;
            // Mutation (D4.8, "bypass by dropping frames"): realize a
            // bypass transition as an instant dry switch — the
            // authorized crossfade's frames are dropped instead of
            // blended. The transition-continuity oracle must catch it.
            #[cfg(all(test, not(loom)))]
            let w = if instant_bypass {
                0.0
            } else {
                blend_weight(local, total)
            };
            #[cfg(not(all(test, not(loom))))]
            let w = blend_weight(local, total);
            for c in 0..channels {
                let i = frame * channels + c;
                let from_out = block[i];
                let to_out = transition.scratch_in[i];
                // w·from + (1-w)·to — the exact blend the oracles
                // recompute.
                block[i] = w.mul_add(from_out, (1.0 - w).mul_add(to_out, 0.0));
            }
        }
        transition.elapsed_frames = base + frames;
        let settled = transition.elapsed_frames >= total;
        if settled {
            // Settle: the `to` side IS the post-transition continuation
            // — it has been running the accepted configuration from the
            // transition start, so keeping it makes the settled stream
            // exactly a fresh instance of the accepted config started at
            // the transition start (never a fresh compile HERE, which
            // would reset the state and click).
            let Transition { to, .. } = self.transition.take().expect("checked");
            self.steady = to;
            #[cfg(all(test, not(loom)))]
            self.tap.record(tap::LiveEvent::TransitionCompleted {
                at_frame: self.tap.processed_frames(),
                block: self.tap.blocks(),
            });
        }
        Ok(())
    }

    fn invalidate_signal_history(&mut self) {
        #[cfg(all(test, not(loom)))]
        {
            if self.transition.is_some() {
                if self.tap.keep_transition_on_invalidate() {
                    // Mutation N3: the Applied cut fails to drop the
                    // in-flight transition — pre-cut contribution leaks
                    // into post-cut presentation. The oracles must catch
                    // it.
                    return;
                }
                self.tap
                    .record(tap::LiveEvent::InvalidationDroppedTransition);
            }
        }
        self.transition = None;
        // Fresh-instance observational equivalence under the ACCEPTED
        // configuration: the accepted target was validated and compiled
        // at acceptance, and this compile is a deterministic pure
        // function of (configuration, episode format), so it cannot
        // fail.
        let applied = self.applied;
        self.steady = EpisodeProcessing::new(&applied, &self.format)
            .expect("the accepted configuration compiled at acceptance");
    }

    fn poll_update(&mut self) -> Option<Result<(), String>> {
        // Complete-in-flight: an accepted transition finishes before the
        // next desired update is accepted (the slot keeps the latest).
        if self.transition.is_some() {
            return None;
        }
        let desired = self.control.take_pending()?;
        self.accept(desired);
        Some(Ok(()))
    }
}

/// The frame-indexed linear blend weight of the OLD side at processed
/// frame `local` of a `total`-frame transition: 1 → 0, exactly 0 from
/// `total` on (the settled stretch is the new side alone).
fn blend_weight(local: usize, total: usize) -> f32 {
    if local >= total {
        0.0
    } else {
        (1.0 - (local as f64 / total as f64)) as f32 + 0.0
    }
}

/// Test instrumentation for the D4 oracles: event/counter bookkeeping
/// (block and apply-boundary oracles) and the deliberate-defect knobs
/// the negative controls must catch. `cfg(all(test, not(loom)))` — never
/// shipped, no production cost, and the honest default is fully disarmed.
#[cfg(all(test, not(loom)))]
impl LiveProcessing {
    /// The tap-carrying construction (campaign #190 oracles): the same
    /// engine, with explicit transition geometry and the instrumentation
    /// half the tests read.
    pub(crate) fn with_tap(
        desired: AudioProcessingConfig,
        format: &PcmFormat,
        transition_frames: usize,
        control: Arc<ProcessingControl>,
        tap: tap::LiveTap,
    ) -> Result<Self, String> {
        Self::from_parts(
            |format| EpisodeProcessing::new(&desired, format),
            desired,
            format,
            transition_frames,
            control,
            tap,
        )
    }

    /// The failure-injection variant: the initial side is a DELIBERATE
    /// processor (the I1/I2 TestDriven injection), so a mid-transition
    /// stage failure is representable. The bookkeeping config is the
    /// config the processor realizes (for an oracle that recompiles).
    pub(crate) fn with_initial_processor(
        steady: EpisodeProcessing,
        applied: AudioProcessingConfig,
        format: &PcmFormat,
        transition_frames: usize,
        control: Arc<ProcessingControl>,
        tap: tap::LiveTap,
    ) -> Result<Self, String> {
        Self::from_parts(
            |_| Ok(steady),
            applied,
            format,
            transition_frames,
            control,
            tap,
        )
    }
}

#[cfg(all(test, not(loom)))]
pub(crate) mod tap {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    /// What one live episode recorded, for the block-boundary oracles.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum LiveEvent {
        /// A transition started: its first block began at this processed
        /// frame and this staging-block index.
        TransitionStarted { at_frame: usize, block: usize },
        /// The transition settled: the first fully-new block begins at
        /// this processed frame / block index.
        TransitionCompleted { at_frame: usize, block: usize },
        /// A desired update was refused at the pickup point.
        UpdateRefused,
        /// An Applied seek dropped an in-flight transition.
        InvalidationDroppedTransition,
    }

    /// The engine's instrumentation half, shared with the test.
    #[derive(Clone)]
    pub(crate) struct LiveTap {
        processed_frames: Arc<AtomicUsize>,
        blocks: Arc<AtomicUsize>,
        events: Arc<Mutex<Vec<LiveEvent>>>,
        keep_transition_on_invalidate: Arc<AtomicBool>,
        wallclock_ramp: Arc<AtomicBool>,
        instant_bypass: Arc<AtomicBool>,
    }

    impl LiveTap {
        /// The fully disarmed tap: the honest engine.
        pub(crate) fn disarmed() -> Self {
            Self {
                processed_frames: Arc::new(AtomicUsize::new(0)),
                blocks: Arc::new(AtomicUsize::new(0)),
                events: Arc::new(Mutex::new(Vec::new())),
                keep_transition_on_invalidate: Arc::new(AtomicBool::new(false)),
                wallclock_ramp: Arc::new(AtomicBool::new(false)),
                instant_bypass: Arc::new(AtomicBool::new(false)),
            }
        }

        pub(crate) fn processed_frames(&self) -> usize {
            self.processed_frames.load(Ordering::SeqCst)
        }

        pub(crate) fn blocks(&self) -> usize {
            self.blocks.load(Ordering::SeqCst)
        }

        pub(crate) fn events(&self) -> Vec<LiveEvent> {
            self.events.lock().unwrap().clone()
        }

        pub(crate) fn advance(&self, frames: usize) {
            self.processed_frames.fetch_add(frames, Ordering::SeqCst);
            self.blocks.fetch_add(1, Ordering::SeqCst);
        }

        pub(crate) fn record(&self, event: LiveEvent) {
            self.events.lock().unwrap().push(event);
        }

        pub(crate) fn keep_transition_on_invalidate(&self) -> bool {
            self.keep_transition_on_invalidate.load(Ordering::SeqCst)
        }

        pub(crate) fn wallclock_ramp(&self) -> bool {
            self.wallclock_ramp.load(Ordering::SeqCst)
        }

        pub(crate) fn instant_bypass(&self) -> bool {
            self.instant_bypass.load(Ordering::SeqCst)
        }

        /// Deliberate-defect knobs for the negative controls. Setting
        /// these makes the engine WRONG ON PURPOSE; every oracle that
        /// pins the honest behavior must then catch it.
        pub(crate) fn arm(
            &self,
            keep_transition_on_invalidate: bool,
            wallclock_ramp: bool,
            instant_bypass: bool,
        ) {
            self.keep_transition_on_invalidate
                .store(keep_transition_on_invalidate, Ordering::SeqCst);
            self.wallclock_ramp.store(wallclock_ramp, Ordering::SeqCst);
            self.instant_bypass.store(instant_bypass, Ordering::SeqCst);
        }
    }
}

// --- mutant worlds (negative controls) ---------------------------------

/// MUTANT N1 (D4.8, "apply update to processed remainder") — the update
/// reprocesses the preserved remainder. The real worker flushes the
/// already-PROCESSED remainder untouched (D14.5/D14.11); this mutant
/// stages the remainder under the NEW configuration instead — exactly
/// the defect the remainder oracle must catch.
#[cfg(all(test, not(loom)))]
pub(crate) fn mutant_remainder_reprocessed(
    remainder: &[f32],
    new_config: &AudioProcessingConfig,
    format: &PcmFormat,
) -> Vec<f32> {
    let mut data = remainder.to_vec();
    let mut new_side = EpisodeProcessing::new(new_config, format).expect("mutant compiles");
    new_side.stage(&mut data).expect("mutant stages");
    data
}

/// MUTANT N2 (D4.8, "half-publish EQ config") — the update applies
/// incoherently: half the bands from the desired configuration, half
/// from the old one. The output is a coherent render of the WRONG
/// config, so the realize-the-desired oracle must reject it.
#[cfg(all(test, not(loom)))]
pub(crate) fn mutant_mixed_config(
    old: &AudioProcessingConfig,
    desired: &AudioProcessingConfig,
) -> AudioProcessingConfig {
    let mut mixed = *desired;
    if let (Some(old_eq), Some(new_eq)) = (old.eq, desired.eq) {
        let mut bands = new_eq.band_gain_db;
        bands[5..10].copy_from_slice(&old_eq.band_gain_db[5..10]);
        mixed.eq = Some(EqConfig::new(bands, new_eq.q));
    }
    mixed
}

/// MUTANT (D4.8, "reset history on RefusedUnchanged") — the refused-seek
/// path resets the processing history instead of preserving it. The
/// honest continuation is the no-seek path bit-exactly; this mutant
/// world stages the post-refusal blocks through a fresh processor, so
/// the refusal oracle (content equality with the run's own no-seek
/// continuation, with real EQ state) must reject it.
#[cfg(all(test, not(loom)))]
pub(crate) fn mutant_reset_history(
    post_refusal: &[f32],
    applied: &AudioProcessingConfig,
    format: &PcmFormat,
) -> Vec<f32> {
    let mut data = post_refusal.to_vec();
    let mut fresh = EpisodeProcessing::new(applied, format).expect("mutant compiles");
    fresh.stage(&mut data).expect("mutant stages");
    data
}

/// MUTANT N9 (D4 review F1, "split-lock read-modify-write loses an
/// unrelated concurrent field update") — the defective command shape the
/// typed `set_*` commands must never express: read the desired
/// configuration under one lock, let an unrelated command commit IN
/// BETWEEN, then commit the STALE snapshot with only one field changed —
/// the intermediate command's field change is silently rolled back. The
/// production path ([`ProcessingControl::update_desired`]) composes and
/// commits under ONE lock hold, so this interleaving is inexpressible
/// there; the scripted world exists so the lost-update oracle can prove
/// its own sensitivity deterministically.
#[cfg(all(test, not(loom)))]
pub(crate) fn mutant_stale_snapshot_rmw(
    control: &ProcessingControl,
    between_locks: impl FnOnce(),
    compose: impl FnOnce(&mut AudioProcessingConfig),
) -> Result<(), String> {
    let mut stale = control.desired();
    between_locks();
    compose(&mut stale);
    control.route(stale)
}
