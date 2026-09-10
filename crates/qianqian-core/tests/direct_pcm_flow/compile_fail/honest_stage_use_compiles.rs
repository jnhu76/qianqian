//! Positive control for the compile-fail evidence: a compliant stage handing
//! the block lifetime through, and a sequential quantum chain with storage
//! reuse after the borrow ends, type-check against the real participant
//! trait shape.

#[path = "../../pcm_edge_contract/harness.rs"]
mod harness;
#[path = "../participants.rs"]
mod participants;

use harness::{PcmFormat, PcmView, PcmViewMut, Sample};
use participants::{IdentityStage, PcmStage};

pub fn honest_quantum_chain_compiles_and_reuses_storage() {
    let format = PcmFormat::new(48_000, 2).expect("valid format");
    let mut stage = IdentityStage;
    let mut storage: Vec<Sample> = vec![0.0; format.scalar_count(4)];

    let block: PcmViewMut<'_> = PcmViewMut::new(format, &mut storage).expect("whole frames");
    let processed: PcmView<'_> = stage.process(block);
    assert_eq!(processed.frames(), 4);
    drop(processed);

    // Storage reuse after the previous borrow has ended is exactly what
    // this shape allows.
    let block = PcmViewMut::new(format, &mut storage).expect("reuse compiles");
    let _processed = stage.process(block);
}
