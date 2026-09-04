# Contributing to Qianqian

本文是 contributor 的 router，不是 development manual。深入文档一律通过
[docs/README.md](docs/README.md) 路由。

## Before you start

- 读 [README.md](README.md)（是什么、怎么构建）和
  [docs/product/product.md](docs/product/product.md)（产品边界）；
- 本仓库是“极致减法”项目，admission 分双轨：native / audio-core 扩张先回答——**没有它，哪一首正常歌曲播不了？**；product / application capability 须证明用户/产品价值并由当前 Issue 授权（见 [AGENTS.md](AGENTS.md) §1）。

## Issues

Issue-first：先建 issue 再动手。使用 `.github/ISSUE_TEMPLATE/` 下的
bug / feature / research 模板；research issue 允许负结果。

## Development setup

构建命令的权威：[docs/development/build-native.md](docs/development/build-native.md)。
回归与验证命令的权威：[tests/songcore/README.md](tests/songcore/README.md)；
测试标准：[docs/standards/testing.md](docs/standards/testing.md)。

## Focused changes

一个 PR 只做一件事。不做顺手重构；重构需要独立 issue 说明收益。

## Tests and verification

- 音频核心变更至少报告：影响 corpus、build profile、binary size delta、
  相关测试、benchmark 影响、是否改变音频 PCM（AGENTS 规则）；
- 涉及 Native vs WASM、性能、音质、codec 支持，必须给可复现证据；
- 未在真实环境验证的变更只能标 `CODE_COMPLETE_PENDING_VALIDATION`。

## Documentation changes

新建长期 Markdown 前：先分类，再使用
[docs/standards/templates/](docs/standards/templates/) 的 canonical template；
规则见 [docs/standards/documentation.md](docs/standards/documentation.md)。

## Commit / PR expectations

- Commit message 使用 scope 前缀（`feat/fix/test/docs/build` + 领域）；
- PR 使用 `.github/PULL_REQUEST_TEMPLATE.md`，明确写出 **What did NOT
  change**（ABI / runtime semantics / production code 未动等）。

## Native-boundary changes

- ABI 已冻结（`songcore.h` / `player_engine.h` v1）：兼容性靠 reserved 字段
  与新增函数，破坏即 v2；
- FFmpeg 类型不得穿越边界；production decode 路径只依赖 pinned FFmpeg 最小
  source closure + Qianqian 自有代码（AGENTS 规则）；
- 跨层契约变更必须同步对应 contract 文档（[docs/contracts/](docs/contracts/)）并给出验证证据。

## Where deeper documentation lives

[docs/README.md](docs/README.md) —— 按任务路由到最小相关文档集合。
