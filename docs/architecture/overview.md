# Architecture overview

This document is the repository-local semantic overview for Qianqian Architecture v2.

Generic composition semantics live in `composition-kernel.md` and the K0 design/implementation authorities. Playback architecture is currently **reopened**; `../adr/ADR-PBK-001.md` is a new PROPOSED foundation, not an accepted production state machine.

The current architecture intentionally separates composition, execution/control, committed facts, and realtime data flow.

---

# Normative constitution

> The normative Playback Foundations constitution lives in [`../adr/ADR-PBK-001.md`](../adr/ADR-PBK-001.md) §1–§2. This overview explains; it does not carry a second normative copy.

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

The generic Base Kernel K0 is implemented/current.

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

> **Command != Fact.**

---

# Fact Plane

A Fact is published only after its truth has been established by its designated semantic authority.

Key frozen points (normative text in `../adr/ADR-PBK-001.md` §2.3):

```text
semantic commit = the producing authority considers the fact established
                  (not ACID/durability/fsync/device completion by default)
commit first -> Fact publication
one designated semantic authority per semantic fact type
mechanism evidence does not directly publish another authority's fact
Projection is derived visibility; a control decision must not use a
Projection as its correctness authority
```

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

Per quantum, the realtime path must not re-enter Context resolution, generic Fact/Event fan-out, plugin dispatch, Reconcile, or filesystem/network/UI machinery; it operates on already-bound/published state. The normative forbidden list lives in `../adr/ADR-PBK-001.md` §2.4.

> **Fact != hot data.**

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

> **Dependency topology != realtime processing topology.**

Never derive DSP/realtime order from Fiber mount order, registration order, HashMap iteration or capability discovery order.

---

# Graph publication boundary

Control side builds and validates the next realtime graph/view; realtime execution loads the currently published view and processes the audio quantum directly.

Normative contracts (in `../adr/ADR-PBK-001.md` §6):

```text
A realtime reader observes one coherent published realtime view
(N or N+1, never half of each).
Any resource that realtime execution may still dereference must remain
valid until no realtime execution or queued reference can dereference it.
```

The publication mechanism (RCU / epoch / double buffering / Arc snapshot / lease / hazard / other) is intentionally unfrozen.

---

# Realtime lifetime safety

The withdrawal ordering, reader-quiescence steps and the exact resource/fiber lifetime binding are normative in `../adr/ADR-PBK-001.md` §6. This is the first clearly identified cross-plane lifetime invariant of the reset architecture.

---

# Parameter vs topology changes

Do not force every cheap runtime parameter update through full Plugin Reconcile.

Examples likely to be parameter/control updates:

```text
volume
filter coefficient
threshold
balance
```

Examples that may require topology/provider rebuild/publication:

```text
insert/remove processing stage
replace decoder mechanism
replace output mechanism
change branch/merge structure
```

The exact boundary remains an Audio Runtime research result.

---

# Current playback status

Playback-specific state-machine authority has been deliberately reopened.

The repository still contains prior experimental concepts such as:

```text
MusicKernel
TransportKernel
TrackSession
DecodeSession
Generation
Active / Prepared
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

The first likely new candidate is a narrow publication/release interleaving if executable evidence demonstrates it:

```text
old graph references A
A withdrawal begins
new graph excludes A
old reader still uses A
A final release
```

A model is justified only after the collision is concrete.

---

# Current workspace

```text
qianqian-core
qianqian-kernel
qianqian-runtime
qianqian-headless
```

The Base Kernel K0 is current.

Playback-specific code is research evidence and may be changed/removed without compatibility obligation while the reset ADR remains PROPOSED.

---

# Authority chain

```text
Generic composition
    docs/architecture/composition-kernel-0-design.md
    docs/architecture/composition-kernel-0-implementation-adr.md
    docs/architecture/composition-kernel.md

Playback foundations
    docs/adr/ADR-PBK-001.md — PROPOSED / REOPENED
    docs/architecture/overview.md

Experimental playback evidence
    qianqian-core playback code/tests
    specs/playback/*

Historical evidence
    git history / explicit historical refs
```

Current architecture work must not silently upgrade experimental playback evidence back into authority.
