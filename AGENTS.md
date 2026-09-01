# AGENTS.md

本仓库是一个“极致减法”项目。Agent 在修改代码或文档前必须遵守以下规则。

## 1. 项目不是通用 FFmpeg wrapper

本项目只服务本地歌曲播放。

任何新增 API、FFmpeg component、依赖或 abstraction 都必须回答：

> 没有它，哪一首正常歌曲播不了？

无法回答时，不应加入。

## 2. 架构权威

优先级：

1. `PRD.md`
2. `docs/audio-core.md`
3. `docs/ffmpeg-minimization.md`、`docs/architecture/*`、`docs/history.md`
4. 当前代码和测试
5. 外部项目仅作为实现参考

KiteCodec、FFmpegKitNext、libav.js 等都不是架构权威。

## 3. 不允许 FFmpeg 类型泄漏

上层不得出现：

- `AVFormatContext`
- `AVCodecContext`
- `AVPacket`
- `AVFrame`
- `SwrContext`

只允许通过 SongCore 自有 contract 交互。

## 4. 默认做减法

新增能力前先检查是否可以：

- 复用现有 capability；
- 删除 abstraction；
- 使用 host IO；
- 关闭 FFmpeg feature；
- 用测试而不是 adapter 层解决差异。

## 5. 实验驱动

涉及以下问题不得凭感觉下结论：

- Native vs WASM；
- resample 是否必须；
- binary size；
- decode performance；
- seek performance；
- DSP 音质；
- codec 支持。

必须给出 corpus、benchmark 或 reproducible evidence。

## 6. 不提前实现播放器功能

Phase 0 不实现：

- UI
- playlist
- skin
- lyrics UI
- online services
- converter
- editor

除非 PRD 明确升级阶段。

## 7. 修改后的最低证据

任何音频核心变更至少报告：

- 支持/影响的 corpus；
- build profile 是否变化；
- binary size delta；
- relevant tests；
- benchmark 是否受影响；
- 是否改变音频 PCM。

## 8. Production dependency boundary

Production decode 路径只允许依赖：

- pinned upstream FFmpeg 的机器推导最小 source slice；
- Qianqian 自有代码。

`libav.js`、`ffmpeg.wasm`、`ffmpeg-kit` 等可以研究其方法，但不得成为 shipping runtime layer。

为了证明 PCM 真正可听，可以在 `tools/` 使用极薄的 test-only audio sink（例如 Python `sounddevice`）；它不得解码压缩音频，也不得进入 SongCore 或 shipping dependency graph。

## 9. Selective-build / upgrade rule

- capability intent 由人维护；source-file closure 由机器推导，禁止手工维护“删过的 FFmpeg fork”。
- FFmpeg `configure/Make` 可以在 import / upstream upgrade 时充当 oracle；normal production-oriented build 应由 Qianqian 自有构建系统重放冻结的 compile manifest。
- 升级 FFmpeg 时重新求 closure 并审查 source/flag/size/symbol/corpus/PCM drift，不得直接复制旧版本 source list。
- 未在真实环境运行的 selective-build 或 audible smoke 只能标记 `CODE_COMPLETE_PENDING_VALIDATION`，不能写 PASS。
