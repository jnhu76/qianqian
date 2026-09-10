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
  <tr><td>组件边界 A0</td><td><StatusBadge status="HISTORICAL_EVIDENCE" /></td></tr>
  <tr><td>Base / Composition Kernel K0</td><td><StatusBadge status="IMPLEMENTED" /></td></tr>
  <tr><td>Playback Foundations</td><td><StatusBadge status="CURRENT" /></td></tr>
  <tr><td>Decoder provider</td><td><StatusBadge status="PLANNED" /></td></tr>
  <tr><td>Audio Processing</td><td><StatusBadge status="PLANNED" /></td></tr>
  <tr><td>AudioOutput provider</td><td><StatusBadge status="PLANNED" /></td></tr>
  <tr><td>UI Host</td><td><StatusBadge status="DEFERRED" /></td></tr>
</table>

---

## Playback Foundations

> 以下结构是旧 Playback 架构实验的 **experimental evidence**（含其短暂 ACCEPTED 的旧版 ADR 修订）；2026-09 重置后不是 current authority。当前 authority：ADR-PBK-001（**ACCEPTED**，不冻结播放状态机名词）。

```text
MusicComponent
├── MusicKernel       music/product semantic authority      （历史实验）
├── TransportKernel   playback temporal authority           （历史实验）
└── TrackSession(s)
    └── DecodeSession(s)
```

当前代码中的 `music` / `transport` 模块即该旧实验留下的 evidence，尚未实现完整 playback state machine、FFmpeg Decoder 或真实 AudioOutput backend。

[阅读 Playback Architecture（历史）→](/architecture/playback-kernel)

---

## Base Kernel K0

<StatusBadge status="IMPLEMENTED" />

通用 Composition Kernel 实现五个原语：Context、Capability、Fiber、Effect、Reconcile。它只处理 composition truth，不理解音乐、PCM、TrackSession、Generation、WASAPI 或 UI payload。

Playback 不通过给 generic kernel 增加更多产品概念来实现。

---

## 形式化验证的角色

<StatusBadge status="VALIDATED" />

旧实验的 playback formal core 曾验证 Dual Window、Generation admission、Physical Fence、submitted/rendered 和 EOF/drained/ENDED 的高风险交错，并真实抓到过 stop × natural ENDED 竞态。

> **TLA+ 用来找撞车，不用来证明整个架构。**

旧 playback formal core 的验证结果是 experimental evidence。播放基础已接受（ADR-PBK-001 ACCEPTED）后，下一步由最小实验（minimal PCM contract 起）驱动，而不是继续扩张形式化模型数量。

---

## 下一步的三个问题

<template v-for="(q, i) in projectState.nextQuestions" :key="i">
1. {{ q }}
</template>

---

<ProvenancePanel
  :authority="['docs/architecture/overview.md', 'docs/adr/ADR-PBK-001.md', 'docs/site/project-state.ts']"
  :decisions="[{ pr: 68 }, { pr: 71 }]"
  :evidence="['crates/qianqian-kernel/tests', 'specs/playback/PlaybackTemporal.tla']"
/>
