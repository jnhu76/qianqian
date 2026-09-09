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
| 组件边界 A0 | <StatusBadge status="FROZEN" /> | #53 历史审计保留；其 playback-specific 结论是历史输入，正由 ADR-PBK-001（PROPOSED）拟议替代，待 ACCEPTED 后才迁移 registry authority |
| Base / Composition Kernel K0 | <StatusBadge status="IMPLEMENTED" /> | Context / Capability / Fiber / Effect / Reconcile 已实现 |
| Playback Architecture | <StatusBadge status="CURRENT" /> | ADR-PBK-001：PROPOSED / FORMAL CORE PASS；当前做全仓 authority alignment / acceptance review |
| Decoder provider | <StatusBadge status="PLANNED" /> | capability seam 已有；真实 provider 实现未授权于本轮 |
| Audio Processing | <StatusBadge status="PLANNED" /> | ordered PCM graph；普通 DSP node 不自动成为 plugin |
| AudioOutput provider | <StatusBadge status="PLANNED" /> | capability seam 已有；真实设备实现未授权于本轮 |
| UI Host | <StatusBadge status="DEFERRED" /> | UI 不参与 realtime correctness |

---

## 当前前沿：统一 Playback authority，再决定是否 ACCEPT

**{{ projectState.currentFrontier }}**

这一阶段的目标不是实现 PlayerEngine，而是让仓库的当前 surface 诚实表达同一组事实：registered ARCH-003 authority 尚未迁移，ADR-PBK-001 是 PROPOSED / FORMAL CORE PASS 的拟议替代候选：

```text
MusicComponent
├── MusicKernel       music/product semantic authority
├── TransportKernel   playback temporal authority
└── TrackSession(s)
    └── DecodeSession(s)
```

本轮允许最小 code shell 对齐 vocabulary，但不把 ADR 的未来 representation 提前变成生产实现。

只有 ADR 经过人工 ACCEPTED 之后，下一阶段才进入 executable playback model，并由实现压力逐步挣得：

- TrackSession / DecodeSession representation；
- Active / Prepared role representation；
- Generation admission executable contract；
- Physical Fence 与真实 AudioOutput provider 的接口；
- fence 在途时后续 intent 的 defer/coalesce/latest-wins 等 policy。

---

## 已经不再是开放问题的边界

```text
MusicKernel != playback timeline authority
TransportKernel = playback temporal authority
TrackSession may own multiple DecodeSessions
Active + Prepared may coexist
stale is admission-based, not global-current equality
logical invalidation != physical stop
Composition Graph != Audio Processing Graph
```

在 ADR-PBK-001 拟议模型内部，这些不是实现者可随意重新选择的风格偏好；模型整体仍处于 PROPOSED，待人工 ACCEPTED。

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

<ProvenancePanel
  :authority="['docs/adr/ADR-PBK-001.md', 'docs/architecture/overview.md', 'docs/site/project-state.ts']"
  :decisions="[{ pr: 78 }, { pr: 79 }]"
  :evidence="['specs/playback/PlaybackTemporal.tla', 'crates/qianqian-core/src/transport.rs']"
/>
