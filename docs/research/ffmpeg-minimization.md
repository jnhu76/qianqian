# FFmpeg minimization — measured evidence

> Status: concluded
> Decision authority: no
> Related decision: [ADR-0004](../adr/0004-ffmpeg-machine-derived-closure.md)、
> [ADR-0003](../adr/0003-dsp-libavfilter-audioengine.md)

实测数据支持"机器推导最小闭包"方法。测量权威是 `bench/results/` 下的
机器文件与 `tools/measure_songcore_artifacts.py` 的输出；本文是这些证据的
摘要。方法本身见
[../architecture/ffmpeg-minimization.md](../architecture/ffmpeg-minimization.md)。

## Current SongCore ABI v1 reference artifact

Machine-measured by `tools/measure_songcore_artifacts.py` into
`bench/results/songcore-v1/reference-artifacts.json` (codec-base closure,
205 TU):

```text
libsongcore.a          raw 30,216 B · stripped 18,118 B · xz -9e 8,444 B
libsongcore.so         raw 981,832 B · xz -9e 353,492 B
shared exports         exactly 15 song_* ABI symbols, zero av_*/ff_*/swr_*
dynamic dependencies   libm, libc
```

The static library is the decoder slice the application archives; the
shared library statically contains the whole FFmpeg closure with a
15-symbol export gate (`SONGCORE_API` + hidden default visibility +
`--exclude-libs`).

## Historical stages (superseded references, kept for continuity)

- **MP3 + FLAC source-minimization stage** (`bench/provenance/`): 205-TU
  oracle closure; shipping candidate (minimal closure, -Os -flto,
  `--gc-sections`) linked stripped 530,664 B / xz 180,408 B; decode
  ≥ 762× realtime. The ASM-disable variant was **rejected**: MP3 PCM
  diverged from the SIMD kernels.
- **Common Formats envelope** (final stage `c6-so-lto`,
  `bench/results/common-formats/summary.json`): 198 TU; historical
  5-symbol shared artifact raw 1,435,336 B / stripped 1,309,520 B / xz
  503,684 B; minimum decode throughput 392.83× realtime (ALAC). This was a
  shipping-shaped measurement, not the current 15-symbol product ABI — the
  current reference is the ABI v1 measurement above.
- **libavfilter DSP closure** (`bench/results/avfilter-minimize/shipping.json`):
  F1 core-gain-eq-tone 801,008 B stripped / 270,504 B xz; full F0–F7 DSP
  envelope 1,198,320 B stripped / 389,080 B xz. The permanent DSP/SRC smoke
  is `tests/songcore/dsp_src.py`.

## Interpretation

- 裁剪收益的权威度量是最终链接产物（不是源文件计数）；ABI v1 参考产物证明
  一个完整的 Common Formats 解码核心可以收缩到亚兆字节 shared 产物且导出面
  恰好 15 个符号。
- 历史阶段保留作连续性参照；它们不是当前 product ABI 的声明。
- 失败实验（ASM-disable 的 PCM 分歧）与方法本身一样有价值：它否定了
  "再关一点 SIMD"的直觉，并确立 PCM 等价是裁剪的硬门。

## Limitations

- 全部测量基于 pinned FFmpeg n9.0.1 与当时的 toolchain；升级需按
  [../architecture/ffmpeg-minimization.md](../architecture/ffmpeg-minimization.md)
  的 upgrade flow 重新推导并重测。
- 历史叙事归档于 git history 与 [../archive/history.md](../archive/history.md)；
  机器文件是 durable authority。

## Reproduction

```bash
python3 tools/measure_songcore_artifacts.py   # 当前参考产物测量
python3 tools/dsp_closure.py --stage avf-c2   # DSP 闭包阶梯（重跑全阶梯见 tools/README.md）
```
