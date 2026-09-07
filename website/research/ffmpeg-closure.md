---
title: FFmpeg Closure Research
status: HISTORICAL_EVIDENCE
---

# FFmpeg Closure Research

<StatusBadge status="HISTORICAL_EVIDENCE" />

## How to minimize FFmpeg for a music player

---

## Source

<ClaimBadge role="evidence" />

FFmpeg build profiles and capabilities established during the playback-reference-v1 experiment. Frozen as architectural decisions in component-boundary-a0.md.

| Source | Type |
|--------|------|
| `native/ffmpeg/profiles/*.json` | Machine artifacts (on `playback-reference-v1` tag) |
| `native/ffmpeg/capabilities/*.json` | Machine artifacts (on `playback-reference-v1` tag) |
| `component-boundary-a0.md §B.2` | Frozen architectural decision |
| Issue #48 | Historical native-media evidence |

---

## What FFmpeg Configure Says

<ClaimBadge role="evidence" />

FFmpeg's configure system acts as an **oracle** — given a set of desired codecs, it determines the minimum component set required. The oracle approach:

1. Declare desired codec set (what Qianqian needs to decode)
2. Run FFmpeg configure to derive required components
3. Build only the derived components
4. Verify the build produces working decoders

Two build profiles were established:

| Profile | Codec Set | Purpose |
|---------|----------|---------|
| `codec-base` | Core decode codecs | Primary decode capability |
| `songcore-test` | Extended test codecs | Test coverage |

---

## What Qianqian Borrows

<ClaimBadge role="authority" />

- **One closure authority** — Decoder and Processing share a single FFmpeg build, not separate copies
- **Configure-as-oracle** — Machine-derived closure, not hand-edited build files
- **Manifest replay** — Closure composition is captured and reproducible
- **Closure minimization** — Only needed components are built

---

## What Qianqian Does NOT Borrow

<ClaimBadge role="interpretation" />

- **Per-codec plugin splitting** — Rejected because it multiplies the closure authority for zero composability gain
- **Runtime codec detection** — Codec coverage is provider configuration, not a runtime layer
- **Full FFmpeg feature set** — Only the declared codec set is needed

---

## What Qianqian Changed

<ClaimBadge role="evidence" />

The traditional approach of linking the full FFmpeg library was replaced with a **capability-driven minimal closure**. The component boundary audit (#53) froze this as an architectural constraint:

> When FFmpeg is reintroduced, Decoder/Processing must continue to share one FFmpeg closure authority rather duplicating dependencies.

---

## Open Questions

- What is the exact FFmpeg configure flag set for the first real Decoder implementation?
- Can the closure be verified automatically in CI?
- How should the shared closure be represented in the future plugin/composition system?

---

<ProvenancePanel
  :authority="['docs/architecture/component-boundary-a0.md §B.2', 'docs/architecture/overview.md']"
  :decisions="[{ issue: 48 }, { issue: 53 }]"
  :evidence="['research/playback-reference-v1']"
/>
