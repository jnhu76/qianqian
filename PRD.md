# PRD — Qianqian Audio Lab / 千千·现代 Phase 0

**状态**：Ready for implementation  
**产品阶段**：Phase 0 — Thin Audio Lab  
**长期产品**：千千·现代（Qianqian Modern）  
**首要目标平台**：Windows x86_64 / arm64  
**后续验证平台**：macOS / Linux / Android / iOS  
**核心原则**：Local-first / Playback-only / Explicit DSP / Extreme subtraction

---

# 1. 为什么先不做播放器

「千千·现代」最终目标仍然是一款简洁、轻量、纯粹的跨平台本地音乐播放器。

但在 UI、媒体库、歌词、皮肤、播放列表之前，项目首先需要回答一个更基础的问题：

> **跨平台本地歌曲播放，真正需要多大的音频核心？**

如果这一层没有稳定边界，播放器会被以下实现细节侵入：

- FFmpeg API；
- JNI；
- Kotlin/Native cinterop；
- AVFoundation；
- Media3；
- JavaSound；
- WASM runtime；
- resampler；
- codec-specific behavior。

因此第一阶段单独建立 Qianqian Audio Lab。

它只负责：

```text
歌曲文件 → metadata / artwork / PCM / seek
```

不负责“播放器产品”。

---

# 2. 第一阶段产品定义

Qianqian Audio Lab 是一个 **Song Playback Appliance**。

它不是：

- 通用媒体框架；
- 通用音频处理库；
- FFmpeg Kotlin binding；
- FFmpeg CLI wrapper；
- 转码工具；
- 编辑器；
- 视频播放器。

它只服务于：

> 用户已经拥有一个本地歌曲文件，我们需要正确、快速、稳定地理解它并将其解码为可播放 PCM。

---

# 3. SongCore 最小能力

第一阶段公开能力仅允许：

```text
open
probe
metadata
artwork
read_pcm
seek
close
```

概念 API：

```c
song_handle* song_open(const song_io* io);

int song_probe(
    song_handle* handle,
    song_info* out_info
);

int song_metadata(
    song_handle* handle,
    song_metadata* out_metadata
);

int song_artwork(
    song_handle* handle,
    song_blob* out_artwork
);

int song_read_pcm(
    song_handle* handle,
    float* output,
    size_t frame_capacity
);

int song_seek(
    song_handle* handle,
    int64_t position_us
);

void song_close(song_handle* handle);
```

API 设计目标不是“覆盖 FFmpeg”，而是**阻止 FFmpeg 泄漏到上层**。

以下类型不得越过边界：

- `AVFormatContext`
- `AVCodecContext`
- `AVPacket`
- `AVFrame`
- `AVDictionary`
- `SwrContext`

---

# 4. 负能力清单（Negative Capability Manifest）

默认规则：**能力默认不存在，只有真实歌曲播放需求才能将它加入 allow-list。**

| 能力 | Phase 0 |
|---|---|
| 本地歌曲 demux | ✅ |
| 音频 decode | ✅ |
| metadata read | ✅ |
| artwork extraction | ✅ |
| seek | ✅ |
| PCM 输出 | ✅ |
| sample-format conversion（packed/planar → Float32） | ✅ |
| resampling / SRC / channel rematrix | ❌（SongCore 只输出 source-rate / source-layout Float32 PCM；SRC 属 AudioEngine 层，E11 冻结决策 = aresample / libswresample，源匹配时 BYPASS，见 §9） |
| 视频 decode | ❌ |
| 视频 encode | ❌ |
| 音频 encode | ❌ |
| mux / remux | ❌ |
| transcode | ❌ |
| filter graph | ❌ |
| subtitle | ❌ |
| network | ❌ |
| streaming protocol | ❌ |
| microphone / capture | ❌ |
| camera | ❌ |
| screen capture | ❌ |
| image decode | ❌ |
| ffmpeg CLI | ❌ |
| ffprobe CLI | ❌ |
| ffplay | ❌ |

---

# 5. FFmpeg 的角色

FFmpeg 只负责：

```text
container
   ↓
compressed audio packets
   ↓
audio decoder
   ↓
PCM
```

以及：

- metadata；
- attached artwork bytes；
- duration / stream info；
- seek。

它不负责 Qianqian 的：

- ReplayGain policy（歌曲级决策在 SongCore 只读取标签，不应用）；
- spectrum / 可视化 policy；
- crossfade；
- UI effects；
- playlist；
- playback queue。

产品 DSP（EQ / volume / balance 等）由 **AudioEngine** 层承担，使用**能力裁剪的
libavfilter**（E10-C0 冻结，capability intent 由人维护、source closure 由机器
推导）。FFmpeg 在 SongCore 内只扮演 decoder；FFmpeg filter 只在 AudioEngine
以产品能力出现（见 §11）。

---

# 6. 极限裁剪目标

初始基线：

```text
libavformat
libavcodec
libavutil
```

`libswresample` **不在** SongCore decode 基线内（E07 已裁掉）。它只可能因
FFmpeg n9.0.1 某 decoder 的上游 build 依赖被强制拉入闭包（当前唯一来源：
Opus decoder）；这是 decoder implementation dependency，不是 SongCore 能力
（见 §9）。AudioEngine 层的 SRC 显式使用 `aresample` / libswresample（E11
冻结决策，见 §9）。

明确排除（SongCore decode 闭包）：

```text
libavfilter
libswscale
libavdevice
ffmpeg
ffprobe
ffplay
```

`libavfilter` 不作为 SongCore 的一部分；产品 DSP 的 libavfilter 能力闭包是
**AudioEngine 的独立裁剪闭包**（E10-C0 逐级成本证据，见 §11）。

构建从：

```text
--disable-everything
```

开始。

只按真实 corpus 打开必要的：

```text
demuxer
decoder
parser
```

原则上：

```text
--disable-protocols
```

并通过 host 提供的 `AVIOContext` 读取数据。

这样：

- Desktop 文件系统由 host 管；
- Android URI / SAF 由 host 管；
- iOS security-scoped resource 由 host 管；
- WASM buffer / host IO 由 host 管。

FFmpeg 只看到字节，不获得网络或平台文件系统权限。

---

# 7. 格式路线

## Core Common Formats（当前核心，issue #8 / E08 已落地）

```text
MP3
FLAC
AAC / M4A
raw ADTS AAC
ALAC / M4A
PCM WAV（u8 / s16le / s24le / s32le / f32le / f64le）
Ogg Vorbis
Ogg Opus
```

验收：

```text
open
→ metadata
→ artwork
→ read PCM
→ seek
→ EOF
```

## Future compatibility（不在 Common Formats 核心内，issue #9）

```text
APE
WMA
AIFF
WavPack
```

格式加入顺序由真实音乐库需求决定，不由 FFmpeg feature list 决定；
每一项都必须先回答"没有它，哪一首正常歌曲播不了"。

---

# 8. Audio Quality 原则

## 8.1 默认透明路径

当用户没有启用任何音效时：

```text
Song
 ↓
Decode
 ↓
PCM
 ↓
必要格式适配
 ↓
AudioSink
```

不得偷偷经过：

- EQ；
- ReplayGain；
- limiter；
- crossfade；
- reverb；
- volume normalization。

原则：

> **No processing unless necessary.**

---

# 9. SongCore 不提供 resample —— 与 libswresample 的关系

SongCore 的 PCM 契约是：

```text
SongCore
→ source-rate / source-layout Float32 PCM
```

SongCore **不提供** SRC / sample-rate conversion / channel rematrix 能力。
sample format 统一（packed/planar → interleaved Float32）属于契约内转换，
sample rate 与 channel layout 一律保持 source 原样；设备侧适配由 host 的
AudioSink 能力协商完成。

`libswresample` 出现在 SongCore 构建闭包中的唯一原因：FFmpeg n9.0.1 的
Opus decoder 在 upstream configure 图中硬依赖 swresample（`Disabled
opus_decoder ... not all dependencies are satisfied: swresample`）。这是
**decoder implementation dependency**，不代表 SongCore 获得 resample 能力；
其真实成员 pull 由 link audit 计量（E08 实测）。

不要把 implementation dependency 写成 product capability。

## E11 冻结：SRC 决策（2026-09-01）

- **SongCore 输出不变**：source-rate / source-layout interleaved Float32；
  SongCore 不 resample / rematrix。
- **AudioEngine SRC**：`aresample`（libswresample）。源 rate/layout 与设备
  目标一致时 **BYPASS**（不进入 resampler）。
- 不使用 SoXR / r8brain / libsamplerate / 第二个 EQ 库。
- 证据：`bench/results/songcore-v1/dsp-src-integration.json`（44.1k→48k 经
  aresample 的 negotiated rate + duration ratio 机器门）。

---

# 10. 内部 PCM

DSP 路径统一使用：

```text
Float32 PCM
```

原因：

- 适合 EQ / gain / FFT；
- headroom 更充足；
- 避免多次整数截断；
- 各平台 AudioSink 普遍可较自然地接入；
- 容易 SIMD 优化。

若最终设备必须使用整数 PCM：

```text
Float32 → 最后一步量化 → device
```

需要降位深时才考虑 dithering。

---

# 11. DSP 与 Decode 必须分层

AudioCore：

```text
File
 ↓
SongCore
 ↓
PCM
```

DSP：

```text
PCM
 ↓
ReplayGain
 ↓
EQ
 ↓
Balance
 ↓
Peak guard (optional)
 ↓
Spectrum tap
 ↓
AudioSink
```

## E11 冻结：DSP 决策（2026-09-01）

- **AudioEngine** 的产品 DSP 使用**能力裁剪的 libavfilter**（E10-C0 证据：
  capability intent 由人维护于 `ffmpeg/capabilities/dsp.json`，source closure
  由 `tools/pcm_c0.py` + pinned configure oracle 机器推导，逐级成本在
  `bench/results/avfilter-minimize/*`）。
- **SongCore 永不运行 libavfilter**：`songcore_ffmpeg.c` 不链接
  avfilter 能力，ABI 上无任何 filter 概念（见 §9）。
- 第一版 DSP 能力集（E10-C0 tier F1 "core-gain-eq-tone"）即覆盖 §12 列表：
  `volume` / `equalizer`（biquad 族）/ `bass` / `treble` / `lowshelf` /
  `highshelf` / `lowpass` / `highpass`；格式适配 = `aresample`（发现式，
  非预付）。
- 不做薄 DSP 对比层、不做第二个 EQ 库（JUCE / KFR / 等）、不做
  crossfade / limiter 之外的额外效果实现。
- 证据：`bench/results/songcore-v1/dsp-src-integration.json`（BYPASS 透明
  形状 + volume/equalizer 有效性机器门）。

---

# 12. 第一版 DSP

播放器阶段第一批 DSP 仅考虑：

- volume / gain → libavfilter `volume`；
- ReplayGain → SongCore 只读标签（E11 已冻结 AV_PKT_DATA_REPLAYGAIN 解析，
  microbels + peak），应用属 AudioEngine 策略；
- 10-band EQ → libavfilter biquad 族（`equalizer` 等）；
- balance → libavfilter `pan`（F3 层能力）；
- spectrum tap → AudioEngine / consumer 侧，不进入 SongCore。

其他音效未来以插件方式加入，不进入 SongCore。

实现载体 = AudioEngine 的能力裁剪 libavfilter 闭包（§11 E11 冻结决策），
不是薄 DSP 自研层。

---

# 13. 音质验证

“音质好”不能依赖主观描述。

项目必须验证：

## Decode correctness

对 lossless 格式：

```text
reference decode
vs
SongCore decode
```

比较：

- frame count；
- channel count；
- sample rate；
- PCM checksum（在可严格等价时）；
- 或 Float PCM tolerance。

## DSP bypass

当：

```text
EQ = flat
ReplayGain = off
Balance = center
```

时应真正 bypass DSP。

目标：

```text
input PCM == output PCM
```

而不是“把 EQ 各段设成 0 dB 后仍然走完整滤波链”。

## EQ correctness

使用：

- sine sweep；
- impulse；
- white/pink noise；
- fixed test tones。

验证：

- frequency response；
- requested gain；
- clipping；
- numerical stability。

---

# 14. Native First，WASM 第二步

Phase 0 不同时把所有路线都做完。

## Step 1：Native

先验证：

```text
KMP/host
 ↓
tiny C ABI
 ↓
trimmed native FFmpeg
```

完成：

- MP3 / FLAC；
- corpus；
- benchmark；
- audio quality tests；
- Windows baseline。

## Step 2：WASM

只有 Native baseline 稳定后，再实现：

```text
same SongCore contract
 ↓
FFmpeg WASM
```

WASM 不改变上层接口。

---

# 15. WASM 的研究问题

WASM 不是“更现代所以更好”。

要回答：

- runtime 增加多少 binary size？
- cold start 是否明显变慢？
- decode throughput 差多少？
- seek latency 差多少？
- PCM copy 是否成为瓶颈？
- memory overhead 多大？
- host IO 是否干净？
- Android/iOS 集成是否比 native 更复杂？
- sandbox / crash containment 是否真的带来价值？
- 是否更容易分发单一 codec artifact？
- 是否为未来 Web 复用提供足够收益？

---

# 16. 不重新发明别人已经做好的工作

Phase 0 明确把以下项目作为 reference，而不是架构权威：

- FFmpeg build system；
- KiteCodec 的 KMP ↔ FFmpeg interop / packaging；
- FFmpegKitNext 的跨平台构建经验；
- libav.js 的模块化 WASM variant 思路；
- ffmpeg.audio.wasm 的 audio-only 裁剪证明。

原则：

> **能偷构建经验，不偷不需要的抽象。**

我们不再造一个“通用 KMP FFmpeg wrapper”。

---

# 17. 项目成功标准

Phase 0 成功不是 Feature 多。

而是：

### S0

MP3 / FLAC：

```text
open → metadata → PCM → seek → EOF
```

全部稳定。

### S1

裁剪 profile 有完整 manifest。

任何 enable 的 FFmpeg component 都能解释：

> 哪一类真实歌曲需要它。

### S2

无 DSP decode 路径通过透明性测试。

### S3

同一 corpus 可持续 regression。

### S4

有可靠 baseline：

- binary size；
- compressed size；
- cold start；
- decode speed；
- seek latency；
- RSS；
- peak memory；
- CPU。

### S5

WASM 后端使用相同 contract 完成对比。

### S6

根据数据做出明确决策：

```text
Native only
Native + WASM experimental
或
WASM eligible for production
```

---

# 18. 第二阶段：千千·现代播放器

只有 SongCore 基线稳定后，才建立产品：

```text
Qianqian Modern
│
├── Media Library
├── Playlist
├── PlayerStore
├── Lyrics
├── Skin
├── Spectrum UI
├── EQ UI
├── Desktop multi-window
└── Mobile UI
        │
        ▼
     SongCore
```

播放器不依赖 FFmpeg API。

播放器只依赖 SongCore contract。

---

# 19. 产品哲学

整个音频层坚持一句话：

> **Decode faithfully. Process explicitly. Output minimally.**

中文：

> **忠实解码，显式处理，最短输出路径。**

而整个项目坚持另一条：

> **没有真实播放需求证明其必要性的能力，不进入核心。**
