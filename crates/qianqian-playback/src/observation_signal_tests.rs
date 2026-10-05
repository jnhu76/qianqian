//! O2–O7 signal/product, publication, lifecycle and focused cost evidence.
use super::*;
use crate::edge_lifecycle_tests::counting_allocator::run_counting_allocations;
use crate::test_common::{TEST_FORMAT, within};
use std::time::{Duration, Instant};

fn tone(hz: f32, amplitude: f32, format: PcmFormat) -> Vec<f32> {
    (0..FFT_SIZE)
        .flat_map(|i| {
            let sample = amplitude
                * (std::f32::consts::TAU * hz * i as f32 / format.sample_rate as f32).sin();
            std::iter::repeat_n(sample, usize::from(format.channels))
        })
        .collect()
}

fn dominant(spectrum: &[f32; SPECTRUM_BANDS]) -> usize {
    spectrum
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .unwrap()
        .0
}

#[test]
fn silence_tones_and_amplitude_have_useful_display_semantics() {
    let mut analysis = Analysis::new(TEST_FORMAT);
    analysis.analyze(&[0.0; FFT_SIZE * 2]);
    assert_eq!(
        analysis.snapshot.spectrum_dbfs,
        [SPECTRUM_FLOOR_DBFS; SPECTRUM_BANDS]
    );
    let low_hz = TEST_FORMAT.sample_rate as f32 * 24.0 / FFT_SIZE as f32;
    let high_hz = TEST_FORMAT.sample_rate as f32 * 93.0 / FFT_SIZE as f32;
    let mut low = Analysis::new(TEST_FORMAT);
    low.analyze(&tone(low_hz, 0.5, TEST_FORMAT));
    let low_band = dominant(&low.snapshot.spectrum_dbfs);
    assert!(
        SPECTRUM_BAND_EDGES_HZ[low_band] <= low_hz
            && low_hz <= SPECTRUM_BAND_EDGES_HZ[low_band + 1]
    );
    let mut high = Analysis::new(TEST_FORMAT);
    high.analyze(&tone(high_hz, 0.5, TEST_FORMAT));
    let high_band = dominant(&high.snapshot.spectrum_dbfs);
    assert!(
        SPECTRUM_BAND_EDGES_HZ[high_band] <= high_hz
            && high_hz <= SPECTRUM_BAND_EDGES_HZ[high_band + 1]
    );
    assert!(high_band > low_band + 3);
    let mut quiet = Analysis::new(TEST_FORMAT);
    quiet.analyze(&tone(low_hz, 0.125, TEST_FORMAT));
    assert!(low.snapshot.spectrum_dbfs[low_band] > quiet.snapshot.spectrum_dbfs[low_band] + 5.0);
    assert!(low.snapshot.channel_levels[0].rms > quiet.snapshot.channel_levels[0].rms * 3.9);
    // Signal sample peak and sine RMS, rather than FFT internals.
    assert!((low.snapshot.channel_levels[0].peak - 0.5).abs() < 0.001);
    assert!((low.snapshot.channel_levels[0].rms - 0.5 / 2.0f32.sqrt()).abs() < 0.001);

    // A product-relevant low source rate: unsupported display bands floor
    // instead of stretching frequency meanings to a new range.
    let mut low_rate = Analysis::new(PcmFormat {
        sample_rate: 8000,
        ..TEST_FORMAT
    });
    low_rate.analyze(&[0.25; FFT_SIZE * 2]);
    for (band, &edge) in SPECTRUM_BAND_EDGES_HZ[..SPECTRUM_BANDS].iter().enumerate() {
        if edge >= 4000.0 {
            assert_eq!(low_rate.snapshot.spectrum_dbfs[band], SPECTRUM_FLOOR_DBFS);
        }
    }
}

#[test]
fn meters_preserve_channel_order_and_nominal_full_scale_overload() {
    let mut analysis = Analysis::new(TEST_FORMAT);
    let block: Vec<_> = (0..FFT_SIZE)
        .flat_map(|i| [if i % 2 == 0 { -1.0 } else { 1.0 }, 1.5])
        .collect();
    analysis.analyze(&block);
    assert_eq!(
        analysis.snapshot.channel_levels.as_ref(),
        &[
            ChannelLevel {
                peak: 1.0,
                rms: 1.0
            },
            ChannelLevel {
                peak: 1.5,
                rms: 1.5
            },
        ]
    );
    // No meter smoothing; the next block replaces both measurements.
    analysis.analyze(&[0.0; FFT_SIZE * 2]);
    assert!(
        analysis
            .snapshot
            .channel_levels
            .iter()
            .all(|v| *v == ChannelLevel::default())
    );
}

#[test]
fn waveform_is_one_bounded_mono_block_with_honest_cancellation() {
    let mut analysis = Analysis::new(TEST_FORMAT);
    let block: Vec<_> = (0..FFT_SIZE)
        .flat_map(|i| [if i < FFT_SIZE / 2 { -0.5 } else { 0.5 }, 0.0])
        .collect();
    analysis.analyze(&block);
    assert_eq!(analysis.snapshot.waveform.len(), WAVEFORM_POINTS);
    assert!(
        analysis.snapshot.waveform[..WAVEFORM_POINTS / 2]
            .iter()
            .all(|&v| v == -0.25)
    );
    assert!(
        analysis.snapshot.waveform[WAVEFORM_POINTS / 2..]
            .iter()
            .all(|&v| v == 0.25)
    );
    analysis.analyze(&[0.5, -0.5].repeat(FFT_SIZE));
    assert!(analysis.snapshot.waveform.iter().all(|&v| v == 0.0));
    // Short final block has the same bounded shape, with no stale tail.
    analysis.analyze(&[0.25, 0.25]);
    assert!(analysis.snapshot.waveform.iter().all(|&v| v == 0.25));
}

#[test]
fn applied_rejects_inflight_precut_publication_and_resets_all_signal_outputs() {
    within(Duration::from_secs(5), || {
        let tap = ObservationTap::new(TEST_FORMAT, FFT_SIZE);
        let reader = tap.reader();
        let mut seed = Analysis::new(TEST_FORMAT);
        seed.analyze(&tone(1000.0, 0.75, TEST_FORMAT));
        tap.shared.publish(&seed.snapshot);
        assert!(reader.latest().is_some());
        tap.offer(&tone(1000.0, 0.5, TEST_FORMAT));
        let (computed_tx, computed_rx) = std::sync::mpsc::sync_channel(1);
        let (resume_tx, resume_rx) = std::sync::mpsc::sync_channel(1);
        let shared = tap.shared.clone();
        let analyst = std::thread::spawn(move || {
            let mut block = Vec::with_capacity(FFT_SIZE * 2);
            let mut analysis = seed;
            let (_, after_cut) = shared.wait_and_take(&mut block).unwrap();
            assert!(!after_cut);
            analysis.analyze(&block);
            computed_tx.send(()).unwrap();
            resume_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            shared.publish(&analysis.snapshot);
            assert!(
                !shared.lock_slot().available,
                "late pre-cut work was published after Applied"
            );
            // A post-cut silent block must equal a fresh silent observation,
            // including no spectrum decay tail, old meters or waveform.
            let (_, after_cut) = shared.wait_and_take(&mut block).unwrap();
            assert!(after_cut);
            analysis.reset();
            analysis.analyze(&block);
            shared.publish(&analysis.snapshot);
        });
        computed_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        tap.invalidate();
        assert!(
            reader.latest().is_none(),
            "Applied withdraws the already-published old result"
        );
        tap.offer(&[0.0; FFT_SIZE * 2]);
        resume_tx.send(()).unwrap();
        analyst.join().unwrap();
        let snapshot = reader.latest().unwrap();
        assert_eq!(
            snapshot.spectrum_dbfs,
            [SPECTRUM_FLOOR_DBFS; SPECTRUM_BANDS]
        );
        assert_eq!(snapshot.waveform, [0.0; WAVEFORM_POINTS]);
        assert!(
            snapshot
                .channel_levels
                .iter()
                .all(|v| *v == ChannelLevel::default())
        );
        // Previously copied values are immutable owned data, not revoked
        // references; the fresh read above is the cut-valid result.
    });
}

#[test]
fn reader_is_coherent_latest_value_and_episode_local() {
    use crate::handle::PlaybackSessionHandle;
    let old_handle = PlaybackSessionHandle::new();
    let old = ObservationTap::new(TEST_FORMAT, FFT_SIZE);
    old_handle.completion.bind_observation_reader(old.reader());
    let reader = old_handle.observation_reader().unwrap();
    let mut analysis = Analysis::new(TEST_FORMAT);
    for value in [0.25, 0.5] {
        analysis.analyze(&[value; FFT_SIZE * 2]);
        old.shared.publish(&analysis.snapshot);
    }
    let snapshot = reader.latest().unwrap();
    assert_eq!(snapshot.waveform, [0.5; WAVEFORM_POINTS]);
    assert!(snapshot.channel_levels.iter().all(|v| *v
        == ChannelLevel {
            peak: 0.5,
            rms: 0.5
        }));
    old.close();
    assert!(reader.is_closed());
    assert!(
        reader.latest().is_some(),
        "closed reader retains final telemetry"
    );
    let new_handle = PlaybackSessionHandle::new();
    assert!(new_handle.observation_reader().is_none());
    let new = ObservationTap::new(TEST_FORMAT, FFT_SIZE);
    new_handle.completion.bind_observation_reader(new.reader());
    assert!(new_handle.observation_reader().unwrap().latest().is_none());
    assert!(!new_handle.observation_reader().unwrap().is_closed());
}

#[test]
fn concurrent_presentation_copies_cannot_tear_snapshots_or_retain_producer_locks() {
    within(Duration::from_secs(5), || {
        let tap = ObservationTap::new(TEST_FORMAT, FFT_SIZE);
        let reader = tap.reader();
        let presenter = std::thread::spawn(move || {
            while !reader.is_closed() {
                if let Some(snapshot) = reader.latest() {
                    let value = snapshot.waveform[0];
                    assert!(snapshot.waveform.iter().all(|&v| v == value));
                    assert!(
                        snapshot
                            .channel_levels
                            .iter()
                            .all(|v| v.peak == value && v.rms == value)
                    );
                    // Sleeping while owning a snapshot must not retain a lock.
                    std::thread::sleep(Duration::from_micros(50));
                }
            }
        });
        let analyst = tap.spawn_worker().unwrap();
        for i in 1..=2000 {
            tap.offer(&[i as f32 / 2000.0; FFT_SIZE * 2]);
        }
        tap.close();
        analyst.join().unwrap();
        presenter.join().unwrap();
        assert!(tap.latest().worker_closed);
    });
}

/// Focused repeatable O6 evidence. Run with --release --nocapture for cost;
/// timing is descriptive, allocations are asserted, no CI speed threshold.
#[test]
fn observation_cost_and_steady_state_allocations() {
    const RUNS: usize = 2000;
    fn measure(mut f: impl FnMut()) -> (Duration, Duration, usize) {
        for _ in 0..20 {
            f();
        }
        let mut times = Vec::with_capacity(RUNS);
        let (_, allocations) = run_counting_allocations(|| {
            for _ in 0..RUNS {
                let start = Instant::now();
                f();
                times.push(start.elapsed());
            }
        });
        times.sort_unstable();
        (times[RUNS / 2], times[RUNS * 99 / 100], allocations)
    }
    for rate in [44100, 48000] {
        let format = PcmFormat {
            sample_rate: rate,
            ..TEST_FORMAT
        };
        let mut analysis = Analysis::new(format);
        let pcm = tone(1000.0, 0.5, format);
        let tap = ObservationTap::new(format, FFT_SIZE);
        let mut dst = Vec::with_capacity(FFT_SIZE * 2);
        let analyze = measure(|| analysis.analyze(&pcm));
        let publish = measure(|| tap.shared.publish(&analysis.snapshot));
        let offer = measure(|| tap.offer(&pcm));
        let drop_offer = tap.run_with_slot_locked(|| measure(|| tap.offer(&pcm)));
        let take = measure(|| {
            tap.offer(&pcm);
            tap.take_into(&mut dst);
        });
        let read = measure(|| {
            std::hint::black_box(tap.reader().latest());
        });
        assert_eq!(analyze.2 + publish.2 + offer.2 + drop_offer.2 + take.2, 0);
        assert_eq!(
            read.2, RUNS,
            "owned presentation copy allocates one channel array per read"
        );
        let slot = tap.shared.lock_slot();
        let dynamic_bytes = slot.block.len() * 4
            + dst.capacity() * 4
            + analysis.input.capacity() * 4
            + analysis.output.capacity() * 8
            + analysis.scratch.capacity() * 8
            + analysis.energy.len() * 8
            + analysis.snapshot.channel_levels.len() * 8
            + slot.snapshot.channel_levels.len() * 8;
        println!(
            "O6 {rate}Hz stereo, median/p99/allocs over {RUNS}: analysis={analyze:?}, publish={publish:?}, offer={offer:?}, contended_drop={drop_offer:?}, offer+take={take:?}, reader={read:?}; dynamic_buffers={dynamic_bytes}B; Analysis_inline={}B; Shared_inline={}B; cadence={:.2} blocks/s; FFT_scratch={} complex",
            std::mem::size_of::<Analysis>(),
            std::mem::size_of::<Shared>(),
            rate as f64 / FFT_SIZE as f64,
            analysis.scratch.len()
        );
    }
    // Small window comparison only; production remains 1024, no accumulation.
    for n in [1024, 2048] {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(n);
        let mut input = fft.make_input_vec();
        let mut output = fft.make_output_vec();
        let mut scratch = fft.make_scratch_vec();
        let result = measure(|| {
            input.fill(0.25);
            fft.process_with_scratch(&mut input, &mut output, &mut scratch)
                .unwrap();
        });
        assert_eq!(result.2, 0);
        println!(
            "FFT comparison n={n}: median/p99/allocs={result:?}; 44.1k resolution={:.2}Hz duration={:.2}ms; 48k resolution={:.2}Hz duration={:.2}ms",
            44100.0 / n as f64,
            n as f64 / 44.1,
            48000.0 / n as f64,
            n as f64 / 48.0
        );
    }
}
