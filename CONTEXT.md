# CONTEXT.md

This file carries stable vocabulary and the current repository mental model. It is a derived status/index, not a substitute for current code, contracts, ADRs, or task-specific evidence.

Current normative authorities:

```text
Playback foundations        -> docs/adr/ADR-PBK-001.md
Current vocabulary / Plugin-Fiber taxonomy /
static playback composition -> docs/adr/ADR-PBK-002.md
K0 semantics                -> docs/architecture/composition-kernel-0-design.md
K0 representation           -> docs/architecture/composition-kernel-0-implementation-adr.md
```

Current execution roadmap:

```text
Phase F v2                  -> Issue #119 PHASE-F-HEADLESS-CONTROL-1
```

Issue #119 is the current **execution roadmap**, derived from the normative authorities and current production reality; it is not a replacement for the ADRs. Issue #141 is the closed post-#139/#140 Phase-F reality-audit record whose REV.3 conclusions are reflected in #119 and this file. Issue #138 records the architecture-corrective basis; it remains design input, not authority.

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

The D11 designation attaches to the Playback Session semantic role for one playback episode; the episode-scoped Plugin/Fiber is its current composition realization, not a frozen identity equation. In particular, **Fiber Active is composition/lifecycle evidence, not automatically the semantic-authority start boundary for a future Open contract**; that boundary must be earned with the episode construction/config semantics.

D11 terminal variants:

```text
Completed
Stopped
Failed
```

Exact resolver representation/precedence remains current implementation detail unless changing it changes the external D11 propositions.

F2 seam representation (reality-gate-2 verdict): the application-facing episode seam is a **public Playback Session handle** (one handle == one playback episode; minimal surface `request_stop` / `observe` / `wait_terminal`). `SessionCompletion`/resolver is an **internal replaceable realization** behind that seam, not the application API; terminal settlement runs on the Playback Session authority-owned execution/teardown path (D14.3), never consumer-triggered.

The following are still OPEN:

```text
Playing / Starting / Paused / Stopping semantics
position / duration authority
seek acceptance / discontinuity / commit semantics
open/session construction + config + replacement semantics
playlist/queue authority
next / previous
volume authority
device switch / replaceable render binding
PlaybackControl topology
PlaybackFacts publication topology
multi-session / preload / gapless
Realtime Audio Runtime representation
```

---

# Phase-F current status

Issue #119 is the current Phase-F v2 execution roadmap.

```text
F0 CLI shell / grammar          DONE / CLOSED
F1 Stop                         DONE / CLOSED
F2 Observable read side         REALITY GATE CLOSED
    F2-READ-SIDE-SEAM-REALITY-GATE-2 verdict:
        A. SessionCompletion directly as application seam   REJECT
        B. episode-scoped public Playback Session
           handle/wrapper                                   SELECT
        C. split handles / generic state/fact infrastructure NOT EARNED
    representation:
        episode-scoped public Playback Session handle/wrapper
    next authorized step:
        QIANQIAN-F2-TRUTHFUL-READ-SIDE-IMPLEMENTATION-2
        (public seam request_stop/observe/wait_terminal;
         D14.3 authority-owned settlement;
         SessionCompletion/resolver becomes crate-internal
         replaceable realization)
F3 Pause / Resume               GATE EARNED (D14.7 mechanism +
                                establishment freeze incl.
                                CORRECTIVE-1: Paused/Resumed =
                                non-authoritative Projections gated on
                                engagement + output-tail-quiescence
                                evidence; evidence
                                experiments/f3-pause-mechanism);
                                IMPLEMENTATION NOT STARTED
F4 Position / Duration          GOAL KEPT / AUTHORITY + COUNTERS OPEN
F5 Seek                         REDESIGNED AROUND DISCONTINUITY PROTOCOL
F6 Open                         CONFIG-MECHANISM-OPEN
Next / Previous                 AFTER OPEN; separate navigation step
F7 Volume                       GOAL KEPT / authority mechanism to earn
F8 Devices / Device switch      split; switch mechanism still OPEN
```

PR #137 is historical implementation/test material only. Do not rebase it or treat it as current design authority.

---

# Phase-F simplification rule

For every new playback feature:

```text
1. Can existing Playback Session Plugin semantics express it?
2. Can existing Decode/Output capability seams realize the mechanism?
3. Can subordinate resources be changed/quiesced/replaced without a new Plugin?
4. Is this an in-place discontinuity on the same resources, or a replacement of resources/worlds?
5. If resources/worlds are replaced, can an old RT reader still dereference the retired world while the new one is current?
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

Two different realtime problems must not be conflated:

```text
same-resource discontinuity
    e.g. a seek that keeps the same decoder/edge/render stream
    may need an explicit decode/edge/output cutover protocol
    but does not by itself earn PBK-001 P1–P5

old/new RT resource overlap
    old world still dereferenceable while new world becomes current
    -> PBK-001 P1–P5 is triggered
```

Seek/open/device-switch do not trigger Realtime Audio Runtime merely because their names sound dangerous. The trigger is concrete overlap/reclamation pressure.

---

# Current Phase-F execution hypotheses (derived projection; #119 is the roadmap)

These are compact execution hypotheses, not new normative authority.

```text
Pause / Resume
    same Playback Session Plugin is still the default ownership hypothesis
    mechanism is OPEN
    compare at least:
      - explicit render-loop pause gate before WASAPI GetBuffer
      - IAudioClient::Stop / Start
      - another smaller proven mechanism
    backpressure is an effect of not consuming; it is not itself the pause mechanism

Seek
    first try same Playback Session Plugin and same resource set
    correctness requires a three-layer SEEK DISCONTINUITY PROTOCOL:

      decode-side cutover
        worker command -> serialization point -> discard OLD staging -> decoder reposition

      edge-side cutover
        invalidate/flush OLD buffered PCM without corrupting terminal monotonicity

      output-side physical cutover
        prove already-submitted OLD PCM cannot remain audible after seek commit
        exact mechanism remains OPEN

    output-side candidates remain OPEN; do not copy the historical #40 Stop()+Reset()
    implementation as current contract

    distinguish:
      seek command accepted
      logical decoder/edge landing
      audible cutover committed

    Loom/native tests cover worker/edge races; a Windows physical-output gate is required
    for audible cutover because device buffering is outside Loom

    if the same resources remain and no old/new resource world overlaps, P1–P5 is NOT earned

Open(source B)
    K0 staged teardown/mount ordering exists
    fresh episode configuration mechanism does NOT yet exist
    current ComponentSpec capture(file, completion) is not an Open contract

    config candidates remain OPEN (per-instance config / App-owned config source /
    multiple concrete definitions / another minimal construction mechanism)

    Fiber Active is not automatically the semantic-authority start boundary
    failure rollback, transactional preflight and zero-gap are separate product/mechanism questions
    zero-gap is the clearest current candidate for real old/new RT overlap

Next / Previous
    only after Open exists
    default minimal hypothesis is App-owned ordered selection + Open(selected)
    PlaylistPlugin must be independently earned by D13

Volume
    parameter/control routed through an existing mechanism service is the default hypothesis
    do not conflate playback volume with PCM gain / DSP / ReplayGain
    no feature-shaped VolumePlugin

Devices / Device switch
    enumeration is an Output-side mechanism query
    switching is not implemented by the current RenderStream ownership shape
    first earn either:
      - a session-owned replaceable render binding with coherent drain semantics, or
      - whole-episode replacement
    seamless switch may create real old/new render overlap and therefore may trigger P1–P5
```

Historical Issue #40 may be used only as a **counterexample/mechanism evidence** that machine/logical seek landing can differ from audible device-buffer truth. Its historical implementation is not current architecture authority.

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

Current Phase-F review has earned **no new TLA+ obligation**. Future F5/F6/F8 designs must be re-evaluated if they introduce an independently legal temporal collision not covered by existing models/tests.

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

Current production facts important to the post-#139 architecture:

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
