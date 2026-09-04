# archive

## What is this directory?

历史证据区。archive 保存曾经为真、已完成或已被取代的材料；**不得作为
current behavior authority**。每个 archived 文档的 header 都标注其
non-authoritative 身份并指向 current equivalent。

## Subdirectories

- `closeouts/` — 已完成 phase 的 closure 记录：campaign 顺序、phase 目标、
  完成证据、corrective 报告。
- `experiments/` — probes 与 one-time 集成证据。
- `superseded/` — 被取代的规格与旧 phase 文档（如 founding PRD）。

## Current equivalents

| 想找 | 去 |
|---|---|
| 当前产品语义 | [../product/](../product/) |
| 当前架构 | [../architecture/](../architecture/) |
| 当前契约 | [../contracts/](../contracts/) |
| 决策理由 | [../adr/](../adr/) |
| 实验证据（active use） | [../research/](../research/) |
| 时间线摘要 | [history.md](history.md) |

## What does not belong here?

- 任何仍表达 current truth 的内容——先迁移到对应 class，再归档原件。
- 为了"以后可能有用"而复制的 current 文档副本（git history 已保存历史）。
