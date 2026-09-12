# ADR-PBK-002 — Canonical Architecture Vocabulary and Real Playback Composition

| Field | Value |
|---|---|
| Status | **ACCEPTED** |
| Date | 2026-09-12 |
| Accepted after | PR #118 corrective adversarial review: taxonomy, historical provenance, authority routing, vocabulary-gate scope |
| Supersedes | — |
| Amends | ADR-PBK-001 (current vocabulary and first earned static playback composition only) |
| Evidence | PR #117 FIRST_AUDIBLE_SLICE |

---

## 1. 为什么现在需要它？

PR #117 第一次实现了完整的 production playback path：

```text
Qianqian App
    ↓
Composition Kernel (K0)
    ↓
Decode Plugin + Output Plugin
    ↓
Playback Session
    ↓
pre-bound PCM Data Plane
    ↓
physical WASAPI device
```

这条真实路径回答了若干此前 OPEN 的边界问题，同时暴露出旧名称 `kernel` / `core` / `runtime` / `host` 的语义重叠。

本 ADR 不发明一套新架构；它做两件事：

1. canonicalize current role names；
2. 将 PR #117 已经由 production evidence 挣得的静态播放组合边界升级为 normative decision。

---

## 2. Authority relation

```text
ADR-PBK-001
    playback foundations
    plane separation
    Fact semantics
    realtime publication/reclamation P1–P5

ADR-PBK-002
    current canonical vocabulary
    first earned static playback composition boundaries

composition-kernel-0-design.md
    Composition Kernel semantic contract

composition-kernel-0-implementation-adr.md
    Composition Kernel representation / implementation decisions
```

PBK-002 只在 **current canonical vocabulary** 与本文明确记录的 **first static playback composition decisions** 上补充 PBK-001；它不覆盖 PBK-001 的 foundations、Fact contract 或 P1–P5。

如果 PBK-001 的历史 noun 与本文 current vocabulary 不一致，本文 governs current vocabulary；PBK-001 的历史 rationale 保留为 decision history。

---

## 3. D1 — Canonical vocabulary

### Qianqian App

| | |
|---|---|
| 定义 | 应用程序 composition root / bootstrap layer |
| 拥有 | bootstrap、component admission、desired composition、top-level shutdown initiation |
| 不负责 | decode、PCM pumping、WASAPI render、playback semantics、realtime lifetime |

代码类型：`QianqianApp`  
crate：`qianqian-app`

`Host` 不再作为 Qianqian application 的 canonical architecture noun。

### Composition Kernel (K0)

| | |
|---|---|
| 定义 | 通用 composition/control-plane kernel |
| 拥有 | existence、reachability、capability dependency、Fiber lifecycle、Effect ownership、desired → running composition |
| 不负责 | PCM transport、audio scheduling、playback timing、realtime execution |

代码类型：`CompositionKernel`  
crate：`qianqian-composition`  
短名：`K0`

当前 Qianqian architecture language 中：

```text
Kernel == Composition Kernel
```

### Component

> A neutral bounded responsibility/resource unit that may be composed by K0.

`Component` 是通用 taxonomy；它不因为拥有 `ComponentSpec`、Fiber 或 capability relation 就自动成为 Plugin。

### Plugin

> A boundary-justified, long-lived capability/lifecycle participant whose Component definition participates in the Composition Kernel common protocol.

Plugin 身份不由以下任一项单独决定：

```text
Rust trait
crate
DLL
thread
feature flag
audio stage
PCM transform
```

当前 Plugin 可以通过 `ComponentSpec` 进入 K0；但 `ComponentSpec` 只是 current composition substrate，不是 frozen final Plugin API。

当前已挣得的 Plugin classification：

```text
Decode Plugin
Output Plugin
```

`Playback Session` 当前分类为 **Component / playback-episode ownership unit**。本文不因它参与 K0 lifecycle/capability protocol 而自动把它升级为 Plugin。

### Fiber

> One live runtime episode/instance of a composed Component.

当该 Component 是 Plugin 时，Fiber 自然也是该 Plugin definition 的 live episode；但 Fiber 不是 Plugin-exclusive primitive。普通 composed Component 同样通过 K0 Fiber lifecycle 存在。

### Capability

> composition-visible typed contract / reachability identity.

不是 PCM bus、payload registry 或 per-block lookup。

### Service

> executable object reached through a Capability.

### Effect

> lifecycle-owned inverse / teardown obligation.

### Audio API

crate：`qianqian-audio-api`

定义 shared audio contracts / vocabulary 与 PCM seams，例如：

```text
PcmFormat
PcmDecode
DecodedPcmStream
AudioOutput
RenderStream
RenderPcmInput
DrainSignal
```

它不 pump PCM、不 schedule audio、不 own threads、不 own playback、不 resolve composition、不 route payload。

> `qianqian-audio-api` defines the seams. PCM payload does not “flow through the audio-api crate”.

### Playback Session

> For the current static playback architecture, one Playback Session is the ownership unit of one playback episode.

当前拥有：

```text
playback-specific decode endpoint
song_handle lifetime
decode worker
bounded PCM edge
render stream
render-thread relationship
completion
EOF / stop orchestration
```

这不等于永久冻结 `one track == one PlaybackSession`。seek / next / preload / gapless / multi-session topology 仍 OPEN。

### Realtime Audio Runtime

Canonical future noun：`Realtime Audio Runtime`。

状态：**RESERVED / NOT IMPLEMENTED**。

未来可能负责：

```text
RT execution views
publication
retirement
quiescence
RT-visible lifetime legality
RT-safe control application
```

预留 crate 名 `qianqian-audio-runtime`，但本 ADR 不创建它，也不冻结 representation。

### PCM Data Plane

> decode → bounded PCM edge → render → device 的 pre-bound data path.

关键约束：kernel-free；per block/quantum 无 capability resolve、Context lookup、Reconcile 或 generic Plugin dispatch。

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

历史 evidence 中的历史名除外。

Realtime 一侧 canonical noun 是 `Realtime Audio Runtime`，不是第二个 Kernel。

---

## 5. D3 — App is outside the composition it operates

```text
Qianqian App
        │
        │ installs / desires / initiates shutdown
        ▼
Composition Kernel
```

App 不自动成为 Plugin，也不负责 decode、render、PCM、playback timing 或 realtime execution。

不要定义 `App Plugin`。

---

## 6. D4 — Component vs Plugin

冻结：

```text
Component
    neutral bounded responsibility/resource unit

Plugin
    boundary-justified, long-lived capability/lifecycle participant
    represented today through the common Component/K0 substrate
```

因此：

```text
Plugin is an architectural classification of some Components.
Not every Component is a Plugin.
Not every Fiber belongs to a Plugin.
```

`ComponentSpec` 仍是 current K0 representation substrate，不是 final Plugin API。

---

## 7. D5 — Mechanism providers

基于 PR #117 production evidence：

```text
Decode Plugin
    long-lived decode mechanism capability provider

Output Plugin
    long-lived output mechanism capability provider
```

第一条真实路径里 provider 不拥有：

```text
one current song
one current song_handle
one current render stream
one playback edge
```

这些 playback-episode-scoped resources 属于 Playback Session。

---

## 8. D6 — Playback Session ownership

当前 static slice 冻结：Playback Session 是一次播放 episode 的 ownership unit。

本文 **不**冻结：

```text
track lifetime
seek lifetime
next-track lifetime
preload lifetime
gapless lifetime
multi-session topology
```

---

## 9. D7 — Audio API is contracts, not runtime

`qianqian-audio-api` 只定义 shared contracts/vocabulary。它不能演化成 PCM router、thread owner、playback engine、composition resolver 或万能 “core”。

---

## 10. D8 — Composition / data-plane firewall

```text
CONTROL / COMPOSITION PLANE

Composition Kernel
      ↓
capability resolution
      ↓ once / setup
pre-bound endpoints

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
per-block capability resolve
per-block Context lookup
per-block Reconcile
PCM through Composition Kernel
generic Plugin dispatch per quantum
generic EventBus carrying PCM
```

K0 controls existence；K0 does not transport PCM。

---

## 11. D9 — Realtime Audio Runtime reserved

`Realtime Audio Runtime` 这个 noun 冻结，representation OPEN。

PBK-001 P1–P5 继续约束它未来必须满足的 publication / retirement / quiescence / reclamation semantics。

本文不设计 ArcSwap、RCU、epochs、hazard pointers、queues 或 graph API。

---

## 12. D10 — UI / control remains open

只冻结负面规则：

```text
UI participation != automatically Plugin membership
```

当前禁止预先定义：

```text
PlayButtonPlugin
SeekBarPlugin
ToHostPlugin
FromHostPlugin
UniversalUiBus
```

未来真正挣得的 control/fact capabilities 可以另行命名，但本文不冻结其 topology。

---

## 13. Architectural regressions forbidden

1. route PCM per block through Composition Kernel；
2. equate Plugin with crate / DLL / feature / thread；
3. add generic Plugin trait solely to confer Plugin identity；
4. let Qianqian App become playback engine；
5. move playback-session-scoped state into long-lived providers without new ownership evidence；
6. introduce generic ToHost / FromHost / global EventBus as universal seam；
7. infer realtime execution topology directly from composition topology；
8. call UI widgets Plugins solely because they invoke playback；
9. create a second architecture `Kernel` noun for PCM/realtime work；
10. use `core` as a generic dumping-ground architecture layer；
11. infer `Plugin` merely from “has ComponentSpec / Fiber / capability relation”。

---

## 14. Open decisions

```text
pause semantics
seek semantics
next-track semantics
preload
gapless
playlist authority
volume authority
device switch
format switch
Processing Plugin
SRC fallback
UI adapter membership
PlaybackControl
PlaybackFacts
multi-session topology
Realtime Audio Runtime representation
P1–P5 concrete production mechanism
```

---

## 15. Evidence

PR #117 FIRST_AUDIBLE_SLICE 是 evidence，不是 normative authority。

真实 production path：

```text
Qianqian App
    ↓
Composition Kernel (K0)
    ↓
Decode Plugin + Output Plugin
    ↓
Playback Session
    ↓
kernel-free PCM data plane
    ↓
physical WASAPI device
```

---

## 16. Vocabulary amendment / historical mapping

Current canonical mapping：

```text
historical Application/Composition Host
    → Qianqian App

Base Kernel / Base Composition Kernel / Composition Kernel
    → Composition Kernel (K0)

historical shared contracts crate qianqian-core
    → qianqian-audio-api / Audio API

Kernel type
    → CompositionKernel

AppRuntime type
    → QianqianApp
```

历史文档在描述历史事实时可以保留旧 noun；current production/current derived docs 应使用 canonical noun。不要为了 grep clean 改写历史 provenance。

The semantic responsibilities remain unchanged. ADR-PBK-002 is authoritative for current vocabulary and the static composition decisions explicitly enumerated above.
