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
2. `docs/architecture/songcore-boundary.md`
3. `docs/experiments/*`
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
