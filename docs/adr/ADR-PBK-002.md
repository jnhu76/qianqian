# ADR-PBK-002 — Canonical Architecture Vocabulary and Real Playback Composition

| Field       | Value |
|-------------|-------|
| Status      | **PROPOSED** |
| Date        | 2026-09-12 |
| Supersedes  | — |
| Amends      | ADR-PBK-001 (vocabulary section only) |
| Evidence    | PR #117 FIRST_AUDIBLE_SLICE |

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

很多 ADR-PBK-001 acceptance-time OPEN 已经通过 production evidence 挣得答案。

同时旧命名：

```text
kernel
core
runtime
host
```

开始产生语义重叠。

因此 ADR-PBK-002：

```text
does not invent architecture
```

而是：

```text
canonicalizes names
+
promotes earned production boundaries into normative decisions
```

---

## 2. Authority Relation

```text
ADR-PBK-001
    playback foundations / vocabulary constitution / planes / P1-P5

ADR-PBK-002
    current canonical role names
    + first production playback composition decisions
```

如果两者在当前 canonical vocabulary 上冲突：

```text
PBK-002 governs current vocabulary
```

但 PBK-001 的历史 rationale 仍然保留。

K0 authority：

```text
composition-kernel-0-design.md
```

仍然定义 K0 semantic contract。

PBK-002 不能偷偷扩大 K0 responsibility。

---

## 3. D1 — Canonical Vocabulary

### Qianqian App

| | |
|---|---|
| 定义 | 应用程序引导层；安装 desired composition、发起 shutdown |
| 拥有 | bootstrap、component admission、desired composition、top-level shutdown initiation |
| 不负责 | decode、PCM pumping、WASAPI render、playback semantics、realtime lifetime |

代码类型：`QianqianApp`
crate：`qianqian-app`

`Host` 不再作为 Qianqian application canonical architecture noun。

---

### Composition Kernel (K0)

| | |
|---|---|
| 定义 | 通用控制面内核；管理 existence、reachability、capability dependency、Fiber lifecycle、Effect ownership |
| 拥有 | desired → running composition、capability resolution、Fiber lifecycle、Effect ownership |
| 不负责 | PCM transport、audio scheduling、playback timing、realtime execution |

代码类型：`CompositionKernel`
crate：`qianqian-composition`
短名：`K0`

`Kernel` 从现在开始成为 reserved architecture noun。在 Qianqian 当前架构语言中：

```text
Kernel == Composition Kernel
```

---

### Component

| | |
|---|---|
| 定义 | Neutral bounded responsibility/resource unit |
| 关系 | Plugin ⊂ architectural Components |

---

### Plugin

| | |
|---|---|
| 定义 | Component that has earned participation in Composition Kernel lifecycle/capability protocol |
| 身份不由 | Rust trait、crate、DLL、thread、feature flag、audio stage、PCM transform 决定 |
| 当前表示 | ComponentSpec（substrate，非 frozen final Plugin API） |

---

### Fiber

| | |
|---|---|
| 定义 | Plugin definition 的 runtime episode；通过 Composition Kernel lifecycle protocol 管理 |

---

### Capability

| | |
|---|---|
| 定义 | composition-visible typed contract / reachability identity |
| 不是 | PCM bus、payload registry、per-block lookup |

---

### Service

| | |
|---|---|
| 定义 | executable object reached through capability |

---

### Effect

| | |
|---|---|
| 定义 | lifecycle-owned inverse / teardown obligation |

---

### Audio API

| | |
|---|---|
| 定义 | shared audio contracts / vocabulary；定义 PCM seams |
| 拥有 | PcmFormat、PcmDecode、DecodedPcmStream、AudioOutput、RenderStream、RenderPcmInput、DrainSignal |
| 不负责 | pump PCM、schedule audio、own threads、own playback、resolve composition、route payload |

crate：`qianqian-audio-api`

> `qianqian-audio-api` defines PCM seams.
> PCM payload does NOT "flow through the audio-api crate"。

---

### Playback Session

| | |
|---|---|
| 定义 | one playback episode ownership unit |
| 拥有 | playback-specific decode endpoint、song_handle lifetime、decode worker、bounded PCM edge、render stream、render-thread relationship、completion、EOF / stop orchestration |

crate：`qianqian-playback`

注意：不要把这句话扩大成 `forever one track == one PlaybackSession`。dynamic playback 尚未挣得。

---

### Realtime Audio Runtime

| | |
|---|---|
| 定义 | RT execution views、publication、retirement、quiescence、RT-visible lifetime legality、RT-safe control application |
| 状态 | **RESERVED / NOT IMPLEMENTED** |

预留 crate 名：`qianqian-audio-runtime`

---

### PCM Data Plane

| | |
|---|---|
| 定义 | decode → bounded PCM edge → render → device 的数据路径 |
| 关键约束 | kernel-free；per-block 无 capability resolve、无 Context lookup、无 Reconcile |

---

## 4. D2 — Kernel Reserved Word

> In current Qianqian architecture, Kernel is reserved for the
> Composition Kernel (K0).

因此 current architecture 禁止：

```text
MusicKernel
TransportKernel
AudioKernel
PcmKernel
PlaybackKernel
RealtimeKernel
```

历史材料除外。

Realtime side canonical noun：

```text
Realtime Audio Runtime
```

---

## 5. D3 — App Is Outside the Composition It Operates

```text
Qianqian App
        │
        │ installs / desires / initiates shutdown
        ▼
Composition Kernel
```

App 不自动成为 Plugin。

App 不负责：decode、render、PCM、playback timing、realtime execution。

不要定义 `App Plugin`。

---

## 6. D4 — Component vs Plugin

冻结语义关系：

```text
Component
    neutral bounded architecture unit

Plugin
    Component that has earned participation
    in Composition Kernel lifecycle/capability protocol
```

明确：

```text
Plugin != Rust trait
Plugin != crate
Plugin != DLL
Plugin != thread
Plugin != audio node
Plugin != every PCM stage
```

`ComponentSpec`：current K0 representation substrate，不是 frozen final Plugin API。

---

## 7. D5 — Mechanism Providers

基于 PR #117 production evidence 冻结：

```text
Decode Plugin
    long-lived decode mechanism capability provider

Output Plugin
    long-lived output mechanism capability provider
```

当前第一条真实路径里 provider 不拥有：

```text
one current song
one current song_handle
one current render stream
one playback edge
```

这些属于 Playback Session。

---

## 8. D6 — Playback Session Ownership

> For the current static playback architecture,
> one Playback Session is the ownership unit of one playback episode.

当前它拥有：

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

明确：

```text
This does NOT yet freeze:
track lifetime
seek lifetime
next-track lifetime
preload lifetime
gapless lifetime
multi-session topology
```

---

## 9. D7 — Audio API Is Contracts, Not Runtime

`qianqian-audio-api` 只定义 shared audio contracts。它不 pump PCM、schedule audio、own threads、own playback、resolve composition、route payload。

---

## 10. D8 — Composition / Data-Plane Firewall

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

K0 controls existence，不 transports PCM。

---

## 11. D9 — Realtime Audio Runtime Reserved

冻结名词 `Realtime Audio Runtime`，但 representation OPEN。

PBK-001 P1–P5 继续约束这部分。

不要在 ADR-PBK-002 设计 ArcSwap、RCU、epochs、hazard pointers、queues、graph API。

---

## 12. D10 — UI / Control Remains Open

只冻结负面规则：

```text
UI participation != automatically Plugin membership
```

禁止现在定义：PlayButtonPlugin、SeekBarPlugin、ToHostPlugin、FromHostPlugin、UniversalUiBus。

---

## 13. Architectural Regressions Forbidden

1. route PCM per block through Composition Kernel
2. equate Plugin with crate / DLL / feature / thread
3. add generic Plugin trait solely to confer Plugin identity
4. let Qianqian App become playback engine
5. move playback-session-scoped state into long-lived providers without new ownership evidence
6. introduce generic ToHost / FromHost / global EventBus as universal seam
7. infer realtime execution topology directly from composition topology
8. call UI widgets Plugins solely because they invoke playback
9. create a second architecture "Kernel" noun for PCM/realtime work
10. use "core" as a generic dumping-ground architecture layer

---

## 14. Open Decisions

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

PR #117 FIRST_AUDIBLE_SLICE 作为 evidence。

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

PR #117 本身不是 normative authority，是 evidence supporting the ADR。

---

## 16. Post-acceptance Vocabulary Amendment to ADR-PBK-001

ADR-PBK-002 canonicalizes:

```text
Host
    → Qianqian App

Base Composition Kernel / Composition Kernel
    → Composition Kernel (K0)

current shared "core" contracts crate
    → Audio API

Kernel type
    → CompositionKernel

AppRuntime type
    → QianqianApp
```

The semantic responsibilities remain unchanged.
ADR-PBK-002 is authoritative for current vocabulary.
