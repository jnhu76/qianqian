//! Negative + positive controls for the test-support oracles (issue #121
//! corrective). The thread-leak diagnostic's bounded-poll tolerance for a
//! just-joined worker's OS exit must not become blindness to a real
//! leak; the watchdog must report a body panic as itself, not as a
//! timeout, and must still bound genuinely slow bodies.

mod common;

use std::time::Duration;

use common::{named_thread_gone_within, within};

#[test]
fn diagnostic_oracle_catches_a_genuine_leak() {
    // A worker that is never signaled and never joined must keep the
    // oracle reporting "still present" for the whole diagnostic window.
    let handle = std::thread::Builder::new()
        .name("qianqian-leak-probe".into())
        .spawn(|| std::thread::sleep(Duration::from_millis(1_500)))
        .expect("leak-probe worker spawns");
    assert!(
        !named_thread_gone_within("qianqian-leak-probe", Duration::from_millis(300)),
        "diagnostic oracle failed to detect a genuinely leaked named thread"
    );
    // Retire the probe so it cannot outlive this window; joining is the
    // semantic cleanup, and the oracle tolerance above covers only the
    // post-join /proc lag.
    handle.join().expect("leak-probe worker joins");
}

#[test]
fn diagnostic_oracle_accepts_a_joined_worker() {
    let handle = std::thread::Builder::new()
        .name("qianqian-join-probe".into())
        .spawn(|| ())
        .expect("join-probe worker spawns");
    handle.join().expect("join-probe worker joins");
    assert!(
        named_thread_gone_within("qianqian-join-probe", Duration::from_secs(2)),
        "diagnostic oracle must tolerate a joined worker's OS exit window"
    );
}

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
