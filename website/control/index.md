---
title: Qianqian — 项目控制
status: CURRENT
---

<script setup>
import { projectState } from '../data/project-state.ts'
</script>

# Qianqian

一个轻量级音乐播放器,也是一座系统架构实验室。

---

## 当前前沿

**{{ projectState.currentFrontier }}**

## 最近里程碑

**{{ projectState.lastMilestone }}**

<StatusBadge status="IMPLEMENTED" />

---

## 当前所处阶段

<table class="layer-table">
  <tr>
    <td>播放参考</td>
    <td><StatusBadge status="HISTORICAL_EVIDENCE" /></td>
  </tr>
  <tr>
    <td>FFmpeg 研究</td>
    <td><StatusBadge status="HISTORICAL_EVIDENCE" /></td>
  </tr>
  <tr>
    <td>组件边界</td>
    <td><StatusBadge status="FROZEN" /></td>
  </tr>
  <tr>
    <td>Base Kernel</td>
    <td><StatusBadge status="IMPLEMENTED" /></td>
  </tr>
  <tr>
    <td>Playback Kernel</td>
    <td><StatusBadge status="NEXT" /></td>
  </tr>
  <tr>
    <td>Decoder</td>
    <td><StatusBadge status="PLANNED" /></td>
  </tr>
  <tr>
    <td>Processing</td>
    <td><StatusBadge status="PLANNED" /></td>
  </tr>
  <tr>
    <td>AudioOutput</td>
    <td><StatusBadge status="PLANNED" /></td>
  </tr>
  <tr>
    <td>UI Host</td>
    <td><StatusBadge status="DEFERRED" /></td>
  </tr>
</table>

---

## 系统架构

Base Kernel、Playback Kernel、Decoder、Processing、AudioOutput 与 UI Host 之间如何关联,见权威的[系统总览图](/architecture/#系统总览)。

---

## 最新成果

### Composition Kernel K0

<StatusBadge status="IMPLEMENTED" />

通用 Composition Kernel 实现五个原语 —— Context、Capability、Fiber、Effect、Reconcile —— 六个语义保证组共 70 项内核 oracle 测试(75 项 workspace 测试)。它领域无关:不了解音乐、PCM、FFmpeg、WASAPI 或 UI 载荷。

**已验证的语义保证:**

- 单 Fiber 局部清理
- 跨 Fiber 独立移除
- 同键贡献安全
- 有序/非交换交互的显式处理
- 提供者消失排序
- 变更历史后的合流性(Confluence)

<ProvenancePanel
  :authority="['docs/architecture/composition-kernel-0-design.md', 'docs/architecture/composition-kernel-0-implementation-adr.md']"
  :decisions="[{ issue: 67 }, { pr: 68 }, { pr: 69 }]"
  :implementation="[{ issue: 70 }, { pr: 71 }]"
  :evidence="['crates/qianqian-kernel/tests']"
  lastVerified="743eb86"
/>

---

## 实验亮点

### FFmpeg 最小化

<StatusBadge status="HISTORICAL_EVIDENCE" />

一个音乐播放器到底需要多少 FFmpeg?Decoder 与 Processing 共享唯一的 FFmpeg 闭包权威。编解码覆盖是提供者配置,不是运行时层。两个构建 profile 证明这个闭包可以最小化。

[阅读实验 →](/experiments/ffmpeg-minimization)

---

## 下一步的三个问题

<template v-for="(q, i) in projectState.nextQuestions" :key="i">
1. {{ q }}
</template>

---

<ProvenancePanel
  :authority="['AGENTS.md', 'CONTEXT.md', 'docs/architecture/overview.md']"
  :decisions="[{ issue: 46 }, { issue: 53 }, { issue: 67 }, { issue: 70 }]"
  lastVerified="743eb86"
/>
