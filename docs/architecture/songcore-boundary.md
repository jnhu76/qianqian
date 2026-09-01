# SongCore Boundary

## 目标

SongCore 是“歌曲文件理解与解码”的最小边界。E11 将 `include/songcore.h`
冻结为 **ABI v1**（`SONGCORE_ABI_VERSION 1`）。

```text
SongSource (host 提供 read/seek/size)
   │  song_io callbacks
   ▼
SongCore ABI v1   ── songcore.h，C、FFI 友好，无 FFmpeg 类型
   │
   ▼
FFmpeg implementation (pinned n9.0.1, machine-derived closure)
   │
   ▼
source-rate / source-layout interleaved Float32 PCM
   + metadata + artwork (compressed bytes)
```

调用方（AudioEngine）在 decode 边界之后拥有交给音频后端所需的全部信息与
PCM，无需重新解析文件。

## 全链路（E11 冻结）

```text
SongSource
   ↓ song_io (host 提供 read/seek/size)
SongCore        → probe 信息 / metadata / artwork / PCM / seek / EOF
   ↓ Float32 interleaved, source rate/layout
AudioEngine     → SRC: aresample/libswresample (源匹配时 BYPASS)
                 → DSP: 能力裁剪 libavfilter (E10-C0, capability intent)
   ↓ 设备目标 rate/layout PCM
AudioBackend
```

- **SongCore 永不运行 libavfilter**，不 resample / rematrix / EQ / limit /
  normalize / ReplayGain-process / crossfade。
- AudioEngine 的 DSP/SRC 是独立裁剪闭包（`bench/dsp-capabilities.json` 人类
  意图 → `tools/pcm_c0.py` + pinned configure oracle 机器推导）。E10 实验
  文档是 historical evidence，不是现行实现。

## Host 负责

- 文件选择；
- Android URI；
- iOS security scoped resource；
- Desktop filesystem；
- buffer ownership；
- AudioBackend（WASAPI / CoreAudio / AAudio 等，Phase 0 不实现）；
- threading policy；
- application lifecycle。

## SongCore 负责

- container detection；
- audio stream enumeration / selection（默认 disposition，否则最低索引；
  切换 = 重建解码器 + 位置回零 + 重建 metadata 快照）；
- metadata（选中的流覆盖容器；缺失不是错误，显式 `has_*`；UTF-8
  长度感知视图；原始枚举支持未知标签）；
- artwork（压缩字节，0..N 项，role/mime/width/height/front-cover 标志）；
- decode（OK+frames>0 / EOF+frames==0 / typed error；部分成功确定性）；
- seek（clamp + 实际位置报告，-1 表示未知；不承诺样本级精确）；
- EOF；
- typed errors（`song_status`：OK / EOF / INVALID_ARGUMENT / IO /
  UNSUPPORTED_CONTAINER / NO_AUDIO_STREAM / UNSUPPORTED_CODEC /
  CORRUPT_DATA / DECODE_ERROR / SEEK_UNSUPPORTED / SEEK_ERROR /
  OUT_OF_MEMORY / INTERNAL_ERROR）。

## SongCore 不负责

- 音量策略；
- EQ / balance / limit / normalize / ReplayGain 应用（只读标签）；
- spectrum；
- SRC（AudioEngine 负责，aresample/swr，源匹配时 BYPASS）；
- playlist / next track；
- UI；
- network。

## ABI v1 不变量（E11 机器权威）

- 一个歌曲快照：probe 信息 / metadata / 封面 / PCM / seek 行为属于同一个
  逻辑歌曲 / 选中流；decode→EOF→seek 之后快照稳定。
- 选中流身份绑定解码器身份；PCM 属于选中流。
- metadata / artwork `pointer+length` 视图在流切换或 `song_close` 前有效。
- 一个 handle 非线程安全；SongCore 内部无 mutex。
- 无 FFmpeg 类型越过 ABI。
- 证据树：`bench/results/songcore-v1/*.json`（read-only `--check` 失败即
  关门）。

## 自定义 IO

优先通过 FFmpeg `AVIOContext` 接受 host callback。

目标是让 FFmpeg core：

```text
无文件系统权限
无网络权限
无 URI 语义
```

它只消费：

```text
read(offset, size)
seek(offset)
size()
```

## Artwork

SongCore 只返回压缩后的 artwork bytes 和 MIME/format hint。

图片解码交给平台/Compose image layer。

因此不为封面启用 FFmpeg video/image decoder。
