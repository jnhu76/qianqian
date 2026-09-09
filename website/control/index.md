---
title: Qianqian — 项目控制
status: CURRENT
---

<script setup>
import { projectState } from '../data/project-state.ts'
</script>

# Qianqian

一个轻量级音乐播放器，也是一座系统架构实验室。

---

## 当前前沿

**{{ projectState.currentFrontier }}**

## 最近里程碑

**{{ projectState.lastMilestone }}**

<StatusBadge status="VALIDATED" />

---

## 当前所处阶段

<table class="layer-table">
  <tr><td>播放参考</td><td><StatusBadge status="HISTORICAL_EVIDENCE" /></td></tr>
  <tr><td>FFmpeg 研究</td><td><StatusBadge status="HISTORICAL_EVIDENCE" /></td></tr>
  <tr><td>组件边界 A0</td><td><StatusBadge status="FROZEN" /></td></tr>
  <tr><td>Base / Composition Kernel K0</td><td><StatusBadge status="IMPLEMENTED" /></td></tr>
  <tr><td>Playback Architecture</td><td><StatusBadge status="CURRENT" /></td></tr>
  <tr><td>Decoder provider</td><td><StatusBadge status="PLANNED" /></td></tr>
  <tr><td>Audio Processing</td><td><StatusBadge status="PLANNED" /></td></tr>
  <tr><td>AudioOutput provider</td><td><StatusBadge status="PLANNED" /></td></tr>
  <tr><td>UI Host</td><td><StatusBadge status="DEFERRED" /></td></tr>
</table>

---

## Playback 已接受结构

ADR-PBK-001（**ACCEPTED**，已登记 ARCH-003 authority）的结构：

```text
MusicComponent
├── MusicKernel       music/product semantic authority
├── TransportKernel   playback temporal authority
└── TrackSession(s)
    └── DecodeSession(s)
```

在该已接受模型中，`MusicKernel` 不承担 timeline/window/generation/fence；`TransportKernel` 是 raw playback evidence 的唯一 temporal interpreter。

当前代码已经建立两个 authority shell，但尚未实现完整 playback state machine、FFmpeg Decoder 或真实 AudioOutput backend。

[阅读 Playback Architecture →](/architecture/playback-kernel)

---

## Base Kernel K0

<StatusBadge status="IMPLEMENTED" />

通用 Composition Kernel 实现五个原语：Context、Capability、Fiber、Effect、Reconcile。它只处理 composition truth，不理解音乐、PCM、TrackSession、Generation、WASAPI 或 UI payload。

Playback 不通过给 generic kernel 增加更多产品概念来实现。

---

## 形式化验证的角色

<StatusBadge status="VALIDATED" />

Playback 的 formal core 已验证 Dual Window、Generation admission、Physical Fence、submitted/rendered 和 EOF/drained/ENDED 的高风险交错，并真实抓到过 stop × natural ENDED 竞态。

> **TLA+ 用来找撞车，不用来证明整个架构。**

ADR-PBK-001 已 ACCEPTED。下一阶段进入 deterministic executable Rust model（implementation entry），优先依靠类型/ownership 与普通测试推动 representation，而不是继续扩张形式化模型数量。

---

## 下一步的三个问题

<template v-for="(q, i) in projectState.nextQuestions" :key="i">
1. {{ q }}
</template>

---

<ProvenancePanel
  :authority="['docs/architecture/overview.md', 'docs/adr/ADR-PBK-001.md', 'docs/site/project-state.ts']"
  :decisions="[{ pr: 68 }, { pr: 71 }, { pr: 78 }, { pr: 79 }]"
  :evidence="['crates/qianqian-kernel/tests', 'specs/playback/PlaybackTemporal.tla']"
/>
