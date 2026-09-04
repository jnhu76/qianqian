# ADR-0003: Product DSP via capability-trimmed libavfilter in AudioEngine; SongCore never runs libavfilter

- Status: accepted
- Date: 2026-09-01

## Context

播放器阶段需要产品 DSP（volume / EQ / tone 等）。可选路线：自研薄 DSP
层、引入第二个 EQ 库（JUCE / KFR / 等）、或复用 FFmpeg 的 libavfilter。
同时必须防止 DSP 能力污染解码核心、防止 filter 能力在 ABI 上泄漏。

## Decision

- **AudioEngine 的产品 DSP 使用能力裁剪的 libavfilter**：
  - capability intent 由人维护于 `ffmpeg/capabilities/dsp.json`；
  - source closure 由 `tools/dsp_closure.py` + pinned configure oracle
    机器推导；
  - 逐级成本证据在 `bench/results/avfilter-minimize/*`。
- **SongCore 永不运行 libavfilter**：`songcore_ffmpeg.c` 不链接 avfilter
  能力，ABI 上没有任何 filter 概念。
- 第一版 DSP 能力集为 tier F1 "core-gain-eq-tone"：`volume` /
  `equalizer`（biquad 族）/ `bass` / `treble` / `lowshelf` / `highshelf` /
  `lowpass` / `highpass`；格式适配 = `aresample`（发现式，非预付）。
- 不做薄 DSP 对比层；不做第二个 EQ 库；不做 crossfade / limiter 之外的
  额外效果实现。

## Alternatives considered

- **自研薄 DSP 层**：重复实现经过验证的滤波原语，且失去逐级成本证据的
  裁剪方法。拒绝。
- **第二个 EQ 库（JUCE / KFR / 等）**：引入与 libavfilter 平行的第二套
  DSP 依赖。拒绝。

## Consequences

- DSP 能力增长 = 修改 capability intent → 机器重新推导闭包 → 附带逐级
  成本证据；不允许手工往构建里加 filter。
- SongCore 保持 decode-only：DSP 与 decode 严格分层，解码核心的 size /
  符号面不受 DSP 能力演化影响。
- 处理永远显式：DSP 只在被 pipeline 请求时运行，BYPASS 是真实旁路。

## Evidence

- `bench/results/avfilter-minimize/*`（F1 core-gain-eq-tone 与 F0–F7
  envelope 的逐级成本）。
- `bench/results/songcore-v1/dsp-src-integration.json`（BYPASS 透明形状 +
  volume/equalizer 有效性机器门）。
- 层与角色：[architecture/audio-core.md](../architecture/audio-core.md)。
