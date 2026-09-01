# corpus/local — 真实歌曲本地语料

`corpus/fixtures/` 是 `corpus/tools/gen_corpus.py` 生成、带 manifest 的合成
边界用例。本目录存放真实版权音乐，音频随仓库一起提交；仅用于本项目本地的
解码/播放测试，不得再分发或用于其他用途。

用途：audible smoke（`tools/play_smoke.py`）与真实世界解码回归，与合成 fixture
互补。真实文件更大、带真实 metadata 与封面，能暴露合成文件覆盖不到的路径。

## 隐形的翅膀（同一首歌的三种编码）

来源：用户于 2026-08-30 提供。三者为同一录音（时长一致，FLAC 精确为
9,881,952 samples ≈ 224.081 s @ 44.1 kHz）。

| 文件 | 音频流 | 封面 | 大小 (bytes) | sha256 |
|---|---|---|---|---|
| `mp3-cbr-128.mp3` | mp3 CBR 128 kbps, 44.1 kHz, 2 ch | 无 | 3,586,705 | `23995935dbfecf70b75cc96d9d3db6047bbbbd86d6663525fc3bbf7e953be371` |
| `mp3-cbr-320-artwork.mp3` | mp3 CBR 320 kbps, 44.1 kHz, 2 ch | mjpeg | 9,136,912 | `b9a069044c7456b1ea554cb1e2f4c5dd3d413c756c0245f829f28cb3577d3764` |
| `flac-16-44-artwork.flac` | flac 16-bit / 44.1 kHz, 2 ch | mjpeg | 24,620,616 | `12c10a3bd842ad5aa2ebddbdde96ac1c8f4b46a2f89e8e4b891f3c4819ace7c0` |

sha256 用于校验 checkout 后音频文件的完整性。

## 使用

```bash
# 外部消费者验收（仅标准库 ctypes；解码/seek/metadata/artwork 全链路）
python3 tools/songcore_ffi_smoke.py corpus/local/yinxing-de-chibi/*.flac \
    corpus/local/yinxing-de-chibi/*.mp3

# 可听验收（wrapper → songcore_ffi_smoke.py --play）
python3 tools/play_smoke.py corpus/local/yinxing-de-chibi/mp3-cbr-320-artwork.mp3
```

## FFI 验收记录（2026-09-01, libsongcore.so, ABI v1）

三首全部 PASS（证据: `bench/results/songcore-v1/local-corpus-ffi.json`）:

| 文件 | container/codec | rate/ch | metadata | artwork | decode | seek |
|---|---|---|---|---|---|---|
| `flac-16-44-artwork.flac` | flac/flac | 44.1k/2 | title=隐形的翅膀 | jpeg 1 件 | PASS | 5s→4.923s |
| `mp3-cbr-128.mp3` | mp3/mp3 | 44.1k/2 | title=隐形的翅膀 | 无 | PASS | 5s→4.989s |
| `mp3-cbr-320-artwork.mp3` | mp3/mp3 | 44.1k/2 | title=隐形的翅膀 | jpeg 1 件 | PASS | 5s→4.989s |

可听运行（`--play`，5s 采样，44.1 kHz / 2 ch Float32 直出 PortAudio）PASS。
