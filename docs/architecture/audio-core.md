# Native Audio Core architecture

> Purpose: 一句话——为什么存在 Native Audio Core：为一个目的（本地歌曲
> 播放）提供一个小的、可复用的原生核心，通过稳定 C ABI 暴露给任意 UI 技术。
> Scope: SongCore / AudioEngine / AudioBackend 三个核心角色的职责与边界。
> 运行时组成细节见 [player-runtime.md](player-runtime.md)；normative 行为
> 见 [contracts/](../contracts/)。

UI technology is replaceable; the Native Audio Core is a small, reusable
library behind a stable C ABI.

## Responsibility

```text
Product / UI (Kotlin / desktop app)
        │  stable FFI / C ABI (song_* + pe_*, one qianqian runtime library)
        ▼
Player Runtime (PlayerEngine + AudioBackend)
        │  frozen SongCore contract (source-rate Float32 PCM)
        ▼
Native Audio Core
        ├── SongCore     parse · metadata · artwork · stream selection ·
        │                decode · seek · typed errors
        └── AudioEngine  SRC = aresample / libswresample (BYPASS when matching)
                         DSP = capability-trimmed libavfilter (opt-in)
```

## Components

### SongCore

```text
local byte source
  → parse container
  → select audio stream
  → metadata / artwork
  → decode
  → source Float32 PCM
```

- Output: **Float32, interleaved, source sample rate, source channel
  layout** — always, for every codec（normative 契约见
  [contracts/songcore-api.md](../contracts/songcore-api.md)）。
- Decode backend: **trimmed FFmpeg n9.0.1**（pin: `native/ffmpeg/pin.json`，
  capability intent: `native/ffmpeg/capabilities/songcore.json`，方法见
  [ffmpeg-minimization.md](ffmpeg-minimization.md)）。
- SongCore does **not** resample, rematrix, run DSP/EQ/ReplayGain,
  normalize, limit, crossfade, or touch a device（决策：
  [ADR-0002](../adr/0002-songcore-src-boundary.md)）。
- Host I/O is fully caller-provided (`song_io` read/seek/size): no
  filesystem, network, or URI semantics inside SongCore.

### AudioEngine

Optional, post-decode PCM processing（决策：
[ADR-0003](../adr/0003-dsp-libavfilter-audioengine.md)）：

```text
source format == required output format  →  BYPASS (no work)
otherwise                                →  aresample / libswresample
```

DSP is a capability-trimmed libavfilter graph (intent:
`native/ffmpeg/capabilities/dsp.json`): volume/preamp, parametric/graphic EQ,
tone, filters, and graph plumbing. Processing is opt-in per capability;
nothing runs unless a pipeline asks for it. ReplayGain 应用属 AudioEngine
策略（SongCore 只读取标签——metadata 中的 microbels + peak）。

### Data representation

DSP 路径统一使用 **Float32 PCM**：适合 EQ / gain / FFT；headroom 更充足；
避免多次整数截断；各平台 AudioSink 普遍可较自然地接入；容易 SIMD 优化。

若最终设备必须使用整数 PCM：

```text
Float32 → 最后一步量化 → device
```

需要降位深时才考虑 dithering。

### AudioBackend

Platform output layer: device negotiation, device buffering, clock, and
actual audio output. Production implementation today: the Windows WASAPI
renderer inside the qianqian runtime（见
[platform-audio.md](platform-audio.md)）；tests 用 in-process
NullAudioBackend 驱动同一 seam。

## Boundaries

- UI / application 层（Kotlin/KMP/Compose/Swift/Qt/etc.）不属于 Native
  Audio Core，只通过稳定 C ABI（`songcore.h` / `player_engine.h`）经
  JNI / JNA / cinterop 访问核心。FFMpeg 类型永不穿越边界
  （[contracts/ffi-boundary.md](../contracts/ffi-boundary.md)）。
- 明确不做的能力清单：
  [negative-capability-manifest.md](negative-capability-manifest.md)。

## Frozen implementations

| Concern | Decision | ADR |
|---|---|---|
| Decode | trimmed FFmpeg n9.0.1 (SongCore) | [ADR-0004](../adr/0004-ffmpeg-machine-derived-closure.md) |
| DSP | capability-trimmed libavfilter (AudioEngine) | [ADR-0003](../adr/0003-dsp-libavfilter-audioengine.md) |
| SRC | aresample / libswresample, conditional BYPASS | [ADR-0002](../adr/0002-songcore-src-boundary.md) |
| Runtime strategy | native-first; WASM measured, future target | [ADR-0001](../adr/0001-native-first-wasm-viable.md) |
| Native build | Xmake (`native/xmake.lua` + `native/build/`) | — |
| Public ABI | `native/include/songcore.h` v1, `native/include/player_engine.h` v1 | — |

## Dependencies

- pinned upstream FFmpeg 的机器推导最小 source closure
  （[ffmpeg-minimization.md](ffmpeg-minimization.md)）；
- Qianqian 自有代码；production decode 路径不依赖任何第三方 runtime 层。

## Invariants

- 无 DSP 请求时路径透明：decode → PCM → sink，没有任何隐藏处理。
- FFmpeg 只扮演 decoder / metadata 来源；filter 能力只存在于 AudioEngine。
- 核心不获得网络或平台文件系统权限——FFmpeg 只看到 host 提供的字节。

## Related contracts

- [contracts/songcore-api.md](../contracts/songcore-api.md) — SongCore
  caller 契约（PCM / 错误 / lifetime）。
- [contracts/player-api.md](../contracts/player-api.md) — PlayerEngine
  行为契约。
- [contracts/ffi-boundary.md](../contracts/ffi-boundary.md) — application
  ↔ native runtime 边界。
