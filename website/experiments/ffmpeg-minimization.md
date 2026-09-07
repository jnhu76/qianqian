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

| Evidence | Source | Status |
|----------|--------|--------|
| One FFmpeg closure authority | `component-boundary-a0.md §B.2` | <StatusBadge status="FROZEN" /> |
| Two build profiles (codec-base, songcore-test) | `playback-reference-v1` tag | HISTORICAL (not on main) |
| No FFmpeg type crosses the seam | SongCore ABI v1 | <StatusBadge status="FROZEN" /> |
| Per-codec plugins rejected | `component-boundary-a0.md §B.2` | <StatusBadge status="FROZEN" /> |

**Evidence quality:** Frozen architectural decisions on main. Machine artifact files preserved on the `playback-reference-v1` tag.

**Last verified against main:** N/A — evidence is historical, frozen on `playback-reference-v1`.

---

## 06 Result

<ClaimBadge role="evidence" />

Decoder and Processing share **one FFmpeg closure authority**. Key findings:

- Codec coverage is **provider configuration**, not a runtime layer
- Per-codec Decoder plugins (MP3/FLAC as separate plugins) would split one closure authority for **zero composability gain**
- The closure is minimizable through the oracle approach
- Two build profiles prove the approach is reproducible

**Machine artifacts:** Preserved on `playback-reference-v1` tag (not on main). See `native/ffmpeg/profiles/*.json` and `capabilities/*.json` on that tag.

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
  :decisions="[{ issue: 48 }, { issue: 53 }]"
  :evidence="['research/playback-reference-v1']"
/>
