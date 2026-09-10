---
title: 控制平面与数据平面
status: CURRENT
---

# 控制平面与数据平面

<StatusBadge status="CURRENT" />

> **Capability Plane != Data Plane。**

本页分两层：**已实现**的 generic control/data-plane 防火墙（Base Kernel K0，PR #71），以及 ADR-PBK-001（**ACCEPTED**）的 Playback Foundations。后者已接受但尚无可执行实现，不代表真实 PCM playback pipeline 已经实现；文中出现的 TransportKernel / Physical Fence 等名词是旧实验证据，不是 current authority。

---

## 分离

<ClaimBadge role="authority" />

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

Composition Kernel 不拥有 PCM、MediaSpan、playback cursor、Window、Generation 或 rendered position。图中 Decoder / Processing / AudioOutput 是 capability seam；真实 provider 实现尚未获得授权。

---

## Realtime 热路径绝不允许

<ClaimBadge role="authority" />

每个 callback/block 内不得执行：

- Context lookup
- Capability resolution
- Fiber Reconcile
- 通用 EventBus 派发
- 文件系统 / 网络 I/O
- UI / JS / 托管运行时往返
- 无界分配、锁等待或阻塞

图的变更在 control side 准备，再在 RT-safe boundary 发布。当前 publication/reclamation 的 normative contract 是 `docs/adr/ADR-PBK-001.md` §6 **P1–P5**（本页不复制其正文；具体机制仍 OPEN，由 §12 Phase D 验证）。

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

具体 `SinkSession` API/representation 可以随实现演进；长期不变量是：

1. data edge 有明确生命周期与 teardown；
2. provider final release 之前 dependent 完成必要 teardown；
3. RT thread 只触碰预先准备好的 bounded state；
4. raw physical evidence 进入语义解释者（旧实验映射为 TransportKernel；重置后该映射本身是 OPEN 问题），而不是全局可写状态袋。

---

## Processing Graph

目标 PCM 路径：

```text
Decoder → Gain / EQ / SRC / ... → AudioOutput
```

这里的 Gain/EQ/SRC 是 Processing Graph node，不等于 Composition plugin。普通参数更新或 node topology change 不自动触发 Fiber Reconcile。

---

<ProvenancePanel
  :authority="['docs/architecture/composition-kernel-0-design.md', 'docs/architecture/overview.md', 'docs/adr/ADR-PBK-001.md']"
  :decisions="[{ pr: 68 }]"
  :implementation="[{ pr: 71 }]"
  :evidence="['crates/qianqian-kernel/tests', 'specs/playback/PlaybackTemporal.tla']"
/>
