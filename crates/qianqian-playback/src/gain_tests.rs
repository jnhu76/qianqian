//! Episode Audio Processing oracles — production Gain (Issue #177
//! Stage 2 / I1; ADR-PBK-002 D14.11). These tests preserve the I0 probe
//! oracles in production form: the config-carrying
//! `playback_session_spec_with_processing` composition arms the REAL
//! episode-owned processing runtime (no test-global arming, no
//! experiment scaffolding), and every oracle remains a D14.11 semantic
//! requirement — unity/half/zero exactness, frame and format
//! conservation, EOF tail-freedom, partial edge acceptance of processed
//! blocks, seek RefusedUnchanged/Applied obligations on processed
//! staging and processed remainder, pause continuity, bypass
//! transparency, configuration establishment, and the D11 failure route
//! with a truthful processing-origin diagnostic.
//!
//! The exactness argument for `==` oracles (no float tolerance): the
//! decode double tags frame `i` of channel 0 with `i as f32` and every
//! other channel with `i as f32 + 0.5` — exact for `i < 2^23`, and
//! every source here is far below that — and for the required factors
//! IEEE-754 f32 multiplication is exact: ×1.0 is the identity, ×0.5 is
//! an exponent decrement (no mantissa change for normal numbers), ×0.0
//! of a finite non-negative value is exactly +0.0. Both channels are
//! witnessed, so a seam that misapplies processing per channel fails.

use std::time::Duration;

use qianqian_audio_api::ports::ProviderSeekOutcome;

use crate::handle::EpisodeTerminalOutcome;
use crate::processing::{AudioProcessingConfig, EpisodeProcessing};
use crate::processing_support::{
    EIGHT_SECONDS, TEST_RATE, assert_processed_exactly_at, assert_stereo_processed_exactly,
    discontinuities, episode, episode_with_test_processor, rejects, wait_until,
};
use crate::test_common::{self, OutputBehavior};

// --- seek / discontinuity oracles (D14.5 extended to processed PCM) ------

/// RefusedUnchanged with processed PCM in flight: the preserved remainder
/// of the interrupted staging block is already PROCESSED (the production
/// stage runs before the edge write), and the refusal must finish it
/// exactly once — the consumed stream is indistinguishable from the
/// no-seek control, frame for frame. A reprocessed remainder would show
/// a ×0.25 stretch (an extra discontinuity and wrong values); a dropped
/// one would shift every later frame; both fail the exact oracle.
#[test]
fn a_refused_seek_finishes_its_processed_remainder_exactly_once() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let (witnesses, handle, mut runtime) = episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            AudioProcessingConfig::gain(0.5),
            vec![ProviderSeekOutcome::RefusedUnchanged],
        );
        wait_until(Duration::from_secs(5), || {
            handle.observe().position.is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        handle.request_seek(Duration::from_secs(5));
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "a refusal is not a failure and not a cut"
        );
        let values = witnesses.content();
        let values_ch1 = witnesses.content_ch1();
        let breaks = discontinuities(&values, 0.5);
        assert!(
            breaks.is_empty(),
            "a refused seek must be content-indistinguishable from no seek \
             (first breaks: {:?} in {} frames)",
            &breaks[..breaks.len().min(8)],
            values.len()
        );
        assert!(
            discontinuities(&values_ch1, 0.5).is_empty(),
            "channel 1 must be content-indistinguishable from no seek too"
        );
        assert_stereo_processed_exactly(&values, &values_ch1, 0.5, EIGHT_SECONDS);
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// Applied with processed PCM in flight: the cut discards the pre-cut
/// processed remainder with the staging, purges the edge under the
/// existing D14.5 mechanism, and post-cut production continues from the
/// provider's landing — still processed. The consumed stream shows
/// exactly ONE discontinuity, landing exactly where the provider
/// reported, with both stretches exactly `frame_index × 0.5`, and the
/// post-cut stretch conserves its frame count exactly (no dropped or
/// duplicated tail after the cutover).
#[test]
fn an_applied_seek_discards_the_processed_remainder_and_cuts_cleanly() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let five_seconds = 5 * TEST_RATE;
        let (witnesses, handle, mut runtime) = episode(
            EIGHT_SECONDS,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            AudioProcessingConfig::gain(0.5),
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            handle.observe().position.is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        handle.request_seek(Duration::from_secs(5));
        assert!(
            wait_until(Duration::from_secs(5), || handle
                .observe()
                .position
                .is_some_and(|p| p >= five_seconds as u64)),
            "the position never rebased to the landing: {:?}",
            handle.observe()
        );
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "the seek must not disturb the episode's ordinary EOF course"
        );
        let values = witnesses.content();
        let values_ch1 = witnesses.content_ch1();
        let breaks = discontinuities(&values, 0.5);
        assert_eq!(
            breaks.len(),
            1,
            "a committed cutover must appear as exactly one content \
             discontinuity (first breaks: {:?} in {} frames)",
            &breaks[..breaks.len().min(8)],
            values.len()
        );
        assert_eq!(
            discontinuities(&values_ch1, 0.5).len(),
            1,
            "channel 1 must show the same single cutover"
        );
        assert_eq!(
            values[breaks[0]],
            five_seconds as f32 * 0.5,
            "the cut must land exactly at the provider's ACTUAL landing \
             (the landing's first processed sample)"
        );
        // Both stretches are exactly the source scaled by the episode's
        // gain: the pre-cut stretch from source frame 0, the post-cut
        // stretch from the landing (the skipped stretch between them is
        // what a seek IS). The post-cut frame count is pinned exactly:
        // pre-cut consumed + (source total − landing) frames, no tail
        // anomaly.
        assert_eq!(
            values.len(),
            breaks[0] + (EIGHT_SECONDS - five_seconds),
            "the post-cut stretch must conserve its frame count exactly"
        );
        assert_processed_exactly_at(&values[..breaks[0]], 0.0, 0.5, breaks[0], "channel 0");
        assert_processed_exactly_at(&values_ch1[..breaks[0]], 0.5, 0.5, breaks[0], "channel 1");
        for (tag_offset, channel, values) in
            [(0.0, "channel 0", &values), (0.5, "channel 1", &values_ch1)]
        {
            let after = &values[breaks[0]..];
            for (offset, value) in after.iter().enumerate() {
                assert_eq!(
                    *value,
                    ((five_seconds + offset) as f32 + tag_offset) * 0.5,
                    "post-cut frame {offset} of {channel} must be exactly \
                     (its source index + {tag_offset}) × 0.5"
                );
            }
        }
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- pause continuity (D14.11: pause never invalidates processing) ------

/// Pause/resume through the real gate with processed PCM in flight: the
/// Paused projection establishes from the current engagement's tail
/// quiescence, resume releases it, and the consumed stream after the
/// cycle is still exactly the source scaled by the episode's gain — no
/// reset, no duplicated stretch, no gap. (Gain is stateless, so the
/// oracle pins the pipeline continuity pause must not disturb; the
/// stateful counterpart is I2's StatefulProbe.)
#[test]
fn a_pause_resume_cycle_preserves_the_processed_stream() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = TEST_RATE * 4;
        let (witnesses, handle, mut runtime) = episode(
            source_frames,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            AudioProcessingConfig::gain(0.5),
            Vec::new(),
        );
        wait_until(Duration::from_secs(5), || {
            handle.observe().position.is_some_and(|p| p >= 22_050)
        })
        .then_some(())
        .expect("the episode never reached half a second");
        handle.request_pause();
        assert!(
            wait_until(Duration::from_secs(5), || handle.observe().paused()),
            "the Paused projection never established: {:?}",
            handle.observe()
        );
        handle.request_resume();
        assert!(
            wait_until(Duration::from_secs(5), || {
                !handle.observe().pause_requested
            }),
            "resume never released the episode: {:?}",
            handle.observe()
        );
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "the pause cycle must not disturb the episode's ordinary course"
        );
        assert_stereo_processed_exactly(
            &witnesses.content(),
            &witnesses.content_ch1(),
            0.5,
            source_frames,
        );
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- failure semantics (D14.11: D11 Failed, truthful origin) ------------

/// The unrecoverable processing failure settles through the EXISTING D11
/// `Failed` terminal class — no new public variant — with a diagnostic
/// that stays truthful about its processing origin: it names the
/// processing stage and never masquerades as a decode failure merely
/// because both run on the decode worker. The route driven here is the
/// production route end-to-end through the REAL composition: the real
/// activation, decode worker, completion publication and settlement,
/// with the failing processor injected as the test-only `TestDriven`
/// state (Scalar Gain itself cannot fail, so no product configuration
/// can reach the failure; the injection exists precisely so the ROUTE
/// stays exercised and never ships). The failed episode never resumes
/// production, the position projection is withdrawn with the terminal
/// Fact, and teardown stays quiet.
#[test]
fn a_processing_failure_takes_the_d11_failed_route_with_a_truthful_diagnostic() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        // Three staging blocks are processed honestly, then the
        // processor reports the synthetic unrecoverable failure — the
        // steady-path failure shape, not a first-block edge case.
        let blocks_seen = std::cell::Cell::new(0u64);
        let stage_blocks = blocks_seen.clone();
        let processing = EpisodeProcessing::test_driven(
            Box::new(move |_block| {
                let index = stage_blocks.get();
                stage_blocks.set(index + 1);
                if index >= 3 {
                    return Err(format!(
                        "synthetic processing failure at staging block {index}"
                    ));
                }
                Ok(())
            }),
            Box::new(|| {}),
        );
        let (witnesses, handle, mut runtime) = episode_with_test_processor(
            EIGHT_SECONDS,
            OutputBehavior::Consume,
            processing,
            Vec::new(),
        );
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Failed,
            "an unrecoverable processing failure is the existing D11 Failed"
        );
        let observation = handle.observe();
        let diagnostic = observation
            .failure_diagnostic
            .as_deref()
            .expect("a failed episode carries its presentation diagnostic");
        assert_eq!(
            diagnostic, "processing: synthetic processing failure at staging block 3",
            "the diagnostic's stage must name the processing origin: {diagnostic}"
        );
        assert!(
            !diagnostic.contains("decode"),
            "a processing failure must not masquerade as a decode failure: {diagnostic}"
        );
        // The failed episode never resumes production.
        let stopped_at = witnesses.consumed();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            witnesses.consumed(),
            stopped_at,
            "a failed episode must never resume production"
        );
        assert!(
            handle.observe().position.is_none(),
            "the terminal Fact withdraws the projection"
        );
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- negative controls (the oracles must be able to FAIL) ---------------

/// Negative control, omission class: with the episode configured BYPASS
/// (the transparent pass-through — the same observable shape as "skip
/// Gain entirely"), the half-gain oracle MUST reject the stream — on
/// both witnessed channels. A green suite whose oracle cannot fail here
/// would prove nothing about the seam.
#[test]
fn negative_control_the_half_gain_oracle_rejects_a_skipped_seam() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = 20_000;
        let (witnesses, handle, mut runtime) = episode(
            source_frames,
            OutputBehavior::Consume,
            AudioProcessingConfig::BYPASS,
            Vec::new(),
        );
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "precondition: the unprocessed episode itself completes"
        );
        let values = witnesses.content();
        let values_ch1 = witnesses.content_ch1();
        let rejected = rejects(|| {
            assert_stereo_processed_exactly(&values, &values_ch1, 0.5, source_frames);
        });
        assert!(
            rejected,
            "the half-gain oracle must REJECT unprocessed audio — \
             a seam detector that passes here proves nothing"
        );
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// Negative control, double-application class: content carrying the
/// ×factor² signature MUST be rejected by the half-gain oracle.
///
/// The double-application failure mode of THIS seam is "the staging
/// block is processed more than once", and its exact signature through
/// the real pipeline is `frame_index × factor²` (the decode double's
/// position tagging and both factors are IEEE-exact, so a seam that
/// applied 0.5 twice would emit EXACTLY `i × 0.25`). The episode below
/// drives the real production seam with gain 0.25 — producing that
/// exact signature through the identical worker/staging/edge path on
/// both witnessed channels — and the oracle must reject it. Together
/// with the positive oracles (which pin `i × 0.5` through the same
/// path), exactly-once is established: a double-applied seam cannot
/// pass, and the signature is proven detectable rather than vacuously
/// tolerated.
#[test]
fn negative_control_the_gain_oracle_rejects_the_double_applied_signature() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = 20_000;
        let (witnesses, handle, mut runtime) = episode(
            source_frames,
            OutputBehavior::Consume,
            AudioProcessingConfig::gain(0.25),
            Vec::new(),
        );
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "precondition: the ×factor²-signature episode itself completes"
        );
        let values = witnesses.content();
        let values_ch1 = witnesses.content_ch1();
        // The witness of the signature: frame 1 is exactly 1 × 0.25
        // (channel 0) and (1 + 0.5) × 0.25 = 0.375 (channel 1) — the
        // values a double-applied 0.5 seam would have produced.
        assert_eq!(values[1], 0.25, "precondition: content is ×0.25");
        assert_eq!(
            values_ch1[1], 0.375,
            "precondition: channel-1 content is ×0.25 of its tag"
        );
        let rejected = rejects(|| {
            assert_stereo_processed_exactly(&values, &values_ch1, 0.5, source_frames);
        });
        assert!(
            rejected,
            "the half-gain oracle must REJECT the double-applied \
             signature — exactly-once is part of the seam contract"
        );
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- realtime posture of the exercised path -----------------------------

/// I1: the production Gain stage performs zero steady-state allocation —
/// measured, not claimed, with the crate's counting allocator over the
/// staging-sized block the worker feeds (the in-place multiply touches
/// only the caller's buffer). The other firewall rows are structural,
/// not measurable: no capability/context resolve, no dispatch, no K0
/// work, no I/O exist anywhere in `EpisodeProcessing::stage`.
#[test]
fn steady_state_processing_allocates_zero() {
    let mut processing =
        EpisodeProcessing::new(&AudioProcessingConfig::gain(0.5)).expect("valid config");
    let mut block = vec![0.25f32; 1024 * 2];
    // Warm the path once outside the window (first-touch page faults and
    // any one-time lazy state are not steady-state processing).
    processing
        .stage(&mut block)
        .expect("warm-up stage succeeds");
    let (_, allocations) =
        crate::edge_lifecycle_tests::counting_allocator::run_counting_allocations(|| {
            for _ in 0..10_000 {
                processing.stage(&mut block).expect("stage succeeds");
            }
        });
    assert_eq!(
        allocations, 0,
        "steady-state processing must allocate nothing per block"
    );
}

// --- conservation oracles ------------------------------------------------

/// Unity is bit-exact: `×1.0` is the IEEE-754 identity, so the consumed
/// stream must equal the raw position-tagged source frame for frame, on
/// both channels.
#[test]
fn unity_gain_preserves_samples_bit_exact() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = 20_000;
        let (witnesses, handle, mut runtime) = episode(
            source_frames,
            OutputBehavior::Consume,
            AudioProcessingConfig::gain(1.0),
            Vec::new(),
        );
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_stereo_processed_exactly(
            &witnesses.content(),
            &witnesses.content_ch1(),
            1.0,
            source_frames,
        );
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// Zero gain is true silence that conserves the stream's shape: every
/// frame arrives exactly +0.0, with the frame count untouched (gain
/// mutes samples, never frames), on both channels.
#[test]
fn zero_gain_is_silence_with_frame_conservation() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = 20_000;
        let (witnesses, handle, mut runtime) = episode(
            source_frames,
            OutputBehavior::Consume,
            AudioProcessingConfig::gain(0.0),
            Vec::new(),
        );
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_stereo_processed_exactly(
            &witnesses.content(),
            &witnesses.content_ch1(),
            0.0,
            source_frames,
        );
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- the seam tracer -----------------------------------------------------

/// The I1 tracer bullet: half gain through the REAL decode-worker
/// staging seam of the production composition. Two seconds of source —
/// far past the 8192-frame edge capacity — with a slow consumer, so the
/// producer runs its bounded-slice write loop against a full edge:
/// partial edge acceptance of already-processed blocks is structural
/// here, not incidental. The exact oracle leaves no room for a wrong
/// seam: any skipped, doubled, dropped, reprocessed or channel-blind
/// staging content fails the frame-index check. Format conservation is
/// pinned on the same episode: processing must not disturb the source
/// format.
#[test]
fn half_gain_processes_the_real_staging_seam() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = TEST_RATE * 2;
        let (witnesses, handle, mut runtime) = episode(
            source_frames,
            OutputBehavior::SlowConsume {
                per_read: Duration::from_millis(1),
            },
            AudioProcessingConfig::gain(0.5),
            Vec::new(),
        );
        assert_eq!(
            handle.wait_terminal(),
            EpisodeTerminalOutcome::Completed,
            "gain must not disturb the episode's ordinary EOF course"
        );
        assert_stereo_processed_exactly(
            &witnesses.content(),
            &witnesses.content_ch1(),
            0.5,
            source_frames,
        );
        let observation = handle.observe();
        assert_eq!(
            observation.source_format,
            Some(test_common::TEST_FORMAT),
            "format conservation: processing must not disturb the source format"
        );
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

// --- configuration establishment ----------------------------------------

/// BYPASS is transparent bit-for-bit through the full composition: an
/// episode established with `enabled: false` and an INERT non-unity
/// gain delivers the raw source untouched (bypass is a configuration,
/// not a processor that runs and does nothing), on both channels.
#[test]
fn bypass_ignores_the_gain_field_and_passes_the_source_through() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = 20_000;
        let (witnesses, handle, mut runtime) = episode(
            source_frames,
            OutputBehavior::Consume,
            AudioProcessingConfig {
                enabled: false,
                gain: 0.5,
            },
            Vec::new(),
        );
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_stereo_processed_exactly(
            &witnesses.content(),
            &witnesses.content_ch1(),
            1.0,
            source_frames,
        );
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}

/// An invalid desired configuration (NaN / ±Inf / negative gain) fails
/// the EPISODE ESTABLISHMENT cleanly: the activation raises with a
/// truthful diagnostic, `activation_error` carries it, and NO terminal
/// Fact is forged — an episode that never started has no terminal
/// outcome (D11 activation firewall).
#[test]
fn an_invalid_processing_config_fails_establishment_without_a_terminal_fact() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        for gain in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.5] {
            let (witnesses, handle, mut runtime) = episode(
                20_000,
                OutputBehavior::Consume,
                AudioProcessingConfig::gain(gain),
                Vec::new(),
            );
            let observation = handle.observe();
            let error = observation
                .activation_error
                .as_deref()
                .unwrap_or_else(|| panic!("gain {gain}: the activation must raise"));
            assert!(
                error.contains("audio processing configuration invalid"),
                "gain {gain}: the diagnostic must name the establishment \
                 failure: {error}"
            );
            assert_eq!(
                observation.terminal_outcome, None,
                "gain {gain}: a never-started episode must not forge a \
                 terminal Fact"
            );
            assert!(
                witnesses.consumed() == 0,
                "gain {gain}: a failed establishment never produces audio"
            );
            let snapshot = runtime.dispose().snapshot;
            assert!(snapshot.quiet, "gain {gain}: teardown must stay quiet");
        }
    });
}

/// Positive gain above unity is documented, not clipped, end-to-end:
/// with gain 2.0 the consumed stream is exactly `frame_index × 2.0` —
/// values ABOVE ±1.0 travel the whole pipeline (the processing stage
/// does not clip or limit; the device owns out-of-range behavior, which
/// the mechanism doubles do not model).
#[test]
fn positive_gain_above_unity_is_not_clipped() {
    let _lifecycle = test_common::lifecycle_lock();
    test_common::within(Duration::from_secs(30), move || {
        let source_frames = 20_000;
        let (witnesses, handle, mut runtime) = episode(
            source_frames,
            OutputBehavior::Consume,
            AudioProcessingConfig::gain(2.0),
            Vec::new(),
        );
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert_stereo_processed_exactly(
            &witnesses.content(),
            &witnesses.content_ch1(),
            2.0,
            source_frames,
        );
        let snapshot = runtime.dispose().snapshot;
        assert!(snapshot.quiet, "teardown must stay quiet: {snapshot:?}");
    });
}
