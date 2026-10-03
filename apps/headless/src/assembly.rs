//! Shared consumption of the fresh assembly operation, PBK-002 D14.6.

use qianqian_app::QianqianApp;
use qianqian_composition::{CompositionErrors, DesiredEntry};
use qianqian_playback::{EstablishmentAttempt, EstablishmentResult};

/// Both hosts invoke this with their fresh Decode/Output/Session composition.
/// K0 admission success permits consuming the Session-owned attempt result;
/// it never substitutes for that result. No projection participates.
pub(crate) fn establish(
    runtime: &mut QianqianApp,
    attempt: EstablishmentAttempt,
    desired: Vec<DesiredEntry>,
) -> Result<EstablishmentResult, CompositionErrors> {
    runtime.revise_desired(desired)?;
    Ok(attempt.finish())
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
        let mut runtime = QianqianApp::new();
        runtime
            .register_component(
                ComponentSpec::new("decode")
                    .provides::<PcmDecodeCapability>()
                    .on_activate(move |ctx| {
                        if matches!(case, Case::Provider) {
                            return Err(ActivationError::new("deliberate provider failure"));
                        }
                        let source = if matches!(case, Case::RuntimeFailure) {
                            SourceBehavior::FailAfter(0)
                        } else {
                            SourceBehavior::EofAfter(0)
                        };
                        ctx.provide::<PcmDecodeCapability>(Rc::new(TestDecode::new(source)))
                            .unwrap();
                        Ok(())
                    }),
            )
            .unwrap();
        runtime
            .register_component(
                ComponentSpec::new("output")
                    .provides::<AudioOutputCapability>()
                    .on_activate(move |ctx| {
                        let output = if matches!(case, Case::Session) {
                            OutputBehavior::FailOpen
                        } else {
                            OutputBehavior::Consume
                        };
                        ctx.provide::<AudioOutputCapability>(Rc::new(TestOutput::new(output)))
                            .unwrap();
                        Ok(())
                    }),
            )
            .unwrap();
        let handle = PlaybackSessionHandle::new();
        let processing = if matches!(case, Case::Processing) {
            AudioProcessingConfig::gain(f32::NAN)
        } else {
            AudioProcessingConfig::BYPASS
        };
        let (spec, attempt) = qianqian_playback::playback_session_spec_with_establishment(
            "test://attempt".into(),
            handle.clone(),
            processing,
        );
        let spec = if matches!(case, Case::NoActivation) {
            spec.on_activate(|_| Ok(()))
        } else {
            spec
        };
        runtime.register_component(spec).unwrap();
        let mut desired = vec![
            DesiredEntry::enabled("output", "output", Revision::new(1)),
            DesiredEntry::enabled("session", "playback_session", Revision::new(1)),
        ];
        if !matches!(case, Case::Dependency) {
            desired.push(DesiredEntry::enabled("decode", "decode", Revision::new(1)));
        }
        let establishment = establish(&mut runtime, attempt, desired)
            .expect("K0 admission succeeds even on failure");
        StartAttempt {
            runtime,
            handle,
            establishment,
        }
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
                let machine = wire(case);
                let reference = wire(case);
                assert_eq!(machine.establishment, reference.establishment);
                assert!(matches!(
                    machine.establishment,
                    EstablishmentResult::NotEstablished { .. }
                ));
                assert_eq!(machine.handle.observe().terminal_outcome, None);
                let disposed = Rc::new(Cell::new(false));
                // Independent disposal witness; a host which waits for a nonexistent
                // terminal cannot reach this inverse (the outer watchdog catches it).
                let marker = disposed.clone();
                let mut root = machine.runtime;
                root.register_component(ComponentSpec::new("witness").on_activate(move |ctx| {
                    let marker = marker.clone();
                    ctx.register_effect(move || {
                        marker.set(true);
                        Discharge::Discharged
                    });
                    Ok(())
                }))
                .unwrap();
                // The witness is an independent test-only mounted effect.
                // Preserve the original desired wiring to avoid changing its result.
                let mut desired = vec![
                    DesiredEntry::enabled("output", "output", Revision::new(1)),
                    DesiredEntry::enabled("session", "playback_session", Revision::new(1)),
                    DesiredEntry::enabled("witness", "witness", Revision::new(1)),
                ];
                if !matches!(case, Case::Dependency) {
                    desired.push(DesiredEntry::enabled("decode", "decode", Revision::new(1)));
                }
                root.revise_desired(desired).unwrap();
                assert_eq!(
                    crate::entry::run_machine_attempt_for_test(
                        root,
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
