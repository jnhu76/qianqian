# Qianqian documentation router

本文件是 documentation router，不是第二份 architecture 文档：它只回答
“当前任务应该读哪几份文档”。

规则：**不要递归通读 `docs/`**。按下面的矩阵加载最小相关集合。

文档分类、house style、canonical templates 的权威在
[standards/documentation.md](standards/documentation.md)。

## Routing matrix

| Task | Read first |
|---|---|
| 产品行为 | [product/](product/README.md)（[product.md](product/product.md) / [player-mvp.md](product/player-mvp.md)） |
| Application / UI | product + [architecture/overview.md](architecture/overview.md)（UI standard 尚未建立） |
| Player runtime | [architecture/player-runtime.md](architecture/player-runtime.md) + [contracts/player-api.md](contracts/player-api.md) |
| SongCore | [architecture/audio-core.md](architecture/audio-core.md) + [contracts/songcore-api.md](contracts/songcore-api.md) |
| 平台音频 / WASAPI | [architecture/platform-audio.md](architecture/platform-audio.md) + seam 契约（[contracts/player-api.md](contracts/player-api.md)） |
| FFI / 集成 | [contracts/ffi-boundary.md](contracts/ffi-boundary.md) |
| FFmpeg 闭包方法 | [architecture/ffmpeg-minimization.md](architecture/ffmpeg-minimization.md)（证据：[research/ffmpeg-minimization.md](research/ffmpeg-minimization.md)） |
| Build / install | [development/build-native.md](development/build-native.md) |
| Release | [development/release.md](development/release.md) |
| Testing | [standards/testing.md](standards/testing.md) + [tests/songcore/README.md](../tests/songcore/README.md) |
| 新建长期文档 | [standards/documentation.md](standards/documentation.md) + [standards/templates/](standards/templates/) |
| Research | [research/README.md](research/README.md) |
| 历史证据 | [archive/README.md](archive/README.md) |
| 为什么这样决策？ | [adr/](adr/) + 相关 research |
