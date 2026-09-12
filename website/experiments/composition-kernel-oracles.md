---
title: Composition Kernel K0 Oracle 测试
status: VALIDATED
---

# Composition Kernel K0 Oracle 测试

<StatusBadge status="VALIDATED" />

## Base Kernel 是否满足其语义设计不变量?

---

## 01 问题

已实现的 Base Kernel K0 是否满足其冻结的语义设计——覆盖单 Fiber 清理、跨 Fiber 独立移除、同键贡献安全、有序交互、提供者消失排序,以及变更历史后的合流性?

---

## 02 基线

语义设计(`composition-kernel-0-design.md`)定义了源自 Cordis 形式模型的精确不变量。在实现之前,它们只是理论保证。

---

## 03 假设

一场可执行的 oracle 战役可以通过构造特定组合历史、断言可观测结果与设计相符,来验证全部六个语义保证组。

---

## 04 方法

测试组织为**对抗性 oracle** —— 每个测试构造一个特定场景:如果实现不正确,该场景将违反某个不变量。

| Oracle 组 | 测试内容 |
|-----------|---------|
| 单 Fiber 清理 | 拥有的 Effect 按 LIFO 展开;fiber 到达终态 |
| 跨 Fiber 移除 | 移除 fiber A 保留独立的 fibers B/C |
| 同键安全 | 贡献组合无隐藏跨键变更 |
| 有序交互 | 非可交换关系使用显式结构 |
| 提供者消失 | 依赖方在提供者释放前完成 teardown |
| 合流性 | 历史 → 静息态 ≡ 全新构建 |

附加 oracle(A1–A21,23 项测试)覆盖评审中识别的对抗性边缘情况。

---

## 05 证据

<ClaimBadge role="evidence" />

| 证据 | 细节 |
|------|------|
| 测试数量 | 70 项内核测试(75 项 workspace 测试) |
| 全部通过 | 合并 commit `743eb86` |
| 测试位置 | `crates/qianqian-composition/tests` |
| 对抗性 oracle | A1–A21(23 项测试) |
| 实现 corrective-1 | `de46bd1`(P0-1..P1-5 修复 + A16–A20) |
| 实现 corrective-2 | `048ebed`(review 5128815134) |

**证据质量:** 可执行的 Rust 测试,在 CI 中运行。每个测试断言冻结语义设计的特定可观测结果。

**最近核验 commit:** `743eb86`

---

## 06 结果

<ClaimBadge role="evidence" />

全部 70 项内核测试通过(75 项 workspace 测试全绿)。六个语义保证组均已验证:

1. **单 Fiber 局部清理** — Effect 按 LIFO 顺序展开;fiber 到达干净的终态。
2. **跨 Fiber 独立移除** — 移除一个 fiber 会保留无关 fiber 的可观测贡献。
3. **同键贡献安全** — 多个 fiber 向同一能力键贡献时,组合过程无隐藏跨键变更。
4. **有序交互** — 非可交换操作使用显式依赖/排序结构,而不是隐式注册顺序。
5. **提供者消失排序** — 依赖方在提供者最终释放之前完成 teardown(带已提交访问)。
6. **合流性** — 任何合法变更历史到达静息态后,可观测结果与对最终期望组合的全新构建一致。

---

## 07 架构后果

<ClaimBadge role="authority" />

Base Kernel K0 是实现 Cordis 风格五原语模型的组合内核的首个已验证实例。它证明:

- 五个原语(Context、Capability、Fiber、Effect、Reconcile)对 K0 范围是充分的
- 合流性可测试,不只是理论
- 提供者撤回可以通过显式排序做到安全
- 领域无关的组合可以强制生命周期不变量

这开启下一个前沿:Playback Architecture,构建在已验证组合基础设施之上的领域特定组件。(写作时点的历史表述是 "Playback Kernel(MusicKernel)";其后 ADR-PBK-001 曾把 playback 拆分为 MusicKernel + TransportKernel authority;2026-09 播放架构重置后,该拆分与该早期说法一样都只是历史/实验证据,不是 current authority。当前 Playback Foundations 已被接受(ACCEPTED)。)

---

## 测试组织

```text
crates/qianqian-composition/tests/
├── capability_oracles.rs            — pending/active ordering, ambiguity, identity (8)
├── lifecycle_oracles.rs             — raise/unwind, FAILED, sibling isolation (5)
├── effect_oracles.rs                — LIFO, same-key removal, provenance (6)
├── revision_quiescence_oracles.rs   — D0–D4 revisions, settle semantics (11)
├── withdrawal_oracles.rs            — provider withdrawal ordering (3)
├── replacement_oracles.rs           — staged replacement (3)
├── data_edge_oracles.rs             — pre-bound endpoints, zero kernel ops (3)
├── confluence_oracles.rs            — history → quiescence ≡ clean build (6)
├── dependency_firewall.rs           — kernel direction/dependency firewall (2)
└── adversarial_review.rs            — A1–A21 adversarial oracles (23)
```

---

<ProvenancePanel
  :authority="['docs/architecture/composition-kernel-0-design.md']"
  :decisions="[{ issue: 67 }, { pr: 68 }]"
  :implementation="[{ issue: 70 }, { pr: 71 }]"
  :evidence="['crates/qianqian-composition/tests']"
  lastVerified="743eb86"
/>
