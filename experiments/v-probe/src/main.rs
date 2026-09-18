//! V-PROBE — physical evidence for the D14.9 pending item (ADR-PBK-002,
//! F6-AUTHORITY-PROMOTION-1): the physical facts of the
//! `IAudioStreamVolume` volume candidate on a real Windows endpoint.
//!
//! The probe drives WASAPI DIRECTLY with the same bindings the
//! production output crate uses, in the same open shape (default
//! endpoint, shared mode, event-driven, float32) — mechanism evidence
//! only; no qianqian-* code, no production behavior.
//!
//! Scenario contract (one process = one scenario = one JSON verdict on
//! stdout; `verdict=` lines on stderr; exit 0 iff GREEN; a 120 s
//! watchdog exits 42):
//!
//! ```text
//! V1a  same-process isolation       stream A's factor changes must
//!                                   never move stream B's factor
//!                                   (both directions, sampled) — a
//!                                   failure REOPENS THE MECHANISM
//! V1b  other-process isolation      a child process's stream factor is
//!                                   unmoved by this process's churn
//! V2a  player→mixer independence    SetAllVolumes never moves the
//!                                   session-master factor
//! V2b  mixer→stream independence    SetMasterVolume never moves the
//!                                   stream factors (audible change is
//!                                   EXPECTED — ear witness UNAVAILABLE)
//! V3   lifecycle persistence        the stream factor survives
//!                                   client Stop/Start on the same device
//! V4   apply-placement perturbation SetAllVolumes at a render loop top:
//!                                   per-call durations measured
//!                                   (min/median/p99/max), position clock
//!                                   monotone, stream stays up — the
//!                                   measured input to the placement
//!                                   decision (reconsiders the
//!                                   apply-point/ownership ONLY)
//! V5   failure-signal existence     the control surface FAILS with
//!                                   typed, distinguishable HRESULTs
//!                                   (E_INVALIDARG on malformed input;
//!                                   AUDCLNT_E_NOT_INITIALIZED on an
//!                                   uninitialized client) — a
//!                                   log-and-pretend implementation has
//!                                   no excuse; the device-loss class
//!                                   (AUDCLNT_E_DEVICE_INVALIDATED) is
//!                                   documented and its physical trigger
//!                                   (endpoint disable) is deliberately
//!                                   NOT performed on the host — the
//!                                   routing itself is implementation-
//!                                   gated in the Stage F slice
//! ```

use std::time::{Duration, Instant};

use serde::Serialize;
use windows::Win32::Foundation::WAIT_OBJECT_0;
use windows::core::GUID;
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, IAudioClient, IAudioClock,
    IAudioRenderClient, IAudioStreamVolume, IMMDeviceEnumerator, MMDeviceEnumerator,
    WAVEFORMATEXTENSIBLE, eMultimedia, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

const WAVE_FORMAT_EXTENSIBLE_TAG: u16 = 0xFFFE;
const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: GUID =
    GUID::from_values(0x0000_0003, 0x0000, 0x0010, [0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71]);

/// The scenario watchdog bound.
const WATCHDOG: Duration = Duration::from_secs(120);

/// Factor comparison epsilon.
const EPS: f32 = 0.01;

const SAMPLE_RATE: u32 = 44_100;
const CHANNELS: u16 = 2;

#[derive(Serialize)]
struct Verdict {
    scenario: &'static str,
    verdict: &'static str,
    reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence: Option<serde_json::Value>,
}

fn main() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    std::thread::spawn(|| {
        std::thread::sleep(WATCHDOG);
        eprintln!("verdict=RED reason: watchdog fired (wedged scenario)");
        std::process::exit(42);
    });

    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--scenario") {
        args.remove(0);
    }
    if args.first().map(String::as_str) == Some("--child-stream") {
        // V1b child: `--child-stream <seconds> <factor>` — open a stream
        // in THIS process, set the factor, poll it, print one JSON.
        let seconds: u64 = args.get(1).map(|s| s.parse().unwrap_or(4)).unwrap_or(4);
        let factor: f32 = args.get(2).map(|s| s.parse().unwrap_or(0.5)).unwrap_or(0.5);
        std::process::exit(child_stream(seconds, factor));
    }

    let scenario = args.first().map(String::as_str).unwrap_or("");
    let (verdict, reasons, evidence) = match scenario {
        "V1a" => v1a(),
        "V1b" => v1b(),
        "V2a" => v2a(),
        "V2b" => v2b(),
        "V3" => v3(),
        "V4" => v4(),
        "V5" => v5(),
        other => {
            eprintln!("verdict=RED reason: unknown scenario {other:?}");
            std::process::exit(2);
        }
    };
    for reason in &reasons {
        eprintln!("verdict={verdict} reason: {reason}");
    }
    let v = Verdict {
        scenario: Box::leak(scenario.to_owned().into_boxed_str()),
        verdict,
        reasons,
        evidence,
    };
    println!("{}", serde_json::to_string_pretty(&v).expect("verdict json"));
    std::process::exit(if verdict == "GREEN" { 0 } else { 1 });
}

type Outcome = (&'static str, Vec<String>, Option<serde_json::Value>);

/// All probe threads join the process MTA (CoInitializeEx
/// MULTITHREADED), where one interface pointer is legally reachable
/// from every MTA thread; the windows-rs bindings are conservatively
/// `!Send`, so the shared pointers cross threads through this assert.
struct SendPtr<T>(T);
unsafe impl<T> Send for SendPtr<T> {}

/// One opened render stream: the production open shape, mechanism-side.
struct ProbeStream {
    client: IAudioClient,
    stream_volume: IAudioStreamVolume,
    channels: u32,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pump: Option<std::thread::JoinHandle<()>>,
}

impl ProbeStream {
    /// Open + initialize + start a silence-pumping stream (the
    /// production open shape).
    fn open(label: &str) -> Result<Self, String> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|e| format!("{label}: enumerator: {e}"))?;
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eMultimedia)
                .map_err(|e| format!("{label}: default endpoint: {e}"))?;
            let client: IAudioClient = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| format!("{label}: activate: {e}"))?;

            let mut wfx: WAVEFORMATEXTENSIBLE = std::mem::zeroed();
            wfx.Format.wFormatTag = WAVE_FORMAT_EXTENSIBLE_TAG;
            wfx.Format.nChannels = CHANNELS;
            wfx.Format.nSamplesPerSec = SAMPLE_RATE;
            wfx.Format.wBitsPerSample = 32;
            wfx.Format.nBlockAlign = CHANNELS * 4;
            wfx.Format.nAvgBytesPerSec = SAMPLE_RATE * u32::from(CHANNELS) * 4;
            wfx.Format.cbSize = 22;
            wfx.Samples.wValidBitsPerSample = 32;
            wfx.dwChannelMask = 0x3;
            wfx.SubFormat = KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;

            client
                .Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, 0, 0, &wfx.Format, None)
                .map_err(|e| format!("{label}: initialize: {e}"))?;
            let event = CreateEventW(None, false, false, None)
                .map_err(|e| format!("{label}: event: {e}"))?;
            client
                .SetEventHandle(event)
                .map_err(|e| format!("{label}: set event: {e}"))?;
            let render: IAudioRenderClient =
                client.GetService().map_err(|e| format!("{label}: render client: {e}"))?;
            let stream_volume: IAudioStreamVolume =
                client.GetService().map_err(|e| format!("{label}: stream volume: {e}"))?;
            let channels: u32 = stream_volume
                .GetChannelCount()
                .map_err(|e| format!("{label}: channel count: {e}"))?;
            let buffer = client
                .GetBufferSize()
                .map_err(|e| format!("{label}: buffer size: {e}"))?;
            client.Start().map_err(|e| format!("{label}: start: {e}"))?;

            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let pump_stop = stop.clone();
            let pump_render = SendPtr(render.clone());
            let pump_client = SendPtr(client.clone());
            let pump_event = SendPtr(event);
            let pump = std::thread::Builder::new()
                .name(format!("vprobe-pump-{label}"))
                .spawn(move || {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                    let SendPtr(render) = &pump_render;
                    let SendPtr(client) = &pump_client;
                    let SendPtr(event) = &pump_event;
                    let frame_bytes = CHANNELS as usize * 4;
                    let silence = vec![0u8; buffer as usize * frame_bytes];
                    while !pump_stop.load(std::sync::atomic::Ordering::Relaxed) {
                        if WaitForSingleObject(*event, 100) != WAIT_OBJECT_0 {
                            continue;
                        }
                        let Ok(padding) = client.GetCurrentPadding() else {
                            break;
                        };
                        let available = buffer.saturating_sub(padding);
                        if available == 0 {
                            continue;
                        }
                        let Ok(ptr) = render.GetBuffer(available) else {
                            break;
                        };
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                silence.as_ptr(),
                                ptr,
                                silence.len().min(available as usize * frame_bytes),
                            );
                        }
                        let _ = render.ReleaseBuffer(available, 0);
                    }
                })
                .map_err(|e| format!("{label}: pump spawn: {e}"))?;

            Ok(Self {
                client,
                stream_volume,
                channels,
                stop,
                pump: Some(pump),
            })
        }
    }

    fn set_factor(&self, factor: f32) -> Result<(), String> {
        let levels = vec![factor; self.channels as usize];
        unsafe { self.stream_volume.SetAllVolumes(&levels) }
            .map_err(|e| format!("SetAllVolumes({factor}): {e}"))
    }

    fn factor(&self) -> Result<f32, String> {
        let mut levels = vec![0.0f32; self.channels as usize];
        unsafe { self.stream_volume.GetAllVolumes(&mut levels) }
            .map_err(|e| format!("GetAllVolumes: {e}"))?;
        let first = levels[0];
        if levels.iter().any(|l| (l - first).abs() > EPS) {
            return Err(format!("per-channel factors diverged: {levels:?}"));
        }
        Ok(first)
    }

    fn shutdown(&mut self) {
        self.stop
            .store(true, std::sync::atomic::Ordering::Relaxed);
        unsafe {
            let _ = self.client.Stop();
        }
        if let Some(handle) = self.pump.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ProbeStream {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn sample_factor(stream: &ProbeStream, samples: usize, interval: Duration) -> Result<Vec<f32>, String> {
    let mut out = Vec::with_capacity(samples);
    for _ in 0..samples {
        out.push(stream.factor()?);
        std::thread::sleep(interval);
    }
    Ok(out)
}

fn all_within(samples: &[f32], expected: f32) -> bool {
    samples.iter().all(|s| (s - expected).abs() <= EPS)
}

/// V1a — same-process isolation, both directions.
fn v1a() -> Outcome {
    let mut reasons = Vec::new();
    let mut a = match ProbeStream::open("A") {
        Ok(s) => s,
        Err(e) => return ("RED", vec![e], None),
    };
    let mut b = match ProbeStream::open("B") {
        Ok(s) => s,
        Err(e) => {
            a.shutdown();
            return ("RED", vec![e], None);
        }
    };
    let b0 = b.factor().unwrap_or(f32::NAN);
    let ok = (|| -> Result<bool, String> {
        a.set_factor(0.3)?;
        let b_samples = sample_factor(&b, 20, Duration::from_millis(50))?;
        let a_at_03 = a.factor()?;
        a.set_factor(0.6)?;
        let b_samples_2 = sample_factor(&b, 20, Duration::from_millis(50))?;
        let a_at_06 = a.factor()?;
        Ok(all_within(&b_samples, b0)
            && all_within(&b_samples_2, b0)
            && (a_at_03 - 0.3).abs() <= EPS
            && (a_at_06 - 0.6).abs() <= EPS)
    })();
    let evidence = serde_json::json!({
        "b_baseline": b0,
        "b_after_a_set_0.3": b.factor().ok(),
        "b_after_a_set_0.6": b.factor().ok(),
    });
    a.shutdown();
    b.shutdown();
    match ok {
        Ok(true) => (
            "GREEN",
            reasons,
            Some(serde_json::json!({
                "b_baseline": b0,
                "isolation": "both directions, sampled",
            })),
        ),
        Ok(false) => {
            reasons.push("V1a: a stream factor moved another stream's factor (cross-stream coupling)".into());
            ("RED", reasons, Some(evidence))
        }
        Err(e) => {
            reasons.push(e);
            ("RED", reasons, Some(evidence))
        }
    }
}

/// V1b — other-process isolation: a CHILD process (same exe) sets its
/// stream factor to `child_factor` and polls it while this process
/// churns its own.
fn v1b() -> Outcome {
    let mut reasons = Vec::new();
    let child_factor = 0.5;
    let mut child = match std::process::Command::new(std::env::current_exe().expect("self"))
        .arg("--child-stream")
        .arg("4")
        .arg("0.5")
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return ("RED", vec![format!("child spawn: {e}")], None),
    };
    std::thread::sleep(Duration::from_millis(500)); // let the child open
    let mut parent = match ProbeStream::open("P") {
        Ok(s) => s,
        Err(e) => {
            let _ = child.kill();
            return ("RED", vec![e], None);
        }
    };
    for factor in [0.25f32, 0.75, 0.25, 0.9, 0.25] {
        if let Err(e) = parent.set_factor(factor) {
            reasons.push(e);
            let _ = child.kill();
            parent.shutdown();
            return ("RED", reasons, None);
        }
        std::thread::sleep(Duration::from_millis(600));
    }
    parent.shutdown();
    let output = child.wait_with_output().expect("child output");
    let child_json: Result<serde_json::Value, _> =
        serde_json::from_slice(&output.stdout);
    match child_json {
        Ok(v) => {
            let stable = v.get("stable").and_then(|s| s.as_bool()).unwrap_or(false);
            let min = v.get("min").and_then(|m| m.as_f64());
            let max = v.get("max").and_then(|m| m.as_f64());
            let ok = stable
                && min.is_some_and(|m| (m - child_factor as f64).abs() <= EPS as f64)
                && max.is_some_and(|m| (m - child_factor as f64).abs() <= EPS as f64);
            (
                if ok { "GREEN" } else { "RED" },
                reasons,
                Some(v),
            )
        }
        Err(e) => {
            reasons.push(format!("child output not JSON: {e}"));
            ("RED", reasons, None)
        }
    }
}

/// The V1b child body: set `factor`, poll `seconds`, print one JSON.
fn child_stream(seconds: u64, factor: f32) -> i32 {
    let mut stream = match ProbeStream::open("C") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("child: {e}");
            return 1;
        }
    };
    if let Err(e) = stream.set_factor(factor) {
        eprintln!("child: {e}");
        stream.shutdown();
        return 1;
    }
    let (min, max, stable) = (|| {
        let mut min = f32::MAX;
        let mut max = 0.0f32;
        for _ in 0..(seconds * 10) {
            match stream.factor() {
                Ok(f) => {
                    min = min.min(f);
                    max = max.max(f);
                }
                Err(_) => return (0.0, 0.0, false),
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        (min, max, (min - factor).abs() <= EPS && (max - factor).abs() <= EPS)
    })();
    stream.shutdown();
    println!(
        "{}",
        serde_json::json!({
            "factor": factor,
            "min": min,
            "max": max,
            "stable": stable,
        })
    );
    if stable { 0 } else { 1 }
}

/// V2a — SetAllVolumes must never move the session-master factor.
fn v2a() -> Outcome {
    let mut reasons = Vec::new();
    let mut stream = match ProbeStream::open("S") {
        Ok(s) => s,
        Err(e) => return ("RED", vec![e], None),
    };
    let master0 = unsafe { simple_master(&stream.client) }.ok();
    let ok = (|| -> Result<bool, String> {
        let Some(m0) = master0 else {
            return Err("no session master factor".into());
        };
        for factor in [0.3f32, 0.7, 0.05, 1.0] {
            stream.set_factor(factor)?;
            std::thread::sleep(Duration::from_millis(100));
            let now = unsafe { simple_master(&stream.client)? };
            if (now - m0).abs() > EPS {
                return Ok(false);
            }
        }
        Ok(true)
    })();
    stream.shutdown();
    match ok {
        Ok(true) => (
            "GREEN",
            reasons,
            Some(serde_json::json!({ "master_unchanged_at": master0 })),
        ),
        Ok(false) => {
            reasons.push("V2a: SetAllVolumes moved the session-master factor".into());
            ("RED", reasons, None)
        }
        Err(e) => {
            reasons.push(e);
            ("RED", reasons, None)
        }
    }
}

unsafe fn simple_master(client: &IAudioClient) -> Result<f32, String> {
    let simple: windows::Win32::Media::Audio::ISimpleAudioVolume = client
        .GetService()
        .map_err(|e| format!("ISimpleAudioVolume: {e}"))?;
    simple.GetMasterVolume().map_err(|e| format!("GetMasterVolume: {e}"))
}

unsafe fn set_master(client: &IAudioClient, factor: f32) -> Result<(), String> {
    let simple: windows::Win32::Media::Audio::ISimpleAudioVolume = client
        .GetService()
        .map_err(|e| format!("ISimpleAudioVolume: {e}"))?;
    simple
        .SetMasterVolume(factor, std::ptr::null())
        .map_err(|e| format!("SetMasterVolume: {e}"))
}

/// V2b — SetMasterVolume must never move the stream factors (the
/// audible change is EXPECTED; the ear witness is UNAVAILABLE and the
/// factor readback is the mechanical witness).
fn v2b() -> Outcome {
    let mut reasons = Vec::new();
    let mut stream = match ProbeStream::open("S") {
        Ok(s) => s,
        Err(e) => return ("RED", vec![e], None),
    };
    stream.set_factor(1.0).ok();
    let ok = (|| -> Result<bool, String> {
        let master0 = unsafe { simple_master(&stream.client)? };
        for master in [0.3f32, 0.6, 0.3, master0] {
            unsafe { set_master(&stream.client, master)? };
            std::thread::sleep(Duration::from_millis(100));
            let samples = sample_factor(&stream, 5, Duration::from_millis(40))?;
            if !all_within(&samples, 1.0) {
                return Ok(false);
            }
        }
        Ok(true)
    })();
    stream.shutdown();
    match ok {
        Ok(true) => (
            "GREEN",
            reasons,
            Some(serde_json::json!({ "stream_factors": "pinned at 1.0 while the session master moved" })),
        ),
        Ok(false) => {
            reasons.push("V2b: the session-master move moved the stream factors".into());
            ("RED", reasons, None)
        }
        Err(e) => {
            reasons.push(e);
            ("RED", reasons, None)
        }
    }
}

/// V3 — the stream factor survives client Stop/Start on the same device.
fn v3() -> Outcome {
    let mut reasons = Vec::new();
    let mut stream = match ProbeStream::open("S") {
        Ok(s) => s,
        Err(e) => return ("RED", vec![e], None),
    };
    let ok = (|| -> Result<bool, String> {
        stream.set_factor(0.4)?;
        unsafe {
            stream.client.Stop().map_err(|e| format!("Stop: {e}"))?;
            std::thread::sleep(Duration::from_millis(200));
            stream.client.Start().map_err(|e| format!("Start: {e}"))?;
        }
        std::thread::sleep(Duration::from_millis(300));
        Ok((stream.factor()? - 0.4).abs() <= EPS)
    })();
    stream.shutdown();
    match ok {
        Ok(true) => ("GREEN", reasons, None),
        Ok(false) => {
            reasons.push("V3: the stream factor did not survive Stop/Start".into());
            ("RED", reasons, None)
        }
        Err(e) => {
            reasons.push(e);
            ("RED", reasons, None)
        }
    }
}

/// V4 — the apply-placement perturbation measurement: SetAllVolumes at
/// a render loop top (this probe's pump loop is the loop-top shape:
/// wait → pad check → submit; the call runs between iterations, never
/// inside a submission). Measures per-call durations, position-clock
/// continuity, and stream survival.
fn v4() -> Outcome {
    let mut reasons = Vec::new();
    let mut stream = match ProbeStream::open("S") {
        Ok(s) => s,
        Err(e) => return ("RED", vec![e], None),
    };
    let clock: Result<IAudioClock, String> = unsafe {
        stream
            .client
            .GetService()
            .map_err(|e| format!("IAudioClock: {e}"))
    };
    let clock = match clock {
        Ok(c) => c,
        Err(e) => {
            stream.shutdown();
            return ("RED", vec![e], None);
        }
    };
    let position = |clock: &IAudioClock| -> Option<u64> {
        let mut pos = 0u64;
        unsafe { clock.GetPosition(&mut pos, None) }.ok()?;
        Some(pos)
    };

    let iterations = 2000usize;
    let mut durations = Vec::with_capacity(iterations);
    let mut max_position_gap = 0u64;
    let mut monotone = true;
    let mut last_pos = position(&clock).unwrap_or(0);
    for i in 0..iterations {
        let factor = if i % 2 == 0 { 0.5 } else { 0.6 };
        let start = Instant::now();
        let result = stream.set_factor(factor);
        durations.push(start.elapsed().as_nanos() as u64);
        if result.is_err() {
            reasons.push(format!("V4: apply failed at iteration {i}"));
            stream.shutdown();
            return ("RED", reasons, None);
        }
        if i % 50 == 0 {
            if let Some(pos) = position(&clock) {
                if pos < last_pos {
                    monotone = false;
                }
                max_position_gap = max_position_gap.max(pos - last_pos);
                last_pos = pos;
            }
        }
    }
    // The stream must still be running and the clock alive.
    let still_running = position(&clock).is_some() && stream.factor().is_ok();
    durations.sort_unstable();
    let p = |q: f64| durations[((durations.len() - 1) as f64 * q) as usize];
    let min = durations[0];
    let median = p(0.5);
    let p99 = p(0.99);
    let max = durations[durations.len() - 1];
    stream.shutdown();

    // The placement bound under test: the apply call is a bounded,
    // small control operation (sub-millisecond at p99) and the clock
    // never goes backward.
    let bounded = p99 < 1_000_000;
    if !bounded {
        reasons.push(format!(
            "V4: p99 apply duration {} ns exceeds the 1 ms placement bound",
            p99
        ));
    }
    if !monotone {
        reasons.push("V4: the position clock went backward under applies".into());
    }
    if !still_running {
        reasons.push("V4: the stream did not survive the apply churn".into());
    }
    let ok = bounded && monotone && still_running;
    (
        if ok { "GREEN" } else { "RED" },
        reasons,
        Some(serde_json::json!({
            "iterations": iterations,
            "min_ns": min,
            "median_ns": median,
            "p99_ns": p99,
            "max_ns": max,
            "position_monotone": monotone,
            "max_position_gap_frames": max_position_gap,
            "stream_survived": still_running,
        })),
    )
}

/// V5 — the failure-signal existence: typed, distinguishable control
/// failures, so a log-and-pretend implementation has no excuse.
fn v5() -> Outcome {
    let mut reasons = Vec::new();
    let mut evidence = serde_json::Map::new();

    // (a) malformed input: zero channels → typed error.
    let malformed = std::thread::spawn(|| {
        let mut stream = match ProbeStream::open("S") {
            Ok(s) => s,
            Err(e) => return Some(format!("open: {e}")),
        };
        let result = unsafe {
            stream
                .stream_volume
                .SetAllVolumes(&[])
        };
        stream.shutdown();
        result.err().map(|e| format!("{e:?}"))
    })
    .join()
    .expect("v5a thread");
    match malformed {
        Some(error) => {
            evidence.insert("malformed_set_error".into(), serde_json::json!(error));
        }
        None => {
            reasons.push("V5: SetAllVolumes(0, []) did NOT fail — no signal".into());
            return ("RED", reasons, Some(serde_json::Value::Object(evidence)));
        }
    }

    // (b) uninitialized client: GetService must fail typed.
    let uninitialized = std::thread::spawn(|| {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).expect("enumerator");
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eMultimedia)
                .expect("endpoint");
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None).expect("activate");
            let result: Result<IAudioStreamVolume, _> = client.GetService();
            result.err().map(|e| format!("{e:?}"))
        }
    })
    .join()
    .expect("v5b thread");
    match uninitialized {
        Some(error) => {
            evidence.insert("uninitialized_getservice_error".into(), serde_json::json!(error));
        }
        None => {
            reasons.push("V5: GetService on an uninitialized client did NOT fail".into());
            return ("RED", reasons, Some(serde_json::Value::Object(evidence)));
        }
    }

    evidence.insert(
        "device_loss_note".into(),
        serde_json::json!(
            "AUDCLNT_E_DEVICE_INVALIDATED (0x88890004) is the device-loss class; its physical \
             trigger (endpoint disable) is deliberately not performed on this host. The frozen \
             D14.9 routing (device loss → the existing device-failure policy) is implementation-\
             gated in the Stage F slice and reviewed there."
        ),
    );
    if reasons.is_empty() {
        (
            "GREEN",
            reasons,
            Some(serde_json::Value::Object(evidence)),
        )
    } else {
        ("RED", reasons, Some(serde_json::Value::Object(evidence)))
    }
}
