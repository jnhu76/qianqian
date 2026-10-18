//! A4/E-7 tier-1 writer-swap sink controls (error-system-migration §7.3),
//! #288 Windows runtime campaign. Child-process oracles: the parent
//! (PowerShell harness, `evidence/windows-runtime-288/harness/`) spawns this
//! test binary with stderr attached to a pipe it then closes (broken sink)
//! or never drains (blocked sink), and this module drives the REAL cone
//! fixture under that sink state.
//!
//! **These tests are inert without their environment**: `QIANQIAN_A4_MODE`
//! absent → the test skips (a plain `cargo test` run executes nothing here).
//! The campaign harness is the only caller that sets the protocol variables:
//!
//! ```text
//! QIANQIAN_A4_MODE        healthy | broken | blocked   (required to arm)
//! QIANQIAN_A4_FACILITY    1 → install the real diagnostics facility
//!                         (stderr exporter + drainer) before the drive
//! QIANQIAN_A4_HOOK        dg4b → install the DG-4b panic-hook posture
//!                         (the headless product posture: α threads publish
//!                         one bounded record, never a synchronous stderr
//!                         print); absent → the Rust default hook stays
//! QIANQIAN_A4_VOLUME_FAULT 1 → arm the #10 volume-apply seam with a
//!                         recoverable (non-device-loss) failure
//! ```
//!
//! **Sink premises are proven IN THE CHILD, never assumed from the parent**
//! (#290 lesson, independently checked on Windows): the broken mode opens
//! with a calibration write that must FAIL before the drive starts; the
//! blocked mode floods stderr first and must OBSERVE its own flooding write
//! blocking (counter freeze with the writer not finished). A refuted premise
//! aborts the row as `PREMISE_REFUTED` — printed, never a property result.
//!
//! **What each test asserts is the POST-SWAP contract** (diagnostic-write
//! failure must not change terminal class / exit path / wakeup ordering —
//! disposition-contract §6): settle within bound + the expected terminal.
//! The pre-swap lanes (worktree builds restoring the `b86a9ce^` writer
//! shapes) run the SAME assertions and are EXPECTED to go red — that red IS
//! the documented hazard reproduced (N-01 hang, #10/#9 misattribution),
//! which is what proves the harness can see the hazard it guards.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use qianqian_audio_api::ports::{DrainVerdict, OutputAbortClass};
use qianqian_playback::EpisodeTerminalOutcome;

use crate::cone_oracle_tests::{ConeFixture, SETTLE_BOUND};

/// Rows go to the REAL stdout descriptor (bypassing libtest's capture) so
/// the parent harness observes them regardless of the test verdict. Never
/// panics on a failing write: an evidence row must not fabricate a red.
fn row(line: &str) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn mode() -> Option<String> {
    std::env::var("QIANQIAN_A4_MODE")
        .ok()
        .filter(|m| m == "healthy" || m == "broken" || m == "blocked")
}

fn flag(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|v| !v.is_empty() && v != "0")
}

/// Prove the broken-sink premise: a stderr write must FAIL here, before the
/// drive, on this process's real stderr handle.
fn prove_broken_premise() -> Result<(), String> {
    let mut err = std::io::stderr();
    match err
        .write_all(b"[a4 premise probe]\n")
        .and_then(|()| err.flush())
    {
        Ok(()) => Err(
            "PREMISE_REFUTED: the calibration write to stderr SUCCEEDED — the sink is not \
             broken in this child; the parent's pipe close did not take effect"
                .to_owned(),
        ),
        Err(error) => {
            row(&format!("[a4 premise] broken-sink confirmed: {error}"));
            Ok(())
        }
    }
}

/// Prove the blocked-sink premise by flooding this process's own stderr
/// until a write blocks. Returns `(blocked, bytes_attempted)`. The flooding
/// writer is deliberately left parked: joining it is impossible (its write
/// never returns while the pipe stays full), and it dies with the process.
fn prove_blocked_premise() -> Result<usize, String> {
    const CHUNK: usize = 4096;
    const MAX_CHUNKS: usize = 512; // 2 MiB — far above any anonymous-pipe buffer
    let attempts = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicBool::new(false));
    let saw_error = Arc::new(AtomicBool::new(false));
    {
        let attempts = Arc::clone(&attempts);
        let finished = Arc::clone(&finished);
        let saw_error = Arc::clone(&saw_error);
        std::thread::Builder::new()
            .name("a4-stderr-flood".into())
            .spawn(move || {
                let mut err = std::io::stderr();
                let chunk = [b'x'; CHUNK];
                loop {
                    let n = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                    if n > MAX_CHUNKS {
                        break;
                    }
                    if err.write_all(&chunk).is_err() {
                        saw_error.store(true, Ordering::Release);
                        break;
                    }
                }
                finished.store(true, Ordering::Release);
            })
            .expect("flood writer spawns");
    }
    // Observe the flood freezing: two samples 400 ms apart with no progress
    // and the writer not finished is a blocked write (a 2 MiB in-memory
    // write completes in microseconds; only a full pipe can freeze it).
    std::thread::sleep(Duration::from_millis(400));
    let first = attempts.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(400));
    let second = attempts.load(Ordering::SeqCst);
    if saw_error.load(Ordering::Acquire) {
        return Err(
            "PREMISE_REFUTED: the flooding write FAILED instead of blocking — this child's \
             stderr is broken, not blocked; run the broken-mode row for this configuration"
                .to_owned(),
        );
    }
    if finished.load(Ordering::Acquire) || first != second {
        return Err(format!(
            "PREMISE_REFUTED: the flood completed all writes ({second} chunks, finished={}) — \
             the pipe never filled; no blocked sink exists in this child",
            finished.load(Ordering::Acquire)
        ));
    }
    row(&format!(
        "[a4 premise] blocked-sink confirmed: flooding write #{} of {MAX_CHUNKS} parked \
         (counter frozen across the 400 ms observation window)",
        second
    ));
    Ok(second * CHUNK)
}

/// The per-row setup shared by both drives: prove the sink premise, install
/// the optional facility (before any emitting thread starts — the
/// installation edge), install the optional DG-4b hook posture, return the
/// facility handle for the exit-boundary report.
fn arm_row() -> Result<Option<qianqian_diagnostics::Diagnostics>, String> {
    match mode().as_deref() {
        Some("broken") => prove_broken_premise()?,
        Some("blocked") => {
            prove_blocked_premise()?;
        }
        Some("healthy") => row("[a4 premise] healthy-sink row (no premise to prove)"),
        _ => unreachable!("caller checked mode"),
    }
    if flag("QIANQIAN_A4_HOOK") {
        // The DG-4b posture, exactly as the headless product installs it
        // (machine_input.rs: on an α thread, one bounded publication and NO
        // chained synchronous sink; otherwise chain to the previous hook).
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if qianqian_diagnostics::is_alpha_producer() {
                qianqian_diagnostics::note_critical_panic();
                return;
            }
            previous(info);
        }));
        row("[a4 hook] DG-4b posture installed (product shape)");
    }
    if flag("QIANQIAN_A4_FACILITY") {
        let diagnostics = qianqian_diagnostics::Diagnostics::install(
            Box::new(qianqian_diagnostics::StderrExporter::default()),
            qianqian_diagnostics::Config::default(),
        )
        .map_err(|e| format!("PREMISE_REFUTED: facility install refused: {e:?}"))?;
        row("[a4 facility] real diagnostics facility installed (stderr exporter)");
        return Ok(Some(diagnostics));
    }
    Ok(None)
}

/// Close the row: bounded shutdown of the facility (if any) with the row's
/// deadline, reporting the honest verdict; then the fixture release.
fn finish_row(
    diagnostics: Option<qianqian_diagnostics::Diagnostics>,
    mut fixture: ConeFixture,
    drive_tag: &str,
) {
    if let Some(diagnostics) = diagnostics {
        let deadline = Duration::from_millis(1000);
        let started = Instant::now();
        let report = diagnostics.shutdown(deadline);
        row(&format!(
            "[a4 {drive_tag}] facility shutdown({deadline:?}) took {:?}: drained={} \
             abandoned={} undelivered={} counters(copied_out={} exported={} export_failed={} \
             alpha_rejected={} beta_queued={} critical_panics={} self_diagnostics={})",
            started.elapsed(),
            report.drained,
            report.abandoned,
            report.undelivered_at_exit,
            report.counters.copied_out,
            report.counters.exported,
            report.counters.export_failed,
            report.counters.alpha_rejected,
            report.counters.beta_queued,
            report.counters.critical_panics,
            report.counters.self_diagnostics,
        ));
    }
    fixture.release_fixture();
}

/// The #8 drive: the injected cone (settle must occur, terminal Failed).
/// This asserts the POST-SWAP contract; pre-swap lanes are expected red.
#[test]
fn a4_child_cone_injected() {
    let Some(mode_name) = mode() else {
        row("[a4 skip] no QIANQIAN_A4_MODE — inert outside the campaign harness");
        return;
    };
    let diagnostics = match arm_row() {
        Ok(d) => d,
        Err(message) => {
            row(&format!("[a4 {mode_name}] {message}"));
            return;
        }
    };
    let mut fixture = ConeFixture::new();
    fixture.activate();
    if let Err(message) = fixture.await_injection() {
        row(&format!("[a4 {mode_name}] {message}"));
        finish_row(diagnostics, fixture, &mode_name);
        panic!("A4 CHILD PRECONDITION FAILED ({mode_name}): {message}");
    }
    let started = Instant::now();
    let settled = fixture.settled_within(SETTLE_BOUND);
    let settle_took = started.elapsed();
    let Some(terminal) = settled else {
        row(&format!(
            "[a4 {mode_name}] A4-POSTSWAP-CONTRACT RED: wait_terminal did not return within \
             {SETTLE_BOUND:?} — the diagnostic sink state changed the failure cone's progress \
             (this is the expected N-01-class signature on a PRE-SWAP lane)"
        ));
        finish_row(diagnostics, fixture, &mode_name);
        panic!("A4-POSTSWAP-CONTRACT RED ({mode_name}): the cone never settled");
    };
    row(&format!(
        "[a4 {mode_name}] settled in {settle_took:?}: terminal={terminal:?} \
         injected={} edge_cuts={} verdict_snapshot={}",
        fixture.witness.injected.load(Ordering::Acquire),
        fixture.witness.edge_cuts.load(Ordering::Acquire),
        fixture.witness.verdict_snapshot.load(Ordering::Acquire),
    ));
    assert_eq!(
        terminal,
        EpisodeTerminalOutcome::Failed,
        "A4-POSTSWAP-CONTRACT RED ({mode_name}): sink state changed the terminal class"
    );
    let quiet = fixture.dispose_quietly();
    assert!(
        quiet,
        "A4 ({mode_name}): settled episode must dispose quietly"
    );
    finish_row(diagnostics, fixture, &mode_name);
}

/// The #9/#10 drive: NO injection (the episode would play forever), a
/// routed volume change (optionally faulted at the seam), then an external
/// stop — the settle class that makes the pre-swap misattribution hazards
/// visible as a terminal-class divergence (healthy → `Stopped`; pre-swap
/// broken sink → `Failed{device}`).
#[test]
fn a4_child_cone_volume_stop() {
    let Some(mode_name) = mode() else {
        row("[a4 skip] no QIANQIAN_A4_MODE — inert outside the campaign harness");
        return;
    };
    let diagnostics = match arm_row() {
        Ok(d) => d,
        Err(message) => {
            row(&format!("[a4 {mode_name}] {message}"));
            return;
        }
    };
    if flag("QIANQIAN_A4_VOLUME_FAULT") {
        crate::wasapi::volume_apply_seam::set_override(Some(
            |_service: &windows::Win32::Media::Audio::IAudioStreamVolume,
             _levels: &[f32]|
             -> windows::core::Result<()> {
                // E_INVALIDARG: a legal-value refusal that is NOT
                // AUDCLNT_E_DEVICE_INVALIDATED, so D14.9 grades it
                // recoverable — exactly the #10 arm.
                Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x8007_0057u32 as i32,
                )))
            },
        ));
        row("[a4 seam] volume-apply fault armed (E_INVALIDARG, recoverable grade)");
    }
    let mut fixture = ConeFixture::with_inject_at(usize::MAX);
    fixture.activate();
    // The episode is live against the real endpoint. Give the open line (#9,
    // gate-gated) its moment, route ONE volume change (the loop-top apply
    // will attempt it next iteration — faulted or not per the seam), let the
    // steady loop run, then settle by external stop.
    std::thread::sleep(Duration::from_millis(300));
    // A level the establishment default (100, clamped) is NOT: routing the
    // default would be a no-op (routed == applied) and the apply arm would
    // never execute. 40 forces exactly one loop-top apply attempt.
    fixture.handle.request_output_level(40);
    std::thread::sleep(Duration::from_millis(500));
    let started = Instant::now();
    fixture.handle.request_stop();
    let Some(terminal) = fixture.settled_within(SETTLE_BOUND) else {
        row(&format!(
            "[a4 {mode_name}] A4-POSTSWAP-CONTRACT RED: the stopped episode never settled \
             within {SETTLE_BOUND:?}"
        ));
        crate::wasapi::volume_apply_seam::set_override(None);
        finish_row(diagnostics, fixture, &mode_name);
        panic!("A4-POSTSWAP-CONTRACT RED ({mode_name}): stop never settled");
    };
    let stopped_in = started.elapsed();
    crate::wasapi::volume_apply_seam::set_override(None);
    row(&format!(
        "[a4 {mode_name}] stopped-episode settled in {stopped_in:?}: terminal={terminal:?} \
         (healthy expectation: Stopped; a Failed here is the pre-swap #9/#10 \
         misattribution signature)"
    ));
    assert_eq!(
        terminal,
        EpisodeTerminalOutcome::Stopped,
        "A4-POSTSWAP-CONTRACT RED ({mode_name}): sink state or the volume diagnostic \
         changed the terminal class of an externally stopped episode"
    );
    let quiet = fixture.dispose_quietly();
    assert!(
        quiet,
        "A4 ({mode_name}): stopped episode must dispose quietly"
    );
    finish_row(diagnostics, fixture, &mode_name);
}

/// The A6-P Device-class row (carrier B, DG-2): a device-loss-class abort
/// reason constructed at the volume seam (`AUDCLNT_E_DEVICE_INVALIDATED` —
/// the one HRESULT the mechanism grades as device loss from the volume
/// call) and driven through the REAL cone into the REAL session, asserting
/// the full carrier B chain: the reason rides the verdict (relay witness),
/// the resolver settles `Failed` WITHOUT reading it (discriminant-only),
/// and the presentation path enriches `failure_diagnostic` with the
/// Device-class message (completion.rs observe_snapshot enrichment).
///
/// Deterministic pipeline witness (fault injected at the seam), honestly
/// separated from a real device-initiated abort (A6-E, own row).
#[test]
fn a6_child_device_reason_enrichment() {
    let Some(mode_name) = mode() else {
        row("[a4 skip] no QIANQIAN_A4_MODE — inert outside the campaign harness");
        return;
    };
    let diagnostics = match arm_row() {
        Ok(d) => d,
        Err(message) => {
            row(&format!("[a6 {mode_name}] {message}"));
            return;
        }
    };
    crate::wasapi::volume_apply_seam::set_override(Some(
        |_service: &windows::Win32::Media::Audio::IAudioStreamVolume,
         _levels: &[f32]|
         -> windows::core::Result<()> {
            Err(windows::core::Error::from_hresult(
                windows::Win32::Media::Audio::AUDCLNT_E_DEVICE_INVALIDATED,
            ))
        },
    ));
    row("[a6 seam] volume-apply fault armed (AUDCLNT_E_DEVICE_INVALIDATED — device-loss grade)");
    let mut fixture = ConeFixture::with_inject_at(usize::MAX);
    fixture.activate();
    std::thread::sleep(Duration::from_millis(300));
    fixture.handle.request_output_level(40);
    // The loop-top apply takes the device-loss branch on its next iteration:
    // break abort_msg(...) -> Aborted{Device, message} -> the real cone
    // (stop, complete-with-reason) -> settle.
    let Some(terminal) = fixture.settled_within(SETTLE_BOUND) else {
        crate::wasapi::volume_apply_seam::set_override(None);
        row(&format!(
            "[a6 {mode_name}] RED: no settle within {SETTLE_BOUND:?}"
        ));
        finish_row(diagnostics, fixture, &mode_name);
        panic!("A6 RED ({mode_name}): the device-loss abort never settled");
    };
    crate::wasapi::volume_apply_seam::set_override(None);
    let observation = fixture.handle.observe();
    let relayed = fixture
        .witness
        .relayed_verdict
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    row(&format!(
        "[a6 {mode_name}] terminal={terminal:?} failure_diagnostic={:?} relayed_verdict={relayed:?}",
        observation.failure_diagnostic
    ));
    assert_eq!(
        terminal,
        EpisodeTerminalOutcome::Failed,
        "A6 RED ({mode_name}): a device-loss abort must settle Failed"
    );
    let verdict = relayed.as_ref().expect("the relay observed the verdict");
    let DrainVerdict::Aborted {
        reason: Some(reason),
    } = verdict
    else {
        panic!("A6 RED ({mode_name}): the verdict carried no reason (carrier B lost it)");
    };
    assert_eq!(
        reason.class,
        OutputAbortClass::Device,
        "A6 RED ({mode_name}): the reason class must be Device for a device-loss abort"
    );
    assert!(
        reason.message.contains("device invalidated"),
        "A6 RED ({mode_name}): the reason message must be the mechanism's own: {}",
        reason.message
    );
    // The presentation enrichment (completion.rs:1126-1136): a Failed
    // terminal + Device-class reason => failure_diagnostic carries the
    // message. This is the display surface; the resolver never read it.
    let diagnostic = observation.failure_diagnostic.as_deref().unwrap_or("");
    assert!(
        diagnostic.contains("device invalidated"),
        "A6 RED ({mode_name}): the Device-class reason did not reach the presentation \
         (failure_diagnostic={diagnostic:?})"
    );
    let quiet = fixture.dispose_quietly();
    assert!(
        quiet,
        "A6 ({mode_name}): settled episode must dispose quietly"
    );
    finish_row(diagnostics, fixture, &mode_name);
}
