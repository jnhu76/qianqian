//! F4-GATE physical probe: the position-evidence shape against the real
//! WASAPI shared-mode event-driven device (Windows only).
//!
//! The probe mirrors the production render-loop order (gate → wait →
//! padding → GetBuffer → read → release, `qianqian-output-wasapi`
//! steady_loop) with the two evidence cells the F4 gate proposes:
//!
//! ```text
//! submitted   counted at the edge handoff site (read_frames return),
//!             once per block — where the session-owned edge would
//!             publish it in production
//! tail        published from every GetCurrentPadding observation:
//!             steady loop, park slices (≤10 ms), and the EOF drain
//!             loop — the same reading D14.7 already trusts for
//!             output-tail quiescence
//! clock       Experiment B: IAudioClock position, sampled on the
//!             render leg only (no cross-thread COM calls)
//! ```
//!
//! Experiments (printed as F4PROBE lines, enforced invariants set the
//! exit code):
//!
//! ```text
//! A  submitted / padding / derived consumed across steady → pause
//!    request → engagement → tail drain → Paused establishment →
//!    resume → EOF → drain: monotonicity of submitted and of the
//!    clamped projection, the one-block torn-read bound, tail ≤
//!    submitted, the projection constant through the quiesced park,
//!    consumed == exact submitted total at drain completion.
//! B  IAudioClock comparison: frequency + position at 48 kHz, and the
//!    decisive unit leg — a 44.1 kHz AUTOCONVERTPCM stream whose clock
//!    frequency is compared against its own stream rate. The clock is
//!    selected only if it adds source-relative truth the padding
//!    algebra lacks.
//! ```
//!
//! The probe plays a quiet 440 Hz tone (~12 s total). It makes no
//! audibility claim: what the numbers MEAN is the gate document's
//! decision, and whether the pause sounds right stays the human
//! reviewer's acceptance (F3 discipline).

#[cfg(not(windows))]
fn main() {
    eprintln!("f4probe: no physical probe on this platform (see src/timeline.rs oracles)");
}

#[cfg(windows)]
fn main() {
    std::process::exit(win::run());
}

#[cfg(windows)]
mod win {
    use std::slice;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use windows::core::GUID;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IAudioClient, IAudioClock, IAudioRenderClient, IMMDeviceEnumerator,
        MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
        AUDCLNT_STREAMFLAGS_EVENTCALLBACK, WAVEFORMATEXTENSIBLE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

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
    const PARK_SLICE_MS: u64 = 10;

    const SAMPLE_RATE: u32 = 48000;
    const CHANNELS: u16 = 2;
    const SINE_HZ: f32 = 440.0;
    const AMPLITUDE: f32 = 0.12;
    const EDGE_CAPACITY: usize = 8192;
    const CHUNK: u64 = 1024;
    /// 8 s of source audio: enough for 2 s steady + pause park 1.5 s +
    /// resume 1 s + EOF drain, with margin.
    const TOTAL_FRAMES: u64 = 48_000 * 8;

    // Phase timeline (ms from phase start), matching the driver sleeps.
    const T_PAUSE_CMD: u64 = 2000;
    const T_RESUME_CMD: u64 = T_PAUSE_CMD + 1500;
    /// Inside the park: engagement lands within ~2 loop periods, the
    /// measured tail drain (F3: 28–30 ms) is well under 300 ms, so the
    /// window below is quiesced-park by construction.
    const T_FROZEN_LO: u64 = T_PAUSE_CMD + 400;
    const T_FROZEN_HI: u64 = T_RESUME_CMD - 50;

    // ---- the proposed evidence cells (session-owned in production) ----

    #[derive(Default)]
    struct Evidence {
        submitted: AtomicU64,
        tail: AtomicU64,
        tail_published: AtomicBool,
        clock_pos: AtomicU64,
    }

    impl Evidence {
        fn submit(&self, n: u64) {
            self.submitted.fetch_add(n, Ordering::Relaxed);
        }
        fn observe_tail(&self, p: u32) {
            self.tail.store(u64::from(p), Ordering::Relaxed);
            self.tail_published.store(true, Ordering::Relaxed);
        }
        /// The gate's raw derivation (`src/timeline.rs` is the
        /// normative algebra; this is the probe's live copy).
        fn raw(&self) -> Option<u64> {
            if !self.tail_published.load(Ordering::Relaxed) {
                return None;
            }
            let submitted = self.submitted.load(Ordering::Relaxed);
            let tail = self.tail.load(Ordering::Relaxed);
            Some(submitted.saturating_sub(tail.min(submitted)))
        }
    }

    // ---- loop-top pause gate (mechanism A shape, probe-local) ----

    struct Gate {
        paused: AtomicBool,
    }

    impl Gate {
        fn new() -> Self {
            Self {
                paused: AtomicBool::new(false),
            }
        }
        fn request_pause(&self) {
            self.paused.store(true, Ordering::Relaxed);
        }
        fn request_resume(&self) {
            self.paused.store(false, Ordering::Relaxed);
        }
        /// Loop-top park, strictly before GetBuffer; publishes the tail
        /// from park slices so the projection keeps moving truthfully
        /// while the device drains, exactly as the production gate's
        /// tail-quiescence closure does.
        fn park_while_paused(&self, ev: &Evidence, observe: impl FnMut() -> Option<u32>) {
            if !self.paused.load(Ordering::Relaxed) {
                return;
            }
            let mut observe = observe;
            while self.paused.load(Ordering::Relaxed) {
                if let Some(p) = observe() {
                    ev.observe_tail(p);
                }
                std::thread::sleep(Duration::from_millis(PARK_SLICE_MS));
            }
        }
    }

    // ---- finite sine producer (stands in for the decode worker) ----

    fn spawn_producer(edge: Arc<ProbeEdge>, total_frames: u64) -> JoinHandle<()> {
        std::thread::Builder::new()
            .name("f4-producer".into())
            .spawn(move || {
                let channels = usize::from(CHANNELS);
                let mut phase = 0.0f32;
                let step = 2.0 * std::f32::consts::PI * SINE_HZ / SAMPLE_RATE as f32;
                let mut staging = vec![0.0f32; CHUNK as usize * channels];
                let mut written = 0u64;
                while written < total_frames {
                    let n = ((total_frames - written) as usize).min(CHUNK as usize);
                    for f in staging[..n * channels].chunks_mut(channels) {
                        let s = AMPLITUDE * phase.sin();
                        for smp in f {
                            *smp = s;
                        }
                        phase += step;
                    }
                    if edge.write(&staging[..n * channels]) == WriteOutcome::Stopped {
                        return;
                    }
                    written += n as u64;
                }
                edge.close_eof();
            })
            .expect("producer spawn")
    }

    // ---- probe edge (production PcmEdge shape, minimal) ----

    struct ProbeEdge {
        channels: usize,
        capacity_samples: usize,
        buf: Mutex<EdgeBuf>,
        data_ready: Condvar,
    }

    struct EdgeBuf {
        samples: Vec<f32>,
        read_pos: usize,
        buffered: usize,
        eof: bool,
        stopped: bool,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum WriteOutcome {
        Written,
        Stopped,
    }

    enum Pull {
        Frames(usize),
        Eof,
        Stopped,
    }

    impl ProbeEdge {
        fn new() -> Self {
            Self {
                channels: usize::from(CHANNELS),
                capacity_samples: EDGE_CAPACITY * usize::from(CHANNELS),
                buf: Mutex::new(EdgeBuf {
                    samples: vec![0.0; EDGE_CAPACITY * usize::from(CHANNELS)],
                    read_pos: 0,
                    buffered: 0,
                    eof: false,
                    stopped: false,
                }),
                data_ready: Condvar::new(),
            }
        }
        /// Blocking producer write: the whole slice is accepted or the
        /// edge is stopped — the production edge's contract.
        fn write(&self, src: &[f32]) -> WriteOutcome {
            let mut offset = 0usize;
            let mut guard = self.buf.lock().expect("edge lock");
            loop {
                if guard.stopped {
                    return WriteOutcome::Stopped;
                }
                if offset == src.len() {
                    return WriteOutcome::Written;
                }
                let free = self.capacity_samples - guard.buffered;
                if free == 0 {
                    guard = self.data_ready.wait(guard).expect("edge lock");
                    continue;
                }
                let take = free.min(src.len() - offset);
                for i in 0..take {
                    let pos = (guard.read_pos + guard.buffered + i) % self.capacity_samples;
                    guard.samples[pos] = src[offset + i];
                }
                guard.buffered += take;
                offset += take;
                drop(guard);
                self.data_ready.notify_all();
                guard = self.buf.lock().expect("edge lock");
            }
        }
        /// Blocking consumer read: at least one frame, or a terminal.
        fn read_frames(&self, dst: &mut [f32]) -> Pull {
            let mut guard = self.buf.lock().expect("edge lock");
            loop {
                if guard.stopped {
                    return Pull::Stopped;
                }
                let frames = guard.buffered / self.channels;
                if frames > 0 {
                    let want = (dst.len() / self.channels).min(frames);
                    let take = want * self.channels;
                    for i in 0..take {
                        dst[i] = guard.samples[(guard.read_pos + i) % self.capacity_samples];
                    }
                    guard.read_pos = (guard.read_pos + take) % self.capacity_samples;
                    guard.buffered -= take;
                    drop(guard);
                    self.data_ready.notify_all();
                    return Pull::Frames(want);
                }
                if guard.eof {
                    return Pull::Eof;
                }
                guard = self.data_ready.wait(guard).expect("edge lock");
            }
        }
        fn close_eof(&self) {
            self.buf.lock().expect("edge lock").eof = true;
            self.data_ready.notify_all();
        }
        fn stop(&self) {
            self.buf.lock().expect("edge lock").stopped = true;
            self.data_ready.notify_all();
        }
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

    fn open_session_inner(
        sample_rate: u32,
        autoconvert: bool,
    ) -> Result<(DeviceSession, Option<IAudioClock>, u64), String> {
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
            wfx.Format.nSamplesPerSec = sample_rate;
            wfx.Format.wBitsPerSample = IEEE_FLOAT_BITS;
            wfx.Format.nBlockAlign = CHANNELS * (IEEE_FLOAT_BYTES as u16);
            wfx.Format.nAvgBytesPerSec = sample_rate * u32::from(CHANNELS) * IEEE_FLOAT_BYTES;
            wfx.Format.cbSize = EXTENSIBLE_CB_SIZE;
            wfx.Samples.wValidBitsPerSample = IEEE_FLOAT_BITS;
            wfx.dwChannelMask = 0x3;
            wfx.SubFormat = KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;

            let mut flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
            if autoconvert {
                flags |= AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM;
            }
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    flags,
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
            // Experiment B: the clock lives on the render leg; all its
            // calls stay on the leg (no cross-thread COM calls).
            let clock: Option<IAudioClock> = client.GetService().ok();
            let clock_freq = clock
                .as_ref()
                .and_then(|c| c.GetFrequency().ok())
                .unwrap_or(0);
            Ok((
                DeviceSession {
                    render,
                    client,
                    event,
                    buffer_frames,
                },
                clock,
                clock_freq,
            ))
        }
    }

    // ---- the instrumented render leg (production loop order) ----

    fn run_render_leg(
        edge: Arc<ProbeEdge>,
        ev: Arc<Evidence>,
        gate: Arc<Gate>,
        sample_rate: u32,
        autoconvert: bool,
        sample_clock: bool,
        diagnostic: std::sync::mpsc::Sender<String>,
    ) {
        unsafe {
            let coinit = CoInitializeEx(None, COINIT_MULTITHREADED);
            if coinit.is_err() {
                let _ = diagnostic.send(format!("CoInitializeEx failed: {coinit:?}"));
                return;
            }
        }
        let _apartment = CoApartmentGuard;
        let (session, clock, clock_freq) = match open_session_inner(sample_rate, autoconvert) {
            Ok(v) => v,
            Err(e) => {
                let _ = diagnostic.send(e);
                return;
            }
        };
        report(format!(
            "OPEN rate={sample_rate} autoconvert={autoconvert} buffer_frames={} clock_freq={clock_freq} clock_acquired={}",
            session.buffer_frames,
            clock.is_some()
        ));

        // Production steady_loop shape: Start before the first period.
        if let Err(e) = unsafe { session.client.Start() } {
            let _ = diagnostic.send(format!("stream start failed: {e}"));
            return;
        }

        let channels = usize::from(CHANNELS);
        let message = loop {
            // Gate at loop top, before any device call this iteration.
            gate.park_while_paused(&ev, || unsafe { session.client.GetCurrentPadding() }.ok());

            unsafe { WaitForSingleObject(session.event.0, EVENT_TIMEOUT_MS) };
            let padding = match unsafe { session.client.GetCurrentPadding() } {
                Ok(p) => p,
                Err(e) => break format!("GetCurrentPadding failed: {e}"),
            };
            ev.observe_tail(padding);
            let available = session.buffer_frames.saturating_sub(padding) as usize;
            if available == 0 {
                continue;
            }
            let ptr = match unsafe { session.render.GetBuffer(available as u32) } {
                Ok(p) => p,
                Err(e) => break format!("GetBuffer failed: {e}"),
            };
            let dst = unsafe { slice::from_raw_parts_mut(ptr as *mut f32, available * channels) };
            match edge.read_frames(dst) {
                Pull::Frames(n) => {
                    // The handoff site: where the session-owned edge
                    // would publish its monotone read total.
                    ev.submit(n as u64);
                    if let Err(e) = unsafe { session.render.ReleaseBuffer(n as u32, 0) } {
                        break format!("ReleaseBuffer failed: {e}");
                    }
                }
                Pull::Eof => {
                    let _ = unsafe { session.render.ReleaseBuffer(0, 0) };
                    break drain_to_zero(&session, &ev);
                }
                Pull::Stopped => {
                    let _ = unsafe { session.render.ReleaseBuffer(0, 0) };
                    break String::new();
                }
            }
            // Experiment B sampling stays on the leg.
            if sample_clock {
                if let Some(clock) = &clock {
                    let mut pos = 0u64;
                    if unsafe { clock.GetPosition(&mut pos, None) }.is_ok() {
                        ev.clock_pos.store(pos, Ordering::Relaxed);
                    }
                }
            }
        };
        drop(session);
        let _ = diagnostic.send(message);
    }

    /// Production drain shape: wait for padding to reach zero, bounded,
    /// publishing the tail from every observation (a frozen cell must
    /// not go stale for want of a published final 0).
    fn drain_to_zero(session: &DeviceSession, ev: &Evidence) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match unsafe { session.client.GetCurrentPadding() } {
                Ok(0) => {
                    ev.observe_tail(0);
                    return String::new();
                }
                Ok(p) => ev.observe_tail(p),
                Err(e) => return format!("drain padding check failed: {e}"),
            }
            if Instant::now() > deadline {
                return "drain deadline passed".to_owned();
            }
            unsafe { WaitForSingleObject(session.event.0, EVENT_TIMEOUT_MS) };
        }
    }

    // ---- reader / analyst thread ----

    #[derive(Clone, Copy)]
    struct Sample {
        at_ms: u64,
        submitted: u64,
        tail: u64,
        clamped: u64,
        clock: u64,
    }

    struct Analysis {
        samples: Vec<Sample>,
        submitted_monotone: bool,
        clamped_monotone: bool,
        max_raw_backward: u64,
        tail_exceeded_submitted: bool,
    }

    fn spawn_reader(
        ev: Arc<Evidence>,
        stop: Arc<AtomicBool>,
        start: Instant,
    ) -> JoinHandle<Analysis> {
        std::thread::Builder::new()
            .name("f4-reader".into())
            .spawn(move || {
                let mut samples = Vec::new();
                let mut last_clamped: Option<u64> = None;
                let mut last_raw: Option<u64> = None;
                let mut last_submitted = 0u64;
                let mut submitted_monotone = true;
                let mut clamped_monotone = true;
                let mut max_raw_backward = 0u64;
                let mut tail_exceeded_submitted = false;
                while !stop.load(Ordering::Relaxed) {
                    if let Some(raw) = ev.raw() {
                        let submitted = ev.submitted.load(Ordering::Relaxed);
                        let tail = ev.tail.load(Ordering::Relaxed);
                        if submitted < last_submitted {
                            submitted_monotone = false;
                        }
                        last_submitted = submitted;
                        if tail > submitted {
                            tail_exceeded_submitted = true;
                        }
                        if let Some(prev) = last_raw {
                            max_raw_backward = max_raw_backward.max(prev.saturating_sub(raw));
                        }
                        last_raw = Some(raw);
                        let clamped = last_clamped.map_or(raw, |p: u64| p.max(raw));
                        if let Some(prev) = last_clamped {
                            if clamped < prev {
                                clamped_monotone = false;
                            }
                        }
                        last_clamped = Some(clamped);
                        samples.push(Sample {
                            at_ms: start.elapsed().as_millis() as u64,
                            submitted,
                            tail,
                            clamped,
                            clock: ev.clock_pos.load(Ordering::Relaxed),
                        });
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                Analysis {
                    samples,
                    submitted_monotone,
                    clamped_monotone,
                    max_raw_backward,
                    tail_exceeded_submitted,
                }
            })
            .expect("reader spawn")
    }

    fn report(msg: impl std::fmt::Display) {
        println!("F4PROBE {msg}");
    }

    // ---- Experiment A phase driver ----

    fn run_phase_48k() -> usize {
        let edge = Arc::new(ProbeEdge::new());
        let ev = Arc::new(Evidence::default());
        let gate = Arc::new(Gate::new());
        let reader_stop = Arc::new(AtomicBool::new(false));
        let start = Instant::now();

        let producer = spawn_producer(edge.clone(), TOTAL_FRAMES);
        let (diag_tx, diag_rx) = std::sync::mpsc::channel::<String>();
        let leg_ev = ev.clone();
        let leg_edge = edge.clone();
        let leg_gate = gate.clone();
        let leg = std::thread::Builder::new()
            .name("f4-render".into())
            .spawn(move || {
                run_render_leg(
                    leg_edge,
                    leg_ev,
                    leg_gate,
                    SAMPLE_RATE,
                    false,
                    true,
                    diag_tx,
                )
            })
            .expect("render leg spawn");
        let reader = spawn_reader(ev.clone(), reader_stop.clone(), start);

        // Steady playback.
        std::thread::sleep(Duration::from_millis(T_PAUSE_CMD));
        let steady_submitted = ev.submitted.load(Ordering::Relaxed);
        report(format!(
            "STEADY submitted={steady_submitted} raw={:?} expected_floor={}",
            ev.raw(),
            T_PAUSE_CMD * u64::from(SAMPLE_RATE) / 1000 - 2 * CHUNK,
        ));

        // ---- pause: request → engage → tail drain → establishment ----
        gate.request_pause();
        shared_sleep(T_RESUME_CMD - T_PAUSE_CMD);
        let submitted_at_park_end = ev.submitted.load(Ordering::Relaxed);
        report(format!(
            "PAUSE submitted_at_park_end={submitted_at_park_end} (steady was {steady_submitted}, advance must be one in-flight block at most)"
        ));

        // ---- resume ----
        gate.request_resume();
        shared_sleep(1000);
        let submitted_at_resume_end = ev.submitted.load(Ordering::Relaxed);
        report(format!(
            "RESUME submitted_at_resume_end={submitted_at_resume_end}"
        ));

        // ---- EOF: producer already wrote everything; wait for drain ----
        producer.join().expect("producer join");
        let drained_to_total = {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let submitted = ev.submitted.load(Ordering::Relaxed);
                if ev.raw() == Some(submitted)
                    && submitted >= TOTAL_FRAMES
                    && ev.tail.load(Ordering::Relaxed) == 0
                {
                    break true;
                }
                if Instant::now() > deadline {
                    break false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        std::thread::sleep(Duration::from_millis(200));
        reader_stop.store(true, Ordering::Relaxed);
        edge.stop();
        gate.request_resume();
        let analysis = reader.join().expect("reader join");
        let _ = leg.join();
        let diagnostic = diag_rx.try_recv().unwrap_or_default();
        let final_submitted = ev.submitted.load(Ordering::Relaxed);
        let final_raw = ev.raw();

        report(format!(
            "EOF drained_to_total={drained_to_total} final_submitted={final_submitted} expected={TOTAL_FRAMES} final_raw={final_raw:?} diag=\"{diagnostic}\""
        ));
        report(format!(
            "BOUNDS submitted_monotone={} clamped_monotone={} max_raw_backward={} (bound {CHUNK}) tail_le_submitted={}",
            analysis.submitted_monotone,
            analysis.clamped_monotone,
            analysis.max_raw_backward,
            !analysis.tail_exceeded_submitted,
        ));

        // Frozen-window check: through the quiesced park the clamped
        // projection must be exactly constant (submitted frozen, tail 0).
        let frozen: Vec<u64> = analysis
            .samples
            .iter()
            .filter(|s| s.at_ms >= T_FROZEN_LO && s.at_ms <= T_FROZEN_HI)
            .map(|s| s.clamped)
            .collect();
        let frozen_constant = frozen.len() >= 100 && frozen.windows(2).all(|w| w[0] == w[1]);
        report(format!(
            "FREEZE_AT_QUIESCENCE window=[{T_FROZEN_LO},{T_FROZEN_HI}]ms samples={} first={:?} last={:?} constant={frozen_constant}",
            frozen.len(),
            frozen.first(),
            frozen.last(),
        ));

        // Experiment B, 48k leg: clock position vs derived consumed
        // across the whole phase (steady + park + resume + drain). The
        // park is the discriminator: consumed freezes, the engine keeps
        // processing silence, so a diverging clock documents that it
        // counts the device timeline, not this episode's source.
        let clock_pairs: Vec<(u64, u64)> = analysis
            .samples
            .iter()
            .filter(|s| s.clock > 0)
            .map(|s| (s.clock, s.clamped))
            .collect();
        if let (Some(first), Some(last)) = (clock_pairs.first(), clock_pairs.last()) {
            let d_clock = last.0.saturating_sub(first.0);
            let d_raw = last.1.saturating_sub(first.1);
            report(format!(
                "CLOCK_48K samples={} clock_first={} clock_last={} delta_clock={d_clock} delta_consumed={d_raw} drift={}",
                clock_pairs.len(),
                first.0,
                last.0,
                d_clock as i64 - d_raw as i64
            ));
        } else {
            report("CLOCK_48K samples=0 (clock sampling produced nothing)");
        }

        usize::from(
            !analysis.submitted_monotone
                || !analysis.clamped_monotone
                || analysis.max_raw_backward > CHUNK
                || analysis.tail_exceeded_submitted
                || !drained_to_total
                || final_submitted != TOTAL_FRAMES
                || final_raw != Some(TOTAL_FRAMES)
                || !frozen_constant,
        )
    }

    fn shared_sleep(ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }

    /// Experiment B, decisive leg: a 44.1 kHz AUTOCONVERTPCM stream on
    /// the same (48 kHz-mix) endpoint. If the stream's own IAudioClock
    /// frequency reports the MIX rate instead of the stream rate, the
    /// device clock is not source-relative and needs mix-format
    /// knowledge the padding algebra never needs — the measured reason
    /// it stays unselected.
    fn run_clock_unit_leg() -> usize {
        unsafe {
            let coinit = CoInitializeEx(None, COINIT_MULTITHREADED);
            if coinit.is_err() {
                eprintln!("F4PROBE CLOCK_44K CoInitializeEx failed: {coinit:?}");
                return 1;
            }
        }
        let _apartment = CoApartmentGuard;
        match open_session_inner(44100, true) {
            Ok((_session, clock, freq)) => {
                report(format!(
                    "CLOCK_44K stream_rate=44100 autoconvert=true clock_freq={freq} source_relative={}",
                    freq == 44100
                ));
                usize::from(clock.is_none())
            }
            Err(e) => {
                report(format!("CLOCK_44K OPEN_FAILED {e}"));
                1
            }
        }
    }

    pub fn run() -> i32 {
        println!(
            "F4PROBE BEGIN rate={SAMPLE_RATE} ch={CHANNELS} edge={EDGE_CAPACITY} block={CHUNK} tone={SINE_HZ}Hz amplitude={AMPLITUDE} total_frames={TOTAL_FRAMES}"
        );
        let mut failures = run_phase_48k();
        failures += run_clock_unit_leg();
        println!("F4PROBE END failures={failures}");
        if failures == 0 {
            0
        } else {
            1
        }
    }
}
