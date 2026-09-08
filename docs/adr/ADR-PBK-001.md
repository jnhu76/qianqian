# ADR-PBK-001：播放时间轴、媒体会话、PCM 数据面与共享状态边界

* **状态**：PROPOSED / 待共同 Review
* **日期**：2026-09-08
* **作用域**：Qianqian Playback Architecture / ARCH-003
* **不重开**：Base / Composition Kernel K0
* **关联权威**：

  * `docs/architecture/component-boundary-a0.md`
  * `docs/architecture/composition-kernel.md`
  * `docs/architecture/composition-kernel-0-design.md`
  * `docs/architecture/composition-kernel-0-implementation-adr.md`
  * Issue #46 / #53 / #67 / #70
  * PR #66 / #68 / #69 / #71
* **历史行为 Oracle**：

  * `research/playback-reference-v1`
  * Issue #49
  * 已验证的 seek / physical flush / render accounting / ENDED 行为

---

# 1. 背景

Base / Composition Kernel K0 已经完成，其职责已经明确：

```text
Context
Capability
Fiber
Effect
Reconcile
```

Composition Kernel 管理：

```text
reachability
ownership
lifetime
dependency
binding
provider withdrawal
desired composition → running composition
```

它属于 **Control Plane**。

它不得理解：

```text
Track
PCM
codec
FFmpeg
WASAPI
播放进度
seek
playlist
```

同时，Qianqian 的下一阶段需要回答另一组问题：

```text
播放器的时间到底由谁拥有？
一首正在播放的媒体和下一首待播放媒体是什么关系？
seek 是否可以统一为窗口切换？
PCM 应该成为多大范围的公共数据面？
软件 generation 与物理设备中已经提交的声音如何区分？
全局共享状态如何避免退化成 GlobalPlayerState？
volume 应由播放器处理 PCM，还是修改系统设备音量？
```

本 ADR 对上述问题进行边界冻结。

---

# 2. 核心决策摘要

Qianqian 的播放架构分成四个不同维度：

```text
Composition Kernel
    管空间组合

Transport Kernel
    管播放器时间结构

Music Kernel
    管音乐产品语义

Canonical Audio Data Plane
    管实际 PCM 数据流
```

并引入：

```text
TrackSession
ActiveWindow
PreparedWindow
MediaSpan
Generation
Physical Fence
PlayerSnapshot
```

作为播放阶段的主要概念。

---

# 3. D1 — Composition Kernel K0 不重开

本 ADR **不得修改** Generic Composition Kernel 的既有职责。

Composition Kernel 继续只负责：

```text
plugin/component topology
capability reachability
binding ownership
Fiber lifetime
Effect lifetime
Reconcile
provider withdrawal
```

它明确不得拥有：

```text
PCM
TrackSession
MediaSpan
playback cursor
seek generation
rendered position
PlayerGain
ENDED
```

冻结：

> **Composition Kernel owns runtime composition topology and lifetime, not playback time or media payload.**

---

# 4. D2 — 引入播放器唯一的时间轴 Authority

当前讨论中的 `TrackKernel` 实际表达的不是“一首 Track”，而是：

> 整个播放器当前正在什么时间位置运行，以及哪一段媒体拥有继续输出的资格。

因此本 ADR 暂定名称：

```text
TransportKernel
```

而不是：

```text
TrackKernel
```

以避免 `Track` 与单曲媒体实例混淆。

TransportKernel 是播放器的 **timeline authority**。

它拥有：

```text
playback cursor
active temporal window
prepared temporal window
frontiers
generation
MediaSpan authority
window promotion
window invalidation
discontinuity execution
physical-cut coordination state
```

它回答：

> **播放器现在到底播哪一段？下一段准备到了哪里？什么时候可以切过去？**

---

# 5. D3 — MusicKernel 不等于每一首 MP3

不采用：

```text
MP3 A → MusicKernel A
MP3 B → MusicKernel B
MP3 C → MusicKernel C
```

原因是 `Kernel` 应代表语义 Authority，而不应退化成媒体实例对象。

本 ADR提议：

```text
MusicKernel         1 个
TransportKernel     1 个
TrackSession        0..N 个
```

## MusicKernel

拥有音乐播放器产品语义，例如：

```text
play
pause
seek intent
next
previous
repeat
shuffle
playlist policy
current-selection semantics
用户可见 playback-state meaning
```

它回答：

> **为什么播放器要发生这次变化？**

## TrackSession

表示一个具体媒体实例。

概念上包括：

```text
TrackSession
├── source identity
├── media descriptor
├── decoder session/handle
├── duration/probe truth
└── 对 playback runtime 的 window contribution
```

TrackSession 可以处于：

```text
active
prepared
retiring
```

但这些名称暂不冻结为 Rust enum。

冻结的只是：

> **一首具体媒体是 Session，不是新的 Kernel。**

---

# 6. D4 — 双 Window 模型

MVP 播放器采用：

```text
ActiveWindow
PreparedWindow
```

而不是永远只有一个 window。

正常播放：

```text
ActiveWindow
[A72][A73][A74][A75]

PreparedWindow
empty
```

同 Track seek：

```text
ActiveWindow
[A@72s ...]

PreparedWindow
[A@100s ...]
```

下一曲：

```text
ActiveWindow
Track A

PreparedWindow
Track B
```

因此 seek 与 next 可以共享：

```text
prepare
    ↓
physical cut
    ↓
promote
    ↓
retire old
```

但它们的 **领域语义不同**。

---

# 7. D5 — 切换分类

播放系统至少区分四类变化。

## 7.1 Continuous Update

例如：

```text
PlayerGain
EQ 参数
balance
DSP parameter
```

性质：

```text
same TrackSession
same generation
no timeline discontinuity
```

不得因为一个参数变化而重建整个 Fiber graph。

---

## 7.2 Intra-Track Discontinuity

例如：

```text
seek
loop jump
chapter jump
```

性质：

```text
same media source
new generation/window
```

它创建新的 PreparedWindow。

---

## 7.3 Track Replacement

例如：

```text
next
previous
open another song
```

性质：

```text
new TrackSession
new generation/window
```

MVP 默认采用 **hard semantic replacement**。

---

## 7.4 Topology Handoff

例如：

```text
AudioOutput replacement
Decoder provider replacement
DSP node insert/remove
```

这是 Composition / data-plane graph 问题。

它不天然意味着 media timeline 改变。

---

# 8. D6 — MVP 不采用 seek 淡出掩盖

此前讨论过：

```text
seek 请求后
让旧位置声音继续淡出几十毫秒
为新位置 decode 争取时间
```

本 ADR 在 MVP 中 **取消该机制**。

原因：

1. 增加 semantic authority 与 render permission 的额外状态；
2. 容易造成用户已经 seek，但旧媒体仍然继续发声；
3. 不是 correctness 必需条件；
4. 双 Window 已经允许后台提前准备新数据；
5. 后续如有真实听感需求，可以独立设计 Transition / Crossfade。

因此 MVP：

```text
prepare new window
        ↓
new window ready
        ↓
physical cut old
        ↓
promote new
```

不提供 old-tail masking。

---

# 9. D7 — Physical Fence / Flush 是硬正确性边界

软件状态：

```text
generation stale
window invalidated
```

不能证明：

```text
用户已经听不到旧声音
```

因为：

```text
decoded
!= queued
!= submitted
!= rendered
```

旧 PCM 可能已经进入：

```text
OS/device buffer
hardware queue
physical playback
```

因此：

> **Logical invalidation != Physical stop.**

所有要求立即切断旧音频的操作必须存在 Physical Fence。

至少包括：

```text
stop
seek commit
hard next/previous
fatal playback recovery
```

抽象：

```text
close old admission
        ↓
prevent new old-generation submission
        ↓
physical fence / flush handshake
        ↓
obtain definitive verdict
        ↓
promote / stop / recover
```

平台实现不同：

```text
WASAPI
CoreAudio
AAudio
...
```

可以使用不同 mechanism。

但跨平台必须证明相同语义：

> **在 fence 成功后，被截断的旧 generation 不得继续产生新的可听输出。**

Reference Playback v1 已经挣得的 physical-flush correctness 不得弱化。

---

# 10. D8 — PCM 收缩为 Canonical Audio Data Plane

不采用：

> Everything flows through PCM.

采用：

> **PCM 是 Qianqian 的 canonical decoded-audio data plane，不是通用 plugin message bus。**

Audio Data Plane：

```text
Encoded Media
      │
      ▼
Decoder
      │
      ▼
Canonical PCM
      │
      ▼
Processing*
      │
      ▼
AudioOutput
```

以下内容不属于 PCM data plane：

```text
metadata
artwork
commands
EOF evidence
errors
render evidence
device status
playlist
UI state
```

这些使用独立 typed contracts。

---

# 11. D9 — Control Plane / Evidence Plane / Data Plane 分离

播放器至少存在三类信息流。

## Control

```text
play
pause
seek
next
volume intent
topology changes
```

## Data

```text
encoded bytes
PCM frames
```

## Evidence

```text
decoder EOF
landing result
submitted accounting
rendered accounting
device loss
underrun
physical fence verdict
```

不得把三者塞进一个 generic EventBus 或 Context。

概念图：

```text
                CONTROL

MusicKernel ───────► TransportKernel
                         │
                         │ commands
                         ▼

------------------------------------------------

             CANONICAL AUDIO DATA PLANE

Decoder ──► Processing ──► AudioOutput

------------------------------------------------

                EVIDENCE

Decoder ───────────────┐
AudioOutput ───────────┼──► Transport/Music authority
Processing diagnostics ┘
```

---

# 12. D10 — MediaSpan 与 PCM Block 必须分离

不允许：

```text
BufferId == timeline identity
```

因为：

```text
SRC
DSP
decoder partial read
block split
block coalesce
convolution
device block sizing
```

都可能导致：

```text
1 input block → N output blocks
N input blocks → 1 output block
```

冻结：

> **Buffer 是存储/处理单位；MediaSpan 是媒体时间单位。**

概念：

```text
MediaSpan {
    generation
    media_start
    media_end
}
```

PCM：

```text
PcmBlock {
    provenance
    media_span
    format
    frames
}
```

一个 Processing 节点可以改变：

```text
frame count
storage layout
sample rate
block boundaries
```

但不得偷偷篡改 MediaSpan 的领域意义。

---

# 13. D11 — Generation 只处理软件时代边界

Generation 用于：

```text
seek
track replacement
stale decode rejection
late callback rejection
window invalidation
```

例如：

```text
Generation 17
    old window

seek

Generation 18
    prepared/new window
```

旧 decode 迟到：

```text
result.generation = 17
current = 18
```

则：

```text
reject stale result
```

但 Generation **不承担** Physical Fence 的职责。

因此冻结：

```text
generation invalidation
        !=
physical-output invalidation
```

两者不得合并为一个 bool 或一个 generation 比较。

---

# 14. D12 — PlayerGain 与 DeviceVolume 分离

“播放器音量”存在两个完全不同的概念。

## PlayerGain

Qianqian 播放器自身的音量控制。

属于 canonical PCM data plane：

```text
PCM
 ↓
Gain processing
 ↓
PCM
```

数学上：

```text
output_sample = input_sample × gain
```

主播放器 UI 的：

```text
🔊 50%
```

默认控制 PlayerGain。

原因：

```text
跨平台语义稳定
不修改整个系统音量
不影响其他应用
可参与 DSP pipeline
可做 ramp
可与 ReplayGain/loudness 等机制组合
```

参数变化应支持短时平滑 ramp，以避免突变产生 click/pop。

该 ramp 是：

```text
DSP 参数平滑
```

不是：

```text
seek transition fade
```

两者不可混淆。

---

## DeviceVolume

属于平台 AudioOutput 的可选设备能力。

例如：

```text
system endpoint volume
hardware volume
OS mixer volume
```

可未来暴露：

```text
DeviceVolume capability
```

但不作为 Qianqian 主播放器 volume 的默认语义。

冻结：

> **PlayerGain is audio processing; DeviceVolume is platform/device control.**

---

# 15. D13 — Data Plane Taxonomy 保留，但不是 Plugin Taxonomy

保留三类数据面角色：

## Producer

```text
Decoder
Synthesizer
future stream decoder
```

输出 PCM。

## Transformer

```text
Gain
EQ
SRC
Limiter
Mixer
other DSP
```

输入 PCM，输出 PCM。

## Consumer

```text
AudioOutput
Recorder
Analyzer tap
```

消费 PCM。

但是：

> Producer / Transformer / Consumer 是 **data-plane taxonomy**，不是 Fiber/plugin taxonomy。

不得推出：

```text
一个 feature = 一个 plugin
一个参数 = 一个 plugin
一个 node = 一个 DLL
```

例如：

```text
GainNode gain: 0.8 → 0.6
```

只是参数更新。

不得变成：

```text
unload Gain80Plugin
reconcile
load Gain60Plugin
```

---

# 16. D14 — Everything is Plugin 的边界

本 ADR 将 Everything is Plugin 收缩为：

> **所有具有独立长期生命周期、ownership、capability、replacement 或 teardown 边界的运行时能力，都应参与统一 Composition lifecycle。**

它不意味着：

```text
所有函数是 plugin
所有 PCM block 是 plugin
每个 EQ 参数是 plugin
每首 MP3 是 plugin
每个 feature 都是 Fiber
所有东西都可 hot swap
```

DSP graph 可以包含多个 node，但只有具有正当独立生命周期边界的对象才值得上升到 Composition component/plugin。

---

# 17. D15 — 全局共享状态：Global Visibility != Global Ownership

Qianqian 不建立：

```text
GlobalPlayerState
Arc<Mutex<AppState>>
MutableEverything
```

作为所有模块共同读写的总状态对象。

冻结：

> **Global visibility does not imply global ownership.**

以及：

> **Every mutable fact has exactly one semantic authority.**

---

# 18. D16 — Authority Islands

运行时由多个小型 Authority Island 组成。

## CompositionKernel

拥有：

```text
component lifecycle
capability reachability
binding topology
composition truth
```

## MusicKernel

拥有：

```text
music-domain policy
playback intent meaning
queue/repeat/shuffle semantics
user-visible product semantics
```

## TransportKernel

拥有：

```text
playback cursor
window authority
MediaSpan timeline
generation
promotion/invalidation
discontinuity execution
```

## AudioOutput

拥有：

```text
physical device state
submitted evidence
rendered evidence
physical fence verdict
hardware/device clock evidence
```

## Processing Nodes

拥有各自：

```text
DSP parameter truth
processing-local state
```

这些 Authority 之间不得共享可写内部结构。

---

# 19. D17 — One Fact, One Authority, One Writer

冻结：

> **One fact, one authority, one writer.**

例如：

```text
plugin topology
    → CompositionKernel

playback cursor
    → TransportKernel

playlist/repeat policy
    → MusicKernel

physical rendered position evidence
    → AudioOutput

PlayerGain runtime parameter
    → Gain processing authority
```

其他模块只能提交：

```text
Command
Intent
Evidence
```

不能直接写别人拥有的状态。

---

# 20. D18 — PlayerSnapshot 是全局只读投影，不是状态 Authority

播放器可以向 UI / tray / integrations 暴露一个统一：

```text
PlayerSnapshot
```

例如：

```text
PlayerSnapshot
├── playback
│   ├── state
│   ├── requested_position
│   ├── committed_position
│   ├── rendered_position
│   ├── duration
│   └── buffering
│
├── track
│   ├── identity
│   ├── title
│   ├── artist
│   └── artwork
│
├── audio
│   ├── player_gain
│   ├── output_device
│   └── format
│
└── diagnostics
    ├── underrun
    ├── landing_quality
    └── output status
```

但是：

```text
PlayerSnapshot
```

只是 Materialized View / Projection。

不得：

```text
snapshot.position = 100s
snapshot.state = Playing
```

来改变系统。

控制必须走：

```text
Command
   ↓
Authority
   ↓
Runtime change
   ↓
Evidence
   ↓
Snapshot projection
```

---

# 21. D19 — Desired State 与 Runtime Truth 分离

例如：

```text
desired_output = Bluetooth Headset
```

不等于：

```text
actual_output = Bluetooth Headset
```

因为设备可能掉线。

同理：

```text
desired_player_gain = 0.5
```

处理节点可能正在：

```text
0.62 → 0.58 → 0.54 → 0.50
```

因此必须允许：

```text
desired != actual
```

不得使用一个字段同时表达：

```text
用户想要什么
```

和：

```text
现实现在是什么
```

该原则与 Composition Kernel 的：

```text
desired composition
vs
running composition
```

保持一致。

---

# 22. D20 — RT 路径不得访问全局共享可变状态

Realtime path 必须只使用已经预绑定、已经发布的 RT-safe view。

禁止：

```text
Mutex<GlobalPlayerState>
Context lookup
Capability resolution
Fiber mutation
Reconcile
generic event dispatch
filesystem/network IO
UI/runtime round trip
unbounded allocation
```

Control thread 可以构建：

```text
new RT view
```

随后使用：

```text
atomic publication / bounded handoff
```

发布给 realtime thread。

RT thread 只能消费该已发布视图。

---

# 23. 整体架构

```text
                        USER / UI
                           │
                        Command
                           │
                           ▼
                    ┌─────────────┐
                    │ MusicKernel │
                    │             │
                    │ product     │
                    │ semantics   │
                    └──────┬──────┘
                           │ intent
                           ▼
                  ┌─────────────────┐
                  │ TransportKernel │
                  │                 │
                  │ timeline        │
                  │ cursor          │
                  │ ActiveWindow    │
                  │ PreparedWindow  │
                  │ MediaSpan       │
                  │ generation      │
                  └───────┬─────────┘
                          │
             ┌────────────┴────────────┐
             │                         │
       Active TrackSession      Prepared TrackSession
             │                         │
             └────────────┬────────────┘
                          │
                          ▼

              CANONICAL AUDIO DATA PLANE

       Decoder → Processing* → AudioOutput
                       │
                       ├ Gain
                       ├ EQ
                       ├ SRC
                       └ future DSP

                          ▲
                          │
                    pre-bound topology
                          │
                 ┌────────┴────────┐
                 │ Composition     │
                 │ Kernel          │
                 │                 │
                 │ Context         │
                 │ Capability      │
                 │ Fiber           │
                 │ Effect          │
                 │ Reconcile       │
                 └─────────────────┘

AudioOutput / Decoder / Processing
        │
        │ evidence
        ▼
Transport / Music Authority
        │
        ▼
PlayerSnapshot
        │
        ▼
UI / integrations
```

---

# 24. Seek 的冻结形状

MVP seek：

```text
用户 seek(T)
     │
     ▼
MusicKernel interprets intent
     │
     ▼
TransportKernel creates PreparedWindow
     │
     ▼
TrackSession / Decoder seek + prime
     │
     ▼
PreparedWindow becomes sufficiently ready
     │
     ▼
close old admission
     │
     ▼
Physical Fence / Flush
     │
     ├── failure → fail closed / recovery policy
     │
     ▼
promote PreparedWindow
     │
     ▼
old generation stale
     │
     ▼
resume from confirmed/estimated landing
```

本 ADR 不冻结：

```text
具体 buffer 数
具体预读毫秒数
WASAPI Stop/Reset 实现
线程模型
ring 实现
lock-free 数据结构
```

这些必须由后续实验赚取。

---

# 25. Next / Previous 的冻结形状

```text
prepare new TrackSession
        ↓
prime PreparedWindow
        ↓
close old admission
        ↓
Physical Fence
        ↓
promote new TrackSession/window
        ↓
retire old TrackSession
```

默认不 crossfade。

未来 crossfade 如有真实需求，需要独立 ADR，因为它会引入：

```text
simultaneously valid dual media contribution
mixing semantics
two render leases
presentation vs semantic authority
```

不得从双 Window 自动推导 crossfade 已被授权。

---

# 26. Stop 的冻结形状

`stop()` 不是：

```text
state = stopped
```

而必须具有物理意义：

```text
close admission
    ↓
prevent new submission
    ↓
Physical Fence / Flush
    ↓
prove old media cannot continue
    ↓
publish stopped state
```

用户可观察语义优先于软件内部 enum 更新速度。

---

# 27. ENDED 原则继续保留

不得：

```text
Decoder EOF
→ immediately ENDED
```

必须继续遵守历史行为 truth：

```text
producer terminal
AND
software media pipeline drained
AND
no relevant in-flight media
AND
no submitted-but-unrendered media
```

才可以形成用户可见 ENDED。

具体 predicate 后续 Playback semantic design 再冻结。

---

# 28. 明确拒绝的方案

本 ADR 拒绝：

### R1 — Composition Kernel 管 PCM / Track

违反 K0 control/data plane firewall。

### R2 — MusicKernel 与 TransportKernel 同时拥有 timeline

产生 dual authority。

### R3 — 每首 MP3 一个 MusicKernel

Kernel 退化为媒体实例。

使用 `TrackSession`。

### R4 — Mutable GlobalPlayerState

例如：

```text
Arc<Mutex<GlobalPlayerState>>
```

作为跨模块 authority。

### R5 — PCM 是通用消息总线

PCM 只用于 decoded audio data plane。

### R6 — BufferId 是媒体时间 identity

使用 `MediaSpan`。

### R7 — generation 代替 physical flush

软件 generation 无法撤回已提交到物理设备的声音。

### R8 — MVP seek fade / old-tail masking

暂不需要。

### R9 — 主播放器音量直接修改系统音量

主 volume 默认使用 `PlayerGain`。

### R10 — 每个参数变化都是 plugin replacement

参数变化属于 node-local update。

### R11 — UI Snapshot 成为可写状态库

Snapshot 只能读取。

---

# 29. 架构不变量

## I1 — Generic kernel firewall

```text
qianqian-kernel
```

不得依赖任何：

```text
Track
PCM
Music
Transport
PlayerSnapshot
```

概念。

---

## I2 — Single authority

任何 mutable semantic fact 只能有一个 Authority。

---

## I3 — Canonical PCM bypasses Composition Kernel

每个 PCM block/callback：

```text
Context lookup = 0
Capability resolve = 0
Reconcile = 0
Fiber mutation = 0
Effect registration = 0
```

---

## I4 — Active + Prepared

MVP temporal model 最多需要：

```text
1 ActiveWindow
1 PreparedWindow
```

更多并发 window 必须由新的需求/ADR 证明。

---

## I5 — Stale generation rejection

旧 generation 在 promotion 后不得重新进入当前软件播放路径。

---

## I6 — Physical fence correctness

成功切换/停止必须具有平台可证明的旧物理 tail 截断语义。

---

## I7 — Buffer != MediaSpan

所有时间真相基于 MediaSpan，不依赖 block allocation identity。

---

## I8 — submitted != rendered

任何 position / ENDED / seek correctness 都不得重新合并这两个事实。

---

## I9 — Snapshot is projection

所有 snapshot 字段必须能追溯到唯一 authority/evidence source。

Snapshot 自身不得拥有 mutation API。

---

## I10 — RT firewall

RT hot path 不允许：

```text
global mutable state locking
Context
resolution
Reconcile
generic event bus
filesystem/network
UI calls
unbounded allocation/blocking
```

---

# 30. 后续实现前必须建立的 Oracle

本 ADR 接受后，仍不得直接接 FFmpeg/WASAPI 大规模实现。

首先建立 deterministic playback model。

至少验证：

### O1 — Normal playback

```text
produce
ready
consume
submit
render
slide window
```

---

### O2 — Seek

```text
ActiveWindow A
PreparedWindow B
physical cut
promote B
old generation rejected
```

---

### O3 — Hard next-track

```text
TrackSession A
→ Prepared TrackSession B
→ physical cut
→ B becomes active
→ A retires
```

---

### O4 — Late decode

旧 generation decode result 在 promotion 后必须丢弃。

---

### O5 — submitted vs rendered

position / ENDED 不得因为 submit 就提前推进。

---

### O6 — Physical fence failure

flush/fence 失败后不得假装切换成功。

必须 fail closed 或进入显式 recovery。

---

### O7 — PlayerGain

Gain 参数变化：

```text
same generation
same track
no Composition reconcile
```

并验证平滑参数更新。

---

### O8 — Global snapshot provenance

每个 PlayerSnapshot 字段都必须注明：

```text
authority
source evidence
freshness
```

不存在双 writer。

---

# 31. 本 ADR 不冻结的内容

以下仍属于后续设计/实验：

```text
TransportKernel 的最终 Rust API
是否最终保留 TransportKernel 这个名字
TrackSession 的 crate/module 布局
ring buffer 实现
buffer pool
线程数量
lock-free 方案
decode worker 调度
具体 PCM quantum
lookahead/window 大小
具体 SRC placement
EQ graph
future crossfade
gapless
ReplayGain
device handoff policy
WASAPI/CoreAudio 具体 fence mechanism
```

架构语义先冻结，representation 后赚取。

---

# 32. 本 ADR 对现有 ARCH-003 的影响

若本 ADR 最终接受，则应修订现有 Playback Kernel authority。

当前较宽泛的：

```text
MusicKernel owns:
playback state
media timeline
active track session
...
```

应拆成：

```text
MusicKernel
    → music/product semantic authority

TransportKernel
    → playback timeline authority

TrackSession
    → concrete media-session state/mechanism

Music component
    → owns the cohesive playback runtime mechanisms
```

这不是重开 Base Kernel。

这是在 Playback Kernel 尚未实现之前，对 ARCH-003 的 authority decomposition 进行一次 corrective refinement。

---

# 33. 最终架构原则

冻结候选：

> **Composition Kernel 管运行时空间结构。**

> **Transport Kernel 管播放器时间结构。**

> **Music Kernel 管音乐产品语义。**

> **TrackSession 表示具体媒体实例，而不是新的 Kernel。**

> **PCM 是 canonical decoded-audio data plane，而不是全局消息总线。**

> **Buffer 是存储单位；MediaSpan 是时间单位。**

> **Generation 管软件时代边界；Physical Fence 管真实声音边界。**

> **PlayerGain 属于 PCM Processing；DeviceVolume 属于平台 AudioOutput。**

> **Global visibility does not imply global ownership：全局可以共享只读 Snapshot，但每个可变事实只能有一个 Authority。**

---

# 34. 待共同 Review 的三个问题

在 ADR 从 `PROPOSED` 变为 `ACCEPTED` 前，只保留以下三个显式问题：

### Q1 — `TransportKernel` 是否是最终名称？

候选：

```text
TransportKernel
PlaybackTimelineKernel
TrackKernel
```

推荐 `TransportKernel`，因为 `TrackKernel` 容易与单曲 `TrackSession` 混淆。

### Q2 — MusicKernel 是否保留 “Kernel” 名称？

当前语义是：

```text
music product/domain policy authority
```

如果未来发现它只是 policy facade，也可以评估：

```text
MusicPolicy
MusicDomainKernel
```

本 ADR 先冻结职责，不冻结名字。

### Q3 — TrackSession 是否直接拥有 Decoder handle？

当前倾向：

```text
TrackSession
    owns one opened Decoder session/handle
```

但需要后续 lifecycle audit 确认它与 provider withdrawal、teardown-access window 是否完全一致。

除此三项外，本文其余原则均可进入冻结 Review。

