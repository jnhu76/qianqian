//! C1 failure/terminal oracles at the real activation continuation.
use super::*;
use crate::test_common::{
    DeviceTail, OutputBehavior, SourceBehavior, StreamLifecycle, TEST_FORMAT, TailProbe,
    TestDecode, TestOutput,
};
use qianqian_app::QianqianApp;
use qianqian_audio_api::ports::{AudioOutput, OutputError, RenderRequest, RenderStream};
use qianqian_composition::{DesiredEntry, DisposeVerdict, FiberState, Revision};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;

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

/// Providers for a session driven with a caller-owned output service.
fn providers_with_service(service: Rc<TestOutput>, source: SourceBehavior) -> QianqianApp {
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
                ctx.provide::<AudioOutputCapability>(service.clone())
                    .unwrap();
                Ok(())
            }),
    )
    .unwrap();
    root
}

/// Test-owned failure cleanup — a RELEASE guarantee, not a join
/// guarantee: on any exit (including an assert unwind) the held tail
/// observation is released, so a premise failure cannot leave the leg
/// blocked behind fixture state. The join is observed separately at the
/// teardown boundary.
struct UnholdOnDrop(TailProbe);

impl Drop for UnholdOnDrop {
    fn drop(&mut self) {
        self.0.unhold();
    }
}

/// Test-owned failure cleanup — a RELEASE guarantee, not a join
/// guarantee: it is owned by the OUTER test thread (not the guarded
/// body), because a `within` watchdog timeout panics the outer thread
/// while the body stays blocked and never unwinds. Closing the gate here
/// therefore frees a fixture-owned parked leg on both the panic and the
/// timeout paths. It cannot make the oracle pass: on the normal path the
/// witnesses are sampled before the test returns and this guard fires
/// last; on the timeout path the watchdog is the verdict, and any later
/// body assertion is not evidence.
struct CloseGateOnDrop(qianqian_audio_api::ports::RenderGate);

impl Drop for CloseGateOnDrop {
    fn drop(&mut self) {
        self.0.close_and_release();
    }
}

/// Given an establishment window (`bind_stop_target` → `open_stream` →
/// worker spawn) in which a concurrent REAL `request_seek` has already
/// planted an internal seek hold, and a decode-worker spawn seam that
/// refuses, when the open succeeds and activation raises at the spawn,
/// then the K0 unwind must release the seek-parked leg before
/// `stop_and_join`, the leg must really join, and the plant must remain
/// unconsumed and unaccepted with no terminal Fact forged.
///
/// PROPERTY / AUTHORITY: the D14.7 teardown wake obligation extended to
/// the D14.5 seek park (ADR-PBK-002), driven through the REAL
/// `activate_established_with_spawn` and the REAL K0 raising-activation
/// unwind; the D11 activation firewall for the terminal outcome. This is
/// the seek-planted counterpart of
/// [`render_open_then_worker_spawn_failure_unwinds_and_never_establishes`]:
/// that fixture's stream joins instantly, so it cannot witness the seek
/// hold release at all.
///
/// PRECONDITION: the output double signals the window after the session
/// bound its stop target, and waits for the real `request_seek` to
/// return before spawning the leg, so the hold is routed strictly before
/// the leg's first gate visit; the armed tail observation (called only
/// from inside the park) proves the leg really parked. The fixture
/// establishes this ordering; production admits it but does not promise
/// it. The requester's unhold is released on the panic path too.
///
/// OBSERVATION: the join witness is the mock stream's `stop_and_join`
/// return path (`lifecycle.joined()`), sampled after `revise_desired`
/// returned. It proves the thread terminated, NOT a panic-free loop exit:
/// the mock leg catches panics inside `consume_loop`, so `joined()` is
/// true even after a swallowed loop panic. The test holds `lifecycle_lock`,
/// which every settlement leak oracle in this binary also takes, so those
/// oracles cannot scan this test's leg; the test deliberately does NOT
/// scan the global name itself. The no-leak absence observation is
/// therefore taken in an isolated run (evidence record), never as a
/// parallel-suite premise.
///
/// FALSIFIER (executed negative control, M-t): delete
/// `set_seek_hold(false)` in `release_pause_gate` and the join wedges
/// forever — the 30s watchdog fails and the outer guard releases the
/// stranded leg on the timeout path, so neighbors are not contaminated.
#[test]
fn worker_spawn_failure_discharges_a_seek_parked_leg_before_the_join() {
    let _lifecycle = crate::test_common::lifecycle_lock();
    // Owned by this (outer) thread: see CloseGateOnDrop. The handle is
    // created here so the guard outlives the watchdog's timeout panic.
    let handle = PlaybackSessionHandle::new();
    let _gate_release = CloseGateOnDrop(handle.completion.render_gate());
    crate::test_common::within(Duration::from_secs(30), move || {
        // Given: a session over the establishment-window output double.
        let completion = handle.completion.clone();
        let tail_probe = TailProbe::default();
        tail_probe.arm();
        let lifecycle = StreamLifecycle::default();
        let lifecycle_witness = lifecycle.clone();
        let (window_tx, window_rx) = mpsc::channel::<()>();
        let (plant_tx, plant_rx) = mpsc::channel::<()>();
        let service = Rc::new(TestOutput::open_after_seek_plant(
            Arc::new(AtomicUsize::new(0)),
            DeviceTail::default(),
            tail_probe.clone(),
            lifecycle,
            (window_tx, plant_rx),
        ));
        let mut root = providers_with_service(service, SourceBehavior::EofAfter(0));

        let attempt = EstablishmentAttempt::new();
        let slot = attempt.0.clone();
        let session_completion = completion.clone();
        root.register_component(
            ComponentSpec::new("session")
                .requires::<PcmDecodeCapability>()
                .requires::<AudioOutputCapability>()
                .on_activate(move |ctx| {
                    let result = activate_established_with_spawn(
                        Path::new("test://spawn-failure-after-seek-plant"),
                        &session_completion,
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
                        session_completion.activation_failed(&error.message);
                    }
                    record_establishment(&slot, &result);
                    result
                }),
        )
        .unwrap();

        // The concurrent requester exercises the REAL admission path
        // (the stdin-thread shape), then releases the held observation so
        // the unwind can never deadlock on fixture state.
        let r_completion = completion.clone();
        let requester_probe = tail_probe.clone();
        let requester = std::thread::spawn(move || {
            let _unhold = UnholdOnDrop(requester_probe.clone());
            assert_eq!(
                window_rx.recv_timeout(Duration::from_secs(5)),
                Ok(()),
                "the establishment window never opened"
            );
            r_completion.request_seek(Duration::from_secs(7));
            assert!(
                r_completion.seek_in_flight(),
                "premise: the plant really landed inside the open window"
            );
            plant_tx.send(()).expect("plant latch");
            assert!(
                requester_probe.wait_held_within(Duration::from_secs(5)),
                "the leg never seek-parked at its first loop-top"
            );
            // The explicit release edge; the guard only covers panics.
            requester_probe.unhold();
        });

        // When: activation raises at the worker spawn and K0 unwinds the
        // registered output relation on this thread.
        root.revise_desired(vec![
            desired("decode"),
            desired("output"),
            DesiredEntry::enabled("session", "session", Revision::new(1)),
        ])
        .expect("the composition is legal");
        requester.join().expect("the requester returned");

        // Then: the teardown release discharged the park and the real
        // join returned.
        assert!(
            lifecycle_witness.joined(),
            "the parked seek leg joined: release_pause_gate must discharge \
             the hold before stop_and_join"
        );

        // Admission truth: an internal plant, never Accepted — no worker
        // ever existed to consume the command or resolve the protocol.
        assert_eq!(
            completion.take_seek_command(),
            Some(Duration::from_secs(7)),
            "the planted command must remain unconsumed"
        );
        assert_eq!(
            completion.seek_protocol_state(),
            (false, false, None),
            "no refusal/commit/landing: the plant never refined to protocol acceptance"
        );
        // The inert slot fragment on the dead episode is deliberately
        // NOT asserted either way: no authority requires teardown to
        // clear it, and pinning the residue would freeze an internal
        // detail rather than the property.

        // D11 activation firewall: an establishment that never completed
        // forges no terminal Fact.
        let observation = handle.observe();
        assert_eq!(
            observation.terminal_outcome, None,
            "a spawn failure must never forge a terminal Fact: {observation:?}"
        );
        assert!(
            observation
                .activation_error
                .as_deref()
                .is_some_and(|m| m.contains("decode worker spawn failed")),
            "activation diagnostic: {observation:?}"
        );
        assert!(matches!(
            attempt.finish(),
            EstablishmentResult::NotEstablished { .. }
        ));
        assert_eq!(
            root.composition_snapshot()
                .fibers
                .get("session")
                .map(|f| f.state),
            Some(FiberState::Failed),
            "the raise lands the session FAILED"
        );
        assert_eq!(root.dispose().verdict, DisposeVerdict::Discharged);
    });
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
