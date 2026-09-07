---
title: FFmpeg Minimization
status: HISTORICAL_EVIDENCE
---

# FFmpeg Minimization

<StatusBadge status="HISTORICAL_EVIDENCE" />

## How much FFmpeg does a music player actually need?

---

## 01 Question

What is the minimum FFmpeg closure required by Qianqian's declared capabilities?

---

## 02 Baseline

A typical FFmpeg integration brings the entire FFmpeg library. Qianqian needs only a subset for decode and future processing.

---

## 03 Hypothesis

The minimum closure can be identified through a machine-derived oracle approach:

1. Declare capability intent (what decode formats are needed)
2. Use FFmpeg configure as an oracle to determine required components
3. Derive the minimal closure from the oracle output
4. Build a reproducible manifest
5. Verify the product works with only the declared closure

---

## 04 Method

### Capability Intent

Qianqian declares two capabilities sharing one FFmpeg closure:

| Capability | Data Flow |
|-----------|-----------|
| Decoder | Encoded media → PCM |
| Processing | PCM → PCM |

### Configure / Import as Oracle

FFmpeg's configure system is used as an oracle to determine which components are needed for the declared codec set. The oracle answers: "given these decode requirements, what is the minimum build?"

### Machine-Derived Closure

The closure is derived mechanically — not by hand-editing build files. Two build profiles were established:

| Profile | Purpose |
|---------|---------|
| `codec-base` | Core decode capabilities |
| `songcore-test` | Extended test coverage |

### Manifest Replay

The manifest captures the exact closure composition. Build replay proves the closure is reproducible.

---

## 05 Evidence

<ClaimBadge role="evidence" />

> **HISTORICAL PLAYBACK-REFERENCE-V1 EVIDENCE** — everything in this section is the record of a frozen experiment (FFmpeg `n9.0.1`, pre-Rust native vertical slice). It is **not** a statement about today's Rust Qianqian binary, which contains no FFmpeg. Verified 2026-09-07 against issue #2 and the `playback-reference-v1` tag.

### Experiment scope

| Item | Value |
|------|-------|
| FFmpeg | `n9.0.1` (tag → commit `bf1b838f…`, zip sha256 pinned in `bench/ffmpeg-pin.json`) |
| Capability | Stage-A local playback: probe / metadata / artwork bytes / decode / seek / EOF for MP3 + FLAC |
| IO model | Host-owned IO only — custom `AVIOContext` over host callbacks; no protocols, no filesystem, no network permission inside FFmpeg |
| Corpus | `stage-a-v1`: 15 deterministic synthetic fixtures (8 MP3, 7 FLAC), sha256 pinned |
| Build baseline | gcc 15.2.0, Linux x86_64, `--disable-autodetect` (zero external libs), `--disable-x86asm` for all profiles |

### N0 → N3 / N3-noswr ladder

| Profile | Static libs (B) | Stripped+xz libs (B) | Linked bench stripped (B) | Symbols |
|---------|----------------:|---------------------:|--------------------------:|--------:|
| N0 full | 39,581,790 | 10,339,040 | 20,515,208 | 56,272 |
| N1 audio | 12,858,160 | 3,738,108 | 8,187,864 | 21,702 |
| N2 stage-a | 2,741,332 | 694,684 | 1,042,680 | 5,057 |
| N3 min | 2,741,332 | 694,684 | 1,042,680 | 5,057 |
| N3 noswr | 2,549,130 | 645,620 | 911,608 | 4,714 |

≈ 14.4× static-lib reduction (N0→N2/N3) and ≈ 14.9× distributable-artifact reduction; N3 equals N2 in size because the deletion experiments proved the N2 set irreducible (below), not because trimming stopped.

Per-library split (N0 vs N3): `libavcodec.a` 21,879,630 B → 791,850 B (477 decoders → 2); `libavformat.a` 5,832,278 B → 484,132 B (359 demuxers → 2); `libavutil.a` ~1.27 MB both (base library, nearly irreducible); `libavfilter.a`/`libswscale.a`/`libavdevice.a` (7.98 MB + 2.32 MB + 98 KB) dropped entirely at N2+.

### Correctness gates

15 fixtures × 5 profiles = 75 runs; every profile: **12 PASS + 3 DEGRADED-PASS + 0 FAIL** (60 pass / 15 degraded / 0 fail total). Failures had to be classified (`open_failed / probe_failed / decode / seek / pcm / metadata / artwork / timeout / crash`), no bare PASS/FAIL.

| Gate | Result |
|------|--------|
| FLAC strict PCM | byte-identical to reference PCM (sha256 over canonical Float32-interleaved bytes) for all FLAC fixtures |
| MP3 cross-profile consistency | identical PCM across N0/N1/N2/N3/noswr for all 14 comparable fixtures (1 truncated-header fixture is rejected at open by every profile) |
| Seek proof | FLAC seek 25%/50%/75% restores to a frame boundary ≤ target; post-seek suffix decode **byte-identical** to sequential decode (e.g. flac-16-44-stereo N3: 1.0 s → 41472 samples, 2.0 s → 87552, 3.0 s → 129024, all byte-identical, sub-ms) |
| EOF discipline | clean dual-side arrival (demux + decoder `AVERROR_EOF`); no crash on truncated/corrupt fixtures |
| Component provenance | demuxer:mp3, demuxer:flac, decoder:mp3float, decoder:flac → **required** (deletion breaks fixtures); parser:mpegaudio, parser:flac → **configure-required** (dependency graph forces them; deletion is a no-op) |

Notable machine fact: the resolved decoder is `mp3float` (not the fixed-point alias `mp3`); enabling `mp3` in N2/N3 would produce different PCM than N0/N1 and the consistency gate (correctly) fails.

### Performance

Minimization did **not** cause a meaningful decode-throughput regression, and headroom stayed very large. Median ×realtime (`songcore-output` path; warm-up 1 + 5 rounds):

| Sample | N0 | N3 | N3 noswr |
|--------|-----:|-----:|-----:|
| MP3 CBR | 1948× | 1753× | 1664× |
| MP3 VBR | 2250× | 2032× | 2234× |
| MP3 long | 1942× | 1782× | 1961× |
| FLAC 16/44 | 1097× | 1063× | 1151× |
| FLAC 24/96 | 428× | 412× | 498× |

All samples stay above 400× realtime (heaviest: flac-24-96 ≈ 410×). N0/N3 fluctuations overlap; no systematic regression. Conversion+output overhead vs raw decode was ~5–20%, in micro/millisecond absolute terms. Cold open (open+probe) actually improved after trimming (N0 0.25–0.57 ms → N2/N3 0.12–0.45 ms).

### libswresample result

**Partially bypassable — narrow scope.** For the Stage-A contract (source-rate / source-layout Float32 interleaved output), libswresample only performed format + planar→interleaved conversion (no resampling, no rematrix), and the SongCore-owned conversion path was **byte-identical (14/14)** while saving 192 KiB static libs / 131 KiB linked.

This proved **only** that swresample is bypassable for that contract. It did **not** prove that resampling or rematrixing will never be needed — if device-rate adaptation is ever required, swresample remains the necessary last mile.

---

## 06 Result

<ClaimBadge role="evidence" />

Decoder and Processing share **one FFmpeg closure authority**. Key findings:

- Codec coverage is **provider configuration**, not a runtime layer
- Per-codec Decoder plugins (MP3/FLAC as separate plugins) would split one closure authority for **zero composability gain**
- The closure is minimizable through the oracle approach
- Two build profiles prove the approach is reproducible

**Machine artifacts:** Preserved on `playback-reference-v1` tag (not on main). See `native/ffmpeg/profiles/*.json`, `native/ffmpeg/capabilities/*.json`, `bench/results/` on that tag.

**Later continuation on the frozen tag:** the same method was continued in the SongCore-v1 era as a capability ladder (MP3+FLAC → +AAC → +ALAC → +WAV → +Vorbis → +Opus → minimized common formats, `bench/results/common-formats/ladder.md` on the tag, final minimized `.so` 1.25 MiB stripped / 492 KiB stripped+xz). That is frozen evidence on the same tag, not current-code truth.

---

## 07 Architectural Consequence

<ClaimBadge role="authority" />

Frozen in component-boundary-a0.md:

> When FFmpeg is reintroduced, Decoder/Processing must continue to share one FFmpeg closure authority rather than duplicating dependencies.

This is a **boundary-level architectural constraint**, not merely a build optimization. The shared closure is part of the component contract.

---

## Open Questions

- How should the shared closure authority be expressed in the future plugin/composition system?
- What is the minimal set of FFmpeg build flags for the first real Decoder implementation?
- Can the closure be verified automatically in CI?

---

<ProvenancePanel
  :authority="['docs/architecture/component-boundary-a0.md', 'docs/architecture/overview.md']"
  :decisions="[{ issue: 2 }, { issue: 3 }, { issue: 48 }, { issue: 53 }]"
  :evidence="['research/playback-reference-v1']"
  last-verified="issue #2 + playback-reference-v1 tag, 2026-09-07"
/>
