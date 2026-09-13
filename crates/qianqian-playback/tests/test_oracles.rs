//! Positive + negative controls for the watchdog oracle (issue #121
//! corrective): a body panic must surface as itself, not as a timeout,
//! and genuinely slow bodies must still be bounded.

mod common;

use std::time::Duration;

use common::within;

#[test]
fn within_returns_the_body_value() {
    assert_eq!(within(Duration::from_secs(10), || 7u32), 7);
}

#[test]
#[should_panic(expected = "probe body exploded")]
fn within_reports_the_body_panic_not_a_timeout() {
    within(Duration::from_secs(10), || panic!("probe body exploded"));
}

#[test]
#[should_panic(expected = "operation exceeded")]
fn within_still_bounds_a_slow_body() {
    within(Duration::from_millis(100), || {
        std::thread::sleep(Duration::from_secs(5))
    });
}
