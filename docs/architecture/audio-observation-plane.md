# Qianqian Audio Observation Plane

> **Truth class: EVIDENCE / implemented observation contract.**
>
> **Status: IMPLEMENTED — O0–O7 candidate completion, owner merge pending.**
> [#187](https://github.com/jnhu76/qianqian/issues/187) owns this slice;
> O0/O1 merged in [#223](https://github.com/jnhu76/qianqian/pull/223).
> This reconciles the original draft with the implemented product surface.
> It changes no playback/DSP/Output authority. #187 remains open until owner
> merge and the SHA-bound closure record. [#188](https://github.com/jnhu76/qianqian/issues/188)
> and a future GUI consume the same read-side API; #188 implementation is outside this work.

## 1. Boundary and ownership

Playback foundations remain with [PBK-001](../adr/ADR-PBK-001.md);
Plugin admission, episode/seek/pause/Position/processing ownership with
[PBK-002](../adr/ADR-PBK-002.md) D11/D13/D14; Output/backend obligations with
[PBK-003](../adr/ADR-PBK-003.md); DSP product meaning with the
[DSP product model](dsp-product-model.md). The
[execution model](playback-execution-model.md) and
[temporal semantics](playback-temporal-semantics.md) explain existing ordering.
This page records observation parameters and implementation evidence only.

```text
decode → DSP → post-DSP staging → PcmEdge → Output
                    │ try_lock; contention drops
                    ↓
             one lossy latest slot
                    ↓
             one analyst worker
            spectrum / peak-RMS / waveform
                    ↓
             one latest snapshot
                    ↓
             read-only presentation reader
```

The tap is **post-DSP / pre-PcmEdge**: immediately after `processing.stage`
succeeds, before the existing edge write. Observation neither modifies PCM
nor changes edge admission. A preserved processed remainder is not re-offered.
The episode's decode worker creates/owns the tap and closes/joins the analyst
at its existing single exit funnel. Replacement gets a fresh worker, tap,
analysis state and reader; no episode ID, generation or global registry exists.

This is the **Qianqian application post-processing PCM view**. It reflects
processing applied to those samples, upstream of Output Volume and host
adaptation. It is not device-consumed PCM, D14.8 Position, device volume,
physical/acoustic output, playback state or proof of audibility. Offered PCM
need not ultimately enter the edge. No observed activity, silence, absence or
closure establishes playback truth.

## 2. Transfer, cadence and publication

Storage is one whole staging block (1024 frames × episode channels), with
preallocated producer slot and analyst copy-out buffer. `offer` uses
`try_lock`: on contention it drops, otherwise it copies/overwrites and notifies.
No retry, acknowledgement, queue growth or observer wait occurs on offer.

One analyst waits for delivered blocks; analysis runs outside the slot lock.
Every block is an **independent FFT/meter/waveform interval**; dropped blocks
are never spliced into an apparently continuous FFT or waveform. Spectrum
smoothing connects display values only, not PCM history.

There is no extra timer or cadence protocol. Full 1024-frame blocks provide
43.07 opportunities/s at 44.1 kHz and 46.88/s at 48 kHz when production is
paced at source rate. Actual delivery is lossy and may burst during prefetch;
these are not wall-clock refresh guarantees. Presentation chooses its own
render cadence. Hidden-route suspension is not implemented: measured cost
supports keeping the small always-on analyst.

One mutex protects the slot and latest publication. The analyst copies a
complete result into preallocated snapshot storage under a short lock; readers
obtain independent owned copies and never receive guards. Readers can cause
telemetry contention/drop, but cannot backpressure playback. The only blocking
producer calls are the inherited control-only Applied invalidation and close;
they wait for bounded copies, never analysis. Allocation/scheduling/lock
acquisition are not promised a wall-clock deadline.

## 3. One snapshot

`ObservationSnapshot` contains exactly:

| Field | Meaning |
| --- | --- |
| `format` | actual tap `PcmFormat` evidence, including channel mask; unknown layout stays unknown |
| `spectrum_dbfs` | 32 fixed log-frequency display bands, smoothed and clamped to [-80, 0] dBFS |
| `channel_levels` | one linear sample peak and block RMS per interleaved channel index; no guessed speaker labels |
| `waveform` | 64 mono bucket-average points from the same delivered block |

No timestamp, source position, delivered counter, episode identity or playback
validity state is public. Reader lifetime identifies the episode. O1 diagnostic
counters remain test-only, outside product semantics. Samples that are not
finite contribute zero to observation only; main PCM is untouched.

### Spectrum

**FFT_SIZE = 1024; FFT_DEPENDENCY = realfft 3.5.0** (RustFFT backend).
The workspace had no FFT implementation. One narrow mature direct dependency
provides a real forward FFT and explicit reusable scratch; no custom FFT or
DSP framework is introduced. Planning and buffer allocation occur once on the
analyst thread, outside playback startup's mandatory work.

1024 matches delivered blocks: 21.33–23.22 ms window duration, 43.07–46.88 Hz
bin spacing. 2048 would improve spacing to 21.53–23.44 Hz but double interval
length to 42.67–46.44 ms and require accumulation/gap handling across lossy
blocks (or artificial zero padding). The small cost comparison below did not
earn that complexity. Low-frequency display resolution is intentionally coarse.

The arithmetic mean across channels becomes mono input. Opposite-phase
channels can cancel; per-channel meters still reveal their energy. Apply a
periodic Hann window, real FFT, then single-sided magnitude scaled by the
window sum (factor two except DC/Nyquist). Full-scale bin-centered sine
amplitude is the 0 dBFS reference before display smoothing. A short final block
is zero-padded and normalized by its actual window sum; a one-frame block at
the Hann zero yields floor spectrum, while its meter/waveform remain measured.

`SPECTRUM_BAND_EDGES_HZ` defines 32 geometrically spaced bands from **40 Hz to
16 kHz**, independent of FFT layout and terminal width. Upper edges are clipped
to Nyquist; bands wholly above Nyquist remain at the floor. Use maximum
amplitude among bin centers in each band; if a narrow band contains no bin,
linearly interpolate magnitude at its geometric center. Safe `20 log10`
projection floors at -80 dBFS and caps at 0 for display. Peak/RMS retain overload
information instead of clipping it away. Apply one local dB-domain exponential
update per delivered block: attack 0.65, decay 0.15. These are display parameters,
not calibrated spectral density or loudness measurement.

### Meter and waveform

Peak = maximum absolute sample; RMS = square root of mean squared samples,
computed per channel over the delivered block with f64 energy accumulation.
Both are linear amplitude relative to nominal full scale 1.0; values above 1.0
remain visible. A client may convert positive amplitude to `20 log10(a)` dBFS,
choosing its own zero display floor. No meter smoothing, LUFS, acoustic
loudness, true peak or device-volume measurement is claimed.

Waveform uses the same mono input: divide this block into 64 temporal buckets
and average each bucket. Short blocks repeat the relevant sample as needed to
fill the fixed shape. Opposite-phase channels cancel. No full-scale clamp, raw
PCM history, cross-block accumulation or waveform history exists.

## 4. Cuts and lifetime

| Event | Observation behavior |
| --- | --- |
| Seek `Applied` | under the slot lock drop pending old PCM, withdraw the latest snapshot, arm the existing boundary bit; reset spectrum smoothing on the next delivered flagged block |
| `RefusedUnchanged` | no observation reset call; same continuation, subject to ordinary lossy delivery |
| Pause / Resume | analyze any PCM that arrives (bounded prefetch remains legal); otherwise retain latest; cosmetic freeze/decay belongs to presentation |
| Stop / EOF / failure | owner requests close; analyst finishes at most its in-flight and one pending analysis, then exits and joins; final snapshot may remain readable |
| Analyst spawn failure / panic | close observation only; playback never branches on reader availability |
| Replacement | fresh episode handle/worker/reader; old retained readers remain old and closed, never alias the new episode |

**Late pre-cut rejection uses the existing bit.** Taking a block clears its
boundary bit under the slot lock. While analyzing it, the single analyst cannot
take another block. If Applied races that work, it arms `after_cut` and withdraws
the snapshot under the same lock. Publication checks that bit under the lock:
a late old result is rejected. If publication won first, Applied withdraws it.
Only the subsequent post-cut take consumes the bit, resetting smoothing before
analysis. Multiple cuts and dropped post-cut offers leave the bit armed.
There is no overlap, meter smoothing or waveform history to reset; each new
analysis overwrites all other signal-derived outputs and FFT input.

A reader copy returned before a cut is already owned presentation data; it
cannot be revoked. A fresh `latest()` after invalidation returns None until
post-cut publication. Readers must poll the current episode reader and must not
label cached copies as current playback truth. `is_closed()` reports observation
close requested/unavailability, not analyst join acknowledgement or a D11 Fact.
It can become true before the bounded final pending block is published.

## 5. Consumer boundary

`PlaybackSessionHandle::observation_reader()` returns an optional read-only
`ObservationReader` once the decode worker attaches it. Before worker startup
(or an activation that never starts one), it is None. The reader has only
`latest()` and `is_closed()`; it exposes no PCM, reset, seek or lifecycle control.
Obtain the new handle's reader on replacement. There is no coherent cross-cell
snapshot with `PlaybackSessionObservation`; playback truth stays on that seam.

#188 can render Spectrum from band edges/values, Peak meter from channel
levels/format, and Waveform from the fixed array. View code needs no FFT or
analyst internals. Public types contain no Ratatui geometry, color, glyph or
GUI dependence. Presentation may skip snapshots and must tolerate unchanged
latest values. Caller-retained owned copies are caller memory; the observation
plane keeps only one current publication.

## 6. Focused cost and evidence (O6 = PASS)

Repeatable command:

```bash
cargo test -p qianqian-playback --release observation_cost_and_steady_state_allocations -- --nocapture
```

Local target: x86_64-unknown-linux-gnu, WSL2 Linux 6.18.33.2, Intel i7-12700H,
Rust 1.97.1, release profile. Base `cbfb4f28228212fd388cf882e6b0926d4796a513`;
completion PR binds its exact head. 20 warmups then 2000 iterations per
operation/format; thread-scoped existing counting allocator. Timings are
local descriptive evidence, not CI speed thresholds or platform promises.

| Operation | 44.1 kHz stereo median / p99 | 48 kHz stereo median / p99 |
| --- | --- | --- |
| complete analyst pass | 9.116 / 19.879 µs | 8.914 / 16.531 µs |
| snapshot publication | 32 / 33 ns | 32 / 33 ns |
| producer offer copy + notify | 211 / 241 ns | 206 / 231 ns |
| contended producer drop | 23 / 24 ns | 23 / 24 ns |
| offer + analyst copy-out | 278 / 305 ns | 255 / 274 ns |
| owned reader snapshot copy | 55 / 69 ns | 55 / 69 ns |

All measured producer, copy-out, analysis and publication paths allocate **0**
times after initialization. Reader copies allocate **one channel array/read**
(16 payload bytes for stereo), entirely on the presentation thread. No extra
snapshot wrapper allocation occurs. FFT-only comparison: 1024 median/p99
0.881/0.978 µs; 2048 1.965/2.554 µs, both zero steady-state allocations.
At natural full-block cadence, median analysis work is about 0.04% of one
CPU core; this excludes wake/scheduling overhead and possible prefetch bursts.
No measured bottleneck earns hidden-route coordination or another worker.

Explicit stereo buffers total **24,632 bytes**: two interleaved blocks, FFT
input/output/scratch, energy, and two channel arrays. Inline analyst storage
is 4,616 bytes; shared storage is 512 bytes in this test build (includes oracle
counters). These sizes exclude fixed-size FFT plan internals, allocator/thread
runtime overhead and the analyst's default thread stack reservation. For C
channels, PCM storage is `2 × 1024 × C × 4` bytes, plus `24 × C` bytes for
energy/channel arrays; fixed FFT/window/display storage is independent of C.
Planning is once for fixed N; there is no retained interval history or queue.
Presentation copies have fixed arrays plus `8 × C` payload bytes each.

Strong regression evidence: deterministic silence/two tones/amplitude,
channel peak/RMS and overload, waveform polarity/cancellation/short final block,
controlled in-flight Applied rejection + complete reset, coherent latest
publication, structural replacement isolation, concurrent readers, measured
allocation paths; reused O1 drop/overwrite/teardown tests and real worker
Applied/offer seam tests (extended to prove public reader attachment).
`RefusedUnchanged` reset absence follows the Applied-only production call site;
existing seek/refusal suites retain their playback content claims.

This evidence does not establish Windows/WASAPI/device/audible behavior,
architecture-wide correctness, or future DSP transforms. Those claims were
not changed. Future DSP L/T/M/F changes first require upstream authority and
then adaptation of actual tap format/time semantics. Non-goals remain
recognition, beat/pitch/BPM, spectrogram/history, recording/lossless delivery,
LUFS, room/microphone analysis, observation-driven control, a new Plugin or
Capability, generic EventBus, worker pool, or analysis runtime framework.
