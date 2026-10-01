//! I2 StatefulProbe oracles (Issue #177 Stage 2 / I2; ADR-PBK-002
//! D14.11). I1's Gain is stateless, so it could not ask — and could not
//! answer — the question this phase exists for:
//!
//! > Does the production processing seam correctly carry STATEFUL DSP
//! > history through arbitrary block fragmentation, partial writes,
//! > seek, pause and episode replacement?
//!
//! The probe is a deliberately simple deterministic one-state causal
//! recurrence per channel,
//!
//! ```text
//! y[n] = 0.5·x[n] + 0.5·y[n-1]      (seed y[-1] = 0.0 per episode)
//! ```
//!
//! — a stable one-pole low-pass (|b| = 0.5 < 1: every state perturbation
//! decays, values stay finite and bounded by the input magnitude), with
//! a trivial scalar oracle and meaningful history. It is TEST-ONLY
//! evidence, injected through the cfg(test) `TestDriven` processor via
//! the real composition (real activation, legs, seek/pause protocol);
//! it is not a product effect and exposes no product surface.
//!
//! Exactness argument for the `==` oracles: the recurrence's per-sample
//! operation sequence (`0.5·x + 0.5·y`, f32, in order) does not depend
//! on how samples are grouped into staging blocks, and the reference
//! below mirrors that exact expression — so fragmentation invariance,
//! the refusal continuation and the fresh-landing equivalence are all
//! pinned BIT-EXACTLY, the strongest form available. The decode double
//! tags frame `i` of channel 0 with `i as f32` and channel 1 with
//! `i as f32 + 0.5` (exact for `i < 2^23`; every source here is far
//! below that), and the probe carries one independent state per channel
//! — the shape a real stateful EQ (I3) will own.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use qianqian_audio_api::ports::ProviderSeekOutcome;

use crate::handle::EpisodeTerminalOutcome;
use crate::processing::EpisodeProcessing;
use crate::processing_support::{
    EIGHT_SECONDS, TEST_RATE, episode_with_test_processor, rejects, wait_until,
};
use crate::test_common::{self, OutputBehavior};

/// The probe's channel count — the test format's stereo layout. The
/// staging blocks the worker feeds are always frame-aligned.
const CHANNELS: usize = 2;

/// The deliberate defects the mutation knobs model. Each one is a REAL
/// run through the real pipeline (the honest pipeline code is unchanged;
/// the test-only probe misbehaves), so the oracles are demonstrated to
/// fail on the exact defect class they exist to catch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mutation {
    /// The honest probe.
    None,
    /// The recurrence step runs twice per sample — state double-advance
    /// (the "reprocess/double-advance" defect class).
    DoubleAdvance,
    /// The state resets at every stage call — block-boundary-dependent
    /// processing (the fragmentation defect class).
    ResetEachCall,
    /// [`StatefulProbe::invalidate_history`] does nothing — a processing
    /// runtime that fails to invalidate pre-cut history on an Applied
    /// seek (the D14.11 Applied defect class). The production
    /// invalidation call still happens; the probe ignores it.
    PreserveAcrossApplied,
}

/// The one-state causal probe. `y` is per channel; the emitted sample
/// IS the state variable, which is what makes the scalar oracles exact.
struct StatefulProbe {
    y: [f32; CHANNELS],
    mutation: Mutation,
}

impl StatefulProbe {
    fn new(mutation: Mutation) -> Self {
        Self {
            y: [0.0; CHANNELS],
            mutation,
        }
    }

    /// THE probe recurrence — the exact expression the reference
    /// mirrors (see the module-doc exactness argument).
    fn step(&mut self, x: f32, c: usize) -> f32 {
        let ny = 0.5 * x + 0.5 * self.y[c];
        self.y[c] = ny;
        ny
    }

    fn stage(&mut self, block: &mut [f32]) -> Result<(), String> {
        assert!(
            block.len().is_multiple_of(CHANNELS),
            "staging blocks are frame-aligned"
        );
        if self.mutation == Mutation::ResetEachCall {
            self.y = [0.0; CHANNELS];
        }
        for chunk in block.as_chunks_mut::<CHANNELS>().0 {
            for (c, sample) in chunk.iter_mut().enumerate() {
                let x = *sample;
                // Under DoubleAdvance the input is (wrongly) applied to
                // the state twice per sample: y = 0.75x + 0.25y — a
                // different, still-stable recurrence, permanently
                // distinguishable from the honest one. (Advancing the
                // state twice on the STEP OUTPUT would be a fixed point
                // of this expression — 0.5z + 0.5z == z exactly — and
                // therefore invisible; the defect class this knob models
                // is a state that advances more than once per input.)
                let y = if self.mutation == Mutation::DoubleAdvance {
                    self.step(x, c);
                    self.step(x, c)
                } else {
                    self.step(x, c)
                };
                *sample = y;
            }
        }
        Ok(())
    }

    /// The D14.11 Applied invalidation target: the production worker
    /// calls this exactly once per applied cut; the honest probe
    /// returns to its episode-start seed.
    fn invalidate_history(&mut self) {
        if self.mutation != Mutation::PreserveAcrossApplied {
            self.y = [0.0; CHANNELS];
        }
    }
}

/// Wrap a probe into the test-only processor injection: one shared
/// probe behind both the stage and the invalidation closures (the
/// closure pair models exactly what an episode-owned stateful runtime
/// exposes to the worker: stage + history invalidation). Test-only: the
/// per-block mutex is legal here for the same reason bounded
/// synchronization is legal in the data plane (D14.11), and no
/// measurement claim rests on it.
fn probe_processor(mutation: Mutation) -> EpisodeProcessing {
    let probe = Arc::new(Mutex::new(StatefulProbe::new(mutation)));
    let stage_probe = probe.clone();
    let invalidate_probe = probe.clone();
    EpisodeProcessing::test_driven(
        Box::new(move |block| stage_probe.lock().unwrap().stage(block)),
        Box::new(move || invalidate_probe.lock().unwrap().invalidate_history()),
    )
}

/// The independent scalar reference over ONE channel's sample stream:
/// the same recurrence, the same expression, seeded fresh. This oracle
/// pins STATE semantics (fresh seed, per-sample streaming, per-channel
/// independence); the arithmetic identity itself is pinned by the
/// fragmentation oracle below, which shows the pipeline computes this
/// per-sample recurrence regardless of blocking.
fn reference_stream(input: &[f32], seed: f32) -> Vec<f32> {
    let mut y = seed;
    input
        .iter()
        .map(|x| {
            let ny = 0.5 * x + 0.5 * y;
            y = ny;
            ny
        })
        .collect()
}

/// The reference for both channels of a source run starting at source
/// frame `start` with FRESH episode state: the decode double's tags are
/// `i` (channel 0) and `i + 0.5` (channel 1).
fn reference_stereo(start: usize, frames: usize) -> (Vec<f32>, Vec<f32>) {
    let ch0: Vec<f32> = (start..start + frames).map(|i| i as f32).collect();
    let ch1: Vec<f32> = (start..start + frames).map(|i| i as f32 + 0.5).collect();
    (reference_stream(&ch0, 0.0), reference_stream(&ch1, 0.0))
}

/// The interleaved input the decode double produces for `frames`
/// frames starting at source frame `start` (stage-level tests only).
fn tagged_interleaved(start: usize, frames: usize) -> Vec<f32> {
    let mut block = Vec::with_capacity(frames * CHANNELS);
    for i in start..start + frames {
        block.push(i as f32);
        block.push(i as f32 + 0.5);
    }
    block
}

fn assert_streams_equal(actual: &[f32], expected: &[f32], channel: &'static str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "frame conservation ({channel}): the probe stream must conserve \
         the stream's shape exactly (no EOF tail, no dropped frame)"
    );
    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(a, e, "frame {i} of {channel} diverges from the oracle");
    }
}

// --- fragmentation invariance (the core stateful semantic property) ------

/// Whole-block vs fragmented delivery, BIT-EXACT: processing the same
/// sample sequence in one staging call must equal processing it through
/// arbitrary frame-aligned fragmentations (1, 7, 31, 64, 128, 257, 1024,
/// 4096, irregular 3333/remainder), and the final recursive state must
/// match. Justified exactness: the recurrence's per-sample operation
/// sequence is identical under any grouping, so any block-boundary
/// dependence (buffered state, per-call rescaling, partial-frame
/// handling) shows as a bit difference.
#[test]
fn the_probe_recurrence_is_invariant_under_staging_fragmentation() {
    let frames = 10_000;
    let input = tagged_interleaved(0, frames);

    // One large call.
    let mut whole = input.clone();
    let mut whole_probe = StatefulProbe::new(Mutation::None);
    whole_probe.stage(&mut whole).expect("stage succeeds");

    // Arbitrary fragmentations.
    let fragment_frames = [1usize, 7, 31, 64, 128, 257, 1024, 4096, 3333];
    let mut fragmented_probe = StatefulProbe::new(Mutation::None);
    let mut fragmented = Vec::with_capacity(input.len());
    let mut off = 0usize;
    for f in fragment_frames {
        let take = (f * CHANNELS).min(input.len() - off);
        let mut piece = input[off..off + take].to_vec();
        fragmented_probe.stage(&mut piece).expect("stage succeeds");
        fragmented.extend_from_slice(&piece);
        off += take;
    }
    let mut rest = input[off..].to_vec();
    fragmented_probe.stage(&mut rest).expect("stage succeeds");
    fragmented.extend_from_slice(&rest);

    assert_streams_equal(&fragmented, &whole, "channel-interleaved");
    assert_eq!(
        fragmented_probe.y, whole_probe.y,
        "the recursive state after fragmented delivery must equal the \
         whole-block state"
    );

    // The independent scalar reference agrees with both — per channel.
    let (ref_ch0, ref_ch1) = reference_stereo(0, frames);
    let mut deinterleaved_whole_ch0 = Vec::with_capacity(frames);
    let mut deinterleaved_whole_ch1 = Vec::with_capacity(frames);
    for pair in whole.as_chunks::<CHANNELS>().0 {
        deinterleaved_whole_ch0.push(pair[0]);
        deinterleaved_whole_ch1.push(pair[1]);
    }
    assert_streams_equal(&deinterleaved_whole_ch0, &ref_ch0, "channel 0 (reference)");
    assert_streams_equal(&deinterleaved_whole_ch1, &ref_ch1, "channel 1 (reference)");

    // Negative control: a block-boundary-DEPENDENT probe (state reset
    // per call) MUST diverge under fragmentation — the oracle can fail.
    let mut dependent_probe = StatefulProbe::new(Mutation::ResetEachCall);
    let mut dependent = Vec::with_capacity(input.len());
    let mut off = 0usize;
    for f in fragment_frames {
        let take = (f * CHANNELS).min(input.len() - off);
        let mut piece = input[off..off + take].to_vec();
        dependent_probe.stage(&mut piece).expect("stage succeeds");
        dependent.extend_from_slice(&piece);
        off += take;
    }
    assert_ne!(
        dependent, whole,
        "a block-boundary-dependent processor must be detectable by the \
         fragmentation oracle"
    );
}

// --- RefusedUnchanged: continuation == the no-seek control ---------------

/// A refused seek with stateful history in flight: the preserved
/// remainder is already PROCESSED (the stage ran over the whole staging
/// block before the partial edge write), and the refusal must finish it
/// exactly once, preserving the recursive state — so the whole consumed
/// stream equals the no-seek control BIT-EXACT, both channels. A reset
/// at the refusal, a reprocessed remainder or a double-advanced state
/// would all break the equality (see the negative controls below).
#[test]
fn a_refused_seek_preserves_the_stateful_continuation_of_the_no_seek_control() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let (control_w, control_handle, mut control_runtime) = episode_with_test_processor(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            probe_processor(Mutation::None),
            Vec::new(),
        );
        assert_eq!(
            control_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );

        let (seeked_w, seeked_handle, mut seeked_runtime) = episode_with_test_processor(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            probe_processor(Mutation::None),
            vec![ProviderSeekOutcome::RefusedUnchanged],
        );
        wait_until(Duration::from_secs(5), || {
            seeked_handle
                .observe()
                .position
                .is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        seeked_handle.request_seek(Duration::from_secs(5));
        assert_eq!(
            seeked_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "a refusal is not a failure and not a cut"
        );

        assert_streams_equal(&seeked_w.content(), &control_w.content(), "channel 0");
        assert_streams_equal(
            &seeked_w.content_ch1(),
            &control_w.content_ch1(),
            "channel 1",
        );
        let snapshot_control = control_runtime.dispose().snapshot;
        assert!(snapshot_control.quiet, "control teardown must stay quiet");
        let snapshot_seeked = seeked_runtime.dispose().snapshot;
        assert!(snapshot_seeked.quiet, "teardown must stay quiet");
    });
}

/// Negative controls for the refusal oracle's two mandated defect
/// classes, constructed on the control's own ground truth (the same
/// honest-construction pattern as I1's double-application control: the
/// defect streams are the exact arithmetic a defective pipeline would
/// emit, and the oracle MUST reject them).
///
/// - Reprocessed remainder: the already-processed tail fed through the
///   recurrence AGAIN, seeded with the state at the cut (the emitted
///   sample is the state variable, so the seed is the control's own
///   value at the cut).
/// - Reset on refusal: the continuation restarted from the episode seed
///   mid-stream over the raw source tags.
#[test]
fn the_refusal_oracle_rejects_the_reprocessed_remainder_and_reset_signatures() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let (control_w, control_handle, mut control_runtime) = episode_with_test_processor(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            probe_processor(Mutation::None),
            Vec::new(),
        );
        assert_eq!(
            control_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );
        let control_values = control_w.content();
        let snapshot = control_runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet");

        // Any mid-stream cut index demonstrates the defect shapes; the
        // equality oracle is indifferent to WHERE the pipeline breaks.
        let cut = 100_000usize;
        assert!(cut < control_values.len());

        // (a) Reprocessed remainder: feed the processed tail through the
        // recurrence once more, from the state at the cut.
        let state_at_cut = control_values[cut - 1];
        let reprocessed_tail = reference_stream(&control_values[cut..], state_at_cut);
        let mut reprocessed = control_values.clone();
        reprocessed.truncate(cut);
        reprocessed.extend_from_slice(&reprocessed_tail);
        assert_ne!(
            reprocessed, control_values,
            "precondition: reprocessing the remainder changes the stream"
        );
        assert!(
            rejects(|| assert_streams_equal(&reprocessed, &control_values, "channel 0")),
            "the refusal oracle must REJECT a reprocessed remainder"
        );

        // (b) Reset on refusal: the post-refusal continuation restarts
        // from the episode seed over the raw source tags.
        let fresh_tail: Vec<f32> = (cut..EIGHT_SECONDS).map(|i| i as f32).collect();
        let reset_tail = reference_stream(&fresh_tail, 0.0);
        let mut reset = control_values.clone();
        reset.truncate(cut);
        reset.extend_from_slice(&reset_tail);
        assert_ne!(
            reset, control_values,
            "precondition: resetting the history at the refusal changes \
             the stream"
        );
        assert!(
            rejects(|| assert_streams_equal(&reset, &control_values, "channel 0")),
            "the refusal oracle must REJECT a history reset at the refusal"
        );
    });
}

// --- Applied: fresh state at the landing ---------------------------------

/// An applied seek with stateful history in flight: the cut discards the
/// pre-cut processed remainder, the pipeline invalidates ALL pre-cut
/// signal-derived processing history (the D14.11 Applied obligation —
/// the worker calls the processing runtime's history invalidation
/// beside the staging discard and the edge purge), and post-cut
/// production equals a FRESH probe instance fed the exact post-landing
/// tags — BIT-EXACT, both channels. The pre-cut stretch equals the
/// no-seek control's prefix.
#[test]
fn an_applied_seek_resumes_with_fresh_state_at_the_landing() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let five_seconds = 5 * TEST_RATE;
        let landing_frames = EIGHT_SECONDS - five_seconds;

        let (control_w, control_handle, mut control_runtime) = episode_with_test_processor(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            probe_processor(Mutation::None),
            Vec::new(),
        );
        assert_eq!(
            control_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );

        let (seeked_w, seeked_handle, mut seeked_runtime) = episode_with_test_processor(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            probe_processor(Mutation::None),
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            seeked_handle
                .observe()
                .position
                .is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        seeked_handle.request_seek(Duration::from_secs(5));
        assert!(
            wait_until(Duration::from_secs(5), || seeked_handle
                .observe()
                .position
                .is_some_and(|p| p >= five_seconds as u64)),
            "the position never rebased to the landing: {:?}",
            seeked_handle.observe()
        );
        assert_eq!(
            seeked_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "the seek must not disturb the episode's ordinary EOF course"
        );

        let values = seeked_w.content();
        let values_ch1 = seeked_w.content_ch1();
        // The cut point: the first frame where the seek episode's stream
        // stops matching the control's (both ran the identical
        // processing until the staging block was cut; the recurrence is
        // not affine in the sample value, so the I1-style discontinuity
        // scan does not apply — the control-prefix comparison IS the cut
        // detector).
        let control_values = control_w.content();
        let cut = values
            .iter()
            .zip(control_values.iter())
            .position(|(a, c)| a != c)
            .expect("the applied cut must appear as a divergence from the control");
        assert_eq!(
            &values[..cut],
            &control_values[..cut],
            "the pre-cut stretch must equal the no-seek control bit-exact"
        );
        let control_ch1 = control_w.content_ch1();
        assert_eq!(
            &values_ch1[..cut],
            &control_ch1[..cut],
            "the pre-cut stretch of channel 1 must equal the control"
        );

        // The post-cut stretch: fresh probe state under the same
        // configuration, fed the exact post-landing tags.
        let (ref_ch0, ref_ch1) = reference_stereo(five_seconds, landing_frames);
        let post = &values[cut..];
        let post_ch1 = &values_ch1[cut..];
        assert_eq!(
            values.len(),
            cut + landing_frames,
            "the post-cut stretch must conserve its frame count exactly"
        );
        assert_streams_equal(post, &ref_ch0, "channel 0 (fresh state)");
        assert_streams_equal(post_ch1, &ref_ch1, "channel 1 (fresh state)");
        let snapshot_control = control_runtime.dispose().snapshot;
        assert!(snapshot_control.quiet, "control teardown must stay quiet");
        let snapshot = seeked_runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet");
    });
}

/// Negative mutation (mandated): a processing runtime that PRESERVES its
/// pre-cut history across an applied seek — the production invalidation
/// call happens, the mutated probe ignores it — MUST be rejected by the
/// fresh-landing oracle. This is a REAL run through the real pipeline;
/// only the test-only probe misbehaves. The honest twin of this oracle
/// is green (`an_applied_seek_resumes_with_fresh_state_at_the_landing`).
#[test]
fn a_probe_that_preserves_history_across_an_applied_seek_is_rejected() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(60), move || {
        let five_seconds = 5 * TEST_RATE;
        let (control_w, control_handle, mut control_runtime) = episode_with_test_processor(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            probe_processor(Mutation::None),
            Vec::new(),
        );
        assert_eq!(
            control_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );
        let control_values = control_w.content();

        let (seeked_w, seeked_handle, mut seeked_runtime) = episode_with_test_processor(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            probe_processor(Mutation::PreserveAcrossApplied),
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            seeked_handle
                .observe()
                .position
                .is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        seeked_handle.request_seek(Duration::from_secs(5));
        assert_eq!(
            seeked_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "precondition: the mutated episode itself completes (the \
             defect is content, not liveness)"
        );
        let values = seeked_w.content();
        // The cut: first divergence from the control's prefix (the
        // pipeline is honest; only the probe's invalidation is
        // suppressed, so the pre-cut stretch still matches).
        let cut = values
            .iter()
            .zip(control_values.iter())
            .position(|(a, c)| a != c)
            .expect("the applied cut must appear as a divergence from the control");
        let landing_frames = values.len() - cut;
        assert_eq!(
            values.len(),
            cut + (EIGHT_SECONDS - five_seconds),
            "precondition: frame conservation across the cut"
        );
        // The fresh-landing oracle MUST reject: a preserved-history
        // continuation cannot reproduce the fresh-seed recurrence at the
        // landing (the state difference decays by half each step, so the
        // divergence lives in a finite window right after the cut —
        // exactly where the equality check strikes first).
        let (ref_ch0, _) = reference_stereo(five_seconds, landing_frames);
        let rejected = rejects(|| {
            assert_streams_equal(&values[cut..], &ref_ch0, "channel 0 (fresh state)");
        });
        assert!(
            rejected,
            "the fresh-landing oracle must REJECT a runtime that \
             preserves pre-cut history across an applied seek"
        );
        let snapshot_control = control_runtime.dispose().snapshot;
        assert!(snapshot_control.quiet, "control teardown must stay quiet");
        let snapshot = seeked_runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet");
    });
}

// --- MutatedThenFailed: no old continuation ------------------------------

/// A destructive provider seek with stateful history in flight: the
/// episode takes the ordinary D11 Failed route (diagnostic truthful
/// about the decode origin of THIS failure — the seek provider's), and
/// the pre-failure stretch is indistinguishable from the no-seek
/// control: the old continuation is never reconstructed because the
/// episode is over.
#[test]
fn a_mutated_then_failed_seek_never_reconstructs_the_old_continuation() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let (control_w, control_handle, mut control_runtime) = episode_with_test_processor(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            probe_processor(Mutation::None),
            Vec::new(),
        );
        assert_eq!(
            control_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );
        let control_values = control_w.content();

        let (failed_w, failed_handle, mut failed_runtime) = episode_with_test_processor(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            probe_processor(Mutation::None),
            vec![ProviderSeekOutcome::MutatedThenFailed {
                diagnostic: "test destructive seek".to_owned(),
            }],
        );
        wait_until(Duration::from_secs(5), || {
            failed_handle
                .observe()
                .position
                .is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        failed_handle.request_seek(Duration::from_secs(5));
        assert_eq!(
            failed_handle.wait_terminal(),
            EpisodeTerminalOutcome::Failed,
            "a destructive provider failure is the existing D11 Failed"
        );
        let diagnostic = failed_handle
            .observe()
            .failure_diagnostic
            .expect("a failed episode carries its diagnostic");
        assert!(
            diagnostic.starts_with("decode: seek failed:"),
            "the seek provider's failure keeps its decode-origin \
             diagnostic: {diagnostic}"
        );
        // The pre-failure stretch is exactly the control's prefix (no
        // old-cursor production resumed, nothing fabricated).
        let values = failed_w.content();
        assert!(
            values.len() <= control_values.len(),
            "a failed episode produces no more than the control"
        );
        assert_eq!(
            &values[..],
            &control_values[..values.len()],
            "the pre-failure stretch must equal the control's prefix \
             bit-exact"
        );
        // No production resumes after the failure.
        let stopped_at = failed_w.consumed();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            failed_w.consumed(),
            stopped_at,
            "no post-failure production"
        );
        let snapshot_control = control_runtime.dispose().snapshot;
        assert!(snapshot_control.quiet, "control teardown must stay quiet");
        let snapshot = failed_runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet");
    });
}

// --- pause: history preserved --------------------------------------------

/// A pause/resume cycle with stateful history in flight: the consumed
/// stream equals the no-seek control BIT-EXACT — the recursive state
/// survives the pause untouched (with stateless Gain this was
/// unobservable; the probe makes it observable, both channels).
#[test]
fn a_stateful_pause_resume_cycle_preserves_the_history() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = TEST_RATE * 4;
        let (control_w, control_handle, mut control_runtime) = episode_with_test_processor(
            source_frames,
            OutputBehavior::Consume,
            probe_processor(Mutation::None),
            Vec::new(),
        );
        assert_eq!(
            control_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );

        let (paused_w, paused_handle, mut paused_runtime) = episode_with_test_processor(
            source_frames,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            probe_processor(Mutation::None),
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            paused_handle
                .observe()
                .position
                .is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        paused_handle.request_pause();
        assert!(
            wait_until(Duration::from_secs(5), || paused_handle.observe().paused()),
            "the Paused projection never established: {:?}",
            paused_handle.observe()
        );
        paused_handle.request_resume();
        assert_eq!(
            paused_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "the pause cycle must not disturb the episode's ordinary course"
        );
        assert_streams_equal(&paused_w.content(), &control_w.content(), "channel 0");
        assert_streams_equal(
            &paused_w.content_ch1(),
            &control_w.content_ch1(),
            "channel 1",
        );
        let snapshot_control = control_runtime.dispose().snapshot;
        assert!(snapshot_control.quiet, "control teardown must stay quiet");
        let snapshot = paused_runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet");
    });
}

// --- Open/replacement: no cross-episode state ----------------------------

/// A new episode must not inherit the old episode's processing state:
/// two sequential episodes over the same source in one process both
/// equal the FRESH reference — and each other — bit-exact. A probe
/// whose state leaked across episodes (process-global storage) would
/// continue the first episode's recurrence into the second and break
/// the equality.
#[test]
fn a_new_episode_starts_from_fresh_processing_state() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = TEST_RATE * 3;
        let (first_w, first_handle, mut first_runtime) = episode_with_test_processor(
            source_frames,
            OutputBehavior::Consume,
            probe_processor(Mutation::None),
            Vec::new(),
        );
        assert_eq!(
            first_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );
        let first_values = first_w.content();
        let snapshot = first_runtime.dispose().snapshot;
        assert!(snapshot.quiet, "first episode teardown must stay quiet");

        let (second_w, second_handle, mut second_runtime) = episode_with_test_processor(
            source_frames,
            OutputBehavior::Consume,
            probe_processor(Mutation::None),
            Vec::new(),
        );
        assert_eq!(
            second_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed
        );
        let second_values = second_w.content();

        let (ref_ch0, ref_ch1) = reference_stereo(0, source_frames);
        assert_streams_equal(&second_values, &ref_ch0, "channel 0 (fresh)");
        assert_streams_equal(&second_w.content_ch1(), &ref_ch1, "channel 1 (fresh)");
        assert_streams_equal(
            &second_values,
            &first_values,
            "channel 0 (no cross-episode state)",
        );
        let snapshot = second_runtime.dispose().snapshot;
        assert!(snapshot.quiet, "second episode teardown must stay quiet");
    });
}

// --- double-advance defect (real run) ------------------------------------

/// Negative mutation (mandated): a state machine that advances its
/// recurrence twice per sample — the double-advance defect class — is a
/// REAL run through the real pipeline, and the continuation oracles
/// MUST reject it against the honest reference.
#[test]
fn a_double_advancing_probe_is_rejected_by_the_continuation_oracle() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = TEST_RATE * 2;
        let (mutated_w, mutated_handle, mut mutated_runtime) = episode_with_test_processor(
            source_frames,
            OutputBehavior::Consume,
            probe_processor(Mutation::DoubleAdvance),
            Vec::new(),
        );
        assert_eq!(
            mutated_handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "precondition: the mutated episode itself completes"
        );
        let values = mutated_w.content();
        let (ref_ch0, _) = reference_stereo(0, source_frames);
        assert_ne!(
            values, ref_ch0,
            "precondition: the double-advanced stream really differs from \
             the honest recurrence"
        );
        assert!(
            rejects(|| assert_streams_equal(&values, &ref_ch0, "channel 0")),
            "the continuation oracle must REJECT a double-advancing state \
             machine"
        );
        let snapshot = mutated_runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet");
    });
}
