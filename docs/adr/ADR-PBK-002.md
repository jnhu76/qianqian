# ADR-PBK-002 — Canonical Architecture Vocabulary and Real Playback Composition

| Field | Value |
|---|---|
| Status | **ACCEPTED** |
| Date | 2026-09-12 |
| Accepted after | PR #118 corrective adversarial review: taxonomy, historical provenance, authority routing, vocabulary-gate scope |
| Supersedes | — |
| Amends | ADR-PBK-001 current vocabulary and earned playback composition decisions; PBK-001 foundations / Fact contract / P1–P5 remain unchanged |
| Amended | 2026-09-14 — §17 D11 episode terminal outcome semantic authority; 2026-09-14 — §18 D12 Everything-is-a-Plugin taxonomy corrective (Issue #138); 2026-09-14 — §19 D13 Plugin admission invariant (PR #139); 2026-09-15 — §17 D11 terminal-settlement ownership corrective + §20 D14 Phase-F playback semantic execution guard (formal evidence PR #142, reality audit Issue #141); 2026-09-16 — §20 D14.7 pause/resume mechanism + establishment freeze (F3-GATE, evidence `experiments/f3-pause-mechanism/`); 2026-09-16 — §20 D14.7 pause establishment corrective: render engagement ≠ audible pause; Paused gated on output-tail quiescence evidence and demarcated as a non-authoritative Projection (F3-GATE-CORRECTIVE-1, same evidence crate); 2026-09-17 — §20 D14.7 AUTHORITY-CORRECTIVE: `Resumed` removed as an application-facing Projection — disengagement evidence cannot prove a viable render leg remains (never-activated/open-abort counterexample); resume is Command only, disengagement stays Mechanism Evidence (PR #150); 2026-09-17 — §20 D14.8 Position/Duration propositions frozen: episode-local device-consumed Position Projection (a monotone mechanism-evidence sample published by the render leg, read as one pure load) + optional source-scoped Duration Mechanism Evidence (F4-GATE, evidence `experiments/f4-timeline-gate/`); 2026-09-17 — §20 D14.8 F4-GATE-CORRECTIVE-1 (pre-merge review): the reader-side monotone clamp and the two-cell reader pair are REMOVED — monotonicity is owned by the writer-side publication, which is what keeps `observe()` a pure read; the "± one in-flight block" accuracy statement is withdrawn as a concurrency correctness bound (freshness is not a bound); the IAudioClock byte-rate wording is narrowed to the exercised endpoint; 2026-09-17 — §20 D14.8 implementation note (F4-IMPLEMENTATION-1, PR #152): the chosen representation and the terminal/duration conformance reading are recorded in D14.8 — representation only, no proposition changed; 2026-09-17 — §20 D14.8 F4-IMPLEMENTATION-CORRECTIVE-1 (same PR, fresh review): the position observation gate must also withdraw on a recorded activation failure — a raising activation can leave a published sample behind (the render mechanism opens before the decode-worker spawn; the open-abort leg publishes from its park slice), so "never-activated fabricates no Position" is not satisfied by the terminal-Fact condition alone (conformance, no new state); 2026-09-17 — §20 D14.5 seek-discontinuity mechanism + policy freeze (F5-GATE, evidence `experiments/f5-seek-discontinuity/` + `specs/f5-seek-discontinuity/`): refusal-first frozen ordering, park + natural drain output mechanism, same-cell position rebase, P1–P5 untriggered; 2026-09-18 — §20 D14.5 F5-GATE-CORRECTIVE-1 (pre-merge review): refusal made zero-content-loss (an in-flight staging block observed mid-write is preserved and finished exactly), and the song_seek provider outcome is frozen three-class — RefusedUnchanged (provably pre-mutation only, the INVALID_ARGUMENT class) / Applied / MutatedThenFailed (routes through the ordinary D11 decode-failure path; generic SEEK_ERROR is NOT a refusal because the ABI also returns it after a destructive reposition + decoder flush); F5 implementation still blocked; 2026-09-18 — §20 D14.5 implementation note (F5-SEEK-IMPLEMENTATION-1, branch `feat/f5-seek-1`): the chosen representation (three-class provider outcome, bounded-slice write + non-terminal edge invalidate, gate-consumed release payload, cell rebase as the one legal backward step) is recorded in D14.5 — representation only, no proposition changed; amended same day (F5-SEEK-IMPLEMENTATION-CORRECTIVE-1, fresh adversarial review): the loop-top parks are unified into one gate operation — realizing the realtime-cost row literally (no new lock acquisition) and withdrawing the first note's extra-acquisition differential — the committed rebase lands mid-park while a paused leg STAYS paused, seek acceptance is one atomic hold linearized against the worker's exit (an accepted seek can never outlive its resolver), and the per-cut evidence latches reset at each acceptance; 2026-09-18 — §20 D14.5 F5-SEEK-IMPLEMENTATION-CORRECTIVE-2 (same branch, authority-conformance review): the provider classification is narrowed to the frozen refusal set — `SONG_ERR_NOT_OPEN` had been promoted to `RefusedUnchanged` and now takes the conservative default (`MutatedThenFailed`), because a not-opened handle certifies no usable old cursor; the gate report's phase-0 lumping is corrected in place (marked corrective) and the map is pinned executably (positive + negative controls); 2026-09-18 — §20 D14.5 F5-SEEK-IMPLEMENTATION-CORRECTIVE-3 (same branch, pre-merge review of the implementation PR): two conformance gaps in the applied-cut path closed without new failure classes — (a) the protocol's post-apply waits now read the frozen failure policy's own "data plane not Open" episode-ending class on the worker's path (teardown stopped the plane before the worker join, so a permanently non-quiescing tail wedged the join); (b) the cutover decision is ONE atomic three-valued sample (Committed / Aborted / Pending) so a transient park-evidence gap is Pending and can never release a purged cut's leg without its rebase ("the only exits from an applied cut are the commit or an episode ending"); plus the `--machine` seek token reader fails closed instead of panicking on unrepresentable float spellings; the implementation mutation gate grows to M1–M10 (10/10 counterexample-witnessed, M1/M5 re-pinned to the corrected shape); 2026-09-18 — §20 D14.5 F5-SEEK-IMPLEMENTATION-CORRECTIVE-4 (same branch, human review of corrective-3): the R1 liveness class has two halves and only the worker's was closed — the parked leg's own tail probe still answered one bool, so a tail observation that itself FAILED (an invalidated endpoint's `GetCurrentPadding` error) was masked as "not quiesced yet" and the park could never reach the loop-level abort that produces the terminal the worker's escape needs; the probe now answers `TailProbeOutcome` (Pending / Quiesced / Failed) and a Failed observation ends the park bounded — no quiescence publishes for it, the release-payload consumption discipline still holds on the leg's path, and `ParkOutcome::TailProbeFailed` hands the decision to the mechanism's EXISTING device-failure path (conformance, no amendment, no new failure class); the implementation mutation gate grows to M1–M11 (11/11 counterexample-witnessed, M11 = the Failed arms collapsed back into the Pending treatment); 2026-09-18 — §20 D14.6 F6-AUTHORITY-PROMOTION-1: the F6 CONFIG-MECHANISM-OPEN and probe/concurrency disclosures are decided on physical evidence (evidence `experiments/f6-source-probe/` PR #157; design source the merged #155 transport-closure package) — probe-before-destruction frozen (an invalid Open candidate never kills live playback; the probe is one public stateless decode-provider SourceFacts query, owns no RT resource, never an episode, P1–P5 untriggered; S-PROBE GREEN ×3 physical runs with the acoustic human-ear witness recorded UNAVAILABLE and the green made explicitly conditional on it), replacement mechanism = whole-episode-composition replacement at the App boundary (fresh QianqianApp per episode; the file is a constructor argument; no config channel / registry / hot component replacement), replacement commit = old-side clear (no current composition root or authoritative Discharged outcome — never forged) ∧ authoritative activation result Activated (covering provider activation failure / unresolved dependency / session activation failure; never absence-of-diagnostic, never a CompositionSnapshot read — PBK-001 §2.3), the start operation is failure-clean (Discharged ⇒ ActivationFailedClean with the attempted root disposed; cleanup TeardownViolated ⇒ FAIL-STOP retaining the root), D1 gains the App realization note and D5 provider lifetime becomes episode-scoped; the same amendment closes §14 playlist/queue authority as application navigation state (commit-on-activation, inert boundaries, no auto-next/auto-skip, Open replaces the playlist — no new authority, no PlaylistPlugin) and freezes the D14.9 volume owner/semantics (App-owned desired 0..=100, stream-local realization, IAudioStreamVolume candidate; the physical realtime apply placement stays pending V-PROBE and remains a D14.10 stop-list item); 2026-09-19 — §20 D14.9 VOLUME-IMPLEMENTATION-1 implementation grounding (evidence `experiments/v-probe/` PR #161): the physical facts the amendment deferred are measured on a real endpoint — stream-factor isolation (V1a same-process, V1b other-process), factor independence in BOTH writing directions (V2a player→mixer, V2b mixer→stream with audible change expected and the ear witness recorded UNAVAILABLE), lifecycle persistence across client Stop/Start (V3), the loop-top apply measured bounded and non-perturbing ON the submitting thread (V4: median ~0.26 ms, p99 ≤ 0.51 ms over 200 routed changes, every apply successful, iteration cadence held at the device period, position clock advancing and monotone; designed coalescing of sub-cadence routing recorded), and typed failure-signal existence (V5: 0x80070057 E_INVALIDARG, 0x88890001 AUDCLNT_E_NOT_INITIALIZED; the device-loss class 0x88890004 not physically triggered — its D14.9 routing into the existing device-failure policy is implementation-gated and reviewed in the implementation PR). NO reopen condition fired: the mechanism selection stands. The candidate apply placement (once at stream open before first meaningful submission; re-apply at the render loop top when the routed value changed, on the submitting thread, one relaxed load + compare; never inside the quantum) is grounded by this evidence and LEAVES the D14.10 stop list; the representation decisions (OutputLevel cell, RenderRequest field, the episode seam's idempotent `request_output_level` command) are recorded as representation only; 2026-09-19 — cross-amendment provenance routing: ADR-PBK-003 (ACCEPTED via PR #163) freezes the stable Output Plugin / pluggable Host Render Backend boundary (Output is the Plugin; the concrete backend is an owned mechanism behind the backend-neutral `AudioOutput` contract) and amends this ADR's D5/D13 reading and the platform interpretation of D14.5/D14.7/D14.8/D14.9 exactly as recorded in ADR-PBK-003 (its §11 differential). ROUTING ONLY — no normative content is restated or duplicated here, and no playback semantic authority moves: D11 terminal ownership, D14.5 seek, D14.6 Open/replacement, D14.7 pause, D14.8 Position/Duration, the §14 navigation ruling, and D14.9 volume ownership are unchanged by it |
| Evidence | PR #117 FIRST_AUDIBLE_SLICE; current K0 / playback production reality audited in Issue #138 |

---

## 1. 为什么现在需要它？

PR #117 第一次实现了完整 production playback path。后续 F0/F1/F2 工作又证明：Playback Session 与 Decode/Output provider 都通过同一个 K0 `ComponentSpec → Fiber` substrate 运行，Playback Session 自身已经拥有 episode-scoped resources/effects 与 terminal semantic authority。

当前真实形状：

```text
Qianqian App
    ↓ desired composition
Composition Kernel (K0)
    ↓ manages Plugin/Fiber lifecycle
┌──────────────────────────────────────────┐
│ Decode Plugin                            │
│ Output Plugin                            │
│ Playback Session Plugin                  │
└──────────────────────────────────────────┘
              │ owns/binds episode resources
==============│================================
              ▼
DecodedPcmStream → decode worker → PcmEdge → RenderPcmInput → output
```

本 ADR 的职责：

1. canonicalize current role names；
2. 冻结已经由 production evidence 挣得的 playback composition boundaries；
3. 保持 PBK-001 的 command/fact/realtime firewalls；
4. 修正早先把 Plugin 误收窄为“long-lived capability provider”的 taxonomy drift。

---

## 2. Authority relation

```text
ADR-PBK-001
    playback foundations
    plane separation
    Command / Fact / Projection semantics
    realtime publication/reclamation P1–P5

ADR-PBK-002
    current canonical vocabulary
    current Plugin/Fiber taxonomy
    Plugin admission invariant (D13)
    first earned static playback composition boundaries
    D11 episode terminal-outcome authority designation
    D14 current Phase-F playback semantic execution guard

composition-kernel-0-design.md
    generic K0 semantic contract

composition-kernel-0-implementation-adr.md
    K0 representation / implementation decisions
```

PBK-002 governs **current architecture vocabulary and Qianqian-specific mapping onto K0**. In particular, current Plugin taxonomy 由本文 D1/D4/D12 定义、Plugin admission invariant 由 D13 定义；PBK-001 §3 已改为只保留 foundational plane 约束并路由到本文（其旧 “Plugin = 长期能力进入 composition protocol” 措辞仅存于 decision history 标注）。

这不改变 PBK-001 的 foundations、Fact contract、realtime data-plane firewall 或 P1–P5。

K0 authority 中的 `component` / `ComponentSpec` 仍可作为 formal/implementation term；它不再构成与 Plugin 平级的 product-architecture identity。

---

## 3. D1 — Canonical vocabulary

### Qianqian App

| | |
|---|---|
| 定义 | application composition root / bootstrap layer |
| 拥有 | bootstrap、Plugin definition admission、desired composition、top-level shutdown initiation |
| 不负责 | decode、PCM pumping、WASAPI render、playback semantics、realtime lifetime |

代码类型：`QianqianApp`；crate：`qianqian-app`。

App 在它操作的 composition 之外；它不是因为“Everything is a Plugin”就自动变成 Plugin。

Realization note (2026-09-18 F6-AUTHORITY-PROMOTION-1; D14.6): the
reference player's canonical realization is one process-level host that
sequentially owns multiple non-overlapping `QianqianApp` composition
roots — one per playback episode. Episodes never overlap; each
replacement constructs a fresh root after the old one is fully retired;
one root is never mutated into another live episode.

### Composition Kernel (K0)

| | |
|---|---|
| 定义 | domain-agnostic Plugin composition/lifecycle runtime |
| 拥有 | existence、reachability、Capability dependency、Fiber lifecycle、composition Effect provenance、desired → running composition |
| 不负责 | PCM transport、audio scheduling、playback timing、domain-resource internals、realtime execution |

代码类型：`CompositionKernel`；crate：`qianqian-composition`；短名：`K0`。

```text
Kernel == Composition Kernel (K0)
```

K0 不是 Plugin；它是 mount/manage Plugins/Fibers 的 runtime。

### Plugin

> **A K0-managed independently composable lifecycle/behavior unit — where independent composition is itself earned (admission invariant: D13).**

Plugin 的 architecture identity 来自它是否**按 D13 挣得**独立 composition unit 身份、并被 K0 mount / activate / invalidate / withdraw，而不是来自：

```text
crate
DLL
Rust trait
thread
feature name
audio-stage name
是否恰好 provide Capability
是否 long-lived
```

“被 K0 mount” 本身不是 admission 理由：**若一个现有 Plugin 可以在不损失 composition correctness 或 lifecycle ordering 的前提下完全拥有该候选，该候选必须保持为 owned resource/effect，而不是变成 Plugin**（D13）。

一个 Plugin：

```text
MAY require Capabilities
MAY provide Capabilities/Services
MAY provide none
MAY own domain resources/effects
MAY be episode-scoped or long-lived
```

`feature != Plugin` 仍然成立：Command、Fact、payload、buffer、endpoint、UI widget 不因存在或有生命周期就自动成为 Plugin。

### Component / ComponentSpec

`ComponentSpec` 是 current K0 representation/formal substrate：静态声明 requires/provides + bounded activation + teardown verdict，并由 K0 mount 成 Fiber。

Current architecture 不再把 `Component` 作为与 Plugin 平级的 product taxonomy。对 Qianqian current composition：

```text
architecture role: Plugin
current K0 representation: ComponentSpec
live instance: Fiber
```

K0 design 文档沿用 paper/formal 的 `component` 一词不意味着引入第二套 runtime identity。

### Fiber

> One live K0 instance/episode of a Plugin definition.

Fiber 持有 composition identity、committed dependency view、composition Effects/provenance 与 lifecycle state。

Plugin 的 domain resources 可以由该 Plugin 的 activation/teardown code 拥有，但 **domain-resource metadata 不因此成为 K0 kernel data**；K0 仍只观察其已冻结的 composition effects/provenance 与 teardown `Discharge` verdict。

### Capability

> composition-visible typed dependency/reachability identity.

Capability 是 Qianqian/K0 当前 canonical spelling；不需要为了贴近其它框架改名。

### Service

> executable object reached through a Capability.

因此：

```text
Plugin = lifecycle/composition unit
Capability = named dependency contract
Service = executable value reached through that contract
```

三者不是同义词。

### Effect / domain resource

K0 `Effect` 保持现有 frozen definition：composition-lifecycle reversible mutation/provenance + total inverse。

Decoder endpoint、worker、PCM edge、render stream 等 domain resources 由所属 Plugin 的 domain semantics 拥有；K0 不获得它们的内部字段或 domain choreography。它只通过现有 effect/disposer + component teardown verdict 管 lifecycle ordering。

### Audio API

crate：`qianqian-audio-api`。

定义 shared contracts/vocabulary 与 PCM seams，例如：

```text
PcmFormat
PcmDecode
DecodedPcmStream
AudioOutput
RenderStream
RenderPcmInput
DrainSignal
```

它不 pump PCM、不 schedule audio、不 own playback、不 resolve composition、不成为万能 runtime。

### Decode Plugin / Output Plugin

```text
Decode Plugin
    long-lived mechanism Plugin
    provides PcmDecodeCapability / PcmDecode service

Output Plugin
    long-lived mechanism Plugin
    provides AudioOutputCapability / AudioOutput service
```

“long-lived”是这两个具体 Plugin 的当前 lifetime property，不再是 Plugin admission requirement。

### Playback Session Plugin

> **One Playback Session Plugin is the current ownership/lifecycle unit of one playback episode.**

Current representation: `playback_session_spec(...) -> ComponentSpec`，live instance 由 K0 作为 Fiber 运行。

当前 episode-owned resources/relations（lifecycle/teardown ownership；allocation/implementation 留在 provider Plugin —— 见 D6 的 ownership 两义区分）：

```text
playback-specific decode endpoint / song_handle lifetime
decode worker
bounded PCM edge
render stream / output relation
SessionCompletion
EOF / stop orchestration
```

它 requires Decode/Output Capabilities，但当前不需要 provide 一个 Capability 才能成为 Plugin。

这不冻结 `one track == one Playback Session` 为永久 topology，也不预先冻结 seek/open/next/preload/gapless 的未来 representation。

### Realtime Audio Runtime

Canonical future noun：`Realtime Audio Runtime`。

状态：**RESERVED / NOT IMPLEMENTED**。它只在真实功能挣得 live view replacement / retirement / quiescence / RT-visible lifetime mechanism 时进入 production。

### PCM Data Plane

> pre-bound episode-owned payload path: decode → bounded PCM edge → render → device.

PCM Data Plane **不是 Plugin**，`PcmEdge` / `PcmBlock` 也不是 Plugin。它们是 Plugin-owned runtime resources/payload flow。

关键约束：per block/quantum 无 Capability resolve、Context lookup、Reconcile 或 generic Plugin dispatch。

---

## 4. D2 — Kernel reserved word

> In current Qianqian architecture, Kernel is reserved for the Composition Kernel (K0).

因此 current architecture 禁止重新引入：

```text
MusicKernel
TransportKernel
AudioKernel
PcmKernel
PlaybackKernel
RealtimeKernel
```

历史 evidence 中的历史名除外。Realtime 一侧 canonical noun 仍是 `Realtime Audio Runtime`，不是第二个 Kernel。

---

## 5. D3 — App is outside the composition it operates

```text
Qianqian App
        │ installs / desires / initiates shutdown
        ▼
Composition Kernel (K0)
        │ manages
        ▼
Plugins / Fibers
```

App 不自动成为 Plugin，也不负责 decode、render、PCM、playback timing 或 realtime execution。

不要定义 `App Plugin` 仅仅为了口号上的“everything”。

---

## 6. D4 — Everything is a Plugin / K0 mapping

冻结：

```text
K0
    manages Plugin definitions/instances through the common composition protocol

Plugin
    independently composable lifecycle/behavior unit

ComponentSpec
    current K0 representation/formal definition of that unit

Fiber
    live Plugin instance/episode

Capability/Service
    optional dependency seam supplied/consumed by Plugins
```

因此：

```text
Every current K0-composed product/runtime unit has Plugin identity.
Every live composed unit is represented by a Fiber.
A Plugin need not provide a Capability.
A Plugin need not be long-lived.
A Plugin is admitted only through the D13 invariant.
```

“K0 已经 compose 它” 是 admission 的 **evidence**，不是 admission 的**理由**。但以下推论仍然错误：

```text
every object == Plugin
every resource == Plugin
every feature == Plugin
every AudioNode == Plugin
every command/fact/payload == Plugin
whatever we choose to mount in K0 is thereby a Plugin
```

Subordinate resources stay resources when they do not need independent K0 composition identity/lifecycle（判定标准即 D13 invariant）。

---

## 7. D5 — Mechanism providers

基于 production evidence：

```text
Decode Plugin
    mechanism provider
    lifetime spans one playback episode (2026-09-18 F6 promotion:
    re-mounted per episode; was "spans playback episodes" before the
    whole-episode-composition replacement mechanism, D14.6)

Output Plugin
    mechanism provider
    lifetime spans one playback episode (same amendment)
```

它们不拥有 one current song / one current decode endpoint / one current render stream / one playback edge；这些 episode-scoped resources 属于 Playback Session Plugin。

---

## 8. D6 — Playback Session Plugin ownership

当前 static slice 冻结：

> Playback Session is an **episode-scoped Plugin** and the ownership envelope of one playback episode.

它与 Decode/Output Plugin 使用相同 K0 `ComponentSpec → Fiber` lifecycle substrate；当前区别在 dependency/ownership role，不在 runtime category。

**Why Playback Session passes the D13 admission invariant**（architecture reason，不是 “production 已经注册了 ComponentSpec”——那只是 evidence）：

```text
- it has independent desired-composition identity（desired entry 独立增删/替换）；
- it independently requires Decode + Output Capabilities；
- its activation / pending / withdrawal lifecycle is independently managed by K0
  （provider withdrawal degrades it as its own composition participant）；
- it owns episode-scoped teardown obligations（stop → join → release 排序跨
  decode/output 两个 provider 边界）；
- collapsing it into another existing Plugin would require re-implementing
  K0-level dependency/lifecycle ordering inside that Plugin。
```

**Ownership 的含义**（本节与 D1/D5 的 “owns” 均按此解读）：

```text
lifecycle / teardown ownership
    Playback Session Plugin 拥有 episode-scoped lifetime 与这些 handle/relation
    的 teardown 责任（activation 建立、inverse 释放、Discharge 汇报）。

allocation / implementation ownership
    仍属于 provider Plugin：Decode/Output Plugin 的 service 仍拥有 endpoint/stream
    的分配机制、实现内部、pool/factory 与 provider-global state。
```

因此 “Playback Session owns the decoder endpoint / render stream” 永远不读作 “Playback Session owns Decode/Output Plugin internals”。

本文不冻结：

```text
track lifetime
seek lifetime beyond D14's current same-episode contract
advanced open/replacement mechanisms beyond D14's no-overlap v1 semantic contract
next-track overlap/preload lifetime
preload lifetime
gapless lifetime
multi-session topology
session construction/config representation beyond D14's current Phase-F seam
```

特别是：当前 `ComponentSpec` 捕获 file/completion 的实现不自动成为未来 `open` 的永久 contract；D14 只冻结 Phase-F 当前最小 **old episode fully retires before new episode becomes live** 的 v1 语义约束。新 source/config 的送入机制已由 2026-09-18 F6-AUTHORITY-PROMOTION-1 裁决（whole-episode-composition replacement，见 D14.6 amendment）。

---

## 9. D7 — Audio API is contracts, not runtime

`qianqian-audio-api` 只定义 shared contracts/vocabulary。它不能演化成 PCM router、thread owner、playback engine、composition resolver 或万能 “core”。

---

## 10. D8 — Composition / data-plane firewall

```text
CONTROL / COMPOSITION PLANE

Composition Kernel (K0)
      ↓ mount/resolve once during setup
Plugins / Fibers
      ↓ bind episode resources/endpoints

================ FIREWALL ================

PCM DATA PLANE

DecodedPcmStream
      ↓
decode worker
      ↓
bounded PCM edge
      ↓
RenderPcmInput
      ↓
output mechanism
```

MUST NOT：

```text
per-block Capability resolve
per-block Context lookup
per-block Reconcile
PCM through Composition Kernel
generic Plugin dispatch per quantum
generic EventBus carrying PCM
```

> **K0 composes the owners; it does not transport their PCM payload.**

---

## 11. D9 — Realtime Audio Runtime reserved

`Realtime Audio Runtime` noun 保留，representation OPEN。

PBK-001 P1–P5 继续约束未来任何 live-view publication / retirement / quiescence / reclamation mechanism。本文不预先设计 ArcSwap、RCU、epochs、hazard pointers、queues、Window、Generation 或 graph API。

---

## 12. D10 — UI / control remains open

只冻结：

```text
feature/widget != automatically Plugin
```

禁止因为 UI 名字直接制造：

```text
PlayButtonPlugin
SeekBarPlugin
PausePlugin
NextPlugin
VolumePlugin
```

如果未来 UI adapter/controller 本身需要独立 K0 composition identity/lifecycle，它可以按同一 Plugin admission rule 挣得；本文不预判。

---

## 13. Architectural regressions forbidden

1. route PCM per block through Composition Kernel；
2. equate Plugin with crate / DLL / feature / thread；
3. require Plugin to provide a Capability merely to earn Plugin identity；
4. require Plugin to be long-lived merely to earn Plugin identity；
5. reintroduce a peer `Component` product taxonomy beside Plugin when `ComponentSpec` is only the K0 representation/formal term；
6. make subordinate endpoint/worker/buffer/payload a Plugin without independent K0 composition identity；
7. let Qianqian App become playback engine or ordinary Plugin by default；
8. move playback-session-scoped resources into Decode/Output providers without new ownership evidence；
9. introduce generic ToHost / FromHost / global EventBus as universal seam；
10. infer realtime execution topology directly from composition topology；
11. create a second architecture `Kernel` noun for PCM/realtime work；
12. let any mechanism provider, the App, K0, observer, projection, or external waiter establish episode terminal outcome outside D11；
13. broaden K0 Effect/kernel data to encode decoder/render/PCM domain internals merely because their owner is a Plugin；
14. admit a candidate as a Plugin when an existing Plugin can own it without losing composition correctness, lifecycle/dependency ordering, or independent replacement/withdrawal semantics（D13）；
15. add a second playback lifecycle (`Dead` / `Reclaimed` / `Retiring` / generation state machine, etc.) when Fiber lifecycle + D11 terminal Fact + ordinary resource ownership already express the requirement；
16. treat an `OPEN` Phase-F semantic/mechanism decision as coding-agent freedom rather than an authority gap（D14）。

---

## 14. Open decisions

```text
volume realtime apply placement / perturbation bounds (V-PROBE pending;
    the D14.9 candidate mechanism IAudioStreamVolume and its
    owner/semantics are frozen — this row is what remains)
device switch authority / replacement mechanism
format switch
Processing Plugin
SRC fallback
UI adapter Plugin membership
PlaybackControl publication/topology beyond D14's episode read/control seam
PlaybackFacts publication/topology
    (episode terminal outcome authority + commit ownership: DESIGNATED — §17 D11;
     all other playback fact kinds/publication topology remain OPEN)
multi-session topology
preload / gapless
Realtime Audio Runtime representation
P1–P5 concrete production mechanism
```

(No longer OPEN: position / duration authority — designated as the
D14.8 episode-local Position Projection and optional Duration evidence
by the 2026-09-17 F4-GATE amendment. The D14.8 representation and the
seek-epoch rebase rule's F5 mechanism remain governed by D14.8/D14.5.
No longer OPEN either: seek physical-output cutover mechanism beyond
D14's frozen stale-PCM invariant — frozen by the 2026-09-17 F5-GATE
amendment inside D14.5 (same-resource discontinuity protocol; park +
natural-drain output cut selected, Stop/Reset/Start rejected with
physical evidence); the F5 production implementation has since landed
(feat/f5-seek-1, PR #156, merged 2026-09-18). No
longer OPEN, by the 2026-09-18 F6-AUTHORITY-PROMOTION-1 amendment
(D14.6, evidence `experiments/f6-source-probe/` PR #157): the
open/session CONFIG mechanism — whole-episode-composition replacement
at the App boundary, probe-before-destruction frozen with S-PROBE
GREEN; playlist / queue authority — closed as application navigation
state with commit-on-activation (no new authority, no PlaylistPlugin);
next / previous navigation policy — inert boundaries, no auto-next/no
auto-skip, Open replaces the playlist (D14.6 amendment); volume
authority / mechanism — App-owned desired level, stream-local
realization, IAudioStreamVolume candidate frozen in D14.9 with the
physical RT placement deliberately left to V-PROBE, which stays OPEN
above.)

Reduction rule for all future phases:

> **Try the current Plugin/Fiber + owned-resource model first. Try ordinary ownership/RAII/join and a local operation invariant before adding any architecture noun. Add Window/Generation/new runtime state only after a concrete counterexample proves the simpler model cannot preserve correctness. Explanatory vocabulary is not architecture vocabulary.**

`AGENTS.md` owns the repository-wide abstraction-earning review procedure; this ADR owns only the Qianqian playback decisions below.

---

## 15. Evidence

Current production path：

```text
Qianqian App
    ↓
Composition Kernel (K0)
    ↓ manages
Decode Plugin + Output Plugin + Playback Session Plugin
    ↓ Playback Session binds/owns episode resources
kernel-free PCM data plane
    ↓
physical WASAPI device
```

Concrete reality:

```text
songcore_decode_plugin()  -> ComponentSpec
output_plugin()           -> ComponentSpec   (owns the WASAPI Host
                                              Render Backend; ADR-PBK-003)
playback_session_spec()   -> ComponentSpec

K0 mounts desired entries as Fibers.
Playback Session requires Decode/Output capabilities and registers the
relation/lifecycle cleanup for its episode resources.
```

This evidence supports the taxonomy correction without a production runtime redesign.

---

## 16. Vocabulary amendment / historical mapping

Current canonical mapping：

```text
historical Application/Composition Host
    → Qianqian App

Base Kernel / Base Composition Kernel / Composition Kernel
    → Composition Kernel (K0)

K0/paper "component" / Rust ComponentSpec
    → formal/representation term for a current architecture Plugin definition

historical shared contracts crate qianqian-core
    → qianqian-audio-api / Audio API

Kernel type
    → CompositionKernel

AppRuntime type
    → QianqianApp

Playback Session component (pre-corrective prose)
    → Playback Session Plugin
```

历史文档描述历史事实时可以保留旧 noun；current authority/current projections 使用本表。不要为了 grep-clean 改写历史 provenance。

ADR-PBK-002 is authoritative for current vocabulary, Plugin/Fiber taxonomy, the Plugin admission invariant (D13), earned static playback composition, D11 terminal-outcome authority/settlement contract, and D14 Phase-F semantic execution guard.

---

## 17. D11 — Episode terminal outcome semantic authority

> 2026-09-14 amendment (F2-TERMINAL-OUTCOME-AUTHORITY-ADR-1; evidence report: PR #135; roadmap: Issue #119)。
>
> 2026-09-15 settlement corrective: formal campaign PR #142 proved the previous text underspecified **who/when triggers semantic commit**. This amendment resolves that authority gap without importing the verifier's A/B/B′ representation as production taxonomy.
>
> D12 taxonomy corrective does not change this fact contract. It changes only the **current composition realization** of the designated semantic role.

### Decision

> **The designated semantic authority for episode terminal outcome is the Playback Session semantic role for one playback episode.**

```text
fact kind:            episode terminal outcome
subject scope:        one playback episode
authority role:       Playback Session semantic role for one playback episode
current variants:     Completed / Stopped / Failed
current realization:  SessionCompletion / resolver (replaceable Rust detail;
                      current consumer-triggered commit path is a known
                      production differential to be corrected by F2)
current composition:  episode-scoped Playback Session Plugin / Fiber (D1/D6)
```

Designation attaches to the **semantic role** — not to `resolve()` or any Rust type, and not to Fiber identity：

```text
semantic authority identity != Fiber identity by definition

current composition realization:
    one episode-scoped Playback Session Plugin/Fiber hosts the role (D6)。
    这是关于今天 realization 的陈述，不是冻结
    "one semantic episode authority == one Fiber forever"。
```

未来 topology 改变（如 multi-session siblings、其它 composition realization）不因此修订本 fact contract，只要每个 playback episode 在同一时刻保持恰好一个 designated terminal-outcome authority。

### Terminal settlement ownership

D11 现在同时冻结 **decision ownership** 与 **commit-progress ownership**：

```text
mechanism evidence arrives
        ↓
Playback Session semantic authority evaluates the terminal contract
        ↓
Playback Session semantic authority commits exactly one terminal outcome
        ↓
external wait/read observes the committed truth
```

对于一个已成功激活的 episode：

> **Once the terminal evidence set becomes decisive under the current terminal decision contract, the Playback Session semantic role MUST settle the terminal Fact on its own execution/teardown path. No external observer, projection, status query, `wait()`, or other consumer call may be required to create that Fact.**

> **Playback Session domain teardown MUST NOT report its episode work quiesced/discharged while decisive terminal evidence exists and the terminal outcome is still uncommitted.**

这不是“每个 episode 最终都会结束”的一般 liveness 承诺：decoder/device 可以永久不产生 terminal evidence。它只裁决 **证据已经足够以后，Fact 由谁负责落锤**。

Read-side / wait-side contract：

```text
observation/read:
    pure visibility only
    MUST NOT resolve or commit

wait-for-terminal:
    waits for authority-owned settlement
    MUST NOT be the semantic writer or the trigger required for settlement
```

具体 production mechanism 仍可由同步调用、session-owned control flow、callback/notification、teardown settlement point 等最小实现实现；D11 不要求独立 resolver thread，也不要求把 commit 与 evidence publication 合并成一个通用 runtime primitive。

### Decision-time stability / late command rule

Terminal classification 不能由一个任意晚到的 consumer 调用时刻重解释已经决定性的 mechanism history。

> **A command arriving after the terminal evidence set has already become decisive MUST NOT relabel the outcome selected from that decisive evidence.**

因此 `Stopped` 的 stop intent 必须在**不晚于 terminal evidence 首次足以判决该 episode 的时刻**已经被 Playback Session authority 记录。之后才到达的 stop intent 可以作为 command history/diagnostic 存在，但不得把已经决定性的 `Failed` / `Completed` 改写成 `Stopped`。

这条冻结的是 semantic classification boundary，不要求 production 新建 `Terminalization`, `Generation`, `Dead`, `Retiring` 等状态或类型；最小实现可以在最后一块决定性 evidence 到达时立即 settlement，或保存足以防止 late-command 重解释的最小 session-owned state。

### Current realization（非 normative）

```text
production code:  crates/qianqian-playback/src/completion.rs
                  （SessionCompletion 保存 eventual SessionOutcome；其 resolver
                  观察 decode_failure / worker_terminal / DrainVerdict /
                  stop_requested 并 memoize 一个 terminal outcome）
                  crates/qianqian-playback/src/session.rs（episode lifecycle）

known differential after PR #142 formal campaign:
    wait() / try_resolve_now() currently call resolve() and can therefore
    be required to create terminalOutcome. That consumer-triggered settlement
    is no longer the target contract after this amendment; F2 must move
    commit progress back under Playback Session-owned execution/teardown.

resolver 精确 precedence：保留在 production code / formal campaign evidence；
本文只冻结对外 terminal propositions、single-writer/immutability、settlement ownership
和 late-command stability。若改变 precedence 会改变这些外部语义命题本身，需回到
authority review。
```

本块明确标注 CURRENT REALIZATION：不是 frozen representation，不代表任何永久承诺。

### Terminal outcome propositions

```text
Completed:
    the episode reached decode EOF,
    the output mechanism reported its drain contract completed,
    and the Playback Session authority committed terminal completion.

    No physical-audibility claim is made.

Stopped:
    when the terminal evidence first became decisive, the episode had already
    recorded stop intent and no higher-precedence failure classification won.

    This is non-causal: it does not claim the user's stop caused the abort.
    A later stop command cannot relabel already-decisive failure/completion.

Failed:
    the episode is classified as terminal failure according to the
    Playback Session authority's decision contract.

    Failure class is selected by semantic precedence, not chronological
    first-failure; diagnostic stage text is not frozen.
```

一个 playback episode 至多有一个 terminal outcome；一旦 commit 即不可改写。这是 cardinality/immutability contract，不是一般 episode-liveness 承诺。

### Mechanism evidence firewall

```text
mechanism evidence
    ↓
Playback Session semantic decision
(current composition realization: episode-scoped
 Playback Session Plugin/Fiber)
    ↓
terminal outcome semantic commit
```

Decode Plugin / Output Plugin / PcmEdge / K0 都不是该 fact 的 designated authority。Evidence producers do not gain terminal authority merely because their publication makes the decision decisive.

### App / K0 / projection firewall

Qianqian App 不是 playback semantic authority；K0 lifecycle facts 不是 playback terminal facts；projection/read-side remains non-authoritative per PBK-001 §2.3。

`source_format` / `activation_failure` remain mechanism observations/diagnostics, not this fact authority。Activation failure before a live playback episode exists does not get upgraded into D11 `Failed` merely for status convenience.

### Still OPEN

```text
Playing / Starting / Paused / Stopping semantics
    (the transport-state semantics remain OPEN; the D14.7
     Paused Projection is NOT this item — it is
     non-authoritative derived visibility, not a transport lifecycle
     state; `Resumed` was removed from the projection set by the
     D14.7 AUTHORITY-CORRECTIVE)
seek product-state vocabulary / actual-landing authority beyond D14 minimum
    (no longer OPEN: the 2026-09-17 F5-GATE amendment in D14.5 froze
     the actual-landing authority — the decode provider's reported
     landing, relayed by the session as the Position rebase basis,
     unknown = withdraw the projection — and the product-state
     vocabulary: no new public seek state, no Seek completion Fact)
source identity / playlist authority / navigation policy
    (partially no longer OPEN: the 2026-09-18 F6-AUTHORITY-PROMOTION-1
     amendment in D14.6 closed playlist/queue authority — application
     navigation state, commit-on-activation, no new authority — and the
     next/previous navigation policy; source identity remains OPEN)
volume / device-switch authority
    (partially no longer OPEN: the same amendment froze the volume
     owner/semantics and the IAudioStreamVolume candidate in D14.9 —
     the physical realtime apply placement stays OPEN pending V-PROBE;
     device-switch authority remains OPEN)
PlaybackFacts publication topology beyond D14 read seam
PlaybackSnapshot
EpisodeId / Generation / Window
multi-session / preload / gapless
Realtime Audio Runtime representation
```

(No longer OPEN: position / duration authority — DESIGNATED by the
D14.8 amendment as the episode-local Position Projection plus optional
source-scoped Duration evidence. Neither is a transport semantic and
neither is a Fact; the transport enum above stays OPEN independently.)

---

## 18. D12 — Everything-is-a-Plugin taxonomy corrective

> 2026-09-14 corrective basis: Issue #138. This section records the correction to the earlier PBK-002 Component-vs-Plugin taxonomy; it does not alter K0's five primitives or PCM firewall.

### Differential

Old PBK-002 said:

```text
Plugin = boundary-justified, long-lived capability/lifecycle participant
Playback Session = Component, not Plugin
not every Fiber belongs to a Plugin
```

Current production reality instead shows Decode, Output and Playback Session all enter K0 through the same `ComponentSpec` substrate and run as Fibers. Playback Session already has independent desired identity/lifecycle, requires capabilities, owns episode-scoped resources/relations, and supplies the D11 semantic authority role.

The old distinction therefore encoded an unnecessary architecture taxonomy rather than a runtime invariant.

### Corrective decision

```text
K0
  ↓ manages
Plugin definition
  ↓ current representation: ComponentSpec
Fiber
  ↓ may require/provide
Capability / Service
  ↓ Plugin domain code owns
resources / effects / episode mechanisms
  ↓
PCM payload/data plane
```

“Everything is a Plugin” means **every independently K0-composed lifecycle/behavior unit** uses the common Plugin/Fiber composition protocol. It does not mean every object, resource, payload, command, fact, UI feature, or PCM stage must become a Plugin.

### K0 ownership firewall

This corrective does **not** expand K0's domain knowledge. `ComponentSpec` teardown closures and registered composition effects remain the boundary: K0 observes its frozen provenance/inverse/discharge semantics, while decoder/render/PCM domain choreography stays inside Plugin code.

### Simplification rule

Future Phase-F work must first attempt to express new behavior using:

```text
existing Plugin/Fiber lifecycle
existing Capability/Service seams
Plugin-owned domain resources
small orthogonal semantic facts/control state
ordinary Rust ownership / RAII / join
local operation invariants
```

Only a concrete counterexample may earn additional runtime concepts such as Window, Generation, extra Plugin boundaries, or a Realtime Audio Runtime mechanism. Explanatory words used during analysis do not automatically become architecture nouns.

---

## 19. D13 — Plugin admission invariant

> 2026-09-14 corrective-2 basis: PR #139 acceptance attack（M2）。D1/D4/D12 定义了 Plugin 是什么；本节定义**什么候选有资格成为 Plugin**，使 admission 不再是 "K0 compose 它，所以它是 Plugin" 的循环论证。

### Invariant

一个候选 earns Plugin identity **当且仅当以下三条同时成立**：

```text
1. its independent presence / absence / replacement
   is itself part of desired composition truth;

2. its lifecycle / dependency ordering must be
   coordinated independently by K0;

3. representing it as a subordinate resource/effect
   of an existing Plugin would lose required
   composition semantics, lifecycle correctness,
   independent replacement/withdrawal,
   or cross-boundary dependency ordering.
```

核心不变式（本节 MUST 规则）：

> **If an existing Plugin can fully own the candidate without losing composition-level correctness or lifecycle ordering, the candidate MUST remain an owned resource/effect, not become a Plugin.**

反之，"K0 已经 mount 它" / "它有 lifecycle" / "它 requires 一个 Capability" 都不是 admission 理由；它们只是 evidence。

### Negative oracle（review 时按此驳回）

以下候选默认**不**是 Plugin：

```text
SeekOperation
TrackOpenOperation
MetadataReadOperation
one-shot worker
temporary transaction
command handler
endpoint wrapper
PCM edge
buffer
short task
```

驳回理由**不是** "它们 short-lived"，而是：

```text
they do not require independent desired-composition identity
or independent K0 lifecycle/dependency ordering;
an existing Plugin can own them without loss of
composition correctness.
```

同时明确：**short-lived != automatically non-Plugin**。Lifetime 长度不是 admission 维度；episode-scoped Playback Session 依据 D6 的 justification 合法地是 Plugin。

### Admission oracle（current 候选判定）

```text
candidate         independent   independent    existing Plugin   Plugin?   reason
                  desired-      K0 lifecycle/  can own without
                  composition   dependency     correctness loss
                  truth?        ordering?
─────────────────────────────────────────────────────────────────────────────────────
SeekOperation     NO            NO             YES               NO        episode 内
                                                                                行为操作；由
                                                                                Session 拥有
MetadataRead      NO            NO             YES               NO        provider 内
Operation                                                                读操作
TrackOpen         NO            NO             YES               NO        open 的
Operation                                                                 representation
                                                                          仍受 D14
                                                                          no-overlap-v1
                                                                          约束
PcmEdge           NO            NO             YES               NO        payload 路径
                                                                                资源（D8）
worker thread     NO            NO             YES               NO        owned effect
decoder endpoint  NO            NO             YES               NO        provider 分配、
                                                                          session 持有
                                                                          lifetime（D6）
Playback Session  YES           YES            NO                YES       见 D6
Decode Plugin     YES           YES            NO                YES       mechanism
                                                                          provider（D5）
Output Plugin     YES           YES            NO                YES       mechanism
                                                                          provider（D5）
```

### Review procedure

新的 Plugin boundary 提案必须按本节 invariant 逐条回答（AGENTS.md 的 questionnaire 是本节的 review prompt）；任何一条不成立即驳回为 owned resource/effect。把 operation-shaped / payload-shaped / convenience-shaped 候选注册进 K0 而不通过本节论证，构成 §13#14 的 forbidden regression。

---

## 20. D14 — Phase-F minimal playback semantic execution guard

> 2026-09-15 amendment. Inputs: Issue #141 post-#139/post-#140 reality audit, PR #142 terminal-commit formal evidence, current production code, and D11 settlement corrective above.
>
> Purpose: remove coding-agent discretion from Phase-F semantic boundaries **without** inventing a second playback lifecycle or a generic playback runtime. This section freezes the smallest implementation shape that current evidence has earned and explicitly blocks everything else.

### D14.1 Current model: no second playback lifecycle

The current implementation model remains exactly:

```text
Qianqian App / application host
        ↓ desired composition
K0
        ↓
Playback Session Plugin / Fiber
        ↓ owns
session semantics + decoder endpoint + worker + PcmEdge + render relation
```

Do not add `PlaybackStateMachine`, `Dead`, `Retiring`, `Reclaimed`, `DataPlaneAuthority`, `TimelineSegment`, `Generation`, `Window`, global current-playback store, or another runtime layer merely to explain lifetime. Current responsibilities are already split by existing mechanisms:

```text
K0 Fiber lifecycle         = whether the Plugin instance exists
D11 terminal Fact          = how the episode semantically ended
Rust ownership/effects     = when owned resources are stopped/joined/dropped
operation invariant        = any extra correctness rule local to Seek/Open/etc.
```

A new noun requires AGENTS.md abstraction-earning evidence.

### D14.2 Episode-scoped control/read seam — semantic contract first

F2 SHALL expose its control/read surface through **one episode-scoped semantic seam**. The seam is:

```text
NOT a Plugin
NOT a Capability
NOT a K0 primitive
NOT a global current-playback store
NOT a second lifecycle owner
```

Required semantic surface:

```text
record stop intent      Command only; idempotent/monotone.

observe                 pure read; no resolve/commit/lifecycle side effect.

wait for terminal       pure blocking wait for the authority-owned committed
                        terminal Fact; it MUST NOT run the semantic resolver
                        or be required for commit progress.
```

The application host/bootstrap may retain this seam **alongside** `QianqianApp`; `QianqianApp` itself must not become the playback semantic owner or store/derive playback truth.

The observation surface SHALL keep truth classes explicit:

```text
terminal outcome        Fact / pending absence only

stop intent             Command state

source format           mechanism evidence, explicitly labeled as such

activation error        diagnostic, not D11 Failed
```

The terminal outcome values SHALL expose only the stable semantic variants:

```text
Completed
Stopped
Failed
```

Current resolver stage/message strings remain diagnostics and MUST NOT be frozen into the semantic terminal enum. A failure diagnostic may be exposed separately if F2 status needs it; prose/message text is not a stable semantic contract.

Product status MUST NOT infer playback truth from:

```text
FiberState
PcmEdge::buffered_frames
logs
WASAPI state
Decode endpoint state
```

`None` terminal outcome means only “no terminal Fact committed yet”; it is not `Playing`, `Starting`, `Paused`, or any fourth outcome.

**Representation is deliberately NOT frozen here.** The concrete Rust shape of this seam — for example a dedicated wrapper type (working names `PlaybackSessionHandle` / `PlaybackSessionObservation` / `EpisodeTerminalOutcome`) versus a trimmed session-owned `SessionCompletion` — is exactly the **F2-READ-SIDE-SEAM-REALITY-GATE** decision (Issue #119 F2 gate, options A/B/C; Issue #141 MINOR-1 disposition). This section does not close that gate: the gate remains F2's first step and picks the representation. Whatever names it produces are **replaceable representation, not semantic authority** — a semantics-preserving rename does not amend this ADR.

### D14.3 Terminal settlement implementation target

Current `SessionCompletion` / resolver remains a replaceable internal realization. F2 SHALL move consumer-visible access behind the D14.2 seam and SHALL make evidence-publisher/resolver mutation surfaces unreachable from ordinary application consumers (crate-private visibility is the current realization spelling, not the semantic requirement).

The target execution shape is **authority-owned structured settlement**:

```text
mechanism evidence producers
    publish only their evidence
        ↓
Playback Session-owned execution/teardown path
    evaluates D11 and commits exactly one terminal Fact
        ↓
observation / wait consume that Fact
```

Do not create an independent resolver Plugin, global fact bus, resolver thread, or K0 primitive merely to satisfy this shape. Use the smallest session-owned control flow that makes D11 true.

Verification-only needs MUST NOT leak mechanism fields back into the product seam. Existing race/precedence tests may move to crate-private unit tests or an explicit test-only support seam; `buffered_frames`, fake `decode_failed`, fake `worker_exited`, etc. do not become product status because tests need them.

### D14.4 Stop — frozen current semantics

`request_stop()` is a Command, not a Fact.

For an active episode:

```text
request_stop
    ↓ recorded stop intent
Playback Session-owned mechanism stops its owned work
    ↓ worker/output evidence
D11 settlement
    ↓
an aborted episode settles `Stopped` iff stop intent was already recorded by
the decisive-evidence boundary and no higher-precedence failure classification
wins
```

Rules:

- late stop after a committed or already-decisive `Completed`/`Failed` cannot relabel it；
- repeated stop is idempotent for terminal truth；
- current F1 stop-before-full-open behavior remains supported for a live activation path；
- activation failure remains diagnostic and is not upgraded into D11 `Failed` merely because a stop was also requested；
- no `Stopping` Fact/state is earned yet。

### D14.5 Seek — same episode, local discontinuity invariant

Current Phase-F Seek MUST remain an operation owned by the existing Playback Session Plugin. It does not create a new Plugin/Fiber and does not earn `Generation`, `TimelineSegment`, `Window`, or a second lifecycle.

Minimal semantic contract:

> **After seek cutover commits, PCM belonging to the pre-seek timeline must not later become post-seek audible output.**

Any implementation must account for every currently known stale-PCM reservoir:

```text
decode worker local staging
PcmEdge buffered PCM
already-submitted output/device buffer
```

The implementation protocol (decode-side serialization → old-staging discard →
decoder reposition → edge invalidate/flush → physical output cutover → landing
acknowledgement) is roadmap execution detail owned by Issue #119 F5
(SEEK DISCONTINUITY PROTOCOL REV.3, three-layer cutover). This ADR freezes
only its semantic spine *(that REV.3 step sketch is historical — superseded
2026-09-17 by the F5-GATE amendment below, which freezes the execution
ordering: seek refusal decided first, then staging discard, then the single
worker-side edge purge)*:

```text
same-episode ownership            no new Plugin/Fiber/lifecycle noun
stale-PCM invariant               the proposition above
reservoir accounting              the mechanism decision must cover every
                                  known stale-PCM reservoir
no single vague success bit       command accepted / decoder repositioned /
                                  old PCM invalidated / physical output
                                  cutover / actual landing stay separable
physical cutover gate stays OPEN  F5 implementation STOPs here
```

*(The "stays OPEN" row above is resolved by the 2026-09-17 F5-GATE amendment below; the stop rule itself remains: F5 implementation does not start until a later slice is explicitly authorized against the frozen text.)*

`seek command accepted`, `decoder repositioned`, `old PCM invalidated`, `physical output cutover`, and `actual landing` are not to be collapsed into one vague success bit if the implementation exposes them internally.

**The physical output cutover mechanism is still OPEN.** F5 production implementation must stop at that gap until a narrow mechanism decision proves one of the existing-output reset/reopen/minimal-cutover choices. A coding agent may not invent `Generation`, a generic cache protocol, or a new runtime to bypass this gate. *(Resolved 2026-09-17 by the F5-GATE amendment below, which freezes the mechanism decisions; that amendment's own block rule governs: the F5 production implementation still stops until a later slice is explicitly authorized against the frozen text.)*

> 2026-09-17 amendment (F5-GATE; mechanism evidence:
> `experiments/f5-seek-discontinuity/` — E1 decoder-seek reality probe
> [SongCore ABI v1, committed corpus], E2 edge-cut protocol probe
> [faithful edge-sync copy, 256×3 scenarios, 51/51 negative control],
> E3 physical WASAPI cutover probe [shared-mode, Windows host, 3 green
> runs]; formal: `specs/f5-seek-discontinuity/` — safety model, 5
> mutations COUNTEREXAMPLE-WITNESSED; roadmap: Issue #119 checkpoint).
> The semantic spine above is UNCHANGED — the amendment freezes the
> mechanism and policy decisions the gate earned and closes the §14
> open item ("seek physical-output cutover mechanism"). The F5
> production implementation remains blocked until a later slice is
> explicitly authorized against this frozen text; nothing here merges
> production code by itself.
>
> 2026-09-18 corrective 1 (F5-GATE-CORRECTIVE-1, pre-merge review;
> fresh evidence: SongCore implementation audit
> `native/src/songcore_ffmpeg.c song_seek`, E2 re-run — 294 scenarios ×3
> green runs incl. refused/destructive families, 51/51 rogue negative
> control, 1/1 drop-remainder must-fire, REFUSAL-EQUIV + FAIL-CLOSED
> oracles; formal model re-run — 672 states, 6 witnesses, 7 mutations
> incl. new M6/M7). The refusal semantics are
> split and made honest, in two moves. (a) Refusal is ZERO-content-
> loss: the in-flight staging block observed mid-write is stopped at
> its written prefix and PRESERVED; a refusal finishes it exactly, so
> the refused-seek output equals the no-seek control. The earlier
> "at most one abandoned in-flight staging block" loss claim is
> WITHDRAWN — it contradicted inertness. (b) The song_seek provider
> outcome is THREE-class — `RefusedUnchanged` (provably pre-mutation:
> only the SongCore parameter/state checks, INVALID_ARGUMENT class),
> `Applied` (success; landing known or unknown), `MutatedThenFailed`
> (every failure not provably pre-mutation) — and `MutatedThenFailed`
> routes through the ordinary D11 decode-failure path (terminal
> Failed) instead of resuming old playback: the ABI returns generic
> SEEK_ERROR both before av_seek_frame and again after a successful
> reposition + decoder flush/reset, so a status code alone cannot
> prove inertness, and the conservative rule is "unprovable means
> destructive". Representation spelling (Rust names) stays open to the
> implementation gate; only the three properties are frozen. The
> successful-cut protocol, output mechanism, commit boundary, position
> rebase and pause interaction below are unchanged.

**Frozen by this amendment:**

```text
acceptance            seek is a same-episode Command accepted only while
                      the data plane is Open (edge terminal == Open) and
                      the episode is unsettled, with no seek in flight
                      (one-seek-in-flight; a second request before the
                      current cut commits or aborts is inert). The drain
                      window after decoder EOF (edge Eof, D11 not yet
                      settled) is explicitly NOT seekable in v1 —
                      reopening an Eof edge is a closed design door.
cutover protocol      same-resource discontinuity protocol (no resource
                      is replaced). The session records the command and
                      parks the render leg at its loop-top gate (the
                      D14.7 park invariant — no device buffer held
                      across a park — attributed to the cut: an
                      internal seek park is NOT pause engagement
                      evidence and never routes pause intent). Ordering
                      is load-bearing in two places. First, the worker
                      keeps producing until the leg's parked evidence
                      has arrived — an early production hold could
                      strand the leg inside a blocked read on an
                      emptied edge and stall the protocol (the flowing
                      production is what keeps the park reachable). The
                      worker must also be able to reach its
                      serialization point with bounded latency
                      regardless of edge occupancy (representation
                      open; e.g. a bounded-slice write wait that
                      observes the command slot). Second, at the
                      serialization point — with the parked evidence in
                      hand — the worker calls song_seek BEFORE anything
                      is invalidated. Observing the command mid-block
                      stops the bounded-slice write at its written
                      prefix and PRESERVES the in-flight staging block;
                      the provider outcome owns the remainder. The
                      outcome is three-class (SongCore reality: only
                      the pre-av_seek_frame parameter/state checks are
                      provably non-mutating):
                        RefusedUnchanged (proven pre-mutation refusal,
                        the INVALID_ARGUMENT class) → the worker
                        publishes seek-failed mechanism evidence,
                        finishes the preserved remainder exactly, and
                        resumes production from its current cursor;
                        edge, device tail and render leg continue the
                        pre-command content (the session releases the
                        leg). A refusal is therefore pre-cut and inert
                        with ZERO content loss: the consumed stream
                        equals the no-seek control.
                        MutatedThenFailed (any failure not provably
                        pre-mutation — generic SEEK_ERROR, which the
                        SongCore ABI also returns after a successful
                        reposition and decoder flush/reset,
                        SEEK_UNSUPPORTED, STREAM_CHANGE,
                        DECODE_ERROR) → the old decoder continuation is
                        not guaranteed; the episode takes the ordinary
                        decode-failure route (D11 failure evidence →
                        terminal Failed) and NEVER resumes old-cursor
                        production. Conservative rule: unprovable means
                        destructive.
                        Applied (success) at landing L → the worker
                        discards its staging (including any preserved
                        remainder), invalidates the edge itself (the ONE
                        purge — the load-bearing stale-PCM exclusion
                        is this program-order discipline on the only
                        producer thread, not the primitive), publishes
                        its actual landing as mechanism evidence, and
                        holds production (writes nothing) until
                        release.
                      The session — seeing the landing with the leg
                      parked — waits for the output tail to quiesce and
                      then commits. The production hold keeps every
                      pre-commit submission pre-landing, so the rebase
                      basis L is exact: no post-landing pre-commit new
                      frame exists to mix into, or lose from, the
                      position accounting.
commit boundary       the session records the cutover commit iff
                      landing published ∧ edge invalidated ∧ output tail
                      quiesced (padding == 0 while parked — the D14.7
                      evidence class) ∧ leg parked ∧ episode unsettled.
                      Before commit, old output is legal; after commit,
                      no PCM of this stream that was queued-to-play
                      before the commit can ever be rendered — the same
                      device-consumed boundary D14.7/D14.8 freeze: the
                      claim covers this stream's queued-to-play set
                      (padding), never the acoustic instant or the
                      unmeasured downstream latency. Seek contributes
                      no terminal evidence and owns no second terminal
                      authority.
output mechanism      park + natural drain (the device consumes the old
                      tail pre-commit; measured 30.2–31.9 ms across
                      three runs at one full device buffer on the probe
                      endpoint). Stop/Reset/
                      Start is REJECTED for v1 (measured: freezes
                      mid-buffer audio, resets the device position
                      origin, adds a stream-state machine for an
                      inaudible latency win) and is re-earnable only by
                      a new narrow authority decision. No stream is
                      replaced; P1–P5 are NOT triggered (same edge,
                      same stream, same device session, same position
                      cell, same threads; no old/new RT-world overlap).
position rebase       realizes the D14.8 seek paragraph: the rebase
                      happens on the render leg's own execution path at
                      commit release — basis := the decoder's reported
                      actual landing in source frames (−1 = unknown → no
                      basis exists; the Position projection is withdrawn
                      for the rest of the episode: unknown stays
                      unknown, never zero, never the requested target),
                      and the leg's handed-off accounting resets, so
                      pre- and post-cutover totals are never mixed (the
                      protocol's production hold between landing and
                      release makes the basis exact — every pre-commit
                      submission is pre-landing). Same cell, one writer,
                      plain store at the commit; within one published
                      stretch — i.e. between committed discontinuities —
                      the publication stays monotone.
                      **Position monotonicity scope is hereby amended:
                      monotone between committed discontinuities** — a
                      committed cutover may step the published sample
                      backward exactly once, on the writer's path, and
                      that step is the discontinuity. No new cell, no
                      Generation/SeekId/TimelineSegment; the requested
                      target NEVER substitutes for the landing (E1
                      measured lossless block-aligned landings up to
                      ~648 frames before target).
pause interaction     pause intent SURVIVES seek: a seek never implicitly
                      resumes and is never rejected because of pause.
                      A paused episode's already-quiesced tail satisfies
                      the output-cut precondition; the leg stays parked
                      through the cut; paused() evaluates unchanged.
                      Conversely, pause intent arriving while a playing
                      episode's cut is in flight routes normally (the
                      seek park is cut-attributed); the leg is already
                      parked, and no pause-attributed engagement exists
                      until a post-release re-park — the frozen
                      attribution rule determines the outcome uniquely.
failure policy        pre-cut failures are inert diagnostics — playback
                      continues from the pre-command content; they are
                      NEVER terminal Failed. They are exactly: seek
                      already in flight; data plane not Open (edge
                      terminal != Open, which includes the post-EOF
                      drain window); settled episode or stop intent
                      already recorded; a RefusedUnchanged song_seek
                      outcome (the provably pre-mutation class,
                      INVALID_ARGUMENT) — under the frozen ordering it
                      happens BEFORE any invalidation and the preserved
                      staging remainder is finished, so edge, tail and
                      leg continue seamlessly with zero content loss.
                      Any other song_seek failure status (SEEK_ERROR /
                      SEEK_UNSUPPORTED / STREAM_CHANGE / DECODE_ERROR)
                      is NOT a refusal: the SongCore ABI returns generic
                      SEEK_ERROR both from a failed av_seek_frame and
                      again after a successful reposition + decoder
                      flush, so a status code alone cannot prove the old
                      decoder survived; such an outcome is classified
                      MutatedThenFailed and routes through the ordinary
                      D11 decode-failure path (terminal Failed) — it is
                      never papered over as a refusal. Reclassifying a
                      provider result as RefusedUnchanged requires a
                      narrow SongCore provider-contract corrective with
                      its own evidence (SEEK_UNSUPPORTED stays
                      destructive until such evidence exists). Post-cut,
                      the selected mechanism confines failure to the
                      existing D11 device-failure path (the edge
                      invalidate is a fail-fast O(1) reset that happens
                      only after song_seek has succeeded; a device
                      failure during the drain settles through the
                      existing precedence). Seek introduces no new
                      terminal variant and no recovery semantics.
realtime cost         normal playback adds no new allocation, no new lock
                      acquisition, no dispatch, no K0/Capability work
                      and no version/epoch comparison per quantum; the
                      render loop's existing loop-top gate check gains
                      one more session-owned seek-park flag test, and
                      the worker gains a loop-top command check off the
                      RT path; seek work is bounded control work
                      outside the quantum path.
public surface        proposed future command:
                      `PlaybackSessionHandle::request_seek(&self,
                      target: Duration)` — infallible, non-negative,
                      source-relative; invalid moments are inert. No
                      SeekManager/SeekSession/transaction noun; no new
                      observation field and no public positive seek
                      state (consumers observe the Position jump). The
                      allowlist update at implementation time remains
                      the explicit architecture event (D14.10).
```

The command accepted / decoder repositioned / old PCM invalidated /
physical output cutover / actual landing propositions stay separable
(they are separate protocol states above); the implementation must not
collapse them into one success bit. Seek completion is NOT a Fact and
not a product state; nothing here earns `Generation`, `Epoch`, `SeekId`,
`DiscontinuityId`, `TimelineSegment`, or a public `SeekState` — the
razor review is recorded in the gate report (`experiments/f5-seek-
discontinuity/RESULTS.md` §15).

> 2026-09-18 implementation note (F5-SEEK-IMPLEMENTATION-1, branch
> `feat/f5-seek-1`). Representation only — no proposition above changed.
> The frozen shape is realized as follows: the public command is
> `PlaybackSessionHandle::request_seek(&self, target: Duration)`
> (infallible, non-negative by type; acceptance fails closed on the
> frozen conditions; the D14.10 allowlist update is done and negative-
> controlled). The provider outcome is the three-class enum
> `ProviderSeekOutcome` (`RefusedUnchanged` / `Applied { landing:
> Option<u64> }` / `MutatedThenFailed { diagnostic }`). The edge's
> blocking whole-slice write is retired for the bounded-slice primitives
> the protocol names (`write_some` + `wait_for_space`), and the ONE
> purge is the non-terminal `PcmEdge::invalidate()` — O(1) cursor reset,
> terminal untouched, both endpoints woken (loom L5–L7 pin it under
> every interleaving). The cut's park is routed as a gate seek hold; the
> commit decision routes a release PAYLOAD (`Committed { landing }` /
> `Aborted`) that the leg consumes AT ITS SEEK GATE — at the park's exit
> or, for a cutover committed while the leg was held by PAUSE (whose
> pause slices stop observing after their own quiescence), at the gate's
> entry — so the rebase happens on the leg's path before any further
> submission under every interleaving (payload-awaits-consumption;
> pinned by the render-gate seek oracles). The one-seek slot stays
> occupied through that consumption: the worker frees it only after the
> routed release has been consumed (it polls the gate's
> release-pending bookkeeping off the RT path), because a later seek's
> hold would otherwise wipe an unconsumed `Committed` and lose the
> rebase — the pause-shaped interleaving the seek matrices caught and
> now pin (`a_committed_release_is_never_wiped_by_a_later_seek` — a
> name this note invented before implementation; the duty is actually
> pinned by the white-box "slot stays occupied through the commit"
> assertion, the end-to-end `a_second_seek_while_one_is_in_flight_is_inert`,
> and the gate's `a_new_hold_drops_a_stale_unconsumed_release` oracle). The
> position cell gains `rebase(landing)` — the one legal backward step,
> a plain store whose `None` encoding withdraws the sample; the
> withdrawal is episode-permanent on the leg's discipline (a later
> KNOWN landing neither resurrects publication nor un-withdraws the
> cell). One conformance fix was made
> against the frozen program order during implementation: a seek
> observed mid-write may still sit in the command slot, and the cut
> point now promotes it into the worker's pending command so the
> serialization point runs THIS seek — "the preserved remainder does
> not defer the seek" is literal (the unpromoted spelling wedged the
> leg's park against the remainder finish; the seek matrices witnessed
> it). One realtime-cost differential was recorded by this note (two
> consecutive gate-intent checks, one extra uncontended acquisition)
> and is WITHDRAWN by the corrective below, which restores the frozen
> row literally. Evidence: `tests/seek_seam.rs` (14 end-to-end
> matrices), crate-internal white-box protocol tests, loom L5–L7, and
> the implementation mutation gate `specs/f5-seek-implementation/`
> (M1–M7, 7/7 COUNTEREXAMPLE-WITNESSED — the Rust twins of the gate
> suite's TLA+ mutations). *(Counts superseded by corrective 3 below:
> 16 matrices, M1–M10, 10/10; superseded again by corrective 4 below:
> 17 matrices, M1–M11, 11/11.)* Windows physical smoke evidence is a gate of
> the implementation PR, not of this note.
>
> 2026-09-18 implementation corrective 1 (F5-SEEK-IMPLEMENTATION-
> CORRECTIVE-1, same branch; fresh adversarial review of the
> implementation PR, four MAJORs). Representation and mechanism
> conformance only — the frozen propositions above are unchanged.
> (C1, paused rebase) The release payload is consumed on the leg's path
> THROUGH the unified loop-top gate even while the leg is parked by
> PAUSE: a committed cut rebases a paused leg MID-PARK — the position
> projection reads at the landing BEFORE the resume — and the pause
> intent survives untouched; the first note's "consume at the gate's
> entry after resume" spelling is superseded (it deferred the rebase
> past the whole pause window, an observable conformance defect).
> (C4, realtime cost) The D14.7 pause park and the D14.5 cut park are
> unified into ONE loop-top gate operation on the shared intent lock:
> a steady iteration of normal playback is a single uncontended mutex
> acquisition with O(1) flag tests — the frozen realtime-cost row is
> realized literally ("no new lock acquisition; the existing loop-top
> gate check gains one more seek-park flag test") and the differential
> recorded above no longer exists; a source-order oracle pins exactly
> one loop-top gate call in the mechanism. (C2, seek × worker-exit
> linearization) Acceptance is one atomic unit under a single
> completion-lock hold — re-validation, the one-seek plant, the
> cut-cycle evidence reset, and the hold routing — and the worker's
> single exit funnel publishes worker-liveness evidence BEFORE an exit
> duty aborts any accepted seek the leaving worker can no longer
> resolve. The frozen acceptance set is unchanged in substance (a
> worker that has left its protocol is not a live data plane; on every
> real exit path the edge terminal has already left Open, so the Open
> condition already covers the substance — the added check makes the
> linearization envelope exact: no plant can exist whose only resolver
> is gone, which closes the request_seek × worker-EOF wedge where a
> hold nobody would release parks the leg past the final drain and D11
> Completed never settles). (C3, current-cut attribution) The per-cut
> evidence latches (landing, refusal, commit) belong to the CURRENT cut
> cycle and are reset when the next seek is accepted — a second commit
> requires the SECOND seek's own landing evidence, exactly the
> current-engagement attribution discipline D14.7 freezes for pause;
> without the reset a second commit could ride the first seek's
> landing. Evidence added by the corrective: the render-gate oracles
> for the unified gate (mid-park consumption, single-acquisition
> shapes), the white-box seek/worker-exit linearization pair, the
> white-box second-seek evidence oracle, an end-to-end seek × EOF
> sweep, the strengthened paused-rebase matrices, mutation M8 (deleting
> the evidence reset must RED), and the render-order oracle's P11/P12
> (one loop-top gate call; the retired per-park calls gone).
>
> 2026-09-18 implementation corrective 2 (F5-SEEK-IMPLEMENTATION-
> CORRECTIVE-2, same branch; authority-conformance review). No
> proposition above changed — this closes a DRIFT: the first
> implementation inherited the gate report's phase-0 lumping and
> classified `SONG_ERR_NOT_OPEN` as `RefusedUnchanged`, exceeding the
> frozen refusal set ("provably pre-mutation: the INVALID_ARGUMENT
> class"). Not mutating is not the same as certifying a usable old
> cursor: `NOT_OPEN` reports `!h->probed || !h->dec` — a handle that is
> NOT in an opened/probed state — so it cannot prove the pre-call
> decoding continuation valid, and on a probed endpoint it is an
> abnormal provider state rather than an inert refusal. The provider
> classification is narrowed to the frozen set: `SONG_ERR_INVALID_ARGUMENT`
> → `RefusedUnchanged`; every other non-success status →
> `MutatedThenFailed` (the conservative default, "unprovable means
> destructive"). The gate report's phase-0 wording is corrected in place
> as a marked corrective (`experiments/f5-seek-discontinuity/RESULTS.md`
> §13). Evidence: a pure raw-status → class map pin plus two raw-ABI
> boundary probes in `qianqian-decode-songcore` — an unprobed handle
> really answers `SONG_ERR_NOT_OPEN` and classifies destructive, and an
> `INVALID_ARGUMENT` rejection leaves the decode continuation
> bit-identical to a no-seek control handle. Both directions are
> negative-controlled: widening the refusal set REDs the map pin,
> narrowing it REDs both pins.
>
> 2026-09-18 implementation corrective 3 (F5-SEEK-IMPLEMENTATION-
> CORRECTIVE-3, same branch; pre-merge review of the implementation
> PR). No proposition above changed and no new failure class is
> introduced — this closes two CONFORMANCE gaps in the applied-cut
> path, both of which made the implementation weaker or less live than
> the frozen text already requires.
> (R1, liveness) The frozen failure policy's episode-ending class
> "data plane not Open (edge terminal != Open)" was implemented at the
> cut's ACCEPTANCE and at its serialization point, but not in the
> protocol's own waits after the provider APPLIED: those polled only
> the session-recorded endings (stop intent / settlement / teardown
> release), and teardown records its release only AFTER the worker
> join — the decode relation unwinds before the output relation's
> release. A cut whose commit boundary is permanently unreachable (a
> device whose queued-to-play tail never quiesces, or a device abort
> that stopped the plane) therefore wedged teardown forever: the join
> could not return and the leg was never released. The waits now read
> the data plane's terminal on the worker's own path and take the
> abort route there — the same class the policy already names, no new
> refusal, no new terminal evidence, no seek-side settlement (the data
> plane's owner settles through the existing D11 precedence).
> (R3, lost rebase) The commit boundary and the episode-ending latches
> are latched evidence, and the leg's pause→cut park handover publishes
> Disengaged-then-SeekEngaged — so an implementation that sampled the
> wait on one read and the decision on another could see the boundary
> hold and then an evidence gap, and route an abort release strictly
> after the provider applied and the edge was purged: the leg would
> resume with its PRE-CUT position accounting, reverting the cut
> through an evidence artifact. The decision is now ONE atomic sample
> with three outcomes — Committed (the boundary held; commit recorded,
> rebase release routed) / Aborted (an episode ending is recorded: the
> only abort) / Pending (the boundary is not satisfiable YET: nothing
> recorded, nothing routed, the protocol keeps waiting). A missing
> park/quiescence sample is a statement about the evidence, never about
> the episode; the only exits from an applied cut are the commit or an
> episode ending.
> (R2, shell conformance) The `--machine` seek token reader used the
> panicking `Duration::from_secs_f64`; tokens a float parser accepts
> but no Duration can represent (`nan`, `inf`, `1e400`, a minutes field
> near `u64::MAX`) aborted the command reader instead of being inert
> input, contradicting the shell's own contract that an unreadable
> token sends NO command. It now fails closed (`try_from_secs_f64`) and
> the token grammar pin covers those spellings.
> Evidence: `tests/seek_seam.rs` gains the never-draining-device
> teardown oracle (and its stop control) and reaches 16 end-to-end
> matrices; the crate-internal white-box suite gains the park-handover
> gap oracle (`a_park_handover_evidence_gap_is_pending_and_never_an_abort`)
> and re-spells the commit-conjunction oracle three-valuedly; the
> implementation mutation gate grows to ten patches
> (`specs/f5-seek-implementation/`, M1–M10, 10/10
> COUNTEREXAMPLE-WITNESSED) — M9 (the wait ignores the data plane's
> terminal) REDs the teardown oracle by harness bound, M10 (a pending
> sample classified as an abort) REDs the handover oracle, M1 and M5
> are re-pinned to the corrected shape (with an idempotent decision a
> premature commit call is a Pending no-op, so M5 pins the program-order
> violation literally: publishing the landing only after the wait).
> `qianqian-headless`'s token pin and its usage text are corrected with
> the reader. One consequence is recorded rather than fixed: a device
> that neither quiesces its tail nor fails and meets neither a stop nor
> a teardown leaves the applied cut Pending indefinitely — the episode
> is silently stalled (`Pending`, no timeout, no signal). The superseded
> bool spelling had the same property (its wait also required the tail
> condition and had no timeout), so nothing regressed, and a timeout
> would be NEW authority (a new failure policy row), which this
> corrective deliberately does not invent. Windows physical smoke
> evidence remains a gate of the implementation PR, not of this note.

> 2026-09-18 implementation corrective 4 (F5-SEEK-IMPLEMENTATION-
> CORRECTIVE-4, same branch; human review of corrective-3). No
> proposition above changed and no new failure class is introduced —
> this closes the REMAINING half of corrective-3's R1 liveness ruling.
> (C9, device failure inside the park) Corrective-3 taught the
> protocol's WAITS to read the data plane's terminal; but a terminal
> is produced only by something that stops the plane, and inside a
> park nothing below the loop could run: the parked leg's tail probe
> answered ONE bool, so a tail observation that itself FAILED — a
> real endpoint invalidation surfacing as a `GetCurrentPadding` error
> — was masked as "not quiesced yet". The park then waited forever,
> the edge stayed Open, and the worker's corrective-3 escape (correct
> as far as it went) waited on a terminal the masked park could never
> let happen: the frozen "existing output/device-failure" class was
> structurally unreachable from inside a park. The probe now answers
> `TailProbeOutcome` — Pending (keep waiting in bounded slices) /
> Quiesced (the D14.5/D14.7 quiescence evidence) / Failed (the
> observation itself failed) — and the three classes are never
> collapsible: a Failed observation is neither quiescence evidence nor
> a wait-forever condition. The gate releases the leg WITHOUT
> publishing quiescence for the failed observation, still consumes any
> routed release payload on the leg's path (the once-on-this-leg
> discipline holds on every exit), still publishes the park's
> disengagement fence, and returns `ParkOutcome::TailProbeFailed` —
> the gate itself still never aborts the leg; the MECHANISM's existing
> device-failure path decides (for the WASAPI render loop, the same
> abort exit a steady-path padding error already took: the loop
> aborts, the dead leg stops the data plane, the edge goes terminal,
> the worker's escape fires, and D11 precedence settles `Failed`).
> After a pause park whose probe failed, a still-routed seek hold does
> not park again — more slices from a dead device can produce neither
> quiescence nor recovery.
> Evidence: the gate protocol suites gain a failure-exit oracle per
> park attribution (a failed observation ends the park bounded with no
> quiescence published, both with and without a routed payload); the
> render-order source oracle grows P13 (the tail arm answers Failed
> exactly once — the masked-pending and pre-corrective-bool
> degradations RED; P8 re-anchored to the three-class arm); the mock
> render leg mirrors the production posture; and `tests/seek_seam.rs`
> gains the REAL failure-shape oracle the review required — the leg is
> provably parked under the cut's hold (its armed observation held
> inside the gate), the endpoint then invalidates, no stop is ever
> requested, and the episode settles D11 `Failed` bounded — reaching
> 17 end-to-end matrices. The implementation mutation gate grows to
> eleven patches (`specs/f5-seek-implementation/`, M1–M11, 11/11
> COUNTEREXAMPLE-WITNESSED) — M11 (both gate Failed arms collapsed
> into the Pending treatment, i.e. the pre-corrective mask) REDs the
> device-failure-in-park oracle by harness bound. One consequence of
> corrective 3 narrows: its recorded unbounded-Pending stall required a
> device whose tail neither quiesces NOR FAILS — an observation
> failure is no longer a stall but the episode ending; the remaining
> stall premise (an observation that keeps succeeding while the tail
> never quiesces, no stop, no teardown) stands as recorded there, and
> a timeout remains NEW authority this corrective deliberately does
> not invent. Windows physical smoke remains a gate of the
> implementation PR, to be run at the final head after this
> corrective.

### D14.6 Open / Next / Previous — no-overlap replacement v1

The current Phase-F v1 semantic requirement is deliberately simple and gap-tolerant:

> **The old playback episode must be fully retired from K0 before the new playback episode becomes live.**

This is a **no-overlap semantic constraint**, not a frozen App/K0 call sequence. It deliberately avoids preload/gapless/dual-world lifetime for the first working slice. It is a **Phase-F v1 constraint, not a permanent playback topology**; any overlap-bearing successor topology must be earned separately under the AGENTS.md abstraction-earning rule.

At the product level:

```text
old episode, if still live
    ↓ intentional stop command
    ↓ D11 authority-owned settlement
    ↓ old Playback Session Fiber withdrawn + teardown/discharge complete
    ↓
new Playback Session episode may become live
```

The exact configuration/handoff mechanism that creates a fresh session definition for the new source is frozen by the 2026-09-18 F6-AUTHORITY-PROMOTION-1 amendment below.

> **2026-09-18 amendment (F6-AUTHORITY-PROMOTION-1).** Evidence base:
> `experiments/f6-source-probe/` + PR #157 (S_PROBE_GREEN, 3 physical
> runs × 11 scenarios on a real Windows host, NEG negative control
> fired; acoustic human-ear witness UNAVAILABLE, recorded as a
> conditional-green review item — a failed ear check reopens this
> verdict); design source: the merged #155
> transport-closure package. The two D14 openings this amendment
> decides are `F6 CONFIG-MECHANISM-OPEN` and the probe/concurrency
> disclosure; everything not stated below remains governed by the
> general Phase-F rules.
>
> **F6 propositions (frozen).**
>
> ```text
> Open = an application composition Command that replaces the WHOLE
>        episode composition. NOT PlaybackSessionHandle::open(), NOT a
>        K0 command, NOT a session mutation. No Open Fact, no
>        Opening/Opened/SourceTransition state: the user-visible result
>        is the new episode's existing D14.2 observation, and Open's
>        own success/failure is application composition feedback
>        (operation results), never a playback semantic.
>
> probe-before-destruction (Candidate B): Open(path) first probes the
>        candidate source OFF the live playback path. An invalid
>        candidate is REFUSED with a diagnostic and the old episode is
>        untouched. The frozen product property is: an invalid Open
>        candidate never kills live playback. A RED S-PROBE would have
>        reopened this decision instead of promoting it; the probe
>        runs while the old episode's decode worker holds its own
>        SongCore handle — physically evidenced green by F6-S-PROBE.
>
> probe ≠ episode: the probe is one public, stateless decode-provider
>        mechanism query (open → read format/duration facts → close),
>        owning NO render stream, NO edge, NO worker, NO device
>        session, NO PCM read. It is not a second live episode and
>        does not trigger P1–P5. Its output is mechanism evidence for
>        an application composition decision ("this source opened and
>        declared X at probe time") — advisory, never episode truth;
>        the new activation's own open/probe publishes the
>        authoritative source evidence. D13 record: the probe query
>        earns NO Plugin, NO Capability and NO composition identity —
>        a plain one-shot read operation owned by its caller; the
>        Open/replacement operation likewise earns none (an App
>        composition Command, not a K0 participant). The existing
>        owners lose nothing by owning them. Public-surface amendment
>        (intentional, narrow): the decode provider crate exposes
>        exactly this query — `SourceFacts` only, never
>        SongcoreDecode / DecodedPcmStream / song handles / service
>        internals; the implementing slice MUST sync
>        tools/check_plugin_boundaries.py with the surface.
>
> mechanism (C3, whole-episode-composition replacement): each Open
>        constructs a FRESH QianqianApp composition root whose
>        session definition takes the file as a constructor argument.
>        No config channel, no per-instance config payload, no
>        registry, no hot component replacement exists. Canonical
>        refinement (recorded in D1): one process-level
>        reference-player host sequentially owns multiple
>        non-overlapping `QianqianApp` composition roots, one per
>        playback episode. Provider lifetime becomes episode-scoped
>        (recorded in D5); per-Open re-mount cost is two stateless
>        re-activations plus the per-episode device open every
>        candidate pays anyway.
>
> replacement commit
>     := old-side clear
>        AND new episode's authoritative activation result == Activated
> old-side clear
>     := no current composition root
>            (first episode, or the previous start ended
>             ActivationFailedClean)
>        OR the current composition's authoritative disposal
>           outcome == Discharged
>
> No disposal outcome is forged when no root exists. Both operands are
> synchronous results of the authority-owned control operations the
> App itself invokes — NOT Facts, NOT new K0 primitives, NOT snapshot
> reads. Control correctness MUST NOT depend on CompositionSnapshot
> (PBK-001 §2.3 firewall; snapshots stay read-side diagnostics).
> `Activated` is defined over the WHOLE fresh composition ("the fresh
> desired composition successfully established the required Playback
> Session episode") — covering provider activation failure, unresolved
> dependency and session activation failure alike; never
> absence-of-diagnostic, never a snapshot FiberState read.
>
> replacement sequence (per Open):
>     candidate probe
>       invalid ⇒ REFUSED, old untouched
>     if old root exists:
>         request_stop() (iff unsettled) → wait_terminal() (D11 truth)
>         → dispose() → require authoritative Discharged
>       already-terminal old: skip stop/wait, dispose directly
>     construct fresh QianqianApp → activate desired composition
>     → require authoritative Activated
> ```
>
> **Failure classes (frozen, no rollback anywhere).**
>
> ```text
> invalid candidate            refused before any destructive step;
>                              old playback continues untouched
> old-episode settlement       existing D11/D14.5/D14.7 semantics; Open
>                              waits on wait_terminal; no timeout is
>                              invented in v1
> old teardown failure         authoritative disposal outcome
>                              TeardownViolated ⇒ FAIL-STOP: the
>                              violated latch has no exit
>                              (composition-kernel-0-design.md §G.6),
>                              no
>                              new episode is constructed, no further
>                              Open/Next/Previous runs in this process;
>                              the App RETAINS the violated root until
>                              process termination (drop runs no
>                              teardown inverses); recovery is not
>                              assumed
> new activation failure       the start operation is FAILURE-CLEAN:
>   (after old is gone)        the attempted fresh root is authoritatively
>                              disposed before the failure returns.
>                              cleanup Discharged ⇒
>                              ActivationFailedClean(diagnostic), no
>                              runtime remains, later Open legal;
>                              cleanup TeardownViolated ⇒ FAIL-STOP
>                              retaining the attempted root. The old
>                              world stays gone; "activation did not
>                              establish" never means "the attempted
>                              root may be dropped"
> ```
>
> Frozen non-events: paused old episode settles per D14.7 stop-from-paused
> (mid-play `Stopped`; post-EOF drain `Completed`); a new episode starts
> unpaused. Open during an in-flight Seek: replacement owns the larger
> lifecycle; frozen D14.5 refusal/precedence rules govern the cut.
> Repeated Open is App-thread-serialized — one complete replace
> operation at a time. No preload / crossfade / gapless / old-new render
> overlap / speculative second output stream / transactional reopen /
> multi-session topology in v1 (§8 of the design; unchanged).
>
> **Representation still OPEN (implementation-slice decisions, not
> agent inventions):** the probe query's Rust spelling; the
> disposal/activation result seams' Rust spelling (candidate shapes
> `DisposeOutcome::{Discharged, TeardownViolated}` /
> `StartOutcome::{Activated, ActivationFailedClean}`); Open input UX.
> C4 (config source + re-incarnation) stays unearned unless a future
> preload/multi-session gate needs providers to survive episodes.
>
> **Playlist / queue authority — CLOSED by the same amendment**
> (design source: NAVIGATION-GATE; this closes the §14 item with "no
> new authority"): the playlist and the current index are application
> navigation state owned by the reference-player App
> (`Vec<PathBuf>` + `Option<usize>`); nothing outside the App ever
> reads them; NO PlaylistPlugin, NavigationPlugin, PlaylistFact,
> CurrentTrackFact or any new observable. Index truth is
> **commit-on-activation**: the index moves only on F6 replacement
> commit evidence, and is never playback truth — the read side stays
> the D14.2 observation. Next/Previous select a candidate and invoke
> the same Open replacement: inert at both ends (no wrap, no
> stop-the-player side effect), no repeat, no shuffle, no EOF
> auto-next, no failed-candidate auto-skip (one keypress advances at
> most one candidate; a probe refusal leaves index and playback
> untouched). Direct user Open replaces the playlist with the single
> opened path and selects index 0 — on commit. Post-destruction
> activation failure leaves the index at the old entry with no episode
> and no runtime residue (failure-clean start above); the cursor then
> names a track that no longer plays — honest, because the cursor is
> navigation state, not audible-source truth. A latched teardown
> violation permanently disables further replacement (fail-stop above);
> no navigation recovery path exists. Direct jump-to-item selection and
> startup-args grammar details remain open representation.

Consequences already frozen:

- Open creates a new playback episode; it is not Seek。
- Next/Previous are navigation/selection decisions followed by the same no-overlap Open semantics; they are not new K0 primitives or data-plane protocols。
- An audible gap is acceptable in v1。
- Because old/new playback episodes do not overlap, `Generation`/`Window`/Realtime Audio Runtime is not earned by Open/Next/Previous v1。
- The old episode keeps any already-committed terminal truth; replacement cannot relabel it。
- If intentional replacement stop was recorded before that episode's terminal evidence became decisive and no higher-precedence failure wins, the existing `Stopped` terminal variant is sufficient. Do **not** invent `Superseded` / `Preempted` as a new terminal Fact for v1. Replacement cause may remain diagnostic/control context if needed。

Playlist/queue selection authority was CLOSED by the 2026-09-18 F6-AUTHORITY-PROMOTION-1 amendment (application navigation state; commit-on-activation; see D14.6).

### D14.7 Pause / Resume — same-episode non-terminal control; mechanism + establishment frozen

> 2026-09-16 amendment (F3-GATE; mechanism evidence:
> `experiments/f3-pause-mechanism/` — synchronization-shape scenario
> suite + physical WASAPI probe; roadmap: Issue #119 checkpoint). It
> replaces the previous "semantic direction fixed, implementation still
> blocked" text: the minimum mechanism and the truthful-establishment
> semantics below are now FROZEN; everything not stated remains governed
> by the general Phase-F rules.
>
> 2026-09-16 corrective (F3-GATE-CORRECTIVE-1; same evidence crate,
> human review round 2). The original establishment formula equated
> render-gate engagement with `Paused`, but mechanism A's own physical
> evidence shows that already-submitted device audio keeps playing
> after engagement until the device-side padding drains. The
> establishment rule below therefore adds **output-tail quiescence**
> evidence, demarcates `Paused`/`Resumed` as application-facing derived
> **Projections** (never Facts, never correctness authority), and
> weakens `Resumed` to exactly what its evidence can support. The
> selected mechanism, the mechanism requirements, the ownership rules
> and D11 are UNCHANGED by this corrective.
>
> 2026-09-17 corrective (F3 AUTHORITY-CORRECTIVE; PR #150
> implementation evidence). **AUTHORITY-CORRECTIVE: the frozen
> `Resumed` projection was too strong and is REMOVED.** The previously
> frozen proposition — unsettled ∧ resume released ∧ disengagement
> observed — is false for a never-activated episode: pause intent
> routed before activation, the render gate engages, the open aborts
> (open timeout / open failure), the abort permanently closes the gate
> and joins the leg (`close_and_release`), activation fails — and a
> later resume then satisfies the formula while the episode's render
> leg is provably gone: disengagement evidence proves only that the
> gate's CURRENT park has ended, not that a viable render leg remains
> to submit future audio. Therefore `Resumed` is removed as an
> application-facing Projection. Resume remains Command state
> (`pause_requested := false`); `Disengaged` remains Mechanism Evidence
> and stays internal to the mechanism/session reasoning (it is not
> public product surface); the episode seam keeps exactly one
> application-facing pause projection, `Paused`. No replacement
> `Playing`/`Running`/`Active`/`Ready`/transport-state noun is
> introduced. This corrective does NOT reopen: mechanism A, the Paused
> establishment rule, D11, the RenderGate race closure (teardown wake,
> open-abort close, engagement fence), or P1–P5.

Pause/Resume is **same-episode, non-terminal control owned by the
Playback Session Plugin**. It never creates or destroys a Playback
Session Fiber and never settles the D11 terminal outcome merely because
playback paused or resumed.

**Minimum accepted mechanism (class frozen; representation open).** The
explicit render-loop pause gate, located in the render mechanism's loop
strictly **before** the device-buffer acquisition (WASAPI `GetBuffer`):

```text
pause command
    ↓ Playback Session routes the episode's pause intent to its gate
render loop reaches the gate check (loop top, no device buffer held)
    ↓ parks; publishes the engagement acknowledgment (mechanism evidence)
resume command / stop release
    ↓ gate wakes the parked leg (bounded park slice; notify + cap)
"unpark-and-continue": the loop proceeds once more;
the DATA-PLANE terminal decides — the gate never aborts the leg
```

Mechanism requirements frozen by that shape:

```text
the render leg MUST NOT hold a device buffer across a parked pause;
stop (and teardown) MUST wake every parked participant with bounded latency;
the device stream stays open — no resource replacement, no reopen;
engagement/disengagement MUST be acknowledged back as mechanism evidence;
P1–P5 are NOT triggered: same edge, same render stream, same device
session — no old/new realtime-world overlap exists in this shape.
```

`IAudioClient::Stop/Start` wrapping the same park was measured credible
but is **not selected** (it freezes mid-buffer audio and adds stream
state changes no Phase-F requirement needs); it may be re-earned only by
a new narrow authority decision. Parking by letting `read_frames` block
while `GetBuffer` is held is explicitly rejected as a pause mechanism.

**Truth classes and truthful establishment.** The mechanism's physical
evidence separates two moments that must not be conflated: render
engagement (the render leg parked at the gate) is **not yet an audible
pause** — frames already submitted to the device buffer keep playing
until the device-side padding drains. The establishment rule therefore
rests on TWO mechanism-evidence factors plus the command state:

```text
pause / resume intent      Command state on the episode seam
                           (idempotent; same family as stop intent)
render engagement /        Mechanism Evidence published through the
disengagement ack          session-owned evidence path — the render leg
                           reached the pre-GetBuffer gate and will
                           submit no further PCM while parked; never a
                           Fact
output-tail quiescence     Mechanism Evidence — no frame submitted
                           BEFORE engagement remains queued for
                           rendering by this output session
Paused                     the one application-facing derived
                           Projection over the above (PBK-001 §2.3
                           sense); NOT a semantic Fact and NOT a
                           correctness basis (see the non-authority
                           rule below). `Resumed` was removed as an
                           application-facing Projection by the
                           AUTHORITY-CORRECTIVE above; resume is
                           Command state only and disengagement stays
                           Mechanism Evidence.
```

**Output-tail quiescence (establishment closed for mechanism A).** For
the selected mechanism the tail-quiescence evidence is observed as
`GetCurrentPadding() == 0` from the episode's own render session at
some time after the engagement ack. This reading is sound for WASAPI
shared mode and the frozen mechanism shape, as a conjunction of:

```text
platform contract   for a shared-mode rendering stream, padding is
                    exactly the number of audio frames of this stream
                    queued up to play in the endpoint buffer
                    (IAudioClient::GetCurrentPadding);
frozen mechanism    the parked leg submits nothing (gate strictly
invariant           before GetBuffer, no device buffer held across the
                    park), so padding cannot increase between
                    engagement and the observation;
therefore           one zero observation after engagement proves every
                    pre-engagement frame has left the queued-to-play
                    set, for the remainder of the park.
```

claim is deliberately narrow — no speaker/DAC motionlessness, no
downstream device-latency claim, no other-session silence, no
human-hears-silence claim. Physical evidence
(`experiments/f3-pause-mechanism`, Windows shared-mode probe): at
engagement the observer reads the submitted tail at up to one full
device buffer (measured padding-at-engage = 1056 frames ≈ 22 ms at the
observed endpoint), which drains to 0 — measured
`tail_drain_latency = T_tail_quiesced − T_engaged ≈ 28–30 ms` — after
which this session's contribution is silence for the rest of the park.
Mechanism B (device Stop/Start) freezes the padding mid-buffer and
therefore cannot observe this evidence while parked; adopting this
establishment rule under B would itself require a new narrow authority
decision (B's non-selection reasons remain the original ones: frozen
mid-buffer audio and added stream-state changes). This evidence stays
Mechanism Evidence; it must never be promoted to a Fact or a P1–P5
trigger (no old/new realtime-world overlap exists here). Its reuse as
the raw position-evidence source is not a silent promotion: the D14.8
amendment makes that a separate, explicit selection of the same
platform reading (the derivation's tail reading), while the
establishment latch keeps its F3 meaning.

Truthful product establishment is then the single projection:

```text
Paused (projection)  ⇔ the episode has no committed terminal outcome
                       ∧ pause intent recorded ∧ render engagement
                       evidence observed ∧ output-tail-quiescence
                       evidence observed
```

`Resumed` is NOT a projection (AUTHORITY-CORRECTIVE above). Resume is
only the command `pause_requested := false`. The disengagement
acknowledgment proves exactly that the gate's current park has ended —
it does NOT prove that pause control is no longer established, and it
does NOT prove that a viable render leg remains available to submit
future audio: a never-activated/open-aborted episode may have
permanently closed and joined that leg while its episode stays
unsettled forever. Audible-time semantics remain F4/D14.8 territory.
Engagement evidence alone is not establishment; a settled episode is
never Paused.

The establishment conjunction is guarded by the episode's unsettled
state: once a terminal Fact commits, pause truth must not evaluate true
on the episode regardless of mechanism engagement still being latched,
so session settlement/teardown MUST release the pause gate (publishing
disengagement evidence) on the authority-owned execution/teardown path.

**Non-authority rule for the Paused projection.** Paused creates no new
fact kind, no new fact authority, no fourth terminal variant, no
`Paused` semantic Fact and no `Playing/Starting/Paused/Stopping`
transport enum (the AUTHORITY-CORRECTIVE additionally removed
`Resumed` from the projection set and forbids re-introducing it, or any
`Running/Active/Ready` replacement, without a fresh authority
amendment): the observation surface gains pause-intent (command state)
and engagement/tail-quiescence (mechanism evidence) fields, and their
spelling is representation (the episode-handle public-surface allowlist
update is the explicit F3-implementation architecture event). As a
Projection in the PBK-001 §2.3 sense it MUST NOT be used as the
correctness basis for resume legality, stop legality, teardown,
terminal settlement, resource lifetime, mechanism wakeup, or K0
lifecycle transitions; control/lifetime correctness uses the
authority-owned command/control state and/or the direct mechanism
state/evidence. Product status MUST NOT infer pause from worker
blocking, PcmEdge occupancy, FiberState, UI state, or ad-hoc WASAPI
observations outside this establishment chain. Disengagement evidence
MUST NOT be promoted into a user-facing transport/product truth
(exactly the defect the AUTHORITY-CORRECTIVE removed).

**Terminal interactions (D11 unchanged).** Stop-from-paused mid-play
produces the existing worker-Stopped × drain-Aborted × intent history →
`Stopped`. Stop-from-paused after decode EOF plays the tail out and
drains → worker-Eof × drain-Drained → `Completed` — the same outcome as
today's stop-after-EOF (the gate must NOT force-abort, which would
fabricate the Failed{device} history). EOF while parked leaves the
episode unsettled until resumed-and-drained or stopped. Failure while
parked settles immediately by the existing precedence. Pause and resume
commands recorded after settlement are inert command history, exactly
like late stop intent.

### D14.8 Position / Duration — episode-local Projection + optional source evidence

> 2026-09-17 amendment (F4-GATE; mechanism evidence:
> `experiments/f4-timeline-gate/` — projection-algebra oracles,
> physical WASAPI probe `f4probe` [shared-mode, Windows host],
> duration provenance probe `f4duration` [SongCore ABI v1]; roadmap:
> Issue #119 checkpoint). It replaces the previous "not yet a product
> Fact" stub: the F4 propositions, truth classes, evidence cells and
> their writer/reader rules below are now FROZEN; everything not
> stated remains governed by the general Phase-F rules. This amendment
> does not by itself merge any production code; it defines what the
> F4 implementation must realize. A pre-merge review
> (F4-GATE-CORRECTIVE-1) then moved monotonicity from a reader-side
> clamp to the writer-side publication — see "Derivation ownership";
> the propositions themselves were not changed.

F4 creates no new Plugin, Capability, Fact kind, fact authority,
lifecycle noun, or global store (D13/D14.1). Its only mechanism surface
is one session-owned, episode-scoped evidence cell — published
monotonically by the render leg, read purely by the observation. The
cell is an owned resource of the existing Plugin boundaries, handed to
the render leg the same way the D14.7 render gate already is (D6); it is
not a Capability and not a Plugin. P1–P5 are NOT triggered: the cell is
bound at activation and never replaced live, so no old/new
realtime-world overlap exists.

**Position proposition (frozen).** Product Position for one playback
episode is the application-facing **Projection**:

> the source-relative location of device-consumed presentation for the
> current episode's render stream, in source PCM frames: a monotone
> non-decreasing sample published by the render mechanism as
> `handed-off-so-far − queued tail`, read by the application as one
> pure load of that sample.
>
> "Device-consumed" means the output engine has taken those frames out
> of this stream's device buffer for rendering. The proposition
> deliberately excludes all latency downstream of that point (engine
> queue, hardware, DAC): the acoustic instant for the boundary frame is
> later by that unmeasured remainder. It is therefore a bounded
> device-consumption estimate, never a measurement of the acoustic
> instant and never an audibility claim.

**Derivation ownership (frozen).** The subtraction happens on the render
leg's own execution path, where the handed-off total and the tail
reading both live as mechanism-local values, and monotonicity is owned
by the publication rather than by the reader:

```text
writer (render leg — one execution path owns both inputs)
    handed_off   plain local accounting of the frames this episode has
                 submitted into the device buffer
                 (read_frames -> ReleaseBuffer(n), wasapi steady loop)
    tail         the leg's own GetCurrentPadding reading
    publish      published = max(published,
                                 handed_off - min(tail, handed_off))
                 one monotone non-decreasing relaxed update of the
                 session-owned cell

reader (D14.2 observation seam)
    position     one pure load of the published cell
                 (undefined until the leg's first publication)
```

A **reader-side clamp is REJECTED** (F4-GATE-CORRECTIVE-1). `observe()`
is one coherent *pure* read of the episode (D14.2) and its purity is
part of the contract — repeating it changes nothing and it settles
nothing. A monotone clamp cannot live inside that read: it would make
the read stateful/mutating, or push Position into application-local
presentation state (which is then no longer the episode's projection but
UI truth), or relocate the same mutation into the session's read path.
Publishing monotonically on the writer side dissolves the conflict — the
reader needs no state because the cell it reads already applied it. The
rejected shape survives only as an executable negative control in the
evidence crate, never as a product rule.

Truth classes:

```text
handed_off       Mechanism-local accounting, NOT a published cell.
                 The render leg's own count of the source frames it
                 took from the session-owned edge and submitted into
                 the device buffer (`read_frames` → `ReleaseBuffer(n)`,
                 wasapi steady loop; a handed-out block that is never
                 submitted can only be a terminal abort, after which the
                 projection is withdrawn). Monotone within one episode
                 (within one seek epoch once F5 exists). It lives only
                 inside the render execution path; it is the
                 projection's base, never a product surface. This is NOT
                 the decoded count (decode runs ahead).

tail             Mechanism Evidence, read on that same execution path:
                 the output mechanism's own latest observation of its
                 queued-to-play tail (GetCurrentPadding), in source
                 frames. NOT monotone. This is the same platform reading
                 D14.7 already trusts for output-tail quiescence — the
                 establishment latch keeps its F3 meaning; D14.8 selects
                 the raw padding reading as the derivation's subtrahend,
                 which is a new narrow decision made here, not a silent
                 reuse of the establishment evidence. No extra device
                 call is needed: the park slice and the steady loop
                 already take this reading.

position_evidence  Mechanism Evidence cell: session-owned, episode-
                 scoped, bound at activation. The render leg publishes
                 it monotonically (the `publish` rule above) once per
                 render-loop iteration, per park slice, and on the drain
                 path. This is the ONLY cross-thread surface F4 adds.
                 Never a Fact; it carries no exactness claim beyond the
                 mechanism's own instant (see the `freshness` rule).

Position         Projection (PBK-001 §2.3 sense): the application-
                 facing visibility of `position_evidence` — one pure
                 load while the episode is unsettled (D14.2 read seam).
                 Never a Fact; never a correctness basis for control,
                 lifetime, settlement, resource lifetime, mechanism
                 wakeup, or K0 lifecycle transitions.
```

Frozen behavioral rules:

```text
unknown ≠ zero    Position is undefined (None) until the render
                  mechanism publishes its first sample (the
                  stream-start evidence); before that, no position
                  exists — not zero. It is undefined again once the
                  terminal Fact is committed: the projection is
                  withdrawn with the mechanism (no final-position latch
                  storage is earned — the read seam simply stops
                  deriving it). A never-activated / open-aborted
                  episode can therefore never fabricate one.

publication       The single writer publishes monotonically:
                  `published = max(published, handed_off −
                  min(tail, handed_off))`, relaxed. Monotonicity is
                  owned by the publication, not by any reader: a pure
                  load cannot go backward because the cell it reads
                  cannot. The max is not an accuracy repair applied to
                  a torn read — there is no cross-cell read anywhere in
                  F4: both inputs belong to one execution path and are
                  consumed in the same loop iteration.

freshness         Each published sample is exact only for the instant
                  the writer read the tail. The reader holds no state
                  and MUST NOT be promised any bound on how old its
                  sample is: the age of what a pure load returns is the
                  reader's own poll interval plus the mechanism's
                  publication cadence — an asynchrony property of the
                  reader's schedule, not a concurrency correctness
                  invariant (relaxed atomics give coherence per
                  location, never freshness). The mechanism owes the
                  reader exactly three things: never backward (between
                  committed seek discontinuities — the D14.8 seek rule
                  below records the F5-GATE re-scope), never
                  above its own handed-off accounting, never
                  fabricated. Any "± one in-flight block"-style error
                  bound is withdrawn as a contract; a measured
                  publication cadence is mechanism evidence, not a
                  promise.

writer lifetime   Each writer exists only while its mechanism is live.
                  The render leg is a teardown-owned resource, so after
                  its teardown no publication can occur, and after the
                  terminal Fact the projection is not derived at all —
                  a late publication by a dying leg is unobservable.
                  No post-settlement timeline state is written and none
                  is stored.

pause             Position freezes exactly where D14.7 Paused
                  establishes: submissions stop at render-gate
                  engagement (measured post-command advance: one
                  in-flight block), the park slices keep publishing
                  while the tail drains, the published sample rises to
                  the frozen handed-off total, and it stops moving at
                  tail quiescence — the same instant Paused establishes.
                  Before that instant (pause command issued, tail still
                  draining) the projection truthfully keeps advancing;
                  it MUST NOT be frozen at command time.

EOF / terminal    Decoder EOF does not move Position to Duration. The
                  published sample rises to the exact handed-off total
                  as the device drains (at D11 Completed the Drained
                  verdict is the same tail == 0 reading). The handed-off
                  total equals the exact decoded total only because the
                  Completed path hands out every produced frame: the
                  edge producer blocks rather than dropping, and
                  buffered frames are abandoned only on a
                  stopped/failed terminal — verdicts that do not claim
                  Drained. Measured on the clean probe run:
                  384000 == 384000. Whether the reported Duration is
                  also equal is NOT guaranteed and MUST NOT be
                  asserted or forced. Stopped/Failed settle the same
                  withdrawal rule.

seek (F5 rule)    The accumulator is episode-local and its
                  source-relative meaning is defined only within one
                  seek epoch; the monotone publication MUST NOT mix
                  pre- and post-cutover handed-off totals. How the
                  accumulator is rebased and how the seek's landing
                  offset enters the projection are owned by the F5
                  cutover decision (no longer OPEN: frozen 2026-09-17
                  by the F5-GATE amendment in D14.5 — same-cell,
                  writer-side rebase at commit release; basis = the
                  decoder's reported actual landing; unknown landing
                  withdraws the projection; monotonicity scope amended
                  to "between committed discontinuities"): F4 freezes
                  only the no-mixing constraint and earns no base term,
                  Generation, SeekId, or TimelineSegment for it.
```

**Duration proposition (frozen).** Product Duration is **optional
source-scoped Mechanism Evidence**: the duration the decode mechanism
reports for the source at probe/open time (SongCore
`song_probe` / audio-stream-info `duration_us`; the no-timestamp
sentinel means unknown):

```text
owner             Decode provider (probe evidence), relayed once by
                  the session as episode evidence — the same shape as
                  source_format.
truth class       Mechanism Evidence. NEVER a Fact; it carries no
                  exactness guarantee.
unknown           unknown stays unknown (absent / sentinel → None);
                  do not estimate to fill the UI.
exactness         NOT exact in general. Measured (f4duration): the
                  intact curated corpus matched the exact decoded
                  total (delta 0), but a 30%-truncated CBR MP3 still
                  reported the full 4.0 s while only 2.82 s was
                  decodable (+1.18 s, ~29% overclaim); truncated FLAC
                  failed decode cleanly. Only the actual decoded
                  total at decode EOF is exact, and it exists only at
                  the end; it is terminal consumption truth, NOT the
                  product Duration.
terminal rule     At D11 Completed the final consumed position equals
                  the exact decoded total; no claim binds it to the
                  reported Duration evidence.
```

**Non-authority rule.** Position/Duration MUST NOT be inferred from
buffer occupancy (`buffered_frames`), K0 lifecycle, FiberState, logs,
decoded-frame counts alone, handed-off counts alone, UI state, or ad-hoc
device observations outside the published sample above. Position is not a
fourth transport state and does not join the D11 terminal contract;
the D14.7 Paused projection and Position are independent derivations
over partially shared evidence (tail quiescence), and neither is the
correctness basis of the other. Product status MUST NOT claim acoustic
audibility — the frozen proposition is device-consumed presentation
position in source frames; physical-audibility claims from software
counters remain forbidden.

**Realtime cost boundary (frozen shape).** The render leg keeps one
plain local counter and derives from its own two values: per loop
iteration / park slice / drain observation it performs one relaxed
monotone RMW on one episode-local cell, with no new device call (the
padding reading already exists in the loop and in the D14.7 park
closure). No locks, no allocation, no per-frame accounting, no generic
dispatch, no K0 visibility. The reader performs one relaxed load inside
the existing observation read: no lock of its own, no reader-side state.
Nothing enters the per-quantum path beyond O(1) work on a cell that is
cache-hot on the rendering thread.

**Rejected alternatives (evidence-backed).**

- decoded-frame count as product position: runs ahead of consumption
  (edge + device queue advance while paused) — Mechanism Evidence
  only, not exposed.
- handed-off count alone: ahead of consumed by up to one device buffer;
  kept only as the mechanism-local derivation base.
- two separately published cells with a reader-side monotone clamp:
  rejected (F4-GATE-CORRECTIVE-1). It cannot be realized inside the
  frozen pure-read seam (see derivation ownership), and it is
  unnecessary once the writer publishes monotonically. The torn-pair
  counterexample that motivated this collapse is kept as an executable
  negative control in the evidence crate — never as a product rule.
- IAudioClock: on the exercised endpoint, GetFrequency numerically
  matched the initialized stream format's byte rate (48 kHz float32 →
  384000 = 48000 × 8; 44.1 kHz AUTOCONVERTPCM → 352800). That is an
  observation about this endpoint, not a general rule: the API's
  documented contract only guarantees that the frequency unit is
  compatible with GetPosition's unit, and the unit may vary across
  streams/devices. Either way the clock requires unit/origin
  interpretation and adds no source-relative truth the algebra lacks
  (after a byte→frame conversion it tracked the same consumed quantity
  within ~8 ms) — rejected as the larger mechanism
  (smallest-mechanism razor); re-earnable only by a new narrow
  authority decision.
- Fact-promoting Position: no designated semantic authority or
  exactness contract is earned; the Projection serves every current
  product proposition. Promotion remains possible only through a
  future narrow authority decision.

**Representation deliberately NOT frozen here.** The concrete Rust
spelling of the evidence cell (including how the monotone value encodes
"undefined" while remaining a pure read), the observation
fields/methods, the Decode seam that surfaces the probe duration, and
the withdrawal spelling are F4-implementation decisions under D14.10;
they must not create new architecture nouns. `PlaybackSnapshot` and
global stores remain forbidden.

> 2026-09-17 implementation note (F4-IMPLEMENTATION-1). Representation
> and conformance only — the propositions above are unchanged, and the
> choices below are replaceable representation under D14.10. The
> implementation realized them as: one `PositionEvidence` cell in
> `qianqian-audio-api::ports` (a session-owned episode resource handed
> to the render leg through `RenderRequest`, like the D14.7 gate; not a
> Capability and not a Plugin), storing `position + 1` in one relaxed
> `AtomicU64` with the zero-initialized value meaning undefined and a
> saturating encode at the top of its legal domain; the render leg keeps
> `handed_off` as a plain local and publishes from the padding readings
> it already takes (steady loop, park slices, drain path), crediting the
> total only after a successful `ReleaseBuffer`; the observation exposes
> `position: Option<u64>` (source PCM frames) and
> `source_duration: Option<Duration>`, both admitted by the episode-
> handle public-surface allowlist as the explicit implementation event.
> Conformance reading recorded because it is easy to get backwards:
> only the position projection is withdrawn at the terminal Fact
> (`None`, with no cell write and no stored final position), while the
> duration evidence is source-scoped and stays observable after
> settlement — exactly the shape the frozen text gives it ("the same
> shape as `source_format`").
>
> 2026-09-17 corrective (F4-IMPLEMENTATION-CORRECTIVE-1, same PR). The
> observation gate above was initially keyed on the terminal Fact alone;
> fresh review showed that is not sufficient for the frozen
> never-activated proposition, and the reachable path is the render
> mechanism's own open-abort protocol. Activation's last fallible step
> (the decode-worker spawn) runs after the render stream is open, and an
> open-aborted leg parks at the D14.7 gate on its way to a failed open —
> publishing from the park slice it takes there — so an activation that
> raises can leave a cell holding `Some(0)` for an episode that never
> played. The gate therefore reads two conditions inside the same lock
> hold: no committed terminal Fact AND no recorded activation failure.
> This is a conformance fix under D14.8 ("never-activated / open-aborted
> episode fabricates no Position"), not a new rule, and it introduces no
> new state: `activation_failure` already existed as the activation
> diagnostic. Duration is deliberately unaffected (source-scoped
> evidence), and the cell still holds whatever the mechanism published —
> withdrawal stays an observation gate, with no cell write, so a late
> publication by a dying leg cannot resurrect the projection.

### D14.9 Volume / Device switch — no generic state invention

> **2026-09-18 amendment (F6-AUTHORITY-PROMOTION-1, volume
> owner/semantics).** Design source: the merged #155 package's
> VOLUME-GATE; Microsoft Learn WASAPI documentation cited there. The
> volume authority/mechanism question left this D14.9 opening; the
> propositions below are FROZEN, with exactly one element still
> pending: the physical realtime apply placement (V-PROBE).
>
> **Volume owner / semantics (frozen).**
>
> ```text
> desired_volume ∈ 0..=100 (integer, clamped; step 5; no
>                dB curve promise; 50 makes no
>                "half perceived loudness" claim)
> truth class:   application configuration (Command family — routed
>                like pause intent, D14.7 precedent), NOT a Fact, NOT
>                mechanism evidence about loudness, NOT a fourth
>                transport anything
> owner:         the reference-player App owns it; it survives
>                episode replacement because replacement rebuilds the
>                episode, not the App; each fresh episode's output
>                mechanism receives the App's current desired level
>                and applies it at stream open (before first
>                meaningful submission). No persistence to disk in v1.
> realization:   stream-local. App → episode seam command (idempotent)
>                → session-owned output-level control (an owned
>                episode resource — NOT a Capability, NOT a Plugin,
>                NOT a Fact) → carried to the render mechanism in the
>                RenderRequest (representation open) → mechanism
>                applies it per stream. No global mixer ownership; no
>                endpoint master volume; no ISimpleAudioVolume as the
>                player setting (session-scoped, SndVol-coupled,
>                persistent across restarts); no software PCM
>                multiplication on Windows (permanent per-sample RT
>                tax; recorded as the portable FALLBACK for platforms
>                without a stream-local control).
> apply points:  applied once at stream open, and re-applied at the
>                render loop top when the routed value changed (one
>                relaxed load + compare per iteration); never inside
>                the quantum between GetBuffer and ReleaseBuffer. THIS
>                PLACEMENT IS CANDIDATE, NOT FINAL — (grounded
>                2026-09-19 by V-PROBE; see the VOLUME-IMPLEMENTATION-1
>                note below)
> read side:     the TUI value means exactly the App's desired stream
>                factor — NOT the effective acoustic level, NOT the
>                Windows session master, NOT the endpoint volume, NOT
>                a mechanism readback (GetAllVolumes stays unexposed to
>                the product read side). Guarantee = factor
>                independence (V2a/V2b), never audible independence;
>                the session-master factor SndVol controls remains an
>                independent multiplier of audible output that the
>                player neither owns, displays, nor writes.
> zero / mute:   Volume = 0 suffices for v1; no separate Mute state.
> terminal:      a volume command itself NEVER establishes or settles
>                terminal truth; it is non-terminal, same-episode, no
>                PCM topology cut — it never flushes the edge, parks
>                the leg (beyond the loop-top apply), resets Position
>                or creates a discontinuity. A failed control call is
>                mechanism evidence graded by what it reveals: an
>                ordinary recoverable failure may warrant only a
>                diagnostic; device/service loss (e.g.
>                AUDCLNT_E_DEVICE_INVALIDATED) routes through the
>                EXISTING output/device-failure policy, under which D11
>                may settle `Failed`.
> ```
>
> **Windows candidate mechanism (selected on documentation evidence;
> physical RT placement PENDING V-PROBE — grounded 2026-09-19, see
> the VOLUME-IMPLEMENTATION-1 note below):** `IAudioStreamVolume` via
> `GetService` on the episode's own render client; `SetAllVolumes`
> across all channels (level/100.0); stream-local by contract
> (Microsoft: "controls the volume of an individual stream in a
> session relative to the other streams in the session"). Its physical
> facts on the exercised endpoint do not close before V-PROBE
> (V1a same-process/same-session stream isolation, V1b other-process,
> V2a player→mixer factor independence, V2b mixer→stream-factor
> independence with audible change EXPECTED, V3 lifecycle
> persistence, V4 apply-placement perturbation measurement, V5
> failure routing with the log-and-pretend shape FAILING). Reopen
> conditions, two tiers as designed: a V1a cross-stream coupling
> failure, or either V2a/V2b factor-writing direction, reopens **the
> mechanism decision itself** (the IAudioStreamVolume selection, the
> VOLUME-GATE §5-B2 candidate); a V4/V5 finding that the apply point
> materially perturbs the render leg reconsiders **the
> apply-point/ownership mechanism**. Until V-PROBE is green, a coding
> agent MUST NOT treat the apply placement — or the mechanism
> selection against a failed V1a/V2a/V2b — as frozen.
>
> **2026-09-19 implementation grounding (VOLUME-IMPLEMENTATION-1,
> evidence `experiments/v-probe/` PR #161).** The pending physical
> facts are measured (V_PROBE_GREEN ×3, 21/21 scenario-runs): V1a/V1b
> isolation, V2a/V2b factor independence in both writing directions,
> V3 lifecycle persistence, V4 loop-top apply boundedness (median
> ~0.26 ms, p99 ≤ 0.51 ms, position advancing and monotone, designed
> coalescing of sub-cadence routing recorded), V5 typed failure
> signals. NO reopen condition fired — the mechanism selection stands,
> and the candidate apply placement is grounded: it leaves the D14.10
> stop list. Representation recorded (representation only, no
> proposition changed): the session-owned `OutputLevel` cell rides the
> RenderRequest; the episode seam's command is the idempotent
> `request_output_level(0..=100)`; the App routes its desired level
> BEFORE activation (constructor argument) and on every change.
> Failure grading realized: recoverable apply failures are one
> bounded diagnostic at the last applied level; device invalidation
> routes through the existing device-failure path.
>
> Representation still open: non-Windows mechanisms (per-platform
> realization behind the same App-owned desired level).

What is already forbidden:

```text
no K0 playback state
no global mutable control store
no per-quantum plugin dispatch
no generic EventBus as control plane
no new Plugin merely named Volume/DeviceSwitch
```

Device switch authority / replacement mechanism remains OPEN (D14.9
pre-amendment scope unchanged) and stays a D14.10 stop-list item.

### D14.10 What Flash/coding agents may and may not decide

A coding agent MAY choose:

```text
private helper names
private struct layout
lock vs equivalent local synchronization when semantics are unchanged
ordinary error plumbing
test organization
small local refactors needed to realize the frozen contract
```

A coding agent MUST STOP and report an authority gap before choosing:

```text
new semantic state or terminal variant
new fact authority
new Plugin / Capability / K0 primitive
new lifetime taxonomy
Generation / Window / epoch / global playback store
consumer-driven vs authority-driven semantic commit
position/duration propositions beyond the frozen D14.8 minimum
preload/gapless/overlap topology
device switch authority / replacement mechanism
```

(The pause/resume semantic commit point and minimum mechanism left this
list when D14.7 froze them; the position/duration propositions left it
when D14.8 froze theirs; the seek physical-output cutover mechanism
left it when the 2026-09-17 F5-GATE amendment froze it inside D14.5;
the F6 fresh-source/config handoff mechanism and the playlist/queue
authority left it with the 2026-09-18 F6-AUTHORITY-PROMOTION-1
amendment (D14.6). Anything beyond the frozen minima still requires a
narrow authority
decision.)

The rule is intentional: **OPEN means “not authorized yet,” not “Flash may invent the missing architecture.”**
