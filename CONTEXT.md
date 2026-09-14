# CONTEXT.md

This file carries stable vocabulary and the current repository mental model. It is a derived status/index, not a substitute for current code, contracts, ADRs, or task-specific evidence.

Current authorities:

```text
Playback foundations        -> docs/adr/ADR-PBK-001.md
Current vocabulary / Plugin-Fiber taxonomy /
static playback composition -> docs/adr/ADR-PBK-002.md
K0 semantics                -> docs/architecture/composition-kernel-0-design.md
K0 representation           -> docs/architecture/composition-kernel-0-implementation-adr.md
```

Issue #138 records the current architecture corrective basis. It is design input, not authority.

---

# Current vocabulary

| Term | Meaning |
|---|---|
| Qianqian / 千千·现代 | Local-first, lightweight, cross-platform music player and composable-runtime architecture testbed. |
| Architecture v2 | Boundary-first architecture built on a generic K0 Plugin/Fiber composition runtime plus domain-specific semantics and realtime data paths. |
| Composition Kernel (K0) | Domain-agnostic runtime that manages Plugin/Fiber existence, reachability, dependency, composition Effects and desired→running composition. It does not transport PCM or own playback semantics. |
| Plugin | **K0-managed independently composable lifecycle/behavior unit.** It may require/provide Capabilities, may provide none, and may be episode-scoped or long-lived. Admission requires the PBK-002 D13 invariant — "K0 composes it" is evidence, not the admission reason. |
| ComponentSpec | Current K0 Rust/formal representation of a Plugin definition: requires/provides + bounded activation + teardown verdict. It is not a second product-architecture taxonomy. |
| Fiber | One live mounted Plugin instance/episode with identity, committed dependency view, Effects/provenance and lifecycle state. |
| Context | Capability namespace/dependency view visible to a Fiber. Not payload bus, event store or global state bag. |
| Capability | Typed composition-visible dependency/reachability contract. |
| Service | Executable object reached through a Capability. |
| Effect | K0 composition-lifecycle reversible mutation/provenance with teardown inverse. Domain-resource internals remain outside K0 data. |
| Reconcile | Moves the running Fiber graph toward desired composition while respecting dependency/lifecycle invariants. |
| Qianqian App | Composition root/bootstrap outside the composition it operates. Installs definitions/desires composition and initiates top-level shutdown; not playback authority. |
| Decode Plugin | Long-lived mechanism Plugin providing decode capability/service. |
| Output Plugin | Long-lived mechanism Plugin providing output capability/service. |
| Playback Session Plugin | Episode-scoped Plugin; current ownership/lifecycle envelope of one playback episode. Requires Decode/Output capabilities and owns one episode's endpoint/worker/PCM edge/render relation/completion. |
| PCM Data Plane | Pre-bound episode-owned payload path: decoder → PCM edge → output. It is not a Plugin and does not re-enter K0 per block. |
| Command | Intent/request. Not proof that the requested outcome happened. |
| Fact | Truth established by its designated semantic authority. |
| Mechanism Evidence | Mechanism/provider observation that may feed a semantic decision; not automatically Fact. |
| Projection | Derived read model/visibility. Never semantic authority or correctness basis for control/lifetime legality. |
| Realtime Audio Runtime | Reserved future specialized runtime for genuinely-earned RT view publication/retirement/quiescence/lifetime mechanisms. Representation remains OPEN. |
| Realtime Execution View | Coherent pre-bound state consumed directly by realtime execution when such a mechanism is earned. |
| Reader Quiescence | No active or queued realtime reader can still dereference the relevant retired resource through any reachable view/generation. |
| Everything is a Plugin | Every **independently K0-composed lifecycle/behavior unit** uses the common Plugin/Fiber protocol. It does **not** mean every object, feature, payload, buffer, endpoint, command, Fact or AudioNode is a Plugin. |

---

# Core mental model

```text
                         APP / UI
                            │
                    command / projection
                            │
                            ▼
╔═══════════════════════════════════════════════╗
║                  K0                          ║
║       Plugin/Fiber composition runtime       ║
║                                               ║
║  Decode Plugin ── Decode Capability ─┐        ║
║                                     │        ║
║  Output Plugin ── Output Capability ─┼──►     ║
║                                     │        ║
║                    Playback Session Plugin   ║
║                                     │        ║
║                    owns episode resources    ║
╚═════════════════════════════════════╪═════════╝
                                      │
================ DATA PLANE ==========╪=============
                                      ▼
                   decoder → PCM edge → output
```

The relationships are:

```text
K0
    manages Plugin/Fiber lifecycle and dependency

Plugin
    independently composed behavior/lifecycle owner

Capability / Service
    optional dependency seam between Plugins

Plugin domain code
    owns subordinate resources / workers / endpoints

PCM
    payload flowing over already-bound data edges
```

K0 composes the owners; it does not carry their PCM payload.

---

# Four reasoning lenses

The four-lens foundation remains unchanged:

```text
Composition Plane
    Plugin/Fiber existence, Context/Capability reachability,
    composition Effects, withdrawal/reconcile

Execution / Control Plane
    Commands / workflow / Capability-Service calls

Fact Plane
    semantic commit -> Fact -> projection/observers/persistence/UI

Realtime Data Plane
    pre-bound hot data such as PCM
```

These are concern boundaries, not four mandatory runtime subsystems.

---

# Plugin / resource rule

A Plugin is not the data flow itself.

A Plugin may:

```text
require Capabilities
provide Capabilities/Services (optional)
own domain resources/effects
register control/fact observers
participate in realtime setup
```

But subordinate resources do not become Plugins merely because they have lifetimes:

```text
DecodedPcmStream != Plugin
PcmEdge          != Plugin
RenderStream     != Plugin
PcmBlock         != Plugin
Command          != Plugin
Fact             != Plugin
```

Promote a thing to Plugin only when it earns the PBK-002 D13 admission invariant — in particular, only when an existing Plugin **cannot** own it without losing composition correctness or lifecycle ordering.

---

# K0 ownership firewall

K0 remains generic. It knows:

```text
Fiber lifecycle
Capability reachability / committed bindings
composition Effect provenance + inverse
teardown Discharge verdict
```

It does **not** know decoder handles, PCM buffers, WASAPI objects, playback position, playlist meaning or seek semantics.

When the Playback Session Plugin owns a decoder endpoint, worker, edge and render stream, that ownership is **lifecycle/teardown ownership** of episode-scoped handles (allocation mechanisms and implementation internals stay with the Decode/Output provider Plugins — PBK-002 D6). It is domain semantics implemented inside activation/effect/teardown closures; it does not widen K0's kernel data model.

---

# Playback Session mental model

Current production classification:

```text
Playback Session Plugin
    lifetime: one current playback episode
    requires: Decode + Output capabilities
    owns lifecycle/teardown of:
        decode endpoint / worker / PCM edge / render relation / completion
    designated semantic authority:
        one episode terminal outcome (D11)
```

The D11 designation attaches to the Playback Session semantic role for one playback episode; the episode-scoped Plugin/Fiber is its current composition realization, not a frozen identity equation.

D11 terminal variants:

```text
Completed
Stopped
Failed
```

Exact resolver representation/precedence remains current implementation detail unless changing it changes the external D11 propositions.

The following are still OPEN:

```text
Playing / Starting / Paused / Stopping semantics
position / duration authority
seek semantics
open/session replacement mechanism
playlist/queue authority
next / previous
volume authority
device switch
PlaybackControl topology
PlaybackFacts publication topology
multi-session / preload / gapless
Realtime Audio Runtime representation
```

---

# Phase-F simplification rule

For every new playback feature:

```text
1. Can existing Playback Session Plugin semantics express it?
2. Can existing Decode/Output capability seams realize the mechanism?
3. Can subordinate resources be changed/quiesced/replaced without a new Plugin?
4. Does any old/new RT resource actually overlap while readers can still dereference the old world?
```

Prefer the smallest model.

Do **not** pre-create:

```text
Window
Generation
Preempted
Seeking
Opening
Stopping
Buffering
Nexting
```

If seek/open/device-switch genuinely creates old/new RT worlds with overlapping dereferenceability, then PBK-001 P1–P5 is triggered and the minimum view/generation/quiescence mechanism may be earned.

---

# Likely future semantic decomposition (NOT YET AUTHORITY)

Issue #138 records the current reduction hypotheses:

```text
Pause/Resume
    same Playback Session Plugin; execution behavior changes

Seek
    first try same Session Plugin:
    quiesce → discard stale PCM → decoder seek → resume

Open(source B)
    first try Session Plugin replacement through K0 lifecycle
    exact construction/config mechanism remains OPEN

Next / Previous
    playlist/queue selection + Open(selected)
    a future PlaylistPlugin must be independently earned

Volume
    parameter/control routed through an existing mechanism service
    no feature-shaped VolumePlugin
```

These are execution hypotheses for future Phase-F work, not accepted semantics yet.

---

# Realtime firewall

Every PCM block/callback must avoid:

```text
Context lookup
Capability resolution
Fiber Reconcile
generic EventBus fan-out
generic Plugin dispatch
filesystem/network/UI round trip
unbounded allocation/blocking
```

Capability resolution and Plugin/Fiber lifecycle happen on setup/control boundaries; the bound data plane executes directly.

---

# Formalization mental model

Ask first:

> Which independently legal states/events can interleave and collide into an illegal state?

No concrete collision -> prefer Rust types/ownership/tests/static checks. Normative policy: `ADR-PBK-001.md` §13; verification guardrails: `AGENTS.md` "Verification authority boundary".

The one normative research/implementation ladder lives in `ADR-PBK-001.md` §12 (composition reality → minimal PCM contract → direct data flow → publication/reader overlap → real decoder → real output → only then playback semantics).

Realtime publication/lifetime evidence already established PBK-001 P1–P5. Do not import old PlaybackTemporal nouns merely because a future feature resembles an old model.

---

# Current code status

Production workspace relevant to current playback includes:

```text
qianqian-composition
qianqian-audio-api
qianqian-app
qianqian-playback
qianqian-decode-songcore
qianqian-output-wasapi
qianqian-headless
```

Current production facts important to Issue #138:

```text
songcore_decode_plugin() -> ComponentSpec
wasapi_output_plugin()   -> ComponentSpec
playback_session_spec()  -> ComponentSpec

all are mounted by K0 as Fibers
```

This is why the old “Playback Session is Component but not Plugin” taxonomy was corrected (Issue #138 basis; PBK-002 D12) without a production runtime redesign.

---

# Historical evidence

The following remain historical/experimental evidence unless explicitly re-earned:

```text
MusicKernel
TransportKernel
TrackSession
DecodeSession
Active / Prepared
Generation
Dual Window
Physical Fence
```

Useful refs:

```text
pre-rust-v2
playback-reference-v1
```

Use history only when the current task needs a reproducer, prior counterexample, or testing technique.
