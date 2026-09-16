//! F3-GATE physical probe: both candidate pause mechanisms against the
//! real WASAPI shared-mode event-driven device (Windows only).
//!
//! The probe is evidence tooling, not a product path. It mirrors the
//! production render-loop order (wait → padding → GetBuffer → read →
//! release, `qianqian-output-wasapi` steady_loop) and inserts the two
//! candidate mechanisms:
//!
//! ```text
//! A  gate at loop top, strictly before GetBuffer
//! B  the same gate wrapped in IAudioClient::Stop / Start
//! ```
//!
//! Measured per phase (printed as F3PROBE lines on stdout):
//!
//! ```text
//! engage ack latency      pause command → parked acknowledgment
//! padding timeline        already-submitted audio fate while parked
//!                         (A: drains to zero; B: frozen)
//! resume latency          resume command → first refill (A) / Start (B)
//! GetBuffer-across-park   count (must be 0, asserted)
//! stop-from-paused        exit latency of the parked render leg
//! device continuity       exactly one device session per phase
//! ```
//!
//! The probe plays a quiet 440 Hz tone through the default endpoint
//! while it runs (~12 s total). It is not an audibility gate: whether
//! the pause SOUNDS right is the human reviewer's acceptance.

#[cfg(not(windows))]
fn main() {
    // The physical probe is Windows-only by nature; on other platforms
    // the crate's synchronization-shape scenarios carry the evidence.
    eprintln!("f3probe: no physical probe on this platform (see tests/scenarios.rs)");
}

#[cfg(windows)]
fn main() {
    std::process::exit(win::run());
}

#[cfg(windows)]
mod win {
    use std::slice;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use windows::core::GUID;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IAudioClient, IAudioRenderClient, IMMDeviceEnumerator,
        MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
        AUDCLNT_STREAMFLAGS_EVENTCALLBACK, WAVEFORMATEXTENSIBLE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

    use f3_pause_mechanism::edge::{PcmEdge, Pull, WriteOutcome};
    use f3_pause_mechanism::gate::PauseGate;

    // ---- constants mirroring the production mechanism ----

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
    const EVENT_TIMEOUT_MS: u32 = 100;

    const SAMPLE_RATE: u32 = 48000;
    const CHANNELS: u16 = 2;
    const SINE_HZ: f32 = 440.0;
    const AMPLITUDE: f32 = 0.12;
    const EDGE_CAPACITY: usize = 8192;
    const CHUNK: usize = 1024;

    // ---- probe instrumentation ----

    #[derive(Clone, Copy, Debug)]
    enum Trace {
        GetBuffer,
        ReleaseBuffer(#[allow(dead_code)] usize),
        DeviceStop,
        DeviceStart,
    }

    #[derive(Default)]
    struct TraceLog {
        entries: Mutex<Vec<(Instant, Trace)>>,
    }

    impl TraceLog {
        fn push(&self, t: Trace) {
            self.entries
                .lock()
                .expect("trace lock")
                .push((Instant::now(), t));
        }
        fn count_between(
            &self,
            from: Instant,
            to: Instant,
            want: &dyn Fn(&Trace) -> bool,
        ) -> usize {
            self.entries
                .lock()
                .expect("trace lock")
                .iter()
                .filter(|(t, e)| *t >= from && *t < to && want(e))
                .count()
        }
        fn first_between(&self, from: Instant, want: &dyn Fn(&Trace) -> bool) -> Option<Duration> {
            self.entries
                .lock()
                .expect("trace lock")
                .iter()
                .filter(|(t, e)| *t >= from && want(e))
                .map(|(t, _)| t.duration_since(from))
                .next()
        }
    }

    struct Probe {
        trace: Arc<TraceLog>,
        padding: Arc<Mutex<Vec<(Instant, u32)>>>,
        gate: Arc<PauseGate>,
        edge: Arc<PcmEdge>,
        sessions: Arc<AtomicUsize>,
    }

    impl Probe {
        fn new() -> Self {
            Self {
                trace: Arc::new(TraceLog::default()),
                padding: Arc::new(Mutex::new(Vec::new())),
                gate: Arc::new(PauseGate::new()),
                edge: Arc::new(PcmEdge::new(CHANNELS, EDGE_CAPACITY)),
                sessions: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    // ---- sine producer (stands in for the decode worker) ----

    fn spawn_producer(edge: Arc<PcmEdge>) -> JoinHandle<()> {
        std::thread::Builder::new()
            .name("probe-producer".into())
            .spawn(move || {
                let channels = usize::from(CHANNELS);
                let mut phase = 0.0f32;
                let step = 2.0 * std::f32::consts::PI * SINE_HZ / SAMPLE_RATE as f32;
                let mut staging = vec![0.0f32; CHUNK * channels];
                loop {
                    for f in staging.chunks_mut(channels) {
                        let s = AMPLITUDE * phase.sin();
                        for smp in f {
                            *smp = s;
                        }
                        phase += step;
                    }
                    if edge.write(&staging) == WriteOutcome::Stopped {
                        return;
                    }
                }
            })
            .expect("producer spawn")
    }

    // ---- WASAPI device session (production open shape) ----

    struct DeviceSession {
        render: IAudioRenderClient,
        client: IAudioClient,
        event: EventHandle,
        buffer_frames: u32,
    }

    struct EventHandle(HANDLE);

    impl Drop for EventHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    impl Drop for DeviceSession {
        fn drop(&mut self) {
            unsafe {
                let _ = self.client.Stop();
            }
        }
    }

    struct CoApartmentGuard;
    impl Drop for CoApartmentGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    fn open_session_inner(probe: &Probe) -> Result<DeviceSession, String> {
        probe.sessions.fetch_add(1, Ordering::AcqRel);
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
            wfx.Format.nChannels = CHANNELS;
            wfx.Format.nSamplesPerSec = SAMPLE_RATE;
            wfx.Format.wBitsPerSample = IEEE_FLOAT_BITS;
            wfx.Format.nBlockAlign = CHANNELS * (IEEE_FLOAT_BYTES as u16);
            wfx.Format.nAvgBytesPerSec = SAMPLE_RATE * u32::from(CHANNELS) * IEEE_FLOAT_BYTES;
            wfx.Format.cbSize = EXTENSIBLE_CB_SIZE;
            wfx.Samples.wValidBitsPerSample = IEEE_FLOAT_BITS;
            wfx.dwChannelMask = 0x3;
            wfx.SubFormat = KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;

            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    0,
                    0,
                    std::ptr::addr_of!(wfx.Format),
                    None,
                )
                .map_err(|e| format!("stream initialize failed: {e}"))?;

            let event = CreateEventW(None, false, false, None)
                .map_err(|e| format!("buffer event creation failed: {e}"))?;
            let event = EventHandle(event);
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

    // ---- the probe render leg (both mechanisms) ----

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mechanism {
        GateOnly,
        GatePlusDeviceStop,
    }

    impl Mechanism {
        fn name(self) -> &'static str {
            match self {
                Mechanism::GateOnly => "A-gate-only",
                Mechanism::GatePlusDeviceStop => "B-gate-plus-device-stop",
            }
        }
    }

    /// One probe render leg: COM apartment + its own device session +
    /// the gated loop. Exits when the data plane stops (or a device
    /// error aborts), reporting a one-line diagnostic via `diagnostic`.
    fn run_render_leg(
        probe: Probe,
        mechanism: Mechanism,
        opened: std::sync::mpsc::Sender<Result<u32, String>>,
        diagnostic: std::sync::mpsc::Sender<String>,
    ) {
        unsafe {
            let coinit = CoInitializeEx(None, COINIT_MULTITHREADED);
            if coinit.is_err() {
                let _ = opened.send(Err(format!("CoInitializeEx failed: {coinit:?}")));
                return;
            }
        }
        let apartment = CoApartmentGuard;
        {
            let session = match open_session_inner(&probe) {
                Ok(s) => {
                    let _ = opened.send(Ok(s.buffer_frames));
                    s
                }
                Err(e) => {
                    let _ = opened.send(Err(e));
                    drop(apartment);
                    return;
                }
            };
            let message = loop_body(&probe, &session, mechanism);
            drop(session);
            drop(apartment);
            let _ = diagnostic.send(message);
        }
        probe.edge.stop();
    }

    fn loop_body(probe: &Probe, session: &DeviceSession, mechanism: Mechanism) -> String {
        let channels = usize::from(CHANNELS);
        let _dst = vec![0.0f32; session.buffer_frames as usize * channels];
        unsafe {
            if let Err(e) = session.client.Start() {
                return format!("stream start failed: {e}");
            }
            loop {
                // ---- pause gate: before any GetBuffer this iteration ----
                match mechanism {
                    Mechanism::GateOnly => probe.gate.park_while_paused(),
                    Mechanism::GatePlusDeviceStop => {
                        let o = probe.gate.observe();
                        if o.pause_requested && !o.stopped {
                            if let Err(e) = session.client.Stop() {
                                return format!("client Stop failed: {e}");
                            }
                            probe.trace.push(Trace::DeviceStop);
                            probe.gate.park_while_paused();
                            if let Err(e) = session.client.Start() {
                                return format!("client Start failed: {e}");
                            }
                            probe.trace.push(Trace::DeviceStart);
                        }
                    }
                }

                WaitForSingleObject(session.event.0, EVENT_TIMEOUT_MS);
                let padding = match session.client.GetCurrentPadding() {
                    Ok(p) => p,
                    Err(e) => return format!("GetCurrentPadding failed: {e}"),
                };
                probe
                    .padding
                    .lock()
                    .expect("padding log lock")
                    .push((Instant::now(), padding));
                let available = session.buffer_frames.saturating_sub(padding) as usize;
                if available == 0 {
                    continue;
                }
                let ptr = match session.render.GetBuffer(available as u32) {
                    Ok(p) => p,
                    Err(e) => return format!("GetBuffer failed: {e}"),
                };
                probe.trace.push(Trace::GetBuffer);
                let buf = slice::from_raw_parts_mut(ptr as *mut f32, available * channels);
                match probe.edge.read_frames(buf) {
                    Pull::Frames(n) => {
                        if let Err(e) = session.render.ReleaseBuffer(n as u32, 0) {
                            return format!("ReleaseBuffer failed: {e}");
                        }
                        probe.trace.push(Trace::ReleaseBuffer(n));
                    }
                    Pull::Eof | Pull::Stopped => {
                        // The probe producer never EOFs; Eof/Stopped both
                        // end the leg the same way (release, then report).
                        let _ = session.render.ReleaseBuffer(0, 0);
                        probe.trace.push(Trace::ReleaseBuffer(0));
                        return String::new(); // data-plane stop: clean leg exit
                    }
                }
            }
        }
    }

    // ---- phase driver ----

    fn wait_engaged(gate: &PauseGate, timeout: Duration) -> Option<Duration> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if gate.observe().engaged {
                return Some(start.elapsed());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        None
    }

    fn join_within<T: Send + 'static>(handle: JoinHandle<T>, timeout: Duration) -> Option<T> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let out = handle.join().expect("joined thread panicked");
            let _ = tx.send(out);
        });
        rx.recv_timeout(timeout).ok()
    }

    fn report(msg: impl std::fmt::Display) {
        println!("F3PROBE {msg}");
    }

    /// One full pause/resume phase against one fresh leg. Leaves the
    /// producer running; the caller stops and joins the leg.
    fn run_phase(probe: &Probe, mechanism: Mechanism) -> (JoinHandle<()>, bool, usize) {
        let (opened_tx, opened_rx) = std::sync::mpsc::channel();
        let (diag_tx, diag_rx) = std::sync::mpsc::channel();
        let _ = &diag_rx;
        let leg_probe = Probe {
            trace: probe.trace.clone(),
            padding: probe.padding.clone(),
            gate: probe.gate.clone(),
            edge: probe.edge.clone(),
            sessions: probe.sessions.clone(),
        };
        let leg = std::thread::Builder::new()
            .name("probe-render".into())
            .spawn(move || run_render_leg(leg_probe, mechanism, opened_tx, diag_tx))
            .expect("render leg spawn");

        let buffer_frames = match opened_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(f)) => f,
            Ok(Err(e)) => panic!("device open failed: {e}"),
            Err(_) => panic!("device open verdict timeout"),
        };
        let sessions_at_open = probe.sessions.load(Ordering::Acquire);
        report(format!(
            "OPEN mech={} buffer_frames={} sessions={} (continuity: expect 1 per phase)",
            mechanism.name(),
            buffer_frames,
            sessions_at_open
        ));

        // Steady playback.
        std::thread::sleep(Duration::from_millis(1500));

        // ---- pause ----
        let pause_cmd = Instant::now();
        probe.gate.request_pause();
        let engage_ack = wait_engaged(&probe.gate, Duration::from_secs(2));
        report(format!(
            "ENGAGE mech={} ack_after_ms={} engaged={}",
            mechanism.name(),
            engage_ack.map(|d| d.as_millis()).unwrap_or(0),
            engage_ack.is_some()
        ));
        let parked_at = Instant::now();

        // Park window. The render leg is the padding observer and is
        // parked, so the park window itself has no samples; the fate of
        // the already-submitted audio is established by the ENDPOINTS:
        // the loop's last sample before the park vs its first sample
        // after the resume (taken before any new submission).
        //
        //   A (gate only):   first-after ≈ 0  (tail played out, then
        //                    nothing submitted → device silence)
        //   B (+device stop): first-after == last-before (frozen)
        let park_ms: u64 = std::env::var("F3PROBE_PARK_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1200);
        std::thread::sleep(Duration::from_millis(park_ms));
        let all_pads = probe.padding.lock().expect("padding log lock").clone();
        let pre_park_last = all_pads
            .iter()
            .filter(|(t, _)| *t < parked_at && *t + Duration::from_millis(200) >= pause_cmd)
            .map(|(_, p)| *p)
            .last();
        let pre_park_all: Vec<u32> = all_pads
            .iter()
            .filter(|(t, _)| *t < parked_at && *t + Duration::from_millis(200) >= pause_cmd)
            .map(|(_, p)| *p)
            .collect();
        report(format!(
            "PRE_PARK mech={} samples={:?} padding_last={:?}",
            mechanism.name(),
            pre_park_all,
            pre_park_last
        ));

        // ---- resume ----
        let resume_cmd = Instant::now();
        probe.gate.request_resume();
        let first_refill = (|| {
            let deadline = resume_cmd + Duration::from_secs(2);
            while Instant::now() < deadline {
                if let Some(d) = probe
                    .trace
                    .first_between(resume_cmd, &|t| matches!(t, Trace::GetBuffer))
                {
                    return d;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            resume_cmd.elapsed()
        })();
        report(format!(
            "RESUME mech={} first_refill_after_ms={}",
            mechanism.name(),
            first_refill.as_millis()
        ));

        // First padding sample after the resume (before any new
        // submission): A expect ≈ 0 (submitted tail played out during
        // the park); B expect == PRE_PARK padding_last (frozen).
        let fresh_pads = probe.padding.lock().expect("padding log lock").clone();
        let post_resume_first = fresh_pads
            .iter()
            .filter(|(t, _)| *t >= resume_cmd && *t <= resume_cmd + Duration::from_millis(300))
            .map(|(_, p)| *p)
            .next();
        let verdict = match (mechanism, pre_park_last, post_resume_first) {
            (Mechanism::GateOnly, _, Some(p)) if p == 0 => "drained-to-zero (as expected for A)",
            (Mechanism::GatePlusDeviceStop, Some(a), Some(b)) if a == b => {
                "frozen (as expected for B)"
            }
            _ => "UNEXPECTED",
        };
        report(format!(
            "PARK_FATE mech={} pre_park_last={:?} post_resume_first={:?} -> {verdict}",
            mechanism.name(),
            pre_park_last,
            post_resume_first
        ));

        // GetBuffer-across-park: between the engagement ack and the
        // resume command no GetBuffer may occur. (An iteration already
        // in flight when the command lands completes before the ack.)
        let getbuffer_in_park = probe
            .trace
            .count_between(parked_at, resume_cmd, &|t| matches!(t, Trace::GetBuffer));
        report(format!(
            "GETBUFFER_IN_PARK mech={} count={} (must be 0)",
            mechanism.name(),
            getbuffer_in_park
        ));

        // Device continuity is an enforced bound, not just printed
        // evidence: no reopen may happen inside the phase.
        let sessions_delta = probe.sessions.load(Ordering::Acquire) - sessions_at_open;
        let ok = engage_ack.is_some() && getbuffer_in_park == 0 && sessions_delta == 0;
        (leg, ok, sessions_delta)
    }

    /// Diagnostic mode (F3PROBE_INIT_MATRIX=1): report the default
    /// endpoint's identity/period/mix format and which Initialize flag/
    /// duration combos the device accepts. Decides whether a 0x88890008
    /// (AUDCLNT_E_INVALID_STREAM_FLAG) is environmental or code-shaped.
    fn run_init_matrix() -> i32 {
        unsafe {
            let coinit = CoInitializeEx(None, COINIT_MULTITHREADED);
            if coinit.is_err() {
                eprintln!("CoInitializeEx failed: {coinit:?}");
                return 1;
            }
            let _apt = CoApartmentGuard;
            let enumerator: IMMDeviceEnumerator =
                match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("CoCreateInstance failed: {e}");
                        return 1;
                    }
                };
            let device = match enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("GetDefaultAudioEndpoint failed: {e}");
                    return 1;
                }
            };
            let device_id: windows::core::PWSTR = match device.GetId() {
                Ok(id) => id,
                Err(e) => {
                    eprintln!("GetId failed: {e}");
                    return 1;
                }
            };
            println!(
                "F3PROBE-MATRIX endpoint={}",
                device_id.to_string().unwrap_or_default()
            );
            let client: IAudioClient = match device.Activate(CLSCTX_ALL, None) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Activate failed: {e}");
                    return 1;
                }
            };
            let mut def_period = 0i64;
            let mut min_period = 0i64;
            if let Ok(()) = client.GetDevicePeriod(Some(&mut def_period), Some(&mut min_period)) {
                println!("F3PROBE-MATRIX period default_hns={def_period} min_hns={min_period}");
            }
            if let Ok(mix_ptr) = client.GetMixFormat() {
                if !mix_ptr.is_null() {
                    // WAVEFORMATEX is a packed struct: copy fields out.
                    let (tag, ch, rate, bits, cb) = unsafe {
                        (
                            (*mix_ptr).wFormatTag,
                            (*mix_ptr).nChannels,
                            (*mix_ptr).nSamplesPerSec,
                            (*mix_ptr).wBitsPerSample,
                            (*mix_ptr).cbSize,
                        )
                    };
                    println!(
                        "F3PROBE-MATRIX mix tag={tag:#x} ch={ch} rate={rate} bits={bits} tagext={cb:#x}",
                    );
                    let _ = windows::Win32::System::Com::CoTaskMemFree(Some(mix_ptr.cast()));
                }
            }

            let mut wfx: WAVEFORMATEXTENSIBLE = std::mem::zeroed();
            wfx.Format.wFormatTag = WAVE_FORMAT_EXTENSIBLE_TAG;
            wfx.Format.nChannels = CHANNELS;
            wfx.Format.nSamplesPerSec = SAMPLE_RATE;
            wfx.Format.wBitsPerSample = IEEE_FLOAT_BITS;
            wfx.Format.nBlockAlign = CHANNELS * (IEEE_FLOAT_BYTES as u16);
            wfx.Format.nAvgBytesPerSec = SAMPLE_RATE * u32::from(CHANNELS) * IEEE_FLOAT_BYTES;
            wfx.Format.cbSize = EXTENSIBLE_CB_SIZE;
            wfx.Samples.wValidBitsPerSample = IEEE_FLOAT_BITS;
            wfx.dwChannelMask = 0x3;
            wfx.SubFormat = KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;

            let combos: &[(&str, u32, i64, i64)] = &[
                ("EVENT dur=0 per=0", AUDCLNT_STREAMFLAGS_EVENTCALLBACK, 0, 0),
                ("NONE   dur=0 per=0", 0, 0, 0),
                (
                    "EVENT dur=10ms per=0",
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    100_000,
                    0,
                ),
                (
                    "EVENT dur=0 per=def",
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    0,
                    {
                        let mut d = 0i64;
                        let _ = client.GetDevicePeriod(Some(&mut d), None);
                        d
                    },
                ),
                (
                    "AUTOCONV|EVENT dur=0 per=0",
                    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    0,
                    0,
                ),
            ];
            for (name, flags, dur, per) in combos {
                let r = client.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    *flags,
                    *dur,
                    *per,
                    std::ptr::addr_of!(wfx.Format),
                    None,
                );
                println!("F3PROBE-MATRIX init {name} -> {r:?}");
            }
        }
        0
    }

    pub fn run() -> i32 {
        if std::env::var("F3PROBE_INIT_MATRIX").as_deref() == Ok("1") {
            return run_init_matrix();
        }
        println!(
            "F3PROBE BEGIN rate={SAMPLE_RATE} ch={CHANNELS} edge={EDGE_CAPACITY} tone={SINE_HZ}Hz amplitude={AMPLITUDE}"
        );
        let mut failures = 0usize;

        // ---- Phase A: gate only, including stop-from-paused ----
        {
            let probe = Probe::new();
            let producer = spawn_producer(probe.edge.clone());
            let (leg, ok, _sessions) = run_phase(&probe, Mechanism::GateOnly);

            // Stop FROM parked on the same leg (device continuity: the
            // same session opened at phase start must carry this).
            probe.gate.request_pause();
            assert!(
                wait_engaged(&probe.gate, Duration::from_secs(2)).is_some(),
                "A: re-park for stop-from-paused failed"
            );
            std::thread::sleep(Duration::from_millis(500));
            let stop_cmd = Instant::now();
            probe.edge.stop();
            probe.gate.release_stop();
            let exited = join_within(leg, Duration::from_secs(5)).is_some();
            let exit_ms = stop_cmd.elapsed().as_millis();
            report(format!(
                "STOP_FROM_PARKED mech=A-gate-only exited={exited} exit_ms={exit_ms}"
            ));
            drop(producer);
            failures += usize::from(!ok || !exited || exit_ms > 2000);
        }

        // ---- Phase B: gate + device Stop/Start ----
        {
            let probe = Probe::new();
            let producer = spawn_producer(probe.edge.clone());
            let (leg, ok, _sessions) = run_phase(&probe, Mechanism::GatePlusDeviceStop);
            probe.edge.stop();
            probe.gate.release_stop();
            let exited = join_within(leg, Duration::from_secs(5)).is_some();
            report(format!(
                "STOP_FROM_RUNNING mech=B-gate-plus-device-stop exited={exited}"
            ));
            drop(producer);
            failures += usize::from(!ok || !exited);
        }

        println!("F3PROBE END failures={failures}");
        if failures == 0 {
            0
        } else {
            1
        }
    }
}
