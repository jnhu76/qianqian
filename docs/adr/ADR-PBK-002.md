# ADR-PBK-002 — Canonical Architecture Vocabulary and Real Playback Composition

| Field | Value |
|---|---|
| Status | **ACCEPTED** |
| Date | 2026-09-12 |
| Accepted after | PR #118 corrective adversarial review: taxonomy, historical provenance, authority routing, vocabulary-gate scope |
| Supersedes | — |
| Amends | ADR-PBK-001 current vocabulary and earned playback composition decisions; PBK-001 foundations / Fact contract / P1–P5 remain unchanged |
| Amended | 2026-09-14 — §17 D11 episode terminal outcome semantic authority; 2026-09-14 — §18 D12 Everything-is-a-Plugin taxonomy corrective (Issue #138); 2026-09-14 — §19 D13 Plugin admission invariant (PR #139) |
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
    lifetime currently spans playback episodes

Output Plugin
    mechanism provider
    lifetime currently spans playback episodes
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
seek lifetime
open/replacement mechanism
next-track lifetime
preload lifetime
gapless lifetime
multi-session topology
session construction/config representation
```

特别是：当前 `ComponentSpec` 捕获 file/completion 的实现不自动成为未来 `open` 的 contract；Open 需要在其 phase 重新挣得最小 replacement/config mechanism。

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
12. let any mechanism provider, the App, or K0 establish episode terminal outcome outside D11；
13. broaden K0 Effect/kernel data to encode decoder/render/PCM domain internals merely because their owner is a Plugin；
14. admit a candidate as a Plugin when an existing Plugin can own it without losing composition correctness, lifecycle/dependency ordering, or independent replacement/withdrawal semantics（D13）。

---

## 14. Open decisions

```text
pause/resume semantics and minimal execution-control mechanism
position / duration authority
seek semantics and whether old/new RT worlds actually overlap
open/session replacement/config mechanism
playlist / queue authority and whether it earns PlaylistPlugin
next / previous semantics
volume authority / mechanism
device switch
format switch
Processing Plugin
SRC fallback
UI adapter Plugin membership
PlaybackControl
PlaybackFacts publication/topology
    (episode terminal outcome authority: DESIGNATED — §17 D11;
     all other playback fact kinds/publication topology remain OPEN)
multi-session topology
preload / gapless
Realtime Audio Runtime representation
P1–P5 concrete production mechanism
```

Reduction rule for all future phases:

> **Try the current Plugin/Fiber + owned-resource model first. Add Window/Generation/new runtime state only after a concrete counterexample proves the simpler model cannot preserve correctness.**

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
wasapi_output_plugin()    -> ComponentSpec
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

ADR-PBK-002 is authoritative for current vocabulary, Plugin/Fiber taxonomy, the Plugin admission invariant (D13), earned static playback composition, and D11 terminal-outcome authority designation.

---

## 17. D11 — Episode terminal outcome semantic authority

> 2026-09-14 amendment (F2-TERMINAL-OUTCOME-AUTHORITY-ADR-1; evidence report: PR #135; roadmap: Issue #119)。
>
> D12 taxonomy corrective does not change this fact contract. It changes only the **current composition realization** of the designated semantic role.

### Decision

> **The designated semantic authority for episode terminal outcome is the Playback Session semantic role for one playback episode.**

```text
fact kind:            episode terminal outcome
subject scope:        one playback episode
authority role:       Playback Session semantic role for one playback episode
current variants:     Completed / Stopped / Failed
current realization:  SessionCompletion / resolver (replaceable Rust detail)
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

### Current realization（非 normative）

```text
production code:  crates/qianqian-playback/src/completion.rs
                  （SessionCompletion 保存 eventual SessionOutcome；其 resolver
                  观察 decode_failure / worker_terminal / DrainVerdict /
                  stop_requested 并 memoize 一个 terminal outcome）
                  crates/qianqian-playback/src/session.rs（episode lifecycle）

resolver 精确 precedence：保留在 evidence report PR #135 Appendix A，本文不冻结。
若改变 precedence 会改变上述外部语义命题本身，需回到 authority review。
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
    at terminal semantic resolution, the aborted episode had recorded
    stop intent and no higher-precedence failure classification won.

    This is non-causal: it does not claim the user's stop caused the abort.

Failed:
    the episode is classified as terminal failure according to the
    Playback Session authority's decision contract.

    Failure class is selected by semantic precedence, not chronological
    first-failure; diagnostic stage text is not frozen.
```

一个 playback episode 至多有一个 terminal outcome；一旦 commit 即不可改写。这是 cardinality/immutability contract，不是 liveness 承诺。

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

Decode Plugin / Output Plugin / PcmEdge / K0 都不是该 fact 的 designated authority。Exact resolver precedence remains current realization; any change that changes the external propositions returns to authority review.

### App / K0 / projection firewall

Qianqian App 不是 playback semantic authority；K0 lifecycle facts 不是 playback terminal facts；projection/read-side remains non-authoritative per PBK-001 §2.3。

`source_format` / `activation_failure` remain mechanism observations/diagnostics, not this fact authority。

### Still OPEN

```text
Playing / Starting / Paused / Stopping semantics
position / duration authority
seek / source identity / playlist / next / previous / volume / device switch
PlaybackControl / PlaybackFacts publication topology / PlaybackSnapshot
EpisodeId / Generation / Window
multi-session / preload / gapless
Realtime Audio Runtime representation
```

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
```

Only a concrete counterexample may earn additional runtime concepts such as Window, Generation, extra Plugin boundaries, or a Realtime Audio Runtime mechanism.

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
                                                                          仍 OPEN（D6）
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
