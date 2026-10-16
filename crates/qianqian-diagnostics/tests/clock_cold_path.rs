//! The process clock is a COLD-path setting (installation, registration); the
//! producer path only READS it. A fresh process witnesses the producer half
//! directly and deterministically: stamping records twice, with time in
//! between and no installation and no registration, must keep reporting "no
//! timestamp" (`ts_micros = 0`). If a stamp started the clock instead, the
//! second one would carry the elapsed time — and the producer path would have
//! taken `OnceLock` initialization synchronization, on the very threads the
//! class-α contract says must never wait on one.
//!
//! This oracle needs its own process (a facility installed by any other test
//! would start the clock first), so it is an integration test binary.

use std::time::Duration;

use qianqian_diagnostics::{
    AlphaRegistration, Config, Diagnostics, ExportError, Exporter, Level, Record,
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

fn stamp(site: &'static str) -> u64 {
    Record::new(site, Level::Info).stamped().ts_micros
}

#[test]
fn stamping_never_starts_the_process_clock_and_the_cold_paths_do() {
    // Producer half: nothing has installed and nothing has registered, so no
    // clock exists — and stamping does not create one.
    let first = stamp("clock-probe");
    assert_eq!(first, 0, "no cold-path start yet: no timestamp exists");
    std::thread::sleep(Duration::from_millis(5));
    let second = stamp("clock-probe");
    assert_eq!(
        second, 0,
        "the second stamp carries elapsed time: a stamp STARTED the clock \
         (the producer path must only read it)"
    );

    // Cold path 1, registration: a class-α thread stamps real times with no
    // facility installed in this process at all.
    let registered = std::thread::spawn(|| {
        let _registration = AlphaRegistration::register();
        std::thread::sleep(Duration::from_millis(5));
        stamp("clock-probe")
    })
    .join()
    .expect("the registering thread");
    assert!(
        registered > 0,
        "registration is a cold-path clock start: {registered}"
    );

    // Cold path 2, installation.
    let diagnostics = Diagnostics::install(Box::new(NullExporter), Config::default())
        .expect("this process owns exactly one facility");
    std::thread::sleep(Duration::from_millis(5));
    let installed = stamp("clock-probe");
    assert!(
        installed > 0,
        "installation is a cold-path clock start: {installed}"
    );
    let report = diagnostics.shutdown(Duration::from_secs(5));
    assert!(report.drained, "an idle drainer exits inside the deadline");
}
