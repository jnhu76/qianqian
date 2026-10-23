//! O-CONE-NECESSITY and O-CONE-ORDER (error-system-migration §7.1/§7.2): the
//! REAL failure-publication cone, driven on a real Windows render endpoint.
//!
//! **Property authority.** D11 settlement ownership (PBK-002 §17): the cone's
//! own `render_input.stop()` + `drain.complete(...)` must propagate a render-leg
//! failure to a waiting host with NO external stop/dispose — `wait_terminal`
//! returns and the episode commits `Failed{device}`. `Failed` is the required
//! class here, not merely a non-`Completed`: with the drain verdict `Aborted` and
//! the worker terminal `Stopped` the resolver discriminates on recorded stop
//! intent (`completion.rs::resolve`), so a `Stopped` outcome in this oracle would
//! mean an external stop (or a stop request) had somehow been recorded, which the
//! harness structurally never does.
//!
//! **Why no existing test can serve.** The session-level oracles bind a
//! test-owned `AudioOutput` whose test leg performs its own `stop()`/
//! `drain.complete()` and whose teardown calls `dispose()`; no line of
//! `wasapi.rs` executes. There is also no platform-neutral tier: the whole
//! module is `#[cfg(windows)]`, so off Windows the cone does not exist in the
//! binary at all and an activation refusal is not a live episode. This oracle is
//! therefore Windows-only and needs a real render endpoint.
//!
//! **Harness shape (no production change).** The test registers a test-owned
//! `AudioOutput` that (a) wraps the session's real edge-backed
//! `RenderPcmInput` in a panicking-on-the-k-th-read wrapper, (b) relays the
//! verdict through an observer that snapshots the edge-cut counter, and (c)
//! delegates to the REAL `WasapiOutput`, so the whole production cone — catch,
//! `stop()`, `complete()`, steady loop, device release, join — is unmodified
//! production code. The injection is at the input-trait boundary.
//!
//! **Regime (each element a fixture fact, not an inference).** The decode
//! source never returns `Eof`, never returns a decode error and never returns
//! the zero-frame stall (one test-owned source with exactly one read body); no
//! seek command exists anywhere in the harness; and between the injection and
//! the settlement observation the test body contains no `request_stop`,
//! `request_resume`, `request_seek` or `dispose` — the only call is the blocking
//! `wait_terminal` on a helper thread. Freezing the consumer is the injection
//! itself: after the k-th read panics the render loop is dead, so edge
//! consumption stays frozen for the rest of the run (program order on the render
//! thread, not a scheduling assumption).
//!
//! **Necessity argument (outcome level; no claim about where the worker stands
//! at injection).** With no external stop and the cone's stop deleted, the
//! worker's exits are exhausted: the regime excludes the read-Err, EOF,
//! processing-failure and MutatedThenFailed arms, and every remaining
//! terminal-observed exit requires `edge.terminal() != Open` — which only the
//! cone's `stop()` can produce here. So `worker_exited(...)` is never published,
//! `Aborted + worker=None` is outside the decisive domain, and `wait_terminal`
//! never returns. The harness bound firing IS that oracle's red.
//!
//! **Order witnesses (program order, no scheduling dependence).**
//! * A — the edge cut: the wrapper's `stop()` forwards to the real input's
//!   `stop()` first and then bumps counter A, on whichever thread invoked it. In
//!   the cone the invoker is the render thread itself, in the post-catch arm,
//!   strictly before `complete()`.
//! * B — the verdict becoming visible: the relay observer runs synchronously
//!   inside `complete()`, before it returns, and snapshots A before forwarding
//!   the verdict to the session's own signal. Assertion at the settlement
//!   boundary: on an aborted outcome, B's snapshot of A must be ≥ 1.
//!   Counter A is monotonic and shared with the legitimate
//!   `stop_and_join` caller, so later increments cannot weaken it.
//!
//! **Failure signatures.** Precondition never reached (k-th read entered /
//! injection fired) → TOOLING-FAIL: report the precondition, never the property
//! (a Windows host with no usable render endpoint lands here, and an activation
//! refusal must never be dressed up as a live episode's settle).
//! `wait_terminal` not returning inside the bound → the necessity red. Terminal
//! ≠ `Failed` → a stop of some kind was recorded, or the cone reported
//! something other than an abort. `verdict_reached == false` or
//! `snapshot(A) == 0` → the order red.
//!
//! **Negative controls (isolated environment only; never committed).**
//! 1. Delete `render_input.stop()` in the cone (`wasapi.rs`, post-catch arm):
//!    the necessity oracle must go red by the harness bound firing.
//! 2. Swap `stop()` and `complete()` there: the order oracle must go red with
//!    `snapshot(A) == 0` while the necessity oracle stays green — deletion and
//!    swapped order are independent controls, and neither substitutes for the
//!    other.
//!    Run each mutation in a scratch checkout on a Windows host, then restore
//!    the bytes; a mutation that merely fails to compile is not a control
//!    result.
//!
//! **Cleanup/timeout evidence boundary.** The fixture releases the real device
//! after the settle assertions (the render-thread join evidence). On a red run
//! the same release is performed only to free the fixture: that path is never
//! asserted as the success path, and its post-cleanup state proves nothing about
//! the property.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use qianqian_app::QianqianApp;
use qianqian_audio_api::ports::{
    AudioOutput, AudioOutputCapability, DecodeError, DecodeOpenError, DecodeOutcome,
    DecodedPcmStream, DrainSignal, DrainVerdict, OutputError, PcmDecode, PcmDecodeCapability,
    PcmFormat, PcmPull, ProviderSeekOutcome, RenderPcmInput, RenderRequest, RenderStream,
};
use qianqian_composition::{ActivationError, ComponentSpec, DesiredEntry, Revision};
use qianqian_playback::{EpisodeTerminalOutcome, PlaybackSessionHandle, playback_session_spec};

use crate::wasapi::WasapiOutput;

const EPISODE_PATH: &str = "oracle://cone-necessity";

/// The device format the harness decodes at. The real endpoint's own accepted
/// format is whatever the mechanism negotiates; this is only what the source
/// declares.
const TEST_FORMAT: PcmFormat = PcmFormat {
    sample_rate: 44_100,
    channels: 2,
    channel_mask: 0x3,
};

/// The read index at which the render leg is injected. Small enough that the
/// injection lands while the edge is still open, so the cone's own stop is what
/// terminalizes the data plane (that is the necessity argument's premise).
const INJECT_AT: usize = 3;

/// Bound for the precondition witness: the render leg reaching its k-th read on
/// a freshly opened real endpoint, device buffer included.
const PRECONDITION_BOUND: Duration = Duration::from_secs(10);

/// Bound for the settlement observation. Generous on purpose: its firing is the
/// oracle's red, and a timed-out healthy run would otherwise be inconclusive
/// rather than a failure.
pub(crate) const SETTLE_BOUND: Duration = Duration::from_secs(15);

// --- the healthy, never-terminal decode source -------------------------------

/// A source that always has audio: silence frames, never `Eof`, never an error,
/// never the zero-frame stall, and a refused seek. It is the regime's producer
/// side, so it lives in exactly one place with one read body.
struct InfiniteSource;

impl PcmDecode for InfiniteSource {
    fn open_media(
        &self,
        _path: &std::path::Path,
    ) -> Result<Box<dyn DecodedPcmStream>, DecodeOpenError> {
        Ok(Box::new(InfiniteStream))
    }
}

struct InfiniteStream;

impl DecodedPcmStream for InfiniteStream {
    fn format(&self) -> PcmFormat {
        TEST_FORMAT
    }

    fn source_duration(&self) -> Option<Duration> {
        None
    }

    fn read_frames(&mut self, dst: &mut [f32]) -> Result<DecodeOutcome, DecodeError> {
        // Healthy frames, always the full request: no fault path and no
        // zero-frame response exist in this source.
        dst.fill(0.0);
        Ok(DecodeOutcome::Frames(
            dst.len() / TEST_FORMAT.channels as usize,
        ))
    }

    fn seek(&mut self, _target: Duration) -> ProviderSeekOutcome {
        ProviderSeekOutcome::RefusedUnchanged
    }
}

// --- the injection + order witness ------------------------------------------

#[derive(Default)]
pub(crate) struct Witness {
    /// The render leg's `read_frames` call count (observation (ii)).
    reads: AtomicUsize,
    /// Set immediately before the injected panic, on the render thread: the
    /// same-thread happens-before edge into the cone's catch (observation (i)).
    pub(crate) injected: AtomicBool,
    /// Counter A: completed edge cuts (the wrapper's `stop()` forwarding).
    pub(crate) edge_cuts: AtomicUsize,
    /// Counter B: set by the relay observer inside `complete()`.
    pub(crate) verdict_reached: AtomicBool,
    /// B's snapshot of A, taken inside `complete()` before it returns.
    pub(crate) verdict_snapshot: AtomicUsize,
    /// The verdict the relay observed inside `complete()` — the carrier B
    /// payload as it crossed the completion seam (A6 pipeline witness).
    /// Written only by the relay (synchronously inside `complete()`), read
    /// only by the test thread after settlement.
    pub(crate) relayed_verdict: std::sync::Mutex<Option<DrainVerdict>>,
}

/// Evidence printout for the Windows runtime campaign (#288): one bounded
/// line per oracle run, written with the NON-panicking writer so a
/// broken/blocked stderr can never turn an evidence row into a false red
/// (`eprintln!` would panic on a failing write; this is test-main-thread
/// code, but the campaign's sink-control children run these same tests).
fn print_cone_witness(tag: &str, witness: &Witness, terminal: EpisodeTerminalOutcome) {
    use std::io::Write;
    let verdict = witness
        .relayed_verdict
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|v| format!("{v:?}"));
    let _ = writeln!(
        std::io::stderr(),
        "[cone-evidence {tag}] reads={} injected={} edge_cuts={} verdict_reached={} \
         verdict_snapshot={} terminal={terminal:?} relayed_verdict={}",
        witness.reads.load(Ordering::Acquire),
        witness.injected.load(Ordering::Acquire),
        witness.edge_cuts.load(Ordering::Acquire),
        witness.verdict_reached.load(Ordering::Acquire),
        witness.verdict_snapshot.load(Ordering::Acquire),
        verdict.unwrap_or_else(|| "<none>".to_owned()),
    );
    let _ = std::io::stderr().flush();
}

/// The session's real edge-backed input, wrapped so the k-th read panics and
/// every `stop()` is witnessed where it actually completes.
struct InjectingInput {
    inner: Arc<dyn RenderPcmInput>,
    witness: Arc<Witness>,
    inject_at: usize,
}

impl RenderPcmInput for InjectingInput {
    fn read_frames(&self, dst: &mut [f32]) -> PcmPull {
        let call = self.witness.reads.fetch_add(1, Ordering::AcqRel) + 1;
        if call == self.inject_at {
            self.witness.injected.store(true, Ordering::Release);
            panic!("O-CONE-NECESSITY: injected render-leg panic");
        }
        self.inner.read_frames(dst)
    }

    fn stop(&self) {
        // Witness A is recorded where the real stop completes, after forwarding
        // it: the edge is cut before the increment, so an observer that sees the
        // increment has seen the cut.
        self.inner.stop();
        self.witness.edge_cuts.fetch_add(1, Ordering::AcqRel);
    }
}

/// The test-owned output: it injects at the input boundary and relays the
/// verdict through witness B, then hands everything to the REAL backend so the
/// production cone runs unmodified.
struct ConeProbeOutput {
    witness: Arc<Witness>,
    inject_at: usize,
}

impl AudioOutput for ConeProbeOutput {
    fn open_stream(&self, request: RenderRequest) -> Result<Box<dyn RenderStream>, OutputError> {
        let RenderRequest {
            format,
            input,
            drain,
            gate,
            level,
            position,
        } = request;
        let injecting: Arc<dyn RenderPcmInput> = Arc::new(InjectingInput {
            inner: input,
            witness: Arc::clone(&self.witness),
            inject_at: self.inject_at,
        });
        // Witness B: an observer on a relayed signal. It runs synchronously
        // inside the cone's `complete()`, snapshots A there, and only then
        // forwards the verdict to the session's own signal — the session mirror
        // sees exactly the same verdict, one synchronous hop later.
        let session_signal = drain.clone();
        let witness = Arc::clone(&self.witness);
        let relayed = DrainSignal::with_on_complete(move |verdict| {
            *witness
                .relayed_verdict
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(verdict.clone());
            witness
                .verdict_snapshot
                .store(witness.edge_cuts.load(Ordering::Acquire), Ordering::Release);
            witness.verdict_reached.store(true, Ordering::Release);
            session_signal.complete(verdict);
        });
        WasapiOutput::new()
            .map_err(|message| OutputError { message })?
            .open_stream(RenderRequest {
                format,
                input: injecting,
                drain: relayed,
                gate,
                level,
                position,
            })
    }
}

// --- the fixture -------------------------------------------------------------

pub(crate) struct ConeFixture {
    runtime: QianqianApp,
    pub(crate) handle: PlaybackSessionHandle,
    pub(crate) witness: Arc<Witness>,
}

impl ConeFixture {
    pub(crate) fn new() -> Self {
        Self::with_inject_at(INJECT_AT)
    }

    /// The A4 sink-control children drive the same fixture with the
    /// injection disabled (`usize::MAX` never fires) so the episode ends by
    /// external stop — the settle class that exposes the #9/#10 pre-swap
    /// misattribution hazards.
    pub(crate) fn with_inject_at(inject_at: usize) -> Self {
        let witness = Arc::new(Witness::default());
        let handle = PlaybackSessionHandle::new();
        let mut runtime = QianqianApp::new();
        runtime
            .register_component(
                ComponentSpec::new("oracle_decode")
                    .provides::<PcmDecodeCapability>()
                    .on_activate(|ctx| {
                        ctx.provide::<PcmDecodeCapability>(Rc::new(InfiniteSource))
                            .map_err(|e| ActivationError::new(format!("{e:?}")))?;
                        Ok(())
                    }),
            )
            .expect("decode provider registers");
        runtime
            .register_component(
                ComponentSpec::new("oracle_output")
                    .provides::<AudioOutputCapability>()
                    .on_activate({
                        let witness = Arc::clone(&witness);
                        move |ctx| {
                            ctx.provide::<AudioOutputCapability>(Rc::new(ConeProbeOutput {
                                witness: Arc::clone(&witness),
                                inject_at,
                            }))
                            .map_err(|e| ActivationError::new(format!("{e:?}")))?;
                            Ok(())
                        }
                    }),
            )
            .expect("output provider registers");
        runtime
            .register_component(playback_session_spec(
                PathBuf::from(EPISODE_PATH),
                handle.clone(),
            ))
            .expect("session registers");
        Self {
            runtime,
            handle,
            witness,
        }
    }

    pub(crate) fn activate(&mut self) {
        self.runtime
            .revise_desired(vec![
                DesiredEntry::enabled("decode", "oracle_decode", Revision::new(1)),
                DesiredEntry::enabled("output", "oracle_output", Revision::new(1)),
                DesiredEntry::enabled("session", "playback_session", Revision::new(1)),
            ])
            .expect("composition is legal");
    }

    /// Precondition witness: the injection actually fired on the render thread.
    /// Not reaching it means the endpoint never delivered PCM to the render leg
    /// (no usable device, or an activation refusal) — a precondition failure,
    /// never a pass and never evidence about the property.
    pub(crate) fn await_injection(&self) -> Result<(), String> {
        let deadline = std::time::Instant::now() + PRECONDITION_BOUND;
        while std::time::Instant::now() < deadline {
            if self.witness.injected.load(Ordering::Acquire) {
                let reads = self.witness.reads.load(Ordering::Acquire);
                assert!(
                    reads >= INJECT_AT,
                    "the injection flag implies {INJECT_AT} reads, saw {reads}"
                );
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Err(format!(
            "PRECONDITION NOT REACHED: the render leg never reached read #{INJECT_AT} \
             (reads so far: {}); no usable render endpoint, or activation refused — \
             this is TOOLING/DEVICE evidence, not a property result",
            self.witness.reads.load(Ordering::Acquire)
        ))
    }

    /// The settlement observation. Runs `wait_terminal` on a helper thread so a
    /// missing settle is bounded instead of hanging the suite; a timeout is the
    /// oracle's red.
    pub(crate) fn settled_within(&self, limit: Duration) -> Option<EpisodeTerminalOutcome> {
        let handle = self.handle.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(handle.wait_terminal());
        });
        rx.recv_timeout(limit).ok()
    }

    /// The teardown relation, observed AFTER the settle assertions: releasing
    /// the episode joins the render leg, so a returned-and-quiet disposal is the
    /// render-thread-join evidence.
    pub(crate) fn dispose_quietly(&mut self) -> bool {
        self.runtime.dispose().snapshot.quiet
    }

    /// Fixture release for a RED run only: an external stop so the run can end.
    /// Never asserted as a success path, and never called between the injection
    /// and the settlement observation.
    pub(crate) fn release_fixture(&mut self) {
        self.handle.request_stop();
        let _ = self.runtime.dispose();
    }
}

/// O-CONE-NECESSITY: the real cone settles the episode by itself.
#[test]
fn o_cone_necessity_the_real_cone_settles_without_an_external_stop() {
    let mut fixture = ConeFixture::new();
    fixture.activate();
    if let Err(message) = fixture.await_injection() {
        // Release the fixture, then fail on the precondition itself.
        fixture.release_fixture();
        panic!("{message}");
    }
    // The frozen regime (module docs). Between here and the settle observation
    // this body calls nothing that could stop, seek, resume or dispose: the only
    // call is the bounded blocking wait below.
    let Some(terminal) = fixture.settled_within(SETTLE_BOUND) else {
        fixture.release_fixture();
        panic!(
            "O-CONE-NECESSITY RED: wait_terminal did not return within {SETTLE_BOUND:?} — \
             the cone's own stop/complete did not reach the episode (or the worker is wedged \
             behind a non-terminalized edge)"
        );
    };
    assert_eq!(
        terminal,
        EpisodeTerminalOutcome::Failed,
        "the render-leg abort must settle Failed{{device}} with no stop intent recorded"
    );
    let quiet = fixture.dispose_quietly();
    assert!(quiet, "releasing the settled episode must dispose quietly");
    print_cone_witness("o-cone-necessity", &fixture.witness, terminal);
}

/// O-CONE-ORDER: the verdict becomes visible only after the edge was cut.
#[test]
fn o_cone_order_the_verdict_becomes_visible_only_after_the_edge_cut() {
    let mut fixture = ConeFixture::new();
    fixture.activate();
    if let Err(message) = fixture.await_injection() {
        fixture.release_fixture();
        panic!("{message}");
    }
    let Some(terminal) = fixture.settled_within(SETTLE_BOUND) else {
        fixture.release_fixture();
        panic!(
            "O-CONE-ORDER INCONCLUSIVE: the episode did not settle within {SETTLE_BOUND:?}; \
             the order witness speaks only about an aborted outcome"
        );
    };
    assert_eq!(
        terminal,
        EpisodeTerminalOutcome::Failed,
        "the order oracle is scoped to the aborted-outcome drive"
    );
    assert!(
        fixture.witness.verdict_reached.load(Ordering::Acquire),
        "the relay observer must have run inside complete()"
    );
    let snapshot = fixture.witness.verdict_snapshot.load(Ordering::Acquire);
    let cuts = fixture.witness.edge_cuts.load(Ordering::Acquire);
    assert!(
        snapshot >= 1,
        "O-CONE-ORDER RED: the verdict became visible with {snapshot} edge cuts recorded \
         (edge cuts now: {cuts}) — the verdict was published before the data plane was cut"
    );
    let quiet = fixture.dispose_quietly();
    assert!(quiet, "releasing the settled episode must dispose quietly");
    print_cone_witness("o-cone-order", &fixture.witness, terminal);
}

/// The relay's own contract, witnessed without any device: the observer runs
/// inside `complete()`, before `complete()` returns, and the downstream signal
/// still receives the verdict. This is what makes the order oracle's snapshot
/// meaningful; it is deliberately not a substitute for the Windows drive above.
#[test]
fn o_cone_order_the_relay_is_synchronous_and_forwards_the_verdict() {
    let witness = Arc::new(Witness::default());
    let forwarded = Arc::new(AtomicBool::new(false));
    let observed_inside = Arc::new(AtomicBool::new(false));
    // The downstream signal stands in for the session's own: the relay must
    // forward to it, and the downstream observer must run before `complete()`
    // returns to the relay's caller.
    let downstream = DrainSignal::with_on_complete({
        let forwarded = Arc::clone(&forwarded);
        move |_| forwarded.store(true, Ordering::Release)
    });
    let relayed = DrainSignal::with_on_complete({
        let witness = Arc::clone(&witness);
        let observed_inside = Arc::clone(&observed_inside);
        move |verdict| {
            witness.edge_cuts.fetch_add(1, Ordering::AcqRel);
            witness
                .verdict_snapshot
                .store(witness.edge_cuts.load(Ordering::Acquire), Ordering::Release);
            witness.verdict_reached.store(true, Ordering::Release);
            downstream.complete(verdict);
            observed_inside.store(true, Ordering::Release);
        }
    });
    relayed.complete(DrainVerdict::aborted());
    assert!(
        observed_inside.load(Ordering::Acquire),
        "the relay observer must run inside complete(), before it returns"
    );
    assert!(
        forwarded.load(Ordering::Acquire),
        "the relay must forward the verdict to the downstream signal"
    );
    assert!(witness.verdict_reached.load(Ordering::Acquire));
    assert_eq!(witness.verdict_snapshot.load(Ordering::Acquire), 1);
}

/// Evidence leg for the #288 Windows runtime campaign: the same cone drive
/// with the host diagnostic facility INSTALLED (stderr exporter), so the
/// class-α records the cone sites publish (#9 open line, #8 abort record)
/// are rendered by the real drainer on Windows, and the bounded shutdown
/// report is observed at the real Windows exit boundary. This is an
/// evidence row, not a property oracle: it re-asserts only the necessity
/// invariants and otherwise prints what the campaign ledger records.
///
/// Process-global note: installing the facility is once-only for the whole
/// test binary. This test must therefore be run in its OWN process when
/// filtered (`cargo test o_cone_evidence -- --exact` semantics), which is
/// how the campaign drives it; if another test happened to install first,
/// the installation is refused and this row degrades to the no-facility
/// shape — asserted here so the degradation is loud, never silent.
#[test]
fn o_cone_evidence_facility_leg_witnesses_records_and_shutdown() {
    use std::io::Write;
    let diagnostics = qianqian_diagnostics::Diagnostics::install(
        Box::new(qianqian_diagnostics::StderrExporter::default()),
        qianqian_diagnostics::Config::default(),
    )
    .expect("evidence leg owns the facility installation of its process");
    assert!(qianqian_diagnostics::Diagnostics::is_installed());

    let mut fixture = ConeFixture::new();
    fixture.activate();
    if let Err(message) = fixture.await_injection() {
        fixture.release_fixture();
        panic!("{message}");
    }
    let Some(terminal) = fixture.settled_within(SETTLE_BOUND) else {
        fixture.release_fixture();
        panic!("EVIDENCE LEG: the episode did not settle within {SETTLE_BOUND:?}");
    };
    assert_eq!(terminal, EpisodeTerminalOutcome::Failed);
    let observation = fixture.handle.observe();
    let quiet = fixture.dispose_quietly();
    assert!(quiet, "releasing the settled episode must dispose quietly");

    // The α records (#9 open, #8 abort) may still be in flight on the
    // drainer's 50 ms poll cadence: the bounded shutdown's final sweep is
    // the deterministic delivery boundary for this evidence row.
    let report = diagnostics.shutdown(Duration::from_secs(2));
    let counters = report.counters;
    let _ = writeln!(
        std::io::stderr(),
        "[cone-evidence facility-leg] terminal={terminal:?} \
         failure_diagnostic={:?} stop_requested={} relayed_verdict={:?} \
         shutdown(drained={} abandoned={} undelivered={}) \
         counters(copied_out={} exported={} export_failed={} alpha_rejected={} \
         beta_queued={} critical_panics={})",
        observation.failure_diagnostic,
        observation.stop_requested,
        fixture
            .witness
            .relayed_verdict
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|v| format!("{v:?}")),
        report.drained,
        report.abandoned,
        report.undelivered_at_exit,
        counters.copied_out,
        counters.exported,
        counters.export_failed,
        counters.alpha_rejected,
        counters.beta_queued,
        counters.critical_panics,
    );
    let _ = std::io::stderr().flush();
    print_cone_witness("facility-leg", &fixture.witness, terminal);
    // The facility leg's own honesty bounds: the sweep must be a complete
    // drain (every α record the cone sites published was delivered or
    // counted), and the drain identity must hold.
    assert!(report.drained, "the evidence leg's shutdown must drain");
    assert_eq!(report.undelivered_at_exit, 0);
    assert_eq!(
        counters.copied_out + counters.beta_queued,
        counters.exported + counters.export_failed + report.undelivered_at_exit,
        "drain boundary identity (copied_out + beta_queued = exported + export_failed)"
    );
}
