//! WASAPI mechanism: shared-mode, event-driven render on the default
//! endpoint. All COM objects, the event handle and the render thread are
//! owned by the acquired stream (one playback episode); COM is initialized
//! and uninitialized on the render thread itself
//! (first-audible-slice design §5; mechanism evidence: historical
//! `wasapi_renderer.cpp`, recovered as mechanism only).
//!
//! Steady-state loop: pause gate (D14.7, park before any device buffer
//! is held) -> device event -> GetCurrentPadding -> GetBuffer -> pull
//! already-available PCM from the pre-bound frame source straight into
//! the device buffer -> ReleaseBuffer. The render thread never touches
//! the filesystem, a decoder, or the kernel; its only stop observation
//! is the frame source's terminal outcomes.
//!
//! Frame accounting and position evidence (D14.8): the same loop owns one
//! plain local `handed_off` total — source frames successfully submitted
//! into the device buffer — and publishes `handed_off -
//! min(padding, handed_off)` monotonically into the session-owned
//! position cell from the padding readings it already takes (steady loop,
//! park slices, drain path). The publication is ordered strictly BEFORE
//! the submission of the current iteration, so the estimate can never
//! count a block the device has not been given yet; the accounting is
//! credited only AFTER `ReleaseBuffer` succeeds. This adds no device
//! call, no lock and no allocation to the render path.
//!
//! Format negotiation is Tier 1 only (design §7): the float32 source
//! format is submitted directly; shared-mode WASAPI mixes it to the
//! device mix format itself. A device that refuses the source format
//! fails the open honestly (Tier-2 SRC fallback is OPEN-1, not faked).
//!
//! Safety: all Win32/COM calls sit in explicit `unsafe` blocks at their
//! call sites; the private functions themselves are safe. Three RAII owners
//! guarantee release on every exit path — panic included:
//! [`ComApartment`] balances `CoInitializeEx` on the render thread,
//! [`EventHandle`] closes the buffer event exactly once, and
//! [`DeviceSession`]'s [`Drop`] runs the historical release order (Stop,
//! render client, audio client, event handle) exactly once on that same
//! thread.

use std::slice;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::{
    AUDCLNT_E_DEVICE_INVALIDATED, AUDCLNT_E_UNSUPPORTED_FORMAT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, IAudioClient, IAudioRenderClient, IAudioStreamVolume,
    IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEXTENSIBLE, eMultimedia, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::GUID;

use qianqian_audio_api::ports::{
    AudioOutput, DrainSignal, DrainVerdict, GateSlice, OutputError, OutputLevel, ParkOutcome,
    PcmFormat, PcmPull, PositionEvidence, RenderGate, RenderPcmInput, RenderRequest, RenderStream,
    SeekParkRelease, TailProbeOutcome,
};

use crate::open_abort::abort_render_thread;

/// Frozen WASAPI ABI values defined locally, exactly so this mechanism
/// never depends on which constants a given Windows SDK happens to export
/// (same posture as the historical renderer).
const WAVE_FORMAT_EXTENSIBLE_TAG: u16 = 0xFFFE;
const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: GUID = GUID::from_values(
    0x0000_0003,
    0x0000,
    0x0010,
    [0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71],
);
const EXTENSIBLE_CB_SIZE: u16 = 22;
const IEEE_FLOAT_BITS: u16 = 32;
const IEEE_FLOAT_BYTES: u32 = 4;

const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const EVENT_TIMEOUT_MS: u32 = 100;
const DRAIN_CAP: Duration = Duration::from_secs(5);

/// Mechanism diagnostics (the per-episode open line and abort/volume
/// notes — never steady-state output) are silent by default: any stderr
/// write corrupts a full-screen terminal UI sharing the console, and a
/// mechanism abort already surfaces through the session's typed
/// activation/terminal evidence. QIANQIAN_AUDIO_LOG=1 restores them for
/// mechanism debugging.
fn mechanism_log_enabled() -> bool {
    std::env::var_os("QIANQIAN_AUDIO_LOG").is_some_and(|v| v != "0")
}

/// The concrete Windows Host Render Backend mechanism (ADR-PBK-003
/// §2/§3): the Output Plugin is the stable composition identity; this
/// type is the backend mechanism it owns — never itself a Plugin.
/// Long-lived and stateless across opens. Crate-private
/// (plugin-boundary hardening H1): the mechanism is not product API;
/// composition roots admit the Output Plugin through the crate-root
/// `output_plugin()` and consumers see only the backend-neutral
/// `AudioOutput` service trait. Built only by the crate-root
/// `selected_backend()` factory.
pub(crate) struct WasapiOutput;

impl WasapiOutput {
    pub(crate) fn new() -> Result<Self, String> {
        Ok(Self)
    }
}

impl AudioOutput for WasapiOutput {
    fn open_stream(&self, request: RenderRequest) -> Result<Box<dyn RenderStream>, OutputError> {
        if request.format.sample_rate == 0 || request.format.channels == 0 {
            return Err(OutputError {
                message: format!(
                    "unrenderable source format: {}",
                    debug_format(&request.format)
                ),
            });
        }
        let slot: OpenSlot = Arc::new((Mutex::new(None), Condvar::new()));
        let handle = std::thread::Builder::new()
            .name("qianqian-wasapi-render".into())
            .spawn({
                let slot = slot.clone();
                let format = request.format;
                let render_input = request.input.clone();
                let gate = request.gate.clone();
                let drain = request.drain.clone();
                let position = request.position.clone();
                let level = request.level.clone();
                move || run_render_thread(format, render_input, gate, drain, position, level, slot)
            })
            .map_err(|e| OutputError {
                message: format!("render thread spawn failed: {e}"),
            })?;

        let (mutex, cv) = &*slot;
        let guard = mutex.lock().expect("open verdict lock");
        let (mut guard, wait) = cv
            .wait_timeout_while(guard, OPEN_TIMEOUT, |v| v.is_none())
            .expect("open verdict wait poisoned");

        // Every path that can join the render thread runs only after the
        // verdict mutex is released: the render thread takes this same
        // mutex to publish its verdict, so joining it while holding the
        // lock deadlocks as soon as an open outlives the timeout and
        // publishes late (a slow-but-eventually-successful device open).
        let verdict = { guard.take() };
        let timed_out = wait.timed_out();
        drop(guard);
        match (verdict, timed_out) {
            (Some(OpenVerdict::Opened { format }), _) => Ok(Box::new(WasapiStream {
                render_input: request.input,
                thread: Some(handle),
                negotiated: format,
            })),
            (Some(OpenVerdict::Failed { message }), _) => {
                abort_render_thread(handle, &request.input, &request.gate);
                Err(OutputError { message })
            }
            (None, true) => {
                abort_render_thread(handle, &request.input, &request.gate);
                Err(OutputError {
                    message: "WASAPI device open did not reach a verdict in time".to_owned(),
                })
            }
            (None, false) => {
                unreachable!("wait_timeout_while returned without a verdict or timeout")
            }
        }
    }
}

/// One acquired render stream: owns the render thread and the device
/// session for one playback episode.
struct WasapiStream {
    render_input: Arc<dyn RenderPcmInput>,
    thread: Option<JoinHandle<()>>,
    negotiated: PcmFormat,
}

impl RenderStream for WasapiStream {
    fn negotiated_format(&self) -> PcmFormat {
        self.negotiated
    }

    /// stop -> join -> release, in one owner-local inverse on the
    /// mechanism side. The data-plane stop wakes a render thread blocked
    /// reading an empty edge; the thread releases the device before
    /// exiting, so a completed join means the device is released.
    ///
    /// The caller owns the gate-release precondition (see the trait
    /// contract): this mechanism manufactures no pause/seek release
    /// intent of its own.
    fn stop_and_join(mut self: Box<Self>) {
        self.render_input.stop();
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

// --- open verdict handshake -------------------------------------------------

enum OpenVerdict {
    Opened { format: PcmFormat },
    Failed { message: String },
}

type OpenSlot = Arc<(Mutex<Option<OpenVerdict>>, Condvar)>;

fn debug_format(f: &PcmFormat) -> String {
    format!(
        "{} Hz, {} channels, mask {:#x}",
        f.sample_rate, f.channels, f.channel_mask
    )
}

// --- render thread -----------------------------------------------------------

/// How the whole render leg terminated (drain verdict input).
enum LoopOutcome {
    /// EOF was pulled from the edge and the device drained to zero padding.
    Drained,
    /// Stop, edge terminal, or a device error prevented further progress.
    Aborted { message: String },
}

fn run_render_thread(
    format: PcmFormat,
    render_input: Arc<dyn RenderPcmInput>,
    gate: RenderGate,
    drain: DrainSignal,
    position: PositionEvidence,
    level: OutputLevel,
    slot: OpenSlot,
) {
    // A panic must not leave the completion unresolved or the producer
    // wedged: it reports like any other abort.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        open_and_run(format, &*render_input, &gate, &position, &level, &slot)
    }))
    .unwrap_or_else(|_| LoopOutcome::Aborted {
        message: "render thread panicked".to_owned(),
    });
    // One terminal diagnostic per episode — never steady-state output.
    match &outcome {
        LoopOutcome::Aborted { message } => {
            if mechanism_log_enabled() {
                eprintln!("[qianqian-wasapi] render aborted: {message}");
            }
            // The device leg is gone: stop the data plane so the decode
            // worker cannot wedge on a full edge against a dead consumer
            // (first-wins on the edge, so it is a no-op after natural EOF).
            render_input.stop();
        }
        LoopOutcome::Drained => {}
    }
    drain.complete(match outcome {
        LoopOutcome::Drained => DrainVerdict::Drained,
        LoopOutcome::Aborted { .. } => DrainVerdict::Aborted,
    });
}

/// COM apartment ownership lives and dies on this thread, around the
/// whole open + loop + release sequence. Both `S_OK` and `S_FALSE` are
/// successful `CoInitializeEx` results; both require a matching
/// `CoUninitialize` (Microsoft COM contract). The `ComApartment` guard
/// guarantees that `CoUninitialize` runs on every exit path — success,
/// error, panic — because it is dropped during unwind as well.
fn open_and_run(
    format: PcmFormat,
    render_input: &dyn RenderPcmInput,
    gate: &RenderGate,
    position: &PositionEvidence,
    level: &OutputLevel,
    slot: &OpenSlot,
) -> LoopOutcome {
    let coinit = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    // Fail closed at the boundary: without a successful CoInitializeEx
    // (S_OK or S_FALSE) this thread has no COM apartment and the open
    // must not proceed into COM calls. (NATIVE-BOUNDARY-AUDIT-0 A3.2（round record：Git 历史 / PR #134）.)
    if coinit.is_err() {
        return LoopOutcome::Aborted {
            message: format!("CoInitializeEx failed: {coinit:?}"),
        };
    }
    let _apartment = ComApartment(true);
    open_and_run_inner(format, render_input, gate, position, level, slot)
}

fn open_and_run_inner(
    format: PcmFormat,
    render_input: &dyn RenderPcmInput,
    gate: &RenderGate,
    position: &PositionEvidence,
    level: &OutputLevel,
    slot: &OpenSlot,
) -> LoopOutcome {
    let Some(session) = open_session(format, level, slot) else {
        return LoopOutcome::Aborted {
            message: "device open failed (see open verdict)".to_owned(),
        };
    };
    // The open verdict is published; from here every exit is reported
    // through the loop outcome only. The session is released by Drop on
    // every path — success, error, panic — before the unwind reaches
    // this scope's boundary.
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        steady_loop(&session, &format, render_input, gate, position)
    })) {
        Ok(outcome) => outcome,
        Err(_) => LoopOutcome::Aborted {
            message: "render loop panicked".to_owned(),
        },
    }
}

/// COM apartment ownership on the render thread. `CoInitializeEx` returns
/// `Ok` for both `S_OK` and `S_FALSE`: both are successful initialization
/// results and both require a matching `CoUninitialize` (Microsoft COM
/// contract). This guard runs that `CoUninitialize` exactly once on every
/// exit path — success, error, panic — because it is dropped during
/// unwind as well.
struct ComApartment(bool);

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}
/// RAII owner of one kernel event HANDLE. `HANDLE` itself is a raw
/// wrapper without `Drop`, so a plain `HANDLE` field releases nothing —
/// the guard's `Drop` runs `CloseHandle` exactly once on every exit path:
/// open failure, panic, stop, and normal release
/// (NATIVE-BOUNDARY-AUDIT-0 A3.3 corrective（round record：Git 历史 / PR #134）).
struct EventHandle(HANDLE);

impl EventHandle {
    fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for EventHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Field order is teardown order (Rust drops fields in declaration
/// order): the dependent COM/service interfaces — the render client
/// and the volume service — are declared BEFORE the lower-level audio
/// client and event handle they may still reference, so no internal
/// reference outlives the device resources it sits on (Stage-B audit
/// B-01). The historical release order (Stop → render → client →
/// event) is preserved; the volume service joins the client side of
/// it.
struct DeviceSession {
    render: IAudioRenderClient,
    /// The episode's desired stream factor (D14.9 cell) and the
    /// mechanism handle that realizes it, plus the last value this leg
    /// APPLIED or ATTEMPTED (the loop-top compare; on a recoverable
    /// apply failure the failing routed value is recorded as attempted
    /// so it is not re-issued every iteration).
    level: OutputLevel,
    stream_volume: IAudioStreamVolume,
    client: IAudioClient,
    event: EventHandle,
    buffer_frames: u32,
    channels: u32,
    applied_bits: std::cell::Cell<u32>,
    /// Per-episode latch for the recoverable-failure diagnostic (D14.9
    /// grading: ONE bounded diagnostic per episode, not per iteration).
    volume_diagnosed: std::cell::Cell<bool>,
}

impl Drop for DeviceSession {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
        // Fields drop in declaration order: render client, volume
        // service, audio client, event handle — the historical release
        // order (Stop → render → client → event), with the dependent
        // COM/service interfaces ahead of the client and the event so
        // no internal reference outlives them (B-01). The COM pointers
        // release through their own smart-pointer Drop; the event
        // closes through EventHandle's Drop.
    }
}

/// Device open + Tier-1 negotiation. Publishes exactly one open verdict;
/// returns the session on success.
/// Device open + Tier-1 negotiation. Publishes exactly one open verdict;
/// returns the session on success.
fn open_session(format: PcmFormat, level: &OutputLevel, slot: &OpenSlot) -> Option<DeviceSession> {
    let publish = |v: OpenVerdict| {
        let (m, cv) = &**slot;
        let mut guard = m.lock().expect("open verdict lock");
        *guard = Some(v);
        cv.notify_all();
    };
    let fail = |message: String| -> Option<DeviceSession> {
        publish(OpenVerdict::Failed { message });
        None
    };

    let enumerator: IMMDeviceEnumerator =
        match unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) } {
            Ok(e) => e,
            Err(e) => return fail(format!("CoCreateInstance(MMDeviceEnumerator): {e}")),
        };
    let device = match unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) } {
        Ok(d) => d,
        Err(e) => return fail(format!("no default render endpoint: {e}")),
    };
    let client: IAudioClient = match unsafe { device.Activate(CLSCTX_ALL, None) } {
        Ok(c) => c,
        Err(e) => return fail(format!("device activation failed: {e}")),
    };

    // Tier 1: float32 EXTENSIBLE at the source rate/channels/mask.
    // Win32 requires the extension struct zero-initialized before the
    // fixed fields are filled in.
    let mut wfx: WAVEFORMATEXTENSIBLE = unsafe { std::mem::zeroed() };
    wfx.Format.wFormatTag = WAVE_FORMAT_EXTENSIBLE_TAG;
    wfx.Format.nChannels = format.channels;
    wfx.Format.nSamplesPerSec = format.sample_rate;
    wfx.Format.wBitsPerSample = IEEE_FLOAT_BITS;
    wfx.Format.nBlockAlign = format.channels * (IEEE_FLOAT_BYTES as u16);
    wfx.Format.nAvgBytesPerSec = format.sample_rate * u32::from(format.channels) * IEEE_FLOAT_BYTES;
    wfx.Format.cbSize = EXTENSIBLE_CB_SIZE;
    wfx.Samples.wValidBitsPerSample = IEEE_FLOAT_BITS;
    wfx.dwChannelMask = u32::try_from(format.channel_mask).unwrap_or(0);
    wfx.SubFormat = KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;

    if let Err(e) = unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            0,
            0,
            std::ptr::addr_of!(wfx.Format),
            None,
        )
    } {
        let hint = if e.code() == AUDCLNT_E_UNSUPPORTED_FORMAT {
            " (device refused the float32 source format; Tier-2 SRC fallback is deferred)"
        } else {
            ""
        };
        return fail(format!("stream initialize failed: {e}{hint}"));
    }

    let event = match unsafe { CreateEventW(None, false, false, None) } {
        Ok(h) => EventHandle(h),
        Err(e) => return fail(format!("buffer event creation failed: {e}")),
    };
    if let Err(e) = unsafe { client.SetEventHandle(event.raw()) } {
        return fail(format!("SetEventHandle failed: {e}"));
    }
    let buffer_frames = match unsafe { client.GetBufferSize() } {
        Ok(f) if f > 0 => f,
        Ok(_) => return fail("device reported a zero-frame buffer".to_owned()),
        Err(e) => return fail(format!("GetBufferSize failed: {e}")),
    };
    let render: IAudioRenderClient = match unsafe { client.GetService() } {
        Ok(r) => r,
        Err(e) => return fail(format!("render client acquire failed: {e}")),
    };
    // D14.9: the episode's desired stream factor applies ONCE at stream
    // open, before first meaningful submission — the initial routed
    // value (or unity) is in effect before the stream ever starts. A
    // failure here is an open failure (the episode fails cleanly, like
    // any other device-open refusal).
    let stream_volume: IAudioStreamVolume = match unsafe { client.GetService() } {
        Ok(v) => v,
        Err(e) => return fail(format!("stream volume acquire failed: {e}")),
    };
    let channels = match unsafe { stream_volume.GetChannelCount() } {
        Ok(c) => c,
        Err(e) => return fail(format!("stream volume channels failed: {e}")),
    };
    let initial = level.load();
    let initial_levels = vec![initial; channels as usize];
    if let Err(e) = unsafe { stream_volume.SetAllVolumes(&initial_levels) } {
        return fail(format!("initial stream volume apply failed: {e}"));
    }
    drop(initial_levels);

    publish(OpenVerdict::Opened { format });
    // One open diagnostic per episode — the real-sound gate's negotiated
    // format evidence; never steady-state output; silent unless
    // mechanism logging is enabled (a product TUI shares this console).
    if mechanism_log_enabled() {
        eprintln!(
            "[qianqian-wasapi] opened: {} Hz, {} channels, mask {:#x}, buffer {} frames (shared, event-driven)",
            format.sample_rate, format.channels, format.channel_mask, buffer_frames
        );
    }
    Some(DeviceSession {
        render,
        client,
        event,
        buffer_frames,
        level: level.clone(),
        stream_volume,
        channels,
        applied_bits: std::cell::Cell::new(initial.to_bits()),
        volume_diagnosed: std::cell::Cell::new(false),
    })
}

/// The steady event-driven loop, then the EOF drain.
fn steady_loop(
    session: &DeviceSession,
    format: &PcmFormat,
    render_input: &dyn RenderPcmInput,
    gate: &RenderGate,
    position: &PositionEvidence,
) -> LoopOutcome {
    if let Err(e) = unsafe { session.client.Start() } {
        return LoopOutcome::Aborted {
            message: format!("stream start failed: {e}"),
        };
    }
    let channels = usize::from(format.channels);
    // F4 (D14.8) writer-local accounting: the source frames THIS stretch
    // has successfully submitted into the device buffer — since the
    // episode start, or since the last committed seek cutover (F5/D14.5
    // resets it on the leg's own path). A plain local on this execution
    // path — never a cell, never a product surface. It is the
    // projection's base and the reason one render leg is the only writer
    // of the episode's position cell.
    let mut handed_off: u64 = 0;
    // The current published stretch's basis in source frames (0 at the
    // episode start; the decoder's actual landing after a committed
    // cutover), and whether publishing is live at all (false forever
    // after an unknown-landing cutover).
    let mut basis: u64 = 0;
    let mut publishing: bool = true;
    loop {
        // THE loop-top gate (D14.7 mechanism A + the D14.5 cut park,
        // unified into one operation): strictly before device-buffer
        // acquisition, no device buffer held across a park, and — the
        // frozen D14.5 realtime row — ONE intent-lock acquisition for a
        // steady iteration of normal playback (O(1) flag tests, no
        // second acquisition, no dispatch, no allocation).
        //
        // The tail slice's reading feeds F4 (D14.8) as before: while
        // parked, submission is frozen, so publishing the consumed
        // estimate from each slice is exactly how the sample rises to
        // the frozen handed-off total as the device drains — one device
        // observation, two explicit semantic uses, truth classes kept
        // separate. The seek-release slice applies a committed cutover's
        // rebase HERE, on this path, before anything further can be
        // submitted — including while the leg STAYS PARKED by pause
        // (pause intent survives the seek; the rebase is bookkeeping,
        // never a submission).
        //
        // A FAILED tail observation is a device failure, never "not
        // quiesced yet" (F5 implementation corrective-4): the gate
        // releases the leg without publishing quiescence and reports
        // the failure back, and THIS loop's existing failure exit — the
        // same one a steady-path GetCurrentPadding error takes — stops
        // the data plane on the way out, which is what makes the edge
        // terminal and the seek worker's data-plane escape fire.
        let park_failed = matches!(
            gate.park_loop_top(|gated| match gated {
                GateSlice::TailProbe => {
                    let Ok(padding) = (unsafe { session.client.GetCurrentPadding() }) else {
                        return TailProbeOutcome::Failed;
                    };
                    publish_consumed(position, basis, handed_off, u64::from(padding), publishing);
                    if padding == 0 {
                        TailProbeOutcome::Quiesced
                    } else {
                        TailProbeOutcome::Pending
                    }
                }
                GateSlice::SeekRelease(release) => match release {
                    SeekParkRelease::Committed { landing } => {
                        handed_off = 0;
                        if landing.is_none() {
                            publishing = false;
                        }
                        if let Some(landing) = landing.filter(|_| publishing) {
                            basis = landing;
                        }
                        // A withdrawal is for the REST of the episode (D14.5
                        // position rebase): once an unknown landing turned
                        // publishing off, a later KNOWN landing neither
                        // resurrects publication nor un-withdraws the cell —
                        // `rebase(None)` is an idempotent re-withdrawal.
                        position.rebase(if publishing { landing } else { None });
                        TailProbeOutcome::Pending
                    }
                    SeekParkRelease::Aborted => TailProbeOutcome::Pending,
                },
            }),
            ParkOutcome::TailProbeFailed
        );
        if park_failed {
            break abort_msg("tail observation failed while parked".to_owned());
        }
        // D14.9 loop-top output-level apply (V-PROBE-grounded): ONE
        // relaxed load + compare per iteration; SetAllVolumes runs here
        // — before the device wait and GetBuffer, never inside the
        // quantum. Failure grading (D14.9): device loss routes through
        // THIS loop's existing device-failure exit (D11 may settle
        // Failed); any other failure is an ordinary recoverable
        // mechanism failure — one diagnostic, playback continues at the
        // last applied level.
        {
            let routed = session.level.load();
            let routed_bits = routed.to_bits();
            if routed_bits != session.applied_bits.get() {
                let routed_levels = vec![routed; session.channels as usize];
                match unsafe { session.stream_volume.SetAllVolumes(&routed_levels) } {
                    Ok(()) => session.applied_bits.set(routed_bits),
                    Err(e) if e.code() == AUDCLNT_E_DEVICE_INVALIDATED => {
                        break abort_msg(format!(
                            "stream volume apply failed (device invalidated): {e}"
                        ));
                    }
                    // Recoverable failure (D14.9 grading): ONE diagnostic
                    // per episode, then the failing routed value is
                    // recorded as attempted — playback holds the last
                    // APPLIED level and no failing COM call is re-issued
                    // per iteration; the next ROUTED change retries once.
                    Err(e) => {
                        session.applied_bits.set(routed_bits);
                        if !session.volume_diagnosed.replace(true) && mechanism_log_enabled() {
                            eprintln!(
                                "[qianqian-wasapi] stream volume apply failed (recoverable; \
                                 holding the last applied level): {e}"
                            );
                        }
                    }
                }
            }
        }
        // Period cadence; the bounded wait is also the stop-latency bound.
        unsafe { WaitForSingleObject(session.event.raw(), EVENT_TIMEOUT_MS) };
        let padding = match unsafe { session.client.GetCurrentPadding() } {
            Ok(p) => p,
            Err(e) => break abort_msg(format!("GetCurrentPadding failed: {e}")),
        };
        // F4 (D14.8) publication order — the pairing is the contract:
        // this estimate is derived from the handed-off total as it stands
        // BEFORE this iteration submits anything, because it is paired
        // with a padding reading taken at that same instant. Publishing
        // after `handed_off += n` below (or crediting `n` before
        // `ReleaseBuffer` succeeds) would count the new block as already
        // consumed and overstate the position by up to one device block.
        // The source-order oracle `render_order_oracle.rs` pins this.
        publish_consumed(position, basis, handed_off, u64::from(padding), publishing);
        let available = session.buffer_frames.saturating_sub(padding) as usize;
        if available == 0 {
            continue;
        }

        // The device buffer IS the fill destination (zero-copy period).
        let ptr = match unsafe { session.render.GetBuffer(available as u32) } {
            Ok(p) => p,
            Err(e) => break abort_msg(format!("GetBuffer failed: {e}")),
        };
        let dst = unsafe { slice::from_raw_parts_mut(ptr as *mut f32, available * channels) };
        match render_input.read_frames(dst) {
            PcmPull::Frames(n) => {
                if let Err(e) = unsafe { session.render.ReleaseBuffer(n as u32, 0) } {
                    break abort_msg(format!("ReleaseBuffer failed: {e}"));
                }
                // Only a successful device submission earns handed-off
                // accounting (D14.8): a failed ReleaseBuffer must not
                // count the block. `n <= available`, so the total is
                // bounded by what the device buffer could have taken.
                handed_off += n as u64;
            }
            PcmPull::Eof => {
                // Edge drained: everything produced has been submitted.
                let _ = unsafe { session.render.ReleaseBuffer(0, 0) };
                break drain_to_zero(session, position, handed_off, basis, publishing);
            }
            PcmPull::Stopped => {
                let _ = unsafe { session.render.ReleaseBuffer(0, 0) };
                break LoopOutcome::Aborted {
                    message: "data plane stopped".to_owned(),
                };
            }
        }
    }
}

/// Wait until the device has played out everything submitted
/// (padding reaches zero), bounded. EOF must be audible, not just queued.
///
/// The same readings keep the F4 position evidence current: the leg
/// submits nothing more here, so each observation publishes the rising
/// consumed estimate, and the zero observation that ends the drain
/// publishes the exact handed-off total. That total is consumption
/// truth — it is not compared with, corrected by, or forced onto the
/// reported source duration.
fn drain_to_zero(
    session: &DeviceSession,
    position: &PositionEvidence,
    handed_off: u64,
    basis: u64,
    publishing: bool,
) -> LoopOutcome {
    let deadline = Instant::now() + DRAIN_CAP;
    loop {
        match unsafe { session.client.GetCurrentPadding() } {
            Ok(padding) => {
                publish_consumed(position, basis, handed_off, u64::from(padding), publishing);
                if padding == 0 {
                    return LoopOutcome::Drained;
                }
            }
            Err(e) => return abort_msg(format!("drain padding check failed: {e}")),
        }
        if Instant::now() > deadline {
            return abort_msg("drain deadline passed before the device played out".to_owned());
        }
        unsafe { WaitForSingleObject(session.event.raw(), EVENT_TIMEOUT_MS) };
    }
}

/// F4 (D14.8) publication with the F5 stretch basis: the consumed
/// estimate is `basis + handed_off − min(tail, handed_off)`, where
/// `handed_off` counts only the CURRENT stretch (since the last
/// committed cutover — or the episode start). `basis` is rebased by the
/// leg itself at a committed cutover; after an unknown-landing cutover
/// `publishing` is false forever and nothing is published (unknown
/// stays unknown). One relaxed monotone RMW; the caller already holds
/// the tail reading.
fn publish_consumed(
    position: &PositionEvidence,
    basis: u64,
    handed_off: u64,
    tail: u64,
    publishing: bool,
) {
    if publishing {
        position.publish_consumed(basis + handed_off, tail);
    }
}

fn abort_msg(message: String) -> LoopOutcome {
    LoopOutcome::Aborted { message }
}
