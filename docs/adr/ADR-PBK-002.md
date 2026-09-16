# ADR-PBK-002 — Canonical Architecture Vocabulary and Real Playback Composition

| Field | Value |
|---|---|
| Status | **ACCEPTED** |
| Date | 2026-09-12 |
| Accepted after | PR #118 corrective adversarial review: taxonomy, historical provenance, authority routing, vocabulary-gate scope |
| Supersedes | — |
| Amends | ADR-PBK-001 current vocabulary and earned playback composition decisions; PBK-001 foundations / Fact contract / P1–P5 remain unchanged |
| Amended | 2026-09-14 — §17 D11 episode terminal outcome semantic authority; 2026-09-14 — §18 D12 Everything-is-a-Plugin taxonomy corrective (Issue #138); 2026-09-14 — §19 D13 Plugin admission invariant (PR #139); 2026-09-15 — §17 D11 terminal-settlement ownership corrective + §20 D14 Phase-F playback semantic execution guard (formal evidence PR #142, reality audit Issue #141); 2026-09-16 — §20 D14.7 pause/resume mechanism + establishment freeze (F3-GATE, evidence `experiments/f3-pause-mechanism/`); 2026-09-16 — §20 D14.7 pause establishment corrective: render engagement ≠ audible pause; Paused gated on output-tail quiescence evidence and demarcated as a non-authoritative Projection (F3-GATE-CORRECTIVE-1, same evidence crate) |
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
seek lifetime beyond D14's current same-episode contract
advanced open/replacement mechanisms beyond D14's no-overlap v1 semantic contract
next-track overlap/preload lifetime
preload lifetime
gapless lifetime
multi-session topology
session construction/config representation beyond D14's current Phase-F seam
```

特别是：当前 `ComponentSpec` 捕获 file/completion 的实现不自动成为未来 `open` 的永久 contract；D14 只冻结 Phase-F 当前最小 **old episode fully retires before new episode becomes live** 的 v1 语义约束。怎样把新 source/config 送进 fresh Playback Session definition 仍是 F6 CONFIG-MECHANISM-OPEN，必须另行窄门裁决。

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
position / duration authority
seek physical-output cutover mechanism beyond D14's frozen stale-PCM invariant
open/session CONFIG mechanism under D14's no-overlap v1 semantic contract
playlist / queue authority and whether it earns PlaylistPlugin
next / previous navigation policy beyond D14's no-overlap replacement semantic shape
volume authority / mechanism
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
     Paused/Resumed Projections are NOT this item — they are
     non-authoritative derived visibility, not transport lifecycle
     states)
position / duration authority
seek product-state vocabulary / actual-landing authority beyond D14 minimum
source identity / playlist authority / navigation policy
volume / device-switch authority
PlaybackFacts publication topology beyond D14 read seam
PlaybackSnapshot
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
only its semantic spine:

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

`seek command accepted`, `decoder repositioned`, `old PCM invalidated`, `physical output cutover`, and `actual landing` are not to be collapsed into one vague success bit if the implementation exposes them internally.

**The physical output cutover mechanism is still OPEN.** F5 production implementation must stop at that gap until a narrow mechanism decision proves one of the existing-output reset/reopen/minimal-cutover choices. A coding agent may not invent `Generation`, a generic cache protocol, or a new runtime to bypass this gate.

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

The exact configuration/handoff mechanism that creates a fresh session definition for the new source is still **F6 CONFIG-MECHANISM-OPEN**. Current `ComponentSpec` definitions are fixed and current `playback_session_spec(file, ...)` captures source config, so a coding agent MUST NOT invent hot component replacement, a mutable global source slot, a registry, or another config channel to make Open work. F6 production code remains blocked until that narrow mechanism decision is explicitly made.

Consequences already frozen:

- Open creates a new playback episode; it is not Seek。
- Next/Previous are navigation/selection decisions followed by the same no-overlap Open semantics; they are not new K0 primitives or data-plane protocols。
- An audible gap is acceptable in v1。
- Because old/new playback episodes do not overlap, `Generation`/`Window`/Realtime Audio Runtime is not earned by Open/Next/Previous v1。
- The old episode keeps any already-committed terminal truth; replacement cannot relabel it。
- If intentional replacement stop was recorded before that episode's terminal evidence became decisive and no higher-precedence failure wins, the existing `Stopped` terminal variant is sufficient. Do **not** invent `Superseded` / `Preempted` as a new terminal Fact for v1. Replacement cause may remain diagnostic/control context if needed。

Playlist/queue selection authority remains OPEN. The first headless Open/Next/Previous slice may use only the selection source explicitly authorized by its issue/task; it must not create a global playlist authority to make the command convenient.

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
Paused / Resumed           application-facing derived Projection over
                           the above (PBK-001 §2.3 sense); NOT a
                           semantic Fact and NOT a correctness basis
                           (see the non-authority rule below)
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
Mechanism Evidence; it must never be promoted to a Fact, a
position/Duration source (F4/D14.8), or a P1–P5 trigger (no old/new
realtime-world overlap exists here).

Truthful product establishment is then the projection:

```text
Paused (projection)  ⇔ the episode has no committed terminal outcome
                       ∧ pause intent recorded ∧ render engagement
                       evidence observed ∧ output-tail-quiescence
                       evidence observed
Resumed (projection) ⇔ the episode has no committed terminal outcome
                       ∧ resume released ∧ disengagement evidence
                       observed
```

`Resumed` claims exactly: pause control is no longer established and
render submission is re-enabled. It does NOT claim new audio is already
audible — after disengagement, refilled frames still traverse the
device buffer before sounding, and audible-time semantics remain
F4/D14.8 territory. Either mechanism-evidence factor alone is not
establishment; a settled episode is never Paused or Resumed.

The conjunction is guarded by the episode's unsettled state: once a
terminal Fact commits, pause truth must not evaluate true on the
episode regardless of mechanism engagement still being latched, so
session settlement/teardown MUST release the pause gate (publishing
disengagement evidence) on the authority-owned execution/teardown path.

**Non-authority rule for the Paused/Resumed projections.** Paused and
Resumed create no new fact kind, no new fact authority, no fourth
terminal variant, no `Paused` semantic Fact and no
`Playing/Starting/Paused/Stopping` transport enum: the observation
surface gains pause-intent (command state) and
engagement/tail-quiescence (mechanism evidence) fields, and their
spelling is representation (the episode-handle public-surface
allowlist update is the explicit F3-implementation architecture event).
As Projections in the PBK-001 §2.3 sense they MUST NOT be used as the
correctness basis for resume legality, stop legality, teardown,
terminal settlement, resource lifetime, mechanism wakeup, or K0
lifecycle transitions; control/lifetime correctness uses the
authority-owned command/control state and/or the direct mechanism
state/evidence. Product status MUST NOT infer pause from worker
blocking, PcmEdge occupancy, FiberState, UI state, or ad-hoc WASAPI
observations outside this establishment chain.

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

### D14.8 Position / Duration — not yet a product Fact

F4 MUST NOT infer product position/duration from buffer occupancy, K0 lifecycle, or arbitrary decoder/output counters. Mechanism counters may exist as diagnostics/evidence, but product `Position`/`Duration` authority remains OPEN until a narrow authority decision defines the proposition and writer.

No `PlaybackSnapshot` or global state store may be introduced merely to make F4 convenient.

### D14.9 Volume / Device switch — no generic state invention

F7/F8 remain bounded by their current roadmap goals, but their authority/mechanism is not frozen here. A coding agent must not choose between session-owned control, output-provider control, episode replacement, stream replacement, or a generic control bus without a narrow authority/mechanism decision.

What is already forbidden:

```text
no K0 playback state
no global mutable control store
no per-quantum plugin dispatch
no generic EventBus as control plane
no new Plugin merely named Volume/DeviceSwitch
```

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
seek physical-output cutover mechanism
F6 fresh-source/config handoff mechanism
Position/Duration authority
playlist/queue authority
volume/device-switch authority
preload/gapless/overlap topology
```

(The pause/resume semantic commit point and minimum mechanism left this
list when D14.7 froze them; anything beyond the frozen D14.7 minimum
still requires a narrow authority decision.)

The rule is intentional: **OPEN means “not authorized yet,” not “Flash may invent the missing architecture.”**
