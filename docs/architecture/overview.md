# Architecture overview

> **This document is a derived architecture overview/router. It does not define normative architecture semantics.** Normative Playback Foundations live only in [`../adr/ADR-PBK-001.md`](../adr/ADR-PBK-001.md); K0 semantic authority is [`composition-kernel-0-design.md`](composition-kernel-0-design.md) (representation decisions: [`composition-kernel-0-implementation-adr.md`](composition-kernel-0-implementation-adr.md)). This page summarizes and routes; on any divergence the authorities win.

This document is the repository-local semantic overview for Qianqian Architecture v2.

Generic composition semantics live in `composition-kernel.md` and the K0 design/implementation authorities. Playback Foundations are **accepted** (`../adr/ADR-PBK-001.md`); they fix plane boundaries and contracts, not a production playback state machine or its vocabulary. Term definitions (Kernel / Plugin / Fiber / Capability / Runtime / Realtime Runtime / Fact / Reclamation, …) are normative in `../adr/ADR-PBK-001.md` §16 (Vocabulary / Role Definitions).

The current architecture intentionally separates composition, execution/control, committed facts, and realtime data flow.

---

# Normative constitution

> The normative Playback Foundations live in [`../adr/ADR-PBK-001.md`](../adr/ADR-PBK-001.md). This overview only summarizes them; it does not carry a second normative copy.

---

# Four-plane model

The following are **reasoning lenses / concern boundaries**, not a claim that the runtime consists of exactly four concrete subsystems:

```text
┌────────────────────────────────────────────┐
│              Composition Plane             │
│ Context / Capability / Fiber / Effect      │
│ Reconcile / dependency / withdrawal        │
└────────────────────┬───────────────────────┘
                     │ establishes reachability/lifetime
                     ▼
┌────────────────────────────────────────────┐
│          Execution / Control Plane         │
│ Command / workflow / Capability-Service    │
│ parameter/control operations               │
└────────────────────┬───────────────────────┘
                     │ semantic commit / graph build
          ┌──────────┴──────────┐
          ▼                     ▼
┌──────────────────────┐  ┌──────────────────────────┐
│      Fact Plane      │  │   Realtime Data Plane   │
│ committed Fact       │  │ published graph/view    │
│ projections          │  │ PCM direct flow         │
│ persistence/UI       │  │ execution/device        │
└──────────────────────┘  └──────────────────────────┘
```

These are cooperating concerns, not one universal bus.

---

# Composition Plane

The generic Composition Kernel K0 is implemented/current.

```text
Context
Capability
Fiber
Effect
Reconcile
```

It decides:

```text
who exists
who may reach whom
which provider satisfies a requirement
who owns composition-visible resources/effects
how providers/dependents withdraw
```

It must not know:

```text
PCM
AudioGraph
FFmpeg
WASAPI
seek
track/playlist semantics
player UI state
```

Context is a capability/dependency view, not a payload bus or global state bag.

---

# Plugin / Fiber

A Plugin is a long-lived component definition participating in the common composition/lifecycle protocol after its boundary has been justified.

A Fiber is its live runtime instance.

A Plugin may provide services, register hooks, observe facts, own resources or provide realtime graph participants. It is **not** automatically one step in a payload pipeline.

Therefore neither of these implications is valid without evidence:

```text
AudioNode => Plugin
Plugin => AudioNode
```

The granularity of Decoder, DSP stages and AudioOutput will be earned experimentally.

---

# Execution / Control Plane

Execution begins with intent. This is currently a constraint-oriented lens, not a new K0 subsystem.

```text
User / UI / automation
        ↓
      Command
        ↓
domain/controller/workflow
        ↓
Capability / Service
        ↓
mechanism / authority
```

A future extension may use middleware/waterfall-like interception for execution seams, but that is different from committed Fact delivery and is not yet a generic primitive.

> **Command != Fact** — frozen in `../adr/ADR-PBK-001.md` §2.2.

---

# Fact Plane

A Fact is published only after its truth has been established by its designated semantic authority.

Frozen contracts (names only — normative text in `../adr/ADR-PBK-001.md` §2.3): semantic-commit definition; commit-first; one designated authority per (fact kind, subject scope); mechanism-evidence firewall; projection read-side firewall.

Event Sourcing/CQRS, durability, replay authority and append-only logging remain open research questions.

---

# Realtime Data Plane

PCM is high-frequency hot data and follows a direct typed path.

```text
source/decoder
      ↓ PCM
processing graph
      ↓ PCM
output/device
```

The ADR forbids per-quantum re-entry of the realtime path into Context resolution, generic Fact/Event fan-out, plugin dispatch, Reconcile, or filesystem/network/UI machinery (normative list in `../adr/ADR-PBK-001.md` §2.4); it operates on already-bound/published state.

> **Fact != hot data** — frozen in `../adr/ADR-PBK-001.md` §8.

---

# Dependency graph vs realtime graph

The Composition dependency graph and realtime processing graph answer different questions.

## Dependency graph

```text
who requires whom
who provides what
who must withdraw before whom
```

## Realtime graph

```text
which processing step executes next
where PCM branches/merges
which concrete pre-bound object/function handles the quantum
```

The same resource may participate in both, but edge semantics differ.

Frozen in `../adr/ADR-PBK-001.md` §5: dependency topology != realtime processing topology; realtime/DSP order is never derived from mount/registration/iteration order.

---

# Graph publication boundary

Control side builds and validates the next realtime graph/view; realtime execution loads the currently published view and processes the audio quantum directly.

ADR §6 freezes the normative P1–P5 publication/reclamation semantic contract — coherent publication (P1), retired-view closure (P2), all-generation quiescence before reclamation (P3), retirement != reclaimability != release (P4), and conditional reclamation progress (P5). The normative text lives in `../adr/ADR-PBK-001.md` §6 only; this section summarizes it.

The publication mechanism (RCU / epoch / double buffering / Arc snapshot / lease / hazard / other) is intentionally unfrozen.

---

# Realtime lifetime safety

The withdrawal ordering and reader-quiescence semantics are normative in `../adr/ADR-PBK-001.md` §6. The ADR freezes the safe resource-release condition relative to realtime readers; it does **not** freeze the concrete Provider Fiber ↔ RT resource lifetime binding (that binding remains OPEN). This is the first cross-plane lifetime invariant earned by the reset architecture.

---

# Parameter vs topology changes

Do not force every cheap runtime parameter update through full Plugin Reconcile.

Cheap parameter updates (volume, filter coefficients, ...) are likely control-path updates; inserting/removing stages or replacing decoder/output mechanisms may require topology/provider rebuild/publication. The exact boundary is research-open; see `../adr/ADR-PBK-001.md` §7.

---

# Current playback status

Playback-specific state-machine authority has been deliberately reopened.

The repository still contains prior experimental concepts such as (full reopened list: `../adr/ADR-PBK-001.md` §0/§10):

```text
MusicKernel
TransportKernel
TrackSession
DecodeSession
Generation
Active / Prepared
Dual Window
Physical Fence
```

They are currently **experimental evidence only**.

They are not stable architecture vocabulary and must not be preserved for compatibility unless a future accepted ADR re-earns them.

Similarly, `specs/playback/*` is a valuable source of bug reproducers and formal/testing techniques, but is not a current acceptance gate.

---

# New research order

The one normative research/implementation ladder lives in `../adr/ADR-PBK-001.md` §12: mechanism-first at the foundation, semantics-later at the player level.

---

# Formal verification boundary

Formal verification remains risk-driven.

Do not model every architectural noun.

The old PlaybackTemporal/PlaybackOwnership models are no longer blocking architecture authority.

Realtime publication/lifetime was the first post-reset formal target and is delivered: `specs/realtime-publication/` proved the publication/reclamation collision at model level (TLC exhaustive check + mutation negative controls), and its semantic conclusions are frozen as the normative P1–P5 contract in ADR §6. Implementation-level mechanism evidence has been delivered (`docs/architecture/realtime-view-publication.md`, PR #97); the final production mechanism remains open — §12 Phase D validates candidate mechanisms against P1–P5.

---

# Current workspace

```text
qianqian-audio-api
qianqian-composition
qianqian-app
qianqian-headless
```

The Composition Kernel K0 is current.

Playback-specific code is research evidence and may be changed/removed without compatibility obligation; the accepted foundations deliberately do not re-freeze legacy playback nouns.

---

# Document classes (router)

```text
Normative authority
    Generic composition (K0 semantics)
        docs/architecture/composition-kernel-0-design.md
        docs/architecture/composition-kernel-0-implementation-adr.md
            (representation decisions)
    Playback foundations
        docs/adr/ADR-PBK-001.md — ACCEPTED (incl. §16 vocabulary, §6 P1–P5)

Production reality
    main-branch source, Cargo dependency graph, actual public APIs

Evidence (never authority)
    qianqian-audio-api playback test-local evidence (tests/playback_temporal_traces/,
    no longer in production src/), specs/playback/*,
    specs/realtime-publication/, architecture evidence records
    (pcm-contract-a0.md, direct-pcm-flow.md, realtime-view-publication.md,
    realtime-publication-lifetime-decision.md, component-boundary-a0.md)

Derived projections (summarize/route/visualize; define nothing)
    README.md / CONTEXT.md / AGENTS.md summaries / docs/README.md /
    this overview / registry.yml / website / diagrams
```

Current architecture work must not silently upgrade experimental playback evidence back into authority. If production code and a normative authority differ, follow the authority resolution rule in `../../AGENTS.md` ("Authority resolution").
