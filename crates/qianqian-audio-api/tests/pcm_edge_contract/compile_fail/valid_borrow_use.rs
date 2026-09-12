//! Positive control for the compile-fail evidence: legitimate sequential
//! borrow, consume, release, and reuse of producer storage compiles.

#[path = "../harness.rs"]
mod harness;

use harness::{PcmFormat, SyntheticConsumer, SyntheticProducer, TransferError};

pub fn borrow_consume_release_then_reuse() -> Result<usize, TransferError> {
    let format = PcmFormat::new(48_000, 2)?;
    let mut producer = SyntheticProducer::new(format);
    let mut consumer = SyntheticConsumer::new(format);
    let mut storage = vec![0.0; format.scalar_count(4)];

    let first = producer.lend_read_only(&mut storage, 4)?;
    let mut verified = 0;
    verified += first.frames();
    consumer.verify_view(&first)?;
    drop(first);

    // Reuse is legal once the previous borrow has ended.
    let second = producer.lend_read_only(&mut storage, 4)?;
    verified += second.frames();
    consumer.verify_view(&second)?;
    drop(second);

    Ok(verified)
}
