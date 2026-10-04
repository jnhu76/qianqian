use super::*;
use crate::assembly::{AssemblyOutcome, establish_specs_for_test};
use qianqian_audio_api::ports::{AudioOutputCapability, PcmDecodeCapability};
use qianqian_composition::{ComponentSpec, Discharge};
use qianqian_playback::{
    AudioProcessingConfig, EpisodeTerminalOutcome, EstablishmentResult,
    playback_session_spec_with_establishment,
};
use std::cell::{Cell, RefCell};
use std::io::{BufRead, Cursor};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::assembly::mechanisms::{OutputBehavior, SourceBehavior, TestDecode, TestOutput};

// Real Session activation/worker/PCM/render/terminal/disposal paths, with test
// mechanisms at the provider seams. These are NOT physical backend evidence.
fn episode(
    source: SourceBehavior,
    paused: bool,
    host: Option<Rc<RefCell<Option<HostInput>>>>,
) -> (AssemblyOutcome, Rc<Cell<bool>>) {
    let disposed = Rc::new(Cell::new(false));
    let marker = disposed.clone();
    let decode = ComponentSpec::new("decode")
        .provides::<PcmDecodeCapability>()
        .on_activate(move |ctx| {
            ctx.provide::<PcmDecodeCapability>(Rc::new(TestDecode::new(source)))
                .unwrap();
            Ok(())
        });
    let output = ComponentSpec::new("output")
        .provides::<AudioOutputCapability>()
        .on_activate(move |ctx| {
            let marker = marker.clone();
            let host = host.clone();
            ctx.register_effect(move || {
                if let Some(host) = &host {
                    let host = host.borrow();
                    let input = host.as_ref().expect("test attached the invocation host");
                    assert!(
                        input.shared.state.try_lock().is_ok(),
                        "host lock across disposal"
                    );
                }
                marker.set(true);
                Discharge::Discharged
            });
            ctx.provide::<AudioOutputCapability>(Rc::new(TestOutput::new(OutputBehavior::Consume)))
                .unwrap();
            Ok(())
        });
    let handle = PlaybackSessionHandle::new();
    if paused {
        handle.request_pause();
    }
    let (session, attempt) = playback_session_spec_with_establishment(
        "test://input".into(),
        handle.clone(),
        AudioProcessingConfig::BYPASS,
    );
    let assembled = establish_specs_for_test(decode, output, session, attempt, handle);
    assert_eq!(
        assembled.start.establishment,
        EstablishmentResult::Established
    );
    (assembled, disposed)
}

fn no_output(_: ReportStream, _: &str) {
    panic!("unexpected reader output");
}
fn eof(input: &HostInput, handle: &PlaybackSessionHandle) {
    read_input(input, handle, |_| Ok(0), no_output);
}
fn read_error(input: &HostInput, handle: &PlaybackSessionHandle) {
    read_input(
        input,
        handle,
        |_| Err(io::Error::other("read failed")),
        no_output,
    );
}
fn read_panic(input: &HostInput, handle: &PlaybackSessionHandle) {
    // resume_unwind isolates the recovery boundary from the process panic hook.
    read_input(
        input,
        handle,
        |_| std::panic::resume_unwind(Box::new("reader panic")),
        no_output,
    );
}

fn wait_closed(input: &HostInput) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while input.is_open() {
        assert!(Instant::now() < deadline, "seal did not close admission");
        std::thread::yield_now();
    }
}

#[test]
fn eof_without_commands_naturally_completes_and_disposes() {
    let (assembled, disposed) = episode(SourceBehavior::EofAfter(16), false, None);
    let handle = assembled.start.handle.clone();
    assert_eq!(
        crate::entry::run_machine_reader_for_test(assembled, |input, handle| eof(&input, &handle)),
        std::process::ExitCode::SUCCESS
    );
    assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
    assert!(!handle.observe().stop_requested);
    assert!(disposed.get());
}

#[test]
fn commands_then_eof_adds_no_failure_or_implicit_stop() {
    let input = HostInput::default();
    let handle = PlaybackSessionHandle::new();
    let mut reader = Cursor::new("pause\nresume\nstatus\n");
    let mut reports = Vec::new();
    read_input(
        &input,
        &handle,
        |line| reader.read_line(line),
        |stream, line| reports.push((stream, line.to_owned())),
    );
    assert!(!handle.observe().pause_requested);
    assert!(!handle.observe().stop_requested);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].0, ReportStream::Stdout);
    assert_eq!(input.seal(), None);
}

#[test]
fn spawn_failure_stops_established_episode_and_returns_host_failure() {
    let (assembled, disposed) = episode(SourceBehavior::EofAfter(100_000), true, None);
    let handle = assembled.start.handle.clone();
    let exit = crate::entry::run_machine_reader_for_test(assembled, |input, handle| {
        start_reader(
            input.clone(),
            handle,
            |_| Err(io::Error::other("spawn failed")),
            || panic!("no reader exists"),
        );
        assert_eq!(
            input.shared.state.lock().unwrap().first_failure,
            Some(HostFailure::Spawn)
        );
    });
    assert_eq!(exit, std::process::ExitCode::from(1));
    assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
    assert!(disposed.get());
}

#[test]
fn read_and_panic_failures_stop_real_episode_without_forging_failed() {
    for (failure, inject) in [
        (
            HostFailure::Read,
            read_error as fn(&HostInput, &PlaybackSessionHandle),
        ),
        (HostFailure::Panic, read_panic),
    ] {
        let host = Rc::new(RefCell::new(None));
        let (assembled, disposed) =
            episode(SourceBehavior::EofAfter(100_000), true, Some(host.clone()));
        let handle = assembled.start.handle.clone();
        let exit = crate::entry::run_machine_reader_for_test(assembled, |actual, handle| {
            *host.borrow_mut() = Some(actual.clone());
            inject(&actual, &handle);
            assert_eq!(
                actual.shared.state.lock().unwrap().first_failure,
                Some(failure)
            );
        });
        assert_eq!(exit, std::process::ExitCode::from(1));
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
        assert!(handle.observe().failure_diagnostic.is_none());
        assert!(disposed.get());
    }
}

#[test]
fn outer_reader_panic_boundary_records_failure_without_join() {
    let input = HostInput::default();
    let handle = PlaybackSessionHandle::new();
    let (finished, receive) = mpsc::channel();
    start_reader(
        input.clone(),
        handle.clone(),
        move |task| {
            Ok(std::thread::spawn(move || {
                task();
                finished.send(()).unwrap();
            }))
        },
        || std::panic::resume_unwind(Box::new("boundary panic")),
    );
    receive.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(input.seal(), Some(HostFailure::Panic));
    assert!(handle.observe().stop_requested);
}

#[test]
fn preseal_failure_preserves_natural_completed_and_runtime_failed() {
    for (source, terminal) in [
        (
            SourceBehavior::EofAfter(16),
            EpisodeTerminalOutcome::Completed,
        ),
        (SourceBehavior::FailAfter(0), EpisodeTerminalOutcome::Failed),
    ] {
        for failure_first in [false, true] {
            let host = Rc::new(RefCell::new(None));
            let (assembled, disposed) = episode(source, false, Some(host.clone()));
            let handle = assembled.start.handle.clone();
            let exit = crate::entry::run_machine_reader_for_test(assembled, |input, _| {
                *host.borrow_mut() = Some(input.clone());
                let operation = input.admit().unwrap();
                if failure_first {
                    // Hold at the real record/response cut. Natural authority
                    // settlement wins D11 before the Stop response runs.
                    operation.fail_with_response(HostFailure::Read, || {
                        assert_eq!(handle.wait_terminal(), terminal);
                        assert!(
                            input.shared.state.try_lock().is_ok(),
                            "host lock across terminal wait"
                        );
                        handle.request_stop();
                    });
                } else {
                    assert_eq!(handle.wait_terminal(), terminal);
                    operation.fail(HostFailure::Read, &handle);
                    assert!(
                        !handle.observe().stop_requested,
                        "no response needed after terminal"
                    );
                }
                drop(operation);
            });
            assert_eq!(exit, std::process::ExitCode::from(1));
            assert_eq!(handle.wait_terminal(), terminal);
            assert!(disposed.get());
            assert_eq!(
                handle.observe().failure_diagnostic.is_some(),
                terminal == EpisodeTerminalOutcome::Failed
            );
        }
    }
}

#[test]
fn existing_user_stop_and_host_failure_keep_one_genuine_terminal() {
    let (assembled, disposed) = episode(SourceBehavior::EofAfter(100_000), true, None);
    let handle = assembled.start.handle.clone();
    handle.request_stop();
    let exit = crate::entry::run_machine_reader_for_test(assembled, |input, handle| {
        read_error(&input, &handle)
    });
    assert_eq!(exit, std::process::ExitCode::from(1));
    assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
    assert!(disposed.get());
}

#[test]
fn failure_is_recorded_before_response_and_lock_is_released() {
    let input = HostInput::default();
    let operation = input.admit().unwrap();
    operation.fail_with_response(HostFailure::Read, || {
        let state = input
            .shared
            .state
            .try_lock()
            .expect("bookkeeping lock across response");
        assert_eq!(
            state.first_failure,
            Some(HostFailure::Read),
            "record before response"
        );
    });
    drop(operation);
    assert_eq!(input.seal(), Some(HostFailure::Read));
}

#[test]
fn first_failure_storage_is_bounded_and_sealed_result_immutable() {
    let input = HostInput::default();
    let handle = PlaybackSessionHandle::new();
    input.failure(HostFailure::Spawn, &handle);
    for _ in 0..1000 {
        input.failure(HostFailure::Read, &handle);
    }
    assert_eq!(input.seal(), Some(HostFailure::Spawn));
    for _ in 0..1000 {
        input.failure(HostFailure::Panic, &handle);
    }
    assert_eq!(input.seal(), Some(HostFailure::Spawn));
}

#[test]
fn seal_waits_for_admitted_report_ack_and_rejects_fresh_work() {
    let input = HostInput::default();
    let (entered, started) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let worker_input = input.clone();
    let worker = std::thread::spawn(move || {
        let handle = PlaybackSessionHandle::new();
        assert!(worker_input.line("status", &handle, &mut |_, _| {
            assert!(
                worker_input.shared.state.try_lock().is_ok(),
                "host lock across blocking output"
            );
            entered.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
        }));
    });
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let sealing_input = input.clone();
    let (sealed, result) = mpsc::channel();
    let sealer = std::thread::spawn(move || sealed.send(sealing_input.seal()).unwrap());
    wait_closed(&input);
    assert!(
        matches!(result.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "seal overtook output acknowledgement"
    );
    assert!(!input.line("stop", &PlaybackSessionHandle::new(), &mut no_output));
    release.send(()).unwrap();
    assert_eq!(result.recv_timeout(Duration::from_secs(5)).unwrap(), None);
    worker.join().unwrap();
    sealer.join().unwrap();
}

#[test]
fn admitted_output_panic_records_before_ack_even_during_seal() {
    let input = HostInput::default();
    let (entered, started) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let worker_input = input.clone();
    let handle = PlaybackSessionHandle::new();
    let worker_handle = handle.clone();
    let worker = std::thread::spawn(move || {
        assert!(!worker_input.line("status", &worker_handle, &mut |_, _| {
            entered.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
            std::panic::resume_unwind(Box::new("output panic"));
        }));
    });
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let sealing_input = input.clone();
    let sealer = std::thread::spawn(move || sealing_input.seal());
    wait_closed(&input);
    release.send(()).unwrap();
    assert_eq!(sealer.join().unwrap(), Some(HostFailure::Panic));
    worker.join().unwrap();
    assert!(handle.observe().stop_requested);
}

#[test]
fn admitted_failure_response_ack_cannot_be_overtaken_by_seal() {
    let input = HostInput::default();
    let operation = input.admit().unwrap();
    let sealing_input = input.clone();
    let (sealed, result) = mpsc::channel();
    let sealer = std::thread::spawn(move || sealed.send(sealing_input.seal()).unwrap());
    wait_closed(&input);
    operation.fail_with_response(HostFailure::Read, || {
        assert!(
            matches!(result.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "seal overtook failure response"
        );
        assert!(input.shared.state.try_lock().is_ok());
    });
    drop(operation);
    assert_eq!(
        result.recv_timeout(Duration::from_secs(5)).unwrap(),
        Some(HostFailure::Read)
    );
    sealer.join().unwrap();
}

#[test]
fn postseal_commands_reports_and_failures_are_inert() {
    let input = HostInput::default();
    let handle = PlaybackSessionHandle::new();
    assert_eq!(input.seal(), None);
    for line in ["stop", "pause", "resume", "seek 1", "status", "bad input"] {
        assert!(!input.line(line, &handle, &mut no_output));
    }
    for failure in [HostFailure::Spawn, HostFailure::Read, HostFailure::Panic] {
        input.failure(failure, &handle);
    }
    assert!(!handle.observe().stop_requested);
    assert!(!handle.observe().pause_requested);
    assert_eq!(input.seal(), None);
}

#[test]
fn finite_episode_returns_while_stdin_blocked_and_late_wake_is_inert() {
    for event in ["stop\n", "status\n", "read error", "panic"] {
        let (assembled, disposed) = episode(SourceBehavior::EofAfter(16), false, None);
        let handle = assembled.start.handle.clone();
        let (entered, started) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let (finished, ended) = mpsc::channel();
        let (save, saved) = mpsc::channel();
        let exit = crate::entry::run_machine_reader_for_test(assembled, |input, handle| {
            save.send(input.clone()).unwrap();
            let read_input_host = input.clone();
            let read_handle = handle.clone();
            start_reader(
                input,
                handle,
                |task| std::thread::Builder::new().spawn(task),
                move || {
                    read_input(
                        &read_input_host,
                        &read_handle,
                        |line| {
                            assert!(
                                read_input_host.shared.state.try_lock().is_ok(),
                                "host lock across stdin"
                            );
                            entered.send(()).unwrap();
                            resume.recv_timeout(Duration::from_secs(5)).unwrap();
                            match event {
                                "read error" => Err(io::Error::other("late read error")),
                                "panic" => std::panic::resume_unwind(Box::new("late panic")),
                                _ => {
                                    line.push_str(event);
                                    Ok(event.len())
                                }
                            }
                        },
                        no_output,
                    );
                    finished.send(()).unwrap();
                },
            );
            started.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        assert_eq!(exit, std::process::ExitCode::SUCCESS);
        assert!(disposed.get());
        assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Completed);
        assert!(
            matches!(ended.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "reader is still blocked when host returns"
        );
        let input = saved.recv().unwrap();
        release.send(()).unwrap();
        ended.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!handle.observe().stop_requested);
        assert_eq!(input.seal(), None);
    }
}

#[test]
fn final_owner_reports_preserve_terminal_and_distinct_failure_domains() {
    for (source, paused, terminal) in [
        (
            SourceBehavior::EofAfter(16),
            false,
            EpisodeTerminalOutcome::Completed,
        ),
        (
            SourceBehavior::FailAfter(0),
            false,
            EpisodeTerminalOutcome::Failed,
        ),
        (
            SourceBehavior::EofAfter(100_000),
            true,
            EpisodeTerminalOutcome::Stopped,
        ),
    ] {
        let (assembled, disposed) = episode(source, paused, None);
        let handle = assembled.start.handle.clone();
        if !paused {
            assert_eq!(handle.wait_terminal(), terminal);
        }
        let host = RefCell::new(None::<HostInput>);
        let mut reports = Vec::new();
        let exit = crate::entry::run_machine_reports_for_test(
            assembled,
            |input, handle| {
                *host.borrow_mut() = Some(input.clone());
                read_error(&input, &handle);
            },
            |stream, line| {
                let host = host.borrow();
                let state = host
                    .as_ref()
                    .unwrap()
                    .shared
                    .state
                    .try_lock()
                    .expect("no host lock across final output");
                assert!(
                    state.admission_closed && !state.operation_active,
                    "final output after seal"
                );
                reports.push((stream, line.to_owned()));
            },
        );
        assert_eq!(exit, std::process::ExitCode::from(1));
        assert_eq!(handle.wait_terminal(), terminal);
        assert!(disposed.get());
        assert_eq!(reports.len(), 2);
        assert_eq!(
            reports[0],
            (ReportStream::Stderr, HostFailure::Read.report().into())
        );
        let diagnostic = handle.observe().failure_diagnostic;
        assert_eq!(
            &reports[1..],
            crate::machine::outcome_report(terminal, diagnostic.as_deref())
        );
    }
}

/// A subprocess isolates the process hook from parallel tests and captures its
/// actual stderr. This helper is inert in the normal test invocation.
#[test]
fn reader_panic_hook_subprocess_probe() {
    let Ok(phase) = std::env::var("QIANQIAN_C2_PANIC_PROBE") else {
        return;
    };
    std::panic::set_hook(Box::new(|info| eprintln!("previous-hook: {info}")));
    install_reader_panic_hook();
    install_reader_panic_hook(); // repeated host setup must not wrap again
    if phase == "unrelated" {
        assert!(
            std::thread::Builder::new()
                .name("unrelated-reader".into())
                .spawn(|| panic!("unrelated-runtime-panic"))
                .unwrap()
                .join()
                .is_err()
        );
        return;
    }
    let input = HostInput::default();
    let handle = PlaybackSessionHandle::new();
    let (entered, started) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let (finished, ended) = mpsc::channel();
    let reader_input = input.clone();
    let reader_handle = handle.clone();
    start_reader(
        input.clone(),
        handle.clone(),
        move |task| {
            std::thread::Builder::new()
                .name(READER_THREAD_NAME.into())
                .spawn(move || {
                    task();
                    finished.send(()).unwrap();
                })
        },
        move || {
            read_input(
                &reader_input,
                &reader_handle,
                |_| {
                    entered.send(()).unwrap();
                    resume.recv_timeout(Duration::from_secs(5)).unwrap();
                    panic!("owned-reader-real-panic");
                },
                no_output,
            );
        },
    );
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    if phase == "postseal" {
        assert_eq!(input.seal(), None);
    }
    release.send(()).unwrap();
    ended.recv_timeout(Duration::from_secs(5)).unwrap();
    let failure = input.seal();
    if phase == "preseal" {
        assert_eq!(failure, Some(HostFailure::Panic));
        assert!(handle.observe().stop_requested);
        eprintln!("{}", failure.unwrap().report());
    } else {
        assert_eq!(failure, None);
        assert!(!handle.observe().stop_requested);
    }
}

#[test]
fn real_reader_panic_hook_is_silent_after_seal_and_other_hooks_preserved() {
    for phase in ["preseal", "postseal", "unrelated"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "machine_input::tests::reader_panic_hook_subprocess_probe",
                "--nocapture",
            ])
            .env("QIANQIAN_C2_PANIC_PROBE", phase)
            .output()
            .unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(output.status.success(), "probe {phase}: {stderr}");
        match phase {
            "preseal" => assert_eq!(stderr.trim(), HostFailure::Panic.report()),
            "postseal" => assert!(
                stderr.is_empty(),
                "late panic output crossed seal: {stderr}"
            ),
            _ => {
                assert!(stderr.contains("unrelated-runtime-panic"));
                assert_eq!(stderr.matches("previous-hook:").count(), 1);
            }
        }
    }
}

#[test]
fn read_error_records_and_stops_before_terminal_wait() {
    let input = HostInput::default();
    let handle = PlaybackSessionHandle::new();
    read_error(&input, &handle);
    assert_eq!(input.seal(), Some(HostFailure::Read));
    assert!(handle.observe().stop_requested);
    assert!(
        handle.observe().terminal_outcome.is_none(),
        "host cannot forge terminal"
    );
}
