# CONTEXT.md

This file carries stable vocabulary and the current repository mental model. It is a derived status/index, not a substitute for current code, contracts, ADRs, or task-specific evidence.

Current normative authorities:

```text
Playback foundations        -> docs/adr/ADR-PBK-001.md
Current vocabulary / Plugin-Fiber taxonomy /
static playback composition -> docs/adr/ADR-PBK-002.md
K0 semantics                -> docs/architecture/composition-kernel-0-design.md
K0 representation           -> docs/architecture/composition-kernel-0-implementation-adr.md
SongCore binding architecture -> docs/architecture/songcore-binding-architecture.md
                             (canonical C ABI description: native/include/songcore.h)
```

Current execution roadmap:

```text
Phase F v2                  -> Issue #119 PHASE-F-HEADLESS-CONTROL-1
```

Issue #119 is the Phase-F **product execution roadmap**, derived from the normative authorities and current production reality; it is not a replacement for the ADRs. The current playback execution-architecture campaign is #198/#201: Stage 4 #207/#214 is merged, Stage 5 #208 is the explicitness pass, C1 #205 and C2 #213 are merged, #209 accepted D1–D6 (`PASS_ARCHITECTURE_ACCEPTED`), and #210 froze the playback execution architecture (subject `a225d524`, bound to validation contract #212, PLAYBACK-EXECUTION-VALIDATION-v1). Stage 8 validation is #211, not started. Start at [the execution model](docs/architecture/playback-execution-model.md) (§1 authority routing; §12 minimum-core 20/20 traceability). Issue #141 is the closed post-#139/#140 Phase-F reality-audit record whose REV.3 conclusions are reflected in #119 and this file. Issue #138 records the architecture-corrective basis; it remains design input, not authority.

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

F2 seam representation (reality-gate-2 verdict): the application-facing episode seam is a **public Playback Session handle** (one handle == one playback episode; surface `request_stop` / `request_pause` / `request_resume` / `request_seek` / `observe` / `wait_terminal` — pause/resume added by the frozen D14.7 F3 amendment, seek by the frozen D14.5 F5 amendment). `SessionCompletion`/resolver is an **internal replaceable realization** behind that seam, not the application API; terminal settlement runs on the Playback Session authority-owned execution/teardown path (D14.3), never consumer-triggered.

The following are still OPEN:

```text
Playing / Starting / Stopping semantics
(Paused is a D14.7-derived Projection, not a transport state;
 Resumed is NOT a product projection — removed by the D14.7
 AUTHORITY-CORRECTIVE (disengagement evidence cannot prove a viable
 render leg remains): resume is Command-only, disengagement is
 Mechanism Evidence; the rest of the transport enum is not earned)
device switch / replaceable render binding
PlaybackControl topology
PlaybackFacts publication topology
multi-session / preload / gapless
Realtime Audio Runtime representation
```

Closed by the same 2026-09-18 F6-AUTHORITY-PROMOTION-1 (D14.6/D14.9
amendment): playlist/queue authority (application navigation state,
commit-on-activation — no new authority) and next/previous policy
(inert boundaries by default, Open replaces the playlist). The
2026-09-19 U2 amendment in D14.6 supersedes that closure's Phase-F v1
scope clauses only: ordering/repeat are App-owned product policy and a
committed D11 `Completed` Fact triggers at most one automatic
transition through the same Open replacement. No ownership moved, no
PlaylistPlugin/navigation Fact was created, and failed-candidate
auto-skip is still forbidden.

---

# Phase-F current status

Issue #119 is the current Phase-F v2 execution roadmap.

```text
F0 CLI shell / grammar          DONE / CLOSED
F1 Stop                         DONE / CLOSED
F2 Observable read side         IMPLEMENTED / REALITY GATE CLOSED
    F2-READ-SIDE-SEAM-REALITY-GATE-2 verdict:
        A. SessionCompletion directly as application seam   REJECT
        B. episode-scoped public Playback Session
           handle/wrapper                                   SELECT
        C. split handles / generic state/fact infrastructure NOT EARNED
    representation:
        episode-scoped public Playback Session handle/wrapper
    current realization:
        public seam request_stop/observe/wait_terminal;
        D14.3 authority-owned settlement;
        SessionCompletion/resolver is crate-internal and replaceable
F3 Pause / Resume               CLOSED after authority corrective
                                (feat/f3-pause-resume-1, PR #150:
                                D14.7 mechanism A render-loop gate
                                behind the episode seam).
                                Paused = earned application-facing
                                Projection (engagement +
                                current-engagement tail-quiescence
                                evidence); Resumed = NOT a product
                                Projection (AUTHORITY-CORRECTIVE:
                                disengagement evidence cannot prove a
                                viable render leg remains —
                                never-activated/open-abort
                                counterexample); resume = Command
                                only; Disengaged = Mechanism Evidence
                                only; evidence
                                experiments/f3-pause-mechanism
F4 Position / Duration          GATE MERGED (research/f4-timeline-gate-1,
                                F4-GATE-CORRECTIVE-1: D14.8 freezes the
                                propositions — Position = episode-local
                                device-consumed Projection, one monotone
                                mechanism-evidence sample published by the
                                render leg from its own handed-off and tail
                                readings and read with one pure load;
                                freezes exactly at the D14.7 tail-quiescence
                                evidence, never at command time; Duration =
                                optional source-scoped Mechanism Evidence,
                                unknown stays unknown; evidence
                                experiments/f4-timeline-gate).
                                IMPLEMENTED on feat/f4-position-duration-1
                                (PR #152) behind the episode seam: one
                                session-owned cell, observation fields
                                position/duration, headless read-side
                                timeline — present in current production
F5 Seek                         GATE MERGED (PR #154, D14.5:
                                refusal-first frozen ordering, three-class
                                provider outcome, park + natural-drain
                                output mechanism, commit boundary,
                                same-cell position rebase; evidence
                                experiments/f5-seek-discontinuity +
                                specs/f5-seek-discontinuity).
                                IMPLEMENTED on feat/f5-seek-1 behind the
                                episode seam: worker-owned cutover
                                protocol, public request_seek command,
                                TUI Left/Right ±5 s; corrective-1 (fresh
                                adversarial review): unified loop-top
                                gate (single steady acquisition, paused
                                rebase mid-park), seek/worker-exit
                                linearization, per-cut evidence reset,
                                mutations M1–M11; corrective-3/4 (human
                                rulings, conformance only): worker waits
                                read the data plane terminal, cut decision
                                is one atomic three-valued sample, a FAILED
                                tail observation escapes the park bounded —
                                present in current production
F6 Open                         AUTHORITY PROMOTED (architecture/
                                f6-open-authority-promotion-1:
                                F6-AUTHORITY-PROMOTION-1 amends D14.6 —
                                probe-before-destruction + whole-episode-
                                composition replacement + commit formula +
                                failure-clean start, frozen on S-PROBE
                                GREEN physical evidence, evidence
                                experiments/f6-source-probe; acoustic
                                human-ear item recorded UNAVAILABLE)
                                IMPLEMENTED (feat/f6-open-1, PR #159;
                                C7 matrix + 4/4 mutation gate +
                                OPEN_SMOKE_GREEN x3 physical; 3 review
                                rounds to 0/0/0/0)
Next / Previous                 AUTHORITY CLOSED as application
                                navigation state (commit-on-activation,
                                inert boundaries by default; same
                                amendment — its Phase-F v1 "no auto-next"
                                scope clause is superseded by the U2
                                amendment below) — IMPLEMENTED (feat/
                                navigation-1, PR #160; Stage D matrix +
                                O6 physical walk x3; 3 review rounds to
                                0/0/0/0)
F7 Volume                       owner/semantics PROMOTED (D14.9);
                                apply placement GROUNDED by V-PROBE
                                (experiments/v-probe, PR #161:
                                V_PROBE_GREEN x3, no reopen fired) and
                                IMPLEMENTED (feat/volume-1: OutputLevel
                                cell in RenderRequest, loop-top apply,
                                request_output_level seam command,
                                App-owned desired 0..=100 step 5; O7
                                physical row; narrow ADR grounding
                                amendment carried)
Historical transport-v1        Stage A dogfood PASS (runs E/F/G:
integrated audit + TUI          26/26 TUI + 7/7 machine), Stage B
v1 closure                      integrated audit PASS_WITH_MINOR ->
                                corrective landed (teardown-gate
                                precondition oracle, ownership docs;
                                A16 scope note; U-1 retained as
                                NON-BLOCKING KNOWN ANOMALY), Stage C
                                headless-TUI v1 closure code+scenarios
                                LANDED (branch hardening/
                                transport-dogfood-tui-v1-closure-1;
                                final physical regression 31/31 TUI +
                                7/7 machine at clean closure HEAD).
                                At that closure checkpoint, verdict
                                QIANQIAN_PHASE_F_TRANSPORT_V1_CLOSED
                                awaited human review; this records that
                                historical checkpoint, not current
                                #198/#201 execution-campaign status.
U1 Windows TUI launch /         CLOSED (Issue #166 U1, PR #167, merged
   folder input                  2026-09-19): canonical qianqian.exe
                                product binary (qianqian-headless kept
                                as the historical regression target), a
                                truthful no-argument TUI (no fabricated
                                episode), deterministic file/folder
                                expansion into ONE ordered candidate
                                list, commit-riding temporary-list
                                seeding, bounded scan diagnostics, and
                                corrected host/presentation exit
                                semantics. Physical evidence:
                                research/transport-dogfood/evidence/
                                ENV-TUI-RUN4/RUN5 + U1-CORRECTIVE.
U2 Playlist / order / repeat    AMENDED + IMPLEMENTED (Issue #166 U2,
   / EOF policy / TUI controls  branch feat/166-playlist-usability-
                                closure): D14.6's Phase-F v1 "no repeat /
                                no shuffle / no EOF auto-next" clauses
                                are reclassified as SCOPE freeze and
                                superseded by one invariant — ordering
                                and repeat policy belong to the App, the
                                Playback Session establishes terminal
                                Facts and nothing else. App-owned
                                temporary playlist (order Sequential/
                                Shuffle, repeat Off/All/One, stable
                                per-cycle permutation, selection cursor,
                                Completed-only exactly-once EOF reaction
                                through the SAME Open replacement, no
                                failed-candidate auto-skip), TUI playlist
                                pane + frozen keymap, Shift+arrows ∓30 s
                                and `G` exact seek over the SAME
                                request_seek Command, `play --shuffle`
                                startup grammar. Still forbidden: any
                                playlist/navigation Plugin or Fact, and
                                the persistence family (no playlist file,
                                M3U, media DB, library, history).
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

# Current Phase-F execution routing (derived projection)

Accepted protocol meanings live in PBK-002; their cross-protocol execution
reading lives in [the execution model](docs/architecture/playback-execution-model.md).
The older Pause/Seek/Open hypotheses are superseded by the narrow accepted
amendments below; this router does not reopen them.

| Concern | Current authority and realization |
| --- | --- |
| Pause / Resume | PBK-002 D14.7: Session routes a render-loop gate; Paused is derived from current engagement/tail evidence, Resume is Command only. Backpressure is an effect of the park. |
| Position / Duration | D14.8: pure Position load, monotone between committed discontinuities; optional source-duration evidence. Neither authorizes control or measures acoustics. |
| Seek | D14.5: provider refusal before invalidation; RefusedUnchanged preserves processed remainder/history; Applied purges then waits for the parked/tail-quiesced cut and release consumption. Same resources, no P1–P5 overlap. |
| Open / Next / Previous | D14.6: probe-before-destruction; old-side clearance from absent root or Discharged; fresh whole-composition establishment through C1. App owns navigation/repeat; Completed-only automatic transition, no failed-candidate auto-skip. |
| Volume | D14.9: App-owned desired stream factor, routed to Output; distinct from processing Gain. Windows loop-top placement is grounded by V-PROBE. |
| DSP | D14.11 + DSP product model §7.3: episode-owned processing on the decode worker; Desired recording, worker Accepted, fresh-block Applied and sample-driven Settled remain distinct. |
| Machine input | Execution-model D3/D5 (accepted #209, frozen #210), realized by C2: EOF is normal closure; Spawn/Read/caught Panic are host failures. Effect admission, acknowledgement and seal do not terminate stdin. |

Device switch/replaceable render binding, broader transport vocabulary,
multi-session/preload/gapless and specialized realtime-runtime representation
remain OPEN. Any future overlap must earn the minimum P1–P5 mechanism.
Historical Issue #40 remains counterexample/mechanism evidence only, not a
current cutover contract.

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
output_plugin()          -> ComponentSpec   (owns the WASAPI Host Render
                                             Backend; ADR-PBK-003)
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
