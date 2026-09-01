# E11 — Audio Core Finalization（DSP/SRC 冻结 + SongCore ABI v1）

E11 冻结 `include/songcore.h` 为 ABI v1，并冻结 DSP/SRC 架构决策
（AudioEngine = 能力裁剪 libavfilter + aresample/libswresample；SongCore
永不运行 libavfilter、不 resample）。机器权威树 =
`bench/results/songcore-v1/*.json`，read-only `--check` 失败即关门。

## 复现

```bash
# 1) 构建 harness（e11-test 生产形 closure）
xmake f -o build/xmake-e11 --av_manifest=build/minimize/e11-test/manifest.json -y
xmake build qn_e11_record

# 2) 主驱动：abi/metadata/artwork/stream-selection/seek/errors/consistency/
#    states/common-formats/summary
python3 bench/e11/run_e11.py --out bench/results/songcore-v1

# 3) sanitizer 密度（ASan/UBSan/leak，136 invocations，~10M frames）
python3 bench/e11/run_e11_sanitizers.py

# 4) DSP/SRC smoke（先确保 avf-c2 裁剪闭包已构建）
python3 tools/pcm_c0.py --stage avf-c2     # 一次性：派生 avf-c2 闭包 + probe
python3 bench/e11/run_e11_dsp_src.py

# 5) 只读校验
python3 bench/e11/run_e11.py --check --out bench/results/songcore-v1
python3 bench/e11/run_e11_sanitizers.py --check --out bench/results/songcore-v1
python3 bench/e11/run_e11_dsp_src.py --check --out bench/results/songcore-v1
```

## 输入

- fixtures：`corpus/fixtures/`（由 `corpus/tools/gen_corpus_e11.py` 生成，
  deterministic / synthetic，无版权素材）。
- manifests：`corpus/manifest/e11.json`（20 个 E11 用例）、
  `corpus/manifest/common-formats.json`（常见格式回归）。
- closure：`bench/profiles/e11-test.json`（c5-opus + matroska demuxer，
  测试专用；生产矩阵不变）。DSP/SRC 裁剪闭包 = `build/minimize/avf-c2`
  （E10-C0 tier F1），意图源 `bench/dsp-capabilities.json`。

## 输出文件

| 文件 | 内容 |
|---|---|
| abi.json | 15 个 contract symbols 在 libsongcore.a 的定义 + WASI guest 15 个 `song_wasm_*` 导出 |
| metadata.json / artwork.json | canonical metadata（has_*、UTF-8 长度感知）、原始枚举、封面 item 结构 |
| stream-selection.json | 默认流选择 + 切换语义（重建解码器、位置回零、PCM 属新流） |
| seek.json | clamp + 实际位置报告；strict/lapped/unsupported 按 E08 证据分族 |
| errors.json | 类型化错误（open/probe/read；无 generic -1） |
| consistency.json | 一歌一快照：decode→EOF→seek 前后 metadata/artwork sha 稳定 |
| states.json | 确定性状态序列（read-before-probe、select 重置、非法 select） |
| common-formats.json | MP3/FLAC/AAC-M4A/ADTS/ALAC/WAV(6 变体)/Vorbis/Opus 回归 |
| sanitizers.json | ASan/UBSan/leak 密度（136 invocations、10M frames、0 报告） |
| dsp-src-integration.json | BYPASS 形状 + 44.1k→48k aresample + volume/equalizer 有效性 |
| summary.json | verdict + abi_frozen |

## 边界

- 不触碰 issue #9 格式（APE/WMA/AIFF/WavPack）。
- 无 UI / playlist / 音频后端。
- `qn_e11_record` 与 sanitizer 仪器只存在于 bench，不进入 shipping graph。
