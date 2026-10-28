//! #296 real WASAPI outlet rows. Only cfg(test) seams; production unchanged.
//! Mandatory #8 settle has no rescue stop; #9/#10 require actual emission
//! before the intentional Stop command. Observe after dispose joins the leg.

#[path = "../../../research/diagnostics-tier2/support.rs"]
mod support;

use qianqian_diagnostics::Level;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static HITS: AtomicUsize = AtomicUsize::new(0);
static REJECTED_BEFORE: AtomicUsize = AtomicUsize::new(0);

fn target(level: Level) -> bool {
    let Ok(site) = std::env::var("Q296_OUTLET") else {
        return false;
    };
    matches!(
        (site.as_str(), level),
        ("8", Level::Error) | ("9", Level::Info) | ("10", Level::Warn)
    )
}

pub(crate) fn before_emit(level: Level) {
    if target(level) && std::env::var("Q296_MODE").as_deref() == Ok("capacity") {
        support::fill_current_alpha();
        REJECTED_BEFORE.store(
            qianqian_diagnostics::Diagnostics::global_counters()
                .unwrap()
                .alpha_rejected as usize,
            Ordering::Release,
        );
    }
}

pub(crate) fn after_emit(level: Level) {
    if target(level) {
        if std::env::var("Q296_MODE").as_deref() == Ok("capacity") {
            let actual = qianqian_diagnostics::Diagnostics::global_counters()
                .unwrap()
                .alpha_rejected as usize;
            let expected = REJECTED_BEFORE.load(Ordering::Acquire) + 1;
            if actual != expected {
                support::line(&format!(
                    "TARGET-REJECT-ORACLE actual={actual} expected={expected}"
                ));
            }
            assert_eq!(
                qianqian_diagnostics::Diagnostics::global_counters()
                    .unwrap()
                    .alpha_rejected as usize,
                REJECTED_BEFORE.load(Ordering::Acquire) + 1,
                "TARGET-REJECT-ORACLE"
            );
        }
        HITS.fetch_add(1, Ordering::Release);
    }
}

fn await_hit() {
    let deadline = Instant::now() + Duration::from_secs(5);
    while HITS.load(Ordering::Acquire) == 0 {
        assert!(
            Instant::now() < deadline,
            "SITE-PREMISE: actual target emission never returned"
        );
        std::thread::yield_now();
    }
}

#[test]
fn tier2_child() {
    let Ok(site) = std::env::var("Q296_OUTLET") else {
        return;
    };
    let Some(control) = support::Control::arm(&site) else {
        return;
    };
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if qianqian_diagnostics::is_alpha_producer() {
            qianqian_diagnostics::note_critical_panic();
        } else {
            previous(info);
        }
    }));
    if site == "10" {
        crate::wasapi::volume_apply_seam::set_override(Some(|_, _| {
            Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x8007_0057u32 as i32,
            )))
        }));
    }
    let mut f = crate::cone_oracle_tests::ConeFixture::with_inject_at(if site == "8" {
        3
    } else {
        usize::MAX
    });
    f.activate();
    if site == "8" {
        f.await_injection()
            .expect("DEVICE-PREMISE: real render injection");
        assert_eq!(
            f.settled_within(Duration::from_secs(5)),
            Some(qianqian_playback::EpisodeTerminalOutcome::Failed),
            "TERMINAL-ORACLE: no external stop permitted"
        );
        await_hit();
        assert!(
            f.witness.verdict_snapshot.load(Ordering::Acquire) > 0,
            "edge stopped before verdict"
        );
    } else {
        if site == "10" {
            f.handle.request_output_level(40);
        }
        await_hit();
        // The hit is Release-published AFTER the actual emit. This command
        // cannot rescue a target that never ran or blocked inside emit.
        f.handle.request_stop();
        assert_eq!(
            f.settled_within(Duration::from_secs(5)),
            Some(qianqian_playback::EpisodeTerminalOutcome::Stopped),
            "TERMINAL-ORACLE"
        );
    }
    assert!(f.dispose_quietly(), "JOIN-ORACLE");
    assert_eq!(
        HITS.load(Ordering::Acquire),
        1,
        "one actual target emission"
    );
    control.keep_held();
    support::line(&format!("Q296-TERMINAL outlet={site} hits=1 joined=true"));
    control.finish();
}
