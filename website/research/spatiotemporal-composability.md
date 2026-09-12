---
title: 时空可组合性
status: VALIDATED
---

# 时空可组合性

<StatusBadge status="VALIDATED" />

## 来源

*A Programming Paradigm for Spatiotemporal Composability*

Yifan Shi, Wei Zhang, Tianyi Cui(北京大学;DeepSeek-AI)

arXiv:2608.25512v1,92 页。

<ClaimBadge role="authority" />

---

## 论文说了什么

论文提出 **Cordis**,一个面向可组合运行时系统的形式模型。关键概念:

- **带逆的 Context 变换** —— 每个 Effect 携带一个逆;逆按 LIFO 顺序组合
- **类型化键表 Σ** —— 依赖是一张类型化键表;`provide = set`(可逆),`consume = get`
- **Fiber 作为组合单元** —— 每个 fiber 有身份、context、effects、生命周期
- **Reconcile** —— 将运行图推向期望组合
- **合流性** —— 合法历史到达静息后,结果与一次全新构建一致

论文证明:

| 定理 | 证明内容 |
|------|---------|
| Thm 5/7 | Effect 按 twisted(LIFO)顺序组合 |
| Thm 15 | 局部可逆性是逐次应用的 |
| Thm 70 | 撤回的 teardown 访问窗口 |
| Thm 73 | 静息/推进 |
| Thm 80 | 组合历史后的合流性 |

---

## Qianqian 借鉴了什么

<ClaimBadge role="authority" />

| 论文概念 | Qianqian 实例化 |
|----------|-----------------|
| Context Σ(类型化键表) | **Context** — 能力命名空间/依赖视图 |
| Fiber 生命周期 | **Fiber** — 拥有身份/作用域/生命周期的存活插件实例 |
| 带逆的 Effect | **Effect** — 拥有全逆算子的组合生命周期变更 |
| 期望 → 运行图 | **Reconcile** — 将图推向期望组合 |
| 能力提供/消费 | **Capability** — 命名/类型化的服务契约 |
| LIFO Effect 组合 | Fiber 内的 Effect 按 LIFO 顺序展开 |
| 合流性 | 由 oracle 战役测试(70 项内核测试) |

---

## Qianqian 不借鉴什么

<ClaimBadge role="interpretation" />

| 论文特性 | 为何不借鉴 |
|----------|-----------|
| 生成器式迭代(Def 17–18) | K0 激活是一个有界步骤;不需要生成器机制 |
| 子 fiber 实例化 | 已从 K0 范围移除 —— 父/子只是论文设计背景 |
| HMR(热模块替换) | 对原生音乐播放器不在范围内 |
| 特定键表示(symbol/TypeId) | K 在 K0 中是抽象的;属实现选择 |
| 动态模块加载 | 逻辑插件 ≠ 动态库 |

---

## Qianqian 改变了什么

<ClaimBadge role="evidence" />

| 论文 | Qianqian | 理由 |
|------|----------|------|
| 通用 context 总线 | Context 仅是能力命名空间 | "能力平面 != 数据平面"防止 Context 变成全局状态袋 |
| 所有 Effect 可逆 | 五标签行动分类;K0 中只有 Reversible 组合生命周期 Effect | 已渲染的声音无法"撤销播放" —— 架构不得为不可逆动作承诺撤销 |
| 完整 HMR 支持 | 不适用 | 静态组合,不是 Web 框架 |
| 父/子 fiber | 已从 K0 移除 | MVP 范围控制;初始组合保证不需要 |

---

## 可执行证据

<ClaimBadge role="evidence" />

Base Kernel K0 实现五个被借鉴的原语,并通过 70 项内核 oracle 测试(75 项 workspace 测试)验证。测试证明:

- **Thm 5/7 实例化** — 单 Fiber 清理测试中验证 LIFO Effect 展开
- **Thm 15 实例化** — 局部可逆性:disposer 证明的是局部回退,不是跨 Fiber 独立性
- **Thm 70 实例化** — 提供者消失测试中强制 teardown 访问窗口
- **Thm 80 实例化** — 通过对比变更历史与全新构建来测试合流性

---

## 开放问题

- 子 fiber 语义应如何(如果需要)引入,同时保持 K0 保证?
- 形式模型能否扩展到覆盖实时音频图组合?
- 在组合模型中,有序 DSP 流水线的正确抽象是什么?

---

<ProvenancePanel
  :authority="['docs/architecture/composition-kernel-0-design.md §A.4']"
  :decisions="[{ issue: 67 }, { pr: 68 }]"
  :evidence="['crates/qianqian-composition/tests']"
  lastVerified="743eb86"
/>
