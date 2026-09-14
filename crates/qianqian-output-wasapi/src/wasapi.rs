//! WASAPI mechanism: shared-mode, event-driven render on the default
//! endpoint. All COM objects, the event handle and the render thread are
//! owned by the acquired stream (one playback episode); COM is initialized
//! and uninitialized on the render thread itself
//! (first-audible-slice design §5; mechanism evidence: historical
//! `wasapi_renderer.cpp`, recovered as mechanism only).
//!
//! Steady-state loop: device event -> GetCurrentPadding -> GetBuffer ->
//! pull already-available PCM from the pre-bound frame source straight
//! into the device buffer -> ReleaseBuffer. The render thread never
//! touches the filesystem, a decoder, or the kernel; its only stop
//! observation is the frame source's terminal outcomes.
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
    AUDCLNT_E_UNSUPPORTED_FORMAT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    IAudioClient, IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator,
    WAVEFORMATEXTENSIBLE, eMultimedia, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::GUID;

use qianqian_audio_api::ports::{
    AudioOutput, DrainSignal, DrainVerdict, OutputError, PcmFormat, PcmPull, RenderPcmInput,
    RenderRequest, RenderStream,
};

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

/// The real output mechanism. Long-lived and stateless across opens.
pub struct WasapiOutput;

impl WasapiOutput {
    pub fn new() -> Result<Self, String> {
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
                let drain = request.drain.clone();
                move || run_render_thread(format, render_input, drain, slot)
            })
            .map_err(|e| OutputError {
                message: format!("render thread spawn failed: {e}"),
            })?;

        let (mutex, cv) = &*slot;
        let guard = mutex.lock().expect("open verdict lock");
        let (mut guard, wait) = cv
            .wait_timeout_while(guard, OPEN_TIMEOUT, |v| v.is_none())
            .expect("open verdict wait poisoned");

        let verdict = { guard.take() };
        match verdict {
            Some(OpenVerdict::Opened { format }) => Ok(Box::new(WasapiStream {
                render_input: request.input,
                thread: Some(handle),
                negotiated: format,
            })),
            Some(OpenVerdict::Failed { message }) => {
                abort_thread(handle, &request.input);
                Err(OutputError { message })
            }
            None if wait.timed_out() => {
                abort_thread(handle, &request.input);
                Err(OutputError {
                    message: "WASAPI device open did not reach a verdict in time".to_owned(),
                })
            }
            None => unreachable!("wait_timeout_while returned without a verdict or timeout"),
        }
    }
}

/// Wake and join a render thread whose stream was never handed to the
/// session (open failure / timeout). The thread observes the data-plane
/// stop at its first read, or exits through its own failed verdict.
fn abort_thread(handle: JoinHandle<()>, render_input: &Arc<dyn RenderPcmInput>) {
    render_input.stop();
    let _ = handle.join();
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
    drain: DrainSignal,
    slot: OpenSlot,
) {
    // A panic must not leave the completion unresolved or the producer
    // wedged: it reports like any other abort.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        open_and_run(format, &*render_input, &slot)
    }))
    .unwrap_or_else(|_| LoopOutcome::Aborted {
        message: "render thread panicked".to_owned(),
    });
    // One terminal diagnostic per episode — never steady-state output.
    match &outcome {
        LoopOutcome::Aborted { message } => {
            eprintln!("[qianqian-wasapi] render aborted: {message}");
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
    slot: &OpenSlot,
) -> LoopOutcome {
    let coinit = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let _apartment = ComApartment(coinit.is_ok());
    open_and_run_inner(format, render_input, slot)
}

fn open_and_run_inner(
    format: PcmFormat,
    render_input: &dyn RenderPcmInput,
    slot: &OpenSlot,
) -> LoopOutcome {
    let Some(session) = open_session(format, slot) else {
        return LoopOutcome::Aborted {
            message: "device open failed (see open verdict)".to_owned(),
        };
    };
    // The open verdict is published; from here every exit is reported
    // through the loop outcome only. The session is released by Drop on
    // every path — success, error, panic — before the unwind reaches
    // this scope's boundary.
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        steady_loop(&session, &format, render_input)
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
/// (NATIVE-BOUNDARY-AUDIT-0 A3.3 corrective).
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

struct DeviceSession {
    render: IAudioRenderClient,
    client: IAudioClient,
    event: EventHandle,
    buffer_frames: u32,
}

impl Drop for DeviceSession {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
        // Fields drop in declaration order: render client, audio client,
        // event handle — the required historical release order. The COM
        // pointers release through their own smart-pointer Drop; the
        // event closes through EventHandle's Drop.
    }
}

/// Device open + Tier-1 negotiation. Publishes exactly one open verdict;
/// returns the session on success.
/// Device open + Tier-1 negotiation. Publishes exactly one open verdict;
/// returns the session on success.
fn open_session(format: PcmFormat, slot: &OpenSlot) -> Option<DeviceSession> {
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
    let mut wfx: WAVEFORMATEXTENSIBLE = std::mem::zeroed();
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

    publish(OpenVerdict::Opened { format });
    // One open diagnostic per episode — the real-sound gate's negotiated
    // format evidence; never steady-state output.
    eprintln!(
        "[qianqian-wasapi] opened: {} Hz, {} channels, mask {:#x}, buffer {} frames (shared, event-driven)",
        format.sample_rate, format.channels, format.channel_mask, buffer_frames
    );
    Some(DeviceSession {
        render,
        client,
        event,
        buffer_frames,
    })
}

/// The steady event-driven loop, then the EOF drain.
fn steady_loop(
    session: &DeviceSession,
    format: &PcmFormat,
    render_input: &dyn RenderPcmInput,
) -> LoopOutcome {
    if let Err(e) = unsafe { session.client.Start() } {
        return LoopOutcome::Aborted {
            message: format!("stream start failed: {e}"),
        };
    }
    let channels = usize::from(format.channels);
    loop {
        // Period cadence; the bounded wait is also the stop-latency bound.
        unsafe { WaitForSingleObject(session.event, EVENT_TIMEOUT_MS) };
        let padding = match unsafe { session.client.GetCurrentPadding() } {
            Ok(p) => p,
            Err(e) => break abort_msg(format!("GetCurrentPadding failed: {e}")),
        };
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
            }
            PcmPull::Eof => {
                // Edge drained: everything produced has been submitted.
                let _ = unsafe { session.render.ReleaseBuffer(0, 0) };
                break drain_to_zero(session);
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
fn drain_to_zero(session: &DeviceSession) -> LoopOutcome {
    let deadline = Instant::now() + DRAIN_CAP;
    loop {
        match unsafe { session.client.GetCurrentPadding() } {
            Ok(0) => return LoopOutcome::Drained,
            Ok(_) => {}
            Err(e) => return abort_msg(format!("drain padding check failed: {e}")),
        }
        if Instant::now() > deadline {
            return abort_msg("drain deadline passed before the device played out".to_owned());
        }
        unsafe { WaitForSingleObject(session.event, EVENT_TIMEOUT_MS) };
    }
}

fn abort_msg(message: String) -> LoopOutcome {
    LoopOutcome::Aborted { message }
}
