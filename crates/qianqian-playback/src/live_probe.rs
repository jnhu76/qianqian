//! The disposable live-DSP transition probe (campaign #190 D3;
//! ADR-PBK-002 D14.11 live-update = OPEN, earned here or not at all).
//!
//! This module is `cfg(all(test, not(loom)))` — NEVER shipped, never a
//! product seam. It exists so the D3 oracles can drive the REAL
//! Playback Session processing seam (real worker loop, real staging
//! placement, real PcmEdge partial writes, real seek/pause/terminal
//! protocol) with a live-update capability whose semantics the campaign
//! must EARN before D4 productionizes the minimum:
//!
//! ```text
//! Desired      a pending AudioProcessingConfig in the depth-1 slot
//!              (latest write wins — replace-pending policy)
//! Accepted     a validated, episode-format-compiled update; the
//!              transition engine holds both processors and the
//!              applied-target identity
//! Applied      the update has reached the defined apply boundary: the
//!              first whole staging block that had not yet been
//!              DSP-processed when the update was polled
//! Transitioning old/new processor contributions coexist ONLY through
//!              the authorized bounded crossfade (weights on processed
//!              samples, never wall clock)
//! Settled      only the new configuration contributes
//! ```
//!
//! The engine is a deliberate probe, the same class as the I2
//! StatefulProbe: it carries MUTATION KNOBS (deliberate defects the
//! negative controls must catch) and event bookkeeping for the
//! block/apply-boundary oracles. The chosen transition model is the
//! dual-processor crossfade (Model C): both sides are REAL
//! `EpisodeProcessing` processors — the old one carrying its live
//! state, the new one starting from rest at the transition start — so
//! the post-transition continuation is exactly a fresh instance of the
//! accepted configuration, and every blend sample is exactly
//! `w·old + (1-w)·new` with a frame-indexed linear `w`.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use qianqian_audio_api::ports::PcmFormat;

use crate::processing::{AudioProcessingConfig, EpisodeProcessing};
use crate::session::ProcessingRuntime;

/// What one probe episode recorded, for the block-boundary oracles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProbeEvent {
    /// A transition started: its first block began at this processed
    /// frame and this staging-block index.
    TransitionStarted { at_frame: usize, block: usize },
    /// The transition settled: the first fully-new block begins at this
    /// processed frame / block index.
    TransitionCompleted { at_frame: usize, block: usize },
    /// A desired update was refused at the pickup point; the old
    /// configuration continues unchanged.
    UpdateRefused { at_frame: usize },
    /// An Applied seek dropped an in-flight transition (the accepted
    /// target stays the applied identity; fresh state post-cut).
    InvalidationDroppedTransition { at_frame: usize },
}

/// The probe's control half, shared with the test: request desired
/// updates, read refusals, and read the frame/block/event bookkeeping.
/// Cheap clones over Arcs.
#[derive(Clone)]
pub(crate) struct LiveProbeControl {
    slot: Arc<Mutex<Option<AudioProcessingConfig>>>,
    refusals: Arc<Mutex<Vec<String>>>,
    processed_frames: Arc<AtomicUsize>,
    blocks: Arc<AtomicUsize>,
    events: Arc<Mutex<Vec<ProbeEvent>>>,
    keep_transition_on_invalidate: Arc<AtomicBool>,
    wallclock_ramp: Arc<AtomicBool>,
}

impl LiveProbeControl {
    /// Desired update, depth-1 slot, LATEST WINS (overwrite): the
    /// replace-pending policy under rapid successive updates. An update
    /// sitting in the slot while a transition is in flight is NOT
    /// accepted until that transition completes (complete-in-flight).
    pub(crate) fn request_update(&self, desired: AudioProcessingConfig) {
        *self.slot.lock().unwrap() = Some(desired);
    }

    /// The pending slot content (for collision probes).
    pub(crate) fn pending(&self) -> Option<AudioProcessingConfig> {
        *self.slot.lock().unwrap()
    }

    pub(crate) fn take_refusals(&self) -> Vec<String> {
        std::mem::take(&mut *self.refusals.lock().unwrap())
    }

    pub(crate) fn processed_frames(&self) -> usize {
        self.processed_frames.load(Ordering::SeqCst)
    }

    pub(crate) fn blocks(&self) -> usize {
        self.blocks.load(Ordering::SeqCst)
    }

    pub(crate) fn events(&self) -> Vec<ProbeEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Deliberate-defect knobs for the D3.7 negative controls. Setting
    /// these makes the engine WRONG ON PURPOSE; every oracle that pins
    /// the honest behavior must then catch it.
    pub(crate) fn arm_mutations(&self, keep_transition_on_invalidate: bool, wallclock_ramp: bool) {
        self.keep_transition_on_invalidate
            .store(keep_transition_on_invalidate, Ordering::SeqCst);
        self.wallclock_ramp.store(wallclock_ramp, Ordering::SeqCst);
    }
}

/// Build a control half before establishment; the spec constructor
/// hands the same arcs to the engine at activation.
pub(crate) fn probe_control() -> LiveProbeControl {
    LiveProbeControl {
        slot: Arc::new(Mutex::new(None)),
        refusals: Arc::new(Mutex::new(Vec::new())),
        processed_frames: Arc::new(AtomicUsize::new(0)),
        blocks: Arc::new(AtomicUsize::new(0)),
        events: Arc::new(Mutex::new(Vec::new())),
        keep_transition_on_invalidate: Arc::new(AtomicBool::new(false)),
        wallclock_ramp: Arc::new(AtomicBool::new(false)),
    }
}

/// The crossfade state. Both sides are REAL episode processors: `from`
/// carries the pre-update signal history, `to` starts from rest at the
/// transition start (so post-settle continuation == a fresh instance of
/// the accepted configuration).
struct Transition {
    from: EpisodeProcessing,
    to: EpisodeProcessing,
    elapsed_frames: usize,
    total_frames: usize,
    scratch_in: Vec<f32>,
    /// The wall-clock witness of the previous stage call — used ONLY by
    /// the N4 mutation (a ramp advanced by wall time, the defect the
    /// pause oracle must catch).
    last_stage: Option<std::time::Instant>,
}

/// The disposable live-transition probe engine. Drives the real staging
/// seam through [`ProcessingRuntime`].
pub(crate) struct LiveProbeEngine {
    format: PcmFormat,
    channels: usize,
    steady: EpisodeProcessing,
    /// The ACCEPTED configuration: the transition target from the
    /// moment a transition starts, the realized configuration once
    /// settled.
    applied: AudioProcessingConfig,
    transition: Option<Transition>,
    transition_frames: usize,
    control: LiveProbeControl,
}

impl LiveProbeEngine {
    /// Compile the initial applied snapshot and bind the probe's
    /// control arcs. Fails establishment on an invalid initial
    /// configuration (the ordinary activation failure).
    pub(crate) fn new(
        initial: AudioProcessingConfig,
        format: &PcmFormat,
        transition_frames: usize,
        control: LiveProbeControl,
    ) -> Result<Self, String> {
        Self::from_parts(
            |format| EpisodeProcessing::new(&initial, format),
            initial,
            format,
            transition_frames,
            control,
        )
    }

    /// The failure-injection variant: the initial side is a DELIBERATE
    /// processor (the I1/I2 TestDriven injection), so a mid-transition
    /// stage failure is representable. The bookkeeping config is the
    /// config the processor realizes (for an oracle that recompiles).
    #[allow(dead_code)]
    pub(crate) fn with_initial_processor(
        steady: EpisodeProcessing,
        applied: AudioProcessingConfig,
        format: &PcmFormat,
        transition_frames: usize,
        control: LiveProbeControl,
    ) -> Result<Self, String> {
        Self::from_parts(|_| Ok(steady), applied, format, transition_frames, control)
    }

    fn from_parts(
        compile: impl FnOnce(&PcmFormat) -> Result<EpisodeProcessing, String>,
        applied: AudioProcessingConfig,
        format: &PcmFormat,
        transition_frames: usize,
        control: LiveProbeControl,
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
        })
    }

    /// The acceptance step at the fresh-block pickup: validate + compile
    /// the desired update against the episode format. A refusal is
    /// reported through the sink and the old configuration continues —
    /// NEVER a processing failure, never a silent fallback to a
    /// different sound.
    fn accept(&mut self, desired: AudioProcessingConfig) {
        let at_frame = self.control.processed_frames();
        let compiled = EpisodeProcessing::new(&desired, &self.format);
        match compiled {
            Err(diagnostic) => {
                self.control.refusals.lock().unwrap().push(diagnostic);
                self.control
                    .events
                    .lock()
                    .unwrap()
                    .push(ProbeEvent::UpdateRefused { at_frame });
            }
            Ok(to) => {
                // Move the live processor out as the crossfade's `from`
                // side. `steady` takes a placeholder that is NEVER
                // staged while a transition owns the stage path (and an
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
                self.control
                    .events
                    .lock()
                    .unwrap()
                    .push(ProbeEvent::TransitionStarted {
                        at_frame,
                        block: self.control.blocks(),
                    });
            }
        }
    }
}

impl ProcessingRuntime for LiveProbeEngine {
    fn stage(&mut self, block: &mut [f32]) -> Result<(), String> {
        let frames = block.len() / self.channels;
        self.control
            .processed_frames
            .fetch_add(frames, Ordering::SeqCst);
        self.control.blocks.fetch_add(1, Ordering::SeqCst);

        let Some(transition) = self.transition.as_mut() else {
            return self.steady.stage(block);
        };

        // Mutation N4 (wall-clock ramp): advance the ramp by the WALL
        // time since the previous stage call on top of the processed
        // frames — progress no processed PCM justifies (a paused worker
        // blocked on a full edge accrues a jump that lands at the first
        // post-resume block). The honest engine never does this.
        if self.control.wallclock_ramp.load(Ordering::SeqCst) {
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
        // input and leaves as the blend. Scratch grows once (transition
        // only — the steady path stays allocation-free).
        transition.scratch_in.clear();
        transition.scratch_in.extend_from_slice(block);

        transition.from.stage(block)?;
        transition.to.stage(&mut transition.scratch_in)?;

        for frame in 0..frames {
            let local = base + frame;
            let w = if local >= total {
                0.0f32
            } else {
                (1.0 - (local as f64 / total as f64)) as f32 + 0.0
            };
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
        if transition.elapsed_frames >= total {
            // Settle: the `to` side IS the post-transition continuation
            // — it has been running the accepted configuration from the
            // transition start, so keeping it makes the settled stream
            // exactly a fresh instance of the accepted config started at
            // the transition start (never a fresh compile HERE, which
            // would reset the state and click).
            let Transition { to, .. } = self.transition.take().expect("checked");
            self.steady = to;
            self.control
                .events
                .lock()
                .unwrap()
                .push(ProbeEvent::TransitionCompleted {
                    at_frame: self.control.processed_frames(),
                    block: self.control.blocks(),
                });
        }
        Ok(())
    }

    fn invalidate_signal_history(&mut self) {
        let at_frame = self.control.processed_frames();
        if self.transition.is_some() {
            if self
                .control
                .keep_transition_on_invalidate
                .load(Ordering::SeqCst)
            {
                // Mutation N3: the Applied cut fails to drop the
                // in-flight transition — pre-cut contribution leaks
                // into post-cut presentation. The oracles must catch it.
                return;
            }
            self.transition = None;
            self.control
                .events
                .lock()
                .unwrap()
                .push(ProbeEvent::InvalidationDroppedTransition { at_frame });
        }
        // Fresh-instance observational equivalence under the ACCEPTED
        // configuration: the accepted target was validated and compiled
        // at acceptance, so this recompile cannot fail.
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
        let desired = self.control.slot.lock().unwrap().take()?;
        self.accept(desired);
        Some(Ok(()))
    }
}

// --- the probe's real-machinery mutants (negative controls) -------------

/// MUTANT N1 — the update reprocesses the preserved remainder. The real
/// worker flushes the already-PROCESSED remainder untouched (D14.5/D14.11);
/// this mutant stages the remainder under the NEW configuration instead —
/// exactly the defect the remainder oracle must catch.
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

/// MUTANT N2 — the update applies incoherently: half the bands from the
/// desired configuration, half from the old one (the half-published
/// config). The output is a coherent render of the WRONG config, so the
/// realize-the-desired oracle must reject it.
#[cfg(all(test, not(loom)))]
pub(crate) fn mutant_mixed_config(
    old: &AudioProcessingConfig,
    desired: &AudioProcessingConfig,
) -> AudioProcessingConfig {
    let mut mixed = *desired;
    if let (Some(old_eq), Some(new_eq)) = (old.eq, desired.eq) {
        let mut bands = new_eq.band_gain_db;
        bands[5..10].copy_from_slice(&old_eq.band_gain_db[5..10]);
        mixed.eq = Some(crate::processing::EqConfig::new(bands, new_eq.q));
    }
    mixed
}
