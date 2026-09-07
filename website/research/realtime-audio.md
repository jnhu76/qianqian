---
title: Realtime Audio Research
status: HISTORICAL_EVIDENCE
---

# Realtime Audio Research

<StatusBadge status="HISTORICAL_EVIDENCE" />

## What makes audio output realtime-safe?

---

## Source

<ClaimBadge role="evidence" />

Historical evidence from the playback-reference-v1 frozen experiment. WASAPI renderer and PlayerEngine architecture.

| Source | Evidence |
|--------|----------|
| `native/src/player/wasapi_renderer.hpp` | Event-driven render thread, mutex-free engine seam |
| `native/src/player/pcm_ring.hpp` | Engine-owned ring buffer |
| `native/src/player/playback_timeline.hpp` | Single-owner timeline accounting |
| `native/src/player/wasapi_submit_accounting.hpp` | Submit vs render accounting |
| `native/src/player/null_audio_backend.*` | Headless correctness mode |
| `native/src/player/commit_flush_handshake.hpp` | Single-flight commit/flush protocol |

---

## What the Evidence Says

<ClaimBadge role="evidence" />

### Render Thread Architecture

The WASAPI renderer runs an **event-driven render thread** over a mutex-free engine seam:

- `fill_output` / `advance_render` with padding-proven playout
- Renderer created **after** engine, destroyed **before** engine
- Device failure degrades to bounded-retry with audible-position freezing

### Timeline Ownership

Single-owner timeline accounting — split across submit/render sides was a real bug source. The fix: timeline ownership must be clearly assigned to one side.

### Commit/Flush Protocol

Single-flight protocol with four states:

```text
REQUESTED → CLAIMED → COMPLETED
                  ↘ CANCELLED
```

Key invariants:
- **I3:** Cancel before claim = never began
- **I4:** Claimed can never be cancelled — no faked rollback of a possibly-started physical flush
- **I5:** No cross-request ACK/ABA

### RT Data Edge

Audio data flows through pre-bound endpoints, not through Context/event dispatch per block.

---

## What Qianqian Borrows

<ClaimBadge role="authority" />

| Evidence | Qianqian Principle |
|---------|-------------------|
| Mutex-free render thread | Realtime path has zero mutex |
| Single-owner timeline | One side owns timeline accounting |
| Renderer lifecycle order | Provider lifecycle ordering matters |
| Pre-bound data edges | No per-block Context lookup |
| Commit/flush single-flight | Irreversible actions have explicit protocol |

---

## What Qianqian Does NOT Borrow

<ClaimBadge role="interpretation" />

- **WASAPI-specific implementation** — The principles are platform-agnostic
- **Specific buffer sizes** — Tuning is implementation-specific
- **Device enumeration details** — Platform concern, not architecture

---

## Architectural Consequence

<ClaimBadge role="authority" />

Frozen in overview.md and component-boundary-a0.md:

> The realtime audio path is a data-plane island. Per callback/block it must not perform Context lookup, capability resolution, Fiber reconciliation, arbitrary event dispatch, filesystem/network I/O, or UI round trips.

Future graph changes must be prepared on the **control plane** and published at an **RT-safe boundary**.

---

## Open Questions

- How should the AudioRuntime plugin own AudioGraph, clock, buffer pool, format negotiation, RT scheduling, and graph publication?
- What is the correct RT-safe boundary for publishing control-plane graph changes?
- Can the commit/flush protocol be generalized for other irreversible operations?

---

<ProvenancePanel
  :authority="['docs/architecture/component-boundary-a0.md §A.2', 'docs/architecture/overview.md']"
  :decisions="[{ issue: 53 }, { pr: 66 }]"
  :evidence="['research/playback-reference-v1']"
/>
