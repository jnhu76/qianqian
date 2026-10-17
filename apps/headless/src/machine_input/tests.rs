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
                }
                drop(operation);
            });
            assert_eq!(exit, std::process::ExitCode::from(1));
            assert_eq!(handle.wait_terminal(), terminal);
            assert!(disposed.get());
            // D14.2 truth-class boundary (PlaybackSessionObservation):
            // Completed/Stopped observations never carry a failure
            // diagnostic, while a Failed observation may or may not
            // publish one — presence is presentation, not semantics, so
            // it is not pinned here. The diagnostic-to-report rendering
            // oracle is owned by
            // final_owner_reports_preserve_terminal_and_distinct_failure_domains.
            if terminal == EpisodeTerminalOutcome::Completed {
                assert!(handle.observe().failure_diagnostic.is_none());
            }
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
fn seal_retains_admitted_postclosure_output_failure_and_rejects_fresh_work() {
    for record_failure in [false, true] {
        std::thread::scope(|scope| {
            // Scope owns both threads; on assertion unwind this local release
            // sender drops before automatic joins and wakes the blocked output.
            let input = HostInput::default();
            let (entered, started) = mpsc::channel();
            let (release, resume) = mpsc::channel();
            let worker_input = input.clone();
            let worker = scope.spawn(move || {
                let handle = PlaybackSessionHandle::new();
                let completed = worker_input.line("status", &handle, &mut |_, _| {
                    assert!(
                        worker_input.shared.state.try_lock().is_ok(),
                        "host lock across blocking output"
                    );
                    entered.send(()).unwrap();
                    resume.recv_timeout(Duration::from_secs(5)).unwrap();
                    if record_failure {
                        // An admitted operation keeps its failure-record right
                        // through admission closure; an output unwind is the
                        // test-reachable way to record from inside the blocked
                        // report (line's catcher owns it before Drop acks).
                        std::panic::resume_unwind(Box::new("output panic"));
                    }
                });
                assert_eq!(completed, !record_failure);
            });
            started.recv_timeout(Duration::from_secs(5)).unwrap();
            let sealing_input = input.clone();
            let (sealed, result) = mpsc::channel();
            let sealer = scope.spawn(move || {
                let _ = sealed.send(sealing_input.seal());
            });
            // Premise: the sealer has executed `admission_closed = true` while
            // the admitted report is still blocked. wait_closed takes the same
            // bookkeeping mutex, so this observation is ordered after the
            // sealer's whole pre-wait critical section — including, for a seal
            // that skips the acknowledgement wait, its first_failure read.
            wait_closed(&input);
            // Fresh work must not overtake the drain while seal is in flight.
            let fresh_handle = PlaybackSessionHandle::new();
            assert!(!input.line("stop", &fresh_handle, &mut no_output));
            assert!(
                !fresh_handle.observe().stop_requested,
                "rejected fresh work must not route Stop"
            );
            release.send(()).unwrap();
            // VALUE witnesses retention of a postclosure admitted failure:
            // a seal reading before this record returns None. The false branch
            // alone cannot witness drain, and neither value distinguishes reading
            // after the record but before response/Drop completion. Full ACK
            // ordering remains a separate obligation (execution-model D3/D5).
            let expected = record_failure.then_some(HostFailure::Panic);
            assert_eq!(
                result.recv_timeout(Duration::from_secs(5)).unwrap(),
                expected,
                "seal lost the admitted postclosure output failure"
            );
            worker.join().unwrap();
            sealer.join().unwrap();
        });
    }
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
    install_panic_hook_posture();
    install_panic_hook_posture(); // repeated host setup must not wrap again
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
            // The production reader is class α from its first statement, and
            // the hook's isolation predicate IS that registration — the harness
            // reader mirrors the production shape so the posture it witnesses
            // is the production posture.
            let _registration = qianqian_diagnostics::AlphaRegistration::register();
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

// Only test builds carry these two atomics. The response marker is written by
// the fixture's response owner, independently of production ACK bookkeeping.
// First capture wins: later Drop/seal calls cannot repair an illegal L snapshot.
#[derive(Default)]
pub(super) struct SealWitness {
    response_completed: std::sync::atomic::AtomicBool,
    snapshot: std::sync::atomic::AtomicU8,
}

impl SealWitness {
    pub(super) fn capture(&self, operation_active: bool) {
        use std::sync::atomic::Ordering;
        let response_completed = self.response_completed.load(Ordering::Acquire);
        let bits = 1 | (u8::from(operation_active) << 1) | (u8::from(response_completed) << 2);
        let _ = self
            .snapshot
            .compare_exchange(0, bits, Ordering::Release, Ordering::Relaxed);
    }
}

// PROPERTY/AUTHORITY: D3/D5 execution-model §8.2/§9, response R -> Drop ACK K
// -> final read L -> return T. C5 additionally fixes F before R; Some(Read)
// cannot mask an early L. No physical reader/TLS/pipe-consumer claim.
// PRECONDITION: actual admission; channel proves callback entry (or returned
// response with a still-owned guard); mutex observation proves closure while
// the owner is held. The closure observer acquires the SAME mutex: a no-wait
// sealer must already have captured L before it can expose closure to us.
// LEGAL OUTCOME/OBSERVATION: at real L, R is complete AND active is false;
// capture 0 is missing evidence and fails. Results alone are not the oracle.
// FALSIFIERS: M-S skips the wait; M-F waits only while failure is absent.
// EXPECTED RED: immutable snapshot is 3 (R/K pending) or 7 (K pending), rather
// than 5. RESTORED GREEN and actual mutation identities live in PR evidence.
fn witness_response_and_drop_before_seal(failure: bool, held_guard: bool) {
    use std::sync::atomic::Ordering;
    std::thread::scope(|scope| {
        let input = HostInput::default();
        let (entered, started) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        // This sender drops on unwind before scope joins. Worker treats sender
        // disappearance as release; all target assertions run after both joins.
        let worker_input = input.clone();
        let worker = scope.spawn(move || {
            let response = || {
                if !held_guard {
                    entered.send(()).unwrap();
                    let _ = resume.recv();
                }
                worker_input
                    .shared
                    .seal_witness
                    .response_completed
                    .store(true, Ordering::Release);
            };
            if failure || held_guard {
                let operation = worker_input.admit().unwrap();
                if failure {
                    operation.fail_with_response(HostFailure::Read, response);
                } else {
                    // Normal legal ownership: completed output does not release
                    // the admitted guard; only its actual Drop publishes ACK.
                    response();
                }
                if held_guard {
                    entered.send(()).unwrap();
                    let _ = resume.recv();
                }
                drop(operation);
            } else {
                let handle = PlaybackSessionHandle::new();
                assert!(worker_input.line("status", &handle, &mut |_, _| response()));
            }
        });
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        {
            let state = input.shared.state.lock().unwrap();
            assert!(state.operation_active);
            assert_eq!(state.first_failure, failure.then_some(HostFailure::Read));
            assert_eq!(
                input
                    .shared
                    .seal_witness
                    .response_completed
                    .load(Ordering::Acquire),
                held_guard
            );
        }
        let sealing_input = input.clone();
        let sealer = scope.spawn(move || sealing_input.seal());
        wait_closed(&input);
        // No try_recv/try_lock Oracle. The snapshot is taken at L itself, so
        // sealer preemption after return cannot hide an early read. Closure
        // under the shared mutex orders the illegal L before this release.
        release.send(()).unwrap();
        worker.join().unwrap();
        let result = sealer.join().unwrap();
        assert_eq!(
            input.shared.seal_witness.snapshot.load(Ordering::Acquire),
            5,
            "seal final read preceded response completion or Drop ACK"
        );
        assert_eq!(result, failure.then_some(HostFailure::Read));
    });
}

#[test]
fn seal_final_read_witnesses_normal_response_and_drop_ack() {
    witness_response_and_drop_before_seal(false, false);
}

#[test]
fn seal_final_read_witnesses_failure_response_and_drop_ack() {
    witness_response_and_drop_before_seal(true, false);
}

#[test]
fn seal_final_read_witnesses_normal_returned_response_with_held_guard() {
    witness_response_and_drop_before_seal(false, true);
}

#[test]
fn seal_final_read_witnesses_failure_returned_response_with_held_guard() {
    witness_response_and_drop_before_seal(true, true);
}

// --- DG-4b: the panic-hook posture, witnessed in a subprocess ---------------

/// Child-process selector for the DG-4b witness below.
const DG4B_SCENARIO: &str = "QIANQIAN_DG4B_SCENARIO";
/// The test-owned previous hook's only output. "The chain ran" is then one
/// unambiguous stderr line instead of a guess about a hook's behavior.
const DG4B_MARKER: &str = "DG4B-PREVIOUS-HOOK";

/// Child entry for the DG-4b witness. The hook posture is a process-wide
/// ONCE-installed setting whose previous hook is captured at install time, so
/// only a fresh process can witness either; this test re-executes its own
/// binary with `DG4B_SCENARIO` set, and does nothing in the parent's run.
///
/// Scenarios (all deterministic: one spawned thread, panics caught inside it):
///
/// * `critical` — the class-α (registered) reader thread panics TWICE with a
///   marker previous hook installed: isolation says the previous hook must never
///   run on it, and the one-shot latch says the first panic publishes while the
///   second is counted but does not take the pool's second slot.
/// * `default` — the production configuration (the std default hook is the
///   previous one): the same thread panics once, and the default print must not
///   appear at all.
/// * `registered-bystander` — a REGISTERED class-α thread under a name that was
///   never in the old name list panics once with the marker hook installed: it
///   must be isolated. This is the scenario that witnesses the predicate being
///   the registration fact (an unregistered thread of the same shape chains).
/// * `named-unregistered` — a thread wearing the reader's NAME without
///   registering panics once with the marker hook installed: it must CHAIN. A
///   name is not an isolation obligation.
/// * `non-critical` — an unregistered bystander panics once with the marker hook
///   installed: chaining must reach the previous hook. This is also the
///   harness-sight control — it proves the parent can see the previous-hook
///   output it asserts is absent in the other scenarios.
#[test]
fn dg4b_child_witness() {
    let Ok(scenario) = std::env::var(DG4B_SCENARIO) else {
        return;
    };
    // `registered` decides the scenario's class-α registration, `panics` the
    // number of injected panics, `marker_hook` the previous hook. The names are
    // deliberately mixed against the old name-list: `registered-bystander`
    // carries a non-critical NAME while registering, and `named-unregistered`
    // carries the reader's name while NOT registering — the pair is what pins
    // that the posture keys on the registration fact, not on a thread name.
    let (thread_name, registered, panics, marker_hook) = match scenario.as_str() {
        "critical" => (READER_THREAD_NAME, true, 2usize, true),
        "default" => (READER_THREAD_NAME, true, 1usize, false),
        "registered-bystander" => ("qianqian-dg4b-registered", true, 1usize, true),
        "named-unregistered" => (READER_THREAD_NAME, false, 1usize, true),
        "non-critical" => ("qianqian-dg4b-bystander", false, 1usize, true),
        other => panic!("unknown DG-4b scenario {other:?}"),
    };
    if marker_hook {
        // The previous hook must be in place BEFORE the posture installs, so
        // the install captures it.
        std::panic::set_hook(Box::new(|_| eprintln!("{DG4B_MARKER}")));
    }
    install_panic_hook_posture();
    let witness = std::thread::Builder::new()
        .name(thread_name.into())
        .spawn(move || {
            let registration = registered.then(qianqian_diagnostics::AlphaRegistration::register);
            for _ in 0..panics {
                let _ = catch_unwind(|| panic!("dg4b injected panic"));
            }
            registration.map_or(0, |registration| registration.outstanding())
        })
        .expect("spawn witness thread");
    let outstanding = witness
        .join()
        .expect("the witness thread catches its own panics");
    eprintln!("DG4B-PANICS={}", qianqian_diagnostics::critical_panics());
    eprintln!("DG4B-OUTSTANDING={outstanding}");
}

#[test]
fn dg4b_the_hook_isolates_critical_threads_and_chains_elsewhere() {
    let exe = std::env::current_exe().expect("test binary path");
    let run = |scenario: &str| {
        let output = std::process::Command::new(&exe)
            .args([
                "--exact",
                "machine_input::tests::dg4b_child_witness",
                "--nocapture",
            ])
            .env(DG4B_SCENARIO, scenario)
            .output()
            .expect("run the witness child");
        assert!(
            output.status.success(),
            "witness child {scenario} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stderr).into_owned()
    };

    // Isolation + one-shot latch: the previous hook is the marker hook and must
    // not run; the first panic publishes, the second is counted only.
    let critical = run("critical");
    assert!(
        !critical.contains(DG4B_MARKER),
        "the previous hook ran on a registered critical thread: {critical}"
    );
    assert!(
        critical.contains("DG4B-PANICS=2"),
        "both critical panics must be counted: {critical}"
    );
    assert!(
        critical.contains("DG4B-OUTSTANDING=1"),
        "exactly one panic record may occupy the pool: {critical}"
    );

    // The production configuration: the std default hook is the previous one,
    // and its print must not appear for a critical-thread panic.
    let default = run("default");
    assert!(
        !default.contains("panicked at") && !default.contains(READER_THREAD_NAME),
        "the default hook printed for a critical-thread panic: {default}"
    );
    assert!(
        default.contains("DG4B-PANICS=1") && default.contains("DG4B-OUTSTANDING=1"),
        "one panic, one record: {default}"
    );

    // Chaining off isolation-obligated threads, and the harness-sight control:
    // the marker the other scenarios assert absent is observable here.
    let bystander = run("non-critical");
    assert!(
        bystander.contains(DG4B_MARKER),
        "the previous hook did not run off an isolation-obligated thread: {bystander}"
    );
    assert!(
        bystander.contains("DG4B-PANICS=0") && bystander.contains("DG4B-OUTSTANDING=0"),
        "an unregistered thread's panic is not isolated bookkeeping: {bystander}"
    );

    // The predicate is the REGISTRATION, not the thread name, in both
    // directions: a registered thread whose name was never in the old name list
    // is isolated, and a thread wearing the reader's name without registering
    // chains.
    let registered = run("registered-bystander");
    assert!(
        !registered.contains(DG4B_MARKER),
        "a registered class-α thread was not isolated: {registered}"
    );
    assert!(
        registered.contains("DG4B-PANICS=1") && registered.contains("DG4B-OUTSTANDING=1"),
        "the registered bystander's panic is isolated bookkeeping: {registered}"
    );
    let named = run("named-unregistered");
    assert!(
        named.contains(DG4B_MARKER),
        "a name alone granted isolation: {named}"
    );
    assert!(
        named.contains("DG4B-PANICS=0"),
        "an unregistered reader-named thread is not critical bookkeeping: {named}"
    );
}

// --- DG-4b x blocked stderr: the isolation obligation, witnessed -----------------
//
// The posture's isolation predicate is the diagnostic outlet's own thread fact
// (`is_alpha_producer`). This pair of subprocess scenarios witnesses WHY that
// matters independently of any argument: with the process stderr FULL (a pipe
// whose reader never reads) and the process stderr lock therefore held by a
// thread blocked inside a write, a panic on a registered class-α thread must
// still unwind and the thread must still end — because another thread's
// join/completion path waits on exactly that (the session teardown joins the
// decode worker; the render leg's catch must reach the failure-publication
// cone). A panic that entered the previous hook instead would block on that
// lock BEFORE `catch_unwind` and the join would never return.

const DG4B_BLOCKED_SCENARIO: &str = "QIANQIAN_DG4B_BLOCKED_SCENARIO";
/// The chaining scenario's previous hook prints this to STDOUT — explicitly not
/// to stderr, so chaining is observable even while stderr is blocked.
const DG4B_BLOCKED_MARKER: &str = "DG4B-BLOCKED-PREVIOUS-HOOK";

/// Fill the process stderr from a helper thread until it BLOCKS inside a write,
/// and return only once that stall is witnessed: at least one chunk completed
/// (the pipe really swallowed data), a chunk is in flight, no chunk completes
/// for a whole window, and no write ever failed (a failing write would mean the
/// parent closed the pipe instead of merely not reading it).
fn block_process_stderr() -> usize {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let completed = std::sync::Arc::new(AtomicUsize::new(0));
    let in_flight = std::sync::Arc::new(AtomicBool::new(false));
    let errored = std::sync::Arc::new(AtomicBool::new(false));
    let (entered, ready) = mpsc::channel();
    let (c, f, e) = (
        std::sync::Arc::clone(&completed),
        std::sync::Arc::clone(&in_flight),
        std::sync::Arc::clone(&errored),
    );
    std::thread::spawn(move || {
        use std::io::Write;
        // 64 KiB per write: the pipe takes at most a couple of these, so the
        // thread sits inside a blocking write almost immediately.
        let chunk = vec![b'.'; 64 * 1024];
        entered.send(()).unwrap();
        loop {
            f.store(true, Ordering::SeqCst);
            if std::io::stderr().write_all(&chunk).is_err() {
                e.store(true, Ordering::SeqCst);
                return;
            }
            f.store(false, Ordering::SeqCst);
            c.fetch_add(1, Ordering::SeqCst);
        }
    });
    ready.recv().expect("the filler started");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let before = completed.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(250));
        let after = completed.load(Ordering::SeqCst);
        assert!(
            !errored.load(Ordering::SeqCst),
            "stderr was not writable: nothing to block"
        );
        if before > 0 && before == after && in_flight.load(Ordering::SeqCst) {
            return before;
        }
        assert!(
            Instant::now() < deadline,
            "stderr never filled (chunks completed: {after})"
        );
    }
}

/// Child entry. Inert without the scenario variable; otherwise runs the scenario
/// and exits explicitly (the harness must not print to the blocked stderr).
#[test]
fn dg4b_blocked_stderr_child_witness() {
    let Ok(scenario) = std::env::var(DG4B_BLOCKED_SCENARIO) else {
        return;
    };
    let chunks = block_process_stderr();
    match scenario.as_str() {
        "isolated" => {
            install_panic_hook_posture();
            // The production decode-worker shape: registered at thread start,
            // panics twice, both caught. Its unwind is what the session
            // teardown's join waits for.
            let caught = std::thread::Builder::new()
                .name("qianqian-decode".into())
                .spawn(|| {
                    let _registration = qianqian_diagnostics::AlphaRegistration::register();
                    let mut caught = 0usize;
                    for _ in 0..2 {
                        if catch_unwind(|| panic!("dg4b blocked-pipe panic")).is_err() {
                            caught += 1;
                        }
                    }
                    caught
                })
                .expect("spawn the registered witness thread")
                .join()
                .expect("an isolated panic must unwind and the thread must end");
            println!("BLOCKED-ISOLATED-CAUGHT={caught}");
            println!("BLOCKED-PANICS={}", qianqian_diagnostics::critical_panics());
            println!("BLOCKED-FILLED-CHUNKS={chunks}");
        }
        "chaining" => {
            // Sight control: an UNREGISTERED thread chains to the previous hook
            // — the isolation above is the registration, not the blockage. The
            // marker goes to stdout, so this control cannot itself hang.
            std::panic::set_hook(Box::new(|_| println!("{DG4B_BLOCKED_MARKER}")));
            install_panic_hook_posture();
            let _ = std::thread::Builder::new()
                .name("qianqian-dg4b-bystander".into())
                .spawn(|| {
                    let _ = catch_unwind(|| panic!("dg4b chaining panic"));
                })
                .expect("spawn the bystander")
                .join();
            println!("BLOCKED-CHAINED=1");
        }
        other => panic!("unknown blocked-stderr scenario {other:?}"),
    }
    std::process::exit(0);
}

/// Run one blocked-stderr child: its stderr read end stays OPEN and UNREAD, so
/// the child's stderr really is a blocked sink. The child must exit on its own
/// inside the deadline.
fn run_blocked_stderr_child(scenario: &str) -> (std::process::ExitStatus, String) {
    use std::io::Read;
    let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "machine_input::tests::dg4b_blocked_stderr_child_witness",
            "--nocapture",
        ])
        .env(DG4B_BLOCKED_SCENARIO, scenario)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn the blocked-stderr child");
    let mut stdout = child.stdout.take().expect("stdout pipe");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match child.try_wait().expect("wait on the child") {
            Some(status) => {
                let mut text = String::new();
                let _ = stdout.read_to_string(&mut text);
                return (status, text);
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "the blocked-stderr child ({scenario}) never exited: a panic on an \
                     isolation-obligated thread waited on the blocked sink"
                );
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

// --- E-9 / E-8 (#290): machine protocol write-failure conformance ------------
//
// The reader's protocol write is a HOST OPERATION (migration §1.7): its
// failure follows the documented D3 classes (catch → HostFailure::Panic →
// record → conditional Stop → ack → seal → exit contribution), never a
// logging-inertness oracle. The main-thread report sites have NO reader
// catch: a write panic unwinds to the process (exit 101), and at the
// post-disposal report sites it skips only later reports. The two classes
// must diverge exactly as classified (reconciliation §8 E-9/E-8).
//
// Subprocess isolation follows the DG-4b pattern: children re-execute this
// test binary with an env selector under `--nocapture` (so test writes reach
// the real process fds, not libtest capture buffers), synchronize with the
// parent through a marker on the stream that stays healthy, and
// `process::exit(0)` explicitly whenever a stream is still broken at test
// end — the harness's own result print must not panic into a broken stream.

/// Child-process selector for the E-9 real-reader scenarios.
#[cfg(unix)]
const E9_SCENARIO: &str = "QIANQIAN_E9_SCENARIO";
/// Child-process selector for the E-8 report-site scenarios.
#[cfg(unix)]
const E8_SITE: &str = "QIANQIAN_E8_SITE";
/// Arming marker: printed by a child on the stream that stays healthy BEFORE
/// the parent breaks the other one, so the close happens after the child
/// reached the state under test (no scheduling premise).
#[cfg(unix)]
const E9_ARMED: &str = "Q290-ARMED";

/// The transport's real report writer shape (entry.rs `emit_machine_report`,
/// module-private): a main-thread `println!`/`eprintln!` pair whose panic on
/// a broken sink is the E-8 subject.
#[cfg(unix)]
fn real_report(stream: ReportStream, line: &str) {
    match stream {
        ReportStream::Stdout => println!("{line}"),
        ReportStream::Stderr => eprintln!("{line}"),
    }
}

/// Witness line on the stream the scenario keeps healthy.
#[cfg(unix)]
fn witness(stderr: bool, line: &str) {
    use std::io::Write;
    if stderr {
        eprintln!("{line}");
        let _ = io::stderr().flush();
    } else {
        println!("{line}");
        let _ = io::stdout().flush();
    }
}

/// Path of a binary built alongside this test binary (`target/<profile>/`),
/// failing loudly when absent instead of skipping.
#[cfg(unix)]
fn real_binary(name: &str) -> std::path::PathBuf {
    let mut path = std::env::current_exe().expect("test binary path");
    path.pop(); // deps/
    path.pop(); // profile/
    let bin = path.join(name);
    assert!(
        bin.exists(),
        "the {name} binary must be built for this test"
    );
    bin
}

/// A piped stream drained on a helper thread into shared text the parent can
/// POLL (marker waits must not consume the pipe or wait for EOF).
#[cfg(unix)]
type SharedText = std::sync::Arc<std::sync::Mutex<String>>;

#[cfg(unix)]
fn shared_capture(
    reader: impl std::io::Read + Send + 'static,
) -> (std::thread::JoinHandle<()>, SharedText) {
    let text: SharedText = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = text.clone();
    let handle = std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        let mut reader = reader;
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let mut guard = sink.lock().expect("capture lock");
                    guard.push_str(&String::from_utf8_lossy(&buffer[..n]));
                }
            }
        }
    });
    (handle, text)
}

#[cfg(unix)]
fn wait_for_marker(text: &SharedText, needle: &str, what: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if text.lock().expect("capture lock").contains(needle) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{what}: never saw {needle:?} (child hung or died)"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn assert_contains(text: &SharedText, needle: &str, what: &str) {
    let guard = text.lock().expect("capture lock");
    assert!(guard.contains(needle), "{what}\n{guard}");
}

#[cfg(unix)]
fn finish_text(handle: std::thread::JoinHandle<()>, text: SharedText, what: &str) -> String {
    handle.join().expect(what);
    std::mem::take(&mut *text.lock().expect("capture lock"))
}

/// Bounded wait for child exit; kills and reports BLOCKED (never PASS) on
/// timeout.
#[cfg(unix)]
fn bounded_wait(child: &mut std::process::Child, what: &str) -> std::process::ExitStatus {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match child.try_wait().expect("wait on the child") {
            Some(status) => return status,
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "BLOCKED/INCONCLUSIVE ({what}): the child never completed inside the bound; \
                     this is a failed oracle, not a pass"
                );
            }
            None => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
}

// PROPERTY/AUTHORITY: B3 of implementation-record §14.6 — a swallowed flush
// result (the production `let _ = io::stdout().flush()`, machine_input.rs:274)
// leaves an admitted operation indistinguishable from success: no record, no
// Stop, no exit contribution. The negative control, inside the same test, is
// the write-PANIC shape at the same boundary, which MUST be classified.
// PRECONDITION: real admission through `line` (the only admitted path).
// TRIGGER: an output closure that fails a flush-shaped tail without
// unwinding; the control closure unwinds. ORACLE: exact `line` return, seal
// value, stop_requested. FALSIFIER: any classification appearing for the
// swallow arm (a non-panic failure got recorded), or a missing classification
// in the panic arm (harness insensitivity).
#[test]
fn flush_err_swallow_is_indistinguishable_from_success() {
    let input = HostInput::default();
    let handle = PlaybackSessionHandle::new();
    let mut writes = Vec::new();
    let completed = input.line("status", &handle, &mut |stream, line| {
        writes.push((stream, line.to_owned()));
        // The production tail shape after a successful write: an explicit
        // flush whose Err is discarded, modeled at the operation boundary.
        // Whether the REAL std flush can fail there at all is the L2 probe's
        // subject (implementation-record §14.8, flush-Err reachability).
        let flush_result: io::Result<()> = Err(io::Error::other("injected flush failure"));
        let _ = flush_result;
    });
    assert!(
        completed,
        "a swallowed flush Err must not fail the operation"
    );
    assert_eq!(input.seal(), None, "a swallowed flush Err records nothing");
    assert!(!handle.observe().stop_requested);
    assert_eq!(writes.len(), 1);

    // Negative control: the panic shape at the SAME boundary is classified.
    let input = HostInput::default();
    let handle = PlaybackSessionHandle::new();
    let completed = input.line("status", &handle, &mut |_, _| {
        std::panic::resume_unwind(Box::new("write panic shape"));
    });
    assert!(!completed);
    assert_eq!(input.seal(), Some(HostFailure::Panic));
    assert!(handle.observe().stop_requested);
}

// PROPERTY/AUTHORITY: B1 of implementation-record §14.6 — the full D3 chain
// for an output-write panic, assembled at transport level: catch #R-dispatch
// → HostFailure::Panic recorded BEFORE the response → conditional Stop on the
// unsettled episode → ack → seal Some(Panic) → exit 1 → D11 stays Stopped
// (never forged Failed; no failure diagnostic) → host-failure report on
// stderr, then the genuine outcome report on stdout. PRECONDITION: real
// established episode (fixture); the injected write-panic is the only
// synthetic element (the E-9 subprocess scenarios supply the real EPIPE).
// ORACLE: exact exit code, D11 outcome, report sequence. FALSIFIER: exit 0,
// a Failed outcome, missing/misordered reports, or a seal without failure.
#[test]
fn reader_output_panic_full_transport_contract() {
    let (assembled, disposed) = episode(SourceBehavior::EofAfter(100_000), true, None);
    let handle = assembled.start.handle.clone();
    let mut reports = Vec::new();
    let mut fed = false;
    let exit = crate::entry::run_machine_reports_for_test(
        assembled,
        |input, handle| {
            read_input(
                &input,
                &handle,
                |line| {
                    if fed {
                        Ok(0)
                    } else {
                        fed = true;
                        line.push_str("status\n");
                        Ok(line.len())
                    }
                },
                |_stream, _line| {
                    std::panic::resume_unwind(Box::new("stdout write panic"));
                },
            );
        },
        |stream, line| reports.push((stream, line.to_owned())),
    );
    assert_eq!(exit, std::process::ExitCode::from(1));
    assert_eq!(handle.wait_terminal(), EpisodeTerminalOutcome::Stopped);
    assert!(
        handle.observe().failure_diagnostic.is_none(),
        "a host failure must not publish a D11 diagnostic"
    );
    assert!(disposed.get());
    assert_eq!(
        reports[0],
        (ReportStream::Stderr, HostFailure::Panic.report().into())
    );
    assert_eq!(
        &reports[1..],
        crate::machine::outcome_report(EpisodeTerminalOutcome::Stopped, None)
    );
}

// PROPERTY/AUTHORITY: E-9 three-mode protocol-write conformance
// (reconciliation §8 E-9; implementation-record §14.6 B1/B2/B4), executed
// against the REAL production reader: `spawn_reader` reads real stdin and
// writes through the real `print!`+`flush` / `eprintln!` closure; the parent
// choreographs a real OS pipe (broken or left unread) and drives real command
// lines. ORACLE: the documented D3 chain — a reader protocol-write panic is
// CAUGHT (no reader-thread crash), classified, conditionally routed to Stop,
// sealed, and reported; blocking is NOT a failure (no record, no Stop, clean
// completion after release). SIGHT CONTROLS: the `healthy` twins prove the
// same choreography delivers rows on the same streams. FALSIFIERS: child exit
// 101 from an uncaught reader panic; a seal without the recorded failure; a
// host-failure report in the blocked scenario; missing rows in a healthy
// twin; a blocked child that never completes after release — the watchdog
// reports that as BLOCKED/INCONCLUSIVE (a failed assertion), never as PASS.
//
// Child scenarios (the parent drivers below own the choreography):
//   stdout-epipe   marker on stderr → parent closes stdout → `status` ⇒ the
//                  reader's print! hits EPIPE inside its admitted operation;
//                  the transport's final outcome row then panics on MAIN
//                  against the same broken sink (ledger #6 class — later
//                  reports only; D11 already committed).
//   stderr-epipe   marker on stdout → parent closes stderr → `badcmd` ⇒ the
//                  reader's eprintln! panics; the owner's host-failure report
//                  then panics on MAIN against the same broken stderr.
//   blocked        `stop` + a status flood with nobody reading ⇒ the reader
//                  parks inside a status write; the settled episode cannot
//                  seal; release ⇒ clean completion with NO host failure.
//   *-healthy      sight controls: same choreography, no break.
#[test]
#[cfg(unix)]
fn e9_real_reader_child() {
    let Ok(scenario) = std::env::var(E9_SCENARIO) else {
        return;
    };
    match scenario.as_str() {
        "stdout-epipe" | "stderr-epipe" | "blocked" | "stdout-healthy" | "stderr-healthy" => {}
        other => panic!("unknown E-9 scenario {other:?}"),
    }
    let marker_on_stderr = !scenario.starts_with("stderr");
    let (assembled, disposed) = episode(SourceBehavior::EofAfter(100_000), true, None);
    let handle = assembled.start.handle.clone();
    if scenario == "blocked" {
        // Pre-fill the unread stdout pipe (64 KiB here, F_GETPIPE_SZ-checked)
        // so the reader's status writes park deterministically inside the
        // admitted dispatch: after this filler and the transport's two
        // opening rows there is room for only a handful of ~140-byte status
        // rows. A flood alone cannot do it: the transport's seal closes
        // admission right after the stop settles, and the reader then exits
        // on the closed admission long before a flood could fill the pipe.
        let filler = "x".repeat(120);
        for _ in 0..536 {
            println!("{filler}");
        }
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
    }
    let host_slot: Rc<RefCell<Option<HostInput>>> = Rc::new(RefCell::new(None));
    let slot = host_slot.clone();
    let start_reader = move |input: HostInput, reader_handle: PlaybackSessionHandle| {
        witness(marker_on_stderr, E9_ARMED);
        *slot.borrow_mut() = Some(input.clone());
        spawn_reader(input, reader_handle);
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        crate::entry::run_machine_reader_for_test(assembled, start_reader)
    }));
    let terminal = handle.wait_terminal();
    // The transport sealed its own host input; `seal` on a closed admission
    // with no active operation is an idempotent read of the same record.
    let sealed = host_slot.borrow().as_ref().map(|input| input.seal());
    witness(
        marker_on_stderr,
        &format!(
            "Q290-E9-UNWIND={} Q290-E9-D11={terminal:?} Q290-E9-DISPOSED={} Q290-E9-SEAL={:?}",
            u8::from(result.is_err()),
            u8::from(disposed.get()),
            sealed.flatten(),
        ),
    );
    if scenario.ends_with("epipe") || scenario == "blocked" {
        // A stream is still broken/parked; the harness's own result print
        // must not run against it.
        std::process::exit(0);
    }
    assert!(!result.is_err());
    assert_eq!(sealed, Some(None));
}

#[cfg(unix)]
fn spawn_e9_child(scenario: &str) -> std::process::Child {
    std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "machine_input::tests::e9_real_reader_child",
            "--nocapture",
        ])
        .env(E9_SCENARIO, scenario)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn the E-9 child")
}

// PROPERTY: B1 mode-1 L2 — the REAL production output closure (print!+flush)
// against a REAL closed pipe, on the REAL reader thread, inside a REAL
// admitted operation: caught, Panic-recorded, Stop routed to the unsettled
// episode, sealed, the host-failure report delivered on the healthy stderr;
// the transport's final outcome row then panics on MAIN (ledger #6 class —
// later reports only; D11 committed before it). PREMISES SYNCHRONIZED: the
// arming marker (stderr) proves the reader is spawned before the close, and
// the close precedes the `status` line by the parent's order.
#[test]
#[cfg(unix)]
fn e9_real_reader_stdout_epipe_follows_the_documented_host_failure_classes() {
    use std::io::Write;
    let mut child = spawn_e9_child("stdout-epipe");
    let (stderr_thread, stderr_text) = shared_capture(child.stderr.take().expect("child stderr"));
    let child_stdout = child.stdout.take().expect("child stdout"); // held, uncaptured
    wait_for_marker(&stderr_text, E9_ARMED, "stdout-epipe child");
    drop(child_stdout); // break stdout AFTER the reader is armed
    child
        .stdin
        .as_mut()
        .expect("child stdin")
        .write_all(b"status\n")
        .expect("write the status line");
    drop(child.stdin.take());
    wait_for_marker(&stderr_text, "Q290-E9-UNWIND", "stdout-epipe witnesses");
    let status = bounded_wait(&mut child, "stdout-epipe");
    let stderr = finish_text(stderr_thread, stderr_text, "stderr capture");
    assert!(stderr.contains(E9_ARMED), "{stderr}");
    assert!(
        stderr.contains("Q290-E9-UNWIND=1"),
        "the transport must unwind at its final outcome row\n{stderr}"
    );
    assert!(stderr.contains("Q290-E9-D11=Stopped"), "{stderr}");
    assert!(stderr.contains("Q290-E9-DISPOSED=1"), "{stderr}");
    assert!(
        stderr.contains("Q290-E9-SEAL=Some(Panic)"),
        "seal lost the reader's recorded failure\n{stderr}"
    );
    assert!(
        stderr.contains(HostFailure::Panic.report()),
        "the host-failure report must reach the healthy stderr\n{stderr}"
    );
    assert!(
        stderr.contains("failed printing to stdout") && stderr.contains("Broken pipe"),
        "the main-thread outcome-row panic (same broken sink) must surface on stderr\n{stderr}"
    );
    assert_eq!(
        status.code(),
        Some(0),
        "the child must exit through its witness path (exit 0), not through a crash"
    );
}

// PROPERTY: B2 mode-1 L2, stderr arm — a real stderr diagnostic write
// (`ignored input: unknown command …`) failing on the REAL reader thread is
// classified by the SAME admitted-operation contract; the owner's
// host-failure report then panics on MAIN against the same broken stderr, so
// the outcome row is skipped and the child reports its witnesses on the
// healthy stdout. PREMISES SYNCHRONIZED: the transport rows and the arming
// marker are observed on stdout BEFORE the close (polled, not assumed); the
// close precedes the command line.
#[test]
#[cfg(unix)]
fn e9_real_reader_stderr_epipe_follows_the_documented_host_failure_classes() {
    use std::io::Write;
    let mut child = spawn_e9_child("stderr-epipe");
    let (stdout_thread, stdout_text) = shared_capture(child.stdout.take().expect("child stdout"));
    let child_stderr = child.stderr.take().expect("child stderr"); // held, uncaptured
    wait_for_marker(&stdout_text, E9_ARMED, "stderr-epipe child");
    assert_contains(
        &stdout_text,
        "source: ",
        "transport rows must precede the marker",
    );
    assert_contains(
        &stdout_text,
        "playing ",
        "the playing row must precede the marker",
    );
    drop(child_stderr); // break stderr AFTER the rows reached the open pipe
    child
        .stdin
        .as_mut()
        .expect("child stdin")
        .write_all(b"badcmd\n")
        .expect("write the command line");
    drop(child.stdin.take());
    wait_for_marker(&stdout_text, "Q290-E9-UNWIND", "stderr-epipe witnesses");
    let status = bounded_wait(&mut child, "stderr-epipe");
    let stdout = finish_text(stdout_thread, stdout_text, "stdout capture");
    assert!(stdout.contains("Q290-E9-UNWIND=1"), "{stdout}");
    assert!(stdout.contains("Q290-E9-D11=Stopped"), "{stdout}");
    assert!(stdout.contains("Q290-E9-DISPOSED=1"), "{stdout}");
    assert!(
        stdout.contains("Q290-E9-SEAL=Some(Panic)"),
        "the stderr-arm panic must be recorded like the stdout arm\n{stdout}"
    );
    assert!(
        !stdout.contains("stopped before completion"),
        "the outcome row must have been skipped by the report panic\n{stdout}"
    );
    assert_eq!(status.code(), Some(0));
}

// PROPERTY: B4 mode-3 L2 — a REAL unread pipe parks the REAL reader INSIDE a
// status write while the episode settles; the transport cannot seal (the
// admitted operation owes its ack) and the child cannot exit; after release
// the run completes cleanly with NO host failure (blocking is not a failure).
// The parked state is constructed by the flood itself (the parent never
// reads; the pipe fills) — its witness is the bounded non-exit window AFTER
// `stop` was delivered; direct seal-wait witnessing remains the in-process
// oracle (`seal_retains_admitted_postclosure_output_failure…`). WATCHDOG:
// safety only — a timeout is reported BLOCKED (a failed oracle), never PASS.
#[test]
#[cfg(unix)]
fn e9_real_reader_blocked_pipe_delays_seal_and_is_not_a_failure() {
    use std::io::Write;
    let mut child = spawn_e9_child("blocked");
    let (stderr_thread, stderr_text) = shared_capture(child.stderr.take().expect("child stderr"));
    // `stop` settles the episode; the child's own filler (see the child arm)
    // leaves room for only a few status rows, so the reader parks inside an
    // admitted status write almost immediately. The parent holds the read end
    // and never reads, so nothing can drain the park away.
    let mut flood = String::from("stop\n");
    for _ in 0..1200 {
        flood.push_str("status\n");
    }
    child
        .stdin
        .as_mut()
        .expect("child stdin")
        .write_all(flood.as_bytes())
        .expect("write the flood");
    // Grace: let the child boot, settle the episode, and reach the park
    // (generous: CI runners run suites in parallel).
    std::thread::sleep(std::time::Duration::from_secs(4));
    // The parked window: the episode is already stop-settled, so the only way
    // a healthy child stays alive here is the seal waiting on the parked
    // reader's ack. Non-exit across the window is the witness.
    let parked_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < parked_deadline {
        if let Some(status) = child.try_wait().expect("poll the child") {
            let seen = stderr_text.lock().expect("capture lock").clone();
            panic!(
                "the child exited early ({status}) while its reader should have been parked \
                 in the blocked write\n--- child stderr so far ---\n{seen}"
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // Release: drain stdout to EOF (the reader answers every queued command,
    // finds admission closed, exits without another read; the transport
    // seals, reports, and exits), then close stdin.
    drop(child.stdin.take());
    let mut child_stdout = child.stdout.take().expect("child stdout");
    let mut stdout = String::new();
    {
        use std::io::Read;
        child_stdout
            .read_to_string(&mut stdout)
            .expect("drain the child stdout");
    }
    drop(child_stdout);
    let status = bounded_wait(&mut child, "blocked-release");
    let stderr = finish_text(stderr_thread, stderr_text, "stderr capture");
    let stdout_tail = &stdout[stdout.len().saturating_sub(400)..];
    assert_eq!(
        status.code(),
        Some(0),
        "a released block must complete cleanly\nstdout tail: {stdout_tail}\nstderr: {stderr}"
    );
    assert!(
        !stderr.contains("machine input failed"),
        "blocking must not be recorded as a host failure\n{stderr}"
    );
    assert!(
        stderr.contains("Q290-E9-SEAL=None"),
        "a released block must seal with no recorded host failure\n{stderr}"
    );
    // Admission closes at the transport's seal, which runs right after the
    // stop settles (machine_input.rs): the post-stop flood is answered only
    // until that closure, so the answered count is the scheduling race, not
    // the contract. This assert is a sanity row only (the pre-park rows
    // already satisfy it); the load-bearing parked-ack ordering oracle is the
    // channel-synchronized L1 `seal_retains_admitted_postclosure_output_
    // failure_and_rejects_fresh_work`, and the non-exit window above is
    // schedule-biased L2 evidence of the park.
    assert!(
        stdout.matches("outcome: ").count() >= 1,
        "at least one admitted status row must have completed\nstdout tail: {stdout_tail}"
    );
    assert!(stdout.contains("stopped before completion"), "{stdout}");
    assert!(
        stderr.contains(E9_ARMED),
        "the arming marker must be on the healthy stderr\n{stderr}"
    );
}

// SIGHT CONTROL for B1 mode-1: the same child/choreography with a HEALTHY
// stdout completes with the status answer and the outcome row, no host
// failure, child exit 0 (the healthy twins also re-assert their own shape
// in-process and must NOT exit early).
#[test]
#[cfg(unix)]
fn e9_real_reader_stdout_healthy_sight_control() {
    use std::io::Write;
    let mut child = spawn_e9_child("stdout-healthy");
    let (stdout_thread, stdout_text) = shared_capture(child.stdout.take().expect("child stdout"));
    let (stderr_thread, stderr_text) = shared_capture(child.stderr.take().expect("child stderr"));
    wait_for_marker(&stderr_text, E9_ARMED, "stdout-healthy child");
    child
        .stdin
        .as_mut()
        .expect("child stdin")
        .write_all(b"status\nstop\n")
        .expect("write the commands");
    drop(child.stdin.take());
    let status = bounded_wait(&mut child, "stdout-healthy");
    let stdout = finish_text(stdout_thread, stdout_text, "stdout capture");
    let stderr = finish_text(stderr_thread, stderr_text, "stderr capture");
    assert_eq!(status.code(), Some(0), "{stdout}\n{stderr}");
    for row in [
        "source: ",
        "playing ",
        "outcome: ",
        "stopped before completion",
    ] {
        assert!(stdout.contains(row), "missing {row:?}\n{stdout}");
    }
    assert!(stderr.contains(E9_ARMED), "{stderr}");
    assert!(
        !stderr.contains("machine input failed"),
        "a healthy sink must not produce a host failure\n{stderr}"
    );
}

// SIGHT CONTROL for B2 mode-1: healthy stderr — the diagnostic reaches it and
// the full report set completes.
#[test]
#[cfg(unix)]
fn e9_real_reader_stderr_healthy_sight_control() {
    use std::io::Write;
    let mut child = spawn_e9_child("stderr-healthy");
    let (stdout_thread, stdout_text) = shared_capture(child.stdout.take().expect("child stdout"));
    let (stderr_thread, stderr_text) = shared_capture(child.stderr.take().expect("child stderr"));
    wait_for_marker(&stdout_text, E9_ARMED, "stderr-healthy child");
    child
        .stdin
        .as_mut()
        .expect("child stdin")
        .write_all(b"badcmd\nstop\n")
        .expect("write the commands");
    drop(child.stdin.take());
    let status = bounded_wait(&mut child, "stderr-healthy");
    let stdout = finish_text(stdout_thread, stdout_text, "stdout capture");
    let stderr = finish_text(stderr_thread, stderr_text, "stderr capture");
    assert_eq!(status.code(), Some(0), "{stdout}\n{stderr}");
    assert!(stdout.contains("stopped before completion"), "{stdout}");
    assert!(
        stderr.contains("ignored input: unknown command"),
        "the diagnostic must reach the healthy stderr\n{stderr}"
    );
    assert!(
        !stderr.contains("machine input failed"),
        "a healthy sink must not produce a host failure\n{stderr}"
    );
}

// --- E-8: main-thread report sites against real broken pipes -----------------

// PROPERTY/AUTHORITY: B6 / E-8 (reconciliation §8; implementation-record
// §14.4) — the machine transport's own report rows (#4 `source:`/`playing`,
// entry.rs:529-535, BEFORE reader spawn and terminal wait) have NO catch: a
// broken stdout pipe panics on MAIN, and the unwind skips wait_terminal,
// dispose, seal AND the reader spawn (a real disposal-skip witness at L1:
// the fixture's disposed marker stays false; no terminal exists). PREMISES:
// the arming marker fires BEFORE the transport is entered, so the close
// precedes the rows by construction. FALSIFIER: a caught/exit-0 run with rows
// skipped, or a disposed=1 witness in the broken arm.
#[test]
#[cfg(unix)]
fn e8_report_site_child() {
    let Ok(site) = std::env::var(E8_SITE) else {
        return;
    };
    match site.as_str() {
        "site4" => {
            let (assembled, disposed) = episode(SourceBehavior::EofAfter(100_000), true, None);
            let handle = assembled.start.handle.clone();
            witness(true, E9_ARMED);
            // Gate: wait for the parent's "go" line so the close of stdout is
            // strictly BEFORE the transport's first row write (no scheduling
            // premise). The gate line sits on the child's own stdin, which
            // the transport never reads (the reader never spawns here).
            {
                use std::io::BufRead;
                let gate = std::io::stdin()
                    .lock()
                    .lines()
                    .next()
                    .expect("the parent's gate line")
                    .expect("read the gate line");
                assert_eq!(gate, "go");
            }
            let reader_spawned = Rc::new(Cell::new(false));
            let flag = reader_spawned.clone();
            let result = catch_unwind(AssertUnwindSafe(|| {
                crate::entry::run_machine_reports_for_test(
                    assembled,
                    move |_, _| flag.set(true),
                    real_report,
                )
            }));
            witness(
                true,
                &format!(
                    "Q290-E8-SITE4-UNWIND={} Q290-E8-SITE4-DISPOSED={} \
                     Q290-E8-SITE4-TERMINAL={:?} Q290-E8-SITE4-READER_SPAWNED={}",
                    u8::from(result.is_err()),
                    u8::from(disposed.get()),
                    handle.observe().terminal_outcome,
                    u8::from(reader_spawned.get()),
                ),
            );
            std::process::exit(0);
        }
        "site6" | "site6-control" => {
            let (assembled, disposed) = episode(SourceBehavior::EofAfter(100_000), true, None);
            let handle = assembled.start.handle.clone();
            // The REAL reader over real stdin: the parent's `stop` line is
            // both the settle trigger and the gate — it is written only after
            // the parent closed stdout, so the final reports strictly
            // happen-after the break (no scheduling premise).
            let start_reader = |input: HostInput, reader_handle: PlaybackSessionHandle| {
                witness(true, E9_ARMED);
                spawn_reader(input, reader_handle);
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                crate::entry::run_machine_reports_for_test(assembled, start_reader, real_report)
            }));
            let terminal = handle.wait_terminal();
            witness(
                true,
                &format!(
                    "Q290-E8-SITE6-UNWIND={} Q290-E8-SITE6-DISPOSED={} Q290-E8-SITE6-TERMINAL={terminal:?}",
                    u8::from(result.is_err()),
                    u8::from(disposed.get()),
                ),
            );
            std::process::exit(0);
        }
        "site7" | "site7-control" => {
            let (assembled, disposed) = episode_not_established();
            let control = site == "site7-control";
            println!("{E9_ARMED}");
            {
                use std::io::Write;
                let _ = std::io::stdout().flush();
            }
            // Gate on the parent's go line so the stderr break is strictly
            // before the transport's report write.
            {
                use std::io::BufRead;
                let gate = std::io::stdin()
                    .lock()
                    .lines()
                    .next()
                    .expect("the parent's gate line")
                    .expect("read the gate line");
                assert_eq!(gate, "go");
            }
            let handle = assembled.start.handle.clone();
            let result = catch_unwind(AssertUnwindSafe(|| {
                crate::entry::run_machine_attempt_for_test(
                    assembled.start.runtime,
                    assembled.start.handle,
                    assembled.start.establishment,
                )
            }));
            // A never-established attempt commits no terminal; `wait_terminal`
            // would wait forever. Observe instead.
            let terminal = handle.observe().terminal_outcome;
            println!(
                "Q290-E8-SITE7-UNWIND={} Q290-E8-SITE7-DISPOSED={} Q290-E8-SITE7-TERMINAL={terminal:?}",
                u8::from(result.is_err()),
                u8::from(disposed.get()),
            );
            if control {
                assert!(
                    !result.is_err(),
                    "the healthy control must complete the transport"
                );
                assert_eq!(result.unwrap(), std::process::ExitCode::from(1));
            } else {
                assert!(
                    result.is_err(),
                    "the broken-stderr run must unwind at the report"
                );
                std::process::exit(0);
            }
        }
        other => panic!("unknown E-8 site {other:?}"),
    }
}

/// NotEstablished fixture: decode provides; output activates WITHOUT
/// declaring or providing its capability, so the Session dependency cannot
/// resolve and the whole attempt stays `NotEstablished { diagnostic: None }`
/// (the assembly's dependency-refusal shape; execution-model §4.10/C1).
/// The declaration must be ABSENT, not merely uninstalled: a declared-but-
/// withheld provision is the kernel §L.4 totality raise, whose activation
/// unwind would discharge the marker during assembly — before the transport
/// whose disposal-skip this fixture exists to witness.
#[cfg(unix)]
fn episode_not_established() -> (AssemblyOutcome, Rc<Cell<bool>>) {
    let disposed = Rc::new(Cell::new(false));
    let marker = disposed.clone();
    let decode = ComponentSpec::new("decode")
        .provides::<PcmDecodeCapability>()
        .on_activate(|ctx| {
            ctx.provide::<PcmDecodeCapability>(Rc::new(TestDecode::new(SourceBehavior::EofAfter(
                0,
            ))))
            .unwrap();
            Ok(())
        });
    let output = ComponentSpec::new("output").on_activate(move |ctx| {
        let marker = marker.clone();
        ctx.register_effect(move || {
            marker.set(true);
            Discharge::Discharged
        });
        Ok(())
    });
    let handle = PlaybackSessionHandle::new();
    let (session, attempt) = playback_session_spec_with_establishment(
        "test://not-established".into(),
        handle.clone(),
        AudioProcessingConfig::BYPASS,
    );
    let assembled = establish_specs_for_test(decode, output, session, attempt, handle);
    assert_eq!(
        assembled.start.establishment,
        EstablishmentResult::NotEstablished { diagnostic: None }
    );
    (assembled, disposed)
}

#[cfg(unix)]
fn spawn_e8_child(site: &str) -> std::process::Child {
    std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "machine_input::tests::e8_report_site_child",
            "--nocapture",
        ])
        .env(E8_SITE, site)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn the E-8 child")
}

// PROPERTY: #4 on the REAL transport wiring — the rows print to the real
// stdout; when that pipe closes before them (the marker already fired), MAIN
// panics; the unwind skips the wait, the disposal, the seal, and the reader
// spawn. Site attribution rides the default-hook panic text on the healthy
// stderr (it names entry.rs and the failed stdout print).
#[test]
#[cfg(unix)]
fn e8_site4_source_playing_rows_panic_skip_disposal_on_broken_stdout() {
    let mut child = spawn_e8_child("site4");
    let (stderr_thread, stderr_text) = shared_capture(child.stderr.take().expect("child stderr"));
    wait_for_marker(&stderr_text, E9_ARMED, "site4 child");
    drop(child.stdout.take()); // break stdout AFTER the marker, BEFORE the rows
    use std::io::Write as _;
    child
        .stdin
        .as_mut()
        .expect("site4 child stdin")
        .write_all(b"go\n")
        .expect("write the gate line");
    wait_for_marker(&stderr_text, "Q290-E8-SITE4-UNWIND", "site4 witnesses");
    let status = bounded_wait(&mut child, "site4");
    let stderr = finish_text(stderr_thread, stderr_text, "site4 stderr");
    assert!(stderr.contains("Q290-E8-SITE4-UNWIND=1"), "{stderr}");
    assert!(
        stderr.contains("Q290-E8-SITE4-DISPOSED=0"),
        "the unwind must skip the attempted root's disposal\n{stderr}"
    );
    assert!(
        stderr.contains("Q290-E8-SITE4-TERMINAL=None"),
        "no terminal may exist when the wait was skipped\n{stderr}"
    );
    assert!(
        stderr.contains("Q290-E8-SITE4-READER_SPAWNED=0"),
        "the reader must not spawn past the panicked rows\n{stderr}"
    );
    assert!(
        stderr.contains("failed printing to stdout") && stderr.contains("Broken pipe"),
        "the panic must name the failed stdout write\n{stderr}"
    );
    assert_eq!(status.code(), Some(0));
}

// PROPERTY: #6 (the final reports through the real report writer) — they run
// AFTER wait_terminal+dispose+seal; a broken stdout makes the outcome row
// panic on MAIN, skipping only LATER reports (D11 committed, disposal done —
// disposed=1). Sight control `site6-control`: healthy sink completes with the
// row and SUCCESS.
#[test]
#[cfg(unix)]
fn e8_site6_final_reports_panic_skip_only_later_reports_on_broken_stdout() {
    let mut child = spawn_e8_child("site6");
    let (stderr_thread, stderr_text) = shared_capture(child.stderr.take().expect("child stderr"));
    wait_for_marker(&stderr_text, E9_ARMED, "site6 child");
    drop(child.stdout.take()); // break stdout before the final reports
    use std::io::Write as _;
    child
        .stdin
        .as_mut()
        .expect("site6 child stdin")
        .write_all(b"stop\n")
        .expect("write the stop line");
    wait_for_marker(&stderr_text, "Q290-E8-SITE6-UNWIND", "site6 witnesses");
    let status = bounded_wait(&mut child, "site6");
    let stderr = finish_text(stderr_thread, stderr_text, "site6 stderr");
    assert!(stderr.contains("Q290-E8-SITE6-UNWIND=1"), "{stderr}");
    assert!(
        stderr.contains("Q290-E8-SITE6-DISPOSED=1"),
        "disposal must have completed BEFORE the panicked report\n{stderr}"
    );
    assert!(
        stderr.contains("Q290-E8-SITE6-TERMINAL=Stopped"),
        "the terminal must be committed before the final reports\n{stderr}"
    );
    assert!(
        stderr.contains("failed printing to stdout") && stderr.contains("Broken pipe"),
        "the outcome-row panic must name the failed stdout write\n{stderr}"
    );
    assert_eq!(status.code(), Some(0));

    // Sight control: the same child with a healthy sink.
    let mut control = spawn_e8_child("site6-control");
    let (stdout_thread, stdout_text) =
        shared_capture(control.stdout.take().expect("control stdout"));
    {
        use std::io::Write as _;
        control
            .stdin
            .as_mut()
            .expect("control stdin")
            .write_all(b"stop\n")
            .expect("write the stop line");
    }
    drop(control.stdin.take());
    let status = bounded_wait(&mut control, "site6-control");
    let stdout = finish_text(stdout_thread, stdout_text, "site6-control stdout");
    assert_eq!(status.code(), Some(0), "{stdout}");
    for row in ["source: ", "playing ", "stopped before completion"] {
        assert!(stdout.contains(row), "missing {row:?}\n{stdout}");
    }
}

// PROPERTY: #7 (entry.rs:515, BEFORE dispose :516) — the NotEstablished
// activation report on the REAL transport wiring; a broken stderr panics MAIN
// and the unwind skips the attempted root's disposal (disposed=0 — the
// ledger #3/#7 leak witnessed at runtime, not just inferred from source).
// Sight control `site7-control`: healthy stderr completes with the exact
// report line (site attribution), disposal runs, transport exit 1.
#[test]
#[cfg(unix)]
fn e8_site7_notestablished_report_panic_skips_disposal_on_broken_stderr() {
    let mut child = spawn_e8_child("site7");
    let (stdout_thread, stdout_text) = shared_capture(child.stdout.take().expect("child stdout"));
    wait_for_marker(&stdout_text, E9_ARMED, "site7 child");
    drop(child.stderr.take()); // break stderr AFTER the marker, BEFORE the report
    {
        use std::io::Write as _;
        child
            .stdin
            .as_mut()
            .expect("site7 child stdin")
            .write_all(b"go\n")
            .expect("write the gate line");
    }
    drop(child.stdin.take());
    wait_for_marker(&stdout_text, "Q290-E8-SITE7-UNWIND", "site7 witnesses");
    let status = bounded_wait(&mut child, "site7");
    let stdout = finish_text(stdout_thread, stdout_text, "site7 stdout");
    assert!(stdout.contains("Q290-E8-SITE7-UNWIND=1"), "{stdout}");
    assert!(
        stdout.contains("Q290-E8-SITE7-DISPOSED=0"),
        "the report panic must skip the attempted root's disposal\n{stdout}"
    );
    assert!(stdout.contains("Q290-E8-SITE7-TERMINAL=None"), "{stdout}");
    assert_eq!(status.code(), Some(0));

    // Sight control: healthy stderr — the exact report line, disposal, exit 1.
    let mut control = spawn_e8_child("site7-control");
    let (stdout_thread, stdout_text) =
        shared_capture(control.stdout.take().expect("control stdout"));
    let (stderr_thread, stderr_text) =
        shared_capture(control.stderr.take().expect("control stderr"));
    {
        use std::io::Write as _;
        control
            .stdin
            .as_mut()
            .expect("control stdin")
            .write_all(b"go\n")
            .expect("write the gate line");
    }
    drop(control.stdin.take());
    let status = bounded_wait(&mut control, "site7-control");
    let stdout = finish_text(stdout_thread, stdout_text, "site7-control stdout");
    let stderr = finish_text(stderr_thread, stderr_text, "site7-control stderr");
    assert_eq!(status.code(), Some(0), "{stdout}\n{stderr}");
    assert!(stdout.contains("Q290-E8-SITE7-UNWIND=0"), "{stdout}");
    assert!(stdout.contains("Q290-E8-SITE7-DISPOSED=1"), "{stdout}");
    assert!(
        stdout.contains("Q290-E8-SITE7-TERMINAL=None"),
        "a NotEstablished attempt has no terminal\n{stdout}"
    );
    assert!(
        stderr.contains(&crate::machine::activation_failure_report(None)),
        "the activation report must be the site's exact line\n{stderr}"
    );
}

// PROPERTY: #1 (entry.rs:70/:74/:102-103) on the REAL production binary —
// the usage/version/argument-error report writes run on MAIN with no catch;
// a pipe closed BEFORE the write panics the process (exit 101). The close is
// deterministic: a `sh` wrapper delays the binary so the parent closes the
// read end first. SIGHT CONTROLS pin the healthy rows (site attribution).
// Linux-portable lane: the Windows runtime cannot reproduce a closed pipe
// through the WSL-interop stdio proxy (implementation-record §14.5), so this
// lane is declared Unix-only.
#[test]
#[cfg(unix)]
fn e8_site1_help_broken_stdout_pipe_exits_101_at_the_usage_site() {
    let bin = real_binary("qianqian-headless");
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("sleep 1; exec '{}' --help", bin.display()))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn the delayed binary");
    drop(child.stdout.take()); // close the read end BEFORE the delayed write
    let out = child.wait_with_output().expect("wait on the binary");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(
        out.status.code(),
        Some(101),
        "a main-thread protocol write panic must exit 101\n{stderr}"
    );
    assert!(stderr.contains("failed printing to stdout"), "{stderr}");
    assert!(stderr.contains("Broken pipe"), "{stderr}");
    // NOTE: the panic LOCATION is std's stdio (print! panics inside std), not
    // entry.rs; site attribution rides the healthy-sink controls below.
}

#[test]
#[cfg(unix)]
fn e8_site1_version_broken_stdout_pipe_exits_101_at_the_version_site() {
    let bin = real_binary("qianqian-headless");
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("sleep 1; exec '{}' --version", bin.display()))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn the delayed binary");
    drop(child.stdout.take());
    let out = child.wait_with_output().expect("wait on the binary");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(101), "{stderr}");
    assert!(stderr.contains("failed printing to stdout"), "{stderr}");
    assert!(stderr.contains("Broken pipe"), "{stderr}");
}

#[test]
#[cfg(unix)]
fn e8_site1_argument_error_broken_stderr_pipe_exits_101_at_the_error_site() {
    let bin = real_binary("qianqian-headless");
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("sleep 1; exec '{}' --bogus-flag", bin.display()))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn the delayed binary");
    drop(child.stderr.take()); // break stderr before the error report
    let out = child.wait_with_output().expect("wait on the binary");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(
        out.status.code(),
        Some(101),
        "the argument-error report panic must exit 101"
    );
    assert!(
        stdout.is_empty(),
        "the error path writes nothing to stdout\n{stdout}"
    );
}

#[test]
#[cfg(unix)]
fn e8_site1_healthy_sinks_pin_the_report_rows_for_attribution() {
    let bin = real_binary("qianqian-headless");
    let help = std::process::Command::new(&bin)
        .arg("--help")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run --help");
    assert!(help.status.success());
    let stdout = String::from_utf8_lossy(&help.stdout);
    assert!(stdout.starts_with("Qianqian"), "{stdout}");
    assert!(stdout.contains("Usage:"), "{stdout}");

    let version = std::process::Command::new(&bin)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run --version");
    assert!(version.status.success());
    assert!(
        String::from_utf8_lossy(&version.stdout).starts_with("qianqian-headless "),
        "{:?}",
        String::from_utf8_lossy(&version.stdout)
    );

    let error = std::process::Command::new(&bin)
        .arg("--bogus-flag")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run the bad invocation");
    assert_eq!(error.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&error.stderr);
    assert!(stderr.starts_with("error:"), "{stderr}");
    assert!(stderr.contains("Usage:"), "{stderr}");
}

// B5 of implementation-record §14.6 cites the EXISTING oracles for the
// late-failure boundary (retention through closure, ack ordering, postseal
// inertness, first-failure cut, D11 immutability). They are re-executed by
// this round's runs; no new scenario is owed.

#[test]
fn dg4b_an_isolated_panic_never_waits_on_a_blocked_stderr() {
    // The isolation leg: registered thread, full stderr, previous hook = the
    // default (which writes to stderr). Both panics must unwind and the thread
    // must end, promptly.
    let (status, stdout) = run_blocked_stderr_child("isolated");
    println!("dg4b blocked-stderr isolated leg: status={status:?} stdout={stdout:?}");
    assert!(status.success(), "isolated leg failed: {stdout}");
    assert!(
        stdout.contains("BLOCKED-ISOLATED-CAUGHT=2"),
        "the registered thread did not finish unwinding: {stdout}"
    );
    assert!(
        stdout.contains("BLOCKED-PANICS=2"),
        "both panics were counted on a non-blocked thread: {stdout}"
    );
    assert!(
        stdout.contains("BLOCKED-FILLED-CHUNKS="),
        "the filler's stall was not witnessed: {stdout}"
    );

    // The sight control: chaining still reaches the previous hook for an
    // unregistered thread, with the same blocked stderr in place.
    let (status, stdout) = run_blocked_stderr_child("chaining");
    println!("dg4b blocked-stderr chaining leg: status={status:?} stdout={stdout:?}");
    assert!(status.success(), "chaining leg failed: {stdout}");
    assert!(
        stdout.contains(DG4B_BLOCKED_MARKER),
        "the previous hook did not run for an unregistered thread: {stdout}"
    );
    assert!(
        stdout.contains("BLOCKED-CHAINED=1"),
        "the chaining leg did not finish: {stdout}"
    );
}
