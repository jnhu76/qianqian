# ADR-0002: SongCore never resamples; AudioEngine SRC via aresample with BYPASS

- Status: accepted
- Date: 2026-09-01

## Context

本地歌曲的采样率 / 声道布局与输出设备的要求经常不一致（例如 44.1 kHz 源、
48 kHz 设备）。SRC 放在哪一层，直接决定解码闭包的大小、PCM 契约的形状，
以及透明路径是否可能被暗中破坏。

## Decision

- **SongCore 输出不变**：source-rate / source-layout interleaved Float32；
  SongCore 不做 SRC、不做 channel rematrix。sample format 统一
  （packed/planar → interleaved Float32）属于契约内转换。
- **AudioEngine 拥有 SRC**：`aresample`（libswresample）。源 rate/layout 与
  设备目标一致时 BYPASS（完全不进入 resampler）。
- 不引入其他 resampler 实现，也不引入第二个 EQ 库。

## Alternatives considered

- **SongCore 内置 SRC**：会扩大解码闭包、让"最短输出路径"多一级常驻处理，
  且把设备适配策略下沉进了核心。拒绝。
- **自研 resampler / 第三方 resampler 库**：新增一条需要长期维护的 DSP
  实现，违背 extreme subtraction。拒绝。

## Consequences

- 设备侧适配由 host / 输出层完成（能力协商），SongCore 保持纯解码器。
- libswresample 可能出现在 SongCore 构建闭包中，但唯一原因是 FFmpeg
  n9.0.1 的 Opus decoder 在 upstream configure 图中硬依赖 swresample——
  这是 decoder implementation dependency，不代表 SongCore 获得 resample
  能力；不要把 implementation dependency 写成 product capability。
- 设备要求与源不一致时，SRC 是显式启用的能力，而不是透明路径的一部分。

## Evidence

- `bench/results/songcore-v1/dsp-src-integration.json`——44.1k→48k 经
  aresample 的 negotiated rate + duration ratio 机器门；
  BYPASS 透明形状机器门。
- SongCore PCM 契约（恒定 source rate/layout）：
  [contracts/songcore-api.md](../contracts/songcore-api.md)。
