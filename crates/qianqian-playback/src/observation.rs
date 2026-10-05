//! Episode-owned visualization telemetry: post-DSP / pre-PcmEdge.
//! One nonblocking lossy slot, one off-path analyst, one latest snapshot.
//! No sample here establishes playback, device, Position or audibility truth.
//!
//! Applied clears pending PCM and the published snapshot under the slot lock,
//! then arms the existing boundary bit. A single analyst cannot take another
//! block while analyzing: if Applied races its work, that bit is still armed
//! at publication, so the late result is rejected. The next take consumes the
//! bit and resets smoothing. No generation or second cut state is needed.
//! Offer uses try_lock; contention drops. Control-only invalidate/close wait
//! for bounded copy sections, never FFT. Presentation gets owned copies and
//! cannot retain a lock. Worker exit closes and joins after at most one pending
//! analysis; scheduling/lock acquisition have no wall-clock deadline.

use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use qianqian_audio_api::ports::PcmFormat;
use realfft::{RealFftPlanner, RealToComplex, num_complex::Complex32};

pub(crate) const FFT_SIZE: usize = 1024;
/// Number of fixed logarithmic display bands, independent of terminal width.
pub const SPECTRUM_BANDS: usize = 32;
/// Number of mono bucket-average points from one delivered block; no history.
pub const WAVEFORM_POINTS: usize = 64;
/// Display floor in dB relative to a nominal full-scale sine's amplitude.
pub const SPECTRUM_FLOOR_DBFS: f32 = -80.0;
/// Fixed band edges in Hz (40 Hz to 16 kHz). Each band's upper edge is
/// additionally bounded by the snapshot format's Nyquist frequency. Bands
/// wholly above Nyquist stay at the floor; they do not shift with sample rate.
pub const SPECTRUM_BAND_EDGES_HZ: [f32; SPECTRUM_BANDS + 1] = [
    40.0, 48.2363, 58.1686, 70.146, 84.5897, 102.007, 123.012, 148.341, 178.885, 215.719, 260.138,
    313.703, 378.297, 456.191, 550.125, 663.4, 800.0, 964.727, 1163.37, 1402.92, 1691.79, 2040.15,
    2460.23, 2966.82, 3577.71, 4314.39, 5202.76, 6274.05, 7565.93, 9123.82, 11002.5, 13268.0,
    16000.0,
];

/// Linear sample-amplitude measurements for one interleaved channel index.
/// Unity is nominal full scale. Values above unity remain visible (overload);
/// these are sample peak and block RMS, never true peak, LUFS or loudness.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChannelLevel {
    pub peak: f32,
    pub rms: f32,
}

/// Complete, owned visualization telemetry from one delivered post-DSP block.
/// This is neither playback state nor device truth, Position, or proof of
/// audibility. Intervals may be skipped; without new PCM the latest snapshot
/// can remain unchanged, including during bounded producer prefetch on pause.
/// Episode identity is the reader's structural lifetime, not a field here.
#[derive(Clone, Debug, PartialEq)]
pub struct ObservationSnapshot {
    /// Actual tap format evidence. Unknown channel_mask remains unknown;
    /// channel_levels follow interleaved index order, never guessed labels.
    pub format: PcmFormat,
    /// Hann-windowed, coherent-gain-normalized amplitude, max per log band,
    /// clamped to [-80, 0] dBFS for display, with local attack/decay smoothing.
    /// Narrow bands below FFT resolution use interpolated center magnitude.
    /// This is a musical display, not a calibrated spectrum analyzer.
    pub spectrum_dbfs: [f32; SPECTRUM_BANDS],
    /// Unsmoothened peak/RMS over this block, with no full-scale clamp.
    pub channel_levels: Box<[ChannelLevel]>,
    /// Arithmetic channel mean, then 64 temporal bucket averages over this
    /// block. Opposite-phase channels can cancel. Values are not clipped.
    pub waveform: [f32; WAVEFORM_POINTS],
}

impl ObservationSnapshot {
    fn empty(format: PcmFormat) -> Self {
        Self {
            format,
            spectrum_dbfs: [SPECTRUM_FLOOR_DBFS; SPECTRUM_BANDS],
            channel_levels: vec![ChannelLevel::default(); usize::from(format.channels)]
                .into_boxed_slice(),
            waveform: [0.0; WAVEFORM_POINTS],
        }
    }

    // Copy into already-sized publication storage, without allocating.
    fn copy_from(&mut self, other: &Self) {
        self.spectrum_dbfs = other.spectrum_dbfs;
        self.channel_levels.copy_from_slice(&other.channel_levels);
        self.waveform = other.waveform;
    }
}

/// Read-only view of one episode's observation lifetime. Clone to share with
/// presentation readers; obtain a new reader on episode replacement. No PCM,
/// reset, control, lock guard or analyst internals are exposed.
#[derive(Clone)]
pub struct ObservationReader {
    shared: Arc<Shared>,
}

impl ObservationReader {
    /// Latest complete owned copy, or None before publication / after Applied
    /// until a post-cut result arrives. Old publications may be skipped. A
    /// previously returned owned copy cannot be revoked by a later cut.
    pub fn latest(&self) -> Option<ObservationSnapshot> {
        let slot = self.shared.lock_slot();
        slot.available.then(|| slot.snapshot.clone())
    }

    /// Observation close requested (or analyst unavailable). The worker may
    /// finish its one pending block; the final snapshot remains readable.
    /// Closure is telemetry lifetime evidence, never a playback terminal Fact.
    pub fn is_closed(&self) -> bool {
        self.shared.lock_slot().closed
    }
}

struct Slot {
    closed: bool,
    pending: bool,
    after_cut: bool,
    block: Box<[f32]>,
    frames: usize,
    snapshot: ObservationSnapshot,
    available: bool,
    #[cfg(all(test, not(loom)))]
    evidence: ProbeEvidence,
}

// O1 counters are oracle-only: they are not product snapshot semantics.
#[cfg(all(test, not(loom)))]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ProbeEvidence {
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
    pub(crate) delivered_blocks: u64,
    pub(crate) delivered_frames: u64,
    pub(crate) cuts: u64,
    pub(crate) worker_closed: bool,
}

struct Shared {
    format: PcmFormat,
    slot: Mutex<Slot>,
    block_ready: Condvar,
}

#[derive(Clone)]
pub(crate) struct ObservationTap {
    shared: Arc<Shared>,
}

impl ObservationTap {
    pub(crate) fn new(format: PcmFormat, staging_frames: usize) -> Self {
        assert!(format.channels > 0 && format.sample_rate > 0);
        assert!(staging_frames > 0 && staging_frames <= FFT_SIZE);
        Self {
            shared: Arc::new(Shared {
                format,
                slot: Mutex::new(Slot {
                    closed: false,
                    pending: false,
                    after_cut: false,
                    block: vec![0.0; staging_frames * usize::from(format.channels)]
                        .into_boxed_slice(),
                    frames: 0,
                    snapshot: ObservationSnapshot::empty(format),
                    available: false,
                    #[cfg(all(test, not(loom)))]
                    evidence: ProbeEvidence {
                        sample_rate: format.sample_rate,
                        channels: format.channels,
                        delivered_blocks: 0,
                        delivered_frames: 0,
                        cuts: 0,
                        worker_closed: false,
                    },
                }),
                block_ready: Condvar::new(),
            }),
        }
    }

    pub(crate) fn reader(&self) -> ObservationReader {
        ObservationReader {
            shared: Arc::clone(&self.shared),
        }
    }

    /// Bounded copy only; no analysis, allocation or wait for observer locks.
    pub(crate) fn offer(&self, block: &[f32]) {
        let mut slot = match self.shared.slot.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::WouldBlock) => return,
            Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
        };
        if slot.closed {
            return;
        }
        assert!(block.len() <= slot.block.len());
        assert_eq!(block.len() % usize::from(self.shared.format.channels), 0);
        if block.is_empty() {
            return;
        }
        slot.block[..block.len()].copy_from_slice(block);
        slot.frames = block.len() / usize::from(self.shared.format.channels);
        slot.pending = true;
        drop(slot);
        self.shared.block_ready.notify_one();
    }

    /// Reliable control-only Applied boundary. RefusedUnchanged never calls it.
    pub(crate) fn invalidate(&self) {
        let mut slot = self.shared.lock_slot();
        slot.pending = false;
        slot.after_cut = true;
        slot.available = false;
    }

    pub(crate) fn close(&self) {
        self.shared.lock_slot().closed = true;
        self.shared.block_ready.notify_one();
    }

    pub(crate) fn spawn_worker(&self) -> Option<std::thread::JoinHandle<()>> {
        let shared = Arc::clone(&self.shared);
        let worker = std::thread::Builder::new()
            .name("qianqian-observation".into())
            .spawn(move || {
                // Observation failure closes this reader only, never playback.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    analyst_loop(&shared)
                }));
                let mut slot = shared.lock_slot();
                slot.closed = true;
                #[cfg(all(test, not(loom)))]
                {
                    slot.evidence.worker_closed = true;
                }
            })
            .ok();
        if worker.is_none() {
            self.close();
        }
        worker
    }

    #[cfg(all(test, not(loom)))]
    pub(crate) fn latest(&self) -> ProbeEvidence {
        self.shared.lock_slot().evidence
    }

    #[cfg(all(test, not(loom)))]
    pub(crate) fn take_into(&self, dst: &mut Vec<f32>) -> Option<(usize, bool)> {
        self.shared.take(&mut self.shared.lock_slot(), dst)
    }

    #[cfg(all(test, not(loom)))]
    pub(crate) fn run_with_slot_locked<R>(&self, f: impl FnOnce() -> R) -> R {
        let _slot = self.shared.lock_slot();
        f()
    }
}

impl Shared {
    fn lock_slot(&self) -> MutexGuard<'_, Slot> {
        self.slot.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn take(&self, slot: &mut Slot, dst: &mut Vec<f32>) -> Option<(usize, bool)> {
        if !slot.pending {
            return None;
        }
        let frames = slot.frames;
        let after_cut = slot.after_cut;
        dst.clear();
        dst.extend_from_slice(&slot.block[..frames * usize::from(self.format.channels)]);
        slot.pending = false;
        slot.after_cut = false;
        #[cfg(all(test, not(loom)))]
        {
            slot.evidence.delivered_blocks += 1;
            slot.evidence.delivered_frames += frames as u64;
            slot.evidence.cuts += u64::from(after_cut);
        }
        Some((frames, after_cut))
    }

    fn wait_and_take(&self, dst: &mut Vec<f32>) -> Option<(usize, bool)> {
        let mut slot = self
            .block_ready
            .wait_while(self.lock_slot(), |s| !s.closed && !s.pending)
            .unwrap_or_else(|p| p.into_inner());
        self.take(&mut slot, dst)
    }

    fn publish(&self, snapshot: &ObservationSnapshot) {
        let mut slot = self.lock_slot();
        // Only this analyst can consume after_cut, and it has not taken
        // another block since beginning this analysis. An armed bit means
        // Applied raced the in-flight block; its result must be discarded.
        if !slot.after_cut {
            slot.snapshot.copy_from(snapshot);
            slot.available = true;
        }
    }
}

fn analyst_loop(shared: &Shared) {
    let capacity = shared.lock_slot().block.len();
    let mut block = Vec::with_capacity(capacity);
    let mut analysis = Analysis::new(shared.format);
    while let Some((_frames, after_cut)) = shared.wait_and_take(&mut block) {
        if after_cut {
            analysis.reset();
        }
        analysis.analyze(&block);
        shared.publish(&analysis.snapshot);
    }
}

// One private analyst-owned implementation, not a DSP framework. Every block
// is an independent window: loss cannot splice unrelated PCM into an FFT.
struct Analysis {
    fft: Arc<dyn RealToComplex<f32>>,
    input: Vec<f32>,
    output: Vec<Complex32>,
    scratch: Vec<Complex32>,
    window: [f32; FFT_SIZE],
    energy: Box<[f64]>,
    snapshot: ObservationSnapshot,
}

impl Analysis {
    fn new(format: PcmFormat) -> Self {
        let fft = RealFftPlanner::new().plan_fft_forward(FFT_SIZE);
        Self {
            input: fft.make_input_vec(),
            output: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            window: std::array::from_fn(|i| {
                0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / FFT_SIZE as f32).cos()
            }),
            energy: vec![0.0; usize::from(format.channels)].into_boxed_slice(),
            snapshot: ObservationSnapshot::empty(format),
        }
    }

    fn reset(&mut self) {
        self.snapshot.spectrum_dbfs.fill(SPECTRUM_FLOOR_DBFS);
        // No overlap, meter smoothing or waveform history exists. All other
        // outputs and FFT input are overwritten in full on every analysis.
    }

    fn analyze(&mut self, block: &[f32]) {
        let channels = usize::from(self.snapshot.format.channels);
        let frames = block.len() / channels;
        assert!(frames > 0 && frames <= FFT_SIZE);
        self.energy.fill(0.0);
        self.snapshot.channel_levels.fill(ChannelLevel::default());
        self.input.fill(0.0);
        for (i, frame) in block.chunks_exact(channels).enumerate() {
            let mut sum = 0.0f64;
            for (ch, &sample) in frame.iter().enumerate() {
                // Malformed non-finite PCM is ignored locally; observation
                // neither modifies nor fails the playback signal.
                let sample = if sample.is_finite() { sample } else { 0.0 };
                self.snapshot.channel_levels[ch].peak =
                    self.snapshot.channel_levels[ch].peak.max(sample.abs());
                self.energy[ch] += f64::from(sample) * f64::from(sample);
                sum += f64::from(sample);
            }
            self.input[i] = (sum / channels as f64) as f32;
        }
        for (level, energy) in self.snapshot.channel_levels.iter_mut().zip(&self.energy) {
            level.rms = (energy / frames as f64).sqrt() as f32;
        }
        for (p, point) in self.snapshot.waveform.iter_mut().enumerate() {
            let start = p * frames / WAVEFORM_POINTS;
            let end = ((p + 1) * frames / WAVEFORM_POINTS).max(start + 1);
            *point = (self.input[start..end]
                .iter()
                .map(|&v| f64::from(v))
                .sum::<f64>()
                / (end - start) as f64) as f32;
        }
        for (sample, window) in self.input.iter_mut().zip(&self.window) {
            *sample *= window;
        }
        self.fft
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
            .expect("fixed preallocated real FFT buffers");
        let gain: f32 = self.window[..frames].iter().sum();
        let bin_hz = self.snapshot.format.sample_rate as f32 / FFT_SIZE as f32;
        let nyquist = self.snapshot.format.sample_rate as f32 / 2.0;
        for b in 0..SPECTRUM_BANDS {
            let low = SPECTRUM_BAND_EDGES_HZ[b];
            let high = SPECTRUM_BAND_EDGES_HZ[b + 1].min(nyquist);
            let db = if low >= high {
                SPECTRUM_FLOOR_DBFS
            } else {
                let first = (low / bin_hz).ceil() as usize;
                let last = (high / bin_hz).floor() as usize;
                let amplitude = if first <= last {
                    (first..=last)
                        .map(|i| self.amplitude(i, gain))
                        .fold(0.0, f32::max)
                } else {
                    let position = (low * high).sqrt() / bin_hz;
                    let i = position.floor() as usize;
                    let fraction = position.fract();
                    self.amplitude(i, gain) * (1.0 - fraction)
                        + self.amplitude(i + 1, gain) * fraction
                };
                (20.0 * amplitude.max(0.0001).log10()).clamp(SPECTRUM_FLOOR_DBFS, 0.0)
            };
            let old = &mut self.snapshot.spectrum_dbfs[b];
            *old += (db - *old) * if db > *old { 0.65 } else { 0.15 };
        }
    }

    fn amplitude(&self, i: usize, gain: f32) -> f32 {
        let value = self.output[i];
        let magnitude = f64::from(value.re).hypot(f64::from(value.im));
        let factor = if i == 0 || i == FFT_SIZE / 2 {
            1.0
        } else {
            2.0
        };
        let amplitude = (magnitude * factor / f64::from(gain.max(f32::MIN_POSITIVE))) as f32;
        if amplitude.is_nan() { 0.0 } else { amplitude }
    }
}

#[cfg(all(test, not(loom)))]
#[path = "observation_signal_tests.rs"]
mod signal_tests;
