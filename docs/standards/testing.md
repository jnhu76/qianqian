# Testing standard

> Authority: Normative（工程测试标准）
> Scope: 什么算 officially supported、corpus 覆盖要求、音质验证要求与
> repository corpus 规则。具体回归命令与机器证据权威在
> [native/tests/songcore/README.md](../../native/tests/songcore/README.md) 与
> `bench/results/`。

## Supported-format principle

Codec 支持不是"FFmpeg 说支持"。

只有进入 corpus 并通过 regression 的格式才算 Qianqian officially
supported。格式加入顺序由真实音乐库需求决定，不由 FFmpeg feature list
决定；每一项都必须先回答"没有它，哪一首正常歌曲播不了"。

## Corpus coverage requirements

### MP3

至少覆盖：

- CBR；
- VBR；
- ID3v2.3；
- ID3v2.4；
- Unicode metadata；
- APIC artwork；
- missing tags；
- short file；
- long file；
- corrupt tail；
- truncated header。

### FLAC

至少覆盖：

- 16-bit / 44.1kHz；
- 24-bit / 96kHz；
- mono；
- stereo；
- Vorbis Comment；
- embedded picture；
- ReplayGain tags；
- missing metadata；
- large artwork；
- truncated stream。

### Synthetic signals

至少生成：

- silence；
- impulse；
- 100 Hz sine；
- 1 kHz sine；
- 10 kHz sine；
- multi-tone；
- 20Hz–20kHz sweep；
- white noise；
- pink noise。

## Audio quality verification requirements

"音质好"不能依赖主观描述。项目必须验证：

### Decode correctness

对 lossless 格式，reference decode 与 SongCore decode 比较：

- frame count；
- channel count；
- sample rate；
- PCM checksum（在可严格等价时）；
- 或 Float PCM tolerance。

### DSP bypass

当 EQ = flat、ReplayGain = off、Balance = center 时，应真正 bypass DSP：

```text
input PCM == output PCM
```

而不是"把 EQ 各段设成 0 dB 后仍然走完整滤波链"。

### EQ correctness

使用 sine sweep、impulse、white/pink noise、fixed test tones 验证：

- frequency response；
- requested gain；
- clipping；
- numerical stability。

## Change evidence floor

任何音频核心变更至少报告（与 root `AGENTS.md` 一致）：影响 corpus、build
profile 是否变化、binary size delta、相关测试、benchmark 影响、是否改变
音频 PCM。未在真实环境验证的变更只能标记
`CODE_COMPLETE_PENDING_VALIDATION`。

## Legal / repository rule

不要把来源不明的商业歌曲直接提交到公开仓库。

公开 corpus 优先：

- 自生成音频；
- CC0 / permissive sample；
- 短 synthetic fixtures；
- 由测试脚本生成的确定性文件。

真实私人音乐库只用于本地 compatibility test，不进入 Git。
