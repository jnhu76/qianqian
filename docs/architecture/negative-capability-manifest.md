# Negative capability manifest

这个文件用来防止项目重新长成“通用媒体框架”。

> 能力默认不存在，只有真实歌曲播放需求才能将它加入。每项准入必须回答：
> 没有它，哪一首正常歌曲播不了？

## Permanently out of core unless PRD changes

- video
- video decoder
- video encoder
- audio encoder
- muxer
- remux
- transcoder
- subtitle
- network protocols
- stream downloader
- capture device
- microphone
- camera
- screen capture
- image decoder（artwork 只以压缩字节交付，SongCore 永不解码图像）
- generic FFmpeg command execution（ffmpeg / ffprobe / ffplay）
- audio editor
- exporter
- format converter

## Admitted by frozen decision (scope-limited)

- **libavfilter** — AudioEngine 的产品 DSP 专用，能力裁剪闭包；SongCore
  永不运行 libavfilter（[ADR-0003](../adr/0003-dsp-libavfilter-audioengine.md)）。
- **libswresample / aresample** — AudioEngine 的 SRC；SongCore 闭包内出现
  仅因 Opus decoder 的 upstream build 依赖，不构成 resample 能力
  （[ADR-0002](../adr/0002-songcore-src-boundary.md)）。

## Requires experiment before entry

- libswscale
- any new FFmpeg protocol
- any external DSP framework
- any WASM runtime（已证明技术可行：
  [research/wasm.md](../research/wasm.md)；不是 shipping 默认，
  [ADR-0001](../adr/0001-native-first-wasm-viable.md)）
- hardware acceleration
- platform-native decoder fallback

## Allowed core direction

Only:

```text
song bytes
→ metadata/artwork
→ audio decode
→ seek
→ PCM
```
