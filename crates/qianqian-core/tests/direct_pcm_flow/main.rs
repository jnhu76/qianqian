//! Test-only executable evidence for the direct PCM flow boundary.
//!
//! Nothing in this file tree is part of the `qianqian-core` library API.
//!
//! The boundary under test: after composition resolves the three realtime
//! participants once, the pre-bound executable path runs Source →
//! ProcessingStage → Sink per PCM quantum with no composition-plane work —
//! no Context lookup, no capability resolution, no Reconcile, no
//! composition-plane generic dispatch (the per-quantum participant calls
//! themselves are pre-bound direct trait invocations), no per-quantum
//! allocation.
//!
//! Layout:
//!
//! ```text
//! direct_pcm_flow/main.rs          scenario tests + compile-fail runner
//! direct_pcm_flow/participants.rs  participant roles + pre-bound flow + observation doubles
//! direct_pcm_flow/composition.rs   kernel-mediated setup fixture
//! direct_pcm_flow/mutations.rs     lookup-seam control + adversarial twins + kill tests
//! direct_pcm_flow/compile_fail/    rustc fixtures (block-retention evidence)
//! ```
//!
//! The PCM harness (deterministic sample oracle, whole-frame formats,
//! checked views) is reused verbatim from the PCM edge experiment, and the
//! counting allocator is the shared test-binary instrumentation:
//!
//! ```bash
//! cargo test -p qianqian-core --test direct_pcm_flow
//! ```

#[path = "../common/counting_allocator.rs"]
mod counting_allocator;
// The shared harness carries the whole PCM edge experiment's vocabulary;
// this binary uses the checked views, formats, sample oracle, and the
// in-place lend — the unused remainder stays for the sibling experiment.
mod composition;
#[allow(dead_code)]
#[path = "../pcm_edge_contract/harness.rs"]
mod harness;
mod mutations;
mod participants;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use composition::{CompositionFixture, compose_prebound_flow};
use counting_allocator::run_counting_allocations;
use harness::{PcmFormat, TransferError};
use participants::{IdentityStage, StreamingSource, VerifyingSink};
use qianqian_kernel::{DesiredEntry, Revision};

fn stereo() -> PcmFormat {
    PcmFormat::new(48_000, 2).expect("stereo is a valid format")
}

/// Composes the honest identity flow: deterministic source, identity stage,
/// verifying sink, all bound through the real kernel.
fn identity_fixture_with_block(block_frames: usize) -> CompositionFixture {
    let format = stereo();
    compose_prebound_flow(
        Rc::new(RefCell::new(StreamingSource::new(format))),
        Rc::new(RefCell::new(IdentityStage)),
        Rc::new(RefCell::new(VerifyingSink::new(format))),
        format,
        block_frames,
    )
}

/// Drives `total_frames` through the fixture's flow in `block_frames`-frame
/// quanta (partial final quantum included), then checks frame conservation
/// from the participants' real cursor state.
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

// ---------------------------------------------------------------------------
// Scenarios: the pre-bound path delivers the stream
// ---------------------------------------------------------------------------

#[test]
fn prebound_flow_delivers_the_stream_across_quantum_counts() {
    for quanta in [1usize, 3, 17, 4096] {
        let mut fixture = identity_fixture_with_block(4);
        let total = quanta * 4;
        run_stream(&mut fixture, total, 4)
            .unwrap_or_else(|e| panic!("{quanta} quanta failed: {e}"));
        assert_eq!(
            fixture.source.borrow().frames_produced(),
            total,
            "the source produced every frame"
        );
        assert_eq!(
            fixture.sink.borrow().frames_consumed(),
            total,
            "the sink consumed every frame"
        );
    }
}

#[test]
fn prebound_flow_survives_arbitrary_block_sizes_and_a_partial_final_block() {
    for block_frames in [1usize, 3, 17, 256] {
        let mut fixture = identity_fixture_with_block(block_frames);
        run_stream(&mut fixture, 100, block_frames)
            .unwrap_or_else(|e| panic!("block size {block_frames} failed: {e}"));
        assert_eq!(
            fixture.sink.borrow().frames_consumed(),
            100,
            "block size {block_frames} conserved the stream"
        );
    }
}

#[test]
fn transforming_stage_value_flows_through_the_middle_participant() {
    let format = stereo();
    let mut fixture = compose_prebound_flow(
        Rc::new(RefCell::new(StreamingSource::new(format))),
        Rc::new(RefCell::new(participants::HalfAmplitudeStage::new(format))),
        Rc::new(RefCell::new(VerifyingSink::with_sample_scale(format, 0.5))),
        format,
        8,
    );
    run_stream(&mut fixture, 40, 8).expect("the transformed stream verifies in order");
    let observed = fixture
        .sink
        .borrow()
        .observed_storage_addr()
        .expect("the sink observed storage");
    assert_eq!(
        observed,
        fixture.flow.block_storage_addr(),
        "the in-place transform inserted no intermediate storage"
    );
}

#[test]
fn honest_path_observes_reused_storage_without_intermediates() {
    let mut fixture = identity_fixture_with_block(8);
    run_stream(&mut fixture, 64, 8).expect("the stream verifies in order");
    let observed = fixture
        .sink
        .borrow()
        .observed_storage_addr()
        .expect("the sink observed storage");
    assert_eq!(
        observed,
        fixture.flow.block_storage_addr(),
        "the sink must observe the flow's own reusable block storage"
    );
}

// ---------------------------------------------------------------------------
// Scenarios: the hot path stays outside the composition plane
// ---------------------------------------------------------------------------

#[test]
fn steady_state_performs_zero_composition_plane_operations_measured() {
    let mut fixture = identity_fixture_with_block(8);

    let ops_after_setup = fixture.kernel.debug_op_count();
    assert!(
        ops_after_setup > 0,
        "setup performed real composition work: the kernel witness is not trivially zero"
    );

    for _ in 0..4096 {
        fixture.flow.run_quantum(8).expect("the quantum is valid");
    }
    assert_eq!(
        fixture.kernel.debug_op_count(),
        ops_after_setup,
        "4096 quanta performed zero kernel operations (the kernel counts every public operation of itself)"
    );

    // Witness sensitivity: kernel activity after the loop would have been
    // counted, so the zero above is a measurement, not a stuck counter.
    fixture.kernel.settle();
    assert!(
        fixture.kernel.debug_op_count() > ops_after_setup,
        "the witness is sensitive to kernel activity"
    );
}

#[test]
fn setup_allocation_is_observable_and_steady_state_is_allocation_free_measured() {
    let (mut fixture, setup_allocations) =
        run_counting_allocations(|| identity_fixture_with_block(8));
    assert!(
        setup_allocations > 0,
        "setup performed real allocation work (kernel registry, participants, block storage)"
    );

    run_stream(&mut fixture, 16, 8).expect("warmup is valid");
    let steady_allocations = {
        let (_, allocations) = run_counting_allocations(|| {
            for _ in 0..64 {
                fixture.flow.run_quantum(8).expect("the quantum is valid");
            }
        });
        allocations
    };
    assert_eq!(
        steady_allocations, 0,
        "steady-state quanta must not heap-allocate (measured {steady_allocations})"
    );
}

#[test]
fn each_participant_is_visited_exactly_once_per_quantum_in_stream_order() {
    let format = stereo();
    let log = participants::visit_log();

    let real_source: Rc<RefCell<dyn participants::PcmSource>> =
        Rc::new(RefCell::new(StreamingSource::new(format)));
    let observed_source = Rc::new(RefCell::new(participants::ObservedSource::new(
        real_source.clone(),
        log.clone(),
    )));
    let real_stage: Rc<RefCell<dyn participants::PcmStage>> = Rc::new(RefCell::new(IdentityStage));
    let observed_stage = Rc::new(RefCell::new(participants::ObservedStage::new(
        real_stage,
        log.clone(),
    )));
    let real_sink: Rc<RefCell<dyn participants::PcmSink>> =
        Rc::new(RefCell::new(VerifyingSink::new(format)));
    let observed_sink = Rc::new(RefCell::new(participants::ObservedSink::new(
        real_sink,
        log.clone(),
    )));

    let mut fixture = compose_prebound_flow(
        observed_source,
        observed_stage.clone(),
        observed_sink,
        format,
        8,
    );

    let quanta = 5;
    for _ in 0..quanta {
        fixture.flow.run_quantum(8).expect("the quantum is valid");
    }
    let expected: Vec<participants::ParticipantRole> = [
        participants::ParticipantRole::Source,
        participants::ParticipantRole::Stage,
        participants::ParticipantRole::Sink,
    ]
    .into_iter()
    .cycle()
    .take(quanta * 3)
    .collect();
    assert_eq!(
        &*log.borrow(),
        &expected,
        "each quantum must visit source → stage → sink exactly once, in order"
    );
    assert_eq!(observed_stage.borrow().frames_in(), quanta * 8);
    assert_eq!(
        observed_stage.borrow().frames_out(),
        quanta * 8,
        "the stage handed every incoming frame onward"
    );
    assert_eq!(fixture.sink.borrow().frames_consumed(), quanta * 8);
}

// ---------------------------------------------------------------------------
// Hazard witness: composition withdrawal does not revoke an already-extracted flow
// ---------------------------------------------------------------------------
//
// Removing a provider from K0 composition makes the capability unreachable,
// but the already-extracted pre-bound flow stays callable. This is recorded
// as a hazard witness — not a correctness requirement — because this
// experiment has no publication/retirement mechanism: nothing here claims
// that new realtime entries after withdrawal are legal.

#[test]
fn composition_withdrawal_does_not_revoke_an_already_extracted_flow() {
    let mut fixture = identity_fixture_with_block(8);
    run_stream(&mut fixture, 32, 8).expect("the stream is valid before the composition change");

    // Control-plane change: the source provider leaves the desired
    // composition; reconcile runs on the control side.
    fixture
        .kernel
        .set_desired(vec![
            DesiredEntry::enabled("processing_stage", "processing_stage", Revision::fresh()),
            DesiredEntry::enabled("stream_sink", "stream_sink", Revision::fresh()),
            DesiredEntry::enabled("flow_assembler", "flow_assembler", Revision::fresh()),
        ])
        .expect("the revised composition is legal");
    fixture.kernel.settle();
    let snapshot = fixture.kernel.snapshot();
    assert_eq!(
        snapshot.capabilities.get("stream source"),
        Some(&None),
        "the source capability is no longer provided by the composition"
    );

    // The witness: the stale extracted flow remains invocable, and its
    // quanta still perform zero kernel operations. This is an observation
    // about the absence of a retirement mechanism in this experiment, not
    // an endorsement of running new realtime entries after withdrawal.
    let ops_after_change = fixture.kernel.debug_op_count();
    run_stream(&mut fixture, 64, 8)
        .expect("the already-extracted flow remains callable on its pre-bound references");
    assert_eq!(
        fixture.kernel.debug_op_count(),
        ops_after_change,
        "continued quanta performed zero kernel operations: runtime invocation never re-resolves"
    );
}

// ---------------------------------------------------------------------------
// Compile-fail evidence: block retention past the call
// ---------------------------------------------------------------------------
//
// The tested stage trait hands the block lifetime through by value, so a
// compliant impl cannot retain the incoming block past the call. These tests
// compile fixture files with a real `rustc` subprocess and assert on the
// outcome:
//
// * positive control: a compliant stage and a sequential quantum chain
//   type-check (including storage reuse after the borrow ends);
// * negative control: a stage trying to retain the incoming block in `self`
//   for the next quantum must be rejected by the borrow/lifetime checker.

mod compile_fail_check {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/direct_pcm_flow/compile_fail")
            .join(name)
    }

    fn compile_fixture(name: &str) -> std::process::Output {
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let out = std::env::temp_dir().join(format!(
            "direct-pcm-flow-fixture-{}-{}.rmeta",
            std::process::id(),
            name.replace('.', "_")
        ));
        let output = Command::new(rustc)
            .arg("--edition=2024")
            .arg("--crate-type=lib")
            .arg("--emit=metadata")
            .arg("-o")
            .arg(&out)
            .arg(fixture(name))
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn rustc for fixture {name}: {e}"));
        let _ = std::fs::remove_file(&out);
        output
    }

    #[test]
    fn positive_control_honest_stage_use_compiles() {
        let output = compile_fixture("honest_stage_use_compiles.rs");
        assert!(
            output.status.success(),
            "positive fixture must compile:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn negative_control_retaining_block_past_call_fails_to_compile() {
        let output = compile_fixture("stage_retains_block_past_call.rs");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "negative fixture compiled but retaining a block past the call must be rejected:\n{stderr}"
        );
        let lifetime_rejection = stderr.contains("lifetime")
            || [
                "error[E0499]",
                "error[E0502]",
                "error[E0521]",
                "error[E0597]",
                "error[E0311]",
            ]
            .iter()
            .any(|code| stderr.contains(code));
        assert!(
            lifetime_rejection,
            "fixture failed for an unexpected reason (expected a borrow/lifetime rejection):\n{stderr}"
        );
    }
}
