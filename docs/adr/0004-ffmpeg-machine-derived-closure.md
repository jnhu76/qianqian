# ADR-0004: FFmpeg ships as a machine-derived source closure, never a hand-maintained fork

- Status: accepted
- Date: 2026-08-30

## Context

Qianqian 需要一个基于 FFmpeg 的、体积极小的本地歌曲解码核心，但不维护
"删过的 FFmpeg fork"。可选路线：维护 patched FFmpeg fork、手工维护删减
文件清单、直接使用完整上游构建或现成打包层（ffmpeg.wasm / ffmpeg-kit）。

## Decision

- **Pin 上游**：pinned upstream FFmpeg tag/commit（`native/ffmpeg/pin.json`），
  upstream tree 保持 pristine，从不打补丁。
- **capability intent 由人维护**（`native/ffmpeg/capabilities/*.json`）；
  **source-file closure 由机器推导**：import 时以 pinned tag 的
  `configure`/`Make` 为 oracle 求出真实依赖图与 per-TU 编译语义，冻结为
  compile manifest。
- **常规构建由 Qianqian 自有构建系统重放冻结 manifest**（Xmake replay），
  FFmpeg Makefile 不参与日常构建。
- 每个 target 用同一 intent + 自身 target recipe 独立推导 manifest；
  target 身份 fail-closed（A target 的 manifest 不能满足 B target 的
  build session）。
- 升级 FFmpeg = 对新 pin 重新求 closure，diff（source/flag/size/symbol/
  corpus/PCM drift）就是 review 证据；禁止复制旧 source list。
- shipping size 的权威是最终链接产物，不是源文件计数。

## Alternatives considered

- **手工维护的 FFmpeg fork / 删减文件清单**：无法证明与任何 upstream
  状态对应，升级即漂移。拒绝。
- **完整上游构建**：体积与符号面不可接受。拒绝。
- **ffmpeg.wasm / ffmpeg-kit / libav.js 作为 shipping runtime**：违反
  production dependency boundary（AGENTS）。拒绝（只作方法参考）。

## Consequences

- 任何被启用的 FFmpeg component 都能回答"哪一类真实歌曲需要它"。
- 构建可从 pin + intent + target recipe 复现；manifest 是机器事实，
  不是文档愿望。
- configure/Make 只在 import / upgrade 时运行一次；它的角色是 oracle，
  不是构建系统。

## Evidence

- 方法与流水线：[architecture/ffmpeg-minimization.md](../architecture/ffmpeg-minimization.md)。
- 实测证据：[research/ffmpeg-minimization.md](../research/ffmpeg-minimization.md)。
- fail-closed target 身份负测试：`native/tests/songcore/target_gate_test.py`。
