# CONTEXT — stable vocabulary

> CONTEXT.md 只承载稳定词汇与 repository mental model。**它不是 architecture
> 或 workflow authority**：每个术语的权威在下方链接指向的文档。

## 术语

| Term | 含义 | Authority |
|---|---|---|
| Qianqian / 千千·现代 | 长期产品：简洁、轻量、跨平台本地音乐播放器 | [PRD.md](PRD.md) |
| Qianqian Audio Lab | Phase 0 载体：Song Playback Appliance，只回答“本地歌曲播放需要的最小音频核心” | [PRD.md](PRD.md) |
| Native Audio Core | SongCore + AudioEngine，稳定 C ABI 之后的可复用原生库 | [docs/audio-core.md](docs/audio-core.md) |
| SongCore | 解码核心：parse / metadata / artwork / stream selection / decode / seek / typed errors；输出 source-rate Float32 interleaved PCM | [docs/audio-core.md](docs/audio-core.md)、[docs/songcore-api.md](docs/songcore-api.md) |
| AudioEngine | 可选的 post-decode PCM 处理（SRC / DSP）；源格式匹配时 BYPASS | [docs/audio-core.md](docs/audio-core.md) |
| PlayerEngine | SongCore 之上的播放状态机：timeline / clock / queue / epochs | [docs/player-engine.md](docs/player-engine.md) |
| AudioBackend | 平台输出层：设备协商、buffering、clock（如 WASAPI） | [docs/player-engine.md](docs/player-engine.md) |
| FFmpeg closure | pin → capability intent → configure oracle → compile manifest → Xmake replay 的机器推导最小 FFmpeg source slice | [docs/ffmpeg-minimization.md](docs/ffmpeg-minimization.md) |
| ABI | 冻结的 C 边界：`include/songcore.h`（v1）、`include/player_engine.h`（v1）；FFmpeg 类型永不穿越 | 对应 public header |
| PCM contract | SongCore 恒输出 Float32、interleaved、source sample rate、source channel layout | [docs/audio-core.md](docs/audio-core.md) |
| Corpus | 只有进入 corpus 并通过 regression 的格式才是 officially supported | [docs/testing/audio-corpus.md](docs/testing/audio-corpus.md) |
| Evidence | Native vs WASM、性能、音质、codec 支持等问题必须以 corpus / benchmark / reproducible 结果回答 | [AGENTS.md](AGENTS.md) |
| Authority | 按事实类型定权威（fact-type authority），不用全局线性排名 | [docs/standards/documentation.md](docs/standards/documentation.md) |
