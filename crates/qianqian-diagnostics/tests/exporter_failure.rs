//! Exporter-failure isolation: a sink that fails disables itself for the rest
//! of the run, the records it does not deliver are COUNTED, exactly one
//! self-diagnostic is recorded, and the producer path is untouched — no
//! panic, no block, no changed disposition. This is the drainer-side half of
//! the "logging failure cannot affect the operation" obligation.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use qianqian_diagnostics::{
    Config, Diagnostics, Disposition, ExportError, Exporter, Level, Record, Text, emit,
};

struct FailingExporter {
    attempts: Arc<AtomicUsize>,
    delivered: Arc<AtomicUsize>,
}

impl Exporter for FailingExporter {
    fn name(&self) -> &'static str {
        "failing"
    }

    fn export(&mut self, _record: &Record) -> Result<(), ExportError> {
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst);
        if attempt == 0 {
            self.delivered.fetch_add(1, Ordering::SeqCst);
            return Ok(());
        }
        Err(ExportError {
            message: "synthetic sink failure",
        })
    }
}

struct Facility {
    diagnostics: Diagnostics,
    attempts: Arc<AtomicUsize>,
    delivered: Arc<AtomicUsize>,
}

fn facility() -> &'static Facility {
    static FACILITY: OnceLock<Facility> = OnceLock::new();
    FACILITY.get_or_init(|| {
        let attempts = Arc::new(AtomicUsize::new(0));
        let delivered = Arc::new(AtomicUsize::new(0));
        let diagnostics = Diagnostics::install(
            Box::new(FailingExporter {
                attempts: Arc::clone(&attempts),
                delivered: Arc::clone(&delivered),
            }),
            Config {
                beta_capacity: 8,
                poll_window: Duration::from_millis(5),
            },
        )
        .expect("the host installs exactly one facility");
        Facility {
            diagnostics,
            attempts,
            delivered,
        }
    })
}

fn record(marker: i64) -> Record {
    let mut message = Text::from_static("failure=");
    message.push_i64(marker);
    Record::new("exporter-failure", Level::Error)
        .with_message(message)
        .stamped()
}

/// Oracle: the first export succeeds, the second fails and disables the
/// exporter, later records count as lost without further attempts, exactly
/// one self-diagnostic is recorded, and the emitting side sees only ordinary
/// dispositions.
#[test]
fn a_failing_exporter_is_disabled_once_and_its_losses_are_counted() {
    let facility = facility();
    let mut dispositions = Vec::new();
    for marker in 1..=6 {
        dispositions.push(emit(record(marker)));
    }
    assert!(
        dispositions
            .iter()
            .all(|disposition| *disposition == Disposition::BetaQueued),
        "the producer path never learns about a sink failure: {dispositions:?}"
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let counters = facility.diagnostics.counters();
        if counters.exported == 1 && counters.export_failed >= 1 && counters.self_diagnostics == 1 {
            // Wait for the rest of the batch to be accounted as lost.
            if counters.exported + counters.export_failed == 6 {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "the failure was never accounted: {:?}",
            facility.diagnostics.counters()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let counters = facility.diagnostics.counters();
    assert_eq!(counters.exported, 1, "only the first record was delivered");
    assert_eq!(
        counters.copied_out + counters.beta_consumed,
        counters.exported + counters.export_failed,
        "drain boundary: {counters:?}"
    );
    assert_eq!(
        counters.export_failed, 5,
        "every later record counts as an export loss: {counters:?}"
    );
    assert_eq!(
        counters.self_diagnostics, 1,
        "the disable is reported exactly once: {counters:?}"
    );
    assert_eq!(
        facility.attempts.load(Ordering::SeqCst),
        2,
        "the disabled exporter is never called again"
    );
    assert_eq!(facility.delivered.load(Ordering::SeqCst), 1);
    // The producer side is unaffected and still admits records.
    assert_eq!(emit(record(7)), Disposition::BetaQueued);
}
