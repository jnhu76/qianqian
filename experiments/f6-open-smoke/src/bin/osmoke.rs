//! F6 Open physical smoke (Stage C evidence; ADR-PBK-002 D14.6).
//!
//! Drives the PRODUCTION [`ReferencePlayerApp`] with the REAL provider
//! wiring (SongCore `probe_media` + WASAPI output) over real media on
//! the real Windows host, and checks the frozen replacement semantics:
//!
//! ```text
//! O1  valid A → Open valid B        replaced: A settles Stopped, B established
//! O2  valid A → Open INVALID        REFUSED; A keeps consuming (position
//!                                   publication keeps advancing); a later
//!                                   valid Open still commits
//! O3  paused A → Open B             D14.7 stop-from-paused ⇒ A Stopped; B
//!                                   established and consuming
//! O4  seeked A → Open B             D14.5 cut, then replacement; A settles
//!                                   Stopped; B established and consuming
//! O5  repeated A→B→C→D replacement  every old Stopped + Discharged, every
//!                                   new established; quit reports Discharged
//! ```
//!
//! The audible continuity property stays a human-ear item (F5/F6
//! precedent, recorded UNAVAILABLE in the results); the mechanical
//! witness is the render leg's own position publication (D14.8):
//! a live episode's published sample advances; a stopped one stops.
//!
//! One process = one scenario = one JSON verdict on stdout; every
//! check also emits a `verdict=` stderr line (the committed evidence
//! trail). Exit 0 iff GREEN. A global watchdog exits 42 on a wedged
//! scenario (no invented timeouts INSIDE the player — the watchdog
//! only bounds the EVIDENCE run, mirroring the S-PROBE posture).

use std::path::{Path, PathBuf};

use std::time::{Duration, Instant};

use qianqian_app::QianqianApp;
use qianqian_headless::player::{
    EpisodeStart, OpenOutcome, QuitReport, ReferencePlayerApp, StartAttempt,
};
use qianqian_playback::{EpisodeTerminalOutcome, PlaybackSessionHandle};
use serde::Serialize;

/// The scenario watchdog bound: generous over the longest scenario;
/// a hit is a RED-class harness failure, never a player timeout.
const WATCHDOG: Duration = Duration::from_secs(120);

/// One position-liveness window: publication cadence is the render
/// leg's own; 300 ms x N windows at 44.1 kHz leaves no ambiguity.
const WINDOW: Duration = Duration::from_millis(300);

#[derive(Serialize)]
struct Verdict {
    scenario: &'static str,
    verdict: &'static str, // GREEN | RED
    reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence: Option<serde_json::Value>,
}

fn main() {
    // Global evidence watchdog (see module docs).
    std::thread::spawn(|| {
        std::thread::sleep(WATCHDOG);
        eprintln!("verdict=RED reason: watchdog fired after {WATCHDOG:?} (wedged scenario)");
        std::process::exit(42);
    });

    let mut scenario = String::new();
    let mut main_file = PathBuf::new();
    let mut candidates: Vec<PathBuf> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scenario" => scenario = args.next().unwrap_or_default(),
            "--main" => main_file = PathBuf::from(args.next().unwrap_or_default()),
            "--cand" => candidates.push(PathBuf::from(args.next().unwrap_or_default())),
            other => {
                eprintln!("verdict=RED reason: unknown argument {other:?}");
                std::process::exit(2);
            }
        }
    }
    let scenario: &'static str = Box::leak(scenario.into_boxed_str());

    let outcome = match scenario {
        "O1" => o1(&main_file, &candidates),
        "O2" => o2(&main_file, &candidates),
        "O3" => o3(&main_file, &candidates),
        "O4" => o4(&main_file, &candidates),
        "O5" => o5(&main_file, &candidates),
        other => {
            eprintln!("verdict=RED reason: unknown scenario {other:?}");
            std::process::exit(2);
        }
    };

    let (verdict, reasons, evidence) = outcome;
    for reason in &reasons {
        eprintln!("verdict={verdict} reason: {reason}");
    }
    let v = Verdict {
        scenario,
        verdict,
        reasons,
        evidence,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&v).expect("verdict json")
    );
    if verdict == "GREEN" {
        std::process::exit(0);
    }
    std::process::exit(1);
}

type Outcome = (&'static str, Vec<String>, Option<serde_json::Value>);

/// The real provider wiring, mirroring the headless binary's
/// RealEpisodeSource (the binary wiring is main.rs-private; the
/// duplication is the evidence harness's, never the product's).
struct RealSource;

impl EpisodeStart for RealSource {
    fn probe(&self, candidate: &Path) -> Result<(), String> {
        // Differential switch: QN_OSMOKE_SKIP_PROBE=1 skips the real
        // probe query to isolate a suspected probe/episode interaction.
        if std::env::var("QN_OSMOKE_SKIP_PROBE").as_deref() == Ok("1") {
            eprintln!("opened: probe SKIPPED by QN_OSMOKE_SKIP_PROBE");
            return Ok(());
        }
        qianqian_decode_songcore::probe_media(candidate)
            .map(|facts| {
                eprintln!(
                    "opened: probe facts {:?} Hz x {} mask {:#x} duration {:?}",
                    facts.format.sample_rate,
                    facts.format.channels,
                    facts.format.channel_mask,
                    facts.duration
                );
            })
            .map_err(|e| e.message)
    }

    fn start(&self, source: &Path) -> StartAttempt {
        let mut runtime = QianqianApp::new();
        let handle = PlaybackSessionHandle::new();
        if let Err(e) =
            runtime.register_component(qianqian_decode_songcore::songcore_decode_plugin())
        {
            return StartAttempt {
                runtime,
                handle,
                refused: Some(format!("decode plugin registration failed: {e:?}")),
            };
        }
        if let Err(e) = runtime.register_component(qianqian_output_wasapi::wasapi_output_plugin()) {
            return StartAttempt {
                runtime,
                handle,
                refused: Some(format!("output plugin registration failed: {e:?}")),
            };
        }
        if let Err(e) = runtime.register_component(qianqian_playback::playback_session_spec(
            source.to_path_buf(),
            handle.clone(),
        )) {
            return StartAttempt {
                runtime,
                handle,
                refused: Some(format!("session registration failed: {e:?}")),
            };
        }
        let desired = vec![
            entry("decode", "songcore_decode_plugin"),
            entry("output", "wasapi_output_plugin"),
            entry("session", "playback_session"),
        ];
        if let Err(errors) = runtime.revise_desired(desired) {
            return StartAttempt {
                runtime,
                handle,
                refused: Some(format!("{errors}")),
            };
        }
        StartAttempt {
            runtime,
            handle,
            refused: None,
        }
    }
}

fn entry(id: &str, component: &'static str) -> qianqian_composition::DesiredEntry {
    qianqian_composition::DesiredEntry::enabled(
        id,
        component,
        qianqian_composition::Revision::new(1),
    )
}

fn dump_observation(handle: &PlaybackSessionHandle, label: &str) {
    let o = handle.observe();
    eprintln!(
        "obs={label} terminal={:?} failure={:?} stop={} pause={} engagement={:?} position={:?} fmt={:?}",
        o.terminal_outcome,
        o.failure_diagnostic,
        o.stop_requested,
        o.pause_requested,
        o.pause_engagement,
        o.position,
        o.source_format,
    );
}

fn expect_liveness(
    handle: &PlaybackSessionHandle,
    windows: usize,
    label: &str,
    reasons: &mut Vec<String>,
) -> bool {
    // Wait for the FIRST publication: the render leg's startup (device
    // open to first consumed-frame publication) is legitimately
    // sub-second but has no contractual bound, so the harness bounds
    // its own WAIT — never the mechanism. Without this, the oracle
    // fires inside the startup gap (measured ~0.5 s on this host) and
    // reports healthy episodes RED.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last = handle.observe().position;
    while last.is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        last = handle.observe().position;
    }
    if last.is_none() {
        dump_observation(handle, label);
        reasons.push(format!(
            "{label}: no position publication within the 5 s startup wait"
        ));
        return false;
    }
    let mut advances = 0usize;
    for _ in 0..windows {
        std::thread::sleep(WINDOW);
        let current = handle.observe().position;
        match (last, current) {
            (Some(l), Some(c)) if c > l => advances += 1,
            _ => {}
        }
        last = current;
    }
    let live = advances * 2 > total_windows(windows); // strictly more than half
    if live {
        eprintln!("verdict=GREEN reason: {label} position advancing ({advances}/{windows})");
    } else {
        dump_observation(handle, label);
        reasons.push(format!(
            "{label}: position publication not advancing ({advances}/{windows})"
        ));
    }
    live
}

fn total_windows(windows: usize) -> usize {
    windows
}

fn expect_established(
    player: &ReferencePlayerApp<RealSource>,
    label: &str,
    reasons: &mut Vec<String>,
) -> bool {
    let Some(handle) = player.active_handle() else {
        reasons.push(format!("{label}: no active episode after Opened"));
        return false;
    };
    let observation = handle.observe();
    let established = observation.source_format.is_some() && observation.activation_error.is_none();
    if established {
        let format = observation.source_format.expect("checked");
        eprintln!(
            "verdict=GREEN reason: {label} established {} Hz x {} mask {:#x}",
            format.sample_rate, format.channels, format.channel_mask
        );
    } else {
        reasons.push(format!(
            "{label}: activation evidence not established (format {:?}, error {:?})",
            observation.source_format, observation.activation_error
        ));
    }
    established
}

fn quit_report(player: &mut ReferencePlayerApp<RealSource>) -> QuitReport {
    let report = player.quit();
    if let (Some(terminal), Some(_diagnostic)) = (report.terminal, &report.diagnostic) {
        eprintln!("verdict=GREEN reason: quit terminal {terminal:?} (diagnostic attached)");
    } else if let Some(terminal) = report.terminal {
        eprintln!("verdict=GREEN reason: quit terminal {terminal:?}");
    }
    if let Some(snapshot) = &report.snapshot {
        eprintln!(
            "verdict=GREEN reason: quit disposal quiet={}",
            snapshot.quiet
        );
    }
    report
}

/// O1 — valid A → Open valid B.
fn o1(main: &Path, candidates: &[PathBuf]) -> Outcome {
    let mut reasons = Vec::new();
    let candidate = candidates[0].clone();
    let mut player = ReferencePlayerApp::new(RealSource);
    if !matches!(player.open(main), OpenOutcome::Opened) {
        return (
            "RED",
            vec!["O1: the first Open did not establish".into()],
            None,
        );
    }
    let a = player.active_handle().expect("committed").clone();
    // QN_OSMOKE_TRACE=1: long observation trace instead of the oracle.
    if std::env::var("QN_OSMOKE_TRACE").as_deref() == Ok("1") {
        for i in 0..20 {
            std::thread::sleep(Duration::from_millis(250));
            let o = a.observe();
            eprintln!(
                "trace[{i}] t={:?} pos={:?} term={:?} fail={:?} eng={:?}",
                i * 250,
                o.position,
                o.terminal_outcome,
                o.failure_diagnostic,
                o.pause_engagement
            );
        }
    }
    if !expect_liveness(&a, 2, "O1 pre-replacement A", &mut reasons) {
        return ("RED", reasons, None);
    }

    let started = Instant::now();
    let outcome = player.open(&candidate);
    let elapsed = started.elapsed();
    if !matches!(outcome, OpenOutcome::Opened) {
        reasons.push(format!("O1: replacement of live A returned {outcome:?}"));
        return ("RED", reasons, None);
    }
    eprintln!("verdict=GREEN reason: O1 replacement committed in {elapsed:?}");
    let a_final = a.observe();
    let b_established = expect_established(&player, "O1 new episode B", &mut reasons);
    let b_live = b_established
        && player
            .active_handle()
            .is_some_and(|h| expect_liveness(h, 2, "O1 new episode B consuming", &mut reasons));
    let report = quit_report(&mut player);
    let ok = b_established
        && b_live
        && a_final.terminal_outcome == Some(EpisodeTerminalOutcome::Stopped)
        && report.disposal == Some(qianqian_composition::DisposeVerdict::Discharged);
    if a_final.terminal_outcome != Some(EpisodeTerminalOutcome::Stopped) {
        reasons.push(format!(
            "O1: old episode settled {:?}",
            a_final.terminal_outcome
        ));
    }
    if report.disposal != Some(qianqian_composition::DisposeVerdict::Discharged) {
        reasons.push(format!("O1: quit disposal {:?}", report.disposal));
    }
    (
        if ok { "GREEN" } else { "RED" },
        reasons,
        Some(serde_json::json!({
            "old_terminal": format!("{:?}", a_final.terminal_outcome),
            "old_stop_requested": a_final.stop_requested,
            "replacement_elapsed_ms": elapsed.as_millis() as u64,
            "quit_disposal": format!("{:?}", report.disposal),
        })),
    )
}

/// O2 — valid A → Open INVALID candidate → REFUSED, A continues; a
/// later valid Open still commits.
fn o2(main: &Path, candidates: &[PathBuf]) -> Outcome {
    let mut reasons = Vec::new();
    let invalid = candidates[0].clone();
    let valid = candidates[1].clone();
    let mut player = ReferencePlayerApp::new(RealSource);
    if !matches!(player.open(main), OpenOutcome::Opened) {
        return (
            "RED",
            vec!["O2: the first Open did not establish".into()],
            None,
        );
    }
    let a_path = player.active_source().expect("committed").to_owned();
    let a = player.active_handle().expect("committed").clone();
    if !expect_liveness(&a, 2, "O2 pre-refusal A", &mut reasons) {
        return ("RED", reasons, None);
    }

    let outcome = player.open(&invalid);
    let refused = matches!(outcome, OpenOutcome::Refused { .. });
    if refused {
        if let OpenOutcome::Refused { diagnostic } = outcome {
            eprintln!("verdict=GREEN reason: O2 refusal diagnostic: {diagnostic}");
        }
    } else {
        reasons.push(format!("O2: invalid candidate returned {outcome:?}"));
    }
    // THE frozen product property: an invalid Open candidate never
    // kills live playback. The witnesses: the player still reports the
    // SAME committed source, the old episode holds no stop intent and
    // no terminal, and its position publication keeps advancing.
    let same_source = player.active_source() == Some(a_path.as_path());
    let still_live = refused
        && same_source
        && !a.observe().stop_requested
        && a.observe().terminal_outcome.is_none()
        && expect_liveness(&a, 4, "O2 post-refusal A", &mut reasons);
    if !same_source {
        reasons.push("O2: the committed episode changed across a refusal".into());
    }

    // And a later valid Open still works (the player stays usable).
    let later = if still_live {
        matches!(player.open(&valid), OpenOutcome::Opened)
    } else {
        false
    };
    let a_final = a.observe();
    let report = quit_report(&mut player);
    let ok = refused
        && still_live
        && later
        && a_final.terminal_outcome == Some(EpisodeTerminalOutcome::Stopped)
        && report.disposal == Some(qianqian_composition::DisposeVerdict::Discharged);
    (
        if ok { "GREEN" } else { "RED" },
        reasons,
        Some(serde_json::json!({
            "refused": refused,
            "post_refusal_stop_requested": a.observe().stop_requested,
            "quit_disposal": format!("{:?}", report.disposal),
        })),
    )
}

/// O3 — paused A → Open B (D14.7 stop-from-paused).
fn o3(main: &Path, candidates: &[PathBuf]) -> Outcome {
    let mut reasons = Vec::new();
    let candidate = candidates[0].clone();
    let mut player = ReferencePlayerApp::new(RealSource);
    if !matches!(player.open(main), OpenOutcome::Opened) {
        return (
            "RED",
            vec!["O3: the first Open did not establish".into()],
            None,
        );
    }
    let a = player.active_handle().expect("committed").clone();
    if !expect_liveness(&a, 2, "O3 pre-pause A", &mut reasons) {
        return ("RED", reasons, None);
    }
    a.request_pause();
    // The D14.7 establishment conjunction must become true on the real
    // mechanism (gate park + output-tail quiescence evidence).
    let mut paused_established = false;
    for _ in 0..20 {
        if a.observe().paused() {
            paused_established = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if paused_established {
        eprintln!("verdict=GREEN reason: O3 pause established (frozen D14.7 conjunction)");
    } else {
        reasons.push("O3: paused() never established on the real mechanism".into());
    }

    let outcome = player.open(&candidate);
    let opened = matches!(outcome, OpenOutcome::Opened);
    let a_final = a.observe();
    let b_established = opened && expect_established(&player, "O3 new episode B", &mut reasons);
    let report = quit_report(&mut player);
    let ok = paused_established
        && opened
        && b_established
        && a_final.terminal_outcome == Some(EpisodeTerminalOutcome::Stopped)
        && report.disposal == Some(qianqian_composition::DisposeVerdict::Discharged);
    (
        if ok { "GREEN" } else { "RED" },
        reasons,
        Some(serde_json::json!({
            "paused_established": paused_established,
            "old_terminal": format!("{:?}", a_final.terminal_outcome),
            "quit_disposal": format!("{:?}", report.disposal),
        })),
    )
}

/// O4 — seeked A → Open B (D14.5 cut, then replacement).
fn o4(main: &Path, candidates: &[PathBuf]) -> Outcome {
    let mut reasons = Vec::new();
    let candidate = candidates[0].clone();
    let mut player = ReferencePlayerApp::new(RealSource);
    if !matches!(player.open(main), OpenOutcome::Opened) {
        return (
            "RED",
            vec!["O4: the first Open did not establish".into()],
            None,
        );
    }
    let a = player.active_handle().expect("committed").clone();
    if !expect_liveness(&a, 2, "O4 pre-seek A", &mut reasons) {
        return ("RED", reasons, None);
    }
    let before = a.observe().position.unwrap_or(0);
    a.request_seek(Duration::from_secs(40));
    // Give the committed cutover a moment; the position rebase (if any)
    // is the D14.5 evidence — this smoke does not re-prove F5, it only
    // requires the episode to remain live and then replace cleanly.
    std::thread::sleep(Duration::from_secs(1));
    let after = a.observe();
    eprintln!(
        "verdict=GREEN reason: O4 post-seek position {:?} (pre {before}) stop_requested={}",
        after.position, after.stop_requested
    );
    if !expect_liveness(&a, 2, "O4 post-seek A", &mut reasons) {
        return ("RED", reasons, None);
    }

    let outcome = player.open(&candidate);
    let opened = matches!(outcome, OpenOutcome::Opened);
    let a_final = a.observe();
    let b_established = opened && expect_established(&player, "O4 new episode B", &mut reasons);
    let report = quit_report(&mut player);
    let ok = opened
        && b_established
        && a_final.terminal_outcome == Some(EpisodeTerminalOutcome::Stopped)
        && report.disposal == Some(qianqian_composition::DisposeVerdict::Discharged);
    (
        if ok { "GREEN" } else { "RED" },
        reasons,
        Some(serde_json::json!({
            "pre_seek_position": before,
            "post_seek_position": after.position,
            "old_terminal": format!("{:?}", a_final.terminal_outcome),
            "quit_disposal": format!("{:?}", report.disposal),
        })),
    )
}

/// O5 — repeated replacement A→B→C→D, then quit.
fn o5(main: &Path, candidates: &[PathBuf]) -> Outcome {
    let mut reasons = Vec::new();
    let mut player = ReferencePlayerApp::new(RealSource);
    if !matches!(player.open(main), OpenOutcome::Opened) {
        return (
            "RED",
            vec!["O5: the first Open did not establish".into()],
            None,
        );
    }
    let mut previous = player.active_handle().expect("committed").clone();
    let mut replacements = Vec::new();
    for (n, candidate) in candidates.iter().enumerate() {
        if !expect_liveness(
            &previous,
            1,
            &format!("O5 episode {n} pre-replacement"),
            &mut reasons,
        ) {
            return ("RED", reasons, None);
        }
        let outcome = player.open(candidate);
        let opened = matches!(outcome, OpenOutcome::Opened);
        let settled = previous.observe().terminal_outcome == Some(EpisodeTerminalOutcome::Stopped);
        replacements.push(opened && settled);
        eprintln!(
            "verdict=GREEN reason: O5 replacement {n} → {} opened={opened} old_stopped={settled}",
            candidate.display()
        );
        match player.active_handle() {
            Some(h) => previous = h.clone(),
            None => {
                reasons.push(format!("O5: replacement {n} left no active episode"));
                return ("RED", reasons, None);
            }
        }
    }
    let report = quit_report(&mut player);
    let ok = replacements.iter().all(|&r| r)
        && report.terminal == Some(EpisodeTerminalOutcome::Stopped)
        && report.disposal == Some(qianqian_composition::DisposeVerdict::Discharged);
    (
        if ok { "GREEN" } else { "RED" },
        reasons,
        Some(serde_json::json!({
            "replacements_ok": replacements,
            "quit_terminal": format!("{:?}", report.terminal),
            "quit_disposal": format!("{:?}", report.disposal),
        })),
    )
}
