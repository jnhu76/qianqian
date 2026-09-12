//! Shared types and the deterministic sample oracle for the PCM edge
//! experiment.
//!
//! Test-only harness for the minimal PCM data-edge experiment (evidence, not
//! normative authority). Nothing here is part of the `qianqian-audio-api` library
//! API.
//!
//! Every payload construction is fail-closed: a shape that cannot be
//! interpreted as whole frames is rejected with [`PcmShapeError`] instead of
//! being silently truncated or reinterpreted.

use core::fmt;

/// One PCM sample value for this experiment.
///
/// `f32` is a first-implementation prior validated only as sufficient for
/// this synthetic harness; it is not a frozen production requirement.
pub type Sample = f32;

/// Why a payload shape cannot be interpreted as whole frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcmShapeError {
    /// Zero channels leave scalar-to-frame grouping undefined.
    ZeroChannelCount,
    /// The scalars do not partition into whole frames; the trailing partial
    /// frame must not be silently truncated.
    TrailingScalar { scalars: usize, channel_count: u16 },
}

impl fmt::Display for PcmShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            PcmShapeError::ZeroChannelCount => {
                write!(
                    f,
                    "channel count must be non-zero to group scalars into frames"
                )
            }
            PcmShapeError::TrailingScalar {
                scalars,
                channel_count,
            } => write!(
                f,
                "{scalars} scalars are not a whole number of {channel_count}-channel frames \
                 (trailing partial frame must be rejected, not truncated)"
            ),
        }
    }
}

impl std::error::Error for PcmShapeError {}

/// How a scalar slice is grouped into frames.
///
/// Zero channels are unrepresentable: [`PcmFormat::new`] rejects them, and the
/// fields are private so the invariant cannot be bypassed by struct literal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PcmFormat {
    sample_rate: u32,
    channel_count: u16,
}

impl PcmFormat {
    /// Fail-closed construction.
    pub fn new(sample_rate: u32, channel_count: u16) -> Result<Self, PcmShapeError> {
        if channel_count == 0 {
            return Err(PcmShapeError::ZeroChannelCount);
        }
        Ok(Self {
            sample_rate,
            channel_count,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channel_count(&self) -> u16 {
        self.channel_count
    }

    /// Scalar count for `frames` frames.
    ///
    /// Panics on address-space overflow: capacity math must fail loudly
    /// rather than silently saturate.
    pub fn scalar_count(&self, frames: usize) -> usize {
        frames
            .checked_mul(self.channel_count as usize)
            .expect("frame count overflows the scalar address space")
    }

    /// Frames covered by exactly `scalars` scalars.
    ///
    /// Returns [`PcmShapeError::TrailingScalar`] when the scalars do not
    /// partition into whole frames. Callers that merely own *capacity* (which
    /// may legally have slack) must use whole-frame capacity arithmetic
    /// instead of this payload check.
    pub fn frames_from_scalar_count(&self, scalars: usize) -> Result<usize, PcmShapeError> {
        let channels = self.channel_count as usize;
        if !scalars.is_multiple_of(channels) {
            return Err(PcmShapeError::TrailingScalar {
                scalars,
                channel_count: self.channel_count,
            });
        }
        Ok(scalars / channels)
    }
}

/// Verifier-side failure vocabulary: these errors *are* the experiment
/// oracles. A transfer that returns `Ok` without one of these having had the
/// chance to fire is not evidence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransferError {
    /// A block arrived whose format does not match the edge's established
    /// format context (the format-swap oracle).
    FormatMismatch { edge: PcmFormat, block: PcmFormat },
    /// A sample value disagrees with the deterministic oracle (the value
    /// oracle for frame/channel order and content).
    SampleMismatch {
        frame: usize,
        channel: usize,
        expected: Sample,
        actual: Sample,
    },
    /// A malformed payload shape reached transfer (the whole-frame oracle).
    MalformedShape(PcmShapeError),
    /// Transfer storage cannot hold the requested whole frames (the
    /// frame/scalar capacity oracle).
    DestinationTooSmall {
        required_scalars: usize,
        available_scalars: usize,
    },
    /// Fewer frames were delivered than were offered (the frame-conservation
    /// oracle).
    FramesLost { offered: usize, delivered: usize },
}

impl fmt::Display for TransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            TransferError::FormatMismatch { edge, block } => write!(
                f,
                "block format {block:?} does not match edge format {edge:?}"
            ),
            TransferError::SampleMismatch {
                frame,
                channel,
                expected,
                actual,
            } => write!(
                f,
                "frame {frame} channel {channel}: expected {expected}, got {actual}"
            ),
            TransferError::MalformedShape(err) => write!(f, "malformed payload: {err}"),
            TransferError::DestinationTooSmall {
                required_scalars,
                available_scalars,
            } => write!(
                f,
                "transfer needs {required_scalars} scalars, storage holds {available_scalars}"
            ),
            TransferError::FramesLost { offered, delivered } => {
                write!(f, "{delivered} of {offered} offered frames were delivered")
            }
        }
    }
}

impl std::error::Error for TransferError {}

impl From<PcmShapeError> for TransferError {
    fn from(err: PcmShapeError) -> Self {
        TransferError::MalformedShape(err)
    }
}

/// Deterministic sample oracle: a stable non-trivial value per
/// `(frame, channel)`, so channel-order and frame-count bugs are observable
/// as wrong values.
#[inline]
pub fn deterministic_sample(frame: usize, channel: usize) -> Sample {
    let x = (frame
        .wrapping_mul(31_337)
        .wrapping_add(channel.wrapping_mul(7))) as u32;
    let x = (x ^ (x >> 16)).wrapping_mul(2_654_435_769);
    let x = (x ^ (x >> 16)).wrapping_mul(2_658_443_221);
    let x = x ^ (x >> 16);
    ((x % 10_000) as f32) / 10_000.0
}

/// A borrowed read-only view over contiguous interleaved PCM.
///
/// The storage stays with its owner; the view is valid only for the borrow
/// duration, after which the owner may reuse the storage. Construction is
/// fail-closed via [`PcmView::new`].
#[derive(Clone, Copy, Debug)]
pub struct PcmView<'a> {
    format: PcmFormat,
    data: &'a [Sample],
}

impl<'a> PcmView<'a> {
    /// Constructs a view only over whole frames.
    pub fn new(format: PcmFormat, data: &'a [Sample]) -> Result<Self, PcmShapeError> {
        format.frames_from_scalar_count(data.len())?;
        Ok(Self { format, data })
    }

    pub fn format(&self) -> PcmFormat {
        self.format
    }

    /// Number of frames; exact by construction (never a truncation).
    pub fn frames(&self) -> usize {
        self.data.len() / self.format.channel_count as usize
    }

    /// Checked sample access.
    ///
    /// Returns `None` for an out-of-range frame *or channel*: an out-of-range
    /// channel must never silently resolve to a neighbouring frame's sample.
    pub fn sample(&self, frame: usize, channel: usize) -> Option<Sample> {
        let channels = self.format.channel_count as usize;
        if channel >= channels {
            return None;
        }
        let index = frame.checked_mul(channels)?.checked_add(channel)?;
        self.data.get(index).copied()
    }

    /// Base address of the backing storage, for the pointer-identity oracle
    /// that observes whether a transfer inserted intermediate storage.
    /// (Identity of storage only; this says nothing about CPU/cache-level
    /// data movement.)
    pub fn storage_ptr(&self) -> *const Sample {
        self.data.as_ptr()
    }

    /// The full whole-frame payload slice (validated at construction).
    pub fn payload(&self) -> &'a [Sample] {
        self.data
    }
}

/// A borrowed mutable interleaved PCM span: the producer fills through the
/// span and the consumer may process in place, but nobody can retain the
/// mutable borrow past its lifetime.
#[derive(Debug)]
pub struct PcmViewMut<'a> {
    format: PcmFormat,
    data: &'a mut [Sample],
}

impl<'a> PcmViewMut<'a> {
    /// Constructs a span only over whole frames.
    pub fn new(format: PcmFormat, data: &'a mut [Sample]) -> Result<Self, PcmShapeError> {
        format.frames_from_scalar_count(data.len())?;
        Ok(Self { format, data })
    }

    pub fn frames(&self) -> usize {
        self.data.len() / self.format.channel_count as usize
    }

    /// Checked in-place write access; `None` when out of range.
    pub fn sample_mut(&mut self, frame: usize, channel: usize) -> Option<&mut Sample> {
        let channels = self.format.channel_count as usize;
        if channel >= channels {
            return None;
        }
        let index = frame.checked_mul(channels)?.checked_add(channel)?;
        self.data.get_mut(index)
    }

    /// Ends the mutable span and hands back a read-only view of the same
    /// storage for verification.
    pub fn freeze(self) -> PcmView<'a> {
        PcmView {
            format: self.format,
            data: self.data,
        }
    }
}

/// An owned block of interleaved PCM: storage ownership moves with the block,
/// so producer and consumer lifetimes are decoupled.
pub struct PcmBlock {
    format: PcmFormat,
    data: Vec<Sample>,
}

impl PcmBlock {
    /// Constructs a block only over whole frames.
    pub fn new(format: PcmFormat, data: Vec<Sample>) -> Result<Self, PcmShapeError> {
        format.frames_from_scalar_count(data.len())?;
        Ok(Self { format, data })
    }

    pub fn view(&self) -> PcmView<'_> {
        PcmView {
            format: self.format,
            data: &self.data,
        }
    }
}

/// Pull-style edge: the consumer owns the destination; the producer fills at
/// most the whole frames the destination can hold and reports how many frames
/// it produced. Zero produced frames means "nothing produced now", never a
/// terminal signal. The edge's format context is established out-of-band by
/// the edge/session, not carried by this trait.
pub trait FillDestination {
    fn fill(&mut self, dest: &mut [Sample]) -> usize;
}

/// Deterministic interleaved-PCM generator (the experiment's producer side).
pub struct SyntheticProducer {
    format: PcmFormat,
    next_frame: usize,
}

impl SyntheticProducer {
    pub fn new(format: PcmFormat) -> Self {
        Self {
            format,
            next_frame: 0,
        }
    }

    pub fn format(&self) -> PcmFormat {
        self.format
    }

    /// Total frames this producer has generated (conservation cross-check
    /// against the consumer's `frames_consumed`).
    pub fn next_frame(&self) -> usize {
        self.next_frame
    }

    fn fill_frames(&self, data: &mut [Sample], first_frame: usize, frames: usize) {
        let channels = self.format.channel_count as usize;
        for f in 0..frames {
            for ch in 0..channels {
                data[f * channels + ch] = deterministic_sample(first_frame + f, ch);
            }
        }
    }

    /// Lends a read-only view after writing `frames` whole frames into a
    /// prefix of `storage`. Remaining capacity slack is untouched and is not
    /// part of the payload. Fails closed when `storage` is too small.
    pub fn lend_read_only<'a>(
        &mut self,
        storage: &'a mut [Sample],
        frames: usize,
    ) -> Result<PcmView<'a>, TransferError> {
        let scalars = self.format.scalar_count(frames);
        if storage.len() < scalars {
            return Err(TransferError::DestinationTooSmall {
                required_scalars: scalars,
                available_scalars: storage.len(),
            });
        }
        let (payload, _) = storage.split_at_mut(scalars);
        self.fill_frames(payload, self.next_frame, frames);
        self.next_frame += frames;
        Ok(PcmView::new(self.format, payload)?)
    }

    /// Lends a mutable span after writing `frames` whole frames into a
    /// prefix of `storage`, for in-place consumer processing.
    pub fn lend_in_place<'a>(
        &mut self,
        storage: &'a mut [Sample],
        frames: usize,
    ) -> Result<PcmViewMut<'a>, TransferError> {
        let scalars = self.format.scalar_count(frames);
        if storage.len() < scalars {
            return Err(TransferError::DestinationTooSmall {
                required_scalars: scalars,
                available_scalars: storage.len(),
            });
        }
        let (payload, _) = storage.split_at_mut(scalars);
        self.fill_frames(payload, self.next_frame, frames);
        self.next_frame += frames;
        Ok(PcmViewMut::new(self.format, payload)?)
    }

    /// Produces an owned block. Allocates one `Vec` per call — visible to a
    /// counting allocator as a per-transfer allocation; that cost is the
    /// honest trade-off of the owned shape, not a hidden accident.
    pub fn produce_owned(&mut self, frames: usize) -> PcmBlock {
        let scalars = self.format.scalar_count(frames);
        let mut data = vec![0.0; scalars];
        self.fill_frames(&mut data, self.next_frame, frames);
        self.next_frame += frames;
        PcmBlock::new(self.format, data).expect("producer emits whole frames by construction")
    }
}

impl FillDestination for SyntheticProducer {
    fn fill(&mut self, dest: &mut [Sample]) -> usize {
        // Whole-frame capacity only: trailing capacity slack stays with the
        // destination owner and is neither written nor read.
        let channels = self.format.channel_count as usize;
        let frames = dest.len() / channels;
        if frames == 0 {
            return 0;
        }
        let scalars = self.format.scalar_count(frames);
        let payload = &mut dest[..scalars];
        self.fill_frames(payload, self.next_frame, frames);
        self.next_frame += frames;
        frames
    }
}

/// Deterministic verifier (the experiment's consumer side). Every `verify_*`
/// method is an oracle: it checks format identity, then checks every sample
/// against [`deterministic_sample`] in order.
pub struct SyntheticConsumer {
    format: PcmFormat,
    next_frame: usize,
    observed_storage_addr: Option<usize>,
}

impl SyntheticConsumer {
    pub fn new(format: PcmFormat) -> Self {
        Self {
            format,
            next_frame: 0,
            observed_storage_addr: None,
        }
    }

    pub fn frames_consumed(&self) -> usize {
        self.next_frame
    }

    /// Storage address most recently observed through a verified payload, for
    /// the pointer-identity oracle.
    pub fn observed_storage_addr(&self) -> Option<usize> {
        self.observed_storage_addr
    }

    fn verify_view_in_order(&mut self, view: &PcmView<'_>) -> Result<(), TransferError> {
        if view.format() != self.format {
            return Err(TransferError::FormatMismatch {
                edge: self.format,
                block: view.format(),
            });
        }
        self.observed_storage_addr = Some(view.storage_ptr() as usize);
        let channels = self.format.channel_count as usize;
        for f in 0..view.frames() {
            for ch in 0..channels {
                let expected = deterministic_sample(self.next_frame + f, ch);
                // Checked access inside the verifier too: the oracle itself
                // must not silently alias an out-of-range channel.
                let actual = view.sample(f, ch).ok_or(TransferError::SampleMismatch {
                    frame: self.next_frame + f,
                    channel: ch,
                    expected,
                    actual: f32::NAN,
                })?;
                if (actual - expected).abs() > 1e-6 {
                    return Err(TransferError::SampleMismatch {
                        frame: self.next_frame + f,
                        channel: ch,
                        expected,
                        actual,
                    });
                }
            }
        }
        self.next_frame += view.frames();
        Ok(())
    }

    /// Verifies a borrowed view in stream order.
    pub fn verify_view(&mut self, view: &PcmView<'_>) -> Result<(), TransferError> {
        self.verify_view_in_order(view)
    }

    /// Verifies and consumes an owned block (ownership moves in and the
    /// block is released at the end of the call).
    pub fn consume_block(&mut self, block: PcmBlock) -> Result<(), TransferError> {
        let view = block.view();
        self.verify_view_in_order(&view)
    }

    /// Verifies the first `frames` whole frames of a filled pull destination.
    pub fn verify_filled(&mut self, dest: &[Sample], frames: usize) -> Result<(), TransferError> {
        let scalars = self.format.scalar_count(frames);
        if dest.len() < scalars {
            return Err(TransferError::DestinationTooSmall {
                required_scalars: scalars,
                available_scalars: dest.len(),
            });
        }
        let view = PcmView::new(self.format, &dest[..scalars])?;
        self.verify_view_in_order(&view)
    }
}
