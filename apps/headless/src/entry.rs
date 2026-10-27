//! Headless bootstrap of Architecture v2: the binary transports' wiring.
//!
//! Correctness authority must not depend on a UI framework, so the
//! product must always be able to start without one.
//!
//! Qianqian App role (first-audible-slice design §6; canonical name per
//! ADR-PBK-002 D1): select the file, install the desired components,
//! wait for top-level completion, initiate explicit shutdown. The App
//! never pumps PCM, decodes, or owns a render loop. Argument grammar
//! lives in the library's [`cli`] module so parsing stays separable
//! from this playback wiring.
//!
//! Presentation is adapter-only: `play` — and, since U1 (Issue #166),
//! a bare no-argument launch — runs the reference-player transport —
//! since F6 (ADR-PBK-002 D14.6) its episode lifetime is owned by the
//! reference player ([`crate::player`]), which sequentially
//! owns non-overlapping composition roots and serves the shell's Open
//! composition command — while `--machine play` keeps the scriptable
//! single-episode stdin/stdout transport. Both drive real episode
//! wiring and render only what the F2 seam observes; neither owns
//! playback truth, and the shell never sees anything past the
//! `PlaybackSessionHandle` (no K0 snapshot types, no PCM or provider
//! mechanisms). The no-argument shell starts with no episode at all;
//! startup argv (files and folders alike) is expanded by
//! [`crate::input`] into candidates for the same Open path.
//!
//! Both binary targets of this crate (`qianqian`, the canonical product
//! binary, and `qianqian-headless`, the historical regression target)
//! are thin wrappers that hand [`run`] their own compiled name; this
//! module is the whole of their behavior.

use std::process::ExitCode;

#[cfg(feature = "playback")]
use std::path::Path;
#[cfg(any(feature = "playback", test))]
use std::path::PathBuf;

use crate::cli::{self, Invocation};
// Presentation/report contract of the transports. `EqPreset` is part of
// the transport signature in BOTH builds (the no-playback stub accepts
// and drops it, like the stub `OrderPreference`/`Shell` enums);
// `AudioProcessingConfig` is used only by playback wiring.
#[cfg(feature = "playback")]
use qianqian_playback::AudioProcessingConfig;
use qianqian_playback::EqPreset;

#[cfg(any(feature = "playback", test))]
use crate::machine;
#[cfg(feature = "playback")]
use crate::player::OpenOutcome;

/// One product invocation of either binary target: parse argv, route
/// to a transport. `bin_name` is the invoking binary target's own
/// compiled name (`CARGO_BIN_NAME` at the wrapper) — the only way the
/// two product binaries differ, and the only thing `--version` prints.
pub fn run(bin_name: &str) -> ExitCode {
    // args_os, never args(): Windows command lines are UTF-16, and a
    // non-Unicode argument (e.g. a pasted corrupted filename) must land
    // in the ordinary unknown-command refusal, not panic the product
    // before the parser ever sees it. Lossy display is fine here: the
    // grammar decides on text, and a path that cannot be represented
    // cleanly is simply not found by the expansion.
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    match cli::parse_invocation(&args) {
        Ok(Invocation::Help) => {
            print!("{}", cli::usage());
            ExitCode::SUCCESS
        }
        Ok(Invocation::Version) => {
            println!("{} {}", bin_name, env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Invocation::Interactive) => run_playback(
            Vec::new(),
            OrderPreference::Sequential,
            None,
            Shell::ReferencePlayer,
        ),
        Ok(Invocation::Play { files, shuffle, eq }) => run_playback(
            files,
            if shuffle {
                OrderPreference::Shuffle
            } else {
                OrderPreference::Sequential
            },
            eq,
            Shell::ReferencePlayer,
        ),
        Ok(Invocation::MachinePlay { file }) => run_playback(
            vec![file],
            OrderPreference::Sequential,
            // The machine transport's grammar has no EQ selection: the
            // scriptable single-episode transport runs bypass (I4).
            None,
            Shell::Machine,
        ),
        Err(error) => {
            eprintln!("error: {error}");
            eprint!("{}", cli::usage());
            ExitCode::from(2)
        }
    }
}

/// Which presentation adapter renders the episode. The choice is the
/// user's (explicit argv), never sniffed from the environment.
#[cfg(feature = "playback")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shell {
    /// Interactive reference-player terminal shell (F6 Open-capable).
    ReferencePlayer,
    /// Scriptable stdin/stdout transport (automation contract).
    Machine,
}

/// The startup traversal-order preference the argv asked for. A product
/// preference, not playback truth; the interactive player owns it from
/// then on (the `R` key toggles the same policy).
#[cfg(feature = "playback")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum OrderPreference {
    Sequential,
    Shuffle,
}

#[cfg(feature = "playback")]
fn run_playback(
    files: Vec<PathBuf>,
    order: OrderPreference,
    eq: Option<EqPreset>,
    shell: Shell,
) -> ExitCode {
    // The host owns the diagnostic outlet (DG-1): one installation at transport
    // startup, one bounded drain at transport exit. The placement is deliberate
    // — HELP/VERSION and argument-grammar refusals run no transport, produce no
    // records and pay nothing.
    let diagnostics = install_host_diagnostics();
    // The panic-hook posture (DG-4b) is established HERE, before any transport
    // starts: every isolation-obligated thread the transports create — the
    // decode worker, the render leg — registers at thread start, and the
    // posture must already be in place when it does, so a panic on such a
    // thread never enters an unknown synchronous sink (whose default print
    // takes the process-global stderr lock and can therefore block).
    crate::machine_input::install_panic_hook_posture();
    // The desired Audio Processing configuration (D14.11): the App owns
    // it; each episode establishment binds its own applied snapshot, so
    // a selection made at startup applies to every track this shell
    // starts. The machine transport selects nothing (bypass).
    let exit = match shell {
        Shell::Machine => machine_transport(start_episode(single_file(files))),
        Shell::ReferencePlayer => reference_player_transport(files, order, eq),
    };
    shutdown_host_diagnostics(diagnostics);
    exit
}

/// The bound on each exit-phase diagnostic wait: the drain's bounded wait and
/// the bounded wait for the report attempt. The drainer's idle window and its
/// publisher-quiescence window (250 ms, `PUBLISHER_QUIESCENCE_WINDOW`) are both
/// shorter than this, so a healthy shutdown returns well inside it even when the
/// close has to wait for an in-flight publication; the bound only matters when a
/// producer stalls or a sink is stuck (T5-5 strategy B: the exit path may
/// abandon — or abandon the *attempt* — and then says so).
#[cfg(any(feature = "playback", test))]
const HOST_DIAGNOSTIC_EXIT_DEADLINE: std::time::Duration = std::time::Duration::from_millis(500);

/// Install the process-wide diagnostic outlet with its stderr exporter.
/// Diagnostics are disposable (Class C): no exit code, protocol row, terminal
/// Fact or disposal verdict depends on this succeeding, so an installation
/// refused because the process already owns an outlet (a test harness owning
/// one) is not a product failure.
#[cfg(any(feature = "playback", test))]
fn install_host_diagnostics() -> Option<qianqian_diagnostics::Diagnostics> {
    use qianqian_diagnostics::{Config, Diagnostics, StderrExporter};
    // `StderrExporter::default()` admits every level: the site gates
    // (`QIANQIAN_AUDIO_LOG`, the av_log opt-in) already decide what is produced,
    // and the drainer is the only thing that renders.
    Diagnostics::install(Box::new(StderrExporter::default()), Config::default()).ok()
}

/// Stop admitting records and drain with the bounded deadline above. An
/// abandoned drainer is reported as abandoned, and a drain that could not be
/// ordered after publisher quiescence is reported as INCOMPLETE — neither is
/// ever dressed up as a completed drain. The line is written only when there is
/// something honest to report: a non-normal drain, undelivered records, an
/// exporter self-diagnostic, or a panic the machine-input hook counted on a
/// critical thread. A normal run prints nothing, and the machine protocol's
/// stdout is never touched.
///
/// The exit boundary is the whole point of the bounded shutdown, so the report
/// itself obeys it — DRAINED OR NOT. A completed drain proves that every write
/// the drainer attempted finished; it does NOT prove the sink still has room
/// for one more line. A pipe reader that stopped has no capacity left by this
/// point, and any other thread parked inside a write holds the process stderr
/// lock — a synchronous report write would then re-enter that sink and hand
/// process exit back to it, which is the one thing the exit path must never do
/// (the drain-deadline scenario's slightly later twin). Every exit-phase report
/// is therefore ATTEMPTED on a detached thread that nothing joins and that
/// holds no resource the exit path needs; the drain-not-abandoned case waits
/// only a bounded instant for that attempt (a healthy sink completes it in
/// microseconds, so the line is not lost to process exit), while an abandoned
/// drain — whose sink is stuck by the abandonment rule — does not wait at all.
/// A report lost to a sink that never drains is honest for a Class C
/// diagnostic.
#[cfg(any(feature = "playback", test))]
fn shutdown_host_diagnostics(diagnostics: Option<qianqian_diagnostics::Diagnostics>) {
    let Some(diagnostics) = diagnostics else {
        return;
    };
    let report = diagnostics.shutdown(HOST_DIAGNOSTIC_EXIT_DEADLINE);
    let counters = report.counters;
    if report.drained
        && report.undelivered_at_exit == 0
        && counters.self_diagnostics == 0
        && counters.critical_panics == 0
    {
        return;
    }
    let verdict = if report.drained {
        "drained"
    } else if report.abandoned {
        "ABANDONED (drainer stuck in a sink; lost records are counted, never claimed as delivered)"
    } else {
        "INCOMPLETE (a presentation admitted before the close outlasted the drainer's quiescence window: \
         the final sweep did not wait for it, and the accounting below is not a complete drain)"
    };
    let line = format!(
        "[qianqian-diagnostics] shutdown {verdict}: undelivered_at_exit={} dropped={} export_failed={} self_diagnostics={} critical_panics={}",
        report.undelivered_at_exit,
        counters.alpha_rejected + counters.beta_rejected,
        counters.export_failed,
        counters.self_diagnostics,
        counters.critical_panics,
    );
    attempt_exit_report(
        line,
        (!report.abandoned).then_some(HOST_DIAGNOSTIC_EXIT_DEADLINE),
    );
}

/// Attempt one exit-phase diagnostic report off the host's return path: one
/// detached writer that nothing joins, plus — only when the drainer EXITED
/// (`wait` is `Some`, i.e. the drain is not abandoned) — a bounded wait for the
/// attempt.
///
/// The wait exists so a healthy sink's report is not lost to process exit; the
/// bound exists so the attempt can never become what the host waits on. In the
/// abandoned case the sink is stuck (a writer is parked inside it, usually
/// holding the stderr lock), so waiting could only burn the deadline: the
/// attempt is simply left parked and dies with the process.
#[cfg(any(feature = "playback", test))]
fn attempt_exit_report(line: String, wait: Option<std::time::Duration>) {
    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signal = std::sync::Arc::clone(&finished);
    let spawned = std::thread::Builder::new()
        .name("qianqian-diagnostics-report".into())
        .spawn(move || {
            #[cfg(test)]
            {
                tier2_tests::report_entered();
                let result = qianqian_diagnostics::write_stderr_line(&line);
                tier2_tests::report_returned(result.is_ok());
            }
            #[cfg(not(test))]
            let _ = qianqian_diagnostics::write_stderr_line(&line);
            signal.store(true, std::sync::atomic::Ordering::Release);
        });
    if spawned.is_err() {
        // A report that cannot even get a thread is an accepted loss: nothing
        // here decides a product outcome (Class C).
        return;
    }
    let Some(bound) = wait else {
        return;
    };
    let deadline = std::time::Instant::now() + bound;
    while !finished.load(std::sync::atomic::Ordering::Acquire) {
        if std::time::Instant::now() >= deadline {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// The machine transport's argv is pinned to exactly one file (grammar
/// level); this unreachable-else conversion keeps that invariant local.
#[cfg(feature = "playback")]
fn single_file(files: Vec<PathBuf>) -> PathBuf {
    let mut it = files.into_iter();
    let first = it.next().expect("grammar guarantees one file");
    debug_assert!(it.next().is_none(), "--machine play takes exactly one file");
    first
}

#[cfg(any(feature = "playback", test))]
struct Episode {
    runtime: qianqian_app::QianqianApp,
    handle: qianqian_playback::PlaybackSessionHandle,
    file: PathBuf,
    /// Completed fresh-assembly result (D14.6), independent of D11 terminal.
    establishment: qianqian_playback::EstablishmentResult,
    /// Transport-specific admission report; never an establishment predicate.
    admission_failure: Option<machine::StartFailure>,
}

/// Install the canonical fresh composition for one machine episode.
#[cfg(feature = "playback")]
fn start_episode(file: PathBuf) -> Episode {
    let assembled = crate::assembly::establish(file.clone(), AudioProcessingConfig::BYPASS, 100);
    machine_episode(file, assembled)
}

#[cfg(any(feature = "playback", test))]
fn machine_episode(file: PathBuf, assembled: crate::assembly::AssemblyOutcome) -> Episode {
    Episode {
        runtime: assembled.start.runtime,
        handle: assembled.start.handle,
        file,
        establishment: assembled.start.establishment,
        admission_failure: assembled.admission_failure,
    }
}

/// The reference-player transport (F6, ADR-PBK-002 D14.6). With argv
/// paths, the first OPENED candidate through the reference player — the
/// same Open composition command the O key serves — is the startup
/// episode, and the input expansion's whole accepted list seeds the
/// startup playlist. With NO argv paths (the U1 interactive launch) no
/// startup Open is attempted at all: the shell starts truthfully idle,
/// and music is loaded from inside it. After the shell exits, the
/// player settles whatever episode is live and this transport reports
/// with the SAME honest contract as the scriptable transport (outcome
/// lines, disposal warnings, the exit-code table).
#[cfg(feature = "playback")]
fn reference_player_transport(
    files: Vec<PathBuf>,
    order: OrderPreference,
    preset: Option<EqPreset>,
) -> ExitCode {
    use crate::input;
    use crate::player::ReferencePlayerApp;
    use crate::playlist::PlaybackOrder;

    let interactive_startup = files.is_empty();
    let mut player = ReferencePlayerApp::new(RealEpisodeSource);
    if let Some(preset) = preset {
        player.set_eq_preset(preset).expect("valid factory preset");
    }
    // `--shuffle` selects the order policy BEFORE the startup Open, so
    // the playlist that rides the commit is already the shuffled one.
    // The start discipline is untouched: the expansion's first accepted
    // candidate is still what opens (probe → old-side clear → fresh
    // root), and the shuffle cycle simply anchors on it (Issue #166
    // §24).
    if order == OrderPreference::Shuffle {
        player.set_order(PlaybackOrder::Shuffle);
    }

    // U1 input expansion: startup argv may name files AND folders. The
    // expansion is host input preparation — candidates, not playback
    // truth. `prepare_startup` runs the first candidate through the
    // EXISTING frozen replacement and seeds the accepted list ONLY on
    // replacement commit evidence, so an empty/unreadable expansion and
    // a refused/failed first candidate can never commit a misleading
    // list (Issue #166 §9/§10) — and an INTERACTIVE launch (no argv)
    // prepares nothing at all: no Open attempted, no feedback
    // fabricated (U1 corrective REQUIRED-1).
    //
    // A big folder scans (enumerate + probe every candidate)
    // synchronously before the TUI appears — the documented
    // synchronous-Open stall — so the user gets one honest line about
    // what is happening instead of a blank console.
    if let Some(first_root) = files.first() {
        println!("scanning {} ...", first_root.display());
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }
    let preparation = input::prepare_startup(&files, &mut player);
    if let Some(OpenOutcome::FailStop { diagnostic }) = &preparation.startup_open {
        // A latched §G.6 violation has no exit and earns no shell.
        eprintln!("fail-stop: {diagnostic}");
        return ExitCode::from(1);
    }
    let expansion = preparation.expansion;
    let startup_open = preparation.startup_open;
    let initial_status = preparation.initial_status;
    // The startup Open's B1 projection (Issue #23 B2): a refused or
    // clean-failed startup enters the shell's operation-error slot, so
    // `E` shows its details exactly like an in-shell refusal. A
    // startup whose expansion produced no candidate at all refused the
    // same operation — its own refusal text is the diagnostic. (A
    // startup FailStop never reaches the shell: the transport returns
    // before this point.)
    let initial_error = match &startup_open {
        Some(outcome) => crate::error_presentation::presentation_for_open(outcome),
        None if !interactive_startup => {
            crate::error_presentation::presentation_for_open(&OpenOutcome::Refused {
                diagnostic: expansion.refusal(),
            })
        }
        None => None,
    };
    // A startup that carries an error presentation reports it in the
    // shell's own B1 status vocabulary (Issue #23 B2), exactly like the
    // same refusal would read after an in-shell Open; the headless
    // transport's startup copy is untouched.
    let initial_status = match &initial_error {
        Some(presentation) => Some(crate::error_presentation::operation_error_feedback(
            "open",
            presentation,
        )),
        None => initial_status,
    };

    let shell = crate::tui::run(&mut player, initial_status, initial_error);
    if let Err(error) = &shell {
        eprintln!("reference-player shell failed: {error}");
    }

    let report = player.quit();
    if let Some(terminal) = report.terminal {
        for (stream, line) in machine::outcome_report(terminal, report.diagnostic.as_deref()) {
            match stream {
                machine::ReportStream::Stdout => println!("{line}"),
                machine::ReportStream::Stderr => eprintln!("{line}"),
            }
        }
    }
    // The startup Open's own feedback for the no-episode case: the
    // old-world contract (a visible reason, exit 1) is preserved; the
    // shell carried the same line as its status feedback.
    match &startup_open {
        Some(OpenOutcome::Opened) => {}
        Some(OpenOutcome::Refused { diagnostic }) => eprintln!("open refused: {diagnostic}"),
        Some(OpenOutcome::ActivationFailedClean { diagnostic }) => {
            eprintln!("{}", machine::activation_failure_report(Some(diagnostic)))
        }
        Some(OpenOutcome::FailStop { .. }) => unreachable!("handled above"),
        // The expansion produced no candidate at all (nothing was ever
        // opened): the same visible refusal the shell showed, but only
        // for an argv-driven start — the interactive launch attempted
        // nothing and owes no stderr line.
        None if !interactive_startup => {
            eprintln!("open refused: {}", expansion.refusal());
        }
        None => {}
    }
    if let Some(snapshot) = &report.snapshot {
        for warning in machine::disposal_warnings(snapshot) {
            eprintln!("{warning}");
        }
    }
    // A §G.6 latch raised INSIDE the shell (O/N/P) has no quit report
    // of its own: the latch's reason is made persistent here so the
    // exit-1 line below always has a visible cause on stderr. (A
    // STARTUP FailStop already returned above; a quit-time disposal
    // violation printed its own warnings.)
    if let Some(reason) = player.fail_stop_reason() {
        eprintln!("fail-stop: {reason}");
    }
    let disposal_quiet = report.snapshot.as_ref().is_none_or(|s| s.quiet);
    // The exit code describes the SESSION — plus, since the U1
    // corrective (REQUIRED-3), the shell itself: a failed TUI (terminal
    // setup/draw/input I/O) is a host/presentation failure that can
    // never exit successfully, whatever the session did afterward, and
    // it is never forged into a playback terminal outcome. The whole
    // table is pinned in [`machine::reference_exit_code`]. An
    // interactive launch was handed no episode by argv, so ending it
    // without a settled episode is its NORMAL end (the user opened the
    // player and closed it) — not the scriptable transport's "episode
    // never activated" failure, which describes a transport that was
    // HANDED an episode and failed to start it. Every argv-driven start
    // reports through the ordinary table, whatever happened.
    machine::reference_exit_code(
        shell.is_ok(),
        interactive_startup,
        report.terminal,
        player.is_fail_stopped(),
        disposal_quiet,
    )
}

/// The real host wiring of the F6 seams: the decode provider's
/// stateless probe query, and the fresh-root start mounting the
/// SongCore decode Plugin, the Output Plugin (whose host-selected
/// backend mechanism is WASAPI), and the playback session —
/// the same desired composition as ever, one fresh root per episode,
/// with the shell's desired Audio Processing configuration (D14.11)
/// handed to each establishment.
#[cfg(feature = "playback")]
struct RealEpisodeSource;

#[cfg(feature = "playback")]
impl crate::player::EpisodeStart for RealEpisodeSource {
    fn probe(&self, candidate: &Path) -> Result<(), String> {
        // The public stateless SourceFacts query (D14.6): open → read
        // format/duration facts → close, no PCM. Its facts are advisory
        // evidence for THIS refusal decision; the authoritative source
        // evidence is the new activation's own.
        qianqian_decode_songcore::probe_media(candidate)
            .map(|_facts| ())
            .map_err(|e| e.message)
    }

    fn start(
        &self,
        source: &Path,
        initial_output_level: u8,
        processing: AudioProcessingConfig,
    ) -> crate::player::StartAttempt {
        crate::assembly::establish(source.to_path_buf(), processing, initial_output_level).start
    }
}

/// The scriptable stdin/stdout transport (automation contract, F1/F2):
/// while the episode runs, stdin lines go through the frozen
/// interactive parser; `stop` requests the stop and `status` renders
/// the seam's pure observation through the shared truthful
/// projection. The transport never touches the edge, the stream, or
/// any mechanism. The session never activating is reported honestly:
/// no episode exists, so there is no terminal Fact to wait for and
/// none may be forged (D14.2).
#[cfg(any(feature = "playback", test))]
fn machine_transport(episode: Episode) -> ExitCode {
    machine_transport_with_reader(
        episode,
        crate::machine_input::spawn_reader,
        emit_machine_report,
    )
}

#[cfg(any(feature = "playback", test))]
fn machine_transport_with_reader(
    mut episode: Episode,
    start_reader: impl FnOnce(crate::machine_input::HostInput, qianqian_playback::PlaybackSessionHandle),
    report: impl FnMut(machine::ReportStream, &str),
) -> ExitCode {
    if let qianqian_playback::EstablishmentResult::NotEstablished { diagnostic } =
        &episode.establishment
    {
        let report = match &episode.admission_failure {
            Some(presentation) => presentation.report(),
            None => machine::activation_failure_report(diagnostic.as_deref()),
        };
        eprintln!("{report}");
        let disposal = episode.runtime.dispose();
        for warning in machine::disposal_warnings(&disposal.snapshot) {
            eprintln!("{warning}");
        }
        if let Some(line) = machine::disposal_verdict_warning(&disposal.verdict) {
            eprintln!("{line}");
        }
        return match &episode.admission_failure {
            Some(presentation) if disposal.snapshot.quiet => presentation.exit_code(),
            _ => machine::episode_exit_code(None, disposal.snapshot.quiet),
        };
    }

    if let Some(format) = episode.handle.observe().source_format {
        println!(
            "source: {} Hz, {} channels, mask {:#x}",
            format.sample_rate, format.channels, format.channel_mask
        );
    }
    println!("playing {} ...", episode.file.display());

    let input = crate::machine_input::HostInput::default();
    start_reader(input.clone(), episode.handle.clone());
    finish_episode(episode, input, report)
}

/// Wait for the committed terminal Fact, dispose, and report. The
/// machine order is terminal wait → root disposal → host admission closure
/// → admitted-effect acknowledgement → result seal → final owner reports.
/// These milestones do not acknowledge stdin-reader or process termination.
/// Report spelling/exit presentation lives in [`machine`].
#[cfg(any(feature = "playback", test))]
fn finish_episode(
    mut episode: Episode,
    input: crate::machine_input::HostInput,
    mut report: impl FnMut(machine::ReportStream, &str),
) -> ExitCode {
    let outcome = episode.handle.wait_terminal();
    let disposal = episode.runtime.dispose();
    // The failure diagnostic is read separately from the settled
    // observation: it is presentation text, not part of the semantic
    // outcome (D14.2).
    let observation = episode.handle.observe();
    let host_failure = input.seal();
    if let Some(failure) = host_failure {
        report(machine::ReportStream::Stderr, failure.report());
    }
    for (stream, line) in
        machine::outcome_report(outcome, observation.failure_diagnostic.as_deref())
    {
        report(stream, &line);
    }
    for warning in machine::disposal_warnings(&disposal.snapshot) {
        report(machine::ReportStream::Stderr, &warning);
    }
    if let Some(line) = machine::disposal_verdict_warning(&disposal.verdict) {
        report(machine::ReportStream::Stderr, &line);
    }
    machine::machine_exit_code(
        Some(outcome),
        disposal.snapshot.quiet,
        host_failure.is_some(),
    )
}

#[cfg(any(feature = "playback", test))]
fn emit_machine_report(stream: machine::ReportStream, line: &str) {
    match stream {
        machine::ReportStream::Stdout => println!("{line}"),
        machine::ReportStream::Stderr => eprintln!("{line}"),
    }
}

#[cfg(not(feature = "playback"))]
fn run_playback(
    files: Vec<std::path::PathBuf>,
    order: OrderPreference,
    eq: Option<EqPreset>,
    shell: Shell,
) -> ExitCode {
    let _ = (files, order, eq, shell);
    eprintln!(
        "this binary was built without the playback slice; \
         rebuild with: cargo build --release --features playback"
    );
    ExitCode::from(2)
}

/// The shell and order enums are referenced by the no-playback stub
/// signature too.
#[cfg(not(feature = "playback"))]
#[derive(Clone, Copy, PartialEq, Eq)]
enum OrderPreference {
    Sequential,
    Shuffle,
}

#[cfg(not(feature = "playback"))]
enum Shell {
    ReferencePlayer,
    Machine,
}

#[cfg(test)]
pub(crate) fn run_machine_attempt_for_test(
    runtime: qianqian_app::QianqianApp,
    handle: qianqian_playback::PlaybackSessionHandle,
    establishment: qianqian_playback::EstablishmentResult,
) -> ExitCode {
    run_machine_assembly_for_test(crate::assembly::AssemblyOutcome {
        start: crate::player::StartAttempt {
            runtime,
            handle,
            establishment,
        },
        admission_failure: None,
    })
}

#[cfg(test)]
pub(crate) fn run_machine_assembly_for_test(
    assembled: crate::assembly::AssemblyOutcome,
) -> ExitCode {
    machine_transport(machine_episode("test://attempt".into(), assembled))
}

#[cfg(test)]
pub(crate) fn run_machine_reader_for_test(
    assembled: crate::assembly::AssemblyOutcome,
    start_reader: impl FnOnce(crate::machine_input::HostInput, qianqian_playback::PlaybackSessionHandle),
) -> ExitCode {
    machine_transport_with_reader(
        machine_episode("test://host-input".into(), assembled),
        start_reader,
        emit_machine_report,
    )
}

#[cfg(test)]
pub(crate) fn run_machine_reports_for_test(
    assembled: crate::assembly::AssemblyOutcome,
    start_reader: impl FnOnce(crate::machine_input::HostInput, qianqian_playback::PlaybackSessionHandle),
    report: impl FnMut(machine::ReportStream, &str),
) -> ExitCode {
    machine_transport_with_reader(
        machine_episode("test://host-input".into(), assembled),
        start_reader,
        report,
    )
}

/// The host exit boundary (F2 of the #287 review): the abandoned-drainer report
/// must not make the exit path wait on the stuck sink. Subprocess witness —
/// see the module docs.
#[cfg(test)]
mod shutdown_tests;

#[cfg(test)]
#[path = "tier2_tests.rs"]
mod tier2_tests;
