//! The Playback Session component (first-audible-slice design §6).
//!
//! Activation ordering (each step immediately followed by its inverse
//! registration so a raise unwinds exactly what the step acquired):
//!
//! ```text
//! resolve Decode capability once
//! resolve Output capability once
//! open decode endpoint            (RAII: rides with the decode worker)
//! compile the applied processing  (D14.11: binds the episode's source
//!                                  format; acquires nothing, no inverse)
//! build bounded edge
//! open render stream              -> inverse: stop_and_join stream
//! spawn decode worker             -> inverse: stop edge + join worker
//! ```
//!
//! Effects unwind strictly LIFO, so disposal runs stop-edge, join worker,
//! then stop-join-release the stream — preserving stop -> join -> release.
//! There is no settlement watcher/resolver thread (D14.3): every terminal
//! evidence publication evaluates settlement synchronously on the publishing leg's own
//! call stack (the worker wrapper for decode/worker evidence; the
//! session-installed one-shot DrainSignal observer for the drain verdict),
//! so when both join inverses return, no decisive evidence can sit
//! uncommitted.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use qianqian_audio_api::ports::{
    AudioOutputCapability, DecodeOutcome, DecodedPcmStream, PcmDecodeCapability, PcmFormat,
};
use qianqian_composition::{ActivationError, ComponentSpec, Discharge};

use crate::completion::{CutoverDecision, SessionCompletion};
use crate::edge::PcmEdge;
use crate::establishment::{EstablishmentAttempt, EstablishmentResult};
use crate::handle::PlaybackSessionHandle;
use crate::live::LiveProcessing;
use crate::processing::{AudioProcessingConfig, EpisodeProcessing};

/// Current edge-capacity tuning: 8192 PCM frames (~185 ms of media at
/// 44.1 kHz). Chosen from the measured decode tail (p99 ~0.2 ms per
/// 1024-frame block, decode-cost-model.md §5) plus scheduling margin.
/// This bounds edge occupancy, not elapsed end-to-end latency or native
/// progress. `pub(crate)` so the in-crate processing
/// oracles can pin geometry against the same bound (a blocked write at a
/// full edge is what forces a mid-block cut with a non-empty remainder).
pub(crate) const EDGE_CAPACITY_FRAMES: usize = 8192;

/// Frames per decode staging refill. Matches the block size the decode
/// baselines were measured at. `pub(crate)` so the live-probe oracles
/// pin the apply boundary against the same constant (campaign #190 D3).
pub(crate) const STAGING_FRAMES: usize = 1024;

/// The Playback Session component definition. The App captures the file
/// and the episode handle it will observe; desired entries need no
/// config payload for the first slice. Audio Processing runs in the
/// transparent BYPASS configuration (D14.11: an episode established
/// without a processing request processes nothing).
pub fn playback_session_spec(file: PathBuf, handle: PlaybackSessionHandle) -> ComponentSpec {
    playback_session_spec_with_processing(file, handle, AudioProcessingConfig::BYPASS)
}

/// [`playback_session_spec`] with the application's desired Audio
/// Processing configuration (ADR-PBK-002 D14.11). This constructor call
/// IS the D14.11 handoff representation: the desired configuration is
/// bound to the episode here, the session compiles ONE coherent applied
/// snapshot from it at activation, and since campaign #190 D4 a changed
/// desired configuration can also be applied LIVE through the handle's
/// typed `set_*` commands under the §7.3 live-update admission
/// contract. The establishment binding itself is unchanged — it seeds
/// the product-control desired state the episode starts from. An
/// invalid configuration fails the activation cleanly with no terminal
/// Fact and no resource left behind; because the EQ stage's coefficients
/// depend on the source rate, the final compile runs once the decode
/// endpoint is open, and an invalid configuration unwinds that endpoint
/// through RAII.
pub fn playback_session_spec_with_processing(
    file: PathBuf,
    handle: PlaybackSessionHandle,
    processing: AudioProcessingConfig,
) -> ComponentSpec {
    playback_session_spec_with_establishment(file, handle, processing).0
}

/// Fresh-assembly variant carrying the D14.6 operation result independently of
/// observations. For the supported Decode/Output/Session wiring, reaching the
/// successful continuation proves provider activation and required bindings;
/// it then owns all Session acquisition steps and their inverse registration.
/// The caller consumes the paired attempt after synchronous assembly returns.
pub fn playback_session_spec_with_establishment(
    file: PathBuf,
    handle: PlaybackSessionHandle,
    processing: AudioProcessingConfig,
) -> (ComponentSpec, EstablishmentAttempt) {
    handle.completion.processing().establish(processing);
    let attempt = EstablishmentAttempt::new();
    let result_slot = attempt.0.clone();
    let spec = ComponentSpec::new("playback_session")
        .requires::<PcmDecodeCapability>()
        .requires::<AudioOutputCapability>()
        .on_activate(move |ctx| {
            let result = activate(&file, &handle.completion, ctx);
            // This is the producing operation's return path, not a reconstruction
            // from its diagnostic/source/terminal publications. No fallible step
            // remains after successful activation and inverse registration.
            record_establishment(&result_slot, &result);
            result
        });
    (spec, attempt)
}

/// Test-only establishment with a DELIBERATE episode processing runtime
/// (Issue #177 I2; ADR-PBK-002 D14.11): the white-box oracles drive the
/// REAL composition — real capability resolve, edge, render stream,
/// worker, seek/pause protocol — with the StatefulProbe or the
/// failure-route injection in place of a config-compiled snapshot, so
/// the stateful transport semantics are proven against the actual
/// mechanism. Never shipped (`cfg(all(test, not(loom)))`), never public
/// (the session module is private), never a product seam: the
/// application reaches processing only through the configuration
/// constructors above. Single-mount by construction: the deliberate
/// runtime is a one-shot value handed out on the first activation, and
/// a second activation of the same spec is a test bug (loud tripwire).
#[cfg(all(test, not(loom)))]
pub(crate) fn playback_session_spec_with_test_processor(
    file: PathBuf,
    handle: PlaybackSessionHandle,
    processing: EpisodeProcessing,
) -> ComponentSpec {
    // `on_activate` is Fn (the kernel may re-run activation across
    // mount cycles), but the deliberate runtime is a one-shot value:
    // hand it out exactly once, loudly.
    let processing = std::cell::RefCell::new(Some(processing));
    ComponentSpec::new("playback_session")
        .requires::<PcmDecodeCapability>()
        .requires::<AudioOutputCapability>()
        .on_activate(move |ctx| {
            let processing = processing
                .borrow_mut()
                .take()
                .expect("the test processor was activated twice");
            // The same activation-failure publication discipline as the
            // product constructors: a raising establishment leaves the
            // diagnostic on the episode seam, never a forged terminal.
            let result = activate_established(&file, &handle.completion, |_| Ok(processing), ctx);
            if let Err(e) = &result {
                handle.completion.activation_failed(&e.message);
            }
            result
        })
}

/// Test-only establishment with the live mechanism's INSTRUMENTATION TAP
/// (campaign #190 D4): the real composition — real capability resolve,
/// edge, render stream, worker, seek/pause/terminal protocol — with the
/// production [`LiveProcessing`] engine plus the cfg(test) tap, so the
/// live oracles exercise the actual staging seam, remainder flush,
/// pickup boundary and invalidation path, and can read the engine's
/// block/frame/event bookkeeping. Never shipped, never public, never a
/// product seam. The transition length is a parameter here (the
/// production constant is derived from the format) so an oracle can pin
/// geometry deterministically.
#[cfg(all(test, not(loom)))]
pub(crate) fn playback_session_spec_with_live_tap(
    file: PathBuf,
    handle: PlaybackSessionHandle,
    initial: AudioProcessingConfig,
    transition_frames: usize,
    tap: crate::live::tap::LiveTap,
) -> ComponentSpec {
    // The establishment binding happens at CONSTRUCTION time — the
    // product constructor's exact shape — so an oracle can issue a
    // typed command into the establishment→activation window and pin
    // the same linearization the product path runs.
    handle.completion.processing().establish(initial);
    let tap = std::cell::RefCell::new(Some(tap));
    ComponentSpec::new("playback_session")
        .requires::<PcmDecodeCapability>()
        .requires::<AudioOutputCapability>()
        .on_activate(move |ctx| {
            let tap = tap
                .borrow_mut()
                .take()
                .expect("the live tap spec was activated twice");
            // The SAME live-engine activation path the product
            // constructor uses (one bind spelling), with the tap-carrying
            // engine constructor — so a `set_*` between this constructor
            // and activation is honored identically: it folds into the
            // initial applied configuration.
            let result = activate_live_engine(
                &file,
                &handle.completion,
                ctx,
                |desired, format, control| {
                    crate::live::LiveProcessing::with_tap(
                        desired,
                        format,
                        transition_frames,
                        control,
                        tap,
                    )
                },
            );
            if let Err(e) = &result {
                handle.completion.activation_failed(&e.message);
            }
            result
        })
}

/// The engine-builder variant of the live establishment (campaign #190
/// D3/D4 failure-injection route): the test constructs the engine itself
/// — e.g. with a deliberate failing processor as the initial side —
/// through the same activation path. One-shot, like the other test-only
/// constructors.
#[cfg(all(test, not(loom)))]
pub(crate) fn playback_session_spec_with_live_engine(
    file: PathBuf,
    handle: PlaybackSessionHandle,
    build_engine: impl FnOnce(&PcmFormat) -> Result<crate::live::LiveProcessing, String>
    + Send
    + 'static,
) -> ComponentSpec {
    let build_engine = std::cell::RefCell::new(Some(build_engine));
    ComponentSpec::new("playback_session")
        .requires::<PcmDecodeCapability>()
        .requires::<AudioOutputCapability>()
        .on_activate(move |ctx| {
            let build_engine = build_engine
                .borrow_mut()
                .take()
                .expect("the live engine spec was activated twice");
            let result = activate_established(&file, &handle.completion, build_engine, ctx);
            if let Err(e) = &result {
                handle.completion.activation_failed(&e.message);
            }
            result
        })
}

fn record_establishment(
    slot: &std::cell::RefCell<EstablishmentResult>,
    result: &Result<(), ActivationError>,
) {
    *slot.borrow_mut() = match result {
        Ok(()) => EstablishmentResult::Established,
        Err(error) => EstablishmentResult::NotEstablished {
            diagnostic: Some(error.message.clone()),
        },
    };
}

#[cfg(all(test, not(loom)))]
#[path = "establishment_tests.rs"]
mod establishment_tests;

fn activate(
    file: &Path,
    completion: &SessionCompletion,
    ctx: &mut qianqian_composition::ActivationCtx<'_>,
) -> Result<(), ActivationError> {
    // The kernel's diagnostic surface carries the FAILED verdict but not
    // the domain message; the session publishes its own activation
    // failure so the App can show why an episode never started.
    let result = activate_inner(file, completion, ctx);
    if let Err(e) = &result {
        completion.activation_failed(&e.message);
    }
    result
}

fn activate_inner(
    file: &Path,
    completion: &SessionCompletion,
    ctx: &mut qianqian_composition::ActivationCtx<'_>,
) -> Result<(), ActivationError> {
    activate_live_engine(file, completion, ctx, |desired, format, control| {
        LiveProcessing::new(desired, format, control)
    })
}

/// The ONE live-engine activation path — the product constructor and the
/// tap-carrying oracle spec both construct through it, so the
/// linearization has exactly one spelling. It reads product-control
/// through the ACTIVATION BIND: the bind consumes any pending update in
/// the same lock hold it reads the desired state, so a `set_*` between
/// spec construction and activation folds into the initial applied
/// configuration (no phantom initial→same transition), and a `set_*`
/// after the bind lands in the pending slot and reaches the worker's
/// fresh-block pickup as an ordinary §7.3 live update.
fn activate_live_engine(
    file: &Path,
    completion: &SessionCompletion,
    ctx: &mut qianqian_composition::ActivationCtx<'_>,
    engine: impl FnOnce(
        AudioProcessingConfig,
        &PcmFormat,
        std::sync::Arc<crate::live::ProcessingControl>,
    ) -> Result<LiveProcessing, String>,
) -> Result<(), ActivationError> {
    let control = completion.processing();
    activate_established(
        file,
        completion,
        |format| engine(control.bind_for_activation(), format, control.clone()),
        ctx,
    )
}

/// The activation continuation parameterized by the one step that needs
/// the SOURCE FORMAT: the applied processing snapshot's final compile.
/// The EQ stage's coefficients are a function of the source sample
/// rate (I3), so the compile runs after the decode endpoint is open —
/// D14.11 binds format-dependent processing state to the episode's
/// format — and a failure unwinds the open endpoint through the
/// ordinary RAII. The closure runs exactly once, on this path.
/// Crate-internal so the white-box oracles can drive the REAL
/// composition with a deliberate test-only processor (the I2
/// StatefulProbe and the failure-route injection) through the same
/// establishment path; product code reaches it only through the config
/// constructors above.
fn activate_established<P: ProcessingRuntime + 'static>(
    file: &Path,
    completion: &SessionCompletion,
    compile_processing: impl FnOnce(&PcmFormat) -> Result<P, String>,
    ctx: &mut qianqian_composition::ActivationCtx<'_>,
) -> Result<(), ActivationError> {
    activate_established_with_spawn(file, completion, compile_processing, ctx, spawn_worker)
}

// Ordinary private operation seam: production uses Builder::spawn, while the
// deterministic oracle can return its error without platform resource exhaustion.
fn activate_established_with_spawn<P: ProcessingRuntime + 'static>(
    file: &Path,
    completion: &SessionCompletion,
    compile_processing: impl FnOnce(&PcmFormat) -> Result<P, String>,
    ctx: &mut qianqian_composition::ActivationCtx<'_>,
    spawn: impl FnOnce(
        Box<dyn DecodedPcmStream>,
        Arc<PcmEdge>,
        SessionCompletion,
        P,
    ) -> Result<std::thread::JoinHandle<()>, ActivationError>,
) -> Result<(), ActivationError> {
    // Control plane: capability resolution happens exactly once, here.
    let decode = ctx.resolve::<PcmDecodeCapability>().map_err(|e| {
        ActivationError::new(format!(
            "decode capability unresolved: {}",
            resolve_error(e)
        ))
    })?;
    let output = ctx.resolve::<AudioOutputCapability>().map_err(|e| {
        ActivationError::new(format!(
            "output capability unresolved: {}",
            resolve_error(e)
        ))
    })?;

    // One playback-specific decode endpoint. Its RAII rides with the
    // worker closure: released when the worker exits (joined before any
    // stream release), dropped on spawn failure, dropped on earlier raises.
    let decode_stream = decode
        .service()
        .open_media(file)
        .map_err(|e| ActivationError::new(format!("decode open failed: {}", e.message)))?;
    let format = decode_stream.format();
    completion.set_source_format(format);
    // The applied processing snapshot completes HERE, bound to this
    // episode's source format (D14.11): intrinsic configuration
    // problems fail the establishment cleanly — the open endpoint drops
    // with the raise, no terminal Fact is forged, no resource is left
    // behind. Source-rate band availability is NOT a failure mode: the
    // rate-aware active-band profile (dsp-product-model.md §2.1)
    // compiles whatever participates.
    let processing = compile_processing(&format).map_err(|e| {
        ActivationError::new(format!("audio processing configuration invalid: {e}"))
    })?;
    // Duration evidence (D14.8), relayed once at activation from the
    // same decode probe that produced the format. A provider that
    // reported none leaves the evidence unset — unknown stays unknown,
    // never zero and never an estimate.
    if let Some(duration) = decode_stream.source_duration() {
        completion.set_source_duration(duration);
    }

    // The one bounded PCM edge: session-owned, preallocated now. It is
    // also the stop target: application-facing stop intent arrives here
    // (through the session completion) and ends the episode with the
    // same first-wins terminal the legs already understand.
    let edge = Arc::new(PcmEdge::new(format.channels, EDGE_CAPACITY_FRAMES));
    completion.bind_stop_target(edge.clone());

    // Playback-specific render stream, pre-bound to the edge's consumer
    // half and the session's drain signal. Open/negotiation errors surface
    // in activation; a local verdict timeout does not bound abort/join cleanup.
    // The session-owned one-shot drain observer synchronously evaluates
    // D11 before the first `complete` returns, committing only if evidence
    // is decisive. This stream's stop_and_join inverse cannot return before
    // that publication/evaluation path has run; a drain verdict alone need
    // not commit terminal truth. The render gate routes the episode's
    // pause intent to the same leg's loop-top check (D14.7), and the
    // position cell is where that leg publishes its consumed estimate
    // from the tail readings it already takes (D14.8) — one episode-owned
    // cell, never replaced live, so P1–P5 are not triggered.
    let stream = output
        .service()
        .open_stream(qianqian_audio_api::ports::RenderRequest {
            format,
            input: edge.clone(),
            drain: completion.drain_signal(),
            gate: completion.render_gate(),
            position: completion.position_evidence(),
            level: completion.output_level(),
        })
        .map_err(|e| ActivationError::new(format!("render stream open failed: {}", e.message)))?;

    // Registered before the worker spawn, so it unwinds after the worker
    // inverse: stop+join the producer before the device is released. It
    // is a relation-bearing effect: the stream is a cross-fiber
    // contribution toward the output provider. The pause gate is
    // released first (D14.7 teardown obligation): a leg parked at the
    // gate is not inside read_frames, so the data-plane stop alone
    // cannot wake it and the join below would never return. The release
    // linearizes pause routing under the completion lock, so no pause
    // can re-park the leg between this release and the join; its exit
    // publishes disengagement evidence and lets the leg reach the
    // stopped edge on its own.
    let teardown_completion = completion.clone();
    ctx.register_relation::<AudioOutputCapability>(&output, move || {
        teardown_completion.release_pause_gate();
        stream.stop_and_join();
        Discharge::Discharged
    });

    let worker_edge = edge.clone();
    let worker_completion = completion.clone();
    let worker = spawn(decode_stream, worker_edge, worker_completion, processing)?;
    // Registered last, so it unwinds first: stop the edge (unblocking
    // both legs), then join the producer. Relation-bearing toward the
    // decode provider (the data edge it feeds).
    ctx.register_relation::<PcmDecodeCapability>(&decode, move || {
        edge.stop();
        let _ = worker.join();
        Discharge::Discharged
    });

    Ok(())
}

fn spawn_worker<P: ProcessingRuntime + 'static>(
    decode_stream: Box<dyn DecodedPcmStream>,
    edge: Arc<PcmEdge>,
    completion: SessionCompletion,
    processing: P,
) -> Result<std::thread::JoinHandle<()>, ActivationError> {
    std::thread::Builder::new()
        .name("qianqian-decode".into())
        .spawn(move || decode_worker(decode_stream, edge, completion, STAGING_FRAMES, processing))
        .map_err(|e| ActivationError::new(format!("decode worker spawn failed: {e}")))
}

/// Bounded wait slice for the decode worker's seek-protocol waits (the
/// commit decision poll and the interruptible write's back-off). Off
/// the device submission path: this caps each requested wait, not
/// serialization/commit latency or a whole-operation deadline. Scheduling,
/// lock acquisition, native calls and missing evidence can delay progress.
const WORKER_WAIT_SLICE: Duration = Duration::from_millis(2);

/// The processing runtime the decode worker drives at the frozen
/// staging placement. Static dispatch only: the worker is generic over
/// this trait and monomorphized at spawn, so the per-block work is a
/// direct call — no per-block lookup, no dynamic dispatch, no
/// capability/context resolution (the D14.11 realtime firewall). Two
/// implementations exist: [`LiveProcessing`] (the production live
/// mechanism, campaign #190 D4) and [`EpisodeProcessing`] (the
/// stateful-transport oracle's deliberate processor through the
/// cfg(test) injection constructors).
pub(crate) trait ProcessingRuntime: Send {
    /// The staging transform (D14.11): in place, frame-count
    /// preserving; `Err` is the unrecoverable processing failure
    /// routing through D11 `Failed`.
    fn stage(&mut self, block: &mut [f32]) -> Result<(), String>;

    /// The D14.11 Applied-seek obligation: invalidate ALL pre-cut
    /// signal-derived processing history before any post-cut PCM is
    /// processed.
    fn invalidate_signal_history(&mut self);

    /// The live-update pickup, called by the worker exactly at the
    /// FRESH-STAGING-BLOCK boundary (after a preserved remainder is
    /// flushed, before the next decode, with no seek in flight). `None`
    /// = nothing to do; `Some(Ok(()))` = handled (an accepted transition
    /// started, or a desired update was refused — a refusal is recorded
    /// as product-control's own mechanism evidence, never a processing
    /// failure); `Some(Err(_))` = an unrecoverable processing failure
    /// routing through D11 `Failed`. The deliberate test processors keep
    /// the default: nothing to pick up.
    fn poll_update(&mut self) -> Option<Result<(), String>> {
        None
    }
}

impl ProcessingRuntime for EpisodeProcessing {
    fn stage(&mut self, block: &mut [f32]) -> Result<(), String> {
        EpisodeProcessing::stage(self, block)
    }

    fn invalidate_signal_history(&mut self) {
        EpisodeProcessing::invalidate_signal_history(self)
    }
    // poll_update: the deliberate test processors have no live updates.
}

/// How one bounded write advanced, from the decode worker's
/// interruptible write loop.
enum WriteStep {
    /// The whole slice was accepted.
    Whole,
    /// A terminal stopped the write; the episode is ending.
    Stopped,
    /// A seek became actionable (command observed AND leg-parked
    /// evidence) at the written prefix: the unwritten tail is
    /// preserved for the provider outcome to own.
    CutPoint,
}

/// The decode worker: SongCore/FFmpeg/filesystem work lives only here.
/// Refills a once-allocated staging buffer and feeds the bounded edge,
/// and — since F5 (ADR-PBK-002 D14.5) — runs the session-owned seek
/// protocol on its own execution path:
///
/// ```text
/// loop-top pickup (one slot try per staging block)
///     ↓ seek pends; production CONTINUES (that is what keeps the
///       leg's park reachable — an early hold could strand the leg
///       inside a blocked read on an emptied edge)
/// bounded-slice write re-observes the command slot each slice
///     ↓ actionable (command ∧ leg parked) → stop at the written
///       prefix, PRESERVE the unwritten tail
/// serialization point (leg parked, holds no device buffer):
///     provider seek BEFORE anything is invalidated
///         RefusedUnchanged → no invalidation; release the leg; finish
///             the preserved remainder EXACTLY (zero content loss;
///             the refused output equals the no-seek control)
///         MutatedThenFailed → never resume old-cursor production;
///             ordinary decode-failure evidence → D11 Failed
///         Applied → discard the staging (incl. the preserved tail),
///             invalidate the episode processing history (D14.11),
///             edge.invalidate() — THE one purge, on this path —
///             publish the actual landing, hold production, and let
///             the session's commit decision (tail quiesced ∧ parked ∧
///             unsettled, sampled atomically per poll) route the rebase
///             release; a poll without that evidence is PENDING — the
///             protocol keeps waiting, and only an episode ending
///             (recorded stop intent / settlement / teardown, or the
///             data plane's own terminal) ends it without a rebase.
///             Then resume post-cut production.
/// ```
fn decode_worker<P: ProcessingRuntime>(
    mut decode_stream: Box<dyn DecodedPcmStream>,
    edge: Arc<PcmEdge>,
    completion: SessionCompletion,
    staging_frames: usize,
    mut processing: P,
) {
    let channels = usize::from(decode_stream.format().channels);
    let catch = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Startup allocation only; the steady loop reuses this buffer.
        let mut staging = vec![0.0f32; staging_frames * channels];
        // The seek command picked up at the loop top, awaiting the leg's
        // parked evidence. Production keeps flowing while it pends.
        let mut pending_seek: Option<Duration> = None;
        // The preserved unwritten tail of an in-flight staging block,
        // observed mid-block when a seek became actionable. Owned
        // storage: one bounded allocation per seek at most, decode-side.
        let mut remainder: Option<Vec<f32>> = None;
        loop {
            // --- loop-top seek pickup (the one slot try per block) ---
            if pending_seek.is_none() && remainder.is_none() {
                pending_seek = completion.take_seek_command();
            }

            // --- serialization point ---
            // A preserved remainder does NOT defer the seek: it is THIS
            // seek's in-flight staging, retained (prompt §10 / D14.5
            // corrective 1) until the provider outcome is known — the
            // refusal finishes it, the cut discards it.
            if let Some(target) = pending_seek {
                if completion.seek_aborted() {
                    // Stop / teardown / settlement won the race: the seek
                    // is inert command history. No provider call, no
                    // invalidation, no state.
                    completion.release_seek_without_commit();
                    pending_seek = None;
                } else if completion.leg_parked_evidence() {
                    // The parked evidence is CURRENT world state, not
                    // per-cycle state: a continuously-held park stays
                    // true across seek-cycle boundaries while the leg
                    // physically remains parked (the D14.7 park
                    // invariant — no device buffer held, tail cannot
                    // refill), and a just-resumed leg may still be
                    // latched for the microseconds before its
                    // Disengaged publication lands. Safety here does not
                    // rest on latch freshness but on worker program
                    // order: the commit boundary re-reads the current
                    // park conjunction under the completion lock, and
                    // the load-bearing stale-PCM wall is THIS
                    // serialization point's discipline — purge →
                    // landing → production hold (D14.5 Applied).
                    // Defense in depth: the data plane was Open at
                    // acceptance; re-validate here, where the provider is
                    // about to be called. An edge that went terminal in
                    // between (EOF drain window, a stop racing the
                    // pickup) is not seekable.
                    if edge.terminal() != crate::edge::EdgeTerminal::Open {
                        completion.release_seek_without_commit();
                        pending_seek = None;
                    } else {
                        match decode_stream.seek(target) {
                            qianqian_audio_api::ports::ProviderSeekOutcome::RefusedUnchanged => {
                                // Proven pre-mutation: the old world is
                                // intact. Release the leg immediately (it
                                // resumes reading the un-purged edge);
                                // the preserved remainder is finished
                                // below, and only then does the slot
                                // free (a second seek must not park the
                                // leg mid-remainder).
                                completion.seek_refused();
                                completion.release_seek_park();
                                pending_seek = None;
                            }
                            qianqian_audio_api::ports::ProviderSeekOutcome::MutatedThenFailed {
                                diagnostic,
                            } => {
                                // Unprovable means destructive: the old
                                // decoder continuation is not guaranteed.
                                // NEVER resume old-cursor production —
                                // the episode takes the ordinary
                                // decode-failure route (D11).
                                completion.release_seek_without_commit();
                                completion.decode_failed(&format!("seek failed: {diagnostic}"));
                                edge.fail();
                                return;
                            }
                            qianqian_audio_api::ports::ProviderSeekOutcome::Applied { landing } => {
                                // Success: the D14.11 Applied obligations
                                // run in the frozen order — the staging
                                // discard (including any preserved
                                // remainder, which belongs to the pre-cut
                                // world), the invalidation of ALL pre-cut
                                // signal-derived processing history, then
                                // the ONE purge on this path — then
                                // landing evidence, then the production
                                // hold until the session's commit decision
                                // routes the release. Post-cut production
                                // therefore processes through FRESH
                                // episode-local processing state under the
                                // SAME applied configuration.
                                remainder = None;
                                processing.invalidate_signal_history();
                                edge.invalidate();
                                completion.seek_landing_published(landing);
                                // The cut is irrevocable from here: the old
                                // staging is discarded and the edge purged.
                                // The wait and the decision are therefore
                                // ONE sampled step per iteration — the
                                // session samples the commit boundary and
                                // the episode-ending latches in a single
                                // lock hold, so a transient gap in the
                                // park/quiescence evidence is Pending and
                                // the protocol keeps waiting; it can never
                                // fall back to pre-cut accounting (no
                                // rebase) while the episode is still live.
                                // The one episode-ending class the session
                                // state cannot see is the data plane's own
                                // terminal (the frozen failure policy's
                                // "data plane not Open": stop, a device
                                // abort that stopped the plane, teardown),
                                // so it is tested here on this path.
                                loop {
                                    match completion.seek_cutover_decision(landing) {
                                        CutoverDecision::Committed | CutoverDecision::Aborted => {
                                            break;
                                        }
                                        CutoverDecision::Pending => {
                                            if edge.terminal() != crate::edge::EdgeTerminal::Open {
                                                // The data plane ended under
                                                // the cut. No rebase is owed
                                                // to an episode this owner is
                                                // already ending, and the
                                                // seek publishes no evidence
                                                // of its own: settle through
                                                // the existing D11 path.
                                                completion.release_seek_without_commit();
                                                return;
                                            }
                                            std::thread::sleep(WORKER_WAIT_SLICE);
                                        }
                                    }
                                }
                                // The commit routed the rebase release;
                                // the one-seek slot stays occupied until
                                // the LEG has consumed the payload (a
                                // later seek's hold would otherwise wipe
                                // an unconsumed `Committed` and lose the
                                // rebase). Bounded polls off the RT
                                // path; stop/teardown and a data plane
                                // that ends under the cut win immediately
                                // (an already-routed payload stays routed
                                // for the exiting leg).
                                while !completion.seek_aborted()
                                    && completion.seek_release_pending()
                                {
                                    if edge.terminal() != crate::edge::EdgeTerminal::Open {
                                        // The data plane ended under the cut:
                                        // the routed payload stays routed for
                                        // the exiting leg, and the slot frees.
                                        // Freeing it cannot lose a rebase that
                                        // mattered — the only terminal a cut can
                                        // meet here is the stop the render abort
                                        // or the teardown itself issued, after
                                        // which the worker writes nothing more —
                                        // but that guarantee is bounded by that
                                        // fact, NOT by acceptance (whose atomic
                                        // hold re-validates the session latches,
                                        // not the edge), so the claim is stated
                                        // no stronger than it is: a racing
                                        // acceptance against a just-stopped plane
                                        // could still plant and wipe a payload
                                        // no leg will read.
                                        completion.clear_seek_in_flight();
                                        return;
                                    }
                                    std::thread::sleep(WORKER_WAIT_SLICE);
                                }
                                completion.clear_seek_in_flight();
                                pending_seek = None;
                                // Post-cut production resumes from the
                                // provider's cursor below.
                            }
                        }
                    }
                }
                // else: the command pends at the loop top while
                // production keeps flowing (that is what lets the leg
                // reach its park promptly).
            }

            // Finish a preserved remainder BEFORE any new decode: a
            // refused seek owes the stream the rest of its own content,
            // frame-for-frame. The slot stays occupied until this
            // completes, and the leg has been released, so the edge
            // drains normally.
            if let Some(data) = remainder.take() {
                let mut off = 0usize;
                while off < data.len() {
                    let wrote = edge.write_some(&data[off..]);
                    off += wrote;
                    if wrote == 0 {
                        if edge.terminal() != crate::edge::EdgeTerminal::Open {
                            break;
                        }
                        edge.wait_for_space(WORKER_WAIT_SLICE);
                    }
                }
                if off < data.len() {
                    // A terminal ended the episode mid-remainder (stop /
                    // failure): the old world is being torn down anyway.
                    return;
                }
                completion.clear_seek_in_flight();
            }

            // --- live-update pickup (fresh-staging-block boundary) ---
            // Reached only with the remainder flushed and no seek past
            // the serialization point (an actionable/accepted cut or a
            // resolved refusal; a merely observed, not-yet-actionable
            // command does not block the pickup — the same D14.5
            // production-continues principle), so an accepted transition
            // starts on exactly one whole staging block that has not yet
            // been DSP-processed — the apply boundary the live authority
            // freezes (dsp-product-model.md §7.3, campaign #190 D3/D4).
            // The production live runtime reads product-control's
            // pending cell here; the deliberate test processors keep the
            // no-update default. Some(Err) is a processing failure and
            // takes the ordinary D11 route; a refusal is product
            // control's own recorded diagnostic, never a failure.
            if let Some(Err(message)) = processing.poll_update() {
                completion.processing_failed(&message);
                edge.fail();
                return;
            }

            // Decode one staging block.
            match decode_stream.read_frames(&mut staging) {
                // A zero-frame response must still observe the data plane:
                // a decoder that never progresses cannot pin the worker
                // past a stop.
                Ok(DecodeOutcome::Frames(0)) => {
                    if edge.terminal() != crate::edge::EdgeTerminal::Open {
                        return;
                    }
                }
                Ok(DecodeOutcome::Frames(n)) => {
                    let total = n * channels;
                    // Production Audio Processing (ADR-PBK-002 D14.11;
                    // Issue #177 I1): the episode's applied processing
                    // snapshot runs at the frozen decode-worker staging
                    // placement — the whole staging block is processed
                    // BEFORE any of it can reach the edge, so a partially
                    // accepted block leaves already-PROCESSED PCM in the
                    // preserved remainder (the D14.5 seek obligations
                    // extend to it). One in-place stage per block: no
                    // per-block K0 work or dispatch. Steady processing
                    // adds no allocation; live-transition scratch may
                    // grow within this episode's staging-block bound.
                    if let Err(message) = processing.stage(&mut staging[..total]) {
                        // D14.11 failure semantics: the unrecoverable
                        // processing failure settles through the existing
                        // D11 `Failed` class via the processing publication
                        // route, whose stage keeps the internal diagnosis
                        // truthful about the origin (never a decode label,
                        // never a new public terminal variant). No bypass,
                        // no partial result.
                        completion.processing_failed(&message);
                        edge.fail();
                        return;
                    }
                    match write_observing_seek(
                        &edge,
                        &completion,
                        &staging[..total],
                        &mut pending_seek,
                        &mut remainder,
                    ) {
                        WriteStep::Whole => {}
                        WriteStep::Stopped => return,
                        WriteStep::CutPoint => {
                            // The unwritten tail is preserved in
                            // `remainder`; the next loop top runs the
                            // serialization point.
                        }
                    }
                }
                Ok(DecodeOutcome::Eof) => {
                    edge.close_eof();
                    return;
                }
                Err(e) => {
                    completion.decode_failed(&e.message);
                    edge.fail();
                    return;
                }
            }
        }
    }));
    match catch {
        Ok(()) => {}
        Err(payload) => {
            let message = if let Some(s) = payload.downcast_ref::<&str>() {
                format!("decode worker panicked: {s}")
            } else if let Some(s) = payload.downcast_ref::<String>() {
                format!("decode worker panicked: {s}")
            } else {
                "decode worker panicked".to_owned()
            };
            completion.decode_failed(&message);
            edge.fail();
        }
    }
    // The single exit funnel (normal and panic paths alike): publish the
    // terminal evidence AND mark the worker gone — the acceptance side
    // of the seek/worker-exit linearization — then abort any stranded
    // seek. The order is load-bearing (implementation corrective-1): a
    // request_seek accepted before the `worker_gone` publication is
    // found and released by the cleanup; one attempted after it is
    // rejected by acceptance. Without the cleanup, a request accepted
    // against an about-to-exit worker (the request × EOF interleaving)
    // could route a hold nobody ever releases and wedge the episode's
    // final drain.
    completion.worker_exited(edge.terminal());
    completion.abort_stranded_seek();
}

/// The interruptible bounded-slice write (D14.5): write `src` into the
/// edge while re-observing the seek command slot between capacity waits.
/// Full occupancy alone cannot suppress those observation points; the wait
/// slice is not a scheduling/native-call or seek-completion deadline.
/// No destructive pre-purge. When a seek becomes
/// actionable (command observed, leg parked, no abort) the write stops
/// at its written prefix and the unwritten tail is PRESERVED: the
/// provider outcome owns it (a refusal finishes it exactly — zero
/// content loss; an applied cut discards it with the staging).
///
/// Lock discipline: every acquisition here (slot peek, completion
/// evidence reads, edge lock) is taken alone, never nested and never
/// held across a wait — no thread holds the edge mutex while waiting on
/// the render park or the session state (F5 lock-order audit). Across
/// the whole seek surface the one nesting that exists is uniform and
/// one-directional — the command routers take gate-intent while holding
/// the completion-state lock, never the reverse — so no cycle is
/// reachable.
fn write_observing_seek(
    edge: &PcmEdge,
    completion: &SessionCompletion,
    src: &[f32],
    pending_seek: &mut Option<Duration>,
    remainder: &mut Option<Vec<f32>>,
) -> WriteStep {
    let mut off = 0usize;
    loop {
        if off == src.len() {
            return WriteStep::Whole;
        }
        // Seek observation point: a command counts as observed whether
        // it still sits in the slot or was already picked up at the
        // loop top.
        let observed = pending_seek.is_some() || completion.seek_command_observed();
        if observed && !completion.seek_aborted() && completion.leg_parked_evidence() {
            // The command may still sit in the slot (this write observed
            // it before the next loop-top pickup): promote it into
            // `pending_seek` so the serialization point at the next loop
            // top runs THIS seek. The preserved remainder is that seek's
            // own in-flight staging, not a deferral — skipping the
            // serialization point here would leave the leg parked while
            // the worker blocks finishing the remainder into the full
            // edge it can no longer drain (a wedge the seek matrices
            // caught).
            if pending_seek.is_none() {
                *pending_seek = completion.take_seek_command();
            }
            *remainder = Some(src[off..].to_vec());
            return WriteStep::CutPoint;
        }
        let wrote = edge.write_some(&src[off..]);
        off += wrote;
        if wrote == 0 {
            if edge.terminal() != crate::edge::EdgeTerminal::Open {
                return WriteStep::Stopped;
            }
            edge.wait_for_space(WORKER_WAIT_SLICE);
        }
    }
}

/// Map a kernel resolution error to a human-readable diagnostic without
/// coupling session.rs to `ResolveError`'s Debug surface.
fn resolve_error(e: qianqian_composition::ResolveError) -> &'static str {
    match e {
        qianqian_composition::ResolveError::Undeclared => "capability not declared",
        qianqian_composition::ResolveError::InactiveAccess => "activation context inactive",
        qianqian_composition::ResolveError::Unresolved => "no active provider",
        qianqian_composition::ResolveError::Ambiguous => "multiple providers",
        qianqian_composition::ResolveError::AlreadyProvided => "already provided",
    }
}
