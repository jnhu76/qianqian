# E01 — FFmpeg Minimal Song Playback Profile

## Hypothesis

只为本地歌曲播放时，FFmpeg 可以从通用多媒体框架裁成很小的不可约集合。

## Baseline

从：

```text
--disable-everything
```

开始。

候选保留：

```text
libavformat
libavcodec
libavutil
libswresample
```

明确不启用：

```text
programs
network
devices
encoders
muxers
video decoders
subtitles
avfilter
swscale
postproc
```

## Stage A

仅支持：

```text
MP3
FLAC
```

要求：

- probe；
- metadata；
- artwork bytes；
- decode；
- seek；
- EOF。

## 实验方法

1. 建立 MP3/FLAC corpus。
2. 编译最小 profile。
3. 运行所有 corpus。
4. 每次删除一个 FFmpeg component。
5. 若 corpus 仍通过，保持删除。
6. 若失败，记录失败样本与 component necessity。
7. 输出 `enabled-components.txt`。

## 每个 component 必须有 provenance

示例：

```text
mp3 demuxer
  required_by:
    corpus/mp3/id3v24-vbr.mp3
    corpus/mp3/id3v23-cbr.mp3

flac decoder
  required_by:
    corpus/flac/16bit-44k.flac
    corpus/flac/24bit-96k.flac
```

禁止：

```text
“以后可能有用，所以留着”
```

## Metrics

- static library size；
- linked binary size；
- compressed package size；
- symbols count；
- startup time；
- MP3 realtime factor；
- FLAC realtime factor；
- peak memory。

---

# 第一轮结果（Phase 0 Step 1，2026-08-29）

完整数据：`bench/results/baseline/`；provenance：`bench/provenance/component-provenance.json`。
复现：`scripts/bench-native`。

## 条件

- FFmpeg `n9.0.1`（commit `bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa`，zip sha256 已 pin）；
- gcc 15.2.0，Linux x86_64，release-static，全部 profile 统一
  `--disable-autodetect`（零外部库）与 `--disable-x86asm`（构建机无 nasm；
  统一关闭以保持比较公平）；
- corpus `stage-a-v1`：15 个确定性合成 fixture（CBR/VBR、ID3v2.3/2.4、
  unicode、APIC、mono/stereo、16/24bit、44.1/96k、截断/损坏头尾）。

## Size ladder（bytes）

| Profile | Static libs | Stripped+xz libs | Linked bench (stripped) | Bench xz | Symbols |
|---|---:|---:|---:|---:|---:|
| N0 full | 39,582,070 | 10,339,148 | 20,515,208 | 7,314,936 | 56,272 |
| N1 audio | 12,858,320 | 3,738,632 | 8,187,864 | 3,355,948 | 21,702 |
| N2 stage-a | 2,741,492 | 694,812 | 1,042,680 | 371,536 | 5,057 |
| N3 min | 2,741,460 | 694,784 | 1,042,680 | 371,604 | 5,057 |
| N3 noswr | 2,549,250 | 645,720 | 911,608 | 330,792 | 4,714 |

可复现性：两次独立 clean-room 构建（重新下载源码 → configure → make）中，
static libs、linked/stripped bench、symbols 全部**逐字节一致**；仅
stripped 库的 xz 尺寸有 ≤64B 抖动（ar 成员时间戳进入压缩流，派生测量）。

- N0→N2：static libs **14.4×** 缩减；可分发 artifact（stripped+xz）**14.9×**。
- N1→N2：static libs **4.7×**（允许列表相对"音频全家桶"再省 79%）。
- N3 与 N2 同体积：说明 N2 的 allow-list 在 corpus 证据下已不可再删
  （见 provenance），"最小可辩护 profile"就是 N2 的集合。

## Correctness

每个 profile × 15 案例：**60 pass / 15 degraded / 0 fail**。
degraded 全部是病态 fixture（corrupt-tail、truncated-header、flac-truncated）
的预期受限行为：有界失败、无 crash、无 timeout。

- FLAC 严格 PCM：canonical Float32-interleaved sha256 与 manifest 相等；
- seek（25/50/75%）：后向 seek 恢复点 ≤ 目标且 suffix 与顺序解码逐字节一致（FLAC）；
- metadata / artwork：与 manifest 一致（APIC 与 METADATA_BLOCK_PICTURE 均提取）；
- EOF：demux 与 decoder 双侧干净到达；
- 跨 profile PCM：14/14 可比较案例完全一致（含 MP3）。

## Throughput（x realtime，median of 5，warm-up 1）

| Profile | MP3 CBR | MP3 VBR | MP3 long | FLAC 16/44 | FLAC 24/96 |
|---|---:|---:|---:|---:|---:|
| N0 | 2049 | 2277 | 1943 | 1098 | 485 |
| N1 | 1990 | 2214 | 1950 | 1121 | 462 |
| N2 | 1910 | 2244 | 1968 | 1067 | 487 |
| N3 | 1993 | 2029 | 1887 | 1133 | 497 |
| N3 noswr | 2045 | 1967 | 1972 | 1121 | 499 |

极限裁剪没有带来性能回退；各 profile 差异在轮次噪声内
（同 fixture 跨 profile 波动 ±10% 以内，远高于 profile 间系统性差异）。

## N3 component provenance（删除实验）

| Component | 结论 | 证据 |
|---|---|---|
| demuxer:mp3 | required | 删除后全部 mp3 案例失败 |
| demuxer:flac | required | 删除后全部 flac 案例失败 |
| decoder:mp3float | required | 删除后全部 mp3 解码失败 |
| decoder:flac | required | 删除后全部 flac 解码失败 |
| parser:mpegaudio | configure-required | `mp3_demuxer_select="mpegaudio_parser"`：allow-list 删除后 configure 仍强制启用 |
| parser:flac | configure-required | `flac_demuxer_select="flac_parser"` |

两个事实值得记录：

1. `decoder:mp3float`（而非 `decoder:mp3` 固定点别名）是完整构建中
   `avcodec_find_decoder(AV_CODEC_ID_MP3)` 实际选中的实现；N2/N3 只启用
   它即可与 N0/N1 的 PCM 保持一致。
2. id3v2/apetag/replaygain/FLAC picture 是 demuxer 的内部模块，不是
   可独立关闭的 component，随 demuxer 进入（machine truth 见
   `meta/enabled-components.txt`）。

## libswresample 判定：partially bypassable

- `n3-min` vs `n3-min-noswr`：PCM **逐字节一致**（14/14 案例）——
  SongCore 若自备"格式转换 + planar→interleaved"的固定小函数，
  输出与 swr 完全相同；
- 代价：swr 在 static libs 中占 **192 KiB**，linked stripped **131 KiB**；
- 转换开销（decode-core vs songcore-output）：6–16%（在 500–2300×
  realtime 的基数上，绝对值可忽略），swr 与手写转换无性能差异；
- 未被 Stage A corpus 触及的 swr 能力：sample-rate conversion、
  channel rematrix（PRD §9 的"必要 resampling"场景）。

结论：对 Stage A contract（源码率/源声道数的 Float32 interleaved），
swresample **可旁路**；若需要设备采样率适配则**必须保留**。是否在产品
中旁路是 SongCore 边界决策，不属于本 profile 裁剪的范围。

## 局限（evidence gaps）

- corpus 为合成信号；真实音乐兼容性（free-format MP3、非常规 block size、
  混合标签风格等）尚未进入 corpus —— 对应 `docs/testing/audio-corpus.md`
  的本地私有库测试条款；
- 单一编译器/平台（gcc 15 / Linux x86_64）；MSVC/Clang、Windows 体积
  结论待 Step 1 Windows baseline；
- 未尝试 `--enable-small`/LTO 等编译优化维度（属于 build profile 实验，
  不属于组件裁剪）。
