---
title: 控制平面与数据平面
status: CURRENT
---

# 控制平面与数据平面

<StatusBadge status="CURRENT" />

> **Capability Plane != Data Plane。**

本页是派生阅读页：generic control/data-plane 防火墙（K0，PR #71）和当前 decode → episode-owned processing → PCM edge → output 路径均已有实现。PBK-001（ACCEPTED）拥有基础契约，PBK-002（ACCEPTED）拥有当前静态组合与最小播放语义，PBK-003（ACCEPTED）拥有 Output/backend 边界。跨协议执行阅读见 [playback execution model](https://github.com/jnhu76/qianqian/blob/main/docs/architecture/playback-execution-model.md)（D1–D6 **CANDIDATE / NOT FROZEN**；§1 authority routing，§12 20/20 traceability）。本页 TransportKernel / Physical Fence 等历史映射不描述当前实现。

---

## 分离

<ClaimBadge role="interpretation" />

Context 决定**谁能到达谁**。绑定之后，载荷通过已解析服务或预绑定数据边直接流动。

**已实现层**：generic Composition Kernel 的 control/data-plane 防火墙（K0 oracle 测试，PR #71）保证控制面不逐块接触载荷。

```mermaid
flowchart TB
    subgraph CP["Composition / 控制平面"]
        DC["期望组合"] --> CK["Composition Kernel"]
        CK --> BIND["resolve / bind"]
    end

    subgraph DP["Canonical Audio Data Plane"]
        direction LR
        DEC["Decoder"] -->|"Canonical PCM + MediaSpan"| PROC["Audio Processing Graph"]
        PROC -->|"Canonical PCM"| AOUT["AudioOutput"]
    end

    BIND -.->|"建立长期/预绑定边，不逐块查找"| DP
```

Composition Kernel 不拥有 PCM、MediaSpan、playback cursor、Window、Generation 或 rendered position。上图是分离示意，不冻结 MediaSpan 或 Processing Graph API。当前 Decode/Output 是 Plugin；processing 与 PCM edge 是 Session-owned resources，processing 不是独立 Capability/Plugin。真实 provider 已实现，具体执行/所有权见 execution model §4；存在实现不等于本次已验证设备运行。

---

## Realtime 热路径绝不允许

<ClaimBadge role="interpretation" />

每个 callback/block 内不得执行：

- Context lookup
- Capability resolution
- Fiber Reconcile
- 通用 EventBus 派发
- 文件系统 / 网络 I/O
- UI / JS / 托管运行时往返
- 无界分配、锁等待或阻塞

需要重叠 realtime views 的变更须遵守 `docs/adr/ADR-PBK-001.md` §6 **P1–P5**。是否触发该契约与需要何种机制，由 owning authority 和具体证据决定；当前静态播放与 live DSP 的 placement/lifetime 见 PBK-002 D6/D14、DSP product model §7.3。本页不预建通用 graph-publication 机制，也不把旧 Phase-D 计划当作当前状态。

---

## 播放证据不是通用事件

**Playback mapping（历史实验证据；旧版 ADR 修订曾 ACCEPTED，重置后已降级）**：AudioOutput 产生的：

```text
submitted evidence
rendered evidence
Physical Fence verdict
device/output evidence
```

属于 playback temporal evidence。历史实验中的证据路由：

```mermaid
flowchart LR
    AO["AudioOutput"] -->|"raw evidence"| TK["TransportKernel"]
    TK -->|"typed derived fact"| MK["MusicKernel"]
```

在该历史实验模型中，TransportKernel 是 raw playback evidence 的语义解释者；MusicKernel 不独立重算 rendered cursor 或 EOF/fence 结果。重置后此映射只是 experimental evidence，不是 current authority。

---

## Physical truth

**Playback semantics（历史实验证据）**：

```text
decoded != queued != submitted != rendered
logical invalidation != physical stop
```

Generation admission 只能阻止新的旧-generation 工作，不能撤回已经提交给设备的旧尾巴。需要 hard cut 时必须得到 Physical Fence 的 definitive verdict。

Fence 进入 claimed 阶段后，后来的 intent 不得假装它没有发生或改写已经开始的 physical transaction。

---

## 数据边生命周期

**历史实验映射（illustrative，非 current architecture 必须形状）**：旧实验中 MusicComponent 通过 Composition Kernel 获得 AudioOutput/PcmSink capability。具体 PCM 边在绑定后成为直接/预绑定 data edge；不能由 composition root 偷偷塞一个反向 Music 指针，也不能让 AudioOutput 通过 Context 在每个 block 反查 Music。当前 contract 只依赖 ADR-PBK-001 §2.4（realtime data plane）与 §6 P1–P5（publication/reclamation）。

下列是旧 `SinkSession` 实验的说明，不冻结当前 API/representation；当前 data-edge lifetime 和 raw evidence 路由分别见 PBK-002 D6/D11/D14、PBK-003 §5–6 与 execution model §4/§9：

1. data edge 有明确生命周期与 teardown；
2. provider final release 之前 dependent 完成必要 teardown；
3. RT thread 只触碰预先准备好的 bounded state；
4. raw physical evidence 进入当时的语义解释者（旧实验映射为 TransportKernel），而不是全局可写状态袋；当前 D11 terminal authority 是 Session semantic role，其他机制证据不能自动成为 Fact。

---

## Processing Graph

长期 processing 示例（当前已实现 Gain + 10-band EQ；SRC 等仍须单独挣得）：

```text
Decoder → Gain / EQ / SRC / ... → AudioOutput
```

这里的 Gain/EQ/SRC 是 Processing Graph node，不等于 Composition plugin。普通参数更新或 node topology change 不自动触发 Fiber Reconcile。

---

<ProvenancePanel
  :authority="['docs/architecture/composition-kernel-0-design.md', 'docs/adr/ADR-PBK-001.md', 'docs/adr/ADR-PBK-002.md', 'docs/adr/ADR-PBK-003.md', 'docs/architecture/dsp-product-model.md', 'docs/architecture/playback-execution-model.md']"
  :decisions="[{ pr: 68 }]"
  :implementation="[{ pr: 71 }]"
  :evidence="['crates/qianqian-audio-api/tests/realtime_view_publication', 'specs/realtime-publication/RealtimePublication.tla']"
/>
