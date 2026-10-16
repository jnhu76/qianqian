//! The shared diagnostic-outlet fixture for this crate's white-box tests.
//!
//! The outlet is a process-global: exactly one facility per test binary, owned
//! by the first test that asks for it. Its exporter COLLECTS instead of
//! writing to stderr, so tests observe what the drainer delivers — the honest
//! observation boundary. (A producer can no longer reach into its own pool:
//! the pool is private to the registration and its consumer is the facility's
//! single drainer, by design.)

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use qianqian_diagnostics::{Config, Diagnostics, ExportError, Exporter, Record};

/// The consumer-facing typed fields the DG-3 test observes:
/// `(status, native_code)` pairs, in delivery order.
pub(crate) type TypedFields = Arc<Mutex<Vec<(Option<i32>, Option<i32>)>>>;

/// One delivered record, as one line: `site|level|message`.
fn render(record: &Record) -> String {
    let mut line = format!("{}|{}|", record.site, record.level.tag());
    line.push_str(&record.message.render());
    line
}

struct CollectingExporter {
    lines: Arc<Mutex<Vec<String>>>,
    typed: TypedFields,
}

impl Exporter for CollectingExporter {
    fn name(&self) -> &'static str {
        "collecting"
    }

    fn export(&mut self, record: &Record) -> Result<(), ExportError> {
        self.lines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(render(record));
        self.typed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((record.status, record.native_code));
        Ok(())
    }
}

pub(crate) struct Outlet {
    pub(crate) diagnostics: Diagnostics,
    /// Delivered lines (`site|level|message`), in delivery order.
    pub(crate) lines: Arc<Mutex<Vec<String>>>,
    /// Delivered typed fields, in delivery order.
    pub(crate) typed: TypedFields,
}

/// The test binary's one facility.
pub(crate) fn facility() -> &'static Outlet {
    static FACILITY: OnceLock<Outlet> = OnceLock::new();
    FACILITY.get_or_init(|| {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let typed: TypedFields = Arc::new(Mutex::new(Vec::new()));
        let diagnostics = Diagnostics::install(
            Box::new(CollectingExporter {
                lines: Arc::clone(&lines),
                typed: Arc::clone(&typed),
            }),
            Config {
                beta_capacity: 64,
                poll_window: Duration::from_millis(5),
            },
        )
        .expect("the test binary installs exactly one facility");
        Outlet {
            diagnostics,
            lines,
            typed,
        }
    })
}

impl Outlet {
    /// The lines delivered so far.
    pub(crate) fn delivered_lines(&self) -> Vec<String> {
        self.lines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Wait (bounded) until a delivered line satisfies `matches` and return it.
    /// The wait is the drainer's poll cadence, not a delivery deadline: the
    /// poll contract is a conditional frequency, so a bounded wait is the test
    /// harness's, never the facility's.
    pub(crate) fn wait_for_line(&self, matches: impl Fn(&str) -> bool, what: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(line) = self
                .delivered_lines()
                .into_iter()
                .find(|line| matches(line))
            {
                return line;
            }
            assert!(
                Instant::now() < deadline,
                "the drainer never delivered {what}: {:?}",
                self.delivered_lines()
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
