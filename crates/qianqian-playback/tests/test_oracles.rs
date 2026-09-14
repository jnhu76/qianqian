//! Negative + positive controls for the test-support oracles (issue #121
//! corrective). The thread-leak diagnostic's bounded-poll tolerance for
//! the /proc listing's observation lag must not become blindness to a
//! real leak; the watchdog must report a body panic as itself, not as a
//! timeout, and must still bound genuinely slow bodies.

mod common;

use std::time::{Duration, Instant};

use common::within;

// The /proc-based leak-oracle controls are Linux-only by nature (the
// oracle reads /proc/self/task); they must not break the suite's build
// on the Windows gate.
#[cfg(target_os = "linux")]
use common::{named_thread_alive, named_thread_gone_within};

#[cfg(target_os = "linux")]
#[test]
fn diagnostic_oracle_catches_a_genuine_leak() {
    // A worker that is never signaled and never joined must keep the
    // oracle reporting "still present" for the whole diagnostic window.
    let handle = std::thread::Builder::new()
        .name("qianqian-leak-probe".into())
        .spawn(|| std::thread::sleep(Duration::from_millis(1_500)))
        .expect("leak-probe worker spawns");
    // std applies the thread's comm name from the child after its first
    // scheduling, so the name has its own startup observation lag — the
    // same /proc lag class the oracle tolerates. Wait (bounded) for the
    // leak to become observable before pinning the diagnostic window;
    // otherwise a pre-name first poll reads as "gone" and the control
    // tests nothing but scheduler timing.
    let visible_by = Instant::now() + Duration::from_secs(2);
    while !named_thread_alive("qianqian-leak-probe") {
        assert!(
            Instant::now() < visible_by,
            "leak-probe name never became observable in /proc/self/task"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        !named_thread_gone_within("qianqian-leak-probe", Duration::from_millis(300)),
        "diagnostic oracle failed to detect a genuinely leaked named thread"
    );
    // Retire the probe so it cannot outlive this window; joining is the
    // semantic cleanup, and the oracle tolerance above covers only the
    // /proc listing's observation lag.
    handle.join().expect("leak-probe worker joins");
}

#[cfg(target_os = "linux")]
#[test]
fn diagnostic_oracle_accepts_a_joined_worker() {
    let handle = std::thread::Builder::new()
        .name("qianqian-join-probe".into())
        .spawn(|| ())
        .expect("join-probe worker spawns");
    handle.join().expect("join-probe worker joins");
    assert!(
        named_thread_gone_within("qianqian-join-probe", Duration::from_secs(2)),
        "diagnostic oracle must tolerate the /proc listing's observation lag"
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
