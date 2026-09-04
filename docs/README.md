# Qianqian documentation router

本文件是 documentation router，不是第二份 architecture 文档：它只回答
“当前任务应该读哪几份文档”。

规则：**不要递归通读 `docs/`**。按下面的矩阵加载最小相关集合。

文档分类、house style、canonical templates 的权威在
[standards/documentation.md](standards/documentation.md)。

## Routing matrix

| Task | Read first |
|---|---|
| 产品行为 | `product/`（目标位置；迁移前见下方当前布局） |
| Application / UI | product + application architecture + UI standard |
| Native runtime | native-runtime architecture + 相关 contract |
| SongCore | SongCore architecture + SongCore contract |
| Build / install | `development/` |
| Testing | testing standard + 组件文档 |
| 新建长期文档 | [standards/documentation.md](standards/documentation.md) + [standards/templates/](standards/templates/) |
| Research | research 目录 + research template |
| 历史证据 | `archive/` |
| 为什么这样决策？ | `adr/` + 相关 research |

## 当前布局（迁移前的事实）

目标分类目录（`product/`、`contracts/`、`development/`、`research/`、
`archive/`）随 DOCS-IA-2 迁移逐步出现。当前 `docs/` 顶层仍是平铺文件，按
语义对应：

| 当前文档 | 语义类别 |
|---|---|
| [audio-core.md](audio-core.md) | architecture（Native Audio Core 总权威） |
| [player-engine.md](player-engine.md) | architecture（PlayerEngine 语义） |
| [songcore-api.md](songcore-api.md) | contract（SongCore caller API） |
| [songcore-release.md](songcore-release.md) | development（release 流程） |
| [ffmpeg-minimization.md](ffmpeg-minimization.md) | architecture（minimization 方法） |
| [wasm.md](wasm.md) | research（WASM 可行性证据） |
| [kmp-ffi-handoff.md](kmp-ffi-handoff.md) | contract（KMP FFI 边界，phase entry） |
| [kotlin-boundary-probe.md](kotlin-boundary-probe.md) | archive（probe 证据记录） |
| [wasapi-native-runtime-closure.md](wasapi-native-runtime-closure.md) | archive（phase 文档） |
| [history.md](history.md) | archive（决策史索引） |
| [architecture/](architecture/) | architecture |
| [adr/](adr/) | ADR |
| [testing/audio-corpus.md](testing/audio-corpus.md) | standard（corpus 原则） |

产品语义当前由 root [PRD.md](../PRD.md) 承载。
