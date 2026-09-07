---
title: FFmpeg Closure Research
status: HISTORICAL_EVIDENCE
---

# FFmpeg Closure Research

<StatusBadge status="HISTORICAL_EVIDENCE" />

## What engineering method did Qianqian learn from the FFmpeg minimization experiment?

---

## Source

<ClaimBadge role="evidence" />

FFmpeg build profiles and capabilities established during the playback-reference-v1 experiment. Frozen as architectural decisions in component-boundary-a0.md.

| Source | Type |
|--------|------|
| `native/ffmpeg/profiles/*.json` | Machine artifacts (on `playback-reference-v1` tag) |
| `native/ffmpeg/capabilities/*.json` | Machine artifacts (on `playback-reference-v1` tag) |
| `tools/ffmpeg_import.py` + Xmake replay | Qianqian-owned import/replay tooling (on the tag) |
| `bench/results/common-formats/ladder.md` | Later capability-ladder continuation (on the tag) |
| `component-boundary-a0.md §B.2` | Frozen architectural decision |
| Issue #2 / #3 / #48 / #49 | Experiment record and boundary audits |

---

## The durable method

<ClaimBadge role="evidence" />

The experiment established a repeatable, upgrade-safe way to keep a dependency closed and minimal:

```text
capability intent
      ↓
upstream configure / Make used as an import / upgrade oracle
      ↓
machine-derived source + semantic flag closure
      ↓
manifest
      ↓
Qianqian-owned normal-build replay
      ↓
behavior / PCM / seek / symbol / size gates
```

The core idea:

> The upstream build system is an **oracle**, not necessarily the normal product build system.

In the reference implementation the normal Qianqian build did **not** re-run the FFmpeg Makefile (`xmake ffmpeg-import` runs configure/Make once to capture real `V=1` compiler invocations; the manifest then feeds a Qianqian-owned Xmake replay that produces `libqianqian_av.a`). The upgrade invariant is: bump the pin → re-run the importer → diff the closure → rebuild → re-run the corpus/PCM/size/symbol gates. No long-lived "deleted source" FFmpeg fork is maintained.

---

## What Qianqian Borrows

<ClaimBadge role="authority" />

- **One closure authority** — Decoder and Processing share a single FFmpeg build, not separate copies
- **Configure-as-oracle** — Machine-derived closure, not hand-edited build files
- **Manifest replay** — Closure composition is captured and reproducible
- **Closure minimization** — Only needed components are built
- **Machine-derived dependency knowledge** — the closure is recomputed per upgrade, never hand-maintained

---

## What Qianqian Does NOT Borrow

<ClaimBadge role="interpretation" />

- **Per-codec plugin splitting** — Rejected because it multiplies the closure authority for zero composability gain
- **Runtime codec detection** — Codec coverage is provider configuration, not a runtime layer
- **Full FFmpeg feature set** — Only the declared codec set is needed
- **No FFmpeg type across the seam** — the ABI discipline is part of the contract (proven by SongCore ABI v1)

---

## What Qianqian Changed

<ClaimBadge role="evidence" />

The traditional approach of linking the full FFmpeg library was replaced with a **capability-driven minimal closure**. The component boundary audit (#53) froze this as an architectural constraint:

> When FFmpeg is reintroduced, Decoder/Processing must continue to share one FFmpeg closure authority rather than duplicating dependencies.

---

## WHAT SURVIVES TODAY

<ClaimBadge role="authority" />

These are **current Architecture v2 constraints** — they survive in `docs/architecture/component-boundary-a0.md` §B.2 even though no FFmpeg code exists on current main:

```text
one closure authority (Decoder + future Processing share it)
machine-derived dependency knowledge
no FFmpeg type crosses the component seam
per-codec runtime plugin split rejected
```

## WHAT DOES NOT EXIST ON CURRENT MAIN

<ClaimBadge role="evidence" />

These are **absent from today's Rust main** — do not read the historical evidence as current implementation:

```text
current Rust Decoder implementation
current Rust FFmpeg build / replay integration
current shipping FFmpeg artifact
```

Verified 2026-09-07: current main contains no FFmpeg crate, no FFmpeg build recipe, and no decoder implementation; the only occurrences are a doc comment (`crates/qianqian-runtime/src/lib.rs:8`) and a capability-naming test string (`crates/qianqian-kernel/tests/adversarial_review.rs:372`).

---

## Open Questions

- What is the exact FFmpeg configure flag set for the first real Decoder implementation?
- Can the closure be verified automatically in CI?
- How should the shared closure be represented in the future plugin/composition system?

---

<ProvenancePanel
  :authority="['docs/architecture/component-boundary-a0.md', 'docs/architecture/overview.md']"
  :decisions="[{ issue: 2 }, { issue: 3 }, { issue: 48 }, { issue: 53 }]"
  :evidence="['research/playback-reference-v1']"
  last-verified="issue #2/#3/#48/#49 + playback-reference-v1 tag + current main scan, 2026-09-07"
/>
