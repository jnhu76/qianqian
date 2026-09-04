# Player MVP scope

> Purpose: 当前播放产品的真实范围——今天已经具备的播放行为、application 层
> 的职责、以及明确尚未开始的部分。
> Scope: user-facing playback 行为与 application 职责边界。运行时内部语义
> 由 [contracts/](../contracts/) 与 [architecture/](../architecture/) 拥有。

## What works today

Native playback runtime 已完成并通过 native regression 验证：

- **解码**：SongCore ABI v1——本地歌曲 open / probe / metadata / artwork /
  decode / seek / close，输出 source-rate Float32 interleaved PCM。
- **播放**：PlayerEngine ABI v1——播放状态机（play / pause / stop / seek /
  snapshot），timeline / epochs / underrun 语义冻结。
- **输出**：Windows 上通过 WASAPI renderer 真实出声（默认端点、共享模式、
  source-rate Float32 直接输出；设备不接受时 device-side SRC）。
- **集成面**：一个 application-facing runtime library（`qianqian`）只导出
  冻结的 `song_*` + `pe_*` C ABI；Kotlin/Native boundary probe 已证明
  Kotlin 消费者可以仅通过该 ABI 完成真实播放生命周期
  （[contracts/ffi-boundary.md](../contracts/ffi-boundary.md)）。

支持的格式集合（Core Common Formats）：MP3、FLAC、AAC/M4A、raw ADTS AAC、
ALAC/M4A、PCM WAV、Ogg Vorbis、Ogg Opus。机器权威是
`native/ffmpeg/capabilities/songcore.json`；只有进入 corpus 并通过 regression 的
格式才算 officially supported
（[standards/testing.md](../standards/testing.md)）。

默认透明路径：未启用任何音效时，用户听到的是未经处理的解码 PCM
（[product.md](product.md) 的 audio quality principle）。

## Application responsibility

播放器 application 只依赖冻结 contract，永不依赖 FFmpeg API：

- 通过 `song_*` / `pe_*` C ABI 控制播放；
- UI 通过轮询 snapshot 观察（µs 位置 / 时长 / 状态），不接触 realtime PCM
  loop；
- 文件 I/O 由 host 提供（`song_io`），平台文件系统 / URI 语义留在
  application 侧。

## What does not exist yet

- 桌面 application / UI（Compose / Qt / 等技术选型未开始）；
- playlist、媒体库、歌词、皮肤、spectrum/EQ UI；
- Windows 以外的平台音频输出后端。

Application 层是下一个独立阶段；其启动以 KMP 消费者能在冻结 ABI 上完成
真实播放生命周期为前提（boundary proof 已完成，见
[contracts/ffi-boundary.md](../contracts/ffi-boundary.md)）。

## Related

- 产品原则与边界：[product.md](product.md)
- FFI 边界契约：[contracts/ffi-boundary.md](../contracts/ffi-boundary.md)
- SongCore 调用契约：[contracts/songcore-api.md](../contracts/songcore-api.md)
- 播放行为契约：[contracts/player-api.md](../contracts/player-api.md)
