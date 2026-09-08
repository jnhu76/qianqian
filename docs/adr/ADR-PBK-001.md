# ADR-PBK-001：播放时间轴、媒体会话、PCM 数据面与共享状态边界

- **状态**：PROPOSED / FORMAL CORE PASS
- **日期**：2026-09-08
- **作用域**：Qianqian Playback Architecture / ARCH-003
- **不重开**：Base / Composition Kernel K0

## 0. Corrective-2 冻结摘要

Corrective-2 关闭此前三个开放项，并收紧 ownership 术语：

```text
TransportKernel = 最终名称
MusicKernel     = 最终名称
TrackSession    = media identity / source lifetime root
DecodeSession   = one independently advancing decoder cursor/handle
```

本文中的 `Kernel` 表示 **semantic authority role**，不表示 Composition plugin boundary。

同时正式区分：

```text
Composition Lifecycle Root
Immediate Lifetime Owner
Semantic Authority
```

Corrective-1 的 G1-G8 设计审计全部 PASS。Formal Acceptance 已收缩为一个**小型 blocking temporal core**：只有 `PlaybackTemporal` 模型（五组高风险 temporal 语义）+ 4 个 core negative controls 阻塞 ACCEPTED。`PlaybackOwnership` 与其余 mutation 保留为 supporting evidence，不阻塞 ACCEPTED。deterministic executable oracle 移到 ACCEPTED 之后，作为 implementation entry，不是 architecture decision 成立的前置条件。

```text
STATUS = PROPOSED / FORMAL CORE PASS
DESIGN REVIEW = PASS
CORE TEMPORAL CHECKS = PASS
SUPPORTING FORMAL EVIDENCE = RETAINED / NON-BLOCKING
IMPLEMENTATION AUTHORIZATION = NO
ARCH-003 AUTHORITY REVISION = NO
```

---

# 1. 总体架构原则

## 1.1 Composition Kernel

Composition Kernel 管：

```text
component/plugin topology
capability reachability
binding lifecycle
Fiber / Effect lifetime
Reconcile
provider withdrawal
```

它不拥有：

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

> **Composition Kernel owns spatial composition and lifecycle, not playback time or media payload.**

## 1.2 MusicKernel

最终名称：`MusicKernel`。

它是 music-domain semantic authority，负责：

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

它不是每首媒体一个实例，也不是 stateless policy helper。

> **MusicKernel is the music-domain semantic authority.**

## 1.3 TransportKernel

最终名称：`TransportKernel`。

它是 playback temporal authority，负责：

```text
playback cursor semantics
MediaSpan timeline authority
ActiveWindow / PreparedWindow
window roles/frontiers
GenerationId / admission
window promotion/invalidation
discontinuity execution
physical-cut coordination
raw playback evidence interpretation
```

> **TransportKernel is the playback temporal authority.**

## 1.4 Kernel != Plugin

冻结：

> **本文中的 Kernel 表示 semantic authority role，不表示 Composition plugin boundary。**

因此：

```text
MusicComponent   = composed lifecycle root
MusicKernel      = music-domain semantic authority
TransportKernel  = playback temporal authority
```

---

# 2. Plugin / Nested Runtime Resource / Data Item

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
    ├── ring/window bookkeeping
    └── component-local processing graph/node when not independently composed

Data Item
    ├── PcmBlock
    ├── MediaSpan / provenance
    └── typed evidence record
```

冻结：

> **Active / Prepared 是 TransportKernel 内的 temporal role / slot，不是具有独立 lifecycle 的 Nested Runtime Resource。**

ActiveWindow / PreparedWindow 描述 slot 上的 temporal role。不为了满足「every nested resource has an immediate owner」而把 Window 实体化为拥有独立生命周期的资源；Window 因此不引入额外的 immediate lifetime owner 问题。

成为 plugin 的判据不是“有没有状态/析构/replace”，而是：

```text
是否具有独立组合身份？
外部 component 是否通过 capability 绑定它？
是否可以独立成为 provider？
withdrawal 是否需要 Composition Kernel 主导 dependent-before-provider-release？
```

所以：

```text
TrackSession != plugin
DecodeSession != plugin
Window != plugin
PcmBlock != plugin
普通 Gain/EQ/SRC node != 自动成为 plugin
```

> **Everything is Plugin 不等于 everything is individually a plugin。独立可组合能力通过 plugin boundary 进入；其余 runtime resource 进入明确的 component-rooted ownership tree。**

---

# 3. Ownership 术语冻结

本文禁止裸用一个 `owns` 同时描述跨层关系。

## 3.1 Composition Lifecycle Root

`MusicComponent` 是 subordinate playback runtime 的 composition lifecycle root。

```text
MusicComponent episode ends
    -> subordinate playback runtime 必须全部退出
```

这不意味着 MusicComponent 是所有内部对象的 immediate parent，也不意味着它对所有事实拥有 semantic authority。

## 3.2 Immediate Lifetime Owner

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

且所有 ownership path 必须最终到达一个 composed lifecycle root；不得有多 owner 或 cycle。

## 3.3 Semantic Authority

Semantic authority 与 lifetime ownership 正交：

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

> **跨层文档必须明确 lifecycle root、immediate lifetime owner 或 semantic authority，不得只写裸 `owns`。**

---

# 4. MusicComponent 边界

MVP 保留 `Music` 作为 composed component / Fiber。

它作为 playback domain 的 lifecycle root，可以包含 subordinate runtime：

```text
MusicComponent
├── MusicKernel
├── TransportKernel
└── TrackSession(s)
    └── DecodeSession(s)
```

但它只绑定独立 provider：

```text
Decoder
PcmSink / AudioOutput
future independent Processing provider
future Metadata capability
```

禁止：

```text
MusicComponent internally new FFmpegDecoder()
MusicComponent internally new WasapiOutput()
MusicComponent absorbs provider-global FFmpeg closure
MusicComponent owns platform output implementation
```

> **MusicComponent roots subordinate playback lifecycle; it binds but does not absorb independently composed providers.**

---

# 5. TrackSession / DecodeSession

## 5.1 TrackSession

TrackSession 表示：

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

TrackSession 不承担“唯一 decoder cursor”语义，也不具有互斥的 active/prepared 状态。

## 5.2 DecodeSession

DecodeSession 表示：

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

冻结：

> **TrackSession is the immediate lifetime owner of its DecodeSession(s).**

> **decoder cursor belongs to DecodeSession rather than TrackSession.**

具体 Rust representation 不冻结：

```text
Box / Arc / lease / provider-issued token / opaque handle / ...
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

---

# 6. Dual Window 与 Generation Admission

MVP temporal model：

```text
1 ActiveWindow
0..1 PreparedWindow
```

合法状态：

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

prepare 阶段：

```text
Active(gen17)
    -> 可继续满足当前 render path

Preparing(gen18)
    -> 可接受 PreparedWindow decode/prime result
    -> 不得冒充当前 physical-output authority
```

promotion 后：

```text
old active -> no longer admitted
prepared   -> Active
prepared slot cleared
```

晚到结果只有同时满足 generation + window role + admission contract 才能被接收。

seek 与 next 共用 execution skeleton：

```text
prepare
    -> prime
    -> close old admission
    -> physical fence
    -> promote
    -> retire old
```

MVP 不授权第三个 simultaneously prepared playback window。

---

# 7. 切换分类

```text
Continuous Update
    PlayerGain / EQ / balance / DSP parameter
    -> no timeline generation change

Intra-Track Discontinuity
    seek / loop jump / chapter jump
    -> same TrackSession
    -> new DecodeSession / generation / PreparedWindow

Track Replacement
    next / previous / open media
    -> new TrackSession / DecodeSession / generation/window

Topology Handoff
    Decoder / AudioOutput / independent Processing provider replacement
    -> Composition topology
```

MVP 不采用 seek fade / old-tail masking。

未来 crossfade 如有真实需求，需要独立 ADR 讨论 simultaneous renderable contributions；Dual Window 本身不自动授权 crossfade。

---

# 8. Physical Fence / Flush

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

> **fence 成功后，被截断 generation 不得继续产生新的可听输出。**

冻结（claim 后不可逆）：

> **Physical Fence 一旦进入不可逆 / claimed 阶段，后续 intent 不得取消或改写已经 claim 的 physical transaction。**

fence 在途期间新命令如何排队（reject / defer / coalesce / latest-wins）不在本文冻结，留给 executable implementation 验证。

---

# 9. Canonical Audio Data Plane

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

# 10. Composition Topology != Audio Processing Graph

## Composition Topology

由 Composition Kernel 管：

```text
Music
Decoder provider
AudioOutput provider
future independent Processing provider
Recorder / Analyzer provider when justified
```

关注 capability / binding / lifecycle / withdrawal / reconcile。

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

关注 ordered PCM transforms / format / RT publication / parameter update / graph swap。

普通 insert/remove/update DSP node 不自动触发 Composition Reconcile。

> **Composition topology composes providers; Audio Processing Graph orders PCM-processing nodes.**

---

# 11. MediaSpan != PCM Block

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

Processing 可以改变 frame count/layout/sample rate/block boundaries，但不能偷偷改变 MediaSpan 的领域意义。

---

# 12. Command / Evidence Routing

冻结：

> **Command goes to the authority that owns the mutated fact.**

```text
UI / Integrations
    ├── PlaybackIntent     -> MusicKernel -> TransportKernel when temporal mutation is needed
    ├── AudioControl       -> Processing authority
    ├── DeviceVolume       -> AudioOutput capability
    └── CompositionIntent  -> CompositionKernel
```

MusicKernel 不是 God Router。

Raw playback evidence：

```text
Decoder EOF
seek landing
late decode result
submitted evidence
rendered evidence
physical fence verdict
```

先进入 TransportKernel；TransportKernel 再产生 typed derived domain facts 给 MusicKernel。

> **Raw evidence is interpreted once by the semantic authority that owns the affected fact. Other authorities receive derived typed facts.**

---

# 13. PlayerGain / DeviceVolume

PlayerGain 是 PCM processing：

```text
PCM -> Gain -> PCM
```

主播放器 UI 的普通 volume 默认表示 PlayerGain。

短 ramp 用于防 click/pop，是 DSP parameter smoothing，不是 seek fade。

DeviceVolume 属于 AudioOutput/platform 的可选 device-control capability。

> **PlayerGain is audio processing; DeviceVolume is platform/device control.**

---

# 14. Data Plane Taxonomy

保留：

```text
Producer
Transformer
Consumer
```

但它只是 data-plane taxonomy，不是 Plugin taxonomy。

不得推出：

```text
一个 feature = 一个 plugin
一个 node = 一个 Fiber
一个 parameter = 一个 plugin
一个 plugin = 一个 DLL
```

---

# 15. 全局共享状态

禁止：

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

# 16. PlayerSnapshot

可以暴露统一只读 projection：

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
├── audio
└── diagnostics
```

Snapshot 不是 authority，不能通过修改 Snapshot 改 runtime。

```text
Command
    -> owning authority
    -> runtime mutation
    -> evidence / authoritative truth
    -> PlayerSnapshot projection
```

每个 Snapshot 字段必须能追溯到唯一 authority/provenance。

Desired 与 actual/runtime truth 必须分离。

---

# 17. RT Firewall

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

Control thread 可 build/validate 新 RT view 或 processing graph，再做 bounded RT-safe publication。

---

# 18. Seek / Next / Stop / ENDED

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
    -> release old TrackSession when ownership subtree drains
```

## Stop

```text
close admission
    -> prevent new submission
    -> Physical Fence
    -> prove old media cannot continue audibly
    -> publish stopped semantic state
```

冻结（stop × 自然 ENDED 竞态）：

> **当 hard stop / discontinuity 的 Physical Fence 在途时，自然 EOF / drain 证据不得提前终态化（ENDED / final terminalization）而销毁完成该 fence 所需的 active temporal state。**

EOF evidence 可以记录，producer terminal 可以记录；但 ENDED / final terminalization 不得抢在在途 Physical Fence 之前销毁必要状态。该不变量由形式化 counterexample 挣得：无此裁决时存在「ENDED 抢先移除 ActiveWindow → fence 永久无法完成 → 命令路径锁死」的可达坏状态。

fence 在途期间是否接受新 seek/next（reject / defer / coalesce / latest-wins）是模型/实现决策，不在本文冻结；本文只冻结上一条 claimed-transaction 不可逆规则（§8）。

## ENDED

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

TransportKernel 从 raw evidence 得出 transport-drained truth；MusicKernel 再解释产品语义。

## Prepared EOF

冻结：

> **EOF evidence does not imply PreparedWindow readiness.**
> **Prepared contribution 在正常 readiness 前 terminal，必须得到显式 outcome，不得 silently become Ready。**

具体 terminal 分类（prepare failed / empty media / seek-to-EOF 等）与处理策略留给实现与 executable oracle，不在本文设计完整状态机。

---

# 19. 架构不变量

```text
I1   Generic Composition Kernel 不依赖 Music/Transport/PCM/TrackSession 等领域概念
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
I12  stop/seek/hard replacement 保留 Physical Fence correctness
I13  Buffer != MediaSpan
I14  submitted != rendered
I15  Composition topology != Processing topology
I16  raw playback evidence 只由 Transport temporal authority 首次解释
I17  Snapshot 是 projection，不是 writer
I18  RT hot path 不访问 mutable global bag
```

---

# 20. Corrective-2 Design Review 结论

| Gate | Verdict |
|---|---|
| G1 Nested Runtime Resource | **PASS** |
| G2 Dual Window + provider withdrawal | **PASS；ownership terminology 已 Corrective-2 收紧** |
| G3 Generation admission | **PASS** |
| G4 Composition graph != Processing graph | **PASS** |
| G5 Command / Evidence authority routing | **PASS** |
| G6 Music component anti-monolith | **PASS** |
| G7 Physical Fence | **PASS** |
| G8 Snapshot / global-state | **PASS** |

三个开放项全部关闭：

```text
Q1 -> TransportKernel final
Q2 -> MusicKernel final
Q3 -> decoder cursor belongs to DecodeSession;
      TrackSession is the immediate lifetime owner of DecodeSession(s)
```

因此：

```text
DESIGN REVIEW = PASS
PROSE OPEN QUESTIONS = CLOSED
```

但这还不是 ACCEPTED。

---

# 21. Formal Acceptance — ACCEPTED 前必须通过

剩余风险已经从“边界是否清楚”变为“合法状态组合是否会撞车”。

形式化验证在这里的职责是**攻击高风险状态组合**：多个本来都合法的状态/事件组合之后，是否产生反直觉的非法状态。它不为 Playback Architecture 建立第二份完整实现；结构性架构边界由本文冻结语义、类型系统、模块边界与普通工程测试约束，只有存在复杂状态交错风险时才升级为形式化模型。

## 必须项（blocking）

### PlaybackTemporal model

一个 TLA+/TLC（或等价显式状态模型），只需覆盖以下五组高风险 temporal 语义：

1. **Dual Window**：1 ActiveWindow + 0..1 PreparedWindow；两者同时存在且 generation 不同是合法状态。
2. **Generation Admission**：stale = 不再被 owning temporal role 的 admission 接纳，而不是“与某个全局 current generation 不等”。至少保护：prepared generation 可以 prime；retired generation 不能 re-enter；来自未接纳 generation 的 late decode 被拒绝。
3. **Physical Fence**：hard discontinuity promotion 要求成功的 Physical Fence；fence 失败不构成成功 promotion；generation invalidation 不能替代物理切断。
4. **submitted != rendered**：rendered <= submitted 恒成立；submit 本身不得推进 physical completion truth。
5. **EOF / drained / ENDED terminalization**：EOF != TransportDrained != ENDED；且 Physical Fence 在途时，自然 EOF / drain 不得提前发布会销毁完成该 fence 所需的 active temporal state（§18 冻结，由 counterexample 挣得）。

rapid seek / next / stop 与 decode/fence/render 证据的交错是攻击这些语义组的主要向量，模型 trace 应包含此类序列。

### Core negative controls

模型必须抓住以下四个故意注入的 mutation，每个保护一组上述语义：

| Mutation | 保护对象 |
|---|---|
| PromoteWithoutFence | Physical Fence |
| AcceptUnadmittedDecode | Generation admission |
| SingleGlobalGenerationCheck | Dual Window / 禁止全局 current generation |
| EndBeforeRenderDrain | EOF / physical drain |

retired-generation re-enter 的保护属于语义组 2，由 PlaybackTemporal 正常模型不变量承担；对应 mutation 属于 extended evidence。

## 支持证据（non-blocking）

以下继续保留在 `specs/` 并继续运行，但不作为 ACCEPTED 前置条件：

```text
PlaybackOwnership model        resource-lifecycle 假设的 supporting formal exploration
RetiredGenerationStillAdmitted extended admission mutation 证据
ReleaseProviderEarly           provider ordering
MultipleImmediateOwners        ownership sanity
OwnershipCycle                 ownership sanity
KernelAdoptsLifetimeOwnership  historical exploratory mutation
```

`PlaybackOwnership` FAIL 不自动推出本文不能 ACCEPTED，除非它发现本文本身存在明确语义矛盾。不为让它完美映射未来 production ownership 而扩大模型。

## 明确不在形式化范围内

以下问题不进入 Formal Acceptance；若未来成为真实风险，再单独验证：

```text
Window 的最终 immediate lifetime owner
semantic authority holder 是否可以同时 lifetime-own 某个资源
完整 provider dependency graph
任意 N generation 的参数化证明 / TLAPS theorem proof
Temporal × Ownership 联合模型
liveness / fairness
crossfade / gapless
完整 command supersede algebra
RT scheduling / 真实 memory ordering
```

## Implementation entry（ACCEPTED 之后）

```text
ADR ACCEPTED
    ↓
deterministic executable oracle
    ↓
implementation authorization
```

Executable oracle 验证 implementation vocabulary 能否承载本文语义；它不是 architecture decision 成立的前置条件。

---

# 22. ADR 状态机

```text
Design Review（G1-G8 PASS）
        ↓
Core PlaybackTemporal（五组高风险 temporal 语义）
        ↓
Core negative controls（4）
        ↓
ADR-PBK-001 = ACCEPTED
        ↓
corrective refinement of ARCH-003 authority
        ↓
deterministic executable oracle
        ↓
implementation issue separately authorizes production work
```

当前：

```text
STATUS = PROPOSED / FORMAL CORE PASS
DESIGN REVIEW = PASS
CORE TEMPORAL CHECKS = PASS
SUPPORTING FORMAL EVIDENCE = RETAINED / NON-BLOCKING
IMPLEMENTATION AUTHORIZATION = NO
EXECUTABLE ORACLE = NOT STARTED
ARCH-003 AUTHORITY REVISION = NO
```

---

# 23. 不冻结的 Representation

以下留给后续 implementation design：

```text
MusicKernel / TransportKernel Rust API
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
PlayerGain 最终 placement
future crossfade/gapless
ReplayGain
device-handoff policy
WASAPI/CoreAudio/AAudio 具体 fence mechanism
```

这些 representation 不得反过来改变本文已冻结的 authority/lifetime/data-plane semantics。

---

# 24. 对 ARCH-003 的影响

当前 ADR 仍为 PROPOSED，因此现在不修改 `registry.yml` 的 ARCH-003 authority。

Formal Acceptance 必须项通过、ADR 改为 ACCEPTED 后，再把当前较宽泛的 Playback authority corrective-refine 为：

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

这不是重开 Base Kernel K0。
