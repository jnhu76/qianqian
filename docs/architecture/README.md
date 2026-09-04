# architecture

## What is this directory?

当前系统的组成与运行（current semantic truth）。不写施工故事；normative
行为规则在 [../contracts/](../contracts/)。

## What belongs here?

- [overview.md](overview.md) — 分层地图与 reading order。
- [audio-core.md](audio-core.md) — Native Audio Core：SongCore / AudioEngine /
  AudioBackend 角色与冻结实现。
- [player-runtime.md](player-runtime.md) — PlayerEngine 运行时组成。
- [platform-audio.md](platform-audio.md) — WASAPI renderer 与 qianqian
  runtime 组成。
- [ffmpeg-minimization.md](ffmpeg-minimization.md) — FFmpeg 机器推导闭包
  方法。
- [negative-capability-manifest.md](negative-capability-manifest.md) —
  能力负清单（防止长成通用媒体框架）。

## What does not belong here?

- 契约 / 实验证据 / 历史 phase 记录（[../contracts/](../contracts/)、
  [../research/](../research/)、[../archive/](../archive/)）。

## What should I read first?

新任务从 [overview.md](overview.md) 进入，按任务路由到组件文档 + 对应
contract（也见 [../README.md](../README.md) 的 routing matrix）。
