# Qianqian Native Audio Core

> 一个只为“本地歌曲播放”服务的极薄跨平台原生音频核心。

Qianqian Native Audio Core 是「千千·现代」的音频底层：一个小的、可复用
的 Native Audio Core，通过稳定 C ABI 暴露给任意 UI 技术。

```text
Product / UI (Kotlin / KMP / Swift / Qt / ...)
        │  stable FFI / C ABI (include/songcore.h)
        ▼
Native Audio Core
        ├── SongCore     parse · metadata · artwork · stream selection ·
        │                decode · seek · typed errors
        └── AudioEngine  SRC = aresample / libswresample
                         DSP = capability-trimmed libavfilter
        ▼
platform AudioBackend (future)
```

**不是播放器**，也不是通用 FFmpeg wrapper。第一阶段只回答一个问题：

> 如果我们只想稳定、高质量地播放本地歌曲，FFmpeg 真正不可约的能力集合是什么？

## 核心决策（已冻结）

- **Decode** = trimmed FFmpeg n9.0.1（SongCore，输出 source-rate Float32
  interleaved PCM + metadata + artwork）。
- **SRC** = aresample / libswresample，源匹配时 BYPASS。
- **DSP** = capability-trimmed libavfilter（AudioEngine 层，SongCore 永不
  运行）。
- **Native build** = Xmake；UI/产品构建独立（Gradle / KMP），只通过 ABI
  接入。

## 构建与回归

```bash
xmake ffmpeg-import                 # 一次性：解析 FFmpeg source closure
xmake f -m release                  # native session
xmake build songcore                # 静态 + 动态两种产物
                                    #   build/artifacts/libsongcore.a
                                    #   build/artifacts/shared/libsongcore.so

python3 tests/songcore/regression.py            # 主回归 + 权威树
python3 tests/songcore/regression.py --check    # 只读 fail-closed 校验
python3 tests/songcore/sanitizers.py            # ASan/UBSan/leak 密度
python3 tests/songcore/dsp_src.py               # DSP/SRC 集成 smoke
```

详见 [`tests/songcore/README.md`](tests/songcore/README.md)。

## 快速开始：像外部调用者一样使用产物

```bash
xmake ffmpeg-import                     # 一次性：FFmpeg source closure
xmake build songcore_shared             # build/artifacts/shared/libsongcore.so
python3 tools/songcore_ffi_smoke.py song.flac          # 解码验收（仅标准库）
python3 tools/songcore_ffi_smoke.py --play --seconds 5 song.flac   # 可听验收
```

只用 Python 标准库 ctypes 直连 `libsongcore.so / songcore.dll`，覆盖
open/probe/metadata/artwork/decode/seek/close 与 typed-error 契约；支持
FLAC / MP3 / AAC(M4A) / ADTS / ALAC / WAV / Ogg Vorbis / Opus。
静态归档外部消费者见
[`tests/consumer/songcore_static_smoke.c`](tests/consumer/songcore_static_smoke.c)，
WASM 独立宿主见 `tools/songcore_wasm_smoke.py`。已验证消费者矩阵：
[`bench/results/songcore-v1/ffi-consumers.json`](bench/results/songcore-v1/ffi-consumers.json)。

## 文档导航

| 想了解 | 读 |
|---|---|
| 架构（Audio Core 是什么） | [docs/audio-core.md](docs/audio-core.md) |
| FFmpeg 裁剪 / 构建 / target recipe | [docs/ffmpeg-minimization.md](docs/ffmpeg-minimization.md) |
| **怎么调用这个库（调用方 API 文档）** | [docs/songcore-api.md](docs/songcore-api.md) |
| WASM 结论 | [docs/wasm.md](docs/wasm.md) |
| 产品需求权威 | [PRD.md](PRD.md) |
| 决策由来 | [docs/history.md](docs/history.md) |
| 明确不做清单 | [docs/architecture/negative-capability-manifest.md](docs/architecture/negative-capability-manifest.md) |

## 明确不做（Phase 0）

- UI / Compose / playlist / 媒体库 / 歌词 / 皮肤
- 均衡器 UI / 转码 / 编码 / 导出 / 网络流媒体
- WASAPI / CoreAudio / AAudio 等音频后端（AudioBackend 属下一阶段）
- 通用 FFmpeg CLI wrapper

## 第一条规则

> **任何新增能力都必须回答：没有它，哪一首正常歌曲播不了？**

答不上来，就不进入 SongCore。
