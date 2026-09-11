---
title: 路线图
status: CURRENT
---

<script setup>
import { projectState } from '../data/project-state.ts'
</script>

# 路线图

路线图只展示当前工程状态；架构语义以仓库 ADR / architecture docs 为准。

---

## 进度阶梯

| 层 | 状态 | 当前事实 |
|---|---|---|
| 播放参考 | <StatusBadge status="HISTORICAL_EVIDENCE" /> | `playback-reference-v1` 是行为证据，不是未来 source-layout 模板 |
| FFmpeg 闭包研究 | <StatusBadge status="HISTORICAL_EVIDENCE" /> | Decoder/Processing 共用单一 closure authority 的历史证据 |
| 组件边界 A0 | <StatusBadge status="HISTORICAL_EVIDENCE" /> | #53 历史审计保留；其 playback-specific 结论是历史证据，不是 current authority |
| Base / Composition Kernel K0 | <StatusBadge status="IMPLEMENTED" /> | Context / Capability / Fiber / Effect / Reconcile 已实现 |
| Playback Foundations | <StatusBadge status="CURRENT" /> | 播放基础已接受（ADR-PBK-001 ACCEPTED）；旧 playback 模型仍是 experimental evidence；production 播放语义仍需实验挣得 |
| Decoder provider | <StatusBadge status="PLANNED" /> | capability seam 已有；真实 provider 实现未授权于本轮 |
| Audio Processing | <StatusBadge status="PLANNED" /> | ordered PCM graph；普通 DSP node 不自动成为 plugin |
| AudioOutput provider | <StatusBadge status="PLANNED" /> | capability seam 已有；真实设备实现未授权于本轮 |
| UI Host | <StatusBadge status="DEFERRED" /> | UI 不参与 realtime correctness |

---

## 当前前沿：Playback Foundations reset

**{{ projectState.currentFrontier }}**

旧版 ADR-PBK-001（MusicComponent / MusicKernel / TransportKernel 模型）及其 deterministic executable temporal core 已随 2026-09 架构重置降级为 experimental evidence。重置后的 Playback Foundations 已 ACCEPTED；realtime publication/lifetime 的语义碰撞已在模型层证明（`specs/realtime-publication/`），其语义协议 **P1–P5 已 normative 冻结于 ADR §6**；Realtime Runtime 责任已由机制证据挣得（`docs/architecture/realtime-view-publication.md`，Issue #94 closed）。当前 ladder：

- composition reality 之上的 minimal PCM contract（Phase B）——**已交付**（PR #93）；
- direct Source → processing → Sink 数据流（Phase C）——**已交付**（PR #96）；
- publication/reclamation **机制验证**：候选机制在真实 Audio Runtime 下满足已冻结的 P1–P5（Phase D 验证机制，不再裁决 P1–P5 是否正确）——首轮机制证据已交付（PR #97），机制最终裁决仍 OPEN；
- 真实 decoder / output 机制（Phase E）；
- 只有到那时才重新挣得 seek / stop / track / session 等播放语义（Phase F）。

（one normative ladder 见 `docs/adr/ADR-PBK-001.md` §12。）

---

## 实验证据中的历史边界（非 current authority）

```text
MusicKernel / TransportKernel authority split
TrackSession may own multiple DecodeSessions
Active + Prepared may coexist
stale is admission-based, not global-current equality
logical invalidation != physical stop
Composition Graph != Audio Processing Graph
```

这些是旧实验的可复用 failure witnesses / 测试技术；重置后它们**不是**实现者必须遵守的已冻结边界，也不得仅因旧代码存在而自动成为新设计前提。是否重新挣得其中某条边界，由 `docs/adr/ADR-PBK-001.md`（ACCEPTED）之下的真实实验决定。

---

## 下一步问题

<template v-for="(q, i) in projectState.nextQuestions" :key="i">
1. {{ q }}
</template>

---

## 验证升级原则

```mermaid
flowchart LR
    Q["真实问题 / 风险"] --> D["设计 authority"]
    D --> E["必要证据"]
    E --> C["最小实现"]
    C --> T["可执行测试"]
    T --> R["回写当前事实"]

    D -.->|"只有高风险状态交错"| F["Formal exploration"]
    F --> E
```

> **TLA+ 用来找撞车，不用来证明整个架构。**

---

PR #78 / #79 属于旧 ADR 修订历史（experimental evidence），不再列为当前决策记录。

<ProvenancePanel
  :authority="['docs/adr/ADR-PBK-001.md', 'docs/site/project-state.ts']"
  :decisions="[]"
  :evidence="['specs/playback/PlaybackTemporal.tla', 'crates/qianqian-core/tests/playback_temporal_traces/transport.rs']"
/>
