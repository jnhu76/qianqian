# Qianqian / 千千·现代

> 一个只为“本地歌曲播放”服务的极薄跨平台本地音乐播放器。不是通用媒体
> 框架，也不是 FFmpeg wrapper。

## What is Qianqian?

「千千·现代」是简洁、轻量、纯粹的跨平台**本地音乐播放器**：local-first、
playback-only、显式 DSP、极致减法。核心 admission 双轨：

> **native / audio-core 扩张必须回答：没有它，哪一首正常歌曲播不了？
> product / application capability 必须证明用户/产品价值，并由当前 Issue 授权。**

产品定位与边界：[docs/product/product.md](docs/product/product.md)。

## What works today

Native playback runtime 已完成并通过 native regression：

- **SongCore**（ABI v1）：本地歌曲 open / probe / metadata / artwork /
  decode / seek，输出 source-rate Float32 PCM；Core Common Formats
  （MP3 / FLAC / AAC-M4A / ADTS / ALAC / WAV / Vorbis / Opus）。
- **PlayerEngine**（ABI v1）：播放状态机 + timeline/epochs，Windows 上经
  WASAPI 真实出声；一个 `qianqian` runtime library 只导出冻结的
  `song_*` + `pe_*` C ABI。
- Application / UI 层尚未开始（见
  [docs/product/player-mvp.md](docs/product/player-mvp.md)）。

## 30-second architecture

```text
Application (Kotlin / KMP consumer)
    │  stable C ABI (song_* + pe_*, one qianqian runtime library)
    ▼
Player Runtime (PlayerEngine + WASAPI backend)
    ▼
SongCore ── machine-derived minimal FFmpeg closure
```

详见 [docs/architecture/overview.md](docs/architecture/overview.md)。

## Quick start

```bash
python3 scripts/fetch-ffmpeg       # 拉取 pinned FFmpeg source（一次）
xmake ffmpeg-import                # 推导 FFmpeg source closure
xmake f -m release && xmake build songcore
xmake test                         # SongCore + Player regression
```

构建细节与多 target 推导：[docs/development/build-native.md](docs/development/build-native.md)。

像外部调用者一样验证产物：

```bash
xmake build songcore_shared
python3 tools/songcore_ffi_smoke.py song.flac                        # 解码验收
python3 tools/songcore_ffi_smoke.py --play --seconds 5 song.flac    # 可听验收
```

## Repository map

| Path | 内容 |
|---|---|
| `include/` | 公共 ABI（`songcore.h` / `player_engine.h`，v1） |
| `src/` | SongCore、Player runtime、WASM bridge |
| `ffmpeg/` | pin + capability intent + target recipes + profiles |
| `tests/` | SongCore regression、player gates、external consumers、Kotlin probe |
| `bench/` | benchmark harness 与机器证据（`bench/results/`） |
| `corpus/` | fixtures、manifests、本地音乐库（不进 Git 的部分） |
| `tools/` | import / 审计 / smoke 工具 |
| `docs/` | 文档（从 router 进入） |

## Documentation map

所有文档从 [docs/README.md](docs/README.md) 的 routing matrix 进入：按任务
加载最小相关集合，不要递归通读。

- 产品：[docs/product/](docs/product/README.md)
- 架构：[docs/architecture/](docs/architecture/README.md)
- 契约：[docs/contracts/](docs/contracts/)
- 构建 / 发布：[docs/development/](docs/development/)
- 研究证据：[docs/research/](docs/research/README.md)
- 决策记录：[docs/adr/](docs/adr/)
- 历史：[docs/archive/](docs/archive/README.md)

## Contributing

Issue-first；一个 PR 只做一件事。见
[CONTRIBUTING.md](CONTRIBUTING.md)；agent 工作规则见
[AGENTS.md](AGENTS.md)。
