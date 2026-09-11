# Decode cost model

Nature: **PERFORMANCE EVIDENCE / DECISION SUPPORT — NOT ARCHITECTURE AUTHORITY**

This document turns the native decode benchmark into a durable cost ledger that can be extended as the production path gains Rust FFI, a safe adapter, a real Decode Plugin, and bounded PCM transport.

Source evidence:

- #110 — decode throughput/latency + integration-tax benchmark track.
- #112 — corrected native decode baseline; Draft, unmerged at the time this record was written.
- #111 — corrected historical native evidence substrate used by #112.
- #109 — Phase E execution roadmap.

The measurements were taken on the corrected historical native substrate because current `main` does not yet contain the production SongCore mechanism. They establish a performance reference; they do **not** make the historical playback architecture current again.

---

## 1. The question we care about

The useful question is not "how many MB/s can FFmpeg decode?".

The useful question is:

> When we add one architectural layer, how much performance does that layer cost us?

We therefore keep a cumulative tax ladder:

```text
raw FFmpeg decode
        │
        │ + SongCore tax
        ▼
SongCore C ABI
        │
        │ + Rust FFI tax
        ▼
Rust raw FFI
        │
        │ + safe-adapter tax
        ▼
Rust safe adapter
        │
        │ + Decode Plugin tax
        ▼
Decode Plugin
        │
        │ + bounded-transport tax
        ▼
PCM transport
```

Every future benchmark should answer the same three questions:

1. How fast was the previous layer?
2. How fast is the new layer?
3. What absolute and relative cost did the new layer add?

This keeps optimization local: if one step suddenly becomes expensive, we know which boundary to investigate first.

---

## 2. Current baseline in plain language

### SongCore costs a few percent, not an order of magnitude

For long real media, wrapping raw FFmpeg decode in SongCore costs roughly **4%–7%** wall time.

Representative real MP3 result:

```text
224 s audio

raw FFmpeg decode     ≈ 1544× realtime
SongCore C ABI        ≈ 1444× realtime

whole-file decode time
raw FFmpeg            ≈ 145 ms
SongCore              ≈ 155 ms
added cost            ≈ 10 ms
relative tax          ≈ 6.9%
```

So the useful interpretation is:

> SongCore buys us a stable C ABI, canonical Float32 PCM and caller-buffer semantics for roughly ten extra milliseconds while decoding a 224-second song.

Across representative codecs, the observed SongCore tax is normally around **1%–10%**. Very short or extremely cheap inputs can show unstable percentages because the absolute difference is only a few microseconds; those cases should not drive architecture decisions.

### Codec-level picture

The corrected benchmark gives this rough range:

| Family | Approximate SongCore tax | Interpretation |
|---|---:|---|
| Long real MP3 | +4% to +7% | small, measurable cost |
| MP3 / AAC fixtures | +6% to +10% | still small relative to available headroom |
| FLAC | +1% to +3% | close to free |
| ALAC | +1% to +2% | close to free |
| Opus | about +3% | small |
| Very cheap PCM fixtures | percentage may look large | absolute cost is tiny; do not over-read the percentage |

This is the first filled row of the tax ledger. Rust and Plugin rows remain intentionally unknown until those layers exist on the real path.

---

## 3. Decode compute is not the current bottleneck

The slowest measured case was ALAC 24/96 at about **245× realtime** through SongCore.

That means decoding one second of that audio needs about:

```text
1 / 245 second ≈ 4.1 ms of one CPU core
```

or roughly **0.4% of one core** to keep up with realtime playback.

The long real MP3 path was about **1444× realtime**, meaning roughly **0.69 ms of decode work per second of audio**.

Therefore, for the measured host and current codec set:

> Average decode compute has enormous headroom and should not be treated as the primary playback bottleneck.

Future performance work should focus more on boundary overhead, scheduling jitter, buffering, processing cost and device-side behavior than on squeezing another few percent out of raw decode throughput.

---

## 4. Startup is already small

For ordinary fixtures, SongCore time-to-first-PCM was roughly:

```text
0.15 ms to 0.62 ms
```

The larger observed cases were structural rather than decoder throughput problems:

- a WAV path spent about 3.5 ms probing duration;
- an MP3 with about 9 MB of artwork spent about 1.0 ms in open/ID3 parsing.

So if a future player path takes tens or hundreds of milliseconds before sound starts, native decode should not be the first suspect without new evidence.

---

## 5. Steady read latency is small relative to an audio block

At 1024 frames per `song_read_pcm` call:

- lossy long-file p99 values were in the tens of microseconds;
- sufficiently sampled long lossy files had p99.9 around **57–88 µs**;
- lossless p99 values reached roughly **100–212 µs** because reads are bimodal: many calls serve already-buffered PCM, while some calls decode a full codec frame;
- lossless p99.9 is **not yet characterized** because the current corpus does not provide enough long lossless samples.

For perspective, 1024 frames correspond to about:

```text
44.1 kHz → 23.2 ms of audio
48 kHz   → 21.3 ms
96 kHz   → 10.7 ms
```

A roughly 0.2 ms lossless p99 decode event is still small compared with a 10–23 ms block duration.

This does **not** prove the final realtime pipeline is safe. It only says the native decode producer currently has substantial latency headroom before Rust/Plugin/transport/device costs are added.

---

## 6. Decoder throughput does not force a large block size

On a 224-second real MP3, sweeping caller block size from 64 to 4096 frames changed throughput by only about ±10%.

The practical consequence is important:

> The future PCM block size does not need to be chosen primarily to keep the decoder efficient.

Later transport design may therefore prioritize:

- latency;
- scheduling margin;
- queue depth;
- backpressure;
- ownership/lifetime simplicity;
- processing and device requirements.

Block size must still be earned by the real end-to-end path; this baseline only removes "FFmpeg needs huge blocks" as an assumed constraint.

---

## 7. Tax ledger

This table is the durable comparison surface for #110.

| Layer | Representative performance | Added cost vs previous layer | Status |
|---|---:|---:|---|
| Raw FFmpeg decode | long MP3 ≈ 1544× realtime | — | measured |
| SongCore C ABI | long MP3 ≈ 1444× realtime | ≈ +6.9% wall time; long media typically +4%–7% | measured |
| Rust raw FFI | TBD | TBD | measure after SongCore exists on current main |
| Rust safe adapter | TBD | TBD | measure after adapter exists |
| Decode Plugin | TBD | TBD | measure the real pre-bound Plugin path |
| Bounded PCM transport | TBD | TBD | measure after the real bounded edge exists |

Future rows should report both:

```text
absolute cost: ns / µs / ms
relative cost: % vs previous layer
```

Percentages alone are misleading when the baseline operation is extremely cheap.

---

## 8. How future comparisons should be reported

Prefer this form:

```text
SongCore
1444× realtime

+ Rust FFI
1438× realtime
FFI tax = 0.4%

+ safe adapter
1429× realtime
adapter tax = 0.6%

+ Decode Plugin
1418× realtime
Plugin tax = 0.8%
```

The numbers above are illustrative except for the measured SongCore baseline.

Do not hide a slow architectural layer behind the fact that native decode is hundreds of times faster than realtime. A layer that adds a few microseconds may be fine; a layer that creates millisecond-scale long-tail stalls may be unacceptable even if total xRT remains high.

For every new layer, keep throughput and latency as separate questions:

```text
throughput: did average work become meaningfully more expensive?
latency:    did p95/p99/max or jitter become meaningfully worse?
```

---

## 9. Evidence limits

The current baseline is intentionally bounded:

- measured on WSL2 / Ryzen 7 5800H with one pinned logical CPU;
- based on the corrected historical SongCore + trimmed FFmpeg substrate;
- long-file coverage is strongest for MP3;
- long lossless p99.9 remains a corpus gap;
- it does not include Rust FFI, Plugin dispatch, bounded transport, processing or WASAPI;
- it is performance evidence, not a latency SLA and not architecture authority.

The local FLAC file excluded by #112 had a corrupt download tail and is not treated as a SongCore/FFmpeg defect.

---

## 10. What this baseline authorizes

This evidence supports the following engineering decisions:

1. Preserve SongCore's Media→PCM mechanism rather than rewriting FFmpeg state machines in Rust for performance reasons.
2. Port the smallest self-contained SongCore mechanism to current `main`, without reviving the old PlayerEngine/runtime architecture.
3. Re-run an equivalent benchmark after that port to prove correctness and performance equivalence.
4. Then extend the same ledger across Rust FFI, safe adapter, Decode Plugin and bounded PCM transport.

The baseline does **not** authorize importing the historical player, playback state machine, WASAPI composition or other old architecture simply because they existed beside SongCore.

The durable rule is:

> Keep the mechanism that earned its evidence; charge every new boundary for the tax it adds.
