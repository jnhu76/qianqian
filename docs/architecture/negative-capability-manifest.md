# Negative Capability Manifest

这个文件用来防止项目重新长成“通用媒体框架”。

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
- generic FFmpeg command execution
- audio editor
- exporter
- format converter

## Requires experiment before entry

- libavfilter
- libswscale
- any new FFmpeg protocol
- any external DSP framework
- any WASM runtime
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
