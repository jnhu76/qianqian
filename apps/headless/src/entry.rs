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
use std::path::{Path, PathBuf};

use crate::cli::{self, Invocation};
// Presentation/report contract of the transports; every use site is
// playback wiring, so the imports follow the same gate.
#[cfg(feature = "playback")]
use crate::machine;
#[cfg(feature = "playback")]
use crate::machine::StartFailure;
#[cfg(feature = "playback")]
use crate::player::OpenOutcome;

/// One product invocation of either binary target: parse argv, route
/// to a transport. `bin_name` is the invoking binary target's own
/// compiled name (`CARGO_BIN_NAME` at the wrapper) — the only way the
/// two product binaries differ, and the only thing `--version` prints.
pub fn run(bin_name: &str) -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse_invocation(&args) {
        Ok(Invocation::Help) => {
            print!("{}", cli::usage());
            ExitCode::SUCCESS
        }
        Ok(Invocation::Version) => {
            println!("{} {}", bin_name, env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Invocation::Interactive) => run_playback(Vec::new(), Shell::ReferencePlayer),
        Ok(Invocation::Play { files }) => run_playback(files, Shell::ReferencePlayer),
        Ok(Invocation::MachinePlay { file }) => run_playback(vec![file], Shell::Machine),
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

#[cfg(feature = "playback")]
fn run_playback(files: Vec<PathBuf>, shell: Shell) -> ExitCode {
    match shell {
        Shell::Machine => match start_episode(single_file(files)) {
            Ok(episode) => machine_transport(episode),
            Err(failure) => report_start_failure(failure),
        },
        Shell::ReferencePlayer => reference_player_transport(files),
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

#[cfg(feature = "playback")]
struct Episode {
    runtime: qianqian_app::QianqianApp,
    handle: qianqian_playback::PlaybackSessionHandle,
    file: PathBuf,
    /// Whether the session fiber activated. Derived once from the K0
    /// snapshot as ACTIVATION diagnostics only — never playback status
    /// truth (F2): it decides whether waiting for a terminal Fact is
    /// meaningful, because an episode that never started has none.
    activated: bool,
}

// Why the episode wiring never reached a running session. The enum and
// its report/exit contract live in [`crate::machine`]; the
// wiring here only constructs it.

/// Install the components for one episode over one local file. The
/// scriptable transport's single-episode wiring, unchanged since F5.
#[cfg(feature = "playback")]
fn start_episode(file: PathBuf) -> Result<Episode, StartFailure> {
    use qianqian_playback::{PlaybackSessionHandle, playback_session_spec};

    let mut runtime = qianqian_app::QianqianApp::new();
    if let Err(e) = runtime.register_component(qianqian_decode_songcore::songcore_decode_plugin()) {
        return Err(StartFailure::Registration {
            message: format!("decode plugin registration failed: {e:?}"),
        });
    }
    if let Err(e) = runtime.register_component(qianqian_output_wasapi::output_plugin()) {
        return Err(StartFailure::Registration {
            message: format!("output plugin registration failed: {e:?}"),
        });
    }
    let handle = PlaybackSessionHandle::new();
    if let Err(e) = runtime.register_component(playback_session_spec(file.clone(), handle.clone()))
    {
        return Err(StartFailure::Registration {
            message: format!("session registration failed: {e:?}"),
        });
    }

    if let Err(errors) = runtime.revise_desired(vec![
        desired("decode", "songcore_decode_plugin"),
        desired("output", "output_plugin"),
        desired("session", "playback_session"),
    ]) {
        return Err(StartFailure::CompositionRefused {
            errors: format!("{errors}"),
        });
    }

    // revise_desired settles before returning: a failed activation is
    // visible in the snapshot, and there is no episode to wait for.
    let activated = runtime
        .composition_snapshot()
        .fibers
        .get("session")
        .map(|f| f.state)
        == Some(qianqian_composition::FiberState::Active);
    Ok(Episode {
        runtime,
        handle,
        file,
        activated,
    })
}

#[cfg(feature = "playback")]
fn report_start_failure(failure: machine::StartFailure) -> ExitCode {
    eprintln!("{}", failure.report());
    failure.exit_code()
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
fn reference_player_transport(files: Vec<PathBuf>) -> ExitCode {
    use crate::input;
    use crate::player::ReferencePlayerApp;

    let interactive_startup = files.is_empty();
    let mut player = ReferencePlayerApp::new(RealEpisodeSource);

    // U1 input expansion: startup argv may name files AND folders. The
    // expansion is host input preparation — candidates, not playback
    // truth. `open_expanded` runs the first candidate through the
    // EXISTING frozen replacement and seeds the accepted list ONLY on
    // replacement commit evidence, so an empty/unreadable expansion and
    // a refused/failed first candidate can never commit a misleading
    // list (Issue #166 §9/§10).
    let expansion = input::expand_inputs(&files);
    let startup_open = input::open_expanded(&mut player, &expansion);
    let initial_status = startup_feedback(&expansion, startup_open.as_ref());
    if let Some(OpenOutcome::FailStop { diagnostic }) = &startup_open {
        // A latched §G.6 violation has no exit and earns no shell.
        eprintln!("fail-stop: {diagnostic}");
        return ExitCode::from(1);
    }

    if let Err(error) = crate::tui::run(&mut player, initial_status) {
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
    let disposal_quiet = report.snapshot.as_ref().is_none_or(|s| s.quiet);
    // The interactive launch was handed NO episode by argv: quitting
    // such a session without a settled episode is its NORMAL end (the
    // user opened the player and closed it), not the scriptable
    // transport's "episode never activated" failure — that contract
    // describes a transport that was HANDED an episode and failed to
    // start it. A fail-stop latch still fails, and any session where an
    // episode went live reports through the ordinary table.
    if interactive_startup
        && report.terminal.is_none()
        && !player.is_fail_stopped()
        && disposal_quiet
    {
        ExitCode::SUCCESS
    } else {
        machine::episode_exit_code(report.terminal, disposal_quiet)
    }
}

/// The shell's first status line: the expansion summary plus the
/// startup Open's feedback. `None` on the quiet successful single-file
/// start (the old-world behavior); a folder start summarizes what got
/// seeded because that is the user's only view of the expansion.
#[cfg(feature = "playback")]
fn startup_feedback(
    expansion: &crate::input::ExpandedInputs,
    startup_open: Option<&OpenOutcome>,
) -> Option<String> {
    let opened = match startup_open {
        None => {
            // No candidate anywhere in the argv: an honest refusal, and
            // the shell still runs so the user can open a valid source.
            // Same operation vocabulary as the in-shell refusal.
            return Some(format!("open refused: {}", expansion.refusal()));
        }
        Some(OpenOutcome::Opened) => None,
        Some(OpenOutcome::Refused { diagnostic }) => Some(format!("open refused: {diagnostic}")),
        Some(OpenOutcome::ActivationFailedClean { diagnostic }) => {
            Some(format!("open failed (clean): {diagnostic}"))
        }
        Some(OpenOutcome::FailStop { .. }) => unreachable!("handled by the caller"),
    };
    // Beyond one accepted candidate (or any skip), say what was seeded.
    let summary = expansion.accepted.len() > 1 || expansion.skipped > 0;
    match (opened, summary) {
        (None, false) => None,
        (None, true) => Some(format!(
            "opened {} ({})",
            expansion.accepted[0].display(),
            expansion.summary()
        )),
        (Some(feedback), _) => Some(match expansion.accepted.len() {
            0 => feedback,
            _ => format!("{feedback} ({})", expansion.summary()),
        }),
    }
}

/// The real host wiring of the F6 seams: the decode provider's
/// stateless probe query, and the fresh-root start mounting the
/// SongCore decode Plugin, the Output Plugin (whose host-selected
/// backend mechanism is WASAPI), and the playback session —
/// the same desired composition as ever, one fresh root per episode.
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

    fn start(&self, source: &Path, initial_output_level: u8) -> crate::player::StartAttempt {
        use crate::player::StartAttempt;
        use qianqian_playback::PlaybackSessionHandle;

        let mut runtime = qianqian_app::QianqianApp::new();
        let handle = PlaybackSessionHandle::new();
        // The App's desired stream factor (D14.9) routes BEFORE
        // activation, so the mechanism applies it at stream open.
        handle.request_output_level(initial_output_level);
        if let Err(e) =
            runtime.register_component(qianqian_decode_songcore::songcore_decode_plugin())
        {
            return StartAttempt {
                runtime,
                handle,
                refused: Some(format!("decode plugin registration failed: {e:?}")),
            };
        }
        if let Err(e) = runtime.register_component(qianqian_output_wasapi::output_plugin()) {
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
        if let Err(errors) = runtime.revise_desired(vec![
            desired("decode", "songcore_decode_plugin"),
            desired("output", "output_plugin"),
            desired("session", "playback_session"),
        ]) {
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

/// The scriptable stdin/stdout transport (automation contract, F1/F2):
/// while the episode runs, stdin lines go through the frozen
/// interactive parser; `stop` requests the stop and `status` renders
/// the seam's coherent observation through the shared truthful
/// projection. The transport never touches the edge, the stream, or
/// any mechanism. The session never activating is reported honestly:
/// no episode exists, so there is no terminal Fact to wait for and
/// none may be forged (D14.2).
#[cfg(feature = "playback")]
fn machine_transport(mut episode: Episode) -> ExitCode {
    if !episode.activated {
        let diagnostic = episode.handle.observe().activation_error;
        eprintln!(
            "{}",
            machine::activation_failure_report(diagnostic.as_deref())
        );
        let disposal = episode.runtime.dispose();
        for warning in machine::disposal_warnings(&disposal.snapshot) {
            eprintln!("{warning}");
        }
        if let Some(line) = machine::disposal_verdict_warning(&disposal.verdict) {
            eprintln!("{line}");
        }
        return machine::episode_exit_code(None, disposal.snapshot.quiet);
    }

    if let Some(format) = episode.handle.observe().source_format {
        println!(
            "source: {} Hz, {} channels, mask {:#x}",
            format.sample_rate, format.channels, format.channel_mask
        );
    }
    println!("playing {} ...", episode.file.display());

    let control_handle = episode.handle.clone();
    let _control = std::thread::Builder::new()
        .name("qianqian-stdin".into())
        .spawn(move || {
            use std::io::BufRead;
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                match cli::parse_interactive_line(&line) {
                    Ok(cli::InteractiveCommand::Stop) => control_handle.request_stop(),
                    Ok(cli::InteractiveCommand::Pause) => control_handle.request_pause(),
                    Ok(cli::InteractiveCommand::Resume) => control_handle.request_resume(),
                    Ok(cli::InteractiveCommand::Seek { time }) => {
                        match cli::parse_seek_time(&time) {
                            Some(target) => control_handle.request_seek(target),
                            None => {
                                eprintln!("ignored input: cannot read seek time {time:?}")
                            }
                        }
                    }
                    Ok(cli::InteractiveCommand::Status) => {
                        print!(
                            "{}",
                            crate::status::format_status(&control_handle.observe())
                        );
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                    }
                    Ok(_) => {
                        eprintln!(
                            "not wired yet: only 'stop', 'pause', 'resume', 'seek' and \
                             'status' control playback"
                        )
                    }
                    Err(error) => eprintln!("ignored input: {error}"),
                }
            }
        });

    finish_episode(episode)
}

/// Wait for the committed terminal Fact, dispose, and report. The
/// scriptable transport's settle order (wait → dispose → outcome
/// lines → disposal report) and the exit-code contract stay identical;
/// the observable contract itself lives in [`machine`].
#[cfg(feature = "playback")]
fn finish_episode(mut episode: Episode) -> ExitCode {
    let outcome = episode.handle.wait_terminal();
    let disposal = episode.runtime.dispose();
    // The failure diagnostic is read separately from the settled
    // observation: it is presentation text, not part of the semantic
    // outcome (D14.2).
    let observation = episode.handle.observe();
    for (stream, line) in
        machine::outcome_report(outcome, observation.failure_diagnostic.as_deref())
    {
        match stream {
            machine::ReportStream::Stdout => println!("{line}"),
            machine::ReportStream::Stderr => eprintln!("{line}"),
        }
    }
    for warning in machine::disposal_warnings(&disposal.snapshot) {
        eprintln!("{warning}");
    }
    if let Some(line) = machine::disposal_verdict_warning(&disposal.verdict) {
        eprintln!("{line}");
    }
    machine::episode_exit_code(Some(outcome), disposal.snapshot.quiet)
}

#[cfg(not(feature = "playback"))]
fn run_playback(files: Vec<std::path::PathBuf>, shell: Shell) -> ExitCode {
    let _ = (files, shell);
    eprintln!(
        "this binary was built without the playback slice; \
         rebuild with: cargo build --release --features playback"
    );
    ExitCode::from(2)
}

#[cfg(feature = "playback")]
fn desired(id: &str, component: &'static str) -> qianqian_composition::DesiredEntry {
    qianqian_composition::DesiredEntry::enabled(
        id,
        component,
        qianqian_composition::Revision::new(1),
    )
}

/// The shell enum is referenced by the no-playback stub signature too.
#[cfg(not(feature = "playback"))]
enum Shell {
    ReferencePlayer,
    Machine,
}
