//! C1 failure/terminal oracles at the real activation continuation.
use super::*;
use crate::test_common::{OutputBehavior, SourceBehavior, TEST_FORMAT, TestDecode, TestOutput};
use qianqian_app::QianqianApp;
use qianqian_audio_api::ports::{AudioOutput, OutputError, RenderRequest, RenderStream};
use qianqian_composition::{DesiredEntry, DisposeVerdict, Revision};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

fn desired(id: &'static str) -> DesiredEntry {
    DesiredEntry::enabled(id, id, Revision::new(1))
}

fn providers(source: SourceBehavior, output: OutputBehavior) -> QianqianApp {
    let mut root = QianqianApp::new();
    root.register_component(
        ComponentSpec::new("decode")
            .provides::<PcmDecodeCapability>()
            .on_activate(move |ctx| {
                ctx.provide::<PcmDecodeCapability>(Rc::new(TestDecode::new(source)))
                    .unwrap();
                Ok(())
            }),
    )
    .unwrap();
    root.register_component(
        ComponentSpec::new("output")
            .provides::<AudioOutputCapability>()
            .on_activate(move |ctx| {
                ctx.provide::<AudioOutputCapability>(Rc::new(TestOutput::new(output)))
                    .unwrap();
                Ok(())
            }),
    )
    .unwrap();
    root
}

#[test]
fn source_evidence_then_processing_failure_is_not_established() {
    let handle = PlaybackSessionHandle::new();
    let mut root = providers(SourceBehavior::EofAfter(0), OutputBehavior::Consume);
    let (spec, attempt) = playback_session_spec_with_establishment(
        "test://invalid-processing".into(),
        handle.clone(),
        AudioProcessingConfig::gain(f32::NAN),
    );
    root.register_component(spec).unwrap();
    assert!(
        root.revise_desired(vec![
            desired("decode"),
            desired("output"),
            DesiredEntry::enabled(
                "session",
                "playback_session",
                qianqian_composition::Revision::new(1)
            )
        ])
        .is_ok()
    );
    assert_eq!(handle.observe().source_format, Some(TEST_FORMAT));
    assert!(matches!(
        attempt.finish(),
        EstablishmentResult::NotEstablished {
            diagnostic: Some(_)
        }
    ));
    assert_eq!(handle.observe().terminal_outcome, None);
    assert_eq!(root.dispose().verdict, DisposeVerdict::Discharged);
}

struct TrackedOutput {
    opened: Arc<AtomicBool>,
    joined: Arc<AtomicBool>,
}
struct TrackedStream {
    joined: Arc<AtomicBool>,
}
impl RenderStream for TrackedStream {
    fn negotiated_format(&self) -> PcmFormat {
        TEST_FORMAT
    }
    fn stop_and_join(self: Box<Self>) {
        self.joined.store(true, Ordering::SeqCst);
    }
}
impl AudioOutput for TrackedOutput {
    fn open_stream(&self, _: RenderRequest) -> Result<Box<dyn RenderStream>, OutputError> {
        self.opened.store(true, Ordering::SeqCst);
        Ok(Box::new(TrackedStream {
            joined: self.joined.clone(),
        }))
    }
}

struct TrackedDecode {
    dropped: Arc<AtomicBool>,
}
struct TrackedEndpoint {
    inner: Box<dyn DecodedPcmStream>,
    dropped: Arc<AtomicBool>,
}
impl Drop for TrackedEndpoint {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}
impl DecodedPcmStream for TrackedEndpoint {
    fn format(&self) -> PcmFormat {
        self.inner.format()
    }
    fn source_duration(&self) -> Option<Duration> {
        self.inner.source_duration()
    }
    fn read_frames(
        &mut self,
        dst: &mut [f32],
    ) -> Result<DecodeOutcome, qianqian_audio_api::ports::DecodeError> {
        self.inner.read_frames(dst)
    }
    fn seek(&mut self, target: Duration) -> qianqian_audio_api::ports::ProviderSeekOutcome {
        self.inner.seek(target)
    }
}
impl qianqian_audio_api::ports::PcmDecode for TrackedDecode {
    fn open_media(
        &self,
        path: &Path,
    ) -> Result<Box<dyn DecodedPcmStream>, qianqian_audio_api::ports::DecodeOpenError> {
        let inner = qianqian_audio_api::ports::PcmDecode::open_media(
            &TestDecode::new(SourceBehavior::EofAfter(0)),
            path,
        )?;
        Ok(Box::new(TrackedEndpoint {
            inner,
            dropped: self.dropped.clone(),
        }))
    }
}

#[test]
fn render_open_then_worker_spawn_failure_unwinds_and_never_establishes() {
    let handle = PlaybackSessionHandle::new();
    let mut root = providers(SourceBehavior::EofAfter(0), OutputBehavior::Consume);
    // Distinct definition, selected instead of the ordinary test output.
    let opened = Arc::new(AtomicBool::new(false));
    let joined = Arc::new(AtomicBool::new(false));
    let service = Rc::new(TrackedOutput {
        opened: opened.clone(),
        joined: joined.clone(),
    });
    root.register_component(
        ComponentSpec::new("tracked")
            .provides::<AudioOutputCapability>()
            .on_activate(move |ctx| {
                ctx.provide::<AudioOutputCapability>(service.clone())
                    .unwrap();
                Ok(())
            }),
    )
    .unwrap();
    let dropped = Arc::new(AtomicBool::new(false));
    let service = Rc::new(TrackedDecode {
        dropped: dropped.clone(),
    });
    root.register_component(
        ComponentSpec::new("tracked_decode")
            .provides::<PcmDecodeCapability>()
            .on_activate(move |ctx| {
                ctx.provide::<PcmDecodeCapability>(service.clone()).unwrap();
                Ok(())
            }),
    )
    .unwrap();
    let attempt = EstablishmentAttempt::new();
    let slot = attempt.0.clone();
    let completion = handle.completion.clone();
    root.register_component(
        ComponentSpec::new("session")
            .requires::<PcmDecodeCapability>()
            .requires::<AudioOutputCapability>()
            .on_activate(move |ctx| {
                let result = activate_established_with_spawn(
                    Path::new("test://spawn-failure"),
                    &completion,
                    |format| EpisodeProcessing::new(&AudioProcessingConfig::BYPASS, format),
                    ctx,
                    |endpoint, _edge, _completion, _processing| {
                        drop(endpoint); // Builder::spawn drops its task on failure.
                        Err(ActivationError::new(
                            "decode worker spawn failed: deliberate test refusal",
                        ))
                    },
                );
                if let Err(error) = &result {
                    completion.activation_failed(&error.message);
                }
                record_establishment(&slot, &result);
                result
            }),
    )
    .unwrap();
    root.revise_desired(vec![
        desired("tracked_decode"),
        desired("tracked"),
        desired("session"),
    ])
    .unwrap();
    assert!(
        opened.load(Ordering::SeqCst),
        "the render resource really opened"
    );
    assert!(
        dropped.load(Ordering::SeqCst),
        "failed spawn drops the decode endpoint task capture"
    );
    assert!(
        joined.load(Ordering::SeqCst),
        "K0 raising-activation unwind joins the opened stream"
    );
    assert!(matches!(
        attempt.finish(),
        EstablishmentResult::NotEstablished { .. }
    ));
    assert_eq!(handle.observe().terminal_outcome, None);
    assert_eq!(root.dispose().verdict, DisposeVerdict::Discharged);
}

#[test]
fn completed_before_result_consumption_remains_established_without_a_fiber() {
    crate::test_common::within(Duration::from_secs(5), || {
        let handle = PlaybackSessionHandle::new();
        let mut root = providers(SourceBehavior::EofAfter(0), OutputBehavior::Consume);
        let (spec, attempt) = playback_session_spec_with_establishment(
            "test://eof".into(),
            handle.clone(),
            AudioProcessingConfig::BYPASS,
        );
        root.register_component(spec).unwrap();
        root.revise_desired(vec![
            desired("decode"),
            desired("output"),
            DesiredEntry::enabled("session", "playback_session", Revision::new(1)),
        ])
        .unwrap();
        assert_eq!(
            handle.wait_terminal(),
            crate::EpisodeTerminalOutcome::Completed
        );
        // Diagnostic mutation cannot revoke the producing operation's result.
        handle
            .completion
            .activation_failed("non-authoritative diagnostic control");
        assert!(handle.observe().activation_error.is_some());
        assert_eq!(root.dispose().verdict, DisposeVerdict::Discharged);
        assert!(root.composition_snapshot().fibers.is_empty());
        assert_eq!(attempt.finish(), EstablishmentResult::Established);
    });
}

// Test-only release makes “later runtime failure” an ordered event, independent
// of thread scheduling. Dropping the sender also releases a panicking test.
struct HeldFailureDecode(std::cell::RefCell<Option<std::sync::mpsc::Receiver<()>>>);
struct HeldFailureEndpoint(std::sync::mpsc::Receiver<()>);
impl qianqian_audio_api::ports::PcmDecode for HeldFailureDecode {
    fn open_media(
        &self,
        _: &Path,
    ) -> Result<Box<dyn DecodedPcmStream>, qianqian_audio_api::ports::DecodeOpenError> {
        Ok(Box::new(HeldFailureEndpoint(
            self.0.borrow_mut().take().unwrap(),
        )))
    }
}
impl DecodedPcmStream for HeldFailureEndpoint {
    fn format(&self) -> PcmFormat {
        TEST_FORMAT
    }
    fn source_duration(&self) -> Option<Duration> {
        None
    }
    fn read_frames(
        &mut self,
        _: &mut [f32],
    ) -> Result<DecodeOutcome, qianqian_audio_api::ports::DecodeError> {
        let _ = self.0.recv();
        Err(qianqian_audio_api::ports::DecodeError {
            message: "controlled later runtime failure".into(),
        })
    }
    fn seek(&mut self, _: Duration) -> qianqian_audio_api::ports::ProviderSeekOutcome {
        qianqian_audio_api::ports::ProviderSeekOutcome::RefusedUnchanged
    }
}

#[test]
fn runtime_failure_remains_distinct_from_establishment() {
    crate::test_common::within(Duration::from_secs(5), || {
        let handle = PlaybackSessionHandle::new();
        let mut root = providers(SourceBehavior::EofAfter(0), OutputBehavior::Consume);
        let (release, held) = std::sync::mpsc::channel();
        let service = Rc::new(HeldFailureDecode(std::cell::RefCell::new(Some(held))));
        root.register_component(
            ComponentSpec::new("held_decode")
                .provides::<PcmDecodeCapability>()
                .on_activate(move |ctx| {
                    ctx.provide::<PcmDecodeCapability>(service.clone()).unwrap();
                    Ok(())
                }),
        )
        .unwrap();
        let (spec, attempt) = playback_session_spec_with_establishment(
            "test://decode-failure".into(),
            handle.clone(),
            AudioProcessingConfig::BYPASS,
        );
        root.register_component(spec).unwrap();
        root.revise_desired(vec![
            desired("held_decode"),
            desired("output"),
            DesiredEntry::enabled("session", "playback_session", Revision::new(1)),
        ])
        .unwrap();
        let result = attempt.finish();
        assert_eq!(result, EstablishmentResult::Established);
        assert_eq!(handle.observe().terminal_outcome, None);
        release.send(()).unwrap();
        assert_eq!(
            handle.wait_terminal(),
            crate::EpisodeTerminalOutcome::Failed
        );
        assert_eq!(handle.observe().activation_error, None);
        assert_eq!(result, EstablishmentResult::Established);
        assert_eq!(root.dispose().verdict, DisposeVerdict::Discharged);
    });
}
