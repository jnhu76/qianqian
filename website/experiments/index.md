---
title: 实验
status: CURRENT
---

# 实验

带溯源的实验结果。每个实验遵循同一契约:

```text
01 问题
02 基线
03 假设
04 方法
05 证据
06 结果
07 架构后果
```

---

## 实验登记

| ID | 名称 | 状态 |
|----|------|------|
| EXP-FFMPEG-001 | [FFmpeg 闭包最小化](/experiments/ffmpeg-minimization) | <StatusBadge status="HISTORICAL_EVIDENCE" /> |
| EXP-COMPOSITION-KERNEL-001 | [Composition Kernel K0 Oracle 测试](/experiments/composition-kernel-oracles) | <StatusBadge status="VALIDATED" /> |

---

## 证据纪律

<ClaimBadge role="authority" />

机器事实优先采信机器制品:

```text
机器制品
>
旧 issue 中的机器事实散文
```

Issue 编号保留为历史溯源,不是当前机器真相。如果制品文件与 issue 文本不一致,以机器制品为准。

---

## 主张分类

每个严肃主张都带有可见的来源角色:

| 角色 | 含义 | 要求 |
|------|------|------|
| <ClaimBadge role="authority" /> | 已接受的架构/项目决策 | 必须指向权威来源 |
| <ClaimBadge role="evidence" /> | 已测量/已测试/已实现的事实 | 必须指向测试/结果/制品 |
| <ClaimBadge role="interpretation" /> | 面向人的解释 | 不得伪装成权威 |

---

<ProvenancePanel
  :authority="['docs/experiments/registry.yml']"
  lastVerified="743eb86"
/>
