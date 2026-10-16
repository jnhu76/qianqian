//! Class-α routing: a REGISTERED critical thread publishes into its own
//! ownership-handoff pool (atomics only) and the drainer discovers it; an
//! unregistered thread's records go to the bounded class-β channel.
//!
//! Scenario boundaries: this binary installs the process's one facility with
//! a plain collecting sink and asserts delivery; the blocked-sink residual is
//! a different binary (`beta_queue_full`).

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use qianqian_diagnostics::{
    AlphaRegistration, Config, Diagnostics, Disposition, ExportError, Exporter, Level, Record,
    Text, emit,
};

#[derive(Clone, Default)]
struct Collected {
    lines: Arc<Mutex<Vec<String>>>,
}

impl Collected {
    fn lines(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }
}

struct CollectingExporter {
    sink: Collected,
}

impl Exporter for CollectingExporter {
    fn name(&self) -> &'static str {
        "collecting"
    }

    fn export(&mut self, record: &Record) -> Result<(), ExportError> {
        let mut line = format!("{}|", record.site);
        line.push_str(&record.message.render());
        if let Some(status) = record.status {
            line.push_str(&format!("|status={status}"));
        }
        if let Some(native) = record.native_code {
            line.push_str(&format!("|native={native}"));
        }
        self.sink.lines.lock().unwrap().push(line);
        Ok(())
    }
}

struct Facility {
    diagnostics: Diagnostics,
    sink: Collected,
}

fn facility() -> &'static Facility {
    static FACILITY: OnceLock<Facility> = OnceLock::new();
    FACILITY.get_or_init(|| {
        let sink = Collected::default();
        let diagnostics = Diagnostics::install(
            Box::new(CollectingExporter { sink: sink.clone() }),
            Config {
                beta_capacity: 8,
                poll_window: Duration::from_millis(5),
            },
        )
        .expect("the host installs exactly one facility");
        Facility { diagnostics, sink }
    })
}

fn record(site: &'static str, marker: i64) -> Record {
    let mut message = Text::from_static("marker=");
    message.push_i64(marker);
    Record::new(site, Level::Error)
        .with_message(message)
        .with_status(108)
        .with_native_code(-1094995529)
        .stamped()
}

/// Oracle: a registered class-α producer's records are recorded in its own
/// pool, the drainer discovers them by its poll cadence, and they arrive with
/// their structured fields intact (site, status, native code).
///
/// The pool holds at most two unconsumed records by design, so the trace
/// waits for each record's delivery before publishing the next: a burst
/// larger than the pool is LOSS (counted, see the admission test below), not
/// a delivery guarantee.
#[test]
fn alpha_pool_records_reach_the_drainer_with_their_fields() {
    let facility = facility();
    let sink_probe = facility.sink.clone();

    std::thread::spawn(move || {
        let _registration = AlphaRegistration::register();
        for marker in 1..=3 {
            assert_eq!(
                emit(record("alpha-site", marker)),
                Disposition::AlphaRecorded,
                "an empty pool accepts the publish (no drainer backlog here)"
            );
            // Wait for this record's delivery before the next publish, so the
            // pool is provably drained and the trace stays deterministic.
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let delivered = sink_probe
                    .lines()
                    .iter()
                    .any(|line| line.starts_with(&format!("alpha-site|marker={marker}|")));
                if delivered {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "the drainer never delivered marker={marker}; sink={:?} counters={:?}",
                    sink_probe.lines(),
                    Diagnostics::global_counters()
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    })
    .join()
    .expect("the α producer thread");

    // The test's own (unregistered) thread is a class-β producer.
    assert_eq!(
        emit(record("beta-probe", 7)),
        Disposition::BetaQueued,
        "an unregistered thread routes to the class-β channel"
    );

    // The β record rides the channel: wait for the drainer's next batch.
    let beta_deadline = Instant::now() + Duration::from_secs(5);
    while !facility
        .sink
        .lines()
        .iter()
        .any(|line| line.starts_with("beta-probe|marker=7"))
    {
        assert!(
            Instant::now() < beta_deadline,
            "the class-β record never arrived: {:?}",
            facility.sink.lines()
        );
        std::thread::sleep(Duration::from_millis(2));
    }

    let lines = facility.sink.lines();
    for marker in 1..=3 {
        let line = lines
            .iter()
            .find(|line| line.starts_with(&format!("alpha-site|marker={marker}")))
            .unwrap_or_else(|| panic!("alpha-site marker {marker} missing from {lines:?}"));
        assert!(
            line.contains("|status=108|native=-1094995529"),
            "the structured fields survive to the sink: {line}"
        );
    }
    assert!(
        lines.iter().any(|line| line.starts_with("beta-probe|marker=")),
        "the class-β records arrive too: {lines:?}"
    );

    let counters = facility.diagnostics.counters();
    assert_eq!(counters.alpha_rejected, 0, "no α record was dropped here");
    assert!(counters.copied_out >= 3, "copied out: {counters:?}");
    assert!(counters.outstanding <= 2, "pool bound: {counters:?}");
    // Drain-boundary identity at quiescence: every record that left a pool
    // or was pulled off the channel was delivered.
    assert_eq!(
        counters.copied_out + counters.beta_consumed,
        counters.exported + counters.export_failed,
        "drain boundary: {counters:?}"
    );
    assert_eq!(counters.export_failed, 0, "{counters:?}");
}
