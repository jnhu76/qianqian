# CONTEXT — stable vocabulary

> CONTEXT.md 只承载稳定词汇与 repository mental model。**它不是 architecture
> 或 workflow authority**：每个术语的权威在下方链接指向的文档。

## 术语

| Term | 含义 | Authority |
|---|---|---|
| Qianqian / 千千·现代 | 长期产品：简洁、轻量、跨平台本地音乐播放器 | [docs/product/product.md](docs/product/product.md) |
| Qianqian Audio Lab | 历史术语：Phase 0 的载体（Song Playback Appliance，只回答“本地歌曲播放需要的最小音频核心”）；其产出已是当前 Native Audio Core | [docs/archive/superseded/prd-phase0-qianqian-audio-lab.md](docs/archive/superseded/prd-phase0-qianqian-audio-lab.md)（historical） |
| Native Audio Core | SongCore + AudioEngine，稳定 C ABI 之后的可复用原生库 | [docs/architecture/audio-core.md](docs/architecture/audio-core.md) |
| SongCore | 解码核心：parse / metadata / artwork / stream selection / decode / seek / typed errors；输出 source-rate Float32 interleaved PCM | [docs/architecture/audio-core.md](docs/architecture/audio-core.md)、[docs/contracts/songcore-api.md](docs/contracts/songcore-api.md) |
| AudioEngine | 可选的 post-decode PCM 处理（SRC / DSP）；源格式匹配时 BYPASS | [docs/architecture/audio-core.md](docs/architecture/audio-core.md) |
| PlayerEngine | SongCore 之上的播放状态机：timeline / clock / queue / epochs | [docs/architecture/player-runtime.md](docs/architecture/player-runtime.md)、[docs/contracts/player-api.md](docs/contracts/player-api.md) |
| AudioBackend | 平台输出层：设备协商、buffering、clock（当前实现：Windows WASAPI renderer） | [docs/architecture/platform-audio.md](docs/architecture/platform-audio.md) |
| FFmpeg closure | pin → capability intent → configure oracle → compile manifest → Xmake replay 的机器推导最小 FFmpeg source slice | [docs/architecture/ffmpeg-minimization.md](docs/architecture/ffmpeg-minimization.md) |
| ABI | 冻结的 C 边界：`include/songcore.h`（v1）、`include/player_engine.h`（v1）；FFmpeg 类型永不穿越 | 对应 public header |
| PCM contract | SongCore 恒输出 Float32、interleaved、source sample rate、source channel layout | [docs/contracts/songcore-api.md](docs/contracts/songcore-api.md) |
| Corpus | 只有进入 corpus 并通过 regression 的格式才是 officially supported | [docs/standards/testing.md](docs/standards/testing.md) |
| Evidence | Native vs WASM、性能、音质、codec 支持等问题必须以 corpus / benchmark / reproducible 结果回答 | [AGENTS.md](AGENTS.md) |
| Authority | 按事实类型定权威（fact-type authority），不用全局线性排名 | [docs/standards/documentation.md](docs/standards/documentation.md) |
