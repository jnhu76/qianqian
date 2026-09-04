# ADR-0001: Native first, WASM viable

- Status: accepted
- Date: 2026-08-29

## Context

SongCore 需要跨 Windows/macOS/Linux/Android/iOS。

FFmpeg 可直接 native cross-compile，也可编译为 WASM。

WASM 有潜在 sandbox 与 artifact portability 优势，但 native app 仍需要额外 runtime。

## Decision

1. 先完成 Native trimmed FFmpeg backend。
2. 固化 SongCore contract。
3. 建立 corpus + benchmark。
4. 再实现 WASM backend。
5. 二者使用完全相同 contract 和 test vectors。

## Alternatives considered

- **WASM 与 native 并行推进**：两条未验证路线互相拖慢，且 native baseline
  缺失时 WASM 没有真实比较对象。拒绝。
- **仅 native，不做 WASM 论证**：portability / sandbox 价值未经测量即被
  排除，违背实验驱动原则。拒绝。

## Consequences

- WASM 不阻塞播放器开发。
- Native baseline 给 WASM 提供真实比较对象。
- 如果 WASM 失败，不影响 SongCore 架构。
- 如果 WASM 成功，可以升级为第二 production backend 或未来 Web backend。
- Native-first 仍是 shipping 默认；WASM 是同一 ABI 之后的 future target
  （测量证据：[research/wasm.md](../research/wasm.md)）。
