# E06 — Xmake-owned FFmpeg slice + Audible SongCore Smoke

## 1. 问题

这次实验不再问“FFmpeg configure 能裁多小”，E01 已经回答了 Stage A 的组件边界。

新的问题是：

> **Qianqian 能否把 FFmpeg 的原始构建系统降级为升级时的 oracle，机器提取播放器需要的源码/编译闭包，随后由 Xmake 独立构造一个 `libqianqian_av.a`，同时保持未来 FFmpeg 版本可机械升级？**

第二个问题是：

> **这个单一 archive 能否通过一个薄薄的 production SongCore，真正连续输出 MP3 PCM，并送到系统音频设备听见？**

这里的优先级是：

1. correctness；
2. upgradability；
3. minimality；
4. 才是构建工具本身。

不要为了绕过 Make 而重写半个 FFmpeg dependency resolver。

---

## 2. 架构

```text
              import / FFmpeg upgrade only
                         │
FFmpeg pristine source ──┼── configure + Make (oracle)
                         │
                         ▼
                exact compiler log
                         │
                         ▼
             manifest.json (generated)
                         │
              normal build boundary
─────────────────────────┼──────────────────────────
                         │
                       Xmake
                         │
             selected translation units
                         │
                         ▼
                libqianqian_av.a
                         │
                         ▼
                     SongCore
                  open / probe /
                read_pcm / seek /
                       close
                         │
                Float32 interleaved
                         │
             ┌───────────┴───────────┐
             │                       │
        qn_pcm_dump            future native host
             │
       tiny QPCM pipe
             │
    Python sounddevice (test only)
             │
             🔊
```

核心规则：

> **FFmpeg configure/Make 是 import-time oracle，不是 Qianqian normal-build dependency。**

Xmake 不理解“MP3”“FLAC”或 FFmpeg dependency semantics。它只重放 importer 已经观察并冻结的 translation units + compile flags。

---

## 3. 为什么不直接手写 FFmpeg 源文件列表

手写：

```text
flacdec.c
mp3dec.c
...
```

看起来更小，但会把上游内部依赖知识变成 Qianqian 的维护债务。

本实验改为：

1. 用 `n3-min-noswr` 表达能力意图；
2. 让 pinned FFmpeg n9.0.1 自己求一次真实 closure；
3. 从 `V=1` 的真实 compiler invocation 机器提取 `.c/.S/...` + flags；
4. manifest 禁止 checkout-specific 绝对路径；
5. Xmake 重放 manifest；
6. 下一个 FFmpeg 版本重新运行 importer 并 diff manifest。

这与 PocketJS/QuickJS 的“小 core + 精确 build input”方向一致，但 FFmpeg 的 closure 必须机器生成，不能人工维护。

---

## 4. 为什么从 `n3-min-noswr` 开始

E01 已证明 Stage A corpus 下：

```text
libswresample path
vs
SongCore-owned sample-format/interleave conversion
```

得到 byte-identical Float32 PCM。

因此 E06 默认使用：

```text
n3-min-noswr
```

只保留：

- libavutil；
- libavcodec；
- libavformat；
- MP3 / FLAC demux；
- MP3Float / FLAC decode；
- configure dependency closure 所需 parser / internal components。

本实验不宣称 sample-rate conversion / rematrix 永远不需要；这里只证明 source-rate/source-layout 的 Stage A 播放链。

---

## 5. 构建步骤

### 5.1 Import / upgrade step

```bash
xmake ffmpeg-import
```

允许执行：

```text
FFmpeg configure
FFmpeg Make
```

但只为了生成：

```text
build/ffmpeg-xmake/oracle/
build/ffmpeg-xmake/manifest.json
build/ffmpeg-xmake/oracle-build.log
```

这些都是 untracked experiment/build artifacts。

`manifest.json` 至少记录：

- FFmpeg tag + exact commit；
- profile hash；
- configure args；
- exact translation units；
- source/generated origin；
- per-unit compile flags；
- reference archive hashes/sizes。

### 5.2 Normal Xmake build

```bash
xmake f -m release
xmake build qianqian_av
```

产物：

```text
build/artifacts/libqianqian_av.a
```

**这一阶段不得调用 FFmpeg configure/Make。**

### 5.3 Production SongCore + PCM transport

```bash
xmake build qn_pcm_dump
```

这会构建：

```text
libqianqian_av.a
libsongcore.a
qn_pcm_dump
```

`qn_pcm_dump` 只负责：

```text
host file IO
→ SongCore
→ QPCM header
→ raw Float32 PCM stdout
```

它不知道任何 FFmpeg type，也不拥有 audio device。

---

## 6. 自动等价验证

```bash
python3 tools/verify_xmake_core.py
```

验证两层：

### A. Build replay equivalence

同一个 `bench/native/qn_bench.c` 分别链接：

```text
oracle:
libavformat.a + libavcodec.a + libavutil.a

replay:
libqianqian_av.a
```

Stage A 全 corpus 比较除 timing/RSS 外的 correctness JSON；包括正常样本和 degraded fixture 的退出语义。

### B. SongCore PCM equivalence

至少：

```text
mp3-cbr-id3v23
flac-16-44-stereo
```

比较：

```text
qn_pcm_dump PCM SHA256
vs
benchmark canonical Float32 PCM SHA256
```

必须 byte-identical。

报告输出：

```text
build/ffmpeg-xmake/verify/report.json
```

---

## 7. Audible smoke

播放器 UI 仍然不进入 Phase 0。

先用 test-only Python sink 证明“真的能听见”：

```bash
python -m pip install sounddevice
python3 tools/play_smoke.py corpus/fixtures/mp3-cbr-id3v23.mp3
```

播放链必须是：

```text
MP3 file
→ Qianqian host IO
→ SongCore
→ libqianqian_av.a
→ Float32 PCM
→ Python sounddevice / PortAudio
→ system audio device
```

禁止 Python 再解码 MP3；`sounddevice` 只消费 Qianqian 已经产生的 PCM。

`sounddevice` 是 acceptance dependency，不是 shipping dependency。

---

## 8. SongCore 当前边界

Production 层只实现：

```text
song_open
song_probe
song_read_pcm
song_seek
song_close
```

必须满足：

- FFmpeg type 不泄漏；
- host owns IO；
- streaming O(frame/buffer) memory，不缓存整首歌；
- `avcodec_send_packet(EAGAIN)` 时保留同一个 packet，先 receive 再重试；
- demux error 与 clean EOF 区分；
- seek 后清空 decoder/pending PCM 状态；
- 当前输出固定为 source-rate/source-layout Float32 interleaved。

本 PR 不实现 metadata/artwork public ABI 扩展；E01 已验证 FFmpeg capability，产品 API 后续按真实播放器需求加入。

---

## 9. Upgrade test

未来 FFmpeg `n9.0.1 → nX.Y` 时，不复制旧源码列表。

流程：

```text
update pin
→ xmake ffmpeg-import
→ diff old/new manifest
→ xmake build qianqian_av
→ verify_xmake_core.py
→ size / symbol / corpus / PCM drift report
```

需要审查：

- translation units added/removed；
- compile flag drift；
- artifact size drift；
- symbol drift；
- corpus behavior；
- PCM behavior。

目标是“重新计算 slice”，不是维护长期 FFmpeg fork。

---

## 10. WASM 方向

E06 不构建 WASM，但 manifest/importer 必须避免把 Native 偶然事实误写成播放器 capability。

未来 WASM 应是：

```text
same capability intent
→ Emscripten-specific oracle/import
→ wasm manifest
→ Xmake/Emscripten replay
→ qianqian_av.wasm
```

Native 与 WASM 不要求 translation-unit closure 字节相同，因为 platform/config dependency 不同；要求的是：

```text
same SongCore behavior
same corpus
same PCM contract
```

---

## 11. Gate

本实验只有以下全部成立才可称 PASS：

```text
[ ] importer 从 pinned n9.0.1 生成非空、无机器绝对路径的 closure
[ ] normal `xmake build qianqian_av` 不调用 FFmpeg Make/configure
[ ] 得到单一 libqianqian_av.a
[ ] oracle vs Xmake 全 Stage A corpus 等价
[ ] SongCore MP3 PCM byte-identical
[ ] SongCore FLAC PCM byte-identical
[ ] SongCore streaming memory，不缓存整首歌曲
[ ] audible MP3 smoke 真正从系统音频设备播放
[ ] Python/sounddevice 不进入 shipping dependency graph
```

在真实机器运行这些 gate 之前，状态只能是：

```text
CODE_COMPLETE_PENDING_VALIDATION
```
