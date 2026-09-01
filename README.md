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
xmake f -o build/xmake              # native session
xmake build songcore songcore_probe # 核心库 + 回归 instrument

python3 tests/songcore/regression.py            # 主回归 + 权威树
python3 tests/songcore/regression.py --check    # 只读 fail-closed 校验
python3 tests/songcore/sanitizers.py            # ASan/UBSan/leak 密度
python3 tests/songcore/dsp_src.py               # DSP/SRC 集成 smoke
```

详见 [`tests/songcore/README.md`](tests/songcore/README.md)。

## 文档

- [PRD](PRD.md) — 产品需求权威。
- [docs/audio-core.md](docs/audio-core.md) — Native Audio Core 唯一人类权威。
- [docs/ffmpeg-minimization.md](docs/ffmpeg-minimization.md) — FFmpeg 裁剪工作流。
- [docs/wasm.md](docs/wasm.md) — WASM 结论（native-first 默认）。
- [docs/history.md](docs/history.md) — 决策由来（极简）。
- [docs/architecture/negative-capability-manifest.md](docs/architecture/negative-capability-manifest.md) — 明确不做清单。

## 明确不做（Phase 0）

- UI / Compose / playlist / 媒体库 / 歌词 / 皮肤
- 均衡器 UI / 转码 / 编码 / 导出 / 网络流媒体
- WASAPI / CoreAudio / AAudio 等音频后端（AudioBackend 属下一阶段）
- 通用 FFmpeg CLI wrapper

## 第一条规则

> **任何新增能力都必须回答：没有它，哪一首正常歌曲播不了？**

答不上来，就不进入 SongCore。
