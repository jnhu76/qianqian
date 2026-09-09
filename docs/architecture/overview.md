# Architecture overview

This document is the repository-local semantic overview for Qianqian Architecture v2.

Generic composition semantics live in `composition-kernel.md` and the K0 design/implementation authorities. Playback architecture is currently **reopened**; `../adr/ADR-PBK-001.md` is a new PROPOSED foundation, not an accepted production state machine.

The current architecture intentionally separates composition, execution/control, committed facts, and realtime data flow.

---

# Architecture constitution

> **Base Kernel is domain-agnostic.**
>
> **Plugin/Fiber identity belongs to composition/lifecycle, not per-payload routing.**
>
> **Commands ask; Facts report committed truth.**
>
> **Committed Facts may fan out; hot PCM does not use generic Fact/Event dispatch.**
>
> **Realtime processing consumes pre-bound published graph/view state.**
>
> **Projection is derived visibility, not authority.**

---

# Four-plane model

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
                     │ authoritative commit / graph build
          ┌──────────┴──────────┐
          ▼                     ▼
┌──────────────────────┐  ┌──────────────────────────┐
│      Fact Plane      │  │   Realtime Data Plane   │
│ committed Fact       │  │ published graph/view    │
│ projections          │  │ PCM direct flow         │
│ persistence/UI       │  │ callbacks/device        │
└──────────────────────┘  └──────────────────────────┘
```

These are cooperating mechanisms, not one universal bus.

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

Execution begins with intent.

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

A Fact is published only after its truth has been committed by the responsible authority/mechanism.

```text
validate / decide
      ↓
authoritative commit
      ↓
committed Fact
      ↓
fan-out
  ├─ projection
  ├─ persistence
  ├─ UI
  ├─ telemetry
  └─ reactions / new commands
```

Observers see the committed fact; they do not serially mutate one event until it becomes truth.

> **commit first -> publish fact**

### Projection

Projection folds committed facts and/or authoritative snapshots into a read model.

It is useful for UI, diagnostics, history and telemetry, but it is not a writer.

> **Projection != authority.**

Qianqian has not yet chosen repository-wide Event Sourcing/CQRS. Durability, replay authority and append-only logging remain open research questions.

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

Per block/callback, the realtime path must not perform:

```text
Context lookup
Capability resolution
Fiber Reconcile
generic Fact/Event fan-out
plugin registry traversal
filesystem/network I/O
UI/JS/managed-runtime round trip
unbounded allocation/blocking
```

The realtime path operates on already-bound/published state.

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

Control side:

```text
composition/configuration/parameter decision
        ↓
build + validate next realtime graph/view
        ↓
publish at an RT-safe boundary
```

Realtime side:

```text
load current published graph/view
        ↓
process audio quantum directly
```

The publication mechanism is intentionally unfrozen.

Candidates may include:

```text
RCU
epoch
double buffering
Arc snapshot
lease/hazard-style schemes
other bounded handoff
```

---

# Realtime lifetime safety

A resource/provider referenced by a published realtime graph/view must remain alive while any old reader can still dereference it.

```text
withdraw/replace A
      ↓
A excluded from future graph build
      ↓
publish graph/view without A
      ↓
stop new readers entering old view
      ↓
old readers/queued refs quiesce
      ↓
release old graph refs
      ↓
final release A
```

This is the first clearly identified cross-plane lifetime invariant of the reset architecture.

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

```text
1. K0 composition reality
2. minimal PCM contract
3. direct Source -> processing -> Sink data flow
4. realtime graph publication / replacement / reader overlap
5. real decoder mechanism
6. real output mechanism
7. only then playback semantics such as seek/stop/track/session
```

This order is intentionally mechanism-first at the foundation and semantics-later at the player level.

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
