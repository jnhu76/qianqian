# ADR-PBK-002：Plugin-composed, direct-flow Audio Data Plane

- **状态**：PROPOSED
- **日期**：2026-09-09
- **作用域**：Qianqian Playback / Audio Data Plane / ARCH-003 / ARCH-004 / ARCH-005
- **类型**：ADR-PBK-001 的 data-plane granularity corrective amendment
- **保留不重开**：ADR-PBK-001 的 MusicKernel / TransportKernel / TrackSession / DecodeSession / Dual Window / Generation Admission / Physical Fence / EOF-drain-ENDED 语义
- **实现状态**：PRODUCTION DATA-PLANE IMPLEMENTATION PAUSED，等待本文接受

---

# 0. 为什么需要这次 Corrective

ADR-PBK-001 已经正确冻结了 playback temporal authority：

```text
MusicKernel      = product/music semantic authority
TransportKernel  = playback temporal authority
TrackSession     = media identity / source lifetime root
DecodeSession    = independently advancing decoder cursor/handle
```

并且正确冻结：

```text
PCM != generic message bus
Composition topology != Processing topology
RT hot path bypasses Context / Reconcile / generic dispatch
```

但它对 **Audio Processing Graph 的 component granularity** 仍保留了一个偏保守的默认假设：

```text
Decoder / AudioOutput
    -> independent Composition providers

Gain / EQ / SRC / Limiter / Mixer
    -> 默认可作为 MusicComponent 内部 processing nodes
```

这会产生一个结构性后果：

> Composition Kernel 只真正管理 PCM 生命周期的两端，中间长期存在的音频处理参与者可能落入另一套私有 lifecycle / graph-management 体系。

这与 Qianqian 更根本的目标不一致：

> **长期存在、可独立配置/替换/撤出的 Audio Data Plane participant 应进入统一 Plugin/Fiber composition lifecycle；PCM 则直接沿已经绑定好的 plugin processing graph 流动。**

因此本 ADR 提议将 Audio Data Plane 改为：

> **Plugin-composed, direct-flow data plane.**

即：

```text
Composition Kernel
    manages participant identity / reachability / lifecycle / replacement

Processing topology authority
    manages ordered PCM edges / branches / graph publication

PCM
    flows directly between pre-bound plugin instances
    and never returns to Composition Kernel per block/callback
```

这是 component granularity / lifecycle boundary 的实质变更，因此不能只作为 implementation representation 修改。

---

# 1. 核心决策

冻结目标：

> **Durable audio data-plane participants are composed as Plugins/Fibers; PCM flows directly across pre-bound typed edges between those plugin instances. The Composition Kernel builds and lifetimes the participants, but never routes individual PCM blocks.**

中文：

> **长期存在的音频数据面参与者进入统一 Plugin/Fiber 生命周期；PCM 从一个已绑定的 Plugin 实例直接流向下一个 Plugin 实例。Composition Kernel 负责让这些参与者存在、可达、可替换和安全退出，但绝不逐块搬运 PCM。**

结构：

```text
                          CONTROL PLANE

                    desired composition
                            |
                            v
                    Composition Kernel
              Context / Capability / Fiber
                   Effect / Reconcile
                            |
                instantiate / bind / withdraw
                            |
                            v
       +---------------------------------------------+
       |         durable plugin participants         |
       |                                             |
       | Decoder  SRC  Gain  EQ  Limiter  Output    |
       +---------------------------------------------+
                            |
                 pre-bind typed RT-safe edges
                            v

                         DATA PLANE

Encoded -> Decoder -PCM-> SRC -PCM-> Gain -PCM-> EQ -PCM-> Output

           ^ no Context / resolve / Reconcile / generic dispatch ^
```

---

# 2. “Everything is Plugin” 在 Audio Data Plane 中的准确含义

本 ADR 不采用：

```text
one feature name == one plugin
one function == one plugin
one PCM block == one plugin
one Rust type == one plugin
```

采用：

> **A durable, independently addressable data-plane participant that owns long-lived state/resources or is independently configurable/replaceable/withdrawable is a Composition plugin candidate and should default toward the common Plugin/Fiber lifecycle.**

典型参与者：

```text
Source / Demux provider        when independently composed
Decoder
Resampler / SRC
PlayerGain
Equalizer
Limiter
Compressor
Mixer
Analyzer
Recorder / Tap
AudioOutput
```

具体是否拆成一个还是多个 Plugin，仍需满足 component-boundary 审计：

```text
independent identity
independent configuration / replacement
owned long-lived state/resource
meaningful capability contract
independent withdrawal/failure domain
clear processing/data edge
reasonable cognitive/configuration cost
```

因此：

```text
Gain != automatically one plugin per scalar parameter
EQ != automatically one plugin per band
SRC != automatically one plugin per algorithmic helper
```

允许一个 Plugin 内部实现多个私有 micro-stage；但**只要某个 durable stage 已被产品/配置/graph 独立寻址和管理，就不应再偷偷藏进 MusicComponent 私有生命周期。**

---

# 3. 明确不是 Plugin 的东西

以下继续不是 Composition Plugin：

```text
PcmBlock
MediaSpan
Buffer / BufferId
GenerationId
Active / Prepared role
Physical Fence transaction record
TransportFact / raw evidence record
TrackSession
DecodeSession
```

原因：

```text
PcmBlock / MediaSpan
    = data items

Generation / Window / Fence state
    = temporal state/protocol state

TrackSession / DecodeSession
    = per-media nested runtime resources
```

它们没有独立 composition identity，不通过 Profile/Reconcile 单独存在或撤出。

冻结：

> **Plugin-composed data plane does not imply every runtime object is a Plugin.**

---

# 4. MusicComponent / Semantic Authorities 继续保留

本 ADR 不把 `MusicKernel` / `TransportKernel` 升级成 Plugin。

继续：

```text
MusicComponent
    = composed playback-domain component / lifecycle root for subordinate
      product+temporal runtime resources

MusicKernel
    = music/product semantic authority

TransportKernel
    = playback temporal authority
```

MusicComponent 不再被描述为“拥有一个私有 Audio Processing Graph”。

它与 data-plane plugins 的关系改为：

```text
MusicComponent
    binds / observes / controls composed data-plane capabilities

Composition Kernel
    owns plugin-instance lifecycle / reachability

Processing topology authority
    owns PCM processing order / direct edges

TransportKernel
    owns temporal admission / generation / physical-cut semantics
```

MusicComponent **不 immediate-own** Decoder/SRC/Gain/EQ/Output plugin Fibers。

---

# 5. Composition Graph 与 Processing Graph：节点可重叠，关系不等价

以前的：

```text
Composition topology != Processing topology
```

继续成立，但含义需要收紧。

## 5.1 Composition Graph

Composition Graph 表达：

```text
which Plugin/Fiber instances exist
which capability contracts are reachable
which provider satisfies which dependency
provider/consumer lifecycle ordering
replacement / withdrawal / reconcile
```

例：

```text
MusicComponent
FFmpegDecoderPlugin
SoXRPlugin
PlayerGainPlugin
ParametricEqPlugin
WasapiOutputPlugin
```

## 5.2 Processing Graph

Processing Graph 表达 PCM 的有向拓扑：

```text
Decoder
   |
   v
SRC
   |
   v
Gain
   |
   v
EQ
   +-------> Analyzer
   |
   v
Output
```

同一组 Plugin instances 可以拥有不同合法 processing topology。

例如 `Analyzer` 可能是 tap，而不是 lifecycle dependency parent。

冻结：

> **Processing order must never be inferred from Fiber creation order, registration order, hash iteration, discovery order, or capability-resolution order.**

> **Composition dependency topology and PCM processing topology are orthogonal structures even when many nodes are the same Plugin instances.**

---

# 6. Processing Topology 必须有唯一 semantic authority

冻结语义，不冻结最终 Rust 类型名：

> **PCM processing topology has exactly one semantic authority responsible for ordered edges, branches, format-compatible graph construction, and RT-safe graph publication.**

暂称：

```text
ProcessingTopologyAuthority
```

它负责：

```text
which durable data-plane plugins participate
ordered PCM edges
branch/tap topology
format compatibility / negotiated edge contract
build immutable/pre-bound executable graph
publish/swap graph at RT-safe boundary
retire old graph after readers quiesce
```

它不负责：

```text
Plugin/Fiber lifecycle ownership         -> Composition Kernel
playback timeline / generation admission -> TransportKernel
product semantics                        -> MusicKernel
per-device mechanism truth               -> AudioOutput provider
```

最终是否命名为 `AudioGraphKernel`、是否作为 MusicComponent subordinate authority，留给 implementation design；**唯一 authority 与职责边界本身冻结。**

---

# 7. Plugin-composed ≠ Plugin-dispatched

禁止：

```text
for every PCM block:
    Context.resolve(...)
    Reconcile(...)
    dispatch(plugin_id, block)
```

正确模型：

```text
CONTROL SIDE

Composition Kernel
    -> resolve participant capabilities
    -> establish lifetime-valid bindings

ProcessingTopologyAuthority
    -> build typed direct graph
    -> publish immutable/pre-bound RT view

RT / DATA SIDE

Decoder.process(...)
    -> direct edge
SRC.process(...)
    -> direct edge
Gain.process(...)
    -> direct edge
EQ.process(...)
    -> direct edge
AudioOutput.submit(...)
```

冻结：

> **Plugin identity exists on the control/lifecycle plane; PCM execution uses pre-bound direct calls/ports/queues and does not re-enter Composition machinery per block.**

---

# 8. Typed PCM Ports / Edges

本 ADR 不冻结最终 Rust trait，但冻结 edge 语义：

```text
Producer -> Transformer -> Consumer
```

每条 PCM edge 必须保存足够的 typed contract：

```text
PCM format
frame count
MediaSpan / provenance
GenerationId / temporal provenance where required
backpressure / bounded-capacity semantics
```

禁止把 PCM edge 降级成：

```text
Any
JSON
string topic
untyped generic EventBus payload
```

Buffer 可以被复用/池化/零拷贝，但：

> **Buffer identity never becomes timeline identity. MediaSpan remains the media-time truth.**

---

# 9. Graph 变更语义

必须区分：

## 9.1 Parameter update

例如：

```text
PlayerGain target value
EQ band coefficient
limiter threshold
```

如果不改变 Plugin identity / graph topology，可以通过该 Plugin 的 typed control capability 更新，并在 RT-safe publication/smoothing 机制中生效。

它**不要求每次都 Fiber Reconcile**。

## 9.2 Node add/remove/replace

例如：

```text
insert EQ plugin
remove Analyzer plugin
replace SRC provider
switch AudioOutput provider
```

这是 composition/lifecycle change：

```text
desired composition changes
    -> Reconcile participant graph
    -> build replacement processing graph
    -> RT-safe publish/swap
    -> retire old graph/readers
    -> release withdrawn plugin when safe
```

冻结：

> **Plugin identity does not mean every parameter mutation is composition mutation; durable node presence/replacement is composition, node-local parameters are normal typed capability state unless a stronger boundary requires otherwise.**

---

# 10. Provider Withdrawal 与 RT Graph Quiescence

这是本 Corrective 新增的关键生命周期不变量。

普通 provider withdrawal 已冻结：

```text
provider begins withdrawal
    -> no new dependency resolution
    -> dependents teardown
    -> provider final release
```

对参与 RT Processing Graph 的 plugin，必须增加 graph-reader quiescence：

```text
provider begins withdrawal
        ↓
provider excluded from new graph construction / new resolution
        ↓
build replacement processing graph without provider
        ↓
publish replacement graph at RT-safe boundary
        ↓
old graph stops receiving new audio quanta
        ↓
wait until old graph readers / callbacks / queued references quiesce
        ↓
release old graph binding/reference
        ↓
provider final release
```

冻结：

> **A data-plane Plugin must never be finally released while any published RT graph can still call it or retain a live data-plane reference to it.**

> **Provider withdrawal completion requires both composition-dependent teardown and data-plane graph quiescence.**

这不是“析构前 sleep 一下”；最终实现必须有可验证的 epoch/lease/RCU/fence/handshake 等机制。

具体 representation 不冻结。

---

# 11. Failure Isolation

每个独立 data-plane Plugin 应拥有明确 failure domain。

例：

```text
Analyzer fails to initialize
```

不应自动推出：

```text
Decoder / Gain / AudioOutput all disappear
```

除非 desired processing topology 明确把 Analyzer 定义为 mandatory dependency。

冻结：

> **Failure propagation follows declared composition/processing dependencies, not accidental plugin-loader batch scope.**

---

# 12. Decoder / TrackSession / DecodeSession

Decoder 现在明确是 data-plane Plugin。

但 `DecodeSession` 仍是 per-media nested resource：

```text
DecoderPlugin
    = durable composed mechanism provider

DecodeSession
    = one independently advancing per-track decoder cursor/session
    = lifetime-owned by TrackSession according to playback resource tree
    = holds/leases provider-issued mechanism state
```

因此真实 seek 仍允许：

```text
TrackSession A
├── DecodeSession gen17 -> DecoderPlugin handle/cursor A17
└── DecodeSession gen18 -> DecoderPlugin handle/cursor A18
```

DecoderPlugin withdrawal 必须等待所有依赖其机制的 DecodeSession settle/release，并且任何 published processing graph 不再引用该 provider。

---

# 13. AudioOutput / Physical Fence

AudioOutput 是 data-plane Plugin，同时仍是 device/session mechanism truth 的 provider。

TransportKernel 继续拥有：

```text
Physical Fence protocol semantics
generation admission
promotion/stop ordering
```

AudioOutputPlugin 只产生机制 evidence：

```text
submitted
rendered/device progress
fence claimed
fence succeeded/failed
device lost
```

冻结：

> **Plugin-composed data plane does not move Physical Fence authority into Composition Kernel or AudioOutput. TransportKernel remains the temporal interpreter; AudioOutput remains the mechanism/evidence producer.**

---

# 14. RT Firewall — Corrective 后的最终解释

RT path 可以调用 Plugin instance 的预绑定 data-plane entrypoint。

这不违反：

```text
RT path bypasses Composition machinery
```

因为禁止的是：

```text
Context lookup
Capability resolution
Fiber state mutation
Reconcile
generic event dispatch
filesystem/network/UI
unbounded allocation/blocking
```

允许的是：

```text
pre-bound plugin function/port
bounded RT-safe queue/ring
immutable/published processing graph
preallocated buffers
RT-safe parameter snapshot
```

冻结口诀：

> **Composition builds the graph; realtime executes the graph.**

> **Plugin-composed does not mean kernel-dispatched.**

---

# 15. Confluence 扩展

Composition confluence 现在必须覆盖 durable audio data-plane participants：

```text
load Decoder A
insert Gain
insert EQ
replace SRC
remove EQ
switch Output
settle
```

最终 desired composition 如果是：

```text
Decoder A + SRC B + Gain + Output C
```

则 settled plugin/capability/lifecycle truth 应与 clean construction 等价。

Processing topology 同样必须由最终 desired graph 决定，而不是历史 mount 顺序。

但已播放的物理声音仍不可回滚；playback timeline continuity 仍由 Transport/Music domain policy 管理。

---

# 16. 对 ADR-PBK-001 的 supersession 范围

若本文 ACCEPTED，则 ADR-PBK-001 以下语义被本文 corrective-refine：

```text
§2  Plugin / Nested Runtime Resource / Data Item
§4  MusicComponent 对 processing graph 的边界描述
§9  Canonical Audio Data Plane
§10 Composition Topology != Audio Processing Graph
§13 PlayerGain / processing placement（仅 lifecycle/granularity 部分）
§14 Data Plane Taxonomy
§17 RT Firewall（补充 plugin-instance direct execution 解释）
§19 I2 / I11 / I15 / I18 的 processing-plugin 解释
§23 processing graph/node representation 的开放范围
§24 ARCH-003 / ARCH-004 data-plane granularity 影响
```

以下 PBK-001 语义明确保留，不重开：

```text
MusicKernel / TransportKernel authority split
TrackSession / DecodeSession meaning
Dual Window
Generation admission
Physical Fence
MediaSpan != Buffer
submitted != rendered
raw evidence -> TransportKernel
EOF / drain / ENDED
Snapshot != authority
Global visibility != global ownership
```

---

# 17. 对 ARCH-004 / ARCH-005 的影响

## ARCH-004 Plugin Graph

当前 frozen v1 不原地修改。

本文接受后必须新增：

```text
ARCH-004 v2
```

表达：

```text
MusicComponent
DecoderPlugin
Processing Plugins (SRC/Gain/EQ/...)
AudioOutputPlugin
```

以及它们与 processing topology 的正交关系。

## ARCH-005 Control Plane vs Data Plane

v1 的核心结论继续正确：

```text
Capability plane != Data plane
Kernel does not route PCM payloads
```

但需要 v2/authority wording corrective 明确：

```text
Data-plane participants themselves can be Plugins;
per-block data flow still bypasses Composition Kernel.
```

---

# 18. Formal / Executable Acceptance Plan

本 Corrective 不是完整重做 PlaybackTemporal。

## 18.1 PlaybackTemporal

默认不改：

```text
Dual Window
Generation Admission
Physical Fence
submitted/rendered
EOF/drained/ENDED
```

只有 implementation pressure 证明 temporal semantics 必须变时才修改。

## 18.2 PlaybackOwnership

必须 corrective，因为现模型只显式建模 DecoderProvider / AudioOutputProvider，而没有 durable processing plugin graph 与 RT graph quiescence。

至少增加/验证：

```text
processing plugin participates in Composition lifecycle
provider withdrawal excludes new graph publication
published old graph may keep provider alive temporarily
provider final release waits for old graph quiescence
MusicKernel / TransportKernel do not become plugin lifetime owners
TrackSession / DecodeSession ownership remains intact
```

## 18.3 新的高风险 interleaving gate

如果状态空间自然可控，增加一个小型显式状态模型（名称不绑定 ADR 编号）攻击：

```text
old graph published with Plugin A
replacement graph built without A
A withdrawal begins
RT callback still reading old graph
new graph published
old callback exits
A final release
```

必须抓住 mutation：

```text
ReleasePluginBeforeGraphQuiescence
```

这是真正具有状态交错风险的部分，值得 formal escalation。

不为所有 DSP order 建 TLA 模型；processing ordering 主要由 types/graph validation/tests 约束。

---

# 19. Implementation Entry（接受后）

本文接受后，真实 Playback implementation 直接使用真实 plugins，不先建立 fake provider 阶段：

```text
FFmpeg Decoder Plugin
    -> Canonical PCM
Processing Plugin(s)
    -> direct pre-bound edges
WASAPI AudioOutput Plugin
```

第一条真实 vertical slice 必须证明：

```text
Composition Kernel truly creates/binds/withdraws providers
PCM never passes through Context/Reconcile per block
TransportKernel still controls generation/fence/evidence interpretation
plugin withdrawal cannot race with published RT graph use-after-free
```

---

# 20. Acceptance Gate

当前：

```text
STATUS                         = PROPOSED
BOUNDARY REVIEW                = NOT RUN
PLAYBACK TEMPORAL SEMANTICS    = RETAINED
OWNERSHIP/GRAPH MODEL          = CORRECTIVE REQUIRED
ARCH-004 V2                    = REQUIRED AFTER ACCEPTANCE
PRODUCTION DATA-PLANE WORK     = PAUSED
```

接受前至少：

```text
G1 Plugin granularity review
G2 Composition-vs-processing topology review
G3 RT direct-flow firewall review
G4 TrackSession/DecodeSession ownership compatibility review
G5 provider withdrawal + graph quiescence review
G6 failure-isolation review
G7 PlaybackOwnership corrective PASS
G8 graph-publication negative control PASS
```

完成后才允许：

```text
ADR-PBK-002 = ACCEPTED
    -> registry/overview authority migration
    -> ARCH-004 v2
    -> real FFmpeg/WASAPI plugin-composed implementation
```

---

# 21. 最终口诀

```text
Everything durable is composed as Plugin.
PCM flows directly through the composed plugin graph.
Composition controls identity, reachability and lifetime.
Processing topology controls ordered PCM edges.
Transport controls playback time and physical-cut semantics.
Realtime executes a pre-bound graph and never re-enters composition machinery per block.
```
