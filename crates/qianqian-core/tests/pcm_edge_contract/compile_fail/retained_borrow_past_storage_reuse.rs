//! Negative control for the compile-fail evidence: a consumer that retains
//! a borrowed view while the producer reuses the backing storage must not
//! compile. The expected rejection is the borrow/lifetime error family
//! (E0499/E0502/E0505/E0597).

#[path = "../harness.rs"]
mod harness;

use harness::{PcmFormat, SyntheticProducer};

pub fn retained_view_blocks_storage_reuse() {
    let format = PcmFormat::new(48_000, 2).expect("valid format");
    let mut producer = SyntheticProducer::new(format);
    let mut storage = vec![0.0; format.scalar_count(8)];

    let retained = producer
        .lend_read_only(&mut storage, 4)
        .expect("first lend is valid");

    // Storage reuse while `retained` is still live: must be rejected.
    let second = producer
        .lend_read_only(&mut storage, 4)
        .expect("reuse while retained");

    let _ = (retained.frames(), second.frames());
}
