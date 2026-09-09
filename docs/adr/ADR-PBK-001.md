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

本次只冻结以下原则：

1. **Base Kernel is domain-agnostic.**
2. **Plugin/Fiber identity belongs to composition/lifecycle, not per-payload routing.**
3. **Commands/capability calls ask the system to do something; facts describe what has already been committed.**
4. **Committed facts may fan out to observers/projections, but observers do not rewrite the committed fact.**
5. **Realtime PCM is a direct typed data flow, not a generic Event/Context/Plugin dispatch stream.**
6. **Control-side graph construction/publication and realtime graph execution are separate responsibilities.**
7. **A projection/read model is derived visibility, not semantic authority.**
8. **No hidden global mutable bag may become the common owner of composition, facts, control state and PCM.**

一句话：

> **Kernel 管“谁存在”；Capability/Service 管“怎么执行”；Fact/Event 管“发生了什么”；Realtime Data Plane 管“PCM 怎么流”。**

这四件事禁止再次揉成一套万能机制。

---

# 2. 四个平面

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

Fact 表示已经由其 owning authority / mechanism **提交成立**的事实。

最小语义：

```text
validate / decide
      ↓
authoritative commit
      ↓
Fact
      ↓
publish / fan-out
      ↓
projection / persistence / UI / telemetry / reactions
```

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

一个已提交 Fact 的 observer 失败，不得偷偷改变“这个事实是否已经发生”。如果某个 observer 需要触发新的动作，它必须发起新的 command 或产生新的事实，而不是回写旧 fact。

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

UI、telemetry、history view、diagnostics 可以使用 projection，但不能通过修改 projection 改 runtime truth。

### 本 ADR 不冻结 Event Sourcing

Qianqian 当前**不因为外部系统采用 append-only event log，就自动选择完整 Event Sourcing**。

尚未决定：

```text
事实是否全部 durable
是否存在唯一 append-only log
内存 commit 与磁盘 durability 的关系
replay 是否成为恢复权威
snapshot + events 还是 state + events
```

本阶段只冻结：

```text
command != fact
commit precedes fact publication
projection != authority
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
FactStore
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
publish at an RT-safe boundary

REALTIME SIDE

load current graph/view
        ↓
process audio quantum directly
```

冻结：

> **Realtime callback consumes a pre-bound published graph/view; it does not construct or reconcile one.**

### Lifetime safety

如果 published realtime graph/view 仍引用某个 provider/resource，那么该对象不能提前 final release。

最小顺序：

```text
provider/node withdrawal requested
        ↓
exclude from future graph construction
        ↓
publish replacement graph/view
        ↓
old realtime readers/quanta stop entering old graph
        ↓
old readers/queued references quiesce
        ↓
release old graph references
        ↓
provider/resource final release
```

冻结：

> **Published realtime references must outlive every reader that can still dereference them.**

具体采用：

```text
RCU
epoch
double buffer
Arc snapshot
hazard/lease
stop-the-world handoff
```

不冻结。

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

这些是 Fact Plane 候选。

但 PCM 本身不是“因为它经过 runtime，所以也顺便做成 event”。

冻结：

> **Facts describe meaningful committed observations; hot data remains on the realtime data plane.**

是否存在：

```text
render position
EOF
submitted/rendered counters
physical flush verdict
```

以及谁解释它们，全部由真实 decoder/output 实验重新挣得。

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

> **One semantic fact must have an explicit writer/authority, even if many projections can see it.**

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

允许：

```text
复用已经证明有价值的 bug reproducer
复用测试技术
比较新实验是否重新撞到旧 failure
```

禁止：

```text
为了兼容旧类型而保留新架构不需要的概念
因为旧 TLA 有某个变量就要求 production 也必须有
把旧 executable core 当作实现授权
```

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
G1  四平面边界 adversarial review PASS
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
