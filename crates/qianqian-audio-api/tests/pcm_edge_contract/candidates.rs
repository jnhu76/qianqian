//! Candidate PCM edge shapes under comparison.
//!
//! Four transfer shapes, each driven as `new` (setup: storage allocation,
//! before any measurement window) plus `run` (steady-state transfer loop that
//! a measurement window can wrap):
//!
//! | semantic name                | shape                                        |
//! |------------------------------|----------------------------------------------|
//! | `BorrowedReadOnlyFlow`       | producer lends a read-only view (push)       |
//! | `BorrowedInPlaceFlow`        | producer lends a mutable span (push)         |
//! | `OwnedTransferFlow`          | ownership of the block moves to the consumer |
//! | `ConsumerFilledFlow`         | consumer owns the destination (pull)         |
//!
//! These shapes are experiment comparisons only; no shape here is a
//! production commitment.
//!
//! There are deliberately no cooperative copy/allocation counters: copies are
//! observed by storage pointer identity (see `FlowReport`) and allocations by
//! the counting allocator in the test binary, not by self-reported numbers.

use super::harness::{
    FillDestination, PcmFormat, Sample, SyntheticConsumer, SyntheticProducer, TransferError,
};

/// Structural observations about one flow run.
///
/// All fields are derived from execution, not self-reported by the code under
/// observation. Storage ownership is recorded per shape, never flattened: the
/// borrowed shapes have producer-owned reusable storage, the pull shape has a
/// consumer-owned destination, and the owned shape has no fixed storage
/// address on either side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlowReport {
    /// Transfer blocks handed across the edge.
    pub blocks: usize,
    /// Frames delivered across the edge.
    pub frames_delivered: usize,
    /// Address of the producer-owned reusable storage handed across the edge.
    /// Set only by the borrowed shapes; the pull shape has no producer-side
    /// storage and the owned shape has no fixed storage address at all.
    pub producer_storage_addr: Option<usize>,
    /// Address of the consumer-owned destination storage. Set only by the
    /// pull shape, where the destination is consumer property the producer
    /// fills in place.
    pub consumer_destination_addr: Option<usize>,
    /// Storage address the consumer observed through the most recent payload.
    pub consumer_observed_storage_addr: Option<usize>,
}

/// Push shape: producer reuses one buffer and lends a read-only view per
/// block. Consumer must finish with each view before the next block.
pub struct BorrowedReadOnlyFlow {
    producer: SyntheticProducer,
    consumer: SyntheticConsumer,
    storage: Vec<Sample>,
    block_frames: usize,
    report: FlowReport,
}

impl BorrowedReadOnlyFlow {
    pub fn new(format: PcmFormat, block_frames: usize) -> Self {
        Self {
            producer: SyntheticProducer::new(format),
            consumer: SyntheticConsumer::new(format),
            storage: vec![0.0; format.scalar_count(block_frames)],
            block_frames,
            report: FlowReport::default(),
        }
    }

    pub fn report(&self) -> FlowReport {
        self.report
    }

    pub fn consumer(&self) -> &SyntheticConsumer {
        &self.consumer
    }

    pub fn run(&mut self, total_frames: usize) -> Result<(), TransferError> {
        let Self {
            producer,
            consumer,
            storage,
            block_frames,
            report,
            ..
        } = self;
        report.producer_storage_addr = Some(storage.as_ptr() as usize);
        let mut delivered = 0;
        while delivered < total_frames {
            let this_block = (*block_frames).min(total_frames - delivered);
            let view = producer.lend_read_only(storage, this_block)?;
            consumer.verify_view(&view)?;
            delivered += this_block;
            report.blocks += 1;
        }
        report.frames_delivered = delivered;
        report.consumer_observed_storage_addr = consumer.observed_storage_addr();
        if delivered != total_frames {
            return Err(TransferError::FramesLost {
                offered: total_frames,
                delivered,
            });
        }
        Ok(())
    }
}

/// Push shape with in-place mutability: producer fills a mutable span; the
/// consumer may process in place before the span is released.
pub struct BorrowedInPlaceFlow {
    producer: SyntheticProducer,
    consumer: SyntheticConsumer,
    storage: Vec<Sample>,
    block_frames: usize,
    report: FlowReport,
}

impl BorrowedInPlaceFlow {
    pub fn new(format: PcmFormat, block_frames: usize) -> Self {
        Self {
            producer: SyntheticProducer::new(format),
            consumer: SyntheticConsumer::new(format),
            storage: vec![0.0; format.scalar_count(block_frames)],
            block_frames,
            report: FlowReport::default(),
        }
    }

    pub fn report(&self) -> FlowReport {
        self.report
    }

    pub fn run(&mut self, total_frames: usize) -> Result<(), TransferError> {
        let Self {
            producer,
            consumer,
            storage,
            block_frames,
            report,
            ..
        } = self;
        report.producer_storage_addr = Some(storage.as_ptr() as usize);
        let mut delivered = 0;
        while delivered < total_frames {
            let this_block = (*block_frames).min(total_frames - delivered);
            let span = producer.lend_in_place(storage, this_block)?;
            // The consumer side reads back through the frozen span over the
            // same storage; the in-place write path itself is exercised by
            // the adversarial twin in the mutations module.
            let view = span.freeze();
            consumer.verify_view(&view)?;
            delivered += this_block;
            report.blocks += 1;
        }
        report.frames_delivered = delivered;
        report.consumer_observed_storage_addr = consumer.observed_storage_addr();
        if delivered != total_frames {
            return Err(TransferError::FramesLost {
                offered: total_frames,
                delivered,
            });
        }
        Ok(())
    }
}

/// Push shape with decoupled lifetimes: each block is freshly allocated and
/// ownership moves to the consumer. The per-block allocation is the honest,
/// allocator-visible cost of decoupling.
pub struct OwnedTransferFlow {
    producer: SyntheticProducer,
    consumer: SyntheticConsumer,
    block_frames: usize,
    report: FlowReport,
}

impl OwnedTransferFlow {
    pub fn new(format: PcmFormat, block_frames: usize) -> Self {
        Self {
            producer: SyntheticProducer::new(format),
            consumer: SyntheticConsumer::new(format),
            block_frames,
            report: FlowReport::default(),
        }
    }

    pub fn report(&self) -> FlowReport {
        self.report
    }

    pub fn run(&mut self, total_frames: usize) -> Result<(), TransferError> {
        let Self {
            producer,
            consumer,
            block_frames,
            report,
        } = self;
        let mut delivered = 0;
        while delivered < total_frames {
            let this_block = (*block_frames).min(total_frames - delivered);
            let block = producer.produce_owned(this_block);
            consumer.consume_block(block)?;
            delivered += this_block;
            report.blocks += 1;
        }
        report.frames_delivered = delivered;
        report.consumer_observed_storage_addr = consumer.observed_storage_addr();
        if delivered != total_frames {
            return Err(TransferError::FramesLost {
                offered: total_frames,
                delivered,
            });
        }
        Ok(())
    }
}

/// Pull shape: the consumer owns the destination and offers whole-frame
/// capacity; the producer fills up to that capacity and reports how many
/// frames it produced.
pub struct ConsumerFilledFlow {
    producer: SyntheticProducer,
    consumer: SyntheticConsumer,
    dest: Vec<Sample>,
    capacity_frames: usize,
    report: FlowReport,
}

impl ConsumerFilledFlow {
    pub fn new(format: PcmFormat, capacity_frames: usize) -> Self {
        Self {
            producer: SyntheticProducer::new(format),
            consumer: SyntheticConsumer::new(format),
            dest: vec![0.0; format.scalar_count(capacity_frames)],
            capacity_frames,
            report: FlowReport::default(),
        }
    }

    pub fn report(&self) -> FlowReport {
        self.report
    }

    pub fn run(&mut self, total_frames: usize) -> Result<(), TransferError> {
        let Self {
            producer,
            consumer,
            dest,
            capacity_frames,
            report,
        } = self;
        report.consumer_destination_addr = Some(dest.as_ptr() as usize);
        let mut delivered = 0;
        while delivered < total_frames {
            let this_capacity = (*capacity_frames).min(total_frames - delivered);
            let offered_scalars = producer.format().scalar_count(this_capacity);
            let produced = {
                let slice = &mut dest[..offered_scalars];
                FillDestination::fill(producer, slice)
            };
            if produced == 0 {
                // Nothing produced now is not a terminal signal; but a
                // synthetic producer with capacity must produce, so stopping
                // here means frames were lost.
                break;
            }
            consumer.verify_filled(dest, produced)?;
            delivered += produced;
            report.blocks += 1;
        }
        report.frames_delivered = delivered;
        report.consumer_observed_storage_addr = consumer.observed_storage_addr();
        if delivered != total_frames {
            return Err(TransferError::FramesLost {
                offered: total_frames,
                delivered,
            });
        }
        Ok(())
    }
}
