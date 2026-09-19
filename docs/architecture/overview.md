# Architecture overview

> **Derived projection/router only.** Normative Playback Foundations: [`../adr/ADR-PBK-001.md`](../adr/ADR-PBK-001.md). Current vocabulary / Plugin-Fiber taxonomy / static playback composition: [`../adr/ADR-PBK-002.md`](../adr/ADR-PBK-002.md). K0 semantics: [`composition-kernel-0-design.md`](composition-kernel-0-design.md); representation: [`composition-kernel-0-implementation-adr.md`](composition-kernel-0-implementation-adr.md).

Qianqian Architecture v2 is a composable Plugin/Fiber runtime with a strict firewall between composition/control and realtime PCM payload flow.

---

# Current architecture at a glance

```text
                         Qianqian App
                              │
                     desired composition
                              ▼
╔══════════════════════════════════════════════════╗
║              Composition Kernel K0              ║
║          Plugin/Fiber composition runtime       ║
║                                                  ║
║  Decode Plugin ── Decode Capability ──┐          ║
║                                      │          ║
║  Output Plugin ── Output Capability ──┼──►       ║
║                                      │          ║
║                     Playback Session Plugin     ║
║                                      │          ║
║                     owns episode resources      ║
╚══════════════════════════════════════╪═══════════╝
                                       │
================= DATA PLANE ==========╪==============
                                       ▼
                 decoder → PCM edge → output → device
```

Key sentence:

> **K0 composes the owners; Plugin code binds/owns domain resources; PCM flows directly through already-bound resources.**

---

# Everything is a Plugin — scoped meaning

Current canonical meaning (PBK-002 D4/D12; admission invariant D13):

> **Every independently K0-composed lifecycle/behavior unit is a Plugin; one live mounted instance is a Fiber.**

This does not mean every object is a Plugin. Independent composition is itself earned: if an existing Plugin can own the candidate without losing composition correctness or lifecycle ordering, the candidate stays an owned resource/effect (D13).

```text
Plugin
    independently mounted/activated/invalidated/withdrawn by K0

ComponentSpec
    current K0 representation/formal definition of a Plugin

Fiber
    one live Plugin instance

Capability / Service
    optional typed dependency seam between Plugins

owned resource/effect
    subordinate runtime object whose lifecycle is controlled by Plugin code

payload
    data such as PCM; never routed through generic Plugin dispatch per block
```

Examples:

```text
Decode Plugin            YES
Output Plugin            YES
Playback Session Plugin  YES (episode-scoped)

DecodedPcmStream         NO — owned endpoint
PcmEdge                  NO — owned resource/data edge
RenderStream             NO — owned mechanism resource
PcmBlock                 NO — payload
Pause / Seek / Next      NO — commands/product semantics, not Plugin identities
```

A future Playlist or Processing boundary may earn Plugin identity only when it needs independent K0 composition/lifecycle identity.

---

# Composition Plane

K0 primitives remain:

```text
Context
Capability
Fiber
Effect
Reconcile
```

K0 decides:

```text
which Plugins/Fibers exist
which capabilities are reachable
which provider satisfies a requirement
which committed dependency view a Fiber holds
how composition effects/provenance unwind
how desired composition becomes running composition
```

K0 must remain ignorant of:

```text
PCM contents
SongCore handles
WASAPI objects
playback position
seek meaning
playlist policy
UI product state
```

`ComponentSpec` is a K0 representation/formal term, not a peer architecture taxonomy beside Plugin.

---

# Plugin ownership is not kernel knowledge

A Plugin may semantically own domain resources while K0 remains generic.

Current Playback Session Plugin owns:

```text
decode endpoint
decode worker
bounded PCM edge
render stream relationship
SessionCompletion
```

Those resources are acquired/cleaned by Plugin activation/effect/teardown code. K0 sees only the generic contract it already owns:

```text
Fiber lifecycle
Capability bindings
composition Effect provenance/inverse
teardown Discharge verdict
```

K0 does not gain fields for decoder/render/PCM internals.

---

# Four reasoning lenses

The accepted foundations still separate four concerns. They are **reasoning lenses**, not necessarily four standalone runtimes:

```text
Composition
    Plugin/Fiber existence + Context/Capability + Effect/Reconcile

Execution / Control
    Command / workflow / Capability-Service call

Fact
    semantic authority -> commit -> Fact -> projection/observers

Realtime Data
    pre-bound execution state + direct PCM flow
```

No universal bus combines them.

---

# Current Playback Session role

Playback Session is now canonically an **episode-scoped Plugin**.

```text
Playback Session Plugin
    requires Decode Capability
    requires Output Capability

    activation:
        open episode-specific decode endpoint
        create/bind bounded PCM edge
        open render stream
        start decode worker

    lifetime ownership (teardown responsibility; allocation/implementation
    stays with the Decode/Output provider Plugins — PBK-002 D6):
        endpoint / worker / edge / render relation / completion

    semantic authority:
        one episode terminal outcome (D11)
```

The D11 designation attaches to the Playback Session semantic role for one playback episode; the episode-scoped Plugin/Fiber is its current composition realization.

D11 terminal outcomes:

```text
Completed
Stopped
Failed
```

Decode/Output mechanism evidence informs that decision but does not establish the terminal semantic Fact itself.

---

# Realtime Data Plane

Current path:

```text
DecodedPcmStream
      ↓
decode worker
      ↓
bounded PcmEdge
      ↓
RenderPcmInput
      ↓
output mechanism / device
```

Per PCM block/callback, forbidden:

```text
Context lookup
Capability resolution
Fiber Reconcile
generic Plugin dispatch
generic Fact/Event fan-out
filesystem/network/UI round trip
unbounded allocation/blocking
```

Composition establishes reachability/lifetime on setup/control boundaries. The data plane then executes directly.

---

# Dependency graph != realtime graph

Composition graph:

```text
who requires whom
who provides what
who withdraws before whom
```

Realtime graph/data path:

```text
which concrete bound object consumes the next PCM quantum
where processing branches/merges
which RT-visible resource is current
```

Never infer realtime order from registration/mount/hash iteration order. Cheap parameter updates (volume, filter coefficients) are control-path updates, not topology updates; inserting/removing stages or replacing decoder/output mechanisms may require composition/provider changes. The exact boundary is research-open; see `../adr/ADR-PBK-001.md` §7.

---

# Realtime publication/lifetime gate

PBK-001 §6 P1–P5 remains unchanged. The one normative research/implementation ladder lives in `../adr/ADR-PBK-001.md` §12 (Phase D validates candidate mechanisms against P1–P5); model-level evidence and mechanism comparison: `docs/architecture/realtime-publication-lifetime-decision.md` + `specs/realtime-publication/`.

Do not create Window/Generation/view-swap machinery in advance. Trigger the specialized Realtime Audio Runtime only when a real feature produces a concrete collision such as:

```text
new execution world becomes current
+
old world remains reachable by active/queued realtime readers
+
release legality depends on reader quiescence
```

Then earn the minimum publication/retirement/quiescence mechanism.

---

# Phase-F reduction model

Before adding runtime nouns for pause/seek/open/next/volume, try the current architecture first:

```text
existing Plugin/Fiber lifecycle
+
existing Capability/Service seams
+
Plugin-owned domain resources
+
small orthogonal semantic facts/control state
```

Current hypotheses (Issue #138; not yet accepted feature semantics):

```text
Pause/Resume
    same Playback Session Plugin; alter execution behavior

Seek
    first try: quiesce -> discard stale PCM -> decoder seek -> resume same Session
    escalate only if old/new RT worlds genuinely overlap

Open(source B)
    first try: replace Playback Session Plugin through K0 lifecycle
    exact construction/config mechanism remains OPEN

Next / Previous
    playlist/queue selection + Open(selected)

Volume
    parameter/control through an existing mechanism service
```

This keeps the model small and avoids a giant `Playing/Seeking/Opening/Preempted/...` FSM unless product semantics genuinely require those states.

---

# App / UI waterline

Qianqian App is outside the composition it operates. It installs definitions, chooses desired composition and initiates top-level shutdown. It is not playback authority and does not pump PCM.

Future UI should see only application-facing commands and read-side facts/projections, not:

```text
CompositionKernel
ComponentSpec
Fiber
PcmEdge
SongCore handle
WASAPI object
```

A UI widget is not a Plugin merely because it invokes a command. A UI adapter/controller may earn Plugin identity only if it needs independent K0 lifecycle/composition identity.

---

# Current accepted / open decisions

Accepted:

```text
K0 generic composition semantics
Plugin/Fiber taxonomy (PBK-002 D1/D4/D12/D13)
Decode Plugin / Output Plugin
Playback Session Plugin episode ownership
PCM composition/data-plane firewall
D11 episode terminal-outcome authority
PBK-001 P1–P5 realtime lifetime contract
```

Still OPEN:

```text
pause/resume semantics
position/duration authority (D14.8 PROPOSED on the F4 gate branch:
    episode-local device-consumed Position Projection — one monotone
    mechanism-evidence sample published by the render leg, read as one
    pure load — + optional Duration evidence; never a Fact; pending
    human review)
seek mechanism/authority
open/session replacement representation
playlist/queue authority
    (CLOSED: application navigation state — the 2026-09-18
     F6-AUTHORITY-PROMOTION-1 amendment in ADR-PBK-002 D14.6; no new
     authority, no PlaylistPlugin/navigation Fact)
next/previous
    (CLOSED: application navigation through the same Open replacement;
     ordering/repeat policy is App-owned product policy per the
     2026-09-19 U2 amendment in ADR-PBK-002 D14.6)
volume
device switch
Processing Plugin
multi-session / preload / gapless
PlaybackControl
PlaybackFacts publication topology
Realtime Audio Runtime representation
```

---

# Historical evidence

Old nouns remain evidence only:

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

Do not preserve or resurrect them merely because a future feature resembles an old design. Re-earn concepts from current constraints and code reality.
