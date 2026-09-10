//! Minimal PCM data-edge experiment — test-only harness.
//!
//! Evidence collection for the experiment recorded in
//! `docs/architecture/pcm-contract-a0.md` (evidence, not normative
//! authority). Nothing in this file tree is part of the `qianqian-core`
//! library API.
//!
//! Layout:
//!
//! ```text
//! pcm_edge_contract/main.rs             instrumentation + scenario tests
//! pcm_edge_contract/harness.rs          types + deterministic sample oracle
//! pcm_edge_contract/candidates.rs       four candidate edge shapes
//! pcm_edge_contract/mutations.rs        adversarial twins + kill tests
//! pcm_edge_contract/compile_fail/       rustc fixtures (borrow-lifetime evidence)
//! ```
//!
//! Run:
//!
//! ```bash
//! cargo test -p qianqian-core --test pcm_edge_contract
//! ```

mod candidates;
mod harness;
mod mutations;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::process::Command;

use candidates::{
    BorrowedInPlaceFlow, BorrowedReadOnlyFlow, ConsumerFilledFlow, OwnedTransferFlow,
};
use harness::{PcmFormat, Sample};

// ---------------------------------------------------------------------------
// Instrumentation: thread-scoped counting allocator
// ---------------------------------------------------------------------------
//
// A counting global allocator gives *measured* allocation evidence for the
// steady-state transfer loops, replacing the earlier cooperative
// self-reporting counters (which could only see allocations the code under
// test chose to report).
//
// Counting is armed per thread and per measurement window: allocations by
// other threads (including test-harness coordination and test dispatch) are
// excluded, so the count attributes heap work performed by the measured
// closure itself. This keeps the measurement deterministic under parallel
// test execution.

thread_local! {
    static ALLOC_WINDOW_ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOC_WINDOW_COUNT: Cell<usize> = const { Cell::new(0) };
}

struct CountingAllocator;

// SAFETY: forwards every allocation call to the system allocator; the
// bookkeeping touches only const-initialized thread-locals without
// destructors, so it performs no allocation of its own on any path.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_if_armed();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_if_armed();
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_if_armed();
        unsafe { System.alloc_zeroed(layout) }
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

#[inline]
fn record_if_armed() {
    ALLOC_WINDOW_ARMED.with(|armed| {
        if armed.get() {
            ALLOC_WINDOW_COUNT.with(|count| count.set(count.get() + 1));
        }
    });
}

/// Runs `f` with allocation counting armed for the current thread only and
/// returns `(f's result, allocations performed)`.
fn run_counting_allocations<R>(f: impl FnOnce() -> R) -> (R, usize) {
    struct DisarmOnDrop;
    impl Drop for DisarmOnDrop {
        fn drop(&mut self) {
            ALLOC_WINDOW_ARMED.with(|armed| armed.set(false));
        }
    }
    ALLOC_WINDOW_COUNT.with(|count| count.set(0));
    ALLOC_WINDOW_ARMED.with(|armed| armed.set(true));
    let _disarm = DisarmOnDrop;
    let result = f();
    let count = ALLOC_WINDOW_COUNT.with(|count| count.get());
    (result, count)
}

// ---------------------------------------------------------------------------
// Scenarios: frame correctness across channel counts and block sizes
// ---------------------------------------------------------------------------

fn stereo() -> PcmFormat {
    PcmFormat::new(48_000, 2).expect("stereo is a valid format")
}

#[test]
fn borrowed_flow_delivers_stream_for_mono_stereo_and_three_channel() {
    for channel_count in [1u16, 2, 3] {
        let format = PcmFormat::new(48_000, channel_count).expect("valid format");
        let mut flow = BorrowedReadOnlyFlow::new(format, 4);
        flow.run(10).expect("stream must verify for every layout");
        assert_eq!(flow.report().frames_delivered, 10);
        assert_eq!(flow.consumer().frames_consumed(), 10);
    }
}

#[test]
fn borrowed_flow_partial_final_block_is_delivered() {
    let mut flow = BorrowedReadOnlyFlow::new(stereo(), 3);
    flow.run(7).expect("partial final block must verify");
    assert_eq!(flow.report().frames_delivered, 7);
    assert_eq!(flow.report().blocks, 3); // 3 + 3 + 1
}

#[test]
fn borrowed_flow_arbitrary_block_sizes_deliver_the_whole_stream() {
    for block_frames in [1usize, 3, 17, 256] {
        let mut flow = BorrowedReadOnlyFlow::new(stereo(), block_frames);
        flow.run(100)
            .unwrap_or_else(|e| panic!("block size {block_frames} failed: {e}"));
        assert_eq!(flow.report().frames_delivered, 100);
    }
}

#[test]
fn producer_and_consumer_cursors_reconcile_across_all_shapes() {
    let total = 37usize;

    let mut borrowed = BorrowedReadOnlyFlow::new(stereo(), 4);
    borrowed.run(total).expect("borrowed flow is valid");
    let mut in_place = BorrowedInPlaceFlow::new(stereo(), 4);
    in_place.run(total).expect("in-place flow is valid");
    let mut owned = OwnedTransferFlow::new(stereo(), 4);
    owned.run(total).expect("owned flow is valid");
    let mut pulled = ConsumerFilledFlow::new(stereo(), 4);
    pulled.run(total).expect("pull flow is valid");

    for (name, delivered) in [
        ("borrowed", borrowed.report().frames_delivered),
        ("in-place", in_place.report().frames_delivered),
        ("owned", owned.report().frames_delivered),
        ("pull", pulled.report().frames_delivered),
    ] {
        assert_eq!(delivered, total, "{name} shape must conserve frames");
    }
}

// ---------------------------------------------------------------------------
// Scenarios: storage identity (structural copy evidence)
// ---------------------------------------------------------------------------

#[test]
fn borrowed_shapes_hand_over_producer_storage_without_intermediates() {
    let mut read_only = BorrowedReadOnlyFlow::new(stereo(), 4);
    read_only.run(16).expect("flow is valid");
    assert_eq!(
        read_only.report().producer_storage_addr,
        read_only.report().consumer_observed_storage_addr,
        "read-only lend must not insert storage"
    );

    let mut in_place = BorrowedInPlaceFlow::new(stereo(), 4);
    in_place.run(16).expect("flow is valid");
    assert_eq!(
        in_place.report().producer_storage_addr,
        in_place.report().consumer_observed_storage_addr,
        "in-place lend must not insert storage"
    );

    let mut pulled = ConsumerFilledFlow::new(stereo(), 4);
    pulled.run(16).expect("flow is valid");
    assert_eq!(
        pulled.report().producer_storage_addr,
        pulled.report().consumer_observed_storage_addr,
        "pull fill must not insert storage"
    );
}

#[test]
fn owned_shape_reports_consumer_observed_storage() {
    let mut owned = OwnedTransferFlow::new(stereo(), 4);
    owned.run(16).expect("flow is valid");
    // There is no single producer storage to compare against: each block is a
    // fresh allocation. The per-block cost evidence is the *measured*
    // allocation count in the mutations module; this test only pins that the
    // report carries the observed storage address.
    assert!(owned.report().consumer_observed_storage_addr.is_some());
}

// ---------------------------------------------------------------------------
// Scenarios: steady-state allocation measurements
// ---------------------------------------------------------------------------

#[test]
fn in_place_and_pull_steady_states_are_allocation_free_measured() {
    let mut in_place = BorrowedInPlaceFlow::new(stereo(), 8);
    in_place.run(8).expect("warmup is valid");
    let (result, allocated) = run_counting_allocations(|| in_place.run(64));
    result.expect("steady state is valid");
    assert_eq!(
        allocated, 0,
        "in-place steady state must not heap-allocate (measured {allocated})"
    );

    let mut pulled = ConsumerFilledFlow::new(stereo(), 8);
    pulled.run(8).expect("warmup is valid");
    let (result, allocated) = run_counting_allocations(|| pulled.run(64));
    result.expect("steady state is valid");
    assert_eq!(
        allocated, 0,
        "pull steady state must not heap-allocate (measured {allocated})"
    );
}

// ---------------------------------------------------------------------------
// Scenarios: pull flow control
// ---------------------------------------------------------------------------

#[test]
fn pull_flow_respects_consumer_capacity_and_conserves_frames() {
    // Capacity 3 frames per fill; total 5 frames -> fills of 3 then 2.
    let mut pulled = ConsumerFilledFlow::new(stereo(), 3);
    pulled.run(5).expect("pull flow is valid");
    assert_eq!(pulled.report().blocks, 2);
    assert_eq!(pulled.report().frames_delivered, 5);
}

#[test]
fn pull_capacity_slack_is_never_written_as_partial_frames() {
    // Destination with 7 scalars of stereo capacity: only 3 whole frames are
    // fillable; the trailing scalar stays untouched and nothing is truncated.
    let format = stereo();
    let mut producer = harness::SyntheticProducer::new(format);
    let mut consumer = harness::SyntheticConsumer::new(format);
    let mut dest: Vec<Sample> = vec![-1.0; 7];
    let sentinel_tail = -1.0;
    let produced = harness::FillDestination::fill(&mut producer, &mut dest);
    assert_eq!(produced, 3, "7 scalars hold 3 whole stereo frames");
    consumer
        .verify_filled(&dest, produced)
        .expect("filled prefix verifies");
    assert_eq!(dest[6], sentinel_tail, "capacity slack must stay untouched");
}

// ---------------------------------------------------------------------------
// Compile-fail evidence: borrowed storage lifetime
// ---------------------------------------------------------------------------
//
// A doc-comment `compile_fail` inside a test module is never collected by
// rustdoc, so it executes nothing. Instead these tests compile fixture files
// with a real `rustc` subprocess and assert on the outcome:
//
// * positive control: legitimate sequential borrow + reuse compiles;
// * negative control: retaining a borrowed view across producer storage
//   reuse must be rejected with a borrow/lifetime error.

mod compile_fail_check {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/pcm_edge_contract/compile_fail")
            .join(name)
    }

    fn compile_fixture(name: &str) -> std::process::Output {
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let out = std::env::temp_dir().join(format!(
            "pcm-edge-fixture-{}-{}.rmeta",
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
    fn positive_control_sequential_borrow_compiles() {
        let output = compile_fixture("valid_borrow_use.rs");
        assert!(
            output.status.success(),
            "positive fixture must compile:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn negative_control_retained_borrow_fails_to_compile() {
        let output = compile_fixture("retained_borrow_past_storage_reuse.rs");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "negative fixture compiled but a retained borrow must be rejected:\n{stderr}"
        );
        let lifetime_rejection = [
            "error[E0499]",
            "error[E0502]",
            "error[E0505]",
            "error[E0597]",
        ]
        .iter()
        .any(|code| stderr.contains(code));
        assert!(
            lifetime_rejection,
            "fixture failed for an unexpected reason (expected a borrow/lifetime rejection):\n{stderr}"
        );
    }
}
