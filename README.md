# Qianqian Audio Lab

> 一个只为“本地歌曲播放”服务的极薄跨平台音频实验项目。

Qianqian Audio Lab 是「千千·现代（Qianqian Modern）」的 Phase 0。

它**不是播放器**，也不是通用 FFmpeg wrapper。第一阶段只回答一个问题：

> 如果我们只想稳定、高质量地播放本地歌曲，FFmpeg 真正不可约的能力集合是什么？

项目先构建一个极小的 `SongCore` 边界：

```text
AudioSource
    │
    ▼
 SongCore
    │
    ├── probe / metadata / artwork
    ├── decode → PCM
    ├── seek
    └── close
    │
    ▼
AudioSink / benchmark / test harness
```

## Phase 0 的目标

1. 建立最小 `SongCore` API。
2. 先实现 **Native FFmpeg** 后端。
3. 用真实歌曲 corpus 驱动 FFmpeg 极限裁剪。
4. 验证无 DSP 路径的音频透明性。
5. 建立音质、seek、内存、启动、decode throughput benchmark。
6. 第二步再加入 **WASM FFmpeg** 后端，用同一 API、同一 corpus 对比。
7. 数据稳定后，再决定 Qianqian Modern 播放器如何接入。

## 明确不做

Phase 0 不做：

- Compose UI
- 播放列表
- 媒体库
- 歌词 UI
- 皮肤
- 均衡器 UI
- 视频
- 转码
- 编码
- 导出
- 网络流媒体
- FFmpeg CLI wrapper

## 第一条规则

> **任何新增能力都必须回答：没有它，哪一首正常歌曲播不了？**

答不上来，就不进入 SongCore。

## 文档入口

- [PRD](PRD.md)
- [架构边界](docs/architecture/songcore-boundary.md)
- [FFmpeg 极限裁剪实验](docs/experiments/e01-ffmpeg-minimal-profile.md)
- [Xmake selective build + audible smoke](docs/experiments/e06-xmake-selective-build-playback-smoke.md)
- [Native vs WASM](docs/experiments/e02-native-vs-wasm.md)
- [音频透明性与音质](docs/experiments/e03-audio-quality.md)
- [DSP 分层实验](docs/experiments/e04-dsp-boundary.md)
- [Codec Corpus](docs/testing/audio-corpus.md)
- [M0 决策门](docs/experiments/m0-decision-gate.md)

## 复现实验（Phase 0 Step 1 Native）

```text
scripts/bench-native
```

一条命令完成：pin 校验的 FFmpeg `n9.0.1` 下载 → N0/N1/N2/N3(+noswr)
构建 → Stage A corpus correctness/PCM/size/throughput。
结果写入 `bench/results/runs/<时间戳>/`，canonical baseline 见
`bench/results/baseline/`。需要 `bash、python3、gcc、make、unzip、xz`；
网络经代理时设置 `https_proxy`（默认尝试 `http://127.0.0.1:7897`）。

## Xmake selective-build 实验（Phase 0 Step 1.5）

E06 的目标不是再造 FFmpeg build system，而是把它降级为 **import/upgrade-time oracle**：

```text
xmake ffmpeg-import       # 上游 configure/Make 只在这里参与，生成 compile closure
xmake f -m release
xmake qianqian_av         # normal build：Xmake 直接产出单一 libqianqian_av.a
xmake qn_pcm_dump         # SongCore → Float32 PCM pipe
python3 tools/verify_xmake_core.py
```

可听 smoke（test-only sink）：

```text
python -m pip install sounddevice
python3 tools/play_smoke.py corpus/fixtures/mp3-cbr-id3v23.mp3
```

Python/sounddevice **不负责解码**、也不进入 shipping dependency graph；它只把 SongCore 已经产生的 Float32 PCM 送进系统音频设备。

在真实机器跑完 E06 gate 前，这条路径只可标记 `CODE_COMPLETE_PENDING_VALIDATION`。
