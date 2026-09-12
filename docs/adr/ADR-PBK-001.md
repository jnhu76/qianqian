# ADR-PBK-001：播放基础重置——组合、执行、事实与实时数据平面

- **状态**：ACCEPTED
- **接受说明**：Accepted after fresh-context adversarial review and fact-authority-scope corrective.
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
3. **Command is intent. A Fact is truth established by its designated semantic authority. For each semantic fact kind and its semantic subject scope, exactly one designated semantic authority may establish that truth at a time.**
4. **Projection is derived visibility. It cannot write authority state, and a control decision must not use a Projection as its correctness authority.**
5. **Realtime audio payload flows through pre-bound realtime execution state. Per-quantum PCM must not re-enter Context resolution, generic events, plugin dispatch, Reconcile, or filesystem/network/control machinery.**
6. **Any resource that realtime execution may still dereference must remain valid until no realtime execution or queued reference can dereference it.**
7. **Previous Playback implementation/spec/model artifacts are evidence only. Failure witnesses may be reused; representations and nouns are not inherited.**

（第 6 条的 normative publication/reclamation 语义由 §6 P1–P5 展开；本节保持极简宪法，不复制协议本体。）

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

一个已提交 Fact 的 observer 失败，不得偷偷改变“这个事实是否已经发生”。如果某个 observer 需要触发新的动作，它必须发起新的 command，或使相应事实的 designated authority 确立新事实，而不是回写旧 fact，也不能自行另立名目发布与既有事实语义重复的“新事实”（何谓语义重复由下方 authority identity contract 裁决）。

### Fact authority identity（fact kind + subject scope）

> **A semantic fact's authority identity is determined by both its fact kind and its semantic subject scope.**

> **For each (fact kind, semantic subject scope), there is exactly one designated semantic authority at a time.**

> **Mechanism observations/evidence must not publish another authority's semantic fact — there is no route by which a mechanism publishes a fact on its authority's behalf without the authority's own semantic decision.**

三个语义概念（architecture semantics，不是 runtime representation——本 ADR 不因此引入 scope 对象、namespace、key、registry 或任何新机制）：

```text
fact kind               事实的种类，如 DeviceLost；kind 本身不携带它描述的对象
semantic subject scope  一条断言所描述的语义对象/范围——可能是 device、
                        decoder instance、media/session、graph publication
                        domain、playback domain、resource 或 global singleton，
                        由对应 fact contract 定义
authority identity      由 (fact kind, subject scope) 共同决定
```

> **Each semantic fact contract must define what subject scope makes two assertions refer to the same authoritative truth.**

subject scope 的具体表示（UUID / integer key / provider id / Fiber id / generation / namespace string / hierarchical path 等）全部不冻结，由未来实验挣得。

由此产生两个方向相反的约束：

- 同一个 fact kind 可以在不重叠的 subject scope 上拥有各自独立的 designated authority。“每个 fact kind 一个全局 authority”是误读：本 contract 不要求、不暗示 `GlobalDeviceAuthority` / `FactAuthority` / `FactKernel` / `CentralEventRouter` 这类全局单 writer。
- 同一个 (fact kind, subject scope) 在同一时刻只能有一个 designated semantic authority。Designation 是显式的 architecture-level contract，按 (fact kind, subject scope) 记录：任何 component 都不因观察了 evidence、发布了该 fact、或在运行时自封而成为 designated authority；re-designation 必须是一次显式的完整交接，不得出现双 authority 窗口，且交接的范围就是该 (fact kind, subject scope) 本身——不影响其它 scope 上独立 authority 的合法性。

权威唯一性不得通过改名或切分绕过：

> **Renaming, wrapping, or arbitrarily subdividing a fact kind or subject scope does not create a new authority identity when the assertions can establish the same semantic truth about the same semantic subject.**

> **Two fact definitions must not be used to evade authority uniqueness when they overlap on, or independently establish, the same authoritative proposition for the same subject scope.**

最小示例（illustrative only——具体 Playback fact authorities 仍 OPEN，以下名字都不是本文冻结的 production authority）：

```text
DeviceLost(device A) 的 authority = output authority A
DeviceLost(device B) 的 authority = output authority B
```

两个 device 是不同 subject scope，允许两个独立 designated authority 并存。但 `DeviceLost(device A)` 同一时刻只能有一个 writer：authority A 与 authority B 不得同时确立它；未经该 scope 的显式完整交接，authority B 也不得发布 `DeviceLost(device A)`。

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

一个 mechanism provider 观察到 raw evidence，不等于它可以因为该观察就直接发布 semantic fact；除非它本身就是该 (fact kind, subject scope) 的 designated authority。本文**不**现在命名任何 playback fact 的 authority（Playback semantic authorities 仍 OPEN），只冻结上述 authority contract。也不冻结 `EvidenceEvent` / `FactEvent` / `TransportEvidence` 等具体类型。

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
single designated authority per (fact kind, subject scope)
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

### Publication / Reclamation 语义协议（P1–P5，normative）

`specs/realtime-publication/` 的形式化证据（TLC 穷举 + M1–M4 mutation 反证 + 可达性探针）已在该模型的显式抽象下证明：publication / reader quiescence / resource reclamation 的交错碰撞真实存在。由此挣得的语义协议在此冻结为 normative contract；其形式化推导与机制比较见本节末尾的 evidence 分层，语义定义只存在于本文。

**P1 — Coherent Publication**

> **一次 realtime-view acquisition 必须观察一个整体一致的 published realtime view。**

（即本 ADR 原冻结的 “A realtime reader observes one coherent published realtime view”。）

publication 从 N → N+1 时：

```text
reader may observe:
    N
or
    N+1

reader must never observe:
    topology(N) + resources(N+1)
    identity(N+1) + resources(N)
    any other logically split publication
```

（即原 “reader 看到的是 N 或 N+1，不得是 half N + half N+1” 的精确形式。）

冻结的是 **coherent acquisition / publication 语义**，不是表示：单一原子不可变 view 只是可能的实现之一；版本校验式整体一致获取等其它实现同样允许，只要满足 P1。被禁止的是无协调的逐分量独立发布。

**P2 — Retired-view Closure**

> **Realtime-view publication of N+1 retires N and closes N to future acquisition, but retirement must not invalidate active executions or queued references that already legitimately hold N.**

```text
publish N+1
    ↓
N becomes retired
    ↓
new acquisition of N forbidden

BUT

existing active holder of N
existing queued holder of N
    ↓
may still legally finish using N
```

`current == N+1` 不意味着 `N has no readers`：仍持有 retired view 的旧读者（active 执行或 queued reference）是合法状态，不是缺陷。

**P3 — Quiescence Before Reclamation Across All Generations**

> **一个资源获得 reclamation eligibility，当且仅当不存在任何 active realtime execution 或 queued reference 仍可能通过任何 generation 的 published / retired realtime view 解引用它。**

（“当且仅当” 定义的是 eligibility 的**语义谓词**，不是 recognition 时限：控制侧何时完成认证由机制与 P5 的 progress 前提决定，允许批量/延迟式认证。）

认证范围必须是 **ANY generation**——不得只检查：

```text
current view
latest retired view
latest generation
```

必须覆盖所有仍可被合法 reader 持有的 generation：

```text
N
N+1
N+2
...
```

并且：

> **Publication itself does not grant reclamation permission.**

**P4 — Retirement != Reclaimability != Release**

```text
Live
  ↓ replacement publication
Retired
  ↓ quiescence satisfied / certified
Reclaimable
  ↓ disposal
Released
```

冻结：

> **Retirement does not imply reclaimability.**

> **Reclaimability does not imply that physical release has already occurred.**

> **Publication is an ordering event, not a reclamation certificate.**

这是**语义状态区分**，不是 representation requirement：生产代码不因此必须实现 `enum ResourceState { Live, Retired, Reclaimable, Released }`，也不因此必须维护任何对应状态字段。形式模型中的 `certified` 同理——它只是 proof/model abstraction，不要求真实实现维护一个 `certified` boolean 或任何对应 runtime 机制。

**P5 — Conditional Reclamation Progress**

> **在 realtime readers 最终离开其持有 view，且 reclamation mechanism 在 quiescence 成立后最终推进 reclamation 的前提下，retired resource 最终应能获得 reclamation eligibility。**

显式前提（与形式模型的 fairness 假设一一对应）：

```text
WF(ReaderExit)        realtime reader 不会永远停留在同一视图内
WF(MarkReclaimable)   控制侧在 quiescence 成立后最终推进 reclamation
```

本条不扩大为：

```text
all retired resources are eventually physically destroyed
all memory is eventually freed
release always happens
```

形式化证明的是 **eventually reclaimable**，不是 **eventually physically released**。

### Semantic contract != representation contract

**P1–P5 are semantic requirements, not representation requirements.**

以下全部继续 OPEN（不因 P1–P5 而冻结）：

```text
ArcSwap / atomic immutable view / atomic pointer
epoch / RCU
hazard pointer
reader count / lease
double-buffer
其它 publication mechanism

queued-reference concrete representation
memory-order details
exact view layout
final-disposal executor
deferred-disposal mechanism
provider Fiber lifetime 与 RT resource lifetime 的具体绑定
```

当前工程倾向记录在 decision record（`docs/architecture/realtime-publication-lifetime-decision.md` §14）中，是 Phase D 可执行实验的默认起点；它不是 architecture decision，也不是本 ADR 的冻结内容。

### Lifetime safety

> **§1 宪法第 6 条适用：Any resource that realtime execution may still dereference must remain valid until no realtime execution or queued reference can dereference it.**

（资源层面的 invariant；**不**冻结 “Provider Fiber lifetime == RT resource lifetime”。）

最小顺序（resource 级语义；不包含 Provider Fiber 自身的退出时机）：

```text
withdrawal / replacement requested
        ↓
exclude old participant/resource from future published realtime views
        ↓
Realtime-view publication of replacement view
        ↓
old realtime executions stop newly entering the retired view
        ↓
old realtime executions / queued references quiesce
        ↓
release old realtime-view references
        ↓
resources no longer dereferenceable by realtime execution
become eligible for release
```

（顺序终点是 resource 相对 realtime 执行获得 release 资格；**不是** "provider final release"。Provider Fiber 何时退出、是否与 resource release 同步，不由本 invariant 决定。顺序中每一步的语义由 P1–P5 精确约束：闭门 = P2，quiescence 认证范围 = P3，状态与资格区分 = P4。）

继续 OPEN（不由本 invariant 决定）：

```text
Provider Fiber completion 相对 resource release 的时机
RT view / resource 是否需要 lease（lease 语义本身 OPEN，不在此设计）
state slab 的 lifetime
Arc / epoch / RCU / hazard pointer / refcount / callback fence
reader-quiescence 的具体机制
```

Publication/reclamation 证据分层：`specs/realtime-publication/` 是可执行形式化证据（模型、mutation 负控制、可达性探针、运行数据）；`docs/architecture/realtime-publication-lifetime-decision.md` 是 P1–P5 的形式化推导、候选机制比较与工程倾向（evidence / engineering record，非 normative authority）。representation 仍全部 OPEN。

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

这些首先是 **Mechanism Evidence 候选**（§2.3）；它们是否、由谁、以何种 fact kind 成为 Semantic Fact，受 §2.3 的 fact authority identity contract 约束，由真实实验挣得。

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

以及谁解释它们、谁是哪个事实的 designated authority，全部由真实 decoder/output 实验重新挣得。

---

# 9. 全局状态

禁止重新引入：

```text
GlobalPlayerState
Arc<Mutex<Everything>>
MutableAppState
```

作为 Composition、Control、Fact、Realtime 四个平面的共同 writer。

Fact 一侧的全局状态规则不再在此复述第二份定义，遵循 §2.3 的 fact-authority identity contract：authority identity 由 fact kind 与 semantic subject scope 共同决定；同一 (fact kind, subject scope) 同一时刻至多一个 designated semantic authority——即使多个 projection 都能看到该事实；同一 fact kind 的不同 subject scope 可以拥有独立 authority，这不构成全局共享 writer，也不要求一个全局 `FactAuthority` / `FactKernel` / `CentralEventRouter`。

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
qianqian-audio-api::music
qianqian-audio-api::transport
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

## Phase D — Graph publication / replacement（mechanism validation）

语义层的 collision 问题已经关闭：`specs/realtime-publication/` 证明 publication 与旧 reader overlap 的交错真实存在，其语义结论已冻结为 §6 P1–P5。Phase D 不再回答 “publication/lifetime collision 是否存在”，而是回答：

> **在真实 Audio Runtime 执行模型中，验证候选 implementation mechanisms 是否满足已冻结的 P1–P5，并取得足够的 realtime / lifetime / disposal 工程事实，以裁决具体机制。**

必须取得的 evidence 至少包括：

```text
1.  RT acquire/release worst-case cost
2.  queued reference 的真实生命周期
3.  final ownership/drop 发生在哪个线程
4.  final drop 是否触发 destructor cascade
5.  是否需要 deferred disposal
6.  N -> N+1 -> N+2 连续 publication / multi-generation overlap
7.  stalled reader 行为
8.  control-side reclamation waiting / polling
9.  是否存在 RT/control coupling
```

（原 Phase D 的可执行目标——Graph N → publish Graph N+1 → old reader overlaps → no use-after-release——保留为机制验证的基础场景，其语义由 P1/P3 表达。）

Phase D validates mechanisms against P1–P5；Phase D does not re-litigate whether P1–P5 are required.

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

Post-reset precedent：**realtime publication/lifetime 成为重置后第一个 formal target**。本节此前点名的候选交错——

```text
old realtime graph references provider A
A withdrawal begins
new graph excludes A
old RT reader still uses A
A final release
```

——已由 `specs/realtime-publication/` 在模型显式抽象下证实为真实 collision（TLC 穷举 + mutation 反证；实现层确认仍随 §12 Phase D / §14 G5），其语义结论已冻结为 §6 P1–P5。该次形式化挣得：

```text
publication != reclamation permission
coherent acquisition is required
retired-view closure is required
quiescence must cover all reachable generations
reclamation progress depends on explicit progress assumptions
```

同一 precedent 也确立了形式化证据的边界：

> **Formal evidence constrains semantics; it does not select a concrete Rust/C++ implementation mechanism.**

机制裁决属于 §12 Phase D 的可执行实验，不属于形式化模型。

不要建一个包含完整播放器、所有 Plugin、所有 PCM node 的“大一统 TLA 模型”。

---

# 14. Acceptance gates

本 ADR 于 2026-09-09 经 fresh-context adversarial review（含 corrective）后
ACCEPTED（PR #87）。gate 语义按实际接受依据与现状如实记录：

```text
Review gates（接受时已满足，依据 = PR #87 多轮 fresh-context adversarial review）：
G1  四关注面（reasoning lenses）边界 adversarial review PASS
G2  K0 domain firewall review PASS
G3  command vs fact vs hot-data distinction review PASS
G7  fresh-context architecture review PASS

Executable evidence gates（post-acceptance research ladder 项，对应 §12 Phase C/D；
状态随证据交付如实更新——evidence 状态不是对本文语义的修订）：
G4  direct realtime data-flow executable experiment        DELIVERED
    （Phase C evidence：docs/architecture/direct-pcm-flow.md，PR #96）
G5  graph publication / lifetime overlap executable evidence DELIVERED
    （mechanism evidence：docs/architecture/realtime-view-publication.md，PR #97，
    在真实 Rust 机制上验证 §6 P1–P5）
G6  若 G5 暴露真实 state collision，则对应最小 formal negative control
    —— 本 gate 的触发条件（G5 实现层碰撞）的最终人工裁决仍 OPEN；独立于本 gate，
    §13 点名候选交错的语义级模型与 negative control（M1 ReleaseBeforeQuiesce
    等）已由 specs/realtime-publication/ 提供（见 §6 evidence 引用）；
    G5 机制证据中的负控制（twin kill 等）记录于 realtime-view-publication.md
```

接受依据是 review gates；executable evidence gates 不是追溯性接受前提，而是
§12 ladder 相应步骤与任何 realtime 机制冻结决策的推进 gate——它们约束"后续
冻结机制需要什么证据"，不改变本 ADR 的 ACCEPTED 状态。不要求在设计完整播放器
语义之后才能接受本 ADR。

---

# 15. 当前状态

```text
Base Kernel K0                      IMPLEMENTED / CURRENT
ADR-PBK-001 playback foundations    ACCEPTED
P1–P5 formal evidence               DELIVERED (specs/realtime-publication/)
minimal PCM contract evidence       DELIVERED (Phase B, pcm-contract-a0.md)
direct-flow evidence                DELIVERED (Phase C, direct-pcm-flow.md)
realtime-view publication/reclamation
mechanism evidence                  DELIVERED (validates §6 P1–P5 on a real
                                    mechanism; realtime-view-publication.md)
Realtime Runtime responsibility     EARNED (Issue #94 closed; semantic roles
                                    defined in §16; production seam 待审)
old playback executable core        EVIDENCE ONLY
old playback formal models          EVIDENCE ONLY
production playback semantics       NOT AUTHORIZED (§10 remains OPEN)
next                                依 §12 ladder 继续:机制裁决 / 真实
                                    decoder / output;Playback 语义最后
```

当前最重要的纪律：

> **先把 composition、execution、fact、realtime data flow 四个世界分清，再让真实音频机制决定播放器应该长什么样。**

---

# 16. Vocabulary / Role Definitions（vocabulary 收口）

本节只收口 vocabulary 与 architecture role 定义。它**不新增 invariant、不重写宪法、不冻结 representation**：与 §1–§2、§6 的 normative 契约冲突时，以其为准；本节不冻结任何 crate 映射、Plugin 粒度或机制表示。其它文档只引用本节定义，不得另立第二份 normative 词汇表。

> **Architecture role != crate name.** 本节定义的都是语义角色；任何 crate 名（如 `qianqian-app`、`qianqian-realtime`）都不是某角色已被正确物理实现的证据，crate 物理归属另行审计。

## 16.1 Composition 侧

**Base Composition Kernel (K0)** — canonical noun。domain-agnostic 的 composition/lifecycle kernel。别名 `Base Kernel` / `Composition Kernel` / `K0` 指同一 referent，行文首选全称。它拥有：

```text
existence / reachability
capability dependency
Fiber lifecycle
Effect ownership
desired → running composition
```

它不拥有：

```text
PCM payload
realtime processing order
playback semantics
Fact semantic authority
UI state
decoder/device mechanism
```

（K0 语义权威：`docs/architecture/composition-kernel-0-design.md`。）

**Component** — 拥有某个 responsibility/resource 边界的有界架构单元。是一个中性粒度概念，不是协议成员资格。

**Plugin** — **经过边界论证、以 Component 身份参与 Base Composition Kernel 统一 composition/lifecycle protocol 的长期 capability/lifecycle participant**（语义见 §3）。一个 Plugin 可以 provide/require capability、own resources/effects、register control participation、provide realtime participant/factory、observe Facts。Plugin 不等于：

```text
crate / DLL
feature
thread
Fact / Command / buffer
```

且 `AudioNode != automatically Plugin`、`PCM stage != Plugin`（§3；AudioNode 仍可通过边界论证挣得 Plugin 身份）。

当前实现中的 `ComponentSpec` 是 K0 的 representation/substrate，**不**因此被冻结为最终 Plugin API。

**Fiber** — 一个 composed component/plugin 的 live runtime instance / episode（K0 语义：design authority §F）。

**Capability** — composition-visible 的 typed contract / reachability identity。它建立跨 composition 边界的 typed execution reachability。Capability 不是：

```text
payload bus
provider 实现
Fact
每个 PCM block 上的 registry 查询
```

**Service** — 通过 Capability 到达的可执行对象/interface；真正做工作的是 service/mechanism。

**Host**（Composition Host / Application Host）— 选择/安装 desired components、创建 Base Kernel、驱动 composition lifecycle、拥有 application 级 bootstrap 与 shutdown 的发起/编排（initiation/orchestration）的 architecture role。发起/编排 shutdown 不等于跨域 shutdown 协议已被冻结：composition teardown、realtime-view retirement、reclamation（含 release）之间的 shutdown ordering 仍 OPEN（§17）。Host 不因方便而自动拥有 playback semantics、PCM graph、decoder/output 实现、realtime lifetime authority。crate 名当前不冻结。

## 16.2 Runtime 侧

**Runtime** — 抽象类别：**拥有持续运行状态、执行规则或 lifecycle authority 的 active mechanism/system**。它不是“任何叫 runtime 的 crate”。裸词 `runtime` 禁止在 architecture 行文中同时指 AppRuntime / process lifetime / Audio Runtime / Realtime Runtime / mechanism library / composition root——必须带限定语使用。

**Realtime Runtime** — **负责 realtime execution-view legality 与 realtime-visible lifetime safety 的 specialised runtime responsibility**（由 §6 P1–P5 机制验证证据挣得，Issue #94 Gate 3）。已挣得的最小 authority 范围：

```text
published realtime-view identity
coherent whole-view replacement
new-entry legality
existing/queued holder legality
retirement
quiescence recognition
reclamation eligibility
deferred release coordination
```

（§5–§7 行文中的 "Audio Runtime / realtime graph mechanism" 是整体机制的描述性占位，尚未作为整体挣得；其**已挣得**的责任子集即上文的 Realtime Runtime，其余部分仍 OPEN。）

Realtime Runtime 不等于：

```text
Base Kernel
Playback Domain
PCM participant 实现
qianqian-realtime crate（必然地）
```

> **An earned runtime responsibility does not freeze one crate boundary or one concrete mechanism representation.**

**Realtime mechanism** — 用于实现某条 Realtime Runtime invariant 的具体实现机制（refcount/Arc ledger、epoch/RCU、hazard pointer 或其它）。它们仍是 representation（§6 的 mechanism 清单继续 OPEN）。`PublishedViews<V>` 是一个 candidate/concrete mechanism realization，不与 responsibility 本身互换称呼。

**Realtime Execution View** — 由 control side build/validate 并发布、供 realtime execution 直接消费的 coherent pre-bound execution state。它可能包含 participants、resources/references、processing topology、parameter snapshot 及其它 RT-safe 绑定；具体 layout OPEN。**Execution View != Projection**——两个 "view" 是完全不同的概念（后者见 §2.3，是 derived visibility）。

## 16.3 Plane / 数据侧

**Realtime Data Plane** — hot、typed、pre-bound payload 的执行路径（§2.4）。对 audio 当前核心 payload 是 PCM。逐 quantum 禁止：

```text
Context lookup
Capability resolve
Reconcile
generic Plugin dispatch
generic Fact/Event fanout（PCM）
```

**Dependency topology != realtime processing topology**（§5 已冻结）。

**Execution / Control Plane** — 保持 §2.2 的 reasoning-lens 属性，不因此物化为 `CommandKernel` / `ControlRuntime` / `WorkflowKernel`。典型路径：Command（intent）→ domain/controller/workflow → Capability/Service → mechanism。

## 16.4 Fact / Evidence / Projection 侧

**Command** — intent（§2.2）。Command 不是 fact，不是它所请求结果的证明。

**Mechanism Evidence** — 由 mechanism/provider 产生的、可能参与后续 semantic decision 的 observation（例如未来的 `DecoderObservedEof` / `DeviceLost` observation / `Underrun`）。**Evidence != Fact**，除非产生它的 mechanism 本身就是该 (fact kind, subject scope) 的 designated semantic authority 并完成对应 semantic decision。

**Fact** — **某 designated semantic authority 按该 fact contract 已经确立成立的语义真相**（normative contract：§2.3）。Fact 不是：

```text
raw observation
event callback
log line
database row
projection state
PCM packet
command
```

**Designated Semantic Authority** — 对某个 authority identity（= (fact kind, semantic subject scope)，§2.3）在同一时刻恰好唯一的语义权威；其职责是 validate / decide / semantic commit；Fact publication 发生在 commit 之后。observer != authority；publication transport != authority；mechanism evidence 的产生者 != 自动 authority；controller != 自动 authority。不引入 `GlobalFactAuthority` / `FactKernel` / `CentralEventRouter`。

**Projection** — derived visibility（§2.3）。不得成为 control-correctness authority、resource-lifetime authority 或 semantic truth writer。README/UI/diagnostics 可以消费 Projection。

## 16.5 Reclamation 词汇链

短定义；normative 协议本体在 §6 P1–P5。按语义时刻排列：

```text
Retirement
    published view 因后续 publication 被 closed to new acquisition；
    既存合法持有者仍可继续使用（P2）

Quiescence
    不存在任何 active realtime execution 或 queued reference 仍可能
    通过任何 generation 的 published/retired view 解引用相关资源（P3）

Reclamation eligibility
    回收的语义资格；当且仅当 Quiescence 谓词成立时成立（P3 的 iff）。
    eligibility 是语义事实：它何时首次为真由该谓词定义，
    不依赖任何机制动作。

Reclamation recognition / certification
    控制侧机制发现并认证 eligibility 已成立的过程；
    recognition 可以滞后于 semantic eligibility 本身（P3/P5）。

Release
    reclamation eligibility 成立之后的物理处置/释放状态（P4）
```

**Reclamation**（总称）— 控制侧识别/认证 reclamation eligibility 并协调安全 release 的过程。它不定义 semantic eligibility 首次为真的时刻（由 P3 谓词决定），也不是物理销毁本身。

保持区分：

```text
eligibility != recognition != release   （P3/P4/P5）
Retired != Reclaimable != Released      （P4）
publication != reclamation certificate  （P4）
```

---

# 17. Known normative gaps（OPEN / UNDEFINED）

以下语义当前**没有** normative 定义；在它们被显式挣得并写入本文之前，任何 crate docs、tests、实现行为都不得被当作这些问题的 normative truth（Issue #99 已识别前三项）：

```text
1. application/runtime container shutdown semantics
   —— Host 关停时的 composition teardown / realtime-view retirement /
      reclamation 排序未定义
2. panic/unwind failure model
   —— panic/unwind 与 composition truth、reclamation 状态的交互
      （K0 §G.6 teardown-verdict latch 之外的层面）未定义
3. terminal realtime-view retirement without replacement
   —— P2 只定义“publish N+1 退役 N”；最后一个 view 无后继退役的语义未定义
4. Production playback semantics 整体（§10 清单继续 OPEN）
```

---

# 18. Post-acceptance Vocabulary Amendment

> **Added by ADR-PBK-002 (PROPOSED).** This section records vocabulary canonicalization decisions that amend ADR-PBK-001's historical terminology without changing its semantic responsibilities.

ADR-PBK-002 canonicalizes:

| ADR-PBK-001 term | Current canonical term | Scope |
|---|---|---|
| Host (Composition Host / Application Host) | **Qianqian App** (`QianqianApp`, `qianqian-app`) | architecture noun, code type, crate |
| Base Kernel / Base Composition Kernel / Composition Kernel | **Composition Kernel (K0)** (`CompositionKernel`, `qianqian-composition`) | architecture noun, code type, crate |
| qianqian-audio-api (shared contracts crate) | **Audio API** (`qianqian-audio-api`) | crate |
| `Kernel` (standalone type) | `CompositionKernel` | code type |
| `AppRuntime` | `QianqianApp` | code type |

The semantic responsibilities remain unchanged. ADR-PBK-002 is authoritative for current vocabulary; this section preserves the amendment trace.

Historical terms (`Host`, `MusicKernel`, `TransportKernel`, `Base Kernel`) may still appear in this ADR's historical rationale sections — they are retained as decision history, not as current authority.
