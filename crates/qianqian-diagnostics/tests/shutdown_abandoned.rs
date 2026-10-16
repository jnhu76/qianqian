//! Shutdown boundary B: a drainer stuck inside a blocking sink must NOT be
//! waited on forever, and the abandonment must be reported HONESTLY — never
//! as "drain complete". Records stranded at the boundary are counted, and a
//! late producer after shutdown begins is dropped + counted instead of
//! reviving the drainer.

use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use qianqian_diagnostics::{
    Config, Diagnostics, Disposition, ExportError, Exporter, Level, Record, Text, emit,
};

#[derive(Default)]
struct GateState {
    entered: bool,
    released: bool,
}

struct StuckExporter {
    state: Mutex<GateState>,
    cv: Condvar,
    exported: Mutex<usize>,
}

impl StuckExporter {
    fn wait_until_entered(&self) {
        let mut state = self.state.lock().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !state.entered {
            let now = Instant::now();
            assert!(now < deadline, "the drainer never reached the sink");
            let (guard, _) = self
                .cv
                .wait_timeout(state, deadline.saturating_duration_since(now))
                .unwrap();
            state = guard;
        }
    }

    fn release(&self) {
        let mut state = self.state.lock().unwrap();
        state.released = true;
        self.cv.notify_all();
    }
}

impl StuckExporter {
    fn export(&self, _record: &Record) -> Result<(), ExportError> {
        {
            let mut state = self.state.lock().unwrap();
            state.entered = true;
            self.cv.notify_all();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !state.released {
                let now = Instant::now();
                assert!(now < deadline, "the test never released the stuck sink");
                let (guard, _) = self
                    .cv
                    .wait_timeout(state, deadline.saturating_duration_since(now))
                    .unwrap();
                state = guard;
            }
        }
        *self.exported.lock().unwrap() += 1;
        Ok(())
    }
}

struct Facility {
    diagnostics: Diagnostics,
    exporter: Arc<StuckExporter>,
}

fn facility() -> &'static Facility {
    static FACILITY: OnceLock<Facility> = OnceLock::new();
    FACILITY.get_or_init(|| {
        let exporter = Arc::new(StuckExporter {
            state: Mutex::new(GateState::default()),
            cv: Condvar::new(),
            exported: Mutex::new(0),
        });
        let diagnostics = Diagnostics::install(
            Box::new(StuckHandle {
                exporter: Arc::clone(&exporter),
            }),
            Config {
                beta_capacity: 8,
                poll_window: Duration::from_millis(5),
            },
        )
        .expect("the host installs exactly one facility");
        Facility {
            diagnostics,
            exporter,
        }
    })
}

struct StuckHandle {
    exporter: Arc<StuckExporter>,
}

impl Exporter for StuckHandle {
    fn name(&self) -> &'static str {
        "stuck"
    }

    fn export(&mut self, record: &Record) -> Result<(), ExportError> {
        StuckExporter::export(&self.exporter, record)
    }
}

fn record(marker: i64) -> Record {
    let mut message = Text::from_static("stranded=");
    message.push_i64(marker);
    Record::new("shutdown-boundary", Level::Error)
        .with_message(message)
        .stamped()
}

/// Oracle: `shutdown` returns inside its deadline with `abandoned = true`
/// (never `drained`), the stranded record is accounted, and a producer
/// emitting afterwards is rejected as `Shutdown` without blocking.
#[test]
fn a_stuck_sink_is_abandoned_not_awaited() {
    let facility = facility();
    assert_eq!(emit(record(1)), Disposition::BetaQueued);
    facility.exporter.wait_until_entered();

    let started = Instant::now();
    let report = facility.diagnostics.shutdown(Duration::from_millis(100));
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "shutdown must be bounded by its deadline, took {elapsed:?}"
    );
    assert!(
        !report.drained,
        "an abandoned drain is never reported drained"
    );
    assert!(report.abandoned, "the abandonment is explicit");
    assert!(
        elapsed >= Duration::from_millis(100),
        "the deadline was actually waited, took {elapsed:?}"
    );

    // A late producer never blocks and never revives the drainer.
    assert_eq!(
        emit(record(2)),
        Disposition::Shutdown,
        "post-shutdown records are dropped and counted"
    );
    // The report's snapshot predates the late emit, so read the live counters.
    let counters = facility.diagnostics.counters();
    assert!(
        counters.beta_rejected >= 1,
        "the late record is counted as an admission loss: {counters:?}"
    );
    assert_eq!(counters.exported, 0, "nothing reached a stuck sink");
    assert_eq!(report.counters.exported, 0, "the report agrees: {report:?}");
    assert!(
        report.undelivered_at_exit >= 1,
        "the abandonment is bounded by what was still undelivered: {report:?}"
    );

    // Release the sink: the abandoned drainer completes its sweep on its own
    // (its records are the ones the boundary already accounted for). The
    // process must not deadlock or panic on the way out.
    facility.exporter.release();
    let deadline = Instant::now() + Duration::from_secs(5);
    while *facility.exporter.exported.lock().unwrap() == 0 {
        assert!(
            Instant::now() < deadline,
            "the abandoned drainer never finished its in-flight record"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
