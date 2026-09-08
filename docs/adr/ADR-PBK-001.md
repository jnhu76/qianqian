# ADR-PBK-001：播放时间轴、媒体会话、PCM 数据面与共享状态边界

- **状态**：PROPOSED / Corrective-1 / 待共同 Review
- **日期**：2026-09-08
- **作用域**：Qianqian Playback Architecture / ARCH-003
- **不重开**：Base / Composition Kernel K0
- **关联权威**：
  - `docs/architecture/component-boundary-a0.md`
  - `docs/architecture/composition-kernel.md`
  - `docs/architecture/composition-kernel-0-design.md`
  - `docs/architecture/composition-kernel-0-implementation-adr.md`
  - Issue #46 / #53 / #67 / #70
  - PR #66 / #68 / #69 / #71
- **历史行为 Oracle**：
  - `research/playback-reference-v1`
  - Issue #49
  - 已验证的 seek / physical flush / stale generation rejection / render accounting / ENDED 行为

---

# 0. Corrective-1 摘要

本修订不改变 ADR 的总体方向，但修正首轮对抗审计发现的 3 个 P0 与 4 个 P1 内部矛盾。

必须修正的三个模型问题：

```text
P0-1  Plugin / Component 与内部 Runtime Resource 边界不清
P0-2  Dual Window 与“一 TrackSession 一个 Decoder cursor”不能同时成立
P0-3  Dual Window 与“global current generation”判 stale 不能同时成立
```

同步收紧：

```text
P1-1  Composition Topology 与 Audio Processing Graph Topology 分离
P1-2  UI Command 按 authority/capability 路由，不经过单一 God Router
P1-3  raw playback evidence 先进入 Transport authority，再向 Music 派生语义事实
P1-4  Music component 只能拥有 subordinate playback runtime，不能吞并 Decoder/AudioOutput provider
```

Corrective-1 新增核心概念：

```text
Composition Component / Plugin
Nested Runtime Resource
TrackSession
DecodeSession
ActiveWindow
PreparedWindow
Generation Admission
Audio Processing Graph
```

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
desired composition -> running composition
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

Playback 阶段需要回答另一组问题：

```text
播放器的时间到底由谁拥有？
一首媒体与一个 decoder cursor 是否具有相同生命周期？
seek 时 Active 与 Prepared 如何同时存在？
什么东西真的应该成为 plugin？
PCM 应该成为多大范围的公共数据面？
Processing graph 与 Composition graph 谁管理？
软件 generation 与物理设备中已经提交的声音如何区分？
UI command 应该进入哪个 authority？
全局共享状态如何避免退化成 GlobalPlayerState？
volume 应由播放器处理 PCM，还是修改系统设备音量？
```

本 ADR 只冻结这些边界与不变量，不冻结最终 Rust representation。

---

# 2. 架构总原则

Qianqian Playback Architecture 采用以下六条总原则。

## C1 — Composition 是空间组合

> **Composition Kernel 管运行时组件的空间组合、capability reachability、ownership 与 lifetime。**

它不拥有播放时间，也不搬运媒体 payload。

## C2 — Transport 是时间 Authority

> **TransportKernel 是播放器唯一的 playback timeline authority。**

它决定当前哪一个媒体时间窗口可以继续推进、哪个窗口正在准备、何时可以完成 discontinuity。

## C3 — Music 是产品语义 Authority

> **MusicKernel 管音乐播放器的产品/领域语义，不拥有 decoder cursor、PCM ring、device handle 等机制资源。**

## C4 — 独立组合对象进入 Plugin；内部资源必须有唯一 Plugin Owner

> **Every independently composable runtime capability enters through the plugin/component model. Every non-plugin runtime resource has exactly one plugin/component owner.**

中文冻结为：

> **所有具有独立组合身份、可被外部绑定、可独立替换或具有独立 provider-withdrawal 边界的 capability provider，必须进入 Composition lifecycle。仅具有内部生命周期或局部 ownership 的 runtime resource，不因此自动成为 plugin；它必须明确嵌套归属于一个 composed component。**

## C5 — PCM 是 canonical audio data plane，不是全局消息总线

> **连续的 decoded-audio payload 收敛到 canonical PCM；control、evidence、metadata 等仍使用独立 typed contracts。**

## C6 — 全局可见不等于全局拥有

> **Global visibility does not imply global ownership. Every mutable fact has exactly one semantic authority.**

---

# 3. D1 — Composition Kernel K0 不重开

本 ADR 不得修改 Generic Composition Kernel 的既有职责。

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
DecodeSession
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

# 4. D2 — Plugin / Nested Runtime Resource / Data Item 三层模型

本 ADR 不再使用“只要有 lifecycle / ownership 就应该成为 plugin”的过宽判据。

运行时对象分三层：

```text
Composition Component / Plugin
    ├── independently bindable capability provider
    ├── independently replaceable provider
    ├── provider withdrawal boundary
    └── composition-level lifecycle

Nested Runtime Resource
    ├── TransportKernel
    ├── TrackSession
    ├── DecodeSession
    ├── ActiveWindow / PreparedWindow
    ├── buffer/ring bookkeeping
    └── component-local processing node/graph when not independently composed

Data Item
    ├── PcmBlock
    ├── MediaSpan / provenance record
    └── typed evidence record
```

判断是否进入 Composition lifecycle 的核心不是：

```text
“它有没有析构函数？”
“它有没有状态？”
“它能不能被替换？”
```

而是：

```text
它是否具有独立组合身份？
外部 component 是否通过 capability 绑定它？
它是否可以独立成为 provider？
它的 withdrawal 是否要求 Composition Kernel 主导 dependent-before-provider-release？
```

因此：

```text
TrackSession != plugin
DecodeSession != plugin
ActiveWindow != plugin
PcmBlock != plugin
普通 Gain/EQ node != 自动成为 plugin
```

这些对象必须具有明确 owner，但不因此自动获得 Fiber 身份。

冻结：

> **Everything is Plugin 不等于 everything is individually a plugin。独立可组合能力必须从 plugin boundary 进入；其他 runtime resource 必须有且只有一个 plugin/component owner。**

---

# 5. D3 — Music component 是生命周期根，但不得成为巨石组件

MVP 仍保留 `Music` 作为 composed component / Fiber。

Music component 可以拥有 subordinate playback runtime：

```text
MusicComponent
├── MusicKernel
├── TransportKernel
├── TrackSession(s)
├── DecodeSession(s)
├── ActiveWindow / PreparedWindow
├── window/frontier bookkeeping
└── component-local playback coordination resources
```

Music component 只 **绑定** 外部 provider capability：

```text
Decoder
PcmSink / AudioOutput
future Processing
future Metadata or other justified capability
```

明确禁止：

```text
MusicComponent internally new FFmpegDecoder()
MusicComponent internally new WasapiOutput()
MusicComponent owns provider-global FFmpeg closure
MusicComponent owns platform device implementation
```

Decoder provider、AudioOutput provider 仍然具有各自独立 Composition identity 与 lifecycle。

冻结：

> **Music component owns subordinate playback runtime resources; it binds, but does not absorb, independently composed providers.**

---

# 6. D4 — MusicKernel 是音乐产品语义 Authority

不采用：

```text
MP3 A -> MusicKernel A
MP3 B -> MusicKernel B
```

`Kernel` 表示 Authority，不表示媒体实例。

MusicKernel 单例于一个 Music component episode，负责：

```text
play / pause intent meaning
seek intent meaning
next / previous
repeat / shuffle
playlist policy
selection semantics
用户可见 playback-state meaning
Transport terminal 之后应该 next/repeat/stop 的产品决策
```

它回答：

> **为什么播放器要发生这次变化？**

MusicKernel 不直接拥有：

```text
decoder cursor
PCM block
ring
WASAPI/CoreAudio handle
physical rendered counter
```

---

# 7. D5 — TransportKernel 是唯一 Playback Timeline Authority

`TrackKernel` 这个名称容易与单曲 TrackSession 混淆，因此本 ADR 暂用：

```text
TransportKernel
```

TransportKernel 是 Music component 内的 Nested Runtime Resource，但它是 playback timeline 的唯一 semantic authority。

它拥有：

```text
playback cursor semantics
MediaSpan timeline authority
ActiveWindow
PreparedWindow
window roles
frontiers
GenerationId / admission
window promotion
window invalidation
discontinuity execution
physical-cut coordination state
raw playback evidence interpretation
```

它回答：

> **播放器现在到底播哪一段？哪一段正在准备？什么时候可以切换？哪些 generation 的结果仍然被允许进入哪一个 window？**

MusicKernel 不再与 TransportKernel 共同解释 raw cursor/render/EOF 事实。

---

# 8. D6 — TrackSession 与 DecodeSession 分离

这是 Dual Window 成立的必要条件。

## TrackSession

TrackSession 表示一个具体媒体的 identity / source lifetime root。

概念上：

```text
TrackSession
├── source identity
├── media descriptor
├── duration / probe truth
├── source-level metadata identity
└── 0..N DecodeSession
```

TrackSession **不拥有唯一 decoder cursor**。

它也不具有：

```text
active
prepared
retiring
```

这种互斥状态，因为同一个 TrackSession 在同 Track seek 时可以同时贡献 ActiveWindow 与 PreparedWindow。

冻结：

> **Active / Prepared / Retiring 是 window/generation contribution role，不是 TrackSession 的互斥 lifecycle state。**

## DecodeSession

DecodeSession 表示一个可以独立推进的 decode cursor/lifetime。

概念上：

```text
DecodeSession
├── decoder handle/cursor
├── GenerationId
├── decode position
├── EOF/seek-local state
└── target window contribution
```

同 Track seek：

```text
TrackSession A
├── DecodeSession gen17 @72s   -> ActiveWindow
└── DecodeSession gen18 @100s  -> PreparedWindow
```

next：

```text
TrackSession A
└── DecodeSession gen17        -> ActiveWindow

TrackSession B
└── DecodeSession gen18        -> PreparedWindow
```

因此，一个 provider 可以仍然是一个 `Decoder` capability provider，但可以创建多个独立 decoder handle/session；“一个 provider”不等于“整个播放器只能有一个 decoder cursor”。

DecodeSession 是 Nested Runtime Resource，不是 plugin。

---

# 9. D7 — 双 Window 模型

MVP temporal model 允许：

```text
1 ActiveWindow
0..1 PreparedWindow
```

正常播放：

```text
ActiveWindow
[A@72s ...]

PreparedWindow
empty
```

同 Track seek：

```text
TrackSession A

ActiveWindow
  generation 17
  DecodeSession A17 @72s

PreparedWindow
  generation 18
  DecodeSession A18 @100s
```

下一曲：

```text
ActiveWindow
  TrackSession A / gen17

PreparedWindow
  TrackSession B / gen18
```

seek 与 next 可以共享 execution shape：

```text
prepare
    ↓
prime
    ↓
close old admission
    ↓
physical fence
    ↓
promote
    ↓
retire old
```

但它们的 Music-domain 语义不同。

本 ADR 不授权第三个 simultaneously prepared playback window。需要超过双 Window 时必须由新的需求和 ADR 证明。

---

# 10. D8 — Generation 使用 admission / role 模型，不使用 global current-generation 判 stale

Dual Window 存在期间必然可能出现：

```text
active_generation   = gen17
prepared_generation = gen18
```

因此禁止把 stale rejection 写成语义：

```text
result.generation != current_generation
    => stale
```

因为 prepare 阶段没有一个 global `current_generation` 可以同时让 gen17 与 gen18 合法。

冻结 Generation 的本质：

> **Generation 是一个 window-scoped temporal identity；结果是否有效取决于该 generation 是否仍被某个 live playback role/admission 接受，而不是是否等于一个全局 current 值。**

概念角色：

```text
Preparing
Active
Retiring
Invalid
```

这些名称暂不冻结为 Rust enum；冻结的是 admission 语义。

prepare 阶段：

```text
admitted_generations = {
    active_generation,
    prepared_generation
}
```

但权限不同：

```text
Active
    可继续满足当前 render path

Preparing
    可接受目标 window 的 decode/prime result
    不得冒充当前 physical output authority
```

进入切换：

```text
old Active
    -> close new producer/submission admission
    -> Retiring / awaiting physical fence

Prepared
    -> remains prepared
```

fence 成功 + promotion：

```text
old active -> Invalid/retired
prepared   -> Active
prepared slot cleared
```

迟到结果只有在它仍满足对应 generation + window + admission contract 时才可接受。

冻结：

> **Stale means “no longer admitted by the owning temporal role”, not merely “not equal to current generation”.**

---

# 11. D9 — 切换分类

播放系统至少区分四类变化。

## 11.1 Continuous Update

例如：

```text
PlayerGain
EQ 参数
balance
DSP parameter
```

性质：

```text
same media timeline
no generation discontinuity
no TrackSession replacement
```

不得因为一个参数变化而重建整个 Fiber graph。

## 11.2 Intra-Track Discontinuity

例如：

```text
seek
loop jump
chapter jump
```

性质：

```text
same TrackSession/source identity
new DecodeSession / generation / PreparedWindow
```

## 11.3 Track Replacement

例如：

```text
next
previous
open another song
```

性质：

```text
new TrackSession
new DecodeSession
new generation/window
```

MVP 默认采用 hard semantic replacement。

## 11.4 Topology Handoff

例如：

```text
Decoder provider replacement
AudioOutput provider replacement
future Processing provider replacement
```

这是 Composition topology 的变化。

它不天然意味着 media timeline 改变；continuity/recovery policy 仍属于 Music/Transport domain。

注意：**DSP graph node insert/remove 不自动等于 Composition topology change**，见 D15。

---

# 12. D10 — MVP 不采用 seek 淡出/old-tail masking

此前讨论过：

```text
seek 请求后
让旧位置声音继续淡出几十毫秒
为新位置 decode 争取时间
```

MVP 明确不采用。

原因：

1. correctness 不需要；
2. 容易混淆 semantic authority 与 temporary render permission；
3. 双 Window 已允许 PreparedWindow 在切换前独立 prime；
4. 如未来需要 crossfade/fade，应独立设计 dual-contribution presentation semantics。

MVP：

```text
prepare new window
        ↓
prime until ready enough
        ↓
physical cut old
        ↓
promote new
```

不授权 old-tail masking。

---

# 13. D11 — Physical Fence / Flush 是硬正确性边界

软件状态：

```text
generation invalidated
window retired
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

因此冻结：

> **Logical invalidation != Physical stop.**

所有要求立即切断旧音频的操作都必须存在 Physical Fence。

至少包括：

```text
stop
seek commit
hard next/previous
fatal playback recovery when old tail must be killed
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
promote / stop / fail closed
```

平台可使用不同 mechanism：

```text
WASAPI
CoreAudio
AAudio
...
```

但必须证明相同跨平台语义：

> **fence 成功后，被截断 generation 不得继续产生新的可听输出。**

Reference Playback v1 已挣得的 physical-flush correctness 不得弱化。

---

# 14. D12 — PCM 收缩为 Canonical Audio Data Plane

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
Audio Processing Graph
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
landing verdict
errors
render evidence
device status
playlist
UI state
```

它们使用独立 typed contracts。

---

# 15. D13 — Control / Data / Evidence 三类信息流分离

播放器至少存在三类信息流。

## Control

```text
playback intent
audio-processing parameter intent
device/output intent
composition intent
```

## Data

```text
encoded bytes
PCM frames
```

## Evidence

```text
decoder EOF
seek landing result
submitted accounting
rendered accounting
device loss
underrun
physical fence verdict
```

不得把三者塞入一个 generic EventBus 或 Context。

---

# 16. D14 — Command 按 Authority / Capability 路由，不建立 MusicKernel God Router

UI / integrations 发出的 command 不统一经过 MusicKernel。

冻结：

> **Command goes to the authority that owns the mutated fact.**

概念：

```text
                         UI / Integrations
                              │
             ┌────────────────┼──────────────────┐
             │                │                  │
      PlaybackIntent     AudioControl      CompositionIntent
             │                │                  │
             ▼                ▼                  ▼
        MusicKernel     Processing/Auth.   CompositionKernel
             │                                   │
             │ playback semantic intent          │ topology/lifecycle
             ▼                                   │
       TransportKernel                           │
```

示例：

```text
play / pause / seek / next
    -> Playback/Music authority
    -> Transport execution when timeline changes

PlayerGain / EQ parameter
    -> audio-processing authority
    -> no Transport hop unless the operation changes timeline semantics

DeviceVolume
    -> DeviceVolume capability / AudioOutput

provider/output replacement
    -> Composition / provider-control path
    -> Music/Transport only receive continuity/invalidation consequences
```

因此 MusicKernel 不是所有 UI command 的中央 dispatcher。

---

# 17. D15 — Composition Topology 与 Audio Processing Graph Topology 分离

这是两种不同 topology。

## Composition Topology

由 Composition Kernel 管：

```text
Music component
Decoder provider
AudioOutput provider
future independently-composed Processing provider
Recorder/Analyzer provider when justified
...
```

关注：

```text
capability
binding
lifetime
provider replacement
withdrawal
reconcile
```

## Audio Processing Graph

由 processing/data-plane authority 管：

```text
Gain
EQ
SRC
Limiter
Mixer
future DSP nodes
```

关注：

```text
ordered PCM transforms
format contract
RT publication
parameter update
graph swap
```

一般的：

```text
insert EQ node
change limiter
replace Gain graph node
```

**不自动触发 Composition Reconcile**。

推荐的 RT shape：

```text
control thread builds new processing graph
        ↓
validate / prepare
        ↓
atomic or otherwise bounded RT-safe publication
        ↓
RT path consumes pre-published graph
```

只有当“Processing”本身具有独立 composed provider identity 时，provider 的 bind/withdraw/replace 才属于 Composition topology。

冻结：

> **Composition topology composes providers; Audio Processing Graph orders PCM-processing nodes. Do not collapse the two graphs.**

---

# 18. D16 — MediaSpan 与 PCM Block 必须分离

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
1 input block -> N output blocks
N input blocks -> 1 output block
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

Processing 节点可以改变：

```text
frame count
storage layout
sample rate
block boundaries
```

但不得偷偷改变 MediaSpan 的领域意义。

---

# 19. D17 — Evidence 单向进入对应 Authority，避免双重解释 raw fact

`One fact, one authority, one writer` 不禁止多个模块观察一个事实，但禁止多个 semantic authority 独立解释同一 raw evidence 并各自派生状态。

播放时间相关 raw evidence 先进入 TransportKernel：

```text
Decoder EOF
seek landing result
late decode result
submitted evidence
rendered evidence
physical fence verdict
        │
        ▼
TransportKernel
```

TransportKernel 根据 timeline/window/generation 状态形成派生语义事实：

```text
LandingCommitted(track, span, quality)
TransportDrained(track)
TransportDiscontinuityFailed(...)
TransportOutputUnavailable(...)
```

再送给 MusicKernel：

```text
TransportKernel
        │ derived domain evidence
        ▼
MusicKernel
        │
        ├── remain Ended
        ├── repeat
        ├── next
        └── user-visible product state
```

例如：

```text
Decoder EOF
        ↓
TransportKernel

RenderedThrough(last_media_span)
        ↓
TransportKernel

producer terminal + pipeline drained + physical media drained
        ↓
TransportDrained(track A)
        ↓
MusicKernel
        ↓
ENDED / repeat / next product semantics
```

非 timeline evidence 直接进入自己的 authority；例如 processing-local diagnostics 不必绕 Transport。

冻结：

> **Raw evidence is interpreted once by the authority that owns the affected fact; other authorities receive derived typed facts, not a second chance to redefine the same truth.**

---

# 20. D18 — Generation 只处理软件时代边界；Physical Fence 处理真实声音边界

Generation 用于：

```text
seek / track replacement temporal identity
stale decode rejection
late callback/result rejection
window invalidation
```

但 Generation 不承担 Physical Fence 的职责。

冻结：

```text
generation/window invalidation
        !=
physical-output invalidation
```

不得合并成：

```text
if gen stale { assume sound stopped }
```

软件时代边界与外部不可逆物理边界必须分别证明。

---

# 21. D19 — PlayerGain 与 DeviceVolume 分离

“音量”有两个不同语义。

## PlayerGain

Qianqian 播放器自身音量属于 PCM processing responsibility：

```text
PCM
 ↓
Gain
 ↓
PCM
```

数学上：

```text
output_sample = input_sample * gain
```

主播放器 UI 的普通：

```text
volume = 50%
```

默认表示 PlayerGain，而不是系统 master/device volume。

PlayerGain 参数变化应支持短时 ramp，以避免 sample-level discontinuity 产生 click/pop。

该 ramp 是：

```text
DSP parameter smoothing
```

不是：

```text
seek transition fade
```

本 ADR 只冻结 PlayerGain 属于 PCM-processing 语义；其 MVP 最终 owner 是 Music component 内部 processing resource，还是未来独立 `Processing` provider 的内部 node，由后续 processing-boundary implementation gate 决定。无论 owner 如何，PlayerGain 都不是 Transport timeline mutation。

## DeviceVolume

DeviceVolume 属于平台 AudioOutput 的可选 device-control capability：

```text
system endpoint volume
hardware volume
OS mixer volume
```

冻结：

> **PlayerGain is audio processing; DeviceVolume is platform/device control.**

---

# 22. D20 — Data Plane Taxonomy 保留，但不是 Plugin Taxonomy

保留三类数据面角色：

## Producer

```text
Decoder output
Synthesizer
future stream decoder
```

## Transformer

```text
Gain
EQ
SRC
Limiter
Mixer
other DSP
```

## Consumer

```text
AudioOutput
Recorder
Analyzer tap
```

但：

> **Producer / Transformer / Consumer 是 data-plane taxonomy，不是 Fiber/plugin taxonomy。**

不得推出：

```text
一个 feature = 一个 plugin
一个 node = 一个 Fiber
一个 parameter = 一个 plugin
一个 plugin = 一个 DLL
```

---

# 23. D21 — Everything is Plugin 的最终边界

本 ADR 将 `Everything is Plugin` 冻结为：

> **所有 independently composable runtime capability 都必须通过 Composition component/plugin model 进入。所有非 plugin runtime resource 都必须有且只有一个 composed component owner。**

因此：

```text
Decoder provider             -> plugin/component
AudioOutput provider         -> plugin/component
Music                        -> plugin/component
future independent Processing provider -> plugin/component when justified

TrackSession                 -> nested runtime resource
DecodeSession                -> nested runtime resource
Window                       -> nested runtime resource
Generation                   -> temporal identity/data
Gain/EQ node                 -> audio graph node unless separately justified
PcmBlock                     -> data item
Evidence record              -> data item
```

“Everything is Plugin”是一条 **独立可组合能力统一进入 Composition lifecycle** 的规则，不是对象数量最大化规则。

---

# 24. D22 — 全局共享状态：禁止 Mutable Global Bag

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

> **One fact, one authority, one writer.**

---

# 25. D23 — Authority Islands

运行时由若干小型 Authority Island 组成。

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
music product/domain policy
playback intent meaning
queue/repeat/shuffle semantics
user-visible product semantics
```

## TransportKernel

拥有：

```text
playback cursor
MediaSpan timeline
window roles
Generation admission
promotion/invalidation
discontinuity execution
playback raw-evidence interpretation
```

## AudioOutput

拥有：

```text
physical device/session mechanism
submitted/rendered evidence production
physical fence mechanism/verdict source
hardware/device clock evidence
```

## Processing Authority

拥有：

```text
processing graph truth
DSP parameters
processing-local runtime state
```

这些 Authority 之间不得共享可写内部结构。

---

# 26. D24 — PlayerSnapshot 是全局只读投影，不是 Authority

播放器可以向 UI / tray / integrations 暴露统一：

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
├── track
│   ├── identity
│   ├── title
│   ├── artist
│   └── artwork
├── audio
│   ├── player_gain
│   ├── output_device
│   └── format
└── diagnostics
    ├── underrun
    ├── landing_quality
    └── output status
```

但 PlayerSnapshot 只是 Materialized View / Projection。

禁止：

```text
snapshot.position = 100s
snapshot.state = Playing
```

来改变系统。

控制闭环：

```text
Command
   ↓
owning Authority
   ↓
Runtime mutation
   ↓
Evidence / authoritative truth
   ↓
Snapshot projection
```

Snapshot 字段必须可追溯到唯一 authority/provenance。

---

# 27. D25 — Desired State 与 Runtime Truth 分离

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

处理节点可能正在 ramp：

```text
0.62 -> 0.58 -> 0.54 -> 0.50
```

因此必须允许：

```text
desired != actual
```

不得用一个字段同时表达：

```text
用户希望什么
```

与：

```text
现实现在是什么
```

该原则与 Composition Kernel 的 desired composition / running composition 分离保持一致。

---

# 28. D26 — RT 路径不得访问全局共享可变状态

Realtime path 必须只消费已经预绑定、已发布、RT-safe 的 data-plane view/graph。

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
unbounded allocation/blocking
```

Control thread 可以：

```text
build new RT view / processing graph
        ↓
validate / prepare
        ↓
atomic or bounded RT-safe publication
```

RT thread 只消费 published state。

---

# 29. 总体架构

```text
                         UI / Integrations
                              │
             ┌────────────────┼──────────────────┐
             │                │                  │
      PlaybackIntent     AudioControl      CompositionIntent
             │                │                  │
             ▼                ▼                  ▼
        MusicKernel      Processing/Auth.  CompositionKernel
             │                                   │
             │ semantic intent                   │ provider topology
             ▼                                   │
       TransportKernel                           │
             │                                   │
             │ owns playback timeline            │
             ▼                                   │
   ┌───────────────────────┐                     │
   │ Music nested runtime  │                     │
   │                       │                     │
   │ TrackSession A        │                     │
   │  ├ DecodeSession A17  │                     │
   │  └ DecodeSession A18  │                     │
   │                       │                     │
   │ ActiveWindow gen17    │                     │
   │ PreparedWindow gen18  │                     │
   └───────────┬───────────┘                     │
               │                                 │
               └─────────────┬───────────────────┘
                             │ pre-bound capability/data edges
                             ▼

                  CANONICAL AUDIO DATA PLANE

       Decoder -> PCM -> Audio Processing Graph -> AudioOutput
                         Gain / EQ / SRC / ...

                             │
                             │ raw playback evidence
                             ▼
                       TransportKernel
                             │
                             │ derived domain evidence
                             ▼
                         MusicKernel
                             │
                             ▼
                       PlayerSnapshot
                             │
                             ▼
                       UI / integrations
```

这张图表达四个不同事实：

```text
Composition Kernel  = provider/component 的空间/lifecycle topology
TransportKernel    = playback temporal authority
MusicKernel        = product/domain semantic authority
PCM graph          = realtime audio data flow
```

---

# 30. Seek 冻结形状

MVP seek：

```text
用户 seek(T)
     │
     ▼
MusicKernel interprets playback intent
     │
     ▼
TransportKernel allocates new generation/window role
     │
     ▼
TrackSession retains media identity
     │
     ├── old DecodeSession -> ActiveWindow
     └── new DecodeSession -> PreparedWindow
     │
     ▼
new DecodeSession seek + prime
     │
     ▼
PreparedWindow sufficiently ready
     │
     ▼
close old admission / submission
     │
     ▼
Physical Fence / Flush
     │
     ├── failure -> no fake success; fail closed / explicit recovery
     │
     ▼
promote PreparedWindow
     │
     ▼
old generation no longer admitted
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

---

# 31. Next / Previous 冻结形状

```text
prepare new TrackSession
        ↓
create DecodeSession / generation / PreparedWindow
        ↓
prime
        ↓
close old admission
        ↓
Physical Fence
        ↓
promote new TrackSession/window contribution
        ↓
retire old DecodeSession/window
        ↓
release old TrackSession when no live resource still owns it
```

默认不 crossfade。

未来 crossfade 如有真实需求，需要独立 ADR，因为它会引入：

```text
simultaneously renderable dual contributions
mixing semantics
presentation authority
additional render-admission rules
```

双 Window 本身不自动授权 crossfade。

---

# 32. Stop 冻结形状

`stop()` 不是：

```text
state = stopped
```

而必须具有物理意义：

```text
close old admission
    ↓
prevent new submission
    ↓
Physical Fence / Flush
    ↓
prove old media cannot continue audibly
    ↓
publish stopped semantic state
```

用户可观察的物理停止语义优先于内部 enum 提前更新。

---

# 33. ENDED 原则继续保留

不得：

```text
Decoder EOF
-> immediately ENDED
```

必须继续满足历史行为 truth 的等价形状：

```text
producer terminal
AND
software media pipeline drained
AND
no relevant in-flight media
AND
no submitted-but-unrendered media
```

TransportKernel 负责从 raw EOF/render/submission evidence 得出 transport drained truth；MusicKernel 再解释其产品语义。

具体 predicate 留给 Playback semantic implementation gate，但不得弱化 Reference Playback v1。

---

# 34. 明确拒绝的方案

## R1 — Composition Kernel 管 PCM / Track

违反 K0 control/data-plane firewall。

## R2 — MusicKernel 与 TransportKernel 同时拥有 timeline

产生 dual authority。

## R3 — 每首 MP3 一个 MusicKernel

媒体实例使用 TrackSession。

## R4 — TrackSession 持有唯一 decoder cursor

无法支持同 Track Dual Window seek；decoder cursor 属于 DecodeSession。

## R5 — `generation != current_generation` 作为 stale 的通用定义

Dual Window prepare 阶段没有单一 global current 可以表达合法性；使用 generation admission / window role。

## R6 — Mutable GlobalPlayerState

禁止万能共享写状态。

## R7 — PCM 是通用消息总线

PCM 只用于 canonical decoded-audio data plane。

## R8 — BufferId 是媒体时间 identity

使用 MediaSpan。

## R9 — generation 代替 physical flush

软件 invalidation 无法撤回已经提交到物理设备的声音。

## R10 — MVP seek fade / old-tail masking

暂不授权。

## R11 — 主播放器音量直接修改系统音量

主 volume 默认语义为 PlayerGain；DeviceVolume 单独建模。

## R12 — 每个 DSP node/参数都是 plugin/Fiber

Audio graph node 与 Composition component 是不同层次。

## R13 — DSP graph mutation 必然触发 Composition Reconcile

provider topology 与 processing topology 分离。

## R14 — 所有 UI command 都经过 MusicKernel

command 必须路由到 owning authority。

## R15 — Decoder/AudioOutput mechanism 被 MusicComponent 吞入内部

Music 绑定 provider，不吸收 provider identity/lifecycle。

## R16 — Raw playback evidence 同时由 MusicKernel 与 TransportKernel 独立解释

raw timeline evidence 先由 Transport authority 解释，再生成 typed derived domain evidence。

## R17 — UI Snapshot 成为可写状态库

Snapshot 只能是只读 projection。

---

# 35. 架构不变量

## I1 — Generic kernel firewall

`qianqian-kernel` 不得依赖：

```text
TrackSession
DecodeSession
PCM
Music
Transport
PlayerSnapshot
```

## I2 — Plugin boundary requires independent composability

内部 ownership/lifecycle 本身不足以把一个 resource 提升成 plugin。

所有 Nested Runtime Resource 必须有唯一 composed owner。

## I3 — Single authority

任何 mutable semantic fact 只能有一个 authority/writer。

## I4 — Dual Window bound

MVP 最多存在：

```text
1 ActiveWindow
0..1 PreparedWindow
```

## I5 — Decoder cursor is window/session scoped

同 Track Active + Prepared 可拥有不同 DecodeSession；不得假设一 TrackSession 只有一个可推进 decoder cursor。

## I6 — Generation admission

结果合法性由 generation/window admission 决定，不由全局 current-generation equality 决定。

## I7 — Canonical PCM bypasses Composition Kernel

每个 PCM block/callback：

```text
Context lookup = 0
Capability resolve = 0
Reconcile = 0
Fiber mutation = 0
Effect registration = 0
```

## I8 — Physical fence correctness

成功 stop/seek/hard replacement 必须具有平台可证明的旧物理 tail 截断语义。

## I9 — Buffer != MediaSpan

时间真相基于 MediaSpan，不依赖 block allocation identity。

## I10 — submitted != rendered

position / ENDED / seek correctness 不得重新合并这两个事实。

## I11 — Composition topology != Processing topology

普通 processing graph node mutation 不自动等于 component lifecycle mutation。

## I12 — Evidence has one interpreting authority

同一 raw playback fact 不得被多个 semantic authority 独立解释成各自状态。

## I13 — Snapshot is projection

Snapshot 无 mutation authority；每个字段可追溯到唯一 authority/evidence source。

## I14 — RT firewall

RT hot path 不允许：

```text
global mutable-state locking
Context
capability resolution
Reconcile
generic event bus
filesystem/network
UI calls
unbounded allocation/blocking
```

---

# 36. 后续实现前必须建立的 Oracle

本 ADR 接受后，仍不得直接开始 FFmpeg/WASAPI 大规模产品实现。

首先建立 deterministic playback model。

## O1 — Normal playback

```text
produce
ready
consume
submit
render
slide window
```

验证 cursor 只从正确 evidence 推进。

## O2 — Same-track Dual Window seek

证明：

```text
TrackSession A
├ Active DecodeSession/gen17 @72s
└ Prepared DecodeSession/gen18 @100s
```

可以同时存在；gen17 继续合法 active，gen18 可以合法 prime。

## O3 — Generation admission

prepare 阶段：

```text
gen17 active result  -> accepted for ActiveWindow
gen18 prepare result -> accepted for PreparedWindow
unrelated gen16      -> rejected
```

promotion 后：

```text
gen17 late decode -> rejected
gen18 -> Active
```

不得使用 `!= current_generation` 假 oracle。

## O4 — Hard next-track

```text
TrackSession A / gen17
TrackSession B / gen18 prepared
physical cut
B promote
A resources retire in ownership order
```

## O5 — Late decode / wrong-window result

即使 GenerationId 存在，结果如果不再满足对应 window/admission，也必须丢弃。

## O6 — submitted vs rendered

position / ENDED 不得因为 submit 就提前推进。

## O7 — Physical fence failure

fence/flush 失败后不得假装切换或 stop 成功；必须显式 fail closed/recovery。

## O8 — PlayerGain

PlayerGain 参数变化：

```text
no timeline generation change
no TrackSession replacement
no required Composition reconcile
```

并验证参数 ramp 不产生不必要的 playback discontinuity。

## O9 — Processing graph swap

构建并发布新的 processing graph 时：

```text
no per-block Composition Kernel operation
no hidden Fiber order as DSP semantic order
```

## O10 — Provider withdrawal

Decoder/AudioOutput provider withdrawal 必须保持 K0 的：

```text
dependent teardown access
before provider final release
```

同时清理 Music-owned Track/Decode/Window nested resources，不把 provider mechanism 吞进 Music component。

## O11 — Evidence ownership

证明：

```text
raw EOF/render/fence
    -> Transport authority once
    -> derived typed domain fact
    -> Music policy
```

不存在两个 writer 对同一 playback fact 各自推进。

## O12 — Global snapshot provenance

每个 PlayerSnapshot 字段都注明：

```text
authority
source evidence
freshness / desired-vs-actual semantics where applicable
```

不存在双 writer。

---

# 37. 本 ADR 不冻结的内容

以下仍属于后续 representation / implementation 设计：

```text
TransportKernel 最终 Rust API
TransportKernel 是否最终保留这个名字
MusicKernel 是否最终保留 Kernel 后缀
TrackSession / DecodeSession 的 crate/module 布局
Decoder handle 的具体 Rust ownership representation
ring buffer 实现
buffer pool
线程数量
lock-free 方案
decode worker 调度
具体 PCM quantum
lookahead/window 大小
具体 SRC placement
EQ graph API
PlayerGain 最终 component owner
future crossfade/gapless
ReplayGain
device handoff policy
WASAPI/CoreAudio 具体 fence mechanism
```

这些未冻结项不得反过来改变本文已经冻结的 authority/lifetime/data-plane semantics。

---

# 38. 对现有 ARCH-003 的影响

如果本 ADR 最终从 `PROPOSED` 变为 `ACCEPTED`，应对现有 Playback Kernel authority 做 corrective refinement。

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
Music component
    -> composed lifecycle root for music/playback domain

MusicKernel
    -> music/product semantic authority

TransportKernel
    -> playback timeline authority

TrackSession
    -> media identity/source lifetime root

DecodeSession
    -> decoder cursor/lifetime for one temporal contribution

Window / Generation
    -> temporal runtime resources owned under Music component
```

这不是重开 Base Kernel K0，而是在 ARCH-003 尚未实现前，对 Playback authority decomposition 做 corrective refinement。

在 ADR ACCEPTED 之前，不修改 `registry.yml` 的 ARCH-003 authority，也不把本文当作已经冻结的 implementation authorization。

---

# 39. 最终冻结候选

> **Composition Kernel 管 provider/component 的空间组合与生命周期。**

> **Music component 是音乐播放域的 composed lifecycle root，但只能拥有 subordinate playback runtime，不能吞并独立 provider。**

> **MusicKernel 管音乐产品语义。**

> **TransportKernel 是唯一 playback timeline authority。**

> **TrackSession 表示媒体 identity/source lifetime；DecodeSession 表示一个可独立推进的 decoder cursor。**

> **ActiveWindow 与 PreparedWindow 可以同时存在，因此 generation 使用 admission/window-role 模型，而不是 global-current equality。**

> **PCM 是 canonical decoded-audio data plane，而不是全局消息总线。**

> **Composition topology 与 Audio Processing Graph topology 是两套不同 topology。**

> **Buffer 是存储单位；MediaSpan 是时间单位。**

> **Generation 管软件时代边界；Physical Fence 管真实声音边界。**

> **PlayerGain 属于 PCM-processing 语义；DeviceVolume 属于平台/device control。**

> **Command goes to its authority；raw evidence is interpreted once by the authority that owns the affected fact.**

> **Global visibility does not imply global ownership：全局可共享只读 Snapshot，但每个可变事实只能有一个 Authority/Writer。**

---

# 40. Review Gate

Corrective-1 已关闭首轮对抗审计中的三个 P0 模型缺口：

```text
Plugin vs Nested Runtime Resource
TrackSession vs DecodeSession under Dual Window
Generation admission under Active + Prepared
```

下一轮 Review 应重点验证：

```text
G1  Nested Runtime Resource 是否足以阻止“所有有 lifecycle 的对象都 Fiber 化”
G2  Dual Window + DecodeSession 是否与 Decoder provider withdrawal/teardown-access 一致
G3  Generation admission 是否足以处理 async late result，而不产生第二 timeline authority
G4  Composition topology 与 Processing graph topology 是否彻底分离
G5  command/evidence routing 是否真正满足 One Fact / One Authority / One Writer
G6  Music component ownership 是否足够窄，不会重新长成 PlayerEngine 巨石
G7  Physical Fence 是否仍完整继承 reference playback 的物理正确性
G8  Snapshot/global-state 模型是否没有任何隐藏 mutable global bag
```

在上述 Gate 通过前：

```text
STATUS = PROPOSED
IMPLEMENTATION AUTHORIZATION = NO
ARCH-003 AUTHORITY REVISION = NO
```
