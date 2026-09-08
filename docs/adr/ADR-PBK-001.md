# ADR-PBK-001：播放时间轴、媒体会话、PCM 数据面与共享状态边界

- **状态**：PROPOSED / Corrective-2 / Formal Gate Pending
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
  - 已验证的 seek / physical flush / stale-generation rejection / render accounting / ENDED 行为

---

# 0. Corrective-2 摘要

Corrective-1 已经关闭首轮对抗审计中的三个模型级 P0：

```text
P0-1  Plugin / Component 与 Nested Runtime Resource 边界不清
P0-2  Dual Window 与“一 TrackSession 一个 decoder cursor”不能同时成立
P0-3  Dual Window 与 global current-generation 判 stale 不能同时成立
```

Corrective-2 不再留下 Q1/Q2/Q3 三个开放命名/ownership 问题，直接冻结：

```text
TransportKernel      = 最终名称
MusicKernel          = 最终名称
TrackSession         = media identity / source lifetime root
DecodeSession        = one independently advancing decoder cursor/handle
```

并正式拆分三个此前都被写成 `owns` 的不同关系：

```text
Composition Lifecycle Root
Immediate Lifetime Owner
Semantic Authority
```

Corrective-2 同时确认 Corrective-1 的 G1-G8 设计审计全部 PASS，但 ADR 尚不进入 ACCEPTED；新增 Formal Gate，在状态模型通过之前：

```text
STATUS = PROPOSED
IMPLEMENTATION AUTHORIZATION = NO
ARCH-003 AUTHORITY REVISION = NO
```

---

# 1. 架构总原则

## C1 — Composition 是空间组合

> **Composition Kernel 管运行时组件的空间组合、capability reachability、Composition lifecycle 与 provider withdrawal。**

它不拥有播放时间，也不搬运媒体 payload。

## C2 — Transport 是时间 Authority

> **TransportKernel is the playback temporal authority.**

TransportKernel 是播放器唯一的 playback timeline / temporal semantic authority。

## C3 — Music 是产品语义 Authority

> **MusicKernel 是音乐播放器的产品/领域 semantic authority。**

它不是 stateless helper，也不是每首媒体一个实例。

## C4 — Kernel 表示 Authority Role，不表示 Plugin Boundary

> **本文中的 `Kernel` 表示 semantic authority role，不表示 Composition plugin boundary。**

因此：

```text
MusicComponent   = composed lifecycle root
MusicKernel      = music-domain semantic authority
TransportKernel  = playback temporal authority
```

## C5 — 独立可组合能力进入 Plugin；内部资源进入 ownership tree

> **Every independently composable runtime capability enters through the plugin/component model.**

> **Every non-plugin runtime resource belongs to exactly one ownership tree rooted at a composed component, and has exactly one immediate lifetime owner.**

内部 resource 有 lifecycle、state、teardown，并不自动意味着它应该成为 Fiber/plugin。

## C6 — PCM 是 canonical audio data plane，不是全局消息总线

> **连续 decoded-audio payload 收敛到 canonical PCM；control、evidence、metadata 等继续使用独立 typed contracts。**

## C7 — 全局可见不等于全局拥有

> **Global visibility does not imply global ownership. One fact, one semantic authority, one writer.**

---

# 2. D1 — Composition Kernel K0 不重开

Composition Kernel K0 继续只负责：

```text
plugin/component topology
capability reachability
binding lifecycle
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
generation admission
rendered position
PlayerGain
ENDED
```

冻结：

> **Composition Kernel owns runtime composition topology and lifecycle, not playback time or media payload.**

---

# 3. D2 — Plugin / Nested Runtime Resource / Data Item 三层模型

运行时对象分三层：

```text
Composition Component / Plugin
    ├── independently bindable capability provider
    ├── independently replaceable provider
    ├── provider-withdrawal boundary
    └── composition-level lifecycle

Nested Runtime Resource
    ├── MusicKernel
    ├── TransportKernel
    ├── TrackSession
    ├── DecodeSession
    ├── ActiveWindow / PreparedWindow
    ├── buffer/ring bookkeeping
    └── component-local processing graph/node when not independently composed

Data Item
    ├── PcmBlock
    ├── MediaSpan / provenance
    └── typed evidence record
```

是否成为 plugin 的核心判据不是“有没有状态/析构/replace”，而是：

```text
是否具有独立组合身份？
外部 component 是否通过 capability 绑定它？
是否可以独立成为 provider？
withdrawal 是否需要 Composition Kernel 主导 dependent-before-provider-release？
```

因此：

```text
TrackSession != plugin
DecodeSession != plugin
Window != plugin
PcmBlock != plugin
普通 Gain/EQ/SRC node != 自动成为 plugin
```

冻结：

> **Everything is Plugin 不等于 everything is individually a plugin。独立可组合能力通过 plugin boundary 进入；其余 runtime resource 必须进入一个明确的 component-rooted ownership tree。**

---

# 4. D3 — Ownership 术语冻结

本文禁止再用一个模糊的 `owns` 同时描述跨层关系。

以后必须区分：

## 4.1 Composition Lifecycle Root

`MusicComponent` 是其 subordinate playback runtime 的 **composition lifecycle root**。

含义：

```text
MusicComponent episode ends
    -> 所有 subordinate runtime resource 必须退出
```

它不意味着 MusicComponent 是所有内部对象的 immediate parent，也不意味着 MusicComponent 对所有事实具有 semantic authority。

## 4.2 Immediate Lifetime Owner

Nested Runtime Resource 形成严格树状 ownership：

```text
MusicComponent
├── MusicKernel
├── TransportKernel
└── TrackSession A
    ├── DecodeSession A17
    └── DecodeSession A18
```

冻结：

> **Every non-plugin runtime resource has exactly one immediate lifetime owner.**

不得存在：

```text
DecodeSession X
    -> owner A
    -> owner B
```

也不得存在 lifetime ownership cycle。

## 4.3 Semantic Authority

Semantic authority 与 lifetime ownership 正交。

例如：

```text
TransportKernel
    semantic authority over:
        playback cursor
        ActiveWindow / PreparedWindow roles
        generation admission
        window promotion/invalidation
        discontinuity execution

MusicKernel
    semantic authority over:
        repeat / shuffle
        playlist policy
        selection semantics
        user-visible playback-state meaning
```

冻结：

> **lifetime ownership != semantic authority**

以及：

> **本文禁止裸用 `owns` 描述跨层关系；必须明确是 lifecycle root、immediate lifetime owner 还是 semantic authority。**

---

# 5. D4 — MusicComponent 边界

MVP 保留 `Music` 作为 composed component / Fiber。

它是 subordinate playback runtime 的 lifecycle root，但不得重新长成 PlayerEngine 巨石。

概念：

```text
MusicComponent
├── MusicKernel
├── TransportKernel
└── TrackSession(s)
    └── DecodeSession(s)
```

MusicComponent 绑定外部 provider capability：

```text
Decoder
PcmSink / AudioOutput
future Processing provider when justified
future Metadata capability when justified
```

明确禁止：

```text
MusicComponent internally new FFmpegDecoder()
MusicComponent internally new WasapiOutput()
MusicComponent absorbs provider-global FFmpeg closure
MusicComponent owns platform output implementation
```

冻结：

> **MusicComponent roots subordinate playback lifecycle; it binds but does not absorb independently composed providers.**

---

# 6. D5 — MusicKernel 名称与职责冻结

最终名称：

```text
MusicKernel
```

不再保留 `MusicPolicy` / `MusicDomainKernel` 开放项。

MusicKernel 负责：

```text
play / pause semantic meaning
seek intent meaning
next / previous
repeat / shuffle
playlist policy
selection semantics
user-visible playback-state meaning
Transport terminal 后 next/repeat/stop 的产品决策
```

MusicKernel 不直接拥有/管理：

```text
decoder cursor
PCM block
ring
physical device handle
raw rendered counter
```

冻结：

> **MusicKernel is the music-domain semantic authority.**

---

# 7. D6 — TransportKernel 名称与职责冻结

最终名称：

```text
TransportKernel
```

不再保留 `TrackKernel` / `PlaybackTimelineKernel` 开放项。

原因：

- `TrackKernel` 与 `TrackSession` 混淆，而且范围过窄；
- `PlaybackTimelineKernel` 无法覆盖 generation admission、window promotion、discontinuity、physical-cut coordination 与 raw playback evidence；
- `TransportKernel` 正好表达完整职责。

TransportKernel 是唯一 playback temporal authority，负责：

```text
playback cursor semantics
MediaSpan timeline authority
ActiveWindow
PreparedWindow
window roles/frontiers
GenerationId / admission
window promotion
window invalidation
discontinuity execution
physical-cut coordination state
raw playback evidence interpretation
```

冻结：

> **TransportKernel is the playback temporal authority.**

MusicKernel 不与 TransportKernel 共同解释 raw cursor/render/EOF 事实。

---

# 8. D7 — TrackSession 与 DecodeSession 的 lifetime 关系冻结

## 8.1 TrackSession

TrackSession 是：

```text
media identity / source lifetime root
```

概念：

```text
TrackSession
├── source identity
├── media descriptor
├── duration / probe truth
├── source-level metadata identity
└── 0..N DecodeSession
```

TrackSession 不持有“唯一 decoder cursor”这个语义。

TrackSession 也不具有 `active / prepared / retiring` 这种互斥状态；同 Track seek 时它可以同时贡献 Active 与 Prepared。

## 8.2 DecodeSession

DecodeSession 是：

```text
one independently advancing decoder cursor/handle
```

概念：

```text
DecodeSession
├── decoder handle/cursor
├── GenerationId
├── decode position
├── EOF / seek-local state
└── target-window contribution
```

冻结 lifetime 关系：

> **TrackSession is the immediate lifetime owner of its DecodeSession(s).**

> **Each DecodeSession owns exactly one independently advancing decoder cursor/handle semantic slot.**

具体 Rust representation 仍不冻结：

```text
Box<>
Arc<>
lease
provider-issued token
opaque handle
...
```

同 Track seek：

```text
TrackSession A
├── DecodeSession A17 @72s   -> ActiveWindow
└── DecodeSession A18 @100s  -> PreparedWindow
```

next：

```text
TrackSession A
└── DecodeSession A17        -> ActiveWindow

TrackSession B
└── DecodeSession B18        -> PreparedWindow
```

冻结：

> **decoder cursor belongs to DecodeSession rather than TrackSession.**

---

# 9. D8 — 双 Window 模型

MVP temporal model：

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
    gen17 / DecodeSession A17 @72s

PreparedWindow
    gen18 / DecodeSession A18 @100s
```

下一曲：

```text
ActiveWindow
    TrackSession A / gen17

PreparedWindow
    TrackSession B / gen18
```

seek 与 next 共用 execution skeleton：

```text
prepare
    -> prime
    -> close old admission
    -> physical fence
    -> promote
    -> retire old
```

但其 Music-domain meaning 不同。

MVP 不授权第三个 simultaneously prepared playback window。

---

# 10. D9 — Generation Admission 模型

Dual Window 下合法状态包括：

```text
active_generation   = gen17
prepared_generation = gen18
```

因此禁止：

```text
result.generation != current_generation
    => stale
```

冻结：

> **Generation is a window-scoped temporal identity. Stale means “no longer admitted by the owning temporal role”, not “not equal to one global current generation”.**

prepare 阶段可存在：

```text
admitted_generations = {
    active_generation,
    prepared_generation
}
```

但角色权限不同：

```text
Active
    -> 可满足当前 render path

Preparing
    -> 可接受 PreparedWindow 的 decode/prime result
    -> 不得冒充 current physical-output authority
```

promotion 后：

```text
old active -> no longer admitted
prepared   -> Active
prepared slot cleared
```

晚到结果只有同时满足 generation + window role + admission contract 才能被接收。

---

# 11. D10 — 切换分类

## Continuous Update

```text
PlayerGain
EQ parameter
balance
DSP parameter
```

性质：

```text
no timeline generation change
no TrackSession replacement
```

## Intra-Track Discontinuity

```text
seek
loop jump
chapter jump
```

性质：

```text
same TrackSession
new DecodeSession / generation / PreparedWindow
```

## Track Replacement

```text
next
previous
open another media
```

性质：

```text
new TrackSession
new DecodeSession
generation/window replacement
```

## Topology Handoff

```text
Decoder provider replacement
AudioOutput provider replacement
future independent Processing provider replacement
```

属于 Composition topology；不天然意味着 media timeline 改变。

---

# 12. D11 — MVP 不采用 seek fade / old-tail masking

MVP 不允许为了掩盖 seek latency 而继续播放旧位置并淡出。

冻结：

```text
prepare new window
    -> prime
    -> physical cut old
    -> promote new
```

未来 crossfade / fade 如有真实需求，独立 ADR 讨论 simultaneous renderable contributions；双 Window 本身不自动授权 crossfade。

---

# 13. D12 — Physical Fence / Flush 是硬正确性边界

历史事实继续成立：

```text
decoded != queued != submitted != rendered
```

因此：

> **Logical invalidation != Physical stop.**

至少这些操作必须经过 Physical Fence：

```text
stop
seek commit
hard next/previous
fatal recovery when old tail must be killed
```

抽象：

```text
close old admission
    -> prevent new old-generation submission
    -> physical fence / flush handshake
    -> definitive verdict
    -> promote / stop / fail closed
```

Generation 不能替代 Physical Fence。

冻结：

> **fence 成功后，被截断 generation 不得继续产生新的可听输出。**

平台 mechanism 可以不同，但跨平台语义必须一致。

---

# 14. D13 — Canonical Audio Data Plane

采用：

> **PCM 是 Qianqian 的 canonical decoded-audio data plane，不是通用 plugin message bus。**

```text
Encoded Media
    -> Decoder
    -> Canonical PCM
    -> Audio Processing Graph
    -> AudioOutput
```

不属于 PCM data plane：

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

这些走独立 typed contracts。

---

# 15. D14 — Composition Topology != Audio Processing Graph Topology

## Composition Topology

由 Composition Kernel 管：

```text
Music
Decoder provider
AudioOutput provider
future independent Processing provider
Recorder / Analyzer provider when justified
```

关注：

```text
capability
binding
lifecycle
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
parameter updates
graph swap
```

普通：

```text
insert EQ node
change limiter
replace Gain node
```

不自动触发 Composition Reconcile。

冻结：

> **Composition topology composes providers; Audio Processing Graph orders PCM-processing nodes.**

---

# 16. D15 — MediaSpan 与 PCM Block 分离

冻结：

> **Buffer 是存储/处理单位；MediaSpan 是媒体时间单位。**

不允许：

```text
BufferId == timeline identity
```

概念：

```text
MediaSpan {
    generation,
    media_start,
    media_end,
}

PcmBlock {
    provenance,
    media_span,
    format,
    frames,
}
```

Processing 可以改变 frame count / layout / sample rate / block boundaries，但不能偷偷改变 MediaSpan 的领域意义。

---

# 17. D16 — Command 按 Authority / Capability 路由

禁止所有 command 都经过 MusicKernel。

冻结：

> **Command goes to the authority that owns the mutated fact.**

```text
UI / Integrations
    ├── PlaybackIntent     -> MusicKernel -> TransportKernel when temporal mutation is needed
    ├── AudioControl       -> Processing authority
    ├── DeviceVolume       -> AudioOutput capability
    └── CompositionIntent  -> CompositionKernel
```

例如：

```text
seek / next
    -> Music semantic intent
    -> Transport execution

PlayerGain / EQ parameter
    -> processing authority
    -> no Transport hop unless timeline semantics change

output provider replacement
    -> Composition/provider-control path
```

MusicKernel 不是 God Router。

---

# 18. D17 — Evidence 单向解释

Raw playback evidence：

```text
Decoder EOF
seek landing
late decode result
submitted evidence
rendered evidence
physical fence verdict
```

先进入 TransportKernel。

TransportKernel 形成 typed derived domain facts：

```text
LandingCommitted(...)
TransportDrained(...)
TransportDiscontinuityFailed(...)
TransportOutputUnavailable(...)
```

MusicKernel 只解释这些派生事实并决定：

```text
ENDED
repeat
next
stop
user-visible product state
```

冻结：

> **Raw evidence is interpreted once by the semantic authority that owns the affected fact. Other authorities receive derived typed facts.**

---

# 19. D18 — PlayerGain 与 DeviceVolume 分离

## PlayerGain

Qianqian 主播放器音量默认表示 PCM processing gain：

```text
PCM -> Gain -> PCM
```

参数变化可以做短 ramp 防 click/pop；这属于 DSP parameter smoothing，不是 seek fade。

## DeviceVolume

属于 AudioOutput/platform 的可选 device-control capability：

```text
system endpoint volume
hardware volume
OS mixer volume
```

冻结：

> **PlayerGain is audio processing; DeviceVolume is platform/device control.**

---

# 20. D19 — Data Plane Taxonomy 保留，但不是 Plugin Taxonomy

保留：

```text
Producer
Transformer
Consumer
```

示例：

```text
Producer     -> Decoder output / Synth
Transformer  -> Gain / EQ / SRC / Limiter / Mixer
Consumer     -> AudioOutput / Recorder / Analyzer tap
```

但这只是 data-plane taxonomy。

不得推出：

```text
一个 feature = 一个 plugin
一个 node = 一个 Fiber
一个 parameter = 一个 plugin
一个 plugin = 一个 DLL
```

---

# 21. D20 — 全局共享状态

Qianqian 不建立：

```text
GlobalPlayerState
Arc<Mutex<AppState>>
MutableEverything
```

作为跨模块共同写入的总状态。

冻结：

> **Global visibility does not imply global ownership.**

> **One fact, one semantic authority, one writer.**

Authority islands：

```text
CompositionKernel
    -> component lifecycle / capability reachability / composition truth

MusicKernel
    -> music-domain/product semantics

TransportKernel
    -> playback timeline / windows / generation admission / raw playback evidence

AudioOutput
    -> device/session mechanism + physical evidence production

Processing Authority
    -> processing graph + DSP parameter truth
```

这些 authority 不共享可写内部结构。

---

# 22. D21 — PlayerSnapshot 只是只读投影

可以暴露统一：

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
```

但 Snapshot 是 materialized read model，不是 authority。

禁止通过修改 Snapshot 改变 runtime。

冻结：

```text
Command
    -> owning authority
    -> runtime mutation
    -> evidence / authoritative truth
    -> PlayerSnapshot projection
```

每个 Snapshot 字段必须能追溯到唯一 authority/provenance。

---

# 23. D22 — Desired State != Runtime Truth

例如：

```text
desired_output = Bluetooth Headset
actual_output  = Speakers
```

或者：

```text
desired_player_gain = 0.5
actual ramp         = 0.62 -> 0.58 -> ... -> 0.50
```

必须允许：

```text
desired != actual
```

不得用一个字段同时表示用户 intent 与 runtime fact。

---

# 24. D23 — RT Firewall

Realtime path 只消费 pre-bound / published RT-safe state。

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

Control thread 可以 build / validate 新 RT view 或 processing graph，再通过 bounded RT-safe publication 切换。

---

# 25. Seek / Next / Stop 冻结形状

## Seek

```text
seek(T)
    -> MusicKernel interprets intent
    -> TransportKernel allocates Prepared role/generation
    -> same TrackSession creates new DecodeSession
    -> prime PreparedWindow
    -> close old admission
    -> Physical Fence
    -> promote PreparedWindow
    -> old generation no longer admitted
```

## Next / Previous

```text
prepare new TrackSession
    -> create DecodeSession / PreparedWindow
    -> prime
    -> close old admission
    -> Physical Fence
    -> promote
    -> retire old DecodeSession(s)
    -> release old TrackSession when ownership tree drains
```

## Stop

```text
close admission
    -> prevent new submission
    -> Physical Fence
    -> prove old media cannot continue audibly
    -> publish stopped semantic state
```

---

# 26. ENDED 原则

禁止：

```text
Decoder EOF -> immediately ENDED
```

必须保留等价 predicate：

```text
producer terminal
AND software media pipeline drained
AND no relevant in-flight media
AND no submitted-but-unrendered media
```

TransportKernel 从 raw EOF/render/submission evidence 得出 transport-drained truth；MusicKernel 再解释其产品语义。

---

# 27. 明确拒绝的方案

```text
R1   Composition Kernel 管 PCM / Track
R2   MusicKernel 与 TransportKernel 同时拥有 timeline truth
R3   每首 MP3 一个 MusicKernel
R4   TrackSession 持有唯一 decoder cursor
R5   generation != current_generation 作为 stale 通用定义
R6   Mutable GlobalPlayerState
R7   PCM 是通用消息总线
R8   BufferId 是媒体时间 identity
R9   generation 代替 physical flush
R10  MVP seek fade / old-tail masking
R11  主播放器 volume 直接修改系统音量
R12  每个 DSP node / parameter 都是 plugin/Fiber
R13  DSP graph mutation 必然触发 Composition Reconcile
R14  所有 UI command 都经过 MusicKernel
R15  Decoder/AudioOutput mechanism 被 MusicComponent 吞入
R16  Raw playback evidence 被 MusicKernel / TransportKernel 各解释一次
R17  PlayerSnapshot 成为可写状态库
R18  裸用 owns 混淆 lifecycle root / immediate owner / semantic authority
```

---

# 28. 架构不变量

```text
I1   Generic Composition Kernel 不得依赖 Music/Transport/PCM/TrackSession 等领域概念
I2   Plugin boundary 必须由 independent composability 挣得
I3   每个 Nested Runtime Resource 恰有一个 immediate lifetime owner
I4   所有 nested ownership path 最终到达一个 composed lifecycle root
I5   lifetime ownership graph 无环
I6   lifetime ownership != semantic authority
I7   One fact / one semantic authority / one writer
I8   MVP 最多 1 ActiveWindow + 0..1 PreparedWindow
I9   同 Track Active + Prepared 可拥有不同 DecodeSession
I10  Generation validity 由 admission/window role 决定
I11  每 PCM block/callback 不经过 Context/resolve/Reconcile/Fiber/Effect
I12  stop/seek/hard replacement 必须保留 Physical Fence correctness
I13  Buffer != MediaSpan
I14  submitted != rendered
I15  Composition topology != Processing topology
I16  raw playback evidence 只由 Transport temporal authority 首次解释
I17  Snapshot 是 projection，不是 writer
I18  RT hot path 不访问 mutable global bag
```

---

# 29. Corrective-2 Design Review 结论

Corrective-1 的 G1-G8 现判定：

| Gate | Verdict |
|---|---|
| G1 Nested Runtime Resource | **PASS** |
| G2 Dual Window + provider withdrawal | **PASS，ownership terminology 已 Corrective-2 收紧** |
| G3 Generation admission | **PASS** |
| G4 Composition graph != Processing graph | **PASS** |
| G5 Command / Evidence authority routing | **PASS** |
| G6 Music component anti-monolith | **PASS** |
| G7 Physical Fence | **PASS** |
| G8 Snapshot / global-state | **PASS** |

Q1/Q2/Q3 已全部关闭：

```text
Q1 -> TransportKernel final
Q2 -> MusicKernel final
Q3 -> decoder cursor belongs to DecodeSession; TrackSession owns DecodeSession lifetime subtree
```

因此：

```text
DESIGN REVIEW = PASS
PROSE OPEN QUESTIONS = CLOSED
```

但这还不是 ACCEPTED。

---

# 30. Formal Gate — ACCEPTED 前必须通过

我们担心的主要剩余风险已经从“边界不清”转为“合法状态组合是否会撞车”。

因此 ADR 从 Corrective-2 起增加 Formal Gate。

## F1 — PlaybackTemporal 模型

建议使用 TLA+/TLC 建立最小 temporal model，至少覆盖：

```text
ActiveWindow
PreparedWindow
Generation admission
DecodeSession
seek / next / stop
PhysicalFence
submitted / rendered
EOF / ENDED
late decode
rapid superseding discontinuity
provider withdrawal interaction
```

至少检查：

```text
AtMostOneActiveWindow
AtMostOnePreparedWindow
ActiveAndPreparedMayHaveDifferentGenerations
AcceptedDecodeResult => generation is admitted for its window role
RetiredGeneration => no longer admitted
HardPromotion => successful PhysicalFence
FenceFailure => no fake promotion success
rendered <= submitted
ENDED => producer terminal + pipeline drained + no submitted-unrendered media
old generation cannot submit after successful promotion
superseded Prepared generation cannot re-enter
```

必须包含 rapid-command traces，例如：

```text
seek(100)
seek(200)
next(B)
stop
```

并明确谁拥有 supersede/cancel/replace authority。

## F2 — PlaybackOwnership 模型

独立验证 lifetime ownership：

```text
MusicComponent
├── MusicKernel
├── TransportKernel
└── TrackSession
    └── DecodeSession
```

至少检查：

```text
exactly one immediate lifetime owner
all nested ownership paths terminate at one composed lifecycle root
no ownership cycles
provider final release cannot precede dependent teardown access
TrackSession teardown implies all owned DecodeSession resources are discharged
```

## F3 — Negative Controls

模型必须能抓到故意植入的错误，否则不得把 TLC PASS 当作有效证据。

至少做：

```text
BUG-A  去掉 Promote 前 fence-completed 条件 -> 必须出现 counterexample
BUG-B  恢复 generation != currentGeneration -> Dual Window 必须失败
BUG-C  允许 retired generation 继续 admitted -> late decode invariant 必须失败
BUG-D  provider 先 final release 再 teardown DecodeSession -> lifetime invariant 必须失败
BUG-E  EOF 直接导致 ENDED -> submitted-not-rendered trace 必须失败
```

## F4 — Deterministic Executable Oracle

TLA+ 通过后，再建立无真实线程/无 FFmpeg/WASAPI 的 deterministic playback simulator，至少覆盖：

```text
normal playback
dual-window same-track seek
rapid seek supersede
hard next
late decode
wrong-window result
physical fence failure
submitted != rendered
provider withdrawal
snapshot provenance
```

生产实现必须对齐该 executable oracle，而不是直接从 prose 自由发挥。

---

# 31. ADR 状态机

Corrective-2 之后的 gate：

```text
ADR-PBK-001 Corrective-2
        ↓
DESIGN REVIEW PASS
        ↓
PlaybackTemporal formal model
        ↓
PlaybackOwnership formal model
        ↓
negative controls PASS
        ↓
deterministic executable oracle
        ↓
ADR-PBK-001 = ACCEPTED
        ↓
corrective refinement of ARCH-003 authority
        ↓
implementation issue may separately authorize production work
```

当前冻结：

```text
STATUS = PROPOSED / CORRECTIVE-2 / FORMAL GATE PENDING
DESIGN REVIEW = PASS
IMPLEMENTATION AUTHORIZATION = NO
ARCH-003 AUTHORITY REVISION = NO
```

---

# 32. 本 ADR 不冻结的 representation

以下留给后续 implementation design：

```text
MusicKernel / TransportKernel 最终 Rust API
TrackSession / DecodeSession crate/module 布局
Decoder handle 的 Box/Arc/lease/token/opaque representation
ring buffer
buffer pool
thread count
lock-free structure
decode-worker scheduling
PCM quantum
lookahead/window size
SRC placement
EQ graph API
PlayerGain 最终 component-local 或 Processing-provider placement
future crossfade/gapless
ReplayGain
device-handoff policy
WASAPI/CoreAudio/AAudio 具体 fence mechanism
```

这些 representation 不得反过来改变本文已冻结的 authority / lifetime / data-plane semantics。

---

# 33. 对 ARCH-003 的影响

ADR 仍为 PROPOSED，因此现在不修改 `registry.yml` 的 ARCH-003 authority。

Formal Gate 全部通过并将本文改为 ACCEPTED 后，再把现有较宽泛的 Playback authority 拆成：

```text
MusicComponent
    -> composed lifecycle root

MusicKernel
    -> music-domain semantic authority

TransportKernel
    -> playback temporal authority

TrackSession
    -> media identity / source lifetime root

DecodeSession
    -> independently advancing decoder cursor/handle lifetime

Window / Generation
    -> subordinate temporal runtime resources
```

这不是重开 Base Kernel K0，而是在 ARCH-003 尚未实现前完成 Playback authority refinement。

---

# 34. Final Freeze Candidate

> **Composition Kernel 管 provider/component 的空间组合与生命周期。**

> **MusicComponent 是 playback domain 的 composed lifecycle root，但不能吞并独立 provider。**

> **MusicKernel is the music-domain semantic authority.**

> **TransportKernel is the playback temporal authority.**

> **Kernel 表示 semantic authority role，不表示 Composition plugin boundary。**

> **TrackSession 表示 media identity / source lifetime；DecodeSession 表示一个 independently advancing decoder cursor/handle。**

> **Every non-plugin runtime resource belongs to exactly one ownership tree rooted at a composed component and has exactly one immediate lifetime owner.**

> **lifetime ownership != semantic authority。**

> **ActiveWindow 与 PreparedWindow 可以同时存在；Generation validity 使用 admission/window-role，而不是 global-current equality。**

> **PCM 是 canonical decoded-audio data plane，而不是全局消息总线。**

> **Composition topology 与 Audio Processing Graph topology 是两套不同 topology。**

> **Buffer 是存储单位；MediaSpan 是时间单位。**

> **Generation 管软件时代边界；Physical Fence 管真实声音边界。**

> **PlayerGain 属于 PCM processing；DeviceVolume 属于平台/device control。**

> **Command goes to its authority；raw evidence is interpreted once by the authority that owns the affected fact.**

> **Global visibility does not imply global ownership；全局可共享只读 Snapshot，但每个 mutable fact 只能有一个 semantic authority/writer。**
