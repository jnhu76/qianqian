//! Realtime participants and the pre-bound executable path for the direct
//! PCM flow experiment.
//!
//! Test-only harness (evidence, not normative authority). Nothing here is
//! part of the `qianqian-core` library API.
//!
//! The three participant roles — [`PcmSource`], [`PcmStage`], [`PcmSink`] —
//! are plain test-local traits over the borrowed, reused-storage PCM edge
//! prior inherited from the PCM edge experiment (reused verbatim through the
//! `harness` module). The representation is temporary: nothing here freezes
//! a production PCM type, layout, participant API, or threading model.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::harness::{
    PcmFormat, PcmView, PcmViewMut, Sample, SyntheticProducer, TransferError, deterministic_sample,
};

/// A realtime source participant: keeps its stream cursor across quanta and
/// lends the next block of whole frames into caller-provided reusable
/// storage, returning a mutable view over exactly the frames it produced.
///
/// The storage parameter keeps the lent view's lifetime outside the
/// participant's own borrow, which is what lets observation doubles
/// delegate transparently; the source still owns its cursor, and the flow
/// owns one reusable arena created at setup.
pub trait PcmSource {
    fn lend_next_block<'storage>(
        &mut self,
        storage: &'storage mut [Sample],
        frames: usize,
    ) -> Result<PcmViewMut<'storage>, TransferError>;

    /// Frames produced so far — the conservation cross-check readout.
    fn frames_produced(&self) -> usize;
}

/// A realtime processing-stage participant: receives the block by value and
/// hands the processed block onward through the return value.
///
/// The handed-through block lifetime is method-level (`for<'block>`), so a
/// compliant impl cannot retain the borrowed input block in `self` past the
/// call (type-system evidence for the tested representation).
///
/// This does NOT prove that the returned view aliases the input storage: a
/// compliant impl may legally construct new storage and return a view over
/// it. Storage provenance is a separate property from lifetime safety and
/// is checked by executable storage-identity and allocation oracles, not by
/// this signature.
pub trait PcmStage {
    fn process<'block>(&mut self, block: PcmViewMut<'block>) -> PcmView<'block>;
}

/// A realtime sink participant: verifies and consumes one block.
pub trait PcmSink {
    fn consume(&mut self, block: &PcmView<'_>) -> Result<(), TransferError>;

    /// Frames consumed so far — the conservation cross-check readout.
    fn frames_consumed(&self) -> usize;

    /// Storage address observed through the most recent block — the
    /// storage-identity readout. `None` before the first block.
    fn observed_storage_addr(&self) -> Option<usize>;
}

// ---------------------------------------------------------------------------
// Honest participants
// ---------------------------------------------------------------------------

/// The experiment's streaming source: the deterministic PCM-edge generator
/// wrapped as a streaming participant. Its cursor persists across quanta, so
/// blocks continue the same sample stream.
pub struct StreamingSource {
    generator: SyntheticProducer,
}

impl StreamingSource {
    pub fn new(format: PcmFormat) -> Self {
        Self {
            generator: SyntheticProducer::new(format),
        }
    }
}

impl PcmSource for StreamingSource {
    fn lend_next_block<'storage>(
        &mut self,
        storage: &'storage mut [Sample],
        frames: usize,
    ) -> Result<PcmViewMut<'storage>, TransferError> {
        self.generator.lend_in_place(storage, frames)
    }

    fn frames_produced(&self) -> usize {
        self.generator.next_frame()
    }
}

/// The identity processing stage: hands the incoming block onward unchanged.
pub struct IdentityStage;

impl PcmStage for IdentityStage {
    fn process<'block>(&mut self, block: PcmViewMut<'block>) -> PcmView<'block> {
        block.freeze()
    }
}

/// A minimal deterministic sample→sample transform: halves every sample of
/// the incoming block in place. Sufficient to prove that data genuinely
/// flows through the middle participant and can be touched there without
/// inserting storage; not a DSP commitment.
pub struct HalfAmplitudeStage {
    channel_count: u16,
}

impl HalfAmplitudeStage {
    pub fn new(format: PcmFormat) -> Self {
        Self {
            channel_count: format.channel_count(),
        }
    }
}

impl PcmStage for HalfAmplitudeStage {
    fn process<'block>(&mut self, mut block: PcmViewMut<'block>) -> PcmView<'block> {
        let channels = self.channel_count as usize;
        for frame in 0..block.frames() {
            for channel in 0..channels {
                if let Some(sample) = block.sample_mut(frame, channel) {
                    *sample *= 0.5;
                }
            }
        }
        block.freeze()
    }
}

/// The experiment's verifying sink: edge-format identity plus the per-sample
/// deterministic oracle, checked in stream order.
///
/// `sample_scale` is 1.0 for generator-identity payloads and 0.5 for the
/// output of the halving stage. The deterministic sample function is the
/// PCM edge experiment's oracle, inherited verbatim.
pub struct VerifyingSink {
    format: PcmFormat,
    sample_scale: f32,
    next_frame: usize,
    observed_storage_addr: Option<usize>,
    quanta_consumed: usize,
}

impl VerifyingSink {
    pub fn new(format: PcmFormat) -> Self {
        Self::with_sample_scale(format, 1.0)
    }

    pub fn with_sample_scale(format: PcmFormat, sample_scale: f32) -> Self {
        Self {
            format,
            sample_scale,
            next_frame: 0,
            observed_storage_addr: None,
            quanta_consumed: 0,
        }
    }

    fn verify_in_order(&mut self, block: &PcmView<'_>) -> Result<(), TransferError> {
        if block.format() != self.format {
            return Err(TransferError::FormatMismatch {
                edge: self.format,
                block: block.format(),
            });
        }
        self.observed_storage_addr = Some(block.storage_ptr() as usize);
        let channels = self.format.channel_count() as usize;
        for frame in 0..block.frames() {
            for channel in 0..channels {
                let expected =
                    deterministic_sample(self.next_frame + frame, channel) * self.sample_scale;
                let actual = block
                    .sample(frame, channel)
                    .ok_or(TransferError::SampleMismatch {
                        frame: self.next_frame + frame,
                        channel,
                        expected,
                        actual: f32::NAN,
                    })?;
                if (actual - expected).abs() > 1e-6 {
                    return Err(TransferError::SampleMismatch {
                        frame: self.next_frame + frame,
                        channel,
                        expected,
                        actual,
                    });
                }
            }
        }
        self.next_frame += block.frames();
        self.quanta_consumed += 1;
        Ok(())
    }
}

impl PcmSink for VerifyingSink {
    fn consume(&mut self, block: &PcmView<'_>) -> Result<(), TransferError> {
        self.verify_in_order(block)
    }

    fn frames_consumed(&self) -> usize {
        self.next_frame
    }

    fn observed_storage_addr(&self) -> Option<usize> {
        self.observed_storage_addr
    }
}

// ---------------------------------------------------------------------------
// The pre-bound executable path
// ---------------------------------------------------------------------------

/// The pre-bound executable path assembled once at setup: direct participant
/// references plus one reusable block-storage arena.
///
/// It holds no kernel handle, no registry, and no lookup seam of any kind,
/// so a quantum structurally cannot re-enter the composition plane. Every
/// field is a plain reference to a resolved participant; `run_quantum` is
/// the whole hot path.
pub struct PreboundPcmFlow {
    source: Rc<RefCell<dyn PcmSource>>,
    stage: Rc<RefCell<dyn PcmStage>>,
    sink: Rc<RefCell<dyn PcmSink>>,
    block_storage: Vec<Sample>,
}

impl PreboundPcmFlow {
    pub fn new(
        source: Rc<RefCell<dyn PcmSource>>,
        stage: Rc<RefCell<dyn PcmStage>>,
        sink: Rc<RefCell<dyn PcmSink>>,
        block_storage_capacity_frames: usize,
        format: PcmFormat,
    ) -> Self {
        Self {
            source,
            stage,
            sink,
            block_storage: vec![0.0; format.scalar_count(block_storage_capacity_frames)],
        }
    }

    /// Runs one PCM quantum: Source → Stage → Sink over pre-bound
    /// references, with no other work of any kind.
    pub fn run_quantum(&mut self, frames: usize) -> Result<(), TransferError> {
        let mut source = self.source.borrow_mut();
        let block = source.lend_next_block(&mut self.block_storage, frames)?;
        let processed = self.stage.borrow_mut().process(block);
        self.sink.borrow_mut().consume(&processed)
    }

    /// Base address of the flow's reusable block storage, the structural
    /// input for the storage-identity oracle.
    pub fn block_storage_addr(&self) -> usize {
        self.block_storage.as_ptr() as usize
    }
}

// ---------------------------------------------------------------------------
// Seam-placed observation doubles
// ---------------------------------------------------------------------------

/// Which participant role a seam observation refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticipantRole {
    Source,
    Stage,
    Sink,
}

/// Visit log shared by the observation doubles. The doubles live at the
/// participant seam (the flow binds them exactly like real participants),
/// so the flow neither sees nor influences the record; the test reads it
/// after the run.
pub type VisitLog = Rc<RefCell<Vec<ParticipantRole>>>;

pub fn visit_log() -> VisitLog {
    Rc::new(RefCell::new(Vec::new()))
}

/// Observation double before the source: delegates and records the visit.
pub struct ObservedSource {
    inner: Rc<RefCell<dyn PcmSource>>,
    log: VisitLog,
}

impl ObservedSource {
    pub fn new(inner: Rc<RefCell<dyn PcmSource>>, log: VisitLog) -> Self {
        Self { inner, log }
    }
}

impl PcmSource for ObservedSource {
    fn lend_next_block<'storage>(
        &mut self,
        storage: &'storage mut [Sample],
        frames: usize,
    ) -> Result<PcmViewMut<'storage>, TransferError> {
        self.log.borrow_mut().push(ParticipantRole::Source);
        self.inner.borrow_mut().lend_next_block(storage, frames)
    }

    fn frames_produced(&self) -> usize {
        self.inner.borrow().frames_produced()
    }
}

/// Observation double around the stage. Also counts frames in and out from
/// the seam, so the stage's frame accounting is externally observed.
pub struct ObservedStage {
    inner: Rc<RefCell<dyn PcmStage>>,
    log: VisitLog,
    frames_in: Cell<usize>,
    frames_out: Cell<usize>,
}

impl ObservedStage {
    pub fn new(inner: Rc<RefCell<dyn PcmStage>>, log: VisitLog) -> Self {
        Self {
            inner,
            log,
            frames_in: Cell::new(0),
            frames_out: Cell::new(0),
        }
    }

    pub fn frames_in(&self) -> usize {
        self.frames_in.get()
    }

    pub fn frames_out(&self) -> usize {
        self.frames_out.get()
    }
}

impl PcmStage for ObservedStage {
    fn process<'block>(&mut self, block: PcmViewMut<'block>) -> PcmView<'block> {
        self.log.borrow_mut().push(ParticipantRole::Stage);
        self.frames_in.set(self.frames_in.get() + block.frames());
        let processed = self.inner.borrow_mut().process(block);
        self.frames_out
            .set(self.frames_out.get() + processed.frames());
        processed
    }
}

/// Observation double before the sink: delegates and records the visit.
pub struct ObservedSink {
    inner: Rc<RefCell<dyn PcmSink>>,
    log: VisitLog,
}

impl ObservedSink {
    pub fn new(inner: Rc<RefCell<dyn PcmSink>>, log: VisitLog) -> Self {
        Self { inner, log }
    }
}

impl PcmSink for ObservedSink {
    fn consume(&mut self, block: &PcmView<'_>) -> Result<(), TransferError> {
        self.log.borrow_mut().push(ParticipantRole::Sink);
        self.inner.borrow_mut().consume(block)
    }

    fn frames_consumed(&self) -> usize {
        self.inner.borrow().frames_consumed()
    }

    fn observed_storage_addr(&self) -> Option<usize> {
        self.inner.borrow().observed_storage_addr()
    }
}
