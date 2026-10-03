//! Shared consumption of the fresh assembly operation, PBK-002 D14.6.

use qianqian_app::QianqianApp;
use qianqian_composition::{ComponentSpec, DesiredEntry, Revision};
use qianqian_playback::{
    AudioProcessingConfig, EstablishmentAttempt, EstablishmentResult, PlaybackSessionHandle,
    playback_session_spec_with_establishment,
};
use std::path::PathBuf;

use crate::machine::StartFailure;
use crate::player::StartAttempt;

pub(crate) struct AssemblyOutcome {
    pub(crate) start: StartAttempt,
    /// Machine report/exit class only. Hosts classify from `start.establishment`.
    pub(crate) admission_failure: Option<StartFailure>,
}

/// The canonical fresh Decode/Output/Session assembly, PBK-002 D14.6.
/// Owns provider selection, root creation, Session construction and the entire
/// desired topology. No caller-supplied root or desired entries can enter it.
/// Expanding this topology requires reviewing the establishment boundary.
#[cfg(feature = "playback")]
pub(crate) fn establish(
    file: PathBuf,
    processing: AudioProcessingConfig,
    initial_output_level: u8,
) -> AssemblyOutcome {
    establish_with_providers(
        qianqian_decode_songcore::songcore_decode_plugin(),
        qianqian_output_wasapi::output_plugin(),
        file,
        processing,
        initial_output_level,
    )
}

fn establish_with_providers(
    decode: ComponentSpec,
    output: ComponentSpec,
    file: PathBuf,
    processing: AudioProcessingConfig,
    initial_output_level: u8,
) -> AssemblyOutcome {
    let handle = PlaybackSessionHandle::new();
    handle.request_output_level(initial_output_level);
    let (session, attempt) =
        playback_session_spec_with_establishment(file, handle.clone(), processing);
    assemble(decode, output, session, attempt, handle)
}

fn assemble(
    decode: ComponentSpec,
    output: ComponentSpec,
    session: ComponentSpec,
    attempt: EstablishmentAttempt,
    handle: PlaybackSessionHandle,
) -> AssemblyOutcome {
    let mut runtime = QianqianApp::new();
    let desired = [
        DesiredEntry::enabled("decode", decode.name(), Revision::new(1)),
        DesiredEntry::enabled("output", output.name(), Revision::new(1)),
        DesiredEntry::enabled("session", session.name(), Revision::new(1)),
    ];
    for (role, spec) in [
        ("decode plugin", decode),
        ("output plugin", output),
        ("session", session),
    ] {
        if let Err(error) = runtime.register_component(spec) {
            return admission_refused(
                runtime,
                handle,
                StartFailure::Registration {
                    message: format!("{role} registration failed: {error:?}"),
                },
            );
        }
    }
    if let Err(errors) = runtime.revise_desired(desired.into()) {
        return admission_refused(
            runtime,
            handle,
            StartFailure::CompositionRefused {
                errors: format!("{errors}"),
            },
        );
    }
    AssemblyOutcome {
        start: StartAttempt {
            runtime,
            handle,
            establishment: attempt.finish(),
        },
        admission_failure: None,
    }
}

fn admission_refused(
    runtime: QianqianApp,
    handle: PlaybackSessionHandle,
    presentation: StartFailure,
) -> AssemblyOutcome {
    AssemblyOutcome {
        start: StartAttempt {
            runtime,
            handle,
            establishment: EstablishmentResult::NotEstablished {
                diagnostic: Some(presentation.report()),
            },
        },
        admission_failure: Some(presentation),
    }
}

/// Substitute mechanisms/activation only; even tests cannot supply a root or
/// extra desired members through this seam.
#[cfg(test)]
pub(crate) fn establish_specs_for_test(
    decode: ComponentSpec,
    output: ComponentSpec,
    session: ComponentSpec,
    attempt: EstablishmentAttempt,
    handle: PlaybackSessionHandle,
) -> AssemblyOutcome {
    assemble(decode, output, session, attempt, handle)
}

#[cfg(test)]
#[path = "../../../crates/qianqian-playback/tests/common/mod.rs"]
mod mechanisms;

#[cfg(test)]
mod tests {
    use super::mechanisms::{OutputBehavior, SourceBehavior, TestDecode, TestOutput};
    use super::*;
    use crate::player::{EpisodeStart, OpenOutcome, ReferencePlayerApp, StartAttempt};
    use qianqian_audio_api::ports::{AudioOutputCapability, PcmDecodeCapability};
    use qianqian_composition::{ActivationError, ComponentSpec, Discharge, FiberState, Revision};
    use qianqian_playback::{AudioProcessingConfig, PlaybackSessionHandle};
    use std::cell::{Cell, RefCell};
    use std::path::Path;
    use std::rc::Rc;
    use std::time::Duration;

    #[derive(Clone, Copy)]
    enum Case {
        Provider,
        Dependency,
        Session,
        Processing,
        NoActivation,
        Eof,
        RuntimeFailure,
    }

    fn wire(case: Case) -> StartAttempt {
        wire_with_disposal(case, Rc::new(Cell::new(false)))
    }

    fn wire_with_disposal(case: Case, disposed: Rc<Cell<bool>>) -> StartAttempt {
        let decode = ComponentSpec::new("decode").on_activate(move |ctx| {
            if matches!(case, Case::Provider) {
                return Err(ActivationError::new("deliberate provider failure"));
            }
            if !matches!(case, Case::Dependency) {
                let source = if matches!(case, Case::RuntimeFailure) {
                    SourceBehavior::FailAfter(0)
                } else {
                    SourceBehavior::EofAfter(0)
                };
                ctx.provide::<PcmDecodeCapability>(Rc::new(TestDecode::new(source)))
                    .unwrap();
            }
            Ok(())
        });
        let decode = if matches!(case, Case::Dependency) {
            decode
        } else {
            decode.provides::<PcmDecodeCapability>()
        };
        let output = ComponentSpec::new("output")
            .provides::<AudioOutputCapability>()
            .on_activate(move |ctx| {
                // An owned provider effect witnesses disposal without expanding
                // the canonical topology. The watchdog detects terminal waits.
                let marker = disposed.clone();
                ctx.register_effect(move || {
                    marker.set(true);
                    Discharge::Discharged
                });
                let output = if matches!(case, Case::Session) {
                    OutputBehavior::FailOpen
                } else {
                    OutputBehavior::Consume
                };
                ctx.provide::<AudioOutputCapability>(Rc::new(TestOutput::new(output)))
                    .unwrap();
                Ok(())
            });
        let handle = PlaybackSessionHandle::new();
        let processing = if matches!(case, Case::Processing) {
            AudioProcessingConfig::gain(f32::NAN)
        } else {
            AudioProcessingConfig::BYPASS
        };
        let (spec, attempt) = playback_session_spec_with_establishment(
            "test://attempt".into(),
            handle.clone(),
            processing,
        );
        let spec = if matches!(case, Case::NoActivation) {
            spec.on_activate(|_| Ok(()))
        } else {
            spec
        };
        let assembled = establish_specs_for_test(decode, output, spec, attempt, handle);
        assert_eq!(assembled.admission_failure, None);
        assembled.start
    }

    struct Prepared(RefCell<Option<StartAttempt>>);
    impl EpisodeStart for Prepared {
        fn probe(&self, _: &Path) -> Result<(), String> {
            Ok(())
        }
        fn start(&self, _: &Path, _: u8) -> StartAttempt {
            self.0.borrow_mut().take().unwrap()
        }
    }

    fn providers() -> (ComponentSpec, ComponentSpec) {
        (
            ComponentSpec::new("decode")
                .provides::<PcmDecodeCapability>()
                .on_activate(|ctx| {
                    ctx.provide::<PcmDecodeCapability>(Rc::new(TestDecode::new(
                        SourceBehavior::EofAfter(0),
                    )))
                    .unwrap();
                    Ok(())
                }),
            ComponentSpec::new("output")
                .provides::<AudioOutputCapability>()
                .on_activate(|ctx| {
                    ctx.provide::<AudioOutputCapability>(Rc::new(TestOutput::new(
                        OutputBehavior::Consume,
                    )))
                    .unwrap();
                    Ok(())
                }),
        )
    }

    #[test]
    fn extra_desired_failure_demonstrates_why_the_canonical_scope_is_structural() {
        mechanisms::within(Duration::from_secs(5), || {
            // Real counterexample to the old unrestricted helper: generic Ok
            // plus a successful Session cannot certify an extra failed Fiber.
            let (decode, output) = providers();
            let handle = PlaybackSessionHandle::new();
            let (session, attempt) = playback_session_spec_with_establishment(
                "test://attempt".into(),
                handle.clone(),
                AudioProcessingConfig::BYPASS,
            );
            let mut generic = QianqianApp::new();
            for spec in [
                decode,
                output,
                session,
                ComponentSpec::new("extra")
                    .on_activate(|_| Err(ActivationError::new("independent desired failure"))),
            ] {
                generic.register_component(spec).unwrap();
            }
            generic
                .revise_desired(vec![
                    DesiredEntry::enabled("decode", "decode", Revision::new(1)),
                    DesiredEntry::enabled("output", "output", Revision::new(1)),
                    DesiredEntry::enabled("session", "playback_session", Revision::new(1)),
                    DesiredEntry::enabled("extra", "extra", Revision::new(1)),
                ])
                .unwrap();
            assert_eq!(attempt.finish(), EstablishmentResult::Established);
            assert_eq!(
                generic.composition_snapshot().fibers["extra"].state,
                FiberState::Failed
            );
            generic.dispose();

            // Compile-time boundary guard: neither a prepopulated root nor a
            // caller-controlled desired list can enter the assembly routine.
            #[cfg(feature = "playback")]
            let _: fn(PathBuf, AudioProcessingConfig, u8) -> AssemblyOutcome = establish;
            let canonical: fn(
                ComponentSpec,
                ComponentSpec,
                PathBuf,
                AudioProcessingConfig,
                u8,
            ) -> AssemblyOutcome = establish_with_providers;
            let (decode, output) = providers();
            let mut assembled = canonical(
                decode,
                output,
                "test://attempt".into(),
                AudioProcessingConfig::BYPASS,
                100,
            );
            assert_eq!(
                assembled.start.establishment,
                EstablishmentResult::Established
            );
            // Projection is used only as an independent topology oracle.
            // Adding ANY desired member inside the factory makes this fail,
            // even if the Session slot incorrectly still says Established.
            let snapshot = assembled.start.runtime.composition_snapshot();
            assert_eq!(
                snapshot
                    .fibers
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["decode", "output", "session"]
            );
            assembled.start.runtime.dispose();
        });
    }

    fn refused_assembly(registration: bool) -> AssemblyOutcome {
        let (decode, output) = providers();
        let (decode, output) = if registration {
            // Duplicate component identity is a real registration refusal.
            (decode.clone(), decode)
        } else {
            // Required-single ambiguity is a real desired admission refusal.
            (decode.provides::<AudioOutputCapability>(), output)
        };
        establish_with_providers(
            decode,
            output,
            "test://attempt".into(),
            AudioProcessingConfig::BYPASS,
            100,
        )
    }

    #[test]
    fn admission_failure_has_one_machine_reference_semantic_result() {
        mechanisms::within(Duration::from_secs(5), || {
            for registration in [true, false] {
                let machine = refused_assembly(registration);
                let reference = refused_assembly(registration);
                assert_eq!(machine.start.establishment, reference.start.establishment);
                assert!(matches!(
                    machine.start.establishment,
                    EstablishmentResult::NotEstablished { .. }
                ));
                assert_eq!(machine.start.handle.observe().terminal_outcome, None);
                assert_eq!(reference.start.handle.observe().terminal_outcome, None);
                assert!(
                    machine
                        .start
                        .runtime
                        .composition_snapshot()
                        .fibers
                        .is_empty()
                );
                assert!(
                    matches!(machine.admission_failure,
                    Some(StartFailure::Registration { .. }) if registration)
                        || matches!(machine.admission_failure,
                        Some(StartFailure::CompositionRefused { .. }) if !registration)
                );
                // Both consume the same classification without waiting. Machine
                // retains its separate registration/usage presentation classes.
                assert_eq!(
                    crate::entry::run_machine_assembly_for_test(machine),
                    std::process::ExitCode::from(if registration { 1 } else { 2 })
                );
                let mut player =
                    ReferencePlayerApp::new(Prepared(RefCell::new(Some(reference.start))));
                assert!(matches!(
                    player.open(Path::new("test://attempt")),
                    OpenOutcome::ActivationFailedClean { .. }
                ));
                assert!(player.active_handle().is_none());

                // Removing every presentation field cannot change semantics.
                let mut machine = refused_assembly(registration);
                machine.start.establishment =
                    EstablishmentResult::NotEstablished { diagnostic: None };
                machine.admission_failure = None;
                assert_eq!(
                    crate::entry::run_machine_assembly_for_test(machine),
                    std::process::ExitCode::from(1)
                );
            }
        });
    }

    #[test]
    fn machine_and_reference_consume_the_same_failure_result_without_terminal_wait() {
        mechanisms::within(Duration::from_secs(5), || {
            for case in [
                Case::Provider,
                Case::Dependency,
                Case::Session,
                Case::Processing,
                Case::NoActivation,
            ] {
                let disposed = Rc::new(Cell::new(false));
                let machine = wire_with_disposal(case, disposed.clone());
                let reference = wire(case);
                assert_eq!(machine.establishment, reference.establishment);
                assert!(matches!(
                    machine.establishment,
                    EstablishmentResult::NotEstablished { .. }
                ));
                assert_eq!(machine.handle.observe().terminal_outcome, None);
                assert_eq!(
                    crate::entry::run_machine_attempt_for_test(
                        machine.runtime,
                        machine.handle,
                        machine.establishment
                    ),
                    std::process::ExitCode::from(1)
                );
                assert!(disposed.get());
                let mut player = ReferencePlayerApp::new(Prepared(RefCell::new(Some(reference))));
                assert!(matches!(
                    player.open(Path::new("test://attempt")),
                    OpenOutcome::ActivationFailedClean { .. }
                ));
                assert!(player.active_handle().is_none());
            }
        });
    }

    #[test]
    fn active_fiber_and_absent_diagnostic_cannot_replace_the_session_result() {
        let mut wired = wire(Case::NoActivation);
        assert_eq!(
            wired.runtime.composition_snapshot().fibers["session"].state,
            FiberState::Active
        );
        assert_eq!(wired.handle.observe().activation_error, None);
        assert_eq!(
            wired.establishment,
            EstablishmentResult::NotEstablished { diagnostic: None }
        );
        assert_eq!(
            wired.runtime.dispose().verdict,
            qianqian_composition::DisposeVerdict::Discharged
        );
    }

    #[test]
    fn consumers_cannot_override_not_established_with_source_or_active_evidence() {
        mechanisms::within(Duration::from_secs(5), || {
            // Deliberate host-contract negative control: retain convincing
            // presentation evidence while supplying a failed operation result.
            // Production partial-acquisition failures are tested in playback.
            for reference in [false, true] {
                let mut wired = wire(Case::Eof);
                assert_eq!(
                    wired.handle.wait_terminal(),
                    qianqian_playback::EpisodeTerminalOutcome::Completed
                );
                assert!(wired.handle.observe().source_format.is_some());
                assert_eq!(wired.handle.observe().activation_error, None);
                assert_eq!(
                    wired.runtime.composition_snapshot().fibers["session"].state,
                    FiberState::Active
                );
                wired.establishment = EstablishmentResult::NotEstablished { diagnostic: None };
                if reference {
                    let mut player = ReferencePlayerApp::new(Prepared(RefCell::new(Some(wired))));
                    assert!(matches!(
                        player.open(Path::new("test://attempt")),
                        OpenOutcome::ActivationFailedClean { .. }
                    ));
                    assert!(player.active_handle().is_none());
                } else {
                    assert_eq!(
                        crate::entry::run_machine_attempt_for_test(
                            wired.runtime,
                            wired.handle,
                            wired.establishment
                        ),
                        std::process::ExitCode::from(1)
                    );
                }
            }
        });
    }

    #[test]
    fn terminal_before_host_consumption_does_not_erase_establishment() {
        mechanisms::within(Duration::from_secs(5), || {
            for (case, terminal) in [
                (
                    Case::Eof,
                    qianqian_playback::EpisodeTerminalOutcome::Completed,
                ),
                (
                    Case::RuntimeFailure,
                    qianqian_playback::EpisodeTerminalOutcome::Failed,
                ),
            ] {
                let wired = wire(case);
                assert_eq!(wired.establishment, EstablishmentResult::Established);
                assert_eq!(wired.handle.wait_terminal(), terminal);
                let mut player = ReferencePlayerApp::new(Prepared(RefCell::new(Some(wired))));
                assert_eq!(
                    player.open(Path::new("test://attempt")),
                    OpenOutcome::Opened
                );
                assert_eq!(player.quit().terminal, Some(terminal));
                let wired = wire(case);
                assert_eq!(wired.handle.wait_terminal(), terminal);
                assert_eq!(
                    crate::entry::run_machine_attempt_for_test(
                        wired.runtime,
                        wired.handle,
                        wired.establishment
                    ),
                    if terminal == qianqian_playback::EpisodeTerminalOutcome::Completed {
                        std::process::ExitCode::SUCCESS
                    } else {
                        std::process::ExitCode::from(1)
                    }
                );
            }
        });
    }
}
