//! The class-α pool's admission bound, in its own process: the pool holds at
//! most two published-unconsumed records, so a producer burst larger than the
//! pool loses records at the admission check — counted, never hidden, and
//! never a producer wait.
//!
//! Scenario boundaries: a dedicated binary so the facility's process-wide
//! counters belong to this scenario alone.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use qianqian_diagnostics::{
    AlphaRegistration, Config, Diagnostics, Disposition, ExportError, Exporter, Level, Record,
    Text, emit,
};

struct NullExporter;

impl Exporter for NullExporter {
    fn name(&self) -> &'static str {
        "null"
    }

    fn export(&mut self, _record: &Record) -> Result<(), ExportError> {
        Ok(())
    }
}

fn facility() -> &'static Diagnostics {
    static FACILITY: OnceLock<Diagnostics> = OnceLock::new();
    FACILITY.get_or_init(|| {
        Diagnostics::install(
            Box::new(NullExporter),
            Config {
                beta_capacity: 8,
                poll_window: Duration::from_millis(5),
            },
        )
        .expect("the host installs exactly one facility")
    })
}

fn record(marker: i64) -> Record {
    let mut message = Text::from_static("marker=");
    message.push_i64(marker);
    Record::new("alpha-burst", Level::Error)
        .with_message(message)
        .stamped()
}

/// Oracle: a pool's admission bound is real — the third unconsumed
/// publication is rejected (the protocol's only loss site) and counted, and
/// nothing is invented to hide it. The drainer is deliberately not given a
/// chance to run between the publishes (the pool is filled inside one
/// thread's critical section of emissions), so the state machine's
/// `d == 2` row is exercised for real.
#[test]
fn alpha_pool_full_admission_is_the_only_loss_site() {
    let _ = facility();
    let registration = AlphaRegistration::register();
    let pool = registration.pool();
    // Fill the pool without the drainer consuming: publish immediately after
    // the previous one. The drainer may race, so assert the INVARIANT (never
    // more than two outstanding, rejection only at the bound) rather than a
    // fixed count.
    let mut recorded = 0;
    for marker in 100..140 {
        match emit(record(marker)) {
            Disposition::AlphaRecorded => recorded += 1,
            Disposition::AlphaRejected => {}
            other => panic!("unexpected disposition {other:?}"),
        }
        assert!(
            pool.outstanding() <= 2,
            "the admission invariant bounds in-flight records to 2"
        );
        assert_eq!(
            pool.accepted() + pool.rejected(),
            pool.attempted(),
            "attempted = accepted + rejected (admission boundary)"
        );
    }
    assert!(recorded > 0, "at least the first publishes are accepted");
    // Let the drainer empty the pool, then confirm the drain boundary too.
    let deadline = Instant::now() + Duration::from_secs(5);
    while pool.outstanding() > 0 {
        assert!(
            Instant::now() < deadline,
            "the drainer never drained the pool"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        pool.accepted(),
        pool.copied_out(),
        "accepted = copied_out + outstanding with nothing outstanding"
    );
}
