# E01 — FFmpeg Minimal Song Playback Profile

## Hypothesis

只为本地歌曲播放时，FFmpeg 可以从通用多媒体框架裁成很小的不可约集合。

## Baseline

从：

```text
--disable-everything
```

开始。

候选保留：

```text
libavformat
libavcodec
libavutil
libswresample
```

明确不启用：

```text
programs
network
devices
encoders
muxers
video decoders
subtitles
avfilter
swscale
postproc
```

## Stage A

仅支持：

```text
MP3
FLAC
```

要求：

- probe；
- metadata；
- artwork bytes；
- decode；
- seek；
- EOF。

## 实验方法

1. 建立 MP3/FLAC corpus。
2. 编译最小 profile。
3. 运行所有 corpus。
4. 每次删除一个 FFmpeg component。
5. 若 corpus 仍通过，保持删除。
6. 若失败，记录失败样本与 component necessity。
7. 输出 `enabled-components.txt`。

## 每个 component 必须有 provenance

示例：

```text
mp3 demuxer
  required_by:
    corpus/mp3/id3v24-vbr.mp3
    corpus/mp3/id3v23-cbr.mp3

flac decoder
  required_by:
    corpus/flac/16bit-44k.flac
    corpus/flac/24bit-96k.flac
```

禁止：

```text
“以后可能有用，所以留着”
```

## Metrics

- static library size；
- linked binary size；
- compressed package size；
- symbols count；
- startup time；
- MP3 realtime factor；
- FLAC realtime factor；
- peak memory。
