//! Test-only #296 controls. Each armed row runs in a fresh process.
//! PROPERTY: E-7 Tier-2 diagnostic producer independence, not delivery latency.
//! PRECONDITION/DAG: parent closes stderr or retains an unread pipe; a helper
//! holds the real stderr lock and signals acquisition before work begins,
//! then stdin GO SW child; held exporter sends entered SW child before work.
//! LEGAL: mandatory work returns while consumer remains held; losses counted.
//! OBSERVATION: caller owns terminal/join assertions, finish owns drain counts.
//! FALSIFIER: runner applies producer-wait and accounting mutations separately.
//! EXECUTION: 20s outer process watchdog, 5s inner watchdogs; no rescue PASS.

use qianqian_diagnostics::{
    Config, Diagnostics, Disposition, ExportError, Exporter, Level, Record, StderrExporter, Text,
    emit,
};
use std::io::{BufRead, Write};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;

pub struct Control {
    pub mode: String,
    pub diagnostics: Diagnostics,
    target: String,
    gate: Arc<(Mutex<bool>, Condvar)>,
}

// Evidence must bypass libtest capture, including assertion-path witnesses.
#[allow(clippy::explicit_write)]
pub fn line(s: &str) {
    writeln!(std::io::stdout(), "{s}").unwrap();
    std::io::stdout().flush().unwrap();
}

fn gate_input() {
    let mut s = String::new();
    std::io::stdin().lock().read_line(&mut s).unwrap();
    assert_eq!(s.trim(), "GO", "PREMISE: parent gate");
}

struct Sink {
    target: String,
    held: bool,
    entered: mpsc::Sender<()>,
    gate: Arc<(Mutex<bool>, Condvar)>,
}

impl Exporter for Sink {
    fn name(&self) -> &'static str {
        "tier2-real-stderr"
    }
    fn export(&mut self, r: &Record) -> Result<(), ExportError> {
        if self.target == "self-report" {
            return Err(ExportError {
                message: "Q296 force the real exporter-disable self-diagnostic",
            });
        }
        if r.site == "tier2-seed" && self.held {
            self.entered.send(()).unwrap();
            let (lock, cv) = &*self.gate;
            let guard = lock.lock().unwrap();
            drop(cv.wait_while(guard, |released| !*released).unwrap());
            return Ok(());
        }
        let matches = match self.target.as_str() {
            "8" => r.site == "qianqian-wasapi" && r.level == Level::Error,
            "9" => r.site == "qianqian-wasapi" && r.level == Level::Info,
            "10" => r.site == "qianqian-wasapi" && r.level == Level::Warn,
            "panic" => r.level == Level::Error,
            _ => true,
        };
        if matches {
            StderrExporter::default().export(r)
        } else {
            Ok(())
        }
    }

    fn flush(&mut self) -> Result<(), ExportError> {
        if self.target == "flush-report" {
            Err(ExportError {
                message: "Q296 force real flush-error self-report",
            })
        } else {
            Ok(())
        }
    }
}

impl Control {
    pub fn arm(target: &str) -> Option<Self> {
        let mode = std::env::var("Q296_MODE").ok()?;
        assert!(["healthy", "blocked", "stopped", "capacity", "broken"].contains(&mode.as_str()));
        // An assertion on a non-alpha thread must never itself hang on stderr.
        // Product hook installation captures this predecessor. Hook tests replace
        // it with their own controlled predecessor before installing posture.
        std::panic::set_hook(Box::new(|info| line(&format!("ASSERTION {info}"))));
        if mode == "blocked" {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _stderr = std::io::stderr().lock();
                tx.send(()).unwrap();
                let pair = (Mutex::new(false), Condvar::new());
                // No actor has a release handle; stderr stays locked until
                // process cleanup. This is SW, not a timed stall observation.
                drop(
                    pair.1
                        .wait_while(pair.0.lock().unwrap(), |open| !*open)
                        .unwrap(),
                );
            });
            rx.recv_timeout(Duration::from_secs(5))
                .expect("stderr lock acquired");
            line("Q296-STDERR-LOCK-HELD");
        }
        gate_input();
        if mode == "broken" {
            assert!(
                std::io::stderr().write_all(b"Q296-CANARY\n").is_err(),
                "PREMISE broken sink must fail inside child"
            );
            line("Q296-BROKEN-CONFIRMED");
        }
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let (tx, rx) = mpsc::channel();
        let held = mode == "stopped" || mode == "capacity";
        let diagnostics = Diagnostics::install(
            Box::new(Sink {
                target: target.into(),
                held,
                entered: tx,
                gate: gate.clone(),
            }),
            Config {
                beta_capacity: 4,
                poll_window: Duration::from_millis(1),
            },
        )
        .unwrap();
        if held {
            assert_eq!(
                emit(Record::new("tier2-seed", Level::Info)),
                Disposition::BetaQueued
            );
            rx.recv_timeout(Duration::from_secs(5))
                .expect("PREMISE exporter entered and awaits release");
            line("Q296-CONSUMER-HELD");
        }
        Some(Self {
            mode,
            diagnostics,
            target: target.into(),
            gate,
        })
    }

    pub fn finish(self) {
        if self.target == "flush-report" && self.mode == "blocked" {
            // First close admission to reach the genuine final-sweep flush.
            // The persistent counter proves that branch was entered before
            // observing a second bounded shutdown while its write is blocked.
            let _ = self.diagnostics.shutdown(Duration::ZERO);
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while self.diagnostics.counters().self_diagnostics == 0 {
                assert!(
                    std::time::Instant::now() < deadline,
                    "FLUSH-SELF-REPORT-PREMISE"
                );
                std::thread::yield_now();
            }
        }
        if self.mode == "blocked" || self.mode == "stopped" || self.mode == "capacity" {
            let r = self.diagnostics.shutdown(Duration::from_millis(50));
            assert!(
                r.abandoned && !r.drained,
                "honest abandoned shutdown: {r:?}"
            );
            line(&format!("Q296-ABANDONED {:?}", r.counters));
            // A held exporter is intentionally never released on the property
            // path. Process exit is cleanup, not a join/quiescence claim.
        } else {
            let r = self.diagnostics.shutdown(Duration::from_secs(5));
            assert!(r.drained && !r.abandoned, "settled drain: {r:?}");
            if self.target == "flush-report" {
                assert_eq!(r.counters.self_diagnostics, 1, "FLUSH-SELF-REPORT-ORACLE");
                assert_eq!(
                    r.counters.export_failed, 0,
                    "flush loss is not a record loss"
                );
            } else if self.mode == "broken" {
                assert!(
                    r.counters.export_failed > 0,
                    "LOSS-ORACLE: real target export must fail"
                );
                assert_eq!(r.counters.self_diagnostics, 1, "one exporter disable");
            }
            assert_eq!(
                r.counters.copied_out + r.counters.beta_consumed,
                r.counters.exported + r.counters.export_failed,
                "ACCOUNTING-ORACLE"
            );
            line(&format!("Q296-DRAINED {:?}", r.counters));
        }
        line("Q296-PASS");
        std::process::exit(0);
    }

    pub fn keep_held(&self) {
        assert!(!*self.gate.0.lock().unwrap());
    }
}

pub fn fill_current_alpha() {
    assert!(qianqian_diagnostics::is_alpha_producer());
    // The consumer was synchronously held before this producer was started.
    // Existing records may occupy either slot; the first rejection witnesses
    // the actual current pool's bound, without assuming how many were free.
    for _ in 0..3 {
        match emit(
            Record::new("tier2-fill", Level::Info).with_message(Text::from_static("capacity")),
        ) {
            Disposition::AlphaRecorded => (),
            Disposition::AlphaRejected => {
                line("Q296-ALPHA-FULL");
                return;
            }
            other => panic!("wrong admission path {other:?}"),
        }
    }
    panic!("CAPACITY-ORACLE: alpha pool did not reject at bound");
}
