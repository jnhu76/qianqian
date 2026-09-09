---
title: FFmpeg 最小化
status: HISTORICAL_EVIDENCE
---

# FFmpeg 最小化

<StatusBadge status="HISTORICAL_EVIDENCE" />

## 一个音乐播放器到底需要多少 FFmpeg?

---

## 01 问题

Qianqian 已声明的能力所需的最小 FFmpeg 闭包是什么?

---

## 02 基线

典型的 FFmpeg 集成会引入整个 FFmpeg 库。Qianqian 只需要其中解码与未来处理所需的一个子集。

---

## 03 假设

最小闭包可以通过机器推导的 oracle 方法确定:

1. 声明能力意图(需要哪些解码格式)
2. 用 FFmpeg configure 作为 oracle 确定所需组件
3. 从 oracle 输出推导最小闭包
4. 构建可复现的 manifest
5. 验证产品仅用已声明的闭包即可工作

---

## 04 方法

### 能力意图

Qianqian 声明两个共享同一 FFmpeg 闭包的能力:

| 能力 | 数据流 |
|------|--------|
| Decoder | 编码媒体 → PCM |
| Processing | PCM → PCM |

### 以 Configure / Import 为 Oracle

FFmpeg 的 configure 系统被用作 oracle,确定已声明的编解码集需要哪些组件。这个 oracle 回答的问题:"给定这些解码需求,最小构建是什么?"

### 机器推导的闭包

闭包由机器推导 —— 不靠手改构建文件。建立了两个构建 profile:

| Profile | 用途 |
|---------|------|
| `codec-base` | 核心解码能力 |
| `songcore-test` | 扩展测试覆盖 |

### Manifest 重放

manifest 记录精确的闭包组成。构建重放证明闭包可复现。

---

## 05 证据

<ClaimBadge role="evidence" />

> **HISTORICAL PLAYBACK-REFERENCE-V1 EVIDENCE** —— 本节一切内容都是一项已冻结实验的记录(FFmpeg `n9.0.1`,Rust 之前的原生垂直切片)。它**不是**对今天 Rust 版 Qianqian 二进制的陈述——后者不含 FFmpeg。已于 2026-09-07 对照 issue #2 与 `playback-reference-v1` tag 核验。

### 实验范围

| 项目 | 值 |
|------|-----|
| FFmpeg | `n9.0.1`(tag → commit `bf1b838f…`,zip sha256 固定于 `bench/ffmpeg-pin.json`) |
| 能力 | Stage-A 本地播放:MP3 + FLAC 的探测 / 元数据 / 封面字节 / 解码 / seek / EOF |
| IO 模型 | 仅宿主自有 IO —— 宿主回调之上的自定义 `AVIOContext`;FFmpeg 内部无协议、无文件系统、无网络权限 |
| 语料 | `stage-a-v1`:15 个确定性合成 fixture(8 MP3、7 FLAC),sha256 固定 |
| 构建基线 | gcc 15.2.0,Linux x86_64,`--disable-autodetect`(零外部库),所有 profile 均 `--disable-x86asm` |

### N0 → N3 / N3-noswr 阶梯

| Profile | 静态库 (B) | strip+xz 库 (B) | 链接后 bench strip (B) | 符号数 |
|---------|----------------:|---------------------:|--------------------------:|--------:|
| N0 full | 39,581,790 | 10,339,040 | 20,515,208 | 56,272 |
| N1 audio | 12,858,160 | 3,738,108 | 8,187,864 | 21,702 |
| N2 stage-a | 2,741,332 | 694,684 | 1,042,680 | 5,057 |
| N3 min | 2,741,332 | 694,684 | 1,042,680 | 5,057 |
| N3 noswr | 2,549,130 | 645,620 | 911,608 | 4,714 |

静态库约缩减 14.4×(N0→N2/N3),可分发制品约缩减 14.9×;N3 与 N2 尺寸相等是因为删除实验证明 N2 集已不可再删(下文),而非裁剪停止。

按库拆分(N0 vs N3):`libavcodec.a` 21,879,630 B → 791,850 B(477 个解码器 → 2);`libavformat.a` 5,832,278 B → 484,132 B(359 个 demuxer → 2);`libavutil.a` 两者均约 1.27 MB(基础库,几乎不可再删);`libavfilter.a`/`libswscale.a`/`libavdevice.a`(7.98 MB + 2.32 MB + 98 KB)自 N2+ 起整体移除。

### 正确性门槛

15 个 fixture × 5 个 profile = 75 次运行;每个 profile:**12 PASS + 3 DEGRADED-PASS + 0 FAIL**(合计 60 pass / 15 degraded / 0 fail)。失败必须分类(`open_failed / probe_failed / decode / seek / pcm / metadata / artwork / timeout / crash`),不允许裸 PASS/FAIL。

| 门槛 | 结果 |
|------|------|
| FLAC 严格 PCM | 所有 FLAC fixture 与参考 PCM 逐字节一致(对规范化 Float32 交错字节取 sha256) |
| MP3 跨 profile 一致性 | 全部 14 个可比 fixture 在 N0/N1/N2/N3/noswr 下 PCM 一致(1 个截断头 fixture 被所有 profile 在打开时拒绝) |
| Seek 证明 | FLAC 25%/50%/75% seek 恢复到 ≤ 目标的帧边界;seek 后后缀解码与顺序解码**逐字节一致**(如 flac-16-44-stereo N3:1.0 s → 41472 样本,2.0 s → 87552,3.0 s → 129024,全部逐字节一致,亚毫秒) |
| EOF 纪律 | 干净的双侧到达(demux + 解码器 `AVERROR_EOF`);截断/损坏 fixture 无崩溃 |
| 组件溯源 | demuxer:mp3、demuxer:flac、decoder:mp3float、decoder:flac → **必需**(删除会破坏 fixture);parser:mpegaudio、parser:flac → **configure 必需**(依赖图强制;删除是 no-op) |

值得注意的机器事实:实际解析出的解码器是 `mp3float`(不是定点别名 `mp3`);在 N2/N3 中启用 `mp3` 会产生与 N0/N1 不同的 PCM,一致性门槛会(正确地)失败。

### 性能

最小化**没有**造成有意义的解码吞吐回退,余量始终非常大。中位 ×realtime(`songcore-output` 路径;预热 1 + 5 轮):

| 样本 | N0 | N3 | N3 noswr |
|------|-----:|-----:|-----:|
| MP3 CBR | 1948× | 1753× | 1664× |
| MP3 VBR | 2250× | 2032× | 2234× |
| MP3 长曲 | 1942× | 1782× | 1961× |
| FLAC 16/44 | 1097× | 1063× | 1151× |
| FLAC 24/96 | 428× | 412× | 498× |

所有样本保持在 400× realtime 以上(最重:flac-24-96 ≈ 410×)。N0/N3 波动相互重叠;无系统性回退。转换+输出相对裸解码的开销约 5–20%,绝对量为微秒/毫秒级。冷打开(open+probe)在裁剪后反而变快(N0 0.25–0.57 ms → N2/N3 0.12–0.45 ms)。

### libswresample 结果

**部分可绕过 —— 范围狭窄。** 对于 Stage-A 契约(源采样率 / 源布局的 Float32 交错输出),libswresample 只执行格式 + planar→interleaved 转换(无重采样、无 rematrix),SongCore 自有的转换路径**逐字节一致(14/14)**,同时省下 192 KiB 静态库 / 131 KiB 链接产物。

这**只**证明了 libswresample 对那个狭窄的 Stage-A 契约可绕过(上文 N3-noswr)。它**没有**证明重采样或 rematrix 永远不会被需要,也**没有**确定这样的阶段必须用什么实现:如果设备采样率适配或 rematrix 成为需求,Qianqian 届时需要引入并验证合适的重采样/rematrix 阶段 —— 该实验对"libswresample 是否是唯一有效选择"没有给出任何结论。

---

## 06 结果

<ClaimBadge role="evidence" />

Decoder 与 Processing 共享**唯一的 FFmpeg 闭包权威**。关键发现:

- 编解码覆盖是**提供者配置**,不是运行时层
- 按 codec 拆分 Decoder 插件(MP3/FLAC 各一个插件)会把唯一的闭包权威拆开,却**换不来任何可组合性收益**
- 闭包可通过 oracle 方法最小化
- 两个构建 profile 证明方法可复现

**机器制品:** 保存于 `playback-reference-v1` tag(不在 main 上)。见该 tag 上的 `native/ffmpeg/profiles/*.json`、`native/ffmpeg/capabilities/*.json`、`bench/results/`。

**冻结 tag 上的后续延续:** 同一方法在 SongCore-v1 时代作为能力阶梯延续(MP3+FLAC → +AAC → +ALAC → +WAV → +Vorbis → +Opus → 最小化常见格式,见该 tag 的 `bench/results/common-formats/ladder.md`,最终最小化 `.so` strip 后 1.25 MiB / strip+xz 后 492 KiB)。那是同一 tag 上的冻结证据,不是当前代码的真相。

---

## 07 架构后果

<ClaimBadge role="authority" />

证据来源：`component-boundary-a0.md`（历史 #53 审计证据；其中 #48 FFmpeg 闭包行为事实仍然有效）：

> 当 FFmpeg 重新引入时,Decoder/Processing 必须继续共享唯一的 FFmpeg 闭包权威,而不是复制依赖。

这是**边界级架构约束**,不只是构建优化。共享闭包是组件契约的一部分。

---

## 开放问题

- 共享闭包权威在未来的插件/组合系统中应如何表达?
- 首个真正的 Decoder 实现所需的最小 FFmpeg 构建旗标集是什么?
- 闭包能否在 CI 中自动验证?

---

<ProvenancePanel
  :authority="['docs/architecture/overview.md']"
  :evidence="['docs/architecture/component-boundary-a0.md', 'research/playback-reference-v1']"
  :decisions="[{ issue: 2 }, { issue: 3 }, { issue: 48 }, { issue: 53 }]"
  last-verified="issue #2 + playback-reference-v1 tag, 2026-09-07"
/>
