//! Headless bootstrap of Architecture v2.
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
//! Presentation is adapter-only: `play` opens the reference-player
//! terminal shell ([`qianqian_headless::tui`]), `--machine play` keeps
//! the scriptable stdin/stdout transport. Both drive the SAME episode
//! wiring below and render only what the F2 seam observes; neither
//! owns playback truth, and the shell never sees anything past the
//! `PlaybackSessionHandle` (no K0 snapshot types, no PCM or provider
//! mechanisms).

use std::process::ExitCode;

#[cfg(feature = "playback")]
use std::path::PathBuf;

use qianqian_headless::cli::{self, Invocation};
// Presentation/report contract of the transports; every use site is
// playback wiring, so the imports follow the same gate.
#[cfg(feature = "playback")]
use qianqian_headless::machine;
#[cfg(feature = "playback")]
use qianqian_headless::machine::StartFailure;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse_invocation(&args) {
        Ok(Invocation::Help) => {
            print!("{}", cli::usage());
            ExitCode::SUCCESS
        }
        Ok(Invocation::Version) => {
            println!("qianqian-headless {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Invocation::Play { file }) => run_playback(file, Shell::ReferencePlayer),
        Ok(Invocation::MachinePlay { file }) => run_playback(file, Shell::Machine),
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
    /// Interactive reference-player terminal shell.
    ReferencePlayer,
    /// Scriptable stdin/stdout transport (automation contract).
    Machine,
}

#[cfg(feature = "playback")]
fn run_playback(file: PathBuf, shell: Shell) -> ExitCode {
    match start_episode(file) {
        Ok(episode) => run_episode(episode, shell),
        Err(failure) => report_start_failure(failure),
    }
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
// its report/exit contract live in qianqian_headless::machine; the
// wiring here only constructs it.

/// Install the components for one episode over one local file.
#[cfg(feature = "playback")]
fn start_episode(file: PathBuf) -> Result<Episode, StartFailure> {
    use qianqian_playback::{PlaybackSessionHandle, playback_session_spec};

    let mut runtime = qianqian_app::QianqianApp::new();
    if let Err(e) = runtime.register_component(qianqian_decode_songcore::songcore_decode_plugin()) {
        return Err(StartFailure::Registration {
            message: format!("decode plugin registration failed: {e:?}"),
        });
    }
    if let Err(e) = runtime.register_component(qianqian_output_wasapi::wasapi_output_plugin()) {
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
        desired("output", "wasapi_output_plugin"),
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

#[cfg(feature = "playback")]
fn run_episode(episode: Episode, shell: Shell) -> ExitCode {
    if !episode.activated {
        return episode_without_session(episode, shell);
    }
    match shell {
        Shell::Machine => machine_transport(episode),
        Shell::ReferencePlayer => tui_transport(episode),
    }
}

/// The session never activated: no episode exists, so there is no
/// terminal Fact to wait for and none may be forged (D14.2). The TUI
/// still opens — the Diagnostics panel is exactly where an activation
/// failure belongs — while the machine transport reports immediately.
#[cfg(feature = "playback")]
fn episode_without_session(mut episode: Episode, shell: Shell) -> ExitCode {
    let diagnostic = episode.handle.observe().activation_error;
    if shell == Shell::ReferencePlayer
        && let Err(error) =
            qianqian_headless::tui::run(&episode.handle, &episode.file.to_string_lossy())
    {
        eprintln!("reference-player shell failed: {error}");
    }
    eprintln!(
        "{}",
        machine::activation_failure_report(diagnostic.as_deref())
    );
    let snapshot = episode.runtime.dispose();
    for warning in machine::disposal_warnings(&snapshot) {
        eprintln!("{warning}");
    }
    machine::episode_exit_code(None, snapshot.quiet)
}

/// The scriptable stdin/stdout transport (automation contract, F1/F2):
/// while the episode runs, stdin lines go through the frozen
/// interactive parser; `stop` requests the stop and `status` renders
/// the seam's coherent observation through the shared truthful
/// projection. The transport never touches the edge, the stream, or
/// any mechanism.
#[cfg(feature = "playback")]
fn machine_transport(episode: Episode) -> ExitCode {
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
                    Ok(cli::InteractiveCommand::Status) => {
                        print!(
                            "{}",
                            qianqian_headless::status::format_status(&control_handle.observe())
                        );
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                    }
                    Ok(_) => {
                        eprintln!(
                            "not wired yet: only 'stop', 'pause', 'resume' and 'status' \
                             control playback"
                        )
                    }
                    Err(error) => eprintln!("ignored input: {error}"),
                }
            }
        });

    finish_episode(episode)
}

/// The interactive reference-player transport: hand the episode to the
/// terminal shell and settle it when the shell exits. The shell only
/// renders observations and routes S to `request_stop`; THIS transport
/// owns the episode lifecycle — on quit (or shell failure) it records
/// stop intent iff no terminal Fact is committed yet, so the wait
/// below is decisive, and it never manufactures an outcome locally.
#[cfg(feature = "playback")]
fn tui_transport(episode: Episode) -> ExitCode {
    let shell_result =
        qianqian_headless::tui::run(&episode.handle, &episode.file.to_string_lossy());
    if let Err(error) = shell_result {
        eprintln!("reference-player shell failed: {error}");
    }
    if episode.handle.observe().terminal_outcome.is_none() {
        episode.handle.request_stop();
    }
    finish_episode(episode)
}

/// Wait for the committed terminal Fact, dispose, and report. Shared
/// by both adapters so the settle order (wait → dispose → outcome
/// lines → disposal report) and the exit-code contract stay identical;
/// the observable contract itself lives in [`machine`].
#[cfg(feature = "playback")]
fn finish_episode(mut episode: Episode) -> ExitCode {
    let outcome = episode.handle.wait_terminal();
    let snapshot = episode.runtime.dispose();
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
    for warning in machine::disposal_warnings(&snapshot) {
        eprintln!("{warning}");
    }
    machine::episode_exit_code(Some(outcome), snapshot.quiet)
}

#[cfg(not(feature = "playback"))]
fn run_playback(file: std::path::PathBuf, shell: Shell) -> ExitCode {
    let _ = (file, shell);
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
