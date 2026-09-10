//! Adversarial twins and the lookup-seam control for the direct PCM flow
//! experiment.
//!
//! Every twin is a real executable implementation of the honest shapes'
//! interfaces (or a deliberately divergent flow shell), and every kill test
//! reads its oracle from execution state — a seam log, a cursor mismatch, a
//! storage address, a measured allocation count, or a `rustc` rejection —
//! never from the twin's self-report. The mutation numbering used in the
//! evidence record's matrix lives there only; test names describe behavior.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::harness::{PcmFormat, PcmView, PcmViewMut, Sample, TransferError, deterministic_sample};
use super::participants::{ParticipantRole, PcmSink, PcmSource, PcmStage, VerifyingSink};

// ---------------------------------------------------------------------------
// The lookup seam and the anti-shape flow (per-quantum participant lookup)
// ---------------------------------------------------------------------------

/// One externally recorded participant-directory access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectoryAccess {
    pub ordinal: usize,
    pub role: ParticipantRole,
}

/// The control path's lookup seam: a fake participant directory that a flow
/// may consult at any time. It records every access from the seam itself;
/// flows cannot edit or suppress the record. This is a test-only double —
/// per-quantum participant lookup is not expressible through the kernel,
/// which is precisely the point of the comparison.
pub struct ParticipantDirectory {
    source: Rc<RefCell<dyn PcmSource>>,
    stage: Rc<RefCell<dyn PcmStage>>,
    sink: Rc<RefCell<dyn PcmSink>>,
    accesses: RefCell<Vec<DirectoryAccess>>,
    next_ordinal: Cell<usize>,
}

impl ParticipantDirectory {
    pub fn new(
        source: Rc<RefCell<dyn PcmSource>>,
        stage: Rc<RefCell<dyn PcmStage>>,
        sink: Rc<RefCell<dyn PcmSink>>,
    ) -> Rc<Self> {
        Rc::new(Self {
            source,
            stage,
            sink,
            accesses: RefCell::new(Vec::new()),
            next_ordinal: Cell::new(0),
        })
    }

    pub fn access_count(&self) -> usize {
        self.accesses.borrow().len()
    }

    pub fn accesses(&self) -> Vec<DirectoryAccess> {
        self.accesses.borrow().clone()
    }

    fn record(&self, role: ParticipantRole) {
        let ordinal = self.next_ordinal.get();
        self.next_ordinal.set(ordinal + 1);
        self.accesses
            .borrow_mut()
            .push(DirectoryAccess { ordinal, role });
    }

    pub fn lookup_source(&self) -> Rc<RefCell<dyn PcmSource>> {
        self.record(ParticipantRole::Source);
        self.source.clone()
    }

    pub fn lookup_stage(&self) -> Rc<RefCell<dyn PcmStage>> {
        self.record(ParticipantRole::Stage);
        self.stage.clone()
    }

    pub fn lookup_sink(&self) -> Rc<RefCell<dyn PcmSink>> {
        self.record(ParticipantRole::Sink);
        self.sink.clone()
    }
}

/// The anti-shape flow: delivers correct data, but every quantum re-resolves
/// all three participants through the directory instead of using the
/// pre-bound references it was handed at setup.
pub struct DirectoryLookupFlow {
    directory: Rc<ParticipantDirectory>,
    block_storage: Vec<Sample>,
}

impl DirectoryLookupFlow {
    pub fn new(
        source: Rc<RefCell<dyn PcmSource>>,
        stage: Rc<RefCell<dyn PcmStage>>,
        sink: Rc<RefCell<dyn PcmSink>>,
        block_storage_capacity_frames: usize,
        format: PcmFormat,
    ) -> Self {
        Self {
            directory: ParticipantDirectory::new(source, stage, sink),
            block_storage: vec![0.0; format.scalar_count(block_storage_capacity_frames)],
        }
    }

    pub fn run_quantum(&mut self, frames: usize) -> Result<(), TransferError> {
        let source = self.directory.lookup_source();
        let stage = self.directory.lookup_stage();
        let sink = self.directory.lookup_sink();
        let mut source = source.borrow_mut();
        let block = source.lend_next_block(&mut self.block_storage, frames)?;
        let processed = stage.borrow_mut().process(block);
        sink.borrow_mut().consume(&processed)
    }

    pub fn directory(&self) -> &ParticipantDirectory {
        &self.directory
    }
}

// ---------------------------------------------------------------------------
// Twin participants (each plugs into the honest flow as a stage/sink/source)
// ---------------------------------------------------------------------------

/// A stage whose `process` surface looks honest but whose body secretly
/// re-resolves through the directory on every call: a lookup hidden behind
/// a helper boundary.
pub struct HiddenLookupStage {
    directory: Rc<ParticipantDirectory>,
    delegate: Rc<RefCell<dyn PcmStage>>,
}

impl HiddenLookupStage {
    pub fn new(directory: Rc<ParticipantDirectory>, delegate: Rc<RefCell<dyn PcmStage>>) -> Self {
        Self {
            directory,
            delegate,
        }
    }
}

impl PcmStage for HiddenLookupStage {
    fn process<'block>(&mut self, block: PcmViewMut<'block>) -> PcmView<'block> {
        // Hidden runtime lookup behind an honest-looking helper signature.
        let _re_resolved = self.directory.lookup_stage();
        self.delegate.borrow_mut().process(block)
    }
}

/// Delivers a zero-frame block on exactly one chosen quantum: the frames the
/// source produced in that quantum never reach the sink, while every
/// delivered value stays oracle-correct. A mid-stream drop also shifts every
/// later frame against the sink's cursor, so the stream-order value oracle
/// fires there first; a final-block drop leaves the conservation cross-check
/// as the only witness. Both are real kills, asserted separately.
pub struct FrameDroppingStage {
    format: PcmFormat,
    drop_quantum: usize,
    quanta_seen: Cell<usize>,
}

impl FrameDroppingStage {
    pub fn new(format: PcmFormat, drop_quantum: usize) -> Self {
        Self {
            format,
            drop_quantum,
            quanta_seen: Cell::new(0),
        }
    }
}

impl PcmStage for FrameDroppingStage {
    fn process<'block>(&mut self, block: PcmViewMut<'block>) -> PcmView<'block> {
        let seen = self.quanta_seen.get() + 1;
        self.quanta_seen.set(seen);
        if seen == self.drop_quantum {
            let empty: &mut [Sample] = &mut [];
            PcmViewMut::new(self.format, empty)
                .expect("zero frames are a valid payload shape")
                .freeze()
        } else {
            block.freeze()
        }
    }
}

/// Swaps the samples of frames 0 and 1 inside every block of at least two
/// frames: an in-stream ordering violation with frame-correct content.
pub struct FramePermutingStage {
    channel_count: u16,
}

impl FramePermutingStage {
    pub fn new(format: PcmFormat) -> Self {
        Self {
            channel_count: format.channel_count(),
        }
    }
}

impl PcmStage for FramePermutingStage {
    fn process<'block>(&mut self, mut block: PcmViewMut<'block>) -> PcmView<'block> {
        if block.frames() >= 2 {
            let channels = self.channel_count as usize;
            for channel in 0..channels {
                let first = *block.sample_mut(0, channel).expect("frame 0 is in range");
                let second = *block.sample_mut(1, channel).expect("frame 1 is in range");
                *block.sample_mut(0, channel).expect("frame 0 is in range") = second;
                *block.sample_mut(1, channel).expect("frame 1 is in range") = first;
            }
        }
        block.freeze()
    }
}

/// A sink twin that re-consumes the first block it receives: a duplicate
/// delivery with frame-correct content.
pub struct ReplayingSink {
    inner: VerifyingSink,
    first_block_consumed: bool,
}

impl ReplayingSink {
    pub fn new(format: PcmFormat) -> Self {
        Self {
            inner: VerifyingSink::new(format),
            first_block_consumed: false,
        }
    }
}

impl PcmSink for ReplayingSink {
    fn consume(&mut self, block: &PcmView<'_>) -> Result<(), TransferError> {
        self.inner.consume(block)?;
        if !self.first_block_consumed {
            self.first_block_consumed = true;
            self.inner.consume(block)?;
        }
        Ok(())
    }

    fn frames_consumed(&self) -> usize {
        self.inner.frames_consumed()
    }

    fn observed_storage_addr(&self) -> Option<usize> {
        self.inner.observed_storage_addr()
    }
}

/// A processing twin that inserts its own scratch storage: the block's
/// samples are copied into storage the stage owns before the sink observes
/// them. The tested stage trait cannot express this shape (the block
/// lifetime is handed through by value), so this twin uses its own inherent
/// method — what insertion looks like wherever a shape permits it. Values
/// stay oracle-correct; only storage identity and the allocator can see it.
pub struct ScratchCopyingStage {
    format: PcmFormat,
    scratch: Vec<Sample>,
}

impl ScratchCopyingStage {
    pub fn new(format: PcmFormat) -> Self {
        Self {
            format,
            scratch: Vec::new(),
        }
    }

    pub fn process<'stage>(&'stage mut self, block: &PcmView<'_>) -> PcmView<'stage> {
        self.scratch = block.payload().to_vec();
        PcmView::new(self.format, &self.scratch).expect("the copied payload is whole frames")
    }

    pub fn scratch_addr(&self) -> usize {
        self.scratch.as_ptr() as usize
    }
}

/// A source twin that changes its declared format context mid-stream. Sample
/// values stay oracle-correct (the sample function does not encode the
/// rate); only the sink's format-identity check can see the change.
pub struct FormatSwappingSource {
    format: PcmFormat,
    swapped_sample_rate: u32,
    swap_at_quantum: usize,
    quanta_served: Cell<usize>,
    next_frame: usize,
}

impl FormatSwappingSource {
    pub fn new(format: PcmFormat, swapped_sample_rate: u32, swap_at_quantum: usize) -> Self {
        Self {
            format,
            swapped_sample_rate,
            swap_at_quantum,
            quanta_served: Cell::new(0),
            next_frame: 0,
        }
    }
}

impl PcmSource for FormatSwappingSource {
    fn lend_next_block<'storage>(
        &mut self,
        storage: &'storage mut [Sample],
        frames: usize,
    ) -> Result<PcmViewMut<'storage>, TransferError> {
        let channels = self.format.channel_count() as usize;
        let scalars = self.format.scalar_count(frames);
        if storage.len() < scalars {
            return Err(TransferError::DestinationTooSmall {
                required_scalars: scalars,
                available_scalars: storage.len(),
            });
        }
        for frame in 0..frames {
            for channel in 0..channels {
                storage[frame * channels + channel] =
                    deterministic_sample(self.next_frame + frame, channel);
            }
        }
        self.next_frame += frames;
        let served = self.quanta_served.get() + 1;
        self.quanta_served.set(served);
        if served >= self.swap_at_quantum {
            let swapped = PcmFormat::new(self.swapped_sample_rate, self.format.channel_count())
                .expect("the swapped format is valid");
            Ok(PcmViewMut::new(swapped, &mut storage[..scalars])?)
        } else {
            Ok(PcmViewMut::new(self.format, &mut storage[..scalars])?)
        }
    }

    fn frames_produced(&self) -> usize {
        self.next_frame
    }
}

// ---------------------------------------------------------------------------
// Kill tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod kill_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use qianqian_kernel::{DesiredEntry, Kernel, Revision};

    use super::{
        DirectoryLookupFlow, FormatSwappingSource, FrameDroppingStage, FramePermutingStage,
        HiddenLookupStage, ParticipantDirectory, ReplayingSink, ScratchCopyingStage,
    };
    use crate::composition::{
        CompositionFixture, compose_prebound_flow, directory_lookup_assembler_component,
        sink_provider_component, source_provider_component, stage_provider_component,
    };
    use crate::counting_allocator::run_counting_allocations;
    use crate::harness::{PcmFormat, TransferError};
    use crate::participants::{
        IdentityStage, ParticipantRole, PcmSink, PcmSource, PcmStage, StreamingSource,
        VerifyingSink,
    };

    fn stereo() -> PcmFormat {
        PcmFormat::new(48_000, 2).expect("stereo is a valid format")
    }

    /// Drives `total_frames` through the fixture's flow in `block_frames`-
    /// frame quanta (partial final quantum included) and then checks frame
    /// conservation from the participants' real cursor state.
    fn run_stream(
        fixture: &mut CompositionFixture,
        total_frames: usize,
        block_frames: usize,
    ) -> Result<(), TransferError> {
        while fixture.source.borrow().frames_produced() < total_frames {
            let remaining = total_frames - fixture.source.borrow().frames_produced();
            fixture.flow.run_quantum(remaining.min(block_frames))?;
        }
        let produced = fixture.source.borrow().frames_produced();
        let consumed = fixture.sink.borrow().frames_consumed();
        if produced != consumed {
            return Err(TransferError::FramesLost {
                offered: produced,
                delivered: consumed,
            });
        }
        Ok(())
    }

    /// The adversarial control: identical kernel-mediated setup, but the
    /// flow consults a participant directory every quantum. The value
    /// oracle still passes — data correctness alone cannot see the
    /// anti-shape. The seam-placed access log can.
    #[test]
    fn control_path_lookup_per_quantum_delivers_correct_data_but_violates_the_firewall() {
        let format = stereo();
        let source = Rc::new(RefCell::new(StreamingSource::new(format)));
        let sink = Rc::new(RefCell::new(VerifyingSink::new(format)));
        let flow_slot: Rc<RefCell<Option<DirectoryLookupFlow>>> = Rc::new(RefCell::new(None));
        let mut kernel = Kernel::new();
        kernel
            .register_component(source_provider_component(source.clone()))
            .expect("the source component registers");
        kernel
            .register_component(stage_provider_component(Rc::new(RefCell::new(
                IdentityStage,
            ))))
            .expect("the stage component registers");
        kernel
            .register_component(sink_provider_component(sink.clone()))
            .expect("the sink component registers");
        kernel
            .register_component(directory_lookup_assembler_component(
                flow_slot.clone(),
                4,
                format,
            ))
            .expect("the flow assembler registers");
        kernel
            .set_desired(vec![
                DesiredEntry::enabled("stream_source", "stream_source", Revision::fresh()),
                DesiredEntry::enabled("processing_stage", "processing_stage", Revision::fresh()),
                DesiredEntry::enabled("stream_sink", "stream_sink", Revision::fresh()),
                DesiredEntry::enabled("flow_assembler", "flow_assembler", Revision::fresh()),
            ])
            .expect("the desired composition is legal");
        kernel.settle();
        let mut flow = flow_slot
            .borrow_mut()
            .take()
            .expect("setup produced the control flow");

        let quanta = 10;
        for _ in 0..quanta {
            flow.run_quantum(4)
                .expect("the control delivers correct data");
        }
        assert_eq!(source.borrow().frames_produced(), quanta * 4);
        assert_eq!(sink.borrow().frames_consumed(), quanta * 4);

        // The lookups bypassed the kernel entirely: the anti-shape is a
        // per-quantum lookup seam of any kind, not the composition kernel.
        let kernel_ops_before = kernel.debug_op_count();
        let accesses = flow.directory().accesses();
        assert!(
            !accesses.is_empty(),
            "the firewall oracle fires: per-quantum directory access is visible at the seam"
        );
        assert_eq!(
            accesses.len(),
            quanta * 3,
            "every quantum looked up all three participants"
        );
        let roles: Vec<ParticipantRole> = accesses.iter().map(|a| a.role).collect();
        assert_eq!(roles.len(), quanta * 3);
        assert_eq!(
            kernel_ops_before,
            kernel.debug_op_count(),
            "reading the witness is not a kernel operation and the control did not touch the kernel"
        );
    }

    /// A lookup hiding behind an honest-looking `process` helper is still
    /// recorded at the seam — instrumentation strength does not depend on
    /// call-site shape.
    #[test]
    fn lookup_hidden_inside_processing_helper_is_detected() {
        let format = stereo();
        let source = Rc::new(RefCell::new(StreamingSource::new(format)));
        let real_stage: Rc<RefCell<dyn PcmStage>> = Rc::new(RefCell::new(IdentityStage));
        let sink = Rc::new(RefCell::new(VerifyingSink::new(format)));
        let directory = ParticipantDirectory::new(source.clone(), real_stage.clone(), sink.clone());
        let hidden = Rc::new(RefCell::new(HiddenLookupStage::new(
            directory.clone(),
            real_stage,
        )));
        let mut fixture = compose_prebound_flow(source, hidden, sink, format, 8);

        for _ in 0..10 {
            fixture
                .flow
                .run_quantum(8)
                .expect("data still flows correctly through the hidden-lookup stage");
        }
        assert_eq!(
            directory.access_count(),
            10,
            "the seam recorded one hidden lookup per quantum"
        );
        assert_eq!(fixture.sink.borrow().frames_consumed(), 80);
    }

    /// Silent loss of a mid-stream block keeps every delivered value correct
    /// — but the loss shifts every later frame against the sink's cursor, so
    /// the stream-order value oracle fires at the first post-drop block, and
    /// the conservation cross-check independently confirms the loss.
    #[test]
    fn silent_mid_stream_block_loss_is_killed_and_the_loss_is_visible_in_the_cursors() {
        let format = stereo();
        let mut fixture = compose_prebound_flow(
            Rc::new(RefCell::new(StreamingSource::new(format))),
            Rc::new(RefCell::new(FrameDroppingStage::new(format, 5))),
            Rc::new(RefCell::new(VerifyingSink::new(format))),
            format,
            4,
        );
        let outcome = run_stream(&mut fixture, 40, 4);
        assert!(
            outcome.is_err(),
            "an oracle must fire on silent mid-stream block loss"
        );
        let produced = fixture.source.borrow().frames_produced();
        let consumed = fixture.sink.borrow().frames_consumed();
        assert_ne!(
            produced, consumed,
            "the source and sink cursors prove frames were lost: {produced} produced, {consumed} consumed"
        );
    }

    /// Silent loss of the final block leaves no later frames to misalign, so
    /// the frame-conservation cross-check is the oracle that kills it.
    #[test]
    fn silent_final_block_loss_breaks_frame_conservation() {
        let format = stereo();
        let mut fixture = compose_prebound_flow(
            Rc::new(RefCell::new(StreamingSource::new(format))),
            Rc::new(RefCell::new(FrameDroppingStage::new(format, 10))),
            Rc::new(RefCell::new(VerifyingSink::new(format))),
            format,
            4,
        );
        let error = run_stream(&mut fixture, 40, 4).expect_err("the conservation oracle must fire");
        assert!(
            matches!(error, TransferError::FramesLost { delivered, .. } if delivered < 40),
            "silent loss is classified as frame loss: {error:?}"
        );
        assert_eq!(
            fixture.sink.borrow().frames_consumed(),
            36,
            "9 of 10 quanta reached the sink"
        );
    }

    /// Reordering frames inside a block fires the per-sample value oracle at
    /// the first misplaced sample.
    #[test]
    fn frame_reorder_inside_a_block_is_detected_by_the_value_oracle() {
        let format = stereo();
        let mut fixture = compose_prebound_flow(
            Rc::new(RefCell::new(StreamingSource::new(format))),
            Rc::new(RefCell::new(FramePermutingStage::new(format))),
            Rc::new(RefCell::new(VerifyingSink::new(format))),
            format,
            4,
        );
        let error =
            run_stream(&mut fixture, 40, 4).expect_err("the value oracle must fire on reorder");
        assert!(
            matches!(error, TransferError::SampleMismatch { .. }),
            "reorder is classified as a sample mismatch: {error:?}"
        );
    }

    /// Duplicate delivery of a block fires the per-sample value oracle when
    /// the sink's cursor has already moved past those frames.
    #[test]
    fn duplicate_block_delivery_is_detected_by_the_value_oracle() {
        let format = stereo();
        let mut fixture = compose_prebound_flow(
            Rc::new(RefCell::new(StreamingSource::new(format))),
            Rc::new(RefCell::new(IdentityStage)),
            Rc::new(RefCell::new(ReplayingSink::new(format))),
            format,
            4,
        );
        let error = run_stream(&mut fixture, 40, 4)
            .expect_err("the value oracle must fire on duplicate delivery");
        assert!(
            matches!(error, TransferError::SampleMismatch { .. }),
            "duplicate delivery is classified as a sample mismatch: {error:?}"
        );
    }

    /// Inserting intermediate storage leaves every value correct — and is
    /// still caught: the sink observes the scratch storage, not the source
    /// storage. The honest path's identity is pinned in main.rs.
    #[test]
    fn intermediate_storage_insertion_is_detected_by_storage_identity() {
        let format = stereo();
        let source = Rc::new(RefCell::new(StreamingSource::new(format)));
        let mut copying_stage = ScratchCopyingStage::new(format);
        let sink = Rc::new(RefCell::new(VerifyingSink::new(format)));
        let mut storage = vec![0.0; format.scalar_count(4)];

        for _ in 0..5 {
            let block = source
                .borrow_mut()
                .lend_next_block(&mut storage, 4)
                .expect("the source lend is valid");
            let frozen = block.freeze();
            let copied = copying_stage.process(&frozen);
            sink.borrow_mut()
                .consume(&copied)
                .expect("copied values are still oracle-correct");
        }
        assert_eq!(source.borrow().frames_produced(), 20);
        assert_eq!(sink.borrow().frames_consumed(), 20);

        let observed = sink
            .borrow()
            .observed_storage_addr()
            .expect("the sink observed storage");
        assert_ne!(
            observed,
            storage.as_ptr() as usize,
            "the source storage never reached the sink: insertion detected"
        );
        assert_eq!(
            observed,
            copying_stage.scratch_addr(),
            "the sink observed the stage's inserted scratch storage"
        );
    }

    /// The same copying twin, measured: insertion costs a heap allocation
    /// per quantum (the honest flow's measured zero is pinned in main.rs).
    #[test]
    fn per_quantum_allocation_is_measured_in_the_copying_twin() {
        let format = stereo();
        let source = Rc::new(RefCell::new(StreamingSource::new(format)));
        let mut copying_stage = ScratchCopyingStage::new(format);
        let sink = Rc::new(RefCell::new(VerifyingSink::new(format)));
        let mut storage = vec![0.0; format.scalar_count(4)];

        let block = source
            .borrow_mut()
            .lend_next_block(&mut storage, 4)
            .expect("the source lend is valid");
        let frozen = block.freeze();
        let copied = copying_stage.process(&frozen);
        sink.borrow_mut().consume(&copied).expect("warmup is valid");

        let (_, allocated) = run_counting_allocations(|| {
            for _ in 0..16 {
                let block = source
                    .borrow_mut()
                    .lend_next_block(&mut storage, 4)
                    .expect("the source lend is valid");
                let frozen = block.freeze();
                let copied = copying_stage.process(&frozen);
                sink.borrow_mut()
                    .consume(&copied)
                    .expect("copied values are oracle-correct");
            }
        });
        assert!(
            allocated >= 16,
            "a per-quantum copy is measurable: {allocated} allocations for 16 quanta"
        );
    }

    /// A mid-stream format change with oracle-correct values is still
    /// rejected at the sink: format identity is checked per block.
    #[test]
    fn mid_stream_format_change_is_rejected_at_the_sink() {
        let format = stereo();
        let mut fixture = compose_prebound_flow(
            Rc::new(RefCell::new(FormatSwappingSource::new(format, 96_000, 3))),
            Rc::new(RefCell::new(IdentityStage)),
            Rc::new(RefCell::new(VerifyingSink::new(format))),
            format,
            4,
        );
        let error =
            run_stream(&mut fixture, 40, 4).expect_err("the format-identity oracle must fire");
        assert!(
            matches!(error, TransferError::FormatMismatch { .. }),
            "the rate-only swap is classified as a format mismatch: {error:?}"
        );
    }
}
