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

use std::process::ExitCode;

#[cfg(feature = "playback")]
use std::path::PathBuf;

use qianqian_headless::cli::{self, Invocation};

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
        Ok(Invocation::Play { file }) => run_playback(file),
        Err(error) => {
            eprintln!("error: {error}");
            eprint!("{}", cli::usage());
            ExitCode::from(2)
        }
    }
}

#[cfg(feature = "playback")]
fn run_playback(file: PathBuf) -> ExitCode {
    use qianqian_playback::{SessionCompletion, SessionOutcome, playback_session_spec};

    let mut runtime = qianqian_app::QianqianApp::new();
    if let Err(e) = runtime.register_component(qianqian_decode_songcore::songcore_decode_plugin()) {
        eprintln!("decode plugin registration failed: {e:?}");
        return ExitCode::from(1);
    }
    if let Err(e) = runtime.register_component(qianqian_output_wasapi::wasapi_output_plugin()) {
        eprintln!("output plugin registration failed: {e:?}");
        return ExitCode::from(1);
    }
    let completion = SessionCompletion::new();
    if let Err(e) =
        runtime.register_component(playback_session_spec(file.clone(), completion.clone()))
    {
        eprintln!("session registration failed: {e:?}");
        return ExitCode::from(1);
    }

    if let Err(errors) = runtime.revise_desired(vec![
        desired("decode", "songcore_decode_plugin"),
        desired("output", "wasapi_output_plugin"),
        desired("session", "playback_session"),
    ]) {
        eprintln!("composition refused: {errors}");
        return ExitCode::from(2);
    }

    // revise_desired settles before returning: a failed activation is
    // visible in the snapshot, and there is no episode to wait for.
    let snapshot = runtime.composition_snapshot();
    if snapshot.fibers.get("session").map(|f| f.state)
        != Some(qianqian_composition::FiberState::Active)
    {
        if let Some(message) = completion.activation_error() {
            eprintln!("playback session failed to activate: {message}");
        } else {
            eprintln!(
                "playback session did not activate (a required capability provider \
                 failed or is missing on this platform)"
            );
        }
        let snapshot = runtime.dispose();
        report_disposal(&snapshot);
        return ExitCode::from(1);
    }

    if let Some(format) = completion.source_format() {
        println!(
            "source: {} Hz, {} channels, mask {:#x}",
            format.sample_rate, format.channels, format.channel_mask
        );
    }
    println!("playing {} ...", file.display());

    let outcome = completion.wait();
    let snapshot = runtime.dispose();
    match &outcome {
        SessionOutcome::Completed => {
            println!("EOF: played out completely");
        }
        SessionOutcome::Failed { stage } => {
            eprintln!("playback failed: {stage}");
        }
        SessionOutcome::Stopped => {
            println!("stopped before completion");
        }
    }
    report_disposal(&snapshot);
    match outcome {
        SessionOutcome::Completed | SessionOutcome::Stopped => {
            if snapshot.quiet {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        SessionOutcome::Failed { .. } => ExitCode::from(1),
    }
}

#[cfg(not(feature = "playback"))]
fn run_playback(file: std::path::PathBuf) -> ExitCode {
    let _ = file;
    eprintln!(
        "this binary was built without the playback slice; \
         rebuild with: cargo build --release --features playback"
    );
    ExitCode::from(2)
}

#[cfg(feature = "playback")]
fn report_disposal(snapshot: &qianqian_composition::CompositionSnapshot) {
    if snapshot.quiet {
        return;
    }
    eprintln!("warning: disposal reported a latched teardown violation");
    for (name, fiber) in &snapshot.fibers {
        if fiber.teardown_violated {
            eprintln!(
                "  fiber '{name}': teardown violated (state {:?})",
                fiber.state
            );
        }
    }
}

#[cfg(feature = "playback")]
fn desired(id: &str, component: &'static str) -> qianqian_composition::DesiredEntry {
    qianqian_composition::DesiredEntry::enabled(
        id,
        component,
        qianqian_composition::Revision::new(1),
    )
}
