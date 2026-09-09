# ADR-PBK-001：播放基础重置——组合、执行、事实与实时数据平面

- **状态**：PROPOSED
- **日期**：2026-09-09
- **作用域**：Qianqian Playback Foundations / ARCH-003
- **保持不变**：Base / Composition Kernel K0

---

# 0. 为什么重置

Qianqian 仍处在足够年轻的阶段。此时最危险的不是 breaking change，而是为了维护已经写下来的名词，继续给一个尚未被真实机制验证的播放模型追加修订层。

因此本 ADR 只描述**当前我们真正愿意承诺的基础边界**。旧的播放状态机、ownership 命名和形式化模型继续保留在 Git 历史、代码和 `specs/playback/*` 中作为实验/反例证据，但它们不再自动构成架构 authority。

冻结：

> **Working tree authority describes the architecture we believe now; Git is the history.**

本次重置不否认此前实验发现的 bug 或工程事实，但重新打开所有尚未经过真实 Audio Runtime 机制挣得的 playback-specific nouns。

当前不再预设：

```text
MusicKernel
TransportKernel
TrackSession
DecodeSession
Active / Prepared
Generation
Dual Window
Physical Fence
完整 seek / next / stop 状态机
```

这些名字可以继续存在于实验代码中，但不得因为“代码已经有了”而反向成为新架构的前提。

---

# 1. 最小宪法

本次只冻结以下原则（本节是唯一 normative 宪法；`AGENTS.md` / `CONTEXT.md` / `overview.md` 只做路由、解释与状态记录，不持有第二份 normative 宪法）：

1. **Base Kernel K0 owns composition existence/reachability/lifecycle and remains domain-agnostic.**
2. **Context/Capability/Plugin/Fiber mechanisms are not payload-routing mechanisms; no hidden global mutable state may become a shared writer.**
3. **Command is intent. A Fact is truth established by its designated semantic authority. Each semantic fact type has exactly one designated authority at a time.**
4. **Projection is derived visibility. It cannot write authority state, and a control decision must not use a Projection as its correctness authority.**
5. **Realtime audio payload flows through pre-bound realtime execution state. Per-quantum PCM must not re-enter Context resolution, generic events, plugin dispatch, Reconcile, or filesystem/network/control machinery.**
6. **Any resource that realtime execution may still dereference must remain valid until no realtime execution or queued reference can dereference it.**
7. **Previous Playback implementation/spec/model artifacts are evidence only. Failure witnesses may be reused; representations and nouns are not inherited.**

一句话：

> **Kernel 管“谁存在”；Capability/Service 管“怎么执行”；Fact 管“发生了什么”；Realtime Data Plane 管“PCM 怎么流”。**

这四件事禁止再次揉成一套万能机制。

---

# 2. 四个关注面（reasoning lenses）

以下四分法是四个 **reasoning lenses / concern boundaries**，用于区分问题归属。它**不**宣称运行时由恰好四个具体子系统组成；某个 lens 是否挣得独立 runtime mechanism，由真实实验决定。

## 2.1 Composition Plane

Composition Plane 由现有 Base Kernel K0 支撑：

```text
Context
Capability
Fiber
Effect
Reconcile
```

它负责：

```text
plugin instance existence
capability reachability
provider / consumer dependency
Fiber lifecycle
effect ownership
provider withdrawal
desired composition -> running composition
```

它不负责：

```text
PCM block routing
audio callback scheduling
track/seek semantics
player state meaning
fact persistence
UI state storage
```

冻结：

> **Composition Kernel controls existence, reachability and lifecycle; it does not transport application payloads.**

K0 目前没有因为本次重置而新增 `Event`、`AudioGraph`、`Session`、`PCM` 等 generic primitive。

如果未来事实系统需要 Event Service，先作为普通 capability/plugin/service 挣得；不得因为外部系统这样设计就自动升级为 K0 primitive。

---

## 2.2 Execution / Control Plane

这是“要求系统做事”的路径。

> 当前它是一个 **constraint-oriented lens**，不是一个新的 K0 subsystem：本文不因此引入 command processor / workflow runtime 等新 generic primitive。

典型形状：

```text
User/UI/Automation
      |
    Command
      v
Domain/controller/workflow
      |
Capability / Service call
      v
Mechanism / authority
```

这里允许未来出现：

```text
command processor
workflow
middleware / waterfall-like interception
retry/cancellation policy
parameter/control messages
```

但这些是否成为独立 runtime primitive **尚未冻结**。

冻结：

> **Command is intent, not fact.**

> **A control interceptor may influence execution, but it must not masquerade as post-commit fact delivery.**

不要把一种 `emit()` 同时用于：

```text
command
middleware
committed fact
PCM block
```

---

## 2.3 Fact Plane

Fact 表示已经由其 designated semantic authority **确立成立**的事实。

最小语义：

```text
validate / decide
      ↓
semantic commit（由 designated authority 确立）
      ↓
Fact
      ↓
Fact publication / fan-out
      ↓
projection / persistence / UI / telemetry / reactions
```

### commit 的精确定义

> **Semantic commit = the producing semantic authority considers the fact established, according to that fact's contract.**

Commit 是**语义确立**，不是 persistence 术语。semantic commit **不**天然意味着：

```text
ACID
durable write
fsync
database transaction
device completion
process-crash durability
```

除非某个具体 fact 的 contract 未来另外要求。

冻结：

> **commit first -> publish fact**

而不是：

```text
listener A mutates event
    ↓
listener B mutates again
    ↓
listener C
    ↓
finally decide what happened
```

一个已提交 Fact 的 observer 失败，不得偷偷改变“这个事实是否已经发生”。如果某个 observer 需要触发新的动作，它必须发起新的 command，或使相应 fact type 的 designated authority 确立新事实，而不是回写旧 fact，也不能自行另立名目发布与既有 fact type 语义重复的“新事实”。

### Fact type-level authority

> **For each semantic fact type, there is exactly one designated semantic authority at a time.**

> **Mechanism observations/evidence must not publish another authority's semantic fact — there is no route by which a mechanism publishes a fact type on its authority's behalf without the authority's own semantic decision.**

Designation 是显式的 architecture-level contract，按 fact type 记录：任何 component 都不因观察了 evidence、发布了该 fact type、或在运行时自封而成为该 fact type 的 designated authority；re-designation 必须是一次显式的完整交接，不得出现双 authority 窗口。语义上重复另一 authority 既有 fact type 的“新类型”视为同一 fact type，改名不产生新 authority。

两层区分（方向冻结，具体类型不冻结）：

```text
Mechanism Evidence
    raw / observer-level evidence，例如未来可能的
    DeviceObservedSilence / DecoderObservedEof / RenderPositionObserved

Semantic Fact
    由 designated authority 确立的事实，例如未来可能的
    PlaybackStopped / PlaybackEnded / TrackCompleted
```

方向：

```text
mechanism observation
    ↓ validation / semantic decision
designated authority
    ↓
semantic Fact publication
```

一个 mechanism provider 观察到 raw evidence，不等于它可以因为该观察就直接发布 semantic fact；除非它本身就是该 fact type 的 designated authority。本文**不**现在命名任何 playback fact type 的 authority（Playback semantic authorities 仍 OPEN），只冻结上述 authority contract。也不冻结 `EvidenceEvent` / `FactEvent` / `TransportEvidence` 等具体类型。

### Fact 不是必然持久化事件

```text
Fact != necessarily persisted event
Fact != necessarily append-only log entry
Fact != necessarily replayable
Fact != necessarily durable
```

### publish 的两种含义

`publish` 一词必须带对象使用，禁止裸用：

```text
Fact publication           事实发布（Fact Plane）
Realtime-view publication  实时图/视图发布（Realtime Data Plane，§6）
```

（Graph publication 即“对 graph 的 Realtime-view publication”；未来任何其它 publication-like 机制同样必须命名对象与所属 lens，不得共享裸动词。）

这是两个不同机制，不得共用一个不带宾语的动词让 reader 猜。

### Projection

Projection / materialized read model 的语义是：

```text
previous view
    +
committed facts / authoritative snapshots
    ↓
next view
```

冻结：

> **Projection is derived visibility, not authority.**
>
> **A control decision must not use a Projection as its correctness authority (its correctness basis).**

Projection 可以用于：

```text
UI
telemetry
diagnostics
history
display
non-authoritative convenience
```

但以下决策必须基于 authority state、authoritative capability result 或 validated fact/evidence（validated 指按该 fact/evidence 的 contract 校验，即由其 designated authority 校验，而不是 controller 自行任意解释），而不是一个可能 stale 的 projection：

```text
control transition
resource lifecycle correctness
semantic decision
```

决定“是否需要”某个 control action/transition（skip / 幂等检查）本身也属于上述决策，必须查询 authority state，不得以 projection 为依据。

本文不禁止 control code 读取 projection；禁止的是 **projection 成为 correctness basis**。

### 本 ADR 不冻结 Event Sourcing

Qianqian 当前**不因为外部系统采用 append-only event log，就自动选择完整 Event Sourcing**。

尚未决定（全部 OPEN）：

```text
完整 Event Sourcing / CQRS
事实是否全部 durable
是否存在唯一 append-only global fact log
内存 commit 与磁盘 durability 的关系
replay 是否成为恢复权威
snapshot + events 还是 state + events
```

不新增 `EventStore` / `FactStore` / `SessionEvent` / `FactLog` 作为 K0 或 global primitive。

本阶段只冻结：

```text
command != fact
commit precedes fact publication
per-fact-type single designated authority
projection != authority（含 read-side firewall）
fact publication 与 realtime-view publication 是不同机制
```

---

## 2.4 Realtime Data Plane

Realtime Data Plane 负责高频、连续、带时间约束的数据流，例如 PCM。

目标形状：

```text
Media/Decoder
     |
   PCM
     v
Audio processing graph
     |
   PCM
     v
Audio device/output
```

冻结：

> **PCM is a direct typed realtime data flow.**

每个 PCM block/callback 禁止经过：

```text
Context lookup
Capability resolution
Fiber Reconcile
generic EventBus fan-out
plugin registry traversal
UI/runtime round trip
filesystem/network I/O
unbounded allocation/blocking
```

Plugin/Fiber 可以决定某个长期能力或 provider 是否存在；但已经建立好的 realtime 数据边必须直接执行，不得每块 PCM 都重新“走插件系统”。

---

# 3. Plugin 到底是什么

本次重置后，“Everything is a Plugin” 采用更严格的解释：

> **Plugin 是长期能力进入统一 composition/lifecycle protocol 的方式，不是宇宙里的数据流原子。**

一个 Plugin/Fiber 可以：

```text
provide service/capability
require other capabilities
register control hooks
observe committed facts
own resources/effects
provide factories or realtime graph participants
```

但是：

```text
Plugin A -> Plugin B -> Plugin C
```

不自动等价于任何业务/PCM pipeline。

是否把：

```text
Decoder
Gain
EQ
SRC
Mixer
Analyzer
Recorder
AudioOutput
```

分别做成独立 Plugin，**本 ADR 暂不冻结**。

真正要先问：

```text
它是否有独立 lifetime？
是否提供/要求稳定 capability？
是否需要独立 replacement/withdrawal？
是否拥有长期资源/状态？
它是否只是某个 provider 内部的 realtime node？
拆出来的配置/认知成本是否值得？
```

所以：

```text
AudioNode != automatically Plugin
Plugin != automatically AudioNode
PcmBlock != Plugin
Buffer != Plugin
Fact != Plugin
Command != Plugin
```

---

# 4. Capability / Service 是执行原子

跨 Plugin seam，consumer 依赖 capability/service definition，而不是具体 provider 类型。

```text
Capability / Service Definition
              ^
         +----+----+
         |         |
      Provider   Consumer
```

一个 command 最终可能通过 capability/service 完成真实工作。

例如未来可能有：

```text
MediaOpen
Decode
AudioDevice
PlaybackControl
FactStore        (illustrative only; any persistence role remains OPEN per §2.3)
Presentation
```

但这些 capability 名称和边界都必须通过后续实验挣得；此处仅冻结 capability 是**执行/可达性契约**，不是 payload bus。

---

# 5. Control Graph 与 Realtime Graph

我们明确接受系统里可能同时存在两种完全不同的图。

## Composition / dependency graph

```text
who exists
who requires whom
who provides what
who must withdraw before whom
```

由 Composition Plane 管。

## Realtime processing graph

```text
which realtime node runs next
where PCM branches/merges
which pre-bound object/function handles the block
```

由 Audio Runtime / realtime graph mechanism 管。

两张图可以共享某些 provider/node 实例，但边的语义不同。

冻结：

> **Dependency topology != realtime processing topology.**

因此 realtime processing order 不得来自：

```text
Fiber mount order
registration order
HashMap iteration order
capability discovery order
```

---

# 6. Graph Publication 是两个世界的桥

真实 Audio Runtime 最关键的边界，不是“每个 node 是否 Plugin”，而是：

```text
CONTROL SIDE

composition / config / parameters
        ↓
build + validate next realtime graph/view
        ↓
Realtime-view publication at an RT-safe boundary

REALTIME SIDE

load current graph/view
        ↓
process audio quantum directly
```

冻结：

> **Realtime execution consumes a pre-bound published graph/view; it does not construct or reconcile one.**

（措辞刻意使用 realtime execution / quantum / task / reader，不预设执行模型必须是 callback。callback / blocking push / pull / worker / hybrid 全部继续 OPEN。）

### 最小 publication correctness contract

实现机制不选，但以下最低语义已 earned：

> **A realtime reader observes one coherent published realtime view.**

publication 从 N 到 N+1 时，reader 看到的是 N 或 N+1，不得是 “half N + half N+1”。

representation 继续 OPEN，全部不选：

```text
ArcSwap
RCU
epoch
double-buffer
atomic pointer
lease
hazard
```

### Lifetime safety

> **§1 宪法第 6 条适用：Any resource that realtime execution may still dereference must remain valid until no realtime execution or queued reference can dereference it.**

（资源层面的 invariant；**不**冻结 “Provider Fiber lifetime == RT resource lifetime”。）

最小顺序：

```text
provider/node withdrawal requested
        ↓
exclude from future graph construction
        ↓
Realtime-view publication of replacement graph/view
        ↓
old realtime readers/quanta stop entering old graph
        ↓
old readers/queued references quiesce
        ↓
release old graph references
        ↓
provider/resource final release
```

继续 OPEN（不由本 invariant 决定）：

```text
Provider Fiber 本身是否保持 alive
RT view 是否持有 lease
state slab 是否 outlive provider
Arc / epoch / RCU / hazard pointer / refcount / callback fence
reader-quiescence 的具体机制
```

---

# 7. Parameter update != topology update

例如：

```text
volume 0.6 -> 0.7
EQ band gain change
limiter threshold change
```

通常属于 control/parameter update，不应自动触发 Plugin unload/reload 或完整 Reconcile。

而：

```text
insert/remove a durable node
replace output mechanism
replace decoder provider
change graph branch/merge topology
```

可能需要新的 composition / graph publication transaction。

具体分界由后续 Audio Runtime 实验确定。

冻结：

> **A cheap realtime-safe parameter update must not be forced through heavyweight composition mutation merely because the owner is a Plugin.**

---

# 8. Fact 与 realtime evidence

Realtime mechanism 可以产生低频 typed evidence，例如未来可能有：

```text
DeviceLost
UnderrunObserved
GraphPublished
DecodeFailed
OutputStopped
```

这些首先是 **Mechanism Evidence 候选**（§2.3）；它们是否、由谁、以何种 fact type 成为 Semantic Fact，受 §2.3 的 fact type-level authority contract 约束，由真实实验挣得。

但 PCM 本身不是“因为它经过 runtime，所以也顺便做成 event”。

冻结：

> **Facts describe meaningful established observations; hot data remains on the realtime data plane.**

是否存在：

```text
render position
EOF
submitted/rendered counters
physical flush verdict
```

以及谁解释它们、谁是哪个 fact type 的 designated authority，全部由真实 decoder/output 实验重新挣得。

---

# 9. 全局状态

禁止重新引入：

```text
GlobalPlayerState
Arc<Mutex<Everything>>
MutableAppState
```

作为 Composition、Control、Fact、Realtime 四个平面的共同 writer。

冻结：

> **Each semantic fact type has exactly one designated semantic authority at a time, even if many projections can see the fact.**

（这是 per-fact-type contract，不是要求一个全局 `FactAuthority` / `FactKernel` / `CentralEventRouter`。）

但本 ADR 不提前命名具体 playback authorities。

---

# 10. 当前明确未决定的 Playback 语义

以下全部重新开放：

```text
MusicKernel 是否存在
TransportKernel 是否存在
TrackSession / DecodeSession 是否是正确 lifetime unit
seek 是否需要 dual decoder cursor
Active / Prepared 是否存在
Generation 是否必要
staleness 如何表达
physical flush/fence contract
submitted vs rendered 的最终定义
EOF / drained / ended 的状态机
playlist / repeat / shuffle authority
PlayerSnapshot 结构
push vs pull vs hybrid audio graph
ring buffer / queue
thread count / worker model
PCM canonical format
AudioNode trait/API
AudioGraph representation
Decoder provider granularity
Processing-node Plugin granularity
AudioOutput capability shape
完整 Event Sourcing 与否
Fact persistence / durability / replay
Fact publication 的 fan-out 传输机制
Realtime-view publication 的 representation（ArcSwap / RCU / epoch / double-buffer / atomic pointer / lease / hazard）
reader-quiescence 的具体机制
Provider Fiber lifetime 与 RT resource lifetime 的绑定关系
realtime 执行模型细节（callback / blocking push / pull / worker / hybrid 的选择）
```
这些问题不得通过引用旧代码、旧 TLA、旧 ADR 文案直接关闭。

它们必须由当前实验、真实机制和新的 adversarial evidence 重新挣得。

---

# 11. 旧 playback code/spec 的地位

当前仓库中已经存在：

```text
qianqian-core::music
qianqian-core::transport
playback_temporal_traces
specs/playback/*
```

这些现在统一定义为：

```text
EXPERIMENTAL / EXECUTABLE EVIDENCE
NOT ARCHITECTURE AUTHORITY
NOT COMPATIBILITY CONTRACT
```

允许（可以继承）：

```text
复用已经证明有价值的 bug reproducer / failure witness
复用测试技术 / negative-control 方法 / verifier runner
复用具体 counterexample
复用已观察到的 hardware/mechanism 事实
比较新实验是否重新撞到旧 failure
```

禁止（不自动继承）：

```text
为了兼容旧类型而保留新架构不需要的概念
因为旧 TLA 有某个变量就要求 production 也必须有
把旧 executable core 当作实现授权
type name / state name / authority split / module boundary 自动延续
generation / window / fence 的旧 representation 自动延续
```

一句话原则：

> **Preserve the bug, not necessarily the old solution.**（保留 bug witness，不必然保留旧解法。）

如果新实验再次独立挣得某个旧概念，可以重新引入；名字也不必相同。

---

# 12. 研究/实现顺序

新的顺序是从更基础的事实开始，而不是先建播放器状态机。

## Phase A — Composition runtime reality

确认 K0：

```text
Context / Capability / Fiber / Effect / Reconcile
```

只解决“谁存在、怎么依赖、怎么退出”。

## Phase B — Minimal PCM contract

用最小实验挣得：

```text
PcmBlock/view 最少需要哪些字段
format / frames / time/provenance 是否需要进入最小 contract
push / pull / hybrid 哪个更自然
```

## Phase C — Direct-flow graph

先证明：

```text
Source -> one processing stage -> Sink
```

运行时 hot path 不进入 Context/Reconcile/EventBus。

## Phase D — Graph publication / replacement

证明：

```text
Graph N
    ↓
publish Graph N+1
    ↓
old reader overlaps
    ↓
no use-after-release
```

这是目前最明确值得形式化/并发压力测试的边界。

## Phase E — Real decoder / real output

分别接触真实：

```text
FFmpeg
platform audio output
```

让真实 cursor、buffer、callback、device lifetime 决定哪些 playback nouns 真正必要。

## Phase F — Playback semantics

只有到这里才重新讨论：

```text
seek
stop
next
track/session
EOF/ended
buffering
playlist semantics
```

---

# 13. Formalization policy

不再把旧 PlaybackTemporal 当作新 architecture acceptance gate。

Formalization 仍遵循：

> **先发现具体 state/interleaving collision，再建立最小模型攻击它。**

目前最明确的新候选是：

```text
old realtime graph references provider A
A withdrawal begins
new graph excludes A
old RT reader still uses A
A final release
```

如果实现/测试证明这个风险真实存在，再建立窄模型和类似：

```text
ReleaseBeforeReadersQuiesce
```

的 negative control。

不要建一个包含完整播放器、所有 Plugin、所有 PCM node 的“大一统 TLA 模型”。

---

# 14. Acceptance gates

本 ADR 从 PROPOSED 变成 ACCEPTED 前至少需要：

```text
G1  四关注面（reasoning lenses）边界 adversarial review PASS
G2  K0 domain firewall review PASS
G3  command vs fact vs hot-data distinction review PASS
G4  direct realtime data-flow executable experiment PASS
G5  graph publication / lifetime overlap executable evidence PASS
G6  若 G5 暴露真实 state collision，则对应最小 formal negative control PASS
G7  fresh-context architecture review PASS
```

不要求在 ACCEPTED 前先设计完整播放器语义。

---

# 15. 当前状态

```text
Base Kernel K0                      IMPLEMENTED / CURRENT
ADR-PBK-001 playback foundations    PROPOSED / REOPENED
old playback executable core        EVIDENCE ONLY
old playback formal models          EVIDENCE ONLY
production playback semantics       NOT AUTHORIZED
real Audio Runtime experiments      NEXT, after this reset is reviewed
```

当前最重要的纪律：

> **先把 composition、execution、fact、realtime data flow 四个世界分清，再让真实音频机制决定播放器应该长什么样。**
