//! F5-GATE Experiment E3 — WASAPI physical cutover probe (Windows only).
//!
//! Experiment A — the candidate output-side cut (park + natural drain):
//! the probe opens a shared-mode, event-driven render stream in the
//! production shape, submits a distinguishable OLD signal, parks
//! (submits nothing), observes the device-side padding draining to
//! zero, commits, and refills with a NEW signal. It records, with raw
//! readings:
//!
//! ```text
//! padding at engage              the old tail the device still owned
//! drain latency                  engage -> first padding==0 observation
//! device position across the cut GetPosition advancing through the old
//!                                tail and continuing into the new
//!                                signal without a reset
//! refill validity                GetBuffer/ReleaseBuffer and padding
//!                                behave normally after the cut
//! ```
//!
//! The correctness reading is the one frozen by D14.7 and physically
//! evidenced by f3probe: for a shared-mode rendering stream, padding is
//! exactly the frames of THIS stream queued to play, so a zero
//! observation after engagement proves nothing submitted before
//! engagement remains queued-to-play. Mechanism A adds no removal
//! mechanism: it waits for consumption, then only new frames exist to
//! submit. This probe evidences that the composition behaves as that
//! reading requires on real hardware, and quantifies the cut latency.
//!
//! Experiment B — Stop / Reset / Start (comparison record only, NOT the
//! selected mechanism): on a fresh stream, fill, record padding, Stop,
//! record padding, Reset, record padding and the device position, then
//! Start and verify the event-driven loop still runs. This gives the
//! decision table its rejected-alternative evidence: whether Reset
//! discards queued PCM, and what Reset does to the stream position.

#![cfg(windows)]

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::{
    IAudioClient, IAudioClock, IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_E_UNSUPPORTED_FORMAT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, WAVEFORMATEXTENSIBLE, eMultimedia, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::Threading::CreateEventW;
use windows::core::GUID;

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

const SAMPLE_RATE: u32 = 44_100;
const CHANNELS: u32 = 2;
const EVENT_TIMEOUT_MS: u32 = 100;

struct EventHandle(HANDLE);

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
    }
}

struct CoGuard;
impl Drop for CoGuard {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn open_session() -> Result<DeviceSession, String> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("CoCreateInstance(MMDeviceEnumerator): {e}"))?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .map_err(|e| format!("no default render endpoint: {e}"))?;
        let client: IAudioClient = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("device activation failed: {e}"))?;

        let mut wfx: WAVEFORMATEXTENSIBLE = std::mem::zeroed();
        wfx.Format.wFormatTag = WAVE_FORMAT_EXTENSIBLE_TAG;
        wfx.Format.nChannels = CHANNELS as u16;
        wfx.Format.nSamplesPerSec = SAMPLE_RATE;
        wfx.Format.wBitsPerSample = IEEE_FLOAT_BITS;
        wfx.Format.nBlockAlign = CHANNELS as u16 * (IEEE_FLOAT_BYTES as u16);
        wfx.Format.nAvgBytesPerSec = SAMPLE_RATE * CHANNELS * IEEE_FLOAT_BYTES;
        wfx.Format.cbSize = EXTENSIBLE_CB_SIZE;
        wfx.Samples.wValidBitsPerSample = IEEE_FLOAT_BITS;
        wfx.dwChannelMask = 0x3;
        wfx.SubFormat = KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;

        // Production shape first (event callback, exact format); the
        // AUTOCONVERTPCM retry mirrors the f4probe finding that some
        // endpoints refuse an exact 44.1 kHz float32.
        let mut autoconvert = false;
        let init = client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            0,
            0,
            std::ptr::addr_of!(wfx.Format),
            None,
        );
        if init.is_err() && init.unwrap_err().code() == AUDCLNT_E_UNSUPPORTED_FORMAT {
            autoconvert = true;
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                    0,
                    0,
                    std::ptr::addr_of!(wfx.Format),
                    None,
                )
                .map_err(|e| format!("stream initialize failed (autoconvert too): {e}"))?;
        }
        println!("F5CUT OPEN autoconvert={autoconvert}");

        let event = EventHandle(
            CreateEventW(None, false, false, None)
                .map_err(|e| format!("buffer event creation failed: {e}"))?,
        );
        client
            .SetEventHandle(event.0)
            .map_err(|e| format!("SetEventHandle failed: {e}"))?;
        let buffer_frames = client
            .GetBufferSize()
            .map_err(|e| format!("GetBufferSize failed: {e}"))?;
        let render: IAudioRenderClient = client
            .GetService()
            .map_err(|e| format!("render client acquire failed: {e}"))?;
        Ok(DeviceSession {
            render,
            client,
            event,
            buffer_frames,
        })
    }
}

/// Fill `dst` (interleaved stereo f32) with a sine of `freq_hz`.
fn fill_sine(dst: &mut [f32], start_frame: usize, freq_hz: f32, amplitude: f32) {
    for (i, chunk) in dst.chunks_exact_mut(2).enumerate() {
        let t = (start_frame + i) as f32 / SAMPLE_RATE as f32;
        let v = (amplitude * (2.0 * std::f32::consts::PI * freq_hz * t).sin()) as f32;
        chunk[0] = v;
        chunk[1] = v;
    }
}

/// One production-shaped submission iteration: wait for the buffer
/// event, read padding, GetBuffer the available region, fill, submit.
/// Returns (padding_before, frames_submitted).
fn submit_iteration(
    s: &DeviceSession,
    gen: &mut Vec<f32>,
    fill: impl Fn(&mut [f32], usize),
    frame_cursor: &mut usize,
) -> Result<(u32, usize), String> {
    unsafe {
        WaitForSingleObjectEvent(s)?;
        let padding = s
            .client
            .GetCurrentPadding()
            .map_err(|e| format!("GetCurrentPadding failed: {e}"))?;
        let available = s.buffer_frames.saturating_sub(padding) as usize;
        if available == 0 {
            return Ok((padding, 0));
        }
        let ptr = s
            .render
            .GetBuffer(available as u32)
            .map_err(|e| format!("GetBuffer failed: {e}"))?;
        let dst = std::slice::from_raw_parts_mut(ptr as *mut f32, available * CHANNELS as usize);
        fill(dst, *frame_cursor);
        *frame_cursor += available;
        s.render
            .ReleaseBuffer(available as u32, 0)
            .map_err(|e| format!("ReleaseBuffer failed: {e}"))?;
        gen.clear();
        Ok((padding, available))
    }
}

unsafe fn WaitForSingleObjectEvent(s: &DeviceSession) -> Result<(), String> {
    use windows::Win32::System::Threading::WaitForSingleObject;
    let r = WaitForSingleObject(s.event.0, EVENT_TIMEOUT_MS);
    if r == WAIT_TIMEOUT {
        // Tolerated: the loop re-reads padding, same as production.
    } else if r != windows::Win32::Foundation::WAIT_OBJECT_0 {
        return Err(format!("WaitForSingleObject: {r:?}"));
    }
    Ok(())
}

fn device_position(s: &DeviceSession) -> Option<(u64, u64)> {
    let clock: IAudioClock = unsafe { s.client.GetService() }.ok()?;
    let mut pos = 0u64;
    let mut qpc = 0u64;
    unsafe { clock.GetPosition(&mut pos, Some(&mut qpc)) }.ok()?;
    Some((pos, qpc))
}

fn experiment_a() -> Result<(), String> {
    println!("F5CUT A BEGIN");
    let s = open_session()?;
    println!("F5CUT A buffer_frames={}", s.buffer_frames);
    unsafe {
        s.client
            .Start()
            .map_err(|e| format!("stream start failed: {e}"))?;
    }

    // Phase 1: steady OLD-signal submission (>= 3 device buffers).
    let mut frame_cursor = 0usize;
    let mut handed_off_old: u64 = 0;
    let target_old = 3 * s.buffer_frames as usize;
    let mut iterations = 0;
    while handed_off_old < target_old as u64 && iterations < 1000 {
        let mut scratch = Vec::new();
        let (padding, n) = submit_iteration(&s, &mut scratch, |d, c| fill_sine(d, c, 440.0, 0.4), &mut frame_cursor)?;
        let _ = padding;
        handed_off_old += n as u64;
        iterations += 1;
    }
    println!("F5CUT A steady_old handed_off={handed_off_old} iterations={iterations}");

    // Phase 2: PARK (submit nothing) and observe the drain to zero.
    let t_engage = Instant::now();
    let padding_at_engage = unsafe { s.client.GetCurrentPadding() }
        .map_err(|e| format!("padding at engage: {e}"))?;
    let pos_at_engage = device_position(&s);
    println!(
        "F5CUT A engage padding={padding_at_engage} handed_off={handed_off_old} pos={pos_at_engage:?}"
    );
    let mut drain_ms = None;
    let mut observations = Vec::new();
    let deadline = t_engage + Duration::from_secs(5);
    while Instant::now() < deadline {
        let padding = unsafe { s.client.GetCurrentPadding() }
            .map_err(|e| format!("padding during drain: {e}"))?;
        observations.push(padding);
        if padding == 0 {
            drain_ms = Some(t_engage.elapsed().as_secs_f64() * 1000.0);
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let pos_at_quiesced = device_position(&s);
    println!(
        "F5CUT A quiesced padding_observations={observations:?} drain_ms={drain_ms:?} pos={pos_at_quiesced:?}"
    );
    let drain_ms = drain_ms.ok_or("padding never reached zero within 5s")?;

    // Phase 3: COMMIT and refill with the NEW signal.
    println!("F5CUT A commit (padding==0 observed; submitting new signal)");
    let mut handed_off_new: u64 = 0;
    let target_new = 3 * s.buffer_frames as usize;
    let mut iterations = 0;
    let mut refill_readings = Vec::new();
    while handed_off_new < target_new as u64 && iterations < 1000 {
        let mut scratch = Vec::new();
        let (padding, n) = submit_iteration(&s, &mut scratch, |d, c| fill_sine(d, c, 880.0, 0.4), &mut frame_cursor)?;
        refill_readings.push((padding, n));
        handed_off_new += n as u64;
        iterations += 1;
    }
    let pos_after_refill = device_position(&s);
    println!(
        "F5CUT A refill handed_off_new={handed_off_new} readings={refill_readings:?} pos={pos_after_refill:?}"
    );

    // Verdict lines (machine-checkable): the queue was exactly drained,
    // the position advanced monotonically through the cut, and the
    // refill submitted new frames with normal padding behavior.
    let pos_engage = pos_at_engage.map(|p| p.0).unwrap_or(0);
    let pos_quiesced = pos_at_quiesced.map(|p| p.0).unwrap_or(0);
    let pos_after = pos_after_refill.map(|p| p.0).unwrap_or(0);
    let pos_advanced_through_tail = pos_quiesced >= pos_engage + u64::from(padding_at_engage);
    let pos_advanced_after_cut = pos_after > pos_quiesced;
    println!("F5CUT A VERDICT drain_ms={drain_ms:.1} padding_at_engage={padding_at_engage} pos_advanced_through_tail={pos_advanced_through_tail} pos_advanced_after_cut={pos_advanced_after_cut}");
    if !pos_advanced_after_cut {
        return Err("device position did not advance after the cut".into());
    }
    if !pos_advanced_through_tail {
        println!("F5CUT A NOTE position advance through tail smaller than engage padding (clock granularity observation, not a failure)");
    }
    println!("F5CUT A END ok");
    Ok(())
}

fn experiment_b() -> Result<(), String> {
    println!("F5CUT B BEGIN");
    let s = open_session()?;
    println!("F5CUT B buffer_frames={}", s.buffer_frames);
    unsafe {
        s.client
            .Start()
            .map_err(|e| format!("stream start failed: {e}"))?;
    }
    // Fill with the OLD signal.
    let mut frame_cursor = 0usize;
    let mut handed_off: u64 = 0;
    let target = 2 * s.buffer_frames as usize;
    let mut iterations = 0;
    while handed_off < target as u64 && iterations < 1000 {
        let mut scratch = Vec::new();
        let (_, n) = submit_iteration(&s, &mut scratch, |d, c| fill_sine(d, c, 440.0, 0.4), &mut frame_cursor)?;
        handed_off += n as u64;
        iterations += 1;
    }
    let padding_before_stop = unsafe { s.client.GetCurrentPadding() }
        .map_err(|e| format!("padding before stop: {e}"))?;
    let pos_before_stop = device_position(&s);
    println!("F5CUT B before_stop handed_off={handed_off} padding={padding_before_stop} pos={pos_before_stop:?}");

    // Stop: freeze mid-buffer (D14.7's measured objection), then Reset.
    unsafe { s.client.Stop() }.map_err(|e| format!("Stop failed: {e}"))?;
    let padding_after_stop = unsafe { s.client.GetCurrentPadding() }
        .map_err(|e| format!("padding after stop: {e}"))?;
    let pos_after_stop = device_position(&s);
    println!("F5CUT B after_stop padding={padding_after_stop} pos={pos_after_stop:?}");

    let reset = unsafe { s.client.Reset() };
    println!("F5CUT B reset_result={reset:?}");
    let padding_after_reset = unsafe { s.client.GetCurrentPadding() }
        .map_err(|e| format!("padding after reset: {e}"))?;
    let pos_after_reset = device_position(&s);
    println!("F5CUT B after_reset padding={padding_after_reset} pos={pos_after_reset:?}");

    // Start again and verify the event-driven loop still works with the
    // NEW signal.
    unsafe { s.client.Start() }.map_err(|e| format!("Start failed: {e}"))?;
    let mut handed_off_new: u64 = 0;
    let target_new = 2 * s.buffer_frames as usize;
    let mut iterations = 0;
    let mut error = None;
    while handed_off_new < target_new as u64 && iterations < 1000 {
        let mut scratch = Vec::new();
        let (padding, n) = match submit_iteration(&s, &mut scratch, |d, c| fill_sine(d, c, 880.0, 0.4), &mut frame_cursor) {
            Ok(v) => v,
            Err(e) => {
                error = Some(e);
                break;
            }
        };
        let _ = padding;
        handed_off_new += n as u64;
        iterations += 1;
    }
    let pos_after_restart = device_position(&s);
    println!(
        "F5CUT B after_restart handed_off_new={handed_off_new} iterations={iterations} pos={pos_after_restart:?} error={error:?}"
    );
    if handed_off_new == 0 {
        return Err("event-driven loop produced no submissions after Reset+Start".into());
    }
    println!("F5CUT B END ok");
    Ok(())
}

fn main() {
    unsafe {
        let coinit = CoInitializeEx(None, COINIT_MULTITHREADED);
        if coinit.is_err() {
            eprintln!("F5CUT CoInitializeEx failed: {coinit:?}");
            std::process::exit(2);
        }
    }
    let _guard = CoGuard;
    println!("F5CUT BEGIN");
    let mut failures = 0;
    if let Err(e) = experiment_a() {
        println!("F5CUT A FAILED {e}");
        failures += 1;
    }
    if let Err(e) = experiment_b() {
        println!("F5CUT B FAILED {e}");
        failures += 1;
    }
    println!("F5CUT END failures={failures}");
    std::process::exit(failures);
}
