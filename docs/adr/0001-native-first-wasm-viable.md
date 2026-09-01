# ADR-0001 — Native First, WASM Viable

**Status:** Accepted for M0

## Context

SongCore 需要跨 Windows/macOS/Linux/Android/iOS。

FFmpeg 可直接 native cross-compile，也可编译为 WASM。

WASM 有潜在 sandbox 与 artifact portability 优势，但 native app 仍需要额外 runtime。

## Decision

M0：

1. 先完成 Native trimmed FFmpeg backend。
2. 固化 SongCore contract。
3. 建立 corpus + benchmark。
4. 再实现 WASM backend。
5. 二者使用完全相同 contract 和 test vectors。

## Consequences

- WASM 不阻塞播放器开发。
- Native baseline 给 WASM 提供真实比较对象。
- 如果 WASM 失败，不影响 SongCore 架构。
- 如果 WASM 成功，可以升级为第二 production backend 或未来 Web backend。
