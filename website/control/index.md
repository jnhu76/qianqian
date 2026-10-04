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

## 当前执行阅读入口

当前播放基础、静态组合/最小播放语义、Output/backend 边界分别由 PBK-001/002/003（ACCEPTED）拥有。跨协议阅读见 [playback execution model](https://github.com/jnhu76/qianqian/blob/main/docs/architecture/playback-execution-model.md)：C1/C2 已合并，Stage 5 #208 明确表达 merged Stage-4 candidate；D1–D6 已由 #209 ACCEPT、#210 FROZEN（绑定 #212 验证契约），§1 路由 owning authorities，§12 为 20/20 minimum-core traceability。本页是派生状态页。

## 历史 reset checkpoint 的前沿

以下 `projectState` 文本保留 PR #101–#103 时的观测站快照；不是当前 #198/#201 执行 campaign 状态。

**{{ projectState.currentFrontier }}**

## 该历史 checkpoint 的里程碑

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
  <tr><td>Decode Plugin / SongCore-backed Decoder</td><td><StatusBadge status="IMPLEMENTED" /></td></tr>
  <tr><td>Episode-owned Gain + 10-band EQ / live update</td><td><StatusBadge status="IMPLEMENTED" /></td></tr>
  <tr><td>Output Plugin / owned WASAPI mechanism</td><td><StatusBadge status="IMPLEMENTED" /></td></tr>
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

该旧实验的可执行证据已随 post-#139 spec reset 从 main 移除（Git 历史存档）；当前验证证据见 `specs/README.md`。当前 decode/PCM/output 及已挣得的最小播放语义已有实现；更广的播放状态机与未挣得的机制仍须 authority review。上表的 IMPLEMENTED 不表示本次进行了设备或跨平台运行验证。

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

旧 playback formal core 的验证结果是 experimental evidence。Phase B/C/D 证据是已发生的历史 milestone；当前验证挑战当前 authority 与 production，不能定义新的状态/机制或接受 D1–D6。当前覆盖/假设边界见 `specs/README.md`，执行 campaign 状态见上方阅读入口。

---

## 该历史 checkpoint 的三个问题

<template v-for="(q, i) in projectState.nextQuestions" :key="i">
1. {{ q }}
</template>

---

<ProvenancePanel
  :authority="['docs/adr/ADR-PBK-001.md', 'docs/adr/ADR-PBK-002.md', 'docs/adr/ADR-PBK-003.md', 'docs/architecture/playback-execution-model.md']"
  :decisions="[{ pr: 68 }, { pr: 71 }]"
  :evidence="['crates/qianqian-composition/tests', 'specs/composition-kernel-0/CompositionKernel0.tla']"
/>
