//! Class-β admission: with the drainer blocked inside a sink (the declared
//! residual a synchronous exporter always has), the bounded channel fills and
//! further records are REJECTED at the queue (dropped + counted, no retry
//! loop, no producer wait). Once the sink is released, every queued record is
//! delivered — rejection is accounted, not hidden.

use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use qianqian_diagnostics::{
    Config, Diagnostics, Disposition, ExportError, Exporter, Level, Record, Text, emit,
};

/// The capacity of the class-β channel in this scenario.
const CAPACITY: usize = 4;

#[derive(Default)]
struct GateState {
    entered: bool,
    released: bool,
}

/// The sink's state plus the test's controls. The exporter owns an `Arc` of
/// this and forwards to `&self` methods, so the test keeps its controls while
/// the facility owns the sink.
struct BlockingExporter {
    state: Mutex<GateState>,
    cv: Condvar,
    exported: Mutex<Vec<String>>,
}

impl BlockingExporter {
    fn wait_until_entered(&self) {
        let mut state = self.state.lock().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !state.entered {
            let now = Instant::now();
            assert!(
                now < deadline,
                "the drainer never reached the sink (so the queue could not be filled)"
            );
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

    fn exported_count(&self) -> usize {
        self.exported.lock().unwrap().len()
    }
}

impl BlockingExporter {
    fn export(&self, record: &Record) -> Result<(), ExportError> {
        {
            let mut state = self.state.lock().unwrap();
            if !state.released {
                state.entered = true;
                self.cv.notify_all();
                let deadline = Instant::now() + Duration::from_secs(5);
                while !state.released {
                    let now = Instant::now();
                    assert!(now < deadline, "the sink was never released");
                    let (guard, _) = self
                        .cv
                        .wait_timeout(state, deadline.saturating_duration_since(now))
                        .unwrap();
                    state = guard;
                }
            }
        }
        self.exported
            .lock()
            .unwrap()
            .push(record.message.render().into_owned());
        Ok(())
    }
}

struct Facility {
    diagnostics: Diagnostics,
    exporter: Arc<BlockingExporter>,
}

fn facility() -> &'static Facility {
    static FACILITY: OnceLock<Facility> = OnceLock::new();
    FACILITY.get_or_init(|| {
        let exporter = Arc::new(BlockingExporter {
            state: Mutex::new(GateState::default()),
            cv: Condvar::new(),
            exported: Mutex::new(Vec::new()),
        });
        let diagnostics = Diagnostics::install(
            Box::new(SharedExporter {
                exporter: Arc::clone(&exporter),
            }),
            Config {
                beta_capacity: CAPACITY,
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

struct SharedExporter {
    exporter: Arc<BlockingExporter>,
}

impl Exporter for SharedExporter {
    fn name(&self) -> &'static str {
        "blocking"
    }

    fn export(&mut self, record: &Record) -> Result<(), ExportError> {
        BlockingExporter::export(&self.exporter, record)
    }
}

fn record(marker: i64) -> Record {
    let mut message = Text::from_static("beta=");
    message.push_i64(marker);
    Record::new("beta-full", Level::Warn)
        .with_message(message)
        .stamped()
}

/// Oracle: with the sink blocked, exactly `CAPACITY` records queue and the
/// rest are rejected; after the release every queued record arrives, the
/// rejected ones do not, and the counters tell the truth about both.
#[test]
fn a_blocked_sink_fills_the_queue_and_rejections_are_counted() {
    let facility = facility();

    // The first record reaches the sink, which blocks the drainer there.
    assert_eq!(emit(record(1)), Disposition::BetaQueued);
    facility.exporter.wait_until_entered();

    // The channel now has CAPACITY free slots: fill it, then push past it.
    let mut queued = 0;
    let mut rejected = 0;
    for marker in 2..=(CAPACITY as i64 + 4) {
        match emit(record(marker)) {
            Disposition::BetaQueued => queued += 1,
            Disposition::BetaRejected => rejected += 1,
            other => panic!("unexpected disposition {other:?}"),
        }
    }
    assert_eq!(
        queued, CAPACITY as i64,
        "exactly the free capacity is admitted"
    );
    assert_eq!(
        rejected, 3,
        "everything past the bound is rejected (7 emissions: 4 queued + 3 rejected)"
    );

    facility.exporter.release();
    let deadline = Instant::now() + Duration::from_secs(5);
    while facility.exporter.exported_count() < CAPACITY + 1 {
        assert!(
            Instant::now() < deadline,
            "the released sink did not receive every queued record: {}/{}",
            facility.exporter.exported_count(),
            CAPACITY + 1
        );
        std::thread::sleep(Duration::from_millis(5));
    }

    let counters = facility.diagnostics.counters();
    assert_eq!(
        counters.beta_queued,
        (CAPACITY + 1) as u64,
        "admission boundary: {counters:?}"
    );
    assert_eq!(counters.beta_rejected, rejected as u64, "loss is counted");
    assert_eq!(
        counters.exported,
        (CAPACITY + 1) as u64,
        "every admitted record was delivered: {counters:?}"
    );
    assert_eq!(counters.export_failed, 0, "no export failed");
    assert_eq!(
        counters.beta_consumed,
        (CAPACITY + 1) as u64,
        "the channel was fully consumed: {counters:?}"
    );
    assert_eq!(counters.undelivered_now(), 0, "{counters:?}");
}
