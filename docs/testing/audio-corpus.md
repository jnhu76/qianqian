# Audio Corpus Plan

## 原则

Codec 支持不是“FFmpeg 说支持”。

只有进入 corpus 并通过 regression 的格式才算 Qianqian officially supported。

## Core Corpus (MP3 / FLAC)

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

## Synthetic Signals

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

## Legal / Repository Rule

不要把来源不明的商业歌曲直接提交到公开仓库。

公开 corpus 优先：

- 自生成音频；
- CC0 / permissive sample；
- 短 synthetic fixtures；
- 由测试脚本生成的确定性文件。

真实私人音乐库只用于本地 compatibility test，不进入 Git。
