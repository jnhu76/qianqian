# Architecture overview

> Purpose: 30 秒看清系统分层，并把每层路由到它的组件文档与契约。
> Scope: semantic architecture map，不是 physical tree 的逐目录描述。

## Layering

```text
Application (Compose Desktop JVM; `apps/desktop/`, runtime 接入进行中)
        │  stable C ABI: song_* + pe_*，由唯一的 qianqian runtime 导出
        ▼
Player Runtime (PlayerEngine)
        │        ├ decode worker / PCM queue / timeline clocks
        │        └ AudioBackend seam
        ▼            └ WASAPI renderer (Windows 生产后端)
SongCore
        │  frozen contract: source-rate Float32 interleaved PCM
        ▼
FFmpeg closure (pinned n9.0.1, machine-derived minimal source slice)

AudioEngine (post-decode SRC/DSP；显式启用，源匹配时 BYPASS)
```

- Application 只依赖冻结 C ABI，永不依赖 FFmpeg / 内部头文件
  （[contracts/ffi-boundary.md](../contracts/ffi-boundary.md)）。
- Player Runtime 的对外行为见
  [contracts/player-api.md](../contracts/player-api.md)，组成见
  [player-runtime.md](player-runtime.md)；Windows 输出实现见
  [platform-audio.md](platform-audio.md)。
- SongCore 角色与冻结实现见 [audio-core.md](audio-core.md)；调用契约见
  [contracts/songcore-api.md](../contracts/songcore-api.md)。
- FFmpeg 闭包方法见 [ffmpeg-minimization.md](ffmpeg-minimization.md)；
  能力负清单见 [negative-capability-manifest.md](negative-capability-manifest.md)。
- 关键决策（为什么是现在这样）：[../adr/](../adr/)。

## Physical layout note

以上是 semantic 分层，不等于目录树。application 层位于 `apps/desktop/`
（Compose Desktop JVM 外壳，runtime 尚未接入）；native runtime 位于
`native/src/player/`，SongCore 位于
`native/src/songcore_ffmpeg.c`，公共 ABI 位于 `native/include/`。构建定义按
所有权拆分：根 `xmake.lua` 是薄 workspace 入口，native 构建路由是
`native/xmake.lua`，所有权模块在 `native/build/`，跨边界的消费者侧证明在
`integration/`。

## Component ↔ contract map

| Component | Architecture | Contract |
|---|---|---|
| SongCore | [audio-core.md](audio-core.md) | [../contracts/songcore-api.md](../contracts/songcore-api.md) |
| PlayerEngine | [player-runtime.md](player-runtime.md) | [../contracts/player-api.md](../contracts/player-api.md) |
| AudioBackend (WASAPI) | [platform-audio.md](platform-audio.md) | [../contracts/player-api.md](../contracts/player-api.md)（seam） |
| FFI / runtime boundary | — | [../contracts/ffi-boundary.md](../contracts/ffi-boundary.md) |
