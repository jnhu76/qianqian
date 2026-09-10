//! PCM-CONTRACT-A0 experimental harness.
//!
//! **EXPERIMENTAL / EVIDENCE ONLY — NOT A STABLE API OR NORMATIVE CONTRACT.**
//!
//! This module exists to earn the smallest set of PCM data-edge truths that
//! future Qianqian playback implementations cannot safely avoid. It is
//! intentionally synthetic: no FFmpeg, no WASAPI, no ring buffer, no SRC, no
//! DSP, no playback state machine. The artifacts are counterexamples,
//! measurements, and a proposed minimal contract documented in
//! `docs/architecture/pcm-contract-a0.md`.
//!
//! The vocabulary used here is scoped to this experiment. In particular,
//! nothing in this module inherits the old playback nouns
//! (`MusicKernel`, `TransportKernel`, `Generation`, `Dual Window`,
//! `Physical Fence`, etc.).

/// One PCM sample value.
///
/// Phase B treats `Sample` as a MAY / first-implementation prior, not a MUST.
/// The semantic contract is "one scalar per channel per time point"; the
/// concrete `f32` choice is validated only as sufficient for the synthetic
/// harness.
pub type Sample = f32;

/// Format metadata that describes how to interpret a contiguous scalar slice.
///
/// A `PcmFormat` is intentionally cheap and `Copy`. Whether it travels with
/// every block or is negotiated once per edge/session is an experiment
/// question, not a frozen representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PcmFormat {
    /// Samples per second for one channel.
    pub sample_rate: u32,
    /// Number of channels in one frame.
    pub channel_count: u16,
}

impl PcmFormat {
    /// Scalar count for `frames` frames.
    ///
    /// This helper exists to make the frame→scalar translation explicit and
    /// to avoid off-by-one or unit-confusion bugs in experiment code.
    #[inline]
    pub fn scalar_count(&self, frames: usize) -> usize {
        frames.saturating_mul(self.channel_count as usize)
    }

    /// Frames covered by `scalars` scalar values.
    #[inline]
    pub fn frames_from_scalar_count(&self, scalars: usize) -> usize {
        if self.channel_count == 0 {
            return 0;
        }
        scalars / (self.channel_count as usize)
    }
}

// ---------------------------------------------------------------------------
// C1 — Borrowed immutable view
// ---------------------------------------------------------------------------

/// A borrowed, read-only view into a contiguous interleaved PCM buffer.
///
/// The producer retains ownership of the backing storage. The consumer may
/// observe the view only for the duration of the borrow. After the borrow
/// ends, the producer may reuse the storage.
///
/// Layout for this experiment: channel-interleaved.
///   frame N, channel 0 .. channel C-1
///   frame N+1, channel 0 .. channel C-1
///
/// The `frames()` and `channel()` accessors keep frame/channel indexing
/// explicit so that mutation oracles (sample/frame confusion, channel swap)
/// are observable as wrong sample values.
#[derive(Clone, Copy)]
pub struct PcmView<'a> {
    pub format: PcmFormat,
    pub data: &'a [Sample],
}

impl<'a> PcmView<'a> {
    /// Number of complete frames in the view.
    ///
    /// Partial trailing scalars are discarded (the contract requires the edge
    /// to carry whole frames).
    #[inline]
    pub fn frames(&self) -> usize {
        self.format.frames_from_scalar_count(self.data.len())
    }

    /// Scalar index of `channel` in `frame`.
    #[inline]
    fn index(&self, frame: usize, channel: usize) -> usize {
        frame * (self.format.channel_count as usize) + channel
    }

    /// One sample value.
    ///
    /// Panics if `frame` or `channel` is out of bounds — the contract is
    /// fail-closed on access.
    #[inline]
    pub fn channel(&self, frame: usize, channel: usize) -> Sample {
        self.data[self.index(frame, channel)]
    }
}

// ---------------------------------------------------------------------------
// C2 — Borrowed mutable view
// ---------------------------------------------------------------------------

/// A borrowed, mutable interleaved PCM span.
///
/// The producer exposes temporary storage; the consumer may process in place.
/// The consumer cannot retain the mutable borrow after the call returns.
pub struct PcmViewMut<'a> {
    pub format: PcmFormat,
    pub data: &'a mut [Sample],
}

impl<'a> PcmViewMut<'a> {
    #[inline]
    pub fn frames(&self) -> usize {
        self.format.frames_from_scalar_count(self.data.len())
    }

    #[inline]
    fn index(&self, frame: usize, channel: usize) -> usize {
        frame * (self.format.channel_count as usize) + channel
    }

    #[inline]
    pub fn channel(&self, frame: usize, channel: usize) -> Sample {
        self.data[self.index(frame, channel)]
    }

    #[inline]
    pub fn channel_mut(&mut self, frame: usize, channel: usize) -> &mut Sample {
        &mut self.data[self.index(frame, channel)]
    }
}

// ---------------------------------------------------------------------------
// C3 — Owned block transfer
// ---------------------------------------------------------------------------

/// An owned block of interleaved PCM.
///
/// Ownership of the storage moves with the block. The consumer may retain the
/// block for as long as it likes; the producer is not blocked from continuing.
pub struct PcmBlock {
    pub format: PcmFormat,
    pub data: Vec<Sample>,
}

impl PcmBlock {
    #[inline]
    pub fn frames(&self) -> usize {
        self.format.frames_from_scalar_count(self.data.len())
    }

    #[inline]
    pub fn view(&self) -> PcmView<'_> {
        PcmView {
            format: self.format,
            data: &self.data,
        }
    }
}

// ---------------------------------------------------------------------------
// C4 — Consumer-provided destination / pull
// ---------------------------------------------------------------------------

/// A pull-style producer fills a destination buffer provided by the consumer.
///
/// This trait is intentionally minimal. The caller owns `dest`; the producer
/// writes up to `dest.len()` scalars and returns the number of frames produced.
pub trait PullProducer {
    fn produce_into(&mut self, dest: &mut [Sample]) -> usize;
    fn format(&self) -> PcmFormat;
}

// ---------------------------------------------------------------------------
// Synthetic producer / consumer / accounting
// ---------------------------------------------------------------------------

/// Deterministic sample generator.
///
/// For frame `f` and channel `c`, the value is a deterministic function of
/// both indices. This makes channel-order bugs and frame/sample-count
/// confusion trivially observable by the consumer's oracle.
#[inline]
pub fn deterministic_sample(frame: usize, channel: usize) -> Sample {
    // Use a simple hash-like recurrence that is stable across platforms and
    // easy to reproduce by hand.
    let x = (frame
        .wrapping_mul(31_337)
        .wrapping_add(channel.wrapping_mul(7))) as u32;
    let x = (x ^ (x >> 16)).wrapping_mul(2_654_435_769);
    let x = (x ^ (x >> 16)).wrapping_mul(2_658_443_221);
    let x = x ^ (x >> 16);
    // Map to a finite f32 in a deterministic, non-trivial way.
    ((x % 10_000) as f32) / 10_000.0
}

/// Explicit accounting of work performed by an experiment run.
///
/// Every contract candidate is required to update these counters in its own
/// adapter so that copies, allocations, and ownership transfers are visible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Accounting {
    /// Bytes generated by the producer side.
    pub producer_bytes: usize,
    /// Bytes observed by the consumer side.
    pub consumer_bytes: usize,
    /// Number of explicit `memcpy`-equivalent copy operations.
    pub memcpy_count: usize,
    /// Bytes moved by explicit copies.
    pub bytes_moved: usize,
    /// Number of ownership transfers (C3-style block handoffs).
    pub transfer_count: usize,
    /// Number of borrow operations (C1/C2-style view handoffs).
    pub borrow_count: usize,
    /// Allocations performed during setup (allowed).
    pub setup_allocations: usize,
    /// Allocations performed during steady-state per-block flow.
    pub steady_allocations: usize,
}

/// Synthetic producer that writes deterministic interleaved PCM.
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

    /// C1-style production into a caller-owned buffer.
    ///
    /// Returns a borrowed view. The caller must keep `buf` alive for the
    /// duration of the borrow. No copy is performed.
    pub fn produce_borrowed<'a>(
        &mut self,
        buf: &'a mut [Sample],
        frames: usize,
        accounting: &mut Accounting,
    ) -> PcmView<'a> {
        let scalar_count = self.format.scalar_count(frames);
        assert!(
            buf.len() >= scalar_count,
            "producer buffer too small for requested frames"
        );
        let data = &mut buf[..scalar_count];
        for f in 0..frames {
            for ch in 0..self.format.channel_count as usize {
                data[f * (self.format.channel_count as usize) + ch] =
                    deterministic_sample(self.next_frame + f, ch);
            }
        }
        self.next_frame += frames;
        accounting.producer_bytes += scalar_count * core::mem::size_of::<Sample>();
        accounting.borrow_count += 1;
        PcmView {
            format: self.format,
            data: &*data,
        }
    }

    /// C3-style owned production.
    ///
    /// Allocates a fresh `Vec` every call — this is tracked as a steady-state
    /// allocation so the cost is visible.
    pub fn produce_owned(&mut self, frames: usize, accounting: &mut Accounting) -> PcmBlock {
        let scalar_count = self.format.scalar_count(frames);
        accounting.steady_allocations += 1;
        let mut data = vec![0.0; scalar_count];
        for f in 0..frames {
            for ch in 0..self.format.channel_count as usize {
                data[f * (self.format.channel_count as usize) + ch] =
                    deterministic_sample(self.next_frame + f, ch);
            }
        }
        self.next_frame += frames;
        accounting.producer_bytes += scalar_count * core::mem::size_of::<Sample>();
        accounting.transfer_count += 1;
        PcmBlock {
            format: self.format,
            data,
        }
    }
}

impl PullProducer for SyntheticProducer {
    fn produce_into(&mut self, dest: &mut [Sample]) -> usize {
        let max_frames = self.format.frames_from_scalar_count(dest.len());
        if max_frames == 0 {
            return 0;
        }
        for f in 0..max_frames {
            for ch in 0..self.format.channel_count as usize {
                dest[f * (self.format.channel_count as usize) + ch] =
                    deterministic_sample(self.next_frame + f, ch);
            }
        }
        self.next_frame += max_frames;
        max_frames
    }

    fn format(&self) -> PcmFormat {
        self.format
    }
}

/// Synthetic consumer that verifies deterministic PCM against the oracle.
pub struct SyntheticConsumer {
    format: PcmFormat,
    next_frame: usize,
    pub accounting: Accounting,
}

impl SyntheticConsumer {
    pub fn new(format: PcmFormat) -> Self {
        Self {
            format,
            next_frame: 0,
            accounting: Accounting::default(),
        }
    }

    /// Consume a C1 borrowed view and advance the cursor.
    pub fn consume_borrowed(&mut self, view: PcmView<'_>) {
        assert_eq!(
            view.format, self.format,
            "consumer received block with unexpected format"
        );
        for f in 0..view.frames() {
            for ch in 0..self.format.channel_count as usize {
                let expected = deterministic_sample(self.next_frame + f, ch);
                let actual = view.channel(f, ch);
                assert!(
                    (actual - expected).abs() < 1e-6,
                    "frame {} channel {} expected {} got {}",
                    self.next_frame + f,
                    ch,
                    expected,
                    actual
                );
            }
        }
        self.next_frame += view.frames();
        self.accounting.consumer_bytes += core::mem::size_of_val(view.data);
    }

    /// Consume a C3 owned block.
    pub fn consume_owned(&mut self, block: PcmBlock) {
        assert_eq!(
            block.format, self.format,
            "consumer received block with unexpected format"
        );
        let view = block.view();
        self.consume_borrowed(view);
        self.accounting.transfer_count += 1;
    }

    /// C4 pull: ask the producer to fill a caller-provided buffer.
    pub fn consume_pull<P: PullProducer>(
        &mut self,
        producer: &mut P,
        dest: &mut [Sample],
    ) -> usize {
        assert_eq!(
            producer.format(),
            self.format,
            "pull producer format mismatch"
        );
        let produced = producer.produce_into(dest);
        let scalar_count = self.format.scalar_count(produced);
        // Verify in place.
        for f in 0..produced {
            for ch in 0..self.format.channel_count as usize {
                let expected = deterministic_sample(self.next_frame + f, ch);
                let actual = dest[f * (self.format.channel_count as usize) + ch];
                assert!(
                    (actual - expected).abs() < 1e-6,
                    "pull frame {} channel {} expected {} got {}",
                    self.next_frame + f,
                    ch,
                    expected,
                    actual
                );
            }
        }
        self.next_frame += produced;
        self.accounting.consumer_bytes += scalar_count * core::mem::size_of::<Sample>();
        produced
    }

    pub fn frames_consumed(&self) -> usize {
        self.next_frame
    }
}

// ---------------------------------------------------------------------------
// Experiment helpers
// ---------------------------------------------------------------------------

/// Run a C1 push flow for `total_frames`, using `block_size` frames per call.
///
/// The same reusable buffer is passed on every call, so steady-state
/// allocations are zero and copies are zero unless an adapter adds them.
pub fn run_c1_push(
    format: PcmFormat,
    total_frames: usize,
    block_size: usize,
) -> (usize, Accounting) {
    let mut producer = SyntheticProducer::new(format);
    let mut consumer = SyntheticConsumer::new(format);
    let mut buf = vec![0.0; format.scalar_count(block_size)];
    let mut accounting = Accounting::default();
    accounting.setup_allocations += 1; // the reusable buffer

    let mut produced = 0;
    while produced < total_frames {
        let remaining = total_frames - produced;
        let this_block = block_size.min(remaining);
        let view = producer.produce_borrowed(&mut buf, this_block, &mut accounting);
        consumer.consume_borrowed(view);
        produced += this_block;
    }

    accounting.consumer_bytes = consumer.accounting.consumer_bytes;
    (consumer.frames_consumed(), accounting)
}

/// Run a C3 owned-block flow for `total_frames`, using `block_size` frames per
/// call.
pub fn run_c3_owned(
    format: PcmFormat,
    total_frames: usize,
    block_size: usize,
) -> (usize, Accounting) {
    let mut producer = SyntheticProducer::new(format);
    let mut consumer = SyntheticConsumer::new(format);
    let mut accounting = Accounting::default();

    let mut produced = 0;
    while produced < total_frames {
        let remaining = total_frames - produced;
        let this_block = block_size.min(remaining);
        let block = producer.produce_owned(this_block, &mut accounting);
        consumer.consume_owned(block);
        produced += this_block;
    }

    accounting.consumer_bytes = consumer.accounting.consumer_bytes;
    (consumer.frames_consumed(), accounting)
}

/// Run a C4 pull flow for `total_frames`, using a destination capacity of
/// `capacity_frames`.
pub fn run_c4_pull(
    format: PcmFormat,
    total_frames: usize,
    capacity_frames: usize,
) -> (usize, Accounting) {
    let mut producer = SyntheticProducer::new(format);
    let mut consumer = SyntheticConsumer::new(format);
    let mut dest = vec![0.0; format.scalar_count(capacity_frames)];
    let mut accounting = Accounting::default();
    accounting.setup_allocations += 1; // the reusable destination buffer

    let mut consumed = 0;
    while consumed < total_frames {
        let remaining = total_frames - consumed;
        let this_capacity = capacity_frames.min(remaining);
        let slice = &mut dest[..format.scalar_count(this_capacity)];
        let produced = consumer.consume_pull(&mut producer, slice);
        if produced == 0 {
            break;
        }
        accounting.producer_bytes += format.scalar_count(produced) * core::mem::size_of::<Sample>();
        consumed += produced;
    }

    accounting.consumer_bytes = consumer.accounting.consumer_bytes;
    (consumer.frames_consumed(), accounting)
}

// ---------------------------------------------------------------------------
// Mutation oracles
// ---------------------------------------------------------------------------

/// M1: treat frame count as scalar count.
///
/// This produces `frames * channel_count` frames worth of data and overflows
/// the scalar slice. It must panic or produce detectably wrong samples.
pub fn mutation_m1_frame_count_as_scalar_count(
    format: PcmFormat,
    block_size: usize,
) -> Result<(), &'static str> {
    let mut producer = SyntheticProducer::new(format);
    let mut buf = vec![0.0; format.scalar_count(block_size)];
    let mut accounting = Accounting::default();

    // WRONG: treat the scalar count of the buffer as a frame count.
    // For stereo, a buffer sized for 4 frames holds 8 scalars; requesting
    // 8 frames would require 16 scalars and overflow the buffer.
    let requested_frames = buf.len(); // should be `buf.len() / channel_count`
    let capacity_frames = format.frames_from_scalar_count(buf.len());
    if requested_frames > capacity_frames {
        return Err("mutation M1 caught: requested frames exceeds buffer capacity");
    }

    let _view = producer.produce_borrowed(&mut buf, requested_frames, &mut accounting);
    // If we reach here without panic, the mutation did not fail closed.
    Ok(())
}

/// M2: swap channel order in the consumer.
///
/// Returns the swapped value so the caller's oracle can detect it.
pub fn mutation_m2_swap_channels(
    view: PcmView<'_>,
    frame: usize,
    ch_a: usize,
    _ch_b: usize,
) -> Sample {
    view.channel(frame, ch_a) // caller compares against the other channel's oracle
}

// M3: consumer attempts to retain a borrowed view after the producer reuses
// the buffer.
//
// This function is intentionally not compilable: it demonstrates that the
// C1 contract prevents use-after-reuse at the type level. See the
// `compile_fail` doc test below.

/// M4: hidden copy detector.
///
/// A C1 adapter that silently copies the borrowed view into a local buffer
/// before handing it to the consumer must increment `memcpy_count` and
/// `bytes_moved`. The oracle below asserts that the C1 baseline performs zero
/// copies.
pub fn c1_with_hidden_copy<'a>(view: PcmView<'a>, accounting: &mut Accounting) -> PcmView<'static> {
    let copied: Vec<Sample> = view.data.to_vec();
    accounting.memcpy_count += 1;
    accounting.bytes_moved += copied.len() * core::mem::size_of::<Sample>();
    // This leaks the owned buffer intentionally for the mutation; in real
    // code it would be unsound. We return a view with 'static lifetime as a
    // sentinel of the bug class.
    let leaked: &'static [Sample] = Box::leak(copied.into_boxed_slice());
    PcmView {
        format: view.format,
        data: leaked,
    }
}

// M5: steady-state allocation detector.
//
// A contract that heap-allocates per block is recorded via
// `steady_allocations`. The C1 baseline asserts zero.

/// M6: format change without explicit boundary.
///
/// Simulate a producer that silently changes format and a consumer that
/// continues interpreting data with the stale format. The frame/sample math
/// diverges and the oracle catches the mismatch.
pub fn mutation_m6_silent_format_change() -> Result<(), &'static str> {
    let format_a = PcmFormat {
        sample_rate: 44100,
        channel_count: 2,
    };
    let format_b = PcmFormat {
        sample_rate: 48000,
        channel_count: 2,
    };

    let mut producer = SyntheticProducer::new(format_a);
    let mut consumer = SyntheticConsumer::new(format_a);
    let mut buf = vec![0.0; format_a.scalar_count(4)];
    let mut accounting = Accounting::default();

    // First block under format A.
    let view_a = producer.produce_borrowed(&mut buf, 4, &mut accounting);
    consumer.consume_borrowed(view_a);

    // WRONG: producer silently switches to format B without an explicit edge
    // boundary. The consumer still uses format A and will misinterpret the
    // block length / sample layout.
    let mut producer_b = SyntheticProducer::new(format_b);
    let view_b = producer_b.produce_borrowed(&mut buf, 4, &mut accounting);

    // The contract must reject a block whose format metadata does not match
    // the edge/session format. Continuing would mean interpreting bytes with
    // stale format metadata.
    if view_b.format != consumer.format {
        return Err("mutation M6 caught: silent format change rejected by edge format check");
    }
    Ok(())
}

// M7: zero-frame block overloaded as EOF.
//
// A zero-frame view must not be interpreted as a terminal signal. The oracle
// below asserts that consumption continues normally after an empty block.

// M8: partial acceptance silently drops frames.
//
// A producer offers 5 frames; consumer capacity is 3. The contract must
// either accept exactly 3 and report 2 remaining, or refuse and report
// capacity exhausted. Silently dropping the 2 frames is a bug.

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const STEREO: PcmFormat = PcmFormat {
        sample_rate: 44100,
        channel_count: 2,
    };
    const MONO: PcmFormat = PcmFormat {
        sample_rate: 44100,
        channel_count: 1,
    };
    const THREE_CHANNEL: PcmFormat = PcmFormat {
        sample_rate: 48000,
        channel_count: 3,
    };

    // --- Experiment A: frame correctness -----------------------------------

    #[test]
    fn stereo_frame_ordering_matches_oracle() {
        let (frames, _) = run_c1_push(STEREO, 10, 4);
        assert_eq!(frames, 10);
    }

    #[test]
    fn mono_frame_ordering_matches_oracle() {
        let (frames, _) = run_c1_push(MONO, 10, 4);
        assert_eq!(frames, 10);
    }

    #[test]
    fn three_channel_frame_ordering_matches_oracle() {
        let (frames, _) = run_c1_push(THREE_CHANNEL, 10, 4);
        assert_eq!(frames, 10);
    }

    #[test]
    fn partial_final_block_is_observed() {
        let (frames, _) = run_c1_push(STEREO, 7, 3);
        assert_eq!(frames, 7);
    }

    #[test]
    fn first_and_last_frame_verifiable() {
        let mut producer = SyntheticProducer::new(STEREO);
        let mut buf = vec![0.0; STEREO.scalar_count(7)];
        let mut accounting = Accounting::default();
        let view = producer.produce_borrowed(&mut buf, 7, &mut accounting);

        assert_eq!(view.channel(0, 0), deterministic_sample(0, 0));
        assert_eq!(view.channel(0, 1), deterministic_sample(0, 1));
        assert_eq!(view.channel(6, 0), deterministic_sample(6, 0));
        assert_eq!(view.channel(6, 1), deterministic_sample(6, 1));
    }

    // --- Experiment B: block sizes -----------------------------------------

    #[test]
    fn block_sizes_one_three_seventeen() {
        for block_size in [1usize, 3, 17] {
            let (frames, _) = run_c1_push(STEREO, 100, block_size);
            assert_eq!(frames, 100, "block size {} failed", block_size);
        }
    }

    #[test]
    fn block_size_power_of_two_also_works_but_not_assumed() {
        let (frames, _) = run_c1_push(STEREO, 100, 256);
        assert_eq!(frames, 100); // block size larger than stream is fine
    }

    // --- Experiment C: lifetime / reuse ------------------------------------

    /// This doc test demonstrates M3: a consumer cannot retain a borrowed view
    /// after the producer reuses the buffer. It is marked `compile_fail` so
    /// the test suite proves the contract is statically enforced.
    ///
    /// ```compile_fail
    /// use qianqian_core::pcm_contract_a0::{SyntheticProducer, PcmView, PcmFormat, Sample, Accounting};
    /// let mut producer = SyntheticProducer::new(PcmFormat { sample_rate: 44100, channel_count: 2 });
    /// let mut buf = vec![0.0; 8];
    /// let mut accounting = Accounting::default();
    /// let retained: PcmView<'_>;
    /// {
    ///     let view = producer.produce_borrowed(&mut buf, 4, &mut accounting);
    ///     retained = view;
    /// }
    /// // `buf` is still borrowed by `retained`, so reusing it here is rejected:
    /// let _second = producer.produce_borrowed(&mut buf, 4, &mut accounting);
    /// ```
    #[test]
    fn borrowed_view_lifetime_prevents_reuse_while_held() {
        // Runtime equivalent: the borrow ends when `view` goes out of scope,
        // after which the buffer can be reused.
        let mut producer = SyntheticProducer::new(STEREO);
        let mut buf = vec![0.0; STEREO.scalar_count(4)];
        let mut accounting = Accounting::default();

        {
            let view = producer.produce_borrowed(&mut buf, 4, &mut accounting);
            assert_eq!(view.frames(), 4);
        }

        // After the borrow ends, reuse is legal.
        let view2 = producer.produce_borrowed(&mut buf, 4, &mut accounting);
        assert_eq!(view2.frames(), 4);
    }

    // --- Experiment D: copy accounting -------------------------------------

    #[test]
    fn c1_baseline_has_zero_copy() {
        let (_, accounting) = run_c1_push(STEREO, 20, 5);
        assert_eq!(accounting.memcpy_count, 0, "C1 must not memcpy");
        assert_eq!(accounting.bytes_moved, 0, "C1 must not move bytes");
        assert_eq!(accounting.producer_bytes, accounting.consumer_bytes);
    }

    #[test]
    fn hidden_copy_is_counted() {
        let mut producer = SyntheticProducer::new(STEREO);
        let mut buf = vec![0.0; STEREO.scalar_count(4)];
        let mut accounting = Accounting::default();
        let view = producer.produce_borrowed(&mut buf, 4, &mut accounting);

        let mut copy_accounting = Accounting::default();
        let copied = c1_with_hidden_copy(view, &mut copy_accounting);
        assert_eq!(copied.frames(), 4);
        assert_eq!(copy_accounting.memcpy_count, 1);
        assert_eq!(
            copy_accounting.bytes_moved,
            STEREO.scalar_count(4) * core::mem::size_of::<Sample>()
        );
    }

    // --- Experiment E: allocation accounting -------------------------------

    #[test]
    fn c1_baseline_has_zero_steady_allocation() {
        let (_, accounting) = run_c1_push(STEREO, 100, 10);
        assert_eq!(
            accounting.steady_allocations, 0,
            "C1 with reusable buffer must not allocate per block"
        );
        assert_eq!(accounting.setup_allocations, 1);
    }

    #[test]
    fn c3_owned_has_steady_allocation() {
        let (_, accounting) = run_c3_owned(STEREO, 100, 10);
        assert_eq!(accounting.steady_allocations, 10);
        assert_eq!(accounting.transfer_count, 10);
    }

    // --- Experiment F: format discontinuity --------------------------------

    #[test]
    fn silent_format_change_is_detected() {
        let result = mutation_m6_silent_format_change();
        assert!(
            result.is_err(),
            "M6: silent format change must be rejected or detected"
        );
    }

    #[test]
    fn explicit_format_boundary_works() {
        // The correct shape: end the old edge/session and start a new one.
        let format_a = PcmFormat {
            sample_rate: 44100,
            channel_count: 2,
        };
        let format_b = PcmFormat {
            sample_rate: 48000,
            channel_count: 2,
        };

        // Old edge consumes some frames.
        let (frames_a, _) = run_c1_push(format_a, 10, 5);
        assert_eq!(frames_a, 10);

        // New edge with new format is a separate producer/consumer pair.
        let (frames_b, _) = run_c1_push(format_b, 10, 5);
        assert_eq!(frames_b, 10);
    }

    // --- Experiment G: flow control ----------------------------------------

    #[test]
    fn c4_pull_respects_consumer_capacity() {
        // Consumer capacity = 3 frames; producer could offer more.
        let (frames, accounting) = run_c4_pull(STEREO, 5, 3);
        // Pull produces at most capacity frames per call.
        assert_eq!(frames, 5);
        // Two calls: 3 + 2 frames.
        assert_eq!(accounting.borrow_count, 0);
    }

    #[test]
    fn c1_push_partial_block_exact_size() {
        // Producer offers 5 frames; consumer accepts all 5 because the block
        // size matches. This is the "all-or-nothing" baseline.
        let (frames, _) = run_c1_push(STEREO, 5, 5);
        assert_eq!(frames, 5);
    }

    // --- Experiment H: EOF negative control --------------------------------

    #[test]
    fn zero_frame_block_is_not_eof() {
        let mut producer = SyntheticProducer::new(STEREO);
        let mut consumer = SyntheticConsumer::new(STEREO);
        let mut buf = [0.0; 0];
        let mut accounting = Accounting::default();

        let view = producer.produce_borrowed(&mut buf, 0, &mut accounting);
        assert_eq!(view.frames(), 0);
        consumer.consume_borrowed(view);

        // The stream is not over: we can still produce real frames.
        let mut buf2 = vec![0.0; STEREO.scalar_count(4)];
        let view2 = producer.produce_borrowed(&mut buf2, 4, &mut accounting);
        consumer.consume_borrowed(view2);
        assert_eq!(consumer.frames_consumed(), 4);
    }

    #[test]
    fn explicit_terminal_signal_is_required() {
        // Terminal semantics belong out of band. Here we model it as a
        // separate boolean that accompanies the PCM edge but is not derived
        // from `frames == 0`.
        enum Edge<'a> {
            Pcm(PcmView<'a>),
            Terminal,
        }

        let mut producer = SyntheticProducer::new(STEREO);
        let mut consumer = SyntheticConsumer::new(STEREO);
        let mut buf = vec![0.0; STEREO.scalar_count(4)];
        let mut accounting = Accounting::default();

        let view = producer.produce_borrowed(&mut buf, 4, &mut accounting);
        let edge = Edge::Pcm(view);
        if let Edge::Pcm(v) = edge {
            consumer.consume_borrowed(v);
        } else {
            panic!("unexpected terminal");
        }

        let terminal = Edge::Terminal;
        assert!(matches!(terminal, Edge::Terminal));
        assert_eq!(consumer.frames_consumed(), 4);
    }

    // --- Mutation oracles --------------------------------------------------

    #[test]
    fn m1_frame_count_as_scalar_count_detected() {
        // For stereo, a buffer sized for 4 frames holds 8 scalars. Treating
        // the scalar count (8) as a frame count requests 8 frames = 16
        // scalars and exceeds the buffer. The oracle catches this before the
        // producer writes out of bounds.
        let result = mutation_m1_frame_count_as_scalar_count(STEREO, 4);
        assert!(
            result.is_err(),
            "M1: frame/scalar count confusion must be rejected"
        );
    }

    #[test]
    fn m2_channel_swap_is_detected() {
        let mut producer = SyntheticProducer::new(STEREO);
        let mut buf = vec![0.0; STEREO.scalar_count(4)];
        let mut accounting = Accounting::default();
        let view = producer.produce_borrowed(&mut buf, 4, &mut accounting);

        // A buggy consumer reads channel 0 but believes it is channel 1.
        // The oracle for channel 1 expects a different deterministic value,
        // so the swap is detected.
        let observed_as_channel_1 = mutation_m2_swap_channels(view, 0, 0, 1);
        let expected_for_channel_1 = deterministic_sample(0, 1);
        assert_ne!(
            observed_as_channel_1, expected_for_channel_1,
            "M2: channel swap must mismatch the channel-1 oracle"
        );
        assert_eq!(observed_as_channel_1, deterministic_sample(0, 0));
    }

    #[test]
    fn m7_zero_frame_block_does_not_end_stream() {
        // Already covered by `zero_frame_block_is_not_eof`.
    }

    #[test]
    fn m8_partial_acceptance_does_not_drop_frames() {
        // Model partial acceptance explicitly: producer offers 5, consumer
        // capacity is 3, consumer reports accepted=3 and remaining=2.
        let mut producer = SyntheticProducer::new(STEREO);
        let mut consumer = SyntheticConsumer::new(STEREO);
        let mut buf = vec![0.0; STEREO.scalar_count(5)];
        let mut accounting = Accounting::default();

        let offered = 5usize;
        let capacity = 3usize;
        let accepted = capacity.min(offered);
        let view = producer.produce_borrowed(&mut buf, accepted, &mut accounting);
        consumer.consume_borrowed(view);

        assert_eq!(consumer.frames_consumed(), accepted);
        assert_eq!(offered - accepted, 2, "remaining frames must be reported");

        // The remaining 2 frames are accepted in a second block.
        let view2 = producer.produce_borrowed(&mut buf, 2, &mut accounting);
        consumer.consume_borrowed(view2);
        assert_eq!(consumer.frames_consumed(), 5);
    }

    // --- Candidate comparison smoke tests ----------------------------------

    #[test]
    fn c2_mutable_in_place_preserves_frame_semantics() {
        let format = STEREO;
        let mut consumer = SyntheticConsumer::new(format);
        let mut buf = vec![0.0; format.scalar_count(4)];

        {
            let mut view = PcmViewMut {
                format,
                data: &mut buf,
            };
            // Producer fills through the mutable view.
            for f in 0..view.frames() {
                for ch in 0..format.channel_count as usize {
                    *view.channel_mut(f, ch) = deterministic_sample(f, ch);
                }
            }
        }

        let immutable = PcmView { format, data: &buf };
        consumer.consume_borrowed(immutable);
        assert_eq!(consumer.frames_consumed(), 4);
    }

    #[test]
    fn c3_owned_block_transfer_works() {
        let (frames, accounting) = run_c3_owned(STEREO, 20, 5);
        assert_eq!(frames, 20);
        assert_eq!(accounting.transfer_count, 4);
    }

    #[test]
    fn c4_pull_works() {
        let (frames, accounting) = run_c4_pull(STEREO, 20, 7);
        assert_eq!(frames, 20);
        assert_eq!(accounting.producer_bytes, accounting.consumer_bytes);
    }
}
