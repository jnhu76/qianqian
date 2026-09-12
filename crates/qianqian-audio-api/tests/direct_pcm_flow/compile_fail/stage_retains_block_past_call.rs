//! Negative control for the compile-fail evidence: a stage that tries to
//! retain the incoming block past the call — storing the borrowed view in
//! `self` for the next quantum — must be rejected. The tested stage trait
//! hands the block lifetime through by value (`for<'block>`), so retention
//! cannot be named. Expected rejection: the borrow/lifetime error family.

#[path = "../../pcm_edge_contract/harness.rs"]
mod harness;
#[path = "../participants.rs"]
mod participants;

use harness::{PcmFormat, PcmView, PcmViewMut};
use participants::PcmStage;

/// A stage that tries to keep the incoming block for the next quantum.
pub struct RetainingStage<'a> {
    format: PcmFormat,
    kept: Option<PcmViewMut<'a>>,
}

impl PcmStage for RetainingStage<'_> {
    fn process<'block>(&mut self, block: PcmViewMut<'block>) -> PcmView<'block> {
        // Must be rejected: `for<'block>` means the block borrow may not be
        // stored into a struct with a fixed lifetime.
        self.kept = Some(block);
        let empty: &mut [harness::Sample] = &mut [];
        PcmViewMut::new(self.format, empty)
            .expect("zero frames are a valid shape")
            .freeze()
    }
}
