//! #296 hook / detached exit-report controls; fresh child per row.
#[path = "../../../research/diagnostics-tier2/support.rs"]
mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;

static REPORT_ENTERED: AtomicUsize = AtomicUsize::new(0);
static REPORT_RESULT: AtomicUsize = AtomicUsize::new(0);

pub(super) fn report_entered() {
    REPORT_ENTERED.store(1, Ordering::Release);
}
pub(super) fn report_returned(ok: bool) {
    REPORT_RESULT.store(if ok { 1 } else { 2 }, Ordering::Release);
}

#[test]
fn tier2_child() {
    let Ok(outlet) = std::env::var("Q296_OUTLET") else {
        return;
    };
    let inherited_hook = std::panic::take_hook();
    let Some(control) = support::Control::arm(&outlet) else {
        return;
    };
    if outlet == "panic" {
        static PREVIOUS: AtomicUsize = AtomicUsize::new(0);
        std::panic::set_hook(Box::new(|info| {
            PREVIOUS.fetch_add(1, Ordering::Relaxed);
            support::line(&format!("Q296-PREVIOUS-HOOK {info}"));
        }));
        crate::machine_input::install_panic_hook_posture();
        let mode = control.mode.clone();
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let registration = qianqian_diagnostics::AlphaRegistration::register();
            if mode == "capacity" {
                support::fill_current_alpha();
            }
            let before = registration.rejected();
            for _ in 0..2 {
                assert!(std::panic::catch_unwind(|| panic!("Q296-critical")).is_err());
            }
            if mode == "capacity" {
                if registration.rejected() != before + 1 {
                    support::line(&format!(
                        "PANIC-REJECT-ORACLE actual={} expected={}",
                        registration.rejected(),
                        before + 1
                    ));
                }
                assert_eq!(
                    registration.rejected(),
                    before + 1,
                    "first full-pool panic rejected, second one-shot suppressed"
                );
            }
            tx.send(()).unwrap();
        });
        rx.recv_timeout(Duration::from_secs(5))
            .expect("HOOK-PROGRESS-ORACLE");
        worker.join().unwrap();
        assert_eq!(PREVIOUS.load(Ordering::Relaxed), 0, "HOOK-CHAIN-ORACLE");
        assert_eq!(qianqian_diagnostics::critical_panics(), 2);
        support::line("Q296-HOOK critical=2 previous=0 joined=true");
    } else if outlet == "previous" {
        if control.mode == "blocked" || control.mode == "broken" {
            let (tx, rx) = mpsc::channel();
            std::panic::set_hook(Box::new(move |info| {
                tx.send(()).unwrap();
                inherited_hook(info);
            }));
            crate::machine_input::install_panic_hook_posture();
            let w = std::thread::spawn(|| {
                assert!(std::panic::catch_unwind(|| panic!("Q296-default-previous")).is_err());
            });
            rx.recv_timeout(Duration::from_secs(5))
                .expect("PREVIOUS-HOOK-PREMISE");
            if control.mode == "broken" {
                w.join().unwrap();
                support::line(
                    "Q296-KNOWN-RESIDUAL default previous hook returned on broken stderr; arbitrary predecessor behavior remains unbounded",
                );
            } else {
                support::line(
                    "Q296-KNOWN-RESIDUAL predecessor wrapper entered; inherited call next in PO, actual default-call entry unobserved; real stderr lock remains held; no join claimed",
                );
            }
            std::process::exit(0);
        }
        // Exact synchronization witness of the documented non-alpha residual.
        let pair = Arc::new((Mutex::new(false), Condvar::new()));
        let gate = pair.clone();
        let (tx, rx) = mpsc::channel();
        std::panic::set_hook(Box::new(move |_| {
            tx.send(()).unwrap();
            let (lock, cv) = &*gate;
            drop(cv.wait_while(lock.lock().unwrap(), |open| !*open).unwrap());
        }));
        crate::machine_input::install_panic_hook_posture();
        let w = std::thread::spawn(|| {
            let _ = std::panic::catch_unwind(|| panic!("Q296-non-alpha"));
        });
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // Reaching the previous hook while release=false proves the dependency
        // structurally. No timeout is used as a positive happens-before edge.
        assert!(!*pair.0.lock().unwrap());
        support::line(
            "Q296-KNOWN-RESIDUAL non-alpha entered synchronous previous hook before release",
        );
        *pair.0.lock().unwrap() = true;
        pair.1.notify_all();
        w.join().unwrap();
        std::process::exit(0);
    } else if outlet == "flush-report" {
        support::line("Q296-FLUSH-REPORT trigger=real-shutdown-final-sweep");
    } else if outlet == "self-report" {
        use qianqian_diagnostics::{Diagnostics, Level, Record, emit};
        let worker = std::thread::spawn(|| {
            let _registration = qianqian_diagnostics::AlphaRegistration::register();
            let _ = emit(Record::new("Q296-failing-export", Level::Error));
        });
        worker.join().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while Diagnostics::global_counters().unwrap().self_diagnostics == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "SELF-DIAGNOSTIC-PREMISE"
            );
            std::thread::yield_now();
        }
        support::line("Q296-SELF-DIAGNOSTIC-ATTEMPTED producer_joined=true");
    } else if outlet == "exit-report" {
        // No diagnostic queue at this outlet. Detached writer return is the
        // host progress boundary. Thread completion itself is not promised.
        super::attempt_exit_report("Q296 exit report".into(), Some(Duration::from_millis(50)));
        support::line("Q296-EXIT-REPORT-RETURNED");
        if control.mode == "broken" || control.mode == "healthy" {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while REPORT_RESULT.load(Ordering::Acquire) == 0 {
                assert!(
                    std::time::Instant::now() < deadline,
                    "REPORT-RESULT-ORACLE real writer never returned"
                );
                std::thread::yield_now();
            }
            assert_eq!(
                REPORT_RESULT.load(Ordering::Acquire),
                if control.mode == "broken" { 2 } else { 1 },
                "REPORT-RESULT-ORACLE"
            );
            support::line("Q296-PASS");
            std::process::exit(0);
        }
        if control.mode == "blocked" {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while REPORT_ENTERED.load(Ordering::Acquire) == 0 {
                assert!(std::time::Instant::now() < deadline, "REPORT-ENTRY-ORACLE");
                std::thread::yield_now();
            }
            assert_eq!(
                REPORT_RESULT.load(Ordering::Acquire),
                0,
                "writer cannot return while stderr lock is held"
            );
            support::line("Q296-PASS report entered, host returned while stderr lock remains held");
            std::process::exit(0);
        }
    } else {
        panic!("unknown outlet {outlet}");
    }
    control.keep_held();
    control.finish();
}
